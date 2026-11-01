//! Dictation orchestrator: hotkey actions -> live record -> Whisper ->
//! filters -> typing at the cursor, with visible status in the tray title and
//! window title and a live preview event for the UI.
//!
//! Engine per hold (`engine::choose_engine`): the on-device streaming engine
//! when speech permission is granted and en-US runs on device, otherwise
//! Whisper with a fixed reason in the log. Streaming flow: the recorder
//! drains audio every 50 ms into the engine session (`stream_hold`); every
//! word of a partial except its last one is typed at once, and each session
//! final is reconciled with what was typed.
//!
//! Whisper flow while the key is held: the recorder drains new audio every
//! 100 ms into the segmenter. About every 1.2 s, when Whisper is idle, the
//! open segment is transcribed as a partial; words two partials agree on are
//! typed right away. When a pause ends a segment (or the key is released),
//! its final text is reconciled with what was typed.
//!
//! Text is only ever typed with key events, never put on the clipboard.
//! Every hold gets an id; worker results carry it so a late result of an
//! earlier hold is dropped. Before every key action a focus guard checks the
//! frontmost app, user input and secure fields (`crate::focus`).
//!
//! Threads: one orchestrator thread owns the state machine; a recorder
//! thread owns the (non-Send) audio session; a Whisper thread owns the model,
//! loaded lazily on first use; one typing thread runs edits in order. The
//! hotkey thread only sends a message. Dictated text and audio are never
//! logged or written to disk.

pub mod engine;
pub mod live;
pub mod state;
pub mod stream_hold;
pub mod stream_text;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use crate::audio::{
    AudioCaptureConfig, AudioCaptureSession, MicPermissionStatus, check_microphone_permission,
    request_microphone_permission,
};
use crate::focus::{FocusProbe, Gate, ProbeGuard, Snapshot, StopReason, SystemProbe, TypingGuard};
use crate::hotkey::HotkeyAction;
use crate::paste::PasteError;
use crate::paste::typing::{EditError, EditOutcome, type_edit_checked};
use crate::stt::filters::{sanitize_transcript, silence};
use crate::stt::local_whisper::LocalWhisper;
use crate::stt::streaming::StreamingStt;
use engine::{EngineChoice, EngineSetting, Fallback};
use live::{Job, JobQueue, Segmenter};
use state::{DictationState, Effect, Input, Machine};
use stream_hold::StreamHold;

/// Id given to the tray icon so status updates can find it.
pub const TRAY_ID: &str = "deepai-tray";
const WINDOW_BASE_TITLE: &str = "DeepAI Voice";
const MODEL_FILE: &str = "models/ggml-base.en.bin";
const ERROR_VISIBLE: Duration = Duration::from_secs(5);
/// How long "Cancelled" stays visible after Esc.
const CANCEL_VISIBLE: Duration = Duration::from_millis(1500);
/// Global shortcut that cancels the active hold.
pub const ESCAPE_SHORTCUT: &str = "Escape";
/// Event with the live preview text (empty clears it).
pub const PARTIAL_EVENT: &str = "dictation-partial";
/// How often the recorder drains new audio while the key is held.
const DRAIN_EVERY: Duration = Duration::from_millis(100);
/// Drain interval with the streaming engine, so partials show up quickly.
const STREAM_DRAIN_EVERY: Duration = Duration::from_millis(50);
/// Hold audio kept for the safety net pass (the 120 s recording cap).
const MAX_HOLD_SAMPLES: usize = 120 * live::SAMPLE_RATE;
/// Open segment audio needed before a partial is worth running.
const PARTIAL_MIN_SAMPLES: usize = live::SAMPLE_RATE / 2;

/// Result of turning recorded audio into text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Text(String),
    /// Too short, silent, empty or filtered: nothing to paste.
    Nothing,
    Failed(String),
}

/// What to do about microphone permission before recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicGate {
    Go,
    Ask,
    Denied,
}

pub fn mic_gate(status: MicPermissionStatus) -> MicGate {
    match status {
        MicPermissionStatus::Granted => MicGate::Go,
        MicPermissionStatus::Undetermined => MicGate::Ask,
        MicPermissionStatus::Denied => MicGate::Denied,
    }
}

/// Fixed status text for failed typing. Nothing was copied anywhere.
pub fn paste_error_status(err: &PasteError) -> String {
    match err {
        PasteError::AccessibilityDenied => "Accessibility needed".into(),
        PasteError::InjectFailed(_) => "Typing failed".into(),
    }
}

/// Model lookup: env value, then `<home>/Library/Application Support/DeepAI
/// Voice/models/...`, then `models/...` relative to the current dir.
pub fn resolve_model_path(
    env: Option<String>,
    home: Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(e) = env.filter(|e| !e.is_empty()) {
        candidates.push(PathBuf::from(e));
    }
    if let Some(h) = home {
        candidates.push(
            h.join("Library/Application Support/DeepAI Voice")
                .join(MODEL_FILE),
        );
    }
    candidates.push(PathBuf::from(MODEL_FILE));
    candidates.into_iter().find(|p| exists(p))
}

/// Filters audio, runs `transcribe` only on real speech, then sanitizes.
pub fn process_audio(
    samples: &[f32],
    transcribe: impl FnOnce(&[f32]) -> Result<String, String>,
) -> Outcome {
    if silence::is_too_short(samples)
        || silence::is_too_silent(samples, silence::DEFAULT_SILENCE_RMS_THRESHOLD)
    {
        return Outcome::Nothing;
    }
    match transcribe(samples) {
        Ok(raw) => match sanitize_transcript(&raw) {
            Some(text) => Outcome::Text(text),
            None => Outcome::Nothing,
        },
        Err(msg) => Outcome::Failed(msg),
    }
}

/// Runs the machine over `rx`. Publishes status when the state changes and
/// hands each effect to `run`. Returns when `rx` closes.
pub fn drive(rx: Receiver<Input>, status: impl FnMut(&str), run: impl FnMut(Effect)) {
    drive_live(rx, status, |_| {}, run);
}

/// Like `drive`, also publishing the live preview text whenever it changes.
pub fn drive_live(
    rx: Receiver<Input>,
    status: impl FnMut(&str),
    partial: impl FnMut(&str),
    run: impl FnMut(Effect),
) {
    drive_live_with(rx, status, partial, run, |_| {});
}

/// Esc is registered as a global shortcut only while a hold is active:
/// recording, thinking, or typing the last words.
pub fn escape_wanted(state: &DictationState) -> bool {
    matches!(
        state,
        DictationState::Recording | DictationState::Transcribing | DictationState::Pasting
    )
}

/// Like `drive_live`, also reporting every state change to `on_state`.
pub fn drive_live_with(
    rx: Receiver<Input>,
    mut status: impl FnMut(&str),
    mut partial: impl FnMut(&str),
    mut run: impl FnMut(Effect),
    mut on_state: impl FnMut(&DictationState),
) {
    let mut machine = Machine::new();
    for input in rx {
        let before = machine.state().clone();
        let before_partial = machine.partial().to_string();
        let effect = machine.handle(input);
        if *machine.state() != before {
            status(&machine.status_text());
            on_state(machine.state());
        }
        if machine.partial() != before_partial {
            partial(machine.partial());
        }
        if let Some(effect) = effect {
            run(effect);
        }
        while let Some(effect) = machine.take_followup() {
            run(effect);
        }
    }
}

enum RecCmd {
    /// Start hold `id`.
    Start(u64),
    Stop,
    Discard,
}

/// Whisper job queue shared by the recorder (producer) and Whisper thread.
type SharedJobs = Arc<(Mutex<JobQueue>, Condvar)>;

fn with_jobs<T>(jobs: &SharedJobs, f: impl FnOnce(&mut JobQueue) -> T) -> T {
    let (lock, cvar) = &**jobs;
    let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
    let out = f(&mut q);
    cvar.notify_all();
    out
}

/// One held key: the audio session plus the live segmenter, or the
/// streaming engine session when `stream` is set.
struct Hold {
    session: AudioCaptureSession,
    segmenter: Segmenter,
    total_samples: usize,
    last_partial: Instant,
    stream: Option<StreamHold>,
    /// Whole hold audio (Whisper path, capped), for the safety net pass.
    audio: Vec<f32>,
    started: Instant,
    /// Partial passes that were due but skipped because Whisper was busy.
    skipped_busy: u32,
}

impl Hold {
    fn new(session: AudioCaptureSession, floor: f32, stream: Option<StreamHold>) -> Self {
        Self {
            session,
            segmenter: Segmenter::with_floor(floor),
            total_samples: 0,
            last_partial: Instant::now(),
            stream,
            audio: Vec::new(),
            started: Instant::now(),
            skipped_busy: 0,
        }
    }
}

fn send_all(inbox: &Sender<Input>, hold_id: u64, inputs: Vec<Input>) {
    for input in inputs {
        let _ = inbox.send(tag(hold_id, input));
    }
}

impl Hold {
    fn feed(&mut self, samples: &[f32], jobs: &SharedJobs) {
        self.total_samples += samples.len();
        let room = MAX_HOLD_SAMPLES.saturating_sub(self.audio.len());
        self.audio
            .extend_from_slice(&samples[..samples.len().min(room)]);
        for seg in self.segmenter.push(samples) {
            with_jobs(jobs, |q| q.push_final(seg.seq, seg.samples, false));
        }
    }

    /// Queues a partial pass when it is due (`live::partial_due`, which
    /// waits at least as long as the last pass took) and Whisper is idle:
    /// never while a partial runs or a final is pending.
    fn maybe_partial(&mut self, jobs: &SharedJobs) {
        let Some((seq, audio)) = self.segmenter.open_audio() else {
            return;
        };
        if audio.len() < PARTIAL_MIN_SAMPLES {
            return;
        }
        let since = self.last_partial.elapsed();
        let queued = with_jobs(jobs, |q| {
            if !live::partial_due(since, q.partial_cost()) {
                return None;
            }
            if q.is_idle() {
                q.push_partial(seq, audio);
                Some(true)
            } else {
                Some(false)
            }
        });
        match queued {
            Some(true) => self.last_partial = Instant::now(),
            Some(false) => self.skipped_busy += 1,
            None => {}
        }
    }
}

/// Key released: drain the rest, then queue the open tail as the last final.
/// A hold shorter than the minimum behaves as before (nothing to type).
/// With the streaming engine, returns the hold's engine state while its
/// final is still on the way.
fn finish_hold(
    h: Hold,
    hold_id: u64,
    inbox: &Sender<Input>,
    jobs: &SharedJobs,
) -> Option<StreamHold> {
    let Hold {
        session,
        mut segmenter,
        mut total_samples,
        stream,
        mut audio,
        started,
        skipped_busy,
        ..
    } = h;
    let hold_ms = started.elapsed().as_millis();
    let rest = session.finish_live();
    if let Some(mut stream) = stream {
        let inputs = match rest {
            Ok(rest) if total_samples + rest.len() >= silence::MIN_DURATION_SAMPLES => {
                stream.release(&rest)
            }
            // Too short to be speech: nothing to type, as with Whisper.
            Ok(_) => {
                stream.cancel();
                vec![Input::Transcribed(Outcome::Nothing)]
            }
            Err(_) => {
                stream.cancel();
                vec![Input::Transcribed(Outcome::Failed(
                    "Recording failed".into(),
                ))]
            }
        };
        send_all(inbox, hold_id, inputs);
        return (!stream.is_done()).then_some(stream);
    }
    let rest = match rest {
        Ok(rest) => rest,
        Err(_) => {
            let _ = inbox.send(tag(
                hold_id,
                Input::Transcribed(Outcome::Failed("Recording failed".into())),
            ));
            return None;
        }
    };
    total_samples += rest.len();
    let room = MAX_HOLD_SAMPLES.saturating_sub(audio.len());
    audio.extend_from_slice(&rest[..rest.len().min(room)]);
    let finished = segmenter.push(&rest);
    let tail = segmenter.flush();
    let too_short = total_samples < silence::MIN_DURATION_SAMPLES;
    // Diagnostics: event names and numbers only, never text or audio.
    if skipped_busy > 0 {
        tracing::info!(
            "dictation event=partial_skipped_busy count={} hold_ms={}",
            skipped_busy,
            hold_ms
        );
    }
    let segments = segmenter.segments();
    if segments == 0 {
        tracing::info!(
            "dictation event=hold_no_segment hold_ms={} audio_ms={}",
            hold_ms,
            audio.len() * 1000 / live::SAMPLE_RATE
        );
    }
    let safety_net = live::safety_net_wanted(segments, &audio);
    if safety_net {
        tracing::info!(
            "dictation event=safety_net_used audio_ms={}",
            audio.len() * 1000 / live::SAMPLE_RATE
        );
    }
    with_jobs(jobs, |q| {
        for seg in finished {
            q.push_final(seg.seq, seg.samples, false);
        }
        let (seq, samples) = match tail {
            Some(t) if !too_short => (t.seq, t.samples),
            // No speech found by the segmenter: transcribe the whole hold.
            _ if safety_net => (1, audio),
            _ => (u64::MAX, Vec::new()),
        };
        q.push_final(seq, samples, true);
    });
    None
}

fn tag(hold: u64, input: Input) -> Input {
    Input::FromHold {
        hold,
        input: Box::new(input),
    }
}

/// Engine choice kept by the recorder thread across holds.
struct EnginePicker {
    setting: EngineSetting,
    /// Speech permission was requested in this run.
    asked: bool,
    engine: Option<Arc<dyn StreamingStt>>,
}

impl EnginePicker {
    fn new() -> Self {
        let value = std::env::var(engine::ENGINE_ENV).ok();
        #[cfg(target_os = "macos")]
        let engine: Option<Arc<dyn StreamingStt>> =
            Some(Arc::new(crate::stt::apple_speech::AppleSpeech::new()));
        #[cfg(not(target_os = "macos"))]
        let engine: Option<Arc<dyn StreamingStt>> = None;
        Self {
            setting: EngineSetting::from_env_value(value.as_deref()),
            asked: false,
            engine,
        }
    }

    #[cfg(target_os = "macos")]
    fn choose(&mut self) -> EngineChoice {
        use crate::stt::apple_speech::{
            OnDeviceProbe, SfSpeechPermission, SpeechPermissionProvider, SystemOnDeviceProbe,
        };
        let permission = SfSpeechPermission.status();
        // Asking is only safe from the bundled app (it has the usage string).
        let bundled = std::env::current_exe()
            .ok()
            .and_then(|e| crate::autostart::bundle_path(&e))
            .is_some();
        if engine::should_request_permission(self.setting, permission, bundled, self.asked) {
            self.asked = true;
            SfSpeechPermission.request(Box::new(|_| {}));
        }
        engine::choose_engine(self.setting, permission, || {
            SystemOnDeviceProbe.support(engine::STREAM_LOCALE)
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn choose(&mut self) -> EngineChoice {
        engine::choose_engine(
            self.setting,
            crate::stt::apple_speech::SpeechPermissionStatus::Denied,
            || crate::stt::apple_speech::LocaleSupport::NotSupported,
        )
    }

    /// Streaming state for a new hold, or None for Whisper. The reason for
    /// Whisper is logged as a fixed string.
    fn start_stream(&mut self) -> Option<StreamHold> {
        let reason = match (self.choose(), self.engine.clone()) {
            (EngineChoice::Streaming, Some(engine)) => {
                match StreamHold::start(engine, engine::STREAM_LOCALE) {
                    Ok(stream) => {
                        tracing::info!("dictation engine=streaming");
                        return Some(stream);
                    }
                    Err(_) => Fallback::StartFailed,
                }
            }
            (EngineChoice::Streaming, None) => Fallback::NotOnDevice,
            (EngineChoice::Whisper(reason), _) => reason,
        };
        tracing::info!("dictation engine=whisper reason=\"{}\"", reason.status());
        None
    }
}

fn recorder_thread(rx: Receiver<RecCmd>, inbox: Sender<Input>, jobs: SharedJobs) {
    let mut hold: Option<Hold> = None;
    let mut hold_id = 0;
    let mut last_error: Option<String> = None;
    // Streaming engine of a released hold whose final is still on the way.
    let mut finishing: Option<StreamHold> = None;
    let mut picker = EnginePicker::new();
    // Noise floor learned in earlier holds, so a noisy room is known at once.
    let mut noise_floor = live::MIN_FLOOR;
    loop {
        let streaming = finishing.is_some() || hold.as_ref().is_some_and(|h| h.stream.is_some());
        let cmd = if hold.is_some() || finishing.is_some() {
            let every = if streaming {
                STREAM_DRAIN_EVERY
            } else {
                DRAIN_EVERY
            };
            match rx.recv_timeout(every) {
                Ok(cmd) => Some(cmd),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => return,
            }
        };
        match cmd {
            None => {
                if let Some(h) = hold.as_mut() {
                    match h.session.drain_16k() {
                        Ok(samples) => match h.stream.as_mut() {
                            Some(stream) => {
                                h.total_samples += samples.len();
                                send_all(&inbox, hold_id, stream.feed(&samples));
                            }
                            None => {
                                h.feed(&samples, &jobs);
                                h.maybe_partial(&jobs);
                            }
                        },
                        Err(_) => last_error = Some("Recording failed".into()),
                    }
                }
                if let Some(stream) = finishing.as_mut() {
                    send_all(&inbox, hold_id, stream.poll());
                    if stream.is_done() {
                        finishing = None;
                    }
                }
            }
            Some(RecCmd::Start(id)) => {
                hold = None;
                if let Some(mut old) = finishing.take() {
                    old.cancel();
                }
                hold_id = id;
                last_error = None;
                with_jobs(&jobs, |q| q.begin_hold(id));
                match mic_gate(check_microphone_permission()) {
                    MicGate::Go => {
                        match AudioCaptureSession::start(AudioCaptureConfig::default()) {
                            Ok(session) => {
                                // Capture runs first, so no audio is lost
                                // while the engine starts.
                                hold = Some(Hold::new(session, noise_floor, picker.start_stream()))
                            }
                            Err(_) => last_error = Some("Mic unavailable".into()),
                        }
                    }
                    MicGate::Ask => {
                        request_microphone_permission(Box::new(|_| {}));
                        last_error = Some("Allow microphone, then hold again".into());
                    }
                    MicGate::Denied => last_error = Some("Mic denied in Settings".into()),
                }
                if let Some(e) = &last_error {
                    let _ = inbox.send(tag(hold_id, Input::StartFailed(e.clone())));
                }
            }
            Some(RecCmd::Stop) => match hold.take() {
                Some(h) => {
                    if h.stream.is_none() {
                        noise_floor = h.segmenter.floor();
                    }
                    finishing = finish_hold(h, hold_id, &inbox, &jobs)
                }
                None => {
                    let msg = last_error.clone().unwrap_or_else(|| "No recording".into());
                    let _ = inbox.send(tag(hold_id, Input::StartFailed(msg)));
                }
            },
            Some(RecCmd::Discard) => {
                // Esc or discard: engine sessions stop, nothing more is sent.
                if let Some(mut stream) = hold.take().and_then(|h| h.stream) {
                    stream.cancel();
                }
                if let Some(mut stream) = finishing.take() {
                    stream.cancel();
                }
                with_jobs(&jobs, |q| q.clear());
            }
        }
    }
}

/// Loads the model on first use.
fn ensure_model(model: &mut Option<LocalWhisper>) -> Result<&mut LocalWhisper, String> {
    if model.is_none() {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = resolve_model_path(std::env::var("DEEPAI_MODEL_PATH").ok(), home, |p| {
            p.is_file()
        })
        .ok_or_else(|| "Model missing".to_string())?;
        let loaded = LocalWhisper::load(&path, crate::stt::local_whisper::default_threads())
            .map_err(|_| "Model failed to load".to_string())?;
        *model = Some(loaded);
    }
    model.as_mut().ok_or_else(|| "Model missing".to_string())
}

fn whisper_thread(jobs: SharedJobs, inbox: Sender<Input>) {
    let mut model: Option<LocalWhisper> = None;
    loop {
        let (hold, job) = {
            let (lock, cvar) = &*jobs;
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = q.take_next() {
                    break (q.hold(), job);
                }
                q = cvar.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let transcribe = |s: &[f32]| {
            ensure_model(&mut model)?
                .transcribe(s)
                .map_err(|_| "Transcription failed".to_string())
        };
        let input = match job {
            Job::Final { seq, samples, last } => {
                let outcome = live::process_segment(&samples, transcribe);
                if last {
                    Some(Input::Transcribed(outcome))
                } else {
                    Some(Input::Segment { seq, outcome })
                }
            }
            Job::Partial { seq, samples } => {
                let started = Instant::now();
                // Reduced audio context: much cheaper for short audio.
                let fast = |s: &[f32]| {
                    ensure_model(&mut model)?
                        .transcribe_partial(s)
                        .map_err(|_| "Transcription failed".to_string())
                };
                let outcome = live::process_segment(&samples, fast);
                let cost = started.elapsed();
                with_jobs(&jobs, |q| q.set_partial_cost(cost));
                match outcome {
                    Outcome::Text(text) => Some(Input::Partial { seq, text }),
                    _ => None,
                }
            }
        };
        with_jobs(&jobs, |q| q.done());
        if let Some(input) = input {
            let _ = inbox.send(tag(hold, input));
        }
    }
}

/// Types one edit through `type_edit` and turns the result into the
/// machine's input. No clipboard and no paste fallback: when typing fails,
/// the rest of the edit is not inserted and a fixed status is shown.
pub fn run_edit_with<G: TypingGuard>(
    guard: &mut G,
    backspaces: usize,
    text: &str,
    type_edit: impl FnOnce(&mut G, usize, &str) -> Result<EditOutcome, EditError>,
) -> Input {
    if backspaces == 0 && text.is_empty() {
        return Input::Pasted(Ok(()));
    }
    match type_edit(guard, backspaces, text) {
        Ok(EditOutcome::Done | EditOutcome::SkippedCorrection) => Input::Pasted(Ok(())),
        Ok(EditOutcome::Stopped(reason)) => Input::Stopped(reason),
        Err(e) => Input::Pasted(Err(paste_error_status(&e.error))),
    }
}

/// Work for the typing thread, in order.
enum EditCmd {
    /// Hold `hold` started; `start` is the focus state at the key press.
    Begin { hold: u64, start: Snapshot },
    Edit {
        hold: u64,
        backspaces: usize,
        text: String,
    },
}

/// Focus guard plus Esc: once hold `hold` is cancelled, every check stops,
/// so edits already queued or half typed send nothing more (no Backspace).
struct HoldTypingGuard {
    hold: u64,
    cancelled: Arc<AtomicU64>,
    focus: ProbeGuard<SystemProbe>,
}

impl TypingGuard for HoldTypingGuard {
    fn check(&mut self) -> Gate {
        if self.cancelled.load(Ordering::SeqCst) == self.hold {
            return Gate::Stop(StopReason::Cancelled);
        }
        self.focus.check()
    }
}

fn typing_thread(rx: Receiver<EditCmd>, inbox: Sender<Input>, cancelled: Arc<AtomicU64>) {
    let mut guard: Option<(u64, HoldTypingGuard)> = None;
    for cmd in rx {
        match cmd {
            EditCmd::Begin { hold, start } => {
                let g = HoldTypingGuard {
                    hold,
                    cancelled: cancelled.clone(),
                    focus: ProbeGuard::with_start(SystemProbe, start),
                };
                guard = Some((hold, g));
            }
            EditCmd::Edit {
                hold,
                backspaces,
                text,
            } => {
                let input = match guard.as_mut() {
                    Some((h, g)) if *h == hold => {
                        run_edit_with(g, backspaces, &text, type_edit_checked)
                    }
                    // No start snapshot for this hold: we cannot confirm the
                    // target, so type nothing.
                    _ => Input::Stopped(StopReason::AppChanged),
                };
                let _ = inbox.send(tag(hold, input));
            }
        }
    }
}

/// Keeps the global Esc shortcut registered exactly while the last value
/// received is true. While registered, Esc is consumed by this app and does
/// not reach the focused app; the rest of the time Esc works normally.
/// If registering fails, Esc cancel is simply unavailable for that hold.
fn escape_thread<R: Runtime>(app: AppHandle<R>, rx: Receiver<bool>, inbox: Sender<Input>) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let Ok(esc) = crate::hotkey::fallback::parse(ESCAPE_SHORTCUT) else {
        tracing::warn!("Esc cancel unavailable: shortcut did not parse");
        return;
    };
    let mut registered = false;
    while let Ok(mut want) = rx.recv() {
        // Only the newest wish matters.
        while let Ok(w) = rx.try_recv() {
            want = w;
        }
        if want && !registered {
            let inbox = inbox.clone();
            let result = app.global_shortcut().on_shortcut(esc, move |_, _, event| {
                if event.state() == ShortcutState::Pressed {
                    let _ = inbox.send(Input::Cancel);
                }
            });
            match result {
                Ok(()) => registered = true,
                Err(_) => tracing::warn!("Esc cancel unavailable: shortcut not registered"),
            }
        } else if !want && registered {
            if app.global_shortcut().unregister(esc).is_err() {
                tracing::warn!("Esc shortcut could not be released");
            }
            registered = false;
        }
    }
}

fn show_status<R: Runtime>(app: &AppHandle<R>, text: &str) {
    let _ = tauri::Emitter::emit(app, "dictation-status", text);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(if text.is_empty() { None } else { Some(text) });
    }
    if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
        let title = if text.is_empty() {
            WINDOW_BASE_TITLE.to_string()
        } else {
            format!("{WINDOW_BASE_TITLE} - {text}")
        };
        let _ = window.set_title(&title);
    }
}

/// Starts the orchestrator. The returned callback is cheap and non-blocking;
/// give it to `hotkey::start`.
pub fn spawn<R: Runtime>(app: AppHandle<R>) -> impl Fn(HotkeyAction) + Send + 'static {
    let (inbox_tx, inbox_rx) = channel::<Input>();
    let (rec_tx, rec_rx) = channel::<RecCmd>();
    let (edit_tx, edit_rx) = channel::<EditCmd>();
    let jobs: SharedJobs = Arc::new((Mutex::new(JobQueue::default()), Condvar::new()));

    let (inbox, q) = (inbox_tx.clone(), jobs.clone());
    std::thread::spawn(move || recorder_thread(rec_rx, inbox, q));
    let inbox = inbox_tx.clone();
    std::thread::spawn(move || whisper_thread(jobs, inbox));
    // Id of the last hold cancelled with Esc (0: none).
    let cancelled = Arc::new(AtomicU64::new(0));
    // One typing thread, so edits land in order.
    let (inbox, c) = (inbox_tx.clone(), cancelled.clone());
    std::thread::spawn(move || typing_thread(edit_rx, inbox, c));
    // Registers Esc while a hold is active; the plugin call blocks on the
    // main thread, so it runs here instead of on the orchestrator.
    let (esc_tx, esc_rx) = channel::<bool>();
    let (inbox, esc_app) = (inbox_tx.clone(), app.clone());
    std::thread::spawn(move || escape_thread(esc_app, esc_rx, inbox));

    let inbox = inbox_tx.clone();
    std::thread::spawn(move || {
        // Mirrors Machine::hold_id: the machine starts a new hold exactly
        // when it emits StartCapture, and both count from 0.
        let mut hold: u64 = 0;
        drive_live_with(
            inbox_rx,
            |text| show_status(&app, text),
            |text| {
                let _ = tauri::Emitter::emit(&app, PARTIAL_EVENT, text);
            },
            |effect| match effect {
                Effect::StartCapture => {
                    hold += 1;
                    // Focus state at the key press, before any audio or typing.
                    let start = SystemProbe.snapshot();
                    let _ = edit_tx.send(EditCmd::Begin { hold, start });
                    let _ = rec_tx.send(RecCmd::Start(hold));
                }
                Effect::StopAndTranscribe => {
                    let _ = rec_tx.send(RecCmd::Stop);
                }
                Effect::DiscardCapture => {
                    let _ = rec_tx.send(RecCmd::Discard);
                }
                Effect::CancelHold => {
                    // Typing stops first, then audio and queued jobs go.
                    cancelled.store(hold, Ordering::SeqCst);
                    let _ = rec_tx.send(RecCmd::Discard);
                    let inbox = inbox.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(CANCEL_VISIBLE);
                        let _ = inbox.send(Input::ClearError);
                    });
                }
                Effect::ScheduleClearError => {
                    let inbox = inbox.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(ERROR_VISIBLE);
                        let _ = inbox.send(Input::ClearError);
                    });
                }
                Effect::Paste(text) => {
                    let _ = edit_tx.send(EditCmd::Edit {
                        hold,
                        backspaces: 0,
                        text,
                    });
                }
                Effect::Correct { backspaces, text } => {
                    let _ = edit_tx.send(EditCmd::Edit {
                        hold,
                        backspaces,
                        text,
                    });
                }
            },
            |state| {
                let _ = esc_tx.send(escape_wanted(state));
            },
        );
    });

    move |action| {
        let _ = inbox_tx.send(Input::Hotkey(action));
    }
}

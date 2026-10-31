//! Dictation orchestrator: hotkey actions -> record -> Whisper -> filters ->
//! paste, with visible status in the tray title and window title.
//!
//! Threads: one orchestrator thread owns the state machine; a recorder
//! thread owns the (non-Send) audio session; a Whisper thread owns the model,
//! loaded lazily on first use; paste runs on a short-lived thread. The hotkey
//! thread only sends a message. Dictated text and audio are never logged.

pub mod state;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::audio::{
    AudioCaptureConfig, AudioCaptureSession, MicPermissionStatus, check_microphone_permission,
    request_microphone_permission,
};
use crate::hotkey::HotkeyAction;
use crate::paste::{PasteError, paste_text_with_delay};
use crate::stt::filters::{sanitize_transcript, silence};
use crate::stt::local_whisper::LocalWhisper;
use state::{Effect, Input, Machine};

/// Id given to the tray icon so status updates can find it.
pub const TRAY_ID: &str = "deepai-tray";
const WINDOW_BASE_TITLE: &str = "DeepAI Voice";
const MODEL_FILE: &str = "models/ggml-base.en.bin";
const PASTE_SETTLE_MS: u64 = 120;
const ERROR_VISIBLE: Duration = Duration::from_secs(5);

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

/// Status text for a failed paste. The text is already in last dictation.
pub fn paste_error_status(err: &PasteError) -> String {
    match err {
        PasteError::AccessibilityDenied => "Copied: paste blocked (Accessibility)".into(),
        _ => "Copied: paste failed".into(),
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
pub fn drive(rx: Receiver<Input>, mut status: impl FnMut(&str), mut run: impl FnMut(Effect)) {
    let mut machine = Machine::new();
    for input in rx {
        let before = machine.state().clone();
        let effect = machine.handle(input);
        if *machine.state() != before {
            status(&machine.status_text());
        }
        if let Some(effect) = effect {
            run(effect);
        }
    }
}

enum RecCmd {
    Start,
    Stop,
    Discard,
}

fn recorder_thread(rx: Receiver<RecCmd>, inbox: Sender<Input>, whisper: Sender<Vec<f32>>) {
    let mut session: Option<AudioCaptureSession> = None;
    let mut last_error: Option<String> = None;
    for cmd in rx {
        match cmd {
            RecCmd::Start => {
                session = None;
                last_error = None;
                match mic_gate(check_microphone_permission()) {
                    MicGate::Go => {
                        match AudioCaptureSession::start(AudioCaptureConfig::default()) {
                            Ok(s) => session = Some(s),
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
                    let _ = inbox.send(Input::StartFailed(e.clone()));
                }
            }
            RecCmd::Stop => match session.take() {
                Some(s) => match s.stop() {
                    Ok(samples) => {
                        let _ = whisper.send(samples);
                    }
                    Err(_) => {
                        let _ = inbox.send(Input::Transcribed(Outcome::Failed(
                            "Recording failed".into(),
                        )));
                    }
                },
                None => {
                    let msg = last_error.clone().unwrap_or_else(|| "No recording".into());
                    let _ = inbox.send(Input::StartFailed(msg));
                }
            },
            RecCmd::Discard => session = None,
        }
    }
}

fn whisper_thread(rx: Receiver<Vec<f32>>, inbox: Sender<Input>) {
    let mut model: Option<LocalWhisper> = None;
    for samples in rx {
        let outcome = process_audio(&samples, |s| {
            if model.is_none() {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                let path = resolve_model_path(std::env::var("DEEPAI_MODEL_PATH").ok(), home, |p| {
                    p.is_file()
                })
                .ok_or_else(|| "Model missing".to_string())?;
                let loaded =
                    LocalWhisper::load(&path, crate::stt::local_whisper::default_threads())
                        .map_err(|_| "Model failed to load".to_string())?;
                model = Some(loaded);
            }
            match model.as_mut() {
                Some(m) => m
                    .transcribe(s)
                    .map_err(|_| "Transcription failed".to_string()),
                None => Err("Model missing".to_string()),
            }
        });
        let _ = inbox.send(Input::Transcribed(outcome));
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
    let (whisper_tx, whisper_rx) = channel::<Vec<f32>>();

    let inbox = inbox_tx.clone();
    std::thread::spawn(move || recorder_thread(rec_rx, inbox, whisper_tx));
    let inbox = inbox_tx.clone();
    std::thread::spawn(move || whisper_thread(whisper_rx, inbox));

    let inbox = inbox_tx.clone();
    std::thread::spawn(move || {
        drive(
            inbox_rx,
            |text| show_status(&app, text),
            |effect| match effect {
                Effect::StartCapture => {
                    let _ = rec_tx.send(RecCmd::Start);
                }
                Effect::StopAndTranscribe => {
                    let _ = rec_tx.send(RecCmd::Stop);
                }
                Effect::DiscardCapture => {
                    let _ = rec_tx.send(RecCmd::Discard);
                }
                Effect::ScheduleClearError => {
                    let inbox = inbox.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(ERROR_VISIBLE);
                        let _ = inbox.send(Input::ClearError);
                    });
                }
                Effect::Paste(text) => {
                    let inbox = inbox.clone();
                    std::thread::spawn(move || {
                        let result = paste_text_with_delay(&text, PASTE_SETTLE_MS).map_err(|e| {
                            // The status says "Copied", so make it true: leave the
                            // text on the clipboard for a manual Cmd+V.
                            let copied = arboard::Clipboard::new()
                                .and_then(|mut c| c.set_text(text.clone()))
                                .is_ok();
                            if copied {
                                paste_error_status(&e)
                            } else {
                                "Paste failed".to_string()
                            }
                        });
                        let _ = inbox.send(Input::Pasted(result));
                    });
                }
            },
        );
    });

    move |action| {
        let _ = inbox_tx.send(Input::Hotkey(action));
    }
}

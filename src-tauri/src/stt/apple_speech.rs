//! On-device streaming speech to text backed by the macOS Speech framework.
//!
//! Privacy rule: recognition must run on this Mac. The request always sets
//! `requiresOnDeviceRecognition`, and `start` refuses a locale that cannot run on device with
//! [`ProviderError::Unsupported`]. There is no server fallback.
//!
//! Threading: every Objective-C object of a session lives on one worker thread. The caller
//! talks to it through an mpsc command channel. The recogniser delivers results on its own
//! background `NSOperationQueue` (not the main queue, so no run loop is needed) into a block
//! that owns only `Send + Sync` data and converts results to owned Rust strings during the call.
//!
//! The pure helpers (permission mapping, event mapping, frame count, on-device decision) are
//! compiled on every platform so they can be unit tested.

use super::streaming::StreamEvent;
use super::trait_def::ProviderError;

/// Speech recognition permission, mirroring `MicPermissionStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechPermissionStatus {
    Granted,
    Denied,
    Undetermined,
}

/// Maps raw `SFSpeechRecognizerAuthorizationStatus` values.
/// 0 NotDetermined, 1 Denied, 2 Restricted, 3 Authorized. Unknown values are Denied.
pub fn map_speech_authorization_status(raw: isize) -> SpeechPermissionStatus {
    match raw {
        0 => SpeechPermissionStatus::Undetermined,
        3 => SpeechPermissionStatus::Granted,
        _ => SpeechPermissionStatus::Denied,
    }
}

/// Seam for the OS speech recognition permission query and request.
pub trait SpeechPermissionProvider: Send + Sync {
    fn status(&self) -> SpeechPermissionStatus;

    /// Asks the OS for speech recognition access. `on_done(granted)` is called once, possibly
    /// on another thread, after the user answers (or immediately if already decided).
    fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>);
}

/// What the OS can do for a locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleSupport {
    /// No recogniser for this locale.
    NotSupported,
    /// A recogniser exists but would need a server. Never used.
    ServerOnly,
    /// Recognition runs on this Mac.
    OnDevice,
}

/// Seam for the on-device capability check.
pub trait OnDeviceProbe: Send + Sync {
    fn support(&self, locale: &str) -> LocaleSupport;
}

/// Only `OnDevice` may proceed. Everything else is `Unsupported` (audio must not leave the Mac).
pub fn ensure_on_device(probe: &dyn OnDeviceProbe, locale: &str) -> Result<(), ProviderError> {
    match probe.support(locale) {
        LocaleSupport::OnDevice => Ok(()),
        other => {
            tracing::warn!(
                "apple_speech event=unsupported locale={} support={:?} reason=on_device_required",
                locale,
                other
            );
            Err(ProviderError::Unsupported)
        }
    }
}

/// True when the recogniser kept the requested locale. The OS may silently pick the keyboard
/// dictation language for an unsupported one, so the identifiers are compared.
pub fn locale_matches(requested: &str, actual: &str) -> bool {
    let norm = |s: &str| s.trim().replace('-', "_").to_ascii_lowercase();
    norm(requested) == norm(actual)
}

/// Error reported by the recogniser. Holds only the domain and code, never transcript text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizerError {
    pub domain: String,
    pub code: isize,
}

/// Maps one result callback to an event.
///
/// A final result wins (its text is complete). Otherwise an error wins over partial text.
/// A callback with neither text nor error yields nothing.
pub fn map_recognition_update(
    text: Option<String>,
    is_final: bool,
    error: Option<RecognizerError>,
) -> Option<StreamEvent> {
    match (text, is_final, error) {
        (Some(t), true, _) => Some(StreamEvent::Final(t)),
        (_, _, Some(e)) => Some(StreamEvent::Error(ProviderError::Local(format!(
            "speech recognizer error domain={} code={}",
            e.domain, e.code
        )))),
        (Some(t), false, None) => Some(StreamEvent::Partial(t)),
        (None, _, None) => None,
    }
}

/// Most phrases the recogniser accepts as `contextualStrings`.
pub const MAX_CONTEXTUAL_STRINGS: usize = 100;

/// Phrases for `contextualStrings`: trimmed, empty ones dropped,
/// case-insensitive repeats dropped, at most `MAX_CONTEXTUAL_STRINGS`.
pub fn contextual_hints(hints: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for h in hints {
        let h = h.trim();
        if h.is_empty() || out.iter().any(|o| o.to_lowercase() == h.to_lowercase()) {
            continue;
        }
        out.push(h.to_string());
        if out.len() == MAX_CONTEXTUAL_STRINGS {
            break;
        }
    }
    out
}

/// Frame count for a mono buffer holding `samples`. `None` for empty or oversized input.
pub fn pcm_frame_count(samples: &[f32]) -> Option<u32> {
    if samples.is_empty() {
        return None;
    }
    u32::try_from(samples.len()).ok()
}

#[cfg(target_os = "macos")]
pub use mac::{AppleSpeech, SfSpeechPermission, SystemOnDeviceProbe};

#[cfg(target_os = "macos")]
mod mac {
    use std::ptr::NonNull;
    use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::AnyThread;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_avf_audio::{AVAudioCommonFormat, AVAudioFormat, AVAudioPCMBuffer};
    use objc2_foundation::{NSArray, NSError, NSLocale, NSOperationQueue, NSString};
    use objc2_speech::{
        SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognitionTask,
        SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
    };

    use super::*;
    use crate::stt::streaming::{StartOptions, StreamingSession, StreamingStt};

    /// How long `finish` waits for the final result before reporting `Timeout`.
    const FINISH_TIMEOUT: Duration = Duration::from_secs(20);
    /// How long `start` waits for the worker to set up the recogniser.
    const READY_TIMEOUT: Duration = Duration::from_secs(5);

    type ResultHandler = RcBlock<dyn Fn(*mut SFSpeechRecognitionResult, *mut NSError)>;

    /// Real permission provider backed by `SFSpeechRecognizer`.
    pub struct SfSpeechPermission;

    impl SpeechPermissionProvider for SfSpeechPermission {
        fn status(&self) -> SpeechPermissionStatus {
            let raw = unsafe { SFSpeechRecognizer::authorizationStatus() };
            map_speech_authorization_status(raw.0)
        }

        fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
            // The handler is an Fn block, so the FnOnce is parked in a Mutex<Option<_>>.
            let slot = Mutex::new(Some(on_done));
            let handler = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
                if let Some(cb) = slot.lock().ok().and_then(|mut g| g.take()) {
                    cb(map_speech_authorization_status(status.0)
                        == SpeechPermissionStatus::Granted);
                }
            });
            // Needs NSSpeechRecognitionUsageDescription in Info.plist, or the OS ends the app.
            unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
        }
    }

    fn recognizer_for(locale: &str) -> Option<Retained<SFSpeechRecognizer>> {
        let ident = NSString::from_str(locale);
        let ns_locale = NSLocale::localeWithLocaleIdentifier(&ident);
        let recognizer =
            unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &ns_locale) }?;
        let actual = unsafe { recognizer.locale() }
            .localeIdentifier()
            .to_string();
        locale_matches(locale, &actual).then_some(recognizer)
    }

    fn support_of(recognizer: Option<&SFSpeechRecognizer>) -> LocaleSupport {
        match recognizer {
            None => LocaleSupport::NotSupported,
            Some(r) if unsafe { r.supportsOnDeviceRecognition() } => LocaleSupport::OnDevice,
            Some(_) => LocaleSupport::ServerOnly,
        }
    }

    /// Real on-device probe: creates a recogniser and asks `supportsOnDeviceRecognition`.
    pub struct SystemOnDeviceProbe;

    impl OnDeviceProbe for SystemOnDeviceProbe {
        fn support(&self, locale: &str) -> LocaleSupport {
            autoreleasepool(|_| support_of(recognizer_for(locale).as_deref()))
        }
    }

    fn mono_16k_format() -> Option<Retained<AVAudioFormat>> {
        unsafe {
            AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(
                AVAudioFormat::alloc(),
                AVAudioCommonFormat::PCMFormatFloat32,
                16_000.0,
                1,
                false,
            )
        }
    }

    /// Copies samples into a new buffer of `format` (must be non-interleaved mono float32).
    fn make_pcm_buffer(
        format: &AVAudioFormat,
        samples: &[f32],
    ) -> Option<Retained<AVAudioPCMBuffer>> {
        let frames = pcm_frame_count(samples)?;
        let buffer = unsafe {
            AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                format,
                frames,
            )
        }?;
        let channels = unsafe { buffer.floatChannelData() };
        if channels.is_null() {
            return None;
        }
        // SAFETY: the format is mono float32, so channel 0 points at `frames` floats owned by
        // `buffer`, which outlives this copy. Source and destination do not overlap.
        unsafe {
            let ch0: NonNull<f32> = *channels;
            std::ptr::copy_nonoverlapping(samples.as_ptr(), ch0.as_ptr(), samples.len());
            buffer.setFrameLength(frames);
        }
        Some(buffer)
    }

    /// Event sender shared by the worker and the result block. `None` once the stream ended,
    /// so the receiver disconnects and nothing is sent after `Final`, `Error` or `cancel`.
    type SharedSender = Arc<Mutex<Option<Sender<StreamEvent>>>>;

    fn deliver(shared: &SharedSender, event: StreamEvent, done: &Sender<()>) {
        let terminal = !matches!(event, StreamEvent::Partial(_));
        let Ok(mut guard) = shared.lock() else { return };
        if terminal {
            if let Some(tx) = guard.take() {
                let _ = tx.send(event);
            }
            let _ = done.send(());
        } else if let Some(tx) = guard.as_ref() {
            let _ = tx.send(event);
        }
    }

    fn close(shared: &SharedSender, last: Option<StreamEvent>) {
        if let Ok(mut guard) = shared.lock()
            && let Some(tx) = guard.take()
            && let Some(event) = last
        {
            let _ = tx.send(event);
        }
    }

    enum Command {
        Push(Vec<f32>),
        Finish,
        Cancel,
    }

    /// Objective-C objects of one session. Created, used and dropped on the worker thread.
    struct Live {
        _recognizer: Retained<SFSpeechRecognizer>,
        _queue: Retained<NSOperationQueue>,
        _handler: ResultHandler,
        request: Retained<SFSpeechAudioBufferRecognitionRequest>,
        task: Retained<SFSpeechRecognitionTask>,
        format: Retained<AVAudioFormat>,
    }

    fn setup(
        locale: &str,
        hints: &[String],
        shared: SharedSender,
        done: Sender<()>,
    ) -> Result<Live, ProviderError> {
        let recognizer = recognizer_for(locale);
        if support_of(recognizer.as_deref()) != LocaleSupport::OnDevice {
            return Err(ProviderError::Unsupported);
        }
        let recognizer = recognizer.ok_or(ProviderError::Unsupported)?;
        let format = mono_16k_format()
            .ok_or_else(|| ProviderError::Local("cannot create 16 kHz audio format".into()))?;

        let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
        unsafe {
            request.setRequiresOnDeviceRecognition(true);
            request.setShouldReportPartialResults(true);
        }
        // Dictionary words as hints. The phrases stay in memory, never logged.
        let phrases = contextual_hints(hints);
        if !phrases.is_empty() {
            let strings: Vec<Retained<NSString>> =
                phrases.iter().map(|p| NSString::from_str(p)).collect();
            let array = NSArray::from_retained_slice(&strings);
            unsafe { request.setContextualStrings(&array) };
        }

        // Results go to a private background queue instead of the main queue.
        let queue = NSOperationQueue::new();
        unsafe { recognizer.setQueue(&queue) };

        let handler: ResultHandler = RcBlock::new(
            move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
                autoreleasepool(|_| {
                    // SAFETY: both pointers are null or valid for the duration of this call.
                    // Everything needed is copied into owned Rust values before returning.
                    let result = unsafe { result.as_ref() };
                    let error = unsafe { error.as_ref() };
                    let (text, is_final) = match result {
                        Some(r) => unsafe {
                            (
                                Some(r.bestTranscription().formattedString().to_string()),
                                r.isFinal(),
                            )
                        },
                        None => (None, false),
                    };
                    let error = error.map(|e| RecognizerError {
                        domain: e.domain().to_string(),
                        code: e.code(),
                    });
                    if let Some(event) = map_recognition_update(text, is_final, error) {
                        deliver(&shared, event, &done);
                    }
                })
            },
        );
        // The task copies (retains) the block, so it stays valid after `handler` drops.
        let task =
            unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler) };

        Ok(Live {
            _recognizer: recognizer,
            _queue: queue,
            _handler: handler,
            request,
            task,
            format,
        })
    }

    fn run_worker(
        locale: String,
        hints: Vec<String>,
        commands: Receiver<Command>,
        shared: SharedSender,
        ready: SyncSender<Result<(), ProviderError>>,
    ) {
        let (done_tx, done_rx) = mpsc::channel();
        let live = match autoreleasepool(|_| setup(&locale, &hints, shared.clone(), done_tx)) {
            Ok(live) => {
                let _ = ready.send(Ok(()));
                live
            }
            Err(err) => {
                let _ = ready.send(Err(err));
                return;
            }
        };

        loop {
            match commands.recv() {
                Ok(Command::Push(samples)) => autoreleasepool(|_| {
                    if let Some(buffer) = make_pcm_buffer(&live.format, &samples) {
                        unsafe { live.request.appendAudioPCMBuffer(&buffer) };
                    }
                }),
                Ok(Command::Finish) => {
                    unsafe { live.request.endAudio() };
                    if done_rx.recv_timeout(FINISH_TIMEOUT).is_err() {
                        close(&shared, Some(StreamEvent::Error(ProviderError::Timeout)));
                        unsafe { live.task.cancel() };
                    }
                    break;
                }
                // Explicit cancel, or the session was dropped.
                Ok(Command::Cancel) | Err(_) => {
                    close(&shared, None);
                    unsafe { live.task.cancel() };
                    break;
                }
            }
        }
        autoreleasepool(|_| drop(live));
    }

    /// On-device streaming engine backed by the macOS Speech framework.
    pub struct AppleSpeech {
        probe: Box<dyn OnDeviceProbe>,
        permission: Box<dyn SpeechPermissionProvider>,
    }

    impl Default for AppleSpeech {
        fn default() -> Self {
            Self::new()
        }
    }

    impl AppleSpeech {
        pub fn new() -> Self {
            Self::with_seams(Box::new(SystemOnDeviceProbe), Box::new(SfSpeechPermission))
        }

        /// Injects the on-device probe and permission provider (tests).
        pub fn with_seams(
            probe: Box<dyn OnDeviceProbe>,
            permission: Box<dyn SpeechPermissionProvider>,
        ) -> Self {
            Self { probe, permission }
        }
    }

    impl StreamingStt for AppleSpeech {
        fn start(&self, locale: &str) -> Result<Box<dyn StreamingSession>, ProviderError> {
            self.start_with(locale, &StartOptions::default())
        }

        fn start_with(
            &self,
            locale: &str,
            options: &StartOptions,
        ) -> Result<Box<dyn StreamingSession>, ProviderError> {
            ensure_on_device(&*self.probe, locale)?;
            if self.permission.status() != SpeechPermissionStatus::Granted {
                return Err(ProviderError::Auth(
                    "speech recognition permission not granted".into(),
                ));
            }

            let (event_tx, event_rx) = mpsc::channel();
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let shared: SharedSender = Arc::new(Mutex::new(Some(event_tx)));
            let locale = locale.to_string();
            let hints = options.hints.clone();
            thread::Builder::new()
                .name("apple-speech".into())
                .spawn(move || run_worker(locale, hints, cmd_rx, shared, ready_tx))
                .map_err(|e| ProviderError::Local(format!("cannot start speech worker: {e}")))?;

            match ready_rx.recv_timeout(READY_TIMEOUT) {
                Ok(Ok(())) => Ok(Box::new(AppleSession {
                    commands: cmd_tx,
                    events: event_rx,
                    ended: false,
                })),
                Ok(Err(err)) => Err(err),
                Err(_) => {
                    let _ = cmd_tx.send(Command::Cancel);
                    Err(ProviderError::Timeout)
                }
            }
        }

        fn name(&self) -> &str {
            "apple-speech"
        }
    }

    struct AppleSession {
        commands: Sender<Command>,
        events: Receiver<StreamEvent>,
        ended: bool,
    }

    impl AppleSession {
        fn send(&self, command: Command) -> Result<(), ProviderError> {
            self.commands
                .send(command)
                .map_err(|_| ProviderError::Local("speech worker stopped".into()))
        }
    }

    impl StreamingSession for AppleSession {
        fn push_audio(&mut self, samples: &[f32]) -> Result<(), ProviderError> {
            if self.ended {
                return Err(ProviderError::BadRequest("session already ended".into()));
            }
            if samples.is_empty() {
                return Ok(());
            }
            self.send(Command::Push(samples.to_vec()))
        }

        fn events(&self) -> &Receiver<StreamEvent> {
            &self.events
        }

        fn finish(&mut self) -> Result<(), ProviderError> {
            if self.ended {
                return Err(ProviderError::BadRequest("session already ended".into()));
            }
            self.ended = true;
            self.send(Command::Finish)
        }

        fn cancel(&mut self) {
            if !self.ended {
                self.ended = true;
                let _ = self.commands.send(Command::Cancel);
            }
        }
    }

    impl Drop for AppleSession {
        fn drop(&mut self) {
            self.cancel();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn pcm_buffer_holds_the_pushed_samples() {
            let format = mono_16k_format().expect("format");
            let samples: Vec<f32> = (0..1600).map(|i| (i as f32 / 1600.0) - 0.5).collect();
            let buffer = make_pcm_buffer(&format, &samples).expect("buffer");
            assert_eq!(unsafe { buffer.frameLength() }, 1600);
            let ch0 = unsafe { *buffer.floatChannelData() };
            let copied = unsafe { std::slice::from_raw_parts(ch0.as_ptr(), 1600) };
            assert_eq!(copied, &samples[..]);
        }

        #[test]
        fn pcm_buffer_rejects_empty_input() {
            let format = mono_16k_format().expect("format");
            assert!(make_pcm_buffer(&format, &[]).is_none());
        }

        #[test]
        fn format_is_16k_mono() {
            let format = mono_16k_format().expect("format");
            assert_eq!(unsafe { format.sampleRate() }, 16_000.0);
            assert_eq!(unsafe { format.channelCount() }, 1);
        }
    }
}

//! Wraps [`LocalWhisper`] as a concrete [`SttProvider`].

use std::path::Path;
use std::sync::Mutex;

use super::local_whisper::{LocalWhisper, WhisperError};
use super::trait_def::{
    Pcm16kMono, ProviderError, SttFuture, SttOpts, SttProvider, Transcript, Usage,
};

/// Converts a [`WhisperError`] into the shared [`ProviderError`] taxonomy.
fn whisper_to_provider(e: WhisperError) -> ProviderError {
    match e {
        WhisperError::ModelMissing(p) => {
            ProviderError::Local(format!("model not found: {}", p.display()))
        }
        WhisperError::Load(msg) => ProviderError::Local(format!("model load failed: {msg}")),
        WhisperError::Transcribe(msg) => ProviderError::Local(format!("transcribe failed: {msg}")),
    }
}

/// Concrete STT provider backed by the local Whisper engine.
pub struct LocalWhisperAdapter {
    engine: Mutex<LocalWhisper>,
}

impl LocalWhisperAdapter {
    /// Loads the model from the given path and warms it up.
    pub fn load(model_path: &Path, threads: usize) -> Result<Self, ProviderError> {
        let engine = LocalWhisper::load(model_path, threads).map_err(whisper_to_provider)?;
        Ok(Self {
            engine: Mutex::new(engine),
        })
    }
}

impl SttProvider for LocalWhisperAdapter {
    fn transcribe<'a>(&'a self, audio: &'a Pcm16kMono, _opts: &'a SttOpts) -> SttFuture<'a> {
        Box::pin(async move {
            let n = audio.samples.len();
            // NOTE: whisper inference is blocking and runs on the polling thread. tokio is not a
            // direct dependency, so spawn_blocking is not used. The caller must poll this future
            // off the UI thread, and the chain timeout cannot interrupt it mid-inference.
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| ProviderError::Local("engine lock poisoned".to_string()))?;
            let t = engine
                .transcribe_timed(&audio.samples)
                .map_err(whisper_to_provider)?;
            let characters = u32::try_from(t.text.chars().count()).ok();
            Ok(Transcript {
                text: t.text,
                audio: t.audio,
                usage: Usage {
                    audio_seconds: n as f32 / 16_000.0,
                    provider_id: "local-whisper".to_string(),
                    tokens: None,
                    characters,
                },
            })
        })
    }

    fn name(&self) -> &str {
        "local-whisper"
    }
}

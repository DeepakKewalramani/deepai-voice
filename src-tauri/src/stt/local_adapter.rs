//! Wraps [`LocalWhisper`] as a concrete [`SttProvider`].

use std::path::Path;

use super::local_whisper::{LocalWhisper, WhisperError};
use super::trait_def::{Pcm16kMono, ProviderError, SttOpts, SttProvider, Transcript, Usage};

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
    engine: LocalWhisper,
}

impl LocalWhisperAdapter {
    /// Loads the model from the given path and warms it up.
    pub fn load(model_path: &Path, threads: usize) -> Result<Self, ProviderError> {
        let engine = LocalWhisper::load(model_path, threads).map_err(whisper_to_provider)?;
        Ok(Self { engine })
    }
}

impl SttProvider for LocalWhisperAdapter {
    fn transcribe(
        &mut self,
        audio: &Pcm16kMono,
        _opts: &SttOpts,
    ) -> Result<Transcript, ProviderError> {
        let n = audio.samples.len();
        let t = self
            .engine
            .transcribe_timed(&audio.samples)
            .map_err(whisper_to_provider)?;
        Ok(Transcript {
            text: t.text,
            audio: t.audio,
            usage: Usage {
                audio_seconds: n as f32 / 16_000.0,
                provider_id: "local-whisper".to_string(),
            },
        })
    }

    fn name(&self) -> &str {
        "local-whisper"
    }
}

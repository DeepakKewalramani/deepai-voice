//! Core STT provider trait, shared types, and error classification.

use std::time::Duration;

/// 16 kHz mono PCM audio buffer (f32 samples). Never written to disk.
#[derive(Debug, Clone)]
pub struct Pcm16kMono {
    /// Audio samples at 16 kHz, mono, f32.
    pub samples: Vec<f32>,
}

impl Pcm16kMono {
    /// Duration of the audio in seconds.
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / 16_000.0
    }
}

/// Options controlling transcription behaviour.
#[derive(Debug, Clone, Default)]
pub struct SttOpts {
    /// BCP-47 language tag hint, e.g. "en" or "es". `None` = auto-detect.
    pub language: Option<String>,
    /// Maximum tokens/characters to return. `None` = no limit.
    pub max_tokens: Option<usize>,
}

/// Metrics returned by a provider alongside the transcript.
#[derive(Debug, Clone)]
pub struct Usage {
    /// Audio processed in seconds.
    pub audio_seconds: f32,
    /// Provider-assigned request identifier or label.
    pub provider_id: String,
}

/// Output of a successful transcription.
#[derive(Debug, Clone)]
pub struct Transcript {
    /// Transcribed text, trimmed.
    pub text: String,
    /// Duration of the audio that was transcribed.
    pub audio: Duration,
    /// Billing / metrics for this call.
    pub usage: Usage,
}

/// Errors from any STT provider. Matches ADR 0002.
#[derive(Debug, Clone)]
pub enum ProviderError {
    /// Authentication failure (wrong API key, expired token). Retryable after key rotation.
    Auth(String),
    /// Rate limit hit. Non-retryable in the current chain cycle.
    RateLimit,
    /// Provider timed out before returning a result. Retryable.
    Timeout,
    /// Network-level failure (DNS, connection refused). Retryable.
    Network(String),
    /// Request was malformed (bad audio format, unsupported encoding). Not retryable.
    BadRequest(String),
    /// Provider internal server error. Retryable with a different provider.
    Server(String),
    /// Feature is not supported by this provider configuration.
    Unsupported,
    /// Local inference failure (model missing, hardware error). Not retryable.
    Local(String),
}

impl ProviderError {
    /// Returns `true` if this error is transient and a fallback provider should be tried.
    ///
    /// `BadRequest`, `Unsupported`, `Local`, and `RateLimit` are not retryable because
    /// the request itself is the problem and a different provider will likely fail the same way.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::Auth(_)
                | ProviderError::Timeout
                | ProviderError::Network(_)
                | ProviderError::Server(_)
        )
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Auth(msg) => write!(f, "auth error: {msg}"),
            ProviderError::RateLimit => write!(f, "rate limit"),
            ProviderError::Timeout => write!(f, "timeout"),
            ProviderError::Network(msg) => write!(f, "network error: {msg}"),
            ProviderError::BadRequest(msg) => write!(f, "bad request: {msg}"),
            ProviderError::Server(msg) => write!(f, "server error: {msg}"),
            ProviderError::Unsupported => write!(f, "unsupported"),
            ProviderError::Local(msg) => write!(f, "local inference error: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Core trait for all STT providers. Synchronous to keep Tauri commands simple.
pub trait SttProvider: Send {
    /// Transcribe 16 kHz mono audio to text.
    fn transcribe(
        &mut self,
        audio: &Pcm16kMono,
        opts: &SttOpts,
    ) -> Result<Transcript, ProviderError>;

    /// Human-readable name used in logs and error messages.
    fn name(&self) -> &str;
}

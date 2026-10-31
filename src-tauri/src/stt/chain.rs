//! Fallback provider chain for STT.
//!
//! Tries providers in order. If a provider fails with a retryable error
//! (Timeout, Network, Server, Auth) the next one is tried. Non-retryable errors
//! (BadRequest, Unsupported, Local, RateLimit) halt the chain immediately.

use super::trait_def::{Pcm16kMono, ProviderError, SttOpts, SttProvider, Transcript};

/// Executes a list of STT providers in sequence with fallback.
pub struct ProviderChain {
    providers: Vec<Box<dyn SttProvider>>,
}

impl ProviderChain {
    /// Creates a new chain. Providers are tried in the order given.
    pub fn new(providers: Vec<Box<dyn SttProvider>>) -> Self {
        Self { providers }
    }

    /// Transcribes audio using the first available provider.
    ///
    /// - Retryable errors cause fallback to the next provider.
    /// - Non-retryable errors halt the chain and are returned immediately.
    /// - If no providers are configured, returns [`ProviderError::Unsupported`].
    /// - If all providers fail with retryable errors, returns the last error.
    pub fn transcribe(
        &mut self,
        audio: &Pcm16kMono,
        opts: &SttOpts,
    ) -> Result<Transcript, ProviderError> {
        if self.providers.is_empty() {
            return Err(ProviderError::Unsupported);
        }

        let mut last_error = None;
        for provider in &mut self.providers {
            match provider.transcribe(audio, opts) {
                Ok(transcript) => {
                    tracing::info!("stt_chain event=success provider={}", provider.name());
                    return Ok(transcript);
                }
                Err(err) if !err.is_retryable() => {
                    tracing::warn!(
                        "stt_chain event=non_retryable_error provider={} error={}",
                        provider.name(),
                        err
                    );
                    return Err(err);
                }
                Err(err) => {
                    tracing::warn!(
                        "stt_chain event=retryable_error provider={} error={} trying_next=true",
                        provider.name(),
                        err
                    );
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or(ProviderError::Unsupported))
    }
}

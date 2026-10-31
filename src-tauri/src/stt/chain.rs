//! Fallback provider chain for STT.
//!
//! Tries providers in order. `Timeout`, `Network`, `Server`, `Auth`, `RateLimit` and `Local`
//! errors fall through to the next provider. `BadRequest` and `Unsupported` halt the chain.
//! Each provider call has a timeout; hitting it counts as [`ProviderError::Timeout`].

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use super::trait_def::{Pcm16kMono, ProviderError, SttOpts, SttProvider, Transcript};

/// Default per-provider timeout (ADR 0002 default `sttMs`).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(8000);

/// Executes a list of STT providers in sequence with fallback.
pub struct ProviderChain {
    providers: Vec<Box<dyn SttProvider>>,
    timeout: Duration,
}

impl ProviderChain {
    /// Creates a new chain. Providers are tried in the order given.
    pub fn new(providers: Vec<Box<dyn SttProvider>>) -> Self {
        Self {
            providers,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Sets the per-provider timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Transcribes audio using the first provider that succeeds.
    ///
    /// - Fall-through errors move on to the next provider.
    /// - `BadRequest` and `Unsupported` halt the chain and are returned immediately.
    /// - If no providers are configured, returns [`ProviderError::Unsupported`].
    /// - If all providers fail, returns the LAST error.
    pub async fn transcribe(
        &self,
        audio: &Pcm16kMono,
        opts: &SttOpts,
    ) -> Result<Transcript, ProviderError> {
        if self.providers.is_empty() {
            return Err(ProviderError::Unsupported);
        }

        let mut last_error = None;
        for provider in &self.providers {
            let result = with_timeout(self.timeout, provider.transcribe(audio, opts)).await;
            match result {
                Ok(transcript) => {
                    tracing::info!("stt_chain event=success provider={}", provider.name());
                    return Ok(transcript);
                }
                Err(err) if !err.is_retryable() => {
                    tracing::warn!(
                        "stt_chain event=halt provider={} error={}",
                        provider.name(),
                        err
                    );
                    return Err(err);
                }
                Err(err) => {
                    tracing::warn!(
                        "stt_chain event=fall_through provider={} error={}",
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

/// Shared state between a [`Timeout`] future and its timer thread.
#[derive(Default)]
struct TimerState {
    fired: bool,
    waker: Option<Waker>,
}

/// Races a future against a timer. Std only, because tokio is not a direct dependency.
/// The timer thread sleeps for the full duration even if the future finishes early.
struct Timeout<F> {
    fut: Pin<Box<F>>,
    duration: Duration,
    state: Option<Arc<Mutex<TimerState>>>,
}

fn with_timeout<F>(duration: Duration, fut: F) -> Timeout<F>
where
    F: Future<Output = Result<Transcript, ProviderError>>,
{
    Timeout {
        fut: Box::pin(fut),
        duration,
        state: None,
    }
}

impl<F> Future for Timeout<F>
where
    F: Future<Output = Result<Transcript, ProviderError>>,
{
    type Output = Result<Transcript, ProviderError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        if let Poll::Ready(v) = this.fut.as_mut().poll(cx) {
            return Poll::Ready(v);
        }
        let state = match &this.state {
            Some(s) => s.clone(),
            None => {
                let s = Arc::new(Mutex::new(TimerState::default()));
                let timer = s.clone();
                let d = this.duration;
                std::thread::spawn(move || {
                    std::thread::sleep(d);
                    let mut g = timer.lock().unwrap_or_else(|e| e.into_inner());
                    g.fired = true;
                    if let Some(w) = g.waker.take() {
                        w.wake();
                    }
                });
                this.state = Some(s.clone());
                s
            }
        };
        let mut g = state.lock().unwrap_or_else(|e| e.into_inner());
        if g.fired {
            return Poll::Ready(Err(ProviderError::Timeout));
        }
        g.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::Duration;

use deepai_voice_lib::stt::chain::ProviderChain;
use deepai_voice_lib::stt::trait_def::{
    Pcm16kMono, ProviderError, SttFuture, SttOpts, SttProvider, Transcript, Usage,
};

// ---------------------------------------------------------------------------
// Minimal executor (no async runtime dependency)
// ---------------------------------------------------------------------------

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = Box::pin(fut);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        thread::park();
    }
}

// ---------------------------------------------------------------------------
// Mock providers
// ---------------------------------------------------------------------------

/// A mock that always succeeds with a fixed transcript.
struct SucceedingProvider {
    text: String,
    call_count: Arc<AtomicUsize>,
}

impl SucceedingProvider {
    fn new(text: &str) -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                text: text.to_string(),
                call_count: count.clone(),
            },
            count,
        )
    }
}

impl SttProvider for SucceedingProvider {
    fn transcribe<'a>(&'a self, _audio: &'a Pcm16kMono, _opts: &'a SttOpts) -> SttFuture<'a> {
        Box::pin(async move {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(Transcript {
                text: self.text.clone(),
                audio: Duration::from_secs(1),
                usage: Usage {
                    audio_seconds: 1.0,
                    provider_id: "mock-ok".to_string(),
                    ..Usage::default()
                },
            })
        })
    }

    fn name(&self) -> &str {
        "SucceedingProvider"
    }
}

/// A mock that always fails with a configurable error.
struct FailingProvider {
    error: ProviderError,
    call_count: Arc<AtomicUsize>,
}

impl FailingProvider {
    fn new(error: ProviderError) -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                error,
                call_count: count.clone(),
            },
            count,
        )
    }
}

impl SttProvider for FailingProvider {
    fn transcribe<'a>(&'a self, _audio: &'a Pcm16kMono, _opts: &'a SttOpts) -> SttFuture<'a> {
        Box::pin(async move {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Err(self.error.clone())
        })
    }

    fn name(&self) -> &str {
        "FailingProvider"
    }
}

// ---------------------------------------------------------------------------
// Chain tests
// ---------------------------------------------------------------------------

#[test]
fn chain_succeeds_with_first_provider() {
    let (primary, primary_calls) = SucceedingProvider::new("hello world");
    let (secondary, secondary_calls) = SucceedingProvider::new("fallback");

    let chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = block_on(chain.transcribe(&audio, &opts)).unwrap();
    assert_eq!(result.text, "hello world");
    assert_eq!(primary_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        secondary_calls.load(Ordering::SeqCst),
        0,
        "secondary must not be called when primary succeeds"
    );
}

#[test]
fn chain_falls_back_on_server_error() {
    let (primary, primary_calls) =
        FailingProvider::new(ProviderError::Server("internal error".to_string()));
    let (secondary, secondary_calls) = SucceedingProvider::new("fallback result");

    let chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = block_on(chain.transcribe(&audio, &opts)).unwrap();
    assert_eq!(result.text, "fallback result");
    assert_eq!(primary_calls.load(Ordering::SeqCst), 1);
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_falls_back_on_timeout_error() {
    let (primary, _) = FailingProvider::new(ProviderError::Timeout);
    let (secondary, secondary_calls) = SucceedingProvider::new("after timeout fallback");

    let chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = block_on(chain.transcribe(&audio, &opts)).unwrap();
    assert_eq!(result.text, "after timeout fallback");
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_falls_back_on_network_error() {
    let (primary, _) =
        FailingProvider::new(ProviderError::Network("connection refused".to_string()));
    let (secondary, secondary_calls) = SucceedingProvider::new("network fallback");

    let chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = block_on(chain.transcribe(&audio, &opts)).unwrap();
    assert_eq!(result.text, "network fallback");
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_halts_on_bad_request_and_does_not_fallback() {
    let (primary, primary_calls) =
        FailingProvider::new(ProviderError::BadRequest("invalid audio".to_string()));
    let (secondary, secondary_calls) = SucceedingProvider::new("should not be called");

    let chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let err = block_on(chain.transcribe(&audio, &opts)).unwrap_err();
    assert!(matches!(err, ProviderError::BadRequest(_)));
    assert_eq!(primary_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        secondary_calls.load(Ordering::SeqCst),
        0,
        "secondary must NOT be called on BadRequest"
    );
}

#[test]
fn chain_returns_error_when_all_providers_fail() {
    let (p1, _) = FailingProvider::new(ProviderError::Server("err1".to_string()));
    let (p2, _) = FailingProvider::new(ProviderError::Timeout);

    let chain = ProviderChain::new(vec![Box::new(p1), Box::new(p2)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let err = block_on(chain.transcribe(&audio, &opts)).unwrap_err();
    assert!(matches!(err, ProviderError::Timeout), "got {err:?}");
}

#[test]
fn chain_empty_returns_unsupported() {
    let chain = ProviderChain::new(vec![]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let err = block_on(chain.transcribe(&audio, &opts)).unwrap_err();
    assert!(matches!(err, ProviderError::Unsupported));
}

#[test]
fn pcm_audio_sample_count() {
    let audio = Pcm16kMono {
        samples: vec![0.5; 32000],
    };
    assert_eq!(audio.duration_secs(), 2.0);
}

#[test]
fn provider_error_is_retryable_classification() {
    assert!(ProviderError::Timeout.is_retryable());
    assert!(ProviderError::Network("x".into()).is_retryable());
    assert!(ProviderError::Server("x".into()).is_retryable());
    assert!(ProviderError::Auth("x".into()).is_retryable());

    assert!(!ProviderError::BadRequest("x".into()).is_retryable());
    assert!(!ProviderError::Unsupported.is_retryable());
    // Changed 31 Oct 2026 (human approved): Local and RateLimit now fall through.
    assert!(ProviderError::Local("x".into()).is_retryable());
    assert!(ProviderError::RateLimit.is_retryable());
}

// ---------------------------------------------------------------------------
// New fallback rules (approved 31 Oct 2026) and timeout
// ---------------------------------------------------------------------------

fn audio() -> Pcm16kMono {
    Pcm16kMono {
        samples: vec![0.0; 16000],
    }
}

#[test]
fn chain_local_failure_falls_through_to_cloud() {
    let (local, local_calls) = FailingProvider::new(ProviderError::Local("model missing".into()));
    let (cloud, cloud_calls) = SucceedingProvider::new("cloud text");
    let chain = ProviderChain::new(vec![Box::new(local), Box::new(cloud)]);
    let out = block_on(chain.transcribe(&audio(), &SttOpts::default())).unwrap();
    assert_eq!(out.text, "cloud text");
    assert_eq!(local_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cloud_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_rate_limit_falls_through() {
    let (p1, _) = FailingProvider::new(ProviderError::RateLimit);
    let (p2, p2_calls) = SucceedingProvider::new("second");
    let chain = ProviderChain::new(vec![Box::new(p1), Box::new(p2)]);
    let out = block_on(chain.transcribe(&audio(), &SttOpts::default())).unwrap();
    assert_eq!(out.text, "second");
    assert_eq!(p2_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_all_fail_returns_last_error_variant() {
    let (p1, _) = FailingProvider::new(ProviderError::Local("first".into()));
    let (p2, _) = FailingProvider::new(ProviderError::RateLimit);
    let (p3, _) = FailingProvider::new(ProviderError::Network("last".into()));
    let chain = ProviderChain::new(vec![Box::new(p1), Box::new(p2), Box::new(p3)]);
    let err = block_on(chain.transcribe(&audio(), &SttOpts::default())).unwrap_err();
    assert!(
        matches!(err, ProviderError::Network(ref m) if m == "last"),
        "got {err:?}"
    );
}

/// A provider whose future never completes.
struct HangingProvider;

impl SttProvider for HangingProvider {
    fn transcribe<'a>(&'a self, _audio: &'a Pcm16kMono, _opts: &'a SttOpts) -> SttFuture<'a> {
        Box::pin(std::future::pending())
    }

    fn name(&self) -> &str {
        "HangingProvider"
    }
}

#[test]
fn chain_timeout_falls_through() {
    let (ok, ok_calls) = SucceedingProvider::new("after hang");
    let chain = ProviderChain::new(vec![Box::new(HangingProvider), Box::new(ok)])
        .with_timeout(Duration::from_millis(50));
    let out = block_on(chain.transcribe(&audio(), &SttOpts::default())).unwrap();
    assert_eq!(out.text, "after hang");
    assert_eq!(ok_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_timeout_alone_returns_timeout_error() {
    let chain =
        ProviderChain::new(vec![Box::new(HangingProvider)]).with_timeout(Duration::from_millis(50));
    let err = block_on(chain.transcribe(&audio(), &SttOpts::default())).unwrap_err();
    assert!(matches!(err, ProviderError::Timeout), "got {err:?}");
}

#[test]
fn usage_has_optional_token_and_char_metrics() {
    let u = Usage::default();
    assert_eq!(u.tokens, None);
    assert_eq!(u.characters, None);
}

#[test]
fn providers_are_object_safe_send_sync() {
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn SttProvider>();
    assert_send_sync::<ProviderChain>();
}

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use deepai_voice_lib::stt::chain::ProviderChain;
use deepai_voice_lib::stt::trait_def::{
    Pcm16kMono, ProviderError, SttOpts, SttProvider, Transcript, Usage,
};

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
    fn transcribe(
        &mut self,
        _audio: &Pcm16kMono,
        _opts: &SttOpts,
    ) -> Result<Transcript, ProviderError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(Transcript {
            text: self.text.clone(),
            audio: Duration::from_secs(1),
            usage: Usage {
                audio_seconds: 1.0,
                provider_id: "mock-ok".to_string(),
            },
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
    fn transcribe(
        &mut self,
        _audio: &Pcm16kMono,
        _opts: &SttOpts,
    ) -> Result<Transcript, ProviderError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Err(self.error.clone())
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

    let mut chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = chain.transcribe(&audio, &opts).unwrap();
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

    let mut chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = chain.transcribe(&audio, &opts).unwrap();
    assert_eq!(result.text, "fallback result");
    assert_eq!(primary_calls.load(Ordering::SeqCst), 1);
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_falls_back_on_timeout_error() {
    let (primary, _) = FailingProvider::new(ProviderError::Timeout);
    let (secondary, secondary_calls) = SucceedingProvider::new("after timeout fallback");

    let mut chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = chain.transcribe(&audio, &opts).unwrap();
    assert_eq!(result.text, "after timeout fallback");
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_falls_back_on_network_error() {
    let (primary, _) =
        FailingProvider::new(ProviderError::Network("connection refused".to_string()));
    let (secondary, secondary_calls) = SucceedingProvider::new("network fallback");

    let mut chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let result = chain.transcribe(&audio, &opts).unwrap();
    assert_eq!(result.text, "network fallback");
    assert_eq!(secondary_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn chain_halts_on_bad_request_and_does_not_fallback() {
    let (primary, primary_calls) =
        FailingProvider::new(ProviderError::BadRequest("invalid audio".to_string()));
    let (secondary, secondary_calls) = SucceedingProvider::new("should not be called");

    let mut chain = ProviderChain::new(vec![Box::new(primary), Box::new(secondary)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let err = chain.transcribe(&audio, &opts).unwrap_err();
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

    let mut chain = ProviderChain::new(vec![Box::new(p1), Box::new(p2)]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    assert!(chain.transcribe(&audio, &opts).is_err());
}

#[test]
fn chain_empty_returns_unsupported() {
    let mut chain = ProviderChain::new(vec![]);
    let audio = Pcm16kMono {
        samples: vec![0.0; 16000],
    };
    let opts = SttOpts::default();

    let err = chain.transcribe(&audio, &opts).unwrap_err();
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
    assert!(!ProviderError::Local("x".into()).is_retryable());
    assert!(!ProviderError::RateLimit.is_retryable());
}

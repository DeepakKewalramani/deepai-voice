//! Streaming STT tests (slice 2.11). Pure mapping tests, the fake engine, the on-device seam,
//! and one ignored real run against the macOS Speech framework.

use std::time::Duration;

use deepai_voice_lib::stt::apple_speech::{
    LocaleSupport, OnDeviceProbe, RecognizerError, SpeechPermissionProvider,
    SpeechPermissionStatus, ensure_on_device, locale_matches, map_recognition_update,
    map_speech_authorization_status, pcm_frame_count,
};
use deepai_voice_lib::stt::streaming::{
    FakeStreamingStt, StreamEvent, StreamingStt, collect_final,
};
use deepai_voice_lib::stt::trait_def::ProviderError;

// ---- permission mapping ----

#[test]
fn speech_permission_maps_every_raw_status() {
    assert_eq!(
        map_speech_authorization_status(0),
        SpeechPermissionStatus::Undetermined
    );
    assert_eq!(
        map_speech_authorization_status(1),
        SpeechPermissionStatus::Denied
    );
    assert_eq!(
        map_speech_authorization_status(2),
        SpeechPermissionStatus::Denied
    );
    assert_eq!(
        map_speech_authorization_status(3),
        SpeechPermissionStatus::Granted
    );
    assert_eq!(
        map_speech_authorization_status(99),
        SpeechPermissionStatus::Denied
    );
    assert_eq!(
        map_speech_authorization_status(-1),
        SpeechPermissionStatus::Denied
    );
}

// ---- event mapping ----

fn err() -> Option<RecognizerError> {
    Some(RecognizerError {
        domain: "TestDomain".into(),
        code: 7,
    })
}

#[test]
fn partial_result_maps_to_partial() {
    let ev = map_recognition_update(Some("hello".into()), false, None);
    assert!(matches!(ev, Some(StreamEvent::Partial(t)) if t == "hello"));
}

#[test]
fn final_result_maps_to_final_even_with_error() {
    let ev = map_recognition_update(Some("hello world".into()), true, None);
    assert!(matches!(ev, Some(StreamEvent::Final(t)) if t == "hello world"));
    let ev = map_recognition_update(Some("hello world".into()), true, err());
    assert!(matches!(ev, Some(StreamEvent::Final(_))));
}

#[test]
fn error_maps_to_local_error_without_text() {
    let ev = map_recognition_update(Some("secret words".into()), false, err());
    match ev {
        Some(StreamEvent::Error(ProviderError::Local(msg))) => {
            assert!(msg.contains("TestDomain") && msg.contains('7'));
            assert!(
                !msg.contains("secret"),
                "error must not carry transcript text"
            );
        }
        other => panic!("expected Local error, got {other:?}"),
    }
}

#[test]
fn empty_callback_maps_to_nothing() {
    assert!(map_recognition_update(None, false, None).is_none());
    assert!(map_recognition_update(None, true, None).is_none());
}

// ---- buffer helper ----

#[test]
fn frame_count_matches_samples_and_rejects_empty() {
    assert_eq!(pcm_frame_count(&[0.0; 1600]), Some(1600));
    assert_eq!(pcm_frame_count(&[0.5]), Some(1));
    assert_eq!(pcm_frame_count(&[]), None);
}

// ---- on-device seam ----

struct Probe(LocaleSupport);
impl OnDeviceProbe for Probe {
    fn support(&self, _locale: &str) -> LocaleSupport {
        self.0
    }
}

#[test]
fn only_on_device_locales_are_allowed() {
    assert!(ensure_on_device(&Probe(LocaleSupport::OnDevice), "en-US").is_ok());
    assert!(matches!(
        ensure_on_device(&Probe(LocaleSupport::ServerOnly), "en-US"),
        Err(ProviderError::Unsupported)
    ));
    assert!(matches!(
        ensure_on_device(&Probe(LocaleSupport::NotSupported), "xx-XX"),
        Err(ProviderError::Unsupported)
    ));
}

#[test]
fn locale_match_ignores_separator_and_case() {
    assert!(locale_matches("en-US", "en_US"));
    assert!(locale_matches("en-us", "en_US"));
    assert!(!locale_matches("en-US", "en_GB"));
    assert!(!locale_matches("xx-XX", "en_US"));
}

#[cfg(target_os = "macos")]
mod apple_seams {
    use super::*;
    use deepai_voice_lib::stt::apple_speech::AppleSpeech;

    struct Perm(SpeechPermissionStatus);
    impl SpeechPermissionProvider for Perm {
        fn status(&self) -> SpeechPermissionStatus {
            self.0
        }
        fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
            on_done(self.0 == SpeechPermissionStatus::Granted);
        }
    }

    #[test]
    fn apple_speech_refuses_server_only_locale() {
        let engine = AppleSpeech::with_seams(
            Box::new(Probe(LocaleSupport::ServerOnly)),
            Box::new(Perm(SpeechPermissionStatus::Granted)),
        );
        assert!(matches!(
            engine.start("en-US"),
            Err(ProviderError::Unsupported)
        ));
    }

    #[test]
    fn apple_speech_refuses_without_permission() {
        let engine = AppleSpeech::with_seams(
            Box::new(Probe(LocaleSupport::OnDevice)),
            Box::new(Perm(SpeechPermissionStatus::Denied)),
        );
        assert!(matches!(engine.start("en-US"), Err(ProviderError::Auth(_))));
    }
}

// ---- fake engine driving a session ----

#[test]
fn fake_session_streams_partials_then_final() {
    let engine = FakeStreamingStt::new(&["hello", "there", "world"]);
    let mut session = engine.start("en-US").unwrap();
    session.push_audio(&[0.1; 160]).unwrap();
    session.push_audio(&[0.1; 160]).unwrap();
    let rx = session.events();
    assert!(matches!(rx.try_recv(), Ok(StreamEvent::Partial(t)) if t == "hello"));
    assert!(matches!(rx.try_recv(), Ok(StreamEvent::Partial(t)) if t == "hello there"));
    session.finish().unwrap();
    let text = collect_final(session.events(), Duration::from_secs(1)).unwrap();
    assert_eq!(text, "hello there world");
    assert!(
        session.push_audio(&[0.1]).is_err(),
        "push after finish must fail"
    );
    assert!(session.finish().is_err(), "second finish must fail");
}

#[test]
fn fake_session_cancel_stops_events() {
    let engine = FakeStreamingStt::new(&["a", "b"]);
    let mut session = engine.start("en-US").unwrap();
    session.cancel();
    assert!(session.push_audio(&[0.1]).is_err());
    assert!(matches!(
        collect_final(session.events(), Duration::from_millis(50)),
        Err(ProviderError::Local(_))
    ));
}

#[test]
fn fake_errors_surface_from_start_and_finish() {
    let engine = FakeStreamingStt::new(&[]).failing_start(ProviderError::Unsupported);
    assert!(matches!(
        engine.start("en-US"),
        Err(ProviderError::Unsupported)
    ));

    let engine = FakeStreamingStt::new(&["x"]).failing_finish(ProviderError::Timeout);
    let mut session = engine.start("en-US").unwrap();
    session.finish().unwrap();
    assert!(matches!(
        collect_final(session.events(), Duration::from_secs(1)),
        Err(ProviderError::Timeout)
    ));
}

#[test]
fn collect_final_times_out_without_events() {
    let engine = FakeStreamingStt::new(&["x"]);
    let session = engine.start("en-US").unwrap();
    assert!(matches!(
        collect_final(session.events(), Duration::from_millis(20)),
        Err(ProviderError::Timeout)
    ));
}

// ---- real engine (needs speech recognition permission in a user session) ----

/// Word-level similarity: 1 - edit distance / longer length, on lowercase alphanumeric words.
fn similarity(a: &str, b: &str) -> f32 {
    let words = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (a, b) = (words(a), words(b));
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, wa) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, wb) in b.iter().enumerate() {
            let cost = usize::from(wa != wb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f32 / longest as f32
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs macOS speech recognition permission and on-device en-US assets"]
fn apple_speech_real_fixtures() {
    use deepai_voice_lib::stt::apple_speech::{AppleSpeech, SfSpeechPermission};

    let status = SfSpeechPermission.status();
    assert_eq!(
        status,
        SpeechPermissionStatus::Granted,
        "speech recognition permission is {status:?}; grant it in a user session first"
    );

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/audio");
    let fixtures = [
        (
            "01_english_greeting.wav",
            "Hello world, this is a test of voice dictation.",
        ),
        (
            "02_english_email.wav",
            "Please review the pull request and let me know your thoughts.",
        ),
        (
            "04_technical_terms.wav",
            "DeepAI Voice runs on Tauri version two with Rust and React.",
        ),
        (
            "05_numbers_and_punctuation.wav",
            "Meeting at three PM with twelve engineers in room four.",
        ),
    ];
    let engine = AppleSpeech::new();
    let mut failures = 0;
    for (file, expected) in fixtures {
        let mut reader = hound::WavReader::open(dir.join(file)).expect("fixture");
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        let mut session = engine.start("en-US").expect("start");
        for chunk in samples.chunks(1600) {
            session.push_audio(chunk).expect("push");
        }
        session.finish().expect("finish");
        let score = match collect_final(session.events(), Duration::from_secs(30)) {
            Ok(text) => similarity(&text, expected),
            Err(e) => {
                println!("{file}: error {e}");
                0.0
            }
        };
        let pass = score >= 0.6;
        failures += usize::from(!pass);
        println!(
            "{file}: {} similarity={score:.2}",
            if pass { "pass" } else { "fail" }
        );
    }
    assert_eq!(failures, 0, "{failures} fixture(s) below similarity 0.6");
}

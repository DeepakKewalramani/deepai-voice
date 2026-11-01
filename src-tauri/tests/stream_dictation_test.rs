// Tests for slice 2.12: engine selection, streaming partial to typed delta,
// session restart at a pause or 50 s, Esc during streaming, fallbacks.
// FakeStreamingStt only: no mic, no OS speech engine. Text is test data.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use deepai_voice_lib::dictation::Outcome;
use deepai_voice_lib::dictation::engine::{
    EngineChoice, EngineSetting, Fallback, choose_engine, should_request_permission,
};
use deepai_voice_lib::dictation::state::{DictationState as S, Effect, Input, Machine};
use deepai_voice_lib::dictation::stream_hold::{
    RESTART_MAX_SAMPLES, RESTART_PAUSE_SAMPLES, STREAM_FAILED, StreamHold,
};
use deepai_voice_lib::dictation::stream_text::{SegmentTyping, eager_stable_text};
use deepai_voice_lib::hotkey::HotkeyAction;
use deepai_voice_lib::stt::apple_speech::{LocaleSupport, SpeechPermissionStatus as P};
use deepai_voice_lib::stt::streaming::{
    FakeStreamingStt, StreamEvent, StreamingSession, StreamingStt,
};
use deepai_voice_lib::stt::trait_def::ProviderError;

// ---------- engine selection ----------

#[test]
fn auto_with_permission_and_on_device_streams() {
    assert_eq!(
        choose_engine(EngineSetting::Auto, P::Granted, || LocaleSupport::OnDevice),
        EngineChoice::Streaming
    );
}

#[test]
fn every_fallback_has_a_fixed_reason() {
    let w = |f| EngineChoice::Whisper(f);
    let probe_never = || -> LocaleSupport { panic!("probe must not run") };
    assert_eq!(
        choose_engine(EngineSetting::Whisper, P::Granted, probe_never),
        w(Fallback::Setting)
    );
    assert_eq!(
        choose_engine(EngineSetting::Auto, P::Undetermined, probe_never),
        w(Fallback::PermissionPending)
    );
    assert_eq!(
        choose_engine(EngineSetting::Auto, P::Denied, probe_never),
        w(Fallback::PermissionDenied)
    );
    for s in [LocaleSupport::NotSupported, LocaleSupport::ServerOnly] {
        assert_eq!(
            choose_engine(EngineSetting::Auto, P::Granted, || s),
            w(Fallback::NotOnDevice)
        );
    }
    let all = [
        Fallback::Setting,
        Fallback::PermissionPending,
        Fallback::PermissionDenied,
        Fallback::NotOnDevice,
        Fallback::StartFailed,
    ];
    let mut texts: Vec<&str> = all.iter().map(|f| f.status()).collect();
    for t in &texts {
        assert!(t.starts_with("Whisper: "), "{t}");
        assert!(!t.contains('\u{2014}'));
    }
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), all.len());
}

#[test]
fn setting_reads_env_value_and_defaults_to_auto() {
    assert_eq!(EngineSetting::default(), EngineSetting::Auto);
    assert_eq!(EngineSetting::from_env_value(None), EngineSetting::Auto);
    assert_eq!(EngineSetting::from_env_value(Some("")), EngineSetting::Auto);
    assert_eq!(
        EngineSetting::from_env_value(Some("auto")),
        EngineSetting::Auto
    );
    assert_eq!(
        EngineSetting::from_env_value(Some(" Whisper ")),
        EngineSetting::Whisper
    );
}

#[test]
fn permission_is_requested_once_and_only_from_the_bundled_app() {
    let a = EngineSetting::Auto;
    assert!(should_request_permission(a, P::Undetermined, true, false));
    assert!(!should_request_permission(a, P::Undetermined, true, true));
    assert!(!should_request_permission(a, P::Undetermined, false, false));
    assert!(!should_request_permission(a, P::Denied, true, false));
    assert!(!should_request_permission(a, P::Granted, true, false));
    assert!(!should_request_permission(
        EngineSetting::Whisper,
        P::Undetermined,
        true,
        false
    ));
}

#[test]
fn start_failure_means_whisper_for_the_hold() {
    let fake = FakeStreamingStt::new(&["hi"]).failing_start(ProviderError::Unsupported);
    assert!(StreamHold::start(Arc::new(fake), "en-US").is_err());
    let denied = FakeStreamingStt::new(&["hi"]).failing_start(ProviderError::Auth("no".into()));
    assert!(StreamHold::start(Arc::new(denied), "en-US").is_err());
}

// ---------- eager typing rule ----------

#[test]
fn eager_rule_types_every_word_but_the_last() {
    assert_eq!(eager_stable_text(""), "");
    assert_eq!(eager_stable_text("hello"), "");
    assert_eq!(eager_stable_text("hello wor"), "hello");
    assert_eq!(eager_stable_text(" hello  big world "), "hello big");
}

#[test]
fn eager_segment_types_deltas_and_never_more_than_the_stable_prefix() {
    let mut t = SegmentTyping::eager(false);
    assert_eq!(t.on_hypothesis("hello"), None);
    assert_eq!(t.on_hypothesis("hello wor").as_deref(), Some("hello"));
    assert_eq!(
        t.on_hypothesis("hello world how").as_deref(),
        Some(" world")
    );
    // A revision of an already typed word types nothing until the final.
    assert_eq!(t.on_hypothesis("yellow world how are"), None);
    assert_eq!(t.typed(), "hello world");
    let edit = t.on_final(Some("yellow world how are you"));
    assert_eq!(edit.backspaces, "hello world".len());
    assert_eq!(edit.text, "yellow world how are you");
}

fn rec_machine() -> Machine {
    let mut m = Machine::new();
    m.handle(Input::Hotkey(HotkeyAction::StartRecording));
    m
}

#[test]
fn stream_partials_type_at_once_and_final_reconciles_within_the_utterance() {
    let mut m = rec_machine();
    let sp = |seq, t: &str| Input::StreamPartial {
        seq,
        text: t.into(),
    };
    assert_eq!(
        m.handle(sp(1, "send the")),
        Some(Effect::Paste("send".into()))
    );
    assert_eq!(m.partial(), "the");
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(
        m.handle(sp(1, "send the pool request")),
        Some(Effect::Paste(" the pool".into()))
    );
    m.handle(Input::Pasted(Ok(())));
    let fin = m.handle(Input::Segment {
        seq: 1,
        outcome: Outcome::Text("send the pull request".into()),
    });
    assert_eq!(
        fin,
        Some(Effect::Correct {
            backspaces: 3,
            text: "ull request".into()
        })
    );
    m.handle(Input::Pasted(Ok(())));
    // The next session starts with one space and never deletes the last one.
    assert_eq!(
        m.handle(sp(2, "now go")),
        Some(Effect::Paste(" now".into()))
    );
}

#[test]
fn esc_during_streaming_drops_late_events_and_sends_no_backspace() {
    let mut m = rec_machine();
    let hold = m.hold_id();
    m.handle(Input::StreamPartial {
        seq: 1,
        text: "one two".into(),
    });
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(m.handle(Input::Cancel), Some(Effect::CancelHold));
    assert_eq!(*m.state(), S::Cancelled);
    let late = [
        Input::StreamPartial {
            seq: 1,
            text: "one two three four".into(),
        },
        Input::Segment {
            seq: 1,
            outcome: Outcome::Text("one too three".into()),
        },
        Input::Transcribed(Outcome::Text("x".into())),
    ];
    for input in late {
        let tagged = Input::FromHold {
            hold,
            input: Box::new(input),
        };
        assert_eq!(m.handle(tagged), None);
        assert!(m.take_followup().is_none());
    }
    // A late event of the old hold during the next hold is dropped too.
    m.handle(Input::Hotkey(HotkeyAction::StartRecording));
    let old = Input::FromHold {
        hold,
        input: Box::new(Input::StreamPartial {
            seq: 1,
            text: "stale words here".into(),
        }),
    };
    assert_eq!(m.handle(old), None);
}

// ---------- stream hold over the fake engine ----------

const VOICE: f32 = 0.2;

fn voice(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| if i % 2 == 0 { VOICE } else { -VOICE })
        .collect()
}

fn silence(n: usize) -> Vec<f32> {
    vec![0.0; n]
}

fn hold_over(words: &[&str]) -> StreamHold {
    StreamHold::start(Arc::new(FakeStreamingStt::new(words)), "en-US").unwrap()
}

fn partials(inputs: &[Input]) -> Vec<(u64, String)> {
    inputs
        .iter()
        .filter_map(|i| match i {
            Input::StreamPartial { seq, text } => Some((*seq, text.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn partials_flow_into_the_machine_and_type_live() {
    let mut h = hold_over(&["hello", "there", "friend"]);
    let mut m = rec_machine();
    let mut typed = String::new();
    for _ in 0..3 {
        for input in h.feed(&voice(1600)) {
            if let Some(Effect::Paste(t)) = m.handle(input) {
                typed.push_str(&t);
                m.handle(Input::Pasted(Ok(())));
            }
        }
    }
    assert_eq!(typed, "hello there");
    assert_eq!(m.partial(), "friend");
    m.handle(Input::Hotkey(HotkeyAction::StopRecording));
    let rel = h.release(&[]);
    assert_eq!(
        rel,
        vec![Input::Transcribed(Outcome::Text(
            "hello there friend".into()
        ))]
    );
    assert!(h.is_done());
    for input in rel {
        if let Some(Effect::Paste(t)) = m.handle(input) {
            typed.push_str(&t);
            m.handle(Input::Pasted(Ok(())));
        }
    }
    assert_eq!(typed, "hello there friend");
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn session_restarts_at_a_pause_after_text() {
    let mut h = hold_over(&["one", "two", "three"]);
    let first = h.feed(&voice(1600));
    assert_eq!(partials(&first), vec![(1, "one".to_string())]);
    // Silence just under the pause: same session.
    let quiet = h.feed(&silence(RESTART_PAUSE_SAMPLES - 320));
    assert_eq!(h.seq(), 1);
    assert_eq!(partials(&quiet), vec![(1, "one two".to_string())]);
    // Reaching the pause finishes session 1 and opens session 2.
    let cut = h.feed(&silence(320));
    assert_eq!(h.seq(), 2);
    assert!(cut.contains(&Input::Segment {
        seq: 1,
        outcome: Outcome::Text("one two three".into())
    }));
    let next = h.feed(&voice(1600));
    assert_eq!(partials(&next), vec![(2, "one".to_string())]);
}

#[test]
fn silence_without_text_does_not_restart() {
    let mut h = hold_over(&[]);
    h.feed(&silence(RESTART_PAUSE_SAMPLES * 3));
    assert_eq!(h.seq(), 1);
}

#[test]
fn session_restarts_after_fifty_seconds_of_speech() {
    let mut h = hold_over(&["a", "b"]);
    // Speech-like: 900 ms of voice and a 100 ms gap every second.
    let chunk = [voice(14_400), silence(1_600)].concat();
    for _ in 0..(RESTART_MAX_SAMPLES / 16_000 - 1) {
        h.feed(&chunk);
    }
    assert_eq!(h.seq(), 1);
    let out = h.feed(&chunk);
    assert_eq!(h.seq(), 2);
    assert!(
        out.iter()
            .any(|i| matches!(i, Input::Segment { seq: 1, .. }))
    );
}

/// Engine whose sessions finish only when the test says so, to check order.
struct Manual {
    finals: Arc<std::sync::Mutex<Vec<Sender<StreamEvent>>>>,
    started: AtomicUsize,
    fail_after: usize,
}

struct ManualSession {
    tx: Option<Sender<StreamEvent>>,
    rx: Receiver<StreamEvent>,
    pushed: usize,
    finals: Arc<std::sync::Mutex<Vec<Sender<StreamEvent>>>>,
    words: Vec<&'static str>,
}

impl StreamingStt for Manual {
    fn start(&self, _: &str) -> Result<Box<dyn StreamingSession>, ProviderError> {
        if self.started.fetch_add(1, Ordering::SeqCst) >= self.fail_after {
            return Err(ProviderError::Unsupported);
        }
        let (tx, rx) = mpsc::channel();
        Ok(Box::new(ManualSession {
            tx: Some(tx),
            rx,
            pushed: 0,
            finals: self.finals.clone(),
            words: vec!["alpha", "beta", "gamma"],
        }))
    }
    fn name(&self) -> &str {
        "manual"
    }
}

impl StreamingSession for ManualSession {
    fn push_audio(&mut self, s: &[f32]) -> Result<(), ProviderError> {
        let tx = self.tx.as_ref().ok_or(ProviderError::Unsupported)?;
        if !s.is_empty() && self.pushed < self.words.len() {
            self.pushed += 1;
            let _ = tx.send(StreamEvent::Partial(self.words[..self.pushed].join(" ")));
        }
        Ok(())
    }
    fn events(&self) -> &Receiver<StreamEvent> {
        &self.rx
    }
    fn finish(&mut self) -> Result<(), ProviderError> {
        let tx = self.tx.take().ok_or(ProviderError::Unsupported)?;
        self.finals.lock().unwrap().push(tx);
        Ok(())
    }
    fn cancel(&mut self) {
        self.tx = None;
    }
}

fn manual(fail_after: usize) -> (Arc<Manual>, StreamHold) {
    let engine = Arc::new(Manual {
        finals: Arc::default(),
        started: AtomicUsize::new(0),
        fail_after,
    });
    let h = StreamHold::start(engine.clone(), "en-US").unwrap();
    (engine, h)
}

#[test]
fn new_session_partials_wait_for_the_old_final() {
    let (engine, mut h) = manual(usize::MAX);
    h.feed(&voice(1600));
    h.feed(&silence(RESTART_PAUSE_SAMPLES));
    assert_eq!(h.seq(), 2);
    // Session 2 speaks, but session 1 has not delivered its final yet.
    let held = h.feed(&voice(1600));
    assert!(partials(&held).is_empty());
    let tx1 = engine.finals.lock().unwrap().remove(0);
    tx1.send(StreamEvent::Final("alpha beta".into())).unwrap();
    drop(tx1);
    let out = h.poll();
    assert_eq!(
        out,
        vec![
            Input::Segment {
                seq: 1,
                outcome: Outcome::Text("alpha beta".into())
            },
            Input::StreamPartial {
                seq: 2,
                text: "alpha".into()
            },
        ]
    );
}

#[test]
fn error_at_finish_keeps_the_last_partial_and_silence_types_nothing() {
    let fake = FakeStreamingStt::new(&["keep", "this"])
        .failing_finish(ProviderError::Local("domain=x code=1110".into()));
    let mut h = StreamHold::start(Arc::new(fake), "en-US").unwrap();
    h.feed(&voice(10));
    h.feed(&voice(10));
    assert_eq!(
        h.release(&[]),
        vec![Input::Transcribed(Outcome::Text("keep this".into()))]
    );
    let quiet = FakeStreamingStt::new(&[])
        .failing_finish(ProviderError::Local("domain=x code=1110".into()));
    let mut h = StreamHold::start(Arc::new(quiet), "en-US").unwrap();
    h.feed(&silence(1600));
    assert_eq!(h.release(&[]), vec![Input::Transcribed(Outcome::Nothing)]);
}

#[test]
fn cancel_stops_all_events() {
    let (engine, mut h) = manual(usize::MAX);
    h.feed(&voice(1600));
    h.feed(&silence(RESTART_PAUSE_SAMPLES));
    h.cancel();
    assert!(h.is_done());
    for tx in engine.finals.lock().unwrap().drain(..) {
        let _ = tx.send(StreamEvent::Final("late".into()));
    }
    assert!(h.poll().is_empty());
    assert!(h.feed(&voice(1600)).is_empty());
    assert!(h.release(&[]).is_empty());
}

#[test]
fn failed_restart_reports_a_fixed_error_at_release() {
    let (engine, mut h) = manual(1);
    h.feed(&voice(1600));
    h.feed(&silence(RESTART_PAUSE_SAMPLES));
    let tx1 = engine.finals.lock().unwrap().remove(0);
    tx1.send(StreamEvent::Final("alpha".into())).unwrap();
    let seg = h.poll();
    assert_eq!(
        seg,
        vec![Input::Segment {
            seq: 1,
            outcome: Outcome::Text("alpha".into())
        }]
    );
    assert_eq!(
        h.release(&[]),
        vec![Input::Transcribed(Outcome::Failed(STREAM_FAILED.into()))]
    );
}

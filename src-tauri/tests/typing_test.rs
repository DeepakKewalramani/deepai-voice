// Keyboard typing pipeline with a mock typer: no real key events.

use deepai_voice_lib::paste::PasteError;
use deepai_voice_lib::paste::typing::{KeyTyper, type_chunks, type_edit_with};
use std::cell::RefCell;

#[derive(Default)]
struct MockTyper {
    log: RefCell<Vec<String>>,
    fail: bool,
}

impl KeyTyper for MockTyper {
    fn backspace(&self, count: usize) -> Result<(), String> {
        self.log.borrow_mut().push(format!("bs{count}"));
        if self.fail { Err("no".into()) } else { Ok(()) }
    }
    fn type_units(&self, units: &[u16]) -> Result<(), String> {
        self.log
            .borrow_mut()
            .push(String::from_utf16(units).unwrap());
        if self.fail { Err("no".into()) } else { Ok(()) }
    }
}

#[test]
fn chunks_hold_at_most_16_units_and_never_split_a_character() {
    assert!(type_chunks("").is_empty());
    let text = "hello world, this is longer than sixteen";
    let chunks = type_chunks(text);
    assert!(chunks.iter().all(|c| c.len() <= 16));
    let joined: Vec<u16> = chunks.concat();
    assert_eq!(String::from_utf16(&joined).unwrap(), text);
    // Emoji with skin tone (4 units) and family (8 units) stay whole.
    let emoji = "aaaaaaaaaaaaaa👍🏽 👨\u{200d}👩\u{200d}👧";
    for c in type_chunks(emoji) {
        assert!(String::from_utf16(&c).is_ok(), "split a surrogate pair");
        assert!(c.len() <= 16);
    }
    assert_eq!(type_chunks("aaaaaaaaaaaaaa👍🏽")[0].len(), 14);
}

#[test]
fn edit_presses_backspace_then_types_in_chunks() {
    let t = MockTyper::default();
    assert_eq!(type_edit_with(&t, true, 3, "abc"), Ok(()));
    assert_eq!(*t.log.borrow(), vec!["bs3", "abc"]);
    let t = MockTyper::default();
    assert_eq!(type_edit_with(&t, true, 0, ""), Ok(()));
    assert!(t.log.borrow().is_empty());
}

#[test]
fn edit_needs_accessibility_and_reports_inject_failures() {
    let t = MockTyper::default();
    assert_eq!(
        type_edit_with(&t, false, 1, "x"),
        Err(PasteError::AccessibilityDenied)
    );
    assert!(
        t.log.borrow().is_empty(),
        "nothing typed without permission"
    );
    let t = MockTyper {
        fail: true,
        ..Default::default()
    };
    assert!(matches!(
        type_edit_with(&t, true, 0, "x"),
        Err(PasteError::InjectFailed(_))
    ));
}

// ---------------------------------------------------------------------------
// Focus and secure-field guard: pure decisions plus the typing seam.
// ---------------------------------------------------------------------------

use deepai_voice_lib::focus::{
    FocusProbe, Gate, HoldGuard, ProbeGuard, Snapshot, StopReason, TypingGuard, gate,
};
use deepai_voice_lib::paste::typing::{EditError, EditOutcome, type_edit_guarded};
use std::collections::VecDeque;

fn snap(app: Option<i32>, input: u64) -> Snapshot {
    Snapshot {
        app,
        input_count: input,
    }
}

#[test]
fn gate_stops_on_secure_field_or_app_change_and_blocks_backspace_after_user_input() {
    let start = snap(Some(10), 5);
    assert_eq!(gate(&start, &snap(Some(10), 5), false), Gate::Go);
    assert_eq!(
        gate(&start, &snap(Some(10), 5), true),
        Gate::Stop(StopReason::SecureField)
    );
    assert_eq!(
        gate(&start, &snap(Some(11), 5), false),
        Gate::Stop(StopReason::AppChanged)
    );
    // Unknown app at either end counts as changed: we cannot confirm the target.
    assert_eq!(
        gate(&start, &snap(None, 5), false),
        Gate::Stop(StopReason::AppChanged)
    );
    assert_eq!(
        gate(&snap(None, 5), &snap(None, 5), false),
        Gate::Stop(StopReason::AppChanged)
    );
    assert_eq!(gate(&start, &snap(Some(10), 6), false), Gate::NoBackspace);
    // Secure wins over everything else.
    assert_eq!(
        gate(&start, &snap(Some(11), 9), true),
        Gate::Stop(StopReason::SecureField)
    );
}

#[test]
fn hold_guard_is_sticky_for_the_rest_of_the_hold() {
    let mut g = HoldGuard::new(snap(Some(10), 0));
    assert_eq!(g.check(snap(Some(10), 0), false), Gate::Go);
    assert_eq!(g.check(snap(Some(10), 2), false), Gate::NoBackspace);
    assert_eq!(
        g.check(snap(Some(10), 2), false),
        Gate::NoBackspace,
        "user input is never forgotten"
    );
    assert_eq!(
        g.check(snap(Some(20), 2), false),
        Gate::Stop(StopReason::AppChanged)
    );
    // Coming back to the first app does not resume typing.
    assert_eq!(
        g.check(snap(Some(10), 2), false),
        Gate::Stop(StopReason::AppChanged)
    );
    let mut g = HoldGuard::new(snap(Some(10), 0));
    assert_eq!(
        g.check(snap(Some(10), 0), true),
        Gate::Stop(StopReason::SecureField)
    );
    assert_eq!(
        g.check(snap(Some(10), 0), false),
        Gate::Stop(StopReason::SecureField)
    );
}

struct FakeProbe {
    app: Option<i32>,
    input: u64,
    secure: bool,
}

impl FocusProbe for FakeProbe {
    fn snapshot(&self) -> Snapshot {
        snap(self.app, self.input)
    }
    fn secure_input(&self) -> bool {
        self.secure
    }
}

#[test]
fn probe_guard_reads_the_probe_on_every_check() {
    let probe = FakeProbe {
        app: Some(1),
        input: 0,
        secure: false,
    };
    let mut g = ProbeGuard::new(probe);
    assert_eq!(g.check(), Gate::Go);
    g.probe_mut().secure = true;
    assert_eq!(g.check(), Gate::Stop(StopReason::SecureField));
}

/// Returns the queued gates in order, then `Go`.
#[derive(Default)]
struct ScriptGuard(VecDeque<Gate>, usize);

impl TypingGuard for ScriptGuard {
    fn check(&mut self) -> Gate {
        self.1 += 1;
        self.0.pop_front().unwrap_or(Gate::Go)
    }
}

#[test]
fn guarded_edit_never_backspaces_after_user_input_but_still_types_new_words() {
    let t = MockTyper::default();
    // The real guard is sticky, so every check after user input says so.
    let mut g = ScriptGuard(VecDeque::from([Gate::NoBackspace, Gate::NoBackspace]), 0);
    assert_eq!(
        type_edit_guarded(&t, &mut g, true, 3, "abc"),
        Ok(EditOutcome::SkippedCorrection)
    );
    assert!(t.log.borrow().is_empty(), "no Backspace, no replacement");

    let t = MockTyper::default();
    let mut g = ScriptGuard(VecDeque::from([Gate::NoBackspace, Gate::NoBackspace]), 0);
    assert_eq!(
        type_edit_guarded(&t, &mut g, true, 0, " more"),
        Ok(EditOutcome::Done)
    );
    assert_eq!(*t.log.borrow(), vec![" more"]);
}

#[test]
fn guarded_edit_checks_before_every_action_and_stops_on_app_change() {
    // Stop before anything: no Backspace at all.
    let t = MockTyper::default();
    let mut g = ScriptGuard(VecDeque::from([Gate::Stop(StopReason::AppChanged)]), 0);
    assert_eq!(
        type_edit_guarded(&t, &mut g, true, 2, "x"),
        Ok(EditOutcome::Stopped(StopReason::AppChanged))
    );
    assert!(t.log.borrow().is_empty());

    // App changes after the first 16-unit chunk: the rest is not typed.
    let text = "aaaaaaaaaaaaaaaabbbb"; // 16 + 4 units, two chunks
    let t = MockTyper::default();
    let mut g = ScriptGuard(
        VecDeque::from([
            Gate::Go,
            Gate::Go,
            Gate::Go,
            Gate::Stop(StopReason::AppChanged),
        ]),
        0,
    );
    assert_eq!(
        type_edit_guarded(&t, &mut g, true, 1, text),
        Ok(EditOutcome::Stopped(StopReason::AppChanged))
    );
    assert_eq!(*t.log.borrow(), vec!["bs1", "aaaaaaaaaaaaaaaa"]);
    // Checked once up front, once before Backspace, once per chunk.
    assert_eq!(g.1, 4);

    // Secure field: nothing typed, even without Accessibility.
    let t = MockTyper::default();
    let mut g = ScriptGuard(VecDeque::from([Gate::Stop(StopReason::SecureField)]), 0);
    assert_eq!(
        type_edit_guarded(&t, &mut g, false, 0, "pw"),
        Ok(EditOutcome::Stopped(StopReason::SecureField))
    );
    assert!(t.log.borrow().is_empty());
}

/// Fails on the `n`th `type_units` call (1-based).
struct FailNth {
    log: RefCell<Vec<String>>,
    n: usize,
}

impl KeyTyper for FailNth {
    fn backspace(&self, count: usize) -> Result<(), String> {
        self.log.borrow_mut().push(format!("bs{count}"));
        Ok(())
    }
    fn type_units(&self, units: &[u16]) -> Result<(), String> {
        if self
            .log
            .borrow()
            .iter()
            .filter(|e| !e.starts_with("bs"))
            .count()
            + 1
            == self.n
        {
            return Err("no".into());
        }
        self.log
            .borrow_mut()
            .push(String::from_utf16(units).unwrap());
        Ok(())
    }
}

#[test]
fn guarded_edit_reports_how_much_text_was_typed_before_a_failure() {
    let text = "aaaaaaaaaaaaaaaa👍🏽 end"; // first chunk is the 16 a's
    let t = FailNth {
        log: RefCell::default(),
        n: 2,
    };
    let mut g = ScriptGuard::default();
    assert_eq!(
        type_edit_guarded(&t, &mut g, true, 0, text),
        Err(EditError {
            error: PasteError::InjectFailed("no".into()),
            typed_bytes: Some(16),
        })
    );
    let t = FailNth {
        log: RefCell::default(),
        n: 1,
    };
    assert_eq!(
        type_edit_guarded(&t, &mut ScriptGuard::default(), true, 0, "hi"),
        Err(EditError {
            error: PasteError::InjectFailed("no".into()),
            typed_bytes: Some(0),
        })
    );
    // Accessibility denied: nothing typed, not a text-phase failure.
    assert_eq!(
        type_edit_guarded(
            &MockTyper::default(),
            &mut ScriptGuard::default(),
            false,
            0,
            "x"
        ),
        Err(EditError {
            error: PasteError::AccessibilityDenied,
            typed_bytes: None,
        })
    );
}

#[test]
fn cancelled_status_is_fixed() {
    assert_eq!(StopReason::Cancelled.status(), "Cancelled");
    assert_eq!(
        StopReason::AppChanged.status(),
        "Typing paused: app changed"
    );
    assert_eq!(StopReason::SecureField.status(), "Secure field: not typing");
}

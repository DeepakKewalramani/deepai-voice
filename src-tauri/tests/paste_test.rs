// Integration tests for the paste module (Slice 2.4).
// All tests use mock/in-memory logic only -- no real system clipboard or CGEvent is needed.

use deepai_voice_lib::paste::{
    ClipboardBackend, KeyInjector, PasteError, PasteResult,
    clipboard::{save_snapshot, should_restore},
    last_dictation::{clear_last_dictation, get_last_dictation, set_last_dictation},
    paste_with, remember_on_failure,
};
use std::cell::RefCell;
use std::sync::Mutex;

// last_dictation is process-global; serialise tests that touch it.
static LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// save_snapshot
// ---------------------------------------------------------------------------

#[test]
fn save_snapshot_captures_current_state() {
    let snapshot = save_snapshot("original content", 3);
    assert_eq!(snapshot.text, "original content");
    assert_eq!(snapshot.change_count, 3);
}

// ---------------------------------------------------------------------------
// Pipeline with a mock clipboard, mock injector and instant sleep
// ---------------------------------------------------------------------------

struct Mock {
    text: String,
    count: u64,
    log: Vec<String>,
    fail_set: bool,
    /// How much a write bumps the counter (NSPasteboard may bump by 1 or 2).
    bump: u64,
}

impl Mock {
    fn new(text: &str, count: u64) -> Self {
        Self {
            text: text.into(),
            count,
            log: vec![],
            fail_set: false,
            bump: 1,
        }
    }
}

impl ClipboardBackend for Mock {
    fn text(&mut self) -> Result<String, String> {
        Ok(self.text.clone())
    }
    fn change_count(&self) -> u64 {
        self.count
    }
    fn set_text(&mut self, t: &str) -> Result<(), String> {
        if self.fail_set {
            return Err("boom".into());
        }
        self.text = t.into();
        self.count += self.bump;
        self.log.push(format!("set:{t}"));
        Ok(())
    }
}

/// Injector that records the clipboard it saw, and optionally simulates
/// another app writing the clipboard during the settle window.
struct Inject<'a> {
    cb: &'a RefCell<Mock>,
    seen: RefCell<Option<String>>,
    fail: bool,
    third_party_write: bool,
}

impl KeyInjector for Inject<'_> {
    fn send_paste(&self) -> Result<(), String> {
        if self.fail {
            return Err("no event".into());
        }
        *self.seen.borrow_mut() = Some(self.cb.borrow().text.clone());
        if self.third_party_write {
            let mut c = self.cb.borrow_mut();
            c.text = "third party".into();
            c.count += 1;
        }
        Ok(())
    }
}

// A RefCell-backed adapter so the injector and the pipeline share one mock.
struct Shared<'a>(&'a RefCell<Mock>);
impl ClipboardBackend for Shared<'_> {
    fn text(&mut self) -> Result<String, String> {
        self.0.borrow_mut().text()
    }
    fn change_count(&self) -> u64 {
        self.0.borrow().change_count()
    }
    fn set_text(&mut self, t: &str) -> Result<(), String> {
        self.0.borrow_mut().set_text(t)
    }
}

fn run(
    mock: &RefCell<Mock>,
    trusted: bool,
    fail: bool,
    third: bool,
) -> (PasteResult, Option<String>, Vec<u64>) {
    let inj = Inject {
        cb: mock,
        seen: RefCell::new(None),
        fail,
        third_party_write: third,
    };
    let sleeps = RefCell::new(vec![]);
    let r = paste_with(&mut Shared(mock), &inj, trusted, "dictated", 150, |d| {
        sleeps.borrow_mut().push(d.as_millis() as u64)
    });
    (r, inj.seen.into_inner(), sleeps.into_inner())
}

#[test]
fn pastes_dictation_then_restores_original() {
    let m = RefCell::new(Mock::new("original", 3));
    let (r, seen, sleeps) = run(&m, true, false, false);
    assert_eq!(r, Ok(()));
    assert_eq!(
        seen.as_deref(),
        Some("dictated"),
        "target app must see the dictation"
    );
    assert_eq!(m.borrow().text, "original");
    assert_eq!(sleeps, vec![150], "settle delay honoured");
}

#[test]
fn does_not_restore_when_third_party_wrote_clipboard() {
    let m = RefCell::new(Mock::new("original", 3));
    let (r, _, _) = run(&m, true, false, true);
    assert_eq!(r, Ok(()));
    assert_eq!(m.borrow().text, "third party");
}

#[test]
fn accessibility_denied_touches_nothing() {
    let m = RefCell::new(Mock::new("original", 3));
    let (r, seen, sleeps) = run(&m, false, false, false);
    assert_eq!(r, Err(PasteError::AccessibilityDenied));
    assert!(seen.is_none() && sleeps.is_empty());
    assert!(m.borrow().log.is_empty(), "clipboard must not be written");
    assert_eq!(m.borrow().text, "original");
}

#[test]
fn inject_failure_restores_clipboard_and_reports_error() {
    let m = RefCell::new(Mock::new("original", 3));
    let (r, _, sleeps) = run(&m, true, true, false);
    assert_eq!(r, Err(PasteError::InjectFailed("no event".into())));
    assert_eq!(m.borrow().text, "original");
    assert!(sleeps.is_empty());
}

#[test]
fn clipboard_write_failure_is_reported() {
    let mut mock = Mock::new("original", 3);
    mock.fail_set = true;
    let m = RefCell::new(mock);
    let (r, _, _) = run(&m, true, false, false);
    assert_eq!(r, Err(PasteError::ClipboardError("boom".into())));
}

#[test]
fn empty_original_clipboard_is_restored_as_empty() {
    let m = RefCell::new(Mock::new("", 0));
    let (r, _, _) = run(&m, true, false, false);
    assert_eq!(r, Ok(()));
    assert_eq!(m.borrow().text, "");
}

#[test]
fn should_restore_only_when_counter_unchanged_since_our_write() {
    assert!(should_restore(11, 11));
    assert!(!should_restore(11, 12), "another app wrote");
}

// Failure keeps the text for "Copy last dictation". Uses mock backends only:
// no real clipboard write and no real key injection.
#[test]
fn accessibility_denied_saves_last_dictation() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_last_dictation();
    let m = RefCell::new(Mock::new("original", 3));
    let (r, _, _) = run(&m, false, false, false);
    let r = remember_on_failure("saved for tray", r);
    assert_eq!(r, Err(PasteError::AccessibilityDenied));
    assert_eq!(get_last_dictation().as_deref(), Some("saved for tray"));
}

#[test]
fn inject_failure_saves_last_dictation() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_last_dictation();
    let m = RefCell::new(Mock::new("original", 3));
    let (r, _, _) = run(&m, true, true, false);
    let r = remember_on_failure("saved for tray", r);
    assert!(matches!(r, Err(PasteError::InjectFailed(_))));
    assert_eq!(get_last_dictation().as_deref(), Some("saved for tray"));
}

#[test]
fn successful_paste_does_not_save_last_dictation() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_last_dictation();
    let m = RefCell::new(Mock::new("original", 3));
    let (r, _, _) = run(&m, true, false, false);
    assert_eq!(remember_on_failure("should not be kept", r), Ok(()));
    assert!(get_last_dictation().is_none());
}

// A write that bumps the counter by 2 (or 1) must not look like a third party.
#[test]
fn restore_happens_when_write_bumps_counter_by_two_or_one() {
    for bump in [1, 2] {
        let mut mock = Mock::new("original", 3);
        mock.bump = bump;
        let m = RefCell::new(mock);
        let (r, _, _) = run(&m, true, false, false);
        assert_eq!(r, Ok(()));
        assert_eq!(m.borrow().text, "original", "bump {bump}");
    }
}

#[test]
fn restore_skipped_when_third_party_writes_with_bump_two() {
    let mut mock = Mock::new("original", 3);
    mock.bump = 2;
    let m = RefCell::new(mock);
    let (r, _, _) = run(&m, true, false, true);
    assert_eq!(r, Ok(()));
    assert_eq!(m.borrow().text, "third party");
}

// ---------------------------------------------------------------------------
// Fallback memory store: last dictation
// ---------------------------------------------------------------------------

#[test]
fn last_dictation_is_none_initially() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_last_dictation();
    assert!(get_last_dictation().is_none());
}

#[test]
fn set_and_get_last_dictation() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_last_dictation("Dictated sentence one.");
    let got = get_last_dictation();
    assert_eq!(got.as_deref(), Some("Dictated sentence one."));
}

#[test]
fn set_last_dictation_overwrites_previous() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_last_dictation("First.");
    set_last_dictation("Second.");
    assert_eq!(get_last_dictation().as_deref(), Some("Second."));
}

#[test]
fn clear_last_dictation_removes_stored_text() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_last_dictation("Something.");
    clear_last_dictation();
    assert!(get_last_dictation().is_none());
}

// ---------------------------------------------------------------------------
// PasteResult and PasteError types compile and are usable
// ---------------------------------------------------------------------------

#[test]
fn paste_error_accessibility_denied_variant() {
    let e = PasteError::AccessibilityDenied;
    let msg = format!("{e}");
    assert!(!msg.is_empty());
}

#[test]
fn paste_error_clipboard_error_variant() {
    let e = PasteError::ClipboardError("arboard failed".to_string());
    let msg = format!("{e}");
    assert!(msg.contains("arboard failed"));
}

#[test]
fn paste_error_inject_failed_variant() {
    let e = PasteError::InjectFailed("cgevent error".to_string());
    let msg = format!("{e}");
    assert!(msg.contains("cgevent error"));
}

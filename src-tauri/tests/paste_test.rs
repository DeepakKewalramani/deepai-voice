// Integration tests for the paste module error types. Dictated text is only
// ever typed with key events (see typing_test.rs); there is no clipboard
// path any more, so the clipboard restore and last-dictation tests are gone.

use deepai_voice_lib::paste::{PasteError, PasteResult};

#[test]
fn paste_error_accessibility_denied_variant() {
    let e = PasteError::AccessibilityDenied;
    let msg = format!("{e}");
    assert!(!msg.is_empty());
}

#[test]
fn paste_error_inject_failed_variant() {
    let e = PasteError::InjectFailed("cgevent error".to_string());
    let msg = format!("{e}");
    assert!(msg.contains("cgevent error"));
    let r: PasteResult = Err(e);
    assert!(r.is_err());
}

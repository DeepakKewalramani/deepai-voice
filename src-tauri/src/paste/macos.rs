//! macOS typing implementation: AXIsProcessTrusted and CGEvent key events.
//! No clipboard is used. Dictated text is never logged.

use super::PasteResult;
use super::typing::{EditError, EditOutcome, KeyTyper, type_edit_guarded, type_edit_with};
use crate::focus::TypingGuard;
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(
        source: *mut c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, string: *const u16);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *mut c_void);
}

/// kVK_Delete (Backspace).
const DELETE_KEY_CODE: u16 = 51;
/// kCGEventSourceStatePrivate: typed events do not inherit held modifiers
/// (the user is holding Fn while dictating).
const PRIVATE_STATE: i32 = -1;
/// Pause between key events so slow apps keep up.
const TYPE_GAP: std::time::Duration = std::time::Duration::from_millis(2);
/// kCGSessionEventTap.
const SESSION_EVENT_TAP: u32 = 1;

/// Whether this process has the Accessibility permission.
pub fn is_accessibility_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no arguments and has no side effects.
    unsafe { AXIsProcessTrusted() }
}

/// Posts one key down + up pair with no modifier flags. `units` (when not
/// empty) replaces the key's character with that Unicode text.
fn post_key(key_code: u16, units: &[u16]) -> Result<(), String> {
    // SAFETY: documented CoreGraphics calls. `units` outlives the calls that
    // read it, and every non-null object created here is released.
    unsafe {
        let source = CGEventSourceCreate(PRIVATE_STATE);
        if source.is_null() {
            return Err("CGEventSourceCreate returned null".to_string());
        }
        let down = CGEventCreateKeyboardEvent(source, key_code, true);
        let up = CGEventCreateKeyboardEvent(source, key_code, false);
        let result = if down.is_null() || up.is_null() {
            Err("CGEventCreateKeyboardEvent returned null".to_string())
        } else {
            for ev in [down, up] {
                CGEventSetFlags(ev, 0);
                if !units.is_empty() {
                    CGEventKeyboardSetUnicodeString(ev, units.len(), units.as_ptr());
                }
            }
            CGEventPost(SESSION_EVENT_TAP, down);
            CGEventPost(SESSION_EVENT_TAP, up);
            Ok(())
        };
        for obj in [down, up, source] {
            if !obj.is_null() {
                CFRelease(obj);
            }
        }
        result
    }
}

struct CgEventTyper;

impl KeyTyper for CgEventTyper {
    fn backspace(&self, count: usize) -> Result<(), String> {
        for _ in 0..count {
            post_key(DELETE_KEY_CODE, &[])?;
            std::thread::sleep(TYPE_GAP);
        }
        Ok(())
    }

    fn type_units(&self, units: &[u16]) -> Result<(), String> {
        // Key code 0 is a placeholder; the Unicode string decides the text.
        post_key(0, units)?;
        std::thread::sleep(TYPE_GAP);
        Ok(())
    }
}

/// Types an edit (Backspaces, then text) with real key events.
pub fn type_edit_via_cgevent(backspaces: usize, text: &str) -> PasteResult {
    type_edit_with(&CgEventTyper, is_accessibility_trusted(), backspaces, text)
}

/// Like `type_edit_via_cgevent`, asking `guard` before every key action.
pub fn type_edit_guarded_via_cgevent<G: TypingGuard>(
    guard: &mut G,
    backspaces: usize,
    text: &str,
) -> Result<EditOutcome, EditError> {
    type_edit_guarded(
        &CgEventTyper,
        guard,
        is_accessibility_trusted(),
        backspaces,
        text,
    )
}

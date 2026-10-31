//! macOS paste implementation: arboard clipboard, NSPasteboard changeCount,
//! AXIsProcessTrusted and a CGEvent Cmd+V. Dictated text is never logged.

use super::{ClipboardBackend, KeyInjector, PasteError, PasteResult, paste_with};
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
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *mut c_void);
}

// Makes sure AppKit is loaded so NSPasteboard resolves at runtime.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

/// kVK_ANSI_V. Position-based, so on non-QWERTY layouts (e.g. Dvorak) the
/// physical key may not map to "v"; most apps still honour Cmd+V by key code.
const V_KEY_CODE: u16 = 9;
/// kCGEventFlagMaskCommand.
const COMMAND_FLAG: u64 = 0x0010_0000;
/// kCGEventSourceStateHIDSystemState.
const HID_SYSTEM_STATE: i32 = 1;
/// kCGSessionEventTap.
const SESSION_EVENT_TAP: u32 = 1;

/// Whether this process has the Accessibility permission.
pub fn is_accessibility_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no arguments and has no side effects.
    unsafe { AXIsProcessTrusted() }
}

struct SystemClipboard(arboard::Clipboard);

impl ClipboardBackend for SystemClipboard {
    fn text(&mut self) -> Result<String, String> {
        match self.0.get_text() {
            Ok(t) => Ok(t),
            Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn change_count(&self) -> u64 {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        // SAFETY: NSPasteboard.generalPasteboard and -changeCount are plain
        // AppKit getters; the returned pasteboard is a valid, autoreleased
        // singleton used only for this call.
        unsafe {
            let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            let count: isize = msg_send![pb, changeCount];
            count as u64
        }
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        if text.is_empty() {
            self.0.clear().map_err(|e| e.to_string())
        } else {
            self.0.set_text(text).map_err(|e| e.to_string())
        }
    }
}

struct CgEventInjector;

impl KeyInjector for CgEventInjector {
    fn send_paste(&self) -> Result<(), String> {
        // SAFETY: documented CoreGraphics calls. Every non-null object we
        // create is released before returning, including on error paths.
        unsafe {
            let source = CGEventSourceCreate(HID_SYSTEM_STATE);
            if source.is_null() {
                return Err("CGEventSourceCreate returned null".to_string());
            }
            let down = CGEventCreateKeyboardEvent(source, V_KEY_CODE, true);
            let up = CGEventCreateKeyboardEvent(source, V_KEY_CODE, false);
            let result = if down.is_null() || up.is_null() {
                Err("CGEventCreateKeyboardEvent returned null".to_string())
            } else {
                CGEventSetFlags(down, COMMAND_FLAG);
                CGEventSetFlags(up, COMMAND_FLAG);
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
}

/// Paste `text` via the real clipboard and a synthetic Cmd+V.
pub fn paste_text_via_cgevent(text: &str, settle_ms: u64) -> PasteResult {
    let trusted = is_accessibility_trusted();
    if !trusted {
        return Err(PasteError::AccessibilityDenied);
    }
    let cb = arboard::Clipboard::new().map_err(|e| PasteError::ClipboardError(e.to_string()))?;
    paste_with(
        &mut SystemClipboard(cb),
        &CgEventInjector,
        trusted,
        text,
        settle_ms,
        std::thread::sleep,
    )
}

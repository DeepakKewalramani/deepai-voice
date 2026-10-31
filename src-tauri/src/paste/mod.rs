//! paste -- inject dictated text at the active cursor.
//!
//! Public interface for Slice 2.4.
//!
//! Execution order on macOS:
//!   1. Check AXIsProcessTrusted() -- bail with AccessibilityDenied if false.
//!   2. Save clipboard text + changeCount (ClipboardSnapshot).
//!   3. Write dictation text to clipboard.
//!   4. Synthesize Cmd+V key-down + key-up via CGEventPost.
//!   5. Wait settle delay (default 120 ms) so slow apps (Electron) can process the event.
//!   6. If changeCount is still the value right after our own write, restore the
//!      original clipboard content (otherwise another app wrote; leave it alone).
//!   7. On any failure: store text in the in-memory last-dictation store so the user
//!      can retrieve it via the tray "Copy last dictation" action.

pub mod clipboard;
pub mod last_dictation;

#[cfg(target_os = "macos")]
pub mod macos;

use std::time::Duration;

use std::fmt;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A snapshot of the clipboard contents and the platform change-counter at
/// the moment the snapshot was taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardSnapshot {
    /// Text held by the clipboard at snapshot time (may be empty).
    pub text: String,
    /// Platform change-counter value at snapshot time.
    pub change_count: u64,
}

/// Error variants produced by the paste pipeline.
#[derive(Debug, PartialEq, Eq)]
pub enum PasteError {
    /// macOS Accessibility permission has not been granted.
    AccessibilityDenied,
    /// Reading or writing the system clipboard failed.
    ClipboardError(String),
    /// Synthesising the key event (CGEventPost / SendInput) failed.
    InjectFailed(String),
}

impl fmt::Display for PasteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasteError::AccessibilityDenied => {
                write!(
                    f,
                    "Accessibility permission denied, grant access in System Settings"
                )
            }
            PasteError::ClipboardError(msg) => write!(f, "Clipboard error: {msg}"),
            PasteError::InjectFailed(msg) => write!(f, "Key-event injection failed: {msg}"),
        }
    }
}

/// The result type returned by paste operations.
pub type PasteResult = Result<(), PasteError>;

// ---------------------------------------------------------------------------
// Backends (real on macOS, mocks in tests)
// ---------------------------------------------------------------------------

/// Text clipboard with a platform change counter.
pub trait ClipboardBackend {
    /// Current text. Non-text content reads as an empty string.
    fn text(&mut self) -> Result<String, String>;
    /// Platform change counter (NSPasteboard changeCount on macOS).
    fn change_count(&self) -> u64;
    /// Replace the clipboard with `text`. Empty text clears the clipboard.
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

/// Sends the platform paste shortcut (Cmd+V on macOS).
pub trait KeyInjector {
    fn send_paste(&self) -> Result<(), String>;
}

/// Default settle delay in milliseconds between the paste key event and the
/// clipboard restore. Configurable for slow Electron apps (Slack, VS Code).
pub const DEFAULT_SETTLE_MS: u64 = 120;

/// The paste pipeline, independent of the platform. `sleep` is injected so
/// tests can run without waiting.
///
/// Never logs `text`.
pub fn paste_with<C: ClipboardBackend, K: KeyInjector>(
    clipboard: &mut C,
    injector: &K,
    accessibility_trusted: bool,
    text: &str,
    settle_ms: u64,
    sleep: impl Fn(Duration),
) -> PasteResult {
    // 1. Accessibility first: do not touch the clipboard if we cannot paste.
    if !accessibility_trusted {
        return Err(PasteError::AccessibilityDenied);
    }

    // 2. Snapshot. Non-text content cannot be preserved and reads as empty.
    let original = clipboard.text().unwrap_or_default();
    let snapshot = clipboard::save_snapshot(&original, clipboard.change_count());

    // 3. Write the dictation and remember the counter right after our write.
    clipboard
        .set_text(text)
        .map_err(PasteError::ClipboardError)?;
    let after_our_write = clipboard.change_count();

    // 4 and 5. Paste, then let the target app read the clipboard.
    let injected = injector.send_paste();
    if injected.is_ok() {
        sleep(Duration::from_millis(settle_ms));
    }

    // 6. Restore only if nobody else wrote since our write.
    if clipboard::should_restore(after_our_write, clipboard.change_count()) {
        let _ = clipboard.set_text(&snapshot.text);
    }

    injected.map_err(PasteError::InjectFailed)
}

// ---------------------------------------------------------------------------
// Top-level paste entry point
// ---------------------------------------------------------------------------

/// Paste `text` at the active cursor with the default settle delay.
pub fn paste_text(text: &str) -> PasteResult {
    paste_text_with_delay(text, DEFAULT_SETTLE_MS)
}

/// Like [`paste_text`] but with a configurable settle delay.
///
/// On any failure the text is kept in memory for the tray "Copy last
/// dictation" action. Only the most recent dictation is kept.
pub fn paste_text_with_delay(text: &str, settle_ms: u64) -> PasteResult {
    remember_on_failure(text, platform_paste(text, settle_ms))
}

/// Keep `text` for "Copy last dictation" when `result` is an error, and
/// pass the result through unchanged.
pub fn remember_on_failure(text: &str, result: PasteResult) -> PasteResult {
    if result.is_err() {
        last_dictation::set_last_dictation(text);
    }
    result
}

#[cfg(target_os = "macos")]
fn platform_paste(text: &str, settle_ms: u64) -> PasteResult {
    macos::paste_text_via_cgevent(text, settle_ms)
}

#[cfg(not(target_os = "macos"))]
fn platform_paste(_text: &str, _settle_ms: u64) -> PasteResult {
    // Windows SendInput is not implemented yet. Report failure so the text
    // lands in the last-dictation fallback instead of being silently lost.
    Err(PasteError::InjectFailed(
        "paste is not implemented on this platform".to_string(),
    ))
}

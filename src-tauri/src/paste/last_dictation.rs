//! In-memory fallback store for the most recently dictated text.
//!
//! When a paste attempt fails (Accessibility denied, clipboard error, etc.)
//! the text is stored here so the user can retrieve it via the tray
//! "Copy last dictation" action without losing their dictation.
//!
//! This store holds at most ONE entry (the most recent dictation).
//! No audio, no transcripts and no text is ever written to disk here.

use std::sync::Mutex;

static LAST_DICTATION: Mutex<Option<String>> = Mutex::new(None);

/// Replace the stored last-dictation text with `text`.
pub fn set_last_dictation(text: &str) {
    let mut guard = LAST_DICTATION.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(text.to_string());
}

/// Return the stored last-dictation text, or `None` if nothing has been stored yet.
pub fn get_last_dictation() -> Option<String> {
    let guard = LAST_DICTATION.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// Clear the stored last-dictation text (useful in tests and on user retrieval).
pub fn clear_last_dictation() {
    let mut guard = LAST_DICTATION.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

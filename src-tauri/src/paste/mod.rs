//! paste -- insert dictated text at the active cursor.
//!
//! Text is only ever typed with synthetic key events (`typing`). There is no
//! clipboard path: dictated text is never written to the clipboard, never
//! pasted with Cmd+V and never kept for a later copy. When typing is not
//! possible (no Accessibility, key events fail, a focus or secure-field guard
//! stops it) nothing is inserted and the caller shows a fixed status.

pub mod typing;

#[cfg(target_os = "macos")]
pub mod macos;

use std::fmt;

/// Error variants produced by the typing pipeline.
#[derive(Debug, PartialEq, Eq)]
pub enum PasteError {
    /// macOS Accessibility permission has not been granted.
    AccessibilityDenied,
    /// Synthesising the key event (CGEventPost) failed.
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
            PasteError::InjectFailed(msg) => write!(f, "Key-event injection failed: {msg}"),
        }
    }
}

/// The result type returned by typing operations.
pub type PasteResult = Result<(), PasteError>;

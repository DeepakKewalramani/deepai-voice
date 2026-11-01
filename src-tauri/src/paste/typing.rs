//! Live typing: inserts text with synthetic keyboard events, so words can
//! appear while the user is still speaking. The only way dictated text is
//! inserted. Never touches the clipboard and never logs the text.

use super::{PasteError, PasteResult};
use crate::dictation::stream_text::clusters;
use crate::focus::{Gate, StopReason, TypingGuard};

/// Most UTF-16 units one key event carries.
pub const MAX_UNITS_PER_EVENT: usize = 16;

/// Sends key events. Real on macOS, mocked in tests.
pub trait KeyTyper {
    fn backspace(&self, count: usize) -> Result<(), String>;
    /// Types one chunk of at most `MAX_UNITS_PER_EVENT` UTF-16 units.
    fn type_units(&self, units: &[u16]) -> Result<(), String>;
}

/// Splits `text` into UTF-16 chunks of at most 16 units without splitting a
/// visible character. A character longer than 16 units gets its own chunk.
pub fn type_chunks(text: &str) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::new();
    let mut cur: Vec<u16> = Vec::new();
    for cluster in clusters(text) {
        let units: Vec<u16> = cluster.encode_utf16().collect();
        if !cur.is_empty() && cur.len() + units.len() > MAX_UNITS_PER_EVENT {
            out.push(std::mem::take(&mut cur));
        }
        cur.extend(units);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// How a guarded edit ended without a key event failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    /// Everything was typed.
    Done,
    /// The edit needed Backspace but the user typed or clicked during the
    /// hold, so nothing was sent: replacing text without deleting first
    /// would duplicate it.
    SkippedCorrection,
    /// A guard stopped typing. Text typed before the stop stays.
    Stopped(StopReason),
}

/// A key event failure, with how far the edit got.
#[derive(Debug, PartialEq, Eq)]
pub struct EditError {
    pub error: PasteError,
    /// Bytes of `text` typed before the failure. None when the failure came
    /// before the text phase (Accessibility or Backspace).
    pub typed_bytes: Option<usize>,
}

/// Never stops: the unguarded `type_edit_with` path.
struct NoGuard;

impl TypingGuard for NoGuard {
    fn check(&mut self) -> Gate {
        Gate::Go
    }
}

/// Presses Backspace `backspaces` times, then types `text`, asking `guard`
/// first and again before the Backspace run and before every chunk. A stop
/// sends nothing more; a `NoBackspace` gate skips an edit that needs
/// Backspace but still types plain appends. Checks Accessibility after the
/// first guard check and types nothing without it.
pub fn type_edit_guarded<T: KeyTyper, G: TypingGuard>(
    typer: &T,
    guard: &mut G,
    accessibility_trusted: bool,
    backspaces: usize,
    text: &str,
) -> Result<EditOutcome, EditError> {
    let fail = |error, typed_bytes| EditError { error, typed_bytes };
    if let Gate::Stop(r) = guard.check() {
        return Ok(EditOutcome::Stopped(r));
    }
    if !accessibility_trusted {
        return Err(fail(PasteError::AccessibilityDenied, None));
    }
    if backspaces > 0 {
        match guard.check() {
            Gate::Stop(r) => return Ok(EditOutcome::Stopped(r)),
            Gate::NoBackspace => return Ok(EditOutcome::SkippedCorrection),
            Gate::Go => {}
        }
        typer
            .backspace(backspaces)
            .map_err(|e| fail(PasteError::InjectFailed(e), None))?;
    }
    let mut typed = 0;
    for chunk in type_chunks(text) {
        if let Gate::Stop(r) = guard.check() {
            return Ok(EditOutcome::Stopped(r));
        }
        typer
            .type_units(&chunk)
            .map_err(|e| fail(PasteError::InjectFailed(e), Some(typed)))?;
        typed += chunk.len();
    }
    Ok(EditOutcome::Done)
}

/// Presses Backspace `backspaces` times, then types `text`. Checks
/// Accessibility first and types nothing without it. No focus guard.
pub fn type_edit_with<T: KeyTyper>(
    typer: &T,
    accessibility_trusted: bool,
    backspaces: usize,
    text: &str,
) -> PasteResult {
    type_edit_guarded(typer, &mut NoGuard, accessibility_trusted, backspaces, text)
        .map(|_| ())
        .map_err(|e| e.error)
}

/// Types the edit at the active cursor with real key events.
pub fn type_edit(backspaces: usize, text: &str) -> PasteResult {
    #[cfg(target_os = "macos")]
    return super::macos::type_edit_via_cgevent(backspaces, text);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (backspaces, text);
        Err(PasteError::InjectFailed(
            "typing is not implemented on this platform".to_string(),
        ))
    }
}

/// Types the edit at the active cursor with real key events, asking `guard`
/// before every key action.
pub fn type_edit_checked<G: TypingGuard>(
    guard: &mut G,
    backspaces: usize,
    text: &str,
) -> Result<EditOutcome, EditError> {
    #[cfg(target_os = "macos")]
    return super::macos::type_edit_guarded_via_cgevent(guard, backspaces, text);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (guard, backspaces, text);
        Err(EditError {
            error: PasteError::InjectFailed("typing is not implemented on this platform".into()),
            typed_bytes: None,
        })
    }
}

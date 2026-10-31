//! Clipboard snapshot and restore logic for the paste pipeline.
//!
//! Pure helpers, testable without the real system clipboard.

use crate::paste::ClipboardSnapshot;

// ---------------------------------------------------------------------------
// Snapshot construction
// ---------------------------------------------------------------------------

/// Build a [`ClipboardSnapshot`] from the current clipboard text and
/// change-counter value.
///
/// Call this *before* writing dictation text to the clipboard.
pub fn save_snapshot(text: &str, change_count: u64) -> ClipboardSnapshot {
    ClipboardSnapshot {
        text: text.to_string(),
        change_count,
    }
}

// ---------------------------------------------------------------------------
// Restore guard
// ---------------------------------------------------------------------------

/// True when the clipboard may be restored to the snapshot.
///
/// `after_our_write` is the change counter read right after we wrote the
/// dictation. If the counter is still that value, nobody else wrote during
/// the paste window. If it moved, another app owns the clipboard now and we
/// must not overwrite it. (Comparing with the post-write value, not
/// `snapshot + 1`, because a platform write may bump the counter by more
/// than one.)
pub fn should_restore(after_our_write: u64, current: u64) -> bool {
    current == after_our_write
}

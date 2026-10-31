//! Pure-logic filters for the hotkey subsystem.
//!
//! All functions here are free of side effects and require no runtime
//! permissions. They can be tested entirely in isolation.

use std::time::Duration;

use crate::hotkey::{HotkeyEvent, HotkeyState};

/// Returns `true` if the hold duration is long enough to be treated as an
/// intentional press-and-hold (i.e. >= 300 ms by default).
///
/// The threshold is 300 ms: anything shorter is discarded as a quick tap.
///
/// # Arguments
/// * `hold_duration` - Wall-clock time between the key-down and key-up events.
///
/// # Examples
/// ```
/// use std::time::Duration;
/// use deepai_voice_lib::hotkey::filter::apply_quick_tap_filter;
///
/// assert!(!apply_quick_tap_filter(Duration::from_millis(200))); // discarded
/// assert!(apply_quick_tap_filter(Duration::from_millis(300)));  // accepted
/// ```
pub fn apply_quick_tap_filter(hold_duration: Duration) -> bool {
    hold_duration.as_millis() >= 300
}

/// Returns `true` if the incoming `event` should be passed through to the
/// state machine given the current `state`.
///
/// Rules:
/// - Any `Press` while `Recording` or `Busy` is blocked (returns `false`).
/// - Everything else is allowed (returns `true`).
///
/// # Arguments
/// * `state` - Current state of the hotkey state machine.
/// * `event` - The incoming `HotkeyEvent` to evaluate.
pub fn apply_busy_guard(state: &HotkeyState, event: HotkeyEvent) -> bool {
    !matches!(
        (state, event),
        (
            HotkeyState::Recording | HotkeyState::Busy,
            HotkeyEvent::Press
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_boundary_299ms() {
        assert!(!apply_quick_tap_filter(Duration::from_millis(299)));
    }

    #[test]
    fn filter_boundary_300ms() {
        assert!(apply_quick_tap_filter(Duration::from_millis(300)));
    }

    #[test]
    fn guard_idle_press_allowed() {
        assert!(apply_busy_guard(&HotkeyState::Idle, HotkeyEvent::Press));
    }

    #[test]
    fn guard_recording_press_blocked() {
        assert!(!apply_busy_guard(
            &HotkeyState::Recording,
            HotkeyEvent::Press
        ));
    }
}

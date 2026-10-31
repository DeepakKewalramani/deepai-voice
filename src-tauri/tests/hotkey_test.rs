/// Integration tests for the hotkey state machine (Slice 2.1).
///
/// These tests exercise the pure-logic layer only and require no runtime
/// permissions (no Input Monitoring, no Accessibility, no CGEventTap).
use deepai_voice_lib::hotkey::{
    HotkeyAction, HotkeyConfig, HotkeyController, HotkeyEvent, HotkeyState,
    INPUT_MONITORING_SETTINGS_URL, PermissionCheck,
    filter::{apply_busy_guard, apply_quick_tap_filter},
    permission_check,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// HotkeyEvent distinctness
// ---------------------------------------------------------------------------

#[test]
fn hotkey_event_press_ne_release() {
    assert_ne!(
        HotkeyEvent::Press,
        HotkeyEvent::Release,
        "Press and Release must not be equal"
    );
}

// ---------------------------------------------------------------------------
// Quick-tap filter: events under 300 ms are discarded
// ---------------------------------------------------------------------------

#[test]
fn quick_tap_under_300ms_is_discarded() {
    let hold = Duration::from_millis(200);
    let result = apply_quick_tap_filter(hold);
    assert!(
        !result,
        "hold of {}ms must be discarded as a quick tap",
        hold.as_millis()
    );
}

#[test]
fn hold_exactly_300ms_is_accepted() {
    let hold = Duration::from_millis(300);
    let result = apply_quick_tap_filter(hold);
    assert!(result, "hold of exactly 300ms must be accepted (boundary)");
}

#[test]
fn hold_over_300ms_is_accepted() {
    let hold = Duration::from_millis(500);
    let result = apply_quick_tap_filter(hold);
    assert!(result, "hold of {}ms must be accepted", hold.as_millis());
}

#[test]
fn quick_tap_at_0ms_is_discarded() {
    let hold = Duration::from_millis(0);
    let result = apply_quick_tap_filter(hold);
    assert!(!result, "zero-duration hold must be discarded");
}

#[test]
fn quick_tap_at_299ms_is_discarded() {
    let hold = Duration::from_millis(299);
    let result = apply_quick_tap_filter(hold);
    assert!(!result, "hold of 299ms must be discarded");
}

// ---------------------------------------------------------------------------
// Busy-state guard: second Press while recording is ignored
// ---------------------------------------------------------------------------

#[test]
fn busy_guard_idle_allows_press() {
    let state = HotkeyState::Idle;
    let allowed = apply_busy_guard(&state, HotkeyEvent::Press);
    assert!(allowed, "Press from Idle must be allowed");
}

#[test]
fn busy_guard_recording_blocks_press() {
    let state = HotkeyState::Recording;
    let allowed = apply_busy_guard(&state, HotkeyEvent::Press);
    assert!(!allowed, "Press while Recording must be blocked");
}

#[test]
fn busy_guard_busy_blocks_press() {
    let state = HotkeyState::Busy;
    let allowed = apply_busy_guard(&state, HotkeyEvent::Press);
    assert!(!allowed, "Press while Busy must be blocked");
}

#[test]
fn busy_guard_recording_allows_release() {
    let state = HotkeyState::Recording;
    let allowed = apply_busy_guard(&state, HotkeyEvent::Release);
    assert!(allowed, "Release from Recording must be allowed");
}

#[test]
fn busy_guard_idle_allows_release() {
    let state = HotkeyState::Idle;
    let allowed = apply_busy_guard(&state, HotkeyEvent::Release);
    assert!(
        allowed,
        "Release from Idle must be allowed (no-op, harmless)"
    );
}

// ---------------------------------------------------------------------------
// HotkeyConfig: primary and fallback key configurations
// ---------------------------------------------------------------------------

#[test]
fn hotkey_config_primary_is_fn() {
    let cfg = HotkeyConfig::default();
    assert!(cfg.primary_is_fn, "default primary key must be Fn (Globe)");
}

#[test]
fn hotkey_config_fallback_is_ctrl_option_space() {
    let cfg = HotkeyConfig::default();
    assert_eq!(
        cfg.fallback_shortcut, "Ctrl+Option+Space",
        "default fallback shortcut must be Ctrl+Option+Space"
    );
}

#[test]
fn hotkey_config_quick_tap_threshold_is_300ms() {
    let cfg = HotkeyConfig::default();
    assert_eq!(
        cfg.quick_tap_threshold_ms, 300,
        "default quick-tap threshold must be 300 ms"
    );
}

#[test]
fn hotkey_config_custom_threshold() {
    let cfg = HotkeyConfig {
        primary_is_fn: true,
        fallback_shortcut: "Ctrl+Option+Space".to_string(),
        quick_tap_threshold_ms: 150,
    };
    assert_eq!(cfg.quick_tap_threshold_ms, 150);
}

// ---------------------------------------------------------------------------
// State machine transitions
// ---------------------------------------------------------------------------

#[test]
fn state_machine_idle_to_recording_on_press() {
    let mut state = HotkeyState::Idle;
    state.transition(HotkeyEvent::Press);
    assert_eq!(state, HotkeyState::Recording);
}

#[test]
fn state_machine_recording_to_idle_on_release() {
    let mut state = HotkeyState::Recording;
    state.transition(HotkeyEvent::Release);
    assert_eq!(state, HotkeyState::Idle);
}

#[test]
fn state_machine_recording_stays_on_press() {
    // A second Press while Recording must not change state (busy guard
    // prevents delivery, but even if it arrives, the machine stays Recording).
    let mut state = HotkeyState::Recording;
    state.transition(HotkeyEvent::Press);
    assert_eq!(state, HotkeyState::Recording);
}

#[test]
fn state_machine_busy_stays_on_press() {
    let mut state = HotkeyState::Busy;
    state.transition(HotkeyEvent::Press);
    assert_eq!(state, HotkeyState::Busy, "Press while Busy must keep Busy");
}

#[test]
fn state_machine_busy_to_idle_on_release() {
    let mut state = HotkeyState::Busy;
    state.transition(HotkeyEvent::Release);
    assert_eq!(state, HotkeyState::Idle);
}

// ---------------------------------------------------------------------------
// Controller: busy guard, quick tap, pairing, permission
// ---------------------------------------------------------------------------

fn controller() -> HotkeyController {
    HotkeyController::new(&HotkeyConfig::default())
}

#[test]
fn controller_busy_ignores_press_and_its_release() {
    let mut c = controller();
    let t = Instant::now();
    c.mark_busy();
    assert_eq!(c.handle(HotkeyEvent::Press, t), HotkeyAction::None);
    assert_eq!(c.state(), HotkeyState::Busy);
    let later = t + Duration::from_millis(900);
    assert_eq!(c.handle(HotkeyEvent::Release, later), HotkeyAction::None);
    assert_eq!(c.state(), HotkeyState::Busy, "release must not clear Busy");
    c.mark_idle();
    assert_eq!(
        c.handle(HotkeyEvent::Press, later),
        HotkeyAction::StartRecording
    );
}

#[test]
fn controller_second_press_while_recording_is_ignored() {
    let mut c = controller();
    let t = Instant::now();
    assert_eq!(
        c.handle(HotkeyEvent::Press, t),
        HotkeyAction::StartRecording
    );
    assert_eq!(c.handle(HotkeyEvent::Press, t), HotkeyAction::None);
    let r = c.handle(HotkeyEvent::Release, t + Duration::from_millis(500));
    assert_eq!(r, HotkeyAction::None, "release of ignored press is ignored");
    // The original hold is still open until its own release.
    assert_eq!(c.state(), HotkeyState::Recording);
}

#[test]
fn controller_quick_tap_under_300ms_is_discarded() {
    let mut c = controller();
    let t = Instant::now();
    c.handle(HotkeyEvent::Press, t);
    let r = c.handle(HotkeyEvent::Release, t + Duration::from_millis(299));
    assert_eq!(r, HotkeyAction::DiscardRecording);
    assert_eq!(c.state(), HotkeyState::Idle);
}

#[test]
fn controller_hold_of_300ms_stops_recording() {
    let mut c = controller();
    let t = Instant::now();
    c.handle(HotkeyEvent::Press, t);
    let r = c.handle(HotkeyEvent::Release, t + Duration::from_millis(300));
    assert_eq!(r, HotkeyAction::StopRecording);
}

#[test]
fn controller_release_without_press_is_noop() {
    let mut c = controller();
    assert_eq!(
        c.handle(HotkeyEvent::Release, Instant::now()),
        HotkeyAction::None
    );
    assert_eq!(c.state(), HotkeyState::Idle);
}

#[test]
fn harness_20_of_20_press_release_pairs() {
    let mut c = controller();
    let mut t = Instant::now();
    let (mut starts, mut stops) = (0, 0);
    for i in 0..20u64 {
        if c.handle(HotkeyEvent::Press, t) == HotkeyAction::StartRecording {
            starts += 1;
        }
        t += Duration::from_millis(400 + i * 10);
        if c.handle(HotkeyEvent::Release, t) == HotkeyAction::StopRecording {
            stops += 1;
        }
        assert_eq!(c.state(), HotkeyState::Idle, "pair {i} must end Idle");
        t += Duration::from_millis(50);
    }
    assert_eq!((starts, stops), (20, 20));
}

#[test]
fn permission_missing_gives_system_settings_deep_link() {
    assert_eq!(
        permission_check(false),
        PermissionCheck::Missing {
            deep_link: INPUT_MONITORING_SETTINGS_URL
        }
    );
    assert!(INPUT_MONITORING_SETTINGS_URL.ends_with("Privacy_ListenEvent"));
    assert_eq!(permission_check(true), PermissionCheck::Granted);
}

#[test]
fn fallback_shortcut_string_parses() {
    let cfg = HotkeyConfig::default();
    assert!(deepai_voice_lib::hotkey::fallback::parse(&cfg.fallback_shortcut).is_ok());
}

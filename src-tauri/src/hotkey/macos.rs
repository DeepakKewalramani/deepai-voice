//! macOS hotkey capture backend: Fn (Globe) key through a listen-only
//! CGEventTap on flagsChanged events, plus a channel mock for tests.
//!
//! Input Monitoring permission is required. Check it with `has_permission`
//! (CGPreflightListenEventAccess) and ask with `request_permission`. The
//! tap reads only the modifier flag bit for Fn; no key codes or characters
//! are read or logged.
//!
//! The real tap cannot be verified headless (needs the permission and a
//! physical Fn key).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::hotkey::HotkeyEvent;

/// A mock event source that replaces CGEventTap in unit tests.
///
/// Call `inject` to push synthetic key events; the receiver is consumed by
/// the state machine under test.
pub struct MockEventSource {
    sender: Sender<HotkeyEvent>,
}

impl MockEventSource {
    /// Create a linked (sender, receiver) pair.
    pub fn channel() -> (Self, Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel();
        (Self { sender: tx }, rx)
    }

    /// Inject a synthetic `HotkeyEvent` as if it came from CGEventTap.
    pub fn inject(&self, event: HotkeyEvent) {
        // Ignore send errors in tests (receiver may have been dropped).
        let _ = self.sender.send(event);
    }
}

// ---------------------------------------------------------------------------
// Real CGEventTap
// ---------------------------------------------------------------------------

/// kCGEventFlagMaskSecondaryFn (1 << 23).
pub const FN_FLAG_MASK: u64 = 0x0080_0000;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
/// kCGEventTapDisabledByTimeout: the system disabled the tap (slow callback).
pub const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
/// kCGEventTapDisabledByUserInput: the system disabled the tap on user input.
pub const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

/// Pure edge detector: given whether Fn was down and the new flags, returns
/// the Press or Release transition, if any.
pub fn fn_edge(was_down: bool, flags: u64) -> Option<HotkeyEvent> {
    let down = flags & FN_FLAG_MASK != 0;
    match (was_down, down) {
        (false, true) => Some(HotkeyEvent::Press),
        (true, false) => Some(HotkeyEvent::Release),
        _ => None,
    }
}

/// What the tap callback must do for an event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapAction {
    /// The system disabled the tap: call CGEventTapEnable(port, true).
    ReEnable,
    /// A modifier flags change: run Fn edge detection.
    HandleFlags,
    Ignore,
}

/// Pure decision for a tap callback event type.
pub fn tap_action(event_type: u32) -> TapAction {
    match event_type {
        K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT | K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT => {
            TapAction::ReEnable
        }
        K_CG_EVENT_FLAGS_CHANGED => TapAction::HandleFlags,
        _ => TapAction::Ignore,
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetFlags(event: *mut c_void) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
    fn CFRelease(cf: *mut c_void);
}

/// True when Input Monitoring is already granted (no prompt).
pub fn has_permission() -> bool {
    // SAFETY: plain C call with no arguments.
    unsafe { CGPreflightListenEventAccess() }
}

/// Shows the system Input Monitoring prompt once; returns current status.
pub fn request_permission() -> bool {
    // SAFETY: plain C call with no arguments.
    unsafe { CGRequestListenEventAccess() }
}

struct TapContext {
    tx: Sender<HotkeyEvent>,
    fn_down: AtomicBool,
    /// The tap mach port, set after creation, used to re-enable the tap.
    port: AtomicPtr<c_void>,
}

extern "C" fn tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void {
    if user_info.is_null() {
        return event;
    }
    // SAFETY: user_info is the leaked TapContext created in
    // install_cg_event_tap and lives for the process lifetime.
    let ctx = unsafe { &*(user_info as *const TapContext) };
    match tap_action(event_type) {
        TapAction::ReEnable => {
            let port = ctx.port.load(Ordering::Acquire);
            if !port.is_null() {
                tracing::warn!("event tap disabled by system, re-enabling");
                // SAFETY: port is the live tap mach port stored at install.
                unsafe { CGEventTapEnable(port, true) };
            }
            // Fn state may be stale after a gap; release if it was held.
            if ctx.fn_down.swap(false, Ordering::Relaxed) {
                let _ = ctx.tx.send(HotkeyEvent::Release);
            }
        }
        TapAction::HandleFlags => {
            // SAFETY: event is valid for the duration of the callback.
            let flags = unsafe { CGEventGetFlags(event) };
            let was_down = ctx.fn_down.load(Ordering::Relaxed);
            if let Some(edge) = fn_edge(was_down, flags) {
                ctx.fn_down
                    .store(edge == HotkeyEvent::Press, Ordering::Relaxed);
                tracing::debug!(?edge, "fn hotkey edge");
                let _ = ctx.tx.send(edge);
            }
        }
        TapAction::Ignore => {}
    }
    event
}

/// Installs the Fn listener on a dedicated thread with its own run loop.
///
/// Returns the event receiver, or an error (for example missing Input
/// Monitoring permission) so the caller can fall back to the shortcut.
pub fn install_cg_event_tap() -> Result<Receiver<HotkeyEvent>, String> {
    if !has_permission() {
        return Err("Input Monitoring permission missing".to_string());
    }
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("fn-hotkey-tap".into())
        .spawn(move || {
            let ctx = Box::into_raw(Box::new(TapContext {
                tx,
                fn_down: AtomicBool::new(false),
                port: AtomicPtr::new(std::ptr::null_mut()),
            }));
            // SAFETY: arguments follow the CGEventTapCreate contract; the
            // context pointer stays valid because it is never freed.
            let port = unsafe {
                CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    1u64 << K_CG_EVENT_FLAGS_CHANGED,
                    tap_callback,
                    ctx as *mut c_void,
                )
            };
            if port.is_null() {
                let _ = ready_tx.send(Err("CGEventTapCreate failed".to_string()));
                return;
            }
            // SAFETY: port is a valid mach port; run loop calls are on this thread.
            unsafe {
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
                if source.is_null() {
                    CFRelease(port);
                    drop(Box::from_raw(ctx));
                    let _ = ready_tx.send(Err("CFMachPortCreateRunLoopSource failed".to_string()));
                    return;
                }
                (*ctx).port.store(port, Ordering::Release);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                CGEventTapEnable(port, true);
            }
            let _ = ready_tx.send(Ok(()));
            // SAFETY: runs this thread's run loop until process exit.
            unsafe { CFRunLoopRun() };
        })
        .map_err(|e| e.to_string())?;
    ready_rx
        .recv()
        .map_err(|_| "tap thread ended early".to_string())??;
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_source_sends_press() {
        let (source, rx) = MockEventSource::channel();
        source.inject(HotkeyEvent::Press);
        assert_eq!(rx.recv().unwrap(), HotkeyEvent::Press);
    }

    #[test]
    fn mock_source_sends_release() {
        let (source, rx) = MockEventSource::channel();
        source.inject(HotkeyEvent::Release);
        assert_eq!(rx.recv().unwrap(), HotkeyEvent::Release);
    }

    #[test]
    fn mock_source_sends_sequence() {
        let (source, rx) = MockEventSource::channel();
        source.inject(HotkeyEvent::Press);
        source.inject(HotkeyEvent::Release);
        assert_eq!(rx.recv().unwrap(), HotkeyEvent::Press);
        assert_eq!(rx.recv().unwrap(), HotkeyEvent::Release);
    }

    #[test]
    fn tap_action_reenables_on_system_disable() {
        assert_eq!(tap_action(0xFFFF_FFFE), TapAction::ReEnable);
        assert_eq!(tap_action(0xFFFF_FFFF), TapAction::ReEnable);
    }

    #[test]
    fn tap_action_handles_flags_changed_only() {
        assert_eq!(tap_action(12), TapAction::HandleFlags);
        assert_eq!(tap_action(10), TapAction::Ignore); // keyDown
        assert_eq!(tap_action(0), TapAction::Ignore);
    }

    #[test]
    fn fn_edge_detects_press_and_release() {
        assert_eq!(fn_edge(false, FN_FLAG_MASK), Some(HotkeyEvent::Press));
        assert_eq!(fn_edge(true, 0), Some(HotkeyEvent::Release));
        assert_eq!(fn_edge(true, FN_FLAG_MASK), None);
        assert_eq!(fn_edge(false, 0), None);
        // Other modifier bits alone are not Fn.
        assert_eq!(fn_edge(false, 0x0004_0000), None);
    }
}

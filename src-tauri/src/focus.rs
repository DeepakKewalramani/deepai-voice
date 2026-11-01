//! Typing safety guard: is it still safe to send key events for this hold?
//!
//! At hold start we record the frontmost app and a count of real user input
//! events. Before every typing or Backspace action the guard compares the
//! current values with that start:
//! - A secure text field is active (IsSecureEventInputEnabled): stop.
//! - The frontmost app changed, or cannot be read: stop for the rest of the
//!   hold. Coming back to the first app does not resume typing.
//! - The user pressed a key or clicked since the hold started: the caret may
//!   have moved, so never press Backspace again in this hold. Plain appends of
//!   new words continue.
//!
//! User input is counted with CGEventSourceCounterForEventType on the
//! HID system state. That state covers only events coming from hardware
//! (Apple docs: "combined state of all hardware event sources posting from
//! the HID system"), so our own typing, posted from a private event source
//! to the session tap, does not move the counter and needs no subtraction.
//! Modifier keys (Fn, Option) produce flagsChanged events, not keyDown, so
//! holding the dictation key does not count either.
//!
//! The decision logic is pure and tested; only `SystemProbe` touches macOS.
//! Nothing here reads or logs any text.

/// Why typing stopped for the rest of a hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    AppChanged,
    SecureField,
    /// The user pressed Esc. Never produced by `gate`.
    Cancelled,
}

impl StopReason {
    /// Fixed status text. Never contains dictated text.
    pub fn status(self) -> &'static str {
        match self {
            StopReason::AppChanged => "Typing paused: app changed",
            StopReason::SecureField => "Secure field: not typing",
            StopReason::Cancelled => "Cancelled",
        }
    }
}

/// What the next key action may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// Typing and Backspace are both fine.
    Go,
    /// New text may be typed, Backspace may not.
    NoBackspace,
    /// Send nothing.
    Stop(StopReason),
}

/// Focus state at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Process id of the frontmost app, None when it cannot be read.
    pub app: Option<i32>,
    /// Real (hardware) key-down plus mouse-down events seen so far.
    pub input_count: u64,
}

/// Pure decision: compare the hold-start snapshot with now.
pub fn gate(start: &Snapshot, now: &Snapshot, secure_input: bool) -> Gate {
    if secure_input {
        return Gate::Stop(StopReason::SecureField);
    }
    if start.app.is_none() || now.app != start.app {
        return Gate::Stop(StopReason::AppChanged);
    }
    if now.input_count != start.input_count {
        return Gate::NoBackspace;
    }
    Gate::Go
}

/// Sticky per-hold guard: once stopped or once Backspace is blocked, it
/// stays that way until the next hold builds a new guard.
#[derive(Debug, Clone)]
pub struct HoldGuard {
    start: Snapshot,
    stopped: Option<StopReason>,
    no_backspace: bool,
}

impl HoldGuard {
    pub fn new(start: Snapshot) -> Self {
        Self {
            start,
            stopped: None,
            no_backspace: false,
        }
    }

    pub fn check(&mut self, now: Snapshot, secure_input: bool) -> Gate {
        if let Some(r) = self.stopped {
            return Gate::Stop(r);
        }
        match gate(&self.start, &now, secure_input) {
            Gate::Stop(r) => {
                self.stopped = Some(r);
                Gate::Stop(r)
            }
            Gate::NoBackspace => {
                self.no_backspace = true;
                Gate::NoBackspace
            }
            Gate::Go if self.no_backspace => Gate::NoBackspace,
            Gate::Go => Gate::Go,
        }
    }
}

/// Asked before every key action. Real guard on macOS, scripted in tests.
pub trait TypingGuard {
    fn check(&mut self) -> Gate;
}

/// Reads the focus state. Real on macOS, faked in tests.
pub trait FocusProbe {
    fn snapshot(&self) -> Snapshot;
    fn secure_input(&self) -> bool;
}

/// A `HoldGuard` fed by a probe. The start snapshot is taken on `new`.
pub struct ProbeGuard<P: FocusProbe> {
    probe: P,
    hold: HoldGuard,
}

impl<P: FocusProbe> ProbeGuard<P> {
    pub fn new(probe: P) -> Self {
        let start = probe.snapshot();
        Self::with_start(probe, start)
    }

    /// Uses a start snapshot taken earlier (at the key press).
    pub fn with_start(probe: P, start: Snapshot) -> Self {
        Self {
            probe,
            hold: HoldGuard::new(start),
        }
    }

    pub fn probe_mut(&mut self) -> &mut P {
        &mut self.probe
    }
}

impl<P: FocusProbe> TypingGuard for ProbeGuard<P> {
    fn check(&mut self) -> Gate {
        let now = self.probe.snapshot();
        let secure = self.probe.secure_input();
        self.hold.check(now, secure)
    }
}

/// The real system probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProbe;

#[cfg(target_os = "macos")]
mod sys {
    use super::Snapshot;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceCounterForEventType(state_id: i32, event_type: u32) -> u32;
    }

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        /// HIToolbox. Returns a C `Boolean` (unsigned char).
        fn IsSecureEventInputEnabled() -> u8;
    }

    // Makes sure AppKit is loaded so NSWorkspace resolves at runtime.
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    /// kCGEventSourceStateHIDSystemState: hardware events only.
    const HID_SYSTEM_STATE: i32 = 1;
    /// kCGEventLeftMouseDown, kCGEventRightMouseDown, kCGEventKeyDown,
    /// kCGEventOtherMouseDown.
    const COUNTED_EVENTS: [u32; 4] = [1, 3, 10, 25];

    fn frontmost_pid() -> Option<i32> {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        objc2::rc::autoreleasepool(|_| {
            // SAFETY: NSWorkspace.sharedWorkspace, -frontmostApplication and
            // -processIdentifier are plain AppKit getters. Each pointer is
            // null-checked before use and only used inside this pool.
            unsafe {
                let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                if ws.is_null() {
                    return None;
                }
                let app: *mut AnyObject = msg_send![ws, frontmostApplication];
                if app.is_null() {
                    return None;
                }
                let pid: i32 = msg_send![app, processIdentifier];
                (pid > 0).then_some(pid)
            }
        })
    }

    fn input_count() -> u64 {
        COUNTED_EVENTS
            .iter()
            // SAFETY: a documented CoreGraphics getter with plain integer args.
            .map(|&t| u64::from(unsafe { CGEventSourceCounterForEventType(HID_SYSTEM_STATE, t) }))
            .sum()
    }

    pub fn snapshot() -> Snapshot {
        Snapshot {
            app: frontmost_pid(),
            input_count: input_count(),
        }
    }

    pub fn secure_input() -> bool {
        // SAFETY: takes no arguments and has no side effects.
        unsafe { IsSecureEventInputEnabled() != 0 }
    }
}

impl FocusProbe for SystemProbe {
    #[cfg(target_os = "macos")]
    fn snapshot(&self) -> Snapshot {
        sys::snapshot()
    }

    /// No frontmost app API here: the guard stops, which is the safe side.
    #[cfg(not(target_os = "macos"))]
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            app: None,
            input_count: 0,
        }
    }

    #[cfg(target_os = "macos")]
    fn secure_input(&self) -> bool {
        sys::secure_input()
    }

    #[cfg(not(target_os = "macos"))]
    fn secure_input(&self) -> bool {
        false
    }
}

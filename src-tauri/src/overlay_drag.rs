//! Dragging the notch pill without ever taking keyboard focus.
//!
//! The overlay window ignores the mouse, so a click on it would normally
//! reach the app below. To drag the pill we watch left mouse events with an
//! active CGEventTap: a press that starts inside the visible capsule (and
//! the drag and release that follow it) is swallowed, so no app sees it and
//! the focused app keeps keyboard focus; every other event passes through
//! unchanged. The window itself never receives a click and never activates.
//!
//! The tap callback only runs `DragState` (pure, tested below) under a tiny
//! mutex and posts window moves to a worker thread over a bounded channel.
//! On release the worker snaps the pill to the nearest screen edge and saves
//! the placement (see `overlay::snap_placement`).
//!
//! An active tap needs the Accessibility permission. Without it the tap is
//! not installed and the check repeats every 3 s. Nothing here logs mouse
//! coordinates.
//!
//! Coordinates: logical points, origin at the top-left of the primary
//! display, y down. CGEventGetLocation already reports this space (it is
//! not the flipped, bottom-left Cocoa space), and so do tao's logical
//! window and monitor positions on macOS, across monitors of any scale.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, LogicalPosition, Manager, Runtime};

use crate::overlay::{self, OVERLAY_HEIGHT, OVERLAY_WIDTH, OVERLAY_WINDOW, Position, Rect, Screen};

/// Movement in points before a press on the capsule becomes a drag. A plain
/// click (less movement) is swallowed and does nothing.
pub const DRAG_THRESHOLD: f64 = 3.0;
/// Event sent to the overlay window with `{dragging: bool}`.
pub const DRAG_EVENT: &str = "overlay-drag";
/// Event the overlay sends with its capsule size `{width, height}`.
pub const CAPSULE_EVENT: &str = "overlay-capsule";
/// Pending window moves the worker may lag behind by; extra moves are
/// dropped (a newer one follows), so the tap never blocks.
const QUEUE: usize = 256;

/// Left mouse event kinds the tap watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down,
    Dragged,
    Up,
}

/// What the tap must do with one mouse event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragAction {
    /// Swallow the event and move the window origin here.
    MoveTo(f64, f64),
    /// Swallow the event, nothing else.
    Swallow,
    /// Hand the event on unchanged.
    PassThrough,
    /// Swallow the event; the drag ended with the window origin here.
    Drop(f64, f64),
}

impl DragAction {
    /// True when the tap must return NULL for this event.
    pub fn swallows(self) -> bool {
        !matches!(self, DragAction::PassThrough)
    }
}

/// Where the overlay window and its capsule are right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub window: (f64, f64),
    pub capsule: Rect,
}

/// Drag gesture state, fed one mouse event at a time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DragState {
    #[default]
    Idle,
    /// Pressed on the capsule; `grab` is the press point minus the window
    /// origin, `start` the press point.
    Pressed { start: (f64, f64), grab: (f64, f64) },
    /// Moving; `last` is the last window origin sent.
    Dragging { grab: (f64, f64), last: (f64, f64) },
}

impl DragState {
    /// Advances the gesture for one event at global `point`. `frame` is
    /// `None` when the window has not been placed yet (nothing to grab).
    pub fn handle(
        &mut self,
        kind: MouseKind,
        point: (f64, f64),
        frame: Option<Frame>,
    ) -> DragAction {
        let (px, py) = point;
        match (*self, kind) {
            (DragState::Idle, MouseKind::Down) => match frame {
                Some(f) if f.capsule.contains(px, py) => {
                    *self = DragState::Pressed {
                        start: point,
                        grab: (px - f.window.0, py - f.window.1),
                    };
                    DragAction::Swallow
                }
                _ => DragAction::PassThrough,
            },
            (DragState::Idle, _) => DragAction::PassThrough,
            (DragState::Pressed { start, grab }, MouseKind::Dragged) => {
                let (dx, dy) = (px - start.0, py - start.1);
                if (dx * dx + dy * dy).sqrt() < DRAG_THRESHOLD {
                    return DragAction::Swallow;
                }
                let last = (px - grab.0, py - grab.1);
                *self = DragState::Dragging { grab, last };
                DragAction::MoveTo(last.0, last.1)
            }
            (DragState::Pressed { .. }, MouseKind::Up) => {
                *self = DragState::Idle;
                DragAction::Swallow
            }
            (DragState::Dragging { grab, .. }, MouseKind::Dragged) => {
                let last = (px - grab.0, py - grab.1);
                *self = DragState::Dragging { grab, last };
                DragAction::MoveTo(last.0, last.1)
            }
            (DragState::Dragging { grab, .. }, MouseKind::Up) => {
                *self = DragState::Idle;
                DragAction::Drop(px - grab.0, py - grab.1)
            }
            // A press while a gesture is open means the release was missed.
            (DragState::Dragging { last, .. }, MouseKind::Down) => {
                *self = DragState::Idle;
                DragAction::Drop(last.0, last.1)
            }
            (DragState::Pressed { .. }, MouseKind::Down) => {
                *self = DragState::Idle;
                self.handle(kind, point, frame)
            }
        }
    }

    /// Forgets the gesture (the tap was disabled and may have missed the
    /// release). Returns a `Drop` at the last origin if a drag was running.
    pub fn reset(&mut self) -> Option<DragAction> {
        let action = match *self {
            DragState::Dragging { last, .. } => Some(DragAction::Drop(last.0, last.1)),
            _ => None,
        };
        *self = DragState::Idle;
        action
    }
}

// ---------------------------------------------------------------------------
// Shared state between the tap callback and the worker
// ---------------------------------------------------------------------------

struct Shared {
    state: DragState,
    window: Option<(f64, f64)>,
    edge: Position,
    capsule: Option<(f64, f64)>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    state: DragState::Idle,
    window: None,
    edge: Position::Top,
    capsule: None,
});

/// Records where the overlay window now is (called by `overlay` after
/// every move) so presses can be hit-tested against the capsule.
pub fn set_window_frame(origin: (f64, f64), edge: Position) {
    if let Ok(mut s) = SHARED.lock() {
        s.window = Some(origin);
        s.edge = edge;
    }
}

/// Records the capsule size the overlay reported.
pub fn set_capsule_size(size: (f64, f64)) {
    if let Ok(mut s) = SHARED.lock() {
        s.capsule = Some(size);
    }
}

fn shared_edge_and_capsule() -> (Position, Option<(f64, f64)>) {
    SHARED
        .lock()
        .map(|s| (s.edge, s.capsule))
        .unwrap_or((Position::Top, None))
}

/// Runs one event through the shared state. Called from the tap callback:
/// one short lock, no allocation.
fn handle_shared(kind: MouseKind, point: (f64, f64)) -> DragAction {
    let Ok(mut s) = SHARED.lock() else {
        return DragAction::PassThrough;
    };
    let frame = s.window.map(|w| Frame {
        window: w,
        capsule: overlay::capsule_rect(w, s.edge, s.capsule),
    });
    s.state.handle(kind, point, frame)
}

fn reset_shared() -> Option<DragAction> {
    SHARED.lock().ok().and_then(|mut s| s.state.reset())
}

// ---------------------------------------------------------------------------
// Worker: moves the window and snaps on drop
// ---------------------------------------------------------------------------

/// Message from the tap to the worker: a window origin.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Msg {
    Move(f64, f64),
    Drop(f64, f64),
}

fn msg_for(action: DragAction) -> Option<Msg> {
    match action {
        DragAction::MoveTo(x, y) => Some(Msg::Move(x, y)),
        DragAction::Drop(x, y) => Some(Msg::Drop(x, y)),
        _ => None,
    }
}

fn post(tx: &SyncSender<Msg>, action: DragAction) {
    if let Some(msg) = msg_for(action) {
        match tx.try_send(msg) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            // A newer move follows; a lost drop is recovered by the next
            // press, which resets the gesture.
            Err(TrySendError::Full(_)) => {}
        }
    }
}

#[derive(Clone, Serialize)]
struct DragPayload {
    dragging: bool,
}

fn emit_dragging<R: Runtime>(app: &AppHandle<R>, dragging: bool) {
    let _ = app.emit_to(OVERLAY_WINDOW, DRAG_EVENT, DragPayload { dragging });
}

fn monitors<R: Runtime>(app: &AppHandle<R>) -> (Vec<Screen>, Option<Screen>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW) else {
        return (Vec::new(), None);
    };
    let all = window
        .available_monitors()
        .map(|ms| ms.iter().map(overlay::screen_of).collect())
        .unwrap_or_default();
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| overlay::screen_of(&m));
    (all, primary)
}

/// The window origin clamped to the monitor under the capsule centre (the
/// primary monitor when the centre is off every monitor), plus that monitor.
fn clamped(
    origin: (f64, f64),
    screens: &[Screen],
    primary: Option<Screen>,
) -> Option<((f64, f64), Screen)> {
    let (edge, size) = shared_edge_and_capsule();
    let (cx, cy) = overlay::capsule_rect(origin, edge, size).center();
    let screen = overlay::screen_at(screens, cx, cy).or(primary)?;
    let o = overlay::clamp_to_screen(screen, OVERLAY_WIDTH, OVERLAY_HEIGHT, origin);
    Some((o, screen))
}

fn run_worker<R: Runtime>(app: AppHandle<R>, rx: Receiver<Msg>) {
    let mut dragging = false;
    let mut screens: Vec<Screen> = Vec::new();
    let mut primary: Option<Screen> = None;
    let mut pending = rx.recv().ok();
    while let Some(mut msg) = pending.take() {
        // Coalesce queued moves to the newest one.
        let mut next = None;
        if let Msg::Move(..) = msg {
            while let Ok(m) = rx.try_recv() {
                if let Msg::Move(..) = m {
                    msg = m;
                } else {
                    next = Some(m);
                    break;
                }
            }
        }
        if !dragging {
            dragging = true;
            (screens, primary) = monitors(&app);
            emit_dragging(&app, true);
        }
        match msg {
            Msg::Move(x, y) => {
                if let Some((o, _)) = clamped((x, y), &screens, primary)
                    && let Some(window) = app.get_webview_window(OVERLAY_WINDOW)
                {
                    let _ = window.set_position(LogicalPosition::new(o.0, o.1));
                    let (edge, _) = shared_edge_and_capsule();
                    set_window_frame(o, edge);
                }
            }
            Msg::Drop(x, y) => {
                if let Some((o, screen)) = clamped((x, y), &screens, primary) {
                    let (edge, size) = shared_edge_and_capsule();
                    let center = overlay::capsule_rect(o, edge, size).center();
                    let placement =
                        overlay::snap_placement(screen, OVERLAY_WIDTH, OVERLAY_HEIGHT, center);
                    overlay::apply_placement(&app, placement, Some(screen));
                }
                dragging = false;
                emit_dragging(&app, false);
            }
        }
        pending = next.or_else(|| rx.recv().ok());
    }
}

/// Starts pill dragging: the capsule size listener, the window worker and
/// (on macOS, once Accessibility is granted) the mouse tap. Never fails;
/// without the permission the pill just stays where it is.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    app.listen_any(CAPSULE_EVENT, |event| {
        if let Some(size) = overlay::parse_capsule_size(event.payload()) {
            set_capsule_size(size);
        }
    });
    let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(QUEUE);
    let worker_app = app.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("overlay-drag-worker".into())
        .spawn(move || run_worker(worker_app, rx))
    {
        tracing::warn!("overlay drag worker not started: {e}");
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let spawned = std::thread::Builder::new()
            .name("overlay-drag-permission".into())
            .spawn(move || {
                let mut warned = false;
                loop {
                    if crate::paste::macos::is_accessibility_trusted() {
                        match tap::install(tx) {
                            Ok(()) => tracing::info!("overlay drag tap started"),
                            Err(e) => tracing::warn!("overlay drag tap unavailable: {e}"),
                        }
                        break;
                    }
                    if !warned {
                        tracing::warn!("Accessibility missing, pill dragging off until granted");
                        warned = true;
                    }
                    // Re-check every 3 s so granting the permission works
                    // without a restart.
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("overlay drag permission check not started: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    drop(tx);
}

// ---------------------------------------------------------------------------
// Event tap (macOS)
// ---------------------------------------------------------------------------

/// kCGEventLeftMouseDown.
pub const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
/// kCGEventLeftMouseUp.
pub const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
/// kCGEventLeftMouseDragged.
pub const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
/// kCGEventTapDisabledByTimeout.
pub const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
/// kCGEventTapDisabledByUserInput.
pub const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

/// What the tap callback must do for an event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapAction {
    /// The system disabled the tap: re-enable it and reset the gesture.
    ReEnable,
    /// A left mouse event for the drag state machine.
    Mouse(MouseKind),
    Ignore,
}

/// Pure decision for a tap callback event type.
pub fn tap_action(event_type: u32) -> TapAction {
    match event_type {
        K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT | K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT => {
            TapAction::ReEnable
        }
        K_CG_EVENT_LEFT_MOUSE_DOWN => TapAction::Mouse(MouseKind::Down),
        K_CG_EVENT_LEFT_MOUSE_DRAGGED => TapAction::Mouse(MouseKind::Dragged),
        K_CG_EVENT_LEFT_MOUSE_UP => TapAction::Mouse(MouseKind::Up),
        _ => TapAction::Ignore,
    }
}

/// Event mask for the three left mouse events.
pub fn event_mask() -> u64 {
    (1u64 << K_CG_EVENT_LEFT_MOUSE_DOWN)
        | (1u64 << K_CG_EVENT_LEFT_MOUSE_UP)
        | (1u64 << K_CG_EVENT_LEFT_MOUSE_DRAGGED)
}

#[cfg(target_os = "macos")]
mod tap {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::sync::mpsc::SyncSender;

    use super::{Msg, TapAction, event_mask, handle_shared, post, reset_shared, tap_action};

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    /// kCGEventTapOptionDefault: an active tap that may swallow events.
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
            user_info: *mut c_void,
        ) -> *mut c_void;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
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

    struct TapContext {
        tx: SyncSender<Msg>,
        /// The tap mach port, set after creation, used to re-enable the tap.
        port: AtomicPtr<c_void>,
    }

    extern "C" fn callback(
        _proxy: *mut c_void,
        event_type: u32,
        event: *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void {
        if user_info.is_null() {
            return event;
        }
        // SAFETY: user_info is the TapContext leaked in `install`; it lives
        // for the process lifetime.
        let ctx = unsafe { &*(user_info as *const TapContext) };
        let kind = match tap_action(event_type) {
            TapAction::ReEnable => {
                let port = ctx.port.load(Ordering::Acquire);
                if !port.is_null() {
                    tracing::warn!("overlay drag tap disabled by system, re-enabling");
                    // SAFETY: port is the live tap mach port stored at install.
                    unsafe { CGEventTapEnable(port, true) };
                }
                if let Some(action) = reset_shared() {
                    post(&ctx.tx, action);
                }
                return event;
            }
            TapAction::Ignore => return event,
            TapAction::Mouse(kind) => kind,
        };
        if event.is_null() {
            return event;
        }
        // SAFETY: event is a valid CGEvent for the duration of the callback.
        let p = unsafe { CGEventGetLocation(event) };
        let action = handle_shared(kind, (p.x, p.y));
        post(&ctx.tx, action);
        if action.swallows() {
            std::ptr::null_mut()
        } else {
            event
        }
    }

    /// Creates the active tap on its own thread with its own run loop.
    /// Returns once the tap is enabled or creation failed.
    pub(super) fn install(tx: SyncSender<Msg>) -> Result<(), &'static str> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), &'static str>>();
        std::thread::Builder::new()
            .name("overlay-drag-tap".into())
            .spawn(move || {
                let ctx = Box::into_raw(Box::new(TapContext {
                    tx,
                    port: AtomicPtr::new(std::ptr::null_mut()),
                }));
                // SAFETY: arguments follow the CGEventTapCreate contract; the
                // context pointer stays valid while the tap exists.
                let port = unsafe {
                    CGEventTapCreate(
                        K_CG_SESSION_EVENT_TAP,
                        K_CG_HEAD_INSERT_EVENT_TAP,
                        K_CG_EVENT_TAP_OPTION_DEFAULT,
                        event_mask(),
                        callback,
                        ctx as *mut c_void,
                    )
                };
                if port.is_null() {
                    // SAFETY: no tap holds the context; free it once.
                    drop(unsafe { Box::from_raw(ctx) });
                    let _ = ready_tx.send(Err("CGEventTapCreate failed"));
                    return;
                }
                // SAFETY: port is a valid mach port; run loop calls happen on
                // this thread. The run loop retains the source, so our
                // reference is released right after adding it.
                unsafe {
                    let source = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
                    if source.is_null() {
                        CFRelease(port);
                        drop(Box::from_raw(ctx));
                        let _ = ready_tx.send(Err("CFMachPortCreateRunLoopSource failed"));
                        return;
                    }
                    (*ctx).port.store(port, Ordering::Release);
                    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                    CFRelease(source);
                    CGEventTapEnable(port, true);
                }
                let _ = ready_tx.send(Ok(()));
                // SAFETY: runs this thread's run loop; it only returns if the
                // source is removed, then the tap is disabled and released.
                // The context is leaked on purpose: a callback may still be
                // referencing it on another run loop pass.
                unsafe {
                    CFRunLoopRun();
                    (*ctx).port.store(std::ptr::null_mut(), Ordering::Release);
                    CGEventTapEnable(port, false);
                    CFRelease(port);
                }
            })
            .map_err(|_| "tap thread not started")?;
        ready_rx.recv().map_err(|_| "tap thread ended early")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Window at (100, 200) with a 56 x 30 test capsule at (132, 213).
    fn frame() -> Option<Frame> {
        Some(Frame {
            window: (100.0, 200.0),
            capsule: Rect {
                x: 132.0,
                y: 213.0,
                w: 56.0,
                h: 30.0,
            },
        })
    }

    #[test]
    fn press_inside_the_capsule_is_swallowed_and_arms_the_drag() {
        let mut s = DragState::Idle;
        assert_eq!(
            s.handle(MouseKind::Down, (140.0, 220.0), frame()),
            DragAction::Swallow
        );
        assert_eq!(
            s,
            DragState::Pressed {
                start: (140.0, 220.0),
                grab: (40.0, 20.0)
            }
        );
    }

    #[test]
    fn press_outside_the_capsule_passes_through() {
        let mut s = DragState::Idle;
        // Inside the transparent window but outside the capsule.
        assert_eq!(
            s.handle(MouseKind::Down, (105.0, 205.0), frame()),
            DragAction::PassThrough
        );
        assert_eq!(s, DragState::Idle);
        // Drags and releases of other gestures pass through too.
        assert_eq!(
            s.handle(MouseKind::Dragged, (140.0, 220.0), frame()),
            DragAction::PassThrough
        );
        assert_eq!(
            s.handle(MouseKind::Up, (140.0, 220.0), frame()),
            DragAction::PassThrough
        );
    }

    #[test]
    fn nothing_is_grabbed_before_the_window_is_placed() {
        let mut s = DragState::Idle;
        assert_eq!(
            s.handle(MouseKind::Down, (140.0, 220.0), None),
            DragAction::PassThrough
        );
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn plain_click_on_the_capsule_does_nothing() {
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        // Jitter under the threshold is swallowed without moving.
        assert_eq!(
            s.handle(MouseKind::Dragged, (142.0, 221.0), frame()),
            DragAction::Swallow
        );
        assert_eq!(
            s.handle(MouseKind::Up, (142.0, 221.0), frame()),
            DragAction::Swallow
        );
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn moving_past_the_threshold_drags_the_window_with_the_grab_offset() {
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        assert_eq!(
            s.handle(MouseKind::Dragged, (144.0, 220.0), frame()),
            DragAction::MoveTo(104.0, 200.0)
        );
        // The frame moving under the cursor does not change the grab.
        assert_eq!(
            s.handle(MouseKind::Dragged, (400.0, 50.0), None),
            DragAction::MoveTo(360.0, 30.0)
        );
        assert_eq!(
            s.handle(MouseKind::Up, (410.0, 60.0), None),
            DragAction::Drop(370.0, 40.0)
        );
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn threshold_is_about_three_points() {
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        assert_eq!(
            s.handle(MouseKind::Dragged, (142.0, 222.0), frame()),
            DragAction::Swallow
        );
        assert!(matches!(
            s.handle(MouseKind::Dragged, (143.0, 223.0), frame()),
            DragAction::MoveTo(..)
        ));
        const {
            assert!(DRAG_THRESHOLD >= 2.0 && DRAG_THRESHOLD <= 4.0);
        }
    }

    #[test]
    fn reset_drops_a_running_drag_and_forgets_a_press() {
        let mut s = DragState::Idle;
        assert_eq!(s.reset(), None);
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        assert_eq!(s.reset(), None);
        assert_eq!(s, DragState::Idle);
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        s.handle(MouseKind::Dragged, (150.0, 230.0), frame());
        assert_eq!(s.reset(), Some(DragAction::Drop(110.0, 210.0)));
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn a_new_press_during_a_drag_ends_it_where_it_was() {
        // The release was missed (for example the tap was off for a moment).
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        s.handle(MouseKind::Dragged, (150.0, 230.0), frame());
        assert_eq!(
            s.handle(MouseKind::Down, (900.0, 900.0), frame()),
            DragAction::Drop(110.0, 210.0)
        );
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn a_new_press_after_a_missed_release_starts_over() {
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (140.0, 220.0), frame());
        assert_eq!(
            s.handle(MouseKind::Down, (900.0, 900.0), frame()),
            DragAction::PassThrough
        );
        assert_eq!(s, DragState::Idle);
    }

    #[test]
    fn only_pass_through_keeps_the_event() {
        assert!(!DragAction::PassThrough.swallows());
        assert!(DragAction::Swallow.swallows());
        assert!(DragAction::MoveTo(0.0, 0.0).swallows());
        assert!(DragAction::Drop(0.0, 0.0).swallows());
    }

    #[test]
    fn tap_action_maps_left_mouse_events_and_reenables() {
        assert_eq!(tap_action(1), TapAction::Mouse(MouseKind::Down));
        assert_eq!(tap_action(6), TapAction::Mouse(MouseKind::Dragged));
        assert_eq!(tap_action(2), TapAction::Mouse(MouseKind::Up));
        assert_eq!(tap_action(0xFFFF_FFFE), TapAction::ReEnable);
        assert_eq!(tap_action(0xFFFF_FFFF), TapAction::ReEnable);
        // Right click, mouse moved, key down are not ours.
        assert_eq!(tap_action(3), TapAction::Ignore);
        assert_eq!(tap_action(5), TapAction::Ignore);
        assert_eq!(tap_action(10), TapAction::Ignore);
        assert_eq!(event_mask(), (1 << 1) | (1 << 2) | (1 << 6));
    }

    #[test]
    fn only_moves_and_drops_reach_the_worker() {
        assert_eq!(
            msg_for(DragAction::MoveTo(1.0, 2.0)),
            Some(Msg::Move(1.0, 2.0))
        );
        assert_eq!(
            msg_for(DragAction::Drop(3.0, 4.0)),
            Some(Msg::Drop(3.0, 4.0))
        );
        assert_eq!(msg_for(DragAction::Swallow), None);
        assert_eq!(msg_for(DragAction::PassThrough), None);
    }
}

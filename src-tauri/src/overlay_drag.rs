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
//! On release the worker snaps the pill to the nearest of the four anchors
//! with a short eased move (see `overlay::nearest_anchor`, `snap_steps`),
//! turning it upright on the left and right, and saves the anchor.
//!
//! Hover controls (2.3c): the worker also polls the global cursor every
//! `HOVER_POLL_MS` and sends `overlay-hover {on}` when it comes near the
//! small capsule (`overlay::hover_zone`); the frontend then opens the
//! control capsule with the Mic and Open buttons. A press on a button is a
//! click (fired on release inside it, never a drag); a press elsewhere in
//! the control capsule drags as before. The window stays click-through and
//! non-focusable; clicks are run in the worker, never in the tap callback.
//!
//! An active tap needs the Accessibility permission. Without it the tap is
//! not installed and the check repeats every 3 s. Nothing here logs mouse
//! coordinates.
//!
//! Coordinates: logical points, origin at the top-left of the primary
//! display, y down. CGEventGetLocation already reports this space (it is
//! not the flipped, bottom-left Cocoa space), and so do tao's logical
//! window and monitor positions on macOS, across monitors of any scale.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, LogicalPosition, LogicalSize, Manager, Runtime};

use crate::hotkey::HotkeyAction;
use crate::overlay::{self, OVERLAY_WINDOW, OverlayButton, Placement, Position, Rect, Screen};

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
/// Window moves in the snap animation after a drop.
pub const SNAP_STEPS: usize = 8;
/// Length of the snap animation in ms.
pub const SNAP_MS: u64 = 150;
/// Extra wait after the snap when the capsule turns, so the CSS turn
/// (overlay.css `rotate 200ms`) ends before the window shrinks around it.
pub const TURN_SETTLE_MS: u64 = 80;
/// Event sent to the overlay window with `{on: bool}` when the cursor comes
/// near the capsule or leaves it.
pub const HOVER_EVENT: &str = "overlay-hover";
/// How often the worker reads the cursor position.
pub const HOVER_POLL_MS: u64 = 50;
/// How long the controls stay open after the cursor leaves.
pub const HOVER_CLOSE_MS: u64 = 400;
/// Status event of the dictation orchestrator (payload: status text).
const DICTATION_STATUS_EVENT: &str = "dictation-status";

/// Steps, pause between steps (ms) and turn settle time (ms) of the snap
/// animation. With reduced motion the pill jumps straight to the anchor.
pub fn snap_plan(reduced_motion: bool) -> (usize, u64, u64) {
    if reduced_motion {
        (1, 0, 0)
    } else {
        (SNAP_STEPS, SNAP_MS / SNAP_STEPS as u64, TURN_SETTLE_MS)
    }
}

/// Window origins for an eased (ease-out cubic) move from `from` to `to`
/// in `steps` steps (at least one). The last step is exactly `to`.
pub fn snap_steps(from: (f64, f64), to: (f64, f64), steps: usize) -> Vec<(f64, f64)> {
    let n = steps.max(1);
    (1..=n)
        .map(|i| {
            if i == n {
                return to;
            }
            let t = i as f64 / n as f64;
            let e = 1.0 - (1.0 - t).powi(3);
            (from.0 + (to.0 - from.0) * e, from.1 + (to.1 - from.1) * e)
        })
        .collect()
}

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
    /// Swallow the event; a control button was clicked.
    Click(OverlayButton),
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
    /// The visible capsule: the small one, or the control capsule on hover.
    pub capsule: Rect,
    /// The control buttons, only while the controls are open.
    pub buttons: Option<[(OverlayButton, Rect); 2]>,
}

/// What a press landed on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PressTarget {
    /// The capsule outside any button: moving drags the pill.
    Body,
    /// A control button (and its rect): fires on release inside it.
    Button(OverlayButton, Rect),
}

/// Drag gesture state, fed one mouse event at a time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DragState {
    #[default]
    Idle,
    /// Pressed on the capsule; `grab` is the press point minus the window
    /// origin, `start` the press point, `target` what was pressed.
    Pressed {
        start: (f64, f64),
        grab: (f64, f64),
        target: PressTarget,
    },
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
                    let button = f
                        .buttons
                        .into_iter()
                        .flatten()
                        .find(|(_, r)| r.contains(px, py));
                    *self = DragState::Pressed {
                        start: point,
                        grab: (px - f.window.0, py - f.window.1),
                        target: match button {
                            Some((b, r)) => PressTarget::Button(b, r),
                            None => PressTarget::Body,
                        },
                    };
                    DragAction::Swallow
                }
                _ => DragAction::PassThrough,
            },
            (DragState::Idle, _) => DragAction::PassThrough,
            (
                DragState::Pressed {
                    target: PressTarget::Button(..),
                    ..
                },
                MouseKind::Dragged,
            ) => DragAction::Swallow,
            (
                DragState::Pressed {
                    target: PressTarget::Button(b, r),
                    ..
                },
                MouseKind::Up,
            ) => {
                *self = DragState::Idle;
                if r.contains(px, py) {
                    DragAction::Click(b)
                } else {
                    DragAction::Swallow
                }
            }
            (DragState::Pressed { start, grab, .. }, MouseKind::Dragged) => {
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

/// Whether the hover controls are open, fed one cursor poll at a time.
/// Opens as soon as the cursor is inside the hover zone; closes
/// `HOVER_CLOSE_MS` after the last poll that was inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoverTracker {
    open: bool,
    last_inside_ms: u64,
}

impl HoverTracker {
    pub fn is_open(self) -> bool {
        self.open
    }

    /// One poll at `now_ms`; returns the new state when it changed.
    pub fn update(&mut self, inside: bool, now_ms: u64) -> Option<bool> {
        if inside {
            self.last_inside_ms = now_ms;
            if !self.open {
                self.open = true;
                return Some(true);
            }
        } else if self.open && now_ms.saturating_sub(self.last_inside_ms) >= HOVER_CLOSE_MS {
            self.open = false;
            return Some(false);
        }
        None
    }

    /// Closes at once (a drag started). True when it was open.
    pub fn close_now(&mut self) -> bool {
        std::mem::take(&mut self.open)
    }
}

/// The Mic button: alternate clicks start and stop dictation through the
/// same callback the hotkeys use. A click is ignored while a recording that
/// a key hold started is running, and any status other than "REC" (done,
/// error, cancelled) resets the toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MicToggle {
    click_recording: bool,
    listening: bool,
}

impl MicToggle {
    /// The action for one click, if any.
    pub fn click(&mut self) -> Option<HotkeyAction> {
        if self.click_recording {
            self.click_recording = false;
            Some(HotkeyAction::StopRecording)
        } else if self.listening {
            None
        } else {
            self.click_recording = true;
            Some(HotkeyAction::StartRecording)
        }
    }

    /// Follows the `dictation-status` text ("REC" while recording).
    pub fn on_status(&mut self, text: &str) {
        self.listening = text.trim() == "REC";
        if !self.listening {
            self.click_recording = false;
        }
    }
}

/// The dictation start/stop callback from `dictation::spawn`, shared by the
/// hotkeys and the overlay Mic button.
pub type DictationCallback = Arc<Mutex<Box<dyn Fn(HotkeyAction) + Send>>>;

/// Wraps the dictation callback so both the hotkeys and the overlay can
/// call it.
pub fn dictation_callback(f: impl Fn(HotkeyAction) + Send + 'static) -> DictationCallback {
    Arc::new(Mutex::new(Box::new(f)))
}

/// Calls the shared dictation callback.
pub fn call_dictation(cb: &DictationCallback, action: HotkeyAction) {
    if let Ok(f) = cb.lock() {
        f(action);
    }
}

/// Tauri managed state for the overlay buttons.
pub struct OverlayControl {
    dictation: DictationCallback,
    open: Box<dyn Fn() + Send + Sync>,
    mic: Mutex<MicToggle>,
}

impl OverlayControl {
    /// `dictation` is the shared start/stop callback; `open` shows the
    /// DeepAI Voice window (the tray Settings item).
    pub fn new(dictation: DictationCallback, open: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            dictation,
            open: Box::new(open),
            mic: Mutex::new(MicToggle::default()),
        }
    }

    fn run(&self, button: OverlayButton) {
        match button {
            OverlayButton::Open => (self.open)(),
            OverlayButton::Mic => {
                // The toggle lock is released before calling dictation,
                // which may emit a status event that takes it again.
                let action = self.mic.lock().ok().and_then(|mut m| m.click());
                if let Some(action) = action {
                    call_dictation(&self.dictation, action);
                }
            }
        }
    }

    fn on_status(&self, text: &str) {
        if let Ok(mut m) = self.mic.lock() {
            m.on_status(text);
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state between the tap callback and the worker
// ---------------------------------------------------------------------------

struct Shared {
    state: DragState,
    window: Option<(f64, f64)>,
    edge: Position,
    /// The small capsule size the frontend reported (not the hover one).
    capsule: Option<(f64, f64)>,
    hover: HoverTracker,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    state: DragState::Idle,
    window: None,
    edge: Position::Top,
    capsule: None,
    hover: HoverTracker {
        open: false,
        last_inside_ms: 0,
    },
});

/// Set once the mouse tap runs. Without it a click on the controls would
/// reach the app below, so the controls never open.
static TAP_READY: AtomicBool = AtomicBool::new(false);

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
    let open = s.hover.is_open();
    let frame = s.window.map(|w| frame_for(w, s.edge, s.capsule, open));
    s.state.handle(kind, point, frame)
}

/// The hit-test frame: the small capsule, or the control capsule and its
/// buttons while the controls are open.
fn frame_for(window: (f64, f64), edge: Position, base: Option<(f64, f64)>, open: bool) -> Frame {
    if open {
        let capsule = overlay::hover_rect(window, edge);
        Frame {
            window,
            capsule,
            buttons: Some(overlay::button_rects(capsule, edge)),
        }
    } else {
        Frame {
            window,
            capsule: overlay::capsule_rect(window, edge, base),
            buttons: None,
        }
    }
}

/// One cursor poll: updates the hover state and returns it when it changed.
/// Nothing changes while a press or drag is running.
fn poll_hover(cursor: (f64, f64), now_ms: u64) -> Option<bool> {
    let mut s = SHARED.lock().ok()?;
    if s.state != DragState::Idle {
        return None;
    }
    let window = s.window?;
    let open = s.hover.is_open();
    let inside = overlay::hover_zone(window, s.edge, s.capsule, open).contains(cursor.0, cursor.1);
    s.hover.update(inside, now_ms)
}

fn close_hover() -> bool {
    SHARED
        .lock()
        .map(|mut s| s.hover.close_now())
        .unwrap_or(false)
}

fn reset_shared() -> Option<DragAction> {
    SHARED.lock().ok().and_then(|mut s| s.state.reset())
}

// ---------------------------------------------------------------------------
// Worker: moves the window and snaps on drop
// ---------------------------------------------------------------------------

/// Message from the tap to the worker: a window origin or a click.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Msg {
    Move(f64, f64),
    Drop(f64, f64),
    Click(OverlayButton),
}

fn msg_for(action: DragAction) -> Option<Msg> {
    match action {
        DragAction::MoveTo(x, y) => Some(Msg::Move(x, y)),
        DragAction::Drop(x, y) => Some(Msg::Drop(x, y)),
        DragAction::Click(b) => Some(Msg::Click(b)),
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

#[derive(Clone, Serialize)]
struct HoverPayload {
    on: bool,
}

fn emit_hover<R: Runtime>(app: &AppHandle<R>, on: bool) {
    let _ = app.emit_to(OVERLAY_WINDOW, HOVER_EVENT, HoverPayload { on });
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
    let (w, h) = overlay::window_size_for(edge);
    let o = overlay::clamp_to_screen(screen, w, h, origin);
    Some((o, screen))
}

/// Eases the pill from where it was dropped to `anchor` on `screen`. Runs
/// in the worker thread, never in the tap callback. While it moves, the
/// window is the square turn window so the capsule can turn between
/// horizontal and upright without clipping; `apply_placement` then gives
/// the window the anchor's own size, with the capsule staying in place.
fn animate_to_anchor<R: Runtime>(
    app: &AppHandle<R>,
    from: (f64, f64),
    from_edge: Position,
    anchor: Position,
    screen: Screen,
) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW) else {
        return;
    };
    let (_, size) = shared_edge_and_capsule();
    let start_center = overlay::capsule_rect(from, from_edge, size).center();
    let target = overlay::anchor_window_rect(anchor, screen, overlay::window_size_for(anchor));
    let end_center = overlay::capsule_rect((target.x, target.y), anchor, size).center();
    let start = overlay::turn_origin(start_center, anchor, size);
    let end = overlay::turn_origin(end_center, anchor, size);
    let (tw, th) = overlay::TURN_WINDOW;
    let _ = window.set_size(LogicalSize::new(tw, th));
    let _ = window.set_position(LogicalPosition::new(start.0, start.1));
    // The frontend lays the capsule out for the new anchor (and turns it).
    let _ = app.emit_to(OVERLAY_WINDOW, overlay::POSITION_EVENT, anchor.as_str());
    let (steps, pause_ms, settle_ms) = snap_plan(reduced_motion());
    for (x, y) in snap_steps(start, end, steps) {
        if pause_ms > 0 {
            std::thread::sleep(Duration::from_millis(pause_ms));
        }
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    if settle_ms > 0 && from_edge.is_vertical() != anchor.is_vertical() {
        std::thread::sleep(Duration::from_millis(settle_ms));
    }
}

/// The system Reduce motion setting (System Settings > Accessibility >
/// Display). Read once per drop.
#[cfg(target_os = "macos")]
fn reduced_motion() -> bool {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: NSWorkspace.sharedWorkspace and
        // -accessibilityDisplayShouldReduceMotion are plain AppKit getters;
        // the pointer is null-checked before use.
        unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws.is_null() {
                return false;
            }
            msg_send![ws, accessibilityDisplayShouldReduceMotion]
        }
    })
}

#[cfg(not(target_os = "macos"))]
fn reduced_motion() -> bool {
    false
}

/// Runs a control button click (worker thread).
fn run_click<R: Runtime>(app: &AppHandle<R>, button: OverlayButton) {
    match app.try_state::<OverlayControl>() {
        Some(control) => control.run(button),
        None => tracing::warn!("overlay controls not ready"),
    }
}

/// Polls the cursor and tells the overlay when the controls open or close.
fn hover_tick<R: Runtime>(app: &AppHandle<R>, started: Instant) {
    if !TAP_READY.load(Ordering::Acquire) {
        return;
    }
    let Some(cursor) = cursor_location() else {
        return;
    };
    let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if let Some(on) = poll_hover(cursor, now_ms) {
        emit_hover(app, on);
    }
}

#[cfg(target_os = "macos")]
fn cursor_location() -> Option<(f64, f64)> {
    tap::cursor_location()
}

#[cfg(not(target_os = "macos"))]
fn cursor_location() -> Option<(f64, f64)> {
    None
}

fn run_worker<R: Runtime>(app: AppHandle<R>, rx: Receiver<Msg>) {
    let started = Instant::now();
    let mut dragging = false;
    let mut screens: Vec<Screen> = Vec::new();
    let mut primary: Option<Screen> = None;
    let mut pending: Option<Msg> = None;
    loop {
        let mut msg = match pending.take() {
            Some(m) => m,
            None => match rx.recv_timeout(Duration::from_millis(HOVER_POLL_MS)) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) => {
                    if !dragging {
                        hover_tick(&app, started);
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        if let Msg::Click(button) = msg {
            run_click(&app, button);
            continue;
        }
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
            // The small capsule is what moves and snaps.
            if close_hover() {
                emit_hover(&app, false);
            }
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
                // The monitor is the one under the capsule centre, which the
                // grab point (and so the cursor) lies inside.
                if let Some((o, screen)) = clamped((x, y), &screens, primary) {
                    let (edge, size) = shared_edge_and_capsule();
                    let center = overlay::capsule_rect(o, edge, size).center();
                    let anchor = overlay::nearest_anchor(center, screen);
                    animate_to_anchor(&app, o, edge, anchor, screen);
                    overlay::apply_placement(&app, Placement::preset(anchor), Some(screen));
                }
                dragging = false;
                emit_dragging(&app, false);
            }
            Msg::Click(_) => {}
        }
        pending = next;
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
    // The Mic button follows the dictation status (Rust listeners also get
    // events emitted from Rust).
    let status_app = app.clone();
    app.listen_any(DICTATION_STATUS_EVENT, move |event| {
        if let (Ok(text), Some(control)) = (
            serde_json::from_str::<String>(event.payload()),
            status_app.try_state::<OverlayControl>(),
        ) {
            control.on_status(&text);
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
                            Ok(()) => {
                                TAP_READY.store(true, Ordering::Release);
                                tracing::info!("overlay drag tap started");
                            }
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
        fn CGEventCreate(source: *mut c_void) -> *mut c_void;
    }

    /// The global cursor position (same space as the tap's event points).
    pub(super) fn cursor_location() -> Option<(f64, f64)> {
        // SAFETY: CGEventCreate(NULL) returns a new event (or null) holding
        // the current cursor location; it is released once read.
        unsafe {
            let event = CGEventCreate(std::ptr::null_mut());
            if event.is_null() {
                return None;
            }
            let p = CGEventGetLocation(event);
            CFRelease(event);
            Some((p.x, p.y))
        }
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
            buttons: None,
        })
    }

    /// The same window, open on hover: an 84 x 36 control capsule at
    /// (114, 210) with Mic at (124, 214) and Open at (160, 214), 28 x 28.
    fn hover_frame() -> Option<Frame> {
        let capsule = Rect {
            x: 114.0,
            y: 210.0,
            w: 84.0,
            h: 36.0,
        };
        Some(Frame {
            window: (100.0, 200.0),
            capsule,
            buttons: Some(overlay::button_rects(capsule, Position::Top)),
        })
    }

    // ---- 2.3c: buttons in the hover capsule ---------------------------------

    #[test]
    fn a_button_click_fires_on_release_inside_that_button() {
        let mut s = DragState::Idle;
        assert_eq!(
            s.handle(MouseKind::Down, (130.0, 220.0), hover_frame()),
            DragAction::Swallow
        );
        assert!(matches!(
            s,
            DragState::Pressed {
                target: PressTarget::Button(OverlayButton::Mic, _),
                ..
            }
        ));
        assert_eq!(
            s.handle(MouseKind::Up, (131.0, 221.0), hover_frame()),
            DragAction::Click(OverlayButton::Mic)
        );
        assert_eq!(s, DragState::Idle);
        s.handle(MouseKind::Down, (170.0, 230.0), hover_frame());
        assert_eq!(
            s.handle(MouseKind::Up, (170.0, 230.0), hover_frame()),
            DragAction::Click(OverlayButton::Open)
        );
    }

    #[test]
    fn a_button_press_never_drags_and_cancels_when_released_outside() {
        let mut s = DragState::Idle;
        s.handle(MouseKind::Down, (130.0, 220.0), hover_frame());
        // Far past the drag threshold: still no move.
        assert_eq!(
            s.handle(MouseKind::Dragged, (300.0, 400.0), hover_frame()),
            DragAction::Swallow
        );
        assert_eq!(
            s.handle(MouseKind::Up, (300.0, 400.0), hover_frame()),
            DragAction::Swallow
        );
        assert_eq!(s, DragState::Idle);
        // Released on the other button: nothing fires either.
        s.handle(MouseKind::Down, (130.0, 220.0), hover_frame());
        assert_eq!(
            s.handle(MouseKind::Up, (170.0, 230.0), hover_frame()),
            DragAction::Swallow
        );
    }

    #[test]
    fn the_hover_body_outside_the_buttons_drags_and_a_plain_click_does_nothing() {
        let mut s = DragState::Idle;
        // Between the two buttons, inside the control capsule.
        assert_eq!(
            s.handle(MouseKind::Down, (156.0, 228.0), hover_frame()),
            DragAction::Swallow
        );
        assert!(matches!(
            s,
            DragState::Pressed {
                target: PressTarget::Body,
                ..
            }
        ));
        assert_eq!(
            s.handle(MouseKind::Up, (156.0, 228.0), hover_frame()),
            DragAction::Swallow
        );
        // A press on the rim then a move drags the window as before.
        s.handle(MouseKind::Down, (116.0, 212.0), hover_frame());
        assert_eq!(
            s.handle(MouseKind::Dragged, (126.0, 212.0), hover_frame()),
            DragAction::MoveTo(110.0, 200.0)
        );
        assert_eq!(
            s.handle(MouseKind::Up, (126.0, 212.0), None),
            DragAction::Drop(110.0, 200.0)
        );
        // Outside the control capsule: not ours.
        assert_eq!(
            s.handle(MouseKind::Down, (102.0, 202.0), hover_frame()),
            DragAction::PassThrough
        );
    }

    #[test]
    fn clicks_reach_the_worker() {
        assert_eq!(
            msg_for(DragAction::Click(OverlayButton::Open)),
            Some(Msg::Click(OverlayButton::Open))
        );
        assert!(DragAction::Click(OverlayButton::Mic).swallows());
    }

    // ---- 2.3c: hover tracker ------------------------------------------------

    #[test]
    fn hover_opens_at_once_and_closes_400_ms_after_the_cursor_leaves() {
        let mut h = HoverTracker::default();
        assert!(!h.is_open());
        assert_eq!(h.update(false, 0), None);
        assert_eq!(h.update(true, 50), Some(true));
        assert_eq!(h.update(true, 100), None);
        // Left at 150: still open until 400 ms after the last inside poll.
        assert_eq!(h.update(false, 150), None);
        assert_eq!(h.update(false, 450), None);
        assert_eq!(h.update(false, 500), Some(false));
        assert!(!h.is_open());
        assert_eq!(h.update(false, 900), None);
    }

    #[test]
    fn coming_back_before_the_close_keeps_it_open() {
        let mut h = HoverTracker::default();
        h.update(true, 0);
        assert_eq!(h.update(false, 300), None);
        assert_eq!(h.update(true, 350), None);
        assert_eq!(h.update(false, 700), None);
        assert_eq!(h.update(false, 750), Some(false));
        const {
            assert!(HOVER_POLL_MS >= 30 && HOVER_POLL_MS <= 80);
            assert!(HOVER_CLOSE_MS >= 300 && HOVER_CLOSE_MS <= 500);
        }
    }

    #[test]
    fn close_now_reports_only_a_change() {
        let mut h = HoverTracker::default();
        assert!(!h.close_now());
        h.update(true, 0);
        assert!(h.close_now());
        assert!(!h.is_open());
    }

    // ---- 2.3c: mic button ---------------------------------------------------

    #[test]
    fn mic_clicks_alternate_start_and_stop() {
        let mut m = MicToggle::default();
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
        m.on_status("REC");
        assert_eq!(m.click(), Some(HotkeyAction::StopRecording));
        m.on_status("...");
        m.on_status("");
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
    }

    #[test]
    fn mic_click_is_ignored_while_a_key_hold_is_recording() {
        let mut m = MicToggle::default();
        // Fn or the shortcut started this recording, not a click.
        m.on_status("REC");
        assert_eq!(m.click(), None);
        assert_eq!(m.click(), None);
        m.on_status("");
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
    }

    #[test]
    fn a_recording_that_ends_another_way_resets_the_toggle() {
        let mut m = MicToggle::default();
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
        // Mic denied, or Esc cancelled: the next click starts again.
        m.on_status("Mic denied in Settings");
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
        m.on_status("REC");
        m.on_status("");
        assert_eq!(m.click(), Some(HotkeyAction::StartRecording));
    }

    // ---- Review fix: reduced motion skips the eased snap ---------------------

    #[test]
    fn reduced_motion_jumps_straight_to_the_anchor() {
        assert_eq!(snap_plan(true), (1, 0, 0));
        assert_eq!(
            snap_plan(false),
            (SNAP_STEPS, SNAP_MS / SNAP_STEPS as u64, TURN_SETTLE_MS)
        );
        let jump = snap_steps((0.0, 0.0), (50.0, 60.0), snap_plan(true).0);
        assert_eq!(jump, vec![(50.0, 60.0)]);
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
                grab: (40.0, 20.0),
                target: PressTarget::Body,
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
    fn snap_steps_ease_out_and_end_on_the_anchor() {
        let steps = snap_steps((0.0, 100.0), (80.0, 20.0), SNAP_STEPS);
        assert_eq!(steps.len(), SNAP_STEPS);
        assert_eq!(*steps.last().unwrap(), (80.0, 20.0));
        // Monotonic toward the target, fast first then slow (ease out).
        let xs: Vec<f64> = steps.iter().map(|p| p.0).collect();
        assert!(xs.windows(2).all(|w| w[1] >= w[0]), "{xs:?}");
        assert!(xs[0] > 80.0 / SNAP_STEPS as f64, "{xs:?}");
        let first = xs[0];
        let last_gap = xs[SNAP_STEPS - 1] - xs[SNAP_STEPS - 2];
        assert!(first > last_gap, "{xs:?}");
        // y moves the other way just as smoothly.
        assert!(steps.windows(2).all(|w| w[1].1 <= w[0].1));
    }

    #[test]
    fn snap_steps_handle_zero_steps_and_no_distance() {
        assert_eq!(snap_steps((1.0, 2.0), (5.0, 6.0), 0), vec![(5.0, 6.0)]);
        let still = snap_steps((5.0, 6.0), (5.0, 6.0), 4);
        assert!(still.iter().all(|p| *p == (5.0, 6.0)));
        // About 150 ms in total, a few steps.
        const {
            assert!(SNAP_MS >= 100 && SNAP_MS <= 200);
            assert!(SNAP_STEPS >= 4 && SNAP_STEPS <= 12);
        }
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

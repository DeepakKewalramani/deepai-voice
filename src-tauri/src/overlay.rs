//! Notch-style floating status pill. A second, borderless, transparent,
//! click-through window that the frontend renders as a black pill and drives
//! from the `dictation-status` event. The pill sits on exactly one of four
//! anchors: the middle of the top (below the notch, the default), bottom,
//! left or right screen edge. On the left and right anchors the capsule is
//! turned 90 degrees and the window is swapped to the upright size. The
//! tray presets and a drag (see `overlay_drag`, which snaps to the nearest
//! anchor) change it, and the choice is saved to `overlay.json` in the app
//! data dir.
//!
//! Coordinates: every rect and origin here is in logical points with the
//! origin at the top-left of the primary display and y growing down. On
//! macOS that is the same space as CGEventGetLocation and as tao's logical
//! window and monitor positions, so no y flip is needed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::CheckMenuItem;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl,
    WebviewWindowBuilder,
};

/// Label of the overlay window. The frontend renders the pill for this label.
pub const OVERLAY_WINDOW: &str = "overlay";
/// Overlay window size in logical pixels for a horizontal capsule (top and
/// bottom): the largest capsule (the hover controls, 84 x 36) plus room for
/// its soft shadow and the drag lift. Left and right use it swapped
/// (56 x 112).
pub const OVERLAY_WIDTH: f64 = 112.0;
pub const OVERLAY_HEIGHT: f64 = 56.0;
/// Window size before 2.3c. Old free placements in `overlay.json` were laid
/// out with it, so `normalise_placement` keeps using it.
const LEGACY_WINDOW: (f64, f64) = (144.0, 44.0);
/// Square window used only while the snap animation turns the capsule
/// between horizontal and upright, so no angle of the turn is clipped.
pub const TURN_WINDOW: (f64, f64) = (OVERLAY_WIDTH, OVERLAY_WIDTH);
/// Gap between the window and the screen edge for non-top anchors.
pub const EDGE_MARGIN: f64 = 12.0;
/// Top position: the window starts at the notch band's lower edge; the
/// capsule sits 8 px inside it (overlay.css `--ov-inset`), so its top is
/// 40 px below the screen edge and fully visible on notched displays.
pub const TOP_OFFSET: f64 = 32.0;
/// Gap between the capsule and the window edge it is parked against
/// (overlay.css `--ov-inset`).
pub const CAPSULE_INSET: f64 = 8.0;
/// Capsule size used for hit testing until the frontend reports one: the
/// idle capsule (overlay state.ts `capsuleSize`).
pub const DEFAULT_CAPSULE: (f64, f64) = (36.0, 8.0);
/// The control capsule shown while the cursor is over the pill (state.ts
/// `HOVER_SIZE`), unrotated. Upright on the left and right anchors.
pub const HOVER_CAPSULE: (f64, f64) = (84.0, 36.0);
/// Side of each round button in the control capsule (state.ts `BUTTON_SIZE`).
pub const BUTTON_SIZE: f64 = 28.0;
/// Gap between the two buttons (overlay.css `.ov-controls` gap).
pub const BUTTON_GAP: f64 = 8.0;
/// How far around the capsule the cursor still counts as over it.
pub const HOVER_PAD: f64 = 8.0;
/// Event sent to the overlay when its position changes (payload: "top" etc).
pub const POSITION_EVENT: &str = "overlay-position";
/// File in the app data dir that stores the chosen position.
pub const SETTINGS_FILE: &str = "overlay.json";
/// Tray item id that puts the pill back at the top, below the notch.
pub const RESET_MENU_ID: &str = "overlay-reset";

/// The two round buttons of the control capsule, in layout order (left to
/// right, or top to bottom when upright).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayButton {
    /// Start or stop dictation.
    Mic,
    /// Open the DeepAI Voice window.
    Open,
}

/// Screen edge the pill is parked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Position {
    pub const ALL: [Position; 4] = [
        Position::Top,
        Position::Bottom,
        Position::Left,
        Position::Right,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Position::Top => "top",
            Position::Bottom => "bottom",
            Position::Left => "left",
            Position::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Position> {
        Position::ALL
            .into_iter()
            .find(|p| p.as_str() == s.trim().to_ascii_lowercase())
    }

    /// Label for the tray check item.
    pub fn label(self) -> &'static str {
        match self {
            Position::Top => "Top",
            Position::Bottom => "Bottom",
            Position::Left => "Left",
            Position::Right => "Right",
        }
    }

    /// Tray menu item id, e.g. "overlay-pos-top".
    pub fn menu_id(self) -> String {
        format!("overlay-pos-{}", self.as_str())
    }

    pub fn from_menu_id(id: &str) -> Option<Position> {
        id.strip_prefix("overlay-pos-").and_then(Position::parse)
    }

    /// True on the left and right anchors, where the capsule stands upright.
    pub fn is_vertical(self) -> bool {
        matches!(self, Position::Left | Position::Right)
    }
}

/// Window size for the capsule orientation at `pos`: 144 x 44 on top and
/// bottom, swapped to 44 x 144 on left and right.
pub fn window_size_for(pos: Position) -> (f64, f64) {
    if pos.is_vertical() {
        (OVERLAY_HEIGHT, OVERLAY_WIDTH)
    } else {
        (OVERLAY_WIDTH, OVERLAY_HEIGHT)
    }
}

/// Where the pill sits: an edge plus how far along it, 0..1 (0 is the
/// left or top end, 0.5 the middle). Only the four anchors (offset 0.5)
/// are used now; other offsets come from old `overlay.json` files and are
/// turned into an anchor by `normalise_placement`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub edge: Position,
    pub offset: f64,
}

impl Placement {
    /// A placement with the offset clamped to 0..1 (non-finite means middle).
    pub fn new(edge: Position, offset: f64) -> Self {
        Self {
            edge,
            offset: clamp_offset(offset),
        }
    }

    /// The tray preset for `edge`: the middle of that edge.
    pub fn preset(edge: Position) -> Self {
        Self { edge, offset: 0.5 }
    }
}

impl Default for Placement {
    fn default() -> Self {
        Placement::preset(Position::Top)
    }
}

/// Clamps an edge offset to 0..1; NaN or infinity becomes 0.5.
pub fn clamp_offset(offset: f64) -> f64 {
    if offset.is_finite() {
        offset.clamp(0.0, 1.0)
    } else {
        0.5
    }
}

/// A monitor in physical pixels, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

impl Screen {
    /// The monitor bounds in logical points as (x, y, width, height).
    pub fn logical(self) -> (f64, f64, f64, f64) {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        (
            self.x / scale,
            self.y / scale,
            self.width / scale,
            self.height / scale,
        )
    }
}

/// An axis-aligned rect in logical points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// True when the point is inside the rect (edges included).
    pub fn contains(self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    pub fn center(self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// The rect grown by `pad` on every side.
    pub fn padded(self, pad: f64) -> Rect {
        Rect {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + 2.0 * pad,
            h: self.h + 2.0 * pad,
        }
    }
}

/// Logical top-left of a `win_w` x `win_h` window placed at `pos` on `screen`.
/// Top is centred, `TOP_OFFSET` below the top edge (clear of the notch);
/// the others are centred on their edge with `EDGE_MARGIN` of gap.
pub fn overlay_origin(screen: Screen, win_w: f64, win_h: f64, pos: Position) -> (f64, f64) {
    placement_origin(screen, win_w, win_h, Placement::preset(pos))
}

/// The window rect of anchor `pos` on `screen` for a window of `size`
/// (normally `window_size_for(pos)`): top centred `TOP_OFFSET` below the
/// top edge (clear of the notch), the others centred on their edge with
/// `EDGE_MARGIN` of gap. Logical points; the scale comes from `screen`.
pub fn anchor_window_rect(pos: Position, screen: Screen, size: (f64, f64)) -> Rect {
    let (x, y) = overlay_origin(screen, size.0, size.1, pos);
    Rect {
        x,
        y,
        w: size.0,
        h: size.1,
    }
}

/// The anchor a dropped capsule snaps to, from its centre `center`.
///
/// The drop is compared with the four screen edges as a fraction of the
/// screen size: the side edges win when the centre is relatively nearer a
/// side than the top or bottom (so at mid height the left half snaps left
/// and the right half snaps right on any aspect ratio). Ties go to the top
/// or bottom edge, and top beats bottom. A non-finite point goes to Top.
pub fn nearest_anchor(center: (f64, f64), screen: Screen) -> Position {
    let (sx, sy, sw, sh) = screen.logical();
    if !(center.0.is_finite() && center.1.is_finite() && sw > 0.0 && sh > 0.0) {
        return Position::Top;
    }
    let left = (center.0 - sx).max(0.0);
    let right = (sx + sw - center.0).max(0.0);
    let top = (center.1 - sy).max(0.0);
    let bottom = (sy + sh - center.1).max(0.0);
    let dx = left.min(right) / sw;
    let dy = top.min(bottom) / sh;
    if dx < dy {
        if left <= right {
            Position::Left
        } else {
            Position::Right
        }
    } else if top <= bottom {
        Position::Top
    } else {
        Position::Bottom
    }
}

/// The anchor for a saved placement. An anchor (offset 0.5) is kept; an old
/// free placement is placed as it used to be (horizontal window, offset
/// along the edge) and moved to the anchor nearest to its centre.
pub fn normalise_placement(placement: Placement, screen: Screen) -> Position {
    if (clamp_offset(placement.offset) - 0.5).abs() < 1e-9 {
        return placement.edge;
    }
    let (w, h) = LEGACY_WINDOW;
    let (x, y) = placement_origin(screen, w, h, placement);
    nearest_anchor((x + w / 2.0, y + h / 2.0), screen)
}

/// Logical top-left of the window for `placement` on `screen` (legacy free
/// placement along an edge; anchors use `anchor_window_rect`).
pub fn placement_origin(
    screen: Screen,
    win_w: f64,
    win_h: f64,
    placement: Placement,
) -> (f64, f64) {
    let (sx, sy, sw, sh) = screen.logical();
    let along = |min: f64, max: f64| {
        if max >= min {
            min + clamp_offset(placement.offset) * (max - min)
        } else {
            // Screen smaller than the window: centre it.
            (min + max) / 2.0
        }
    };
    // Top and bottom run between the side margins; left and right run from
    // below the notch band to the same gap above the bottom.
    let x = along(sx + EDGE_MARGIN, sx + sw - win_w - EDGE_MARGIN);
    let y = along(sy + TOP_OFFSET, sy + sh - win_h - TOP_OFFSET);
    match placement.edge {
        Position::Top => (x, sy + TOP_OFFSET),
        Position::Bottom => (x, sy + sh - win_h - EDGE_MARGIN),
        Position::Left => (sx + EDGE_MARGIN, y),
        Position::Right => (sx + sw - win_w - EDGE_MARGIN, y),
    }
}

/// The visible capsule inside the overlay window at logical `window` origin,
/// laid out as overlay.css does for `edge` in its normal window
/// (`window_size_for(edge)`). `capsule` is the unrotated (width, height)
/// the frontend reported; `None` uses the idle size.
pub fn capsule_rect(window: (f64, f64), edge: Position, capsule: Option<(f64, f64)>) -> Rect {
    capsule_rect_in(window, window_size_for(edge), edge, capsule)
}

/// `capsule_rect` for a window of any `size` (the square turn window
/// included). Top and bottom: centred across, `CAPSULE_INSET` from the top
/// or bottom. Left and right: turned upright (width and height swapped)
/// and centred in the window.
pub fn capsule_rect_in(
    window: (f64, f64),
    size: (f64, f64),
    edge: Position,
    capsule: Option<(f64, f64)>,
) -> Rect {
    let (w, h) = match capsule {
        Some((w, h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => {
            // At most the window less the inset on both sides, so the
            // capsule never reaches past the window edge.
            (
                w.min(OVERLAY_WIDTH - 2.0 * CAPSULE_INSET),
                h.min(OVERLAY_HEIGHT - 2.0 * CAPSULE_INSET),
            )
        }
        _ => DEFAULT_CAPSULE,
    };
    let (ox, oy) = window;
    let (ww, wh) = size;
    let (w, h) = if edge.is_vertical() { (h, w) } else { (w, h) };
    let mid_x = ox + (ww - w) / 2.0;
    let mid_y = oy + (wh - h) / 2.0;
    let (x, y) = match edge {
        Position::Top => (mid_x, oy + CAPSULE_INSET),
        Position::Bottom => (mid_x, oy + wh - CAPSULE_INSET - h),
        Position::Left | Position::Right => (mid_x, mid_y),
    };
    Rect { x, y, w, h }
}

/// The control capsule shown on hover, for the window at `window` on `edge`
/// (upright on the left and right).
pub fn hover_rect(window: (f64, f64), edge: Position) -> Rect {
    capsule_rect(window, edge, Some(HOVER_CAPSULE))
}

/// The two button rects inside the control capsule `capsule`: a row on
/// top and bottom, a column on the left and right. Mic comes first (left,
/// or top when upright), centred with `BUTTON_GAP` between them, as
/// overlay.css lays them out.
pub fn button_rects(capsule: Rect, edge: Position) -> [(OverlayButton, Rect); 2] {
    let (cx, cy) = capsule.center();
    let half = BUTTON_SIZE / 2.0;
    let step = (BUTTON_SIZE + BUTTON_GAP) / 2.0;
    let at = |d: f64| {
        let (x, y) = if edge.is_vertical() {
            (cx - half, cy + d - half)
        } else {
            (cx + d - half, cy - half)
        };
        Rect {
            x,
            y,
            w: BUTTON_SIZE,
            h: BUTTON_SIZE,
        }
    };
    [
        (OverlayButton::Mic, at(-step)),
        (OverlayButton::Open, at(step)),
    ]
}

/// Where the cursor counts as over the pill: the visible capsule plus
/// `HOVER_PAD`. While closed that is the small capsule (`base`, the size the
/// frontend reported, idle when `None`); while open it is the control
/// capsule, so the cursor can move onto the buttons without it closing.
pub fn hover_zone(
    window: (f64, f64),
    edge: Position,
    base: Option<(f64, f64)>,
    open: bool,
) -> Rect {
    let r = if open {
        hover_rect(window, edge)
    } else {
        capsule_rect(window, edge, base)
    };
    r.padded(HOVER_PAD)
}

/// Origin of the square turn window that shows a capsule laid out for
/// `edge` with its centre at `center`. Used by the snap animation: the
/// capsule keeps its place when the window switches to the turn square,
/// and again when it shrinks to the anchor's own size.
pub fn turn_origin(center: (f64, f64), edge: Position, capsule: Option<(f64, f64)>) -> (f64, f64) {
    let (cx, cy) = capsule_rect_in((0.0, 0.0), TURN_WINDOW, edge, capsule).center();
    (center.0 - cx, center.1 - cy)
}

/// Placement for a pill dropped with its capsule centre at `center`: the
/// nearest of the four anchors (always the middle of its edge).
pub fn snap_placement(screen: Screen, center: (f64, f64)) -> Placement {
    Placement::preset(nearest_anchor(center, screen))
}

/// The screen whose logical bounds contain the point, if any.
pub fn screen_at(screens: &[Screen], x: f64, y: f64) -> Option<Screen> {
    screens.iter().copied().find(|s| {
        let (sx, sy, sw, sh) = s.logical();
        x >= sx && x < sx + sw && y >= sy && y < sy + sh
    })
}

/// Clamps a window origin so the whole window stays on `screen`.
pub fn clamp_to_screen(screen: Screen, win_w: f64, win_h: f64, origin: (f64, f64)) -> (f64, f64) {
    let (sx, sy, sw, sh) = screen.logical();
    let x = origin.0.min(sx + sw - win_w).max(sx);
    let y = origin.1.min(sy + sh - win_h).max(sy);
    (x, y)
}

/// Parses the frontend's capsule size payload `{"width":w,"height":h}`.
/// Values outside the window are rejected.
pub fn parse_capsule_size(json: &str) -> Option<(f64, f64)> {
    #[derive(Deserialize)]
    struct Size {
        width: f64,
        height: f64,
    }
    let s: Size = serde_json::from_str(json).ok()?;
    let ok = |v: f64, max: f64| v.is_finite() && v > 0.0 && v <= max;
    (ok(s.width, OVERLAY_WIDTH) && ok(s.height, OVERLAY_HEIGHT)).then_some((s.width, s.height))
}

/// File format. New: `{"edge":"left","offset":0.3}`. Old (four presets):
/// `{"position":"left"}`, read as the middle of that edge.
#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edge: Option<Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position: Option<Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset: Option<f64>,
}

/// Reads the saved placement. Missing or invalid file means top, middle.
/// Accepts the old `{"position":"left"}` format (offset 0.5).
pub fn load_placement(path: &Path) -> Placement {
    let Some(stored) = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Stored>(&s).ok())
    else {
        return Placement::default();
    };
    match stored.edge.or(stored.position) {
        Some(edge) => Placement::new(edge, stored.offset.unwrap_or(0.5)),
        None => Placement::default(),
    }
}

/// Writes the placement, creating the parent dir if needed. The JSON goes
/// to a temp file next to it that is then renamed over the old file, so a
/// crash mid-write never leaves a half-written `overlay.json`.
pub fn save_placement(path: &Path, placement: Placement) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let stored = Stored {
        edge: Some(placement.edge),
        position: None,
        offset: Some(clamp_offset(placement.offset)),
    };
    let json = serde_json::to_string(&stored).map_err(std::io::Error::other)?;
    let tmp = temp_path(path);
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The temp file `save_placement` writes first: `overlay.json.tmp`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Reads the saved edge. Missing or invalid file means Top.
pub fn load_position(path: &Path) -> Position {
    load_placement(path).edge
}

/// Writes the tray preset for `position` (middle of that edge).
pub fn save_position(path: &Path, position: Position) -> std::io::Result<()> {
    save_placement(path, Placement::preset(position))
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join(SETTINGS_FILE))
}

/// Current position plus the tray check items that mirror it.
pub struct OverlayState<R: Runtime> {
    position: Mutex<Position>,
    items: Vec<(Position, CheckMenuItem<R>)>,
}

impl<R: Runtime> OverlayState<R> {
    pub fn new(position: Position, items: Vec<(Position, CheckMenuItem<R>)>) -> Self {
        Self {
            position: Mutex::new(position),
            items,
        }
    }
}

/// Saved edge for this app (Top when none is saved).
pub fn saved_position<R: Runtime>(app: &AppHandle<R>) -> Position {
    saved_placement(app).edge
}

/// Saved placement for this app (top, middle when none is saved).
pub fn saved_placement<R: Runtime>(app: &AppHandle<R>) -> Placement {
    settings_path(app)
        .map(|p| load_placement(&p))
        .unwrap_or_default()
}

/// Converts a tao monitor to a `Screen`.
pub fn screen_of(monitor: &tauri::Monitor) -> Screen {
    Screen {
        x: monitor.position().x as f64,
        y: monitor.position().y as f64,
        width: monitor.size().width as f64,
        height: monitor.size().height as f64,
        scale: monitor.scale_factor(),
    }
}

/// Moves the overlay window to the anchor of `placement` on `screen` (the
/// primary monitor when `None`), sized for its orientation.
pub fn move_overlay_to<R: Runtime>(
    app: &AppHandle<R>,
    placement: Placement,
    screen: Option<Screen>,
) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW) else {
        return;
    };
    let screen = match screen {
        Some(s) => s,
        None => match window.primary_monitor() {
            Ok(Some(monitor)) => screen_of(&monitor),
            _ => return,
        },
    };
    let size = window_size_for(placement.edge);
    let r = anchor_window_rect(placement.edge, screen, size);
    let _ = window.set_size(LogicalSize::new(size.0, size.1));
    let _ = window.set_position(LogicalPosition::new(r.x, r.y));
    crate::overlay_drag::set_window_frame((r.x, r.y), placement.edge);
    let _ = app.emit_to(OVERLAY_WINDOW, POSITION_EVENT, placement.edge.as_str());
}

/// Moves the overlay window to the middle of `pos` on the primary monitor.
pub fn move_overlay<R: Runtime>(app: &AppHandle<R>, pos: Position) {
    move_overlay_to(app, Placement::preset(pos), None);
}

/// Moves the overlay, saves the placement and updates the tray check marks.
/// `screen` is the monitor to place it on (primary when `None`); only the
/// anchor is saved, so a restart uses the primary monitor.
pub fn apply_placement<R: Runtime>(
    app: &AppHandle<R>,
    placement: Placement,
    screen: Option<Screen>,
) {
    let placement = Placement::preset(placement.edge);
    move_overlay_to(app, placement, screen);
    if let Some(path) = settings_path(app)
        && let Err(e) = save_placement(&path, placement)
    {
        tracing::warn!("could not save overlay position: {e}");
    }
    if let Some(state) = app.try_state::<OverlayState<R>>() {
        if let Ok(mut current) = state.position.lock() {
            *current = placement.edge;
        }
        for (p, item) in &state.items {
            let _ = item.set_checked(*p == placement.edge);
        }
    }
}

/// Tray preset: moves the pill to the middle of `pos` and remembers it.
pub fn apply_position<R: Runtime>(app: &AppHandle<R>, pos: Position) {
    apply_placement(app, Placement::preset(pos), None);
}

/// Moves the pill to "top", "bottom", "left" or "right" and remembers it.
#[tauri::command]
pub fn set_overlay_position<R: Runtime>(app: AppHandle<R>, position: String) -> Result<(), String> {
    let pos = Position::parse(&position).ok_or_else(|| format!("unknown position: {position}"))?;
    apply_position(&app, pos);
    Ok(())
}

/// Current pill position, so the overlay can orient itself on load.
#[tauri::command]
pub fn get_overlay_position<R: Runtime>(app: AppHandle<R>) -> String {
    app.try_state::<OverlayState<R>>()
        .and_then(|s| s.position.lock().ok().map(|p| p.as_str().to_string()))
        .unwrap_or_else(|| saved_position(&app).as_str().to_string())
}

/// Creates the overlay window and parks it on anchor `pos`. An old saved
/// free placement on that edge is moved to its nearest anchor (on the
/// primary monitor) and saved in the new form. Call from `setup` (main
/// thread).
pub fn create<R: Runtime>(app: &AppHandle<R>, pos: Position) -> tauri::Result<()> {
    let (w, h) = window_size_for(pos);
    let window = WebviewWindowBuilder::new(app, OVERLAY_WINDOW, WebviewUrl::default())
        .title("DeepAI Voice Status")
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    #[cfg(target_os = "macos")]
    raise_above_menu_bar(&window);
    let saved = saved_placement(app);
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| screen_of(&m));
    let anchor = match primary {
        Some(screen) if saved.edge == pos => normalise_placement(saved, screen),
        _ => pos,
    };
    if anchor != pos || saved != Placement::preset(pos) {
        // Old free placement: move it to its anchor and save that.
        apply_placement(app, Placement::preset(anchor), None);
    } else {
        move_overlay_to(app, Placement::preset(anchor), None);
    }
    window.show()?;
    Ok(())
}

/// Puts the window above the menu bar so the top pill can sit in the notch
/// area. NSStatusWindowLevel is 25; the menu bar is 24.
#[cfg(target_os = "macos")]
fn raise_above_menu_bar<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    const NS_STATUS_WINDOW_LEVEL: isize = 25;
    if let Ok(ptr) = window.ns_window() {
        let ns_window = ptr.cast::<AnyObject>();
        if !ns_window.is_null() {
            // SAFETY: `ns_window` is the live NSWindow of this webview window and
            // `create` runs on the main thread, as AppKit requires.
            unsafe {
                let _: () = msg_send![ns_window, setLevel: NS_STATUS_WINDOW_LEVEL];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETINA: Screen = Screen {
        x: 0.0,
        y: 0.0,
        width: 3024.0,
        height: 1964.0,
        scale: 2.0,
    };

    #[test]
    fn top_is_centred_below_the_notch_band() {
        // 3024 / 2 = 1512 logical, (1512 - 112) / 2 = 700 (2.3c window).
        assert_eq!(
            overlay_origin(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, Position::Top),
            (700.0, 32.0)
        );
    }

    #[test]
    fn bottom_left_right_keep_a_margin() {
        // 1964 / 2 = 982 logical high.
        let (w, h) = (OVERLAY_WIDTH, OVERLAY_HEIGHT);
        assert_eq!(
            overlay_origin(RETINA, w, h, Position::Bottom),
            (700.0, 982.0 - 56.0 - 12.0)
        );
        let mid_y = (982.0 - 56.0) / 2.0;
        assert_eq!(overlay_origin(RETINA, w, h, Position::Left), (12.0, mid_y));
        assert_eq!(
            overlay_origin(RETINA, w, h, Position::Right),
            (1512.0 - 112.0 - 12.0, mid_y)
        );
    }

    #[test]
    fn origin_respects_monitor_offset_and_bad_scale() {
        let screen = Screen {
            x: 1920.0,
            y: -100.0,
            width: 1920.0,
            height: 1080.0,
            scale: 0.0,
        };
        assert_eq!(
            overlay_origin(screen, OVERLAY_WIDTH, OVERLAY_HEIGHT, Position::Top),
            (1920.0 + 904.0, -100.0 + 32.0)
        );
    }

    #[test]
    fn position_parses_and_round_trips_menu_ids() {
        for p in Position::ALL {
            assert_eq!(Position::parse(p.as_str()), Some(p));
            assert_eq!(Position::from_menu_id(&p.menu_id()), Some(p));
        }
        assert_eq!(Position::parse(" Left "), Some(Position::Left));
        assert_eq!(Position::parse("middle"), None);
        assert_eq!(Position::from_menu_id("settings"), None);
        assert_eq!(Position::from_menu_id(RESET_MENU_ID), None);
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deepai-overlay-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(SETTINGS_FILE)
    }

    #[test]
    fn save_then_load_returns_the_saved_position() {
        let path = temp_file("roundtrip");
        save_position(&path, Position::Right).unwrap();
        assert_eq!(load_position(&path), Position::Right);
        save_position(&path, Position::Bottom).unwrap();
        assert_eq!(load_position(&path), Position::Bottom);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_or_invalid_file_falls_back_to_top() {
        let path = temp_file("invalid");
        assert_eq!(load_position(&path), Position::Top);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_position(&path), Position::Top);
        std::fs::write(&path, r#"{"position":"diagonal"}"#).unwrap();
        assert_eq!(load_position(&path), Position::Top);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // ---- Placement: edge plus offset ----------------------------------------

    #[test]
    fn placement_offset_is_clamped() {
        assert_eq!(Placement::new(Position::Left, -0.4).offset, 0.0);
        assert_eq!(Placement::new(Position::Left, 1.7).offset, 1.0);
        assert_eq!(Placement::new(Position::Left, 0.25).offset, 0.25);
        assert_eq!(Placement::new(Position::Left, f64::NAN).offset, 0.5);
        assert_eq!(Placement::new(Position::Left, f64::INFINITY).offset, 0.5);
        assert_eq!(Placement::default(), Placement::preset(Position::Top));
        assert_eq!(Placement::preset(Position::Right).offset, 0.5);
    }

    #[test]
    fn placement_origin_spans_each_edge() {
        let (w, h) = (OVERLAY_WIDTH, OVERLAY_HEIGHT);
        // Top and bottom: x runs from the margin to the far margin.
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Top, 0.0)),
            (12.0, 32.0)
        );
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Bottom, 1.0)),
            (1512.0 - 112.0 - 12.0, 982.0 - 56.0 - 12.0)
        );
        // Left and right: y runs from below the notch band to the same gap
        // above the bottom.
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Left, 0.0)),
            (12.0, 32.0)
        );
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Right, 1.0)),
            (1512.0 - 112.0 - 12.0, 982.0 - 56.0 - 32.0)
        );
    }

    #[test]
    fn placement_origin_centres_when_the_screen_is_too_small() {
        let tiny = Screen {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 60.0,
            scale: 1.0,
        };
        let (x, _) = placement_origin(tiny, 120.0, 56.0, Placement::new(Position::Top, 0.0));
        assert_eq!(x, (100.0 - 120.0) / 2.0);
        let (_, y) = placement_origin(tiny, 120.0, 56.0, Placement::new(Position::Left, 1.0));
        assert_eq!(y, (60.0 - 56.0) / 2.0);
    }

    #[test]
    fn new_format_round_trips_edge_and_offset() {
        let path = temp_file("placement");
        save_placement(&path, Placement::new(Position::Left, 0.25)).unwrap();
        assert_eq!(load_placement(&path), Placement::new(Position::Left, 0.25));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#""edge":"left""#), "{text}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn old_format_loads_as_the_middle_of_its_edge() {
        let path = temp_file("oldformat");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"position":"bottom"}"#).unwrap();
        assert_eq!(load_placement(&path), Placement::preset(Position::Bottom));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn stored_offset_is_clamped_and_bad_files_fall_back_to_top() {
        let path = temp_file("clamp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"edge":"right","offset":3.5}"#).unwrap();
        assert_eq!(load_placement(&path), Placement::new(Position::Right, 1.0));
        std::fs::write(&path, r#"{"edge":"right"}"#).unwrap();
        assert_eq!(load_placement(&path), Placement::preset(Position::Right));
        std::fs::write(&path, r#"{"edge":"corner","offset":0.2}"#).unwrap();
        assert_eq!(load_placement(&path), Placement::default());
        std::fs::write(&path, r#"{"offset":0.2}"#).unwrap();
        assert_eq!(load_placement(&path), Placement::default());
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(load_placement(&path), Placement::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // ---- Capsule rect inside the window --------------------------------------

    #[test]
    fn capsule_rect_follows_the_css_layout() {
        let o = (100.0, 200.0);
        // 2.3c: the compact listening capsule (72 x 20) in the 112 x 56
        // window.
        let size = Some((72.0, 20.0));
        // Top and bottom: 112 x 56 window, centred across, 8 px inset.
        assert_eq!(
            capsule_rect(o, Position::Top, size),
            Rect {
                x: 100.0 + 20.0,
                y: 208.0,
                w: 72.0,
                h: 20.0
            }
        );
        assert_eq!(
            capsule_rect(o, Position::Bottom, size),
            Rect {
                x: 120.0,
                y: 200.0 + 56.0 - 8.0 - 20.0,
                w: 72.0,
                h: 20.0
            }
        );
        // Left and right (changed by the four-anchor work): the window is
        // 56 x 112 and the capsule is turned upright, so width and height
        // swap and it is centred in the window.
        let upright = Rect {
            x: 100.0 + (56.0 - 20.0) / 2.0,
            y: 200.0 + (112.0 - 72.0) / 2.0,
            w: 20.0,
            h: 72.0,
        };
        assert_eq!(capsule_rect(o, Position::Left, size), upright);
        assert_eq!(capsule_rect(o, Position::Right, size), upright);
    }

    #[test]
    fn upright_hit_rect_uses_the_idle_size_and_stays_in_the_window() {
        let r = capsule_rect((0.0, 0.0), Position::Left, None);
        assert_eq!((r.w, r.h), (DEFAULT_CAPSULE.1, DEFAULT_CAPSULE.0));
        for p in Position::ALL {
            let (ww, wh) = window_size_for(p);
            for size in [
                None,
                Some((72.0, 20.0)),
                Some(HOVER_CAPSULE),
                Some((9000.0, 9000.0)),
            ] {
                let r = capsule_rect((0.0, 0.0), p, size);
                assert!(r.x >= 0.0 && r.y >= 0.0, "{p:?} {r:?}");
                assert!(r.x + r.w <= ww && r.y + r.h <= wh, "{p:?} {r:?}");
            }
        }
    }

    #[test]
    fn capsule_rect_defaults_to_idle_and_rejects_bad_sizes() {
        let idle = capsule_rect((0.0, 0.0), Position::Top, None);
        assert_eq!((idle.w, idle.h), DEFAULT_CAPSULE);
        let bad = capsule_rect((0.0, 0.0), Position::Top, Some((f64::NAN, -3.0)));
        assert_eq!((bad.w, bad.h), DEFAULT_CAPSULE);
        let huge = capsule_rect((0.0, 0.0), Position::Top, Some((9000.0, 9000.0)));
        assert!(huge.w <= OVERLAY_WIDTH && huge.h <= OVERLAY_HEIGHT);
    }

    // ---- Four anchors --------------------------------------------------------

    #[test]
    fn window_size_swaps_on_the_left_and_right() {
        assert_eq!(window_size_for(Position::Top), (112.0, 56.0));
        assert_eq!(window_size_for(Position::Bottom), (112.0, 56.0));
        assert_eq!(window_size_for(Position::Left), (56.0, 112.0));
        assert_eq!(window_size_for(Position::Right), (56.0, 112.0));
        assert!(Position::Left.is_vertical() && Position::Right.is_vertical());
        assert!(!Position::Top.is_vertical() && !Position::Bottom.is_vertical());
    }

    #[test]
    fn anchor_rects_sit_centred_on_each_edge() {
        let a = |p| anchor_window_rect(p, RETINA, window_size_for(p));
        assert_eq!(
            a(Position::Top),
            Rect {
                x: 700.0,
                y: 32.0,
                w: 112.0,
                h: 56.0
            }
        );
        assert_eq!(
            a(Position::Bottom),
            Rect {
                x: 700.0,
                y: 982.0 - 56.0 - 12.0,
                w: 112.0,
                h: 56.0
            }
        );
        assert_eq!(
            a(Position::Left),
            Rect {
                x: 12.0,
                y: (982.0 - 112.0) / 2.0,
                w: 56.0,
                h: 112.0
            }
        );
        assert_eq!(
            a(Position::Right),
            Rect {
                x: 1512.0 - 56.0 - 12.0,
                y: (982.0 - 112.0) / 2.0,
                w: 56.0,
                h: 112.0
            }
        );
        // The top capsule clears the notch band: its top is 40 px down.
        let top = a(Position::Top);
        assert_eq!(capsule_rect((top.x, top.y), Position::Top, None).y, 40.0);
    }

    #[test]
    fn nearest_anchor_picks_the_nearest_edge_as_a_fraction_of_the_screen() {
        // Logical screen 1512 x 982.
        assert_eq!(nearest_anchor((756.0, 54.0), RETINA), Position::Top);
        assert_eq!(nearest_anchor((500.0, 150.0), RETINA), Position::Top);
        assert_eq!(nearest_anchor((900.0, 800.0), RETINA), Position::Bottom);
        assert_eq!(nearest_anchor((200.0, 500.0), RETINA), Position::Left);
        assert_eq!(nearest_anchor((1300.0, 420.0), RETINA), Position::Right);
        // Top-left corner: 40 px is a smaller share of the width than of the
        // height, so the left edge wins.
        assert_eq!(nearest_anchor((40.0, 40.0), RETINA), Position::Left);
        // Mid height: the left half snaps left and the right half snaps
        // right (the old centre-distance rule sent these to Top).
        assert_eq!(nearest_anchor((700.0, 491.0), RETINA), Position::Left);
        assert_eq!(nearest_anchor((820.0, 491.0), RETINA), Position::Right);
        // Dead centre is a tie between the sides and top: top wins.
        assert_eq!(nearest_anchor((756.0, 491.0), RETINA), Position::Top);
        // Not a number: the default anchor.
        assert_eq!(nearest_anchor((f64::NAN, 3.0), RETINA), Position::Top);
    }

    #[test]
    fn old_free_placements_are_normalised_to_the_nearest_anchor() {
        // Anchors stay as they are.
        for p in Position::ALL {
            assert_eq!(normalise_placement(Placement::preset(p), RETINA), p);
        }
        // Old top, a quarter across: still nearest the top anchor.
        let q = Placement::new(Position::Top, 0.25);
        assert_eq!(normalise_placement(q, RETINA), Position::Top);
        // Old left, at the very top: a corner, nearer the top edge as a share
        // of the height than the left edge as a share of the width.
        let lt = Placement::new(Position::Left, 0.0);
        assert_eq!(normalise_placement(lt, RETINA), Position::Top);
        // Old left, a bit above the middle: the left anchor.
        let l = Placement::new(Position::Left, 0.4);
        assert_eq!(normalise_placement(l, RETINA), Position::Left);
        // Old bottom far right: a corner, nearer the bottom edge as a share
        // of the screen height than the right edge as a share of the width.
        let br = Placement::new(Position::Bottom, 1.0);
        assert_eq!(normalise_placement(br, RETINA), Position::Bottom);
        // Old right, halfway down: the right anchor.
        let r = Placement::new(Position::Right, 0.4);
        assert_eq!(normalise_placement(r, RETINA), Position::Right);
    }

    #[test]
    fn turn_window_keeps_the_capsule_in_place() {
        // 2.3c sizes: the compact listening capsule and the hover controls.
        for size in [Some((72.0, 20.0)), Some(HOVER_CAPSULE), None] {
            for p in Position::ALL {
                let anchor = anchor_window_rect(p, RETINA, window_size_for(p));
                let fin = capsule_rect((anchor.x, anchor.y), p, size);
                let o = turn_origin(fin.center(), p, size);
                // Same capsule rect in the square window as in the final
                // one, so shrinking the window at the end does not move it.
                assert_eq!(capsule_rect_in(o, TURN_WINDOW, p, size), fin, "{p:?}");
            }
        }
        // The largest capsule fits the square at every angle of the turn.
        let diag = (HOVER_CAPSULE.0.powi(2) + HOVER_CAPSULE.1.powi(2)).sqrt();
        assert!(diag <= TURN_WINDOW.0, "{diag}");
    }

    #[test]
    fn rect_contains_and_center() {
        let r = Rect {
            x: 10.0,
            y: 20.0,
            w: 64.0,
            h: 34.0,
        };
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(74.0, 54.0));
        assert!(!r.contains(9.9, 30.0));
        assert!(!r.contains(40.0, 54.1));
        assert_eq!(r.center(), (42.0, 37.0));
    }

    // ---- Snap on drop --------------------------------------------------------
    // Changed by the four-anchor work: a drop now snaps to the middle of an
    // edge (offset 0.5), never to a free offset along it.

    #[test]
    fn snap_picks_the_top_anchor() {
        let p = snap_placement(RETINA, (414.0, 60.0));
        assert_eq!(p, Placement::preset(Position::Top));
    }

    #[test]
    fn snap_picks_the_bottom_anchor() {
        let p = snap_placement(RETINA, (756.0, 960.0));
        assert_eq!(p, Placement::preset(Position::Bottom));
    }

    #[test]
    fn snap_picks_the_left_anchor_with_no_offset() {
        let p = snap_placement(RETINA, (40.0, 700.0));
        assert_eq!(p, Placement::preset(Position::Left));
    }

    #[test]
    fn snap_picks_the_right_anchor_and_never_a_free_offset() {
        let p = snap_placement(RETINA, (1500.0, 500.0));
        assert_eq!(p, Placement::preset(Position::Right));
        let high = snap_placement(RETINA, (1505.0, 40.0));
        assert_eq!(high, Placement::preset(Position::Right));
        // Dropped past the screen corner still lands on an anchor.
        let off = snap_placement(RETINA, (5000.0, 2000.0));
        assert_eq!(off.offset, 0.5);
    }

    #[test]
    fn snap_works_on_a_second_monitor_with_another_scale() {
        // 1920x1080 points to the right of the Retina panel.
        let ext = Screen {
            x: 1512.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let p = snap_placement(ext, (1512.0 + 30.0, 540.0));
        assert_eq!(p, Placement::preset(Position::Left));
        let r = anchor_window_rect(Position::Left, ext, window_size_for(Position::Left));
        assert_eq!((r.x, r.y), (1512.0 + 12.0, (1080.0 - 112.0) / 2.0));
    }

    // ---- Monitors ------------------------------------------------------------

    #[test]
    fn screen_at_finds_the_monitor_under_the_point() {
        let ext = Screen {
            x: 1512.0,
            y: -200.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let screens = [RETINA, ext];
        assert_eq!(screen_at(&screens, 100.0, 100.0), Some(RETINA));
        assert_eq!(screen_at(&screens, 2000.0, -100.0), Some(ext));
        // Outside every monitor: the caller falls back to the primary.
        assert_eq!(screen_at(&screens, -50.0, 50.0), None);
        assert_eq!(screen_at(&[], 0.0, 0.0), None);
    }

    #[test]
    fn clamp_keeps_the_window_on_screen() {
        let (w, h) = (OVERLAY_WIDTH, OVERLAY_HEIGHT);
        assert_eq!(clamp_to_screen(RETINA, w, h, (-40.0, -9.0)), (0.0, 0.0));
        assert_eq!(
            clamp_to_screen(RETINA, w, h, (1500.0, 990.0)),
            (1512.0 - 112.0, 982.0 - 56.0)
        );
        assert_eq!(
            clamp_to_screen(RETINA, w, h, (300.0, 400.0)),
            (300.0, 400.0)
        );
    }

    #[test]
    fn capsule_size_payload_is_parsed_and_checked() {
        assert_eq!(
            parse_capsule_size(r#"{"width":72,"height":20}"#),
            Some((72.0, 20.0))
        );
        assert_eq!(parse_capsule_size(r#"{"width":0,"height":26}"#), None);
        assert_eq!(parse_capsule_size(r#"{"width":500,"height":26}"#), None);
        assert_eq!(parse_capsule_size(r#"{"width":116,"height":60}"#), None);
        assert_eq!(parse_capsule_size(r#"{"width":92}"#), None);
        assert_eq!(parse_capsule_size("nope"), None);
    }
    // ---- 2.3c: tiny idle capsule, hover controls -----------------------------

    #[test]
    fn idle_is_tiny_and_the_window_holds_the_hover_controls_with_the_lift() {
        assert_eq!(DEFAULT_CAPSULE, (36.0, 8.0));
        assert_eq!(HOVER_CAPSULE, (84.0, 36.0));
        // 8 px inset each side and the 4% drag lift.
        assert!(HOVER_CAPSULE.0 * 1.04 + 2.0 * CAPSULE_INSET <= OVERLAY_WIDTH);
        assert!(HOVER_CAPSULE.1 * 1.04 + 2.0 * CAPSULE_INSET <= OVERLAY_HEIGHT);
        let r = capsule_rect((0.0, 0.0), Position::Right, None);
        assert_eq!((r.w, r.h), (8.0, 36.0));
    }

    #[test]
    fn rect_padded_grows_every_side() {
        let r = Rect {
            x: 10.0,
            y: 20.0,
            w: 36.0,
            h: 8.0,
        };
        assert_eq!(
            r.padded(8.0),
            Rect {
                x: 2.0,
                y: 12.0,
                w: 52.0,
                h: 24.0
            }
        );
    }

    #[test]
    fn hover_rect_and_buttons_on_the_top_anchor() {
        let top = anchor_window_rect(Position::Top, RETINA, window_size_for(Position::Top));
        let h = hover_rect((top.x, top.y), Position::Top);
        assert_eq!(
            h,
            Rect {
                x: 700.0 + 14.0,
                y: 40.0,
                w: 84.0,
                h: 36.0
            }
        );
        let [(b0, mic), (b1, open)] = button_rects(h, Position::Top);
        assert_eq!((b0, b1), (OverlayButton::Mic, OverlayButton::Open));
        assert_eq!(
            mic,
            Rect {
                x: 714.0 + 10.0,
                y: 44.0,
                w: 28.0,
                h: 28.0
            }
        );
        assert_eq!(
            open,
            Rect {
                x: 714.0 + 46.0,
                y: 44.0,
                w: 28.0,
                h: 28.0
            }
        );
    }

    #[test]
    fn hover_buttons_stack_mic_over_open_when_upright() {
        let left = anchor_window_rect(Position::Left, RETINA, window_size_for(Position::Left));
        let h = hover_rect((left.x, left.y), Position::Left);
        // 36 x 84 upright, centred in the 56 x 112 window at (12, 435).
        assert_eq!(
            h,
            Rect {
                x: 22.0,
                y: 449.0,
                w: 36.0,
                h: 84.0
            }
        );
        let [(_, mic), (_, open)] = button_rects(h, Position::Left);
        assert_eq!((mic.x, mic.y), (26.0, 459.0));
        assert_eq!((open.x, open.y), (26.0, 495.0));
    }

    #[test]
    fn buttons_sit_inside_the_hover_capsule_apart_on_every_anchor() {
        for p in Position::ALL {
            let win = anchor_window_rect(p, RETINA, window_size_for(p));
            let h = hover_rect((win.x, win.y), p);
            // The hover capsule stays inside the window.
            assert!(h.x >= win.x && h.y >= win.y, "{p:?}");
            assert!(
                h.x + h.w <= win.x + win.w && h.y + h.h <= win.y + win.h,
                "{p:?}"
            );
            let [(_, a), (_, b)] = button_rects(h, p);
            for r in [a, b] {
                assert!(
                    h.contains(r.x, r.y) && h.contains(r.x + r.w, r.y + r.h),
                    "{p:?}"
                );
            }
            // No overlap: a press can only ever hit one button.
            let apart = a.x + a.w < b.x || a.y + a.h < b.y;
            assert!(apart, "{p:?} {a:?} {b:?}");
        }
    }

    #[test]
    fn hover_zone_is_the_small_capsule_closed_and_the_controls_open() {
        let top = anchor_window_rect(Position::Top, RETINA, window_size_for(Position::Top));
        let o = (top.x, top.y);
        let closed = hover_zone(o, Position::Top, None, false);
        // Idle 36 x 8 at (738, 40), plus 8 px.
        assert_eq!(
            closed,
            Rect {
                x: 730.0,
                y: 32.0,
                w: 52.0,
                h: 24.0
            }
        );
        assert!(closed.contains(756.0, 34.0));
        assert!(!closed.contains(756.0, 70.0));
        let open = hover_zone(o, Position::Top, None, true);
        assert_eq!(open, hover_rect(o, Position::Top).padded(HOVER_PAD));
        // Once open, the cursor can travel down to the buttons.
        assert!(open.contains(756.0, 70.0));
        // Upright: the zone turns with the capsule.
        let left = anchor_window_rect(Position::Left, RETINA, window_size_for(Position::Left));
        let z = hover_zone((left.x, left.y), Position::Left, None, false);
        assert_eq!((z.w, z.h), (24.0, 52.0));
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let path = temp_file("atomic");
        save_placement(&path, Placement::preset(Position::Left)).unwrap();
        // A stale temp file from a crash is simply replaced.
        std::fs::write(temp_path(&path), "half").unwrap();
        save_placement(&path, Placement::preset(Position::Bottom)).unwrap();
        assert_eq!(load_placement(&path), Placement::preset(Position::Bottom));
        assert!(!temp_path(&path).exists());
        assert_eq!(
            temp_path(&path).file_name().unwrap().to_str(),
            Some("overlay.json.tmp")
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

//! Notch-style floating status pill. A second, borderless, transparent,
//! click-through window that the frontend renders as a black pill and drives
//! from the `dictation-status` event. The pill sits on one screen edge (top,
//! bottom, left or right) at an offset along that edge; the tray presets and
//! a drag (see `overlay_drag`) change it, and the choice is saved to
//! `overlay.json` in the app data dir.
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
    AppHandle, Emitter, LogicalPosition, Manager, Runtime, WebviewUrl, WebviewWindowBuilder,
};

/// Label of the overlay window. The frontend renders the pill for this label.
pub const OVERLAY_WINDOW: &str = "overlay";
/// Overlay window size in logical pixels: the widest capsule (listening,
/// 92 x 36) plus room for its soft shadow and the drag lift.
pub const OVERLAY_WIDTH: f64 = 120.0;
pub const OVERLAY_HEIGHT: f64 = 56.0;
/// Gap between the window and the screen edge for non-top positions.
pub const EDGE_MARGIN: f64 = 12.0;
/// Top position: the window starts at the notch band's lower edge; the
/// capsule sits 8 px inside it (overlay.css `--ov-inset`), so its top is
/// about 40 px below the screen edge and fully visible on notched displays.
pub const TOP_OFFSET: f64 = 32.0;
/// Gap between the capsule and the window edge it is parked against
/// (overlay.css `--ov-inset`).
pub const CAPSULE_INSET: f64 = 8.0;
/// Capsule size used for hit testing until the frontend reports one: the
/// idle capsule (overlay state.ts `capsuleSize`).
pub const DEFAULT_CAPSULE: (f64, f64) = (60.0, 30.0);
/// Event sent to the overlay when its position changes (payload: "top" etc).
pub const POSITION_EVENT: &str = "overlay-position";
/// File in the app data dir that stores the chosen position.
pub const SETTINGS_FILE: &str = "overlay.json";
/// Tray item id that puts the pill back at the top, below the notch.
pub const RESET_MENU_ID: &str = "overlay-reset";

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
}

/// Where the pill sits: an edge plus how far along it, 0..1 (0 is the
/// left or top end, 0.5 the middle).
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
}

/// Logical top-left of a `win_w` x `win_h` window placed at `pos` on `screen`.
/// Top is centred, `TOP_OFFSET` below the top edge (clear of the notch);
/// the others are centred on their edge with `EDGE_MARGIN` of gap.
pub fn overlay_origin(screen: Screen, win_w: f64, win_h: f64, pos: Position) -> (f64, f64) {
    placement_origin(screen, win_w, win_h, Placement::preset(pos))
}

/// Logical top-left of the window for `placement` on `screen`.
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
/// laid out as overlay.css does for `edge`. `capsule` is the (width, height)
/// the frontend reported; `None` uses the idle size.
pub fn capsule_rect(window: (f64, f64), edge: Position, capsule: Option<(f64, f64)>) -> Rect {
    let (w, h) = match capsule {
        Some((w, h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => {
            (w.min(OVERLAY_WIDTH), h.min(OVERLAY_HEIGHT))
        }
        _ => DEFAULT_CAPSULE,
    };
    let (ox, oy) = window;
    let mid_x = ox + (OVERLAY_WIDTH - w) / 2.0;
    let mid_y = oy + (OVERLAY_HEIGHT - h) / 2.0;
    let (x, y) = match edge {
        Position::Top => (mid_x, oy + CAPSULE_INSET),
        Position::Bottom => (mid_x, oy + OVERLAY_HEIGHT - CAPSULE_INSET - h),
        Position::Left => (ox + CAPSULE_INSET, mid_y),
        Position::Right => (ox + OVERLAY_WIDTH - CAPSULE_INSET - w, mid_y),
    };
    Rect { x, y, w, h }
}

/// Placement for a pill dropped with its capsule centre at `center`: the
/// nearest screen edge, with the offset along it clamped to 0..1.
pub fn snap_placement(screen: Screen, win_w: f64, win_h: f64, center: (f64, f64)) -> Placement {
    let (sx, sy, sw, sh) = screen.logical();
    let (cx, cy) = center;
    let distances = [
        (Position::Top, cy - sy),
        (Position::Bottom, sy + sh - cy),
        (Position::Left, cx - sx),
        (Position::Right, sx + sw - cx),
    ];
    let mut edge = Position::Top;
    let mut best = f64::INFINITY;
    for (p, d) in distances {
        if d < best {
            best = d;
            edge = p;
        }
    }
    let fraction = |value: f64, min: f64, max: f64| {
        if max > min {
            (value - min) / (max - min)
        } else {
            0.5
        }
    };
    let offset = match edge {
        Position::Top | Position::Bottom => fraction(
            cx - win_w / 2.0,
            sx + EDGE_MARGIN,
            sx + sw - win_w - EDGE_MARGIN,
        ),
        Position::Left | Position::Right => fraction(
            cy - win_h / 2.0,
            sy + TOP_OFFSET,
            sy + sh - win_h - TOP_OFFSET,
        ),
    };
    Placement::new(edge, offset)
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

/// Writes the placement, creating the parent dir if needed.
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
    std::fs::write(path, json)
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

/// Moves the overlay window to `placement` on `screen` (the primary
/// monitor when `None`).
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
    let (x, y) = placement_origin(screen, OVERLAY_WIDTH, OVERLAY_HEIGHT, placement);
    let _ = window.set_position(LogicalPosition::new(x, y));
    crate::overlay_drag::set_window_frame((x, y), placement.edge);
    let _ = app.emit_to(OVERLAY_WINDOW, POSITION_EVENT, placement.edge.as_str());
}

/// Moves the overlay window to the middle of `pos` on the primary monitor.
pub fn move_overlay<R: Runtime>(app: &AppHandle<R>, pos: Position) {
    move_overlay_to(app, Placement::preset(pos), None);
}

/// Moves the overlay, saves the placement and updates the tray check marks.
/// `screen` is the monitor to place it on (primary when `None`); only the
/// edge and offset are saved, so a restart uses the primary monitor.
pub fn apply_placement<R: Runtime>(
    app: &AppHandle<R>,
    placement: Placement,
    screen: Option<Screen>,
) {
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

/// Creates the overlay window and parks it on edge `pos`, at the saved
/// offset when the saved placement is on that edge. Call from `setup`
/// (main thread).
pub fn create<R: Runtime>(app: &AppHandle<R>, pos: Position) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, OVERLAY_WINDOW, WebviewUrl::default())
        .title("DeepAI Voice Status")
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
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
    let placement = if saved.edge == pos {
        saved
    } else {
        Placement::preset(pos)
    };
    move_overlay_to(app, placement, None);
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
        // 3024 / 2 = 1512 logical, (1512 - 120) / 2 = 696.
        assert_eq!(
            overlay_origin(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, Position::Top),
            (696.0, 32.0)
        );
    }

    #[test]
    fn bottom_left_right_keep_a_margin() {
        // 1964 / 2 = 982 logical high.
        let (w, h) = (OVERLAY_WIDTH, OVERLAY_HEIGHT);
        assert_eq!(
            overlay_origin(RETINA, w, h, Position::Bottom),
            (696.0, 982.0 - 56.0 - 12.0)
        );
        let mid_y = (982.0 - 56.0) / 2.0;
        assert_eq!(overlay_origin(RETINA, w, h, Position::Left), (12.0, mid_y));
        assert_eq!(
            overlay_origin(RETINA, w, h, Position::Right),
            (1512.0 - 120.0 - 12.0, mid_y)
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
            (1920.0 + 900.0, -100.0 + 32.0)
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
            (1512.0 - 120.0 - 12.0, 982.0 - 56.0 - 12.0)
        );
        // Left and right: y runs from below the notch band to the same gap
        // above the bottom.
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Left, 0.0)),
            (12.0, 32.0)
        );
        assert_eq!(
            placement_origin(RETINA, w, h, Placement::new(Position::Right, 1.0)),
            (1512.0 - 120.0 - 12.0, 982.0 - 56.0 - 32.0)
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
        let size = Some((92.0, 36.0));
        assert_eq!(
            capsule_rect(o, Position::Top, size),
            Rect {
                x: 100.0 + 14.0,
                y: 208.0,
                w: 92.0,
                h: 36.0
            }
        );
        assert_eq!(
            capsule_rect(o, Position::Bottom, size),
            Rect {
                x: 114.0,
                y: 200.0 + 56.0 - 8.0 - 36.0,
                w: 92.0,
                h: 36.0
            }
        );
        assert_eq!(
            capsule_rect(o, Position::Left, size),
            Rect {
                x: 108.0,
                y: 210.0,
                w: 92.0,
                h: 36.0
            }
        );
        assert_eq!(
            capsule_rect(o, Position::Right, size),
            Rect {
                x: 100.0 + 120.0 - 8.0 - 92.0,
                y: 210.0,
                w: 92.0,
                h: 36.0
            }
        );
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

    #[test]
    fn snap_picks_the_top_edge_and_keeps_the_x_offset() {
        // Capsule centre near the top, a quarter of the way across.
        let p = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (414.0, 60.0));
        assert_eq!(p.edge, Position::Top);
        // Window x = 414 - 60 = 354; range 12..1380, (354 - 12) / 1368 = 0.25.
        assert!((p.offset - 0.25).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn snap_picks_the_bottom_edge() {
        let p = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (756.0, 960.0));
        assert_eq!(p, Placement::new(Position::Bottom, 0.5));
    }

    #[test]
    fn snap_picks_the_left_edge_and_keeps_the_y_offset() {
        // Window y = cy - 28; range 32..894. cy = 32 + 28 + 862 * 0.75.
        let cy = 32.0 + 28.0 + 862.0 * 0.75;
        let p = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (40.0, cy));
        assert_eq!(p.edge, Position::Left);
        assert!((p.offset - 0.75).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn snap_picks_the_right_edge_and_clamps_the_offset() {
        let p = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (1500.0, 500.0));
        assert_eq!(p.edge, Position::Right);
        // Far below the top of the range is clamped, never negative.
        let high = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (1505.0, 40.0));
        assert_eq!(high.edge, Position::Right);
        assert_eq!(high.offset, 0.0);
        // Dropped past the screen corner still clamps to 0..1.
        let off = snap_placement(RETINA, OVERLAY_WIDTH, OVERLAY_HEIGHT, (5000.0, 2000.0));
        assert!((0.0..=1.0).contains(&off.offset), "{off:?}");
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
        let p = snap_placement(ext, OVERLAY_WIDTH, OVERLAY_HEIGHT, (1512.0 + 30.0, 540.0));
        assert_eq!(p.edge, Position::Left);
        assert!((p.offset - 0.5).abs() < 0.01, "{p:?}");
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
            (1512.0 - 120.0, 982.0 - 56.0)
        );
        assert_eq!(
            clamp_to_screen(RETINA, w, h, (300.0, 400.0)),
            (300.0, 400.0)
        );
    }

    #[test]
    fn capsule_size_payload_is_parsed_and_checked() {
        assert_eq!(
            parse_capsule_size(r#"{"width":92,"height":36}"#),
            Some((92.0, 36.0))
        );
        assert_eq!(parse_capsule_size(r#"{"width":0,"height":36}"#), None);
        assert_eq!(parse_capsule_size(r#"{"width":500,"height":36}"#), None);
        assert_eq!(parse_capsule_size(r#"{"width":92}"#), None);
        assert_eq!(parse_capsule_size("nope"), None);
    }
}

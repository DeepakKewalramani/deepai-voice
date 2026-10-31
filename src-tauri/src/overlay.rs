//! Notch-style floating status pill. A second, borderless, transparent,
//! click-through window that the frontend renders as a black pill and drives
//! from the `dictation-status` event. The user can park it at one of four
//! preset spots; the choice is saved to `overlay.json` in the app data dir.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::CheckMenuItem;
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, Runtime, WebviewUrl, WebviewWindowBuilder,
};

/// Label of the overlay window. The frontend renders the pill for this label.
pub const OVERLAY_WINDOW: &str = "overlay";
/// Overlay window size in logical pixels. Big enough for the widest pill.
pub const OVERLAY_WIDTH: f64 = 360.0;
pub const OVERLAY_HEIGHT: f64 = 84.0;
/// Gap between the window and the screen edge for non-top positions.
pub const EDGE_MARGIN: f64 = 12.0;
/// Event sent to the overlay when its position changes (payload: "top" etc).
pub const POSITION_EVENT: &str = "overlay-position";
/// File in the app data dir that stores the chosen position.
pub const SETTINGS_FILE: &str = "overlay.json";

/// Preset spots for the pill.
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

/// A monitor in physical pixels, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// Logical top-left of a `win_w` x `win_h` window placed at `pos` on `screen`.
/// Top is centred and flush with the top edge (y = 0) so it meets the notch;
/// the others are centred on their edge with `EDGE_MARGIN` of gap.
pub fn overlay_origin(screen: Screen, win_w: f64, win_h: f64, pos: Position) -> (f64, f64) {
    let scale = if screen.scale > 0.0 {
        screen.scale
    } else {
        1.0
    };
    let (sx, sy) = (screen.x / scale, screen.y / scale);
    let (sw, sh) = (screen.width / scale, screen.height / scale);
    let cx = sx + (sw - win_w) / 2.0;
    let cy = sy + (sh - win_h) / 2.0;
    match pos {
        Position::Top => (cx, sy),
        Position::Bottom => (cx, sy + sh - win_h - EDGE_MARGIN),
        Position::Left => (sx + EDGE_MARGIN, cy),
        Position::Right => (sx + sw - win_w - EDGE_MARGIN, cy),
    }
}

#[derive(Serialize, Deserialize)]
struct Stored {
    position: Position,
}

/// Reads the saved position. Missing or invalid file means Top.
pub fn load_position(path: &Path) -> Position {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Stored>(&s).ok())
        .map(|s| s.position)
        .unwrap_or_default()
}

/// Writes the position, creating the parent dir if needed.
pub fn save_position(path: &Path, position: Position) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string(&Stored { position }).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
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

/// Saved position for this app (Top when none is saved).
pub fn saved_position<R: Runtime>(app: &AppHandle<R>) -> Position {
    settings_path(app)
        .map(|p| load_position(&p))
        .unwrap_or_default()
}

/// Moves the overlay window to `pos` on the primary monitor.
pub fn move_overlay<R: Runtime>(app: &AppHandle<R>, pos: Position) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW) else {
        return;
    };
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let screen = Screen {
        x: monitor.position().x as f64,
        y: monitor.position().y as f64,
        width: monitor.size().width as f64,
        height: monitor.size().height as f64,
        scale: monitor.scale_factor(),
    };
    let (x, y) = overlay_origin(screen, OVERLAY_WIDTH, OVERLAY_HEIGHT, pos);
    let _ = window.set_position(LogicalPosition::new(x, y));
    let _ = app.emit_to(OVERLAY_WINDOW, POSITION_EVENT, pos.as_str());
}

/// Moves the overlay, saves the choice and updates the tray check marks.
pub fn apply_position<R: Runtime>(app: &AppHandle<R>, pos: Position) {
    move_overlay(app, pos);
    if let Some(path) = settings_path(app)
        && let Err(e) = save_position(&path, pos)
    {
        tracing::warn!("could not save overlay position: {e}");
    }
    if let Some(state) = app.try_state::<OverlayState<R>>() {
        if let Ok(mut current) = state.position.lock() {
            *current = pos;
        }
        for (p, item) in &state.items {
            let _ = item.set_checked(*p == pos);
        }
    }
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

/// Creates the overlay window and parks it at `pos`. Call from `setup`
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
    move_overlay(app, pos);
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
    fn top_is_centred_and_flush_with_the_top_edge() {
        // 3024 / 2 = 1512 logical, (1512 - 360) / 2 = 576.
        assert_eq!(
            overlay_origin(RETINA, 360.0, 84.0, Position::Top),
            (576.0, 0.0)
        );
    }

    #[test]
    fn bottom_left_right_keep_a_margin() {
        // 1964 / 2 = 982 logical high.
        assert_eq!(
            overlay_origin(RETINA, 360.0, 84.0, Position::Bottom),
            (576.0, 982.0 - 84.0 - 12.0)
        );
        let mid_y = (982.0 - 84.0) / 2.0;
        assert_eq!(
            overlay_origin(RETINA, 360.0, 84.0, Position::Left),
            (12.0, mid_y)
        );
        assert_eq!(
            overlay_origin(RETINA, 360.0, 84.0, Position::Right),
            (1512.0 - 360.0 - 12.0, mid_y)
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
            overlay_origin(screen, 360.0, 84.0, Position::Top),
            (1920.0 + 780.0, -100.0)
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
}

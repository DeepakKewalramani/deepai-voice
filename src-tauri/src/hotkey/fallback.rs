//! Fallback hold-to-talk shortcut (Ctrl+Option+Space) through
//! tauri-plugin-global-shortcut. Works on macOS and Windows. Only
//! press/release state is forwarded, never key contents.

use std::sync::mpsc::Sender;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::HotkeyEvent;

/// Maps the plugin's key state to a logical event.
pub fn map_state(state: ShortcutState) -> HotkeyEvent {
    match state {
        ShortcutState::Pressed => HotkeyEvent::Press,
        ShortcutState::Released => HotkeyEvent::Release,
    }
}

/// Parses the shortcut string, e.g. "Ctrl+Option+Space".
pub fn parse(shortcut: &str) -> Result<Shortcut, String> {
    shortcut.parse::<Shortcut>().map_err(|e| e.to_string())
}

/// True for the plain Escape shortcut used to cancel a dictation.
pub fn is_escape(shortcut: &Shortcut) -> bool {
    parse(crate::dictation::ESCAPE_SHORTCUT).is_ok_and(|esc| esc.id() == shortcut.id())
}

/// Builds the plugin; events go to `tx`.
pub fn plugin<R: Runtime>(tx: Sender<HotkeyEvent>) -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, shortcut, event| {
            // Esc (dictation cancel) has its own handler; the plugin calls
            // this global handler for it too, so it must not act as the
            // hold-to-talk key.
            if is_escape(shortcut) {
                return;
            }
            let _ = tx.send(map_state(event.state()));
        })
        .build()
}

/// Registers the shortcut; call from `setup` after adding the plugin.
pub fn register<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String> {
    let parsed = parse(shortcut)?;
    app.global_shortcut()
        .register(parsed)
        .map_err(|e| e.to_string())
}

/// Removes the shortcut again, for example once the Fn key tap is running.
pub fn unregister<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String> {
    let parsed = parse(shortcut)?;
    app.global_shortcut()
        .unregister(parsed)
        .map_err(|e| e.to_string())
}

pub mod audio;
pub mod dictation;
pub mod hotkey;
pub mod logging;
pub mod overlay;
pub mod paste;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, image::Image};

pub mod settings;
pub mod stt;

/// Id of the tray menu item that shows the settings window.
pub const SETTINGS_ID: &str = "settings";
/// Id of the tray menu item that quits the app.
pub const QUIT_ID: &str = "quit";
/// Label of the hidden main window, shown by the Settings item.
pub const MAIN_WINDOW: &str = "main";

/// Tray menu entries, top to bottom, as (id, label).
pub fn tray_menu_items() -> Vec<(&'static str, &'static str)> {
    vec![(SETTINGS_ID, "Settings"), (QUIT_ID, "Quit")]
}

/// What a tray menu click should do.
#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    ShowSettings,
    Quit,
    MoveOverlay(overlay::Position),
    Ignore,
}

/// Maps a tray menu item id to its action.
pub fn tray_action(id: &str) -> TrayAction {
    match id {
        SETTINGS_ID => TrayAction::ShowSettings,
        QUIT_ID => TrayAction::Quit,
        _ => overlay::Position::from_menu_id(id)
            .map(TrayAction::MoveOverlay)
            .unwrap_or(TrayAction::Ignore),
    }
}

/// Shows and focuses the hidden main window.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Redacted logs to stderr; RUST_LOG overrides the default filter.
    let _ = logging::init_logging();
    let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
    tauri::Builder::default()
        // Closing the window hides it; the app keeps running in the menu bar.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .plugin(hotkey::fallback::plugin(hotkey_tx.clone()))
        .invoke_handler(tauri::generate_handler![
            overlay::set_overlay_position,
            overlay::get_overlay_position
        ])
        .setup(move |app| {
            let on_action = dictation::spawn(app.handle().clone());
            hotkey::start(app.handle(), hotkey_tx, hotkey_rx, on_action);
            // Menu bar app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let items = tray_menu_items()
                .into_iter()
                .map(|(id, label)| MenuItem::with_id(app, id, label, true, None::<&str>))
                .collect::<Result<Vec<_>, _>>()?;
            let mut refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = items
                .iter()
                .map(|i| i as &dyn tauri::menu::IsMenuItem<_>)
                .collect();

            // "Notch position" submenu, placed between Settings and Quit.
            let position = overlay::saved_position(app.handle());
            let checks = overlay::Position::ALL
                .into_iter()
                .map(|p| {
                    CheckMenuItem::with_id(
                        app,
                        p.menu_id(),
                        p.label(),
                        true,
                        p == position,
                        None::<&str>,
                    )
                    .map(|item| (p, item))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let check_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = checks
                .iter()
                .map(|(_, i)| i as &dyn tauri::menu::IsMenuItem<_>)
                .collect();
            let notch_menu = Submenu::with_items(app, "Notch position", true, &check_refs)?;
            refs.insert(refs.len().saturating_sub(1), &notch_menu);
            let menu = Menu::with_items(app, &refs)?;
            app.manage(overlay::OverlayState::new(position, checks));
            // The pill is cosmetic: a failure must never stop the app from starting.
            if let Err(e) = overlay::create(app.handle(), position) {
                tracing::warn!("notch overlay not created: {e}");
            }

            let icon: Image<'static> = tauri::include_image!("icons/32x32.png");
            TrayIconBuilder::with_id(dictation::TRAY_ID)
                .icon(icon)
                .icon_as_template(true)
                .tooltip("DeepAI Voice")
                .menu(&menu)
                .on_menu_event(|app, event| match tray_action(event.id().as_ref()) {
                    TrayAction::ShowSettings => show_main_window(app),
                    TrayAction::Quit => app.exit(0),
                    TrayAction::MoveOverlay(p) => overlay::apply_position(app, p),
                    TrayAction::Ignore => {}
                })
                .build(app)?;
            // Show the settings window on launch so the app is visibly running
            // even when the menu bar hides the tray icon.
            show_main_window(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Clicking the app again (Finder, Spotlight) brings the window back.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => show_main_window(app),
            // Keep running in the menu bar after the last window is closed.
            tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
                api.prevent_exit();
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_has_settings_above_quit() {
        assert_eq!(
            tray_menu_items(),
            vec![("settings", "Settings"), ("quit", "Quit")]
        );
    }

    #[test]
    fn every_menu_item_maps_to_an_action() {
        for (id, _) in tray_menu_items() {
            assert_ne!(tray_action(id), TrayAction::Ignore, "id {id}");
        }
        assert_eq!(tray_action("settings"), TrayAction::ShowSettings);
        assert_eq!(tray_action("quit"), TrayAction::Quit);
        assert_eq!(tray_action("unknown"), TrayAction::Ignore);
    }

    #[test]
    fn notch_position_items_map_to_move_actions() {
        for p in overlay::Position::ALL {
            assert_eq!(tray_action(&p.menu_id()), TrayAction::MoveOverlay(p));
        }
    }
}

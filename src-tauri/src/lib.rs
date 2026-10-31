pub mod audio;
pub mod logging;
pub mod paste;

use tauri::menu::{Menu, MenuItem};
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
    Ignore,
}

/// Maps a tray menu item id to its action.
pub fn tray_action(id: &str) -> TrayAction {
    match id {
        SETTINGS_ID => TrayAction::ShowSettings,
        QUIT_ID => TrayAction::Quit,
        _ => TrayAction::Ignore,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Menu bar app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let items = tray_menu_items()
                .into_iter()
                .map(|(id, label)| MenuItem::with_id(app, id, label, true, None::<&str>))
                .collect::<Result<Vec<_>, _>>()?;
            let refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = items
                .iter()
                .map(|i| i as &dyn tauri::menu::IsMenuItem<_>)
                .collect();
            let menu = Menu::with_items(app, &refs)?;

            let icon: Image<'static> = tauri::include_image!("icons/32x32.png");
            TrayIconBuilder::new()
                .icon(icon)
                .icon_as_template(true)
                .tooltip("DeepAI Voice")
                .menu(&menu)
                .on_menu_event(|app, event| match tray_action(event.id().as_ref()) {
                    TrayAction::ShowSettings => {
                        let window = app
                            .get_webview_window(MAIN_WINDOW)
                            .expect("main window must exist (tauri.conf.json)");
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    TrayAction::Quit => app.exit(0),
                    TrayAction::Ignore => {}
                })
                .build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
}

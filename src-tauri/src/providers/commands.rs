//! Tauri commands for Settings > Providers. Thin wrappers over `hub::Hub` with the
//! real Keychain store and the app config dir. Errors are fixed plain-English
//! strings; no command result or error ever contains a key.

use super::catalog::{self, CATALOG, CatalogEntry};
use super::custom::CustomProviderForm;
use super::http::UreqProbe;
use super::hub::{Hub, ProviderStatus};
use super::probe::TestResult;
use crate::settings::keys::KeyringStore;
use crate::settings::settings_path;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

/// Serialises load-modify-save of settings.json between commands.
static LOCK: Mutex<()> = Mutex::new(());

fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| settings_path(&d))
        .map_err(|_| "Could not find the settings folder.".to_owned())
}

fn with_hub<T>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&Hub<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let store = KeyringStore;
    let hub = Hub::new(path(app)?, &store);
    f(&hub)
}

#[tauri::command]
pub fn provider_catalog() -> &'static [CatalogEntry] {
    CATALOG
}

#[tauri::command(async)]
pub fn providers_status(app: tauri::AppHandle) -> Result<Vec<ProviderStatus>, String> {
    with_hub(&app, |h| h.status())
}

#[tauri::command(async)]
pub fn connect_provider(
    app: tauri::AppHandle,
    id: String,
    key: String,
) -> Result<ProviderStatus, String> {
    with_hub(&app, |h| h.connect(&id, &key))
}

#[tauri::command(async)]
pub fn disconnect_provider(app: tauri::AppHandle, id: String) -> Result<(), String> {
    with_hub(&app, |h| h.disconnect(&id))
}

#[tauri::command(async)]
pub fn save_custom_provider(
    app: tauri::AppHandle,
    form: CustomProviderForm,
    key: String,
) -> Result<ProviderStatus, String> {
    with_hub(&app, |h| h.save_custom(&form, &key))
}

#[tauri::command(async)]
pub fn delete_custom_provider(app: tauri::AppHandle, id: String) -> Result<(), String> {
    with_hub(&app, |h| h.delete_custom(&id))
}

/// Runs only when the user presses Test connection.
#[tauri::command(async)]
pub fn test_provider(app: tauri::AppHandle, id: String) -> Result<TestResult, String> {
    let p = path(&app)?;
    let store = KeyringStore;
    // No settings lock: the network call must not block other commands.
    Hub::new(p, &store).test(&id, &UreqProbe::default())
}

/// Opens the docs page of a catalog entry. The URL comes from the catalog, never the UI.
#[tauri::command]
pub fn open_provider_docs(id: String) -> Result<(), String> {
    let entry = catalog::find(&id).ok_or("That provider is not in the list.")?;
    if !entry.docs_url.starts_with("https://") {
        return Err("This link cannot be opened.".into());
    }
    open_url(entry.docs_url)
}

#[cfg(target_os = "macos")]
fn open_url(url: &'static str) -> Result<(), String> {
    let mut child = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|_| "Could not open the browser.".to_owned())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_url(_url: &'static str) -> Result<(), String> {
    Err("Opening links is only available on macOS for now.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_open_only_for_catalog_ids() {
        for bad in ["", "https://evil.example", "gnani; rm -rf /", "../x"] {
            assert!(open_provider_docs(bad.into()).is_err(), "{bad}");
        }
    }

    #[test]
    fn catalog_command_returns_the_static_catalog() {
        assert_eq!(provider_catalog().len(), CATALOG.len());
    }
}

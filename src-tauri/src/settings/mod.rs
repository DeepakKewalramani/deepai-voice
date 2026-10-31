//! Versioned JSON settings (ADR 0002 section 5). Keys never live here; see `keys`.

pub mod keys;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current schema version written by this build.
pub const SCHEMA_VERSION: u32 = 1;
/// File name inside the app config dir (tech.tequity.deepaivoice).
pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub hotkeys: Hotkeys,
    pub providers: Vec<Provider>,
    pub chains: Chains,
    pub styles: Vec<Style>,
    pub active_style: String,
    pub timeouts: Timeouts,
    pub privacy: Privacy,
    pub max_record_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub dictate: String,
}

/// A provider entry. There is deliberately no field for a raw key: unknown
/// fields such as `apiKey` are dropped on load and never written back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub kind: String,
    pub adapter: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Chains {
    pub stt: Vec<String>,
    pub llm: Vec<String>,
    pub tts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub user_template: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Timeouts {
    pub stt_ms: u64,
    pub llm_ms: u64,
    pub total_deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Privacy {
    /// provider id -> date the user acknowledged cloud use.
    pub cloud_acknowledged: BTreeMap<String, String>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            dictate: "Ctrl+Alt+Space".into(),
        }
    }
}

impl Default for Timeouts {
    // Placeholders until spikes S3 and S5 (ADR 0002).
    fn default() -> Self {
        Self {
            stt_ms: 8000,
            llm_ms: 2500,
            total_deadline_ms: 10000,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            hotkeys: Hotkeys::default(),
            providers: vec![Provider {
                id: "whisper-local".into(),
                kind: "stt".into(),
                adapter: "whisper_local".into(),
                model: "base.en".into(),
                base_url: None,
                key_ref: None,
            }],
            chains: Chains { stt: vec!["whisper-local".into()], llm: vec![], tts: vec![] },
            styles: vec![Style {
                id: "clean".into(),
                name: "Clean up".into(),
                system_prompt: "Clean up the dictated text. Fix punctuation and filler words. Do not add content.".into(),
                user_template: "{{transcript}}".into(),
            }],
            active_style: "clean".into(),
            timeouts: Timeouts::default(),
            privacy: Privacy::default(),
            max_record_seconds: 120,
        }
    }
}

/// Result of loading the settings file.
#[derive(Debug)]
pub struct LoadResult {
    pub settings: Settings,
    /// Version found on disk if migrations ran.
    pub migrated_from: Option<u32>,
    /// Backup path when the file was invalid and defaults were loaded.
    pub backup: Option<PathBuf>,
    /// True when the file must not be saved over: newer app version,
    /// unreadable file, or an invalid file that could not be backed up.
    pub read_only: bool,
    /// Provider or style entries skipped because they were invalid.
    pub dropped_entries: usize,
    /// Provider ids whose raw key was removed by migration. The UI should ask
    /// the user to re-enter them. Ids only, never values.
    pub keys_dropped: Vec<String>,
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    Json(serde_json::Error),
    ReadOnly,
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "settings io error: {e}"),
            Self::Json(e) => write!(f, "settings json error: {e}"),
            Self::ReadOnly => write!(f, "settings file is read-only for this session; not saving"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<io::Error> for SettingsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Path of the settings file inside the app config dir.
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE)
}

/// Ordered migrations: entry `i` upgrades version `i` to `i + 1`.
/// The second argument collects provider ids whose raw key was dropped.
type Migration = fn(&mut Value, &mut Vec<String>);
const MIGRATIONS: &[Migration] = &[migrate_v0_to_v1];

/// v0 had no `schemaVersion` and could hold raw `apiKey` values on providers.
/// v1 moves to `keyRef` only; raw keys are removed and never carried forward.
fn migrate_v0_to_v1(v: &mut Value, keys_dropped: &mut Vec<String>) {
    if let Some(providers) = v.get_mut("providers").and_then(Value::as_array_mut) {
        for p in providers.iter_mut().filter_map(Value::as_object_mut) {
            let had_key = p.remove("apiKey").is_some() | p.remove("key").is_some();
            let id = p.get("id").and_then(Value::as_str).map(str::to_owned);
            if had_key && let Some(id) = &id {
                keys_dropped.push(id.clone());
            }
            let local = p.get("adapter").and_then(Value::as_str) == Some("whisper_local");
            if !local
                && !p.contains_key("keyRef")
                && let Some(id) = id
            {
                p.insert("keyRef".into(), id.into());
            }
        }
    }
}

/// Runs migrations from version `from` up to `SCHEMA_VERSION`.
/// Returns the provider ids whose raw key was dropped.
pub fn migrate(v: &mut Value, from: u32) -> Vec<String> {
    let mut keys_dropped = Vec::new();
    for m in MIGRATIONS.iter().skip(from as usize) {
        m(v, &mut keys_dropped);
    }
    if let Some(obj) = v.as_object_mut() {
        obj.insert("schemaVersion".into(), SCHEMA_VERSION.into());
    }
    keys_dropped
}

/// Moves an invalid file aside under a unique name. None if that failed.
fn backup_invalid(path: &Path) -> Option<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup = (0..1000)
        .map(|n| path.with_file_name(format!("{SETTINGS_FILE}.{millis}-{n}.bak")))
        .find(|b| !b.exists())?;
    fs::rename(path, &backup).ok().map(|_| backup)
}

/// Parses a list field entry by entry, keeping valid entries and counting
/// the rest. A non-array value counts as one dropped entry.
fn take_list<T: serde::de::DeserializeOwned>(
    v: &mut Value,
    field: &str,
    dropped: &mut usize,
) -> Option<Vec<T>> {
    let raw = v.as_object_mut()?.remove(field)?;
    let Value::Array(items) = raw else {
        *dropped += 1;
        return Some(Vec::new());
    };
    let total = items.len();
    let kept: Vec<T> = items
        .into_iter()
        .filter_map(|i| serde_json::from_value(i).ok())
        .collect();
    *dropped += total - kept.len();
    Some(kept)
}

/// Loads settings. Never panics and never leaves the user's file at risk:
/// - missing file: defaults
/// - unreadable file (permissions, not UTF-8, a directory): defaults, read-only
/// - invalid JSON: backed up, defaults (read-only if the backup failed)
/// - newer version: known fields, read-only
/// - bad provider or style entries: dropped and counted, the rest kept
pub fn load(path: &Path) -> LoadResult {
    let result = |settings, read_only| LoadResult {
        settings,
        migrated_from: None,
        backup: None,
        read_only,
        dropped_entries: 0,
        keys_dropped: Vec::new(),
    };
    let invalid = || {
        let backup = backup_invalid(path);
        let mut r = result(Settings::default(), backup.is_none());
        r.backup = backup;
        r
    };
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return result(Settings::default(), false),
        Err(_) => return result(Settings::default(), true),
    };
    let mut value: Value = match serde_json::from_str(&text) {
        Ok(v @ Value::Object(_)) => v,
        _ => return invalid(),
    };
    let version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let read_only = version > u64::from(SCHEMA_VERSION);
    let mut keys_dropped = Vec::new();
    if !read_only {
        keys_dropped = migrate(&mut value, version as u32);
    }
    let mut dropped = 0;
    let providers = take_list::<Provider>(&mut value, "providers", &mut dropped);
    let styles = take_list::<Style>(&mut value, "styles", &mut dropped);
    let mut settings: Settings = match serde_json::from_value(value) {
        Ok(s) => s,
        Err(_) if read_only => Settings::default(),
        Err(_) => return invalid(),
    };
    if let Some(p) = providers {
        settings.providers = p;
    }
    if let Some(s) = styles {
        settings.styles = s;
    }
    LoadResult {
        settings,
        migrated_from: (!read_only && version < u64::from(SCHEMA_VERSION))
            .then_some(version as u32),
        backup: None,
        read_only,
        dropped_entries: dropped,
        keys_dropped,
    }
}

/// Writes settings atomically (temp file then rename). Refuses when read-only.
pub fn save(path: &Path, settings: &Settings, read_only: bool) -> Result<(), SettingsError> {
    if read_only {
        return Err(SettingsError::ReadOnly);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, export(settings)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// JSON export. Contains only `keyRef` names, never key values.
pub fn export(settings: &Settings) -> Result<String, SettingsError> {
    Ok(serde_json::to_string_pretty(settings)?)
}

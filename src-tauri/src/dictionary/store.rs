//! `dictionary.json` in the app data folder: `{ "version": 1, "entries": [...] }`.
//!
//! Local file only, no network. Written atomically (temp file, then rename).
//! A missing file is seeded once with two entries. An unreadable or invalid
//! file is moved aside to `dictionary.json.bad` and an empty dictionary is
//! used (and saved, so the seed never comes back). Nothing here logs words.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{Entry, EntryInput, MAX_ENTRIES, validate, word_key};

pub const FILE: &str = "dictionary.json";
pub const BAD_FILE: &str = "dictionary.json.bad";
pub const VERSION: u32 = 1;

/// Path of the dictionary inside the app data dir.
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

#[derive(Serialize, Deserialize)]
struct FileV1 {
    version: u32,
    entries: Vec<Entry>,
}

/// Result of [`load`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub entries: Vec<Entry>,
    /// The file did not exist and was created with the seed entries.
    pub seeded: bool,
    /// The file was unreadable or held invalid data; a copy was kept as
    /// `dictionary.json.bad`.
    pub recovered: bool,
}

/// A new unique entry id.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("d{nanos:x}-{n:x}")
}

/// Entries written when no dictionary file exists yet.
pub fn seed_entries() -> Vec<Entry> {
    ["Deepak", "DeepakKewalramani"]
        .into_iter()
        .map(|word| Entry {
            id: new_id(),
            word: word.to_string(),
            heared_as: Vec::new(),
            language: super::ANY_LANGUAGE.to_string(),
            note: None,
        })
        .collect()
}

/// Writes the dictionary atomically: temp file in the same folder, then rename.
pub fn save(path: &Path, entries: &[Entry]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_string_pretty(&FileV1 {
        version: VERSION,
        entries: entries.to_vec(),
    })
    .map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)
}

fn bad_path(path: &Path) -> PathBuf {
    path.with_file_name(BAD_FILE)
}

/// Keeps only valid entries: fields re-validated, no duplicate word, no
/// duplicate id, at most `MAX_ENTRIES`. Returns them and whether any was dropped.
fn clean(entries: Vec<Entry>) -> (Vec<Entry>, bool) {
    let total = entries.len();
    let mut out: Vec<Entry> = Vec::with_capacity(total.min(MAX_ENTRIES));
    for e in entries {
        if out.len() >= MAX_ENTRIES || e.id.trim().is_empty() {
            continue;
        }
        let input = EntryInput {
            word: e.word,
            heared_as: e.heared_as,
            language: e.language,
            note: e.note,
        };
        let Ok(valid) = validate(&input, e.id) else {
            continue;
        };
        let key = word_key(&valid.word);
        if out
            .iter()
            .any(|o| o.id == valid.id || word_key(&o.word) == key)
        {
            continue;
        }
        out.push(valid);
    }
    let dropped = out.len() != total;
    (out, dropped)
}

/// Serializes every read-modify-write of the file, including the writes a
/// load may do (seeding, recovery), so two writers never share the temp file.
pub(crate) static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Reads the dictionary. Never fails; see the module docs for recovery.
pub fn load(path: &Path) -> Loaded {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let entries = seed_entries();
            let _ = save(path, &entries);
            return Loaded {
                entries,
                seeded: true,
                recovered: false,
            };
        }
        Err(_) => {
            // Unreadable: move it aside; only then is it safe to write a new one.
            if fs::rename(path, bad_path(path)).is_ok() {
                let _ = save(path, &[]);
            }
            return recovered(Vec::new());
        }
    };
    match serde_json::from_slice::<FileV1>(&bytes) {
        Ok(file) if file.version == VERSION => {
            let (entries, dropped) = clean(file.entries);
            if !dropped {
                return Loaded {
                    entries,
                    ..Loaded::default()
                };
            }
            // Keep the original next to the cleaned file.
            let _ = fs::write(bad_path(path), &bytes);
            let _ = save(path, &entries);
            recovered(entries)
        }
        _ => {
            if fs::rename(path, bad_path(path)).is_ok() {
                let _ = save(path, &[]);
            }
            recovered(Vec::new())
        }
    }
}

fn recovered(entries: Vec<Entry>) -> Loaded {
    tracing::warn!(
        "dictionary event=recovered kept={} backup={}",
        entries.len(),
        BAD_FILE
    );
    Loaded {
        entries,
        seeded: false,
        recovered: true,
    }
}

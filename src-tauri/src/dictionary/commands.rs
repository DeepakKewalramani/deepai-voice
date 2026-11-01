//! Tauri commands for the Dictionary page. The `*_in` functions hold the
//! logic and take the app data dir, so tests run them on a temp folder.
//! Error strings are fixed sentences and never contain a word or note.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Manager;

use super::store::{self, new_id};
use super::{Entry, EntryInput, MAX_ENTRIES, validate, validate_language, validate_word, word_key};

pub const ERR_DUPLICATE: &str = "This word is already in the dictionary.";
pub const ERR_LIMIT: &str = "The dictionary is full (500 words). Delete some to add more.";
pub const ERR_NOT_FOUND: &str = "That entry no longer exists.";
pub const ERR_SAVE: &str = "Could not save the dictionary. Try again.";
pub const ERR_NO_DIR: &str = "The app data folder is not available.";

use super::store::STORE_LOCK as LOCK;

fn with_entries<T>(
    dir: &Path,
    f: impl FnOnce(&mut Vec<Entry>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = store::path(dir);
    let mut entries = store::load(&path).entries;
    let out = f(&mut entries)?;
    store::save(&path, &entries).map_err(|_| ERR_SAVE.to_string())?;
    Ok(out)
}

fn is_duplicate(entries: &[Entry], word: &str, except_id: Option<&str>) -> bool {
    let key = word_key(word);
    entries
        .iter()
        .any(|e| Some(e.id.as_str()) != except_id && word_key(&e.word) == key)
}

/// Counts reported by "Add many".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddManyReport {
    pub added: usize,
    /// Already in the dictionary or repeated in the pasted text.
    pub duplicates: usize,
    /// Lines that are not a valid word (too long, control characters).
    pub invalid: usize,
    /// Valid new words left out because the dictionary is full.
    pub over_limit: usize,
}

pub fn list_in(dir: &Path) -> Result<Vec<Entry>, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store::load(&store::path(dir)).entries)
}

pub fn add_in(dir: &Path, input: &EntryInput) -> Result<Entry, String> {
    let entry = validate(input, new_id()).map_err(|e| e.message().to_string())?;
    with_entries(dir, |entries| {
        if is_duplicate(entries, &entry.word, None) {
            return Err(ERR_DUPLICATE.into());
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(ERR_LIMIT.into());
        }
        entries.push(entry.clone());
        Ok(entry)
    })
}

pub fn update_in(dir: &Path, id: &str, input: &EntryInput) -> Result<Entry, String> {
    let entry = validate(input, id.to_string()).map_err(|e| e.message().to_string())?;
    with_entries(dir, |entries| {
        let index = entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| ERR_NOT_FOUND.to_string())?;
        if is_duplicate(entries, &entry.word, Some(id)) {
            return Err(ERR_DUPLICATE.into());
        }
        entries[index] = entry.clone();
        Ok(entry)
    })
}

pub fn delete_in(dir: &Path, id: &str) -> Result<(), String> {
    with_entries(dir, |entries| {
        let before = entries.len();
        entries.retain(|e| e.id != id);
        if entries.len() == before {
            return Err(ERR_NOT_FOUND.into());
        }
        Ok(())
    })
}

/// One word per line. Blank lines are ignored; duplicates and invalid lines
/// are counted, not added.
pub fn add_many_in(dir: &Path, text: &str, language: &str) -> Result<AddManyReport, String> {
    let language = validate_language(language).map_err(|e| e.message().to_string())?;
    with_entries(dir, |entries| {
        let mut report = AddManyReport::default();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(word) = validate_word(line) else {
                report.invalid += 1;
                continue;
            };
            if is_duplicate(entries, &word, None) {
                report.duplicates += 1;
                continue;
            }
            if entries.len() >= MAX_ENTRIES {
                report.over_limit += 1;
                continue;
            }
            entries.push(Entry {
                id: new_id(),
                word,
                heared_as: Vec::new(),
                language: language.clone(),
                note: None,
            });
            report.added += 1;
        }
        Ok(report)
    })
}

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| ERR_NO_DIR.to_string())
}

#[tauri::command(async)]
pub fn dictionary_list(app: tauri::AppHandle) -> Result<Vec<Entry>, String> {
    list_in(&data_dir(&app)?)
}

#[tauri::command(async)]
pub fn dictionary_add(app: tauri::AppHandle, entry: EntryInput) -> Result<Entry, String> {
    add_in(&data_dir(&app)?, &entry)
}

#[tauri::command(async)]
pub fn dictionary_update(
    app: tauri::AppHandle,
    id: String,
    entry: EntryInput,
) -> Result<Entry, String> {
    update_in(&data_dir(&app)?, &id, &entry)
}

#[tauri::command(async)]
pub fn dictionary_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    delete_in(&data_dir(&app)?, &id)
}

#[tauri::command(async)]
pub fn dictionary_add_many(
    app: tauri::AppHandle,
    text: String,
    language: String,
) -> Result<AddManyReport, String> {
    add_many_in(&data_dir(&app)?, &text, &language)
}

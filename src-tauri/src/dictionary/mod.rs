//! Personal dictionary: names and terms the speech engines often get wrong.
//!
//! Stored only on this Mac (`dictionary.json` in the app data folder), never
//! sent anywhere and never logged: no word, sound-alike or note ever reaches
//! the log, tracing or an error string. Used three ways per hold:
//! a vocabulary prompt for Whisper (`prompt`), hint phrases for the streaming
//! engine (`prompt::hint_words`) and a correction map applied to every
//! hypothesis before it is typed (`replace`).

pub mod commands;
pub mod prompt;
pub mod replace;
pub mod store;

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Most entries a dictionary may hold.
pub const MAX_ENTRIES: usize = 500;
/// Longest word, in characters.
pub const MAX_WORD_CHARS: usize = 60;
/// Most sound-alike alternatives per entry.
pub const MAX_HEARD_AS: usize = 5;
/// Longest sound-alike alternative, in characters.
pub const MAX_HEARD_AS_CHARS: usize = 60;
/// Longest note, in characters.
pub const MAX_NOTE_CHARS: usize = 80;
/// Language tag for an entry used with every language.
pub const ANY_LANGUAGE: &str = "any";

fn any_language() -> String {
    ANY_LANGUAGE.to_string()
}

/// One dictionary entry, as stored and as sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    /// The correct spelling.
    pub word: String,
    /// What the engines wrongly produce instead of `word`.
    #[serde(default)]
    pub heared_as: Vec<String>,
    /// "any" or an ISO 639-1 code such as "en" or "hi".
    #[serde(default = "any_language")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Entry fields from the UI, before validation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub word: String,
    #[serde(default)]
    pub heared_as: Vec<String>,
    #[serde(default = "any_language")]
    pub language: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Why an entry was refused. Messages are fixed and never contain user text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidEntry {
    WordEmpty,
    WordTooLong,
    WordControl,
    HeardAsTooMany,
    HeardAsTooLong,
    HeardAsControl,
    Language,
    NoteTooLong,
    NoteControl,
}

impl InvalidEntry {
    pub fn message(self) -> &'static str {
        match self {
            Self::WordEmpty => "Enter a word.",
            Self::WordTooLong => "Use at most 60 characters for the word.",
            Self::WordControl => "The word cannot contain tabs or line breaks.",
            Self::HeardAsTooMany => "Use at most 5 sound-alike spellings.",
            Self::HeardAsTooLong => "Each sound-alike spelling can have at most 60 characters.",
            Self::HeardAsControl => "Sound-alike spellings cannot contain tabs or line breaks.",
            Self::Language => "Choose a language.",
            Self::NoteTooLong => "Use at most 80 characters for the note.",
            Self::NoteControl => "The note cannot contain tabs or line breaks.",
        }
    }
}

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// Trims and checks a word.
pub fn validate_word(raw: &str) -> Result<String, InvalidEntry> {
    let word = raw.trim();
    if word.is_empty() {
        return Err(InvalidEntry::WordEmpty);
    }
    if has_control(word) {
        return Err(InvalidEntry::WordControl);
    }
    if word.chars().count() > MAX_WORD_CHARS {
        return Err(InvalidEntry::WordTooLong);
    }
    Ok(word.to_string())
}

/// Trims each alternative, drops empty ones and case-insensitive repeats
/// (the first spelling wins), then checks the limits.
pub fn validate_heard_as(raw: &[String]) -> Result<Vec<String>, InvalidEntry> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if has_control(item) {
            return Err(InvalidEntry::HeardAsControl);
        }
        if item.chars().count() > MAX_HEARD_AS_CHARS {
            return Err(InvalidEntry::HeardAsTooLong);
        }
        let key = item.to_lowercase();
        if !out.iter().any(|o| o.to_lowercase() == key) {
            out.push(item.to_string());
        }
    }
    if out.len() > MAX_HEARD_AS {
        return Err(InvalidEntry::HeardAsTooMany);
    }
    Ok(out)
}

/// "any" or a 2 or 3 letter lowercase ISO code.
pub fn validate_language(raw: &str) -> Result<String, InvalidEntry> {
    let ok = raw == ANY_LANGUAGE
        || ((2..=3).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_lowercase()));
    if ok {
        Ok(raw.to_string())
    } else {
        Err(InvalidEntry::Language)
    }
}

/// Trims a note; empty means none.
pub fn validate_note(raw: Option<&str>) -> Result<Option<String>, InvalidEntry> {
    let note = raw.unwrap_or("").trim();
    if note.is_empty() {
        return Ok(None);
    }
    if has_control(note) {
        return Err(InvalidEntry::NoteControl);
    }
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(InvalidEntry::NoteTooLong);
    }
    Ok(Some(note.to_string()))
}

/// Validates every field and builds the entry with `id`.
pub fn validate(input: &EntryInput, id: String) -> Result<Entry, InvalidEntry> {
    Ok(Entry {
        id,
        word: validate_word(&input.word)?,
        heared_as: validate_heard_as(&input.heared_as)?,
        language: validate_language(&input.language)?,
        note: validate_note(input.note.as_deref())?,
    })
}

/// Case-insensitive key used for duplicate words.
pub fn word_key(word: &str) -> String {
    word.trim().to_lowercase()
}

/// Primary language subtag of a BCP-47 locale or ISO code: "en-IN" -> "en".
pub fn language_of(locale: &str) -> String {
    locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// True when an entry tagged `entry_language` applies to `active`.
pub fn applies_to(entry_language: &str, active: &str) -> bool {
    entry_language == ANY_LANGUAGE || entry_language == language_of(active)
}

/// What one hold uses from the dictionary, read once at hold start.
#[derive(Debug, Clone, Default)]
pub struct HoldVocab {
    /// Vocabulary prompt for Whisper; empty when there is none.
    pub prompt: String,
    /// Phrases for the streaming engine (at most 100).
    pub hints: Vec<String>,
    /// Correction map for this hold's language.
    pub replacer: replace::Replacer,
}

impl HoldVocab {
    /// Builds the vocabulary of `entries` for the active `language`.
    pub fn from_entries(entries: &[Entry], language: &str) -> Self {
        let active: Vec<Entry> = entries
            .iter()
            .filter(|e| applies_to(&e.language, language))
            .cloned()
            .collect();
        Self {
            prompt: prompt::build_prompt(&active, language),
            hints: prompt::hint_words(&active, language),
            replacer: replace::Replacer::new(&active),
        }
    }

    /// Reads the dictionary in `dir` (no dir: empty). Never fails: an
    /// unreadable dictionary means no hints for this hold.
    pub fn load(dir: Option<&Path>, language: &str) -> Self {
        match dir {
            Some(dir) => {
                let _guard = store::STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                Self::from_entries(&store::load(&store::path(dir)).entries, language)
            }
            None => Self::default(),
        }
    }

    /// Whisper prompt, or None when empty.
    pub fn whisper_hint(&self) -> Option<&str> {
        (!self.prompt.is_empty()).then_some(self.prompt.as_str())
    }

    /// Applies the correction map to one hypothesis.
    pub fn correct(&self, text: &str) -> String {
        self.replacer.apply(text)
    }
}

//! Filler word filtering and transcript cleanup.

use regex::Regex;
use std::sync::LazyLock;

static FILLER_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches standalone fillers with optional preceding or succeeding commas and spaces
    Regex::new(r"(?i)(?:,\s*)?\b(um|uh|ah|er|hmm)\b(?:,\s*)?").expect("valid filler regex")
});

static MULTI_SPACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid space regex"));

static CLEAN_PUNCT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+([,.!?])").expect("valid punct regex"));

/// Filters standalone conversational fillers from the input text while preserving
/// words containing filler substrings (e.g. umbrella, human, Uhura).
pub fn filter_fillers(text: &str) -> String {
    let replaced = FILLER_RE.replace_all(text, " ");
    let spaces_cleaned = MULTI_SPACE_RE.replace_all(&replaced, " ");
    let punct_cleaned = CLEAN_PUNCT_RE.replace_all(&spaces_cleaned, "$1");
    let trimmed = punct_cleaned.trim();

    if trimmed.is_empty() {
        return String::new();
    }

    // Ensure first character is capitalized if ASCII
    let mut chars = trimmed.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

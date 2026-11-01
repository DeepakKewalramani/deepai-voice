//! Vocabulary hints built from dictionary entries. Pure: entries in, text out.
//! Only the correct words are used, never the sound-alike spellings, so the
//! engines are never nudged toward the wrong word.

use super::{Entry, applies_to, word_key};

/// Prompt prefix. Whisper treats the prompt as earlier text, so a short
/// label plus a comma list biases it toward these spellings.
pub const PROMPT_PREFIX: &str = "Vocabulary: ";
/// Most words (whitespace separated, across all terms) in the prompt.
pub const MAX_PROMPT_WORDS: usize = 40;
/// Longest prompt, in characters, prefix included.
pub const MAX_PROMPT_CHARS: usize = 400;
/// Most phrases given to the streaming engine (its documented limit).
pub const MAX_HINTS: usize = 100;

/// Words of `entries` for `language` ("any" entries included), trimmed,
/// first spelling of each case-insensitive duplicate kept, in entry order.
fn words_for<'a>(entries: &'a [Entry], language: &str) -> Vec<&'a str> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| applies_to(&e.language, language)) {
        let word = e.word.trim();
        let key = word_key(word);
        if word.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(word);
    }
    out
}

/// Whisper vocabulary prompt such as "Vocabulary: Deepak, DeepakKewalramani".
/// Empty when no word applies. Capped at `MAX_PROMPT_WORDS` words and
/// `MAX_PROMPT_CHARS` characters; a term that does not fit is skipped.
pub fn build_prompt(entries: &[Entry], language: &str) -> String {
    let mut prompt = String::from(PROMPT_PREFIX);
    let mut chars = PROMPT_PREFIX.chars().count();
    let mut words = 0;
    let mut terms = 0;
    for term in words_for(entries, language) {
        let term_words = term.split_whitespace().count();
        let sep = if terms == 0 { 0 } else { 2 };
        let term_chars = term.chars().count() + sep;
        if words + term_words > MAX_PROMPT_WORDS || chars + term_chars > MAX_PROMPT_CHARS {
            continue;
        }
        if terms > 0 {
            prompt.push_str(", ");
        }
        prompt.push_str(term);
        words += term_words;
        chars += term_chars;
        terms += 1;
    }
    if terms == 0 { String::new() } else { prompt }
}

/// Phrases for the streaming engine: the words for `language`, deduplicated,
/// at most `MAX_HINTS`.
pub fn hint_words(entries: &[Entry], language: &str) -> Vec<String> {
    words_for(entries, language)
        .into_iter()
        .take(MAX_HINTS)
        .map(str::to_string)
        .collect()
}

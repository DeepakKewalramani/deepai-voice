//! Whisper hallucination detection filters.

use std::collections::HashSet;
use std::sync::LazyLock;

static KNOWN_HALLUCINATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "[blank_audio]",
        "[silence]",
        "(silence)",
        "[music]",
        "(music)",
        "[applause]",
        "(applause)",
        "thank you",
        "thank you very much",
        "thank you so much",
        "thanks for watching",
        "please subscribe",
        "subtitles by the amara.org community",
        "subtitles by",
    ]
    .into_iter()
    .collect()
});

fn is_trailing_punct(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | '!' | '?' | ',' | ';' | ':')
}

/// Lowercased words with surrounding punctuation removed, empties dropped.
fn normalized_words(lower: &str) -> Vec<String> {
    lower
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// True if the whole word list is one phrase repeated back to back.
fn is_known_or_long_phrase_loop(words: &[String]) -> bool {
    for k in 1..=4 {
        if words.len() < 2 * k || !words.len().is_multiple_of(k) {
            continue;
        }
        let phrase = &words[..k];
        if !words.chunks(k).all(|c| c == phrase) {
            continue;
        }
        let reps = words.len() / k;
        let min_reps = if k == 1 { 5 } else { 4 };
        if reps >= min_reps || KNOWN_HALLUCINATIONS.contains(phrase.join(" ").as_str()) {
            return true;
        }
    }
    false
}

/// Returns true if the transcript string matches known Whisper hallucinations or loops.
pub fn is_hallucination(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    // Nothing but punctuation or symbols (".", "?", "...").
    if !lower.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }

    let key = lower.trim_end_matches(is_trailing_punct);
    if KNOWN_HALLUCINATIONS.contains(key) || lower.starts_with("subtitles by") {
        return true;
    }

    let words = normalized_words(&lower);
    if is_known_or_long_phrase_loop(&words) {
        return true;
    }

    // Repeated word loops (e.g. "word word word word word...")
    if words.len() >= 5 {
        let identical = words.iter().filter(|w| **w == words[0]).count();
        if identical * 100 / words.len() >= 80 {
            return true;
        }
    }

    false
}

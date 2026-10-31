//! Whisper hallucination detection filters.

use std::collections::HashSet;
use std::sync::LazyLock;

static KNOWN_HALLUCINATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    s.insert("[blank_audio]");
    s.insert("[silence]");
    s.insert("(silence)");
    s.insert("[music]");
    s.insert("(music)");
    s.insert("[applause]");
    s.insert("(applause)");
    s.insert("thank you.");
    s.insert("thank you");
    s.insert("thank you very much.");
    s.insert("thank you very much");
    s.insert("thank you so much.");
    s.insert("thank you so much");
    s.insert("thanks for watching.");
    s.insert("thanks for watching!");
    s.insert("thanks for watching");
    s.insert("please subscribe.");
    s.insert("please subscribe");
    s.insert("subtitles by the amara.org community");
    s.insert("subtitles by");
    s
});

/// Returns true if the transcript string matches known Whisper hallucinations or loops.
pub fn is_hallucination(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    if KNOWN_HALLUCINATIONS.contains(lower.as_str()) {
        return true;
    }

    if lower.starts_with("subtitles by") {
        return true;
    }

    // Check for repetitive punctuation loops (e.g. "..............")
    if trimmed.len() >= 10 && trimmed.chars().all(|c| c == '.' || c == '-' || c == ' ') {
        return true;
    }

    // Check for repeated word loops (e.g. "word word word word word...")
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 5 {
        let first = words[0];
        let identical_count = words
            .iter()
            .filter(|&&w| w.eq_ignore_ascii_case(first))
            .count();
        if identical_count * 100 / words.len() >= 80 {
            return true;
        }
    }

    false
}

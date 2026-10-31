//! Filler word filtering and transcript cleanup.

use regex::Regex;
use std::sync::LazyLock;

const FILLERS: [&str; 5] = ["um", "uh", "ah", "er", "hmm"];

static CLEAN_PUNCT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+([,.!?])").expect("valid punct regex"));

fn is_punct(c: char) -> bool {
    matches!(c, ',' | '.' | '!' | '?' | ';' | ':')
}

/// Filters standalone conversational fillers from the input text while preserving
/// words containing filler substrings (e.g. umbrella, human, Uhura, uh-huh).
/// Returns an empty string when no alphanumeric content remains.
pub fn filter_fillers(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut capitalize_next = false;

    for token in text.split_whitespace() {
        let core = token.trim_end_matches(is_punct);
        let trailing = &token[core.len()..];
        let is_filler = FILLERS.iter().any(|f| core.eq_ignore_ascii_case(f));
        if !is_filler {
            let mut t = token.to_string();
            if capitalize_next {
                let mut chars = t.chars();
                if let Some(first) = chars.next() {
                    t = first.to_uppercase().collect::<String>() + chars.as_str();
                }
                capitalize_next = false;
            }
            out.push(t);
            continue;
        }
        // Drop the filler and the comma that preceded it; keep sentence punctuation.
        let keep: String = trailing.chars().filter(|&c| c != ',').collect();
        match out.last_mut() {
            Some(prev) => {
                if prev.ends_with(',') {
                    prev.pop();
                }
                prev.push_str(&keep);
            }
            None => capitalize_next = true,
        }
    }

    let joined = out.join(" ");
    let cleaned = CLEAN_PUNCT_RE.replace_all(&joined, "$1");
    let trimmed = cleaned.trim();
    if !trimmed.chars().any(|c| c.is_alphanumeric()) {
        return String::new();
    }
    trimmed.to_string()
}

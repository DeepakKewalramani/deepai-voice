//! Correction map: rewrites sound-alike spellings to the dictionary word.
//! Pure, no I/O. Whole words only, case-insensitive, Unicode aware (accents,
//! Devanagari vowel signs and other combining marks belong to the word).
//! One pass over the input: a replacement's output is never matched again,
//! so replacements never chain.

use std::collections::HashMap;

use super::Entry;

/// Combining marks and joiners that belong to the word before them. Covers
/// Latin accents, Devanagari and Bengali signs and the general combining
/// blocks. An approximation, enough for word boundaries.
fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A
        | 0x064B..=0x065F | 0x0900..=0x0903 | 0x093A..=0x094F | 0x0951..=0x0957
        | 0x0962..=0x0963 | 0x0981..=0x0983 | 0x09BC..=0x09D7 | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF | 0x200C..=0x200D | 0x20D0..=0x20FF | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// Byte ranges of the words in `text`. An apostrophe between two word
/// characters stays inside the word ("don't" is one word).
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (k, &(i, c)) in chars.iter().enumerate() {
        let inner_apostrophe = is_apostrophe(c)
            && start.is_some()
            && chars.get(k + 1).is_some_and(|&(_, n)| is_word_char(n));
        if is_word_char(c) || inner_apostrophe {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            spans.push((s, i));
        }
    }
    if let Some(s) = start {
        spans.push((s, text.len()));
    }
    spans
}

fn lower_words(text: &str) -> Vec<String> {
    word_spans(text)
        .into_iter()
        .map(|(s, e)| text[s..e].to_lowercase())
        .collect()
}

/// True when the text before a match starts a sentence.
fn at_sentence_start(before: &str) -> bool {
    match before.trim_end().chars().last() {
        None => true,
        Some(c) => matches!(c, '.' | '!' | '?' | '\u{0964}' | '\u{0965}'),
    }
}

/// `word` with an upper-case first letter.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[derive(Debug, Clone)]
struct Pattern {
    /// Lower-case words of the sound-alike spelling.
    words: Vec<String>,
    replacement: String,
}

/// Compiled correction map for a set of entries.
#[derive(Debug, Clone, Default)]
pub struct Replacer {
    /// Patterns by their first word, longest first, then entry order.
    by_first: HashMap<String, Vec<Pattern>>,
}

impl Replacer {
    pub fn new(entries: &[Entry]) -> Self {
        let mut by_first: HashMap<String, Vec<Pattern>> = HashMap::new();
        for entry in entries {
            let replacement = entry.word.trim();
            if replacement.is_empty() {
                continue;
            }
            for heard in &entry.heared_as {
                let words = lower_words(heard);
                let Some(first) = words.first().cloned() else {
                    continue;
                };
                by_first.entry(first).or_default().push(Pattern {
                    words,
                    replacement: replacement.to_string(),
                });
            }
        }
        for patterns in by_first.values_mut() {
            // Stable: equal lengths keep entry order, so the first entry wins.
            patterns.sort_by_key(|p| std::cmp::Reverse(p.words.len()));
        }
        Self { by_first }
    }

    pub fn is_empty(&self) -> bool {
        self.by_first.is_empty()
    }

    /// Rewrites every whole-word, case-insensitive sound-alike in `text`.
    /// Multi-word spellings match across plain spaces only. At a sentence
    /// start, a capitalized match keeps a capital first letter.
    pub fn apply(&self, text: &str) -> String {
        if self.is_empty() {
            return text.to_string();
        }
        let spans = word_spans(text);
        let lower: Vec<String> = spans
            .iter()
            .map(|&(s, e)| text[s..e].to_lowercase())
            .collect();
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        let mut i = 0;
        while i < spans.len() {
            let found = self.by_first.get(&lower[i]).and_then(|patterns| {
                patterns.iter().find(|p| {
                    let n = p.words.len();
                    i + n <= spans.len()
                        && (0..n).all(|k| lower[i + k] == p.words[k])
                        && (1..n).all(|k| {
                            text[spans[i + k - 1].1..spans[i + k].0]
                                .chars()
                                .all(char::is_whitespace)
                        })
                })
            });
            let Some(p) = found else {
                i += 1;
                continue;
            };
            let (start, _) = spans[i];
            let (_, end) = spans[i + p.words.len() - 1];
            out.push_str(&text[copied..start]);
            let matched_upper = text[start..].chars().next().is_some_and(char::is_uppercase);
            // Only a plain lowercase word is capitalised at a sentence start;
            // a spelling the user chose with capitals (iPhone) is kept as typed.
            let plain_lower = !p.replacement.chars().any(char::is_uppercase);
            if matched_upper && plain_lower && at_sentence_start(&text[..start]) {
                out.push_str(&capitalized(&p.replacement));
            } else {
                out.push_str(&p.replacement);
            }
            copied = end;
            i += p.words.len();
        }
        out.push_str(&text[copied..]);
        out
    }
}

/// Rewrites sound-alike spellings of `entries` in `text` to their words.
/// See [`Replacer::apply`]. Compile a [`Replacer`] once to reuse it.
pub fn apply_replacements(text: &str, entries: &[Entry]) -> String {
    Replacer::new(entries).apply(text)
}

//! Pure live-typing logic. No I/O and no logging: text in, edits out.
//!
//! Each partial hypothesis of the open segment is compared with the previous
//! one; only words both agree on are typed, and the newest last word is always
//! held back because it is often cut mid-word. When the segment's final text
//! arrives, the typed text is reconciled with it: the remainder is typed, or the
//! mismatched tail is removed with Backspace and the correct text typed.

/// One keyboard edit: press Backspace `backspaces` times, then type `text`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edit {
    pub backspaces: usize,
    pub text: String,
}

/// True for a char that joins the previous one into a single visible
/// character: combining marks, variation selectors, skin tone modifiers,
/// zero width joiner. An approximation of grapheme clusters, enough for
/// counting Backspace presses.
fn extends_cluster(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x1F3FB..=0x1F3FF | 0x200D
        | 0xE0020..=0xE007F)
}

/// Splits `s` into user-visible characters (approximate grapheme clusters).
pub fn clusters(s: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut prev_zwj = false;
    for (i, c) in s.char_indices() {
        let joins = i > 0 && (extends_cluster(c) || prev_zwj);
        if i > 0 && !joins {
            out.push(&s[start..i]);
            start = i;
        }
        prev_zwj = c == '\u{200d}';
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Number of Backspace presses that remove `s`.
pub fn cluster_count(s: &str) -> usize {
    clusters(s).len()
}

/// Words both hypotheses agree on, from the start, never including the last
/// word of `newest`. Joined with single spaces.
pub fn stable_text(previous: &str, newest: &str) -> String {
    let prev: Vec<&str> = previous.split_whitespace().collect();
    let new: Vec<&str> = newest.split_whitespace().collect();
    let cap = new.len().saturating_sub(1);
    let agreed = prev
        .iter()
        .zip(&new)
        .take(cap)
        .take_while(|(a, b)| a == b)
        .count();
    new[..agreed].join(" ")
}

/// Edit that turns `typed` into `target`, keeping their common start.
/// Backspaces are counted in visible characters and never split one.
pub fn reconcile(typed: &str, target: &str) -> Edit {
    let a = clusters(typed);
    let b = clusters(target);
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    Edit {
        backspaces: a.len() - common,
        text: b[common..].concat(),
    }
}

/// Typing state for one segment of a hold.
#[derive(Debug, Clone)]
pub struct SegmentTyping {
    /// A single space goes before this segment's text (an earlier segment of
    /// the same hold typed something).
    lead_space: bool,
    /// Exactly what this segment has typed so far, including the lead space.
    typed: String,
    previous: String,
}

impl SegmentTyping {
    pub fn new(lead_space: bool) -> Self {
        Self {
            lead_space,
            typed: String::new(),
            previous: String::new(),
        }
    }

    pub fn typed(&self) -> &str {
        &self.typed
    }

    fn with_lead(&self, body: &str) -> String {
        if body.is_empty() {
            String::new()
        } else if self.lead_space {
            format!(" {body}")
        } else {
            body.to_string()
        }
    }

    /// Feeds a new partial hypothesis. Returns text to type when the agreed
    /// words extend what is already typed. Never deletes.
    pub fn on_hypothesis(&mut self, hypothesis: &str) -> Option<String> {
        let stable = stable_text(&self.previous, hypothesis);
        self.previous = hypothesis.to_string();
        let target = self.with_lead(&stable);
        if target.len() > self.typed.len() && target.starts_with(&self.typed) {
            let add = target[self.typed.len()..].to_string();
            self.typed = target;
            Some(add)
        } else {
            None
        }
    }

    /// Part of `hypothesis` not typed yet, for the live preview.
    pub fn preview(&self, hypothesis: &str) -> String {
        let words = hypothesis.split_whitespace().collect::<Vec<_>>().join(" ");
        let typed = self.typed.trim_start();
        match words.strip_prefix(typed) {
            Some(rest) if !typed.is_empty() => rest.trim_start().to_string(),
            _ => words,
        }
    }

    /// Reconciles typed text with the segment's final text (`None` when the
    /// final is empty or filtered). Backspaces stay within this segment.
    pub fn on_final(&mut self, final_text: Option<&str>) -> Edit {
        let target = self.with_lead(final_text.unwrap_or("").trim());
        let edit = reconcile(&self.typed, &target);
        self.typed = target;
        self.previous.clear();
        edit
    }
}

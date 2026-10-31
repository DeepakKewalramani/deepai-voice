use regex::Regex;
use std::sync::LazyLock;

static ANTHROPIC_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"sk-ant-[a-zA-Z0-9_\-]+").expect("valid regex for Anthropic API key")
});

static GENERIC_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"sk-[a-zA-Z0-9_\-]{16,}").expect("valid regex for generic API key")
});

static BEARER_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{10,}").expect("valid regex for Bearer token")
});

/// Redacts sensitive API keys and tokens from a string.
pub fn redact_secrets(input: &str) -> String {
    let s = ANTHROPIC_KEY_RE.replace_all(input, "[REDACTED_KEY]");
    let s = GENERIC_KEY_RE.replace_all(&s, "[REDACTED_KEY]");
    let s = BEARER_TOKEN_RE.replace_all(&s, "Bearer [REDACTED_KEY]");
    s.into_owned()
}

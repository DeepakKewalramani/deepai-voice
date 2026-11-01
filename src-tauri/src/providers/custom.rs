//! "Make your own" provider (slice 2.14): a validated, pure description of a
//! user-defined endpoint. Error messages are fixed plain-English strings and never
//! echo the input, so a pasted secret cannot leak through an error.
//! The same rules live in `src/settings/providers/validate.ts`; both sides run
//! `src/settings/providers/validation-vectors.json`.

use super::{AuthStyle, Capability};
use serde::Deserialize;
use std::borrow::Cow;

pub const NAME_MAX: usize = 40;
pub const MODEL_MAX: usize = 80;
pub const HEADER_MAX: usize = 64;
pub const URL_MAX: usize = 2048;

/// What the UI sends. Every field is checked by `validate`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomProviderForm {
    /// Set when editing an existing custom provider.
    pub id: Option<String>,
    pub capability: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// `bearer` (default when empty), `token`, `x-api-key`, `none` or `custom`.
    pub auth_style: String,
    /// Header name when `auth_style` is `custom`.
    pub header_name: String,
    /// `openai-compatible` (default when empty).
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomProvider {
    pub capability: Capability,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub auth: AuthStyle,
}

impl CustomProvider {
    /// True for endpoints on this Mac (localhost, 127.0.0.1, ::1).
    pub fn is_local(&self) -> bool {
        is_local_url(&self.base_url)
    }
}

/// A rejected field and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: &'static str,
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}

fn err(field: &'static str, message: &'static str) -> FieldError {
    FieldError { field, message }
}

pub fn validate(form: &CustomProviderForm) -> Result<CustomProvider, FieldError> {
    let capability = Capability::from_id(form.capability.trim()).ok_or(err(
        "capability",
        "Choose speech to text, language model or voice.",
    ))?;
    let name = validate_name(&form.name).map_err(|m| err("name", m))?;
    let base_url = validate_base_url(&form.base_url).map_err(|m| err("baseUrl", m))?;
    let model = validate_model(&form.model).map_err(|m| err("model", m))?;
    let auth = match form.auth_style.trim() {
        "" | "bearer" => AuthStyle::Bearer,
        "token" => AuthStyle::Token,
        "x-api-key" => AuthStyle::XApiKey,
        "none" => AuthStyle::None,
        "custom" => {
            let h = validate_header_name(&form.header_name).map_err(|m| err("headerName", m))?;
            AuthStyle::Header(Cow::Owned(h))
        }
        _ => return Err(err("authStyle", "Choose how the key is sent.")),
    };
    match form.protocol.trim() {
        "" | "openai-compatible" => {}
        _ => {
            return Err(err(
                "protocol",
                "Only OpenAI-compatible endpoints are supported for now.",
            ));
        }
    }
    Ok(CustomProvider {
        capability,
        name,
        base_url,
        model,
        auth,
    })
}

/// 1 to 40 characters after trimming, no control characters.
pub fn validate_name(name: &str) -> Result<String, &'static str> {
    let t = name.trim();
    if t.is_empty() {
        return Err("Enter a name.");
    }
    if t.chars().count() > NAME_MAX {
        return Err("Use 40 characters or fewer for the name.");
    }
    if t.chars().any(char::is_control) {
        return Err("The name cannot contain line breaks or tabs.");
    }
    Ok(t.to_owned())
}

/// 1 to 80 characters, no spaces or control characters.
pub fn validate_model(model: &str) -> Result<String, &'static str> {
    let t = model.trim();
    if t.is_empty() {
        return Err("Enter the model id, for example llama3.2.");
    }
    if t.chars().count() > MODEL_MAX {
        return Err("Use 80 characters or fewer for the model id.");
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The model id cannot contain spaces.");
    }
    Ok(t.to_owned())
}

/// A legal HTTP header name (RFC 9110 token), not one the HTTP client manages.
pub fn validate_header_name(name: &str) -> Result<String, &'static str> {
    let t = name.trim();
    if t.is_empty() {
        return Err("Enter the header name, for example api-subscription-key.");
    }
    if t.len() > HEADER_MAX {
        return Err("Use 64 characters or fewer for the header name.");
    }
    let tchar = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
    if !t.chars().all(tchar) {
        return Err("Use only letters, digits and - _ . in the header name.");
    }
    const RESERVED: [&str; 7] = [
        "host",
        "content-length",
        "content-type",
        "transfer-encoding",
        "connection",
        "cookie",
        "user-agent",
    ];
    if RESERVED.contains(&t.to_ascii_lowercase().as_str()) {
        return Err("That header is managed by the app. Choose another name.");
    }
    Ok(t.to_owned())
}

const LOCAL_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// Checks and normalises a base URL. https only, or http for localhost,
/// 127.0.0.1 and ::1. No credentials, query or fragment. Lowercases scheme
/// and host, removes trailing slashes.
pub fn validate_base_url(raw: &str) -> Result<String, &'static str> {
    const GENERIC: &str =
        "Enter a full address that starts with https://, for example https://api.openai.com/v1.";
    let s = raw.trim();
    if s.is_empty() {
        return Err(GENERIC);
    }
    if s.len() > URL_MAX {
        return Err("The address is too long.");
    }
    if s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err("The address cannot contain spaces or backslashes.");
    }
    if s.contains('?') || s.contains('#') {
        return Err("Remove the ? or # part. Keys never go in the address.");
    }
    let (scheme, rest) = s.split_once("://").ok_or(GENERIC)?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(GENERIC);
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err("Remove the user name or password from the address. Keys go in the key field.");
    }
    let (host, port) = split_host_port(authority)?;
    let host = host.to_ascii_lowercase();
    if scheme == "http" && !LOCAL_HOSTS.contains(&host.as_str()) {
        return Err("Use https://. Plain http:// is allowed only for localhost, 127.0.0.1 or ::1.");
    }
    let path_ok = |c: char| c.is_ascii_alphanumeric() || "-._~/%:+@!$&'()*,;=".contains(c);
    if !path.chars().all(path_ok) {
        return Err("The address path has characters that are not allowed.");
    }
    let path = path.trim_end_matches('/');
    let port = port.map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{scheme}://{host}{port}{path}"))
}

fn split_host_port(authority: &str) -> Result<(&str, Option<u16>), &'static str> {
    const BAD_HOST: &str = "The server name in the address is not valid.";
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or(BAD_HOST)?;
        let inner = &rest[..end];
        if inner.is_empty()
            || !inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return Err(BAD_HOST);
        }
        let after = &rest[end + 1..];
        let port = match after {
            "" => None,
            p => Some(p.strip_prefix(':').ok_or(BAD_HOST)?),
        };
        (&authority[..end + 2], port)
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(BAD_HOST);
    }
    if !host.starts_with('[') {
        let label_ok = |l: &str| {
            !l.is_empty()
                && l.len() <= 63
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        };
        if host.len() > 253 || !host.split('.').all(label_ok) {
            return Err(BAD_HOST);
        }
    }
    let port = match port {
        None => None,
        Some(p) => {
            let n: u16 = p
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("The port must be a number from 1 to 65535.")?;
            Some(n)
        }
    };
    Ok((host, port))
}

/// True when a normalised base URL points at this Mac.
pub fn is_local_url(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    split_host_port(authority)
        .map(|(h, _)| LOCAL_HOSTS.contains(&h.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

//! Provider hub (slices 2.13 and 2.14): the catalog of speech, language model and
//! voice providers, the user's own ("custom") providers, and the Tauri commands that
//! connect them. Keys live only in the OS credential store (`settings::keys`);
//! settings.json holds a `keyRef` name. The only network call is `http::UreqProbe`,
//! run when the user presses Test connection.

pub mod catalog;
pub mod commands;
pub mod custom;
pub mod http;
pub mod hub;
pub mod probe;

use serde::{Serialize, Serializer};
use std::borrow::Cow;

/// What a provider does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Stt,
    Llm,
    Tts,
}

impl Capability {
    pub fn id(self) -> &'static str {
        match self {
            Self::Stt => "stt",
            Self::Llm => "llm",
            Self::Tts => "tts",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "stt" => Some(Self::Stt),
            "llm" => Some(Self::Llm),
            "tts" => Some(Self::Tts),
            _ => None,
        }
    }
}

/// Yes, no, or not verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tri {
    Yes,
    No,
    Unknown,
}

/// How far a catalog entry is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatus {
    /// Ships with the app, nothing to connect.
    Builtin,
    /// Facts checked against the vendor's docs (research R.4).
    Verified,
    /// Listed so the user knows it is coming; cannot be connected yet.
    Candidate,
}

/// How the user turns a catalog entry on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Setup {
    /// Nothing to set up (built in) or not allowed yet (candidate).
    None,
    /// Paste an API key.
    Key,
    /// Needs a base URL and model, so it goes through the "Make your own" form.
    Custom,
}

/// How an API key is sent to the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStyle {
    /// `Authorization: Bearer <key>`
    Bearer,
    /// `Authorization: Token <key>`
    Token,
    /// `x-api-key: <key>`
    XApiKey,
    /// `<name>: <key>` with a validated header name.
    Header(Cow<'static, str>),
    /// No key (local servers).
    None,
    /// Not recorded in our research yet. Catalog only, never accepted from the UI.
    Unknown,
}

impl AuthStyle {
    /// The string stored in settings.json and sent to the UI.
    pub fn as_setting(&self) -> String {
        match self {
            Self::Bearer => "bearer".into(),
            Self::Token => "token".into(),
            Self::XApiKey => "x-api-key".into(),
            Self::Header(name) => format!("header:{name}"),
            Self::None => "none".into(),
            Self::Unknown => "unknown".into(),
        }
    }

    pub fn needs_key(&self) -> bool {
        !matches!(self, Self::None)
    }
}

impl Serialize for AuthStyle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_setting())
    }
}

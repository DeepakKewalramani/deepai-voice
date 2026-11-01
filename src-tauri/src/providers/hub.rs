//! Provider hub logic over settings.json and a `KeyStore`. Pure of Tauri so the
//! tests run it with a fake store. Keys go only to the store under
//! `provider:<id>`; results and errors never contain a key.

use super::catalog::{self, CatalogEntry};
use super::custom::{self, CustomProviderForm};
use super::probe::{
    HttpProbe, ProbeTarget, REASON_NO_CLIENT, REASON_NO_FREE_CHECK, REASON_NO_KEY, TestResult,
};
use super::{AuthStyle, Capability, Setup};
use crate::settings::keys::KeyStore;
use crate::settings::{self, Provider, Settings};
use serde::Serialize;
use std::borrow::Cow;
use std::path::PathBuf;

pub const KEY_REF_PREFIX: &str = "provider:";
pub const CUSTOM_PREFIX: &str = "custom-";
pub const OPENAI_COMPATIBLE: &str = "openai_compatible";
const KEY_MAX: usize = 512;

/// Credential store account for a provider id.
pub fn key_ref(id: &str) -> String {
    format!("{KEY_REF_PREFIX}{id}")
}

/// Last 4 characters, only for keys of 8 or more (same rule as the UI's `keyTail`).
pub fn key_tail(key: &str) -> Option<String> {
    let n = key.chars().count();
    (n >= 8).then(|| key.chars().skip(n - 4).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Set,
    NotSet,
    NotNeeded,
}

/// A configured provider as the UI sees it. Never holds a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub kind: String,
    pub adapter: String,
    pub model: String,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub auth_style: Option<String>,
    pub custom: bool,
    pub connected: bool,
    pub key: KeyState,
    pub key_tail: Option<String>,
}

pub struct Hub<'a> {
    path: PathBuf,
    store: &'a dyn KeyStore,
}

const READ_ONLY: &str =
    "Settings could not be changed: the settings file is from a newer version or unreadable.";
const SAVE_FAILED: &str = "Could not save settings.";
const STORE_FAILED: &str = "Could not reach the macOS Keychain. The key was not saved.";
const STORE_DELETE_FAILED: &str = "Could not remove the key from the macOS Keychain.";
const BAD_KEY: &str = "Paste the API key. It cannot be empty or contain spaces.";
const UNKNOWN_PROVIDER: &str = "That provider is not in the list.";

impl<'a> Hub<'a> {
    pub fn new(path: PathBuf, store: &'a dyn KeyStore) -> Self {
        Self { path, store }
    }

    fn load(&self) -> Result<Settings, String> {
        let r = settings::load(&self.path);
        if r.read_only {
            return Err(READ_ONLY.into());
        }
        Ok(r.settings)
    }

    fn save(&self, s: &Settings) -> Result<(), String> {
        settings::save(&self.path, s, false).map_err(|_| SAVE_FAILED.to_owned())
    }

    fn status_of(&self, p: &Provider) -> ProviderStatus {
        let (key, key_tail) = match &p.key_ref {
            None => (KeyState::NotNeeded, None),
            Some(r) => match self.store.get_key(r) {
                Ok(Some(k)) => (KeyState::Set, self::key_tail(&k)),
                _ => (KeyState::NotSet, None),
            },
        };
        ProviderStatus {
            id: p.id.clone(),
            kind: p.kind.clone(),
            adapter: p.adapter.clone(),
            model: p.model.clone(),
            name: p.name.clone(),
            base_url: p.base_url.clone(),
            auth_style: p.auth_style.clone(),
            custom: p.id.starts_with(CUSTOM_PREFIX),
            connected: key != KeyState::NotSet,
            key,
            key_tail,
        }
    }

    /// Every provider in settings.json with its connected state.
    pub fn status(&self) -> Result<Vec<ProviderStatus>, String> {
        let s = settings::load(&self.path).settings;
        Ok(s.providers.iter().map(|p| self.status_of(p)).collect())
    }

    /// Stores the key for a catalog entry and writes its settings entry.
    pub fn connect(&self, id: &str, key: &str) -> Result<ProviderStatus, String> {
        let entry = catalog::find(id).ok_or(UNKNOWN_PROVIDER)?;
        if entry.setup != Setup::Key {
            return Err(match entry.setup {
                Setup::Custom => "Add this one with Make your own.".into(),
                _ => "This provider cannot be connected yet.".into(),
            });
        }
        let key = check_key(key)?;
        let mut s = self.load()?;
        let provider = catalog_provider(entry);
        self.store_then_save(&mut s, provider, Some(key))
    }

    /// Writes the key (if any), then settings. Rolls the key back if settings fail.
    fn store_then_save(
        &self,
        s: &mut Settings,
        provider: Provider,
        key: Option<&str>,
    ) -> Result<ProviderStatus, String> {
        // Remember the key being replaced, so a failed settings save can put it back.
        let old_key = match (&provider.key_ref, key) {
            (Some(r), Some(_)) => self.store.get_key(r).ok().flatten(),
            _ => None,
        };
        if let (Some(r), Some(k)) = (&provider.key_ref, key) {
            self.store
                .set_key(r, k)
                .map_err(|_| STORE_FAILED.to_owned())?;
        }
        let status_provider = provider.clone();
        match s.providers.iter_mut().find(|p| p.id == provider.id) {
            Some(existing) => *existing = provider,
            None => s.providers.push(provider),
        }
        if let Err(e) = self.save(s) {
            if let (Some(r), Some(_)) = (&status_provider.key_ref, key) {
                match &old_key {
                    Some(old) => {
                        let _ = self.store.set_key(r, old);
                    }
                    None => {
                        let _ = self.store.delete_key(r);
                    }
                }
            }
            return Err(e);
        }
        Ok(self.status_of(&status_provider))
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        let mut s = self.load()?;
        let Some(pos) = s.providers.iter().position(|p| p.id == id) else {
            return Ok(());
        };
        let removed = s.providers.remove(pos);
        for chain in [&mut s.chains.stt, &mut s.chains.llm, &mut s.chains.tts] {
            chain.retain(|c| c != id);
        }
        if let Some(r) = &removed.key_ref {
            self.store
                .delete_key(r)
                .map_err(|_| STORE_DELETE_FAILED.to_owned())?;
        }
        self.save(&s)
    }

    /// Removes a connected catalog provider and its key.
    pub fn disconnect(&self, id: &str) -> Result<(), String> {
        let entry = catalog::find(id).ok_or(UNKNOWN_PROVIDER)?;
        if entry.setup != Setup::Key {
            return Err("Built-in engines cannot be disconnected.".into());
        }
        self.remove(id)
    }

    /// Creates or edits a custom provider. An empty key on edit keeps the stored key.
    pub fn save_custom(
        &self,
        form: &CustomProviderForm,
        key: &str,
    ) -> Result<ProviderStatus, String> {
        let v = custom::validate(form).map_err(|e| e.message.to_owned())?;
        let mut s = self.load()?;
        let id = match form.id.as_deref() {
            Some(id) => {
                let exists = s.providers.iter().any(|p| p.id == id);
                if !id.starts_with(CUSTOM_PREFIX) || !exists {
                    return Err("That custom provider no longer exists.".into());
                }
                id.to_owned()
            }
            None => new_custom_id(&s, &v.name),
        };
        let needs_key = v.auth.needs_key();
        let r = key_ref(&id);
        let key = if needs_key && !key.is_empty() {
            Some(check_key(key)?)
        } else {
            None
        };
        if needs_key && key.is_none() && !matches!(self.store.get_key(&r), Ok(Some(_))) {
            return Err("Enter the API key, or choose No key for a server on this Mac.".into());
        }
        // Switched to no key: the stored key is dropped only after settings are saved.
        let stale_key_ref = (!needs_key).then(|| r.clone());
        let provider = Provider {
            id,
            kind: v.capability.id().into(),
            adapter: OPENAI_COMPATIBLE.into(),
            model: v.model,
            base_url: Some(v.base_url),
            key_ref: needs_key.then_some(r),
            name: Some(v.name),
            auth_style: Some(v.auth.as_setting()),
        };
        let result = self.store_then_save(&mut s, provider, key);
        if result.is_ok()
            && let Some(r) = stale_key_ref
        {
            let _ = self.store.delete_key(&r);
        }
        result
    }

    pub fn delete_custom(&self, id: &str) -> Result<(), String> {
        if !id.starts_with(CUSTOM_PREFIX) {
            return Err("Only your own providers can be removed here.".into());
        }
        self.remove(id)
    }

    /// Runs the connection test for a configured provider.
    pub fn test(&self, id: &str, probe: &dyn HttpProbe) -> Result<TestResult, String> {
        let s = settings::load(&self.path).settings;
        let p = s
            .providers
            .iter()
            .find(|p| p.id == id)
            .ok_or("That provider is not set up.")?;
        let (Some(base_url), true) = (&p.base_url, p.adapter == OPENAI_COMPATIBLE) else {
            return Ok(TestResult::NotAvailable {
                reason: REASON_NO_FREE_CHECK,
            });
        };
        let Some(auth) = p.auth_style.as_deref().and_then(parse_auth) else {
            return Ok(TestResult::NotAvailable {
                reason: REASON_NO_FREE_CHECK,
            });
        };
        if !probe.available() {
            return Ok(TestResult::NotAvailable {
                reason: REASON_NO_CLIENT,
            });
        }
        let key = match &p.key_ref {
            Some(r) => match self.store.get_key(r) {
                Ok(Some(k)) => Some(k),
                _ => {
                    return Ok(TestResult::NotAvailable {
                        reason: REASON_NO_KEY,
                    });
                }
            },
            None => None,
        };
        let target = ProbeTarget {
            capability: Capability::from_id(&p.kind).unwrap_or(Capability::Llm),
            base_url,
            auth: &auth,
            model: &p.model,
        };
        Ok(probe.probe(&target, key.as_deref()))
    }
}

/// Parses an `authStyle` value from settings.json. `unknown` is never accepted.
pub fn parse_auth(s: &str) -> Option<AuthStyle> {
    match s {
        "bearer" => Some(AuthStyle::Bearer),
        "token" => Some(AuthStyle::Token),
        "x-api-key" => Some(AuthStyle::XApiKey),
        "none" => Some(AuthStyle::None),
        _ => {
            let name = s.strip_prefix("header:")?;
            custom::validate_header_name(name)
                .ok()
                .map(|n| AuthStyle::Header(Cow::Owned(n)))
        }
    }
}

fn check_key(key: &str) -> Result<&str, String> {
    let k = key.trim();
    if k.is_empty() || k.len() > KEY_MAX || k.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(BAD_KEY.into());
    }
    Ok(k)
}

fn catalog_provider(e: &CatalogEntry) -> Provider {
    Provider {
        id: e.id.into(),
        kind: e.capability.id().into(),
        adapter: e.adapter.into(),
        model: e.default_model.into(),
        base_url: e.base_url_default.map(Into::into),
        key_ref: e.auth.needs_key().then(|| key_ref(e.id)),
        name: None,
        auth_style: Some(e.auth.as_setting()),
    }
}

/// `custom-<slug>`, with `-2`, `-3` ... when taken.
fn new_custom_id(s: &Settings, name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(24).collect();
    let slug = slug.trim_end_matches('-');
    let base = if slug.is_empty() {
        format!("{CUSTOM_PREFIX}provider")
    } else {
        format!("{CUSTOM_PREFIX}{slug}")
    };
    let taken = |id: &str| s.providers.iter().any(|p| p.id == id);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .unwrap_or(base)
}

//! API key storage in the OS credential store (macOS Keychain via keyring).
//! Keys are addressed by `keyRef` and never written to settings or logs.

/// Keychain service name for all DeepAI Voice keys.
pub const SERVICE: &str = "com.deepakkewalramani.deepaivoice";

#[derive(Debug)]
pub struct KeyError(pub String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Message only, never the secret value.
        write!(f, "key store error: {}", self.0)
    }
}

impl std::error::Error for KeyError {}

/// Storage backend. The real one is `KeyringStore`; tests use `MemoryKeyStore`.
pub trait KeyStore {
    fn get_key(&self, key_ref: &str) -> Result<Option<String>, KeyError>;
    fn set_key(&self, key_ref: &str, secret: &str) -> Result<(), KeyError>;
    fn delete_key(&self, key_ref: &str) -> Result<(), KeyError>;
    fn has_key(&self, key_ref: &str) -> Result<bool, KeyError> {
        Ok(self.get_key(key_ref)?.is_some())
    }
}

/// OS credential store (macOS Keychain, Windows Credential Manager).
pub struct KeyringStore;

impl KeyringStore {
    fn entry(key_ref: &str) -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(SERVICE, key_ref).map_err(|e| KeyError(e.to_string()))
    }
}

impl KeyStore for KeyringStore {
    fn get_key(&self, key_ref: &str) -> Result<Option<String>, KeyError> {
        match Self::entry(key_ref)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(KeyError(e.to_string())),
        }
    }
    fn set_key(&self, key_ref: &str, secret: &str) -> Result<(), KeyError> {
        Self::entry(key_ref)?
            .set_password(secret)
            .map_err(|e| KeyError(e.to_string()))
    }
    fn delete_key(&self, key_ref: &str) -> Result<(), KeyError> {
        match Self::entry(key_ref)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError(e.to_string())),
        }
    }
}

/// In-memory store for tests and dev. Never touches the real Keychain.
#[derive(Default)]
pub struct MemoryKeyStore(std::sync::Mutex<std::collections::HashMap<String, String>>);

impl KeyStore for MemoryKeyStore {
    fn get_key(&self, key_ref: &str) -> Result<Option<String>, KeyError> {
        Ok(self
            .0
            .lock()
            .map_err(|e| KeyError(e.to_string()))?
            .get(key_ref)
            .cloned())
    }
    fn set_key(&self, key_ref: &str, secret: &str) -> Result<(), KeyError> {
        self.0
            .lock()
            .map_err(|e| KeyError(e.to_string()))?
            .insert(key_ref.into(), secret.into());
        Ok(())
    }
    fn delete_key(&self, key_ref: &str) -> Result<(), KeyError> {
        self.0
            .lock()
            .map_err(|e| KeyError(e.to_string()))?
            .remove(key_ref);
        Ok(())
    }
}

/// Dev env var for a keyRef: `anthropic` -> `ANTHROPIC_API_KEY`, `custom-1` -> `CUSTOM_1_API_KEY`.
pub fn env_var_name(key_ref: &str) -> String {
    let base: String = key_ref
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{base}_API_KEY")
}

/// Resolves a key: a non-empty env var wins (dev use), then the store.
/// `env` is injected so tests do not depend on the process environment.
pub fn resolve_key(
    store: &dyn KeyStore,
    key_ref: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, KeyError> {
    if let Some(v) = env(&env_var_name(key_ref)).filter(|v| !v.is_empty()) {
        return Ok(Some(v));
    }
    store.get_key(key_ref)
}

/// `resolve_key` with the real process environment.
pub fn resolve_key_from_env(
    store: &dyn KeyStore,
    key_ref: &str,
) -> Result<Option<String>, KeyError> {
    resolve_key(store, key_ref, |n| std::env::var(n).ok())
}

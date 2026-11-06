//! Secret storage for API keys.
//!
//! [`KeyringSecrets`] keeps secrets in the OS credential store (Windows Credential Manager,
//! macOS Keychain, Linux kernel keyring). [`MemorySecrets`] is for tests. Secret values are
//! wrapped in [`Secret`], whose `Debug` output is redacted so they can't end up in logs.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

/// A secret value. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value. Only pass it to the code that needs it (an HTTP header, the keychain).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the system keychain is unavailable: {0}")]
    Unavailable(String),
    #[error("keychain error: {0}")]
    Backend(String),
}

/// A place to keep secrets by name. Implementations block (keychain calls are synchronous),
/// so async callers run them on a blocking thread.
pub trait SecretStore: Send + Sync + 'static {
    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError>;
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError>;
    /// Removes a secret. Removing one that doesn't exist is not an error.
    fn delete(&self, name: &str) -> Result<(), SecretError>;
}

/// OS credential store, with entries named `<name>` under one service.
#[derive(Debug, Clone)]
pub struct KeyringSecrets {
    service: String,
}

impl KeyringSecrets {
    /// `service` groups Oxide's entries (use the app identifier).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, name).map_err(map_keyring)
    }
}

fn map_keyring(e: keyring::Error) -> SecretError {
    match e {
        keyring::Error::PlatformFailure(err) | keyring::Error::NoStorageAccess(err) => {
            SecretError::Unavailable(err.to_string())
        }
        // Other variants describe the entry, never the secret itself.
        other => SecretError::Backend(other.to_string()),
    }
}

impl SecretStore for KeyringSecrets {
    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError> {
        self.entry(name)?
            .set_password(secret.expose())
            .map_err(map_keyring)
    }

    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(Secret::new(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring(e)),
        }
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring(e)),
        }
    }
}

/// Largest piece stored in one keychain entry, in characters. Windows Credential Manager
/// keeps at most 2,560 bytes per entry (1,280 UTF-16 characters).
const CHUNK_CHARS: usize = 1_000;
/// Marks an entry whose value is split over `<name>#1` … `<name>#n`.
const CHUNKED_PREFIX: &str = "oxide-chunked:";

/// Stores a value that may be longer than one keychain entry allows (OAuth tokens), split
/// over several entries when needed.
pub fn set_large(store: &dyn SecretStore, name: &str, value: &str) -> Result<(), SecretError> {
    delete_large(store, name)?;
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= CHUNK_CHARS && !value.starts_with(CHUNKED_PREFIX) {
        return store.set(name, &Secret::new(value));
    }
    let parts: Vec<String> = chars
        .chunks(CHUNK_CHARS)
        .map(|c| c.iter().collect())
        .collect();
    for (i, part) in parts.iter().enumerate() {
        store.set(&format!("{name}#{}", i + 1), &Secret::new(part.clone()))?;
    }
    store.set(
        name,
        &Secret::new(format!("{CHUNKED_PREFIX}{}", parts.len())),
    )
}

/// Reads a value written by [`set_large`].
pub fn get_large(store: &dyn SecretStore, name: &str) -> Result<Option<Secret>, SecretError> {
    let Some(head) = store.get(name)? else {
        return Ok(None);
    };
    let Some(count) = head
        .expose()
        .strip_prefix(CHUNKED_PREFIX)
        .and_then(|n| n.parse::<usize>().ok())
    else {
        return Ok(Some(head));
    };
    let mut value = String::new();
    for i in 1..=count {
        match store.get(&format!("{name}#{i}"))? {
            Some(part) => value.push_str(part.expose()),
            // A missing piece: the value is unusable.
            None => return Ok(None),
        }
    }
    Ok(Some(Secret::new(value)))
}

/// Removes a value written by [`set_large`] and all its pieces.
pub fn delete_large(store: &dyn SecretStore, name: &str) -> Result<(), SecretError> {
    if let Some(head) = store.get(name)?
        && let Some(count) = head
            .expose()
            .strip_prefix(CHUNKED_PREFIX)
            .and_then(|n| n.parse::<usize>().ok())
    {
        for i in 1..=count {
            store.delete(&format!("{name}#{i}"))?;
        }
    }
    store.delete(name)
}

/// In-process secret store for tests.
#[derive(Debug, Default)]
pub struct MemorySecrets {
    map: Mutex<HashMap<String, Secret>>,
}

impl MemorySecrets {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Secret>> {
        // A poisoned map is still a valid map; keep going.
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl SecretStore for MemorySecrets {
    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError> {
        self.lock().insert(name.to_owned(), secret.clone());
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self.lock().get(name).cloned())
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        self.lock().remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("sk-very-secret");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
    }

    /// Talks to the real Windows Credential Manager (an entry under a test-only service name,
    /// removed again at the end).
    #[cfg(windows)]
    #[test]
    fn keyring_round_trips_through_windows_credential_manager() {
        let store = KeyringSecrets::new("dev.oxide.app.test");
        let name = format!("provider:test-{}", std::process::id());
        store
            .set(&name, &Secret::new("sk-test-value"))
            .expect("Credential Manager accepts the key");
        assert_eq!(
            store.get(&name).expect("reads"),
            Some(Secret::new("sk-test-value"))
        );
        store.delete(&name).expect("deletes");
        assert_eq!(store.get(&name).expect("reads"), None);
    }

    #[test]
    fn large_values_are_split_and_joined() {
        let m = MemorySecrets::default();
        let long: String = "abcdé".repeat(700);
        set_large(&m, "tok", &long).expect("ok");
        assert_eq!(
            m.get("tok#4")
                .expect("ok")
                .map(|s| s.expose().chars().count()),
            Some(500)
        );
        assert_eq!(get_large(&m, "tok").expect("ok"), Some(Secret::new(long)));
        // Rewriting with a short value removes the old pieces.
        set_large(&m, "tok", "short").expect("ok");
        assert_eq!(m.get("tok#1").expect("ok"), None);
        assert_eq!(
            get_large(&m, "tok").expect("ok"),
            Some(Secret::new("short"))
        );
        delete_large(&m, "tok").expect("ok");
        assert_eq!(get_large(&m, "tok").expect("ok"), None);
    }

    #[test]
    fn memory_store_round_trips() {
        let m = MemorySecrets::default();
        assert_eq!(m.get("a").expect("ok"), None);
        m.set("a", &Secret::new("x")).expect("ok");
        assert_eq!(m.get("a").expect("ok"), Some(Secret::new("x")));
        m.delete("a").expect("ok");
        m.delete("a").expect("deleting twice is fine");
        assert_eq!(m.get("a").expect("ok"), None);
    }
}

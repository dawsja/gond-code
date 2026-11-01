//! The secrets vault: account keys and tokens live in Windows Credential Manager,
//! never in SQLite, argv, logs or the state tree. Everything else refers to a secret by
//! its `SecretRef`; only the runner resolves one, just before it spawns.

#[cfg(windows)]
mod windows;

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// A secret value. Its `Debug` never prints it, it can't be serialized (only read from
/// command input), and its bytes are zeroed on drop.
pub struct Secret(String);

impl Drop for Secret {
    fn drop(&mut self) {
        // SAFETY: zero bytes are valid UTF-8, so the String stays well-formed.
        for b in unsafe { self.0.as_bytes_mut() } {
            // SAFETY: `b` is a valid, exclusive reference; volatile keeps the write.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Secret)
    }
}

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// The value, for the one place that needs it: the child's env at spawn.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Where a secret is stored: a Credential Manager target name, `GondCode/<harness>/<accountId>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef(pub String);

impl SecretRef {
    pub fn for_account(harness: &str, account_id: &str) -> Self {
        SecretRef(format!(
            "{}{harness}/{account_id}",
            crate::appdata::CREDENTIAL_PREFIX
        ))
    }
}

/// Credential storage. Windows Credential Manager in the app; `MemoryVault` in tests and
/// on the Linux dev path.
pub trait Vault: Send + Sync {
    fn put(&self, at: &SecretRef, secret: &Secret) -> AppResult<()>;
    fn get(&self, at: &SecretRef) -> AppResult<Option<Secret>>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, at: &SecretRef) -> AppResult<()>;
}

/// What the runner is given: turn a `SecretRef` into its value, or fail clearly.
pub trait SecretResolver: Send + Sync {
    fn resolve(&self, at: &SecretRef) -> AppResult<Secret>;
}

impl<V: Vault + ?Sized> SecretResolver for V {
    fn resolve(&self, at: &SecretRef) -> AppResult<Secret> {
        self.get(at)?.ok_or_else(|| {
            AppError::NotFound(format!(
                "the secret for {} is missing from Credential Manager; add the account again",
                at.0
            ))
        })
    }
}

/// The platform's vault: Credential Manager on Windows, memory elsewhere (dev only).
#[cfg(windows)]
pub type SystemVault = windows::CredentialManager;
#[cfg(not(windows))]
pub type SystemVault = MemoryVault;

#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, String>>);

impl Vault for MemoryVault {
    fn put(&self, at: &SecretRef, secret: &Secret) -> AppResult<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(at.0.clone(), secret.expose().to_string());
        Ok(())
    }

    fn get(&self, at: &SecretRef) -> AppResult<Option<Secret>> {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        Ok(map.get(&at.0).map(|v| Secret::new(v.clone())))
    }

    fn delete(&self, at: &SecretRef) -> AppResult<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&at.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_value() {
        let s = Secret::new("sk-ant-very-secret");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "sk-ant-very-secret");
        let from_json: Secret = serde_json::from_str("\"k\"").unwrap();
        assert_eq!(from_json.expose(), "k");
    }

    #[test]
    fn memory_vault_round_trip_and_resolve() {
        let v = MemoryVault::default();
        let at = SecretRef::for_account("claude", "01ABC");
        assert_eq!(at.0, "GondCode/claude/01ABC");
        assert_eq!(v.resolve(&at).unwrap_err().code(), "not_found");
        v.put(&at, &Secret::new("k1")).unwrap();
        assert_eq!(v.resolve(&at).unwrap().expose(), "k1");
        v.delete(&at).unwrap();
        v.delete(&at).unwrap();
        assert!(v.get(&at).unwrap().is_none());
    }
}

//! OS-keychain boundary for rotating refresh tokens.

use coop_cloud::{RefreshToken, SecretError};
use thiserror::Error;

/// Stable service namespace used by the launcher credential store.
pub const KEYCHAIN_SERVICE: &str = "pokecrossroads-coop-launcher";

#[derive(Debug, Error)]
pub enum KeychainError {
    #[error("credential vault is unavailable")]
    Unavailable,
    #[error("credential vault operation failed")]
    Operation,
    #[error("credential value is invalid")]
    Invalid(#[from] SecretError),
}

/// Narrow abstraction over an OS credential vault. Production implementations
/// must never replace this with a file, environment, or plaintext fallback.
pub trait RefreshTokenStore: Send + Sync {
    /// Loads the token stored for the stable service and canonical username.
    ///
    /// # Errors
    ///
    /// Returns an error when the OS vault is unavailable or rejects the lookup.
    fn load(&self, service: &str, username: &str) -> Result<Option<RefreshToken>, KeychainError>;
    /// Stores one rotated refresh token in the OS vault.
    ///
    /// # Errors
    ///
    /// Returns an error when the OS vault is unavailable or rejects the write.
    fn store(
        &self,
        service: &str,
        username: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError>;
    /// Deletes the token for a username from the OS vault.
    ///
    /// # Errors
    ///
    /// Returns an error when the OS vault is unavailable or rejects the delete.
    fn delete(&self, service: &str, username: &str) -> Result<(), KeychainError>;
}

/// A fail-closed production adapter. Platform adapters can implement the
/// trait without changing the launcher protocol or persistence policy.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeychain;

impl RefreshTokenStore for OsKeychain {
    fn load(&self, service: &str, username: &str) -> Result<Option<RefreshToken>, KeychainError> {
        #[cfg(windows)]
        {
            ensure_windows_store()?;
            let entry = keyring_core::Entry::new(service, username)
                .map_err(|error| map_keyring_error(&error))?;
            match entry.get_password() {
                Ok(value) => RefreshToken::new(value)
                    .map(Some)
                    .map_err(KeychainError::Invalid),
                Err(keyring_core::Error::NoEntry) => Ok(None),
                Err(error) => Err(map_keyring_error(&error)),
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (service, username);
            Err(KeychainError::Unavailable)
        }
    }

    fn store(
        &self,
        service: &str,
        username: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError> {
        #[cfg(windows)]
        {
            ensure_windows_store()?;
            let entry = keyring_core::Entry::new(service, username)
                .map_err(|error| map_keyring_error(&error))?;
            entry
                .set_password(token.expose_secret())
                .map_err(|error| map_keyring_error(&error))
        }

        #[cfg(not(windows))]
        {
            let _ = (service, username, token);
            Err(KeychainError::Unavailable)
        }
    }

    fn delete(&self, service: &str, username: &str) -> Result<(), KeychainError> {
        #[cfg(windows)]
        {
            ensure_windows_store()?;
            let entry = keyring_core::Entry::new(service, username)
                .map_err(|error| map_keyring_error(&error))?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                Err(error) => Err(map_keyring_error(&error)),
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (service, username);
            Err(KeychainError::Unavailable)
        }
    }
}

#[cfg(windows)]
fn ensure_windows_store() -> Result<(), KeychainError> {
    use std::sync::OnceLock;

    static INITIALIZED: OnceLock<bool> = OnceLock::new();
    if *INITIALIZED.get_or_init(|| {
        if keyring_core::get_default_store().is_some() {
            return true;
        }
        match windows_native_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                true
            }
            Err(_) => false,
        }
    }) {
        Ok(())
    } else {
        Err(KeychainError::Unavailable)
    }
}

#[cfg(windows)]
fn map_keyring_error(error: &keyring_core::Error) -> KeychainError {
    match error {
        keyring_core::Error::NoEntry => KeychainError::Unavailable,
        _ => KeychainError::Operation,
    }
}

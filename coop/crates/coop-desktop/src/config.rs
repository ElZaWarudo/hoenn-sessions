//! Immutable pilot configuration and the bounded per-user state record.

use std::{
    env,
    path::{Path, PathBuf},
};

use coop_cloud::{CharacterId, UserId};
use coop_launcher::{TrustedManifestKey, TrustedReleaseKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PRODUCT_NAME: &str = "Hoenn Sessions";
pub const API_BASE: &str = match option_env!("HOENN_COOP_API_BASE") {
    Some(value) => value,
    None => "https://coop.private-pilot.invalid",
};
pub const RELEASE_KEY_ID: &str = match option_env!("HOENN_RELEASE_TRUST_KEY_ID") {
    Some(value) => value,
    None => "pilot-v1",
};
pub const RELEASE_PUBLIC_KEY_HEX: Option<&str> = option_env!("HOENN_RELEASE_TRUST_KEY_HEX");
pub const MANIFEST_KEY_ID: &str = match option_env!("HOENN_MANIFEST_TRUST_KEY_ID") {
    Some(value) => value,
    None => "pilot-manifest-v1",
};
pub const MANIFEST_PUBLIC_KEY_HEX: Option<&str> = option_env!("HOENN_MANIFEST_TRUST_KEY_HEX");

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("LOCALAPPDATA is unavailable")]
    MissingLocalAppData,
    #[error("LOCALAPPDATA is not a safe absolute directory")]
    InvalidLocalAppData,
    #[error("compiled release trust is unavailable")]
    MissingReleaseTrust,
    #[error("compiled release trust is malformed")]
    InvalidReleaseTrust,
    #[error("account record is invalid")]
    InvalidAccount,
    #[error("account state could not be read")]
    StateIo(#[source] std::io::Error),
    #[error("account state could not be encoded")]
    StateFormat(#[source] serde_json::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPaths {
    local_app_data: PathBuf,
    state_root: PathBuf,
    runtime_root: PathBuf,
    generations_root: PathBuf,
    workspace_parent: PathBuf,
    account_file: PathBuf,
    cleanup_file: PathBuf,
    epoch_file: PathBuf,
}

impl UserPaths {
    pub fn resolve() -> Result<Self, ConfigError> {
        let root = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(ConfigError::MissingLocalAppData)?;
        Self::from_local_app_data(root)
    }

    pub fn from_local_app_data(root: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let local_app_data = root.into();
        if !local_app_data.is_absolute()
            || local_app_data.file_name().is_none()
            || local_app_data
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ConfigError::InvalidLocalAppData);
        }
        let state_root = local_app_data.join(PRODUCT_NAME);
        let runtime_root = state_root.join("runtime");
        Ok(Self {
            local_app_data,
            generations_root: runtime_root.join("releases"),
            workspace_parent: state_root.join("sessions"),
            account_file: state_root.join("account.json"),
            cleanup_file: state_root.join("credential-cleanup.txt"),
            epoch_file: state_root.join("epoch.json"),
            state_root,
            runtime_root,
        })
    }

    pub fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }
    pub fn generations_root(&self) -> &Path {
        &self.generations_root
    }
    pub fn workspace_parent(&self) -> &Path {
        &self.workspace_parent
    }
    pub fn account_file(&self) -> &Path {
        &self.account_file
    }
    pub fn epoch_file(&self) -> &Path {
        &self.epoch_file
    }

    pub fn ensure_directories(&self) -> Result<(), ConfigError> {
        for path in [
            self.state_root.as_path(),
            self.runtime_root.as_path(),
            self.generations_root.as_path(),
            self.workspace_parent.as_path(),
        ] {
            std::fs::create_dir_all(path).map_err(ConfigError::StateIo)?;
            let metadata = std::fs::symlink_metadata(path).map_err(ConfigError::StateIo)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(ConfigError::InvalidLocalAppData);
            }
        }
        Ok(())
    }

    pub fn load_account(&self) -> Result<Option<AccountRecord>, ConfigError> {
        let bytes = match std::fs::read(&self.account_file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ConfigError::StateIo(error)),
        };
        if bytes.len() > 4096 {
            return Err(ConfigError::InvalidAccount);
        }
        let account: AccountRecord =
            serde_json::from_slice(&bytes).map_err(|_| ConfigError::InvalidAccount)?;
        account.validate()?;
        Ok(Some(account))
    }

    pub fn save_account(&self, account: &AccountRecord) -> Result<(), ConfigError> {
        account.validate()?;
        self.ensure_directories()?;
        let bytes = serde_json::to_vec(account).map_err(ConfigError::StateFormat)?;
        let temporary = self
            .account_file
            .with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(ConfigError::StateIo)?;
        use std::io::Write;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(ConfigError::StateIo(error));
        }
        drop(file);
        if let Err(error) = std::fs::rename(&temporary, &self.account_file) {
            let _ = std::fs::remove_file(&temporary);
            return Err(ConfigError::StateIo(error));
        }
        Ok(())
    }

    pub fn load_cleanup_username(&self) -> Result<Option<String>, ConfigError> {
        let bytes = match std::fs::read(&self.cleanup_file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ConfigError::StateIo(error)),
        };
        if bytes.len() > 257 || !bytes.ends_with(b"\n") {
            return Err(ConfigError::InvalidAccount);
        }
        let username = std::str::from_utf8(&bytes[..bytes.len() - 1])
            .map_err(|_| ConfigError::InvalidAccount)?
            .to_owned();
        coop_cloud::Username::new(username.clone()).map_err(|_| ConfigError::InvalidAccount)?;
        Ok(Some(username))
    }

    pub fn save_cleanup_username(&self, username: &str) -> Result<(), ConfigError> {
        coop_cloud::Username::new(username.to_owned()).map_err(|_| ConfigError::InvalidAccount)?;
        self.ensure_directories()?;
        let mut bytes = username.as_bytes().to_vec();
        bytes.push(b'\n');
        let temporary = self
            .cleanup_file
            .with_extension(format!("txt.{}.tmp", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(ConfigError::StateIo)?;
        use std::io::Write;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(ConfigError::StateIo(error));
        }
        drop(file);
        if let Err(error) = std::fs::rename(&temporary, &self.cleanup_file) {
            let _ = std::fs::remove_file(&temporary);
            return Err(ConfigError::StateIo(error));
        }
        Ok(())
    }

    pub fn clear_cleanup_username(&self) -> Result<(), ConfigError> {
        match std::fs::remove_file(&self.cleanup_file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ConfigError::StateIo(error)),
        }
    }

    pub fn remove_account(&self) -> Result<(), ConfigError> {
        match std::fs::remove_file(&self.account_file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ConfigError::StateIo(error)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountRecord {
    pub username: String,
    pub user_id: UserId,
    pub character_id: CharacterId,
}

impl AccountRecord {
    pub fn validate(&self) -> Result<(), ConfigError> {
        coop_cloud::Username::new(self.username.clone())
            .map(|_| ())
            .map_err(|_| ConfigError::InvalidAccount)
    }
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub api_base: String,
    pub release_key: TrustedReleaseKey,
    pub manifest_key: TrustedManifestKey,
}

impl std::fmt::Debug for RuntimeConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeConfig")
            .field("api_base", &self.api_base)
            .field("release_key", &self.release_key.key_id())
            .field("manifest_key", &self.manifest_key.key_id())
            .finish()
    }
}

impl RuntimeConfig {
    pub fn compiled() -> Result<Self, ConfigError> {
        let release_key = trusted_release_key(RELEASE_KEY_ID, RELEASE_PUBLIC_KEY_HEX)?;
        let manifest_key = trusted_manifest_key(MANIFEST_KEY_ID, MANIFEST_PUBLIC_KEY_HEX)?;
        Ok(Self {
            api_base: API_BASE.to_owned(),
            release_key,
            manifest_key,
        })
    }

    pub fn for_test(
        api_base: impl Into<String>,
        release_key: TrustedReleaseKey,
        manifest_key: TrustedManifestKey,
    ) -> Self {
        Self {
            api_base: api_base.into(),
            release_key,
            manifest_key,
        }
    }
}

pub fn trusted_release_key(
    key_id: &str,
    encoded: Option<&str>,
) -> Result<TrustedReleaseKey, ConfigError> {
    let bytes = decode_hex_32(encoded.ok_or(ConfigError::MissingReleaseTrust)?)
        .ok_or(ConfigError::InvalidReleaseTrust)?;
    TrustedReleaseKey::new(key_id, bytes).map_err(|_| ConfigError::InvalidReleaseTrust)
}

pub fn trusted_manifest_key(
    key_id: &str,
    encoded: Option<&str>,
) -> Result<TrustedManifestKey, ConfigError> {
    let bytes = decode_hex_32(encoded.ok_or(ConfigError::MissingReleaseTrust)?)
        .ok_or(ConfigError::InvalidReleaseTrust)?;
    TrustedManifestKey::new(key_id, bytes).map_err(|_| ConfigError::InvalidReleaseTrust)
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
    }
    Some(bytes)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountRecord, UserPaths};
    use coop_cloud::{CharacterId, UserId};

    fn account(username: &str, seed: u128) -> AccountRecord {
        AccountRecord {
            username: username.to_owned(),
            user_id: UserId::new(uuid::Uuid::from_u128(seed)).expect("user id"),
            character_id: CharacterId::new(uuid::Uuid::from_u128(seed + 1)).expect("character id"),
        }
    }

    #[test]
    fn account_record_atomically_replaces_existing_file() {
        let root = tempfile::tempdir().expect("temp directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");
        let first = account("first-player", 10);
        let replacement = account("second-player", 20);

        paths.save_account(&first).expect("first account");
        paths
            .save_account(&replacement)
            .expect("replacement account");

        assert_eq!(paths.load_account().expect("load"), Some(replacement));
        assert_eq!(
            std::fs::read_dir(paths.state_root())
                .expect("state directory")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
                .count(),
            0,
        );
    }

    #[test]
    fn credential_cleanup_username_survives_restart_until_cleared() {
        let root = tempfile::tempdir().expect("temp directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");

        paths
            .save_cleanup_username("cleanup-player")
            .expect("save cleanup owner");
        assert_eq!(
            paths.load_cleanup_username().expect("load cleanup owner"),
            Some("cleanup-player".to_owned())
        );
        paths.clear_cleanup_username().expect("clear cleanup owner");
        assert_eq!(paths.load_cleanup_username().expect("load cleared"), None);
    }

    #[test]
    fn malformed_cleanup_username_is_not_treated_as_absent() {
        let root = tempfile::tempdir().expect("temp directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");
        paths.ensure_directories().expect("state directory");
        std::fs::write(&paths.cleanup_file, b"truncated-without-newline")
            .expect("malformed cleanup marker");

        assert!(paths.load_cleanup_username().is_err());
    }

    #[test]
    fn failed_cleanup_marker_replacement_preserves_existing_owner() {
        let root = tempfile::tempdir().expect("temp directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");
        paths
            .save_cleanup_username("original-player")
            .expect("original cleanup owner");

        let result = paths.save_cleanup_username("replacement-player");

        assert!(result.is_ok());
        assert_eq!(
            paths.load_cleanup_username().expect("load replacement"),
            Some("replacement-player".to_owned())
        );
        assert_eq!(
            std::fs::read_dir(paths.state_root())
                .expect("state directory")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
                .count(),
            0,
        );
    }

    #[test]
    fn cleanup_marker_write_failure_leaves_no_temporary_record() {
        let root = tempfile::tempdir().expect("temp directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");
        paths.ensure_directories().expect("state directory");
        std::fs::create_dir(&paths.cleanup_file).expect("block marker destination");

        assert!(paths.save_cleanup_username("cleanup-player").is_err());
        assert_eq!(
            std::fs::read_dir(paths.state_root())
                .expect("state directory")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
                .count(),
            0,
        );
    }
}

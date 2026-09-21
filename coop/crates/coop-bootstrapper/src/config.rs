//! Fixed, MSI-owned paths and compile-time trust configuration for the
//! Windows bootstrapper.

use std::{
    env,
    path::{Path, PathBuf},
};

use coop_launcher::TrustedReleaseKey;
use thiserror::Error;

pub const PRODUCT_NAME: &str = "Hoenn Sessions";
pub const INSTALL_DIRECTORY: &str = "Programs";
pub const STATE_DIRECTORY: &str = "Hoenn Sessions";
pub const RUNTIME_DIRECTORY: &str = "runtime";
pub const APP_DIRECTORY: &str = "app";
pub const BOOTSTRAPPER_FILE: &str = "hoenn-sessions-bootstrapper.exe";
pub const ONBOARDING_FILE: &str = "hoenn-sessions-onboarding.exe";
pub const DESKTOP_FILE: &str = "coop-launcher.exe";
pub const TRUST_KEY_ID_ENV: &str = "HOENN_RELEASE_TRUST_KEY_ID";
pub const TRUST_PUBLIC_KEY_ENV: &str = "HOENN_RELEASE_TRUST_PUBLIC_KEY_HEX";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("LOCALAPPDATA is missing")]
    MissingLocalAppData,
    #[error("LOCALAPPDATA must be an absolute directory")]
    InvalidLocalAppData,
    #[error("compiled release trust key is not configured")]
    MissingTrustKey,
    #[error("compiled release trust key is malformed")]
    InvalidTrustKey,
}

/// All roots are derived from the per-user LocalAppData directory.  No path
/// is read from command-line input or mutable release metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallRoots {
    local_app_data: PathBuf,
    install_root: PathBuf,
    state_root: PathBuf,
    runtime_root: PathBuf,
}

impl InstallRoots {
    pub fn resolve() -> Result<Self, ConfigError> {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(ConfigError::MissingLocalAppData)?;
        Self::from_local_app_data(local_app_data)
    }

    pub fn from_local_app_data(local_app_data: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let local_app_data = local_app_data.into();
        if !local_app_data.is_absolute()
            || local_app_data.file_name().is_none()
            || local_app_data
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ConfigError::InvalidLocalAppData);
        }
        let install_root = local_app_data.join(INSTALL_DIRECTORY).join(PRODUCT_NAME);
        let state_root = local_app_data.join(STATE_DIRECTORY);
        let runtime_root = state_root.join(RUNTIME_DIRECTORY);
        Ok(Self {
            local_app_data,
            install_root,
            state_root,
            runtime_root,
        })
    }

    pub fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.state_root.join("bootstrap.lock")
    }

    pub fn onboarding_path(&self) -> PathBuf {
        self.install_root.join(APP_DIRECTORY).join(ONBOARDING_FILE)
    }

    pub fn bootstrap_path(&self) -> PathBuf {
        self.install_root
            .join(APP_DIRECTORY)
            .join(BOOTSTRAPPER_FILE)
    }

    pub fn desktop_path(&self, generation_root: &Path) -> PathBuf {
        generation_root.join("app").join(DESKTOP_FILE)
    }
}

/// The public key is a build input, not a mutable file/config value.  An
/// unsigned development build therefore cannot accidentally accept a release.
pub fn compiled_trust_key() -> Result<TrustedReleaseKey, ConfigError> {
    let key_id = option_env!("HOENN_RELEASE_TRUST_KEY_ID").ok_or(ConfigError::MissingTrustKey)?;
    let encoded = option_env!("HOENN_RELEASE_TRUST_KEY_HEX").ok_or(ConfigError::MissingTrustKey)?;
    let bytes = decode_hex_32(encoded).ok_or(ConfigError::InvalidTrustKey)?;
    TrustedReleaseKey::new(key_id, bytes).map_err(|_| ConfigError::InvalidTrustKey)
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        output[index] = (high << 4) | low;
    }
    Some(output)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

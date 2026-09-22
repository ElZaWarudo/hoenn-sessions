//! Fail-closed selection and launch logic for the stable bootstrapper.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use coop_launcher::update::ArtifactIdentity;
use coop_launcher::{AcceptedGeneration, GenerationHandoff, GenerationStore, TrustedReleaseKey};
use thiserror::Error;

use crate::config::{self, ConfigError, InstallRoots};

pub const TRUSTED_RESELECTION_EXIT_CODE: i32 = 10;
const MAX_RESELECTION_ATTEMPTS: u8 = 1;

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("bootstrap configuration is unavailable")]
    Config(#[from] ConfigError),
    #[error("another Hoenn Sessions instance is already running")]
    AlreadyRunning,
    #[error("bootstrap lock failed")]
    Lock(#[source] io::Error),
    #[error("bootstrap could not launch the fixed executable")]
    Spawn(#[source] io::Error),
    #[error("bootstrap could not resolve the fixed executable")]
    InvalidExecutable(PathBuf),
}

/// A per-user lock held for the entire lifetime of the bootstrap process.
pub struct SingleInstanceGuard {
    _file: File,
}

impl SingleInstanceGuard {
    pub fn acquire(roots: &InstallRoots) -> Result<Self, LaunchError> {
        fs::create_dir_all(roots.state_root()).map_err(LaunchError::Lock)?;
        let path = roots.lock_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(LaunchError::Lock)?;
        fs4::FileExt::try_lock(&file).map_err(|error| match error {
            fs4::TryLockError::WouldBlock => LaunchError::AlreadyRunning,
            fs4::TryLockError::Error(error) => LaunchError::Lock(error),
        })?;
        Ok(Self { _file: file })
    }
}

/// The only two launch targets permitted by the bootstrap.
pub enum LaunchTarget {
    Accepted {
        handoff: GenerationHandoff,
        desktop: PathBuf,
    },
    Onboarding(PathBuf),
}

impl LaunchTarget {
    pub fn path(&self) -> &Path {
        match self {
            Self::Accepted { desktop, .. } => desktop,
            Self::Onboarding(path) => path,
        }
    }
}

/// Opens the updater's accepted generation.  Every failure deliberately
/// selects MSI-owned onboarding, which is the only path allowed to restore
/// readiness or perform authentication/update work.
pub fn select_target(
    roots: &InstallRoots,
    trusted_key: Option<&TrustedReleaseKey>,
    min_release_sequence: Option<u64>,
    now: i64,
) -> LaunchTarget {
    let (Some(trusted_key), Some(min_release_sequence)) = (trusted_key, min_release_sequence)
    else {
        return LaunchTarget::Onboarding(roots.onboarding_path());
    };

    let accepted = GenerationStore::new(roots.runtime_root().join("releases"))
        .ok()
        .and_then(|store| store.open_accepted_current(trusted_key, now).ok());
    let Some(accepted) = accepted else {
        return LaunchTarget::Onboarding(roots.onboarding_path());
    };
    if accepted.sequence() < min_release_sequence {
        return LaunchTarget::Onboarding(roots.onboarding_path());
    }
    accepted_target(roots, accepted)
        .unwrap_or_else(|| LaunchTarget::Onboarding(roots.onboarding_path()))
}

fn accepted_target(roots: &InstallRoots, accepted: AcceptedGeneration) -> Option<LaunchTarget> {
    let handoff = accepted.handoff();
    let desktop = {
        let artifact = handoff.artifact(ArtifactIdentity::DesktopApp)?;
        artifact.path().to_path_buf()
    };
    let expected = roots.desktop_path(handoff.path());
    if desktop != expected || !is_fixed_regular_file(&desktop) {
        return None;
    }
    Some(LaunchTarget::Accepted { handoff, desktop })
}

fn is_fixed_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn launch(target: LaunchTarget, working_directory: &Path) -> Result<ExitStatus, LaunchError> {
    let path = target.path().to_path_buf();
    if !is_fixed_regular_file(&path) {
        return Err(LaunchError::InvalidExecutable(path));
    }

    // Keep the opaque accepted-generation handoff alive until process
    // creation returns.  On Windows that retains executable and ancestor
    // identity/share guards over the complete spawn boundary.
    let _handoff = match target {
        LaunchTarget::Accepted { handoff, .. } => Some(handoff),
        LaunchTarget::Onboarding(_) => None,
    };
    Command::new(&path)
        .current_dir(working_directory)
        .status()
        .map_err(LaunchError::Spawn)
}

pub fn run() -> Result<i32, LaunchError> {
    let roots = InstallRoots::resolve()?;
    let _instance = SingleInstanceGuard::acquire(&roots)?;
    let trusted_key = config::compiled_trust_key().ok();
    let min_release_sequence = config::compiled_min_release_sequence();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LaunchError::Config(ConfigError::InvalidTrustKey))?
        .as_secs() as i64;

    let mut attempts = 0_u8;
    loop {
        let target = select_target(&roots, trusted_key.as_ref(), min_release_sequence, now);
        let status = launch(target, roots.install_root())?;
        let exit_code = status.code().unwrap_or(1);
        if exit_code == TRUSTED_RESELECTION_EXIT_CODE && attempts < MAX_RESELECTION_ATTEMPTS {
            attempts = attempts.saturating_add(1);
            continue;
        }
        return Ok(exit_code);
    }
}

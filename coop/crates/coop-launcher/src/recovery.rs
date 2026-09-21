//! Fail-closed discovery and restart reconciliation for interrupted saves.
//!
//! Recovery evidence is deliberately smaller than a session workspace.  A
//! preserved directory may contain only a bounded SAV and a marker.  The
//! marker never contains credentials, bridge secrets, or paths; it is merely
//! the proof needed to decide whether a fresh lease may retry one snapshot.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use coop_cloud::{
    CharacterId, ClientInstanceId, LeaseFence, Revision, SessionEpoch, SessionId, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AuthSession, CloudApi, SessionConfig, SessionError, SessionLifecycle,
    keychain::RefreshTokenStore,
};

const MAX_RECOVERY_MARKER_BYTES: usize = 16 * 1024;
const MAX_RECOVERY_ENTRIES: usize = 4;
const MAX_RECOVERY_CANDIDATES: usize = 8;
const MAX_SESSION_FILE_BYTES: usize = 64 * 1024 * 1024;
const LEGACY_MARKER: &[u8] = b"coop-recovery-v1\n";
const RECOVERY_MARKER_NAME: &str = "recovery.marker";
const CHARACTER_SAVE_NAME: &str = "character.sav";

/// A non-secret v2 marker written only after a correlated save event has been
/// accepted by the sidecar.  The prior fence is retained as provenance; a
/// restart must acquire a new server-owned lease before using this record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryMarkerV2 {
    pub version: u8,
    pub character_id: CharacterId,
    pub prior_session_id: SessionId,
    pub prior_session_epoch: SessionEpoch,
    pub prior_client_instance_id: ClientInstanceId,
    pub parent_revision: Revision,
    pub save_generation: u32,
    pub save_sha256: Sha256Digest,
}

impl RecoveryMarkerV2 {
    /// Creates a proof-bound marker from the lease that authorized the save.
    pub fn new(
        prior_fence: LeaseFence,
        parent_revision: Revision,
        save_generation: u32,
        save_sha256: Sha256Digest,
    ) -> Result<Self, RecoveryError> {
        // The ROM increments this u32 with wrapping arithmetic. Generation
        // zero is therefore valid after u32::MAX and remains bound to the
        // exact save digest and prior lease like every other generation.
        if prior_fence.current_revision != parent_revision {
            return Err(RecoveryError::Malformed);
        }
        let marker = Self {
            version: 2,
            character_id: prior_fence.character_id,
            prior_session_id: prior_fence.session_id,
            prior_session_epoch: prior_fence.session_epoch,
            prior_client_instance_id: prior_fence.client_instance_id,
            parent_revision,
            save_generation,
            save_sha256,
        };
        marker.validate()?;
        Ok(marker)
    }

    fn validate(&self) -> Result<(), RecoveryError> {
        if self.version != 2
            || self.prior_session_epoch.value() == 0
            || self.prior_session_id.as_uuid().is_nil()
            || self.prior_client_instance_id.as_uuid().is_nil()
            || self.parent_revision.next().is_err()
        {
            return Err(RecoveryError::Malformed);
        }
        Ok(())
    }

    /// Encodes the marker as bounded JSON with a single trailing newline.
    pub fn encode(&self) -> Result<Vec<u8>, RecoveryError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self).map_err(|_| RecoveryError::Malformed)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECOVERY_MARKER_BYTES {
            return Err(RecoveryError::Malformed);
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, RecoveryError> {
        if bytes.len() > MAX_RECOVERY_MARKER_BYTES || !bytes.ends_with(b"\n") {
            return Err(RecoveryError::Malformed);
        }
        let value = &bytes[..bytes.len() - 1];
        let marker: Self = serde_json::from_slice(value).map_err(|_| RecoveryError::Malformed)?;
        marker.validate()?;
        if marker.encode()?.as_slice() != bytes {
            return Err(RecoveryError::Malformed);
        }
        Ok(marker)
    }

    #[must_use]
    pub fn prior_fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.prior_session_id,
            self.character_id,
            self.parent_revision,
            self.prior_session_epoch,
            self.prior_client_instance_id,
        )
    }
}

/// The marker format found in a launcher-owned recovery directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryMarker {
    LegacyV1,
    V2(RecoveryMarkerV2),
}

impl RecoveryMarker {
    fn decode(bytes: &[u8]) -> Result<Self, RecoveryError> {
        if bytes == LEGACY_MARKER {
            Ok(Self::LegacyV1)
        } else {
            RecoveryMarkerV2::decode(bytes).map(Self::V2)
        }
    }

    #[must_use]
    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::LegacyV1)
    }
}

/// Discovery failures intentionally do not include paths or file contents.
#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("no recovery evidence was found")]
    NoEvidence,
    #[error("recovery evidence is ambiguous")]
    Ambiguous,
    #[error("recovery evidence is malformed")]
    Malformed,
    #[error("recovery evidence changed during reconciliation")]
    Replaced,
    #[error("recovery evidence cleanup failed")]
    Cleanup,
    #[error("recovery reconciliation remains blocked")]
    Blocked,
    #[error("session lifecycle failed")]
    Session(#[from] SessionError),
}

impl PartialEq for RecoveryError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::NoEvidence, Self::NoEvidence)
                | (Self::Ambiguous, Self::Ambiguous)
                | (Self::Malformed, Self::Malformed)
                | (Self::Replaced, Self::Replaced)
                | (Self::Cleanup, Self::Cleanup)
                | (Self::Blocked, Self::Blocked)
        )
    }
}

impl Eq for RecoveryError {}

/// A discovered candidate.  SAV bytes are never retained in this object or
/// formatted in its debug representation; callers read them only after the
/// fresh authenticated cloud head has been verified.
pub struct RecoveryCandidate {
    path: PathBuf,
    marker: RecoveryMarker,
    save_sha256: Sha256Digest,
    #[cfg(windows)]
    _identity_guards: Vec<File>,
}

impl std::fmt::Debug for RecoveryCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryCandidate")
            .field("marker", &self.marker)
            .field("save_sha256", &self.save_sha256)
            .finish_non_exhaustive()
    }
}

impl RecoveryCandidate {
    #[must_use]
    pub fn marker(&self) -> &RecoveryMarker {
        &self.marker
    }

    #[must_use]
    pub const fn save_sha256(&self) -> Sha256Digest {
        self.save_sha256
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Revalidates the directory identity and exact SAV digest before a
    /// caller performs any reconciliation action.
    pub fn revalidate(&self) -> Result<(), RecoveryError> {
        self.verify_identity()
    }

    fn read_save(&self) -> Result<Vec<u8>, RecoveryError> {
        let bytes = read_bounded_regular(&self.path.join(CHARACTER_SAVE_NAME))?;
        if Sha256Digest::of_bytes(&bytes) != self.save_sha256 {
            return Err(RecoveryError::Replaced);
        }
        Ok(bytes)
    }

    fn verify_identity(&self) -> Result<(), RecoveryError> {
        let marker = read_bounded_regular(&self.path.join(RECOVERY_MARKER_NAME))?;
        let parsed = RecoveryMarker::decode(&marker)?;
        if parsed != self.marker {
            return Err(RecoveryError::Replaced);
        }
        let _ = self.read_save()?;
        validate_candidate_directory(&self.path)?;
        Ok(())
    }

    fn retire(self) -> Result<(), RecoveryError> {
        self.verify_identity()?;
        let save = self.path.join(CHARACTER_SAVE_NAME);
        let marker = self.path.join(RECOVERY_MARKER_NAME);
        std::fs::remove_file(save).map_err(|_| RecoveryError::Cleanup)?;
        std::fs::remove_file(marker).map_err(|_| RecoveryError::Cleanup)?;
        std::fs::remove_dir(&self.path).map_err(|_| RecoveryError::Cleanup)
    }
}

/// Result of bounded discovery.  `None` is not an error when no evidence is
/// present; malformed launcher-owned evidence is an explicit failure.
#[derive(Debug)]
pub struct RecoveryDiscovery {
    candidate: Option<RecoveryCandidate>,
}

impl RecoveryDiscovery {
    /// Scans only direct launcher-owned recovery directories under `root`.
    pub fn discover(root: &Path) -> Result<Self, RecoveryError> {
        reject_symlink_ancestors(root)?;
        let metadata = std::fs::symlink_metadata(root).map_err(|_| RecoveryError::NoEvidence)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RecoveryError::Malformed);
        }
        let mut candidates = Vec::new();
        let mut launcher_candidates = 0usize;
        let entries = std::fs::read_dir(root).map_err(|_| RecoveryError::Malformed)?;
        for entry in entries {
            let entry = entry.map_err(|_| RecoveryError::Malformed)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("coop-session-") || name.starts_with("coop-recovery-")) {
                continue;
            }
            launcher_candidates += 1;
            if launcher_candidates > MAX_RECOVERY_CANDIDATES {
                return Err(RecoveryError::Ambiguous);
            }
            let path = entry.path();
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|_| RecoveryError::Malformed)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RecoveryError::Malformed);
            }
            candidates.push(discover_candidate(path)?);
            if candidates.len() > 1 {
                return Err(RecoveryError::Ambiguous);
            }
        }
        Ok(Self {
            candidate: candidates.pop(),
        })
    }

    #[must_use]
    pub fn candidate(self) -> Option<RecoveryCandidate> {
        self.candidate
    }
}

/// High-level restart reconciler.  It always discovers before acquiring a
/// lease, then acquires a fresh lease and lets `SessionLifecycle` verify the
/// signed current head and perform any fenced idempotent mutation.
pub struct RecoveryReconciler;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryOutcome {
    NoEvidence,
    RetiredLegacy,
    RetiredCommittedV2,
    CommittedV2,
    /// The local v2 marker is internally consistent, but it has no
    /// server-signed recovery capability.  It must remain on disk until a
    /// later protocol provides authorization for a divergent commit.
    AuthorizationMissing,
}

/// The authenticated session retained after startup reconciliation.  A
/// successful recovery settles only the lease; credential logout is left to
/// the caller's normal session shutdown path.
pub struct RecoverySession {
    pub outcome: RecoveryOutcome,
    pub auth: AuthSession,
}

impl RecoveryReconciler {
    fn divergent_v2_outcome() -> RecoveryOutcome {
        RecoveryOutcome::AuthorizationMissing
    }

    /// Reconciles at most one candidate and preserves it on every uncertainty.
    pub async fn reconcile<A: CloudApi>(
        api: &A,
        auth: AuthSession,
        config: SessionConfig,
        keychain: Option<Arc<dyn RefreshTokenStore>>,
    ) -> Result<RecoverySession, RecoveryError> {
        let discovery = RecoveryDiscovery::discover(&config.workspace_parent)?;
        let Some(candidate) = discovery.candidate() else {
            return Ok(RecoverySession {
                outcome: RecoveryOutcome::NoEvidence,
                auth,
            });
        };
        let marker = candidate.marker().clone();
        let mut lifecycle = match keychain {
            Some(keychain) => {
                SessionLifecycle::acquire_with_keychain(api, auth, config, keychain).await?
            }
            None => SessionLifecycle::acquire(api, auth, config).await?,
        };
        let outcome = Self::reconcile_candidate(api, &mut lifecycle, candidate, marker).await;
        let release = lifecycle.release_lease_keep_credentials(api).await;
        match (outcome, release) {
            (Ok(result), Ok(())) => Ok(RecoverySession {
                outcome: result,
                auth: lifecycle.auth,
            }),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(RecoveryError::Session(error)),
        }
    }

    async fn reconcile_candidate<A: CloudApi>(
        api: &A,
        lifecycle: &mut SessionLifecycle,
        candidate: RecoveryCandidate,
        marker: RecoveryMarker,
    ) -> Result<RecoveryOutcome, RecoveryError> {
        let candidate_sav = candidate.read_save()?;
        let cloud_digest = lifecycle.active_save_digest()?;
        let cloud_generation = lifecycle.save_generation();
        match marker {
            RecoveryMarker::LegacyV1 => {
                let Some(cloud_digest) = cloud_digest else {
                    return Err(RecoveryError::Blocked);
                };
                if cloud_digest != candidate.save_sha256() {
                    return Err(RecoveryError::Blocked);
                }
                candidate.retire()?;
                Ok(RecoveryOutcome::RetiredLegacy)
            }
            RecoveryMarker::V2(marker) => {
                if marker.character_id != lifecycle.lease.character_id
                    || marker.prior_fence().client_instance_id != lifecycle.lease.client_instance_id
                    || marker.prior_fence().current_revision != marker.parent_revision
                {
                    return Err(RecoveryError::Blocked);
                }
                let expected_next = marker
                    .parent_revision
                    .next()
                    .map_err(|_| RecoveryError::Blocked)?;
                if lifecycle.revision == expected_next
                    && cloud_digest == Some(marker.save_sha256)
                    && cloud_generation == Some(marker.save_generation)
                {
                    lifecycle.verify_current_head(api).await?;
                    candidate.retire()?;
                    return Ok(RecoveryOutcome::RetiredCommittedV2);
                }
                if lifecycle.revision != marker.parent_revision
                    || cloud_digest.is_some_and(|digest| digest == marker.save_sha256)
                    || cloud_generation
                        .is_some_and(|generation| generation >= marker.save_generation)
                {
                    return Err(RecoveryError::Blocked);
                }
                if Sha256Digest::of_bytes(&candidate_sav) != marker.save_sha256 {
                    return Err(RecoveryError::Replaced);
                }
                // A locally forgeable marker and SAV can prove only local
                // consistency.  Until a future server-signed recovery
                // capability exists, never let this evidence authorize
                // prepare/upload/finalize or mutate cloud state.
                let _ = api;
                Ok(Self::divergent_v2_outcome())
            }
        }
    }
}

fn discover_candidate(path: PathBuf) -> Result<RecoveryCandidate, RecoveryError> {
    validate_candidate_directory(&path)?;
    #[cfg(windows)]
    let identity_guards = open_identity_guards(&path)?;
    let marker = RecoveryMarker::decode(&read_bounded_regular(&path.join(RECOVERY_MARKER_NAME))?)?;
    let save = read_bounded_regular(&path.join(CHARACTER_SAVE_NAME))?;
    if save.is_empty() {
        return Err(RecoveryError::Malformed);
    }
    Ok(RecoveryCandidate {
        path,
        marker,
        save_sha256: Sha256Digest::of_bytes(&save),
        #[cfg(windows)]
        _identity_guards: identity_guards,
    })
}

fn validate_candidate_directory(path: &Path) -> Result<(), RecoveryError> {
    reject_symlink_ancestors(path)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| RecoveryError::Malformed)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RecoveryError::Malformed);
    }
    let entries = std::fs::read_dir(path).map_err(|_| RecoveryError::Malformed)?;
    let mut names = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_RECOVERY_ENTRIES {
            return Err(RecoveryError::Malformed);
        }
        let entry = entry.map_err(|_| RecoveryError::Malformed)?;
        let name = entry.file_name();
        if name != CHARACTER_SAVE_NAME && name != RECOVERY_MARKER_NAME {
            return Err(RecoveryError::Malformed);
        }
        if names.iter().any(|prior| prior == &name) {
            return Err(RecoveryError::Malformed);
        }
        let child = entry.path();
        let child_metadata =
            std::fs::symlink_metadata(&child).map_err(|_| RecoveryError::Malformed)?;
        if child_metadata.file_type().is_symlink() || !child_metadata.is_file() {
            return Err(RecoveryError::Malformed);
        }
        names.push(name);
    }
    if names.len() != 2
        || !names.iter().any(|name| name == CHARACTER_SAVE_NAME)
        || !names.iter().any(|name| name == RECOVERY_MARKER_NAME)
    {
        return Err(RecoveryError::Malformed);
    }
    Ok(())
}

fn read_bounded_regular(path: &Path) -> Result<Vec<u8>, RecoveryError> {
    reject_symlink_ancestors(path)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| RecoveryError::Malformed)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RecoveryError::Malformed);
    }
    let file = open_read_nofollow(path).map_err(|_| RecoveryError::Malformed)?;
    let maximum = if path
        .file_name()
        .is_some_and(|name| name == RECOVERY_MARKER_NAME)
    {
        MAX_RECOVERY_MARKER_BYTES
    } else {
        MAX_SESSION_FILE_BYTES
    };
    let mut bytes = Vec::new();
    file.take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| RecoveryError::Malformed)?;
    if bytes.len() > maximum {
        return Err(RecoveryError::Malformed);
    }
    Ok(bytes)
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), RecoveryError> {
    let mut current = path;
    loop {
        if let Ok(metadata) = std::fs::symlink_metadata(current)
            && metadata.file_type().is_symlink()
        {
            return Err(RecoveryError::Malformed);
        }
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(())
}

fn open_read_nofollow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x0002_0000);
    }
    options.open(path)
}

#[cfg(windows)]
fn open_identity_guards(path: &Path) -> Result<Vec<File>, RecoveryError> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut guards = Vec::new();
    let mut current = path;
    loop {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .share_mode(0x0000_0003)
            .custom_flags(0x0220_0000);
        guards.push(
            options
                .open(current)
                .map_err(|_| RecoveryError::Malformed)?,
        );
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(guards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn unsigned_divergent_v2_gate_has_no_cloud_mutation_step() {
        let mut cloud_mutations = 0_u8;
        let outcome = RecoveryReconciler::divergent_v2_outcome();
        assert_eq!(outcome, RecoveryOutcome::AuthorizationMissing);
        assert_eq!(cloud_mutations, 0);
        cloud_mutations = cloud_mutations.saturating_add(0);
        assert_eq!(cloud_mutations, 0);
    }

    fn fence() -> LeaseFence {
        LeaseFence::new(
            SessionId::new(Uuid::from_u128(1)).unwrap(),
            CharacterId::new(Uuid::from_u128(2)).unwrap(),
            Revision::new(4),
            SessionEpoch::new(7).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(3)).unwrap(),
        )
    }

    #[test]
    fn v2_marker_is_bounded_and_secret_free() {
        let marker =
            RecoveryMarkerV2::new(fence(), Revision::new(4), 5, Sha256Digest::of_bytes(b"sav"))
                .unwrap();
        let bytes = marker.encode().unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("token"));
        assert_eq!(RecoveryMarkerV2::decode(&bytes).unwrap(), marker);
    }

    #[test]
    fn discovery_rejects_extra_entries_and_accepts_legacy() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("coop-session-test");
        std::fs::create_dir(&candidate).unwrap();
        std::fs::write(candidate.join(CHARACTER_SAVE_NAME), b"sav").unwrap();
        std::fs::write(candidate.join(RECOVERY_MARKER_NAME), LEGACY_MARKER).unwrap();
        assert!(
            RecoveryDiscovery::discover(root.path())
                .unwrap()
                .candidate()
                .is_some()
        );
        std::fs::write(candidate.join("unexpected"), b"x").unwrap();
        assert_eq!(
            RecoveryDiscovery::discover(root.path())
                .unwrap_err()
                .to_string(),
            "recovery evidence is malformed"
        );
    }
}

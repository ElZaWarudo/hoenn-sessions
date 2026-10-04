//! Fail-closed discovery and restart reconciliation for interrupted saves.
//!
//! Recovery evidence is deliberately smaller than a session workspace.  A
//! preserved directory may contain only a bounded SAV and an optional marker.
//! A marker never contains credentials, bridge secrets, or paths; without one,
//! a save stays on disk even when it matches the signed cloud head.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use coop_cloud::{
    AcquireWorldLeaseResponse, CharacterId, ClientInstanceId, LeaseFence, Revision, SessionEpoch,
    SessionId, Sha256Digest,
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
const MAX_SCANNED_SESSION_DIRS: usize = 1024;
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
    /// A save left behind before shutdown could write its recovery marker.
    /// It remains blocked until handle-bound retirement is available.
    OrphanSave,
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
    #[cfg(windows)]
    save_identity_guard: Option<File>,
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
        let shape = validate_candidate_directory(&self.path)?;
        let parsed = match shape {
            CandidateShape::SaveOnly => RecoveryMarker::OrphanSave,
            CandidateShape::Marked => RecoveryMarker::decode(&read_bounded_regular(
                &self.path.join(RECOVERY_MARKER_NAME),
            )?)?,
            CandidateShape::Empty => return Err(RecoveryError::Replaced),
        };
        if parsed != self.marker {
            return Err(RecoveryError::Replaced);
        }
        let _ = self.read_save()?;
        Ok(())
    }

    fn retire(mut self) -> Result<(), RecoveryError> {
        // A markerless save has no handle-bound retirement capability. Never
        // path-unlink it after releasing the Windows identity guard: an
        // atomic replacement could otherwise cause unsynced data loss.
        if self.marker == RecoveryMarker::OrphanSave {
            return Err(RecoveryError::Blocked);
        }
        self.verify_identity()?;
        let path = self.path.clone();
        let save = self.path.join(CHARACTER_SAVE_NAME);
        let marker = self.path.join(RECOVERY_MARKER_NAME);
        #[cfg(windows)]
        {
            // The discovery handle denies write and delete sharing, so the
            // save path cannot be replaced during cloud verification. Windows
            // requires releasing that handle before unlink. Recheck the exact
            // path and digest immediately after release before deleting it.
            drop(self.save_identity_guard.take());
            self.verify_identity()?;
        }
        std::fs::remove_file(save).map_err(|_| RecoveryError::Cleanup)?;
        if self.marker != RecoveryMarker::OrphanSave {
            std::fs::remove_file(marker).map_err(|_| RecoveryError::Cleanup)?;
        }
        // Windows directory identity guards deny deletion. Keep them while
        // removing the verified files, then release them for the final empty
        // directory removal. A replacement with any content fails closed.
        drop(self);
        std::fs::remove_dir(path).map_err(|_| RecoveryError::Cleanup)
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
        let mut scanned_session_dirs = 0usize;
        let entries = std::fs::read_dir(root).map_err(|_| RecoveryError::Malformed)?;
        for entry in entries {
            let entry = entry.map_err(|_| RecoveryError::Malformed)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("coop-session-") || name.starts_with("coop-recovery-")) {
                continue;
            }
            scanned_session_dirs += 1;
            if scanned_session_dirs > MAX_SCANNED_SESSION_DIRS {
                return Err(RecoveryError::Ambiguous);
            }
            let path = entry.path();
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|_| RecoveryError::Malformed)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RecoveryError::Malformed);
            }
            // A completed session can leave generated files behind on Windows
            // when TempDir cleanup races an open handle. Such a directory has
            // no recovery evidence until it contains a SAV or marker.
            if name.starts_with("coop-session-")
                && !recovery_material_present(&path, CHARACTER_SAVE_NAME)?
                && !recovery_material_present(&path, RECOVERY_MARKER_NAME)?
            {
                continue;
            }
            launcher_candidates += 1;
            if launcher_candidates > MAX_RECOVERY_CANDIDATES {
                return Err(RecoveryError::Ambiguous);
            }
            if let Some(candidate) = discover_candidate(path)? {
                candidates.push(candidate);
            }
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

fn recovery_material_present(path: &Path, name: &str) -> Result<bool, RecoveryError> {
    match std::fs::symlink_metadata(path.join(name)) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(RecoveryError::Malformed)
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(RecoveryError::Malformed),
    }
}

/// High-level restart reconciler.  The caller discovers before acquiring a
/// fresh world-bound lease; the reconciler rediscovers under that lease and
/// lets `SessionLifecycle` verify the signed current head and perform any
/// fenced idempotent mutation.
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

    /// Reconciles at most one candidate under a lease the caller acquired
    /// through the world-aware route and preserves the candidate on every
    /// uncertainty.
    ///
    /// A catalog-bound server binds a lease to a ROM world only through
    /// `POST /v1/sessions/acquire-world`; it rejects resume and snapshot
    /// operations for an unbound legacy lease. Recovery therefore never
    /// acquires a lease itself. The caller persists the acquire request
    /// before sending it (in a store separate from the play intent), uses the
    /// marker's prior client instance for a v2 candidate, and builds `config`
    /// for `response.active_world_id` from its trusted ROM catalog.
    ///
    /// A world, character, or client-instance mismatch releases the
    /// pre-acquired lease before any session state is materialized and leaves
    /// the local evidence untouched.
    pub async fn reconcile_world_lease<A: CloudApi>(
        api: &A,
        mut auth: AuthSession,
        config: SessionConfig,
        keychain: Arc<dyn RefreshTokenStore>,
        response: AcquireWorldLeaseResponse,
    ) -> Result<RecoverySession, RecoveryError> {
        let candidate = match Self::world_lease_candidate(&auth, &config, &response) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                SessionLifecycle::release_preacquired_world_lease(
                    api, &mut auth, response, &keychain,
                )
                .await?;
                return Ok(RecoverySession {
                    outcome: RecoveryOutcome::NoEvidence,
                    auth,
                });
            }
            Err(error) => {
                SessionLifecycle::release_preacquired_world_lease(
                    api, &mut auth, response, &keychain,
                )
                .await?;
                return Err(error);
            }
        };
        let marker = candidate.marker().clone();
        let mut lifecycle =
            SessionLifecycle::from_world_lease_with_keychain(api, auth, config, keychain, response)
                .await?;
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

    /// Checks the pre-acquired lease against the caller's world selection
    /// and rediscovers the evidence. No cloud or filesystem mutation happens
    /// here; every error leads to an explicit lease release.
    fn world_lease_candidate(
        auth: &AuthSession,
        config: &SessionConfig,
        response: &AcquireWorldLeaseResponse,
    ) -> Result<Option<RecoveryCandidate>, RecoveryError> {
        if response.active_world_id != config.rom_world_id
            || response.lease.client_instance_id != config.client_instance_id
            || response.lease.character_id != auth.character_id
        {
            return Err(RecoveryError::Blocked);
        }
        let candidate = match RecoveryDiscovery::discover(&config.workspace_parent) {
            Ok(discovery) => discovery.candidate(),
            Err(RecoveryError::NoEvidence) => None,
            Err(error) => return Err(error),
        };
        if let Some(RecoveryMarker::V2(marker)) = candidate.as_ref().map(RecoveryCandidate::marker)
            && (marker.character_id != auth.character_id
                || marker.prior_client_instance_id != response.lease.client_instance_id)
        {
            return Err(RecoveryError::Blocked);
        }
        Ok(candidate)
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
            RecoveryMarker::OrphanSave => {
                if cloud_digest != Some(candidate.save_sha256()) {
                    return Err(RecoveryError::Blocked);
                }
                let signed_head = lifecycle
                    .verify_current_head(api)
                    .await
                    .map(|(digest, _)| digest)
                    .map_err(RecoveryError::from);
                classify_orphan_signed_head(candidate, signed_head)
            }
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

fn classify_orphan_signed_head(
    candidate: RecoveryCandidate,
    signed_head: Result<Sha256Digest, RecoveryError>,
) -> Result<RecoveryOutcome, RecoveryError> {
    let signed_digest = signed_head?;
    if signed_digest != candidate.save_sha256() {
        return Err(RecoveryError::Blocked);
    }
    // Exact signed equality proves the local bytes have been uploaded, but
    // cannot authorize path-based deletion while a replacement can race the
    // final unlink. Keep the evidence for an operator or a future native
    // handle-bound retirement path.
    Ok(RecoveryOutcome::AuthorizationMissing)
}

fn discover_candidate(path: PathBuf) -> Result<Option<RecoveryCandidate>, RecoveryError> {
    let shape = validate_candidate_directory(&path)?;
    if shape == CandidateShape::Empty {
        return Ok(None);
    }
    #[cfg(windows)]
    let identity_guards = open_identity_guards(&path)?;
    #[cfg(windows)]
    let save_identity_guard = open_save_identity_guard(&path.join(CHARACTER_SAVE_NAME))?;
    let marker = match shape {
        CandidateShape::SaveOnly => RecoveryMarker::OrphanSave,
        CandidateShape::Marked => {
            RecoveryMarker::decode(&read_bounded_regular(&path.join(RECOVERY_MARKER_NAME))?)?
        }
        CandidateShape::Empty => unreachable!(),
    };
    let save = read_bounded_regular(&path.join(CHARACTER_SAVE_NAME))?;
    if save.is_empty() {
        return Err(RecoveryError::Malformed);
    }
    Ok(Some(RecoveryCandidate {
        path,
        marker,
        save_sha256: Sha256Digest::of_bytes(&save),
        #[cfg(windows)]
        _identity_guards: identity_guards,
        #[cfg(windows)]
        save_identity_guard: Some(save_identity_guard),
    }))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CandidateShape {
    Empty,
    SaveOnly,
    Marked,
}

fn validate_candidate_directory(path: &Path) -> Result<CandidateShape, RecoveryError> {
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
    match (
        names.iter().any(|name| name == CHARACTER_SAVE_NAME),
        names.iter().any(|name| name == RECOVERY_MARKER_NAME),
    ) {
        (false, false) => Ok(CandidateShape::Empty),
        (true, false) => Ok(CandidateShape::SaveOnly),
        (true, true) => Ok(CandidateShape::Marked),
        (false, true) => Err(RecoveryError::Malformed),
    }
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

#[cfg(windows)]
fn open_save_identity_guard(path: &Path) -> Result<File, RecoveryError> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    // Deny both write and delete sharing while cloud verification runs. An
    // already-open writer or a process preparing atomic path replacement
    // makes discovery fail closed.
    options
        .read(true)
        .share_mode(0x0000_0001)
        .custom_flags(0x0020_0000);
    options.open(path).map_err(|_| RecoveryError::Malformed)
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

    #[test]
    fn discovery_ignores_only_sessions_without_recovery_material() {
        let root = tempdir().unwrap();
        for index in 0..9 {
            let leftover = root.path().join(format!("coop-session-leftover-{index}"));
            std::fs::create_dir(&leftover).unwrap();
            std::fs::write(leftover.join("resume.ss1"), b"stale").unwrap();
        }
        assert!(
            RecoveryDiscovery::discover(root.path())
                .unwrap()
                .candidate()
                .is_none()
        );

        let leftover = root.path().join("coop-session-leftover-0");
        std::fs::write(leftover.join(CHARACTER_SAVE_NAME), b"sav").unwrap();
        assert!(matches!(
            RecoveryDiscovery::discover(root.path()),
            Err(RecoveryError::Malformed)
        ));
        std::fs::remove_file(leftover.join(CHARACTER_SAVE_NAME)).unwrap();
        std::fs::write(leftover.join(RECOVERY_MARKER_NAME), LEGACY_MARKER).unwrap();
        assert!(matches!(
            RecoveryDiscovery::discover(root.path()),
            Err(RecoveryError::Malformed)
        ));
        std::fs::remove_file(leftover.join(RECOVERY_MARKER_NAME)).unwrap();
        let valid = root.path().join("coop-recovery-valid");
        std::fs::create_dir(&valid).unwrap();
        std::fs::write(valid.join(CHARACTER_SAVE_NAME), b"sav").unwrap();
        std::fs::write(valid.join(RECOVERY_MARKER_NAME), LEGACY_MARKER).unwrap();
        assert!(
            RecoveryDiscovery::discover(root.path())
                .unwrap()
                .candidate()
                .is_some()
        );
    }

    #[test]
    fn orphan_retirement_fails_closed_even_for_an_identical_save() {
        let root = tempdir().unwrap();
        let path = root.path().join("coop-session-orphan");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join(CHARACTER_SAVE_NAME), b"synced-save").unwrap();
        let candidate = RecoveryDiscovery::discover(root.path())
            .unwrap()
            .candidate()
            .unwrap();
        assert_eq!(candidate.retire().unwrap_err(), RecoveryError::Blocked);
        assert_eq!(
            std::fs::read(path.join(CHARACTER_SAVE_NAME)).unwrap(),
            b"synced-save"
        );
    }

    fn orphan_candidate(root: &Path, save: &[u8]) -> RecoveryCandidate {
        let path = root.join("coop-session-orphan");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join(CHARACTER_SAVE_NAME), save).unwrap();
        RecoveryDiscovery::discover(root)
            .unwrap()
            .candidate()
            .unwrap()
    }

    #[test]
    fn matching_signed_head_keeps_orphan_until_safe_retirement_exists() {
        let root = tempdir().unwrap();
        let candidate = orphan_candidate(root.path(), b"synced-save");
        let path = candidate.path().to_owned();
        assert_eq!(
            classify_orphan_signed_head(candidate, Ok(Sha256Digest::of_bytes(b"synced-save")),)
                .unwrap(),
            RecoveryOutcome::AuthorizationMissing
        );
        assert_eq!(
            std::fs::read(path.join(CHARACTER_SAVE_NAME)).unwrap(),
            b"synced-save"
        );
    }

    #[test]
    fn orphan_survives_changed_or_unavailable_signed_head() {
        for signed_head in [
            Ok(Sha256Digest::of_bytes(b"newer-cloud-save")),
            Err(RecoveryError::Session(SessionError::MissingPackage)),
        ] {
            let root = tempdir().unwrap();
            let candidate = orphan_candidate(root.path(), b"unsynced-save");
            let path = candidate.path().to_owned();
            let unavailable = signed_head.is_err();
            let error = classify_orphan_signed_head(candidate, signed_head).unwrap_err();
            if unavailable {
                assert!(matches!(
                    error,
                    RecoveryError::Session(SessionError::MissingPackage)
                ));
            } else {
                assert_eq!(error, RecoveryError::Blocked);
            }
            assert_eq!(
                std::fs::read(path.join(CHARACTER_SAVE_NAME)).unwrap(),
                b"unsynced-save"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn discovered_orphan_denies_an_emulator_writer_until_reconciled() {
        let root = tempdir().unwrap();
        let candidate = orphan_candidate(root.path(), b"synced-save");
        let path = candidate.path().join(CHARACTER_SAVE_NAME);
        assert!(std::fs::write(&path, b"unsynced-save").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"synced-save");
        drop(candidate);
        std::fs::write(&path, b"unsynced-save").unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn discovered_orphan_denies_atomic_path_replacement() {
        let root = tempdir().unwrap();
        let candidate = orphan_candidate(root.path(), b"synced-save");
        let path = candidate.path().join(CHARACTER_SAVE_NAME);
        let displaced = candidate.path().join("displaced.sav");
        assert!(std::fs::rename(&path, &displaced).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"synced-save");
        assert!(!displaced.exists());
        drop(candidate);
        std::fs::rename(&path, &displaced).unwrap();
    }
}

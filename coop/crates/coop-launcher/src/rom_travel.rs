//! Dormant, crash-recoverable journal for switching between registered ROM worlds.
//!
//! The caller must validate both save images and release artifacts. This module
//! records ordering and recovery state; it never launches a ROM or copies a save.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use coop_cloud::{
    CharacterId, ClientInstanceId, IdempotencyKey, Revision, RomHandoffRecoveryStatus,
    SessionEpoch, SessionId, Sha256Digest, SnapshotId,
};
use coop_protocol::RomWorldId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const FORMAT_VERSION: u16 = 3;
const MAX_RECORD_BYTES: u64 = 4096;
const MAX_RETAINED_ENTRIES: usize = 16;
const MAX_PORTAL_ID_BYTES: usize = 96;

/// The persistence boundary a successful journal write can claim on this
/// platform. `FileOnly` covers process interruption, but a sudden power loss
/// may discard a recently renamed directory entry on Windows filesystems
/// that refuse directory flushing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalSyncLevel {
    FileOnly,
    FileAndDirectory,
}

#[derive(Debug, Error)]
pub enum RomTravelError {
    #[error("travel journal I/O failed")]
    Io(#[from] io::Error),
    #[error("travel journal is corrupt or incomplete")]
    Corrupt,
    #[error("travel journal belongs to another character")]
    CharacterMismatch,
    #[error("ROM world is not in the trusted world registry")]
    UnknownWorld,
    #[error("travel transition does not match the current journal state")]
    Conflict,
    #[error("travel checkpoint is stale or exhausted")]
    Checkpoint,
    #[error("portal identity is invalid")]
    Portal,
    #[error("travel journal has reached its bounded capacity")]
    Full,
}

/// An intent is written before its external action; later phases record success.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelPhase {
    Idle,
    /// The source head and retry key are durable before the prepare request.
    PrepareIntent,
    Prepared,
    SourceSaved,
    /// Destination save is staged; the source remains authoritative.
    DestinationReady,
    Launched,
    /// Destination arrival has been verified, but commit is still pending.
    ArrivalAcknowledged,
    /// The destination is now the authoritative active world.
    Committed,
    /// Server abort is required before a staged destination can be discarded.
    AbortPending,
    Aborted,
}

/// The non-revision lease identity that fences a handoff against stale
/// launcher processes. The source revision is persisted separately because it
/// identifies the exact finalized snapshot used for the handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseFenceIdentity {
    pub session_id: SessionId,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl LeaseFenceIdentity {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
    ) -> Self {
        Self {
            session_id,
            session_epoch,
            client_instance_id,
        }
    }
}

/// Immutable data captured before the server handoff prepare request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TravelPreparation {
    pub source_snapshot_id: SnapshotId,
    pub source_revision: Revision,
    pub prepare_idempotency_key: IdempotencyKey,
    pub trusted_catalog_digest: Sha256Digest,
    pub lease_fence: LeaseFenceIdentity,
}

impl TravelPreparation {
    #[must_use]
    pub const fn new(
        source_snapshot_id: SnapshotId,
        source_revision: Revision,
        prepare_idempotency_key: IdempotencyKey,
        trusted_catalog_digest: Sha256Digest,
        lease_fence: LeaseFenceIdentity,
    ) -> Self {
        Self {
            source_snapshot_id,
            source_revision,
            prepare_idempotency_key,
            trusted_catalog_digest,
            lease_fence,
        }
    }
}

/// One complete snapshot in the rolling immutable-record journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TravelRecord {
    pub format_version: u16,
    pub sequence: u64,
    pub character_id: CharacterId,
    pub checkpoint: u64,
    pub active_world: RomWorldId,
    pub phase: TravelPhase,
    pub source_world: Option<RomWorldId>,
    pub destination_world: Option<RomWorldId>,
    pub portal_id: Option<String>,
    pub source_snapshot_id: Option<SnapshotId>,
    pub source_revision: Option<Revision>,
    pub server_stage_snapshot_id: Option<SnapshotId>,
    pub prepare_idempotency_key: Option<IdempotencyKey>,
    pub trusted_catalog_digest: Option<Sha256Digest>,
    pub lease_fence: Option<LeaseFenceIdentity>,
    /// Digest of the authoritative source head before calling prepare.
    pub source_head_save_sha256: Option<Sha256Digest>,
    pub source_save_sha256: Option<Sha256Digest>,
    pub destination_save_sha256: Option<Sha256Digest>,
    /// Expected nonce persisted before destination launch.
    pub arrival_nonce: Option<[u8; 16]>,
}

/// Evidence supplied by a future authenticated ROM arrival channel. The
/// journal checks correlation, but does not authenticate the channel itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrivalEvidence {
    pub checkpoint: u64,
    pub server_stage_snapshot_id: SnapshotId,
    pub destination_world: RomWorldId,
    pub portal_id: String,
    pub destination_save_sha256: Sha256Digest,
    pub nonce: [u8; 16],
    pub lease_fence: LeaseFenceIdentity,
}

/// A catalog-scoped travel journal. World IDs come from a trusted caller;
/// persisted records cannot register worlds or silently change identity.
/// Successful writes have the platform-specific guarantee in [`Self::sync_level`].
#[derive(Clone, Debug)]
pub struct RomTravelJournal {
    directory: PathBuf,
    character_id: CharacterId,
    worlds: BTreeSet<RomWorldId>,
}

impl RomTravelJournal {
    /// Describes the maximum synchronization claimed for successful writes.
    /// A file-only journal must be reconciled against actual save images after
    /// power loss before allowing further travel.
    #[must_use]
    pub const fn sync_level(&self) -> JournalSyncLevel {
        #[cfg(windows)]
        {
            JournalSyncLevel::FileOnly
        }
        #[cfg(not(windows))]
        {
            JournalSyncLevel::FileAndDirectory
        }
    }

    /// Construct a journal using the trusted release catalog's world IDs.
    ///
    /// # Errors
    /// Returns an error for an empty registry.
    pub fn new(
        directory: impl Into<PathBuf>,
        character_id: CharacterId,
        worlds: impl IntoIterator<Item = RomWorldId>,
    ) -> Result<Self, RomTravelError> {
        let worlds = worlds.into_iter().collect::<BTreeSet<_>>();
        if worlds.is_empty() {
            return Err(RomTravelError::UnknownWorld);
        }
        Ok(Self {
            directory: directory.into(),
            character_id,
            worlds,
        })
    }

    /// Return the last published state, or `None` before initialization.
    ///
    /// # Errors
    /// Fails closed on corrupt, missing, or foreign records.
    pub fn read(&self) -> Result<Option<TravelRecord>, RomTravelError> {
        let _lock = self.lock()?;
        self.read_locked()
    }

    /// Initialize a fresh character at a registered world. A repeated call is
    /// idempotent only when it names the existing initial state.
    ///
    /// # Errors
    /// Rejects an unknown world or a conflicting existing journal.
    pub fn initialize(&self, world: RomWorldId) -> Result<TravelRecord, RomTravelError> {
        self.require_world(world)?;
        let _lock = self.lock()?;
        if let Some(record) = self.read_locked()? {
            if record.sequence == 0 && record.active_world == world {
                return Ok(record);
            }
            return Err(RomTravelError::Conflict);
        }
        let record = TravelRecord {
            format_version: FORMAT_VERSION,
            sequence: 0,
            character_id: self.character_id,
            checkpoint: 0,
            active_world: world,
            phase: TravelPhase::Idle,
            source_world: None,
            destination_world: None,
            portal_id: None,
            source_snapshot_id: None,
            source_revision: None,
            server_stage_snapshot_id: None,
            prepare_idempotency_key: None,
            trusted_catalog_digest: None,
            lease_fence: None,
            source_head_save_sha256: None,
            source_save_sha256: None,
            destination_save_sha256: None,
            arrival_nonce: None,
        };
        self.append(&record)?;
        Ok(record)
    }

    /// Persist the source head and retry identity before sending HTTP prepare.
    /// The server alone chooses the destination, so it is absent here.
    ///
    /// # Errors
    /// Rejects stale or conflicting attempts and invalid source evidence.
    pub fn prepare_intent(
        &self,
        source: RomWorldId,
        portal_id: &str,
        checkpoint: u64,
        source_head_save_sha256: Sha256Digest,
        preparation: TravelPreparation,
    ) -> Result<TravelRecord, RomTravelError> {
        self.require_world(source)?;
        validate_portal(portal_id)?;
        validate_preparation(&preparation)?;
        if digest_is_zero(source_head_save_sha256) {
            return Err(RomTravelError::Conflict);
        }
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.phase == TravelPhase::PrepareIntent {
            return if current.active_world == source
                && current.source_world == Some(source)
                && current.destination_world.is_none()
                && current.portal_id.as_deref() == Some(portal_id)
                && current.checkpoint == checkpoint
                && current.source_snapshot_id == Some(preparation.source_snapshot_id)
                && current.source_revision == Some(preparation.source_revision)
                && current.prepare_idempotency_key == Some(preparation.prepare_idempotency_key)
                && current.trusted_catalog_digest == Some(preparation.trusted_catalog_digest)
                && current.lease_fence == Some(preparation.lease_fence)
                && current.source_head_save_sha256 == Some(source_head_save_sha256)
            {
                Ok(current)
            } else {
                Err(RomTravelError::Conflict)
            };
        }
        if !matches!(
            current.phase,
            TravelPhase::Idle | TravelPhase::Committed | TravelPhase::Aborted
        ) || current.active_world != source
        {
            return Err(RomTravelError::Conflict);
        }
        if checkpoint <= current.checkpoint || checkpoint == u64::MAX {
            return Err(RomTravelError::Checkpoint);
        }
        let next = TravelRecord {
            sequence: next_sequence(current.sequence)?,
            checkpoint,
            phase: TravelPhase::PrepareIntent,
            source_world: Some(source),
            destination_world: None,
            portal_id: Some(portal_id.to_owned()),
            source_snapshot_id: Some(preparation.source_snapshot_id),
            source_revision: Some(preparation.source_revision),
            server_stage_snapshot_id: None,
            prepare_idempotency_key: Some(preparation.prepare_idempotency_key),
            trusted_catalog_digest: Some(preparation.trusted_catalog_digest),
            lease_fence: Some(preparation.lease_fence),
            source_head_save_sha256: Some(source_head_save_sha256),
            source_save_sha256: None,
            destination_save_sha256: None,
            arrival_nonce: None,
            ..current
        };
        self.append(&next)?;
        Ok(next)
    }

    /// Advance a matching durable intent after the server resolves the destination.
    ///
    /// # Errors
    /// Rejects stale or conflicting requests, unregistered worlds, and invalid portals.
    pub fn begin(
        &self,
        source: RomWorldId,
        destination: RomWorldId,
        portal_id: &str,
        checkpoint: u64,
        preparation: TravelPreparation,
    ) -> Result<TravelRecord, RomTravelError> {
        self.require_world(source)?;
        self.require_world(destination)?;
        if source == destination {
            return Err(RomTravelError::Conflict);
        }
        validate_portal(portal_id)?;
        validate_preparation(&preparation)?;
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if matches!(current.phase, TravelPhase::Prepared)
            && current.source_world == Some(source)
            && current.destination_world == Some(destination)
            && current.portal_id.as_deref() == Some(portal_id)
            && current.checkpoint == checkpoint
            && current.source_snapshot_id == Some(preparation.source_snapshot_id)
            && current.source_revision == Some(preparation.source_revision)
            && current.prepare_idempotency_key == Some(preparation.prepare_idempotency_key)
            && current.trusted_catalog_digest == Some(preparation.trusted_catalog_digest)
            && current.lease_fence == Some(preparation.lease_fence)
        {
            return Ok(current);
        }
        if current.phase != TravelPhase::PrepareIntent
            || current.active_world != source
            || current.source_world != Some(source)
            || current.portal_id.as_deref() != Some(portal_id)
            || current.checkpoint != checkpoint
            || current.source_snapshot_id != Some(preparation.source_snapshot_id)
            || current.source_revision != Some(preparation.source_revision)
            || current.prepare_idempotency_key != Some(preparation.prepare_idempotency_key)
            || current.trusted_catalog_digest != Some(preparation.trusted_catalog_digest)
            || current.lease_fence != Some(preparation.lease_fence)
        {
            return Err(RomTravelError::Conflict);
        }
        let next = TravelRecord {
            sequence: next_sequence(current.sequence)?,
            phase: TravelPhase::Prepared,
            destination_world: Some(destination),
            ..current
        };
        self.append(&next)?;
        Ok(next)
    }

    /// Record the source save digest after its checkpoint succeeds.
    ///
    /// # Errors
    /// Rejects a different checkpoint, phase, or digest.
    pub fn source_saved(
        &self,
        checkpoint: u64,
        digest: Sha256Digest,
    ) -> Result<TravelRecord, RomTravelError> {
        if digest_is_zero(digest) {
            return Err(RomTravelError::Conflict);
        }
        self.advance(
            checkpoint,
            TravelPhase::Prepared,
            TravelPhase::SourceSaved,
            Some(digest),
        )
    }

    /// Record the destination save digest and expected arrival nonce
    /// after projection succeeds, before attempting to start the destination.
    ///
    /// # Errors
    /// Rejects a different checkpoint, phase, or digest.
    pub fn destination_ready(
        &self,
        checkpoint: u64,
        server_stage_snapshot_id: SnapshotId,
        digest: Sha256Digest,
        expected_arrival_nonce: [u8; 16],
    ) -> Result<TravelRecord, RomTravelError> {
        if expected_arrival_nonce == [0; 16] || digest_is_zero(digest) {
            return Err(RomTravelError::Conflict);
        }
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.source_snapshot_id == Some(server_stage_snapshot_id) {
            return Err(RomTravelError::Conflict);
        }
        if current.phase == TravelPhase::DestinationReady {
            return if current.server_stage_snapshot_id == Some(server_stage_snapshot_id)
                && current.destination_save_sha256 == Some(digest)
                && current.arrival_nonce == Some(expected_arrival_nonce)
            {
                Ok(current)
            } else {
                Err(RomTravelError::Conflict)
            };
        }
        if current.phase != TravelPhase::SourceSaved {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::DestinationReady;
        next.server_stage_snapshot_id = Some(server_stage_snapshot_id);
        next.destination_save_sha256 = Some(digest);
        next.arrival_nonce = Some(expected_arrival_nonce);
        self.append(&next)?;
        Ok(next)
    }

    /// Record a successful destination process start. This alone never
    /// authorizes changing the active-world pointer.
    ///
    /// # Errors
    /// Rejects a different checkpoint or out-of-order phase.
    pub fn launched(&self, checkpoint: u64) -> Result<TravelRecord, RomTravelError> {
        self.advance(
            checkpoint,
            TravelPhase::DestinationReady,
            TravelPhase::Launched,
            None,
        )
    }

    /// Record a correlated arrival acknowledgement from the destination ROM.
    /// A future caller must authenticate the channel and prove the save digest.
    ///
    /// # Errors
    /// Rejects wrong world, portal, digest, checkpoint, nonce, or phase.
    pub fn arrival_acknowledged(
        &self,
        evidence: &ArrivalEvidence,
    ) -> Result<TravelRecord, RomTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if evidence.checkpoint != current.checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if evidence.nonce == [0; 16]
            || current.arrival_nonce != Some(evidence.nonce)
            || current.server_stage_snapshot_id != Some(evidence.server_stage_snapshot_id)
            || current.destination_world != Some(evidence.destination_world)
            || current.portal_id.as_deref() != Some(evidence.portal_id.as_str())
            || current.destination_save_sha256 != Some(evidence.destination_save_sha256)
            || current.lease_fence != Some(evidence.lease_fence)
        {
            return Err(RomTravelError::Conflict);
        }
        if current.phase == TravelPhase::ArrivalAcknowledged {
            return if current.arrival_nonce == Some(evidence.nonce)
                && current.server_stage_snapshot_id == Some(evidence.server_stage_snapshot_id)
            {
                Ok(current)
            } else {
                Err(RomTravelError::Conflict)
            };
        }
        if current.phase != TravelPhase::Launched {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::ArrivalAcknowledged;
        self.append(&next)?;
        Ok(next)
    }

    /// Atomically publish the destination as the active world. A restart from
    /// any earlier phase must resume or explicitly reconcile that phase first.
    ///
    /// # Errors
    /// Rejects a different checkpoint or an unprepared destination.
    pub fn commit(
        &self,
        checkpoint: u64,
        server_stage_snapshot_id: SnapshotId,
        destination_save_sha256: Sha256Digest,
        lease_fence: LeaseFenceIdentity,
    ) -> Result<TravelRecord, RomTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.phase == TravelPhase::Committed {
            return if current.server_stage_snapshot_id == Some(server_stage_snapshot_id)
                && current.destination_save_sha256 == Some(destination_save_sha256)
                && current.lease_fence == Some(lease_fence)
            {
                Ok(current)
            } else {
                Err(RomTravelError::Conflict)
            };
        }
        if current.phase != TravelPhase::ArrivalAcknowledged
            || current.server_stage_snapshot_id != Some(server_stage_snapshot_id)
            || current.destination_save_sha256 != Some(destination_save_sha256)
            || current.lease_fence != Some(lease_fence)
        {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::Committed;
        next.active_world = next.destination_world.ok_or(RomTravelError::Corrupt)?;
        self.append(&next)?;
        Ok(next)
    }

    /// Request abort of a staged destination. The source remains authoritative,
    /// but the stage and retry identity remain fenced until the server confirms
    /// abort. A caller must not retry travel while this phase is pending.
    /// Before destination staging, a prepare request may have succeeded with
    /// its response lost; the original retry key must remain available for
    /// reconciliation. Source save evidence remains in the journal.
    ///
    /// # Errors
    /// Rejects an incorrect checkpoint or travel already acknowledged.
    pub fn abort_to_source(&self, checkpoint: u64) -> Result<TravelRecord, RomTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.phase == TravelPhase::AbortPending || current.phase == TravelPhase::Aborted {
            return Ok(current);
        }
        if !matches!(
            current.phase,
            TravelPhase::DestinationReady | TravelPhase::Launched
        ) || current.server_stage_snapshot_id.is_none()
        {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::AbortPending;
        self.append(&next)?;
        Ok(next)
    }

    /// Confirm that the server has aborted the exact staged snapshot before
    /// allowing another travel intent. The caller must authenticate that result.
    ///
    /// # Errors
    /// Rejects an unrelated checkpoint, stage, or phase.
    pub fn confirm_server_abort(
        &self,
        checkpoint: u64,
        server_stage_snapshot_id: SnapshotId,
    ) -> Result<TravelRecord, RomTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.server_stage_snapshot_id != Some(server_stage_snapshot_id) {
            return Err(RomTravelError::Conflict);
        }
        if current.phase == TravelPhase::Aborted {
            return Ok(current);
        }
        if current.phase != TravelPhase::AbortPending {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::Aborted;
        self.append(&next)?;
        Ok(next)
    }

    /// Settle an exact server key that has been durably aborted while the
    /// source remains authoritative. This also covers an acknowledged
    /// offline arrival whose uncommitted stage expired before commit. The
    /// caller must obtain this status through the current source lease's
    /// authenticated reconcile operation; a live `Staged` status must be
    /// aborted on the server first.
    pub fn reconcile_aborted_prepare(
        &self,
        checkpoint: u64,
        status: &RomHandoffRecoveryStatus,
    ) -> Result<TravelRecord, RomTravelError> {
        let RomHandoffRecoveryStatus::Aborted {
            stage_id,
            source_snapshot_id,
            source_world_id,
            expected_revision,
            idempotency_key,
        } = status
        else {
            return Err(RomTravelError::Conflict);
        };
        self.require_world(*source_world_id)?;
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.active_world != *source_world_id
            || current.source_world != Some(*source_world_id)
            || current.source_snapshot_id != Some(*source_snapshot_id)
            || current.source_revision != Some(*expected_revision)
            || current.prepare_idempotency_key != Some(*idempotency_key)
            || *stage_id == *source_snapshot_id
            || current
                .server_stage_snapshot_id
                .is_some_and(|known| known != *stage_id)
        {
            return Err(RomTravelError::Conflict);
        }
        if current.phase == TravelPhase::Aborted {
            return Ok(current);
        }
        let phase_matches_stage = match current.server_stage_snapshot_id {
            None => matches!(
                current.phase,
                TravelPhase::PrepareIntent | TravelPhase::Prepared | TravelPhase::SourceSaved
            ),
            Some(_) => matches!(
                current.phase,
                TravelPhase::DestinationReady
                    | TravelPhase::Launched
                    | TravelPhase::ArrivalAcknowledged
                    | TravelPhase::AbortPending
            ),
        };
        if !phase_matches_stage {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = TravelPhase::Aborted;
        next.server_stage_snapshot_id = Some(*stage_id);
        self.append(&next)?;
        Ok(next)
    }

    fn advance(
        &self,
        checkpoint: u64,
        expected: TravelPhase,
        target: TravelPhase,
        digest: Option<Sha256Digest>,
    ) -> Result<TravelRecord, RomTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(RomTravelError::Conflict)?;
        if current.checkpoint != checkpoint {
            return Err(RomTravelError::Checkpoint);
        }
        if current.phase == target {
            let matches = match target {
                TravelPhase::SourceSaved => current.source_save_sha256 == digest,
                TravelPhase::Committed => true,
                TravelPhase::Launched => true,
                _ => false,
            };
            return if matches {
                Ok(current)
            } else {
                Err(RomTravelError::Conflict)
            };
        }
        if current.phase != expected {
            return Err(RomTravelError::Conflict);
        }
        let mut next = current;
        next.sequence = next_sequence(next.sequence)?;
        next.phase = target;
        match target {
            TravelPhase::SourceSaved => next.source_save_sha256 = digest,
            TravelPhase::Committed => {
                next.active_world = next.destination_world.ok_or(RomTravelError::Corrupt)?;
            }
            TravelPhase::Launched => {}
            _ => return Err(RomTravelError::Conflict),
        }
        self.append(&next)?;
        Ok(next)
    }

    fn require_world(&self, world: RomWorldId) -> Result<(), RomTravelError> {
        if self.worlds.contains(&world) {
            Ok(())
        } else {
            Err(RomTravelError::UnknownWorld)
        }
    }

    fn lock(&self) -> Result<File, RomTravelError> {
        // The caller must provision this directory durably before creating a
        // journal. Creating it here would also require syncing every newly
        // created parent directory before a record could claim durability.
        if !fs::metadata(&self.directory)?.is_dir() {
            return Err(RomTravelError::Corrupt);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(self.directory.join(".lock"))?;
        file.lock()?;
        Ok(file)
    }

    fn read_locked(&self) -> Result<Option<TravelRecord>, RomTravelError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(RomTravelError::Corrupt);
            };
            if name == ".lock" {
                continue;
            }
            if name.starts_with(".tmp-") {
                fs::remove_file(entry.path())?;
                continue;
            }
            if !name.ends_with(".json") || name.len() != 25 {
                return Err(RomTravelError::Corrupt);
            }
            let sequence = name[..20]
                .parse::<u64>()
                .map_err(|_| RomTravelError::Corrupt)?;
            if !entry.file_type()?.is_file() {
                return Err(RomTravelError::Corrupt);
            }
            entries.push((sequence, entry.path()));
            if entries.len() > MAX_RETAINED_ENTRIES {
                return Err(RomTravelError::Full);
            }
        }
        entries.sort_unstable_by_key(|(sequence, _)| *sequence);
        let mut previous: Option<TravelRecord> = None;
        for (index, (sequence, path)) in entries.into_iter().enumerate() {
            if index > 0
                && previous
                    .as_ref()
                    .is_none_or(|record| record.sequence.checked_add(1) != Some(sequence))
            {
                return Err(RomTravelError::Corrupt);
            }
            let file = File::open(path)?;
            let mut bytes = Vec::new();
            file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_RECORD_BYTES {
                return Err(RomTravelError::Corrupt);
            }
            let record: TravelRecord =
                serde_json::from_slice(&bytes).map_err(|_| RomTravelError::Corrupt)?;
            self.validate(&record, previous.as_ref(), sequence)?;
            previous = Some(record);
        }
        Ok(previous)
    }

    fn validate(
        &self,
        record: &TravelRecord,
        previous: Option<&TravelRecord>,
        sequence: u64,
    ) -> Result<(), RomTravelError> {
        if record.format_version != FORMAT_VERSION || record.sequence != sequence {
            return Err(RomTravelError::Corrupt);
        }
        if record.character_id != self.character_id {
            return Err(RomTravelError::CharacterMismatch);
        }
        self.require_world(record.active_world)?;
        if record.phase == TravelPhase::Idle {
            if sequence != 0
                || previous.is_some()
                || record.checkpoint != 0
                || record.source_world.is_some()
                || record.destination_world.is_some()
                || record.portal_id.is_some()
                || record.source_snapshot_id.is_some()
                || record.source_revision.is_some()
                || record.server_stage_snapshot_id.is_some()
                || record.prepare_idempotency_key.is_some()
                || record.trusted_catalog_digest.is_some()
                || record.lease_fence.is_some()
                || record.source_head_save_sha256.is_some()
                || record.source_save_sha256.is_some()
                || record.destination_save_sha256.is_some()
                || record.arrival_nonce.is_some()
            {
                return Err(RomTravelError::Corrupt);
            }
            return Ok(());
        }
        if sequence == 0 || record.checkpoint == 0 || record.checkpoint == u64::MAX {
            return Err(RomTravelError::Corrupt);
        }
        let source = record.source_world.ok_or(RomTravelError::Corrupt)?;
        self.require_world(source)?;
        if record.active_world != source && record.phase != TravelPhase::Committed {
            return Err(RomTravelError::Corrupt);
        }
        let destination = record.destination_world;
        if let Some(destination) = destination {
            self.require_world(destination)?;
            if source == destination {
                return Err(RomTravelError::Corrupt);
            }
        } else if !matches!(
            record.phase,
            TravelPhase::PrepareIntent | TravelPhase::Aborted
        ) {
            return Err(RomTravelError::Corrupt);
        }
        if record
            .portal_id
            .as_deref()
            .is_none_or(|portal| validate_portal(portal).is_err())
        {
            return Err(RomTravelError::Corrupt);
        }
        let source_snapshot_id = record.source_snapshot_id.ok_or(RomTravelError::Corrupt)?;
        let source_revision = record.source_revision.ok_or(RomTravelError::Corrupt)?;
        if source_revision.is_initial()
            || record
                .server_stage_snapshot_id
                .is_some_and(|id| id == source_snapshot_id)
            || record.prepare_idempotency_key.is_none()
            || record.trusted_catalog_digest.is_none_or(digest_is_zero)
            || record.lease_fence.is_none()
            || record.source_head_save_sha256.is_none_or(digest_is_zero)
        {
            return Err(RomTravelError::Corrupt);
        }
        if previous.is_none() {
            let valid = match record.phase {
                TravelPhase::PrepareIntent => {
                    destination.is_none()
                        && record.active_world == source
                        && record.server_stage_snapshot_id.is_none()
                        && record.source_save_sha256.is_none()
                        && record.destination_save_sha256.is_none()
                        && record.arrival_nonce.is_none()
                }
                TravelPhase::Prepared => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_none()
                        && record.source_save_sha256.is_none()
                        && record.destination_save_sha256.is_none()
                        && record.arrival_nonce.is_none()
                }
                TravelPhase::SourceSaved => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_none()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.destination_save_sha256.is_none()
                        && record.arrival_nonce.is_none()
                }
                TravelPhase::DestinationReady => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_some()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record
                            .destination_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
                }
                TravelPhase::Launched => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_some()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record
                            .destination_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
                }
                TravelPhase::ArrivalAcknowledged => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_some()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record
                            .destination_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
                }
                TravelPhase::Committed => {
                    Some(record.active_world) == destination
                        && record.server_stage_snapshot_id.is_some()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record
                            .destination_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
                }
                TravelPhase::AbortPending => {
                    record.active_world == source
                        && destination.is_some()
                        && record.server_stage_snapshot_id.is_some()
                        && record
                            .source_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record
                            .destination_save_sha256
                            .is_some_and(|digest| !digest_is_zero(digest))
                        && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
                }
                TravelPhase::Aborted => {
                    record.active_world == source
                        && record.server_stage_snapshot_id.is_some()
                        && match destination {
                            None => {
                                record.source_save_sha256.is_none()
                                    && record.destination_save_sha256.is_none()
                                    && record.arrival_nonce.is_none()
                            }
                            Some(_) => {
                                (record.destination_save_sha256.is_none()
                                    && record.arrival_nonce.is_none()
                                    && record
                                        .source_save_sha256
                                        .is_none_or(|digest| !digest_is_zero(digest)))
                                    || (record
                                        .source_save_sha256
                                        .is_some_and(|digest| !digest_is_zero(digest))
                                        && record
                                            .destination_save_sha256
                                            .is_some_and(|digest| !digest_is_zero(digest))
                                        && record
                                            .arrival_nonce
                                            .is_some_and(|nonce| nonce != [0; 16]))
                            }
                        }
                }
                TravelPhase::Idle => false,
            };
            return if valid {
                Ok(())
            } else {
                Err(RomTravelError::Corrupt)
            };
        }
        let previous = previous.ok_or(RomTravelError::Corrupt)?;
        let entering = matches!(
            previous.phase,
            TravelPhase::Idle | TravelPhase::Committed | TravelPhase::Aborted
        ) && record.phase == TravelPhase::PrepareIntent;
        if entering {
            if record.checkpoint <= previous.checkpoint
                || record.active_world != source
                || previous.active_world != source
                || record.server_stage_snapshot_id.is_some()
                || record.destination_world.is_some()
                || record.source_save_sha256.is_some()
                || record.destination_save_sha256.is_some()
                || record.arrival_nonce.is_some()
            {
                return Err(RomTravelError::Corrupt);
            }
            return Ok(());
        }
        if record.checkpoint != previous.checkpoint
            || record.source_world != previous.source_world
            || record.portal_id != previous.portal_id
            || record.source_snapshot_id != previous.source_snapshot_id
            || record.source_revision != previous.source_revision
            || record.prepare_idempotency_key != previous.prepare_idempotency_key
            || record.trusted_catalog_digest != previous.trusted_catalog_digest
            || record.lease_fence != previous.lease_fence
            || record.source_head_save_sha256 != previous.source_head_save_sha256
        {
            return Err(RomTravelError::Corrupt);
        }
        if record.destination_world != previous.destination_world
            && !(previous.phase == TravelPhase::PrepareIntent
                && record.phase == TravelPhase::Prepared
                && previous.destination_world.is_none()
                && record.destination_world.is_some())
        {
            return Err(RomTravelError::Corrupt);
        }
        let valid = match (previous.phase, record.phase) {
            (TravelPhase::PrepareIntent, TravelPhase::Prepared) => {
                record.active_world == source
                    && record.server_stage_snapshot_id.is_none()
                    && record.source_save_sha256.is_none()
                    && record.destination_save_sha256.is_none()
                    && record.arrival_nonce.is_none()
            }
            (TravelPhase::Prepared, TravelPhase::SourceSaved) => {
                record.active_world == source
                    && record.server_stage_snapshot_id.is_none()
                    && record
                        .source_save_sha256
                        .is_some_and(|digest| !digest_is_zero(digest))
                    && record.destination_save_sha256.is_none()
                    && record.arrival_nonce.is_none()
            }
            (TravelPhase::SourceSaved, TravelPhase::DestinationReady) => {
                record.active_world == source
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.server_stage_snapshot_id.is_some()
                    && record
                        .destination_save_sha256
                        .is_some_and(|digest| !digest_is_zero(digest))
                    && record.arrival_nonce.is_some_and(|nonce| nonce != [0; 16])
            }
            (TravelPhase::DestinationReady, TravelPhase::Launched) => {
                record.active_world == source
                    && record.server_stage_snapshot_id == previous.server_stage_snapshot_id
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            (TravelPhase::Launched, TravelPhase::ArrivalAcknowledged) => {
                record.active_world == source
                    && record.server_stage_snapshot_id == previous.server_stage_snapshot_id
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            (TravelPhase::ArrivalAcknowledged, TravelPhase::Committed) => {
                Some(record.active_world) == destination
                    && record.server_stage_snapshot_id == previous.server_stage_snapshot_id
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            (TravelPhase::DestinationReady | TravelPhase::Launched, TravelPhase::AbortPending)
            | (TravelPhase::AbortPending, TravelPhase::Aborted) => {
                record.active_world == source
                    && record.server_stage_snapshot_id == previous.server_stage_snapshot_id
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            (
                TravelPhase::DestinationReady
                | TravelPhase::Launched
                | TravelPhase::ArrivalAcknowledged,
                TravelPhase::Aborted,
            ) => {
                record.active_world == source
                    && record.server_stage_snapshot_id == previous.server_stage_snapshot_id
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            (
                TravelPhase::PrepareIntent | TravelPhase::Prepared | TravelPhase::SourceSaved,
                TravelPhase::Aborted,
            ) => {
                record.active_world == source
                    && previous.server_stage_snapshot_id.is_none()
                    && record.server_stage_snapshot_id.is_some()
                    && record.source_save_sha256 == previous.source_save_sha256
                    && record.destination_save_sha256 == previous.destination_save_sha256
                    && record.arrival_nonce == previous.arrival_nonce
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(RomTravelError::Corrupt)
        }
    }

    fn append(&self, record: &TravelRecord) -> Result<(), RomTravelError> {
        let bytes = serde_json::to_vec(record).map_err(|_| RomTravelError::Corrupt)?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(RomTravelError::Corrupt);
        }
        let target = self.directory.join(format!("{:020}.json", record.sequence));
        let temporary = self
            .directory
            .join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            if target.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "journal entry already exists",
                ));
            }
            fs::rename(&temporary, &target)?;
            sync_directory(&self.directory)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(RomTravelError::Io)?;
        self.prune_locked()?;
        Ok(())
    }

    fn prune_locked(&self) -> Result<(), RomTravelError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(RomTravelError::Corrupt)?;
            if name.ends_with(".json") && name.len() == 25 {
                entries.push(entry.path());
            }
        }
        entries.sort_unstable();
        // Keep the latest transition pair. Removing oldest first leaves a
        // contiguous suffix even if power fails during cleanup.
        let remove_count = entries.len().saturating_sub(2);
        for path in entries.into_iter().take(remove_count) {
            fs::remove_file(path)?;
        }
        if remove_count > 0 {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_BACKUP_SEMANTICS permits a directory handle. FlushFileBuffers
    // requires GENERIC_WRITE, which OpenOptions::write requests. Some Windows
    // filesystems still reject directory flushing. Report only FileOnly
    // synchronization for every Windows success, even if this flush works.
    match OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0x0000_0007)
        .custom_flags(0x0200_0000)
        .open(path)
        .and_then(|directory| directory.sync_all())
    {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn next_sequence(current: u64) -> Result<u64, RomTravelError> {
    current.checked_add(1).ok_or(RomTravelError::Full)
}

fn validate_preparation(preparation: &TravelPreparation) -> Result<(), RomTravelError> {
    if preparation.source_revision.is_initial()
        || digest_is_zero(preparation.trusted_catalog_digest)
    {
        Err(RomTravelError::Conflict)
    } else {
        Ok(())
    }
}

fn digest_is_zero(digest: Sha256Digest) -> bool {
    digest.as_bytes().iter().all(|byte| *byte == 0)
}

fn validate_portal(portal: &str) -> Result<(), RomTravelError> {
    if portal.len() > MAX_PORTAL_ID_BYTES
        || !portal
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        || !portal
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        Err(RomTravelError::Portal)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use uuid::Uuid;

    fn world(id: u16) -> RomWorldId {
        RomWorldId::new(id).unwrap()
    }

    fn snapshot(id: u64) -> SnapshotId {
        SnapshotId::new(Uuid::from_u128(u128::from(id) + 1000)).unwrap()
    }

    fn idempotency_key(id: u64) -> IdempotencyKey {
        IdempotencyKey::new(Uuid::from_u128(u128::from(id) + 2000)).unwrap()
    }

    fn digest(value: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([value; 32])
    }

    fn fence(id: u64) -> LeaseFenceIdentity {
        LeaseFenceIdentity::new(
            SessionId::new(Uuid::from_u128(u128::from(id) + 3000)).unwrap(),
            SessionEpoch::new(id as u32).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(u128::from(id) + 4000)).unwrap(),
        )
    }

    fn preparation(id: u64) -> TravelPreparation {
        TravelPreparation::new(
            snapshot(id),
            Revision::new(id),
            idempotency_key(id),
            digest(0xcc),
            fence(id),
        )
    }

    fn stage_id(checkpoint: u64) -> SnapshotId {
        SnapshotId::new(Uuid::from_u128(u128::from(checkpoint) + 5000)).unwrap()
    }

    fn fixture() -> (tempfile::TempDir, RomTravelJournal) {
        let root = tempfile::tempdir().unwrap();
        let character = CharacterId::new(Uuid::new_v4()).unwrap();
        let journal =
            RomTravelJournal::new(root.path(), character, [world(1), world(2), world(3)]).unwrap();
        (root, journal)
    }

    fn begin(
        journal: &RomTravelJournal,
        source: RomWorldId,
        destination: RomWorldId,
        portal: &str,
        checkpoint: u64,
    ) -> TravelRecord {
        if journal.read().unwrap().unwrap().phase != TravelPhase::Prepared {
            journal
                .prepare_intent(
                    source,
                    portal,
                    checkpoint,
                    digest(0xaa),
                    preparation(checkpoint),
                )
                .unwrap();
        }
        journal
            .begin(
                source,
                destination,
                portal,
                checkpoint,
                preparation(checkpoint),
            )
            .unwrap()
    }

    fn acknowledge(
        journal: &RomTravelJournal,
        checkpoint: u64,
        destination: RomWorldId,
        portal: &str,
    ) {
        journal.launched(checkpoint).unwrap();
        journal
            .arrival_acknowledged(&ArrivalEvidence {
                checkpoint,
                server_stage_snapshot_id: stage_id(checkpoint),
                destination_world: destination,
                portal_id: portal.to_owned(),
                destination_save_sha256: digest(2),
                nonce: [checkpoint as u8; 16],
                lease_fence: fence(checkpoint),
            })
            .unwrap();
    }

    fn stage(journal: &RomTravelJournal, checkpoint: u64) {
        journal.source_saved(checkpoint, digest(1)).unwrap();
        journal
            .destination_ready(
                checkpoint,
                stage_id(checkpoint),
                digest(2),
                [checkpoint as u8; 16],
            )
            .unwrap();
    }

    fn commit(journal: &RomTravelJournal, checkpoint: u64) -> TravelRecord {
        journal
            .commit(
                checkpoint,
                stage_id(checkpoint),
                digest(2),
                fence(checkpoint),
            )
            .unwrap()
    }

    #[test]
    fn sync_contract_and_fresh_initialization() {
        let (root, journal) = fixture();
        #[cfg(windows)]
        assert_eq!(journal.sync_level(), JournalSyncLevel::FileOnly);
        #[cfg(not(windows))]
        assert_eq!(journal.sync_level(), JournalSyncLevel::FileAndDirectory);
        assert!(journal.read().unwrap().is_none());
        journal.initialize(world(1)).unwrap();
        let reopened = RomTravelJournal::new(
            root.path(),
            journal.character_id,
            [world(1), world(2), world(3)],
        )
        .unwrap();
        assert_eq!(reopened.read().unwrap().unwrap().active_world, world(1));
        let missing = RomTravelJournal::new(
            root.path().join("unprovisioned"),
            journal.character_id,
            [world(1)],
        )
        .unwrap();
        assert!(matches!(
            missing.initialize(world(1)),
            Err(RomTravelError::Io(_))
        ));
    }

    #[test]
    fn three_world_itinerary_and_idempotent_replay() {
        let (_root, journal) = fixture();
        assert_eq!(
            journal.initialize(world(1)).unwrap(),
            journal.initialize(world(1)).unwrap()
        );
        let prepared = begin(&journal, world(1), world(2), "to_cormoria", 10);
        assert_eq!(
            prepared,
            begin(&journal, world(1), world(2), "to_cormoria", 10)
        );
        assert_eq!(prepared.active_world, world(1));
        let saved = journal.source_saved(10, digest(1)).unwrap();
        assert_eq!(saved, journal.source_saved(10, digest(1)).unwrap());
        assert!(matches!(
            journal.source_saved(10, digest(9)),
            Err(RomTravelError::Conflict)
        ));
        let ready = journal
            .destination_ready(10, stage_id(10), digest(2), [10; 16])
            .unwrap();
        assert_eq!(ready.active_world, world(1));
        assert_eq!(
            ready,
            journal
                .destination_ready(10, stage_id(10), digest(2), [10; 16])
                .unwrap()
        );
        assert_eq!(ready.server_stage_snapshot_id, Some(stage_id(10)));
        assert_eq!(ready.destination_save_sha256, Some(digest(2)));
        assert!(matches!(
            journal.commit(10, stage_id(10), digest(2), fence(10)),
            Err(RomTravelError::Conflict)
        ));
        journal.launched(10).unwrap();
        for evidence in [
            ArrivalEvidence {
                checkpoint: 10,
                server_stage_snapshot_id: stage_id(99),
                destination_world: world(2),
                portal_id: "to_cormoria".to_owned(),
                destination_save_sha256: digest(2),
                nonce: [10; 16],
                lease_fence: fence(10),
            },
            ArrivalEvidence {
                checkpoint: 10,
                server_stage_snapshot_id: stage_id(10),
                destination_world: world(2),
                portal_id: "to_cormoria".to_owned(),
                destination_save_sha256: digest(2),
                nonce: [10; 16],
                lease_fence: fence(99),
            },
            ArrivalEvidence {
                checkpoint: 10,
                server_stage_snapshot_id: stage_id(10),
                destination_world: world(2),
                portal_id: "to_cormoria".to_owned(),
                destination_save_sha256: digest(9),
                nonce: [10; 16],
                lease_fence: fence(10),
            },
        ] {
            assert!(matches!(
                journal.arrival_acknowledged(&evidence),
                Err(RomTravelError::Conflict)
            ));
        }
        acknowledge(&journal, 10, world(2), "to_cormoria");
        assert_eq!(
            journal
                .arrival_acknowledged(&ArrivalEvidence {
                    checkpoint: 10,
                    server_stage_snapshot_id: stage_id(10),
                    destination_world: world(2),
                    portal_id: "to_cormoria".to_owned(),
                    destination_save_sha256: digest(2),
                    nonce: [10; 16],
                    lease_fence: fence(10),
                })
                .unwrap(),
            journal.read().unwrap().unwrap()
        );
        assert!(matches!(
            journal.commit(10, stage_id(99), digest(2), fence(10)),
            Err(RomTravelError::Conflict)
        ));
        assert!(matches!(
            journal.commit(10, stage_id(10), digest(9), fence(10)),
            Err(RomTravelError::Conflict)
        ));
        assert!(matches!(
            journal.commit(10, stage_id(10), digest(2), fence(99)),
            Err(RomTravelError::Conflict)
        ));
        let committed = commit(&journal, 10);
        assert_eq!(committed, commit(&journal, 10));
        assert_eq!(committed.active_world, world(2));

        begin(&journal, world(2), world(3), "third_gate", 11);
        stage(&journal, 11);
        acknowledge(&journal, 11, world(3), "third_gate");
        assert_eq!(commit(&journal, 11).active_world, world(3));
        assert!(matches!(
            journal.prepare_intent(world(3), "back", 11, digest(3), preparation(11)),
            Err(RomTravelError::Checkpoint)
        ));
    }

    #[test]
    fn interrupted_transition_reopens_at_last_durable_phase() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        begin(&journal, world(1), world(3), "portal_7", 1);
        journal.source_saved(1, digest(7)).unwrap();
        let reopened = RomTravelJournal::new(
            root.path(),
            journal.character_id,
            [world(1), world(2), world(3)],
        )
        .unwrap();
        let state = reopened.read().unwrap().unwrap();
        assert_eq!(state.phase, TravelPhase::SourceSaved);
        assert_eq!(state.active_world, world(1));
        assert_eq!(state.source_snapshot_id, Some(snapshot(1)));
        assert_eq!(state.source_revision, Some(Revision::new(1)));
        assert_eq!(state.prepare_idempotency_key, Some(idempotency_key(1)));
        assert_eq!(state.lease_fence, Some(fence(1)));
        assert_eq!(state.source_save_sha256, Some(digest(7)));
        assert!(matches!(
            reopened.begin(world(1), world(2), "other", 2, preparation(2)),
            Err(RomTravelError::Conflict)
        ));
        reopened
            .destination_ready(1, stage_id(1), digest(8), [1; 16])
            .unwrap();
        reopened.launched(1).unwrap();
        reopened
            .arrival_acknowledged(&ArrivalEvidence {
                checkpoint: 1,
                server_stage_snapshot_id: stage_id(1),
                destination_world: world(3),
                portal_id: "portal_7".to_owned(),
                destination_save_sha256: digest(8),
                nonce: [1; 16],
                lease_fence: fence(1),
            })
            .unwrap();
        assert_eq!(
            reopened
                .commit(1, stage_id(1), digest(8), fence(1))
                .unwrap()
                .active_world,
            world(3)
        );
    }

    #[test]
    fn launch_failure_can_resume_source_and_retry_at_new_checkpoint() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        begin(&journal, world(1), world(2), "gate", 1);
        stage(&journal, 1);
        journal.launched(1).unwrap();
        let reopened = RomTravelJournal::new(
            root.path(),
            journal.character_id,
            [world(1), world(2), world(3)],
        )
        .unwrap();
        assert_eq!(
            reopened.read().unwrap().unwrap().phase,
            TravelPhase::Launched
        );
        assert_eq!(reopened.read().unwrap().unwrap().active_world, world(1));
        assert!(matches!(
            reopened.commit(1, stage_id(1), digest(2), fence(1)),
            Err(RomTravelError::Conflict)
        ));
        let pending = reopened.abort_to_source(1).unwrap();
        assert_eq!(pending.phase, TravelPhase::AbortPending);
        assert_eq!(pending, reopened.abort_to_source(1).unwrap());
        assert!(matches!(
            reopened.prepare_intent(world(1), "retry", 2, digest(2), preparation(2)),
            Err(RomTravelError::Conflict)
        ));
        let aborted = reopened.confirm_server_abort(1, stage_id(1)).unwrap();
        assert_eq!(aborted.active_world, world(1));
        assert_eq!(aborted.source_save_sha256, Some(digest(1)));
        begin(&reopened, world(1), world(3), "retry", 2);
    }

    #[test]
    fn rebegin_after_abort_reopens_retained_evidence() {
        for phase in [TravelPhase::DestinationReady, TravelPhase::Launched] {
            let (root, journal) = fixture();
            journal.initialize(world(1)).unwrap();
            begin(&journal, world(1), world(2), "first_gate", 1);
            journal.source_saved(1, digest(1)).unwrap();
            journal
                .destination_ready(1, stage_id(1), digest(2), [1; 16])
                .unwrap();
            if phase == TravelPhase::Launched {
                journal.launched(1).unwrap();
            }
            let pending = journal.abort_to_source(1).unwrap();
            assert_eq!(pending.phase, TravelPhase::AbortPending);
            let aborted = journal.confirm_server_abort(1, stage_id(1)).unwrap();
            assert_eq!(aborted.phase, TravelPhase::Aborted);

            let reopened = RomTravelJournal::new(
                root.path(),
                journal.character_id,
                [world(1), world(2), world(3)],
            )
            .unwrap();
            begin(&reopened, world(1), world(3), "retry_gate", 2);
            let state = reopened.read().unwrap().unwrap();
            assert_eq!(state.phase, TravelPhase::Prepared);
            assert_eq!(state.active_world, world(1));
            assert_eq!(state.checkpoint, 2);
        }
    }

    #[test]
    fn compaction_does_not_limit_lifetime_trips() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        let mut current = world(1);
        for checkpoint in 1..=32 {
            let destination = if current == world(1) {
                world(2)
            } else {
                world(1)
            };
            begin(&journal, current, destination, "repeat", checkpoint);
            stage(&journal, checkpoint);
            acknowledge(&journal, checkpoint, destination, "repeat");
            current = commit(&journal, checkpoint).active_world;
        }
        assert_eq!(journal.read().unwrap().unwrap().checkpoint, 32);
        assert!(fs::read_dir(root.path()).unwrap().count() <= 3); // lock plus two records
    }

    #[test]
    fn old_and_corrupt_records_fail_closed() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        let path = root.path().join("00000000000000000000.json");
        let old = fs::read_to_string(&path).unwrap().replace(
            &format!("\"format_version\":{FORMAT_VERSION}"),
            "\"format_version\":1",
        );
        fs::write(&path, old).unwrap();
        assert!(matches!(journal.read(), Err(RomTravelError::Corrupt)));

        let (corrupt_root, corrupt_journal) = fixture();
        corrupt_journal.initialize(world(1)).unwrap();
        fs::write(
            corrupt_root.path().join("00000000000000000000.json"),
            b"{not-json",
        )
        .unwrap();
        assert!(matches!(
            corrupt_journal.read(),
            Err(RomTravelError::Corrupt)
        ));
    }

    #[test]
    fn unknown_world_and_foreign_character_fail_closed() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        assert!(matches!(
            journal.begin(world(1), world(4), "gate", 1, preparation(1)),
            Err(RomTravelError::UnknownWorld)
        ));
        let narrowed =
            RomTravelJournal::new(root.path(), journal.character_id, [world(2), world(3)]).unwrap();
        assert!(matches!(narrowed.read(), Err(RomTravelError::UnknownWorld)));
        let foreign = RomTravelJournal::new(
            root.path(),
            CharacterId::new(Uuid::new_v4()).unwrap(),
            [world(1)],
        )
        .unwrap();
        assert!(matches!(
            foreign.read(),
            Err(RomTravelError::CharacterMismatch)
        ));
    }

    #[test]
    fn portal_ids_match_server_grammar() {
        let (_root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        for invalid in [
            "hoenn/cormoria",
            "To_cormoria",
            "to-cormoria",
            "to.cormoria",
        ] {
            assert!(matches!(
                journal.begin(world(1), world(2), invalid, 1, preparation(1)),
                Err(RomTravelError::Portal)
            ));
        }
        assert_eq!(
            begin(&journal, world(1), world(2), "to_cormoria", 1).portal_id,
            Some("to_cormoria".to_owned())
        );
    }

    #[test]
    fn intent_reopens_and_replays_only_the_same_prepare_request() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        assert!(matches!(
            journal.begin(world(1), world(3), "third_gate", 7, preparation(7)),
            Err(RomTravelError::Conflict)
        ));
        let intent = journal
            .prepare_intent(world(1), "third_gate", 7, digest(8), preparation(7))
            .unwrap();
        assert_eq!(intent.phase, TravelPhase::PrepareIntent);
        assert_eq!(intent.destination_world, None);
        assert_eq!(intent.source_head_save_sha256, Some(digest(8)));
        let reopened = RomTravelJournal::new(
            root.path(),
            journal.character_id,
            [world(1), world(2), world(3)],
        )
        .unwrap();
        assert_eq!(reopened.read().unwrap(), Some(intent.clone()));
        assert_eq!(
            reopened
                .prepare_intent(world(1), "third_gate", 7, digest(8), preparation(7))
                .unwrap(),
            intent
        );
        for (portal, head_digest, prep) in [
            ("other_gate", digest(8), preparation(7)),
            ("third_gate", digest(9), preparation(7)),
            ("third_gate", digest(8), preparation(8)),
        ] {
            assert!(matches!(
                reopened.prepare_intent(world(1), portal, 7, head_digest, prep),
                Err(RomTravelError::Conflict)
            ));
        }
        assert!(matches!(
            reopened.begin(world(1), world(1), "third_gate", 7, preparation(7)),
            Err(RomTravelError::Conflict)
        ));
        assert!(matches!(
            reopened.begin(world(1), world(2), "other_gate", 7, preparation(7)),
            Err(RomTravelError::Conflict)
        ));
        let prepared = reopened
            .begin(world(1), world(3), "third_gate", 7, preparation(7))
            .unwrap();
        assert_eq!(prepared.destination_world, Some(world(3)));
        assert_eq!(prepared.source_head_save_sha256, Some(digest(8)));
        assert_eq!(
            reopened
                .begin(world(1), world(3), "third_gate", 7, preparation(7))
                .unwrap(),
            prepared
        );
    }

    #[test]
    fn lost_prepare_response_keeps_retry_key_until_staging_is_recorded() {
        let (root, journal) = fixture();
        journal.initialize(world(2)).unwrap();
        for (checkpoint, head_digest, prep) in [
            (0, digest(1), preparation(1)),
            (1, digest(0), preparation(1)),
            (
                1,
                digest(1),
                TravelPreparation {
                    source_revision: Revision::initial(),
                    ..preparation(1)
                },
            ),
        ] {
            assert!(
                journal
                    .prepare_intent(world(2), "third_gate", checkpoint, head_digest, prep)
                    .is_err()
            );
        }
        let intent = journal
            .prepare_intent(world(2), "third_gate", 1, digest(1), preparation(1))
            .unwrap();
        assert!(matches!(
            journal.abort_to_source(1),
            Err(RomTravelError::Conflict)
        ));
        assert_eq!(journal.read().unwrap(), Some(intent.clone()));
        let reopened = RomTravelJournal::new(
            root.path(),
            journal.character_id,
            [world(1), world(2), world(3)],
        )
        .unwrap();
        assert_eq!(reopened.read().unwrap(), Some(intent.clone()));
        assert_eq!(
            reopened
                .prepare_intent(world(2), "third_gate", 1, digest(1), preparation(1))
                .unwrap(),
            intent
        );
        let prepared = reopened
            .begin(world(2), world(3), "third_gate", 1, preparation(1))
            .unwrap();
        assert!(matches!(
            reopened.abort_to_source(1),
            Err(RomTravelError::Conflict)
        ));
        assert_eq!(reopened.read().unwrap(), Some(prepared));
        let saved = reopened.source_saved(1, digest(4)).unwrap();
        assert!(matches!(
            reopened.abort_to_source(1),
            Err(RomTravelError::Conflict)
        ));
        assert_eq!(reopened.read().unwrap(), Some(saved));
        reopened
            .destination_ready(1, stage_id(1), digest(5), [1; 16])
            .unwrap();
        assert!(matches!(
            reopened.confirm_server_abort(1, stage_id(1)),
            Err(RomTravelError::Conflict)
        ));
        let pending = reopened.abort_to_source(1).unwrap();
        assert_eq!(pending.phase, TravelPhase::AbortPending);
        assert!(matches!(
            reopened.confirm_server_abort(1, stage_id(2)),
            Err(RomTravelError::Conflict)
        ));
        assert!(matches!(
            reopened.prepare_intent(world(2), "third_gate", 2, digest(2), preparation(2)),
            Err(RomTravelError::Conflict)
        ));
        let aborted = reopened.confirm_server_abort(1, stage_id(1)).unwrap();
        assert_eq!(
            aborted,
            reopened.confirm_server_abort(1, stage_id(1)).unwrap()
        );
        assert_eq!(aborted.phase, TravelPhase::Aborted);
        assert_eq!(aborted.active_world, world(2));
        assert_eq!(aborted.destination_world, Some(world(3)));
        assert_eq!(aborted.prepare_idempotency_key, Some(idempotency_key(1)));
        let next = reopened
            .prepare_intent(world(2), "third_gate", 2, digest(2), preparation(2))
            .unwrap();
        assert_eq!(next.active_world, world(2));
        assert_eq!(next.destination_world, None);
    }

    #[test]
    fn key_only_server_abort_settles_each_pre_stage_phase_and_reopens() {
        for phase in [
            TravelPhase::PrepareIntent,
            TravelPhase::Prepared,
            TravelPhase::SourceSaved,
        ] {
            let (root, journal) = fixture();
            journal.initialize(world(1)).unwrap();
            journal
                .prepare_intent(world(1), "to_next", 1, digest(7), preparation(1))
                .unwrap();
            if phase != TravelPhase::PrepareIntent {
                journal
                    .begin(world(1), world(2), "to_next", 1, preparation(1))
                    .unwrap();
            }
            if phase == TravelPhase::SourceSaved {
                journal.source_saved(1, digest(8)).unwrap();
            }
            let status = RomHandoffRecoveryStatus::Aborted {
                stage_id: stage_id(1),
                source_snapshot_id: snapshot(1),
                source_world_id: world(1),
                expected_revision: Revision::new(1),
                idempotency_key: idempotency_key(1),
            };
            let aborted = journal.reconcile_aborted_prepare(1, &status).unwrap();
            assert_eq!(aborted.phase, TravelPhase::Aborted);
            assert_eq!(aborted.server_stage_snapshot_id, Some(stage_id(1)));
            assert_eq!(
                aborted.source_save_sha256,
                (phase == TravelPhase::SourceSaved).then_some(digest(8))
            );
            assert_eq!(
                journal.reconcile_aborted_prepare(1, &status).unwrap(),
                aborted
            );
            let reopened = RomTravelJournal::new(
                root.path(),
                journal.character_id,
                [world(1), world(2), world(3)],
            )
            .unwrap();
            assert_eq!(reopened.read().unwrap(), Some(aborted));
            let next = reopened
                .prepare_intent(
                    world(1),
                    "to_next",
                    2,
                    digest(7),
                    TravelPreparation {
                        prepare_idempotency_key: idempotency_key(2),
                        ..preparation(1)
                    },
                )
                .unwrap();
            assert_eq!(next.phase, TravelPhase::PrepareIntent);
        }
    }

    #[test]
    fn authenticated_abort_settles_expired_acknowledged_arrival() {
        let (_root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        begin(&journal, world(1), world(2), "to_cormoria", 10);
        journal.source_saved(10, digest(1)).unwrap();
        journal
            .destination_ready(10, stage_id(10), digest(2), [10; 16])
            .unwrap();
        journal.launched(10).unwrap();
        acknowledge(&journal, 10, world(2), "to_cormoria");
        let status = RomHandoffRecoveryStatus::Aborted {
            stage_id: stage_id(10),
            source_snapshot_id: snapshot(10),
            source_world_id: world(1),
            expected_revision: Revision::new(10),
            idempotency_key: idempotency_key(10),
        };
        let aborted = journal.reconcile_aborted_prepare(10, &status).unwrap();
        assert_eq!(aborted.phase, TravelPhase::Aborted);
        assert_eq!(aborted.active_world, world(1));
        assert_eq!(aborted.source_save_sha256, Some(digest(1)));
        assert_eq!(journal.reconcile_aborted_prepare(10, &status).unwrap(), aborted);
        assert!(matches!(
            journal.commit(10, stage_id(10), digest(2), fence(10)),
            Err(RomTravelError::Conflict)
        ));
    }

    #[test]
    fn key_only_abort_requires_exact_source_and_aborted_status() {
        let (_root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        journal
            .prepare_intent(world(1), "to_next", 1, digest(7), preparation(1))
            .unwrap();
        let aborted =
            |stage_id, source_snapshot_id, source_world_id, expected_revision, idempotency_key| {
                RomHandoffRecoveryStatus::Aborted {
                    stage_id,
                    source_snapshot_id,
                    source_world_id,
                    expected_revision,
                    idempotency_key,
                }
            };
        let exact = aborted(
            stage_id(1),
            snapshot(1),
            world(1),
            Revision::new(1),
            idempotency_key(1),
        );
        for wrong in [
            RomHandoffRecoveryStatus::Staged {
                stage_id: stage_id(1),
                source_snapshot_id: snapshot(1),
                source_world_id: world(1),
                expected_revision: Revision::new(1),
                idempotency_key: idempotency_key(1),
            },
            aborted(
                stage_id(1),
                snapshot(2),
                world(1),
                Revision::new(1),
                idempotency_key(1),
            ),
            aborted(
                stage_id(1),
                snapshot(1),
                world(2),
                Revision::new(1),
                idempotency_key(1),
            ),
            aborted(
                stage_id(1),
                snapshot(1),
                world(1),
                Revision::new(2),
                idempotency_key(1),
            ),
            aborted(
                stage_id(1),
                snapshot(1),
                world(1),
                Revision::new(1),
                idempotency_key(2),
            ),
        ] {
            assert!(matches!(
                journal.reconcile_aborted_prepare(1, &wrong),
                Err(RomTravelError::Conflict)
            ));
        }
        assert_eq!(
            journal.read().unwrap().unwrap().phase,
            TravelPhase::PrepareIntent
        );
        assert_eq!(
            journal.reconcile_aborted_prepare(1, &exact).unwrap().phase,
            TravelPhase::Aborted
        );
        assert!(matches!(
            journal.reconcile_aborted_prepare(
                1,
                &aborted(
                    stage_id(2),
                    snapshot(1),
                    world(1),
                    Revision::new(1),
                    idempotency_key(1)
                ),
            ),
            Err(RomTravelError::Conflict)
        ));
    }

    #[test]
    fn fabricated_intent_destination_and_changed_retry_key_fail_on_read() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        journal
            .prepare_intent(world(1), "third_gate", 1, digest(1), preparation(1))
            .unwrap();
        let intent_path = root.path().join("00000000000000000001.json");
        let original = fs::read(&intent_path).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["destination_world"] = serde_json::json!(3);
        fs::write(&intent_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(journal.read(), Err(RomTravelError::Corrupt)));
        fs::write(&intent_path, original).unwrap();
        journal
            .begin(world(1), world(3), "third_gate", 1, preparation(1))
            .unwrap();
        let prepared_path = root.path().join("00000000000000000002.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&prepared_path).unwrap()).unwrap();
        value["prepare_idempotency_key"] = serde_json::json!(idempotency_key(2).as_str());
        fs::write(&prepared_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(journal.read(), Err(RomTravelError::Corrupt)));
    }

    #[test]
    fn forged_abort_without_server_confirmation_fails_on_read() {
        let (root, journal) = fixture();
        journal.initialize(world(1)).unwrap();
        begin(&journal, world(1), world(3), "third_gate", 1);
        stage(&journal, 1);
        let pending = journal.abort_to_source(1).unwrap();
        assert_eq!(pending.phase, TravelPhase::AbortPending);
        let path = root.path().join(format!("{:020}.json", pending.sequence));
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["phase"] = serde_json::json!("aborted");
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(journal.read(), Err(RomTravelError::Corrupt)));
    }
}

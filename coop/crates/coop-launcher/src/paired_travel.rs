//! Durable, per-member journal for a paired cross-ROM handoff.
//!
//! The caller verifies server replies, save bytes, and ROM arrival. This
//! journal orders those facts and keeps retry identities across restarts.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use coop_cloud::{
    CharacterId, GroupRomHandoffJoinRequest, IdempotencyKey, Revision, Sha256Digest, SnapshotId,
};
use coop_protocol::{RomWorldId, WorldZone};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const FORMAT_VERSION: u16 = 1;
const MAX_RECORD_BYTES: u64 = 4096;
const MAX_ENTRIES: usize = 16;

#[derive(Debug, Error)]
pub enum PairedTravelError {
    #[error("paired travel journal I/O failed")]
    Io(#[from] io::Error),
    #[error("paired travel journal is corrupt or inconsistent")]
    Corrupt,
    #[error("paired travel journal transition conflicts with persisted state")]
    Conflict,
}

/// The exact member request and trusted source identity persisted before join.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedJoinIntent {
    pub request: GroupRomHandoffJoinRequest,
    pub source_world_id: RomWorldId,
    pub source_save_sha256: Sha256Digest,
    pub catalog_digest: Sha256Digest,
}

/// Member-specific staged identity. The save bytes themselves are recovered
/// from the server and must be checked against this digest before use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedStage {
    pub stage_id: SnapshotId,
    pub destination_world_id: RomWorldId,
    pub arrival_portal_id: String,
    pub destination_save_sha256: Sha256Digest,
    pub arrival_challenge: Sha256Digest,
    pub expected_nonce: [u8; 16],
}

/// Evidence the caller has authenticated against the exact staged identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedArrivalEvidence {
    pub stage_id: SnapshotId,
    pub destination_save_sha256: Sha256Digest,
    pub nonce: [u8; 16],
}

/// Compact terminal receipt; the caller must validate the complete server
/// response and the resulting authoritative snapshot before recording it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedCommit {
    pub destination_zone: WorldZone,
    pub group_zone_revision: u64,
    pub own_snapshot_id: SnapshotId,
    pub own_world_id: RomWorldId,
    pub own_revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairedPhase {
    JoinIntent,
    AttemptKnown,
    Staged,
    ArrivalVerified,
    Committed,
    Adopted,
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairedTerminal {
    Committed(PairedCommit),
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedTravelRecord {
    pub format_version: u16,
    pub sequence: u64,
    pub character_id: CharacterId,
    pub phase: PairedPhase,
    pub intent: PairedJoinIntent,
    pub attempt_key: Option<IdempotencyKey>,
    pub stage: Option<PairedStage>,
    pub verified_arrival: Option<PairedArrivalEvidence>,
    pub terminal: Option<PairedTerminal>,
}

/// Append-only snapshots with a bounded rolling pair for transition checks.
/// The directory must already be provisioned by the caller.
#[derive(Clone, Debug)]
pub struct PairedTravelJournal {
    directory: PathBuf,
    character_id: CharacterId,
}

impl PairedTravelJournal {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>, character_id: CharacterId) -> Self {
        Self {
            directory: directory.into(),
            character_id,
        }
    }

    /// Returns the durable state, rejecting malformed entries and transitions.
    pub fn read(&self) -> Result<Option<PairedTravelRecord>, PairedTravelError> {
        let _lock = self.lock()?;
        self.read_locked()
    }

    /// Must complete before the member sends its first join request.
    pub fn begin(
        &self,
        intent: &PairedJoinIntent,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        if !valid_intent(intent, self.character_id) {
            return Err(PairedTravelError::Conflict);
        }
        let _lock = self.lock()?;
        let previous = self.read_locked()?;
        if let Some(record) = &previous {
            if record.phase == PairedPhase::JoinIntent && record.intent == *intent {
                return Ok(record.clone());
            }
            if !matches!(record.phase, PairedPhase::Adopted | PairedPhase::Aborted)
                || record.intent.request.client_intent_key == intent.request.client_intent_key
            {
                return Err(PairedTravelError::Conflict);
            }
        }
        let record = PairedTravelRecord {
            format_version: FORMAT_VERSION,
            sequence: next_sequence(previous.as_ref())?,
            character_id: self.character_id,
            phase: PairedPhase::JoinIntent,
            intent: intent.clone(),
            attempt_key: None,
            stage: None,
            verified_arrival: None,
            terminal: None,
        };
        self.append(&record)?;
        Ok(record)
    }

    /// Persists the server attempt identity before staging or arrival work.
    pub fn record_attempt(
        &self,
        client_key: IdempotencyKey,
        attempt_key: IdempotencyKey,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        self.advance(client_key, |current| {
            if current.phase != PairedPhase::JoinIntent {
                return if current.attempt_key == Some(attempt_key) {
                    Ok(None)
                } else {
                    Err(PairedTravelError::Conflict)
                };
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::AttemptKnown,
                attempt_key: Some(attempt_key),
                ..current.clone()
            }))
        })
    }

    /// Persists member-specific stage, digest, challenge, and nonce before
    /// launching the destination ROM.
    pub fn record_staged(
        &self,
        client_key: IdempotencyKey,
        stage: PairedStage,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        if !valid_stage(&stage) {
            return Err(PairedTravelError::Conflict);
        }
        self.advance(client_key, |current| {
            if current.phase != PairedPhase::AttemptKnown {
                return if current.stage.as_ref() == Some(&stage) {
                    Ok(None)
                } else {
                    Err(PairedTravelError::Conflict)
                };
            }
            if stage.destination_world_id == current.intent.source_world_id {
                return Err(PairedTravelError::Conflict);
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::Staged,
                stage: Some(stage),
                ..current.clone()
            }))
        })
    }

    /// Called only after authenticating ROM arrival against the staged nonce.
    pub fn record_verified_arrival(
        &self,
        client_key: IdempotencyKey,
        evidence: PairedArrivalEvidence,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        self.advance(client_key, |current| {
            if current.phase != PairedPhase::Staged {
                return if current.verified_arrival == Some(evidence) {
                    Ok(None)
                } else {
                    Err(PairedTravelError::Conflict)
                };
            }
            let stage = current.stage.as_ref().ok_or(PairedTravelError::Corrupt)?;
            if !matches_stage(stage, evidence) {
                return Err(PairedTravelError::Conflict);
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::ArrivalVerified,
                verified_arrival: Some(evidence),
                ..current.clone()
            }))
        })
    }

    /// Records a verified committed server receipt. This does not itself
    /// change the active ROM or save authority.
    pub fn record_committed(
        &self,
        client_key: IdempotencyKey,
        commit: PairedCommit,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        self.advance(client_key, |current| {
            if current.phase != PairedPhase::ArrivalVerified {
                return if current.terminal == Some(PairedTerminal::Committed(commit.clone())) {
                    Ok(None)
                } else {
                    Err(PairedTravelError::Conflict)
                };
            }
            if !valid_commit(current, &commit) {
                return Err(PairedTravelError::Conflict);
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::Committed,
                terminal: Some(PairedTerminal::Committed(commit)),
                ..current.clone()
            }))
        })
    }

    /// Acknowledges that the solo ROM journal has durably adopted this exact
    /// committed proof. Until this transition, no later intent may replace it.
    pub fn record_adopted(
        &self,
        client_key: IdempotencyKey,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        self.advance(client_key, |current| {
            if current.phase == PairedPhase::Adopted {
                return Ok(None);
            }
            if current.phase != PairedPhase::Committed {
                return Err(PairedTravelError::Conflict);
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::Adopted,
                ..current.clone()
            }))
        })
    }

    /// Records an authenticated server abort. A committed result cannot be
    /// changed into an abort.
    pub fn record_aborted(
        &self,
        client_key: IdempotencyKey,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        self.advance(client_key, |current| {
            if current.phase == PairedPhase::Aborted {
                return Ok(None);
            }
            if matches!(current.phase, PairedPhase::Committed | PairedPhase::Adopted) {
                return Err(PairedTravelError::Conflict);
            }
            Ok(Some(PairedTravelRecord {
                phase: PairedPhase::Aborted,
                terminal: Some(PairedTerminal::Aborted),
                ..current.clone()
            }))
        })
    }

    fn advance(
        &self,
        client_key: IdempotencyKey,
        transition: impl FnOnce(
            &PairedTravelRecord,
        ) -> Result<Option<PairedTravelRecord>, PairedTravelError>,
    ) -> Result<PairedTravelRecord, PairedTravelError> {
        let _lock = self.lock()?;
        let current = self.read_locked()?.ok_or(PairedTravelError::Conflict)?;
        if current.intent.request.client_intent_key != client_key {
            return Err(PairedTravelError::Conflict);
        }
        let Some(mut next) = transition(&current)? else {
            return Ok(current);
        };
        next.sequence = current
            .sequence
            .checked_add(1)
            .ok_or(PairedTravelError::Conflict)?;
        validate_record(&next, self.character_id)?;
        if !valid_transition(&current, &next) {
            return Err(PairedTravelError::Conflict);
        }
        self.append(&next)?;
        Ok(next)
    }

    fn lock(&self) -> Result<File, PairedTravelError> {
        let directory_metadata = fs::symlink_metadata(&self.directory)?;
        if !directory_metadata.is_dir() || directory_metadata.file_type().is_symlink() {
            return Err(PairedTravelError::Corrupt);
        }
        let lock_path = self.directory.join(".lock");
        match fs::symlink_metadata(&lock_path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(PairedTravelError::Corrupt);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(lock_path)?;
        crate::file_lock::lock_file(&file)?;
        Ok(file)
    }

    fn read_locked(&self) -> Result<Option<PairedTravelRecord>, PairedTravelError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(PairedTravelError::Corrupt)?;
            if name == ".lock" {
                continue;
            }
            if name.starts_with(".tmp-") {
                fs::remove_file(entry.path())?;
                continue;
            }
            if name.len() != 25 || !name.ends_with(".json") || !entry.file_type()?.is_file() {
                return Err(PairedTravelError::Corrupt);
            }
            let sequence = name[..20]
                .parse::<u64>()
                .map_err(|_| PairedTravelError::Corrupt)?;
            entries.push((sequence, entry.path()));
            if entries.len() > MAX_ENTRIES {
                return Err(PairedTravelError::Corrupt);
            }
        }
        entries.sort_unstable_by_key(|(sequence, _)| *sequence);
        let mut previous: Option<PairedTravelRecord> = None;
        for (sequence, path) in entries {
            if previous
                .as_ref()
                .is_some_and(|record| record.sequence.checked_add(1) != Some(sequence))
            {
                return Err(PairedTravelError::Corrupt);
            }
            let mut bytes = Vec::new();
            File::open(path)?
                .take(MAX_RECORD_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_RECORD_BYTES {
                return Err(PairedTravelError::Corrupt);
            }
            let record: PairedTravelRecord =
                serde_json::from_slice(&bytes).map_err(|_| PairedTravelError::Corrupt)?;
            if record.sequence != sequence {
                return Err(PairedTravelError::Corrupt);
            }
            validate_record(&record, self.character_id)?;
            if previous
                .as_ref()
                .is_some_and(|prior| !valid_transition(prior, &record))
            {
                return Err(PairedTravelError::Corrupt);
            }
            previous = Some(record);
        }
        Ok(previous)
    }

    fn append(&self, record: &PairedTravelRecord) -> Result<(), PairedTravelError> {
        let bytes = serde_json::to_vec(record).map_err(|_| PairedTravelError::Corrupt)?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(PairedTravelError::Corrupt);
        }
        let target = self.directory.join(format!("{:020}.json", record.sequence));
        let temporary = self
            .directory
            .join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
        let result = (|| -> io::Result<()> {
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
                    "journal entry exists",
                ));
            }
            fs::rename(&temporary, &target)?;
            sync_directory(&self.directory)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        let mut entries = fs::read_dir(&self.directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        entries.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
        entries.sort_unstable();
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

fn next_sequence(previous: Option<&PairedTravelRecord>) -> Result<u64, PairedTravelError> {
    previous.map_or(Ok(0), |record| {
        record
            .sequence
            .checked_add(1)
            .ok_or(PairedTravelError::Conflict)
    })
}

fn valid_portal(value: &str) -> bool {
    value.len() <= 96
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_intent(intent: &PairedJoinIntent, character_id: CharacterId) -> bool {
    intent.request.fence.character_id == character_id
        && intent.request.valid_portal_id()
        && intent.request.fence.current_revision.value() > 0
        && intent.source_save_sha256.as_bytes() != &[0; 32]
        && intent.catalog_digest.as_bytes() != &[0; 32]
}

fn valid_stage(stage: &PairedStage) -> bool {
    valid_portal(&stage.arrival_portal_id)
        && stage.destination_save_sha256.as_bytes() != &[0; 32]
        && stage.arrival_challenge.as_bytes() != &[0; 32]
        && stage.expected_nonce != [0; 16]
}

fn matches_stage(stage: &PairedStage, evidence: PairedArrivalEvidence) -> bool {
    stage.stage_id == evidence.stage_id
        && stage.destination_save_sha256 == evidence.destination_save_sha256
        && stage.expected_nonce == evidence.nonce
}

fn valid_commit(record: &PairedTravelRecord, commit: &PairedCommit) -> bool {
    let Some(stage) = record.stage.as_ref() else {
        return false;
    };
    commit.destination_zone.validate().is_ok()
        && commit.own_snapshot_id == stage.stage_id
        && commit.own_world_id == stage.destination_world_id
        && record.intent.request.fence.current_revision.next().ok() == Some(commit.own_revision)
}

fn validate_record(
    record: &PairedTravelRecord,
    character_id: CharacterId,
) -> Result<(), PairedTravelError> {
    if record.format_version != FORMAT_VERSION
        || record.character_id != character_id
        || !valid_intent(&record.intent, character_id)
        || record.stage.as_ref().is_some_and(|stage| {
            !valid_stage(stage) || stage.destination_world_id == record.intent.source_world_id
        })
        || record.verified_arrival.is_some_and(|evidence| {
            record
                .stage
                .as_ref()
                .is_none_or(|stage| !matches_stage(stage, evidence))
        })
    {
        return Err(PairedTravelError::Corrupt);
    }
    let shape = match record.phase {
        PairedPhase::JoinIntent => {
            record.attempt_key.is_none()
                && record.stage.is_none()
                && record.verified_arrival.is_none()
                && record.terminal.is_none()
        }
        PairedPhase::AttemptKnown => {
            record.attempt_key.is_some()
                && record.stage.is_none()
                && record.verified_arrival.is_none()
                && record.terminal.is_none()
        }
        PairedPhase::Staged => {
            record.attempt_key.is_some()
                && record.stage.is_some()
                && record.verified_arrival.is_none()
                && record.terminal.is_none()
        }
        PairedPhase::ArrivalVerified => {
            record.attempt_key.is_some()
                && record.stage.is_some()
                && record.verified_arrival.is_some()
                && record.terminal.is_none()
        }
        PairedPhase::Committed | PairedPhase::Adopted => {
            record.attempt_key.is_some()
                && record.stage.is_some()
                && record.verified_arrival.is_some()
                && matches!(record.terminal.as_ref(), Some(PairedTerminal::Committed(commit)) if valid_commit(record, commit))
        }
        PairedPhase::Aborted => matches!(record.terminal.as_ref(), Some(PairedTerminal::Aborted)),
    };
    if shape {
        Ok(())
    } else {
        Err(PairedTravelError::Corrupt)
    }
}

fn valid_transition(previous: &PairedTravelRecord, next: &PairedTravelRecord) -> bool {
    if previous.sequence.checked_add(1) != Some(next.sequence)
        || previous.character_id != next.character_id
        || previous.format_version != next.format_version
    {
        return false;
    }
    if next.phase == PairedPhase::JoinIntent {
        return matches!(previous.phase, PairedPhase::Adopted | PairedPhase::Aborted)
            && next.intent.request.client_intent_key != previous.intent.request.client_intent_key;
    }
    if previous.intent != next.intent {
        return false;
    }
    match (&previous.phase, &next.phase) {
        (PairedPhase::JoinIntent, PairedPhase::AttemptKnown) => {
            next.stage.is_none() && next.terminal.is_none()
        }
        (PairedPhase::AttemptKnown, PairedPhase::Staged) => {
            previous.attempt_key == next.attempt_key
        }
        (PairedPhase::Staged, PairedPhase::ArrivalVerified) => {
            previous.attempt_key == next.attempt_key && previous.stage == next.stage
        }
        (PairedPhase::ArrivalVerified, PairedPhase::Committed) => {
            previous.attempt_key == next.attempt_key
                && previous.stage == next.stage
                && previous.verified_arrival == next.verified_arrival
        }
        (PairedPhase::Committed, PairedPhase::Adopted) => {
            previous.attempt_key == next.attempt_key
                && previous.stage == next.stage
                && previous.verified_arrival == next.verified_arrival
                && previous.terminal == next.terminal
        }
        (
            PairedPhase::JoinIntent
            | PairedPhase::AttemptKnown
            | PairedPhase::Staged
            | PairedPhase::ArrivalVerified,
            PairedPhase::Aborted,
        ) => {
            previous.attempt_key == next.attempt_key
                && previous.stage == next.stage
                && previous.verified_arrival == next.verified_arrival
        }
        _ => false,
    }
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
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

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{ApiVersion, ClientInstanceId, GroupId, LeaseFence, SessionEpoch, SessionId};
    use coop_protocol::RegionId;
    use uuid::Uuid;

    fn key(value: u128) -> IdempotencyKey {
        IdempotencyKey::new(Uuid::from_u128(value)).unwrap()
    }

    fn snapshot(value: u128) -> SnapshotId {
        SnapshotId::new(Uuid::from_u128(value)).unwrap()
    }

    fn intent(character_id: CharacterId) -> PairedJoinIntent {
        PairedJoinIntent {
            request: GroupRomHandoffJoinRequest {
                api_version: ApiVersion::V1,
                group_id: GroupId::new(Uuid::from_u128(10)).unwrap(),
                fence: LeaseFence::new(
                    SessionId::new(Uuid::from_u128(11)).unwrap(),
                    character_id,
                    Revision::new(1),
                    SessionEpoch::new(1).unwrap(),
                    ClientInstanceId::new(Uuid::from_u128(12)).unwrap(),
                ),
                source_snapshot_id: snapshot(13),
                portal_id: "to_cormoria".into(),
                client_intent_key: key(14),
            },
            source_world_id: RomWorldId::new(1).unwrap(),
            source_save_sha256: Sha256Digest::of_bytes(b"finalized-source-save"),
            catalog_digest: Sha256Digest::of_bytes(b"trusted-catalog"),
        }
    }

    fn stage() -> PairedStage {
        PairedStage {
            stage_id: snapshot(20),
            destination_world_id: RomWorldId::new(2).unwrap(),
            arrival_portal_id: "rivetshore_harbor".into(),
            destination_save_sha256: Sha256Digest::of_bytes(b"staged-save"),
            arrival_challenge: Sha256Digest::of_bytes(b"arrival-challenge"),
            expected_nonce: [9; 16],
        }
    }

    fn commit() -> PairedCommit {
        PairedCommit {
            destination_zone: WorldZone::new(RegionId::Hoenn, "LILYCOVE_CITY_HARBOR", 1).unwrap(),
            group_zone_revision: 2,
            own_snapshot_id: snapshot(20),
            own_world_id: RomWorldId::new(2).unwrap(),
            own_revision: Revision::new(2),
        }
    }

    fn fixture() -> (tempfile::TempDir, CharacterId, PairedTravelJournal) {
        let directory = tempfile::tempdir().unwrap();
        let character_id = CharacterId::new(Uuid::from_u128(1)).unwrap();
        let journal = PairedTravelJournal::new(directory.path(), character_id);
        (directory, character_id, journal)
    }

    #[test]
    fn durable_member_intent_replays_exactly_before_server_attempt() {
        let (directory, character_id, journal) = fixture();
        let intent = intent(character_id);
        let first = journal.begin(&intent).unwrap();
        assert_eq!(first.phase, PairedPhase::JoinIntent);
        assert_eq!(first.attempt_key, None);
        let reopened = PairedTravelJournal::new(directory.path(), character_id);
        assert_eq!(reopened.read().unwrap(), Some(first.clone()));
        assert_eq!(reopened.begin(&intent).unwrap(), first);
        let mut changed = intent.clone();
        changed.request.portal_id = "to_main".into();
        assert!(matches!(
            reopened.begin(&changed),
            Err(PairedTravelError::Conflict)
        ));
        let attempt = reopened.record_attempt(key(14), key(15)).unwrap();
        assert_eq!(attempt.attempt_key, Some(key(15)));
        assert!(matches!(
            reopened.record_attempt(key(14), key(16)),
            Err(PairedTravelError::Conflict)
        ));
        assert!(matches!(
            reopened.record_attempt(key(17), key(15)),
            Err(PairedTravelError::Conflict)
        ));
    }

    #[test]
    fn staged_identity_and_verified_arrival_survive_restart() {
        let (directory, character_id, journal) = fixture();
        journal.begin(&intent(character_id)).unwrap();
        journal.record_attempt(key(14), key(15)).unwrap();
        let staged = journal.record_staged(key(14), stage()).unwrap();
        assert_eq!(staged.stage, Some(stage()));
        let reopened = PairedTravelJournal::new(directory.path(), character_id);
        assert_eq!(reopened.read().unwrap(), Some(staged));
        let wrong = PairedArrivalEvidence {
            stage_id: snapshot(20),
            destination_save_sha256: stage().destination_save_sha256,
            nonce: [8; 16],
        };
        assert!(matches!(
            reopened.record_verified_arrival(key(14), wrong),
            Err(PairedTravelError::Conflict)
        ));
        let evidence = PairedArrivalEvidence {
            nonce: [9; 16],
            ..wrong
        };
        let verified = reopened.record_verified_arrival(key(14), evidence).unwrap();
        assert_eq!(verified.phase, PairedPhase::ArrivalVerified);
        assert_eq!(
            reopened.record_verified_arrival(key(14), evidence).unwrap(),
            verified
        );
        let committed = reopened.record_committed(key(14), commit()).unwrap();
        assert_eq!(committed.phase, PairedPhase::Committed);
        assert!(matches!(
            reopened.record_aborted(key(14)),
            Err(PairedTravelError::Conflict)
        ));
    }

    #[test]
    fn committed_proof_blocks_new_intent_until_durable_adoption_acknowledgment() {
        let (directory, character_id, journal) = fixture();
        journal.begin(&intent(character_id)).unwrap();
        journal.record_attempt(key(14), key(15)).unwrap();
        journal.record_staged(key(14), stage()).unwrap();
        journal
            .record_verified_arrival(
                key(14),
                PairedArrivalEvidence {
                    stage_id: snapshot(20),
                    destination_save_sha256: stage().destination_save_sha256,
                    nonce: [9; 16],
                },
            )
            .unwrap();
        let committed = journal.record_committed(key(14), commit()).unwrap();
        let reopened = PairedTravelJournal::new(directory.path(), character_id);
        assert_eq!(reopened.read().unwrap(), Some(committed));
        let mut next = intent(character_id);
        next.request.client_intent_key = key(30);
        assert!(matches!(
            reopened.begin(&next),
            Err(PairedTravelError::Conflict)
        ));
        let adopted = reopened.record_adopted(key(14)).unwrap();
        assert_eq!(adopted.phase, PairedPhase::Adopted);
        assert_eq!(adopted.terminal, Some(PairedTerminal::Committed(commit())));
        assert_eq!(reopened.record_adopted(key(14)).unwrap(), adopted);
        assert_eq!(
            reopened.begin(&next).unwrap().phase,
            PairedPhase::JoinIntent
        );
    }

    #[test]
    fn terminal_commit_requires_exact_stage_and_next_source_revision() {
        let (_directory, character_id, journal) = fixture();
        journal.begin(&intent(character_id)).unwrap();
        journal.record_attempt(key(14), key(15)).unwrap();
        journal.record_staged(key(14), stage()).unwrap();
        journal
            .record_verified_arrival(
                key(14),
                PairedArrivalEvidence {
                    stage_id: snapshot(20),
                    destination_save_sha256: stage().destination_save_sha256,
                    nonce: [9; 16],
                },
            )
            .unwrap();
        for bad in [
            PairedCommit {
                own_snapshot_id: snapshot(21),
                ..commit()
            },
            PairedCommit {
                own_world_id: RomWorldId::new(1).unwrap(),
                ..commit()
            },
            PairedCommit {
                own_revision: Revision::new(3),
                ..commit()
            },
        ] {
            assert!(matches!(
                journal.record_committed(key(14), bad),
                Err(PairedTravelError::Conflict)
            ));
            assert_eq!(
                journal.read().unwrap().unwrap().phase,
                PairedPhase::ArrivalVerified
            );
        }
        assert_eq!(
            journal.record_committed(key(14), commit()).unwrap().phase,
            PairedPhase::Committed
        );
    }

    #[test]
    fn corrupt_record_and_forged_transition_fail_closed() {
        let (directory, character_id, journal) = fixture();
        journal.begin(&intent(character_id)).unwrap();
        journal.record_attempt(key(14), key(15)).unwrap();
        let path = directory.path().join("00000000000000000001.json");
        let original = fs::read(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["phase"] = serde_json::json!("committed");
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(journal.read(), Err(PairedTravelError::Corrupt)));
        fs::write(&path, original).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["intent"]["request"]["client_intent_key"] = serde_json::json!(key(16).as_str());
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(journal.read(), Err(PairedTravelError::Corrupt)));
    }

    #[test]
    fn authenticated_abort_is_terminal_and_allows_new_member_intent() {
        let (_directory, character_id, journal) = fixture();
        journal.begin(&intent(character_id)).unwrap();
        let aborted = journal.record_aborted(key(14)).unwrap();
        assert_eq!(aborted.phase, PairedPhase::Aborted);
        assert_eq!(journal.record_aborted(key(14)).unwrap(), aborted);
        let mut next = intent(character_id);
        next.request.client_intent_key = key(30);
        assert_eq!(journal.begin(&next).unwrap().phase, PairedPhase::JoinIntent);
    }
}

//! The durable boundary between a finalized source portal checkpoint and a
//! server-staged destination. Launch, arrival proof, and commit happen later.

use std::{future::Future, pin::Pin};

use coop_cloud::{
    ApiVersion, IdempotencyKey, RomHandoffPrepareRequest, RomHandoffPrepareResponse,
    RomHandoffRecoveryRequest, RomHandoffRecoveryStatus, Sha256Digest, SnapshotId,
};
use coop_protocol::{IDENTITY_REGISTRY_DIGEST, IDENTITY_REGISTRY_VERSION, RomWorldId};
use coop_save::{RegistryContract, parse_v2};
use thiserror::Error;

use crate::{
    AuthSession, CloudApi, SessionLifecycle, TrustedRomCatalog,
    rom_travel::{
        LeaseFenceIdentity, RomTravelError, RomTravelJournal, TravelPhase, TravelPreparation,
        TravelRecord,
    },
    session::{PortalTravelSource, SessionError},
};

/// The destination is staged on the server. The source lease and active-world
/// pointer remain authoritative until a separately verified arrival commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedDestination {
    pub checkpoint: u64,
    pub source_world: RomWorldId,
    pub destination_world: RomWorldId,
    pub source_snapshot_id: SnapshotId,
    pub stage_id: SnapshotId,
    pub destination_save_sha256: Sha256Digest,
    pub destination_save: Vec<u8>,
    pub arrival_portal_id: String,
    /// Map group, map number, and warp in the ROM-selected save slot.
    pub arrival_location: [u8; 3],
    pub destination_save_generation: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageOutcome {
    Staged(StagedDestination),
    /// The server proved the exact prepare key was aborted; the source remains
    /// active and the journal has durably recorded that result.
    Aborted,
}

#[derive(Debug, Error)]
pub enum TravelCoordinatorError {
    #[error("portal source does not match the current lease or validated save")]
    SourceMismatch,
    #[error("server handoff result does not match the durable prepare intent")]
    ResponseMismatch,
    #[error("server handoff state remains uncertain; retry reconciliation with the same key")]
    Uncertain,
    #[error("travel journal failed")]
    Journal(#[from] RomTravelError),
}

/// Stage a portal destination using one durable retry key. Retrying after a
/// transport failure must reuse that same key and finalized source checkpoint.
/// No destination save is published or ROM process launched here.
pub async fn stage_portal_travel<A: CloudApi>(
    api: &A,
    session: &SessionLifecycle,
    source: &PortalTravelSource,
    catalog: &TrustedRomCatalog,
    journal: &RomTravelJournal,
    prepare_key: IdempotencyKey,
) -> Result<StageOutcome, TravelCoordinatorError> {
    let source_world = session.rom_world_id();
    if let Some(record) = journal.read()? {
        if requires_settlement(&record, current_fence(session), catalog.digest()) {
            // A replaced lease cannot replay the old prepare request. The
            // server must first settle the durable key under the new lease.
            return recover_pending_handoff(api, session, journal).await;
        }
    }
    catalog
        .world(source_world)
        .map_err(|_| TravelCoordinatorError::SourceMismatch)?;
    if source.source_revision != session.revision
        || source.source_revision != session.lease.current_revision
        || source.source_snapshot_id
            != session
                .last_finalized_snapshot_id()
                .ok_or(TravelCoordinatorError::SourceMismatch)?
        || source.source_revision.is_initial()
        || source.source_save_digest
            != session
                .active_save_digest()
                .map_err(|_| TravelCoordinatorError::SourceMismatch)?
                .ok_or(TravelCoordinatorError::SourceMismatch)?
        || source.portal_sequence == 0
        || source.checkpoint_ready_sequence == 0
    {
        return Err(TravelCoordinatorError::SourceMismatch);
    }

    let preparation = TravelPreparation::new(
        source.source_snapshot_id,
        source.source_revision,
        prepare_key,
        catalog.digest(),
        current_fence(session),
    );
    let checkpoint = source.source_revision.value();
    persist_or_resume_intent(journal, source_world, source, preparation)?;
    let request = RomHandoffPrepareRequest {
        api_version: ApiVersion::V1,
        character_id: session.lease.character_id,
        session_id: session.lease.session_id,
        session_epoch: session.lease.session_epoch,
        client_instance_id: session.lease.client_instance_id,
        expected_revision: source.source_revision,
        source_snapshot_id: source.source_snapshot_id,
        portal_id: source.portal_id.clone(),
        idempotency_key: prepare_key,
    };
    // A lost prepare response may still mean the server staged the save.
    // Reconciliation is mandatory even on a successful response because the
    // response does not repeat the source-head identity.
    let first = api
        .prepare_rom_handoff(&session.auth, request.clone())
        .await;
    let status = api
        .reconcile_rom_handoff(
            &session.auth,
            RomHandoffRecoveryRequest {
                api_version: ApiVersion::V1,
                character_id: request.character_id,
                idempotency_key: prepare_key,
                source_snapshot_id: source.source_snapshot_id,
                expected_revision: source.source_revision,
                source_world_id: source_world,
                portal_id: source.portal_id.clone(),
            },
        )
        .await
        .map_err(|_| TravelCoordinatorError::Uncertain)?;
    match checked_status(&status, source_world, source, prepare_key)? {
        CheckedStatus::Aborted => {
            settle_aborted(journal, checkpoint, &status)?;
            Ok(StageOutcome::Aborted)
        }
        CheckedStatus::Staged(stage_id) => {
            let response = match first {
                Ok(response) => response,
                Err(SessionError::Unauthorized | SessionError::Auth(_)) => {
                    return Err(TravelCoordinatorError::Uncertain);
                }
                Err(_) => api
                    .prepare_rom_handoff(&session.auth, request)
                    .await
                    .map_err(|_| TravelCoordinatorError::Uncertain)?,
            };
            let destination = checked_response(
                response,
                stage_id,
                source_world,
                source,
                catalog,
                checkpoint,
            )?;
            record_staged(journal, source_world, source, preparation, &destination)?;
            Ok(StageOutcome::Staged(destination))
        }
    }
}

fn current_fence(session: &SessionLifecycle) -> LeaseFenceIdentity {
    LeaseFenceIdentity::new(
        session.lease.session_id,
        session.lease.session_epoch,
        session.lease.client_instance_id,
    )
}

fn is_unfinished_prepare(phase: TravelPhase) -> bool {
    matches!(
        phase,
        TravelPhase::PrepareIntent | TravelPhase::Prepared | TravelPhase::SourceSaved
    )
}

fn requires_settlement(
    record: &TravelRecord,
    lease_fence: LeaseFenceIdentity,
    catalog_digest: Sha256Digest,
) -> bool {
    is_unfinished_prepare(record.phase)
        && (record.lease_fence != Some(lease_fence)
            || record.trusted_catalog_digest != Some(catalog_digest))
}

/// Settle an unfinished prepare after a lease replacement, catalog update, or
/// cold restart. The original key is reconciled under the *current* source
/// lease. A live stage is explicitly aborted and then reconciled again before
/// local `Aborted` is written. This never sends the old prepare request.
pub async fn recover_pending_handoff<A: CloudApi>(
    api: &A,
    session: &SessionLifecycle,
    journal: &RomTravelJournal,
) -> Result<StageOutcome, TravelCoordinatorError> {
    let record = journal
        .read()?
        .ok_or(TravelCoordinatorError::ResponseMismatch)?;
    if !matches!(record.phase, TravelPhase::Aborted) && !is_unfinished_prepare(record.phase)
        || record.character_id != session.lease.character_id
        || record.active_world != session.rom_world_id()
        || record.source_world != Some(session.rom_world_id())
        || record.source_revision != Some(session.revision)
        || session.lease.current_revision != session.revision
        || record.source_head_save_sha256
            != session
                .active_save_digest()
                .map_err(|_| TravelCoordinatorError::SourceMismatch)?
        || record.source_snapshot_id.is_none()
        || record.prepare_idempotency_key.is_none()
    {
        return Err(TravelCoordinatorError::SourceMismatch);
    }
    if record.phase == TravelPhase::Aborted {
        return Ok(StageOutcome::Aborted);
    }
    recover_record(
        &CloudRecovery {
            api,
            auth: &session.auth,
            character_id: record.character_id,
            source_snapshot_id: record
                .source_snapshot_id
                .ok_or(TravelCoordinatorError::SourceMismatch)?,
            expected_revision: record
                .source_revision
                .ok_or(TravelCoordinatorError::SourceMismatch)?,
            source_world_id: record
                .source_world
                .ok_or(TravelCoordinatorError::SourceMismatch)?,
            portal_id: record
                .portal_id
                .clone()
                .ok_or(TravelCoordinatorError::SourceMismatch)?,
        },
        journal,
        &record,
    )
    .await
}

type RecoveryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, SessionError>> + Send + 'a>>;

trait RecoveryTransport {
    fn reconcile(&self, key: IdempotencyKey) -> RecoveryFuture<'_, RomHandoffRecoveryStatus>;
    fn abort(&self, stage_id: SnapshotId) -> RecoveryFuture<'_, ()>;
}

struct CloudRecovery<'a, A> {
    api: &'a A,
    auth: &'a AuthSession,
    character_id: coop_cloud::CharacterId,
    source_snapshot_id: SnapshotId,
    expected_revision: coop_cloud::Revision,
    source_world_id: RomWorldId,
    portal_id: String,
}

impl<A: CloudApi> RecoveryTransport for CloudRecovery<'_, A> {
    fn reconcile(&self, key: IdempotencyKey) -> RecoveryFuture<'_, RomHandoffRecoveryStatus> {
        self.api.reconcile_rom_handoff(
            self.auth,
            RomHandoffRecoveryRequest {
                api_version: ApiVersion::V1,
                character_id: self.character_id,
                idempotency_key: key,
                source_snapshot_id: self.source_snapshot_id,
                expected_revision: self.expected_revision,
                source_world_id: self.source_world_id,
                portal_id: self.portal_id.clone(),
            },
        )
    }

    fn abort(&self, stage_id: SnapshotId) -> RecoveryFuture<'_, ()> {
        self.api
            .abort_rom_handoff(self.auth, self.character_id, stage_id)
    }
}

async fn recover_record<T: RecoveryTransport>(
    transport: &T,
    journal: &RomTravelJournal,
    record: &TravelRecord,
) -> Result<StageOutcome, TravelCoordinatorError> {
    let key = record
        .prepare_idempotency_key
        .ok_or(TravelCoordinatorError::ResponseMismatch)?;
    let status = transport
        .reconcile(key)
        .await
        .map_err(|_| TravelCoordinatorError::Uncertain)?;
    match checked_record_status(&record, &status)? {
        CheckedStatus::Aborted => {
            settle_aborted(journal, record.checkpoint, &status)?;
            Ok(StageOutcome::Aborted)
        }
        CheckedStatus::Staged(stage_id) => {
            transport
                .abort(stage_id)
                .await
                .map_err(|_| TravelCoordinatorError::Uncertain)?;
            let after = transport
                .reconcile(key)
                .await
                .map_err(|_| TravelCoordinatorError::Uncertain)?;
            if !matches!(
                checked_record_status(&record, &after)?,
                CheckedStatus::Aborted
            ) || recovery_stage_id(&after) != stage_id
            {
                return Err(TravelCoordinatorError::Uncertain);
            }
            settle_aborted(journal, record.checkpoint, &after)?;
            Ok(StageOutcome::Aborted)
        }
    }
}

fn recovery_stage_id(status: &RomHandoffRecoveryStatus) -> SnapshotId {
    match status {
        RomHandoffRecoveryStatus::Staged { stage_id, .. }
        | RomHandoffRecoveryStatus::Aborted { stage_id, .. } => *stage_id,
    }
}

fn checked_record_status(
    record: &TravelRecord,
    status: &RomHandoffRecoveryStatus,
) -> Result<CheckedStatus, TravelCoordinatorError> {
    let result = checked_status_fields(
        status,
        record
            .source_world
            .ok_or(TravelCoordinatorError::ResponseMismatch)?,
        record
            .source_snapshot_id
            .ok_or(TravelCoordinatorError::ResponseMismatch)?,
        record
            .source_revision
            .ok_or(TravelCoordinatorError::ResponseMismatch)?,
        record
            .prepare_idempotency_key
            .ok_or(TravelCoordinatorError::ResponseMismatch)?,
    )?;
    if let CheckedStatus::Staged(stage) = result {
        if record
            .server_stage_snapshot_id
            .is_some_and(|known| known != stage)
        {
            return Err(TravelCoordinatorError::ResponseMismatch);
        }
    }
    Ok(result)
}

fn persist_or_resume_intent(
    journal: &RomTravelJournal,
    source_world: RomWorldId,
    source: &PortalTravelSource,
    preparation: TravelPreparation,
) -> Result<(), TravelCoordinatorError> {
    let checkpoint = source.source_revision.value();
    if let Some(record) = journal.read()? {
        if matches!(
            record.phase,
            TravelPhase::Prepared | TravelPhase::SourceSaved
        ) {
            if record.checkpoint != checkpoint
                || record.active_world != source_world
                || record.source_world != Some(source_world)
                || record.portal_id.as_deref() != Some(source.portal_id.as_str())
                || record.source_snapshot_id != Some(source.source_snapshot_id)
                || record.source_revision != Some(source.source_revision)
                || record.prepare_idempotency_key != Some(preparation.prepare_idempotency_key)
                || record.trusted_catalog_digest != Some(preparation.trusted_catalog_digest)
                || record.lease_fence != Some(preparation.lease_fence)
                || record.source_head_save_sha256 != Some(source.source_save_digest)
                || record.destination_world.is_none()
                || (record.phase == TravelPhase::SourceSaved
                    && record.source_save_sha256 != Some(source.source_save_digest))
            {
                return Err(TravelCoordinatorError::ResponseMismatch);
            }
            return Ok(());
        }
    }
    journal.prepare_intent(
        source_world,
        &source.portal_id,
        checkpoint,
        source.source_save_digest,
        preparation,
    )?;
    Ok(())
}

fn settle_aborted(
    journal: &RomTravelJournal,
    checkpoint: u64,
    status: &RomHandoffRecoveryStatus,
) -> Result<(), TravelCoordinatorError> {
    journal.reconcile_aborted_prepare(checkpoint, status)?;
    Ok(())
}

fn record_staged(
    journal: &RomTravelJournal,
    source_world: RomWorldId,
    source: &PortalTravelSource,
    preparation: TravelPreparation,
    destination: &StagedDestination,
) -> Result<(), TravelCoordinatorError> {
    let current = journal
        .read()?
        .ok_or(TravelCoordinatorError::ResponseMismatch)?;
    if current.phase == TravelPhase::SourceSaved {
        if current.destination_world != Some(destination.destination_world)
            || current.source_save_sha256 != Some(source.source_save_digest)
        {
            return Err(TravelCoordinatorError::ResponseMismatch);
        }
        return Ok(());
    }
    journal.begin(
        source_world,
        destination.destination_world,
        &source.portal_id,
        destination.checkpoint,
        preparation,
    )?;
    journal.source_saved(destination.checkpoint, source.source_save_digest)?;
    Ok(())
}

enum CheckedStatus {
    Staged(SnapshotId),
    Aborted,
}

fn checked_status(
    status: &RomHandoffRecoveryStatus,
    source_world: RomWorldId,
    source: &PortalTravelSource,
    key: IdempotencyKey,
) -> Result<CheckedStatus, TravelCoordinatorError> {
    checked_status_fields(
        status,
        source_world,
        source.source_snapshot_id,
        source.source_revision,
        key,
    )
}

fn checked_status_fields(
    status: &RomHandoffRecoveryStatus,
    source_world: RomWorldId,
    source_snapshot_id: SnapshotId,
    source_revision: coop_cloud::Revision,
    key: IdempotencyKey,
) -> Result<CheckedStatus, TravelCoordinatorError> {
    let (stage, snapshot, world, revision, status_key, staged) = match status {
        RomHandoffRecoveryStatus::Staged {
            stage_id,
            source_snapshot_id,
            source_world_id,
            expected_revision,
            idempotency_key,
        } => (
            stage_id,
            source_snapshot_id,
            source_world_id,
            expected_revision,
            idempotency_key,
            true,
        ),
        RomHandoffRecoveryStatus::Aborted {
            stage_id,
            source_snapshot_id,
            source_world_id,
            expected_revision,
            idempotency_key,
        } => (
            stage_id,
            source_snapshot_id,
            source_world_id,
            expected_revision,
            idempotency_key,
            false,
        ),
    };
    if *stage == source_snapshot_id
        || *snapshot != source_snapshot_id
        || *world != source_world
        || *revision != source_revision
        || *status_key != key
    {
        return Err(TravelCoordinatorError::ResponseMismatch);
    }
    Ok(if staged {
        CheckedStatus::Staged(*stage)
    } else {
        CheckedStatus::Aborted
    })
}

fn checked_response(
    response: RomHandoffPrepareResponse,
    stage_id: SnapshotId,
    source_world: RomWorldId,
    source: &PortalTravelSource,
    catalog: &TrustedRomCatalog,
    checkpoint: u64,
) -> Result<StagedDestination, TravelCoordinatorError> {
    if response.api_version != ApiVersion::V1
        || response.stage_id != stage_id
        || response.destination_world_id == source_world
        || response.arrival_portal_id.is_empty()
        || response.arrival_portal_id.len() > 96
        || !response
            .arrival_portal_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        || Sha256Digest::of_bytes(&response.destination_save) != response.destination_save_sha256
        || catalog.world(response.destination_world_id).is_err()
    {
        return Err(TravelCoordinatorError::ResponseMismatch);
    }
    let parsed = parse_v2(
        &response.destination_save,
        RegistryContract::new(IDENTITY_REGISTRY_VERSION, IDENTITY_REGISTRY_DIGEST),
    )
    .map_err(|_| TravelCoordinatorError::ResponseMismatch)?;
    let location: [u8; 3] = parsed
        .logical_sector_payload(1)
        .and_then(|bytes| bytes.get(4..7))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(TravelCoordinatorError::ResponseMismatch)?;
    Ok(StagedDestination {
        checkpoint,
        source_world,
        destination_world: response.destination_world_id,
        source_snapshot_id: source.source_snapshot_id,
        stage_id,
        destination_save_sha256: response.destination_save_sha256,
        destination_save: response.destination_save,
        arrival_portal_id: response.arrival_portal_id,
        arrival_location: location,
        destination_save_generation: parsed.coop().save_generation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom_travel::TravelPhase;
    use coop_cloud::{CharacterId, ClientInstanceId, Revision, SessionEpoch, SessionId};
    use std::{collections::VecDeque, sync::Mutex};
    use uuid::Uuid;

    fn world(number: u16) -> RomWorldId {
        RomWorldId::new(number).unwrap()
    }

    fn snapshot(number: u128) -> SnapshotId {
        SnapshotId::new(Uuid::from_u128(number)).unwrap()
    }

    fn key() -> IdempotencyKey {
        IdempotencyKey::new(Uuid::from_u128(300)).unwrap()
    }

    fn source() -> PortalTravelSource {
        PortalTravelSource {
            portal_id: "hoenn_to_cormoria".to_owned(),
            portal_sequence: 8,
            checkpoint_ready_sequence: 9,
            source_snapshot_id: snapshot(100),
            source_revision: Revision::new(7),
            source_save_digest: Sha256Digest::from_bytes([3; 32]),
            source_save_generation: 2,
        }
    }

    fn preparation() -> TravelPreparation {
        TravelPreparation::new(
            snapshot(100),
            Revision::new(7),
            key(),
            Sha256Digest::from_bytes([4; 32]),
            LeaseFenceIdentity::new(
                SessionId::new(Uuid::from_u128(201)).unwrap(),
                SessionEpoch::new(1).unwrap(),
                ClientInstanceId::new(Uuid::from_u128(202)).unwrap(),
            ),
        )
    }

    fn journal() -> (tempfile::TempDir, RomTravelJournal) {
        let root = tempfile::tempdir().unwrap();
        let character = CharacterId::new(Uuid::from_u128(203)).unwrap();
        let journal = RomTravelJournal::new(root.path(), character, [world(1), world(2)]).unwrap();
        journal.initialize(world(1)).unwrap();
        (root, journal)
    }

    fn status(staged: bool) -> RomHandoffRecoveryStatus {
        if staged {
            RomHandoffRecoveryStatus::Staged {
                stage_id: snapshot(101),
                source_snapshot_id: snapshot(100),
                source_world_id: world(1),
                expected_revision: Revision::new(7),
                idempotency_key: key(),
            }
        } else {
            RomHandoffRecoveryStatus::Aborted {
                stage_id: snapshot(101),
                source_snapshot_id: snapshot(100),
                source_world_id: world(1),
                expected_revision: Revision::new(7),
                idempotency_key: key(),
            }
        }
    }

    #[test]
    fn staged_result_records_source_checkpoint_without_switching_authority() {
        let (_root, journal) = journal();
        let source = source();
        journal
            .prepare_intent(
                world(1),
                &source.portal_id,
                7,
                source.source_save_digest,
                preparation(),
            )
            .unwrap();
        assert!(
            matches!(checked_status(&status(true), world(1), &source, key()).unwrap(), CheckedStatus::Staged(id) if id == snapshot(101))
        );
        let destination = StagedDestination {
            checkpoint: 7,
            source_world: world(1),
            destination_world: world(2),
            source_snapshot_id: source.source_snapshot_id,
            stage_id: snapshot(101),
            destination_save_sha256: Sha256Digest::from_bytes([5; 32]),
            destination_save: vec![1],
            arrival_portal_id: "cormoria_arrival".to_owned(),
            arrival_location: [79, 1, 0],
            destination_save_generation: 1,
        };
        record_staged(&journal, world(1), &source, preparation(), &destination).unwrap();
        let record = journal.read().unwrap().unwrap();
        assert_eq!(record.phase, TravelPhase::SourceSaved);
        assert_eq!(record.active_world, world(1));
        assert_eq!(record.destination_world, Some(world(2)));
        assert_eq!(record.source_save_sha256, Some(source.source_save_digest));
        assert!(record.server_stage_snapshot_id.is_none());
    }

    #[test]
    fn lost_prepare_response_reuses_exact_intent_and_can_settle_abort() {
        let (_root, journal) = journal();
        let source = source();
        let original = journal
            .prepare_intent(
                world(1),
                &source.portal_id,
                7,
                source.source_save_digest,
                preparation(),
            )
            .unwrap();
        let retry = journal
            .prepare_intent(
                world(1),
                &source.portal_id,
                7,
                source.source_save_digest,
                preparation(),
            )
            .unwrap();
        assert_eq!(retry, original);
        assert!(matches!(
            checked_status(&status(false), world(1), &source, key()).unwrap(),
            CheckedStatus::Aborted
        ));
        settle_aborted(&journal, 7, &status(false)).unwrap();
        let record = journal.read().unwrap().unwrap();
        assert_eq!(record.phase, TravelPhase::Aborted);
        assert_eq!(record.active_world, world(1));
        assert_eq!(record.server_stage_snapshot_id, Some(snapshot(101)));
    }

    #[test]
    fn stale_reconciliation_cannot_advance_intent() {
        let (_root, journal) = journal();
        let source = source();
        journal
            .prepare_intent(
                world(1),
                &source.portal_id,
                7,
                source.source_save_digest,
                preparation(),
            )
            .unwrap();
        let wrong = RomHandoffRecoveryStatus::Staged {
            stage_id: snapshot(101),
            source_snapshot_id: snapshot(100),
            source_world_id: world(1),
            expected_revision: Revision::new(6),
            idempotency_key: key(),
        };
        assert!(matches!(
            checked_status(&wrong, world(1), &source, key()),
            Err(TravelCoordinatorError::ResponseMismatch)
        ));
        assert_eq!(
            journal.read().unwrap().unwrap().phase,
            TravelPhase::PrepareIntent
        );
    }

    #[test]
    fn reopened_source_saved_resumes_only_the_exact_prepare_key() {
        let (root, journal) = journal();
        let source = source();
        persist_or_resume_intent(&journal, world(1), &source, preparation()).unwrap();
        let destination = StagedDestination {
            checkpoint: 7,
            source_world: world(1),
            destination_world: world(2),
            source_snapshot_id: source.source_snapshot_id,
            stage_id: snapshot(101),
            destination_save_sha256: Sha256Digest::from_bytes([5; 32]),
            destination_save: vec![1],
            arrival_portal_id: "cormoria_arrival".to_owned(),
            arrival_location: [79, 1, 0],
            destination_save_generation: 1,
        };
        record_staged(&journal, world(1), &source, preparation(), &destination).unwrap();
        let reopened = RomTravelJournal::new(
            root.path(),
            CharacterId::new(Uuid::from_u128(203)).unwrap(),
            [world(1), world(2)],
        )
        .unwrap();
        persist_or_resume_intent(&reopened, world(1), &source, preparation()).unwrap();
        record_staged(&reopened, world(1), &source, preparation(), &destination).unwrap();
        assert_eq!(
            reopened.read().unwrap().unwrap().phase,
            TravelPhase::SourceSaved
        );
        let mut other = preparation();
        other.prepare_idempotency_key = IdempotencyKey::new(Uuid::from_u128(301)).unwrap();
        assert!(matches!(
            persist_or_resume_intent(&reopened, world(1), &source, other),
            Err(TravelCoordinatorError::ResponseMismatch)
        ));
        assert_eq!(reopened.read().unwrap().unwrap().active_world, world(1));
    }

    #[test]
    fn replacement_lease_requires_reconcile_then_confirmed_abort() {
        let (_root, journal) = journal();
        let source = source();
        persist_or_resume_intent(&journal, world(1), &source, preparation()).unwrap();
        let record = journal.read().unwrap().unwrap();
        let replacement = LeaseFenceIdentity::new(
            SessionId::new(Uuid::from_u128(401)).unwrap(),
            SessionEpoch::new(2).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(202)).unwrap(),
        );
        assert!(requires_settlement(
            &record,
            replacement,
            preparation().trusted_catalog_digest
        ));
        assert!(matches!(
            checked_record_status(&record, &status(true)).unwrap(),
            CheckedStatus::Staged(id) if id == snapshot(101)
        ));
        assert_eq!(
            journal.read().unwrap().unwrap().phase,
            TravelPhase::PrepareIntent
        );
        assert!(matches!(
            checked_record_status(&record, &status(false)).unwrap(),
            CheckedStatus::Aborted
        ));
        settle_aborted(&journal, 7, &status(false)).unwrap();
        assert_eq!(journal.read().unwrap().unwrap().phase, TravelPhase::Aborted);
        assert_eq!(journal.read().unwrap().unwrap().active_world, world(1));
    }

    #[test]
    fn changed_catalog_digest_settles_old_key_before_new_checkpoint() {
        let (_root, journal) = journal();
        let source = source();
        persist_or_resume_intent(&journal, world(1), &source, preparation()).unwrap();
        let record = journal.read().unwrap().unwrap();
        assert!(requires_settlement(
            &record,
            preparation().lease_fence,
            Sha256Digest::from_bytes([9; 32]),
        ));
        assert!(matches!(
            persist_or_resume_intent(
                &journal,
                world(1),
                &source,
                TravelPreparation {
                    trusted_catalog_digest: Sha256Digest::from_bytes([9; 32]),
                    ..preparation()
                }
            ),
            Err(TravelCoordinatorError::Journal(RomTravelError::Conflict))
        ));
        assert!(matches!(
            checked_record_status(&record, &status(false)).unwrap(),
            CheckedStatus::Aborted
        ));
        settle_aborted(&journal, 7, &status(false)).unwrap();
        assert_eq!(journal.read().unwrap().unwrap().phase, TravelPhase::Aborted);
        assert_eq!(journal.read().unwrap().unwrap().active_world, world(1));
    }

    struct FakeRecovery {
        events: Mutex<Vec<&'static str>>,
        statuses: Mutex<VecDeque<RomHandoffRecoveryStatus>>,
    }

    impl RecoveryTransport for FakeRecovery {
        fn reconcile(
            &self,
            request_key: IdempotencyKey,
        ) -> RecoveryFuture<'_, RomHandoffRecoveryStatus> {
            assert_eq!(request_key, key());
            self.events.lock().unwrap().push("reconcile");
            let result = self
                .statuses
                .lock()
                .unwrap()
                .pop_front()
                .expect("expected server status");
            Box::pin(async move { Ok(result) })
        }

        fn abort(&self, stage_id: SnapshotId) -> RecoveryFuture<'_, ()> {
            assert_eq!(stage_id, snapshot(101));
            self.events.lock().unwrap().push("abort");
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn replacement_recovery_aborts_live_stage_before_local_settlement() {
        let (_root, journal) = journal();
        let source = source();
        persist_or_resume_intent(&journal, world(1), &source, preparation()).unwrap();
        let record = journal.read().unwrap().unwrap();
        let transport = FakeRecovery {
            events: Mutex::new(Vec::new()),
            statuses: Mutex::new(VecDeque::from([status(true), status(false)])),
        };
        assert_eq!(
            recover_record(&transport, &journal, &record).await.unwrap(),
            StageOutcome::Aborted
        );
        assert_eq!(
            *transport.events.lock().unwrap(),
            ["reconcile", "abort", "reconcile"]
        );
        let settled = journal.read().unwrap().unwrap();
        assert_eq!(settled.phase, TravelPhase::Aborted);
        assert_eq!(settled.active_world, world(1));
        assert_eq!(settled.server_stage_snapshot_id, Some(snapshot(101)));
    }
}

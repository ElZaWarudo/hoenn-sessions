//! Durable coordinator for a paired cross-ROM handoff.
//!
//! The paired server protocol has two independent members, so the launcher
//! cannot treat a successful request from one client as a committed travel.
//! This module orders the local journal, staged-save validation, ROM arrival
//! proof, and server acknowledgement.  The active ROM is switched by the
//! desktop/embedded owner after [`PairedHandoffOutcome::Committed`] returns.

use std::future::Future;

use coop_cloud::{
    ArtifactIdentity, GroupRomHandoffAbortRequest, GroupRomHandoffArrivalRequest,
    GroupRomHandoffStatus, GroupRomHandoffStatusRequest, IdempotencyKey, RuntimeBuildIdentity,
    SnapshotId, SnapshotRecord,
};
use coop_protocol::{IDENTITY_REGISTRY_DIGEST, IDENTITY_REGISTRY_VERSION, RomWorldId};
use coop_save::{RegistryContract, parse_v2};
use thiserror::Error;

use crate::{
    TrustedRomCatalog,
    arrival_verifier::AuthenticatedArrivalEvidence,
    auth::AuthSession,
    paired_travel::{
        PairedArrivalEvidence, PairedCommit, PairedJoinIntent, PairedPhase, PairedStage,
        PairedTerminal, PairedTravelError, PairedTravelJournal, PairedTravelRecord,
    },
    session::{CloudApi, PortalTravelSource, SessionError},
    travel_coordinator::{
        PairedStagedDestination, TravelCoordinatorError, checked_paired_destination,
    },
};

/// The coordinator has reached a durable boundary. A caller may invoke the
/// same function again with the same journal and client intent key after a
/// process restart.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairedHandoffOutcome {
    /// The companion has not joined or has not acknowledged its arrival yet.
    Pending { attempt_key: IdempotencyKey },
    /// This member is staged and has acknowledged arrival; the companion still
    /// must acknowledge before the server can atomically commit both members.
    Staged {
        attempt_key: IdempotencyKey,
        stage_id: SnapshotId,
    },
    /// Both members' server-side arrivals were committed atomically.
    Committed(PairedCommit),
    /// The server recorded an abort or expiry for this exact attempt.
    Aborted,
}

/// Errors from the paired coordinator. `V` is deliberately supplied by the
/// caller's ROM verifier so platform-specific verifier errors stay out of the
/// cloud/session layer.
#[derive(Debug, Error)]
pub enum PairedCoordinatorError<V> {
    #[error("paired travel journal failed")]
    Journal(#[from] PairedTravelError),
    #[error("paired cloud request failed")]
    Cloud(#[from] SessionError),
    #[error("paired staged destination failed validation")]
    Destination(#[from] TravelCoordinatorError),
    #[error("paired server response does not match the durable handoff")]
    ResponseMismatch,
    #[error("paired ROM arrival verification failed")]
    Verification(V),
    #[error("paired arrival failed and its recovery status could not be read")]
    ArrivalRecovery(#[source] SessionError),
    #[error("paired arrival verification failed and server abort also failed")]
    AbortRecovery(#[source] SessionError),
}

/// Run or recover one member's paired handoff.
///
/// `verify_arrival` is invoked only after the exact server-staged save has
/// been validated and the expected nonce has been durably recorded. It must
/// authenticate that the destination ROM loaded that save and return the
/// platform's authenticated arrival evidence. The coordinator records that
/// evidence before sending the server arrival request.
///
/// `resolve_destination_build` runs only after the staged destination's world
/// has been checked against the signed catalog. It must return the exact
/// runtime identity for that world; the returned identity is bound into the
/// server arrival MAC and request.
///
/// A response of `Committed` is returned only after the server has observed
/// both members' authenticated arrivals and returned this member's committed
/// snapshot. A lost arrival response is reconciled through the exact status
/// request before the error is surfaced.
pub async fn run_paired_handoff<C, B, V, F, Fut>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: PairedJoinIntent,
    source: &PortalTravelSource,
    source_world: RomWorldId,
    catalog: &TrustedRomCatalog,
    resolve_destination_build: B,
    expected_nonce: [u8; 16],
    verify_arrival: F,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    B: Fn(RomWorldId) -> Result<RuntimeBuildIdentity, SessionError> + Sync,
    V: Send + 'static,
    F: FnOnce(&PairedStagedDestination, [u8; 16]) -> Fut,
    Fut: Future<Output = Result<AuthenticatedArrivalEvidence, V>> + Send,
{
    validate_source_binding::<V>(&intent, source, source_world, catalog)?;

    let mut record = match journal.read()? {
        Some(current) if current.intent == intent => current,
        Some(current) if !is_terminal(&current) => return Err(PairedTravelError::Conflict.into()),
        Some(_) => journal.begin(&intent)?,
        None => journal.begin(&intent)?,
    };

    if let Some(outcome) = terminal_outcome(&record)? {
        return Ok(outcome);
    }

    let mut verify_arrival = Some(verify_arrival);
    let status = if record.phase == PairedPhase::JoinIntent {
        let joined = cloud
            .join_group_rom_handoff(auth, intent.request.clone())
            .await?;
        let key = status_key(&joined)?;
        validate_join_response(&joined, &intent, key)?;
        record = journal.record_attempt(intent.request.client_intent_key, key)?;
        joined
    } else {
        let attempt_key = record
            .attempt_key
            .ok_or(PairedCoordinatorError::ResponseMismatch)?;
        cloud
            .status_group_rom_handoff(
                auth,
                GroupRomHandoffStatusRequest {
                    api_version: intent.request.api_version,
                    group_id: intent.request.group_id,
                    fence: intent.request.fence,
                    idempotency_key: attempt_key,
                },
            )
            .await?
    };

    process_status(
        cloud,
        auth,
        journal,
        &intent,
        source,
        source_world,
        catalog,
        &resolve_destination_build,
        expected_nonce,
        &mut record,
        status,
        &mut verify_arrival,
    )
    .await
}

/// Reconcile a persisted paired handoff without invoking a destination ROM
/// verifier. This is useful after the verifier has already written
/// `ArrivalVerified` and the launcher was interrupted during the network call.
pub async fn recover_paired_handoff<C>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<()>>
where
    C: CloudApi,
{
    let mut record = journal
        .read()?
        .ok_or(PairedCoordinatorError::ResponseMismatch)?;
    if let Some(outcome) = terminal_outcome(&record)? {
        return Ok(outcome);
    }
    let status = if record.phase == PairedPhase::JoinIntent {
        let joined = cloud
            .join_group_rom_handoff(auth, record.intent.request.clone())
            .await?;
        let key = status_key(&joined)?;
        validate_join_response(&joined, &record.intent, key)?;
        record = journal.record_attempt(record.intent.request.client_intent_key, key)?;
        joined
    } else {
        let attempt_key = record
            .attempt_key
            .ok_or(PairedCoordinatorError::ResponseMismatch)?;
        cloud
            .status_group_rom_handoff(
                auth,
                GroupRomHandoffStatusRequest {
                    api_version: record.intent.request.api_version,
                    group_id: record.intent.request.group_id,
                    fence: record.intent.request.fence,
                    idempotency_key: attempt_key,
                },
            )
            .await?
    };
    recover_status(journal, &record, status).await
}

async fn process_status<C, B, V, F, Fut>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: &PairedJoinIntent,
    source: &PortalTravelSource,
    source_world: RomWorldId,
    catalog: &TrustedRomCatalog,
    resolve_destination_build: &B,
    expected_nonce: [u8; 16],
    record: &mut PairedTravelRecord,
    status: GroupRomHandoffStatus,
    verify_arrival: &mut Option<F>,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    B: Fn(RomWorldId) -> Result<RuntimeBuildIdentity, SessionError> + Sync,
    V: Send + 'static,
    F: FnOnce(&PairedStagedDestination, [u8; 16]) -> Fut,
    Fut: Future<Output = Result<AuthenticatedArrivalEvidence, V>> + Send,
{
    let attempt_key = status_key(&status)?;
    validate_status_identity(&status, intent, attempt_key)?;
    if record.attempt_key != Some(attempt_key) {
        return Err(PairedCoordinatorError::ResponseMismatch);
    }
    // Once a stage exists, its nonce is the only retry identity accepted.
    // The caller may be a fresh process and must not be able to replace it.
    let expected_nonce = record
        .stage
        .as_ref()
        .map_or(expected_nonce, |stage| stage.expected_nonce);
    match status {
        GroupRomHandoffStatus::Pending { .. } => Ok(PairedHandoffOutcome::Pending { attempt_key }),
        GroupRomHandoffStatus::Aborted { .. } => {
            journal.record_aborted(intent.request.client_intent_key)?;
            Ok(PairedHandoffOutcome::Aborted)
        }
        GroupRomHandoffStatus::Staged { .. } => {
            let checked = checked_paired_destination(
                status,
                intent.request.group_id,
                attempt_key,
                source_world,
                source,
                catalog,
            )?;
            let stage = PairedStage {
                stage_id: checked.destination().stage_id(),
                destination_world_id: checked.destination().destination_world(),
                arrival_portal_id: checked.destination().arrival_portal_id().to_owned(),
                destination_save_sha256: checked.destination().destination_save_sha256(),
                arrival_challenge: checked.arrival_challenge(),
                expected_nonce,
            };
            if record.phase == PairedPhase::AttemptKnown {
                *record = journal.record_staged(intent.request.client_intent_key, stage.clone())?;
            } else if record.phase != PairedPhase::Staged
                && record.phase != PairedPhase::ArrivalVerified
            {
                return Err(PairedCoordinatorError::ResponseMismatch);
            }
            let persisted_stage = record
                .stage
                .as_ref()
                .ok_or(PairedCoordinatorError::ResponseMismatch)?;
            if persisted_stage != &stage {
                return Err(PairedCoordinatorError::ResponseMismatch);
            }

            if record.phase == PairedPhase::Staged {
                let verifier = verify_arrival
                    .take()
                    .ok_or(PairedCoordinatorError::ResponseMismatch)?;
                let authenticated = match verifier(&checked, expected_nonce).await {
                    Ok(evidence) => evidence,
                    Err(error) => {
                        abort_for_failure(cloud, auth, journal, intent, attempt_key).await?;
                        return Err(PairedCoordinatorError::Verification(error));
                    }
                };
                let evidence = PairedArrivalEvidence {
                    stage_id: stage.stage_id,
                    destination_save_sha256: authenticated.full_sav_sha256,
                    nonce: authenticated.nonce,
                };
                let expected_flash = staged_flash_digest(&checked);
                let location = checked.destination().arrival_location();
                if !arrival_evidence_matches(
                    &authenticated,
                    stage.destination_save_sha256,
                    expected_flash,
                    stage.destination_world_id,
                    checked.destination().destination_save_generation(),
                    location[0],
                    location[1],
                    stage.expected_nonce,
                ) {
                    abort_for_failure(cloud, auth, journal, intent, attempt_key).await?;
                    return Err(PairedCoordinatorError::ResponseMismatch);
                }
                *record =
                    journal.record_verified_arrival(intent.request.client_intent_key, evidence)?;
            }
            submit_arrival(
                cloud,
                auth,
                journal,
                intent,
                resolve_destination_build,
                record,
                checked,
            )
            .await
        }
        GroupRomHandoffStatus::Committed {
            destination_zone,
            group_zone_revision,
            own_snapshot,
            ..
        } => {
            let commit =
                checked_commit(record, destination_zone, group_zone_revision, own_snapshot)?;
            *record = journal.record_committed(intent.request.client_intent_key, commit.clone())?;
            Ok(PairedHandoffOutcome::Committed(commit))
        }
    }
}

async fn recover_status(
    journal: &PairedTravelJournal,
    record: &PairedTravelRecord,
    status: GroupRomHandoffStatus,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<()>> {
    let attempt_key = status_key(&status)?;
    validate_status_identity(&status, &record.intent, attempt_key)?;
    if record.attempt_key != Some(attempt_key) {
        return Err(PairedCoordinatorError::ResponseMismatch);
    }
    match status {
        GroupRomHandoffStatus::Pending { .. } => Ok(PairedHandoffOutcome::Pending { attempt_key }),
        GroupRomHandoffStatus::Aborted { .. } => {
            journal.record_aborted(record.intent.request.client_intent_key)?;
            Ok(PairedHandoffOutcome::Aborted)
        }
        GroupRomHandoffStatus::Committed {
            destination_zone,
            group_zone_revision,
            own_snapshot,
            ..
        } => {
            let commit =
                checked_commit(record, destination_zone, group_zone_revision, own_snapshot)?;
            journal.record_committed(record.intent.request.client_intent_key, commit.clone())?;
            Ok(PairedHandoffOutcome::Committed(commit))
        }
        GroupRomHandoffStatus::Staged { .. } => {
            if record.phase != PairedPhase::ArrivalVerified {
                return Err(PairedCoordinatorError::ResponseMismatch);
            }
            Ok(PairedHandoffOutcome::Staged {
                attempt_key,
                stage_id: record
                    .stage
                    .as_ref()
                    .ok_or(PairedCoordinatorError::ResponseMismatch)?
                    .stage_id,
            })
        }
    }
}

async fn submit_arrival<C, B, V>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: &PairedJoinIntent,
    resolve_destination_build: &B,
    record: &PairedTravelRecord,
    checked: PairedStagedDestination,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    B: Fn(RomWorldId) -> Result<RuntimeBuildIdentity, SessionError> + Sync,
    V: Send + 'static,
{
    let stage = record
        .stage
        .as_ref()
        .ok_or(PairedCoordinatorError::ResponseMismatch)?;
    let attempt_key = record
        .attempt_key
        .ok_or(PairedCoordinatorError::ResponseMismatch)?;
    let destination_build = resolve_destination_build(stage.destination_world_id)?;
    let mut request = GroupRomHandoffArrivalRequest {
        api_version: intent.request.api_version,
        group_id: intent.request.group_id,
        fence: intent.request.fence,
        idempotency_key: attempt_key,
        stage_id: stage.stage_id,
        destination_save_sha256: stage.destination_save_sha256,
        destination_build,
        acknowledgment_mac: coop_cloud::Sha256Digest::from_bytes([0; 32]),
    };
    request.acknowledgment_mac = request.expected_mac(stage.arrival_challenge);
    let status = match cloud.arrive_group_rom_handoff(auth, request).await {
        Ok(status) => status,
        Err(error) => {
            let recovered = cloud
                .status_group_rom_handoff(
                    auth,
                    GroupRomHandoffStatusRequest {
                        api_version: intent.request.api_version,
                        group_id: intent.request.group_id,
                        fence: intent.request.fence,
                        idempotency_key: attempt_key,
                    },
                )
                .await
                .map_err(|_| PairedCoordinatorError::ArrivalRecovery(error))?;
            return recover_arrival_status(
                cloud, auth, journal, intent, record, checked, recovered,
            )
            .await;
        }
    };
    handle_arrival_status(cloud, auth, journal, intent, record, checked, status).await
}

async fn recover_arrival_status<C, V>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: &PairedJoinIntent,
    record: &PairedTravelRecord,
    checked: PairedStagedDestination,
    status: GroupRomHandoffStatus,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    V: Send + 'static,
{
    handle_arrival_status(cloud, auth, journal, intent, record, checked, status).await
}

async fn handle_arrival_status<C, V>(
    _cloud: &C,
    _auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: &PairedJoinIntent,
    record: &PairedTravelRecord,
    checked: PairedStagedDestination,
    status: GroupRomHandoffStatus,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    V: Send + 'static,
{
    let attempt_key = status_key(&status)?;
    validate_status_identity(&status, intent, attempt_key)?;
    if record.attempt_key != Some(attempt_key)
        || checked.group_id() != intent.request.group_id
        || checked.attempt_key() != attempt_key
    {
        return Err(PairedCoordinatorError::ResponseMismatch);
    }
    match status {
        GroupRomHandoffStatus::Pending { .. } => Ok(PairedHandoffOutcome::Pending { attempt_key }),
        GroupRomHandoffStatus::Staged {
            stage_id,
            destination_world_id,
            arrival_portal_id,
            destination_save_sha256,
            arrival_challenge,
            ..
        } => {
            let stage = checked.destination();
            if stage_id != stage.stage_id()
                || destination_world_id != stage.destination_world()
                || arrival_portal_id != stage.arrival_portal_id()
                || destination_save_sha256 != stage.destination_save_sha256()
                || arrival_challenge != checked.arrival_challenge()
            {
                return Err(PairedCoordinatorError::ResponseMismatch);
            }
            Ok(PairedHandoffOutcome::Staged {
                attempt_key,
                stage_id,
            })
        }
        GroupRomHandoffStatus::Aborted { .. } => {
            journal.record_aborted(intent.request.client_intent_key)?;
            Ok(PairedHandoffOutcome::Aborted)
        }
        GroupRomHandoffStatus::Committed {
            destination_zone,
            group_zone_revision,
            own_snapshot,
            ..
        } => {
            let commit =
                checked_commit(record, destination_zone, group_zone_revision, own_snapshot)?;
            journal.record_committed(intent.request.client_intent_key, commit.clone())?;
            Ok(PairedHandoffOutcome::Committed(commit))
        }
    }
}

fn checked_commit<V: Send + 'static>(
    record: &PairedTravelRecord,
    destination_zone: coop_protocol::WorldZone,
    group_zone_revision: u64,
    own_snapshot: SnapshotRecord,
) -> Result<PairedCommit, PairedCoordinatorError<V>> {
    let stage = record
        .stage
        .as_ref()
        .ok_or(PairedCoordinatorError::ResponseMismatch)?;
    if record.phase != PairedPhase::ArrivalVerified
        || own_snapshot.snapshot_id != stage.stage_id
        || own_snapshot.character_id != record.character_id
        || own_snapshot.rom_world_id != stage.destination_world_id
        || own_snapshot.revision.value() == 0
        || group_zone_revision == 0
        || destination_zone.validate().is_err()
        || !own_snapshot.files.iter().any(|file| {
            file.artifact == ArtifactIdentity::CharacterSav
                && file.sha256 == stage.destination_save_sha256
        })
    {
        return Err(PairedCoordinatorError::ResponseMismatch);
    }
    Ok(PairedCommit {
        destination_zone,
        group_zone_revision,
        own_snapshot_id: own_snapshot.snapshot_id,
        own_world_id: own_snapshot.rom_world_id,
        own_revision: own_snapshot.revision,
    })
}

async fn abort_for_failure<C, V>(
    cloud: &C,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
    intent: &PairedJoinIntent,
    attempt_key: IdempotencyKey,
) -> Result<PairedHandoffOutcome, PairedCoordinatorError<V>>
where
    C: CloudApi,
    V: Send + 'static,
{
    let request = GroupRomHandoffAbortRequest {
        api_version: intent.request.api_version,
        group_id: intent.request.group_id,
        fence: intent.request.fence,
        idempotency_key: attempt_key,
    };
    let status = cloud
        .abort_group_rom_handoff(auth, request)
        .await
        .map_err(PairedCoordinatorError::AbortRecovery)?;
    validate_status_identity(&status, intent, attempt_key)?;
    match status {
        GroupRomHandoffStatus::Aborted { .. } => {
            journal.record_aborted(intent.request.client_intent_key)?;
            Ok(PairedHandoffOutcome::Aborted)
        }
        _ => Err(PairedCoordinatorError::ResponseMismatch),
    }
}

fn status_key<V: Send + 'static>(
    status: &GroupRomHandoffStatus,
) -> Result<IdempotencyKey, PairedCoordinatorError<V>> {
    Ok(match status {
        GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        }
        | GroupRomHandoffStatus::Staged {
            idempotency_key, ..
        }
        | GroupRomHandoffStatus::Committed {
            idempotency_key, ..
        }
        | GroupRomHandoffStatus::Aborted {
            idempotency_key, ..
        } => *idempotency_key,
    })
}

fn validate_source_binding<V: Send + 'static>(
    intent: &PairedJoinIntent,
    source: &PortalTravelSource,
    source_world: RomWorldId,
    catalog: &TrustedRomCatalog,
) -> Result<(), PairedCoordinatorError<V>> {
    if !source_binding_matches(intent, source, source_world, catalog.digest()) {
        return Err(PairedCoordinatorError::ResponseMismatch);
    }
    Ok(())
}

fn source_binding_matches(
    intent: &PairedJoinIntent,
    source: &PortalTravelSource,
    source_world: RomWorldId,
    catalog_digest: coop_cloud::Sha256Digest,
) -> bool {
    intent.request.source_snapshot_id == source.source_snapshot_id
        && intent.request.fence.current_revision == source.source_revision
        && intent.request.portal_id == source.portal_id
        && intent.source_world_id == source_world
        && intent.source_save_sha256 == source.source_save_digest
        && intent.catalog_digest == catalog_digest
}

fn validate_join_response<V: Send + 'static>(
    joined: &GroupRomHandoffStatus,
    intent: &PairedJoinIntent,
    attempt_key: IdempotencyKey,
) -> Result<(), PairedCoordinatorError<V>> {
    validate_status_identity(joined, intent, attempt_key)
}

fn staged_flash_digest(checked: &PairedStagedDestination) -> Option<coop_cloud::Sha256Digest> {
    parse_v2(
        checked.destination().destination_save(),
        RegistryContract::new(IDENTITY_REGISTRY_VERSION, IDENTITY_REGISTRY_DIGEST),
    )
    .ok()
    .map(|parsed| coop_cloud::Sha256Digest::of_bytes(parsed.flash_bytes()))
}

fn arrival_evidence_matches(
    evidence: &AuthenticatedArrivalEvidence,
    expected_full_sav_sha256: coop_cloud::Sha256Digest,
    expected_flash_sha256: Option<coop_cloud::Sha256Digest>,
    expected_world: RomWorldId,
    expected_save_generation: u32,
    expected_map_group: u8,
    expected_map_num: u8,
    expected_nonce: [u8; 16],
) -> bool {
    evidence.full_sav_sha256 == expected_full_sav_sha256
        && expected_flash_sha256.is_none_or(|digest| evidence.flash_sha256 == digest)
        && evidence.destination_world == expected_world
        && evidence.save_generation == expected_save_generation
        && evidence.map_group == expected_map_group
        && evidence.map_num == expected_map_num
        && evidence.nonce == expected_nonce
}

fn validate_status_identity<V: Send + 'static>(
    status: &GroupRomHandoffStatus,
    intent: &PairedJoinIntent,
    attempt_key: IdempotencyKey,
) -> Result<(), PairedCoordinatorError<V>> {
    let valid = match status {
        GroupRomHandoffStatus::Pending {
            group_id,
            idempotency_key,
            portal_id,
            ..
        } => {
            *group_id == intent.request.group_id
                && *idempotency_key == attempt_key
                && portal_id == &intent.request.portal_id
        }
        GroupRomHandoffStatus::Staged {
            group_id,
            idempotency_key,
            ..
        }
        | GroupRomHandoffStatus::Committed {
            group_id,
            idempotency_key,
            ..
        }
        | GroupRomHandoffStatus::Aborted {
            group_id,
            idempotency_key,
        } => *group_id == intent.request.group_id && *idempotency_key == attempt_key,
    };
    if valid {
        Ok(())
    } else {
        Err(PairedCoordinatorError::ResponseMismatch)
    }
}

fn is_terminal(record: &PairedTravelRecord) -> bool {
    matches!(
        record.phase,
        PairedPhase::Committed | PairedPhase::Adopted | PairedPhase::Aborted
    )
}

fn terminal_outcome<V>(
    record: &PairedTravelRecord,
) -> Result<Option<PairedHandoffOutcome>, PairedCoordinatorError<V>> {
    Ok(match (&record.phase, record.terminal.as_ref()) {
        (
            PairedPhase::Committed | PairedPhase::Adopted,
            Some(PairedTerminal::Committed(commit)),
        ) => Some(PairedHandoffOutcome::Committed(commit.clone())),
        (PairedPhase::Aborted, Some(PairedTerminal::Aborted)) => {
            Some(PairedHandoffOutcome::Aborted)
        }
        (PairedPhase::Committed | PairedPhase::Adopted | PairedPhase::Aborted, _) => {
            return Err(PairedCoordinatorError::ResponseMismatch);
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{
        ApiVersion, CharacterId, ClientInstanceId, GroupId, GroupRomHandoffJoinRequest, LeaseFence,
        Revision, SessionEpoch, SessionId, Sha256Digest,
    };
    use uuid::Uuid;

    fn fixture_intent() -> PairedJoinIntent {
        let character_id = CharacterId::new(Uuid::from_u128(1)).unwrap();
        PairedJoinIntent {
            request: GroupRomHandoffJoinRequest {
                api_version: ApiVersion::V1,
                group_id: GroupId::new(Uuid::from_u128(2)).unwrap(),
                fence: LeaseFence::new(
                    SessionId::new(Uuid::from_u128(3)).unwrap(),
                    character_id,
                    Revision::new(4),
                    SessionEpoch::new(5).unwrap(),
                    ClientInstanceId::new(Uuid::from_u128(6)).unwrap(),
                ),
                source_snapshot_id: SnapshotId::new(Uuid::from_u128(7)).unwrap(),
                portal_id: "to_cormoria".to_owned(),
                client_intent_key: IdempotencyKey::new(Uuid::from_u128(8)).unwrap(),
            },
            source_world_id: RomWorldId::new(1).unwrap(),
            source_save_sha256: Sha256Digest::from_bytes([10; 32]),
            catalog_digest: Sha256Digest::from_bytes([9; 32]),
        }
    }

    fn fixture_source() -> PortalTravelSource {
        let intent = fixture_intent();
        PortalTravelSource {
            portal_id: intent.request.portal_id,
            portal_sequence: 1,
            checkpoint_ready_sequence: 2,
            source_snapshot_id: intent.request.source_snapshot_id,
            source_revision: intent.request.fence.current_revision,
            source_save_digest: intent.source_save_sha256,
            source_save_generation: 3,
        }
    }

    #[test]
    fn join_response_is_fenced_to_the_exact_group_and_portal_before_attempt_persistence() {
        let intent = fixture_intent();
        let key = intent.request.client_intent_key;
        let status = GroupRomHandoffStatus::Pending {
            group_id: intent.request.group_id,
            idempotency_key: key,
            portal_id: intent.request.portal_id.clone(),
            submitted_by: [true, false],
        };
        assert!(validate_join_response::<()>(&status, &intent, key).is_ok());

        let wrong_portal = GroupRomHandoffStatus::Pending {
            group_id: intent.request.group_id,
            idempotency_key: key,
            portal_id: "other_portal".to_owned(),
            submitted_by: [true, false],
        };
        assert!(validate_join_response::<()>(&wrong_portal, &intent, key).is_err());

        let wrong_group = GroupRomHandoffStatus::Pending {
            group_id: GroupId::new(Uuid::from_u128(10)).unwrap(),
            idempotency_key: key,
            portal_id: intent.request.portal_id.clone(),
            submitted_by: [true, false],
        };
        assert!(validate_join_response::<()>(&wrong_group, &intent, key).is_err());
    }

    #[test]
    fn abort_response_must_match_group_and_attempt_before_journal_change() {
        let intent = fixture_intent();
        let key = intent.request.client_intent_key;
        let valid = GroupRomHandoffStatus::Aborted {
            group_id: intent.request.group_id,
            idempotency_key: key,
        };
        assert!(validate_status_identity::<()>(&valid, &intent, key).is_ok());
        let wrong_group = GroupRomHandoffStatus::Aborted {
            group_id: GroupId::new(Uuid::from_u128(10)).unwrap(),
            idempotency_key: key,
        };
        assert!(validate_status_identity::<()>(&wrong_group, &intent, key).is_err());
        let wrong_key = GroupRomHandoffStatus::Aborted {
            group_id: intent.request.group_id,
            idempotency_key: IdempotencyKey::new(Uuid::from_u128(11)).unwrap(),
        };
        assert!(validate_status_identity::<()>(&wrong_key, &intent, key).is_err());
    }

    #[test]
    fn source_binding_rejects_every_live_identity_mismatch() {
        let intent = fixture_intent();
        let source = fixture_source();
        let catalog_digest = intent.catalog_digest;
        assert!(source_binding_matches(
            &intent,
            &source,
            intent.source_world_id,
            catalog_digest
        ));

        let mut wrong = intent.clone();
        wrong.request.source_snapshot_id = SnapshotId::new(Uuid::from_u128(70)).unwrap();
        assert!(!source_binding_matches(
            &wrong,
            &source,
            intent.source_world_id,
            catalog_digest
        ));

        let mut wrong = intent.clone();
        wrong.request.fence = LeaseFence::new(
            SessionId::new(Uuid::from_u128(3)).unwrap(),
            wrong.request.fence.character_id,
            Revision::new(40),
            SessionEpoch::new(5).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(6)).unwrap(),
        );
        assert!(!source_binding_matches(
            &wrong,
            &source,
            intent.source_world_id,
            catalog_digest
        ));

        let mut wrong = intent.clone();
        wrong.request.portal_id = "other_portal".to_owned();
        assert!(!source_binding_matches(
            &wrong,
            &source,
            intent.source_world_id,
            catalog_digest
        ));

        let mut wrong = intent.clone();
        wrong.source_save_sha256 = Sha256Digest::from_bytes([11; 32]);
        assert!(!source_binding_matches(
            &wrong,
            &source,
            intent.source_world_id,
            catalog_digest
        ));

        assert!(!source_binding_matches(
            &intent,
            &source,
            RomWorldId::new(99).unwrap(),
            catalog_digest
        ));
        assert!(!source_binding_matches(
            &intent,
            &source,
            intent.source_world_id,
            Sha256Digest::from_bytes([12; 32])
        ));
    }

    #[test]
    fn arrival_evidence_rejects_every_unexpected_authenticated_field() {
        let expected_full = Sha256Digest::from_bytes([1; 32]);
        let expected_flash = Sha256Digest::from_bytes([2; 32]);
        let expected_world = RomWorldId::new(2).unwrap();
        let expected_nonce = [5; 16];
        let expected = AuthenticatedArrivalEvidence {
            full_sav_sha256: expected_full,
            flash_sha256: expected_flash,
            destination_world: expected_world,
            save_generation: 7,
            map_group: 3,
            map_num: 4,
            nonce: expected_nonce,
        };
        let matches = |evidence: &AuthenticatedArrivalEvidence| {
            arrival_evidence_matches(
                evidence,
                expected_full,
                Some(expected_flash),
                expected_world,
                7,
                3,
                4,
                expected_nonce,
            )
        };
        assert!(matches(&expected));

        let mut wrong = expected.clone();
        wrong.full_sav_sha256 = Sha256Digest::from_bytes([8; 32]);
        assert!(!matches(&wrong));
        let mut wrong = expected.clone();
        wrong.flash_sha256 = Sha256Digest::from_bytes([8; 32]);
        assert!(!matches(&wrong));
        let mut wrong = expected.clone();
        wrong.destination_world = RomWorldId::new(3).unwrap();
        assert!(!matches(&wrong));
        let mut wrong = expected.clone();
        wrong.save_generation = 8;
        assert!(!matches(&wrong));
        let mut wrong = expected.clone();
        wrong.map_group = 8;
        assert!(!matches(&wrong));
        let mut wrong = expected.clone();
        wrong.map_num = 8;
        assert!(!matches(&wrong));
        let mut wrong = expected;
        wrong.nonce = [8; 16];
        assert!(!matches(&wrong));
    }

    #[tokio::test]
    async fn restart_replays_persisted_arrival_status_after_lost_response() {
        let intent = fixture_intent();
        let attempt_key = IdempotencyKey::new(Uuid::from_u128(20)).unwrap();
        let stage_id = SnapshotId::new(Uuid::from_u128(21)).unwrap();
        let destination_world = RomWorldId::new(2).unwrap();
        let stage = PairedStage {
            stage_id,
            destination_world_id: destination_world,
            arrival_portal_id: "to_hoenn".to_owned(),
            destination_save_sha256: Sha256Digest::from_bytes([22; 32]),
            arrival_challenge: Sha256Digest::from_bytes([23; 32]),
            expected_nonce: [24; 16],
        };
        let record = PairedTravelRecord {
            format_version: 1,
            sequence: 2,
            character_id: intent.request.fence.character_id,
            phase: PairedPhase::ArrivalVerified,
            intent: intent.clone(),
            attempt_key: Some(attempt_key),
            stage: Some(stage.clone()),
            verified_arrival: None,
            terminal: None,
        };
        let status = GroupRomHandoffStatus::Staged {
            group_id: intent.request.group_id,
            idempotency_key: attempt_key,
            stage_id,
            destination_world_id: destination_world,
            arrival_portal_id: stage.arrival_portal_id.clone(),
            destination_save_sha256: stage.destination_save_sha256,
            destination_save: Vec::new(),
            arrival_challenge: stage.arrival_challenge,
            acknowledged_by: [true, false],
        };
        let root = tempfile::tempdir().unwrap();
        let journal = PairedTravelJournal::new(root.path(), record.character_id);
        let outcome = recover_status(&journal, &record, status).await.unwrap();
        assert_eq!(
            outcome,
            PairedHandoffOutcome::Staged {
                attempt_key,
                stage_id,
            }
        );
    }
}

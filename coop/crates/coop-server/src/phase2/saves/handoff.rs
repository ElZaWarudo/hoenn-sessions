//! Durable staging for cross-ROM travel. Preparing never changes the active
//! world; a separate acknowledged commit performs that transition.

use coop_cloud::{
    ApiVersion, ArtifactIdentity, RomHandoffCommitRequest, RomHandoffPrepareRequest,
    RomHandoffPrepareResponse, RomHandoffRecoveryRequest, RomHandoffRecoveryStatus, Sha256Digest,
    SnapshotFence, SnapshotFile, SnapshotId, SnapshotRecord,
};
use coop_save::{TransferDescriptorPair, project_arrival};

use super::super::storage::{
    MAX_RETIRED_SNAPSHOTS, MAX_ROM_HANDOFF_ABORT_TOMBSTONES_PER_CHARACTER,
    MAX_SNAPSHOT_STORAGE_BYTES, MAX_SNAPSHOTS_PER_CHARACTER, ROM_HANDOFF_STAGE_TTL_MS,
    RomHandoffAbortTombstone, RomHandoffStage, Store,
};
use super::super::{AuthenticatedActor, Phase2Error};
use super::{
    active_lease_identity_for_handoff, seal_object, snapshot_save, validate_character_sav,
    validate_runtime_binding, verified_source_objects,
};

const EMPTY_PENDING: &[u8] = b"[]";

fn prune_abort_tombstones(
    state: &mut super::super::storage::State,
    character_id: coop_cloud::CharacterId,
) {
    let Some(character) = state.characters.get(&character_id) else {
        state
            .rom_handoff_aborts
            .retain(|(id, _), _| id != &character_id);
        return;
    };
    let revision = character.revision;
    let active_snapshot = character.active_snapshot;
    state.rom_handoff_aborts.retain(|(id, _), tombstone| {
        id != &character_id
            || (tombstone.request.expected_revision == revision
                && active_snapshot == Some(tombstone.request.source_snapshot_id))
    });
}

fn has_abort_tombstone_for_stage(
    state: &super::super::storage::State,
    character_id: coop_cloud::CharacterId,
    stage_id: SnapshotId,
) -> bool {
    state
        .rom_handoff_aborts
        .iter()
        .any(|((id, _), tombstone)| *id == character_id && tombstone.stage_id == stage_id)
}

/// Complete object retirement for snapshots already removed from history by
/// an authoritative handoff. The durable queue survives crashes and retries.
pub(super) fn drain_retirements(
    store: &Store,
    actor: AuthenticatedActor,
) -> Result<(), Phase2Error> {
    let pending = store.read_transaction(|state| {
        if !super::owner(state, actor, actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        Ok::<Vec<SnapshotRecord>, Phase2Error>(
            state
                .retiring_snapshots
                .values()
                .filter(|record| record.character_id == actor.character_id)
                .cloned()
                .collect(),
        )
    })?;
    for record in pending {
        for file in &record.files {
            let key = Store::object_key(actor.character_id, record.snapshot_id, file.artifact);
            seal_object(store, &key)?;
        }
        store.write_transaction(|state| {
            if state.snapshots.contains_key(&record.snapshot_id) {
                return Err(Phase2Error::Conflict);
            }
            match state.retiring_snapshots.get(&record.snapshot_id) {
                None => return Ok(()),
                Some(current) if current != &record => return Err(Phase2Error::Conflict),
                Some(_) => {}
            }
            state.retiring_snapshots.remove(&record.snapshot_id);
            if state.retired_snapshots.len() < MAX_RETIRED_SNAPSHOTS {
                state.retired_snapshots.insert(record.snapshot_id);
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Select only superseded history. The first snapshot anchors character
/// lineage; the active head and every world-local head must remain restorable.
/// Work out the complete quota plan before mutating the in-memory repository,
/// whose transaction adapter does not roll back a closure returning an error.
fn snapshot_retirement_plan(
    state: &super::super::storage::State,
    character_id: coop_cloud::CharacterId,
    files: &[SnapshotFile],
    excluded_prepared: Option<SnapshotId>,
    excluded_restore: Option<SnapshotId>,
    protected_source: Option<SnapshotId>,
) -> Result<Vec<SnapshotRecord>, Phase2Error> {
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::NotFound)?;
    let protected = character
        .world_heads
        .values()
        .copied()
        .chain(character.active_snapshot)
        .chain(protected_source)
        .collect::<std::collections::HashSet<_>>();
    let mut candidates = state
        .snapshots
        .values()
        .filter(|snapshot| {
            snapshot.character_id == character_id
                && snapshot.revision != coop_cloud::Revision::new(1)
                && !protected.contains(&snapshot.snapshot_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|snapshot| snapshot.revision.value());
    let mut count = state
        .snapshots
        .values()
        .filter(|snapshot| snapshot.character_id == character_id)
        .count();
    let snapshots_used = super::snapshot_storage_usage(state, character_id)?;
    let prepared_used = super::prepared_storage_usage(state, character_id, excluded_prepared)?;
    let restore_used = super::restore_staging_storage_usage(state, character_id, excluded_restore)?;
    let mut used = snapshots_used
        .checked_add(prepared_used)
        .and_then(|bytes| bytes.checked_add(restore_used))
        .ok_or(Phase2Error::Internal)?;
    let requested = super::files_storage_usage(files)?;
    let mut victims = Vec::new();
    for candidate in candidates {
        if count < MAX_SNAPSHOTS_PER_CHARACTER
            && used
                .checked_add(requested)
                .is_some_and(|total| total <= MAX_SNAPSHOT_STORAGE_BYTES)
        {
            break;
        }
        count -= 1;
        used = used
            .checked_sub(super::files_storage_usage(&candidate.files)?)
            .ok_or(Phase2Error::Internal)?;
        victims.push(candidate);
    }
    if count >= MAX_SNAPSHOTS_PER_CHARACTER
        || used
            .checked_add(requested)
            .is_none_or(|total| total > MAX_SNAPSHOT_STORAGE_BYTES)
    {
        return Err(Phase2Error::Busy);
    }
    Ok(victims)
}

pub(super) fn can_make_snapshot_room(
    state: &super::super::storage::State,
    character_id: coop_cloud::CharacterId,
    files: &[SnapshotFile],
    excluded_prepared: Option<SnapshotId>,
    excluded_restore: Option<SnapshotId>,
    protected_source: Option<SnapshotId>,
) -> Result<(), Phase2Error> {
    snapshot_retirement_plan(
        state,
        character_id,
        files,
        excluded_prepared,
        excluded_restore,
        protected_source,
    )?;
    Ok(())
}

pub(super) fn make_snapshot_room(
    state: &mut super::super::storage::State,
    character_id: coop_cloud::CharacterId,
    files: &[SnapshotFile],
    excluded_prepared: Option<SnapshotId>,
    excluded_restore: Option<SnapshotId>,
    protected_source: Option<SnapshotId>,
) -> Result<(), Phase2Error> {
    let victims = snapshot_retirement_plan(
        state,
        character_id,
        files,
        excluded_prepared,
        excluded_restore,
        protected_source,
    )?;
    for victim in victims {
        let id = victim.snapshot_id;
        state.snapshots.remove(&id);
        state
            .snapshot_by_revision
            .remove(&(character_id, victim.revision));
        state
            .prepare_ops
            .retain(|_, snapshot_id| *snapshot_id != id);
        state
            .finalize_ops
            .retain(|_, (_, record)| record.snapshot_id != id);
        state
            .restore_ops
            .retain(|_, (request, record)| request.snapshot_id != id && record.snapshot_id != id);
        state
            .rom_handoff_commits
            .retain(|_, (_, record)| record.snapshot_id != id);
        state.retiring_snapshots.insert(id, victim);
    }
    Ok(())
}

/// A stage left behind by expiry, reconnect, or lease replacement cannot be
/// committed. Retire it before accepting a fresh request from the owner.
pub(crate) fn cleanup_abandoned(
    store: &Store,
    actor: AuthenticatedActor,
    request: &RomHandoffPrepareRequest,
) -> Result<(), Phase2Error> {
    if request.api_version != ApiVersion::V1
        || !request.valid_portal_id()
        || request.character_id != actor.character_id
    {
        return Err(Phase2Error::InvalidRequest);
    }
    store.write_transaction(|state| {
        if !super::owner(state, actor, actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        prune_abort_tombstones(state, actor.character_id);
        Ok::<(), Phase2Error>(())
    })?;
    let abandoned = store.read_transaction(|state| {
        if !super::owner(state, actor, actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let Some(stage) = state.rom_handoff_staging.get(&actor.character_id) else {
            return Ok(None);
        };
        let lease = state.leases.get(&actor.character_id);
        let stale = stage.expires_at <= store.now()
            || lease.is_none_or(|lease| {
                lease.released
                    || lease.contract.expires_at.value() <= store.now()
                    || lease.contract.session_id != stage.request.session_id
                    || lease.contract.session_epoch != stage.request.session_epoch
                    || lease.contract.client_instance_id != stage.request.client_instance_id
            });
        Ok(stale.then_some(stage.stage_id))
    })?;
    if let Some(stage_id) = abandoned {
        abort(store, actor, stage_id)?;
    }
    super::cleanup_expired_staging_for_handoff(store, actor, request)?;
    drain_retirements(store, actor)?;
    Ok(())
}

fn source_state(
    store: &Store,
    actor: AuthenticatedActor,
    request: &RomHandoffPrepareRequest,
    now: u64,
) -> Result<
    (
        SnapshotRecord,
        Option<SnapshotRecord>,
        coop_protocol::RomWorldId,
        String,
        Option<RomHandoffStage>,
    ),
    Phase2Error,
> {
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let descriptor = catalog
        .transfer_descriptor()
        .ok_or(Phase2Error::Forbidden)?;
    store.read_transaction(|state| {
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        if lease.contract.current_revision != request.expected_revision
            || state
                .active_group_by_member
                .contains_key(&request.character_id)
            || state
                .live_group_travel_by_member
                .contains_key(&request.character_id)
            || state.restore_staging.contains_key(&request.character_id)
            || state
                .prepared
                .values()
                .any(|prepared| prepared.request.character_id == request.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if state
            .rom_handoff_aborts
            .contains_key(&(request.character_id, request.idempotency_key))
        {
            return Err(Phase2Error::Conflict);
        }
        if character.revision != request.expected_revision
            || character.active_snapshot != Some(request.source_snapshot_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&request.source_snapshot_id)
            .filter(|snapshot| snapshot.character_id == request.character_id)
            .ok_or(Phase2Error::Conflict)?;
        validate_runtime_binding(catalog, &lease, source.rom_world_id)?;
        let (destination_world, arrival_id) = catalog
            .resolve_portal(
                source.rom_world_id,
                &request.portal_id,
                Sha256Digest::of_bytes(descriptor),
            )
            .ok_or(Phase2Error::Forbidden)?;
        let dormant = character
            .world_heads
            .get(&destination_world)
            .map(|id| {
                state
                    .snapshots
                    .get(id)
                    .filter(|snapshot| {
                        snapshot.character_id == request.character_id
                            && snapshot.rom_world_id == destination_world
                    })
                    .cloned()
                    .ok_or(Phase2Error::Conflict)
            })
            .transpose()?;
        let stage = state
            .rom_handoff_staging
            .get(&request.character_id)
            .cloned();
        if stage.as_ref().is_some_and(|stage| {
            stage.request != *request
                || stage.expires_at <= now
                || stage.source_world_id != source.rom_world_id
                || stage.destination_world_id != destination_world
                || stage.arrival_portal_id != arrival_id
        }) {
            return Err(Phase2Error::Conflict);
        }
        Ok((
            source.clone(),
            dormant,
            destination_world,
            arrival_id.to_owned(),
            stage,
        ))
    })
}

fn put_exact(
    store: &Store,
    character_id: coop_cloud::CharacterId,
    stage_id: SnapshotId,
    artifact: ArtifactIdentity,
    bytes: &[u8],
) -> Result<(), Phase2Error> {
    let key = Store::object_key(character_id, stage_id, artifact);
    if !store.objects.put_if_absent(key.clone(), bytes.to_vec())? {
        let existing = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
        if existing != bytes {
            return Err(Phase2Error::Conflict);
        }
    }
    Ok(())
}

/// Stage exactly one destination save for an authenticated finalized source.
/// Replays use the same stage ID and immutable object keys. An interrupted
/// upload can be completed by retrying the same request; the source head stays
/// authoritative until a separate commit acknowledges the imported save.
pub(crate) fn prepare(
    store: &Store,
    actor: AuthenticatedActor,
    request: &RomHandoffPrepareRequest,
) -> Result<RomHandoffPrepareResponse, Phase2Error> {
    if request.api_version != ApiVersion::V1
        || !request.valid_portal_id()
        || request.character_id != actor.character_id
    {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let descriptor = catalog
        .transfer_descriptor()
        .ok_or(Phase2Error::Forbidden)?;
    let (source, dormant, destination_world, arrival_id, existing) =
        source_state(store, actor, request, now)?;
    let (objects, source_save) = verified_source_objects(store, request.character_id, &source)?;
    if !objects.iter().any(|(artifact, bytes)| {
        *artifact == ArtifactIdentity::PendingCommits && bytes == EMPTY_PENDING
    }) {
        return Err(Phase2Error::Conflict);
    }
    let first_arrival = dormant.is_none();
    let destination_save = if let Some(dormant) = dormant {
        snapshot_save(store, request.character_id, &dormant)?
    } else {
        let bytes = catalog
            .arrival_save(destination_world, &arrival_id)
            .ok_or(Phase2Error::Forbidden)?;
        coop_save::parse_v2(bytes, super::identity_registry_contract())
            .map_err(|_| Phase2Error::Conflict)?
    };
    let projected = project_arrival(
        &source_save,
        &destination_save,
        TransferDescriptorPair {
            source: descriptor,
            destination: descriptor,
        },
        first_arrival,
    )
    .map_err(|_| Phase2Error::Conflict)?;
    let projected_bytes = projected.raw_bytes();
    let digest = Sha256Digest::of_bytes(projected_bytes);
    let stage_id = existing
        .as_ref()
        .map(|stage| stage.stage_id)
        .unwrap_or(store.snapshot_id()?);
    let expiry = now
        .checked_add(ROM_HANDOFF_STAGE_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let stage = store.write_transaction(|state| {
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            store.now(),
        )?;
        if lease.contract.current_revision != request.expected_revision
            || state
                .active_group_by_member
                .contains_key(&request.character_id)
            || state
                .live_group_travel_by_member
                .contains_key(&request.character_id)
            || state.restore_staging.contains_key(&request.character_id)
            || state
                .prepared
                .values()
                .any(|prepared| prepared.request.character_id == request.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        validate_runtime_binding(catalog, &lease, source.rom_world_id)?;
        let (character_revision, character_active_snapshot) = {
            let character = state
                .characters
                .get(&request.character_id)
                .ok_or(Phase2Error::NotFound)?;
            (character.revision, character.active_snapshot)
        };
        prune_abort_tombstones(state, request.character_id);
        if state
            .rom_handoff_aborts
            .contains_key(&(request.character_id, request.idempotency_key))
        {
            return Err(Phase2Error::Conflict);
        }
        let current = state
            .rom_handoff_staging
            .get(&request.character_id)
            .cloned();
        if current.is_none()
            && state
                .rom_handoff_aborts
                .keys()
                .filter(|(character_id, _)| character_id == &request.character_id)
                .count()
                >= MAX_ROM_HANDOFF_ABORT_TOMBSTONES_PER_CHARACTER
        {
            return Err(Phase2Error::Busy);
        }
        if character_revision != request.expected_revision
            || character_active_snapshot != Some(request.source_snapshot_id)
            || state.snapshots.contains_key(&stage_id)
            || state.retiring_snapshots.contains_key(&stage_id)
            || state.retired_snapshots.contains(&stage_id)
        {
            return Err(Phase2Error::Conflict);
        }
        if let Some(current) = current {
            if current.request != *request
                || current.stage_id != stage_id
                || current.destination_save_sha256 != digest
                || current.expires_at <= store.now()
            {
                return Err(Phase2Error::Conflict);
            }
            return Ok(current);
        }
        let stage = RomHandoffStage {
            request: request.clone(),
            stage_id,
            source_world_id: source.rom_world_id,
            destination_world_id: destination_world,
            arrival_portal_id: arrival_id.clone(),
            destination_save_sha256: digest,
            expires_at: expiry,
        };
        state
            .rom_handoff_staging
            .insert(request.character_id, stage.clone());
        Ok(stage)
    })?;
    put_exact(
        store,
        request.character_id,
        stage.stage_id,
        ArtifactIdentity::CharacterSav,
        projected_bytes,
    )?;
    put_exact(
        store,
        request.character_id,
        stage.stage_id,
        ArtifactIdentity::PendingCommits,
        EMPTY_PENDING,
    )?;
    Ok(RomHandoffPrepareResponse {
        api_version: ApiVersion::V1,
        stage_id: stage.stage_id,
        destination_world_id: stage.destination_world_id,
        arrival_portal_id: stage.arrival_portal_id,
        destination_save_sha256: digest,
        destination_save: projected_bytes.to_vec(),
    })
}

/// Promote only the exact staged image after the destination ROM reports its
/// loaded save digest. The repository transaction changes the active head,
/// dormant-world index, snapshot revision, and source lease in one write.
pub(crate) fn commit(
    store: &Store,
    actor: AuthenticatedActor,
    request: &RomHandoffCommitRequest,
) -> Result<(SnapshotRecord, bool), Phase2Error> {
    if request.api_version != ApiVersion::V1 || request.character_id != actor.character_id {
        return Err(Phase2Error::InvalidRequest);
    }
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let now = store.now();
    let (stage, source) = store.read_transaction(|state| {
        if !super::owner(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        if let Some((known, record)) = state
            .rom_handoff_commits
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok((None, Some(record.clone())))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let stage = state
            .rom_handoff_staging
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if stage.stage_id != request.stage_id
            || stage.destination_save_sha256 != request.destination_save_sha256
            || stage.request.idempotency_key != request.idempotency_key
            || stage.request.expected_revision != request.expected_revision
            || stage.request.session_id != request.session_id
            || stage.request.session_epoch != request.session_epoch
            || stage.request.client_instance_id != request.client_instance_id
            || stage.expires_at <= now
        {
            return Err(Phase2Error::Conflict);
        }
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        validate_runtime_binding(catalog, &lease, stage.source_world_id)?;
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != request.expected_revision
            || character.active_snapshot != Some(stage.request.source_snapshot_id)
            || lease.contract.current_revision != request.expected_revision
            || state
                .active_group_by_member
                .contains_key(&request.character_id)
            || state
                .live_group_travel_by_member
                .contains_key(&request.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&stage.request.source_snapshot_id)
            .filter(|snapshot| {
                snapshot.character_id == request.character_id
                    && snapshot.rom_world_id == stage.source_world_id
            })
            .cloned()
            .ok_or(Phase2Error::Conflict)?;
        Ok((Some((stage.clone(), source)), None))
    })?;
    if let Some(record) = source {
        return Ok((record, false));
    }
    let (stage, source) = stage.ok_or(Phase2Error::Internal)?;
    let save_key = Store::object_key(
        request.character_id,
        stage.stage_id,
        ArtifactIdentity::CharacterSav,
    );
    let pending_key = Store::object_key(
        request.character_id,
        stage.stage_id,
        ArtifactIdentity::PendingCommits,
    );
    let save_bytes = store.objects.get(&save_key)?.ok_or(Phase2Error::Conflict)?;
    let pending_bytes = store
        .objects
        .get(&pending_key)?
        .ok_or(Phase2Error::Conflict)?;
    if Sha256Digest::of_bytes(&save_bytes) != stage.destination_save_sha256
        || pending_bytes != EMPTY_PENDING
    {
        return Err(Phase2Error::Conflict);
    }
    let revision = request
        .expected_revision
        .next()
        .map_err(|_| Phase2Error::Conflict)?;
    let projected =
        validate_character_sav(&save_bytes, revision).map_err(|_| Phase2Error::Conflict)?;
    let source_save = snapshot_save(store, request.character_id, &source)?;
    if projected.character_lineage() != source_save.character_lineage()
        || projected.coop().save_generation
            != source_save
                .coop()
                .save_generation
                .checked_add(1)
                .ok_or(Phase2Error::Conflict)?
    {
        return Err(Phase2Error::Conflict);
    }
    let files = vec![
        SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &save_bytes)
            .map_err(|_| Phase2Error::Internal)?,
        SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, EMPTY_PENDING)
            .map_err(|_| Phase2Error::Internal)?,
    ];
    let record = SnapshotRecord::new(
        stage.stage_id,
        stage.destination_world_id,
        SnapshotFence::new(
            request.session_id,
            request.character_id,
            request.session_epoch,
        ),
        request.expected_revision,
        revision,
        files.clone(),
        Sha256Digest::of_bytes(EMPTY_PENDING),
        source.last_applied_commit,
        Store::unix_timestamp(now)?,
    )
    .map_err(|_| Phase2Error::Internal)?;
    store.write_transaction(|state| {
        if !super::owner(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        if let Some((known, existing)) = state
            .rom_handoff_commits
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok((existing.clone(), false))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let current = state
            .rom_handoff_staging
            .get(&request.character_id)
            .ok_or(Phase2Error::Conflict)?;
        if current.request != stage.request
            || current.stage_id != stage.stage_id
            || current.destination_save_sha256 != stage.destination_save_sha256
            || current.expires_at <= store.now()
        {
            return Err(Phase2Error::Conflict);
        }
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            store.now(),
        )?;
        validate_runtime_binding(catalog, &lease, stage.source_world_id)?;
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != request.expected_revision
            || character.active_snapshot != Some(stage.request.source_snapshot_id)
            || lease.contract.current_revision != request.expected_revision
            || state
                .active_group_by_member
                .contains_key(&request.character_id)
            || state
                .live_group_travel_by_member
                .contains_key(&request.character_id)
            || state.snapshots.contains_key(&stage.stage_id)
            || state
                .snapshot_by_revision
                .contains_key(&(request.character_id, revision))
        {
            return Err(Phase2Error::Conflict);
        }
        make_snapshot_room(state, request.character_id, &files, None, None, None)?;
        state.snapshots.insert(stage.stage_id, record.clone());
        state
            .snapshot_by_revision
            .insert((request.character_id, revision), stage.stage_id);
        if let Some(character) = state.characters.get_mut(&request.character_id) {
            character.revision = revision;
            character.active_snapshot = Some(stage.stage_id);
            character
                .world_heads
                .insert(stage.destination_world_id, stage.stage_id);
        }
        if let Some(lease) = state.leases.get_mut(&request.character_id) {
            lease.released = true;
            lease.runtime_binding = None;
        }
        state
            .realtime_tickets
            .retain(|_, ticket| ticket.character_id != request.character_id);
        let live_tickets = state
            .realtime_tickets
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        state
            .realtime_by_runtime
            .retain(|_, fingerprint| live_tickets.contains(fingerprint));
        state.rom_handoff_staging.remove(&request.character_id);
        // A successful commit advances the source head.  Every abort fence
        // tied to that old head is now safely retired; stale prepares are
        // already rejected by the revision and active-snapshot checks.
        state
            .rom_handoff_aborts
            .retain(|(character_id, _), _| character_id != &request.character_id);
        state.rom_handoff_commits.insert(
            (request.character_id, request.idempotency_key),
            (request.clone(), record.clone()),
        );
        Ok((record.clone(), true))
    })
}

/// Abandon an uncommitted stage and keep the source world authoritative.
/// Deleting and retiring its immutable objects precedes removal of the
/// persistent stage, so a crash can safely retry the same abort.
pub(crate) fn abort(
    store: &Store,
    actor: AuthenticatedActor,
    stage_id: SnapshotId,
) -> Result<(), Phase2Error> {
    let stage = store.read_transaction(|state| {
        if !super::owner(state, actor, actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let stage = state
            .rom_handoff_staging
            .get(&actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if stage.stage_id != stage_id {
            return Err(Phase2Error::Conflict);
        }
        Ok(stage.clone())
    })?;
    for artifact in [
        ArtifactIdentity::CharacterSav,
        ArtifactIdentity::PendingCommits,
    ] {
        let key = Store::object_key(actor.character_id, stage_id, artifact);
        seal_object(store, &key)?;
    }
    store.write_transaction(|state| {
        if !super::owner(state, actor, actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        if state
            .rom_handoff_staging
            .get(&actor.character_id)
            .is_none_or(|current| current.request != stage.request || current.stage_id != stage_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let tombstone_count = state
            .rom_handoff_aborts
            .keys()
            .filter(|(character_id, _)| character_id == &actor.character_id)
            .count();
        if tombstone_count >= MAX_ROM_HANDOFF_ABORT_TOMBSTONES_PER_CHARACTER
            && !state
                .rom_handoff_aborts
                .contains_key(&(actor.character_id, stage.request.idempotency_key))
        {
            // Keep the stage authoritative when the bounded replay fence is
            // full.  The caller can retry after the source head advances.
            return Err(Phase2Error::Busy);
        }
        state.rom_handoff_staging.remove(&actor.character_id);
        state.rom_handoff_aborts.insert(
            (actor.character_id, stage.request.idempotency_key),
            RomHandoffAbortTombstone {
                request: stage.request.clone(),
                stage_id,
                source_world_id: stage.source_world_id,
            },
        );
        if state.retired_snapshots.len() < MAX_RETIRED_SNAPSHOTS {
            state.retired_snapshots.insert(stage_id);
        }
        Ok(())
    })
}

/// Public aborts require the current authenticated source lease at the exact
/// source head that created the stage. The lease session may have replaced
/// the original one: the source revision, active snapshot, and source ROM
/// still fence the operation. An already-recorded tombstone is idempotent.
pub(crate) fn abort_fenced(
    store: &Store,
    actor: AuthenticatedActor,
    fence: coop_cloud::LeaseFence,
    stage_id: SnapshotId,
) -> Result<(), Phase2Error> {
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    store.read_transaction(|state| {
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            fence.character_id,
            fence.session_id,
            fence.session_epoch,
            fence.client_instance_id,
            store.now(),
        )?;
        if lease.contract.current_revision != fence.current_revision {
            return Err(Phase2Error::Conflict);
        }
        let character = state
            .characters
            .get(&fence.character_id)
            .ok_or(Phase2Error::NotFound)?;
        let (request, source_world_id) = if let Some(stage) =
            state.rom_handoff_staging.get(&fence.character_id)
        {
            if stage.stage_id != stage_id {
                return Err(Phase2Error::Conflict);
            }
            (stage.request.clone(), stage.source_world_id)
        } else if let Some(tombstone) = state.rom_handoff_aborts.values().find(|tombstone| {
            tombstone.request.character_id == fence.character_id && tombstone.stage_id == stage_id
        }) {
            (tombstone.request.clone(), tombstone.source_world_id)
        } else {
            return Err(Phase2Error::NotFound);
        };
        if request.expected_revision != fence.current_revision
            || character.revision != fence.current_revision
            || character.active_snapshot != Some(request.source_snapshot_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&request.source_snapshot_id)
            .filter(|snapshot| {
                snapshot.character_id == fence.character_id
                    && snapshot.rom_world_id == source_world_id
            })
            .ok_or(Phase2Error::Conflict)?;
        if source.rom_world_id != source_world_id {
            return Err(Phase2Error::Conflict);
        }
        validate_runtime_binding(catalog, &lease, source_world_id)?;
        Ok(())
    })?;
    // The stage may have disappeared between the read and this call only
    // after a successful commit/abort under the transition gate. A matching
    // tombstone is the successful idempotent result; otherwise remove and
    // tombstone the live stage with the same atomic cleanup path.
    let already_aborted = store.read_transaction(|state| {
        Ok::<bool, Phase2Error>(has_abort_tombstone_for_stage(
            state,
            actor.character_id,
            stage_id,
        ))
    })?;
    if already_aborted {
        Ok(())
    } else {
        abort(store, actor, stage_id)
    }
}

/// Return authenticated recovery state for one exact prepare key.  The
/// caller supplies the current lease fence, so a replaced lease can recover a
/// tombstone while a foreign character, changed source head, or wrong source
/// world cannot inspect it.  Because an expired stage is reconciled as part of
/// this operation, the HTTP/API layer should expose it as an explicit POST
/// reconcile endpoint rather than a cacheable GET status endpoint.
pub(crate) fn recovery_status(
    store: &Store,
    actor: AuthenticatedActor,
    fence: coop_cloud::LeaseFence,
    request: &RomHandoffRecoveryRequest,
) -> Result<RomHandoffRecoveryStatus, Phase2Error> {
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let idempotency_key = request.idempotency_key;
    // Reconciliation is the recovery trigger for a stage whose five-minute
    // lease expired before the client received the prepare response.  This
    // preserves the same object cleanup and tombstone transaction as the next
    // prepare call, while the current lease fence still authenticates the
    // caller before any mutation. A mere lease rollover leaves a live stage
    // observable; it is not an implicit abort.
    let expired_stage =
        store.read_transaction(|state| -> Result<Option<SnapshotId>, Phase2Error> {
            let lease = active_lease_identity_for_handoff(
                state,
                actor,
                fence.character_id,
                fence.session_id,
                fence.session_epoch,
                fence.client_instance_id,
                store.now(),
            )?;
            let Some(stage) = state
                .rom_handoff_staging
                .get(&fence.character_id)
                .filter(|stage| stage.request.idempotency_key == idempotency_key)
            else {
                return Ok(None);
            };
            if stage.request.source_snapshot_id != request.source_snapshot_id
                || stage.request.expected_revision != request.expected_revision
                || stage.source_world_id != request.source_world_id
                || stage.request.portal_id != request.portal_id
            {
                return Err(Phase2Error::Conflict);
            }
            let character = state
                .characters
                .get(&fence.character_id)
                .ok_or(Phase2Error::NotFound)?;
            if stage.request.expected_revision != fence.current_revision
                || lease.contract.current_revision != fence.current_revision
                || character.revision != fence.current_revision
                || character.active_snapshot != Some(stage.request.source_snapshot_id)
            {
                return Err(Phase2Error::Conflict);
            }
            let source = state
                .snapshots
                .get(&stage.request.source_snapshot_id)
                .filter(|snapshot| {
                    snapshot.character_id == fence.character_id
                        && snapshot.rom_world_id == stage.source_world_id
                })
                .ok_or(Phase2Error::Conflict)?;
            validate_runtime_binding(catalog, &lease, source.rom_world_id)?;
            Ok((stage.expires_at <= store.now()).then_some(stage.stage_id))
        })?;
    if let Some(stage_id) = expired_stage {
        abort(store, actor, stage_id)?;
    }
    // A prepare may have been journaled locally but never reached the server.
    // Reserve its exact key in the same write transaction that verifies the
    // current source head. A delayed prepare then sees the tombstone and can
    // never create a stage, even if its HTTP call races reconciliation.
    store.write_transaction(|state| {
        let lease = active_lease_identity_for_handoff(
            state,
            actor,
            fence.character_id,
            fence.session_id,
            fence.session_epoch,
            fence.client_instance_id,
            store.now(),
        )?;
        let (request, stage_id, source_world_id, aborted) = if let Some(stage) = state
            .rom_handoff_staging
            .get(&fence.character_id)
            .filter(|stage| stage.request.idempotency_key == idempotency_key)
        {
            if stage.request.source_snapshot_id != request.source_snapshot_id
                || stage.request.expected_revision != request.expected_revision
                || stage.source_world_id != request.source_world_id
                || stage.request.portal_id != request.portal_id
            {
                return Err(Phase2Error::Conflict);
            }
            (
                stage.request.clone(),
                stage.stage_id,
                stage.source_world_id,
                false,
            )
        } else if let Some(tombstone) = state
            .rom_handoff_aborts
            .get(&(fence.character_id, idempotency_key))
        {
            if tombstone.request.source_snapshot_id != request.source_snapshot_id
                || tombstone.request.expected_revision != request.expected_revision
                || tombstone.source_world_id != request.source_world_id
                || tombstone.request.portal_id != request.portal_id
            {
                return Err(Phase2Error::Conflict);
            }
            (
                tombstone.request.clone(),
                tombstone.stage_id,
                tombstone.source_world_id,
                true,
            )
        } else {
            let character = state
                .characters
                .get(&fence.character_id)
                .ok_or(Phase2Error::NotFound)?;
            if lease.contract.current_revision != fence.current_revision
                || request.expected_revision != fence.current_revision
                || character.revision != fence.current_revision
                || character.active_snapshot != Some(request.source_snapshot_id)
            {
                return Err(Phase2Error::Conflict);
            }
            let source = state
                .snapshots
                .get(&request.source_snapshot_id)
                .filter(|snapshot| {
                    snapshot.character_id == fence.character_id
                        && snapshot.rom_world_id == request.source_world_id
                })
                .ok_or(Phase2Error::Conflict)?;
            validate_runtime_binding(catalog, &lease, source.rom_world_id)?;
            let synthetic_request = RomHandoffPrepareRequest {
                api_version: ApiVersion::V1,
                character_id: fence.character_id,
                session_id: fence.session_id,
                session_epoch: fence.session_epoch,
                client_instance_id: fence.client_instance_id,
                expected_revision: request.expected_revision,
                source_snapshot_id: request.source_snapshot_id,
                portal_id: request.portal_id.clone(),
                idempotency_key,
            };
            if !synthetic_request.valid_portal_id() {
                return Err(Phase2Error::InvalidRequest);
            }
            let descriptor = catalog
                .transfer_descriptor()
                .ok_or(Phase2Error::Forbidden)?;
            if catalog
                .resolve_portal(
                    source.rom_world_id,
                    &request.portal_id,
                    Sha256Digest::of_bytes(descriptor),
                )
                .is_none()
            {
                return Err(Phase2Error::Forbidden);
            }
            let tombstone_count = state
                .rom_handoff_aborts
                .keys()
                .filter(|(character_id, _)| character_id == &fence.character_id)
                .count();
            let live_stage_slot =
                usize::from(state.rom_handoff_staging.contains_key(&fence.character_id));
            if tombstone_count + live_stage_slot >= MAX_ROM_HANDOFF_ABORT_TOMBSTONES_PER_CHARACTER {
                return Err(Phase2Error::Busy);
            }
            let absent_stage_id = store.snapshot_id()?;
            if absent_stage_id == request.source_snapshot_id
                || state.snapshots.contains_key(&absent_stage_id)
                || state.prepared.contains_key(&absent_stage_id)
                || state.retiring_snapshots.contains_key(&absent_stage_id)
                || state.retired_snapshots.contains(&absent_stage_id)
                || state
                    .restore_staging
                    .values()
                    .any(|stage| stage.snapshot_id == absent_stage_id)
                || state
                    .rom_handoff_staging
                    .values()
                    .any(|stage| stage.stage_id == absent_stage_id)
                || state
                    .rom_handoff_aborts
                    .values()
                    .any(|tombstone| tombstone.stage_id == absent_stage_id)
            {
                return Err(Phase2Error::Conflict);
            }
            state.rom_handoff_aborts.insert(
                (fence.character_id, idempotency_key),
                RomHandoffAbortTombstone {
                    request: synthetic_request,
                    stage_id: absent_stage_id,
                    source_world_id: source.rom_world_id,
                },
            );
            return Ok(RomHandoffRecoveryStatus::Aborted {
                stage_id: absent_stage_id,
                source_snapshot_id: request.source_snapshot_id,
                source_world_id: source.rom_world_id,
                expected_revision: request.expected_revision,
                idempotency_key,
            });
        };
        if request.expected_revision != fence.current_revision
            || lease.contract.current_revision != fence.current_revision
        {
            return Err(Phase2Error::Conflict);
        }
        let character = state
            .characters
            .get(&fence.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != fence.current_revision
            || character.active_snapshot != Some(request.source_snapshot_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&request.source_snapshot_id)
            .filter(|snapshot| {
                snapshot.character_id == fence.character_id
                    && snapshot.rom_world_id == source_world_id
            })
            .ok_or(Phase2Error::Conflict)?;
        validate_runtime_binding(catalog, &lease, source.rom_world_id)?;
        if aborted {
            Ok(RomHandoffRecoveryStatus::Aborted {
                stage_id,
                source_snapshot_id: request.source_snapshot_id,
                source_world_id,
                expected_revision: request.expected_revision,
                idempotency_key,
            })
        } else {
            Ok(RomHandoffRecoveryStatus::Staged {
                stage_id,
                source_snapshot_id: request.source_snapshot_id,
                source_world_id,
                expected_revision: request.expected_revision,
                idempotency_key,
            })
        }
    })
}

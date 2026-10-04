//! Authenticated paired cross-ROM rendezvous, staging, and atomic promotion.

use coop_cloud::{
    ApiVersion, ArtifactIdentity, CharacterCloudState, GroupRomHandoffAbortRequest,
    GroupRomHandoffArrivalRequest, GroupRomHandoffIntent, GroupRomHandoffJoinRequest,
    GroupRomHandoffSource, GroupRomHandoffStatus, GroupRomHandoffStatusRequest, IdempotencyKey,
    LeaseFence, MAX_WORLD_REVISION, RuntimeLeaseFence, Sha256Digest, SnapshotFence, SnapshotFile,
    SnapshotId, SnapshotRecord, StableRuntimeSession,
};
use coop_protocol::{RegionalProgress, WorldLocation, WorldZone};
use coop_save::{TransferDescriptorPair, project_arrival};
use subtle::ConstantTimeEq;

use super::saves::{
    seal_object, snapshot_save, validate_character_sav, validate_runtime_binding,
    verified_source_objects,
};
use super::storage::{
    GroupRomHandoffProposal, GroupRomHandoffReceipt, GroupRomHandoffStage, GroupStatus,
    MAX_GROUP_ROM_HANDOFF_STAGES, ROM_HANDOFF_STAGE_TTL_MS, State, Store, VerifiedGroupRomArrival,
    ensure_group_rom_handoff_receipt_capacity, record_group_rom_handoff_receipt,
    reserve_group_rom_handoff_receipt,
};
use super::{AuthenticatedActor, Phase2Error};

const EMPTY_PENDING: &[u8] = b"[]";

struct PairSources {
    source: [SnapshotRecord; 2],
    dormant: [Option<SnapshotRecord>; 2],
    existing: Option<GroupRomHandoffStage>,
}

fn preflight_state(
    state: &State,
    store: &Store,
    actors: [AuthenticatedActor; 2],
    intent: &GroupRomHandoffIntent,
    now: u64,
) -> Result<PairSources, Phase2Error> {
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    intent.validate().map_err(|_| Phase2Error::InvalidRequest)?;
    let descriptor = catalog
        .transfer_descriptor()
        .ok_or(Phase2Error::Forbidden)?;
    if catalog.digest() != intent.catalog_sha256
        || Sha256Digest::of_bytes(descriptor) != intent.descriptor_sha256
        || catalog.resolve_portal(
            intent.source_world_id,
            &intent.portal_id,
            intent.descriptor_sha256,
        ) != Some((
            intent.destination_world_id,
            intent.arrival_portal_id.as_str(),
        ))
        || catalog
            .arrival_template(intent.destination_world_id, &intent.arrival_portal_id)
            .is_none_or(|(_, digest, _)| digest != intent.arrival_template_sha256)
        || actors[0].character_id != intent.members[0].fence.character_id
        || actors[1].character_id != intent.members[1].fence.character_id
    {
        return Err(Phase2Error::Conflict);
    }
    let group = state
        .groups
        .get(&intent.group_id)
        .ok_or(Phase2Error::NotFound)?;
    if group.status != GroupStatus::Active
        || group.group.members() != actors.map(|actor| actor.character_id)
        || group.zone != intent.source_zone
        || group.zone_revision != intent.group_zone_revision
        || state
            .live_group_travel_by_group
            .contains_key(&intent.group_id)
        // A battle reservation must not survive into another world: the group
        // now outlives the crossing, so refuse until the battle settles.
        || actors
            .iter()
            .any(|actor| state.active_battle_by_member.contains_key(&actor.character_id))
    {
        return Err(Phase2Error::Conflict);
    }
    let existing = state
        .group_rom_handoff_stages
        .get(&intent.group_id)
        .cloned();
    if let Some(stage) = &existing {
        stage.validate().map_err(|_| Phase2Error::Conflict)?;
    }
    if existing
        .as_ref()
        .is_some_and(|stage| stage.intent != *intent || stage.expires_at <= now)
        || state
            .group_rom_handoff_receipts
            .contains_key(&(intent.group_id, intent.idempotency_key))
    {
        return Err(Phase2Error::Conflict);
    }
    let mut sources = Vec::with_capacity(2);
    let mut dormant = Vec::with_capacity(2);
    for (actor, member) in actors.into_iter().zip(intent.members) {
        super::group_travel::authenticate_caller(state, actor, member.fence.character_id)?;
        let id = actor.character_id;
        if state.active_group_by_member.get(&id) != Some(&intent.group_id)
            || state.rom_handoff_staging.contains_key(&id)
            || state.restore_staging.contains_key(&id)
            || state.live_group_travel_by_member.contains_key(&id)
            || state
                .prepared
                .values()
                .any(|prepared| prepared.request.character_id == id)
        {
            return Err(Phase2Error::Conflict);
        }
        let lease = state.leases.get(&id).ok_or(Phase2Error::Expired)?;
        if lease.released
            || lease.contract.expires_at.value() <= now
            || lease.contract.fence() != member.fence
        {
            return Err(Phase2Error::Conflict);
        }
        validate_runtime_binding(catalog, lease, intent.source_world_id)?;
        let character = state.characters.get(&id).ok_or(Phase2Error::NotFound)?;
        if character.revision != member.fence.current_revision
            || character.active_snapshot != Some(member.source_snapshot_id)
            || character.state.world_zone != intent.source_zone
        {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&member.source_snapshot_id)
            .filter(|source| {
                source.character_id == id && source.rom_world_id == intent.source_world_id
            })
            .ok_or(Phase2Error::Conflict)?;
        let destination_head = character
            .world_heads
            .get(&intent.destination_world_id)
            .map(|head| {
                state
                    .snapshots
                    .get(head)
                    .filter(|snapshot| {
                        snapshot.character_id == id
                            && snapshot.rom_world_id == intent.destination_world_id
                    })
                    .cloned()
                    .ok_or(Phase2Error::Conflict)
            })
            .transpose()?;
        sources.push(source.clone());
        dormant.push(destination_head);
    }
    Ok(PairSources {
        source: sources.try_into().map_err(|_| Phase2Error::Internal)?,
        dormant: dormant.try_into().map_err(|_| Phase2Error::Internal)?,
        existing,
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
    if !store.objects.put_if_absent(key.clone(), bytes.to_vec())?
        && store.objects.get(&key)?.as_deref() != Some(bytes)
    {
        return Err(Phase2Error::Conflict);
    }
    Ok(())
}

/// Stages two exact destination images under one group intent. `actors` must
/// come from two separately authenticated bearer requests in a future
/// rendezvous layer; a single client may never supply the companion actor.
/// No endpoint invokes this until commit/abort recovery is ready.
#[allow(dead_code)]
pub(crate) fn prepare_pair(
    store: &Store,
    actors: [AuthenticatedActor; 2],
    intent: &GroupRomHandoffIntent,
) -> Result<GroupRomHandoffStage, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    prepare_pair_locked(store, actors, intent, None)
}

fn prepare_pair_locked(
    store: &Store,
    actors: [AuthenticatedActor; 2],
    intent: &GroupRomHandoffIntent,
    client_intent_keys: Option<[IdempotencyKey; 2]>,
) -> Result<GroupRomHandoffStage, Phase2Error> {
    let now = store.now();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let descriptor = catalog
        .transfer_descriptor()
        .ok_or(Phase2Error::Forbidden)?;
    let preflight =
        store.read_transaction(|state| preflight_state(state, store, actors, intent, now))?;
    let mut projected = Vec::with_capacity(2);
    for index in 0..2 {
        let id = actors[index].character_id;
        let (objects, source_save) = verified_source_objects(store, id, &preflight.source[index])?;
        if !objects.iter().any(|(artifact, bytes)| {
            *artifact == ArtifactIdentity::PendingCommits && bytes == EMPTY_PENDING
        }) {
            return Err(Phase2Error::Conflict);
        }
        let destination_save = if let Some(dormant) = &preflight.dormant[index] {
            snapshot_save(store, id, dormant)?
        } else {
            let bytes = catalog
                .arrival_save(intent.destination_world_id, &intent.arrival_portal_id)
                .ok_or(Phase2Error::Forbidden)?;
            if Sha256Digest::of_bytes(bytes) != intent.arrival_template_sha256 {
                return Err(Phase2Error::Conflict);
            }
            coop_save::parse_v2(bytes, super::saves::identity_registry_contract())
                .map_err(|_| Phase2Error::Conflict)?
        };
        let save = project_arrival(
            &source_save,
            &destination_save,
            TransferDescriptorPair {
                source: descriptor,
                destination: descriptor,
            },
            preflight.dormant[index].is_none(),
        )
        .map_err(|_| Phase2Error::Conflict)?;
        projected.push(save.raw_bytes().to_vec());
    }
    let bytes: [Vec<u8>; 2] = projected.try_into().map_err(|_| Phase2Error::Internal)?;
    let digests = bytes.each_ref().map(|save| Sha256Digest::of_bytes(save));
    let ids = if let Some(stage) = &preflight.existing {
        stage.stage_ids
    } else {
        [store.snapshot_id()?, store.snapshot_id()?]
    };
    let challenges = if let Some(stage) = &preflight.existing {
        stage.arrival_challenges
    } else {
        [store.random_challenge()?, store.random_challenge()?]
    };
    let expiry = now
        .checked_add(ROM_HANDOFF_STAGE_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let stage = store.write_transaction(|state| {
        let latest = preflight_state(state, store, actors, intent, store.now())?;
        if latest.source != preflight.source || latest.dormant != preflight.dormant {
            return Err(Phase2Error::Conflict);
        }
        if let Some(existing) = latest.existing {
            if existing.stage_ids != ids || existing.destination_save_sha256 != digests {
                return Err(Phase2Error::Conflict);
            }
            return Ok(existing);
        }
        if state.group_rom_handoff_stages.len() >= MAX_GROUP_ROM_HANDOFF_STAGES
            || ids[0] == ids[1]
            || ids.iter().any(|id| {
                state.snapshots.contains_key(id)
                    || state.retiring_snapshots.contains_key(id)
                    || state.retired_snapshots.contains(id)
                    || state
                        .group_rom_handoff_stages
                        .values()
                        .any(|stage| stage.stage_ids.contains(id))
            })
        {
            return Err(Phase2Error::Busy);
        }
        let stage = GroupRomHandoffStage {
            intent: intent.clone(),
            stage_ids: ids,
            client_intent_keys,
            destination_save_sha256: digests,
            arrival_challenges: challenges,
            verified_arrivals: [None, None],
            expires_at: expiry,
        };
        stage.validate().map_err(|_| Phase2Error::Internal)?;
        reserve_group_rom_handoff_receipt(state, (intent.group_id, intent.idempotency_key), now)?;
        state
            .group_rom_handoff_stages
            .insert(intent.group_id, stage.clone());
        Ok(stage)
    })?;
    for index in 0..2 {
        put_exact(
            store,
            actors[index].character_id,
            stage.stage_ids[index],
            ArtifactIdentity::CharacterSav,
            &bytes[index],
        )?;
        put_exact(
            store,
            actors[index].character_id,
            stage.stage_ids[index],
            ArtifactIdentity::PendingCommits,
            EMPTY_PENDING,
        )?;
    }
    Ok(stage)
}

fn member_index(
    state: &State,
    actor: AuthenticatedActor,
    group_id: coop_cloud::GroupId,
) -> Result<usize, Phase2Error> {
    super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
    let group = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if group.status != GroupStatus::Active
        || state.active_group_by_member.get(&actor.character_id) != Some(&group_id)
    {
        return Err(Phase2Error::Conflict);
    }
    group
        .group
        .members()
        .iter()
        .position(|id| *id == actor.character_id)
        .ok_or(Phase2Error::NotFound)
}

fn historical_member_index(
    state: &State,
    actor: AuthenticatedActor,
    group_id: coop_cloud::GroupId,
) -> Result<usize, Phase2Error> {
    super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
    state
        .groups
        .get(&group_id)
        .ok_or(Phase2Error::NotFound)?
        .group
        .members()
        .iter()
        .position(|id| *id == actor.character_id)
        .ok_or(Phase2Error::NotFound)
}

fn proposal_intent(
    proposal: &GroupRomHandoffProposal,
) -> Result<GroupRomHandoffIntent, Phase2Error> {
    let intent = GroupRomHandoffIntent {
        api_version: ApiVersion::V1,
        group_id: proposal.group_id,
        group_zone_revision: proposal.group_zone_revision,
        source_zone: proposal.source_zone.clone(),
        source_world_id: proposal.source_world_id,
        destination_world_id: proposal.destination_world_id,
        portal_id: proposal.portal_id.clone(),
        arrival_portal_id: proposal.arrival_portal_id.clone(),
        catalog_sha256: proposal.catalog_sha256,
        descriptor_sha256: proposal.descriptor_sha256,
        arrival_template_sha256: proposal.arrival_template_sha256,
        members: [
            proposal.members[0].ok_or(Phase2Error::Conflict)?,
            proposal.members[1].ok_or(Phase2Error::Conflict)?,
        ],
        idempotency_key: proposal.idempotency_key,
    };
    intent.validate().map_err(|_| Phase2Error::Conflict)?;
    Ok(intent)
}

fn actors_for_intent(
    state: &State,
    intent: &GroupRomHandoffIntent,
) -> Result<[AuthenticatedActor; 2], Phase2Error> {
    let members = intent.members.map(|member| member.fence.character_id);
    let first = AuthenticatedActor {
        character_id: members[0],
        user_id: state
            .characters
            .get(&members[0])
            .ok_or(Phase2Error::NotFound)?
            .owner,
    };
    let second = AuthenticatedActor {
        character_id: members[1],
        user_id: state
            .characters
            .get(&members[1])
            .ok_or(Phase2Error::NotFound)?
            .owner,
    };
    Ok([first, second])
}

fn join_matches_receipt(
    receipt: &GroupRomHandoffReceipt,
    index: usize,
    request: &GroupRomHandoffJoinRequest,
) -> bool {
    let matches_source = |source: GroupRomHandoffSource, portal_id: &str| {
        source.fence == request.fence
            && source.source_snapshot_id == request.source_snapshot_id
            && portal_id == request.portal_id
    };
    match receipt {
        GroupRomHandoffReceipt::Withdrawn { proposal, .. } => proposal.members[index]
            .zip(proposal.client_intent_keys[index])
            .is_some_and(|(source, key)| {
                key == request.client_intent_key && matches_source(source, &proposal.portal_id)
            }),
        GroupRomHandoffReceipt::Aborted {
            intent,
            client_intent_keys,
            ..
        }
        | GroupRomHandoffReceipt::Committed {
            intent,
            client_intent_keys,
            ..
        } => client_intent_keys
            .and_then(|keys| keys.get(index).copied())
            .is_some_and(|key| {
                key == request.client_intent_key
                    && matches_source(intent.members[index], &intent.portal_id)
            }),
    }
}

/// First caller opens a server-keyed rendezvous; only the companion's own
/// bearer may fill the second slot. The group revision is pinned server-side.
pub(crate) fn join(
    store: &Store,
    actor: AuthenticatedActor,
    request: &GroupRomHandoffJoinRequest,
) -> Result<GroupRomHandoffStatus, Phase2Error> {
    if request.api_version != ApiVersion::V1
        || request.fence.character_id != actor.character_id
        || !request.valid_portal_id()
    {
        return Err(Phase2Error::InvalidRequest);
    }
    let _gate = store.lock_runtime_transition_gate();
    expire_locked(store, request.group_id)?;
    let now = store.now();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let descriptor = catalog
        .transfer_descriptor()
        .ok_or(Phase2Error::Forbidden)?;
    let replay_key = store.read_transaction(|state| {
        // A closed group still owns its earlier client-key receipts. Resolve
        // that durable identity before requiring live group membership.
        let index = historical_member_index(state, actor, request.group_id)?;
        for ((group_id, key), receipt) in &state.group_rom_handoff_receipts {
            if *group_id != request.group_id {
                continue;
            }
            if receipt.client_intent_keys()[index] == Some(request.client_intent_key) {
                if !join_matches_receipt(receipt, index, request) {
                    return Err(Phase2Error::Conflict);
                }
                return Ok(Some(*key));
            }
        }
        if let Some(stage) = state.group_rom_handoff_stages.get(&request.group_id) {
            if stage
                .client_intent_keys
                .and_then(|keys| keys.get(index).copied())
                == Some(request.client_intent_key)
            {
                if stage.intent.members[index].fence != request.fence
                    || stage.intent.members[index].source_snapshot_id != request.source_snapshot_id
                    || stage.intent.portal_id != request.portal_id
                {
                    return Err(Phase2Error::Conflict);
                }
                return Ok(Some(stage.intent.idempotency_key));
            }
        }
        if let Some(proposal) = state.group_rom_handoff_proposals.get(&request.group_id) {
            if proposal.client_intent_keys[index] == Some(request.client_intent_key) {
                if proposal.members[index].is_none_or(|source| {
                    source.fence != request.fence
                        || source.source_snapshot_id != request.source_snapshot_id
                }) || proposal.portal_id != request.portal_id
                {
                    return Err(Phase2Error::Conflict);
                }
                return Ok(Some(proposal.idempotency_key));
            }
        }
        Ok(None)
    })?;
    if let Some(key) = replay_key {
        return status_locked(store, actor, request.group_id, request.fence, key);
    }
    // A persisted JoinIntent whose original lease has been replaced cannot
    // start a fresh attempt. Since that client has no server attempt key yet,
    // it also cannot have verified arrival; report an exact local abort so
    // the launcher can release the old intent without guessing about a stage.
    let stale_source = store.read_transaction(|state| {
        let group_active = state.groups.get(&request.group_id).is_some_and(|group| {
            group.status == GroupStatus::Active
                && state.active_group_by_member.get(&actor.character_id) == Some(&request.group_id)
        });
        Ok::<_, Phase2Error>(
            !group_active
                || state
                    .leases
                    .get(&actor.character_id)
                    .is_none_or(|lease| lease.released || lease.contract.fence() != request.fence),
        )
    })?;
    if stale_source {
        return Ok(GroupRomHandoffStatus::Aborted {
            group_id: request.group_id,
            idempotency_key: request.client_intent_key,
        });
    }
    let new_key = IdempotencyKey::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal)?;
    let proposal = store.write_transaction(|state| {
        let index = member_index(state, actor, request.group_id)?;
        let group = state
            .groups
            .get(&request.group_id)
            .ok_or(Phase2Error::NotFound)?;
        if group.zone_revision >= coop_cloud::MAX_WORLD_REVISION
            || state
                .live_group_travel_by_group
                .contains_key(&request.group_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let lease = state
            .leases
            .get(&actor.character_id)
            .ok_or(Phase2Error::Expired)?;
        if lease.released
            || lease.contract.expires_at.value() <= now
            || lease.contract.fence() != request.fence
            || state.rom_handoff_staging.contains_key(&actor.character_id)
            || state.restore_staging.contains_key(&actor.character_id)
            || state
                .prepared
                .values()
                .any(|prepared| prepared.request.character_id == actor.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let source_world_id = lease
            .runtime_binding
            .as_ref()
            .ok_or(Phase2Error::Authentication)?
            .world_id;
        validate_runtime_binding(catalog, lease, source_world_id)?;
        let source = state
            .snapshots
            .get(&request.source_snapshot_id)
            .ok_or(Phase2Error::Conflict)?;
        let character = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if source.character_id != actor.character_id
            || source.rom_world_id != source_world_id
            || character.active_snapshot != Some(request.source_snapshot_id)
            || character.revision != request.fence.current_revision
            || character.state.world_zone != group.zone
        {
            return Err(Phase2Error::Conflict);
        }
        if let Some(existing) = state.group_rom_handoff_proposals.get(&request.group_id) {
            if (existing.expires_at <= now
                && !state
                    .group_rom_handoff_stages
                    .get(&request.group_id)
                    .is_some_and(|stage| stage.expires_at > now))
                || existing.portal_id != request.portal_id
                || existing.group_zone_revision != group.zone_revision
                || existing.source_zone != group.zone
                || existing.source_world_id != source_world_id
                || existing.catalog_sha256 != catalog.digest()
            {
                return Err(Phase2Error::Conflict);
            }
            let expected = GroupRomHandoffSource {
                fence: request.fence,
                source_snapshot_id: request.source_snapshot_id,
            };
            if existing.members[index].is_some_and(|member| member != expected) {
                return Err(Phase2Error::Conflict);
            }
            if existing.client_intent_keys[index]
                .is_some_and(|key| key != request.client_intent_key)
            {
                return Err(Phase2Error::Conflict);
            }
            let mut proposal = existing.clone();
            proposal.members[index] = Some(expected);
            proposal.client_intent_keys[index] = Some(request.client_intent_key);
            state
                .group_rom_handoff_proposals
                .insert(request.group_id, proposal.clone());
            return Ok(proposal);
        }
        if state
            .group_rom_handoff_stages
            .contains_key(&request.group_id)
            || state.group_rom_handoff_proposals.len() >= MAX_GROUP_ROM_HANDOFF_STAGES
        {
            return Err(Phase2Error::Busy);
        }
        let descriptor_sha256 = Sha256Digest::of_bytes(descriptor);
        let (destination_world_id, arrival_portal_id) = catalog
            .resolve_portal(source_world_id, &request.portal_id, descriptor_sha256)
            .ok_or(Phase2Error::Forbidden)?;
        let arrival_template_sha256 = catalog
            .arrival_template(destination_world_id, arrival_portal_id)
            .ok_or(Phase2Error::Forbidden)?
            .1;
        let mut members = [None, None];
        let mut client_intent_keys = [None, None];
        members[index] = Some(GroupRomHandoffSource {
            fence: request.fence,
            source_snapshot_id: request.source_snapshot_id,
        });
        client_intent_keys[index] = Some(request.client_intent_key);
        let proposal = GroupRomHandoffProposal {
            group_id: request.group_id,
            idempotency_key: new_key,
            group_zone_revision: group.zone_revision,
            source_zone: group.zone.clone(),
            source_world_id,
            destination_world_id,
            portal_id: request.portal_id.clone(),
            arrival_portal_id: arrival_portal_id.to_owned(),
            catalog_sha256: catalog.digest(),
            descriptor_sha256,
            arrival_template_sha256,
            members,
            client_intent_keys,
            expires_at: now
                .checked_add(ROM_HANDOFF_STAGE_TTL_MS)
                .ok_or(Phase2Error::Internal)?,
        };
        proposal.validate().map_err(|_| Phase2Error::Internal)?;
        reserve_group_rom_handoff_receipt(
            state,
            (request.group_id, proposal.idempotency_key),
            now,
        )?;
        state
            .group_rom_handoff_proposals
            .insert(request.group_id, proposal.clone());
        Ok(proposal)
    })?;
    if proposal.members.iter().all(Option::is_some) {
        let intent = proposal_intent(&proposal)?;
        let actors = store.read_transaction(|state| actors_for_intent(state, &intent))?;
        let keys = [
            proposal.client_intent_keys[0].ok_or(Phase2Error::Conflict)?,
            proposal.client_intent_keys[1].ok_or(Phase2Error::Conflict)?,
        ];
        prepare_pair_locked(store, actors, &intent, Some(keys))?;
    }
    status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        proposal.idempotency_key,
    )
}

fn status_locked(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: coop_cloud::GroupId,
    fence: LeaseFence,
    key: IdempotencyKey,
) -> Result<GroupRomHandoffStatus, Phase2Error> {
    if fence.character_id != actor.character_id {
        return Err(Phase2Error::InvalidRequest);
    }
    let lookup = store.read_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
        if let Some(receipt) = state.group_rom_handoff_receipts.get(&(group_id, key)) {
            let index = match receipt {
                GroupRomHandoffReceipt::Withdrawn { proposal, .. } => proposal
                    .members
                    .iter()
                    .position(|member| member.is_some_and(|member| member.fence == fence)),
                GroupRomHandoffReceipt::Aborted { intent, .. }
                | GroupRomHandoffReceipt::Committed { intent, .. } => intent
                    .members
                    .iter()
                    .position(|member| member.fence == fence),
            }
            .ok_or(Phase2Error::NotFound)?;
            return Ok((Some(receipt.clone()), None, None, index));
        }
        let index = member_index(state, actor, group_id)?;
        if let Some(stage) = state.group_rom_handoff_stages.get(&group_id) {
            if stage.intent.idempotency_key != key || stage.intent.members[index].fence != fence {
                return Err(Phase2Error::NotFound);
            }
            return Ok((None, Some(stage.clone()), None, index));
        }
        let proposal = state
            .group_rom_handoff_proposals
            .get(&group_id)
            .ok_or(Phase2Error::NotFound)?;
        if proposal.idempotency_key != key
            || proposal.members[index].is_none_or(|member| member.fence != fence)
        {
            return Err(Phase2Error::NotFound);
        }
        Ok((None, None, Some(proposal.clone()), index))
    })?;
    let (receipt, stage, proposal, index) = lookup;
    if let Some(receipt) = receipt {
        return Ok(match receipt {
            GroupRomHandoffReceipt::Withdrawn { .. } | GroupRomHandoffReceipt::Aborted { .. } => {
                GroupRomHandoffStatus::Aborted {
                    group_id,
                    idempotency_key: key,
                }
            }
            GroupRomHandoffReceipt::Committed {
                snapshots,
                destination_zone,
                group_zone_revision,
                ..
            } => GroupRomHandoffStatus::Committed {
                group_id,
                idempotency_key: key,
                destination_zone,
                group_zone_revision,
                own_snapshot: snapshots[index].clone(),
            },
        });
    }
    if let Some(stage) = stage {
        let save_key = Store::object_key(
            actor.character_id,
            stage.stage_ids[index],
            ArtifactIdentity::CharacterSav,
        );
        let destination_save = store.objects.get(&save_key)?.ok_or(Phase2Error::Busy)?;
        if Sha256Digest::of_bytes(&destination_save) != stage.destination_save_sha256[index] {
            return Err(Phase2Error::Conflict);
        }
        return Ok(GroupRomHandoffStatus::Staged {
            group_id,
            idempotency_key: key,
            stage_id: stage.stage_ids[index],
            destination_world_id: stage.intent.destination_world_id,
            arrival_portal_id: stage.intent.arrival_portal_id,
            destination_save_sha256: stage.destination_save_sha256[index],
            destination_save,
            arrival_challenge: stage.arrival_challenges[index],
            acknowledged_by: stage.verified_arrivals.map(|arrival| arrival.is_some()),
        });
    }
    let proposal = proposal.ok_or(Phase2Error::Internal)?;
    Ok(GroupRomHandoffStatus::Pending {
        group_id,
        idempotency_key: key,
        portal_id: proposal.portal_id,
        submitted_by: proposal.members.map(|member| member.is_some()),
    })
}

pub(crate) fn status(
    store: &Store,
    actor: AuthenticatedActor,
    request: &GroupRomHandoffStatusRequest,
) -> Result<GroupRomHandoffStatus, Phase2Error> {
    if request.api_version != ApiVersion::V1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let _gate = store.lock_runtime_transition_gate();
    expire_locked(store, request.group_id)?;
    commit_verified_pair_locked(store, request.group_id)?;
    status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        request.idempotency_key,
    )
}

fn retire_stage_objects(store: &Store, stage: &GroupRomHandoffStage) -> Result<(), Phase2Error> {
    for (index, member) in stage.intent.members.iter().enumerate() {
        for artifact in [
            ArtifactIdentity::CharacterSav,
            ArtifactIdentity::PendingCommits,
        ] {
            let key =
                Store::object_key(member.fence.character_id, stage.stage_ids[index], artifact);
            seal_object(store, &key)?;
        }
    }
    Ok(())
}

fn expire_locked(store: &Store, group_id: coop_cloud::GroupId) -> Result<(), Phase2Error> {
    let now = store.now();
    let expired = store.read_transaction(|state| {
        let group_closed = state
            .groups
            .get(&group_id)
            .is_none_or(|group| group.status != GroupStatus::Active);
        let stage = state
            .group_rom_handoff_stages
            .get(&group_id)
            .filter(|stage| stage.expires_at <= now || group_closed)
            .cloned();
        let proposal = if state.group_rom_handoff_stages.contains_key(&group_id) {
            None
        } else {
            state
                .group_rom_handoff_proposals
                .get(&group_id)
                .filter(|proposal| proposal.expires_at <= now || group_closed)
                .cloned()
        };
        let receipt_gc = state.group_rom_handoff_receipts.values().any(|receipt| {
            receipt.resolved_at()
                <= now.saturating_sub(super::storage::GROUP_ROM_HANDOFF_RECEIPT_RETENTION_MS)
        });
        Ok::<_, Phase2Error>((stage, proposal, receipt_gc))
    })?;
    if let Some(stage) = &expired.0 {
        store.read_transaction(|state| {
            ensure_group_rom_handoff_receipt_capacity(
                state,
                (group_id, stage.intent.idempotency_key),
                now,
            )
        })?;
        retire_stage_objects(store, stage)?;
    }
    if expired.0.is_none() && expired.1.is_none() && !expired.2 {
        return Ok(());
    }
    store.write_transaction(|state| {
        if let Some(receipt_key) = expired
            .0
            .as_ref()
            .map(|stage| (group_id, stage.intent.idempotency_key))
            .or_else(|| {
                expired
                    .1
                    .as_ref()
                    .map(|proposal| (group_id, proposal.idempotency_key))
            })
        {
            ensure_group_rom_handoff_receipt_capacity(state, receipt_key, now)?;
        }
        super::storage::prune_group_rom_handoff_receipts(state, now);
        if let Some(stage) = &expired.0 {
            if state.group_rom_handoff_stages.get(&group_id) != Some(stage) {
                return Err(Phase2Error::Conflict);
            }
            state.group_rom_handoff_stages.remove(&group_id);
            state.group_rom_handoff_proposals.remove(&group_id);
            record_group_rom_handoff_receipt(
                state,
                (group_id, stage.intent.idempotency_key),
                GroupRomHandoffReceipt::Aborted {
                    intent: stage.intent.clone(),
                    stage_ids: stage.stage_ids,
                    client_intent_keys: stage.client_intent_keys,
                    resolved_at: now,
                },
                now,
            )?;
        } else if let Some(proposal) = &expired.1 {
            if state.group_rom_handoff_proposals.get(&group_id) != Some(proposal) {
                return Err(Phase2Error::Conflict);
            }
            state.group_rom_handoff_proposals.remove(&group_id);
            record_group_rom_handoff_receipt(
                state,
                (group_id, proposal.idempotency_key),
                GroupRomHandoffReceipt::Withdrawn {
                    proposal: proposal.clone(),
                    resolved_at: now,
                },
                now,
            )?;
        }
        Ok(())
    })
}

pub(crate) fn abort(
    store: &Store,
    actor: AuthenticatedActor,
    request: &GroupRomHandoffAbortRequest,
) -> Result<GroupRomHandoffStatus, Phase2Error> {
    if request.api_version != ApiVersion::V1 || request.fence.character_id != actor.character_id {
        return Err(Phase2Error::InvalidRequest);
    }
    let _gate = store.lock_runtime_transition_gate();
    expire_locked(store, request.group_id)?;
    let prior = status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        request.idempotency_key,
    )?;
    if matches!(
        prior,
        GroupRomHandoffStatus::Committed { .. } | GroupRomHandoffStatus::Aborted { .. }
    ) {
        return Ok(prior);
    }
    let stage = store.read_transaction(|state| {
        Ok::<_, Phase2Error>(
            state
                .group_rom_handoff_stages
                .get(&request.group_id)
                .cloned(),
        )
    })?;
    let now = store.now();
    if stage.is_some() {
        store.read_transaction(|state| {
            ensure_group_rom_handoff_receipt_capacity(
                state,
                (request.group_id, request.idempotency_key),
                now,
            )
        })?;
    }
    if let Some(stage) = &stage {
        retire_stage_objects(store, stage)?;
    }
    store.write_transaction(|state| {
        let has_receipt_target = stage.is_some()
            || state
                .group_rom_handoff_proposals
                .contains_key(&request.group_id);
        if has_receipt_target {
            ensure_group_rom_handoff_receipt_capacity(
                state,
                (request.group_id, request.idempotency_key),
                now,
            )?;
        }
        if let Some(stage) = &stage {
            if state.group_rom_handoff_stages.get(&request.group_id) != Some(stage)
                || stage.intent.idempotency_key != request.idempotency_key
            {
                return Err(Phase2Error::Conflict);
            }
            state.group_rom_handoff_stages.remove(&request.group_id);
            state.group_rom_handoff_proposals.remove(&request.group_id);
            record_group_rom_handoff_receipt(
                state,
                (request.group_id, request.idempotency_key),
                GroupRomHandoffReceipt::Aborted {
                    intent: stage.intent.clone(),
                    stage_ids: stage.stage_ids,
                    client_intent_keys: stage.client_intent_keys,
                    resolved_at: now,
                },
                now,
            )?;
        } else {
            let proposal = state
                .group_rom_handoff_proposals
                .get(&request.group_id)
                .cloned()
                .ok_or(Phase2Error::Conflict)?;
            if proposal.idempotency_key != request.idempotency_key {
                return Err(Phase2Error::Conflict);
            }
            state.group_rom_handoff_proposals.remove(&request.group_id);
            record_group_rom_handoff_receipt(
                state,
                (request.group_id, request.idempotency_key),
                GroupRomHandoffReceipt::Withdrawn {
                    proposal,
                    resolved_at: now,
                },
                now,
            )?;
        }
        Ok(())
    })?;
    status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        request.idempotency_key,
    )
}

pub(crate) fn arrive(
    store: &Store,
    actor: AuthenticatedActor,
    request: &GroupRomHandoffArrivalRequest,
) -> Result<GroupRomHandoffStatus, Phase2Error> {
    if request.api_version != ApiVersion::V1 || request.fence.character_id != actor.character_id {
        return Err(Phase2Error::InvalidRequest);
    }
    let _gate = store.lock_runtime_transition_gate();
    expire_locked(store, request.group_id)?;
    let prior = status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        request.idempotency_key,
    )?;
    if matches!(
        prior,
        GroupRomHandoffStatus::Committed { .. } | GroupRomHandoffStatus::Aborted { .. }
    ) {
        return Ok(prior);
    }
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        let stage = state
            .group_rom_handoff_stages
            .get(&request.group_id)
            .cloned()
            .ok_or(Phase2Error::Conflict)?;
        let index = stage
            .intent
            .members
            .iter()
            .position(|member| member.fence == request.fence)
            .ok_or(Phase2Error::Conflict)?;
        let actors = actors_for_intent(state, &stage.intent)?;
        preflight_state(state, store, actors, &stage.intent, store.now())?;
        let expected_build = catalog
            .for_snapshot(
                Some(stage.intent.destination_world_id),
                Some(stage.intent.destination_world_id),
            )
            .map_err(|_| Phase2Error::Forbidden)?;
        if stage.expires_at <= store.now()
            || stage.intent.idempotency_key != request.idempotency_key
            || stage.stage_ids[index] != request.stage_id
            || stage.destination_save_sha256[index] != request.destination_save_sha256
            || &request.destination_build != expected_build
            || !bool::from(
                request.acknowledgment_mac.as_bytes().ct_eq(
                    request
                        .expected_mac(stage.arrival_challenges[index])
                        .as_bytes(),
                ),
            )
        {
            return Err(Phase2Error::Conflict);
        }
        let verified = VerifiedGroupRomArrival {
            stage_id: request.stage_id,
            destination_save_sha256: request.destination_save_sha256,
            runtime: RuntimeLeaseFence::new(
                StableRuntimeSession::from_lease_fence(&request.fence),
                request.destination_build.clone(),
            ),
            acknowledgment_sha256: request.acknowledgment_mac,
        };
        let current = state
            .group_rom_handoff_stages
            .get_mut(&request.group_id)
            .ok_or(Phase2Error::Conflict)?;
        if current.verified_arrivals[index]
            .as_ref()
            .is_some_and(|arrival| arrival != &verified)
        {
            return Err(Phase2Error::Conflict);
        }
        current.verified_arrivals[index] = Some(verified);
        Ok(())
    })?;
    commit_verified_pair_locked(store, request.group_id)?;
    status_locked(
        store,
        actor,
        request.group_id,
        request.fence,
        request.idempotency_key,
    )
}

fn commit_verified_pair_locked(
    store: &Store,
    group_id: coop_cloud::GroupId,
) -> Result<(), Phase2Error> {
    let now = store.now();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let ready = store.read_transaction(|state| {
        let Some(stage) = state.group_rom_handoff_stages.get(&group_id) else {
            return Ok(None);
        };
        if stage.verified_arrivals.iter().any(Option::is_none) {
            return Ok(None);
        }
        let expected_build = catalog
            .for_snapshot(
                Some(stage.intent.destination_world_id),
                Some(stage.intent.destination_world_id),
            )
            .map_err(|_| Phase2Error::Forbidden)?;
        if stage
            .verified_arrivals
            .iter()
            .flatten()
            .any(|arrival| &arrival.runtime.build != expected_build)
        {
            return Err(Phase2Error::Conflict);
        }
        let actors = actors_for_intent(state, &stage.intent)?;
        let sources = preflight_state(state, store, actors, &stage.intent, now)?;
        Ok::<_, Phase2Error>(Some((stage.clone(), actors, sources.source)))
    })?;
    let Some((stage, actors, sources)) = ready else {
        return Ok(());
    };
    let revisions = stage.intent.members.map(|member| {
        member
            .fence
            .current_revision
            .next()
            .map_err(|_| Phase2Error::Conflict)
    });
    let revisions = [revisions[0].clone()?, revisions[1].clone()?];
    let mut records = Vec::with_capacity(2);
    let mut file_pairs = Vec::with_capacity(2);
    let mut zones = Vec::with_capacity(2);
    for index in 0..2 {
        let id = actors[index].character_id;
        let save_key =
            Store::object_key(id, stage.stage_ids[index], ArtifactIdentity::CharacterSav);
        let pending_key =
            Store::object_key(id, stage.stage_ids[index], ArtifactIdentity::PendingCommits);
        let bytes = store.objects.get(&save_key)?.ok_or(Phase2Error::Conflict)?;
        let pending = store
            .objects
            .get(&pending_key)?
            .ok_or(Phase2Error::Conflict)?;
        if Sha256Digest::of_bytes(&bytes) != stage.destination_save_sha256[index]
            || pending != EMPTY_PENDING
        {
            return Err(Phase2Error::Conflict);
        }
        let projected =
            validate_character_sav(&bytes, revisions[index]).map_err(|_| Phase2Error::Conflict)?;
        let source_save = snapshot_save(store, id, &sources[index])?;
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
        let local = projected
            .logical_sector_payload(1)
            .ok_or(Phase2Error::Conflict)?;
        let map_group = u16::from(*local.get(4).ok_or(Phase2Error::Conflict)?);
        let map_number = u16::from(*local.get(5).ok_or(Phase2Error::Conflict)?);
        let map = coop_protocol::catalog::resolve_unique_map_coordinates(map_group, map_number)
            .ok_or(Phase2Error::Conflict)?;
        if !catalog.allows_presence_region(stage.intent.destination_world_id, map.region) {
            return Err(Phase2Error::Forbidden);
        }
        let location = WorldLocation::new(map.region, map_group, map_number, 0, 0)
            .map_err(|_| Phase2Error::Conflict)?;
        let zone = WorldZone::from_location(&location, stage.intent.source_zone.channel)
            .map_err(|_| Phase2Error::Conflict)?;
        zones.push(zone);
        let files = vec![
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &bytes)
                .map_err(|_| Phase2Error::Internal)?,
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, EMPTY_PENDING)
                .map_err(|_| Phase2Error::Internal)?,
        ];
        records.push(
            SnapshotRecord::new(
                stage.stage_ids[index],
                stage.intent.destination_world_id,
                SnapshotFence::new(
                    stage.intent.members[index].fence.session_id,
                    id,
                    stage.intent.members[index].fence.session_epoch,
                ),
                stage.intent.members[index].fence.current_revision,
                revisions[index],
                files.clone(),
                Sha256Digest::of_bytes(EMPTY_PENDING),
                sources[index].last_applied_commit,
                Store::unix_timestamp(now)?,
            )
            .map_err(|_| Phase2Error::Internal)?,
        );
        file_pairs.push(files);
    }
    let zone = zones[0].clone();
    if zones[1] != zone {
        return Err(Phase2Error::Conflict);
    }
    let records: [SnapshotRecord; 2] = records.try_into().map_err(|_| Phase2Error::Internal)?;
    let file_pairs: [Vec<SnapshotFile>; 2] =
        file_pairs.try_into().map_err(|_| Phase2Error::Internal)?;
    store.write_transaction(|state| {
        let current = state
            .group_rom_handoff_stages
            .get(&group_id)
            .ok_or(Phase2Error::Conflict)?;
        if current != &stage
            || current.expires_at <= store.now()
            || current.verified_arrivals.iter().any(Option::is_none)
        {
            return Err(Phase2Error::Conflict);
        }
        ensure_group_rom_handoff_receipt_capacity(
            state,
            (group_id, stage.intent.idempotency_key),
            store.now(),
        )?;
        let reacquire_by = super::sessions::handoff_reacquire_deadline(store.now())?;
        let latest = preflight_state(state, store, actors, &stage.intent, store.now())?;
        if latest.source != sources
            || records
                .iter()
                .any(|record| state.snapshots.contains_key(&record.snapshot_id))
            || stage
                .intent
                .group_zone_revision
                .checked_add(1)
                .is_none_or(|next| next > MAX_WORLD_REVISION)
        {
            return Err(Phase2Error::Conflict);
        }
        let mut destination_states = Vec::with_capacity(2);
        let mut world_revisions = Vec::with_capacity(2);
        for index in 0..2 {
            let id = actors[index].character_id;
            let character = state.characters.get(&id).ok_or(Phase2Error::NotFound)?;
            if state
                .snapshot_by_revision
                .contains_key(&(id, revisions[index]))
            {
                return Err(Phase2Error::Conflict);
            }
            let world_revision = character
                .world_revision
                .checked_add(1)
                .filter(|revision| *revision <= MAX_WORLD_REVISION)
                .ok_or(Phase2Error::Conflict)?;
            let mut progress = character.state.regional_progress.clone();
            if !progress.iter().any(|entry| entry.region == zone.region) {
                progress.push(
                    RegionalProgress::new(zone.region, 0, 0, vec![], vec![])
                        .map_err(|_| Phase2Error::Conflict)?,
                );
            }
            destination_states.push(
                CharacterCloudState::new(id, zone.clone(), progress)
                    .map_err(|_| Phase2Error::Conflict)?,
            );
            world_revisions.push(world_revision);
            super::saves::handoff::can_make_snapshot_room(
                state,
                id,
                &file_pairs[index],
                None,
                None,
                Some(stage.intent.members[index].source_snapshot_id),
            )?;
        }
        // All fallible validation and both quota plans precede the mutation suffix.
        for index in 0..2 {
            super::saves::handoff::make_snapshot_room(
                state,
                actors[index].character_id,
                &file_pairs[index],
                None,
                None,
                Some(stage.intent.members[index].source_snapshot_id),
            )?;
        }
        for index in 0..2 {
            let id = actors[index].character_id;
            state
                .snapshots
                .insert(stage.stage_ids[index], records[index].clone());
            state
                .snapshot_by_revision
                .insert((id, revisions[index]), stage.stage_ids[index]);
            let character = state.characters.get_mut(&id).ok_or(Phase2Error::Internal)?;
            character.revision = revisions[index];
            character.world_revision = world_revisions[index];
            character.active_snapshot = Some(stage.stage_ids[index]);
            character.state = destination_states[index].clone();
            character
                .world_heads
                .insert(stage.intent.destination_world_id, stage.stage_ids[index]);
            let lease = state.leases.get_mut(&id).ok_or(Phase2Error::Internal)?;
            lease.released = true;
            lease.runtime_binding = None;
            // Travelling, not leaving: the group survives until the member
            // acquires the destination lease or the bounded window lapses.
            super::sessions::record_handoff_release(state, id, reacquire_by);
            state
                .realtime_tickets
                .retain(|_, ticket| ticket.character_id != id);
        }
        let live_tickets = state
            .realtime_tickets
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        state
            .realtime_by_runtime
            .retain(|_, fingerprint| live_tickets.contains(fingerprint));
        let committed_zone_revision = {
            let group = state
                .groups
                .get_mut(&group_id)
                .ok_or(Phase2Error::Internal)?;
            group.zone = zone.clone();
            group.zone_revision += 1;
            group.zone_revision
        };
        state.group_rom_handoff_stages.remove(&group_id);
        state.group_rom_handoff_proposals.remove(&group_id);
        record_group_rom_handoff_receipt(
            state,
            (group_id, stage.intent.idempotency_key),
            GroupRomHandoffReceipt::Committed {
                intent: stage.intent.clone(),
                snapshots: records.clone(),
                destination_zone: zone.clone(),
                group_zone_revision: committed_zone_revision,
                client_intent_keys: stage.client_intent_keys,
                resolved_at: now,
            },
            now,
        )?;
        Ok(())
    })
}

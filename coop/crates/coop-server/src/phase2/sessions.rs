//! Atomic server-issued lease transitions.

use super::storage::{
    ACQUIRE_IDEMPOTENCY_TTL_MS, AcquireRecord, HEARTBEAT_INTERVAL_MS, LEASE_TTL_MS, LeaseRecord,
    MAX_ACQUIRE_HISTORY, MAX_RELEASE_KEYS, RECONNECT_GRACE_MS, Store,
};
use super::{AuthenticatedActor, Phase2Error};
use coop_cloud::{
    AcquireLeaseRequest, AcquireWorldLeaseResponse, HeartbeatLeaseRequest, LeaseContract,
    LeaseFence, LogoutResponse, ReconnectLeaseRequest, ReleaseLeaseRequest, RuntimeBuildIdentity,
    SessionEpoch, SnapshotId,
};
use coop_protocol::RomWorldId;

fn owns(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    character_id: coop_cloud::CharacterId,
) -> bool {
    state
        .characters
        .get(&character_id)
        .is_some_and(|character| {
            character.owner == actor.user_id && actor.character_id == character_id
        })
}

fn fence_matches(contract: LeaseContract, fence: LeaseFence) -> bool {
    contract.fence() == fence
}

pub(crate) fn acquire(
    store: &Store,
    actor: AuthenticatedActor,
    request: &AcquireLeaseRequest,
) -> Result<LeaseContract, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.character_id != actor.character_id {
        return Err(Phase2Error::NotFound);
    }
    let now = store.now();
    let expires = now.checked_add(LEASE_TTL_MS).ok_or(Phase2Error::Internal)?;
    let grace_until = expires
        .checked_add(RECONNECT_GRACE_MS)
        .ok_or(Phase2Error::Internal)?;
    let history_expires = now
        .checked_add(ACQUIRE_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        if !owns(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        state
            .acquire_history
            .retain(|_, record| record.expires_at > now);
        if let Some(record) = state.acquire_history.get(&request.idempotency_key) {
            if record.character_id == request.character_id
                && record.client_instance_id == request.client_instance_id
                && state
                    .leases
                    .get(&request.character_id)
                    .is_some_and(|lease| lease.contract.fence() == record.contract.fence())
            {
                return Ok(record.contract);
            }
            return Err(Phase2Error::Conflict);
        }
        if state
            .leases
            .get(&request.character_id)
            .is_some_and(|existing| {
                !existing.released
                    && existing.grace_until > now
                    && !(request.replace_same_client
                        && existing.contract.client_instance_id == request.client_instance_id)
            })
        {
            return Err(Phase2Error::Conflict);
        }
        let history_for_character = state
            .acquire_history
            .values()
            .filter(|record| record.character_id == request.character_id)
            .count();
        if history_for_character >= MAX_ACQUIRE_HISTORY {
            return Err(Phase2Error::Busy);
        }
        if request.replace_same_client
            && state
                .leases
                .get(&request.character_id)
                .is_some_and(|existing| {
                    !existing.released
                        && existing.grace_until > now
                        && existing.contract.client_instance_id == request.client_instance_id
                })
        {
            super::group_travel::cancel_pending_for_member(state, actor.character_id);
        }
        let character = state
            .characters
            .get_mut(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        let next_epoch = character
            .last_session_epoch
            .checked_add(1)
            .ok_or(Phase2Error::Internal)?;
        let epoch = SessionEpoch::new(next_epoch).map_err(|_| Phase2Error::Internal)?;
        let session_id = store.session_id()?;
        let contract = LeaseContract::new(
            LeaseFence::new(
                session_id,
                request.character_id,
                character.revision,
                epoch,
                request.client_instance_id,
            ),
            Store::unix_timestamp(expires)?,
            HEARTBEAT_INTERVAL_MS,
        )
        .map_err(|_| Phase2Error::Internal)?;
        character.last_session_epoch = next_epoch;
        state.leases.insert(
            request.character_id,
            LeaseRecord {
                contract,
                grace_until,
                released: false,
                reconnect: None,
                release_keys: Vec::new(),
                runtime_binding: None,
            },
        );
        state.acquire_history.insert(
            request.idempotency_key,
            AcquireRecord {
                character_id: request.character_id,
                client_instance_id: request.client_instance_id,
                contract,
                expires_at: history_expires,
            },
        );
        Ok(contract)
    })
}

fn authoritative_world(
    state: &super::storage::State,
    store: &Store,
    character_id: coop_cloud::CharacterId,
) -> Result<
    (
        RomWorldId,
        Option<SnapshotId>,
        RuntimeBuildIdentity,
        coop_cloud::Revision,
    ),
    Phase2Error,
> {
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::NotFound)?;
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    if character.revision == coop_cloud::Revision::initial() {
        if character.active_snapshot.is_some() || !character.world_heads.is_empty() {
            return Err(Phase2Error::Conflict);
        }
        let world_id = RomWorldId::new(1).map_err(|_| Phase2Error::Internal)?;
        let build = catalog
            .for_snapshot(Some(world_id), Some(world_id))
            .map_err(|_| Phase2Error::Internal)?
            .clone();
        return Ok((world_id, None, build, character.revision));
    }
    let snapshot_id = character.active_snapshot.ok_or(Phase2Error::Conflict)?;
    let snapshot = state
        .snapshots
        .get(&snapshot_id)
        .ok_or(Phase2Error::Conflict)?;
    if snapshot.character_id != character_id
        || snapshot.revision != character.revision
        || character.world_heads.get(&snapshot.rom_world_id) != Some(&snapshot_id)
    {
        return Err(Phase2Error::Conflict);
    }
    let build = catalog
        .for_snapshot(Some(snapshot.rom_world_id), Some(snapshot.rom_world_id))
        .map_err(|_| Phase2Error::Internal)?
        .clone();
    Ok((
        snapshot.rom_world_id,
        Some(snapshot_id),
        build,
        character.revision,
    ))
}

/// Acquires a lease and binds it to the server-authoritative active ROM world
/// in the same repository transaction. This route is deliberately separate
/// from the legacy acquire route so old clients retain their behavior while
/// new clients can safely use resume-package and realtime capabilities before
/// minting a ticket.
pub(crate) fn acquire_world(
    store: &Store,
    actor: AuthenticatedActor,
    request: &AcquireLeaseRequest,
) -> Result<AcquireWorldLeaseResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.character_id != actor.character_id {
        return Err(Phase2Error::NotFound);
    }
    let now = store.now();
    let expires = now.checked_add(LEASE_TTL_MS).ok_or(Phase2Error::Internal)?;
    let grace_until = expires
        .checked_add(RECONNECT_GRACE_MS)
        .ok_or(Phase2Error::Internal)?;
    let history_expires = now
        .checked_add(ACQUIRE_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        if !owns(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let (world_id, snapshot_id, build, revision) =
            authoritative_world(state, store, request.character_id)?;
        state
            .acquire_history
            .retain(|_, record| record.expires_at > now);
        if let Some(record) = state.acquire_history.get(&request.idempotency_key) {
            if record.character_id != request.character_id
                || record.client_instance_id != request.client_instance_id
            {
                return Err(Phase2Error::Conflict);
            }
            let lease = state.leases.get(&request.character_id);
            let binding_matches = lease.is_some_and(|lease| {
                lease.runtime_binding.as_ref().is_some_and(|binding| {
                    binding.world_id == world_id
                        && binding.build == build
                        && binding.session == lease.contract.stable_runtime_session()
                })
            });
            let Some(lease) = lease else {
                return Err(Phase2Error::AcquireClosed);
            };
            if lease.contract.stable_runtime_session()
                != record.contract.stable_runtime_session()
                || lease.released
            {
                return Err(Phase2Error::AcquireClosed);
            }
            if lease.contract.expires_at.value() <= now || lease.grace_until <= now {
                return Err(Phase2Error::AcquireClosed);
            }
            if lease.contract.current_revision == revision
                && lease.contract.current_revision.value()
                    >= record.contract.current_revision.value()
                && binding_matches
            {
                return Ok(AcquireWorldLeaseResponse {
                    lease: lease.contract,
                    active_world_id: world_id,
                    active_snapshot_id: snapshot_id,
                });
            }
            return Err(Phase2Error::Conflict);
        }
        if state
            .leases
            .get(&request.character_id)
            .is_some_and(|existing| {
                !existing.released
                    && existing.grace_until > now
                    && !(request.replace_same_client
                        && existing.contract.client_instance_id == request.client_instance_id)
            })
        {
            return Err(Phase2Error::Conflict);
        }
        let history_for_character = state
            .acquire_history
            .values()
            .filter(|record| record.character_id == request.character_id)
            .count();
        if history_for_character >= MAX_ACQUIRE_HISTORY {
            return Err(Phase2Error::Busy);
        }
        if request.replace_same_client
            && state
                .leases
                .get(&request.character_id)
                .is_some_and(|existing| {
                    !existing.released
                        && existing.grace_until > now
                        && existing.contract.client_instance_id == request.client_instance_id
                })
        {
            super::group_travel::cancel_pending_for_member(state, actor.character_id);
        }
        let character = state
            .characters
            .get_mut(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != revision {
            return Err(Phase2Error::Conflict);
        }
        let next_epoch = character
            .last_session_epoch
            .checked_add(1)
            .ok_or(Phase2Error::Internal)?;
        let epoch = SessionEpoch::new(next_epoch).map_err(|_| Phase2Error::Internal)?;
        let session_id = store.session_id()?;
        let contract = LeaseContract::new(
            LeaseFence::new(
                session_id,
                request.character_id,
                revision,
                epoch,
                request.client_instance_id,
            ),
            Store::unix_timestamp(expires)?,
            HEARTBEAT_INTERVAL_MS,
        )
        .map_err(|_| Phase2Error::Internal)?;
        character.last_session_epoch = next_epoch;
        state.leases.insert(
            request.character_id,
            LeaseRecord {
                contract,
                grace_until,
                released: false,
                reconnect: None,
                release_keys: Vec::new(),
                runtime_binding: Some(super::storage::RuntimeWorldBinding {
                    world_id,
                    build,
                    session: contract.stable_runtime_session(),
                }),
            },
        );
        state.acquire_history.insert(
            request.idempotency_key,
            AcquireRecord {
                character_id: request.character_id,
                client_instance_id: request.client_instance_id,
                contract,
                expires_at: history_expires,
            },
        );
        Ok(AcquireWorldLeaseResponse {
            lease: contract,
            active_world_id: world_id,
            active_snapshot_id: snapshot_id,
        })
    })
}

pub(crate) fn heartbeat(
    store: &Store,
    actor: AuthenticatedActor,
    request: &HeartbeatLeaseRequest,
) -> Result<LeaseContract, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.character_id != actor.character_id {
        return Err(Phase2Error::NotFound);
    }
    let now = store.now();
    store.write_transaction(|state| {
        if !owns(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let lease = state
            .leases
            .get_mut(&request.character_id)
            .ok_or(Phase2Error::Expired)?;
        if lease.released || lease.contract.expires_at.value() <= now {
            return Err(Phase2Error::Expired);
        }
        if !fence_matches(lease.contract, request.fence()) {
            return Err(Phase2Error::Conflict);
        }
        let expires = now.checked_add(LEASE_TTL_MS).ok_or(Phase2Error::Internal)?;
        let grace_until = expires
            .checked_add(RECONNECT_GRACE_MS)
            .ok_or(Phase2Error::Internal)?;
        let contract = LeaseContract::new(
            lease.contract.fence(),
            Store::unix_timestamp(expires)?,
            HEARTBEAT_INTERVAL_MS,
        )
        .map_err(|_| Phase2Error::Internal)?;
        lease.contract = contract;
        lease.grace_until = grace_until;
        Ok(contract)
    })
}

pub(crate) fn reconnect(
    store: &Store,
    actor: AuthenticatedActor,
    request: &ReconnectLeaseRequest,
) -> Result<LeaseContract, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.character_id != actor.character_id {
        return Err(Phase2Error::NotFound);
    }
    let now = store.now();
    store.write_transaction(|state| {
        if !owns(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let (lease_contract, grace_until, released, reconnect, runtime_binding) = {
            let lease = state
                .leases
                .get(&request.character_id)
                .ok_or(Phase2Error::Expired)?;
            (
                lease.contract,
                lease.grace_until,
                lease.released,
                lease.reconnect,
                lease.runtime_binding.clone(),
            )
        };
        if let Some((key, old_fence, rotated)) = reconnect {
            if key == request.idempotency_key {
                return if old_fence == request.fence() {
                    Ok(rotated)
                } else {
                    Err(Phase2Error::Conflict)
                };
            }
            if request.session_epoch == old_fence.session_epoch {
                return Err(Phase2Error::Conflict);
            }
        }
        if released
            || request.session_id != lease_contract.session_id
            || request.session_epoch != lease_contract.session_epoch
            || request.current_revision != lease_contract.current_revision
            || request.client_instance_id != lease_contract.client_instance_id
        {
            return Err(Phase2Error::Conflict);
        }
        if now < lease_contract.expires_at.value() || now > grace_until {
            return Err(Phase2Error::Expired);
        }
        let (last_epoch, revision) = {
            let character = state
                .characters
                .get(&request.character_id)
                .ok_or(Phase2Error::NotFound)?;
            (character.last_session_epoch, character.revision)
        };
        let next = last_epoch.checked_add(1).ok_or(Phase2Error::Internal)?;
        let epoch = SessionEpoch::new(next).map_err(|_| Phase2Error::Internal)?;
        let old_fence = request.fence();
        let expires = now.checked_add(LEASE_TTL_MS).ok_or(Phase2Error::Internal)?;
        let grace_until = expires
            .checked_add(RECONNECT_GRACE_MS)
            .ok_or(Phase2Error::Internal)?;
        let contract = LeaseContract::new(
            LeaseFence::new(
                lease_contract.session_id,
                request.character_id,
                revision,
                epoch,
                request.client_instance_id,
            ),
            Store::unix_timestamp(expires)?,
            HEARTBEAT_INTERVAL_MS,
        )
        .map_err(|_| Phase2Error::Internal)?;
        let runtime_binding = runtime_binding
            .map(|binding| {
                let catalog = store
                    .config
                    .release_catalog
                    .as_ref()
                    .ok_or(Phase2Error::Authentication)?;
                if binding.session != lease_contract.stable_runtime_session()
                    || catalog.for_snapshot(Some(binding.world_id), Some(binding.world_id))
                        != Ok(&binding.build)
                {
                    return Err(Phase2Error::Authentication);
                }
                Ok(super::storage::RuntimeWorldBinding {
                    world_id: binding.world_id,
                    build: binding.build,
                    session: contract.stable_runtime_session(),
                })
            })
            .transpose()?;
        if let Some(character) = state.characters.get_mut(&request.character_id) {
            character.last_session_epoch = next;
        }
        let lease = state
            .leases
            .get_mut(&request.character_id)
            .ok_or(Phase2Error::Expired)?;
        lease.contract = contract;
        lease.grace_until = grace_until;
        lease.reconnect = Some((request.idempotency_key, old_fence, contract));
        lease.runtime_binding = runtime_binding;
        Ok(contract)
    })
}

pub(crate) fn release(
    store: &Store,
    actor: AuthenticatedActor,
    request: &ReleaseLeaseRequest,
) -> Result<LogoutResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.character_id != actor.character_id {
        return Err(Phase2Error::NotFound);
    }
    let now = store.now();
    store.write_transaction(|state| {
        if !owns(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let lease = state
            .leases
            .get(&request.character_id)
            .ok_or(Phase2Error::Expired)?
            .clone();
        let request_fence = LeaseFence::new(
            request.session_id,
            request.character_id,
            request.current_revision,
            request.session_epoch,
            request.client_instance_id,
        );
        if let Some((_, known_fence)) = lease
            .release_keys
            .iter()
            .find(|(key, _)| *key == request.idempotency_key)
        {
            return if *known_fence == request_fence {
                Ok(LogoutResponse::default())
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if lease.contract.expires_at.value() <= now {
            return Err(Phase2Error::Expired);
        }
        if lease.released {
            return Err(Phase2Error::Conflict);
        }
        if !fence_matches(lease.contract, request_fence) {
            return Err(Phase2Error::Conflict);
        }
        if lease.release_keys.len() >= MAX_RELEASE_KEYS {
            return Err(Phase2Error::Busy);
        }
        let lease = state
            .leases
            .get_mut(&request.character_id)
            .ok_or(Phase2Error::Expired)?;
        lease.released = true;
        lease.runtime_binding = None;
        lease
            .release_keys
            .push((request.idempotency_key, request_fence));
        super::group_travel::cancel_pending_for_member(state, actor.character_id);
        Ok(LogoutResponse::default())
    })
}

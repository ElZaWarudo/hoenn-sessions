//! Atomic server-issued lease transitions.

use super::storage::{
    ACQUIRE_IDEMPOTENCY_TTL_MS, AcquireRecord, HEARTBEAT_INTERVAL_MS, LEASE_TTL_MS, LeaseRecord,
    MAX_ACQUIRE_HISTORY, MAX_RELEASE_KEYS, RECONNECT_GRACE_MS, Store,
};
use super::{AuthenticatedActor, Phase2Error};
use coop_cloud::{
    AcquireLeaseRequest, HeartbeatLeaseRequest, LeaseContract, LeaseFence, LogoutResponse,
    ReconnectLeaseRequest, ReleaseLeaseRequest, SessionEpoch,
};

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
            {
                return Ok(record.contract);
            }
            return Err(Phase2Error::Conflict);
        }
        if state
            .leases
            .get(&request.character_id)
            .is_some_and(|existing| !existing.released && existing.grace_until > now)
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
        let (lease_contract, grace_until, released, reconnect) = {
            let lease = state
                .leases
                .get(&request.character_id)
                .ok_or(Phase2Error::Expired)?;
            (
                lease.contract,
                lease.grace_until,
                lease.released,
                lease.reconnect,
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
        lease
            .release_keys
            .push((request.idempotency_key, request_fence));
        Ok(LogoutResponse::default())
    })
}

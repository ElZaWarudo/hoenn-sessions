//! Atomic server-issued lease transitions.

use super::storage::{
    ACQUIRE_IDEMPOTENCY_TTL_MS, AcquireRecord, GroupEndNoticeRecord, GroupStatus,
    HEARTBEAT_INTERVAL_MS, LEASE_TTL_MS, LeaseRecord, MAX_ACQUIRE_HISTORY, MAX_RELEASE_KEYS,
    RECONNECT_GRACE_MS, Store,
};
use super::{AuthenticatedActor, Phase2Error};
use coop_cloud::{
    AcquireLeaseRequest, CharacterId, GroupId, HeartbeatLeaseRequest, LeaseContract, LeaseFence,
    LogoutResponse, ReconnectLeaseRequest, ReleaseLeaseRequest, SessionEpoch,
};

/// A group closed because one member's reconnect window ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpiredGroup {
    pub group_id: GroupId,
    pub expired_member: CharacterId,
    pub partner: CharacterId,
    pub partner_session: Option<coop_cloud::StableRuntimeSession>,
}

/// Close expired groups in one repository transaction. The returned events can
/// be delivered after the transaction commits, without holding the store lock.
pub(crate) fn expire_groups(store: &Store) -> Result<Vec<ExpiredGroup>, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        let expired: Vec<_> = state
            .groups
            .iter()
            .filter_map(|(group_id, record)| {
                if record.status != GroupStatus::Active {
                    return None;
                }
                let members = record.group.members();
                let expired_member = members.iter().copied().find(|member| {
                    state
                        .leases
                        .get(member)
                        .is_none_or(|lease| lease.released || lease.grace_until < now)
                })?;
                let partner = if members[0] == expired_member {
                    members[1]
                } else {
                    members[0]
                };
                Some(ExpiredGroup {
                    group_id: *group_id,
                    expired_member,
                    partner,
                    partner_session: state.leases.get(&partner).and_then(|lease| {
                        (!lease.released && lease.grace_until >= now)
                            .then(|| lease.contract.stable_runtime_session())
                    }),
                })
            })
            .collect();
        for event in &expired {
            super::group_travel::cancel_pending_for_member(state, event.expired_member);
            state
                .groups
                .get_mut(&event.group_id)
                .expect("selected group exists")
                .status = GroupStatus::Closed;
            let members = [event.expired_member, event.partner];
            for member in members {
                state.active_group_by_member.remove(&member);
            }
            if let Some(session) = event.partner_session {
                state.group_end_notices.insert(
                    event.partner,
                    GroupEndNoticeRecord {
                        group_id: event.group_id,
                        session,
                    },
                );
            }
        }
        state.group_end_notices.retain(|character_id, notice| {
            state.leases.get(character_id).is_some_and(|lease| {
                !lease.released
                    && lease.grace_until >= now
                    && lease.contract.stable_runtime_session() == notice.session
            })
        });
        // The member cancellation above runs before the group is marked
        // closed so that logout/session replacement cannot discard active
        // scene recovery. Reconcile after all closures are durable; this is
        // the safe point at which AwaitingSceneReceipts/Suspended proposals
        // may release their live indexes while retaining their evidence.
        super::group_travel::prune_group_state(state, now);
        super::storage::prune_closed_progress_feeds(state);
        Ok(expired)
    })
}

/// Replay one durably committed closure to the surviving lease, including its
/// fenced reconnect, while the character has not joined another group.
pub(crate) fn pending_group_end_for_session(
    store: &Store,
    session: coop_cloud::StableRuntimeSession,
) -> Result<Option<GroupId>, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    let now = store.now();
    store.read_transaction(|state| {
        let Some(lease) = state.leases.get(&session.character_id) else {
            return Ok(None);
        };
        if lease.released
            || lease.contract.expires_at.value() <= now
            || lease.contract.stable_runtime_session() != session
            || state
                .active_group_by_member
                .contains_key(&session.character_id)
        {
            return Ok(None);
        }
        Ok(state
            .group_end_notices
            .get(&session.character_id)
            .filter(|notice| notice.session == session)
            .map(|notice| notice.group_id))
    })
}

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
        let previous_session = state
            .leases
            .get(&request.character_id)
            .filter(|lease| !lease.released && lease.grace_until >= now)
            .map(|lease| lease.contract.stable_runtime_session());
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
        if let Some(notice) = state.group_end_notices.get_mut(&request.character_id) {
            if previous_session == Some(notice.session) {
                notice.session = contract.stable_runtime_session();
            }
        }
        state.acquire_history.insert(
            request.idempotency_key,
            AcquireRecord {
                character_id: request.character_id,
                client_instance_id: request.client_instance_id,
                contract,
                expires_at: history_expires,
            },
        );
        state.last_seen_at.insert(request.character_id, now);
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
        state.last_seen_at.insert(request.character_id, now);
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
        if let Some(notice) = state.group_end_notices.get_mut(&request.character_id) {
            if notice.session == lease_contract.stable_runtime_session() {
                notice.session = contract.stable_runtime_session();
            }
        }
        state.last_seen_at.insert(request.character_id, now);
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
        super::group_travel::cancel_pending_for_member(state, actor.character_id);
        Ok(LogoutResponse::default())
    })
}

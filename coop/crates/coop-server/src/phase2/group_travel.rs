//! Authenticated, atomic UUID group invitations and regional travel.

use coop_cloud::{
    AcceptGroupInvitationRequest, AcceptGroupInvitationResponse, CharacterId,
    CreateGroupInvitationRequest, CreateGroupInvitationResponse, Group, GroupId, GroupInvitationId,
    GroupInvitationView, GroupTravelRequest, GroupTravelResponse, GroupView, LeaseFence,
    MAX_WORLD_REVISION,
};
use coop_protocol::{RegionId, TravelRoute, WorldZone};
use sha2::{Digest, Sha256};

use super::storage::{
    GROUP_IDEMPOTENCY_TTL_MS, GROUP_INVITATION_TTL_MS, GroupIdempotencyRecord,
    GroupIdempotencyResponse, GroupInvitationRecord, GroupRecord, GroupStatus,
    MAX_GROUP_IDEMPOTENCY, MAX_GROUP_INVITATIONS, Store,
};
use super::{AuthenticatedActor, Phase2Error};

const OP_CREATE: &str = "group_invitation_create_v1";
const OP_ACCEPT: &str = "group_invitation_accept_v1";
const OP_TRAVEL: &str = "group_travel_v1";
const MAX_ID_CANDIDATES: usize = 8;

#[derive(Clone)]
struct RouteDefinition {
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    route: TravelRoute,
}

fn route(
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    badges: u8,
    story: u32,
) -> RouteDefinition {
    let from = source.region;
    let to = destination.region;
    RouteDefinition {
        id,
        source,
        destination,
        route: TravelRoute::new(from, to, badges, story)
            .expect("group travel route constants are valid"),
    }
}

fn route_catalog() -> [RouteDefinition; 4] {
    let slateport =
        || WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY_HARBOR", 1).expect("map catalog");
    let one_island =
        || WorldZone::new(RegionId::Sevii, "ONE_ISLAND_HARBOR", 1).expect("map catalog");
    let vermilion = || WorldZone::new(RegionId::Kanto, "VERMILION_CITY", 1).expect("map catalog");
    [
        route(
            "HOENN:SLATEPORT_SEVII_FERRY",
            slateport(),
            one_island(),
            8,
            0,
        ),
        route(
            "SEVII:ONE_ISLAND_HOENN_FERRY",
            one_island(),
            slateport(),
            0,
            0,
        ),
        route(
            "SEVII:ONE_ISLAND_KANTO_FERRY",
            one_island(),
            vermilion(),
            0,
            0,
        ),
        route(
            "KANTO:VERMILION_SEVII_FERRY",
            vermilion(),
            one_island(),
            0,
            0,
        ),
    ]
}

fn route_definition(id: &str) -> Result<RouteDefinition, Phase2Error> {
    route_catalog()
        .into_iter()
        .find(|route| route.id == id)
        .ok_or(Phase2Error::Forbidden)
}

fn request_fingerprint<T: serde::Serialize>(
    domain: &[u8],
    request: &T,
) -> Result<[u8; 32], Phase2Error> {
    let encoded = serde_json::to_vec(request).map_err(|_| Phase2Error::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

fn path_fingerprint<T: serde::Serialize>(
    domain: &[u8],
    path: &[u8],
    request: &T,
) -> Result<[u8; 32], Phase2Error> {
    let encoded = serde_json::to_vec(request).map_err(|_| Phase2Error::Internal)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(path);
    hasher.update([0]);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

fn authenticate_caller(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    character_id: CharacterId,
) -> Result<(), Phase2Error> {
    let user = state
        .users_by_id
        .get(&actor.user_id)
        .ok_or(Phase2Error::Authentication)?;
    if user.user_id != actor.user_id
        || user.disabled
        || user.character_id != actor.character_id
        || actor.character_id != character_id
    {
        return Err(Phase2Error::Authentication);
    }
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::Authentication)?;
    if character.owner != actor.user_id || character.state.character_id != character_id {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

fn validate_member(
    state: &super::storage::State,
    character_id: CharacterId,
) -> Result<(), Phase2Error> {
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::Forbidden)?;
    let user = state
        .users_by_id
        .get(&character.owner)
        .ok_or(Phase2Error::Forbidden)?;
    if user.user_id != character.owner
        || user.disabled
        || user.character_id != character_id
        || character.state.character_id != character_id
    {
        return Err(Phase2Error::Forbidden);
    }
    Ok(())
}

fn lease_matches(
    state: &super::storage::State,
    character_id: CharacterId,
    fence: LeaseFence,
    now: u64,
) -> Result<(), Phase2Error> {
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Authentication)?;
    if lease.released || lease.contract.expires_at.value() <= now {
        return Err(Phase2Error::Authentication);
    }
    if lease.contract.fence() != fence {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

fn active_member_lease(
    state: &super::storage::State,
    character_id: CharacterId,
    now: u64,
) -> Result<(), Phase2Error> {
    validate_member(state, character_id)?;
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Forbidden)?;
    if lease.contract.character_id != character_id
        || lease.released
        || lease.contract.expires_at.value() <= now
    {
        return Err(Phase2Error::Forbidden);
    }
    Ok(())
}

fn prune_group_state(state: &mut super::storage::State, now: u64) {
    state
        .group_invitations
        .retain(|_, invitation| !invitation.consumed && invitation.expires_at > now);
    state
        .group_idempotency
        .retain(|_, record| record.expires_at > now);
}

fn live_invitation_count(state: &super::storage::State, now: u64) -> usize {
    state
        .group_invitations
        .values()
        .filter(|invitation| !invitation.consumed && invitation.expires_at > now)
        .count()
}

fn live_idempotency_count(state: &super::storage::State, now: u64) -> usize {
    state
        .group_idempotency
        .values()
        .filter(|record| record.expires_at > now)
        .count()
}

fn idempotency_lookup(
    state: &super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
    now: u64,
) -> Result<Option<GroupIdempotencyResponse>, Phase2Error> {
    let Some(record) = state
        .group_idempotency
        .get(&(actor, operation.to_owned(), key))
    else {
        return Ok(None);
    };
    if record.expires_at <= now {
        return Ok(None);
    }
    if record.fingerprint != fingerprint {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(record.response.clone()))
}

fn build_invitation_view(
    record: &GroupInvitationRecord,
) -> Result<GroupInvitationView, Phase2Error> {
    Ok(GroupInvitationView {
        api_version: coop_cloud::ApiVersion::V1,
        invitation_id: record.invitation_id,
        inviter_character_id: record.inviter,
        invitee_character_id: record.invitee,
        expires_at: Store::unix_timestamp(record.expires_at)?,
    })
}

fn invitation_candidates(store: &Store) -> Result<Vec<GroupInvitationId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| GroupInvitationId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
        .collect()
}

fn group_candidates(store: &Store) -> Result<Vec<GroupId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| GroupId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
        .collect()
}

fn group_view(state: &super::storage::State, group_id: GroupId) -> Result<GroupView, Phase2Error> {
    let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if record.status != GroupStatus::Active {
        return Err(Phase2Error::NotFound);
    }
    let members = record.group.members();
    let revisions = [
        state
            .characters
            .get(&members[0])
            .ok_or(Phase2Error::Internal)?
            .world_revision,
        state
            .characters
            .get(&members[1])
            .ok_or(Phase2Error::Internal)?
            .world_revision,
    ];
    GroupView::new(group_id, record.group, record.zone.clone(), revisions)
        .map_err(|_| Phase2Error::Internal)
}

pub(crate) fn create_invitation(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = request_fingerprint(OP_CREATE.as_bytes(), request)?;
    let now = store.now();
    let expires_at = now
        .checked_add(GROUP_INVITATION_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let receipt_expires = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        if let Some(replay) = idempotency_lookup(
            state,
            actor.character_id,
            OP_CREATE,
            request.idempotency_key(),
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Invitation(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        if live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY
            || live_invitation_count(state, now) >= MAX_GROUP_INVITATIONS
        {
            return Err(Phase2Error::Busy);
        }
        let target = state
            .characters
            .get(&request.invitee_character_id)
            .ok_or(Phase2Error::Forbidden)?;
        validate_member(state, request.invitee_character_id)?;
        active_member_lease(state, request.invitee_character_id, now)?;
        let inviter = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if target.state.world_zone != inviter.state.world_zone {
            return Err(Phase2Error::Forbidden);
        }
        if state
            .active_group_by_member
            .contains_key(&actor.character_id)
            || state
                .active_group_by_member
                .contains_key(&request.invitee_character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let invitation_candidates = invitation_candidates(store)?;
        let invitation_id = invitation_candidates
            .iter()
            .copied()
            .find(|candidate| {
                state
                    .group_invitations
                    .get(candidate)
                    .is_none_or(|record| record.consumed || record.expires_at <= now)
            })
            .ok_or(Phase2Error::Conflict)?;
        let invitation = GroupInvitationRecord {
            invitation_id,
            inviter: actor.character_id,
            invitee: request.invitee_character_id,
            expires_at,
            consumed: false,
        };
        let view = build_invitation_view(&invitation)?;
        prune_group_state(state, now);
        state.group_invitations.insert(invitation_id, invitation);
        state.group_idempotency.insert(
            (
                actor.character_id,
                OP_CREATE.to_owned(),
                request.idempotency_key(),
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Invitation(view),
                expires_at: receipt_expires,
            },
        );
        Ok(view)
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the acceptance transaction keeps all validation before its mutation suffix"
)]
pub(crate) fn accept_invitation(
    store: &Store,
    actor: AuthenticatedActor,
    invitation_id: GroupInvitationId,
    request: &AcceptGroupInvitationRequest,
) -> Result<AcceptGroupInvitationResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = path_fingerprint(
        OP_ACCEPT.as_bytes(),
        invitation_id.as_uuid().as_bytes(),
        request,
    )?;
    let now = store.now();
    let receipt_expires = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        if let Some(replay) = idempotency_lookup(
            state,
            actor.character_id,
            OP_ACCEPT,
            request.idempotency_key(),
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Accept(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        if live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY {
            return Err(Phase2Error::Busy);
        }
        let invitation = state
            .group_invitations
            .get(&invitation_id)
            .ok_or(Phase2Error::NotFound)?;
        if invitation.invitee != actor.character_id {
            return Err(Phase2Error::NotFound);
        }
        if invitation.consumed || invitation.expires_at <= now {
            return Err(Phase2Error::Expired);
        }
        let initiator = invitation.inviter;
        let recipient = invitation.invitee;
        if initiator == recipient {
            return Err(Phase2Error::Internal);
        }
        active_member_lease(state, initiator, now)?;
        active_member_lease(state, recipient, now)?;
        let initiator_record = state
            .characters
            .get(&initiator)
            .ok_or(Phase2Error::Internal)?;
        let recipient_record = state
            .characters
            .get(&recipient)
            .ok_or(Phase2Error::Internal)?;
        if initiator_record.state.world_zone != recipient_record.state.world_zone {
            return Err(Phase2Error::Forbidden);
        }
        if state.active_group_by_member.contains_key(&initiator)
            || state.active_group_by_member.contains_key(&recipient)
        {
            return Err(Phase2Error::Conflict);
        }
        let group_candidates = group_candidates(store)?;
        let group_id = group_candidates
            .iter()
            .copied()
            .find(|candidate| !state.groups.contains_key(candidate))
            .ok_or(Phase2Error::Conflict)?;
        let group = Group::new(initiator, recipient).map_err(|_| Phase2Error::Internal)?;
        let zone = initiator_record.state.world_zone.clone();
        let group_members = group.members();
        let world_revisions = [
            if group_members[0] == initiator {
                initiator_record.world_revision
            } else {
                recipient_record.world_revision
            },
            if group_members[1] == initiator {
                initiator_record.world_revision
            } else {
                recipient_record.world_revision
            },
        ];
        let view = GroupView::new(group_id, group, zone.clone(), world_revisions)
            .map_err(|_| Phase2Error::Internal)?;
        let response = AcceptGroupInvitationResponse {
            api_version: coop_cloud::ApiVersion::V1,
            group: view,
        };
        if !state.group_invitations.contains_key(&invitation_id) {
            return Err(Phase2Error::Internal);
        }
        prune_group_state(state, now);
        state
            .group_invitations
            .get_mut(&invitation_id)
            .expect("validated invitation exists")
            .consumed = true;
        state.groups.insert(
            group_id,
            GroupRecord {
                group,
                zone,
                status: GroupStatus::Active,
            },
        );
        state.active_group_by_member.insert(initiator, group_id);
        state.active_group_by_member.insert(recipient, group_id);
        state.group_idempotency.insert(
            (
                actor.character_id,
                OP_ACCEPT.to_owned(),
                request.idempotency_key(),
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Accept(response.clone()),
                expires_at: receipt_expires,
            },
        );
        Ok(response)
    })
}

pub(crate) fn inspect_group(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<GroupView, Phase2Error> {
    let now = store.now();
    store.read_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
        if record.status != GroupStatus::Active || !record.group.contains(actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let members = record.group.members();
        if state.active_group_by_member.get(&members[0]) != Some(&group_id)
            || state.active_group_by_member.get(&members[1]) != Some(&group_id)
        {
            return Err(Phase2Error::Internal);
        }
        validate_member(state, members[0])?;
        validate_member(state, members[1])?;
        lease_matches(state, actor.character_id, fence, now)?;
        group_view(state, group_id)
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the travel transaction keeps all validation before its mutation suffix"
)]
pub(crate) fn travel(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    request: &GroupTravelRequest,
) -> Result<GroupTravelResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint =
        path_fingerprint(OP_TRAVEL.as_bytes(), group_id.as_uuid().as_bytes(), request)?;
    let now = store.now();
    let receipt_expires = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        if let Some(replay) = idempotency_lookup(
            state,
            actor.character_id,
            OP_TRAVEL,
            request.idempotency_key(),
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Travel(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        let definition = route_definition(request.route_id())?;
        let destination = definition.destination.clone();
        let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
        if record.status != GroupStatus::Active || !record.group.contains(actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let members = record.group.members();
        if state.active_group_by_member.get(&members[0]) != Some(&group_id)
            || state.active_group_by_member.get(&members[1]) != Some(&group_id)
        {
            return Err(Phase2Error::Internal);
        }
        if record.zone != definition.source {
            return Err(Phase2Error::Forbidden);
        }
        let first = state
            .characters
            .get(&members[0])
            .ok_or(Phase2Error::Internal)?;
        let second = state
            .characters
            .get(&members[1])
            .ok_or(Phase2Error::Internal)?;
        if first.state.world_zone != definition.source
            || second.state.world_zone != definition.source
        {
            return Err(Phase2Error::Forbidden);
        }
        active_member_lease(state, members[0], now)?;
        active_member_lease(state, members[1], now)?;
        let participants = [first, second];
        let mut revisions = [0_u64; 2];
        for (index, character) in participants.into_iter().enumerate() {
            let progress = character
                .state
                .progress_for(definition.route.from())
                .ok_or(Phase2Error::Forbidden)?;
            if progress.badge_count() < definition.route.minimum_badges()
                || progress.story_checkpoint < definition.route.minimum_story_checkpoint()
                || character
                    .state
                    .progress_for(definition.route.to())
                    .is_none()
            {
                return Err(Phase2Error::Forbidden);
            }
            if character.world_revision >= MAX_WORLD_REVISION {
                return Err(Phase2Error::Internal);
            }
            revisions[index] = character
                .world_revision
                .checked_add(1)
                .ok_or(Phase2Error::Internal)?;
        }
        let group_view = GroupView::new(group_id, record.group, destination.clone(), revisions)
            .map_err(|_| Phase2Error::Internal)?;
        let response = GroupTravelResponse {
            api_version: coop_cloud::ApiVersion::V1,
            group: group_view,
        };
        if live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY {
            return Err(Phase2Error::Busy);
        }
        // Every fallible operation is above this point.  This suffix is the
        // single atomic mutation of characters, group, and receipt.
        if !state.groups.contains_key(&group_id)
            || !state.characters.contains_key(&members[0])
            || !state.characters.contains_key(&members[1])
        {
            return Err(Phase2Error::Internal);
        }
        prune_group_state(state, now);
        for (index, character_id) in members.into_iter().enumerate() {
            let character = state
                .characters
                .get_mut(&character_id)
                .expect("validated group member exists");
            character.state.world_zone = destination.clone();
            character.world_revision = revisions[index];
        }
        state
            .groups
            .get_mut(&group_id)
            .expect("validated group exists")
            .zone = destination;
        state.group_idempotency.insert(
            (
                actor.character_id,
                OP_TRAVEL.to_owned(),
                request.idempotency_key(),
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Travel(response.clone()),
                expires_at: receipt_expires,
            },
        );
        Ok(response)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::CharacterCloudState;
    use coop_cloud::{
        AcquireLeaseRequest, ClientInstanceId, CreateGroupInvitationRequest, IdempotencyKey,
        InvitationCode, Password, RegisterRequest,
    };
    use coop_protocol::{RegionalProgress, WorldZone};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use uuid::Uuid;

    #[derive(Clone)]
    struct ToggleRepository {
        inner: super::super::super::phase2::InMemoryRepository,
        fail_writes: Arc<AtomicBool>,
    }

    impl super::super::super::phase2::Repository for ToggleRepository {
        fn read_transaction(
            &self,
            operation: &mut dyn FnMut(
                &super::super::super::phase2::storage::State,
            ) -> Result<
                (),
                super::super::super::phase2::storage::StorageError,
            >,
        ) -> Result<(), super::super::super::phase2::storage::StorageError> {
            self.inner.read_transaction(operation)
        }

        fn write_transaction(
            &self,
            operation: &mut dyn FnMut(
                &mut super::super::super::phase2::storage::State,
            ) -> Result<
                (),
                super::super::super::phase2::storage::StorageError,
            >,
        ) -> Result<(), super::super::super::phase2::storage::StorageError> {
            if self.fail_writes.load(Ordering::Acquire) {
                return Err(super::super::super::phase2::storage::StorageError::Transaction);
            }
            self.inner.write_transaction(operation)
        }
    }

    fn app_with_toggle_repository() -> (super::super::Phase2App, Arc<AtomicBool>) {
        let fail_writes = Arc::new(AtomicBool::new(false));
        let repository = ToggleRepository {
            inner: super::super::super::phase2::InMemoryRepository::new(),
            fail_writes: fail_writes.clone(),
        };
        let config = super::super::Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(super::super::FixedClock::new(1_700_000_000_000)),
            Arc::new(super::super::FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            super::super::ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ))
        .with_adapters(
            Arc::new(repository),
            Arc::new(super::super::InMemoryObjectStore::new()),
        );
        (
            super::super::Phase2App::new(config).expect("test config is local"),
            fail_writes,
        )
    }

    fn account(
        app: &super::super::Phase2App,
        name: &str,
        invitation: &str,
    ) -> (AuthenticatedActor, coop_cloud::LeaseContract) {
        app.add_invitation(invitation).expect("invite");
        let registration = app
            .register(
                RegisterRequest::new(
                    name,
                    Password::new("correct horse battery staple").expect("password"),
                    InvitationCode::new(invitation).expect("invitation"),
                )
                .expect("register"),
            )
            .expect("registered");
        let actor = AuthenticatedActor {
            user_id: registration.user_id,
            character_id: registration.character_id,
        };
        let client = ClientInstanceId::new(Uuid::new_v4()).expect("client");
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(
                    registration.character_id,
                    client,
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
            .expect("lease");
        (actor, lease)
    }

    fn two_member_group(
        app: &super::super::Phase2App,
    ) -> (
        AuthenticatedActor,
        coop_cloud::LeaseContract,
        AuthenticatedActor,
        coop_cloud::LeaseContract,
        coop_cloud::GroupId,
    ) {
        let (first_actor, first_lease) = account(app, "first", "first-invite");
        let (second_actor, second_lease) = account(app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("group invitation");
        let accepted = accept_invitation(
            &app.store,
            second_actor,
            invitation.invitation_id,
            &AcceptGroupInvitationRequest::new(
                second_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("accepted");
        (
            first_actor,
            first_lease,
            second_actor,
            second_lease,
            accepted.group.group_id,
        )
    }

    fn set_progress(
        app: &super::super::Phase2App,
        actor: AuthenticatedActor,
        badges: u16,
        include_destination: bool,
    ) {
        let source = WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY_HARBOR", 1).expect("source");
        let mut records =
            vec![RegionalProgress::new(RegionId::Hoenn, badges, 0, vec![], vec![]).expect("hoenn")];
        if include_destination {
            records
                .push(RegionalProgress::new(RegionId::Sevii, 0, 0, vec![], vec![]).expect("sevii"));
            records.push(
                RegionalProgress::new(RegionId::Kanto, 0xff, 99, vec![], vec![]).expect("kanto"),
            );
        }
        app.store
            .write_transaction(|state| {
                let character = state
                    .characters
                    .get_mut(&actor.character_id)
                    .ok_or(super::super::storage::StorageError::Transaction)?;
                character.state =
                    CharacterCloudState::new(actor.character_id, source.clone(), records.clone())
                        .map_err(|_| super::super::storage::StorageError::Transaction)?;
                if let Some(group_id) = state
                    .active_group_by_member
                    .get(&actor.character_id)
                    .copied()
                {
                    state.groups.get_mut(&group_id).expect("active group").zone = source;
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("state");
    }

    #[test]
    fn catalog_has_only_pinned_maps_and_no_johto_route() {
        let routes = route_catalog();
        assert_eq!(routes.len(), 4);
        assert!(
            routes
                .iter()
                .all(|route| route.source.map_entry().is_ok()
                    && route.destination.map_entry().is_ok())
        );
        assert!(route_definition("JOHTO:TO_KANTO").is_err());
    }

    #[test]
    fn create_prunes_expired_state_before_admission() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, _second_lease) = account(&app, "second", "second-invite");
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                for index in 0..16_u128 {
                    let invitation_id = GroupInvitationId::new(Uuid::from_u128(u128::MAX - index))
                        .map_err(|_| Phase2Error::Internal)?;
                    state.group_invitations.insert(
                        invitation_id,
                        GroupInvitationRecord {
                            invitation_id,
                            inviter: first_actor.character_id,
                            invitee: second_actor.character_id,
                            expires_at: now - 1,
                            consumed: false,
                        },
                    );
                    let key = coop_cloud::IdempotencyKey::new(Uuid::from_u128(index + 1))
                        .map_err(|_| Phase2Error::Internal)?;
                    state.group_idempotency.insert(
                        (first_actor.character_id, "expired".to_owned(), key),
                        GroupIdempotencyRecord {
                            fingerprint: [0; 32],
                            response: GroupIdempotencyResponse::Invitation(GroupInvitationView {
                                api_version: coop_cloud::ApiVersion::V1,
                                invitation_id,
                                inviter_character_id: first_actor.character_id,
                                invitee_character_id: second_actor.character_id,
                                expires_at: coop_cloud::UnixTimestampMillis::new(now - 1),
                            }),
                            expires_at: now - 1,
                        },
                    );
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("expired state");
        let request = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            second_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        create_invitation(&app.store, first_actor, &request).expect("create after prune");
        let lengths = app
            .store
            .inspect_state(|state| (state.group_invitations.len(), state.group_idempotency.len()))
            .expect("state");
        assert_eq!(lengths, (1, 1));
    }

    #[test]
    fn concurrent_acceptance_has_one_committed_group() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, second_lease) = account(&app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("group invitation");
        let request_one = AcceptGroupInvitationRequest::new(
            second_lease.fence(),
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let request_two = AcceptGroupInvitationRequest::new(
            second_lease.fence(),
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let app_one = app.clone();
        let app_two = app.clone();
        let (result_one, result_two) = std::thread::scope(|scope| {
            let first = scope.spawn(move || {
                app_one.accept_group_invitation(second_actor, invitation.invitation_id, request_one)
            });
            let second = scope.spawn(move || {
                app_two.accept_group_invitation(second_actor, invitation.invitation_id, request_two)
            });
            (
                first.join().expect("first acceptance"),
                second.join().expect("second acceptance"),
            )
        });
        assert_eq!(
            i32::from(result_one.is_ok()) + i32::from(result_two.is_ok()),
            1
        );
        assert_eq!(
            app.store
                .inspect_state(|state| (state.groups.len(), state.active_group_by_member.len()))
                .expect("state"),
            (1, 2)
        );
    }

    #[test]
    fn caller_identity_is_revalidated_for_every_group_operation() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease, _second_actor, _second_lease, group_id) =
            two_member_group(&app);
        let (third_actor, _third_lease) = account(&app, "third", "third-invite");
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("caller")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable caller");

        let create = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            third_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Authentication)
        );
        let travel_request = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel request");
        assert_eq!(
            travel(&app.store, first_actor, group_id, &travel_request),
            Err(Phase2Error::Authentication)
        );

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("caller")
                    .disabled = false;
                state
                    .characters
                    .get_mut(&first_actor.character_id)
                    .expect("caller character")
                    .state
                    .character_id = third_actor.character_id;
                Ok::<(), Phase2Error>(())
            })
            .expect("inconsistent caller");
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Authentication)
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn companion_identity_and_lease_failures_are_policy_denials() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, _second_lease) = account(&app, "second", "second-invite");
        let (third_actor, _third_lease) = account(&app, "third", "third-invite");

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&second_actor.user_id)
                    .expect("target")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable target");
        let create = CreateGroupInvitationRequest::new(
            first_lease.fence(),
            second_actor.character_id,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&second_actor.user_id)
                    .expect("target")
                    .disabled = false;
                state
                    .characters
                    .get_mut(&second_actor.character_id)
                    .expect("target character")
                    .state
                    .character_id = third_actor.character_id;
                Ok::<(), Phase2Error>(())
            })
            .expect("inconsistent target");
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&second_actor.character_id)
                    .expect("target character")
                    .state
                    .character_id = second_actor.character_id;
                state
                    .leases
                    .get_mut(&second_actor.character_id)
                    .expect("target lease")
                    .released = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("inactive target lease");
        assert_eq!(
            create_invitation(&app.store, first_actor, &create),
            Err(Phase2Error::Forbidden)
        );

        // Rebuild the target lease through a fresh app so acceptance reaches
        // the companion checks without depending on lease internals.
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease) = account(&app, "first", "first-invite");
        let (second_actor, second_lease) = account(&app, "second", "second-invite");
        let invitation = create_invitation(
            &app.store,
            first_actor,
            &CreateGroupInvitationRequest::new(
                first_lease.fence(),
                second_actor.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("invitation");
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("companion")
                    .disabled = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("disable companion");
        assert_eq!(
            accept_invitation(
                &app.store,
                second_actor,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    second_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::Forbidden)
        );
        app.store
            .write_transaction(|state| {
                state
                    .users_by_id
                    .get_mut(&first_actor.user_id)
                    .expect("companion")
                    .disabled = false;
                state
                    .leases
                    .get_mut(&first_actor.character_id)
                    .expect("companion lease")
                    .released = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("release companion");
        assert_eq!(
            accept_invitation(
                &app.store,
                second_actor,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    second_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    fn foreign_invitation_acceptance_is_hidden_across_terminal_states() {
        let app = super::super::Phase2App::test();
        let (sender, sender_lease) = account(&app, "inviter", "inviter-invite");
        let (recipient, _recipient_lease) = account(&app, "invitee", "invitee-invite");
        let (foreign, foreign_lease) = account(&app, "foreign", "foreign-invite");
        let invitation = create_invitation(
            &app.store,
            sender,
            &CreateGroupInvitationRequest::new(
                sender_lease.fence(),
                recipient.character_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("invitation");
        let accept = || {
            accept_invitation(
                &app.store,
                foreign,
                invitation.invitation_id,
                &AcceptGroupInvitationRequest::new(
                    foreign_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            )
        };
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state
                    .group_invitations
                    .get_mut(&invitation.invitation_id)
                    .expect("invitation")
                    .expires_at = now - 1;
                Ok::<(), Phase2Error>(())
            })
            .expect("expire invitation");
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        app.store
            .write_transaction(|state| {
                state
                    .group_invitations
                    .get_mut(&invitation.invitation_id)
                    .expect("invitation")
                    .consumed = true;
                Ok::<(), Phase2Error>(())
            })
            .expect("consume invitation");
        assert_eq!(accept(), Err(Phase2Error::NotFound));
        let missing = GroupInvitationId::new(Uuid::new_v4()).expect("missing");
        assert_eq!(
            accept_invitation(
                &app.store,
                foreign,
                missing,
                &AcceptGroupInvitationRequest::new(
                    foreign_lease.fence(),
                    IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                ),
            ),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn inspection_fails_closed_when_reverse_membership_index_is_inconsistent() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease, second_actor, _second_lease, group_id) =
            two_member_group(&app);
        app.store
            .write_transaction(|state| {
                state
                    .active_group_by_member
                    .remove(&second_actor.character_id);
                Ok::<(), Phase2Error>(())
            })
            .expect("remove reverse index");
        assert_eq!(
            inspect_group(&app.store, first_actor, group_id, first_lease.fence()),
            Err(Phase2Error::Internal)
        );
    }

    #[test]
    fn injected_repository_failure_preserves_group_state() {
        let (app, fail_writes) = app_with_toggle_repository();
        let (first_actor, first_lease, second_actor, _second_lease, group_id) =
            two_member_group(&app);
        set_progress(&app, first_actor, 0xff, true);
        set_progress(&app, second_actor, 0xff, true);
        let before = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&first_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&second_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&first_actor.character_id].world_revision,
                    state.characters[&second_actor.character_id].world_revision,
                    state.groups[&group_id].zone.clone(),
                    state.active_group_by_member.clone(),
                    state.group_invitations.len(),
                    state.group_idempotency.len(),
                )
            })
            .expect("state");
        fail_writes.store(true, Ordering::Release);
        let request = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel");
        assert_eq!(
            app.travel_group(first_actor, group_id, request),
            Err(Phase2Error::Internal)
        );
        let after = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&first_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&second_actor.character_id]
                        .state
                        .world_zone
                        .clone(),
                    state.characters[&first_actor.character_id].world_revision,
                    state.characters[&second_actor.character_id].world_revision,
                    state.groups[&group_id].zone.clone(),
                    state.active_group_by_member.clone(),
                    state.group_invitations.len(),
                    state.group_idempotency.len(),
                )
            })
            .expect("state");
        assert_eq!(after, before);
    }

    #[test]
    fn travel_requires_source_progress_and_destination_entitlement() {
        let app = super::super::Phase2App::test();
        let (first_actor, first_lease, second_actor, _second_lease, group_id) =
            two_member_group(&app);
        set_progress(&app, first_actor, 0xff, false);
        set_progress(&app, second_actor, 0xff, false);
        let denied = travel(
            &app.store,
            first_actor,
            group_id,
            &GroupTravelRequest::new(
                first_lease.fence(),
                "HOENN:SLATEPORT_SEVII_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("travel"),
        );
        assert_eq!(denied, Err(Phase2Error::Forbidden));
        set_progress(&app, first_actor, 0, true);
        set_progress(&app, second_actor, 0, true);
        let denied_unrelated = travel(
            &app.store,
            first_actor,
            group_id,
            &GroupTravelRequest::new(
                first_lease.fence(),
                "HOENN:SLATEPORT_SEVII_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("travel"),
        );
        assert_eq!(denied_unrelated, Err(Phase2Error::Forbidden));
        set_progress(&app, first_actor, 0xff00, true);
        set_progress(&app, second_actor, 0xff00, true);
        let denied_reserved_badges = travel(
            &app.store,
            first_actor,
            group_id,
            &GroupTravelRequest::new(
                first_lease.fence(),
                "HOENN:SLATEPORT_SEVII_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("travel"),
        );
        assert_eq!(denied_reserved_badges, Err(Phase2Error::Forbidden));
        set_progress(&app, first_actor, 0xff, true);
        set_progress(&app, second_actor, 0xff, true);
        let request = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel");
        let moved = travel(&app.store, first_actor, group_id, &request).expect("travel success");
        assert_eq!(moved.group.members[0].world_revision, 1);
        assert_eq!(moved.group.members[1].world_revision, 1);
        let replay = travel(&app.store, first_actor, group_id, &request).expect("replay");
        assert_eq!(replay, moved);
        let changed = GroupTravelRequest::new(
            first_lease.fence(),
            "SEVII:ONE_ISLAND_HOENN_FERRY",
            request.idempotency_key(),
        )
        .expect("changed");
        assert_eq!(
            travel(&app.store, first_actor, group_id, &changed),
            Err(Phase2Error::Conflict)
        );
        let state = app
            .store
            .inspect_state(|state| {
                let first = &state.characters[&first_actor.character_id];
                let second = &state.characters[&second_actor.character_id];
                (
                    first.revision,
                    second.revision,
                    first.world_revision,
                    second.world_revision,
                    first.state.world_zone.clone(),
                )
            })
            .expect("state");
        assert_eq!(state.0, coop_cloud::Revision::initial());
        assert_eq!(state.1, coop_cloud::Revision::initial());
        assert_eq!((state.2, state.3), (1, 1));
        assert_eq!(state.4.region, RegionId::Sevii);
    }
}

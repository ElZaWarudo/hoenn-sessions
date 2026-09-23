//! Authenticated, atomic UUID group invitations and regional travel.

use coop_cloud::{
    AcceptGroupInvitationRequest, AcceptGroupInvitationResponse, CharacterId,
    CreateGroupInvitationRequest, CreateGroupInvitationResponse, Group, GroupId, GroupInvitationId,
    GroupInvitationView, GroupMemberView, GroupTravelAction, GroupTravelActionRequest,
    GroupTravelCommit, GroupTravelProposalId, GroupTravelProposalRequest,
    GroupTravelProposalStatus, GroupTravelProposalView, GroupTravelRequest, GroupTravelResponse,
    GroupView, LeaseFence, MAX_WORLD_REVISION,
};
use coop_protocol::{GroupTravelDeparture, RegionId, WorldZone};
use sha2::{Digest, Sha256};

use super::storage::{
    GROUP_IDEMPOTENCY_TTL_MS, GROUP_INVITATION_TTL_MS, GroupIdempotencyRecord,
    GroupIdempotencyResponse, GroupInvitationRecord, GroupRecord, GroupStatus,
    GroupTravelProposalIdempotencyRecord, GroupTravelProposalRecord, MAX_GROUP_IDEMPOTENCY,
    MAX_GROUP_INVITATIONS, Store,
};
use super::{AuthenticatedActor, Phase2Error};

const OP_CREATE: &str = "group_invitation_create_v1";
const OP_ACCEPT: &str = "group_invitation_accept_v1";
const OP_TRAVEL: &str = "group_travel_v1";
const OP_PROPOSE_TRAVEL: &str = "group_travel_propose_v1";
const OP_TRAVEL_ACTION: &str = "group_travel_action_v1";
const MAX_ID_CANDIDATES: usize = 8;
const MAX_TRAVEL_PROPOSALS: usize = 4_096;
const MAX_TRAVEL_CREATE_RECEIPTS: usize = 1_024;
const MAX_TRAVEL_LIFECYCLE_RECEIPTS: usize = MAX_TRAVEL_PROPOSALS - MAX_TRAVEL_CREATE_RECEIPTS;
const TRAVEL_PROPOSAL_REPLAY_TTL_MS: u64 = GROUP_IDEMPOTENCY_TTL_MS;

#[derive(Clone)]
struct RouteDefinition {
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    minimum_badges: u8,
    minimum_story_checkpoint: u32,
}

fn route(
    id: &'static str,
    source: WorldZone,
    destination: WorldZone,
    badges: u8,
    story: u32,
) -> RouteDefinition {
    RouteDefinition {
        id,
        source,
        destination,
        minimum_badges: badges,
        minimum_story_checkpoint: story,
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

fn consent_route_catalog() -> [RouteDefinition; 6] {
    let goldenrod =
        || WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("map catalog");
    let olivine =
        || WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("map catalog");
    let reception = || WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("map catalog");
    let original_train = || {
        WorldZone::new(
            RegionId::Kanto,
            "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
            1,
        )
        .expect("map catalog")
    };
    let later_train = || {
        WorldZone::new(RegionId::Kanto, "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION", 1)
            .expect("map catalog")
    };
    let original_ferry = || {
        WorldZone::new(
            RegionId::Kanto,
            "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
            1,
        )
        .expect("map catalog")
    };
    let later_ferry = || {
        WorldZone::new(RegionId::Kanto, "KANTO_LATER_VERMILION_CITY_PORT_INSIDE", 1)
            .expect("map catalog")
    };
    let original_route22 = || WorldZone::new(RegionId::Kanto, "ROUTE22", 1).expect("map catalog");
    let later_route22 =
        || WorldZone::new(RegionId::Kanto, "KANTO_LATER_ROUTE22", 1).expect("map catalog");
    [
        route(
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            goldenrod(),
            original_train(),
            0,
            0,
        ),
        route(
            "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
            goldenrod(),
            later_train(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
            olivine(),
            original_ferry(),
            0,
            0,
        ),
        route(
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            olivine(),
            later_ferry(),
            0,
            0,
        ),
        route(
            "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
            reception(),
            original_route22(),
            0,
            0,
        ),
        route(
            "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
            reception(),
            later_route22(),
            0,
            0,
        ),
    ]
}

fn consent_route_definition(id: &str) -> Result<RouteDefinition, Phase2Error> {
    consent_route_catalog()
        .into_iter()
        .find(|route| route.id == id)
        .ok_or(Phase2Error::Forbidden)
}

fn departure_matches_route(route: &RouteDefinition, departure: GroupTravelDeparture) -> bool {
    if route.id.ends_with("_TRAIN") {
        departure == GroupTravelDeparture::Train
    } else if route.id.ends_with("_FERRY") {
        matches!(
            departure,
            GroupTravelDeparture::Ferry | GroupTravelDeparture::SsaquaMaiden
        )
    } else if route.id.ends_with("_ROUTE22") {
        departure == GroupTravelDeparture::Gate
    } else {
        false
    }
}

pub(super) fn request_fingerprint<T: serde::Serialize>(
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

pub(super) fn authenticate_caller(
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

pub(super) fn validate_member(
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

pub(super) fn lease_matches(
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

pub(super) fn prune_group_state(state: &mut super::storage::State, now: u64) {
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

pub(super) fn live_idempotency_count(state: &super::storage::State, now: u64) -> usize {
    state
        .group_idempotency
        .values()
        .filter(|record| record.expires_at > now)
        .count()
}

pub(super) fn idempotency_lookup(
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

pub(super) fn build_invitation_view(
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

pub(super) fn group_view(
    state: &super::storage::State,
    group_id: GroupId,
) -> Result<GroupView, Phase2Error> {
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
    create_invitation_with_fingerprint(store, actor, request, OP_CREATE, fingerprint)
}

pub(super) fn create_invitation_with_fingerprint(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreateGroupInvitationRequest,
    operation: &str,
    fingerprint: [u8; 32],
) -> Result<CreateGroupInvitationResponse, Phase2Error> {
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
            operation,
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
                operation.to_owned(),
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
                zone_revision: 0,
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
        // The consent proposal flow owns atomic group movement while a
        // proposal is live. Legacy travel must not move the group out from
        // under a pending proposal (or vice versa); the caller retries after
        // the proposal resolves. Expired proposals are pruned first so only
        // live ones fence this path.
        prune_travel_proposal_state(state, now);
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
        if state.live_group_travel_by_group.contains_key(&group_id)
            || members
                .iter()
                .any(|member| state.live_group_travel_by_member.contains_key(member))
        {
            return Err(Phase2Error::Conflict);
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
        let zone_revision = record
            .zone_revision
            .checked_add(1)
            .filter(|revision| *revision <= MAX_WORLD_REVISION)
            .ok_or(Phase2Error::Internal)?;
        for (index, character) in participants.into_iter().enumerate() {
            let progress = character
                .state
                .progress_for(definition.source.region)
                .ok_or(Phase2Error::Forbidden)?;
            if progress.badge_count() < definition.minimum_badges
                || progress.story_checkpoint < definition.minimum_story_checkpoint
                || character
                    .state
                    .progress_for(definition.destination.region)
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
        let group = state
            .groups
            .get_mut(&group_id)
            .expect("validated group exists");
        group.zone = destination;
        group.zone_revision = zone_revision;
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

fn proposal_candidates(store: &Store) -> Result<Vec<GroupTravelProposalId>, Phase2Error> {
    (0..MAX_ID_CANDIDATES)
        .map(|_| {
            GroupTravelProposalId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal)
        })
        .collect()
}

fn release_proposal_indexes(state: &mut super::storage::State, proposal_id: GroupTravelProposalId) {
    state
        .live_group_travel_by_group
        .retain(|_, indexed| *indexed != proposal_id);
    state
        .live_group_travel_by_member
        .retain(|_, indexed| *indexed != proposal_id);
}

fn expire_pending_proposals(state: &mut super::storage::State, now: u64) {
    let expired: Vec<_> = state
        .group_travel_proposals
        .iter()
        .filter_map(|(proposal_id, record)| {
            (record.view.status == GroupTravelProposalStatus::Pending
                && record.view.expires_at.value() <= now)
                .then_some(*proposal_id)
        })
        .collect();
    for proposal_id in expired {
        if let Some(record) = state.group_travel_proposals.get_mut(&proposal_id) {
            record.view.status = GroupTravelProposalStatus::Expired;
            record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
        }
        release_proposal_indexes(state, proposal_id);
    }
}

fn prune_travel_proposal_state(state: &mut super::storage::State, now: u64) {
    expire_pending_proposals(state, now);
    state.group_travel_proposals.retain(|_, record| {
        record
            .retain_until
            .is_none_or(|retain_until| retain_until > now)
    });
    state
        .group_travel_proposal_idempotency
        .retain(|_, record| record.expires_at > now);
}

fn lifecycle_receipt_count(state: &super::storage::State) -> usize {
    state
        .group_travel_proposal_idempotency
        .values()
        .filter(|record| record.lifecycle)
        .count()
}

fn create_receipt_count(state: &super::storage::State) -> usize {
    state
        .group_travel_proposal_idempotency
        .values()
        .filter(|record| !record.lifecycle)
        .count()
}

fn reserved_lifecycle_receipts(state: &super::storage::State) -> usize {
    state
        .group_travel_proposals
        .values()
        .map(|record| match record.view.status {
            GroupTravelProposalStatus::Pending => 3,
            GroupTravelProposalStatus::Committed => 2_usize.saturating_sub(
                record
                    .view
                    .applied_by
                    .iter()
                    .filter(|value| **value)
                    .count(),
            ),
            GroupTravelProposalStatus::Declined
            | GroupTravelProposalStatus::Cancelled
            | GroupTravelProposalStatus::Expired => 0,
        })
        .sum()
}

fn ensure_lifecycle_receipt_slot(state: &super::storage::State) -> Result<(), Phase2Error> {
    if lifecycle_receipt_count(state) < MAX_TRAVEL_LIFECYCLE_RECEIPTS {
        Ok(())
    } else {
        Err(Phase2Error::Busy)
    }
}

pub(super) fn cancel_pending_for_member(
    state: &mut super::storage::State,
    character_id: CharacterId,
) {
    let Some(proposal_id) = state
        .live_group_travel_by_member
        .get(&character_id)
        .copied()
    else {
        return;
    };
    if let Some(record) = state.group_travel_proposals.get_mut(&proposal_id)
        && record.view.status == GroupTravelProposalStatus::Pending
    {
        record.view.status = GroupTravelProposalStatus::Cancelled;
        record.retain_until = Some(
            record
                .view
                .expires_at
                .value()
                .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
        );
        release_proposal_indexes(state, proposal_id);
    }
}

fn proposal_idempotency_lookup(
    state: &super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
) -> Result<Option<GroupTravelProposalView>, Phase2Error> {
    let Some(record) =
        state
            .group_travel_proposal_idempotency
            .get(&(actor, operation.to_owned(), key))
    else {
        return Ok(None);
    };
    if record.fingerprint != fingerprint {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(record.response.clone()))
}

fn insert_proposal_receipt(
    state: &mut super::storage::State,
    actor: CharacterId,
    operation: &str,
    key: coop_cloud::IdempotencyKey,
    fingerprint: [u8; 32],
    response: GroupTravelProposalView,
    now: u64,
) {
    state.group_travel_proposal_idempotency.insert(
        (actor, operation.to_owned(), key),
        GroupTravelProposalIdempotencyRecord {
            fingerprint,
            response,
            expires_at: now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
            lifecycle: operation == OP_TRAVEL_ACTION,
        },
    );
}

fn proposal_group(
    state: &super::storage::State,
    group_id: GroupId,
    actor: CharacterId,
) -> Result<(Group, [CharacterId; 2]), Phase2Error> {
    let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if record.status != GroupStatus::Active || !record.group.contains(actor) {
        return Err(Phase2Error::NotFound);
    }
    let members = record.group.members();
    if members
        .iter()
        .any(|member| state.active_group_by_member.get(member) != Some(&group_id))
    {
        return Err(Phase2Error::Internal);
    }
    Ok((record.group, members))
}

#[allow(
    clippy::too_many_lines,
    reason = "one transaction snapshots every immutable proposal precondition"
)]
pub(crate) fn create_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    request: &GroupTravelProposalRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let fingerprint = path_fingerprint(
        OP_PROPOSE_TRAVEL.as_bytes(),
        group_id.as_uuid().as_bytes(),
        request,
    )?;
    let candidates = proposal_candidates(store)?;
    let now = store.now();
    let expires_at = now
        .checked_add(coop_cloud::GROUP_INVITATION_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_travel_proposal_state(state, now);
        if let Some(replay) = proposal_idempotency_lookup(
            state,
            actor.character_id,
            OP_PROPOSE_TRAVEL,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        if state.group_travel_proposals.len() >= MAX_TRAVEL_PROPOSALS
            || create_receipt_count(state) >= MAX_TRAVEL_CREATE_RECEIPTS
            || lifecycle_receipt_count(state)
                .saturating_add(reserved_lifecycle_receipts(state))
                .saturating_add(3)
                > MAX_TRAVEL_LIFECYCLE_RECEIPTS
        {
            return Err(Phase2Error::Busy);
        }
        let definition = consent_route_definition(request.route_id.as_str())?;
        if !departure_matches_route(&definition, request.departure) {
            return Err(Phase2Error::Forbidden);
        }
        let (group, members) = proposal_group(state, group_id, actor.character_id)?;
        if state.live_group_travel_by_group.contains_key(&group_id)
            || members
                .iter()
                .any(|member| state.live_group_travel_by_member.contains_key(member))
        {
            return Err(Phase2Error::Conflict);
        }
        let group_record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
        if group_record.zone != definition.source {
            return Err(Phase2Error::Forbidden);
        }
        active_member_lease(state, members[0], now)?;
        active_member_lease(state, members[1], now)?;
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
        let proposal_id = candidates
            .iter()
            .copied()
            .find(|candidate| !state.group_travel_proposals.contains_key(candidate))
            .ok_or(Phase2Error::Conflict)?;
        let expected_members = [
            GroupMemberView {
                character_id: members[0],
                world_revision: first.world_revision,
            },
            GroupMemberView {
                character_id: members[1],
                world_revision: second.world_revision,
            },
        ];
        let responder = if members[0] == actor.character_id {
            members[1]
        } else {
            members[0]
        };
        let view = GroupTravelProposalView {
            api_version: coop_cloud::ApiVersion::V1,
            proposal_id,
            group_id,
            requester_character_id: actor.character_id,
            responder_character_id: responder,
            route_id: request.route_id.clone(),
            departure: request.departure,
            source: definition.source,
            destination: definition.destination,
            expected_group_zone_revision: group_record.zone_revision,
            expected_members,
            status: GroupTravelProposalStatus::Pending,
            expires_at: Store::unix_timestamp(expires_at)?,
            commit: None,
            applied_by: [false; 2],
        };
        insert_proposal_receipt(
            state,
            actor.character_id,
            OP_PROPOSE_TRAVEL,
            request.idempotency_key,
            fingerprint,
            view.clone(),
            now,
        );
        state.group_travel_proposals.insert(
            proposal_id,
            GroupTravelProposalRecord {
                view: view.clone(),
                retain_until: None,
            },
        );
        state
            .live_group_travel_by_group
            .insert(group_id, proposal_id);
        for member in group.members() {
            state
                .live_group_travel_by_member
                .insert(member, proposal_id);
        }
        Ok(view)
    })
}

pub(crate) fn current_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        proposal_group(state, group_id, actor.character_id)?;
        prune_travel_proposal_state(state, now);
        let proposal_id = state
            .live_group_travel_by_group
            .get(&group_id)
            .copied()
            .ok_or(Phase2Error::NotFound)?;
        let view = &state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::Internal)?
            .view;
        if view.requester_character_id != actor.character_id
            && view.responder_character_id != actor.character_id
        {
            return Err(Phase2Error::NotFound);
        }
        Ok(view.clone())
    })
}

pub(crate) fn get_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    fence: LeaseFence,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, actor.character_id)?;
        lease_matches(state, actor.character_id, fence, now)?;
        prune_travel_proposal_state(state, now);
        let view = &state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?
            .view;
        if view.group_id != group_id
            || (view.requester_character_id != actor.character_id
                && view.responder_character_id != actor.character_id)
        {
            return Err(Phase2Error::NotFound);
        }
        Ok(view.clone())
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "one transaction validates and commits both members"
)]
pub(crate) fn act_on_travel_proposal(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: &GroupTravelActionRequest,
) -> Result<GroupTravelProposalView, Phase2Error> {
    let mut path = Vec::with_capacity(32);
    path.extend_from_slice(group_id.as_uuid().as_bytes());
    path.extend_from_slice(proposal_id.as_uuid().as_bytes());
    let fingerprint = path_fingerprint(OP_TRAVEL_ACTION.as_bytes(), &path, request)?;
    let now = store.now();
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_travel_proposal_state(state, now);
        if let Some(replay) = proposal_idempotency_lookup(
            state,
            actor.character_id,
            OP_TRAVEL_ACTION,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        let snapshot = state
            .group_travel_proposals
            .get(&proposal_id)
            .ok_or(Phase2Error::NotFound)?
            .view
            .clone();
        if snapshot.group_id != group_id
            || (snapshot.requester_character_id != actor.character_id
                && snapshot.responder_character_id != actor.character_id)
        {
            return Err(Phase2Error::NotFound);
        }
        let mut response = snapshot.clone();
        match request.action {
            GroupTravelAction::Decline => {
                if actor.character_id != snapshot.responder_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.status = GroupTravelProposalStatus::Declined;
                let record = state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists");
                record.view = response.clone();
                record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
                release_proposal_indexes(state, proposal_id);
            }
            GroupTravelAction::Cancel => {
                if actor.character_id != snapshot.requester_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.status = GroupTravelProposalStatus::Cancelled;
                let record = state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists");
                record.view = response.clone();
                record.retain_until = Some(now.saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS));
                release_proposal_indexes(state, proposal_id);
            }
            GroupTravelAction::Applied => {
                if snapshot.status != GroupTravelProposalStatus::Committed {
                    return Err(Phase2Error::Conflict);
                }
                let members = snapshot.expected_members.map(|member| member.character_id);
                let index = members
                    .iter()
                    .position(|member| *member == actor.character_id)
                    .ok_or(Phase2Error::NotFound)?;
                if snapshot.applied_by[index] {
                    return Err(Phase2Error::Conflict);
                }
                ensure_lifecycle_receipt_slot(state)?;
                response.applied_by[index] = true;
                if response.applied_by == [true, true] {
                    state.group_travel_proposals.remove(&proposal_id);
                    release_proposal_indexes(state, proposal_id);
                } else {
                    state
                        .group_travel_proposals
                        .get_mut(&proposal_id)
                        .expect("proposal exists")
                        .view = response.clone();
                }
            }
            GroupTravelAction::Accept => {
                if actor.character_id != snapshot.responder_character_id {
                    return Err(Phase2Error::Forbidden);
                }
                if snapshot.status != GroupTravelProposalStatus::Pending {
                    return Err(Phase2Error::Conflict);
                }
                let definition = consent_route_definition(snapshot.route_id.as_str())?;
                if definition.source != snapshot.source
                    || definition.destination != snapshot.destination
                    || !departure_matches_route(&definition, snapshot.departure)
                {
                    return Err(Phase2Error::Conflict);
                }
                let (group, members) = proposal_group(state, group_id, actor.character_id)?;
                active_member_lease(state, members[0], now)?;
                active_member_lease(state, members[1], now)?;
                let group_record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
                if group_record.zone != snapshot.source
                    || group_record.zone_revision != snapshot.expected_group_zone_revision
                {
                    return Err(Phase2Error::Conflict);
                }
                let mut revisions = [0_u64; 2];
                for (index, member) in members.iter().copied().enumerate() {
                    let character = state.characters.get(&member).ok_or(Phase2Error::Internal)?;
                    if character.state.world_zone != snapshot.source
                        || character.world_revision
                            != snapshot.expected_members[index].world_revision
                        || snapshot.expected_members[index].character_id != member
                    {
                        return Err(Phase2Error::Conflict);
                    }
                    let progress = character
                        .state
                        .progress_for(definition.source.region)
                        .ok_or(Phase2Error::Forbidden)?;
                    if progress.badge_count() < definition.minimum_badges
                        || progress.story_checkpoint < definition.minimum_story_checkpoint
                        || character
                            .state
                            .progress_for(definition.destination.region)
                            .is_none()
                    {
                        return Err(Phase2Error::Forbidden);
                    }
                    revisions[index] = character
                        .world_revision
                        .checked_add(1)
                        .filter(|revision| *revision <= MAX_WORLD_REVISION)
                        .ok_or(Phase2Error::Internal)?;
                }
                let zone_revision = group_record
                    .zone_revision
                    .checked_add(1)
                    .filter(|revision| *revision <= MAX_WORLD_REVISION)
                    .ok_or(Phase2Error::Internal)?;
                ensure_lifecycle_receipt_slot(state)?;
                response.status = GroupTravelProposalStatus::Committed;
                response.commit = Some(GroupTravelCommit {
                    group_zone_revision: zone_revision,
                    members: [
                        GroupMemberView {
                            character_id: members[0],
                            world_revision: revisions[0],
                        },
                        GroupMemberView {
                            character_id: members[1],
                            world_revision: revisions[1],
                        },
                    ],
                    destination: snapshot.destination.clone(),
                });
                for (index, member) in members.iter().copied().enumerate() {
                    let character = state.characters.get_mut(&member).expect("member exists");
                    character.state.world_zone = snapshot.destination.clone();
                    character.world_revision = revisions[index];
                }
                let record = state.groups.get_mut(&group_id).expect("group exists");
                record.zone = snapshot.destination.clone();
                record.zone_revision = zone_revision;
                state
                    .group_travel_proposals
                    .get_mut(&proposal_id)
                    .expect("proposal exists")
                    .view = response.clone();
                let _ = group;
            }
        }
        insert_proposal_receipt(
            state,
            actor.character_id,
            OP_TRAVEL_ACTION,
            request.idempotency_key,
            fingerprint,
            response.clone(),
            now,
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

    fn app_with_clock() -> (super::super::Phase2App, Arc<super::super::FixedClock>) {
        let clock = Arc::new(super::super::FixedClock::new(1_700_000_000_000));
        let config = super::super::Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            clock.clone(),
            Arc::new(super::super::FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            super::super::ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ));
        (
            super::super::Phase2App::new(config).expect("test config is local"),
            clock,
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

    fn set_consent_progress(
        app: &super::super::Phase2App,
        group_id: GroupId,
        members: [AuthenticatedActor; 2],
        source: WorldZone,
    ) {
        app.store
            .write_transaction(|state| {
                for actor in members {
                    let progress = vec![
                        RegionalProgress::new(RegionId::Johto, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                        RegionalProgress::new(RegionId::Kanto, 0, 0, vec![], vec![])
                            .map_err(|_| Phase2Error::Internal)?,
                    ];
                    let character = state
                        .characters
                        .get_mut(&actor.character_id)
                        .ok_or(Phase2Error::Internal)?;
                    character.state =
                        CharacterCloudState::new(actor.character_id, source.clone(), progress)
                            .map_err(|_| Phase2Error::Internal)?;
                }
                let group = state
                    .groups
                    .get_mut(&group_id)
                    .ok_or(Phase2Error::Internal)?;
                group.zone = source;
                group.zone_revision = 0;
                Ok::<(), Phase2Error>(())
            })
            .expect("consent state");
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
        assert_eq!(consent_route_catalog().len(), 6);
        assert!(consent_route_catalog().iter().all(|route| {
            route.source.map_entry().is_ok()
                && route.destination.map_entry().is_ok()
                && route.minimum_badges == 0
                && route.minimum_story_checkpoint == 0
        }));
        assert!(
            route_definition("JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN").is_err(),
            "legacy immediate travel must not address consent routes"
        );
    }

    #[test]
    fn consent_catalog_creates_proposals_with_exact_route_contracts() {
        let cases = [
            (
                "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
                RegionId::Kanto,
                "KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
            ),
            (
                "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
                GroupTravelDeparture::Train,
                RegionId::Johto,
                "GOLDENROD_CITY_TRAIN_STATION",
                RegionId::Kanto,
                "KANTO_LATER_SAFFRON_CITY_TRAIN_STATION",
            ),
            (
                "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
                RegionId::Kanto,
                "KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "JOHTO:OLIVINE_KANTO_LATER_FERRY",
                GroupTravelDeparture::Ferry,
                RegionId::Johto,
                "OLIVINE_CITY_PORT_INSIDE",
                RegionId::Kanto,
                "KANTO_LATER_VERMILION_CITY_PORT_INSIDE",
            ),
            (
                "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "RECEPTION_GATE",
                RegionId::Kanto,
                "ROUTE22",
            ),
            (
                "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
                GroupTravelDeparture::Gate,
                RegionId::Kanto,
                "RECEPTION_GATE",
                RegionId::Kanto,
                "KANTO_LATER_ROUTE22",
            ),
        ];

        for (route_id, departure, source_region, source_map, destination_region, destination_map) in
            cases
        {
            let app = super::super::Phase2App::test();
            let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
            let source = WorldZone::new(source_region, source_map, 1).expect("source");
            let destination =
                WorldZone::new(destination_region, destination_map, 1).expect("destination");
            set_consent_progress(&app, group_id, [first, second], source.clone());
            let request = GroupTravelProposalRequest::new_with_departure(
                first_lease.fence(),
                route_id,
                departure,
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request");

            let proposal = create_travel_proposal(&app.store, first, group_id, &request)
                .expect("catalog route creates a proposal");
            assert_eq!(proposal.route_id.as_str(), route_id);
            assert_eq!(proposal.departure, departure);
            assert_eq!(proposal.source, source);
            assert_eq!(proposal.destination, destination);
            assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);
            assert_eq!(proposal.requester_character_id, first.character_id);
            assert_eq!(proposal.responder_character_id, second.character_id);
        }
    }

    #[test]
    fn proposal_rejects_route_departure_context_mismatch() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Gate,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request shape");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &request),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the lifecycle assertion keeps one proposal identity from create through cleanup"
    )]
    fn consent_accept_is_atomic_idempotent_and_waits_for_both_applied() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);

        let create = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &create).expect("proposal");
        assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &create).expect("create replay"),
            proposal
        );
        let conflicting = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &conflicting),
            Err(Phase2Error::Conflict)
        );

        let requester_accept = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &requester_accept,
            ),
            Err(Phase2Error::Forbidden)
        );

        let accept = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let committed =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept)
                .expect("accept");
        assert_eq!(committed.status, GroupTravelProposalStatus::Committed);
        let commit = committed.commit.as_ref().expect("commit payload");
        assert_eq!(commit.group_zone_revision, 1);
        assert_eq!(commit.members[0].world_revision, 1);
        assert_eq!(commit.members[1].world_revision, 1);
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept,)
                .expect("accept replay"),
            committed
        );

        app.store
            .write_transaction(|state| {
                let missing_create =
                    MAX_TRAVEL_CREATE_RECEIPTS.saturating_sub(create_receipt_count(state));
                for index in 0..missing_create {
                    let key = IdempotencyKey::new(Uuid::from_u128(
                        10_000_u128.saturating_add(index as u128),
                    ))
                    .map_err(|_| Phase2Error::Internal)?;
                    state.group_travel_proposal_idempotency.insert(
                        (first.character_id, format!("capacity-{index:04}"), key),
                        GroupTravelProposalIdempotencyRecord {
                            fingerprint: [u8::try_from(index % 256).expect("bounded"); 32],
                            response: committed.clone(),
                            expires_at: 1_700_000_000_000_u64
                                .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
                            lifecycle: false,
                        },
                    );
                }
                let protected = reserved_lifecycle_receipts(state);
                let missing_lifecycle = MAX_TRAVEL_LIFECYCLE_RECEIPTS
                    .saturating_sub(protected)
                    .saturating_sub(lifecycle_receipt_count(state));
                for index in 0..missing_lifecycle {
                    let key = IdempotencyKey::new(Uuid::from_u128(
                        20_000_u128.saturating_add(index as u128),
                    ))
                    .map_err(|_| Phase2Error::Internal)?;
                    state.group_travel_proposal_idempotency.insert(
                        (
                            first.character_id,
                            format!("lifecycle-capacity-{index:04}"),
                            key,
                        ),
                        GroupTravelProposalIdempotencyRecord {
                            fingerprint: [u8::try_from(index % 256).expect("bounded"); 32],
                            response: committed.clone(),
                            expires_at: 1_700_000_000_000_u64
                                .saturating_add(TRAVEL_PROPOSAL_REPLAY_TTL_MS),
                            lifecycle: true,
                        },
                    );
                }
                assert_eq!(
                    state
                        .group_travel_proposal_idempotency
                        .len()
                        .saturating_add(protected),
                    MAX_TRAVEL_PROPOSALS,
                );
                Ok::<(), Phase2Error>(())
            })
            .expect("fill replay capacity");
        assert_eq!(
            create_travel_proposal(&app.store, first, group_id, &create)
                .expect("create replay at capacity"),
            proposal
        );
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept)
                .expect("accept replay at capacity"),
            committed
        );

        let first_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let delivered = act_on_travel_proposal(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            &first_applied,
        )
        .expect("first applied");
        let conflicting_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Cancel,
            first_applied.idempotency_key,
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &conflicting_applied,
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &first_applied,
            )
            .expect("applied exact replay"),
            delivered
        );
        let before_fresh_replay = app
            .store
            .inspect_state(|state| {
                (
                    lifecycle_receipt_count(state),
                    reserved_lifecycle_receipts(state),
                )
            })
            .expect("receipt accounting");
        let fresh_applied = GroupTravelActionRequest::new(
            first_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("fresh key"),
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                first,
                group_id,
                proposal.proposal_id,
                &fresh_applied,
            ),
            Err(Phase2Error::Conflict)
        );
        let after_fresh_replay = app
            .store
            .inspect_state(|state| {
                (
                    lifecycle_receipt_count(state),
                    reserved_lifecycle_receipts(state),
                )
            })
            .expect("receipt accounting");
        assert_eq!(after_fresh_replay, before_fresh_replay);
        assert!(delivered.applied_by.iter().any(|applied| *applied));
        assert!(
            get_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                second_lease.fence(),
            )
            .is_ok()
        );

        let second_applied = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Applied,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let fully_delivered = act_on_travel_proposal(
            &app.store,
            second,
            group_id,
            proposal.proposal_id,
            &second_applied,
        )
        .expect("second applied");
        assert_eq!(fully_delivered.applied_by, [true, true]);
        assert_eq!(
            get_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                second_lease.fence(),
            ),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            act_on_travel_proposal(
                &app.store,
                second,
                group_id,
                proposal.proposal_id,
                &second_applied,
            )
            .expect("applied replay"),
            fully_delivered
        );
    }

    #[test]
    fn decline_and_explicit_release_cancel_without_world_mutation() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let request = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal =
            create_travel_proposal(&app.store, first, group_id, &request).expect("proposal");
        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(declined.status, GroupTravelProposalStatus::Declined);
        let state = app
            .store
            .inspect_state(|state| {
                let group = state.groups.get(&group_id).expect("group");
                (
                    group.zone.clone(),
                    group.zone_revision,
                    state.characters[&first.character_id].world_revision,
                    state.characters[&second.character_id].world_revision,
                )
            })
            .expect("state");
        assert_eq!(state, (source, 0, 0, 0));

        let second_request = GroupTravelProposalRequest::new(
            first_lease.fence(),
            "JOHTO:OLIVINE_KANTO_LATER_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let second_proposal = create_travel_proposal(&app.store, first, group_id, &second_request)
            .expect("second proposal");
        app.release(
            first,
            coop_cloud::ReleaseLeaseRequest::new(
                first_lease.fence(),
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            ),
        )
        .expect("release");
        let cancelled = app
            .store
            .inspect_state(|state| {
                state.group_travel_proposals[&second_proposal.proposal_id]
                    .view
                    .status
            })
            .expect("state");
        assert_eq!(cancelled, GroupTravelProposalStatus::Cancelled);
    }

    #[test]
    fn legacy_travel_conflicts_while_a_consent_proposal_is_live() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal_request = GroupTravelProposalRequest::new_with_departure(
            first_lease.fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Train,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("request");
        let proposal = create_travel_proposal(&app.store, first, group_id, &proposal_request)
            .expect("proposal");
        assert_eq!(proposal.status, GroupTravelProposalStatus::Pending);

        // The group sits at a Johto station, not a legacy ferry source, so
        // legacy travel would normally fail its zone check. While the
        // proposal is live it must fail on the proposal fence instead.
        let legacy = GroupTravelRequest::new(
            first_lease.fence(),
            "HOENN:SLATEPORT_SEVII_FERRY",
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        )
        .expect("travel request");
        assert_eq!(
            travel(&app.store, first, group_id, &legacy),
            Err(Phase2Error::Conflict)
        );

        // Once the proposal resolves, legacy travel runs its normal checks
        // again instead of staying fenced.
        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(declined.status, GroupTravelProposalStatus::Declined);
        assert_eq!(
            travel(&app.store, first, group_id, &legacy),
            Err(Phase2Error::Forbidden)
        );
    }

    #[test]
    fn terminal_state_and_receipts_replay_then_prune_before_new_admission() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "OLIVINE_CITY_PORT_INSIDE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        let decline = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Decline,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        let declined =
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline)
                .expect("decline");
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &decline,)
                .expect("exact replay within retention"),
            declined
        );

        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state
                    .group_travel_proposals
                    .get_mut(&proposal.proposal_id)
                    .expect("terminal proposal")
                    .retain_until = Some(now.saturating_sub(1));
                for receipt in state.group_travel_proposal_idempotency.values_mut() {
                    receipt.expires_at = now.saturating_sub(1);
                }
                Ok::<(), Phase2Error>(())
            })
            .expect("age terminal state");
        let replacement = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:OLIVINE_KANTO_LATER_FERRY",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("new proposal after pruning");
        assert_eq!(replacement.status, GroupTravelProposalStatus::Pending);
        let retained = app
            .store
            .inspect_state(|state| {
                (
                    state
                        .group_travel_proposals
                        .contains_key(&proposal.proposal_id),
                    state.group_travel_proposal_idempotency.len(),
                )
            })
            .expect("state");
        assert_eq!(retained, (false, 1));
    }

    #[test]
    fn expiry_and_accept_revalidation_never_partially_mutate_world_state() {
        let (app, clock) = app_with_clock();
        let (first, first_lease, second, second_lease, group_id) = two_member_group(&app);
        let source =
            WorldZone::new(RegionId::Johto, "GOLDENROD_CITY_TRAIN_STATION", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source.clone());
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");

        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&first.character_id)
                    .ok_or(Phase2Error::Internal)?
                    .world_revision = 1;
                Ok::<(), Phase2Error>(())
            })
            .expect("stale expected revision");
        let accept = GroupTravelActionRequest::new(
            second_lease.fence(),
            GroupTravelAction::Accept,
            IdempotencyKey::new(Uuid::new_v4()).expect("key"),
        );
        assert_eq!(
            act_on_travel_proposal(&app.store, second, group_id, proposal.proposal_id, &accept,),
            Err(Phase2Error::Conflict)
        );
        let unchanged = app
            .store
            .inspect_state(|state| {
                (
                    state.groups[&group_id].zone.clone(),
                    state.groups[&group_id].zone_revision,
                    state.characters[&first.character_id].world_revision,
                    state.characters[&second.character_id].world_revision,
                )
            })
            .expect("state");
        assert_eq!(unchanged, (source, 0, 1, 0));

        clock.advance(20_000);
        let first_heartbeat = app
            .heartbeat(
                first,
                coop_cloud::HeartbeatLeaseRequest::new(first_lease.fence()),
            )
            .expect("first heartbeat");
        let _second_heartbeat = app
            .heartbeat(
                second,
                coop_cloud::HeartbeatLeaseRequest::new(second_lease.fence()),
            )
            .expect("second heartbeat");
        clock.advance(10_001);
        assert_eq!(
            current_travel_proposal(&app.store, first, group_id, first_heartbeat.fence()),
            Err(Phase2Error::NotFound)
        );
        let expired = get_travel_proposal(
            &app.store,
            first,
            group_id,
            proposal.proposal_id,
            first_heartbeat.fence(),
        )
        .expect("expired proposal remains inspectable");
        assert_eq!(expired.status, GroupTravelProposalStatus::Expired);
    }

    #[test]
    fn online_leave_cancels_pending_proposal() {
        let app = super::super::Phase2App::test();
        let (first, first_lease, second, _second_lease, group_id) = two_member_group(&app);
        let source = WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("source");
        set_consent_progress(&app, group_id, [first, second], source);
        let proposal = create_travel_proposal(
            &app.store,
            first,
            group_id,
            &GroupTravelProposalRequest::new(
                first_lease.fence(),
                "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
                IdempotencyKey::new(Uuid::new_v4()).expect("key"),
            )
            .expect("request"),
        )
        .expect("proposal");
        let response = super::super::online::action(
            &app,
            first,
            &coop_cloud::OnlineActionRequest {
                api_version: coop_cloud::ApiVersion::V1,
                fence: first_lease.fence(),
                idempotency_key: IdempotencyKey::new(Uuid::new_v4()).expect("key"),
                action: coop_cloud::OnlineAction::Leave { group_id },
            },
        )
        .expect("leave");
        assert_eq!(response, coop_cloud::OnlineActionResponse::Left);
        let status = app
            .store
            .inspect_state(|state| {
                state.group_travel_proposals[&proposal.proposal_id]
                    .view
                    .status
            })
            .expect("state");
        assert_eq!(status, GroupTravelProposalStatus::Cancelled);
    }

    #[test]
    fn old_group_record_defaults_zone_revision() {
        let first = CharacterId::new(Uuid::from_u128(1)).expect("id");
        let second = CharacterId::new(Uuid::from_u128(2)).expect("id");
        let record = GroupRecord {
            group: Group::new(first, second).expect("group"),
            zone: WorldZone::new(RegionId::Kanto, "RECEPTION_GATE", 1).expect("zone"),
            status: GroupStatus::Active,
            zone_revision: 9,
        };
        let mut value = serde_json::to_value(record).expect("serialize");
        value
            .as_object_mut()
            .expect("object")
            .remove("zone_revision");
        let decoded: GroupRecord = serde_json::from_value(value).expect("old record");
        assert_eq!(decoded.zone_revision, 0);
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

//! Online menu operations serialized with runtime transitions and group mutations.

use super::storage::{
    GROUP_IDEMPOTENCY_TTL_MS, GroupIdempotencyRecord, GroupIdempotencyResponse, GroupStatus,
    MAX_GROUP_IDEMPOTENCY, State,
};
use super::{AuthenticatedActor, Phase2App, Phase2Error, group_travel as groups};
use coop_cloud::{
    AcceptGroupInvitationRequest, ApiVersion, CharacterId, CreateGroupInvitationRequest,
    ONLINE_PAGE_SIZE, OnlineAction, OnlineActionRequest, OnlineActionResponse, OnlineGroup,
    OnlineInvitation, OnlineRememberedPartner, OnlineSnapshotRequest, OnlineSnapshotResponse,
};
use coop_protocol::CanonicalUsername;

const INVITE: &str = "online_invite_v1";
const INVITE_LAST_PARTNER: &str = "online_invite_last_partner_v1";
const MUTATE: &str = "online_mutate_v1";

fn username(state: &State, id: CharacterId) -> Result<CanonicalUsername, Phase2Error> {
    let character = state.characters.get(&id).ok_or(Phase2Error::Internal)?;
    let user = state
        .users_by_id
        .get(&character.owner)
        .ok_or(Phase2Error::Internal)?;
    CanonicalUsername::new(user.username.as_str()).map_err(|_| Phase2Error::Internal)
}

fn last_partner_id(state: &State, actor: CharacterId) -> Result<CharacterId, Phase2Error> {
    let group_id = state
        .last_group_by_member
        .get(&actor)
        .copied()
        .ok_or(Phase2Error::NotFound)?;
    let group = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if !group.group.contains(actor) {
        return Err(Phase2Error::NotFound);
    }
    group
        .group
        .members()
        .into_iter()
        .find(|member| *member != actor)
        .ok_or(Phase2Error::NotFound)
}

fn last_partner_view(
    state: &State,
    actor: CharacterId,
) -> Result<Option<OnlineRememberedPartner>, Phase2Error> {
    let Some(group_id) = state.last_group_by_member.get(&actor).copied() else {
        return Ok(None);
    };
    let Some(group) = state.groups.get(&group_id) else {
        return Ok(None);
    };
    if !group.group.contains(actor) {
        return Ok(None);
    }
    let Some(partner) = group
        .group
        .members()
        .into_iter()
        .find(|member| *member != actor)
    else {
        return Ok(None);
    };
    groups::validate_member(state, partner)?;
    Ok(Some(OnlineRememberedPartner {
        character_id: partner,
        username: username(state, partner)?,
    }))
}

pub(super) fn snapshot(
    app: &Phase2App,
    actor: AuthenticatedActor,
    request: &OnlineSnapshotRequest,
) -> Result<OnlineSnapshotResponse, Phase2Error> {
    let _gate = app.store.lock_runtime_transition_gate();
    let now = app.store.now();
    let mut response =
        app.store
            .read_transaction(|state| -> Result<_, Phase2Error> {
                groups::authenticate_caller(state, actor, request.fence.character_id)?;
                groups::lease_matches(state, actor.character_id, request.fence, now)?;
                let group = state
                    .active_group_by_member
                    .get(&actor.character_id)
                    .map(|id| {
                        let view = groups::group_view(state, *id)?;
                        if !view
                            .members
                            .iter()
                            .any(|m| m.character_id == actor.character_id)
                            || view.members.iter().any(|m| {
                                state.active_group_by_member.get(&m.character_id) != Some(id)
                            })
                        {
                            return Err(Phase2Error::Internal);
                        }
                        for member in &view.members {
                            groups::validate_member(state, member.character_id)?;
                        }
                        let counterpart = view
                            .members
                            .iter()
                            .find(|m| m.character_id != actor.character_id)
                            .ok_or(Phase2Error::Internal)?
                            .character_id;
                        Ok(OnlineGroup {
                            username: username(state, counterpart)?,
                            group: view,
                        })
                    })
                    .transpose()?;
                let mut records: Vec<_> = state
                    .group_invitations
                    .values()
                    .filter(|invitation| {
                        invitation.invitee == actor.character_id
                            && !invitation.consumed
                            && invitation.expires_at > now
                            && request
                                .incoming_after
                                .is_none_or(|after| invitation.invitation_id > after)
                    })
                    .collect();
                records.sort_by_key(|invitation| invitation.invitation_id);
                let has_more = records.len() > ONLINE_PAGE_SIZE;
                records.truncate(ONLINE_PAGE_SIZE);
                let incoming_next = if has_more {
                    records.last().map(|invitation| invitation.invitation_id)
                } else {
                    None
                };
                let incoming = records
                    .into_iter()
                    .map(|record| {
                        Ok(OnlineInvitation {
                            invitation: groups::build_invitation_view(record)?,
                            username: username(state, record.inviter)?,
                        })
                    })
                    .collect::<Result<_, Phase2Error>>()?;
                let mut sent: Vec<_> = state
                    .group_invitations
                    .values()
                    .filter(|invitation| {
                        invitation.inviter == actor.character_id
                            && !invitation.consumed
                            && invitation.expires_at > now
                    })
                    .collect();
                sent.sort_by_key(|invitation| invitation.invitation_id);
                sent.truncate(ONLINE_PAGE_SIZE);
                let outgoing = sent
                    .into_iter()
                    .map(|record| {
                        Ok(OnlineInvitation {
                            invitation: groups::build_invitation_view(record)?,
                            username: username(state, record.invitee)?,
                        })
                    })
                    .collect::<Result<_, Phase2Error>>()?;
                let last_partner = last_partner_view(state, actor.character_id)?;
                Ok(OnlineSnapshotResponse {
                    api_version: ApiVersion::V1,
                    nearby: Vec::new(),
                    incoming,
                    outgoing,
                    incoming_next,
                    group,
                    last_partner,
                })
            })?;
    if let Some(group) = response.group.as_mut() {
        let counterpart = group
            .group
            .members
            .iter()
            .find(|member| member.character_id != actor.character_id)
            .map(|member| member.character_id);
        if let Some(counterpart) = counterpart
            && let Some(live_zone) = app.presence.live_location_locked(counterpart)
            && let Some(index) = group
                .group
                .members
                .iter()
                .position(|member| member.character_id == counterpart)
        {
            // Expose only the authenticated caller's partner's fresh map.
            // The caller's own member zone remains the durable save zone.
            group.group.member_world_zones[index] = live_zone.clone();
            // Preserve the legacy shared-zone field for older clients; it is
            // explicitly the partner's location in this authenticated view.
            group.group.world_zone = live_zone;
        }
    }
    if response.group.is_none() {
        response.nearby = app
            .presence
            .online_peers_locked(actor, request.fence)?
            .into_iter()
            .map(|(peer, _)| peer)
            .collect();
    }
    Ok(response)
}

pub(super) fn action(
    app: &Phase2App,
    actor: AuthenticatedActor,
    request: &OnlineActionRequest,
) -> Result<OnlineActionResponse, Phase2Error> {
    let _gate = app.store.lock_runtime_transition_gate();
    let now = app.store.now();
    // Validate the precise fence even for a successful operation replay.
    app.store.read_transaction(|state| {
        groups::authenticate_caller(state, actor, request.fence.character_id)?;
        groups::lease_matches(state, actor.character_id, request.fence, now)
    })?;
    match request.action {
        OnlineAction::Invite { handle, generation } => {
            let fingerprint = groups::request_fingerprint(INVITE.as_bytes(), request)?;
            let replay = app.store.read_transaction(|state| {
                groups::idempotency_lookup(
                    state,
                    actor.character_id,
                    INVITE,
                    request.idempotency_key,
                    fingerprint,
                    now,
                )
            })?;
            if let Some(replay) = replay {
                return match replay {
                    GroupIdempotencyResponse::Invitation(invitation) => {
                        Ok(OnlineActionResponse::Invited { invitation })
                    }
                    _ => Err(Phase2Error::Conflict),
                };
            }
            let target = app
                .presence
                .online_peers_locked(actor, request.fence)?
                .into_iter()
                .find(|(peer, _)| peer.handle == handle && peer.generation == generation)
                .map(|(_, target)| target)
                .ok_or(Phase2Error::NotFound)?;
            let invitation = groups::create_invitation_with_fingerprint(
                &app.store,
                actor,
                &CreateGroupInvitationRequest::new(request.fence, target, request.idempotency_key),
                INVITE,
                fingerprint,
            )?;
            Ok(OnlineActionResponse::Invited { invitation })
        }
        OnlineAction::Accept { invitation_id } => {
            let response = groups::accept_invitation(
                &app.store,
                actor,
                invitation_id,
                &AcceptGroupInvitationRequest::new(request.fence, request.idempotency_key),
            )?;
            app.queue_group_started(&response.group);
            Ok(OnlineActionResponse::Accepted {
                group: response.group,
            })
        }
        OnlineAction::InviteLastPartner => {
            let fingerprint = groups::request_fingerprint(INVITE_LAST_PARTNER.as_bytes(), request)?;
            let replay = app.store.read_transaction(|state| {
                groups::idempotency_lookup(
                    state,
                    actor.character_id,
                    INVITE_LAST_PARTNER,
                    request.idempotency_key,
                    fingerprint,
                    now,
                )
            })?;
            if let Some(replay) = replay {
                return match replay {
                    GroupIdempotencyResponse::Invitation(invitation) => {
                        Ok(OnlineActionResponse::Invited { invitation })
                    }
                    _ => Err(Phase2Error::Conflict),
                };
            }
            let target = app
                .store
                .read_transaction(|state| last_partner_id(state, actor.character_id))?;
            let invitation = groups::create_last_partner_invitation_with_fingerprint(
                &app.store,
                actor,
                &CreateGroupInvitationRequest::new(request.fence, target, request.idempotency_key),
                INVITE_LAST_PARTNER,
                fingerprint,
            )?;
            Ok(OnlineActionResponse::Invited { invitation })
        }
        OnlineAction::Decline { .. } | OnlineAction::Cancel { .. } | OnlineAction::Leave { .. } => {
            terminate(app, actor, request, now)
        }
    }
}

fn terminate(
    app: &Phase2App,
    actor: AuthenticatedActor,
    request: &OnlineActionRequest,
    now: u64,
) -> Result<OnlineActionResponse, Phase2Error> {
    let fingerprint = groups::request_fingerprint(MUTATE.as_bytes(), request)?;
    let expires_at = now
        .checked_add(GROUP_IDEMPOTENCY_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    app.store.write_transaction(|state| {
        groups::authenticate_caller(state, actor, request.fence.character_id)?;
        groups::lease_matches(state, actor.character_id, request.fence, now)?;
        if let Some(replay) = groups::idempotency_lookup(
            state,
            actor.character_id,
            MUTATE,
            request.idempotency_key,
            fingerprint,
            now,
        )? {
            return match replay {
                GroupIdempotencyResponse::Online(response) => Ok(response),
                _ => Err(Phase2Error::Conflict),
            };
        }
        if groups::live_idempotency_count(state, now) >= MAX_GROUP_IDEMPOTENCY {
            return Err(Phase2Error::Busy);
        }
        let response = match request.action {
            OnlineAction::Decline { invitation_id } => {
                let invitation = state
                    .group_invitations
                    .get_mut(&invitation_id)
                    .ok_or(Phase2Error::NotFound)?;
                if invitation.invitee != actor.character_id {
                    return Err(Phase2Error::NotFound);
                }
                if invitation.consumed || invitation.expires_at <= now {
                    return Err(Phase2Error::Expired);
                }
                invitation.consumed = true;
                OnlineActionResponse::Declined
            }
            OnlineAction::Cancel { invitation_id } => {
                let invitation = state
                    .group_invitations
                    .get_mut(&invitation_id)
                    .ok_or(Phase2Error::NotFound)?;
                if invitation.inviter != actor.character_id {
                    return Err(Phase2Error::NotFound);
                }
                if invitation.consumed || invitation.expires_at <= now {
                    return Err(Phase2Error::Expired);
                }
                invitation.consumed = true;
                OnlineActionResponse::Cancelled
            }
            OnlineAction::Leave { group_id } => {
                let group = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
                if group.status != GroupStatus::Active || !group.group.contains(actor.character_id)
                {
                    return Err(Phase2Error::NotFound);
                }
                let members = group.group.members();
                if members
                    .iter()
                    .any(|id| state.active_group_by_member.get(id) != Some(&group_id))
                {
                    return Err(Phase2Error::Internal);
                }
                groups::cancel_pending_for_member(state, actor.character_id);
                state
                    .groups
                    .get_mut(&group_id)
                    .expect("validated group exists")
                    .status = GroupStatus::Closed;
                for member in members {
                    state.active_group_by_member.remove(&member);
                }
                OnlineActionResponse::Left
            }
            _ => return Err(Phase2Error::InvalidRequest),
        };
        groups::prune_group_state(state, now);
        state.group_idempotency.insert(
            (
                actor.character_id,
                MUTATE.to_owned(),
                request.idempotency_key,
            ),
            GroupIdempotencyRecord {
                fingerprint,
                response: GroupIdempotencyResponse::Online(response.clone()),
                expires_at,
            },
        );
        Ok(response)
    })
}

//! Remote pairing-code issuance and redemption.

use coop_cloud::{
    CreatePairingCodeRequest, CreatePairingCodeResponse, GROUP_PAIRING_CODE_LINK_PREFIX,
    GROUP_PAIRING_CODE_TTL_MS, Group, GroupId, GroupView, RedeemPairingCodeRequest,
    RedeemPairingCodeResponse,
};

use super::group_travel::{authenticate_caller, lease_matches, validate_member};
use super::storage::{GroupRecord, GroupStatus, Store};
use super::{AuthenticatedActor, Phase2Error};

const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const MAX_PAIRING_CODES: usize = 1_024;
const MAX_ISSUANCES_PER_MINUTE: usize = 3;
const MAX_REDEEM_ATTEMPTS_PER_MINUTE: usize = 12;
const ISSUANCE_WINDOW_MS: u64 = 60_000;
const MAX_CODE_CANDIDATES: usize = 16;

fn format_code(uuid: uuid::Uuid) -> String {
    let bytes = uuid.as_bytes();
    let mut code = String::with_capacity(7);
    for (index, byte) in bytes[..6].iter().enumerate() {
        if index == 3 {
            code.push('-');
        }
        code.push(CODE_ALPHABET[usize::from(*byte) % CODE_ALPHABET.len()] as char);
    }
    code
}

fn prune_codes(state: &mut super::storage::State, now: u64) {
    state
        .group_pairing_codes
        .retain(|_, record| !record.consumed && record.expires_at > now);
    state.group_pairing_code_issuances.retain(|_, issuances| {
        issuances.retain(|issued_at| issued_at.saturating_add(ISSUANCE_WINDOW_MS) > now);
        !issuances.is_empty()
    });
    state.group_pairing_code_attempts.retain(|_, attempts| {
        attempts.retain(|attempted_at| attempted_at.saturating_add(ISSUANCE_WINDOW_MS) > now);
        !attempts.is_empty()
    });
}

fn candidate_group_id(
    store: &Store,
    state: &super::storage::State,
) -> Result<GroupId, Phase2Error> {
    for _ in 0..MAX_CODE_CANDIDATES {
        let group_id = GroupId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal)?;
        if !state.groups.contains_key(&group_id) {
            return Ok(group_id);
        }
    }
    Err(Phase2Error::Conflict)
}

fn revisions_and_zones(
    state: &super::storage::State,
    group: Group,
) -> Result<([u64; 2], [coop_protocol::WorldZone; 2]), Phase2Error> {
    let members = group.members();
    let mut revisions = [0; 2];
    let mut zones = [
        coop_protocol::WorldZone::new(coop_protocol::RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
            .map_err(|_| Phase2Error::Internal)?,
        coop_protocol::WorldZone::new(coop_protocol::RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
            .map_err(|_| Phase2Error::Internal)?,
    ];
    for (index, member) in members.into_iter().enumerate() {
        let character = state.characters.get(&member).ok_or(Phase2Error::Internal)?;
        revisions[index] = character.world_revision;
        zones[index] = character.state.world_zone.clone();
    }
    Ok((revisions, zones))
}

pub(crate) fn create_code(
    store: &Store,
    actor: AuthenticatedActor,
    request: &CreatePairingCodeRequest,
) -> Result<CreatePairingCodeResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let now = store.now();
    let expires_at = now
        .checked_add(GROUP_PAIRING_CODE_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_codes(state, now);
        if state
            .active_group_by_member
            .contains_key(&actor.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        let issuances = state
            .group_pairing_code_issuances
            .entry(actor.character_id)
            .or_default();
        if issuances.len() >= MAX_ISSUANCES_PER_MINUTE {
            return Err(Phase2Error::Busy);
        }
        if state.group_pairing_codes.len() >= MAX_PAIRING_CODES {
            return Err(Phase2Error::Busy);
        }
        let (code, fingerprint) = (0..MAX_CODE_CANDIDATES)
            .map(|_| store.random_uuid().map(format_code))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|code| {
                let fingerprint = store.invitation_fingerprint(&code);
                (code, fingerprint)
            })
            .find(|(_, fingerprint)| !state.group_pairing_codes.contains_key(fingerprint))
            .ok_or(Phase2Error::Conflict)?;
        state.group_pairing_codes.insert(
            fingerprint,
            super::storage::GroupPairingCodeRecord {
                inviter: actor.character_id,
                expires_at,
                consumed: false,
            },
        );
        issuances.push(now);
        let expires_at = Store::unix_timestamp(expires_at).map_err(|_| Phase2Error::Internal)?;
        Ok(CreatePairingCodeResponse {
            api_version: coop_cloud::ApiVersion::V1,
            join_link: format!("{GROUP_PAIRING_CODE_LINK_PREFIX}{code}"),
            code: coop_cloud::PairingCode::new(code).map_err(|_| Phase2Error::Internal)?,
            expires_at,
        })
    })
}

pub(crate) fn redeem_code(
    store: &Store,
    actor: AuthenticatedActor,
    request: &RedeemPairingCodeRequest,
) -> Result<RedeemPairingCodeResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let now = store.now();
    let fingerprint = store.invitation_fingerprint(request.code.as_str());
    let result = store.write_transaction(|state| {
        authenticate_caller(state, actor, request.character_id)?;
        lease_matches(state, actor.character_id, request.fence(), now)?;
        prune_codes(state, now);
        if state
            .active_group_by_member
            .contains_key(&actor.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        if state
            .group_pairing_code_attempts
            .get(&actor.character_id)
            .is_some_and(|attempts| attempts.len() >= MAX_REDEEM_ATTEMPTS_PER_MINUTE)
        {
            return Err(Phase2Error::Busy);
        }
        let code = state.group_pairing_codes.get(&fingerprint).ok_or_else(|| {
            state
                .group_pairing_code_attempts
                .entry(actor.character_id)
                .or_default()
                .push(now);
            Phase2Error::NotFound
        });
        let Ok(code) = code else {
            // Keep the failed-attempt counter in the committed transaction
            // while presenting the same response for unknown and expired
            // bearer codes.
            return Ok(Err(Phase2Error::NotFound));
        };
        if code.consumed || code.expires_at <= now {
            state
                .group_pairing_code_attempts
                .entry(actor.character_id)
                .or_default()
                .push(now);
            return Ok(Err(Phase2Error::NotFound));
        }
        let inviter = code.inviter;
        if inviter == actor.character_id {
            return Err(Phase2Error::Forbidden);
        }
        validate_member(state, inviter)?;
        if state.handoff_for_member(inviter) {
            return Err(Phase2Error::Conflict);
        }
        if state.active_group_by_member.contains_key(&inviter) {
            return Err(Phase2Error::Conflict);
        }
        let inviter_lease = state.leases.get(&inviter).ok_or(Phase2Error::NotFound)?;
        if inviter_lease.released || inviter_lease.contract.expires_at.value() <= now {
            return Err(Phase2Error::NotFound);
        }
        let group = Group::new(inviter, actor.character_id).map_err(|_| Phase2Error::Internal)?;
        let group_id = candidate_group_id(store, state)?;
        let (revisions, zones) = revisions_and_zones(state, group)?;
        let view = GroupView::new_with_member_zones(group_id, group, zones.clone(), revisions)
            .map_err(|_| Phase2Error::Internal)?;
        state
            .group_pairing_codes
            .get_mut(&fingerprint)
            .ok_or(Phase2Error::Internal)?
            .consumed = true;
        state.groups.insert(
            group_id,
            GroupRecord {
                group,
                zone: zones[0].clone(),
                status: GroupStatus::Active,
                zone_revision: 0,
            },
        );
        let members = group.members();
        state.group_end_notices.remove(&members[0]);
        state.group_end_notices.remove(&members[1]);
        state.active_group_by_member.insert(members[0], group_id);
        state.active_group_by_member.insert(members[1], group_id);
        state.last_group_by_member.insert(members[0], group_id);
        state.last_group_by_member.insert(members[1], group_id);
        state.group_member_world_zones.insert(group_id, zones);
        Ok(Ok(RedeemPairingCodeResponse {
            api_version: coop_cloud::ApiVersion::V1,
            group: view,
        }))
    })?;
    result
}

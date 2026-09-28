//! Authenticated read model for the desktop partner panel.

use super::{AuthenticatedActor, Phase2App, Phase2Error, group_travel, storage::GroupStatus};
use coop_cloud::{ApiVersion, PartnerStatus, PartnerStatusResponse, UnixTimestampMillis};

pub(super) fn status(
    app: &Phase2App,
    actor: AuthenticatedActor,
) -> Result<PartnerStatusResponse, Phase2Error> {
    let _gate = app.store.lock_runtime_transition_gate();
    let store = &app.store;
    let now = store.now();
    let presence_now = store.presence_now();
    store.read_transaction(|state| {
        group_travel::authenticate_caller(state, actor, actor.character_id)?;
        let group_id = state
            .active_group_by_member
            .get(&actor.character_id)
            .or_else(|| state.last_group_by_member.get(&actor.character_id));
        let partner = if let Some(group_id) = group_id {
            let group = state.groups.get(group_id).ok_or(Phase2Error::Internal)?;
            let partner_id = group
                .group
                .members()
                .into_iter()
                .find(|member| *member != actor.character_id)
                .ok_or(Phase2Error::Internal)?;
            group_travel::validate_member(state, partner_id)?;
            let character = state
                .characters
                .get(&partner_id)
                .ok_or(Phase2Error::Internal)?;
            let user = state
                .users_by_id
                .get(&character.owner)
                .ok_or(Phase2Error::Internal)?;
            let world_zone = character.state.world_zone.clone();
            let badge_count = character
                .state
                .progress_for(world_zone.region)
                .ok_or(Phase2Error::Internal)?
                .badge_count();
            let group_active = group.status == GroupStatus::Active
                && state.active_group_by_member.get(&actor.character_id) == Some(group_id)
                && state.active_group_by_member.get(&partner_id) == Some(group_id);
            let online = state
                .leases
                .get(&partner_id)
                .is_some_and(|lease| !lease.released && lease.contract.expires_at.value() > now);
            let live_world_zone = if group_active {
                app.presence
                    .live_location_from_state(state, partner_id, presence_now)
            } else {
                None
            };
            Some(PartnerStatus {
                username: user.username.clone(),
                online,
                last_seen_at: state
                    .last_seen_at
                    .get(&partner_id)
                    .copied()
                    .map(UnixTimestampMillis::new),
                world_zone,
                live_world_zone,
                badge_count,
                group_active,
            })
        } else {
            None
        };
        Ok(PartnerStatusResponse {
            api_version: ApiVersion::V1,
            partner,
        })
    })
}

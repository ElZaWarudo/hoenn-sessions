//! Server-owned, observational progress feed for co-op groups.
//!
//! The ROM may report a badge, first catch, or story milestone, but those
//! reports are never accepted as gameplay authority. The only durable effect
//! is a small, bounded group history and a realtime notification to the
//! authenticated partner.

use coop_cloud::{
    CharacterId, ProgressFeedEventV1, RuntimeLeaseFence, ServerRealtimeFrameV1, UnixTimestampMillis,
};
use coop_protocol::ProgressObservationV1;

use super::super::storage::{
    GroupStatus, MAX_PROGRESS_FEED_EVENTS_PER_GROUP, MAX_PROGRESS_FEED_EVENTS_PER_MINUTE,
    MAX_PROGRESS_FEED_KEYS_PER_GROUP, ProgressFeedEventRecord, ProgressFeedObservationKey,
    ProgressFeedRateWindow, ProgressFeedState, ProgressFeedWatermark, State, Store,
};
use super::{AuthenticatedActor, Phase2Error};

/// One accepted event and the only character allowed to receive it.
#[derive(Debug)]
pub(super) struct AcceptedProgress {
    pub recipient: CharacterId,
    pub frame: ServerRealtimeFrameV1,
}

/// Validates and records one ROM observation under the repository boundary.
///
/// The runtime fence is checked again for every event. A connected socket is
/// therefore not allowed to keep publishing after its lease is released or a
/// newer session takes over the character.
pub(super) fn accept(
    store: &Store,
    actor: AuthenticatedActor,
    runtime: RuntimeLeaseFence,
    observation: ProgressObservationV1,
) -> Result<Option<AcceptedProgress>, Phase2Error> {
    if !observation.is_valid() {
        return Err(Phase2Error::InvalidRequest);
    }
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let build = runtime.build.clone();
    let _gate = store.lock_runtime_transition_gate();
    let now = store.now();
    store.write_transaction(|state| {
        super::validate_runtime_state_in_state(state, actor, &runtime, now, &build)?;
        super::validate_bound_runtime(state, catalog, actor, &runtime)?;
        if observation.session_epoch != runtime.session.session_epoch.value() {
            return Err(Phase2Error::Authentication);
        }
        // Progress hooks also run during solo play. A well-authenticated
        // observation without an active group is simply uninteresting to the
        // co-op feed and must not tear down the realtime socket.
        if !state
            .active_group_by_member
            .contains_key(&actor.character_id)
        {
            return Ok(None);
        }
        let (group_id, recipient, member_index) =
            active_group_for_actor(state, actor.character_id)?;
        let character = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::Authentication)?;
        let username = state
            .users_by_id
            .get(&character.owner)
            .ok_or(Phase2Error::Internal)?
            .username
            .clone();
        let feed = state
            .group_progress_feeds
            .entry(group_id)
            .or_insert_with(ProgressFeedState::default);
        record_feed_event(
            feed,
            actor.character_id,
            member_index,
            recipient,
            username,
            observation,
            now,
        )
    })
}

fn record_feed_event(
    feed: &mut ProgressFeedState,
    source_character_id: CharacterId,
    member_index: usize,
    recipient: CharacterId,
    source_username: coop_cloud::Username,
    observation: ProgressObservationV1,
    now: u64,
) -> Result<Option<AcceptedProgress>, Phase2Error> {
    if feed.next_event_id == 0 {
        // A missing/old default is repaired before issuing a durable ID.
        feed.next_event_id = 1;
    }
    let observation_key = ProgressFeedObservationKey {
        source_character_id,
        kind: observation.kind,
        region_id: observation.region_id,
        subject_id: observation.subject_id,
    };
    if feed.observed_keys.contains(&observation_key) {
        return Ok(None);
    }

    let rate_window =
        feed.rate_windows[member_index].get_or_insert_with(|| ProgressFeedRateWindow {
            character_id: source_character_id,
            timestamps: Vec::new(),
        });
    if rate_window.character_id != source_character_id {
        *rate_window = ProgressFeedRateWindow {
            character_id: source_character_id,
            timestamps: Vec::new(),
        };
    }
    rate_window
        .timestamps
        .retain(|timestamp| timestamp.saturating_add(60_000) > now);
    if rate_window.timestamps.len() >= MAX_PROGRESS_FEED_EVENTS_PER_MINUTE {
        return Err(Phase2Error::Busy);
    }

    match feed.watermarks[member_index] {
        Some(watermark) if watermark.session_epoch > observation.session_epoch => {
            return Err(Phase2Error::Expired);
        }
        Some(watermark)
            if watermark.session_epoch == observation.session_epoch
                && observation.source_sequence < watermark.source_sequence
                && observation.source_sequence != 1 =>
        {
            return Err(Phase2Error::Expired);
        }
        Some(watermark)
            if watermark.session_epoch == observation.session_epoch
                && observation.source_sequence == watermark.source_sequence
                && observation.source_sequence != 1 =>
        {
            return Err(Phase2Error::Expired);
        }
        _ => {}
    }

    // The launcher may rearm the bridge without incrementing the runtime
    // epoch. A source sequence of one therefore starts a fresh local stream,
    // while the observation key above still deduplicates a replay of the same
    // badge/species event.
    let event_id = feed.next_event_id;
    feed.next_event_id = event_id.checked_add(1).ok_or(Phase2Error::Internal)?;
    feed.watermarks[member_index] = Some(ProgressFeedWatermark {
        character_id: source_character_id,
        session_epoch: observation.session_epoch,
        source_sequence: observation.source_sequence,
    });
    rate_window.timestamps.push(now);
    feed.observed_keys.push(observation_key);
    if feed.observed_keys.len() > MAX_PROGRESS_FEED_KEYS_PER_GROUP {
        let remove = feed.observed_keys.len() - MAX_PROGRESS_FEED_KEYS_PER_GROUP;
        feed.observed_keys.drain(..remove);
    }
    feed.events.push(ProgressFeedEventRecord {
        event_id,
        source_character_id,
        source_username: source_username.clone(),
        kind: observation.kind,
        region_id: observation.region_id,
        subject_id: observation.subject_id,
        source_sequence: observation.source_sequence,
        occurred_at: now,
    });
    if feed.events.len() > MAX_PROGRESS_FEED_EVENTS_PER_GROUP {
        let remove = feed.events.len() - MAX_PROGRESS_FEED_EVENTS_PER_GROUP;
        feed.events.drain(..remove);
    }
    Ok(Some(AcceptedProgress {
        recipient,
        frame: ServerRealtimeFrameV1::progress_event(ProgressFeedEventV1 {
            event_id,
            source_character_id,
            source_username,
            kind: observation.kind,
            region_id: observation.region_id,
            subject_id: observation.subject_id,
            source_sequence: observation.source_sequence,
            occurred_at: UnixTimestampMillis::new(now),
        }),
    }))
}

/// Read the current bounded history for a caller's active group. The parent
/// module can expose this through an authenticated HTTP endpoint when a UI
/// needs replay after a reconnect; realtime delivery remains the fast path.
pub(crate) fn recent_for_partner(
    store: &Store,
    actor: AuthenticatedActor,
) -> Result<Vec<ProgressFeedEventV1>, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    store.read_transaction(|state| {
        let character = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::Authentication)?;
        if character.owner != actor.user_id {
            return Err(Phase2Error::Authentication);
        }
        let (group_id, _, _) = active_group_for_actor(state, actor.character_id)?;
        let feed = state.group_progress_feeds.get(&group_id);
        Ok(feed
            .map(|feed| partner_events(feed, actor.character_id))
            .unwrap_or_default())
    })
}

fn partner_events(feed: &ProgressFeedState, actor: CharacterId) -> Vec<ProgressFeedEventV1> {
    feed.events
        .iter()
        .filter(|event| event.source_character_id != actor)
        .map(to_cloud_event)
        .collect()
}

fn active_group_for_actor(
    state: &State,
    actor: CharacterId,
) -> Result<(coop_cloud::GroupId, CharacterId, usize), Phase2Error> {
    let group_id = state
        .active_group_by_member
        .get(&actor)
        .copied()
        .ok_or(Phase2Error::Forbidden)?;
    let group = state.groups.get(&group_id).ok_or(Phase2Error::Internal)?;
    if group.status != GroupStatus::Active || !group.group.contains(actor) {
        return Err(Phase2Error::Forbidden);
    }
    let members = group.group.members();
    let member_index = members
        .iter()
        .position(|member| *member == actor)
        .ok_or(Phase2Error::Internal)?;
    let recipient = members[1 - member_index];
    if state.active_group_by_member.get(&recipient) != Some(&group_id) {
        return Err(Phase2Error::Forbidden);
    }
    for member in members {
        let character = state.characters.get(&member).ok_or(Phase2Error::Internal)?;
        let user = state
            .users_by_id
            .get(&character.owner)
            .ok_or(Phase2Error::Internal)?;
        if user.disabled || user.character_id != member || character.state.character_id != member {
            return Err(Phase2Error::Forbidden);
        }
    }
    Ok((group_id, recipient, member_index))
}

fn to_cloud_event(event: &ProgressFeedEventRecord) -> ProgressFeedEventV1 {
    ProgressFeedEventV1 {
        event_id: event.event_id,
        source_character_id: event.source_character_id,
        source_username: event.source_username.clone(),
        kind: event.kind,
        region_id: event.region_id,
        subject_id: event.subject_id,
        source_sequence: event.source_sequence,
        occurred_at: UnixTimestampMillis::new(event.occurred_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{CharacterId, Username};
    use coop_protocol::{ProgressKindV1, RegionId};

    fn character(value: u128) -> CharacterId {
        CharacterId::new(uuid::Uuid::from_u128(value)).expect("character ID")
    }

    fn observation(sequence: u32, subject: u16, epoch: u32) -> ProgressObservationV1 {
        ProgressObservationV1 {
            kind: ProgressKindV1::FirstCaught,
            region_id: RegionId::Hoenn,
            subject_id: subject,
            session_epoch: epoch,
            source_sequence: sequence,
        }
    }

    #[test]
    fn identity_replay_is_deduplicated_and_reconnect_sequence_can_reset() {
        let source = character(1);
        let recipient = character(2);
        let username = Username::new("source").unwrap();
        let mut feed = ProgressFeedState::default();
        assert!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username.clone(),
                observation(4, 25, 7),
                100,
            )
            .unwrap()
            .is_some()
        );
        assert!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username.clone(),
                observation(4, 25, 7),
                101,
            )
            .unwrap()
            .is_none()
        );
        assert!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username,
                observation(1, 26, 7),
                102,
            )
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn stale_sequence_and_session_are_rejected() {
        let source = character(1);
        let recipient = character(2);
        let username = Username::new("source").unwrap();
        let mut feed = ProgressFeedState::default();
        record_feed_event(
            &mut feed,
            source,
            0,
            recipient,
            username.clone(),
            observation(5, 25, 7),
            100,
        )
        .unwrap();
        assert_eq!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username.clone(),
                observation(4, 26, 7),
                101,
            )
            .unwrap_err(),
            Phase2Error::Expired
        );
        assert_eq!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username,
                observation(1, 27, 6),
                102,
            )
            .unwrap_err(),
            Phase2Error::Expired
        );
    }

    #[test]
    fn rate_and_history_bounds_are_enforced() {
        let source = character(1);
        let recipient = character(2);
        let username = Username::new("source").unwrap();
        let mut feed = ProgressFeedState::default();
        for subject in 1..=MAX_PROGRESS_FEED_EVENTS_PER_MINUTE as u16 {
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username.clone(),
                observation(subject as u32, subject, 7),
                100,
            )
            .unwrap();
        }
        assert_eq!(
            record_feed_event(
                &mut feed,
                source,
                0,
                recipient,
                username,
                observation(61, 61, 7),
                100,
            )
            .unwrap_err(),
            Phase2Error::Busy
        );
        assert!(feed.events.len() <= MAX_PROGRESS_FEED_EVENTS_PER_GROUP);
    }

    #[test]
    fn rate_limit_survives_history_eviction_and_alternating_members() {
        let source = character(1);
        let partner = character(2);
        let username = Username::new("source").unwrap();
        let partner_username = Username::new("partner").unwrap();
        let mut feed = ProgressFeedState::default();
        for sequence in 1..=MAX_PROGRESS_FEED_EVENTS_PER_MINUTE as u32 {
            record_feed_event(
                &mut feed,
                source,
                0,
                partner,
                username.clone(),
                observation(sequence, sequence as u16, 7),
                100,
            )
            .unwrap();
            record_feed_event(
                &mut feed,
                partner,
                1,
                source,
                partner_username.clone(),
                observation(sequence, sequence as u16, 7),
                100,
            )
            .unwrap();
        }
        assert!(feed.events.len() < MAX_PROGRESS_FEED_EVENTS_PER_MINUTE * 2);
        assert_eq!(
            record_feed_event(
                &mut feed,
                source,
                0,
                partner,
                username,
                observation(61, 61, 7),
                100,
            )
            .unwrap_err(),
            Phase2Error::Busy
        );
    }

    #[test]
    fn group_membership_is_required_before_recording() {
        let state = State::default();
        assert_eq!(
            active_group_for_actor(&state, character(1)).unwrap_err(),
            Phase2Error::Forbidden
        );
    }

    #[test]
    fn replay_excludes_the_callers_own_events() {
        let source = character(1);
        let partner = character(2);
        let username = Username::new("source").unwrap();
        let mut feed = ProgressFeedState::default();
        record_feed_event(
            &mut feed,
            source,
            0,
            partner,
            username.clone(),
            observation(1, 25, 7),
            100,
        )
        .unwrap();
        record_feed_event(
            &mut feed,
            partner,
            1,
            source,
            username,
            observation(1, 26, 7),
            101,
        )
        .unwrap();
        let replay = partner_events(&feed, source);
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].source_character_id, partner);
    }

    #[test]
    fn pruning_removes_closed_group_feed_state() {
        let mut state = State::default();
        let group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(10)).unwrap();
        let active_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(11)).unwrap();
        state
            .group_progress_feeds
            .insert(group_id, ProgressFeedState::default());
        state
            .group_progress_feeds
            .insert(active_id, ProgressFeedState::default());
        state.groups.insert(
            group_id,
            crate::phase2::storage::GroupRecord {
                group: coop_cloud::Group::new(character(1), character(2)).unwrap(),
                zone: coop_protocol::WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 0).unwrap(),
                status: GroupStatus::Closed,
                zone_revision: 0,
            },
        );
        state.groups.insert(
            active_id,
            crate::phase2::storage::GroupRecord {
                group: coop_cloud::Group::new(character(1), character(2)).unwrap(),
                zone: coop_protocol::WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 0).unwrap(),
                status: GroupStatus::Active,
                zone_revision: 0,
            },
        );
        crate::phase2::storage::prune_closed_progress_feeds(&mut state);
        assert!(!state.group_progress_feeds.contains_key(&group_id));
        assert!(state.group_progress_feeds.contains_key(&active_id));
    }
}

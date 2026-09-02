//! In-memory HTTP/domain adapter for the first `PokéCrossroads` co-op slice.
//!
//! This crate intentionally keeps persistence out of the vertical slice. The
//! state is guarded by one process-local lock, which makes domain transitions
//! atomic and leaves an explicit adapter boundary for a future repository.

pub mod phase2;

pub use phase2::{
    AuthenticatedActor, Phase2App, Phase2Config, Phase2Error, PresenceConnection, PresenceDrain,
    PresenceOutboundV1, PresenceService, PresenceServiceError, PresenceSubmitOutcome,
    PresenceTickReport, ProductionConfig, ValidatedInteraction, serve_phase2_local,
};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{
        FromRequest, FromRequestParts, Path, Query, Request, State, rejection::JsonRejection,
    },
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use coop_protocol::{
    Group, ParticipantProgress, ProtocolError, RegionId, RegionalProgress, TrainerInstanceId,
    TravelRoute, WorldZone, group_battle_tier,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use uuid::Uuid;

/// The first slice supports the same two-member group shape as the protocol.
pub const MAX_GROUP_MEMBERS: usize = 2;
/// Commit hashes are intentionally bounded at the HTTP/domain boundary.
pub const MAX_FINAL_STATE_HASH_BYTES: usize = 256;
/// Pending invitations expire after this many seconds of monotonic time.
pub const PENDING_RESERVATION_TTL_SECONDS: u64 = 15;
/// This adapter intentionally has no authentication and is for local/dev use.
pub const UNAUTHENTICATED_ADAPTER_IS_DEV_ONLY: bool = true;

/// Returns the configured level cap for a regional co-op tier.
///
/// Tiers above eight are deliberately not silently clamped: they require a
/// balancing decision before the server can safely run a battle at them.
#[must_use]
pub const fn level_cap_for_tier(tier: u8) -> Option<u8> {
    match tier {
        0 => Some(15),
        1 => Some(19),
        2 => Some(24),
        3 => Some(29),
        4 => Some(31),
        5 => Some(33),
        6 => Some(42),
        7 => Some(46),
        8 => Some(58),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct ParticipantRecord {
    progress: ParticipantProgress,
    zone: WorldZone,
}

#[derive(Clone, Debug)]
struct GroupRecord {
    group: Group,
    zone: WorldZone,
}

#[derive(Clone, Debug)]
struct ReservationRecord {
    battle_id: Uuid,
    group_id: Uuid,
    initiator: u64,
    trainer_id: TrainerInstanceId,
    battle_region: RegionId,
    tier: u8,
    level_cap: u8,
    roles: [ProgressRole; MAX_GROUP_MEMBERS],
    state: ReservationState,
    /// Enforcement deadline. `Instant` is deliberately never exposed or
    /// derived from client-provided wall-clock values.
    deadline: Instant,
    /// Unix projection retained only for the client-facing reservation view.
    expires_at_unix_seconds: u64,
}

#[derive(Clone, Debug)]
struct CommitRecord {
    request: CommitFingerprint,
    response: BattleCommitView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommitFingerprint {
    battle_id: Uuid,
    outcome: BattleOutcome,
    final_state_hash: Option<String>,
}

#[derive(Default)]
struct ServerState {
    participants: HashMap<u64, ParticipantRecord>,
    groups: HashMap<Uuid, GroupRecord>,
    active_group_by_member: HashMap<u64, Uuid>,
    routes: HashMap<String, RouteDefinition>,
    trainers: HashMap<TrainerInstanceId, TrainerDefinition>,
    reservations: HashMap<Uuid, ReservationRecord>,
    active_battle_by_member: HashMap<u64, Uuid>,
    commits_by_key: HashMap<String, CommitRecord>,
    commits_by_battle: HashMap<Uuid, CommitRecord>,
}

/// Cloneable handle to the process-local service state.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<ServerState>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Creates an empty service with the deterministic MVP route and trainer
    /// catalogs.
    ///
    /// # Panics
    ///
    /// Panics only if the compile-time MVP route or trainer identifiers are
    /// accidentally changed to invalid protocol values.
    #[must_use]
    pub fn new() -> Self {
        let hoenn_to_kanto = RouteDefinition::new(
            "HOENN_TO_KANTO",
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
                .expect("MVP departure zone is valid"),
            WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).expect("MVP arrival zone is valid"),
            8,
            0,
        )
        .expect("MVP route constants are valid");
        let kanto_to_hoenn = RouteDefinition::new(
            "KANTO_TO_HOENN",
            WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).expect("MVP departure zone is valid"),
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
                .expect("MVP arrival zone is valid"),
            8,
            0,
        )
        .expect("MVP route constants are valid");
        let trainers = vec![
            TrainerDefinition::new(
                TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK")
                    .expect("MVP trainer constant is valid"),
                WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1)
                    .expect("MVP trainer zone is valid"),
                0,
            ),
            TrainerDefinition::new(
                TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1")
                    .expect("MVP trainer constant is valid"),
                WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
                    .expect("MVP trainer zone is valid"),
                0,
            ),
        ];
        Self::with_catalog(vec![hoenn_to_kanto, kanto_to_hoenn], trainers)
    }

    /// Creates a service with caller-owned route and trainer catalogs.
    ///
    /// Catalog entries are copied into the process-local service. Duplicate
    /// route and trainer IDs are rejected to avoid silently changing a
    /// destination or prerequisite.
    ///
    /// # Panics
    ///
    /// Panics if the supplied trainer catalog contains duplicate IDs. Use
    /// [`Self::try_with_catalog`] when catalog data is external.
    #[must_use]
    pub fn with_catalog(routes: Vec<RouteDefinition>, trainers: Vec<TrainerDefinition>) -> Self {
        Self::try_with_catalog(routes, trainers)
            .expect("trainer catalog must not contain duplicates")
    }

    /// Fallible variant of [`Self::with_catalog`] for external catalog data.
    ///
    /// # Errors
    ///
    /// Returns an error if the catalog repeats a trainer identity.
    pub fn try_with_catalog(
        routes: Vec<RouteDefinition>,
        trainers: Vec<TrainerDefinition>,
    ) -> Result<Self, ServiceError> {
        let mut trainer_map = HashMap::with_capacity(trainers.len());
        for trainer in trainers {
            trainer.zone.validate()?;
            if trainer.zone.region != trainer.trainer_id.region() {
                return Err(ServiceError::InvalidRequest {
                    code: "trainer_zone_region_mismatch",
                    message: format!(
                        "trainer {} is cataloged in the wrong region",
                        trainer.trainer_id
                    ),
                });
            }
            if trainer_map
                .insert(trainer.trainer_id.clone(), trainer)
                .is_some()
            {
                return Err(ServiceError::InvalidRequest {
                    code: "duplicate_trainer_catalog_entry",
                    message: "trainer catalog contains a duplicate region-qualified ID".to_owned(),
                });
            }
        }
        let mut route_map = HashMap::with_capacity(routes.len());
        for route in routes {
            if route_map.insert(route.route_id.clone(), route).is_some() {
                return Err(ServiceError::InvalidRequest {
                    code: "duplicate_route_catalog_entry",
                    message: "route catalog contains a duplicate route ID".to_owned(),
                });
            }
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(ServerState {
                routes: route_map,
                trainers: trainer_map,
                ..ServerState::default()
            })),
        })
    }

    /// Registers a character snapshot together with its authoritative world
    /// zone. The zone is server-owned and is not inferred from client input.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot or zone is invalid, regional
    /// progress is missing for the zone, or the character already exists.
    pub fn register_participant(
        &self,
        participant: ParticipantProgress,
        zone: WorldZone,
    ) -> Result<(), ServiceError> {
        participant.validate()?;
        zone.validate()?;
        if participant.progress_for(zone.region).is_none() {
            return Err(ServiceError::InvalidRequest {
                code: "missing_regional_progress",
                message: format!(
                    "participant {} has no progress record for {}",
                    participant.character_id(),
                    zone.region
                ),
            });
        }
        let character_id = participant.character_id();
        let mut state = self.write_state()?;
        if state.participants.contains_key(&character_id) {
            return Err(ServiceError::Conflict {
                code: "participant_already_registered",
                message: format!("participant {character_id} is already registered"),
            });
        }
        state.participants.insert(
            character_id,
            ParticipantRecord {
                progress: participant,
                zone,
            },
        );
        Ok(())
    }

    /// Returns a registered character snapshot without exposing mutable zone
    /// ownership to callers.
    ///
    /// # Errors
    ///
    /// Returns an error when the character is not registered or state cannot
    /// be locked.
    pub fn participant(&self, character_id: u64) -> Result<ParticipantProgress, ServiceError> {
        let state = self.read_state()?;
        state
            .participants
            .get(&character_id)
            .map(|record| record.progress.clone())
            .ok_or(ServiceError::NotFound {
                code: "participant_not_found",
                message: format!("participant {character_id} is not registered"),
            })
    }

    /// Returns the server-authoritative zone for a registered character.
    ///
    /// # Errors
    ///
    /// Returns an error when the character is not registered or state cannot
    /// be locked.
    pub fn participant_zone(&self, character_id: u64) -> Result<WorldZone, ServiceError> {
        let state = self.read_state()?;
        state
            .participants
            .get(&character_id)
            .map(|record| record.zone.clone())
            .ok_or(ServiceError::NotFound {
                code: "participant_not_found",
                message: format!("participant {character_id} is not registered"),
            })
    }

    /// Returns one atomically captured participant progress-and-zone view.
    ///
    /// # Errors
    ///
    /// Returns an error when the character is not registered or state cannot
    /// be locked.
    pub fn participant_view(&self, character_id: u64) -> Result<ParticipantView, ServiceError> {
        let state = self.read_state()?;
        let record = state
            .participants
            .get(&character_id)
            .ok_or(ServiceError::NotFound {
                code: "participant_not_found",
                message: format!("participant {character_id} is not registered"),
            })?;
        Ok(ParticipantView {
            character_id,
            progress: record.progress.clone(),
            zone: record.zone.clone(),
        })
    }

    /// Expires pending reservations using the process monotonic clock and
    /// releases both member battle locks for every expired invitation.
    ///
    /// # Errors
    ///
    /// Returns an error if the state lock is unavailable.
    pub fn expire_pending_battles(&self) -> Result<usize, ServiceError> {
        let mut state = self.write_state()?;
        Ok(Self::cleanup_expired_locked(&mut state, Instant::now()))
    }

    #[cfg(test)]
    fn expire_pending_battles_at(&self, now: Instant) -> Result<usize, ServiceError> {
        let mut state = self.write_state()?;
        Ok(Self::cleanup_expired_locked(&mut state, now))
    }

    fn cleanup_expired_locked(state: &mut ServerState, now: Instant) -> usize {
        let expired: Vec<Uuid> = state
            .reservations
            .values()
            .filter(|reservation| {
                reservation.state == ReservationState::Pending && reservation.deadline <= now
            })
            .map(|reservation| reservation.battle_id)
            .collect();
        for battle_id in &expired {
            if let Some(reservation) = state.reservations.remove(battle_id)
                && let Some(group) = state.groups.get(&reservation.group_id)
            {
                for character_id in group.group.members() {
                    state.active_battle_by_member.remove(&character_id);
                }
            }
        }
        expired.len()
    }

    /// Creates a symmetric exactly-two-member group. Its current zone is
    /// derived only after both registered character zones match exactly.
    ///
    /// # Errors
    ///
    /// Returns an error when IDs are equal, either character is absent, zones
    /// differ, or either character already belongs to an active group.
    pub fn create_group(
        &self,
        first_character_id: u64,
        second_character_id: u64,
    ) -> Result<GroupView, ServiceError> {
        let group = Group::new(first_character_id, second_character_id)?;
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let members = group.members();
        let first = state
            .participants
            .get(&members[0])
            .ok_or(ServiceError::NotFound {
                code: "participant_not_found",
                message: format!("participant {} is not registered", members[0]),
            })?;
        let second = state
            .participants
            .get(&members[1])
            .ok_or(ServiceError::NotFound {
                code: "participant_not_found",
                message: format!("participant {} is not registered", members[1]),
            })?;
        if first.zone != second.zone {
            return Err(ServiceError::Conflict {
                code: "participant_zones_do_not_match",
                message: "both participants must be in the same authoritative zone".to_owned(),
            });
        }
        for character_id in members {
            if let Some(existing_group) = state.active_group_by_member.get(&character_id) {
                return Err(ServiceError::Conflict {
                    code: "participant_already_grouped",
                    message: format!(
                        "participant {character_id} already belongs to group {existing_group}"
                    ),
                });
            }
        }
        let zone = first.zone.clone();
        let group_id = Uuid::new_v4();
        state.groups.insert(
            group_id,
            GroupRecord {
                group,
                zone: zone.clone(),
            },
        );
        for character_id in members {
            state.active_group_by_member.insert(character_id, group_id);
        }
        Ok(GroupView::new(group_id, group, zone))
    }

    /// Returns the current group view.
    ///
    /// # Errors
    ///
    /// Returns an error when the group does not exist or state cannot be
    /// locked.
    pub fn group(&self, group_id: Uuid) -> Result<GroupView, ServiceError> {
        let state = self.read_state()?;
        let record = state.groups.get(&group_id).ok_or(ServiceError::NotFound {
            code: "group_not_found",
            message: format!("group {group_id} does not exist"),
        })?;
        Ok(GroupView::new(group_id, record.group, record.zone.clone()))
    }

    /// Atomically transfers a group using a server-owned route ID. The route
    /// owns exact departure and arrival zones; both participant zones and the
    /// group zone change together on success.
    ///
    /// # Errors
    ///
    /// Returns an error when a member is in a battle, no catalog route exists,
    /// or any protocol entitlement check fails. All zones are unchanged on
    /// failure.
    #[allow(clippy::too_many_lines)]
    pub fn travel(&self, group_id: Uuid, route_id: &str) -> Result<GroupView, ServiceError> {
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let group_record = state.groups.get(&group_id).ok_or(ServiceError::NotFound {
            code: "group_not_found",
            message: format!("group {group_id} does not exist"),
        })?;
        let group = group_record.group;
        let source_zone = group_record.zone.clone();
        let members = group.members();
        for character_id in members {
            if let Some(battle_id) = state.active_battle_by_member.get(&character_id) {
                return Err(ServiceError::Conflict {
                    code: "travel_during_battle",
                    message: format!("participant {character_id} is in battle {battle_id}"),
                });
            }
        }
        let route = state
            .routes
            .get(route_id)
            .cloned()
            .ok_or(ServiceError::Forbidden {
                code: "route_not_available",
                message: format!("route {route_id} is not in the authoritative catalog"),
            })?;
        if source_zone != route.departure {
            return Err(ServiceError::Forbidden {
                code: "route_departure_mismatch",
                message: format!("group is at {source_zone:?}, not at the route departure zone"),
            });
        }
        let destination = route.arrival.clone();
        let participants = [
            state
                .participants
                .get(&members[0])
                .ok_or(ServiceError::NotFound {
                    code: "participant_not_found",
                    message: format!("participant {} is not registered", members[0]),
                })?
                .clone(),
            state
                .participants
                .get(&members[1])
                .ok_or(ServiceError::NotFound {
                    code: "participant_not_found",
                    message: format!("participant {} is not registered", members[1]),
                })?
                .clone(),
        ];
        if participants.iter().any(|record| record.zone != source_zone) {
            return Err(ServiceError::Conflict {
                code: "group_zone_state_mismatch",
                message: "group and participant authoritative zones disagree".to_owned(),
            });
        }
        if participants
            .iter()
            .any(|record| record.progress.progress_for(destination.region).is_none())
        {
            return Err(ServiceError::InvalidRequest {
                code: "missing_destination_regional_progress",
                message: format!(
                    "every participant needs progress for {} before travel",
                    destination.region
                ),
            });
        }
        let mut candidate_zone = source_zone;
        group
            .transfer(
                &mut candidate_zone,
                destination.clone(),
                &participants
                    .iter()
                    .map(|record| record.progress.clone())
                    .collect::<Vec<_>>(),
                &route.route,
            )
            .map_err(ServiceError::from_protocol)?;
        // Every fallible check is complete before these cross-record writes,
        // preserving the transition atomically even if the in-memory state is
        // ever corrupted by a future adapter.
        if !state.groups.contains_key(&group_id)
            || members
                .iter()
                .any(|character_id| !state.participants.contains_key(character_id))
        {
            return Err(ServiceError::StateUnavailable);
        }
        {
            let group_record = state
                .groups
                .get_mut(&group_id)
                .ok_or(ServiceError::StateUnavailable)?;
            group_record.zone = candidate_zone;
        }
        for character_id in members {
            state
                .participants
                .get_mut(&character_id)
                .ok_or(ServiceError::StateUnavailable)?
                .zone = destination.clone();
        }
        Ok(GroupView::new(group_id, group, destination))
    }

    /// Reserves a known region-qualified trainer. The returned reservation is
    /// `PENDING` until the companion explicitly accepts it.
    ///
    /// # Errors
    ///
    /// Returns an error for fabricated trainers, story-ineligible members,
    /// wrong-region trainers, missing progress, active groups/battles, or an
    /// initiator who already cleared the trainer.
    #[allow(clippy::too_many_lines)]
    pub fn reserve_battle(
        &self,
        group_id: Uuid,
        initiator: u64,
        trainer_id: TrainerInstanceId,
    ) -> Result<BattleReservationView, ServiceError> {
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let trainer = state
            .trainers
            .get(&trainer_id)
            .cloned()
            .ok_or(ServiceError::NotFound {
                code: "trainer_not_found",
                message: format!("trainer {trainer_id} is not in the authoritative catalog"),
            })?;
        let group_record = state.groups.get(&group_id).ok_or(ServiceError::NotFound {
            code: "group_not_found",
            message: format!("group {group_id} does not exist"),
        })?;
        let group = group_record.group;
        let zone = group_record.zone.clone();
        let members = group.members();
        if !group.contains(initiator) {
            return Err(ServiceError::Forbidden {
                code: "group_membership_mismatch",
                message: format!("participant {initiator} is not a member of group {group_id}"),
            });
        }
        for character_id in members {
            if let Some(existing_battle) = state.active_battle_by_member.get(&character_id) {
                return Err(ServiceError::Conflict {
                    code: "battle_already_active",
                    message: format!(
                        "participant {character_id} already has battle {existing_battle}"
                    ),
                });
            }
        }
        if zone.region != trainer_id.region() {
            return Err(ServiceError::Forbidden {
                code: "trainer_region_mismatch",
                message: format!(
                    "trainer {trainer_id} belongs to {}, but the group is in {}",
                    trainer_id.region(),
                    zone.region
                ),
            });
        }
        if zone != trainer.zone {
            return Err(ServiceError::Forbidden {
                code: "trainer_zone_mismatch",
                message: format!(
                    "trainer {trainer_id} is only reservable at its cataloged logical zone"
                ),
            });
        }
        let participants = [
            state
                .participants
                .get(&members[0])
                .ok_or(ServiceError::NotFound {
                    code: "participant_not_found",
                    message: format!("participant {} is not registered", members[0]),
                })?
                .clone(),
            state
                .participants
                .get(&members[1])
                .ok_or(ServiceError::NotFound {
                    code: "participant_not_found",
                    message: format!("participant {} is not registered", members[1]),
                })?
                .clone(),
        ];
        if participants.iter().any(|record| record.zone != zone) {
            return Err(ServiceError::Conflict {
                code: "group_zone_state_mismatch",
                message: "group and participant authoritative zones disagree".to_owned(),
            });
        }
        for record in &participants {
            let progress = record.progress.progress_for(trainer_id.region()).ok_or(
                ProtocolError::MissingRegionalProgress {
                    character_id: record.progress.character_id(),
                    region: trainer_id.region(),
                },
            )?;
            if progress.story_checkpoint < trainer.minimum_story_checkpoint {
                return Err(ServiceError::Forbidden {
                    code: "story_prerequisite_not_met",
                    message: format!(
                        "participant {} has not reached the story checkpoint for {trainer_id}",
                        record.progress.character_id()
                    ),
                });
            }
        }
        let tier = group_battle_tier(
            &participants
                .iter()
                .map(|record| record.progress.clone())
                .collect::<Vec<_>>(),
            trainer_id.region(),
        )
        .map_err(ServiceError::from_protocol)?;
        let level_cap = level_cap_for_tier(tier).ok_or(ServiceError::InvalidRequest {
            code: "unsupported_battle_tier",
            message: format!("regional tier {tier} has no configured level cap"),
        })?;
        let roles = [
            role_for(
                &participants[0].progress,
                trainer_id.region(),
                &trainer_id,
                members[0],
                initiator,
            )?,
            role_for(
                &participants[1].progress,
                trainer_id.region(),
                &trainer_id,
                members[1],
                initiator,
            )?,
        ];
        let battle_id = Uuid::new_v4();
        let battle_region = trainer_id.region();
        let now = Instant::now();
        let reservation = ReservationRecord {
            battle_id,
            group_id,
            initiator,
            trainer_id,
            battle_region,
            tier,
            level_cap,
            roles,
            state: ReservationState::Pending,
            deadline: now + Duration::from_secs(PENDING_RESERVATION_TTL_SECONDS),
            expires_at_unix_seconds: unix_expiry_seconds(PENDING_RESERVATION_TTL_SECONDS)?,
        };
        for character_id in members {
            state
                .active_battle_by_member
                .insert(character_id, battle_id);
        }
        state.reservations.insert(battle_id, reservation.clone());
        Ok(reservation.view(members))
    }

    /// Accepts a pending reservation by the non-initiating companion.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-member, the initiator, a repeated acceptance,
    /// or an unknown battle.
    pub fn accept_battle(
        &self,
        battle_id: Uuid,
        accepter: u64,
    ) -> Result<BattleReservationView, ServiceError> {
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let reservation =
            state
                .reservations
                .get(&battle_id)
                .cloned()
                .ok_or(ServiceError::NotFound {
                    code: "battle_not_found",
                    message: format!("battle {battle_id} does not have an active reservation"),
                })?;
        let members = reservation.group_members(&state)?;
        if !members.contains(&accepter) {
            return Err(ServiceError::Forbidden {
                code: "group_membership_mismatch",
                message: format!("participant {accepter} is not a member of battle {battle_id}"),
            });
        }
        if accepter == reservation.initiator {
            return Err(ServiceError::Forbidden {
                code: "initiator_cannot_accept",
                message: "only the other group member can accept the invitation".to_owned(),
            });
        }
        if reservation.state == ReservationState::Ready {
            return Err(ServiceError::Conflict {
                code: "battle_already_ready",
                message: format!("battle {battle_id} was already accepted"),
            });
        }
        let stored = state
            .reservations
            .get_mut(&battle_id)
            .ok_or(ServiceError::StateUnavailable)?;
        stored.state = ReservationState::Ready;
        Ok(stored.view(members))
    }

    /// Cancels a pending or ready reservation by either group member. This is
    /// the companion decline path and releases both member locks immediately.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown battle or a caller outside its group.
    pub fn decline_battle(
        &self,
        battle_id: Uuid,
        requester: u64,
    ) -> Result<BattleCancellationView, ServiceError> {
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let reservation =
            state
                .reservations
                .get(&battle_id)
                .cloned()
                .ok_or(ServiceError::NotFound {
                    code: "battle_not_found",
                    message: format!("battle {battle_id} does not have an active reservation"),
                })?;
        let members = reservation.group_members(&state)?;
        if !members.contains(&requester) {
            return Err(ServiceError::Forbidden {
                code: "group_membership_mismatch",
                message: format!("participant {requester} is not a member of battle {battle_id}"),
            });
        }
        state.reservations.remove(&battle_id);
        for character_id in members {
            state.active_battle_by_member.remove(&character_id);
        }
        Ok(BattleCancellationView { battle_id })
    }

    /// Alias for callers that describe a cancellation rather than an invite
    /// decline.
    ///
    /// # Errors
    ///
    /// Propagates the authorization and lookup errors from
    /// [`Self::decline_battle`].
    pub fn cancel_battle(
        &self,
        battle_id: Uuid,
        requester: u64,
    ) -> Result<BattleCancellationView, ServiceError> {
        self.decline_battle(battle_id, requester)
    }

    /// Commits a ready battle once, applying only first-clear candidates.
    /// Reusing the same idempotency key with the same request returns the
    /// original receipt and does not mutate progress a second time.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid hash/key, a pending or unknown battle,
    /// or a conflicting reuse of a key. Won commits require a bounded,
    /// non-empty final state hash.
    #[allow(clippy::too_many_lines)]
    pub fn commit_battle(
        &self,
        battle_id: Uuid,
        request: BattleCommitRequest,
    ) -> Result<BattleCommitResponse, ServiceError> {
        validate_idempotency_key(&request.idempotency_key)?;
        validate_final_state_hash(request.outcome, request.final_state_hash.as_deref())?;
        let mut state = self.write_state()?;
        Self::cleanup_expired_locked(&mut state, Instant::now());
        let fingerprint = CommitFingerprint {
            battle_id,
            outcome: request.outcome,
            final_state_hash: request.final_state_hash.clone(),
        };
        if let Some(existing) = state.commits_by_key.get(&request.idempotency_key) {
            if existing.request == fingerprint {
                return Ok(BattleCommitResponse {
                    commit: existing.response.clone(),
                    replayed: true,
                });
            }
            return Err(ServiceError::Conflict {
                code: "idempotency_key_conflict",
                message: "idempotency key was already used for a different commit request"
                    .to_owned(),
            });
        }
        if state.commits_by_battle.contains_key(&battle_id) {
            return Err(ServiceError::Conflict {
                code: "battle_already_committed",
                message: format!("battle {battle_id} is already committed"),
            });
        }
        let reservation =
            state
                .reservations
                .get(&battle_id)
                .cloned()
                .ok_or(ServiceError::NotFound {
                    code: "battle_not_found",
                    message: format!("battle {battle_id} does not have an active reservation"),
                })?;
        if reservation.state != ReservationState::Ready {
            return Err(ServiceError::Conflict {
                code: "companion_acceptance_required",
                message: "the companion must accept before battle commit".to_owned(),
            });
        }
        let members = reservation.group_members(&state)?;
        let mut replacements = Vec::with_capacity(MAX_GROUP_MEMBERS);
        let mut participant_results = Vec::with_capacity(MAX_GROUP_MEMBERS);
        // Build and validate every replacement before mutating any participant.
        // This is the failure-atomic boundary for progress application.
        for (index, character_id) in members.into_iter().enumerate() {
            let mark_trainer_defeated = request.outcome == BattleOutcome::Won
                && reservation.roles[index] == ProgressRole::FirstClearCandidate;
            if mark_trainer_defeated {
                let participant = state
                    .participants
                    .get(&character_id)
                    .ok_or(ServiceError::StateUnavailable)?;
                let replacement =
                    mark_trainer_defeated_for(&participant.progress, &reservation.trainer_id)?;
                replacement.validate()?;
                replacements.push((character_id, replacement));
            }
            participant_results.push(BattleParticipantResult {
                character_id,
                role: reservation.roles[index],
                mark_trainer_defeated,
            });
        }
        let response = BattleCommitView {
            commit_id: Uuid::new_v4(),
            battle_id,
            group_id: reservation.group_id,
            trainer_id: reservation.trainer_id.clone(),
            battle_region: reservation.battle_region,
            tier: reservation.tier,
            level_cap: reservation.level_cap,
            outcome: request.outcome,
            participants: participant_results,
        };
        let record = CommitRecord {
            request: fingerprint,
            response: response.clone(),
        };
        if replacements
            .iter()
            .any(|(character_id, _)| !state.participants.contains_key(character_id))
        {
            return Err(ServiceError::StateUnavailable);
        }
        for (character_id, replacement) in replacements {
            state
                .participants
                .get_mut(&character_id)
                .ok_or(ServiceError::StateUnavailable)?
                .progress = replacement;
        }
        state
            .commits_by_key
            .insert(request.idempotency_key, record.clone());
        state.commits_by_battle.insert(battle_id, record);
        state.reservations.remove(&battle_id);
        for character_id in members {
            state.active_battle_by_member.remove(&character_id);
        }
        Ok(BattleCommitResponse {
            commit: response,
            replayed: false,
        })
    }

    fn read_state(&self) -> Result<std::sync::RwLockReadGuard<'_, ServerState>, ServiceError> {
        self.inner
            .read()
            .map_err(|_| ServiceError::StateUnavailable)
    }

    fn write_state(&self) -> Result<std::sync::RwLockWriteGuard<'_, ServerState>, ServiceError> {
        self.inner
            .write()
            .map_err(|_| ServiceError::StateUnavailable)
    }
}

/// A server-known trainer identity, exact logical battle zone, and story
/// prerequisite. Channel equality is intentionally strict in this slice.
#[derive(Clone, Debug)]
pub struct TrainerDefinition {
    trainer_id: TrainerInstanceId,
    zone: WorldZone,
    minimum_story_checkpoint: u32,
}

impl TrainerDefinition {
    /// Creates a catalog definition for a known trainer at an exact zone.
    #[must_use]
    pub const fn new(
        trainer_id: TrainerInstanceId,
        zone: WorldZone,
        minimum_story_checkpoint: u32,
    ) -> Self {
        Self {
            trainer_id,
            zone,
            minimum_story_checkpoint,
        }
    }

    /// Returns this definition's qualified trainer ID.
    #[must_use]
    pub fn trainer_id(&self) -> &TrainerInstanceId {
        &self.trainer_id
    }

    /// Returns the exact logical zone where this trainer may be reserved.
    #[must_use]
    pub const fn zone(&self) -> &WorldZone {
        &self.zone
    }
}

/// A server-owned route identity with exact departure and arrival zones.
#[derive(Clone, Debug)]
pub struct RouteDefinition {
    route_id: String,
    departure: WorldZone,
    arrival: WorldZone,
    route: TravelRoute,
}

impl RouteDefinition {
    /// Creates a route whose thresholds are controlled by the catalog.
    ///
    /// # Errors
    ///
    /// Returns an error when the route ID is empty, either zone is invalid, or
    /// the zones do not match the route's regional endpoints.
    pub fn new(
        route_id: impl Into<String>,
        departure: WorldZone,
        arrival: WorldZone,
        minimum_badges: u8,
        minimum_story_checkpoint: u32,
    ) -> Result<Self, ServiceError> {
        let route_id = route_id.into();
        validate_catalog_id(&route_id, "route_id")?;
        departure.validate()?;
        arrival.validate()?;
        let route = TravelRoute::new(
            departure.region,
            arrival.region,
            minimum_badges,
            minimum_story_checkpoint,
        )?;
        Ok(Self {
            route_id,
            departure,
            arrival,
            route,
        })
    }

    /// Returns this route's server-owned ID.
    #[must_use]
    pub fn route_id(&self) -> &str {
        &self.route_id
    }
}

impl ReservationRecord {
    fn group_members(&self, state: &ServerState) -> Result<[u64; MAX_GROUP_MEMBERS], ServiceError> {
        state
            .groups
            .get(&self.group_id)
            .map(|record| record.group.members())
            .ok_or(ServiceError::StateUnavailable)
    }

    fn view(&self, members: [u64; MAX_GROUP_MEMBERS]) -> BattleReservationView {
        BattleReservationView {
            battle_id: self.battle_id,
            group_id: self.group_id,
            initiator: self.initiator,
            trainer_id: self.trainer_id.clone(),
            battle_region: self.battle_region,
            tier: self.tier,
            level_cap: self.level_cap,
            state: self.state,
            expires_at_unix_seconds: self.expires_at_unix_seconds,
            participants: [
                BattleParticipantView {
                    character_id: members[0],
                    role: self.roles[0],
                },
                BattleParticipantView {
                    character_id: members[1],
                    role: self.roles[1],
                },
            ],
        }
    }
}

fn role_for(
    participant: &ParticipantProgress,
    region: RegionId,
    trainer_id: &TrainerInstanceId,
    character_id: u64,
    initiator: u64,
) -> Result<ProgressRole, ServiceError> {
    let progress =
        participant
            .progress_for(region)
            .ok_or(ProtocolError::MissingRegionalProgress {
                character_id,
                region,
            })?;
    let defeated = progress
        .defeated_trainers
        .iter()
        .any(|trainer| trainer == trainer_id);
    if defeated && character_id == initiator {
        return Err(ServiceError::Conflict {
            code: "trainer_already_defeated",
            message: format!("initiator {character_id} has already defeated trainer {trainer_id}"),
        });
    }
    Ok(if defeated {
        ProgressRole::RepeatHelper
    } else {
        ProgressRole::FirstClearCandidate
    })
}

fn mark_trainer_defeated_for(
    participant: &ParticipantProgress,
    trainer_id: &TrainerInstanceId,
) -> Result<ParticipantProgress, ServiceError> {
    let region = trainer_id.region();
    let mut records = participant.regional_progress().to_vec();
    let index = records
        .iter()
        .position(|progress| progress.region == region)
        .ok_or(ProtocolError::MissingRegionalProgress {
            character_id: participant.character_id(),
            region,
        })?;
    let progress = &mut records[index];
    if !progress
        .defeated_trainers
        .iter()
        .any(|trainer| trainer == trainer_id)
    {
        progress.defeated_trainers.push(trainer_id.clone());
    }
    let updated = RegionalProgress::new(
        progress.region,
        progress.badge_mask,
        progress.story_checkpoint,
        progress.defeated_trainers.clone(),
        progress.unlocked_fly_points.clone(),
    )?;
    records[index] = updated;
    Ok(ParticipantProgress::new(
        participant.character_id(),
        records,
    )?)
}

fn validate_idempotency_key(key: &str) -> Result<(), ServiceError> {
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(ServiceError::InvalidRequest {
            code: "invalid_idempotency_key",
            message: "idempotency_key must contain 1 to 128 printable ASCII characters".to_owned(),
        });
    }
    Ok(())
}

fn validate_catalog_id(value: &str, field: &str) -> Result<(), ServiceError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ServiceError::InvalidRequest {
            code: "invalid_catalog_id",
            message: format!("{field} must be 1 to 64 uppercase ASCII characters"),
        });
    }
    Ok(())
}

fn unix_expiry_seconds(ttl_seconds: u64) -> Result<u64, ServiceError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ServiceError::StateUnavailable)?;
    now.as_secs()
        .checked_add(ttl_seconds)
        .ok_or(ServiceError::StateUnavailable)
}

fn validate_final_state_hash(
    outcome: BattleOutcome,
    hash: Option<&str>,
) -> Result<(), ServiceError> {
    if outcome == BattleOutcome::Won {
        let value = hash
            .filter(|value| !value.is_empty())
            .ok_or(ServiceError::InvalidRequest {
                code: "missing_final_state_hash",
                message: "Won commits require a non-empty final_state_hash".to_owned(),
            })?;
        if value.len() > MAX_FINAL_STATE_HASH_BYTES {
            return Err(ServiceError::InvalidRequest {
                code: "final_state_hash_too_large",
                message: format!("final_state_hash exceeds {MAX_FINAL_STATE_HASH_BYTES} bytes"),
            });
        }
    }
    Ok(())
}

/// JSON request for participant registration. The zone is intentionally
/// separate from the save payload because it is authoritative server state.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterParticipantRequest {
    pub participant: ParticipantProgress,
    pub zone: WorldZone,
}

/// JSON request for group creation. The zone is never client-supplied.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGroupRequest {
    #[serde(alias = "first")]
    pub first_character_id: u64,
    #[serde(alias = "second")]
    pub second_character_id: u64,
}

/// JSON request for an atomic group transfer.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TravelRequest {
    pub route_id: String,
}

/// JSON request for a trainer reservation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveBattleRequest {
    #[serde(alias = "initiator_character_id")]
    pub character_id: u64,
    #[serde(alias = "trainer_instance_id")]
    pub trainer_id: TrainerInstanceId,
}

/// JSON request for companion acceptance.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptBattleRequest {
    pub character_id: u64,
}

/// Final outcome accepted by the idempotent commit endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleOutcome {
    Won,
    Aborted,
}

/// JSON request for an idempotent battle commit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleCommitRequest {
    #[serde(alias = "commit_id")]
    pub idempotency_key: String,
    pub outcome: BattleOutcome,
    /// Required and bounded for WON commits. This slice carries the value but
    /// does not treat one client hash as authoritative; deterministic
    /// two-client hash validation belongs to the deferred lockstep boundary.
    #[serde(default)]
    pub final_state_hash: Option<String>,
}

/// Reservation readiness state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReservationState {
    Pending,
    Ready,
}

/// Progress role recorded in a reservation and echoed in its commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProgressRole {
    FirstClearCandidate,
    RepeatHelper,
}

/// Public state of a registered participant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ParticipantView {
    pub character_id: u64,
    pub progress: ParticipantProgress,
    pub zone: WorldZone,
}

/// Public state of a group.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroupView {
    pub group_id: Uuid,
    pub group: Group,
    pub zone: WorldZone,
}

impl GroupView {
    fn new(group_id: Uuid, group: Group, zone: WorldZone) -> Self {
        Self {
            group_id,
            group,
            zone,
        }
    }
}

/// Public reservation manifest for the MVP.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleReservationView {
    pub battle_id: Uuid,
    pub group_id: Uuid,
    pub initiator: u64,
    pub trainer_id: TrainerInstanceId,
    pub battle_region: RegionId,
    pub tier: u8,
    pub level_cap: u8,
    pub state: ReservationState,
    pub expires_at_unix_seconds: u64,
    pub participants: [BattleParticipantView; MAX_GROUP_MEMBERS],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleParticipantView {
    pub character_id: u64,
    pub role: ProgressRole,
}

/// Per-character progress effect in a committed battle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleParticipantResult {
    pub character_id: u64,
    pub role: ProgressRole,
    pub mark_trainer_defeated: bool,
}

/// Immutable battle commit receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleCommitView {
    pub commit_id: Uuid,
    pub battle_id: Uuid,
    pub group_id: Uuid,
    pub trainer_id: TrainerInstanceId,
    pub battle_region: RegionId,
    pub tier: u8,
    pub level_cap: u8,
    pub outcome: BattleOutcome,
    pub participants: Vec<BattleParticipantResult>,
}

/// Commit response. `replayed` distinguishes a retry from first application.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleCommitResponse {
    pub commit: BattleCommitView,
    pub replayed: bool,
}

/// Receipt for a declined or canceled reservation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BattleCancellationView {
    pub battle_id: Uuid,
}

/// Service/domain errors mapped to typed JSON responses by the HTTP adapter.
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid request: {message}")]
    InvalidRequest { code: &'static str, message: String },
    #[error("resource not found: {message}")]
    NotFound { code: &'static str, message: String },
    #[error("request is forbidden: {message}")]
    Forbidden { code: &'static str, message: String },
    #[error("request conflicts with current state: {message}")]
    Conflict { code: &'static str, message: String },
    #[error("HTTP request rejected: {message}")]
    HttpStatus {
        status: StatusCode,
        code: &'static str,
        message: String,
    },
    #[error("protocol invariant failed: {0}")]
    Protocol(ProtocolError),
    #[error("service state is unavailable")]
    StateUnavailable,
}

impl ServiceError {
    fn from_protocol(error: ProtocolError) -> Self {
        match error {
            ProtocolError::TravelDenied { .. }
            | ProtocolError::SourceZoneMismatch { .. }
            | ProtocolError::DestinationZoneMismatch { .. }
            | ProtocolError::IdentityRegionMismatch { .. } => Self::Forbidden {
                code: "protocol_boundary_denied",
                message: error.to_string(),
            },
            ProtocolError::MissingRegionalProgress { .. } => Self::InvalidRequest {
                code: "missing_regional_progress",
                message: error.to_string(),
            },
            other => Self::Protocol(other),
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest { .. } | Self::Protocol(_) => StatusCode::BAD_REQUEST,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::HttpStatus { status, .. } => *status,
            Self::StateUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest { code, .. }
            | Self::NotFound { code, .. }
            | Self::Forbidden { code, .. }
            | Self::Conflict { code, .. }
            | Self::HttpStatus { code, .. } => code,
            Self::Protocol(_) => "protocol_invariant_failed",
            Self::StateUnavailable => "state_unavailable",
        }
    }
}

impl From<ProtocolError> for ServiceError {
    fn from(error: ProtocolError) -> Self {
        Self::from_protocol(error)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
            },
        });
        (status, body).into_response()
    }
}

/// JSON extractor whose rejections use this crate's typed error envelope.
struct ApiJson<T>(T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + 'static,
{
    type Rejection = ServiceError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error: JsonRejection| ServiceError::HttpStatus {
                status: error.status(),
                code: "invalid_json",
                message: error.to_string(),
            })
    }
}

/// Path extractor whose rejections use this crate's typed error envelope.
struct ApiPath<T>(T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ServiceError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|error| ServiceError::HttpStatus {
                status: error.status(),
                code: "invalid_path",
                message: error.to_string(),
            })
    }
}

/// Query extractor whose rejections use this crate's typed error envelope.
struct ApiQuery<T>(T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ServiceError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|error| ServiceError::HttpStatus {
                status: error.status(),
                code: "invalid_query",
                message: error.to_string(),
            })
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn register_participant_http(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<RegisterParticipantRequest>,
) -> Result<(StatusCode, Json<ParticipantView>), ServiceError> {
    state.register_participant(request.participant.clone(), request.zone.clone())?;
    Ok((
        StatusCode::CREATED,
        Json(ParticipantView {
            character_id: request.participant.character_id(),
            progress: request.participant,
            zone: request.zone,
        }),
    ))
}

async fn get_participant_http(
    State(state): State<AppState>,
    ApiPath(character_id): ApiPath<u64>,
) -> Result<Json<ParticipantView>, ServiceError> {
    Ok(Json(state.participant_view(character_id)?))
}

async fn create_group_http(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupView>), ServiceError> {
    let view = state.create_group(request.first_character_id, request.second_character_id)?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn get_group_http(
    State(state): State<AppState>,
    ApiPath(group_id): ApiPath<Uuid>,
) -> Result<Json<GroupView>, ServiceError> {
    Ok(Json(state.group(group_id)?))
}

async fn travel_http(
    State(state): State<AppState>,
    ApiPath(group_id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<TravelRequest>,
) -> Result<Json<GroupView>, ServiceError> {
    Ok(Json(state.travel(group_id, &request.route_id)?))
}

async fn reserve_battle_http(
    State(state): State<AppState>,
    ApiPath(group_id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<ReserveBattleRequest>,
) -> Result<(StatusCode, Json<BattleReservationView>), ServiceError> {
    let view = state.reserve_battle(group_id, request.character_id, request.trainer_id)?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn accept_battle_http(
    State(state): State<AppState>,
    ApiPath(battle_id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AcceptBattleRequest>,
) -> Result<Json<BattleReservationView>, ServiceError> {
    Ok(Json(state.accept_battle(battle_id, request.character_id)?))
}

async fn decline_battle_http(
    State(state): State<AppState>,
    ApiPath(battle_id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AcceptBattleRequest>,
) -> Result<Json<BattleCancellationView>, ServiceError> {
    Ok(Json(state.decline_battle(battle_id, request.character_id)?))
}

async fn commit_battle_http(
    State(state): State<AppState>,
    ApiPath(battle_id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<BattleCommitRequest>,
) -> Result<Json<BattleCommitResponse>, ServiceError> {
    Ok(Json(state.commit_battle(battle_id, request)?))
}

#[derive(Deserialize)]
struct LevelCapQuery {
    tier: u8,
}

#[derive(Serialize)]
struct LevelCapView {
    region: RegionId,
    tier: u8,
    level_cap: u8,
}

async fn level_cap_http(
    ApiPath(region): ApiPath<RegionId>,
    ApiQuery(query): ApiQuery<LevelCapQuery>,
) -> Result<Json<LevelCapView>, ServiceError> {
    let level_cap = level_cap_for_tier(query.tier).ok_or(ServiceError::InvalidRequest {
        code: "unsupported_battle_tier",
        message: format!("regional tier {} has no configured level cap", query.tier),
    })?;
    region.ensure_concrete()?;
    Ok(Json(LevelCapView {
        region,
        tier: query.tier,
        level_cap,
    }))
}

async fn not_found_http() -> ServiceError {
    ServiceError::NotFound {
        code: "route_not_found",
        message: "the requested endpoint does not exist".to_owned(),
    }
}

async fn method_not_allowed_http() -> ServiceError {
    ServiceError::HttpStatus {
        status: StatusCode::METHOD_NOT_ALLOWED,
        code: "method_not_allowed",
        message: "the HTTP method is not supported for this endpoint".to_owned(),
    }
}

/// Builds the unauthenticated **internal/dev-only** router backed by a fresh
/// in-memory service. This adapter must not be exposed beyond loopback until
/// the deferred auth and lease boundary is implemented.
pub fn app() -> Router {
    app_with_state(AppState::new())
}

/// Builds the unauthenticated **internal/dev-only** HTTP adapter around
/// caller-owned state, useful for integration tests and local embedding.
pub fn app_with_state(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/participants", post(register_participant_http))
        .route("/v1/participants/{character_id}", get(get_participant_http))
        .route("/v1/groups", post(create_group_http))
        .route("/v1/groups/{group_id}", get(get_group_http))
        .route("/v1/groups/{group_id}/travel", post(travel_http))
        .route(
            "/v1/groups/{group_id}/battles/reserve",
            post(reserve_battle_http),
        )
        .route("/v1/battles/{battle_id}/accept", post(accept_battle_http))
        .route("/v1/battles/{battle_id}/decline", post(decline_battle_http))
        .route("/v1/battles/{battle_id}/commit", post(commit_battle_http))
        .route("/v1/regions/{region}/level-cap", get(level_cap_http))
        .method_not_allowed_fallback(method_not_allowed_http)
        .fallback(not_found_http)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn badges(count: u8) -> u16 {
        if count == 0 { 0 } else { (1u16 << count) - 1 }
    }

    fn progress(
        region: RegionId,
        badge_count: u8,
        story_checkpoint: u32,
        defeated: Vec<TrainerInstanceId>,
    ) -> RegionalProgress {
        RegionalProgress::new(
            region,
            badges(badge_count),
            story_checkpoint,
            defeated,
            vec![],
        )
        .expect("test progress is valid")
    }

    fn progress_with_mask(
        region: RegionId,
        badge_mask: u16,
        story_checkpoint: u32,
    ) -> RegionalProgress {
        RegionalProgress::new(region, badge_mask, story_checkpoint, vec![], vec![])
            .expect("test progress is valid")
    }

    fn participant(
        id: u64,
        hoenn_badges: u8,
        kanto_badges: u8,
        kanto_story: u32,
        defeated: Vec<TrainerInstanceId>,
    ) -> ParticipantProgress {
        ParticipantProgress::new(
            id,
            vec![
                progress(
                    RegionId::Hoenn,
                    hoenn_badges,
                    u32::from(hoenn_badges),
                    vec![],
                ),
                progress(RegionId::Kanto, kanto_badges, kanto_story, defeated),
            ],
        )
        .expect("test participant is valid")
    }

    fn participant_with_mask(id: u64, region: RegionId, badge_mask: u16) -> ParticipantProgress {
        let other_region = if region == RegionId::Hoenn {
            RegionId::Kanto
        } else {
            RegionId::Hoenn
        };
        ParticipantProgress::new(
            id,
            vec![
                progress_with_mask(region, badge_mask, 8),
                progress_with_mask(other_region, 0, 0),
            ],
        )
        .expect("test participant is valid")
    }

    fn zone(region: RegionId, map: &str) -> WorldZone {
        WorldZone::new(region, map, 1).expect("test zone is valid")
    }

    fn register(state: &AppState, _id: u64, progress: ParticipantProgress, zone: WorldZone) {
        state
            .register_participant(progress, zone)
            .expect("registration");
    }

    async fn json_request<T: Serialize>(
        router: Router,
        method: axum::http::Method,
        uri: &str,
        body: &T,
    ) -> axum::response::Response {
        router
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(body).expect("json")))
                    .expect("request"),
            )
            .await
            .expect("response")
    }

    async fn raw_request(router: Router, uri: &str, body: &str) -> axum::response::Response {
        router
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .expect("request"),
            )
            .await
            .expect("response")
    }

    async fn response_json<T: for<'de> Deserialize<'de>>(response: axum::response::Response) -> T {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("json response")
    }

    #[test]
    fn level_caps_match_balance_table() {
        assert_eq!(level_cap_for_tier(0), Some(15));
        assert_eq!(level_cap_for_tier(1), Some(19));
        assert_eq!(level_cap_for_tier(2), Some(24));
        assert_eq!(level_cap_for_tier(3), Some(29));
        assert_eq!(level_cap_for_tier(4), Some(31));
        assert_eq!(level_cap_for_tier(5), Some(33));
        assert_eq!(level_cap_for_tier(6), Some(42));
        assert_eq!(level_cap_for_tier(7), Some(46));
        assert_eq!(level_cap_for_tier(8), Some(58));
        assert_eq!(level_cap_for_tier(9), None);
    }

    #[test]
    fn duplicate_registration_does_not_overwrite_authoritative_state() {
        let state = AppState::new();
        register(
            &state,
            10,
            participant(10, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        let replacement = participant(10, 0, 0, 0, vec![]);
        let error = state.register_participant(replacement, zone(RegionId::Kanto, "PALLET_TOWN"));
        assert!(matches!(
            error,
            Err(ServiceError::Conflict {
                code: "participant_already_registered",
                ..
            })
        ));
        assert_eq!(
            state.participant_zone(10).expect("zone"),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN")
        );
        assert_eq!(
            state
                .participant(10)
                .expect("participant")
                .progress_for(RegionId::Hoenn)
                .expect("Hoenn")
                .badge_count(),
            8
        );
    }

    #[test]
    fn group_derives_matching_zone_and_rejects_client_zone_bypass() {
        let state = AppState::new();
        register(
            &state,
            10,
            participant(10, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        register(
            &state,
            11,
            participant(11, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        let group = state.create_group(10, 11).expect("group");
        assert_eq!(group.zone, zone(RegionId::Hoenn, "LITTLEROOT_TOWN"));
        register(
            &state,
            12,
            participant(12, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        let mismatch = state.create_group(10, 12);
        assert!(matches!(
            mismatch,
            Err(ServiceError::Conflict {
                code: "participant_zones_do_not_match",
                ..
            })
        ));
        assert_eq!(group.group.members(), [10, 11]);
    }

    #[test]
    fn travel_uses_catalog_threshold_and_updates_all_authoritative_zones() {
        let state = AppState::new();
        register(
            &state,
            10,
            participant(10, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        register(
            &state,
            11,
            participant(11, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        let group = state.create_group(10, 11).expect("group");
        let moved = state
            .travel(group.group_id, "HOENN_TO_KANTO")
            .expect("travel");
        assert_eq!(moved.zone, zone(RegionId::Kanto, "PALLET_TOWN"));
        assert_eq!(state.participant_zone(10).expect("zone"), moved.zone);
        assert_eq!(state.participant_zone(11).expect("zone"), moved.zone);
    }

    #[test]
    fn reserved_badge_bits_do_not_authorize_travel_or_raise_battle_tier() {
        let travel_state = AppState::new();
        for id in [10, 11] {
            register(
                &travel_state,
                id,
                participant_with_mask(id, RegionId::Hoenn, 0xFF00),
                zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
            );
        }
        let group = travel_state.create_group(10, 11).expect("group");
        let before = travel_state.group(group.group_id).expect("group");
        assert!(matches!(
            travel_state.travel(group.group_id, "HOENN_TO_KANTO"),
            Err(ServiceError::Forbidden {
                code: "protocol_boundary_denied",
                ..
            })
        ));
        assert_eq!(travel_state.group(group.group_id).expect("group"), before);

        let battle_state = AppState::new();
        for id in [20, 21] {
            register(
                &battle_state,
                id,
                participant_with_mask(id, RegionId::Kanto, 0xFF00),
                zone(RegionId::Kanto, "PALLET_TOWN"),
            );
        }
        let group = battle_state.create_group(20, 21).expect("group");
        let trainer = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        let reservation = battle_state
            .reserve_battle(group.group_id, 20, trainer)
            .expect("tier-zero battle remains available");
        assert_eq!(reservation.tier, 0);
        assert_eq!(reservation.level_cap, level_cap_for_tier(0).unwrap());
    }

    #[test]
    fn travel_requires_destination_progress_for_each_member() {
        let state = AppState::new();
        let only_hoenn =
            ParticipantProgress::new(20, vec![progress(RegionId::Hoenn, 8, 8, vec![])])
                .expect("progress");
        let only_hoenn_two =
            ParticipantProgress::new(21, vec![progress(RegionId::Hoenn, 8, 8, vec![])])
                .expect("progress");
        register(
            &state,
            20,
            only_hoenn,
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        register(
            &state,
            21,
            only_hoenn_two,
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        let group = state.create_group(20, 21).expect("group");
        let error = state.travel(group.group_id, "HOENN_TO_KANTO");
        assert!(matches!(
            error,
            Err(ServiceError::InvalidRequest {
                code: "missing_destination_regional_progress",
                ..
            })
        ));
        assert_eq!(
            state.participant_zone(20).expect("zone"),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN")
        );
        assert_eq!(
            state.participant_zone(21).expect("zone"),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN")
        );
    }

    #[tokio::test]
    async fn denial_keeps_zone_and_zero_threshold_input_is_rejected() {
        let state = AppState::new();
        register(
            &state,
            10,
            participant(10, 8, 3, 3, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        register(
            &state,
            11,
            participant(11, 5, 1, 1, vec![]),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
        );
        let group = state.create_group(10, 11).expect("group");
        let before = state.group(group.group_id).expect("group");
        let denied = state.travel(group.group_id, "HOENN_TO_KANTO");
        assert!(matches!(
            denied,
            Err(ServiceError::Forbidden {
                code: "protocol_boundary_denied",
                ..
            })
        ));
        assert_eq!(state.group(group.group_id).expect("group"), before);
        let router = app_with_state(state);
        let response = raw_request(
            router,
            &format!("/v1/groups/{}/travel", group.group_id),
            r#"{"destination":{"region":"KANTO","map":"PALLET_TOWN","channel":1},"route":{"from":"HOENN","to":"KANTO","minimum_badges":0,"minimum_story_checkpoint":0}}"#,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = response_json(response).await;
        assert_eq!(body["error"]["code"], "invalid_json");
    }

    #[test]
    fn fake_and_story_ineligible_trainers_are_denied() {
        let trainer = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        let route = RouteDefinition::new(
            "KANTO_TO_HOENN",
            zone(RegionId::Kanto, "PALLET_TOWN"),
            zone(RegionId::Hoenn, "LITTLEROOT_TOWN"),
            0,
            0,
        )
        .expect("route");
        let catalog =
            TrainerDefinition::new(trainer.clone(), zone(RegionId::Kanto, "PALLET_TOWN"), 10);
        let state = AppState::with_catalog(vec![route], vec![catalog]);
        register(
            &state,
            10,
            participant(10, 3, 3, 1, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        register(
            &state,
            11,
            participant(11, 3, 3, 1, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        let group = state.create_group(10, 11).expect("group");
        let story = state.reserve_battle(group.group_id, 10, trainer);
        assert!(matches!(
            story,
            Err(ServiceError::Forbidden {
                code: "story_prerequisite_not_met",
                ..
            })
        ));
        let fake = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_FALKNER").expect("trainer");
        assert!(matches!(
            state.reserve_battle(group.group_id, 10, fake),
            Err(ServiceError::NotFound {
                code: "trainer_not_found",
                ..
            })
        ));

        let wrong_zone_state = AppState::new();
        register(
            &wrong_zone_state,
            30,
            participant(30, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "VIRIDIAN_CITY"),
        );
        register(
            &wrong_zone_state,
            31,
            participant(31, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "VIRIDIAN_CITY"),
        );
        let wrong_zone_group = wrong_zone_state.create_group(30, 31).expect("group");
        let brock = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        assert!(matches!(
            wrong_zone_state.reserve_battle(wrong_zone_group.group_id, 30, brock),
            Err(ServiceError::Forbidden {
                code: "trainer_zone_mismatch",
                ..
            })
        ));
    }

    #[test]
    fn pending_expiry_and_companion_decline_release_both_member_locks() {
        let state = AppState::new();
        register(
            &state,
            40,
            participant(40, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        register(
            &state,
            41,
            participant(41, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        let group = state.create_group(40, 41).expect("group");
        let trainer = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        let pending = state
            .reserve_battle(group.group_id, 40, trainer.clone())
            .expect("reservation");
        assert_eq!(pending.state, ReservationState::Pending);
        assert_eq!(
            state
                .expire_pending_battles_at(
                    Instant::now() + Duration::from_secs(PENDING_RESERVATION_TTL_SECONDS + 1)
                )
                .expect("expiry"),
            1
        );
        assert!(matches!(
            state.accept_battle(pending.battle_id, 41),
            Err(ServiceError::NotFound {
                code: "battle_not_found",
                ..
            })
        ));
        let next = state
            .reserve_battle(group.group_id, 40, trainer.clone())
            .expect("reservation after expiry");
        assert!(state.decline_battle(next.battle_id, 41).is_ok());
        assert!(matches!(
            state.accept_battle(next.battle_id, 41),
            Err(ServiceError::NotFound {
                code: "battle_not_found",
                ..
            })
        ));
        assert!(state.reserve_battle(group.group_id, 40, trainer).is_ok());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn http_reserve_accept_commit_and_retry_are_one_flow() {
        let state = AppState::new();
        let trainer = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        register(
            &state,
            10,
            participant(10, 8, 3, 3, vec![]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        register(
            &state,
            11,
            participant(11, 5, 1, 1, vec![trainer.clone()]),
            zone(RegionId::Kanto, "PALLET_TOWN"),
        );
        let router = app_with_state(state.clone());
        let group_response = json_request(
            router.clone(),
            axum::http::Method::POST,
            "/v1/groups",
            &CreateGroupRequest {
                first_character_id: 10,
                second_character_id: 11,
            },
        )
        .await;
        assert_eq!(group_response.status(), StatusCode::CREATED);
        let group: GroupView = response_json(group_response).await;
        let reservation_response = json_request(
            router.clone(),
            axum::http::Method::POST,
            &format!("/v1/groups/{}/battles/reserve", group.group_id),
            &ReserveBattleRequest {
                character_id: 10,
                trainer_id: trainer,
            },
        )
        .await;
        assert_eq!(reservation_response.status(), StatusCode::CREATED);
        let reservation: serde_json::Value = response_json(reservation_response).await;
        assert_eq!(reservation["state"], "PENDING");
        assert_eq!(reservation["tier"], 1);
        assert_eq!(reservation["level_cap"], 19);
        assert_eq!(reservation["participants"][1]["role"], "REPEAT_HELPER");
        let battle_id = reservation["battle_id"].as_str().expect("battle id");
        let premature = json_request(router.clone(), axum::http::Method::POST, &format!("/v1/battles/{battle_id}/commit"), &serde_json::json!({"idempotency_key":"premature","outcome":"WON","final_state_hash":"hash"})).await;
        assert_eq!(premature.status(), StatusCode::CONFLICT);
        let accepted = json_request(
            router.clone(),
            axum::http::Method::POST,
            &format!("/v1/battles/{battle_id}/accept"),
            &AcceptBattleRequest { character_id: 11 },
        )
        .await;
        assert_eq!(accepted.status(), StatusCode::OK);
        let ready: serde_json::Value = response_json(accepted).await;
        assert_eq!(ready["state"], "READY");
        let body = serde_json::json!({"idempotency_key":"commit-brock-1","outcome":"WON","final_state_hash":"state-hash"});
        let first_response = json_request(
            router.clone(),
            axum::http::Method::POST,
            &format!("/v1/battles/{battle_id}/commit"),
            &body,
        )
        .await;
        assert_eq!(first_response.status(), StatusCode::OK);
        let first: serde_json::Value = response_json(first_response).await;
        assert_eq!(first["replayed"], false);
        assert_eq!(
            first["commit"]["participants"][0]["mark_trainer_defeated"],
            true
        );
        assert_eq!(
            first["commit"]["participants"][1]["mark_trainer_defeated"],
            false
        );
        let retry_response = json_request(
            router,
            axum::http::Method::POST,
            &format!("/v1/battles/{battle_id}/commit"),
            &body,
        )
        .await;
        assert_eq!(retry_response.status(), StatusCode::OK);
        let retry: serde_json::Value = response_json(retry_response).await;
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["commit"]["commit_id"], first["commit"]["commit_id"]);
        assert!(
            state
                .participant(10)
                .expect("participant")
                .progress_for(RegionId::Kanto)
                .expect("Kanto")
                .defeated_trainers
                .iter()
                .any(|id| id.as_str() == "KANTO:TRAINER_BROCK")
        );
        assert!(
            !state
                .participant(11)
                .expect("participant")
                .progress_for(RegionId::Kanto)
                .expect("Kanto")
                .defeated_trainers
                .is_empty()
        );
    }

    #[test]
    fn overlap_and_authorization_guards_are_enforced_without_global_trainer_lock() {
        let state = AppState::new();
        for id in 1..=4 {
            register(
                &state,
                id,
                participant(id, 8, 3, 3, vec![]),
                zone(RegionId::Kanto, "PALLET_TOWN"),
            );
        }
        let group_one = state.create_group(1, 2).expect("group");
        assert!(matches!(
            state.create_group(1, 3),
            Err(ServiceError::Conflict {
                code: "participant_already_grouped",
                ..
            })
        ));
        let trainer = TrainerInstanceId::new(RegionId::Kanto, "TRAINER_BROCK").expect("trainer");
        let reservation = state
            .reserve_battle(group_one.group_id, 1, trainer.clone())
            .expect("reservation");
        assert!(matches!(
            state.travel(group_one.group_id, "KANTO_TO_HOENN"),
            Err(ServiceError::Conflict {
                code: "travel_during_battle",
                ..
            })
        ));
        assert!(matches!(
            state.accept_battle(reservation.battle_id, 1),
            Err(ServiceError::Forbidden {
                code: "initiator_cannot_accept",
                ..
            })
        ));
        assert!(matches!(
            state.accept_battle(reservation.battle_id, 3),
            Err(ServiceError::Forbidden {
                code: "group_membership_mismatch",
                ..
            })
        ));
        assert!(matches!(
            state.reserve_battle(group_one.group_id, 2, trainer.clone()),
            Err(ServiceError::Conflict {
                code: "battle_already_active",
                ..
            })
        ));
        let group_two = state.create_group(3, 4).expect("unrelated group");
        assert!(state.reserve_battle(group_two.group_id, 3, trainer).is_ok());
    }

    #[tokio::test]
    async fn malformed_path_and_json_use_typed_error_envelopes() {
        let router = app();
        let invalid_json = raw_request(router.clone(), "/v1/groups", "not-json").await;
        assert_eq!(invalid_json.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response_json(invalid_json).await;
        assert_eq!(body["error"]["code"], "invalid_json");
        let invalid_path = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/v1/groups/not-a-uuid")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(invalid_path.status(), StatusCode::BAD_REQUEST);
        let path_body: serde_json::Value = response_json(invalid_path).await;
        assert_eq!(path_body["error"]["code"], "invalid_path");
        let invalid_query = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/v1/regions/KANTO/level-cap?tier=bad")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(invalid_query.status(), StatusCode::BAD_REQUEST);
        let query_body: serde_json::Value = response_json(invalid_query).await;
        assert_eq!(query_body["error"]["code"], "invalid_query");

        let missing_content_type = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/groups")
                    .body(Body::from("{}"))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(
            missing_content_type.status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        let content_type_body: serde_json::Value = response_json(missing_content_type).await;
        assert_eq!(content_type_body["error"]["code"], "invalid_json");

        let oversized_body = "x".repeat(2 * 1024 * 1024 + 1);
        let oversized = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/groups")
                    .header("content-type", "application/json")
                    .body(Body::from(oversized_body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let oversized_body: serde_json::Value = response_json(oversized).await;
        assert_eq!(oversized_body["error"]["code"], "invalid_json");

        let method_not_allowed = router
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::GET)
                    .uri("/v1/groups")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
        let method_body: serde_json::Value = response_json(method_not_allowed).await;
        assert_eq!(method_body["error"]["code"], "method_not_allowed");
    }
}

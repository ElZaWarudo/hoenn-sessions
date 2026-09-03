//! Ephemeral, lease-linearizable player presence for the Phase 3 vertical
//! slice.
//!
//! Presence deliberately lives beside (and not inside) the repository.  A
//! [`PresenceService`] clone shares only the process-local runtime state; a
//! newly-created [`Phase2App`](super::Phase2App) starts with an empty world.
//! All values crossing this module are the already validated V1 records from
//! `coop-protocol`.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    num::NonZeroU64,
    sync::{Arc, Mutex, MutexGuard},
};

use coop_cloud::{CharacterId, RuntimeBuildIdentity, RuntimeLeaseFence, StableRuntimeSession};
use coop_protocol::{
    CanonicalUsername, DespawnReason, Direction, LocalPresenceStateV1, PresenceHandle,
    PresenceInteractionV1, RegionId, RemotePlayerDespawnV1, RemotePlayerSpawnV1,
    RemotePlayerUpdateV1, WorldLocation,
};
use thiserror::Error;

use super::AuthenticatedActor;
use super::storage::{State, StorageError, Store};

/// The fixed runtime shard used by the first online presence slice.
pub const PRESENCE_SHARD_ID: u16 = 1;
/// Presence is published at ten updates per second.
pub const PRESENCE_TICK_MS: u64 = 100;
/// A client that has not supplied a fresh state for this long is stale.
pub const PRESENCE_STALE_MS: u64 = 1_500;
/// Maximum number of remote players visible to one client.
pub const PRESENCE_MAX_REMOTES: usize = 4;
/// Maximum connections in the supported map partition.
pub const PRESENCE_MAX_PARTITION_CONNECTIONS: usize = 5;
/// Process-wide connection bound.
pub const PRESENCE_MAX_GLOBAL_CONNECTIONS: usize = 1_024;
/// Maximum number of queued outbound lifecycle or update records.
pub const PRESENCE_OUTBOUND_QUEUE_CAPACITY: usize = 32;
/// Number of entropy candidates attempted for an opaque handle.
pub const PRESENCE_HANDLE_CANDIDATES: usize = 16;
/// The only map used by this initial presence implementation.
pub const PRESENCE_MAP_GROUP: u16 = 0;
/// The only map used by this initial presence implementation.
pub const PRESENCE_MAP_NUMBER: u16 = 9;
/// The only map key used by this initial presence implementation.
pub const PRESENCE_MAP: &str = "LITTLEROOT_TOWN";

/// Precise internal failure taxonomy.  A later network adapter should collapse
/// these into a deliberately smaller public error surface.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PresenceServiceError {
    #[error("authentication failed")]
    Authentication,
    #[error("lease is inactive")]
    LeaseInactive,
    #[error("lease fence does not match")]
    LeaseFenceMismatch,
    #[error("runtime build is incompatible")]
    IncompatibleBuild,
    #[error("world zone is not supported by this partition")]
    UnsupportedZone,
    #[error("global presence capacity is exhausted")]
    GlobalCapacity,
    #[error("presence partition capacity is exhausted")]
    PartitionCapacity,
    #[error("could not allocate an opaque presence handle")]
    HandleAllocation,
    #[error("presence connection is not active")]
    NotConnected,
    #[error("presence state is invalid")]
    InvalidState,
    #[error("interaction target is unavailable")]
    InteractionTargetUnavailable,
    #[error("interaction observation does not match server state")]
    InteractionObservationMismatch,
    #[error("interaction target is out of range")]
    InteractionOutOfRange,
    #[error("internal presence failure")]
    Internal,
}

/// Result of submitting a source state to a live connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceSubmitOutcome {
    /// The state was accepted and will be considered by the next tick.
    Accepted,
    /// The state was duplicate, old, zero, or RFC-1982 ambiguous.
    Ignored,
    /// The connection was removed because it attempted to change partition
    /// or warp.  The caller must establish a new regional session.
    DisconnectedUnsupportedTravel,
}

/// One server-to-client presence record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceOutboundV1 {
    Spawn(RemotePlayerSpawnV1),
    Update(RemotePlayerUpdateV1),
    Despawn(RemotePlayerDespawnV1),
}

impl PresenceOutboundV1 {
    #[must_use]
    pub fn handle(&self) -> PresenceHandle {
        match self {
            Self::Spawn(value) => value.handle(),
            Self::Update(value) => value.handle(),
            Self::Despawn(value) => value.handle(),
        }
    }

    #[must_use]
    pub fn server_sequence(&self) -> u32 {
        match self {
            Self::Spawn(value) => value.server_sequence(),
            Self::Update(value) => value.server_sequence(),
            Self::Despawn(value) => value.server_sequence(),
        }
    }
}

/// A bounded snapshot drained from one connection's outbound FIFO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceDrain {
    pub events: Vec<PresenceOutboundV1>,
    /// Saturating count of position updates discarded because this consumer's
    /// queue was full.  Lifecycle records are never silently discarded.
    pub dropped_updates: u32,
}

impl PresenceDrain {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// A deterministic summary of one globally gated tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresenceTickReport {
    pub observed_connections: usize,
    pub published_updates: usize,
    pub removed_connections: usize,
    pub next_tick_at_ms: Option<u64>,
}

/// An interaction after exact server-side observation validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedInteraction {
    pub initiator: StableRuntimeSession,
    pub target: StableRuntimeSession,
    pub target_handle: PresenceHandle,
}

/// The opaque capability returned by [`PresenceService::connect`].
///
/// Both the random handle and the private generation are checked on every
/// operation.  This prevents a stale capability from controlling a later
/// connection if an old numeric handle is ever deliberately reused.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct PresenceConnection {
    handle: PresenceHandle,
    generation: NonZeroU64,
}

impl fmt::Debug for PresenceConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PresenceConnection([REDACTED])")
    }
}

impl PresenceConnection {
    #[must_use]
    pub const fn handle(self) -> PresenceHandle {
        self.handle
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PartitionKey {
    build: RuntimeBuildIdentity,
    shard: u16,
    region: RegionId,
    map_group: u16,
    map_number: u16,
    channel: u16,
}

#[derive(Clone)]
struct PresenceEntry {
    connection: PresenceConnection,
    actor: AuthenticatedActor,
    stable_session: StableRuntimeSession,
    character_id: CharacterId,
    username: CanonicalUsername,
    partition: PartitionKey,
    state: LocalPresenceStateV1,
    published_state: LocalPresenceStateV1,
    pending_state: Option<LocalPresenceStateV1>,
    last_accepted_at_ms: u64,
    lease_expires_at_ms: u64,
    server_sequence: u32,
    advertised: bool,
    queue: VecDeque<PresenceOutboundV1>,
    dropped_updates: u32,
}

#[derive(Default)]
struct PresenceState {
    entries: BTreeMap<PresenceHandle, PresenceEntry>,
    by_character: BTreeMap<CharacterId, PresenceHandle>,
    partitions: std::collections::HashMap<PartitionKey, BTreeSet<PresenceHandle>>,
    next_generation: u64,
    next_tick_at_ms: Option<u64>,
}

impl PresenceState {
    fn new() -> Self {
        Self {
            next_generation: 1,
            ..Self::default()
        }
    }

    fn clear_fail_closed(&mut self) {
        self.entries.clear();
        self.by_character.clear();
        self.partitions.clear();
        self.next_tick_at_ms = None;
    }

    fn schedule(&mut self, now_ms: u64) {
        let deadline = now_ms.saturating_add(PRESENCE_TICK_MS);
        self.next_tick_at_ms = Some(
            self.next_tick_at_ms
                .map_or(deadline, |old| old.min(deadline)),
        );
    }

    fn allocate_generation(&mut self) -> Result<NonZeroU64, PresenceServiceError> {
        let current =
            NonZeroU64::new(self.next_generation).ok_or(PresenceServiceError::Internal)?;
        let next = current
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(PresenceServiceError::Internal)?;
        self.next_generation = next.get();
        Ok(current)
    }

    fn partition_count(&self, partition: &PartitionKey) -> usize {
        self.partitions.get(partition).map_or(0, BTreeSet::len)
    }

    fn add_entry(&mut self, entry: PresenceEntry) {
        let handle = entry.connection.handle;
        self.by_character.insert(entry.character_id, handle);
        self.partitions
            .entry(entry.partition.clone())
            .or_default()
            .insert(handle);
        self.entries.insert(handle, entry);
    }

    fn enqueue(&mut self, receiver: PresenceHandle, event: PresenceOutboundV1) -> bool {
        let Some(entry) = self.entries.get_mut(&receiver) else {
            return true;
        };
        let is_update = matches!(event, PresenceOutboundV1::Update(_));
        if let Some(last) = entry.queue.back_mut() {
            let same_subject = last.handle() == event.handle();
            if same_subject {
                match (&mut *last, event.clone()) {
                    (PresenceOutboundV1::Update(existing), PresenceOutboundV1::Update(new)) => {
                        *existing = new;
                        return true;
                    }
                    (PresenceOutboundV1::Spawn(existing), PresenceOutboundV1::Update(new)) => {
                        let spawn = RemotePlayerSpawnV1::new(
                            existing.handle(),
                            new.server_sequence(),
                            new.state().clone(),
                            existing.username().clone(),
                        );
                        if let Ok(spawn) = spawn {
                            *existing = spawn;
                            return true;
                        }
                    }
                    (PresenceOutboundV1::Spawn(existing), PresenceOutboundV1::Spawn(new)) => {
                        *existing = new;
                        return true;
                    }
                    _ => {}
                }
            }
        }
        if entry.queue.len() >= PRESENCE_OUTBOUND_QUEUE_CAPACITY {
            if is_update {
                entry.dropped_updates = entry.dropped_updates.saturating_add(1);
            }
            return false;
        }
        entry.queue.push_back(event);
        true
    }

    fn visible_recipients(
        &self,
        partition: &PartitionKey,
        excluded: PresenceHandle,
    ) -> Vec<PresenceHandle> {
        self.partitions
            .get(partition)
            .into_iter()
            .flat_map(|handles| handles.iter().copied())
            .filter(|handle| *handle != excluded)
            // Visibility belongs to the subject, not the observer. Hidden
            // clients continue observing visible peers so that their queues
            // do not retain ghosts while they are locally unadvertised.
            .filter(|handle| self.entries.contains_key(handle))
            .collect()
    }

    fn fanout_critical(
        &mut self,
        partition: &PartitionKey,
        source: PresenceHandle,
        event: &PresenceOutboundV1,
    ) {
        let recipients = self.visible_recipients(partition, source);
        let mut slow = Vec::new();
        for receiver in recipients {
            if !self.enqueue(receiver, event.clone()) {
                slow.push(receiver);
            }
        }
        for receiver in slow {
            self.remove_entry(receiver, DespawnReason::Disconnected);
        }
    }

    fn fanout_best_effort(
        &mut self,
        partition: &PartitionKey,
        source: PresenceHandle,
        event: &PresenceOutboundV1,
    ) {
        let recipients = self.visible_recipients(partition, source);
        for receiver in recipients {
            // A full queue may drop only this position update.  Lifecycle
            // records use `fanout_critical` and are never routed here.
            let _ = self.enqueue(receiver, event.clone());
        }
    }

    fn remove_entry(&mut self, handle: PresenceHandle, reason: DespawnReason) -> bool {
        let Some(mut removed) = self.entries.remove(&handle) else {
            return false;
        };
        self.by_character.remove(&removed.character_id);
        if let Some(handles) = self.partitions.get_mut(&removed.partition) {
            handles.remove(&handle);
            if handles.is_empty() {
                self.partitions.remove(&removed.partition);
            }
        }
        if removed.advertised {
            removed.server_sequence = coop_protocol::next_sequence(removed.server_sequence);
            let Ok(event) = RemotePlayerDespawnV1::new(handle, removed.server_sequence, reason)
            else {
                self.clear_fail_closed();
                return true;
            };
            let outbound = PresenceOutboundV1::Despawn(event);
            self.fanout_critical(&removed.partition, handle, &outbound);
        }
        if self.entries.is_empty() {
            self.next_tick_at_ms = None;
        }
        true
    }
}

struct PresenceInner {
    store: Store,
    build: RuntimeBuildIdentity,
    state: Mutex<PresenceState>,
}

/// Synchronous process-local online presence service.
#[derive(Clone)]
pub struct PresenceService {
    inner: Arc<PresenceInner>,
}

impl fmt::Debug for PresenceService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PresenceService([REDACTED])")
    }
}

impl PresenceService {
    pub(crate) fn new(store: Store) -> Result<Self, super::Phase2Error> {
        let build = super::saves::current_runtime_build_identity()?;
        Ok(Self {
            inner: Arc::new(PresenceInner {
                store,
                build,
                state: Mutex::new(PresenceState::new()),
            }),
        })
    }

    fn lock_state(&self) -> MutexGuard<'_, PresenceState> {
        match self.inner.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.clear_fail_closed();
                // The reset is the fail-closed recovery boundary. Clear the
                // poison bit so subsequent calls operate on the fresh empty
                // state instead of clearing a newly admitted connection.
                self.inner.state.clear_poison();
                guard
            }
        }
    }

    fn lock_gate(&self) -> Result<MutexGuard<'_, ()>, PresenceServiceError> {
        self.inner
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| PresenceServiceError::Internal)
    }

    fn map_location(location: WorldLocation) -> Result<(), PresenceServiceError> {
        if location.region != RegionId::Hoenn
            || location.map_group != PRESENCE_MAP_GROUP
            || location.map_number != PRESENCE_MAP_NUMBER
        {
            Err(PresenceServiceError::UnsupportedZone)
        } else {
            Ok(())
        }
    }

    fn partition(build: RuntimeBuildIdentity, channel: u16) -> PartitionKey {
        PartitionKey {
            build,
            shard: PRESENCE_SHARD_ID,
            region: RegionId::Hoenn,
            map_group: PRESENCE_MAP_GROUP,
            map_number: PRESENCE_MAP_NUMBER,
            channel,
        }
    }

    fn entropy_candidates(
        &self,
    ) -> Result<[u64; PRESENCE_HANDLE_CANDIDATES], PresenceServiceError> {
        let mut output = [0_u64; PRESENCE_HANDLE_CANDIDATES];
        for candidate in &mut output {
            let mut bytes = [0_u8; 8];
            self.inner
                .store
                .config
                .entropy
                .fill(&mut bytes)
                .map_err(|_| PresenceServiceError::Internal)?;
            *candidate = u64::from_le_bytes(bytes);
        }
        Ok(output)
    }

    fn validate_repository(
        &self,
        actor: AuthenticatedActor,
        fence: &RuntimeLeaseFence,
        initial: &LocalPresenceStateV1,
        now_ms: u64,
        build: &RuntimeBuildIdentity,
    ) -> Result<(CanonicalUsername, PartitionKey, u64), PresenceServiceError> {
        let location = initial.pose().location();
        Self::map_location(location)?;
        if initial.pose().player_state() == coop_protocol::PlayerState::Hidden {
            // Hidden initial presence remains in the same authoritative map;
            // no special admission exception is needed.
        }
        self.inner
            .store
            .read_transaction(|state| -> Result<_, AdmissionReadError> {
                let user = state
                    .users_by_id
                    .get(&actor.user_id)
                    .ok_or(PresenceServiceError::Authentication)?;
                if user.disabled || user.character_id != actor.character_id {
                    return Err(PresenceServiceError::Authentication.into());
                }
                if state
                    .users_by_name
                    .get(user.username.as_str())
                    .is_none_or(|record| record.user_id != user.user_id)
                {
                    return Err(PresenceServiceError::Authentication.into());
                }
                let username = CanonicalUsername::new(user.username.as_str())
                    .map_err(|_| PresenceServiceError::Authentication)?;
                let character = state
                    .characters
                    .get(&actor.character_id)
                    .ok_or(PresenceServiceError::Authentication)?;
                if character.owner != actor.user_id
                    || character.state.character_id != actor.character_id
                {
                    return Err(PresenceServiceError::Authentication.into());
                }
                character
                    .state
                    .validate()
                    .map_err(|_| PresenceServiceError::Internal)?;
                if character.state.world_zone.region != RegionId::Hoenn
                    || character.state.world_zone.map != PRESENCE_MAP
                    || character.state.world_zone.channel == 0
                {
                    return Err(PresenceServiceError::UnsupportedZone.into());
                }
                let map = character
                    .state
                    .world_zone
                    .map_entry()
                    .map_err(|_| PresenceServiceError::UnsupportedZone)?;
                if map.map_group != PRESENCE_MAP_GROUP || map.map_number != PRESENCE_MAP_NUMBER {
                    return Err(PresenceServiceError::UnsupportedZone.into());
                }
                if location.region != character.state.world_zone.region
                    || location.map_group != map.map_group
                    || location.map_number != map.map_number
                {
                    return Err(PresenceServiceError::UnsupportedZone.into());
                }
                let lease = state
                    .leases
                    .get(&actor.character_id)
                    .ok_or(PresenceServiceError::LeaseInactive)?;
                if lease.released || lease.contract.expires_at.value() <= now_ms {
                    return Err(PresenceServiceError::LeaseInactive.into());
                }
                if lease.contract.stable_runtime_session() != fence.session {
                    return Err(PresenceServiceError::LeaseFenceMismatch.into());
                }
                if &fence.build != build {
                    return Err(PresenceServiceError::IncompatibleBuild.into());
                }
                Ok((
                    username,
                    Self::partition(build.clone(), character.state.world_zone.channel),
                    lease.contract.expires_at.value(),
                ))
            })
            .map_err(|error| match error {
                AdmissionReadError::Presence(error) => error,
                AdmissionReadError::Storage(error) => {
                    let _ = error;
                    PresenceServiceError::Internal
                }
            })
    }

    fn choose_handle(
        state: &PresenceState,
        candidates: &[u64; PRESENCE_HANDLE_CANDIDATES],
    ) -> Result<PresenceHandle, PresenceServiceError> {
        candidates
            .iter()
            .copied()
            .filter_map(|value| PresenceHandle::new(value).ok())
            .map(|handle| (handle, !state.entries.contains_key(&handle)))
            .find_map(|(handle, free)| free.then_some(handle))
            .ok_or(PresenceServiceError::HandleAllocation)
    }

    /// Connects an authenticated active lease to the single supported map.
    ///
    /// Entropy is consumed before the runtime transition gate.  The gate is
    /// held from repository validation through presence insertion, so a
    /// concurrent lease transition has one unambiguous linearization order.
    ///
    /// # Errors
    ///
    /// Returns an admission, capacity, build, or state error when this
    /// connection cannot be established.
    pub fn connect(
        &self,
        actor: AuthenticatedActor,
        fence: RuntimeLeaseFence,
        initial: LocalPresenceStateV1,
    ) -> Result<PresenceConnection, PresenceServiceError> {
        initial
            .validate()
            .map_err(|_| PresenceServiceError::InvalidState)?;
        let candidates = self.entropy_candidates()?;
        let build = self.inner.build.clone();
        let _gate = self.lock_gate()?;
        let now_ms = self.inner.store.now();
        let (username, partition, lease_expires_at_ms) =
            self.validate_repository(actor, &fence, &initial, now_ms, &build)?;
        let mut state = self.lock_state();
        let existing = state.by_character.get(&actor.character_id).copied();
        if existing.is_none() && state.entries.len() >= PRESENCE_MAX_GLOBAL_CONNECTIONS {
            return Err(PresenceServiceError::GlobalCapacity);
        }
        let replaces_same_partition = existing.is_some_and(|handle| {
            state
                .entries
                .get(&handle)
                .is_some_and(|entry| entry.partition == partition)
        });
        if !replaces_same_partition
            && state.partition_count(&partition) >= PRESENCE_MAX_PARTITION_CONNECTIONS
        {
            return Err(PresenceServiceError::PartitionCapacity);
        }
        let handle = Self::choose_handle(&state, &candidates)?;
        let generation = state.allocate_generation()?;
        if let Some(old_handle) = existing {
            state.remove_entry(old_handle, DespawnReason::Replaced);
        }
        let stable_session = fence.session;
        let advertised = initial.pose().player_state() == coop_protocol::PlayerState::Overworld;
        let connection = PresenceConnection { handle, generation };
        let mut entry = PresenceEntry {
            connection,
            actor,
            stable_session,
            character_id: actor.character_id,
            username,
            partition: partition.clone(),
            state: initial.clone(),
            published_state: initial.clone(),
            pending_state: None,
            last_accepted_at_ms: now_ms,
            lease_expires_at_ms,
            server_sequence: 0,
            advertised,
            queue: VecDeque::new(),
            dropped_updates: 0,
        };
        let existing_visible = state
            .entries
            .values()
            .filter(|other| other.advertised && other.partition == partition)
            .map(|other| {
                (
                    other.connection.handle,
                    other.published_state.clone(),
                    other.username.clone(),
                    other.server_sequence,
                )
            })
            .collect::<Vec<_>>();
        if advertised {
            entry.server_sequence = 1;
        }
        state.add_entry(entry);
        // Existing spawns are delivered to the newcomer in ascending handle
        // order.  The newcomer never receives a self event.
        for (other_handle, other_state, other_username, other_sequence) in existing_visible {
            let Ok(spawn) = RemotePlayerSpawnV1::new(
                other_handle,
                other_sequence,
                LocalPresenceStateV1::new(
                    other_state.pose().clone(),
                    other_state.source_sequence(),
                )
                .map_err(|_| PresenceServiceError::Internal)?,
                other_username,
            ) else {
                state.clear_fail_closed();
                return Err(PresenceServiceError::Internal);
            };
            let _ = state.enqueue(handle, PresenceOutboundV1::Spawn(spawn));
        }
        if advertised {
            let Some(new_entry) = state.entries.get(&handle) else {
                state.clear_fail_closed();
                return Err(PresenceServiceError::Internal);
            };
            let spawn_state = new_entry.published_state.clone();
            let username = new_entry.username.clone();
            let Ok(spawn) = RemotePlayerSpawnV1::new(handle, 1, spawn_state, username) else {
                state.clear_fail_closed();
                return Err(PresenceServiceError::Internal);
            };
            let outbound = PresenceOutboundV1::Spawn(spawn);
            state.fanout_critical(&partition, handle, &outbound);
        }
        state.schedule(now_ms);
        drop((fence, initial));
        Ok(connection)
    }

    /// Convenience operation for transports that immediately need the
    /// initial FIFO snapshot.
    ///
    /// # Errors
    ///
    /// Propagates connection admission or FIFO errors.
    pub fn connect_and_drain(
        &self,
        actor: AuthenticatedActor,
        fence: RuntimeLeaseFence,
        initial: LocalPresenceStateV1,
    ) -> Result<(PresenceConnection, PresenceDrain), PresenceServiceError> {
        let connection = self.connect(actor, fence, initial)?;
        let drain = self.drain(connection)?;
        Ok((connection, drain))
    }

    fn validate_connection(
        state: &PresenceState,
        connection: PresenceConnection,
    ) -> Result<&PresenceEntry, PresenceServiceError> {
        let entry = state
            .entries
            .get(&connection.handle)
            .ok_or(PresenceServiceError::NotConnected)?;
        if entry.connection.generation != connection.generation {
            return Err(PresenceServiceError::NotConnected);
        }
        Ok(entry)
    }

    /// Drains at most the bounded contents currently queued for a connection.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceServiceError::NotConnected`] for an inactive or stale
    /// capability.
    pub fn drain(
        &self,
        connection: PresenceConnection,
    ) -> Result<PresenceDrain, PresenceServiceError> {
        let _gate = self.lock_gate()?;
        let mut state = self.lock_state();
        let entry = state
            .entries
            .get_mut(&connection.handle)
            .ok_or(PresenceServiceError::NotConnected)?;
        if entry.connection.generation != connection.generation {
            return Err(PresenceServiceError::NotConnected);
        }
        Ok(PresenceDrain {
            events: entry.queue.drain(..).collect(),
            dropped_updates: entry.dropped_updates,
        })
    }

    /// Submits a newer source state.  Publication remains globally gated at
    /// the next ten-Hz tick.
    ///
    /// # Errors
    ///
    /// Returns an error for an inactive capability or invalid state.
    pub fn submit_state(
        &self,
        connection: PresenceConnection,
        submitted: LocalPresenceStateV1,
    ) -> Result<PresenceSubmitOutcome, PresenceServiceError> {
        submitted
            .validate()
            .map_err(|_| PresenceServiceError::InvalidState)?;
        let _gate = self.lock_gate()?;
        let now_ms = self.inner.store.now();
        let mut state = self.lock_state();
        let Some(entry) = state.entries.get(&connection.handle) else {
            return Err(PresenceServiceError::NotConnected);
        };
        if entry.connection.generation != connection.generation {
            return Err(PresenceServiceError::NotConnected);
        }
        if !coop_protocol::sequence_is_newer(
            submitted.source_sequence(),
            entry.state.source_sequence(),
        ) {
            return Ok(PresenceSubmitOutcome::Ignored);
        }
        let old_location = entry.state.pose().location();
        let new_location = submitted.pose().location();
        if old_location.region != new_location.region
            || old_location.map_group != new_location.map_group
            || old_location.map_number != new_location.map_number
            || submitted.pose().warp_sequence() != entry.state.pose().warp_sequence()
        {
            state.remove_entry(connection.handle, DespawnReason::PartitionLeft);
            return Ok(PresenceSubmitOutcome::DisconnectedUnsupportedTravel);
        }
        let entry = state
            .entries
            .get_mut(&connection.handle)
            .ok_or(PresenceServiceError::NotConnected)?;
        entry.state = submitted.clone();
        entry.pending_state = Some(submitted);
        entry.last_accepted_at_ms = now_ms;
        state.schedule(now_ms);
        Ok(PresenceSubmitOutcome::Accepted)
    }

    /// Alias retained for callers that name the operation after the wire
    /// record rather than the local-state concept.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::submit_state`] errors.
    pub fn submit(
        &self,
        connection: PresenceConnection,
        submitted: LocalPresenceStateV1,
    ) -> Result<PresenceSubmitOutcome, PresenceServiceError> {
        self.submit_state(connection, submitted)
    }

    /// Idempotently removes one connection and informs its visible peers.
    ///
    /// # Errors
    ///
    /// Returns an internal error only if the runtime transition gate is
    /// unavailable.
    pub fn disconnect(&self, connection: PresenceConnection) -> Result<(), PresenceServiceError> {
        let _gate = self.lock_gate()?;
        let mut state = self.lock_state();
        let Some(entry) = state.entries.get(&connection.handle) else {
            return Ok(());
        };
        if entry.connection.generation != connection.generation {
            return Ok(());
        }
        state.remove_entry(connection.handle, DespawnReason::Disconnected);
        Ok(())
    }

    fn reconcile_publication(state: &mut PresenceState, handle: PresenceHandle) -> usize {
        let Some(entry) = state.entries.get(&handle) else {
            return 0;
        };
        let Some(next) = entry.pending_state.clone() else {
            return 0;
        };
        let was_advertised = entry.advertised;
        let is_advertised = next.pose().player_state() == coop_protocol::PlayerState::Overworld;
        let partition = entry.partition.clone();
        let current_server_sequence = entry.server_sequence;
        let username = entry.username.clone();
        if let Some(entry) = state.entries.get_mut(&handle) {
            entry.pending_state = None;
            entry.published_state = next.clone();
            entry.advertised = is_advertised;
            if !was_advertised && !is_advertised {
                return 1;
            }
            entry.server_sequence = coop_protocol::next_sequence(current_server_sequence);
        }
        let sequence = state
            .entries
            .get(&handle)
            .map_or(1, |entry| entry.server_sequence);
        if was_advertised && !is_advertised {
            let Ok(despawn) = RemotePlayerDespawnV1::new(handle, sequence, DespawnReason::Hidden)
            else {
                state.clear_fail_closed();
                return 0;
            };
            let outbound = PresenceOutboundV1::Despawn(despawn);
            state.fanout_critical(&partition, handle, &outbound);
        } else if !was_advertised && is_advertised {
            let Ok(spawn) = RemotePlayerSpawnV1::new(handle, sequence, next, username) else {
                state.clear_fail_closed();
                return 0;
            };
            let outbound = PresenceOutboundV1::Spawn(spawn);
            state.fanout_critical(&partition, handle, &outbound);
        } else if was_advertised {
            let Ok(update) = RemotePlayerUpdateV1::new(handle, sequence, next) else {
                state.clear_fail_closed();
                return 0;
            };
            let outbound = PresenceOutboundV1::Update(update);
            state.fanout_best_effort(&partition, handle, &outbound);
        }
        1
    }

    /// Runs one globally gated tick.  Delayed ticks publish at most one
    /// coalesced value and schedule the following window from `now_ms`.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the runtime gate or repository fails.
    pub fn tick(&self) -> Result<PresenceTickReport, PresenceServiceError> {
        let now_ms = self.inner.store.now();
        self.tick_at(now_ms)
    }

    /// Deterministic clock-injected form used by tests and local adapters.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the runtime gate or repository fails.
    pub fn tick_at(&self, now_ms: u64) -> Result<PresenceTickReport, PresenceServiceError> {
        let _gate = self.lock_gate()?;
        let handles = {
            let state = self.lock_state();
            if state
                .next_tick_at_ms
                .is_none_or(|deadline| now_ms < deadline)
            {
                return Ok(PresenceTickReport {
                    next_tick_at_ms: state.next_tick_at_ms,
                    ..PresenceTickReport::default()
                });
            }
            state
                .entries
                .values()
                .map(|entry| PresenceSnapshot {
                    handle: entry.connection.handle,
                    generation: entry.connection.generation,
                    actor: entry.actor,
                    stable_session: entry.stable_session,
                    character_id: entry.character_id,
                    channel: entry.partition.channel,
                })
                .collect::<Vec<_>>()
        };
        let validation = self
            .inner
            .store
            .read_transaction(|repository| {
                Ok::<Vec<RepositoryDisposition>, StorageError>(
                    handles
                        .iter()
                        .map(|snapshot| repository_disposition(repository, snapshot, now_ms))
                        .collect(),
                )
            })
            .map_err(|_| PresenceServiceError::Internal)?;
        let mut state = self.lock_state();
        let mut report = PresenceTickReport {
            observed_connections: handles.len(),
            ..PresenceTickReport::default()
        };
        for (snapshot, disposition) in handles.into_iter().zip(validation) {
            let Some(entry) = state.entries.get(&snapshot.handle) else {
                continue;
            };
            if entry.connection.generation != snapshot.generation
                || entry.stable_session != snapshot.stable_session
            {
                continue;
            }
            match disposition {
                RepositoryDisposition::LeaseInvalid => {
                    if state.remove_entry(snapshot.handle, DespawnReason::LeaseInvalid) {
                        report.removed_connections += 1;
                    }
                }
                RepositoryDisposition::ZoneChanged => {
                    if state.remove_entry(snapshot.handle, DespawnReason::PartitionLeft) {
                        report.removed_connections += 1;
                    }
                }
                RepositoryDisposition::Valid { expires_at_ms } => {
                    if let Some(entry) = state.entries.get_mut(&snapshot.handle) {
                        entry.lease_expires_at_ms = expires_at_ms;
                        if now_ms.saturating_sub(entry.last_accepted_at_ms) >= PRESENCE_STALE_MS {
                            if state.remove_entry(snapshot.handle, DespawnReason::Stale) {
                                report.removed_connections += 1;
                            }
                            continue;
                        }
                    }
                    report.published_updates +=
                        Self::reconcile_publication(&mut state, snapshot.handle);
                }
            }
        }
        report.removed_connections = report
            .observed_connections
            .saturating_sub(state.entries.len());
        if state.entries.is_empty() {
            state.next_tick_at_ms = None;
        } else {
            state.next_tick_at_ms = Some(now_ms.saturating_add(PRESENCE_TICK_MS));
        }
        report.next_tick_at_ms = state.next_tick_at_ms;
        Ok(report)
    }

    /// Validates a client-observed target without refreshing liveness or
    /// mutating any presence state.
    ///
    /// # Errors
    ///
    /// Returns a precise observation, range, or capability error when the
    /// target is not currently interactable.
    pub fn validate_interaction(
        &self,
        initiator: PresenceConnection,
        interaction: PresenceInteractionV1,
    ) -> Result<ValidatedInteraction, PresenceServiceError> {
        interaction
            .validate()
            .map_err(|_| PresenceServiceError::InvalidState)?;
        let target_handle = interaction.handle();
        let observed_server_sequence = interaction.observed_server_sequence();
        let observed_warp_sequence = interaction.observed_warp_sequence();
        let observed_x = interaction.x();
        let observed_y = interaction.y();
        std::hint::black_box(interaction);
        let _gate = self.lock_gate()?;
        let now_ms = self.inner.store.now();
        let state = self.lock_state();
        let source = Self::validate_connection(&state, initiator)?;
        if !source.advertised
            || source.lease_expires_at_ms <= now_ms
            || now_ms.saturating_sub(source.last_accepted_at_ms) >= PRESENCE_STALE_MS
        {
            return Err(PresenceServiceError::InteractionTargetUnavailable);
        }
        let target = state
            .entries
            .get(&target_handle)
            .ok_or(PresenceServiceError::InteractionTargetUnavailable)?;
        if target.connection.handle == source.connection.handle
            || !target.advertised
            || target.partition != source.partition
            || target.lease_expires_at_ms <= now_ms
            || now_ms.saturating_sub(target.last_accepted_at_ms) >= PRESENCE_STALE_MS
        {
            return Err(PresenceServiceError::InteractionTargetUnavailable);
        }
        let observed = target.published_state.pose();
        let observed_location = observed.location();
        if observed_server_sequence != target.server_sequence
            || observed_warp_sequence != observed.warp_sequence()
            || observed_x != observed_location.x
            || observed_y != observed_location.y
        {
            return Err(PresenceServiceError::InteractionObservationMismatch);
        }
        if source.state.pose().elevation() != observed.elevation() {
            return Err(PresenceServiceError::InteractionOutOfRange);
        }
        let source_location = source.state.pose().location();
        let expected = match source.state.pose().direction() {
            Direction::North => source_location
                .y
                .checked_sub(1)
                .map(|y| (source_location.x, y)),
            Direction::South => source_location
                .y
                .checked_add(1)
                .map(|y| (source_location.x, y)),
            Direction::West => source_location
                .x
                .checked_sub(1)
                .map(|x| (x, source_location.y)),
            Direction::East => source_location
                .x
                .checked_add(1)
                .map(|x| (x, source_location.y)),
        };
        if expected != Some((observed_x, observed_y)) {
            return Err(PresenceServiceError::InteractionOutOfRange);
        }
        Ok(ValidatedInteraction {
            initiator: source.stable_session,
            target: target.stable_session,
            target_handle: target.connection.handle,
        })
    }

    /// Returns the cumulative update-drop counter without draining events.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceServiceError::NotConnected`] for a stale capability.
    pub fn dropped_updates(
        &self,
        connection: PresenceConnection,
    ) -> Result<u32, PresenceServiceError> {
        let _gate = self.lock_gate()?;
        let state = self.lock_state();
        Ok(Self::validate_connection(&state, connection)?.dropped_updates)
    }

    /// Returns the number of currently live capabilities in this process.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the runtime transition gate is poisoned.
    pub fn connection_count(&self) -> Result<usize, PresenceServiceError> {
        let _gate = self.lock_gate()?;
        Ok(self.lock_state().entries.len())
    }

    /// Returns the current globally coalesced tick deadline.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the runtime transition gate is poisoned.
    pub fn next_tick_at_ms(&self) -> Result<Option<u64>, PresenceServiceError> {
        let _gate = self.lock_gate()?;
        Ok(self.lock_state().next_tick_at_ms)
    }

    /// Reconciles an already committed lease transition.  This function is
    /// intentionally non-fallible: a poisoned presence mutex is recovered and
    /// cleared fail-closed so Phase 2 never reports a committed lease as a
    /// failed operation.
    pub(crate) fn reconcile_lease_success(
        &self,
        character_id: CharacterId,
        contract: &coop_cloud::LeaseContract,
    ) {
        let mut state = self.lock_state();
        let stable = contract.stable_runtime_session();
        let handles = state
            .by_character
            .get(&character_id)
            .copied()
            .into_iter()
            .collect::<Vec<_>>();
        for handle in handles {
            let matches = state
                .entries
                .get(&handle)
                .is_some_and(|entry| entry.stable_session == stable);
            if matches {
                if let Some(entry) = state.entries.get_mut(&handle) {
                    entry.lease_expires_at_ms = contract.expires_at.value();
                }
            } else {
                state.remove_entry(handle, DespawnReason::LeaseInvalid);
            }
        }
    }

    pub(crate) fn reconcile_lease_release(&self, character_id: CharacterId) {
        let mut state = self.lock_state();
        if let Some(handle) = state.by_character.get(&character_id).copied() {
            state.remove_entry(handle, DespawnReason::LeaseInvalid);
        }
    }
}

#[derive(Clone, Copy)]
struct PresenceSnapshot {
    handle: PresenceHandle,
    generation: NonZeroU64,
    actor: AuthenticatedActor,
    stable_session: StableRuntimeSession,
    character_id: CharacterId,
    channel: u16,
}

enum RepositoryDisposition {
    LeaseInvalid,
    ZoneChanged,
    Valid { expires_at_ms: u64 },
}

fn repository_disposition(
    state: &State,
    snapshot: &PresenceSnapshot,
    now_ms: u64,
) -> RepositoryDisposition {
    let Some(user) = state.users_by_id.get(&snapshot.actor.user_id) else {
        return RepositoryDisposition::LeaseInvalid;
    };
    let Some(character) = state.characters.get(&snapshot.character_id) else {
        return RepositoryDisposition::LeaseInvalid;
    };
    let Some(lease) = state.leases.get(&snapshot.character_id) else {
        return RepositoryDisposition::LeaseInvalid;
    };
    if user.disabled
        || user.character_id != snapshot.character_id
        || character.owner != snapshot.actor.user_id
        || lease.released
        || lease.contract.expires_at.value() <= now_ms
        || lease.contract.stable_runtime_session() != snapshot.stable_session
    {
        return RepositoryDisposition::LeaseInvalid;
    }
    let zone = &character.state.world_zone;
    if zone.region != RegionId::Hoenn
        || zone.map != PRESENCE_MAP
        || zone.channel == 0
        || zone.channel != snapshot.channel
        || zone.map_entry().is_err()
    {
        return RepositoryDisposition::ZoneChanged;
    }
    RepositoryDisposition::Valid {
        expires_at_ms: lease.contract.expires_at.value(),
    }
}

enum AdmissionReadError {
    Presence(PresenceServiceError),
    Storage(StorageError),
}

impl From<PresenceServiceError> for AdmissionReadError {
    fn from(error: PresenceServiceError) -> Self {
        Self::Presence(error)
    }
}

impl From<StorageError> for AdmissionReadError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase2::storage::{
        InMemoryObjectStore, InMemoryRepository, Repository, StorageError,
    };
    use crate::phase2::{ArgonPasswordEngine, FixedClock, FixedEntropy, Phase2Config};
    use crate::{Phase2App, Phase2Error};
    use coop_cloud::{
        AcquireLeaseRequest, ClientInstanceId, HeartbeatLeaseRequest, IdempotencyKey, Password,
        ReconnectLeaseRequest, RegisterRequest, ReleaseLeaseRequest, RuntimeLeaseFence,
    };
    use coop_protocol::{
        AnimationId, AvatarId, DespawnReason, Direction, MovementMode, PlayerState,
        PresenceInteractionV1, PresencePoseV1, RegionId, WorldLocation,
    };
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::thread;
    use uuid::Uuid;

    #[derive(Default)]
    struct HandleReuseEntropy(AtomicU64);

    impl super::super::storage::Entropy for HandleReuseEntropy {
        fn fill(&self, output: &mut [u8]) -> Result<(), StorageError> {
            let cursor = self.0.fetch_add(1, Ordering::SeqCst);
            if output.len() == 8 {
                output.fill(0x42);
            } else {
                for (index, byte) in output.iter_mut().enumerate() {
                    *byte = cursor.to_le_bytes()[0].wrapping_add(index.to_le_bytes()[0]);
                }
            }
            Ok(())
        }
    }

    struct BlockingRepository {
        inner: InMemoryRepository,
        block_next_read: AtomicBool,
        fail_next_read: AtomicBool,
        entered: Arc<Barrier>,
        resume: Arc<Barrier>,
    }

    impl BlockingRepository {
        fn new(entered: Arc<Barrier>, resume: Arc<Barrier>) -> Self {
            Self {
                inner: InMemoryRepository::new(),
                block_next_read: AtomicBool::new(false),
                fail_next_read: AtomicBool::new(false),
                entered,
                resume,
            }
        }
    }

    impl Repository for BlockingRepository {
        fn read_transaction(
            &self,
            operation: &mut dyn FnMut(&super::super::storage::State) -> Result<(), StorageError>,
        ) -> Result<(), StorageError> {
            if self.block_next_read.swap(false, Ordering::SeqCst) {
                self.entered.wait();
                self.resume.wait();
            }
            if self.fail_next_read.swap(false, Ordering::SeqCst) {
                return Err(StorageError::Transaction);
            }
            self.inner.read_transaction(operation)
        }

        fn write_transaction(
            &self,
            operation: &mut dyn FnMut(
                &mut super::super::storage::State,
            ) -> Result<(), StorageError>,
        ) -> Result<(), StorageError> {
            self.inner.write_transaction(operation)
        }
    }

    fn id<T>(make: fn(Uuid) -> Result<T, coop_cloud::IdError>, value: u128) -> T {
        make(Uuid::from_u128(value)).expect("non-nil test id")
    }

    fn pose_at(
        location: WorldLocation,
        source: u32,
        player_state: PlayerState,
    ) -> LocalPresenceStateV1 {
        pose_with(location, 0, Direction::East, source, 1, player_state)
    }

    fn pose_with(
        location: WorldLocation,
        elevation: u8,
        direction: Direction,
        source: u32,
        warp_sequence: u32,
        player_state: PlayerState,
    ) -> LocalPresenceStateV1 {
        LocalPresenceStateV1::new(
            PresencePoseV1::new(
                location,
                elevation,
                direction,
                source,
                warp_sequence,
                MovementMode::Walk,
                AnimationId::Locomotion,
                AvatarId::Brendan,
                player_state,
            )
            .unwrap(),
            source,
        )
        .unwrap()
    }

    fn pose(x: i16, y: i16, source: u32, player_state: PlayerState) -> LocalPresenceStateV1 {
        pose_at(
            WorldLocation::new(RegionId::Hoenn, 0, 9, x, y).unwrap(),
            source,
            player_state,
        )
    }

    fn runtime_fence(lease: &coop_cloud::LeaseContract) -> RuntimeLeaseFence {
        RuntimeLeaseFence::new(
            lease.stable_runtime_session(),
            super::super::saves::current_runtime_build_identity().unwrap(),
        )
    }

    fn observed_interaction(
        handle: PresenceHandle,
        server_sequence: u32,
        warp_sequence: u32,
        x: i16,
        y: i16,
    ) -> PresenceInteractionV1 {
        PresenceInteractionV1::new(handle, server_sequence, warp_sequence, x, y).unwrap()
    }

    fn liveness(service: &PresenceService, handle: PresenceHandle) -> (u64, u32) {
        let state = service.inner.state.lock().unwrap();
        let entry = state.entries.get(&handle).unwrap();
        (entry.last_accepted_at_ms, entry.server_sequence)
    }

    fn set_liveness(service: &PresenceService, handle: PresenceHandle, timestamp: u64) {
        service
            .inner
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut(&handle)
            .unwrap()
            .last_accepted_at_ms = timestamp;
    }

    struct InteractionCase {
        source_sequence: u32,
        target_sequence: u32,
        source_state: LocalPresenceStateV1,
        target_state: LocalPresenceStateV1,
        observed_x: i16,
        observed_y: i16,
        expected: PresenceServiceError,
    }

    fn assert_interaction_rejected(
        app: &Phase2App,
        service: &PresenceService,
        case: InteractionCase,
    ) {
        let source_actor = account(
            app,
            &format!("src{}", case.source_sequence),
            &format!("invite-geometry-src-{}", case.source_sequence),
        );
        let target_actor = account(
            app,
            &format!("tgt{}", case.target_sequence),
            &format!("invite-geometry-tgt-{}", case.target_sequence),
        );
        let source_lease = acquire(app, source_actor, 54 + u128::from(case.source_sequence));
        let target_lease = acquire(app, target_actor, 154 + u128::from(case.target_sequence));
        let source_connection = service
            .connect(
                source_actor,
                runtime_fence(&source_lease),
                case.source_state,
            )
            .unwrap();
        let target_connection = service
            .connect(
                target_actor,
                runtime_fence(&target_lease),
                case.target_state,
            )
            .unwrap();
        let _ = service.drain(target_connection);
        let _ = service.drain(source_connection);
        assert_eq!(
            service.validate_interaction(
                source_connection,
                observed_interaction(
                    target_connection.handle(),
                    1,
                    1,
                    case.observed_x,
                    case.observed_y,
                ),
            ),
            Err(case.expected)
        );
        service.disconnect(source_connection).unwrap();
        service.disconnect(target_connection).unwrap();
    }

    fn account(app: &Phase2App, username: &str, invitation: &str) -> AuthenticatedActor {
        app.add_invitation(invitation).unwrap();
        let request = RegisterRequest::new(
            username,
            Password::new("test-password-123").unwrap(),
            coop_cloud::InvitationCode::new(invitation).unwrap(),
        )
        .unwrap();
        let created = app.register(request).unwrap();
        let login = app
            .login(
                coop_cloud::LoginRequest::new(
                    username,
                    Password::new("test-password-123").unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let headers = axum::http::HeaderMap::from_iter([(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", login.access_token.expose_secret())
                .parse()
                .unwrap(),
        )]);
        let actor = super::super::auth::actor_from_headers(&app.store, &headers).unwrap();
        assert_eq!(actor.character_id, created.character_id);
        actor
    }

    fn acquire(
        app: &Phase2App,
        actor: AuthenticatedActor,
        seed: u128,
    ) -> coop_cloud::LeaseContract {
        app.acquire(
            actor,
            AcquireLeaseRequest::new(
                actor.character_id,
                id(ClientInstanceId::new, seed),
                id(IdempotencyKey::new, seed + 10),
            ),
        )
        .unwrap()
    }

    fn app_with_unique_test_entropy() -> Phase2App {
        let mut entropy = Vec::with_capacity(8_192);
        let mut seed = 0x9e37_79b9_u32;
        for _ in 0..8_192 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            entropy.push((seed >> 24) as u8);
        }
        let config = Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .unwrap()
        .with_test_adapters(
            std::sync::Arc::new(FixedClock::new(1_700_000_000_000)),
            std::sync::Arc::new(FixedEntropy::new(entropy)),
        )
        .with_password_engine(std::sync::Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).unwrap(),
        ));
        Phase2App::new(config).unwrap()
    }

    fn app_with_handle_reuse_entropy() -> Phase2App {
        let config = Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .unwrap()
        .with_test_adapters(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(HandleReuseEntropy::default()),
        )
        .with_password_engine(Arc::new(ArgonPasswordEngine::new(8_192, 1, 1).unwrap()));
        Phase2App::new(config).unwrap()
    }

    fn app_with_blocking_repository(
        entered: Arc<Barrier>,
        resume: Arc<Barrier>,
    ) -> (Phase2App, Arc<BlockingRepository>, Arc<FixedClock>) {
        let repository = Arc::new(BlockingRepository::new(entered, resume));
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let config = Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .unwrap()
        .with_test_adapters(
            clock.clone(),
            Arc::new(FixedEntropy::new((0..=255).collect())),
        )
        .with_password_engine(Arc::new(ArgonPasswordEngine::new(8_192, 1, 1).unwrap()))
        .with_adapters(repository.clone(), Arc::new(InMemoryObjectStore::new()));
        (Phase2App::new(config).unwrap(), repository, clock)
    }

    #[test]
    fn real_auth_acquire_and_symmetric_spawns_are_region_safe() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-a");
        let second = account(&app, "bob", "invite-b");
        let first_lease = acquire(&app, first, 1);
        let second_lease = acquire(&app, second, 2);
        let service = app.presence();
        let first_state = pose(1, 1, 1, PlayerState::Overworld);
        let second_state = pose(2, 1, 1, PlayerState::Overworld);
        let one = service
            .connect(first, runtime_fence(&first_lease), first_state.clone())
            .unwrap();
        let two = service
            .connect(second, runtime_fence(&second_lease), second_state.clone())
            .unwrap();
        assert_ne!(one.handle(), two.handle());
        let first_events = service.drain(one).unwrap().events;
        let second_events = service.drain(two).unwrap().events;
        let [PresenceOutboundV1::Spawn(first_spawn)] = first_events.as_slice() else {
            panic!("first observer must receive exactly one spawn");
        };
        let [PresenceOutboundV1::Spawn(second_spawn)] = second_events.as_slice() else {
            panic!("second observer must receive exactly one spawn");
        };
        assert_eq!(first_spawn.handle(), two.handle());
        assert_eq!(second_spawn.handle(), one.handle());
        assert_ne!(first_spawn.handle(), one.handle());
        assert_ne!(second_spawn.handle(), two.handle());
        assert_eq!(first_spawn.username().as_str(), "bob");
        assert_eq!(second_spawn.username().as_str(), "alice");
        assert_eq!(first_spawn.state(), &second_state);
        assert_eq!(second_spawn.state(), &first_state);
        assert_eq!(first_spawn.server_sequence(), 1);
        assert_eq!(second_spawn.server_sequence(), 1);
    }

    #[test]
    fn sequence_coalescing_hidden_lifecycle_and_stale_are_deterministic() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-c");
        let second = account(&app, "bob", "invite-d");
        let first_lease = acquire(&app, first, 3);
        let second_lease = acquire(&app, second, 4);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        assert_eq!(
            service
                .submit_state(one, pose(3, 1, 2, PlayerState::Overworld))
                .unwrap(),
            PresenceSubmitOutcome::Accepted
        );
        assert_eq!(
            service
                .submit_state(one, pose(4, 1, 3, PlayerState::Overworld))
                .unwrap(),
            PresenceSubmitOutcome::Accepted
        );
        assert_eq!(
            service
                .tick_at(app.store.now() + 100)
                .unwrap()
                .published_updates,
            1
        );
        let update = service.drain(two).unwrap().events;
        assert!(matches!(update.as_slice(), [PresenceOutboundV1::Update(_)]));
        assert_eq!(
            service
                .submit_state(one, pose(4, 1, 4, PlayerState::Hidden))
                .unwrap(),
            PresenceSubmitOutcome::Accepted
        );
        service.tick_at(app.store.now() + 100).unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Despawn(_)]
        ));
    }

    #[test]
    fn duplicate_older_zero_and_delayed_source_states_are_fail_closed() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-sequences");
        let second = account(&app, "bob", "invite-sequence-peer");
        let first_lease = acquire(&app, first, 41);
        let second_lease = acquire(&app, second, 42);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);

        assert_eq!(
            service.submit_state(one, pose(3, 1, 2, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Accepted)
        );
        let accepted_at = service
            .inner
            .state
            .lock()
            .unwrap()
            .entries
            .get(&one.handle())
            .unwrap()
            .last_accepted_at_ms;
        assert_eq!(
            service.submit_state(one, pose(4, 1, 2, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Ignored)
        );
        assert_eq!(
            service.submit_state(one, pose(5, 1, 1, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Ignored)
        );
        assert_eq!(
            service.submit_state(one, pose(6, 1, u32::MAX, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Ignored)
        );
        assert_eq!(
            service
                .inner
                .state
                .lock()
                .unwrap()
                .entries
                .get(&one.handle())
                .unwrap()
                .last_accepted_at_ms,
            accepted_at
        );

        let mut zero_state = pose(7, 1, 2, PlayerState::Overworld).to_bytes();
        let zero_sequence_offset = zero_state.len().saturating_sub(std::mem::size_of::<u32>());
        zero_state[zero_sequence_offset..].fill(0);
        assert!(LocalPresenceStateV1::decode(&zero_state).is_err());

        let base = app.store.now();
        let first_tick = service.tick_at(base + PRESENCE_TICK_MS * 10).unwrap();
        assert_eq!(first_tick.published_updates, 1);
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(event)] if event.state().pose().location().x == 3
        ));

        service
            .submit_state(one, pose(8, 1, 3, PlayerState::Overworld))
            .unwrap();
        let delayed_now = base + PRESENCE_TICK_MS * 12;
        let delayed = service.tick_at(delayed_now).unwrap();
        assert_eq!(delayed.published_updates, 1);
        assert_eq!(
            delayed.next_tick_at_ms,
            Some(delayed_now + PRESENCE_TICK_MS)
        );
        let no_catch_up = service.tick_at(delayed_now + 1).unwrap();
        assert_eq!(no_catch_up.published_updates, 0);
        assert_eq!(no_catch_up.next_tick_at_ms, delayed.next_tick_at_ms);
    }

    #[test]
    fn server_sequence_wrap_skips_zero_and_preserves_subject_sequence() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-server-wrap");
        let second = account(&app, "bob", "invite-server-peer");
        let first_lease = acquire(&app, first, 43);
        let second_lease = acquire(&app, second, 44);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        service
            .inner
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut(&one.handle())
            .unwrap()
            .server_sequence = u32::MAX - 1;

        let base = app.store.now();
        service
            .submit_state(one, pose(3, 1, 2, PlayerState::Overworld))
            .unwrap();
        service.tick_at(base + PRESENCE_TICK_MS).unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(event)]
                if event.server_sequence() == u32::MAX
        ));

        service
            .submit_state(one, pose(4, 1, 3, PlayerState::Overworld))
            .unwrap();
        service.tick_at(base + PRESENCE_TICK_MS * 2).unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(event)] if event.server_sequence() == 1
        ));
    }

    #[test]
    fn hidden_observers_continue_receiving_visible_peer_lifecycle_events() {
        let app = Phase2App::test();
        let visible_actor = account(&app, "alice", "invite-j");
        let hidden_actor = account(&app, "bob", "invite-k");
        let newcomer_actor = account(&app, "carol", "invite-l");
        let visible_lease = acquire(&app, visible_actor, 10);
        let hidden_lease = acquire(&app, hidden_actor, 11);
        let newcomer_lease = acquire(&app, newcomer_actor, 12);
        let service = app.presence();
        let visible = service
            .connect(
                visible_actor,
                runtime_fence(&visible_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let hidden = service
            .connect(
                hidden_actor,
                runtime_fence(&hidden_lease),
                pose(2, 1, 1, PlayerState::Hidden),
            )
            .unwrap();
        assert!(matches!(
            service.drain(hidden).unwrap().events.as_slice(),
            [PresenceOutboundV1::Spawn(event)] if event.handle() == visible.handle()
        ));
        assert!(service.drain(visible).unwrap().is_empty());

        let newcomer = service
            .connect(
                newcomer_actor,
                runtime_fence(&newcomer_lease),
                pose(3, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert!(matches!(
            service.drain(hidden).unwrap().events.as_slice(),
            [PresenceOutboundV1::Spawn(event)] if event.handle() == newcomer.handle()
        ));
        service
            .submit_state(newcomer, pose(4, 1, 2, PlayerState::Overworld))
            .unwrap();
        service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert!(matches!(
            service.drain(hidden).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(event)]
                if event.handle() == newcomer.handle()
                    && event.server_sequence() == 2
        ));
        service.disconnect(newcomer).unwrap();
        assert!(matches!(
            service.drain(hidden).unwrap().events.as_slice(),
            [PresenceOutboundV1::Despawn(event)]
                if event.handle() == newcomer.handle()
                    && event.reason() == DespawnReason::Disconnected
        ));
    }

    #[test]
    fn queued_spawn_folds_to_latest_state_and_server_sequence() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-m");
        let second = account(&app, "bob", "invite-n");
        let first_lease = acquire(&app, first, 13);
        let second_lease = acquire(&app, second, 14);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        // Keep the initial spawn queued for the observer so the next update
        // exercises Spawn + Update folding rather than a plain Update.
        let _ = service.drain(two).unwrap();
        service
            .submit_state(two, pose(3, 1, 2, PlayerState::Overworld))
            .unwrap();
        service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        let drained = service.drain(one).unwrap();
        let [PresenceOutboundV1::Spawn(spawn)] = drained.events.as_slice() else {
            panic!("expected folded spawn");
        };
        assert_eq!(spawn.handle(), two.handle());
        assert_eq!(spawn.server_sequence(), 2);
        assert_eq!(spawn.state().pose().location().x, 3);
    }

    #[test]
    fn unsupported_travel_and_stale_capability_fail_closed() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-e");
        let lease = acquire(&app, actor, 5);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        service.disconnect(connection).unwrap();
        assert_eq!(service.disconnect(connection), Ok(()));
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
    }

    #[test]
    fn distinct_warp_histories_share_partition_and_validate_target_sequence() {
        let app = Phase2App::test();
        let source = account(&app, "alice", "invite-distinct-warp-source");
        let target = account(&app, "bob", "invite-distinct-warp-target");
        let source_lease = acquire(&app, source, 66);
        let target_lease = acquire(&app, target, 67);
        let service = app.presence();
        let source_connection = service
            .connect(
                source,
                runtime_fence(&source_lease),
                pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 1, 1).unwrap(),
                    0,
                    Direction::East,
                    7,
                    7,
                    PlayerState::Overworld,
                ),
            )
            .unwrap();
        let target_connection = service
            .connect(
                target,
                runtime_fence(&target_lease),
                pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 2, 1).unwrap(),
                    0,
                    Direction::West,
                    41,
                    41,
                    PlayerState::Overworld,
                ),
            )
            .unwrap();

        let source_events = service.drain(source_connection).unwrap().events;
        let target_events = service.drain(target_connection).unwrap().events;
        let [PresenceOutboundV1::Spawn(target_spawn)] = source_events.as_slice() else {
            panic!("source must receive the target spawn");
        };
        let [PresenceOutboundV1::Spawn(source_spawn)] = target_events.as_slice() else {
            panic!("target must receive the source spawn");
        };
        assert_eq!(target_spawn.handle(), target_connection.handle());
        assert_eq!(target_spawn.state().pose().warp_sequence(), 41);
        assert_eq!(source_spawn.handle(), source_connection.handle());
        assert_eq!(source_spawn.state().pose().warp_sequence(), 7);

        let valid_observation = observed_interaction(
            target_connection.handle(),
            target_spawn.server_sequence(),
            41,
            2,
            1,
        );
        let validated = service
            .validate_interaction(source_connection, valid_observation)
            .unwrap();
        assert_eq!(validated.target_handle, target_connection.handle());

        let before = liveness(&service, target_connection.handle());
        let invalid_observation = observed_interaction(
            target_connection.handle(),
            target_spawn.server_sequence(),
            7,
            2,
            1,
        );
        assert_eq!(
            service.validate_interaction(source_connection, invalid_observation),
            Err(PresenceServiceError::InteractionObservationMismatch)
        );
        assert_eq!(liveness(&service, target_connection.handle()), before);
        assert_eq!(service.connection_count(), Ok(2));
    }

    #[test]
    fn interaction_observations_are_exact_and_read_only() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-f");
        let second = account(&app, "bob", "invite-g");
        let first_lease = acquire(&app, first, 6);
        let second_lease = acquire(&app, second, 7);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(two).unwrap();
        let spawn = service.drain(one).unwrap().events;
        let [PresenceOutboundV1::Spawn(spawn)] = spawn.as_slice() else {
            panic!("expected one remote spawn");
        };
        assert_eq!(spawn.handle(), two.handle());
        assert_eq!(spawn.server_sequence(), 1);
        let observed = PresenceInteractionV1::new(
            two.handle(),
            spawn.server_sequence(),
            spawn.state().pose().warp_sequence(),
            spawn.state().pose().location().x,
            spawn.state().pose().location().y,
        )
        .unwrap();
        let count_before = service.connection_count().unwrap();
        let validated = service.validate_interaction(one, observed).unwrap();
        assert_eq!(validated.target_handle, two.handle());
        assert_eq!(service.connection_count().unwrap(), count_before);

        let wrong_sequence = PresenceInteractionV1::new(
            two.handle(),
            spawn.server_sequence().wrapping_add(1),
            spawn.state().pose().warp_sequence(),
            spawn.state().pose().location().x,
            spawn.state().pose().location().y,
        )
        .unwrap();
        assert_eq!(
            service.validate_interaction(one, wrong_sequence),
            Err(PresenceServiceError::InteractionObservationMismatch)
        );
        let wrong_direction = PresenceInteractionV1::new(
            two.handle(),
            spawn.server_sequence(),
            spawn.state().pose().warp_sequence(),
            3,
            1,
        )
        .unwrap();
        assert_eq!(
            service.validate_interaction(one, wrong_direction),
            Err(PresenceServiceError::InteractionObservationMismatch)
        );
        let self_interaction = PresenceInteractionV1::new(one.handle(), 1, 1, 1, 1).unwrap();
        assert_eq!(
            service.validate_interaction(one, self_interaction),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
    }

    #[test]
    fn interaction_matrix_rejects_unknown_hidden_stale_and_spoofed_observations() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-interaction-matrix");
        let second = account(&app, "bob", "invite-interaction-matrix-peer");
        let first_lease = acquire(&app, first, 49);
        let second_lease = acquire(&app, second, 50);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(two);
        let spawn = service.drain(one).unwrap().events;
        let [PresenceOutboundV1::Spawn(spawn)] = spawn.as_slice() else {
            panic!("expected one interaction target spawn");
        };
        let target_sequence = spawn.server_sequence();
        let target_handle = spawn.handle();
        let before_source = liveness(&service, one.handle());

        assert_eq!(
            service.validate_interaction(
                one,
                observed_interaction(
                    PresenceHandle::new(0xfeed).unwrap(),
                    target_sequence,
                    1,
                    2,
                    1,
                ),
            ),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
        assert_eq!(
            service.validate_interaction(
                one,
                observed_interaction(target_handle, target_sequence, 2, 2, 1),
            ),
            Err(PresenceServiceError::InteractionObservationMismatch)
        );
        assert_eq!(
            service.validate_interaction(
                one,
                observed_interaction(target_handle, target_sequence, 1, 3, 1),
            ),
            Err(PresenceServiceError::InteractionObservationMismatch)
        );
        assert_eq!(liveness(&service, one.handle()), before_source);

        service
            .submit_state(two, pose(2, 1, 2, PlayerState::Hidden))
            .unwrap();
        service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert_eq!(
            service.validate_interaction(
                one,
                observed_interaction(target_handle, target_sequence + 1, 1, 2, 1),
            ),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
        let before_hidden = liveness(&service, two.handle());
        assert_eq!(
            service.validate_interaction(
                one,
                observed_interaction(target_handle, target_sequence + 1, 1, 2, 1),
            ),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
        assert_eq!(liveness(&service, two.handle()), before_hidden);
    }

    #[test]
    fn stale_interaction_target_is_rejected_without_refreshing_liveness() {
        let app = Phase2App::test();
        let source = account(&app, "alice", "invite-interaction-stale");
        let target = account(&app, "bob", "invite-interaction-stale-peer");
        let source_lease = acquire(&app, source, 51);
        let target_lease = acquire(&app, target, 52);
        let service = app.presence();
        let source_connection = service
            .connect(
                source,
                runtime_fence(&source_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let target_connection = service
            .connect(
                target,
                runtime_fence(&target_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(target_connection);
        let _ = service.drain(source_connection);
        let before = liveness(&service, target_connection.handle());
        let stale_timestamp = app.store.now().saturating_sub(PRESENCE_STALE_MS);
        set_liveness(&service, target_connection.handle(), stale_timestamp);
        assert_eq!(
            service.validate_interaction(
                source_connection,
                observed_interaction(target_connection.handle(), 1, 1, 2, 1),
            ),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
        assert_eq!(
            liveness(&service, target_connection.handle()),
            (stale_timestamp, before.1)
        );
    }

    #[test]
    fn interaction_matrix_rejects_foreign_channel() {
        let app = app_with_unique_test_entropy();
        let source = account(&app, "alice", "invite-interaction-geometry");
        let foreign = account(&app, "bob", "invite-interaction-foreign");
        let source_lease = acquire(&app, source, 53);
        let foreign_lease = acquire(&app, foreign, 54);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&foreign.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .state
                    .world_zone
                    .channel = 2;
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        let source_connection = service
            .connect(
                source,
                runtime_fence(&source_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let foreign_connection = service
            .connect(
                foreign,
                runtime_fence(&foreign_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert_eq!(
            service.validate_interaction(
                source_connection,
                observed_interaction(foreign_connection.handle(), 1, 1, 2, 1),
            ),
            Err(PresenceServiceError::InteractionTargetUnavailable)
        );
    }

    #[test]
    fn interaction_matrix_rejects_elevation_facing_distance_and_overflow() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        assert_interaction_rejected(
            &app,
            &service,
            InteractionCase {
                source_sequence: 11,
                target_sequence: 12,
                source_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 1, 1).unwrap(),
                    0,
                    Direction::East,
                    11,
                    1,
                    PlayerState::Overworld,
                ),
                target_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 2, 1).unwrap(),
                    1,
                    Direction::West,
                    12,
                    1,
                    PlayerState::Overworld,
                ),
                observed_x: 2,
                observed_y: 1,
                expected: PresenceServiceError::InteractionOutOfRange,
            },
        );
        assert_interaction_rejected(
            &app,
            &service,
            InteractionCase {
                source_sequence: 21,
                target_sequence: 22,
                source_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 4, 4).unwrap(),
                    0,
                    Direction::North,
                    21,
                    1,
                    PlayerState::Overworld,
                ),
                target_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 5, 4).unwrap(),
                    0,
                    Direction::West,
                    22,
                    1,
                    PlayerState::Overworld,
                ),
                observed_x: 5,
                observed_y: 4,
                expected: PresenceServiceError::InteractionOutOfRange,
            },
        );
        assert_interaction_rejected(
            &app,
            &service,
            InteractionCase {
                source_sequence: 31,
                target_sequence: 32,
                source_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 6, 6).unwrap(),
                    0,
                    Direction::East,
                    31,
                    1,
                    PlayerState::Overworld,
                ),
                target_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, 8, 6).unwrap(),
                    0,
                    Direction::West,
                    32,
                    1,
                    PlayerState::Overworld,
                ),
                observed_x: 8,
                observed_y: 6,
                expected: PresenceServiceError::InteractionOutOfRange,
            },
        );
    }

    #[test]
    fn interaction_coordinate_overflow_is_rejected() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        assert_interaction_rejected(
            &app,
            &service,
            InteractionCase {
                source_sequence: 41,
                target_sequence: 42,
                source_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, i16::MAX, 1).unwrap(),
                    0,
                    Direction::East,
                    41,
                    1,
                    PlayerState::Overworld,
                ),
                target_state: pose_with(
                    WorldLocation::new(RegionId::Hoenn, 0, 9, i16::MAX, 1).unwrap(),
                    0,
                    Direction::West,
                    42,
                    1,
                    PlayerState::Overworld,
                ),
                observed_x: i16::MAX,
                observed_y: 1,
                expected: PresenceServiceError::InteractionOutOfRange,
            },
        );
    }

    #[test]
    fn travel_is_partition_left_and_staleness_is_exactly_1500_ms() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-h");
        let second = account(&app, "bob", "invite-i");
        let first_lease = acquire(&app, first, 8);
        let second_lease = acquire(&app, second, 9);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        let travel = pose_at(
            WorldLocation::new(RegionId::Hoenn, 0, 10, 1, 1).unwrap(),
            2,
            PlayerState::Overworld,
        );
        assert_eq!(
            service.submit_state(one, travel),
            Ok(PresenceSubmitOutcome::DisconnectedUnsupportedTravel)
        );
        let events = service.drain(two).unwrap().events;
        assert!(matches!(
            events.as_slice(),
            [PresenceOutboundV1::Despawn(event)]
                if event.reason() == DespawnReason::PartitionLeft
        ));
        assert_eq!(service.connection_count().unwrap(), 1);

        let now = app.store.now();
        let not_yet_stale = service.tick_at(now + PRESENCE_TICK_MS).unwrap();
        assert_eq!(not_yet_stale.removed_connections, 0);
        let stale = service.tick_at(now + PRESENCE_STALE_MS).unwrap();
        assert_eq!(stale.removed_connections, 1);
        assert_eq!(service.connection_count().unwrap(), 0);
        assert_eq!(service.next_tick_at_ms().unwrap(), None);
    }

    #[test]
    fn admission_rejects_wrong_map_without_mutating_presence() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-p");
        let lease = acquire(&app, actor, 16);
        let service = app.presence();
        let wrong_map = pose_at(
            WorldLocation::new(RegionId::Hoenn, 0, 10, 1, 1).unwrap(),
            1,
            PlayerState::Overworld,
        );
        assert_eq!(
            service.connect(actor, runtime_fence(&lease), wrong_map),
            Err(PresenceServiceError::UnsupportedZone)
        );
        assert_eq!(service.connection_count().unwrap(), 0);
        assert_eq!(service.next_tick_at_ms().unwrap(), None);
    }

    #[test]
    fn stable_fence_mismatch_is_rejected_before_insertion() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-q");
        let second = account(&app, "bob", "invite-r");
        let first_lease = acquire(&app, first, 17);
        let second_lease = acquire(&app, second, 18);
        let service = app.presence();
        assert_eq!(
            service.connect(
                first,
                runtime_fence(&second_lease),
                pose(1, 1, 1, PlayerState::Overworld)
            ),
            Err(PresenceServiceError::LeaseFenceMismatch)
        );
        assert_eq!(service.connection_count().unwrap(), 0);
        assert_eq!(first_lease.character_id, first.character_id);
    }

    #[test]
    fn channels_are_distinct_presence_partitions() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-s");
        let second = account(&app, "bob", "invite-t");
        let first_lease = acquire(&app, first, 19);
        let second_lease = acquire(&app, second, 20);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&second.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .state
                    .world_zone
                    .channel = 2;
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert!(service.drain(one).unwrap().is_empty());
        assert!(service.drain(two).unwrap().is_empty());
    }

    #[test]
    fn authoritative_channel_change_disconnects_on_the_next_tick() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-channel-change");
        let lease = acquire(&app, actor, 37);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .state
                    .world_zone
                    .channel = 2;
                Ok(())
            })
            .unwrap();
        let report = service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert_eq!(report.removed_connections, 1);
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
    }

    #[test]
    fn same_character_replacement_rotates_capability_at_constant_capacity() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-u");
        let lease = acquire(&app, actor, 21);
        let service = app.presence();
        let old = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let replacement = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert_ne!(old.handle(), replacement.handle());
        assert_eq!(service.connection_count().unwrap(), 1);
        assert_eq!(service.drain(old), Err(PresenceServiceError::NotConnected));
        assert!(service.drain(replacement).unwrap().is_empty());
    }

    #[test]
    fn same_character_replacement_cannot_enter_a_full_destination_partition() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        let old_actor = account(&app, "traveler", "invite-destination-old");
        let old_lease = acquire(&app, old_actor, 38);
        let old_connection = service
            .connect(
                old_actor,
                runtime_fence(&old_lease),
                pose(0, 1, 1, PlayerState::Overworld),
            )
            .unwrap();

        let mut destination = Vec::new();
        for index in 0..PRESENCE_MAX_PARTITION_CONNECTIONS {
            let username = format!("dest{index}");
            let invitation = format!("invite-destination-{index}");
            let actor = account(&app, &username, &invitation);
            let lease = acquire(
                &app,
                actor,
                39 + u128::try_from(index).expect("bounded test index"),
            );
            app.store
                .write_transaction(|state| -> Result<(), Phase2Error> {
                    state
                        .characters
                        .get_mut(&actor.character_id)
                        .ok_or(Phase2Error::NotFound)?
                        .state
                        .world_zone
                        .channel = 2;
                    Ok(())
                })
                .unwrap();
            destination.push((actor, lease));
        }
        for (index, (actor, lease)) in destination.iter().enumerate() {
            service
                .connect(
                    *actor,
                    runtime_fence(lease),
                    pose(
                        i16::try_from(index + 1).expect("bounded test index"),
                        1,
                        1,
                        PlayerState::Overworld,
                    ),
                )
                .unwrap();
        }

        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&old_actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .state
                    .world_zone
                    .channel = 2;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            service.connect(
                old_actor,
                runtime_fence(&old_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            ),
            Err(PresenceServiceError::PartitionCapacity)
        );
        assert_eq!(
            service.connection_count(),
            Ok(PRESENCE_MAX_PARTITION_CONNECTIONS + 1)
        );
        assert!(service.drain(old_connection).is_ok());
        assert_eq!(
            service.submit_state(old_connection, pose(0, 1, 2, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Accepted)
        );
    }

    #[test]
    fn handle_reuse_rejects_every_stale_generation_operation() {
        let app = app_with_handle_reuse_entropy();
        let actor = account(&app, "alice", "invite-aba");
        let lease = acquire(&app, actor, 24);
        let service = app.presence();
        let old = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        service.disconnect(old).unwrap();

        let replacement = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert_eq!(old.handle(), replacement.handle());
        assert_ne!(old.generation, replacement.generation);

        assert_eq!(service.drain(old), Err(PresenceServiceError::NotConnected));
        assert_eq!(
            service.submit_state(old, pose(3, 1, 2, PlayerState::Overworld)),
            Err(PresenceServiceError::NotConnected)
        );
        let stale_interaction =
            PresenceInteractionV1::new(replacement.handle(), 1, 1, 2, 1).unwrap();
        assert_eq!(
            service.validate_interaction(old, stale_interaction),
            Err(PresenceServiceError::NotConnected)
        );
        assert_eq!(service.disconnect(old), Ok(()));
        assert_eq!(service.connection_count(), Ok(1));

        service
            .submit_state(replacement, pose(3, 1, 2, PlayerState::Overworld))
            .unwrap();
        assert_eq!(
            service
                .tick_at(app.store.now() + PRESENCE_TICK_MS)
                .unwrap()
                .published_updates,
            1
        );
        assert_eq!(service.connection_count(), Ok(1));
        assert!(service.drain(replacement).unwrap().is_empty());
    }

    #[test]
    fn connect_and_lease_release_are_linearizable_under_the_shared_gate() {
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let release_started = Arc::new(Barrier::new(2));
        let (app, repository, _clock) =
            app_with_blocking_repository(entered.clone(), resume.clone());
        let actor = account(&app, "alice", "invite-linearizable");
        let lease = acquire(&app, actor, 25);
        let service = app.presence();
        repository.block_next_read.store(true, Ordering::SeqCst);

        let connect_service = service.clone();
        let connect_fence = runtime_fence(&lease);
        let connect_thread = thread::spawn(move || {
            connect_service.connect(actor, connect_fence, pose(1, 1, 1, PlayerState::Overworld))
        });
        entered.wait();

        let release_app = app.clone();
        let release_request = ReleaseLeaseRequest::new(lease.fence(), id(IdempotencyKey::new, 35));
        let release_finished = Arc::new(AtomicBool::new(false));
        let release_finished_flag = release_finished.clone();
        let release_started_thread = release_started.clone();
        let release_thread = thread::spawn(move || {
            release_started_thread.wait();
            let result = release_app.release(actor, release_request);
            release_finished_flag.store(true, Ordering::SeqCst);
            result
        });
        release_started.wait();
        assert!(!release_finished.load(Ordering::SeqCst));

        resume.wait();
        let connected = connect_thread
            .join()
            .expect("connect thread must not panic")
            .expect("connect should linearize before lease release");
        assert!(
            release_thread
                .join()
                .expect("release thread must not panic")
                .is_ok()
        );
        assert_eq!(service.connection_count(), Ok(0));
        assert_eq!(
            service.drain(connected),
            Err(PresenceServiceError::NotConnected)
        );
        assert_eq!(
            service.connect(
                actor,
                runtime_fence(&lease),
                pose(2, 1, 1, PlayerState::Overworld),
            ),
            Err(PresenceServiceError::LeaseInactive)
        );
    }

    #[test]
    fn repository_failure_preserves_due_deadline_and_pending_presence() {
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let (app, repository, _clock) = app_with_blocking_repository(entered, resume);
        let actor = account(&app, "alice", "invite-repository-failure");
        let lease = acquire(&app, actor, 45);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(connection);
        service
            .submit_state(connection, pose(2, 1, 2, PlayerState::Overworld))
            .unwrap();
        let deadline = service.next_tick_at_ms().unwrap().unwrap();
        repository.fail_next_read.store(true, Ordering::SeqCst);
        assert_eq!(
            service.tick_at(deadline),
            Err(PresenceServiceError::Internal)
        );
        assert_eq!(service.next_tick_at_ms(), Ok(Some(deadline)));
        assert_eq!(service.connection_count(), Ok(1));
        assert_eq!(service.drain(connection).unwrap().events, Vec::new());

        let report = service.tick_at(deadline).unwrap();
        assert_eq!(report.published_updates, 1);
        assert_eq!(
            service.next_tick_at_ms(),
            Ok(Some(deadline + PRESENCE_TICK_MS))
        );
    }

    #[test]
    fn gated_tick_linearizes_before_same_character_replacement() {
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let replacement_started = Arc::new(Barrier::new(2));
        let (app, repository, _clock) =
            app_with_blocking_repository(entered.clone(), resume.clone());
        let actor = account(&app, "alice", "invite-tick-replacement");
        let lease = acquire(&app, actor, 46);
        let service = app.presence();
        let old = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(old);
        repository.block_next_read.store(true, Ordering::SeqCst);

        let tick_service = service.clone();
        let tick_thread = thread::spawn(move || tick_service.tick_at(1_700_000_000_100));
        entered.wait();

        let replacement_service = service.clone();
        let replacement_started_thread = replacement_started.clone();
        let replacement_thread = thread::spawn(move || {
            replacement_started_thread.wait();
            replacement_service.connect(
                actor,
                runtime_fence(&lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
        });
        replacement_started.wait();
        resume.wait();
        let tick_report = tick_thread
            .join()
            .expect("tick thread must not panic")
            .expect("tick should complete before replacement");
        assert_eq!(tick_report.observed_connections, 1);
        let replacement = replacement_thread
            .join()
            .expect("replacement thread must not panic")
            .expect("replacement should complete after tick");
        assert_eq!(service.drain(old), Err(PresenceServiceError::NotConnected));
        assert_eq!(service.connection_count(), Ok(1));
        assert!(service.drain(replacement).unwrap().is_empty());
    }

    #[test]
    fn heartbeat_and_reconnect_reconcile_presence_before_return() {
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let heartbeat_started = Arc::new(Barrier::new(2));
        let (app, repository, clock) =
            app_with_blocking_repository(entered.clone(), resume.clone());
        let actor = account(&app, "alice", "invite-heartbeat-reconnect");
        let lease = acquire(&app, actor, 47);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(connection);
        repository.block_next_read.store(true, Ordering::SeqCst);

        let tick_service = service.clone();
        let tick_thread = thread::spawn(move || tick_service.tick_at(1_700_000_000_100));
        entered.wait();

        let heartbeat_app = app.clone();
        let heartbeat_started_thread = heartbeat_started.clone();
        let heartbeat_thread = thread::spawn(move || {
            heartbeat_started_thread.wait();
            heartbeat_app.heartbeat(actor, HeartbeatLeaseRequest::new(lease.fence()))
        });
        heartbeat_started.wait();
        resume.wait();
        tick_thread
            .join()
            .expect("tick thread must not panic")
            .expect("tick should complete before heartbeat");
        let renewed = heartbeat_thread
            .join()
            .expect("heartbeat thread must not panic")
            .expect("heartbeat should complete after tick");
        assert_eq!(
            service
                .inner
                .state
                .lock()
                .unwrap()
                .entries
                .get(&connection.handle())
                .unwrap()
                .lease_expires_at_ms,
            renewed.expires_at.value()
        );

        clock.set(renewed.expires_at.value() + 1);
        let reconnected = app
            .reconnect(
                actor,
                ReconnectLeaseRequest::new(renewed.fence(), id(IdempotencyKey::new, 48)),
            )
            .unwrap();
        assert_ne!(
            reconnected.stable_runtime_session(),
            renewed.stable_runtime_session()
        );
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
        assert_eq!(service.connection_count(), Ok(0));
    }

    #[test]
    fn rfc1982_source_wrap_and_ambiguous_values_are_deterministic() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-v");
        let second = account(&app, "bob", "invite-w");
        let first_lease = acquire(&app, first, 22);
        let second_lease = acquire(&app, second, 23);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, u32::MAX, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        assert_eq!(
            service.submit_state(one, pose(1, 1, 1, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Accepted)
        );
        assert_eq!(
            service.submit_state(one, pose(1, 1, 0x8000_0001, PlayerState::Overworld)),
            Ok(PresenceSubmitOutcome::Ignored)
        );
        service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(event)] if event.server_sequence() == 2
        ));
    }

    #[test]
    fn hidden_resurface_has_ordered_lifecycle_sequences() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-x");
        let second = account(&app, "bob", "invite-y");
        let first_lease = acquire(&app, first, 24);
        let second_lease = acquire(&app, second, 25);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        service
            .submit_state(one, pose(1, 1, 2, PlayerState::Hidden))
            .unwrap();
        service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Despawn(event)]
                if event.server_sequence() == 2 && event.reason() == DespawnReason::Hidden
        ));
        service
            .submit_state(one, pose(1, 1, 3, PlayerState::Overworld))
            .unwrap();
        service
            .tick_at(app.store.now() + 2 * PRESENCE_TICK_MS)
            .unwrap();
        assert!(matches!(
            service.drain(two).unwrap().events.as_slice(),
            [PresenceOutboundV1::Spawn(event)] if event.server_sequence() == 3
        ));
    }

    #[test]
    fn release_evicts_presence_before_returning_success() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-z");
        let lease = acquire(&app, actor, 26);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        app.release(
            actor,
            ReleaseLeaseRequest::new(lease.fence(), id(IdempotencyKey::new, 36)),
        )
        .unwrap();
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
        assert_eq!(service.connection_count().unwrap(), 0);
    }

    #[test]
    fn app_clones_share_ephemeral_presence_state() {
        let app = Phase2App::test();
        let clone = app.clone();
        let actor = account(&app, "alice", "invite-aa");
        let lease = acquire(&app, actor, 27);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert_eq!(clone.presence().connection_count(), Ok(1));
        assert!(clone.presence().drain(connection).is_ok());
    }

    #[test]
    fn released_lease_is_rejected_without_presence_mutation() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-ab");
        let lease = acquire(&app, actor, 28);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .leases
                    .get_mut(&actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .released = true;
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        assert_eq!(
            service.connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld)
            ),
            Err(PresenceServiceError::LeaseInactive)
        );
        assert_eq!(service.connection_count().unwrap(), 0);
    }

    #[test]
    fn disabled_user_is_rejected_even_with_a_valid_lease() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-ac");
        let lease = acquire(&app, actor, 29);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .users_by_id
                    .get_mut(&actor.user_id)
                    .ok_or(Phase2Error::NotFound)?
                    .disabled = true;
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        assert_eq!(
            service.connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld)
            ),
            Err(PresenceServiceError::Authentication)
        );
        assert_eq!(service.connection_count().unwrap(), 0);
    }

    #[test]
    fn character_ownership_is_checked_against_the_authenticated_user() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-ad");
        let second = account(&app, "bob", "invite-ae");
        let first_lease = acquire(&app, first, 30);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&first.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .owner = second.user_id;
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        assert_eq!(
            service.connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld)
            ),
            Err(PresenceServiceError::Authentication)
        );
    }

    #[test]
    fn save_revision_does_not_participate_in_presence_admission() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-af");
        let lease = acquire(&app, actor, 31);
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .revision = coop_cloud::Revision::new(999);
                Ok(())
            })
            .unwrap();
        let service = app.presence();
        assert!(
            service
                .connect(
                    actor,
                    runtime_fence(&lease),
                    pose(1, 1, 1, PlayerState::Overworld)
                )
                .is_ok()
        );
    }

    #[test]
    fn partition_capacity_counts_hidden_and_visible_connections() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        let mut actors = Vec::new();
        for index in 0..6_u128 {
            let username = format!("user{index}");
            let invitation = format!("invite-ag-{index}");
            let actor = account(&app, &username, &invitation);
            let lease = acquire(&app, actor, 40 + index);
            let state = pose(
                i16::try_from(index).expect("bounded test index"),
                1,
                1,
                if index == 0 {
                    PlayerState::Hidden
                } else {
                    PlayerState::Overworld
                },
            );
            actors.push((actor, lease, state));
        }
        for (actor, lease, state) in actors.iter().take(5) {
            service
                .connect(*actor, runtime_fence(lease), state.clone())
                .unwrap();
        }
        let (actor, lease, state) = &actors[5];
        assert_eq!(
            service.connect(*actor, runtime_fence(lease), state.clone()),
            Err(PresenceServiceError::PartitionCapacity)
        );
        assert_eq!(service.connection_count().unwrap(), 5);
    }

    #[test]
    fn authoritative_zone_change_disconnects_on_the_next_tick() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-am");
        let lease = acquire(&app, actor, 32);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .characters
                    .get_mut(&actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .state
                    .world_zone = coop_protocol::WorldZone::new(RegionId::Hoenn, "OLDALE_TOWN", 1)
                    .map_err(|_| Phase2Error::InvalidRequest)?;
                Ok(())
            })
            .unwrap();
        let report = service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert_eq!(report.removed_connections, 1);
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
    }

    #[test]
    fn lease_invalidation_disconnects_before_a_due_tick_returns() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-an");
        let lease = acquire(&app, actor, 33);
        let service = app.presence();
        let connection = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        app.store
            .write_transaction(|state| -> Result<(), Phase2Error> {
                state
                    .leases
                    .get_mut(&actor.character_id)
                    .ok_or(Phase2Error::NotFound)?
                    .released = true;
                Ok(())
            })
            .unwrap();
        let report = service.tick_at(app.store.now() + PRESENCE_TICK_MS).unwrap();
        assert_eq!(report.removed_connections, 1);
        assert_eq!(
            service.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
    }

    #[test]
    fn a_new_app_over_the_same_repository_does_not_restore_ephemeral_presence() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-ao");
        let lease = acquire(&app, actor, 34);
        let service = app.presence();
        service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let config = (*app.store.config)
            .clone()
            .with_adapters(app.store.repository.clone(), app.store.objects.clone());
        let fresh_app = Phase2App::new(config).unwrap();
        assert_eq!(service.connection_count().unwrap(), 1);
        assert_eq!(fresh_app.presence().connection_count().unwrap(), 0);
    }

    #[test]
    fn bounded_update_queues_drop_updates_and_cascade_critical_overflow() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        let mut actors = Vec::new();
        for index in 0..5_u128 {
            let username = format!("peer{index}");
            let invitation = format!("invite-ap-{index}");
            let actor = account(&app, &username, &invitation);
            let lease = acquire(&app, actor, 50 + index);
            actors.push((actor, lease));
        }
        let mut connections = Vec::new();
        for (index, (actor, lease)) in actors.iter().enumerate() {
            connections.push(
                service
                    .connect(
                        *actor,
                        runtime_fence(lease),
                        pose(
                            i16::try_from(index).expect("bounded test index"),
                            1,
                            1,
                            PlayerState::Overworld,
                        ),
                    )
                    .unwrap(),
            );
        }
        for connection in &connections {
            let _ = service.drain(*connection).unwrap();
        }
        let base = app.store.now();
        for round in 0..12_u32 {
            for (index, connection) in connections.iter().enumerate().skip(1) {
                service
                    .submit_state(
                        *connection,
                        pose(
                            i16::try_from(index).expect("bounded test index"),
                            1,
                            round + 2,
                            PlayerState::Overworld,
                        ),
                    )
                    .unwrap();
            }
            service
                .tick_at(base + (u64::from(round) + 1) * PRESENCE_TICK_MS)
                .unwrap();
        }
        let receiver_state = service.inner.state.lock().unwrap();
        let receiver = receiver_state
            .entries
            .get(&connections[0].handle())
            .unwrap();
        assert_eq!(receiver.queue.len(), PRESENCE_OUTBOUND_QUEUE_CAPACITY);
        assert_eq!(receiver.dropped_updates, 12);
        let observed = receiver
            .queue
            .iter()
            .map(|event| match event {
                PresenceOutboundV1::Update(update) => {
                    (update.handle(), update.state().source_sequence())
                }
                _ => panic!("full queue must contain only position updates"),
            })
            .collect::<Vec<_>>();
        let mut sorted_handles = connections[1..]
            .iter()
            .map(|connection| connection.handle())
            .collect::<Vec<_>>();
        sorted_handles.sort_unstable();
        let mut expected = (0..8_u32)
            .flat_map(|round| {
                sorted_handles
                    .iter()
                    .copied()
                    .map(move |handle| (handle, round + 2))
            })
            .collect::<Vec<_>>();
        expected[31].1 = 13;
        assert_eq!(observed, expected);
        drop(receiver_state);
        assert!(service.dropped_updates(connections[0]).unwrap() > 0);
        service.disconnect(connections[1]).unwrap();
        assert_eq!(
            service.dropped_updates(connections[0]),
            Err(PresenceServiceError::NotConnected)
        );
        assert!(service.connection_count().unwrap() < connections.len());
    }

    #[test]
    fn initial_spawn_fanout_is_ascending_and_non_self() {
        let app = app_with_unique_test_entropy();
        let service = app.presence();
        let mut actors = Vec::new();
        for index in 0..4_u128 {
            let actor = account(
                &app,
                &format!("sortpeer{index}"),
                &format!("invite-sort-peer-{index}"),
            );
            actors.push((actor, acquire(&app, actor, 60 + index)));
        }
        let mut connections = Vec::new();
        for (index, (actor, lease)) in actors.iter().enumerate() {
            let connection = service
                .connect(
                    *actor,
                    runtime_fence(lease),
                    pose(
                        i16::try_from(index).expect("bounded test index"),
                        1,
                        1,
                        PlayerState::Overworld,
                    ),
                )
                .unwrap();
            connections.push(connection);
            if index < 3 {
                let _ = service.drain(connection);
            }
        }
        let newcomer = connections[3];
        let newcomer_events = service.drain(newcomer).unwrap().events;
        let handles = newcomer_events
            .iter()
            .map(PresenceOutboundV1::handle)
            .collect::<Vec<_>>();
        assert_eq!(handles, {
            let mut sorted = connections[..3]
                .iter()
                .map(|connection| connection.handle())
                .collect::<Vec<_>>();
            sorted.sort_unstable();
            sorted
        });
        assert!(handles.iter().all(|handle| *handle != newcomer.handle()));
        assert!(
            newcomer_events
                .iter()
                .all(|event| matches!(event, PresenceOutboundV1::Spawn(_)))
        );
    }

    #[test]
    fn critical_disconnect_lifecycle_is_retained_in_fifo_order() {
        let app = Phase2App::test();
        let first = account(&app, "alice", "invite-critical-first");
        let second = account(&app, "bob", "invite-critical-second");
        let first_lease = acquire(&app, first, 64);
        let second_lease = acquire(&app, second, 65);
        let service = app.presence();
        let one = service
            .connect(
                first,
                runtime_fence(&first_lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let two = service
            .connect(
                second,
                runtime_fence(&second_lease),
                pose(2, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let _ = service.drain(one);
        let _ = service.drain(two);
        service.disconnect(two).unwrap();
        let events = service.drain(one).unwrap().events;
        assert!(matches!(
            events.as_slice(),
            [PresenceOutboundV1::Despawn(event)]
                if event.handle() == two.handle()
                    && event.server_sequence() == 2
                    && event.reason() == DespawnReason::Disconnected
        ));
        assert_eq!(service.connection_count(), Ok(1));
    }

    #[test]
    fn poisoned_presence_recovers_fail_closed_and_remains_usable() {
        let app = Phase2App::test();
        let actor = account(&app, "alice", "invite-o");
        let lease = acquire(&app, actor, 15);
        let service = app.presence();
        let old = service
            .connect(
                actor,
                runtime_fence(&lease),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = service.inner.state.lock().expect("state lock");
            panic!("deliberate test poison");
        }));
        assert!(poison.is_err());

        let renewed = app
            .heartbeat(actor, coop_cloud::HeartbeatLeaseRequest::new(lease.fence()))
            .unwrap();
        assert_eq!(
            renewed.stable_runtime_session(),
            lease.stable_runtime_session()
        );
        assert_eq!(service.drain(old), Err(PresenceServiceError::NotConnected));
        let fresh = service
            .connect(
                actor,
                runtime_fence(&renewed),
                pose(1, 1, 1, PlayerState::Overworld),
            )
            .unwrap();
        assert_eq!(service.dropped_updates(fresh), Ok(0));
        assert!(service.drain(fresh).unwrap().is_empty());
        service
            .submit_state(fresh, pose(2, 1, 2, PlayerState::Overworld))
            .unwrap();
        assert_eq!(
            service
                .tick_at(app.store.now() + PRESENCE_TICK_MS)
                .unwrap()
                .published_updates,
            1
        );
    }
}

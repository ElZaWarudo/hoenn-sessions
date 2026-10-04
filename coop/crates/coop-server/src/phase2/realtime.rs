//! Authenticated, bounded loopback realtime transport.
//!
//! Realtime tickets are short-lived capabilities.  The repository stores only
//! a domain-separated fingerprint; the plaintext ticket exists solely while a
//! mint or upgrade request is being processed.  The WebSocket adapter is
//! intentionally kept in this module so transport admission and presence
//! cleanup have one small, auditable boundary.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{
        FromRequest, Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use coop_cloud::{
    CharacterId, ClientRealtimeFrameV1, MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES,
    MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES, MintRealtimeTicketRequest, MintRealtimeTicketResponse,
    REALTIME_TICKET_ENTROPY_BYTES, REALTIME_TICKET_REQUEST_BODY_MAX_BYTES, RealtimeTicket,
    RefreshFamilyId, RuntimeLeaseFence, ServerRealtimeFrameV1, StableRuntimeSession,
};
use tokio::sync::{Semaphore, watch};
use tokio::time::{self, Instant as TokioInstant};
use zeroize::Zeroizing;

use super::auth;
use super::presence::{
    PRESENCE_TICK_MS, PresenceConnection, PresenceOutboundV1, PresenceServiceError,
};
use super::storage::{
    MAX_REALTIME_TICKETS_GLOBAL, RealtimeTicketRecord, State as StorageState, Store,
};

#[path = "progress_feed.rs"]
pub(crate) mod progress_feed;
use super::{AuthenticatedActor, Phase2App, Phase2Error};

const MAX_REALTIME_GLOBAL_SOCKETS: usize = 1_024;
const MAX_REALTIME_SOCKETS_PER_RUNTIME: usize = 2;
const MAX_TICKET_CANDIDATES: usize = 16;
/// Ticket minting replaces the user's current capability and persists the
/// complete state, so keep repeated requests from monopolizing the repository.
const TICKET_MINT_WINDOW: Duration = Duration::from_secs(60);
const MAX_TICKET_MINTS_PER_WINDOW: usize = 8;
const FIRST_FRAME_DEADLINE: Duration = Duration::from_millis(3_000);
const OUTBOUND_DEADLINE: Duration = Duration::from_millis(500);
const CLOSE_DEADLINE: Duration = Duration::from_millis(250);
const INBOUND_WINDOW: Duration = Duration::from_millis(1_000);
const MAX_INBOUND_FRAMES: usize = 64;
const MAX_WRITE_BUFFER_BYTES: usize = 4_096;
const PRESENCE_WORK_WAIT: Duration = Duration::from_millis(100);
const PRESENCE_DISCONNECT_WAIT: Duration = Duration::from_secs(2);
/// Backstop for a silent socket. Presence stale eviction (1.5s without fresh
/// state) normally reaps idle clients first; this closes the transport when a
/// stalled tick or half-open connection leaves one behind.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_QUEUED_PROGRESS_EVENTS_PER_CHARACTER: usize = 32;
type ProgressRecipientId = u64;

/// Reports whether a realtime socket has been silent longer than the idle
/// budget. Extracted so the backstop bound is unit-tested.
fn is_idle_expired(last_inbound: Instant, now: Instant) -> bool {
    now.duration_since(last_inbound) >= IDLE_TIMEOUT
}

/// Process-local transport admission shared by all clones of one app.
pub(crate) struct RealtimeTransportState {
    admission: Mutex<AdmissionState>,
    ticket_mints: Mutex<HashMap<coop_cloud::UserId, VecDeque<Instant>>>,
    /// Realtime progress fanout is process-local and only retained while the
    /// recipient has an authenticated socket. The durable history lives in
    /// `State::group_progress_feeds`.
    progress_recipients:
        Mutex<HashMap<CharacterId, HashMap<ProgressRecipientId, StableRuntimeSession>>>,
    progress_outbound: Mutex<HashMap<ProgressRecipientId, VecDeque<ServerRealtimeFrameV1>>>,
    next_progress_recipient: AtomicU64,
    /// Presence methods are synchronous because their state and repository
    /// adapters are synchronous. Keep their work off Tokio worker threads and
    /// reject new work when the bounded executor is occupied.
    presence_workers: Arc<Semaphore>,
    tick_worker: Arc<Semaphore>,
    tick_started: AtomicBool,
    tick_signal: watch::Sender<u64>,
    cleanup: Mutex<Option<SyncSender<PresenceCleanup>>>,
}

impl RealtimeTransportState {
    pub(crate) fn new() -> Self {
        let (tick_signal, _) = watch::channel(0_u64);
        Self {
            admission: Mutex::new(AdmissionState::default()),
            ticket_mints: Mutex::new(HashMap::new()),
            progress_recipients: Mutex::new(HashMap::new()),
            progress_outbound: Mutex::new(HashMap::new()),
            next_progress_recipient: AtomicU64::new(1),
            presence_workers: Arc::new(Semaphore::new(31)),
            tick_worker: Arc::new(Semaphore::new(1)),
            tick_started: AtomicBool::new(false),
            tick_signal,
            cleanup: Mutex::new(None),
        }
    }

    fn ensure_cleanup_worker(&self) {
        let Ok(mut cleanup) = self.cleanup.lock() else {
            return;
        };
        if cleanup.is_some() {
            return;
        }
        let (sender, receiver) =
            std::sync::mpsc::sync_channel::<PresenceCleanup>(MAX_REALTIME_GLOBAL_SOCKETS);
        if std::thread::Builder::new()
            .name("coop-presence-cleanup".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let _ = request.service.disconnect(request.connection);
                }
            })
            .is_ok()
        {
            *cleanup = Some(sender);
        }
    }

    fn queue_cleanup(&self, request: PresenceCleanup) {
        let queued = self.cleanup.lock().ok().and_then(|cleanup| {
            cleanup
                .as_ref()
                .map(|sender| sender.try_send(request.clone()))
        });
        if !matches!(queued, Some(Ok(()))) {
            // The queue is bounded, but a cancelled socket must still release
            // its presence entry when the cleanup worker is unavailable.
            let _ = std::thread::Builder::new()
                .name("coop-presence-cleanup-fallback".to_owned())
                .spawn(move || {
                    let _ = request.service.disconnect(request.connection);
                });
        }
    }

    fn tick_receiver(&self) -> watch::Receiver<u64> {
        self.tick_signal.subscribe()
    }

    async fn run_presence<R, F>(&self, operation: F) -> Result<R, PresenceWorkError>
    where
        R: Send + 'static,
        F: FnOnce() -> R + Send + 'static,
    {
        self.run_presence_for(PRESENCE_WORK_WAIT, operation).await
    }

    async fn run_presence_for<R, F>(
        &self,
        wait: Duration,
        operation: F,
    ) -> Result<R, PresenceWorkError>
    where
        R: Send + 'static,
        F: FnOnce() -> R + Send + 'static,
    {
        let permit = self.presence_workers.clone().acquire_owned();
        let permit = time::timeout(wait, permit)
            .await
            .map_err(|_| PresenceWorkError::Busy)?
            .map_err(|_| PresenceWorkError::Failed)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .map_err(|_| PresenceWorkError::Panicked)
    }

    async fn run_tick(
        &self,
        presence: super::presence::PresenceService,
    ) -> Result<(), PresenceWorkError> {
        let permit = self
            .tick_worker
            .clone()
            .try_acquire_owned()
            .map_err(|_| PresenceWorkError::Busy)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            presence.tick()
        })
        .await
        .map_err(|_| PresenceWorkError::Panicked)?
        .map(|_| ())
        .map_err(|_| PresenceWorkError::Failed)
    }

    fn start_tick_worker(
        self: &Arc<Self>,
        presence: super::presence::PresenceService,
        mut shutdown: watch::Receiver<bool>,
    ) {
        self.ensure_cleanup_worker();
        if self
            .tick_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let state = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = time::interval(Duration::from_millis(PRESENCE_TICK_MS));
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            let mut sequence = 0_u64;
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        // A transient repository error is retried on the next
                        // tick. It must not turn into a broadcast disconnect.
                        let _ = state.run_tick(presence.clone()).await;
                        sequence = sequence.wrapping_add(1);
                        state.tick_signal.send_replace(sequence);
                    }
                }
            }
            state.tick_started.store(false, Ordering::Release);
        });
    }

    /// Reserves one authenticated ticket mint for this user.  Buckets are
    /// pruned lazily on the next mint, which keeps this process-local limiter
    /// bounded without a background task.
    fn admit_ticket_mint(&self, user_id: coop_cloud::UserId) -> Result<(), Phase2Error> {
        let now = Instant::now();
        let mut ticket_mints = match self.ticket_mints.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                let guard = poisoned.into_inner();
                self.ticket_mints.clear_poison();
                guard
            }
        };
        ticket_mints.retain(|_, timestamps| {
            while timestamps
                .front()
                .is_some_and(|at| now.duration_since(*at) >= TICKET_MINT_WINDOW)
            {
                timestamps.pop_front();
            }
            !timestamps.is_empty()
        });
        let timestamps = ticket_mints.entry(user_id).or_default();
        if timestamps.len() >= MAX_TICKET_MINTS_PER_WINDOW {
            return Err(Phase2Error::Busy);
        }
        timestamps.push_back(now);
        Ok(())
    }

    fn reserve_global(self: &Arc<Self>) -> Result<TransportReservation, Phase2Error> {
        let mut admission = self.admission.lock().map_err(|_| Phase2Error::Internal)?;
        if admission.global >= MAX_REALTIME_GLOBAL_SOCKETS {
            return Err(Phase2Error::Busy);
        }
        admission.global += 1;
        Ok(TransportReservation {
            state: Arc::clone(self),
            key: None,
        })
    }

    fn register_progress_recipient(&self, session: StableRuntimeSession) -> ProgressRecipientId {
        let recipient = self.next_progress_recipient.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut recipients) = self.progress_recipients.lock() {
            recipients
                .entry(session.character_id)
                .or_default()
                .insert(recipient, session);
            if let Ok(mut outbound) = self.progress_outbound.lock() {
                outbound.entry(recipient).or_default();
            }
        }
        recipient
    }

    fn unregister_progress_recipient(
        &self,
        character_id: CharacterId,
        recipient: ProgressRecipientId,
    ) {
        if let Ok(mut recipients) = self.progress_recipients.lock() {
            if let Some(ids) = recipients.get_mut(&character_id) {
                ids.remove(&recipient);
                if ids.is_empty() {
                    recipients.remove(&character_id);
                }
            }
            if let Ok(mut outbound) = self.progress_outbound.lock() {
                outbound.remove(&recipient);
            }
        }
    }

    pub(crate) fn queue_group_ended(
        &self,
        session: StableRuntimeSession,
        group_id: coop_cloud::GroupId,
    ) {
        let Ok(recipients) = self.progress_recipients.lock() else {
            return;
        };
        let Some(recipient_ids) = recipients.get(&session.character_id) else {
            return;
        };
        let Ok(mut outbound) = self.progress_outbound.lock() else {
            return;
        };
        let frame = ServerRealtimeFrameV1::group_ended(group_id);
        for (recipient_id, registered_session) in recipient_ids {
            if *registered_session == session {
                let queue = outbound.entry(*recipient_id).or_default();
                if queue.len() >= MAX_QUEUED_PROGRESS_EVENTS_PER_CHARACTER {
                    queue.pop_front();
                }
                queue.push_back(frame.clone());
            }
        }
    }

    pub(crate) fn queue_group_started(
        &self,
        session: StableRuntimeSession,
        group_id: coop_cloud::GroupId,
    ) {
        let Ok(recipients) = self.progress_recipients.lock() else {
            return;
        };
        let Some(recipient_ids) = recipients.get(&session.character_id) else {
            return;
        };
        let Ok(mut outbound) = self.progress_outbound.lock() else {
            return;
        };
        let frame = ServerRealtimeFrameV1::group_started(group_id, session.session_epoch);
        for (recipient_id, registered_session) in recipient_ids {
            if *registered_session == session {
                let queue = outbound.entry(*recipient_id).or_default();
                if queue.len() >= MAX_QUEUED_PROGRESS_EVENTS_PER_CHARACTER {
                    queue.pop_front();
                }
                queue.push_back(frame.clone());
            }
        }
    }

    fn queue_progress(&self, recipient: CharacterId, frame: ServerRealtimeFrameV1) {
        let Ok(recipients) = self.progress_recipients.lock() else {
            return;
        };
        let Some(recipient_ids) = recipients.get(&recipient) else {
            return;
        };
        let Ok(mut outbound) = self.progress_outbound.lock() else {
            return;
        };
        for recipient_id in recipient_ids.keys() {
            let queue = outbound.entry(*recipient_id).or_default();
            if queue.len() >= MAX_QUEUED_PROGRESS_EVENTS_PER_CHARACTER {
                queue.pop_front();
            }
            queue.push_back(frame.clone());
        }
    }

    fn drain_progress(&self, recipient: ProgressRecipientId) -> Vec<ServerRealtimeFrameV1> {
        let Ok(mut outbound) = self.progress_outbound.lock() else {
            return Vec::new();
        };
        let Some(queue) = outbound.get_mut(&recipient) else {
            return Vec::new();
        };
        let count = queue.len().min(MAX_QUEUED_PROGRESS_EVENTS_PER_CHARACTER);
        let frames = queue.drain(..count).collect::<Vec<_>>();
        if queue.is_empty() {
            outbound.remove(&recipient);
        }
        frames
    }
}

#[derive(Default)]
struct AdmissionState {
    global: usize,
    by_runtime: HashMap<(coop_cloud::UserId, StableRuntimeSession), usize>,
}

#[derive(Clone)]
struct PresenceCleanup {
    service: super::presence::PresenceService,
    connection: PresenceConnection,
}

#[derive(Debug)]
enum PresenceWorkError {
    Busy,
    Failed,
    Panicked,
}

impl PresenceWorkError {
    const fn close_code(&self) -> u16 {
        match self {
            Self::Busy => 1013,
            Self::Failed | Self::Panicked => 1011,
        }
    }
}

struct TransportReservation {
    state: Arc<RealtimeTransportState>,
    key: Option<(coop_cloud::UserId, StableRuntimeSession)>,
}

impl TransportReservation {
    fn bind(
        &mut self,
        user_id: coop_cloud::UserId,
        session: StableRuntimeSession,
    ) -> Result<(), Phase2Error> {
        if self.key.is_some() {
            return Err(Phase2Error::Internal);
        }
        let key = (user_id, session);
        let mut admission = self
            .state
            .admission
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        let current = admission.by_runtime.get(&key).copied().unwrap_or(0);
        if current >= MAX_REALTIME_SOCKETS_PER_RUNTIME {
            return Err(Phase2Error::Busy);
        }
        admission.by_runtime.insert(key, current + 1);
        self.key = Some(key);
        Ok(())
    }
}

impl Drop for TransportReservation {
    fn drop(&mut self) {
        let Ok(mut admission) = self.state.admission.lock() else {
            return;
        };
        admission.global = admission.global.saturating_sub(1);
        if let Some(key) = self.key.take()
            && let Some(count) = admission.by_runtime.get_mut(&key)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                admission.by_runtime.remove(&key);
            }
        }
    }
}

#[derive(Clone)]
struct Redemption {
    actor: AuthenticatedActor,
    family_id: RefreshFamilyId,
    runtime: RuntimeLeaseFence,
}

/// Adds the transport's cache policy to success, upgrade, and error responses.
async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(crate) fn router() -> Router<Phase2App> {
    Router::new()
        .route(
            "/v1/realtime/tickets",
            post(mint).layer(axum::extract::DefaultBodyLimit::max(
                REALTIME_TICKET_REQUEST_BODY_MAX_BYTES,
            )),
        )
        .route("/v1/realtime", get(upgrade))
        .layer(axum::middleware::from_fn(no_store))
}

async fn mint(
    State(app): State<Phase2App>,
    headers: HeaderMap,
    super::Phase2Json(request): super::Phase2Json<MintRealtimeTicketRequest>,
) -> Result<(StatusCode, Json<MintRealtimeTicketResponse>), Phase2Error> {
    let response = mint_ticket(&app, &headers, &request)?;
    Ok((StatusCode::OK, Json(response)))
}

/// Mints one capability for the complete revision-independent runtime fence.
#[allow(clippy::too_many_lines)]
fn mint_ticket(
    app: &Phase2App,
    headers: &HeaderMap,
    request: &MintRealtimeTicketRequest,
) -> Result<MintRealtimeTicketResponse, Phase2Error> {
    if request.realtime_version() != coop_cloud::RealtimeVersion::v1() {
        return Err(Phase2Error::InvalidRequest);
    }
    let runtime = request.runtime().clone();
    let catalog = app
        .store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let build = runtime.build.clone();
    let world_id = catalog
        .world_for_build(&build)
        .ok_or(Phase2Error::Authentication)?;
    let (actor, family_id) = auth::actor_and_family_from_headers(&app.store, headers)?;
    app.realtime.admit_ticket_mint(actor.user_id)?;
    // Entropy is deliberately consumed before entering the runtime gate.
    let mut candidates = Vec::with_capacity(MAX_TICKET_CANDIDATES);
    for _ in 0..MAX_TICKET_CANDIDATES {
        let mut bytes = Zeroizing::new([0_u8; REALTIME_TICKET_ENTROPY_BYTES]);
        app.store
            .config
            .entropy
            .fill(&mut bytes[..])
            .map_err(|_| Phase2Error::Internal)?;
        let Ok(ticket) = RealtimeTicket::from_bytes(*bytes) else {
            continue;
        };
        let fingerprint = *ticket.fingerprint().as_bytes();
        candidates.push((ticket, fingerprint));
    }
    if candidates.is_empty() {
        return Err(Phase2Error::Internal);
    }

    let _gate = app.store.lock_runtime_transition_gate();
    if runtime.session.character_id != actor.character_id {
        return Err(Phase2Error::Authentication);
    }
    let now = app.store.now();
    validate_runtime_state(&app.store, actor, &runtime, now, &build)?;
    let expires_at = now
        .checked_add(coop_cloud::REALTIME_TICKET_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let expires_at = Store::unix_timestamp(expires_at)?;

    let key = (actor.user_id, runtime.session);
    let candidate_index = app
        .store
        .read_transaction(|state| select_realtime_candidate(state, &candidates, key, now))?;
    let (ticket, fingerprint) = candidates.swap_remove(candidate_index);
    let response = MintRealtimeTicketResponse::v1(runtime.clone(), ticket, expires_at)
        .map_err(|_| Phase2Error::Internal)?;

    app.store.write_transaction(|state| {
        validate_realtime_indexes(state)?;
        // Release and expiry can change after the earlier read preflight.
        // Recheck the lease in the same transaction as ticket publication.
        validate_runtime_state_in_state(state, actor, &runtime, now, &build)?;
        // A new lease has no runtime binding yet. Its first ticket must still
        // follow the character's durable active snapshot; otherwise a client
        // can select another catalog ROM and resume stale regional data.
        let character = state
            .characters
            .get(&actor.character_id)
            .ok_or(Phase2Error::Authentication)?;
        let active_world = if let Some(snapshot_id) = character.active_snapshot {
            state
                .snapshots
                .get(&snapshot_id)
                .ok_or(Phase2Error::Internal)?
                .rom_world_id
        } else {
            // Registration currently creates a new character in the Main ROM.
            // Future alternate starting worlds require an explicit durable
            // starting-world choice, not the client's first ticket.
            coop_protocol::RomWorldId::new(1).map_err(|_| Phase2Error::Internal)?
        };
        if world_id != active_world {
            return Err(Phase2Error::Authentication);
        }
        let lease = state
            .leases
            .get(&actor.character_id)
            .ok_or(Phase2Error::Authentication)?;
        if lease.contract.stable_runtime_session() != runtime.session
            || lease.runtime_binding.as_ref().is_some_and(|binding| {
                binding.session != runtime.session
                    || binding.world_id != world_id
                    || binding.build != build
            })
        {
            return Err(Phase2Error::Authentication);
        }
        let existing = state.realtime_by_runtime.get(&key).copied();
        let expired = expired_realtime_tickets(state, now);
        let active_count = state
            .realtime_tickets
            .values()
            .filter(|record| record.expires_at > now)
            .count();
        let existing_is_active = existing.is_some_and(|old_fingerprint| {
            state
                .realtime_tickets
                .get(&old_fingerprint)
                .is_some_and(|record| record.expires_at > now)
        });
        if active_count >= MAX_REALTIME_TICKETS_GLOBAL && !existing_is_active {
            return Err(Phase2Error::Busy);
        }
        if state
            .realtime_tickets
            .get(&fingerprint)
            .is_some_and(|record| record.expires_at > now && existing != Some(fingerprint))
        {
            return Err(Phase2Error::Internal);
        }
        if let Some(old_fingerprint) = existing {
            let Some(old) = state.realtime_tickets.get(&old_fingerprint) else {
                return Err(Phase2Error::Internal);
            };
            if old.user_id != actor.user_id || old.session != runtime.session {
                return Err(Phase2Error::Internal);
            }
        }
        let lease = state
            .leases
            .get_mut(&actor.character_id)
            .ok_or(Phase2Error::Internal)?;
        // The repository callback is not rollback-safe on error.  All
        // fallible validation is complete before this mutation-only suffix.
        lease.runtime_binding = Some(super::storage::RuntimeWorldBinding {
            world_id,
            build: build.clone(),
            session: runtime.session,
        });
        for expired_fingerprint in expired {
            if let Some(record) = state.realtime_tickets.remove(&expired_fingerprint) {
                state.realtime_ticket_families.remove(&expired_fingerprint);
                let expired_key = (record.user_id, record.session);
                if state.realtime_by_runtime.get(&expired_key).copied() == Some(expired_fingerprint)
                {
                    state.realtime_by_runtime.remove(&expired_key);
                }
            }
        }
        if let Some(old_fingerprint) = existing
            && old_fingerprint != fingerprint
        {
            state.realtime_tickets.remove(&old_fingerprint);
            state.realtime_ticket_families.remove(&old_fingerprint);
            state.realtime_by_runtime.remove(&key);
        }
        state.realtime_tickets.insert(
            fingerprint,
            RealtimeTicketRecord {
                user_id: actor.user_id,
                character_id: actor.character_id,
                session: runtime.session,
                runtime: runtime.clone(),
                expires_at: expires_at.value(),
            },
        );
        state
            .realtime_ticket_families
            .insert(fingerprint, family_id);
        state.realtime_by_runtime.insert(key, fingerprint);
        Ok(())
    })?;
    Ok(response)
}

/// Selects a candidate without changing either ticket index.  Keeping this
/// preflight pure is important: an entropy collision or a full capability
/// store must not partially replace the previous runtime ticket.
fn select_realtime_candidate(
    state: &StorageState,
    candidates: &[(RealtimeTicket, [u8; 32])],
    key: (coop_cloud::UserId, StableRuntimeSession),
    now: u64,
) -> Result<usize, Phase2Error> {
    validate_realtime_indexes(state)?;
    let existing = state.realtime_by_runtime.get(&key).copied();
    let active_count = state
        .realtime_tickets
        .values()
        .filter(|record| record.expires_at > now)
        .count();
    if active_count >= MAX_REALTIME_TICKETS_GLOBAL
        && !existing.is_some_and(|fingerprint| {
            state
                .realtime_tickets
                .get(&fingerprint)
                .is_some_and(|record| record.expires_at > now)
        })
    {
        return Err(Phase2Error::Busy);
    }
    candidates
        .iter()
        .position(|(_, fingerprint)| {
            state
                .realtime_tickets
                .get(fingerprint)
                .is_none_or(|record| record.expires_at <= now)
        })
        .ok_or(Phase2Error::Internal)
}

fn expired_realtime_tickets(state: &StorageState, now: u64) -> Vec<[u8; 32]> {
    state
        .realtime_tickets
        .iter()
        .filter_map(|(fingerprint, record)| (record.expires_at <= now).then_some(*fingerprint))
        .collect()
}

fn validate_realtime_indexes(state: &StorageState) -> Result<(), Phase2Error> {
    if state.realtime_tickets.len() > MAX_REALTIME_TICKETS_GLOBAL
        || state.realtime_by_runtime.len() > MAX_REALTIME_TICKETS_GLOBAL
        || state.realtime_ticket_families.len() > MAX_REALTIME_TICKETS_GLOBAL
        || state
            .realtime_ticket_families
            .keys()
            .any(|fingerprint| !state.realtime_tickets.contains_key(fingerprint))
    {
        return Err(Phase2Error::Internal);
    }
    for (key, fingerprint) in &state.realtime_by_runtime {
        let Some(record) = state.realtime_tickets.get(fingerprint) else {
            return Err(Phase2Error::Internal);
        };
        if (record.user_id, record.session) != *key {
            return Err(Phase2Error::Internal);
        }
    }
    for (fingerprint, record) in &state.realtime_tickets {
        let key = (record.user_id, record.session);
        if state.realtime_by_runtime.get(&key).copied() != Some(*fingerprint)
            || record.character_id != record.runtime.session.character_id
            || record.session != record.runtime.session
        {
            return Err(Phase2Error::Internal);
        }
    }
    Ok(())
}

fn validate_runtime_state(
    store: &Store,
    actor: AuthenticatedActor,
    runtime: &RuntimeLeaseFence,
    now: u64,
    build: &coop_cloud::RuntimeBuildIdentity,
) -> Result<(), Phase2Error> {
    store.read_transaction(|state| {
        validate_runtime_state_in_state(state, actor, runtime, now, build)
    })
}

fn validate_runtime_state_in_state(
    state: &StorageState,
    actor: AuthenticatedActor,
    runtime: &RuntimeLeaseFence,
    now: u64,
    build: &coop_cloud::RuntimeBuildIdentity,
) -> Result<(), Phase2Error> {
    if state.handoff_for_member(actor.character_id) {
        return Err(Phase2Error::Conflict);
    }
    let user = state
        .users_by_id
        .get(&actor.user_id)
        .ok_or(Phase2Error::Authentication)?;
    if user.disabled || user.character_id != actor.character_id {
        return Err(Phase2Error::Authentication);
    }
    let character = state
        .characters
        .get(&actor.character_id)
        .ok_or(Phase2Error::Authentication)?;
    if character.owner != actor.user_id || character.state.character_id != actor.character_id {
        return Err(Phase2Error::Authentication);
    }
    let lease = state
        .leases
        .get(&actor.character_id)
        .ok_or(Phase2Error::Authentication)?;
    if lease.released
        || lease.contract.expires_at.value() <= now
        || lease.contract.stable_runtime_session() != runtime.session
        || runtime.build != *build
    {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

pub(super) fn validate_bound_runtime(
    state: &StorageState,
    catalog: &super::saves::build_catalog::TrustedBuildCatalog,
    actor: AuthenticatedActor,
    runtime: &RuntimeLeaseFence,
) -> Result<(), Phase2Error> {
    let lease = state
        .leases
        .get(&actor.character_id)
        .ok_or(Phase2Error::Authentication)?;
    let binding = lease
        .runtime_binding
        .as_ref()
        .ok_or(Phase2Error::Authentication)?;
    let character = state
        .characters
        .get(&actor.character_id)
        .ok_or(Phase2Error::Authentication)?;
    let active_world = character
        .active_snapshot
        .and_then(|id| state.snapshots.get(&id))
        .map(|snapshot| snapshot.rom_world_id)
        .unwrap_or(coop_protocol::RomWorldId::new(1).expect("Main world"));
    if active_world != binding.world_id {
        return Err(Phase2Error::Authentication);
    }
    if binding.session != runtime.session
        || binding.build != runtime.build
        || catalog.world_for_build(&runtime.build) != Some(binding.world_id)
    {
        return Err(Phase2Error::Authentication);
    }
    Ok(())
}

async fn upgrade(State(app): State<Phase2App>, request: Request<Body>) -> Response {
    if request.uri().query().is_some() {
        return Phase2Error::InvalidRequest.into_response();
    }
    let headers = request.headers().clone();
    if headers.contains_key(header::SEC_WEBSOCKET_PROTOCOL) {
        return Phase2Error::InvalidRequest.into_response();
    }
    // Complete framework validation before ticket parsing or consumption.
    let Ok(websocket) = WebSocketUpgrade::from_request(request, &app).await else {
        return Phase2Error::InvalidRequest.into_response();
    };
    let ticket = match ticket_from_headers(&headers) {
        Ok(ticket) => ticket,
        Err(error) => return error.into_response(),
    };
    let mut reservation = match app.realtime.reserve_global() {
        Ok(reservation) => reservation,
        Err(error) => return error.into_response(),
    };
    let redemption = match preflight_ticket(&app.store, &ticket) {
        Ok(redemption) => redemption,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = reservation.bind(redemption.actor.user_id, redemption.runtime.session) {
        return error.into_response();
    }
    if let Err(error) = consume_ticket(&app.store, &ticket, &redemption) {
        return error.into_response();
    }
    let task_app = app.clone();
    let websocket = websocket
        .read_buffer_size(MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_WRITE_BUFFER_BYTES)
        .max_message_size(MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES)
        .max_frame_size(MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES)
        .accept_unmasked_frames(false);
    websocket
        .on_upgrade(move |socket| realtime_session(task_app, reservation, redemption, socket))
        .into_response()
}

fn ticket_from_headers(headers: &HeaderMap) -> Result<RealtimeTicket, Phase2Error> {
    let values = headers.get_all(header::AUTHORIZATION);
    if values.iter().count() != 1 {
        return Err(Phase2Error::Authentication);
    }
    let value = values.iter().next().ok_or(Phase2Error::Authentication)?;
    if value.as_bytes().len() > 256 {
        return Err(Phase2Error::Authentication);
    }
    let value = value.to_str().map_err(|_| Phase2Error::Authentication)?;
    let value = value
        .strip_prefix("Bearer ")
        .filter(|ticket| !ticket.is_empty() && !ticket.contains(char::is_whitespace))
        .ok_or(Phase2Error::Authentication)?;
    RealtimeTicket::parse(value).map_err(|_| Phase2Error::Authentication)
}

fn preflight_ticket(store: &Store, ticket: &RealtimeTicket) -> Result<Redemption, Phase2Error> {
    let fingerprint = *ticket.fingerprint().as_bytes();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let _gate = store.lock_runtime_transition_gate();
    let now = store.now();
    store.read_transaction(|state| {
        validate_realtime_indexes(state)?;
        let record = state
            .realtime_tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?;
        let family_id = state
            .realtime_ticket_families
            .get(&fingerprint)
            .copied()
            .ok_or(Phase2Error::Authentication)?;
        let family = state
            .families
            .get(&family_id)
            .ok_or(Phase2Error::Authentication)?;
        if family.revoked
            || family.expires_at <= now
            || family.user_id != record.user_id
            || family.character_id != record.character_id
        {
            return Err(Phase2Error::Authentication);
        }
        if record.expires_at <= now {
            return Err(Phase2Error::Authentication);
        }
        let actor = AuthenticatedActor {
            user_id: record.user_id,
            character_id: record.character_id,
        };
        validate_runtime_state_in_state(state, actor, &record.runtime, now, &record.runtime.build)?;
        validate_bound_runtime(state, catalog, actor, &record.runtime)?;
        Ok(Redemption {
            actor,
            family_id,
            runtime: record.runtime.clone(),
        })
    })
}

fn consume_ticket(
    store: &Store,
    ticket: &RealtimeTicket,
    redemption: &Redemption,
) -> Result<(), Phase2Error> {
    let fingerprint = *ticket.fingerprint().as_bytes();
    let catalog = store
        .config
        .release_catalog
        .as_ref()
        .ok_or(Phase2Error::Internal)?;
    let _gate = store.lock_runtime_transition_gate();
    let now = store.now();
    store.write_transaction(|state| {
        validate_realtime_indexes(state)?;
        let record = state
            .realtime_tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?
            .clone();
        let family_id = state
            .realtime_ticket_families
            .get(&fingerprint)
            .copied()
            .ok_or(Phase2Error::Authentication)?;
        if record.expires_at <= now
            || record.user_id != redemption.actor.user_id
            || record.character_id != redemption.actor.character_id
            || family_id != redemption.family_id
            || record.runtime != redemption.runtime
        {
            return Err(Phase2Error::Authentication);
        }
        let family = state
            .families
            .get(&family_id)
            .ok_or(Phase2Error::Authentication)?;
        if family.revoked
            || family.expires_at <= now
            || family.user_id != record.user_id
            || family.character_id != record.character_id
        {
            return Err(Phase2Error::Authentication);
        }
        validate_runtime_state_in_state(
            state,
            redemption.actor,
            &record.runtime,
            now,
            &record.runtime.build,
        )?;
        validate_bound_runtime(state, catalog, redemption.actor, &record.runtime)?;
        let key = (record.user_id, record.session);
        if state.realtime_by_runtime.get(&key).copied() != Some(fingerprint) {
            return Err(Phase2Error::Internal);
        }
        state.realtime_tickets.remove(&fingerprint);
        state.realtime_ticket_families.remove(&fingerprint);
        state.realtime_by_runtime.remove(&key);
        Ok(())
    })
}

/// Ensures a cancellation between connection admission and the normal async
/// cleanup path still removes the process-local capability. Drop only queues
/// the bounded cleanup request; the blocking mutex and gate work stays on the
/// dedicated cleanup thread.
struct PresenceGuard {
    state: Arc<RealtimeTransportState>,
    service: super::presence::PresenceService,
    connection: Option<PresenceConnection>,
}

impl PresenceGuard {
    fn new(
        state: Arc<RealtimeTransportState>,
        service: super::presence::PresenceService,
        connection: PresenceConnection,
    ) -> Self {
        Self {
            state,
            service,
            connection: Some(connection),
        }
    }

    fn disarm(&mut self) {
        self.connection = None;
    }
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.state.queue_cleanup(PresenceCleanup {
                service: self.service.clone(),
                connection,
            });
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn realtime_session(
    app: Phase2App,
    _reservation: TransportReservation,
    redemption: Redemption,
    socket: WebSocket,
) {
    let mut socket = socket;
    let mut shutdown = app.shutdown.subscribe();
    if *shutdown.borrow() {
        let _ = close_socket(&mut socket, 1001).await;
        return;
    }
    let mut rate = InboundRate::default();
    let first_deadline = TokioInstant::now() + FIRST_FRAME_DEADLINE;
    let initial = loop {
        let message = tokio::select! {
            _ = shutdown.changed() => {
                let _ = close_socket(&mut socket, 1001).await;
                return;
            }
            message = next_before(&mut socket, first_deadline) => message,
        };
        let Some(message) = message else {
            let _ = close_socket(&mut socket, 1008).await;
            return;
        };
        match message {
            Ok(Message::Text(text)) => {
                if !rate.admit() {
                    let _ = close_socket(&mut socket, 1008).await;
                    return;
                }
                match coop_cloud::decode_client_realtime_frame(text.as_bytes()) {
                    Ok(ClientRealtimeFrameV1::PlayerState(state)) => break state,
                    Err(coop_cloud::RealtimeError::MessageTooLarge) => {
                        let _ = close_socket(&mut socket, 1009).await;
                        return;
                    }
                    Ok(
                        ClientRealtimeFrameV1::InteractRemotePlayer(_)
                        | ClientRealtimeFrameV1::Companion(_)
                        | ClientRealtimeFrameV1::SocialSignal(_)
                        | ClientRealtimeFrameV1::ProgressObservation(_),
                    )
                    | Err(_) => {
                        let _ = close_socket(&mut socket, 1008).await;
                        return;
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                if !rate.admit()
                    || send_message(&mut socket, Message::Pong(payload))
                        .await
                        .is_err()
                {
                    let _ = close_socket(&mut socket, 1008).await;
                    return;
                }
            }
            Ok(Message::Pong(_)) => {
                if !rate.admit() {
                    let _ = close_socket(&mut socket, 1008).await;
                    return;
                }
            }
            Ok(Message::Binary(_)) => {
                if !rate.admit() {
                    let _ = close_socket(&mut socket, 1008).await;
                    return;
                }
                let _ = close_socket(&mut socket, 1003).await;
                return;
            }
            Ok(Message::Close(_)) => return,
            Err(error) => {
                let _ = close_socket(&mut socket, websocket_error_close_code(&error)).await;
                return;
            }
        }
    };

    let presence = app.presence();
    let connected = match time::timeout(
        PRESENCE_WORK_WAIT,
        app.realtime.presence_workers.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let runtime = redemption.runtime.clone();
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let result = presence.connect_and_drain_with_family(
                    redemption.actor,
                    redemption.family_id,
                    runtime,
                    initial,
                );
                if let Err(Ok((connection, _))) = sender.send(result) {
                    // A disconnected websocket may cancel the waiter while
                    // the blocking connect is still in flight.
                    let _ = presence.disconnect(connection);
                }
            });
            receiver.await.map_err(|_| PresenceWorkError::Panicked)
        }
        Ok(Err(_)) => Err(PresenceWorkError::Failed),
        Err(_) => Err(PresenceWorkError::Busy),
    };
    let (connection, initial_drain) = match connected {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let _ = close_presence_error(&mut socket, error).await;
            return;
        }
        Err(error) => {
            let _ = close_socket(&mut socket, error.close_code()).await;
            return;
        }
    };
    app.realtime
        .start_tick_worker(app.presence(), app.shutdown.subscribe());
    let mut presence_guard =
        PresenceGuard::new(Arc::clone(&app.realtime), app.presence(), connection);
    let character_id = redemption.actor.character_id;
    let mut initial_frames = Vec::with_capacity(initial_drain.events.len() + 1);
    initial_frames.push(ServerRealtimeFrameV1::presence_ready(connection.handle()));
    initial_frames.extend(initial_drain.events.iter().map(server_frame));
    // Register before reading durable history. Any event accepted during the
    // replay read is queued for this exact socket and merged by event ID below.
    let progress_recipient = app
        .realtime
        .register_progress_recipient(redemption.runtime.session);
    let store = app.store.clone();
    let replay_actor = redemption.actor;
    let progress_frames = if let Ok(Ok(events)) = app
        .realtime
        .run_presence(move || progress_feed::recent_for_partner(&store, replay_actor))
        .await
    {
        // Replay only the newest bounded slice so a reconnect cannot turn
        // feed history into an unbounded initial write burst.
        merge_progress_frames(
            events
                .into_iter()
                .rev()
                .take(8)
                .rev()
                .map(ServerRealtimeFrameV1::progress_event)
                .collect(),
            app.realtime.drain_progress(progress_recipient),
        )
    } else {
        app.realtime.drain_progress(progress_recipient)
    };
    let store = app.store.clone();
    let partner_session = redemption.runtime.session;
    let pending_notice = app
        .realtime
        .run_presence(move || {
            super::sessions::pending_group_end_for_session(&store, partner_session)
        })
        .await;
    let pending_notice = match pending_notice {
        Ok(Ok(notice)) => notice,
        Ok(Err(_)) => {
            app.realtime
                .unregister_progress_recipient(character_id, progress_recipient);
            let _ = close_socket(&mut socket, 1011).await;
            return;
        }
        Err(error) => {
            app.realtime
                .unregister_progress_recipient(character_id, progress_recipient);
            let _ = close_socket(&mut socket, error.close_code()).await;
            return;
        }
    };
    let replay_notice = pending_notice
        .map(ServerRealtimeFrameV1::group_ended)
        .into_iter()
        .collect();
    // Drain again after the durable read. An expiry between the first drain
    // and this read is now present in both sources and deduped by group ID.
    let late_frames = app.realtime.drain_progress(progress_recipient);
    let mut startup_frames = merge_progress_frames(
        replay_notice,
        merge_progress_frames(progress_frames, late_frames),
    );
    if let Err(code) =
        filter_group_end_frames(&app, redemption.runtime.session, &mut startup_frames).await
    {
        app.realtime
            .unregister_progress_recipient(character_id, progress_recipient);
        let _ = close_socket(&mut socket, code).await;
        return;
    }
    if let Err(code) =
        filter_group_start_frames(&app, redemption.runtime.session, &mut startup_frames).await
    {
        app.realtime
            .unregister_progress_recipient(character_id, progress_recipient);
        let _ = close_socket(&mut socket, code).await;
        return;
    }
    initial_frames.extend(startup_frames);
    run_connected_session(
        &app,
        &mut socket,
        connection,
        progress_recipient,
        redemption.actor,
        redemption.runtime.clone(),
        initial_frames,
        rate,
        &mut shutdown,
    )
    .await;
    app.realtime
        .unregister_progress_recipient(character_id, progress_recipient);
    let presence = app.presence();
    let disconnected = app
        .realtime
        .run_presence_for(PRESENCE_DISCONNECT_WAIT, move || {
            presence.disconnect(connection)
        })
        .await;
    if matches!(disconnected, Ok(Ok(()))) {
        presence_guard.disarm();
    }
}

#[allow(clippy::too_many_lines)]
async fn run_connected_session(
    app: &Phase2App,
    socket: &mut WebSocket,
    connection: PresenceConnection,
    progress_recipient: ProgressRecipientId,
    actor: AuthenticatedActor,
    runtime: RuntimeLeaseFence,
    initial_frames: Vec<ServerRealtimeFrameV1>,
    mut rate: InboundRate,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) {
    if send_frames(socket, &initial_frames).await.is_err() {
        return;
    }

    let mut tick = app.realtime.tick_receiver();
    let mut last_inbound = Instant::now();
    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_ok() {
                    let _ = close_socket(socket, 1001).await;
                }
                return;
            }
            changed = tick.changed() => {
                if changed.is_err() {
                    return;
                }
                if is_idle_expired(last_inbound, Instant::now()) {
                    // Going away, not policy: clients never retry a 1008, and
                    // an idle reap must stay recoverable after a stall.
                    let _ = close_socket(socket, 1001).await;
                    return;
                }
                let presence = app.presence();
                let drain = match app.realtime.run_presence(move || presence.drain(connection)).await {
                    Ok(Ok(drain)) => drain,
                    Ok(Err(error)) => {
                        let _ = close_presence_error(socket, error).await;
                        return;
                    }
                    Err(PresenceWorkError::Busy) => continue,
                    Err(error) => {
                        let _ = close_socket(socket, error.close_code()).await;
                        return;
                    }
                };
                let mut frames = drain.events.iter().map(server_frame).collect::<Vec<_>>();
                frames.extend(app.realtime.drain_progress(progress_recipient));
                if let Err(code) = filter_group_end_frames(app, runtime.session, &mut frames).await {
                    let _ = close_socket(socket, code).await;
                    return;
                }
                if let Err(code) = filter_group_start_frames(app, runtime.session, &mut frames).await {
                    let _ = close_socket(socket, code).await;
                    return;
                }
                if send_frames(socket, &frames).await.is_err() {
                    return;
                }
            }
            message = socket.recv() => {
                let Some(message) = message else { return; };
                last_inbound = Instant::now();
                match message {
                    Ok(Message::Text(text)) => {
                        if !rate.admit() { let _ = close_socket(socket, 1008).await; return; }
                        match coop_cloud::decode_client_realtime_frame(text.as_bytes()) {
                            Ok(ClientRealtimeFrameV1::PlayerState(state)) => {
                                let presence = app.presence();
                                let result = app.realtime.run_presence(move || presence.submit_state(connection, state)).await;
                                match result {
                                    Err(error) => { let _ = close_socket(socket, error.close_code()).await; return; }
                                    Ok(Err(error)) => {
                                        let _ = close_presence_error(socket, error).await;
                                        return;
                                    }
                                    Ok(Ok(super::presence::PresenceSubmitOutcome::DisconnectedUnsupportedTravel)) => {
                                        let _ = close_socket(socket, 1008).await;
                                        return;
                                    }
                                    Ok(Ok(_)) => {},
                                }
                            }
                            Ok(ClientRealtimeFrameV1::InteractRemotePlayer(interaction)) => {
                                let presence = app.presence();
                                let interaction_target = interaction.handle();
                                let interaction_result = app.realtime.run_presence(move || {
                                    presence.validate_and_forward_interaction(connection, interaction)
                                }).await;
                                match classify_interaction_result(&mut *socket, interaction_target, interaction_result).await {
                                    Ok(Ok(_)) => {}
                                    Ok(Err(error)) => { let _ = close_presence_error(socket, error).await; return; }
                                    Err(error) => { let _ = close_socket(socket, error.close_code()).await; return; }
                                }
                            }
                            Ok(ClientRealtimeFrameV1::Companion(companion)) => {
                                let presence = app.presence();
                                match app.realtime.run_presence(move || presence.submit_companion(connection, companion)).await {
                                    Ok(Ok(_)) => {},
                                    Ok(Err(error)) => { let _ = close_presence_error(socket, error).await; return; }
                                    Err(error) => { let _ = close_socket(socket, error.close_code()).await; return; }
                                }
                            }
                            Ok(ClientRealtimeFrameV1::SocialSignal(signal)) => {
                                let presence = app.presence();
                                match app.realtime.run_presence(move || presence.submit_signal(connection, signal)).await {
                                    Ok(Ok(_)) => {},
                                    Ok(Err(error)) => { let _ = close_presence_error(socket, error).await; return; }
                                    Err(error) => { let _ = close_socket(socket, error.close_code()).await; return; }
                                }
                            }
                            Ok(ClientRealtimeFrameV1::ProgressObservation(observation)) => {
                                let store = app.store.clone();
                                let runtime = runtime.clone();
                                let accepted = app
                                    .realtime
                                    .run_presence(move || {
                                        progress_feed::accept(&store, actor, runtime, observation)
                                    })
                                    .await;
                                match accepted {
                                    Ok(Ok(Some(accepted))) => {
                                        app.realtime.queue_progress(
                                            accepted.recipient,
                                            accepted.frame,
                                        );
                                    }
                                    Ok(Ok(None)) => {}
                                    // A valid observation over the feed budget is
                                    // a soft rejection, not a protocol violation.
                                    Ok(Err(Phase2Error::Busy)) => {}
                                    Ok(Err(_)) => {
                                        let _ = close_socket(socket, 1008).await;
                                        return;
                                    }
                                    Err(error) => {
                                        let _ = close_socket(socket, error.close_code()).await;
                                        return;
                                    }
                                }
                            }
                            Err(coop_cloud::RealtimeError::MessageTooLarge) => { let _ = close_socket(socket, 1009).await; return; }
                            Err(_) => { let _ = close_socket(socket, 1008).await; return; }
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        if !rate.admit() || send_message(socket, Message::Pong(payload)).await.is_err() { let _ = close_socket(socket, 1008).await; return; }
                    }
                    Ok(Message::Pong(_)) => {
                        if !rate.admit() { let _ = close_socket(socket, 1008).await; return; }
                    }
                    Ok(Message::Binary(_)) => {
                        if !rate.admit() { let _ = close_socket(socket, 1008).await; return; }
                        let _ = close_socket(socket, 1003).await;
                        return;
                    }
                    Ok(Message::Close(_)) => return,
                    Err(error) => {
                        let _ = close_socket(socket, websocket_error_close_code(&error)).await;
                        return;
                    }
                }
            }
        }
    }
}

async fn filter_group_end_frames(
    app: &Phase2App,
    session: StableRuntimeSession,
    frames: &mut Vec<ServerRealtimeFrameV1>,
) -> Result<(), u16> {
    if !frames
        .iter()
        .any(|frame| matches!(frame, ServerRealtimeFrameV1::GroupEnded(_)))
    {
        return Ok(());
    }
    let store = app.store.clone();
    let current = app
        .realtime
        .run_presence(move || super::sessions::pending_group_end_for_session(&store, session))
        .await
        .map_err(|error| error.close_code())?
        .map_err(|_| 1011_u16)?;
    frames.retain(|frame| match frame {
        ServerRealtimeFrameV1::GroupEnded(event) => Some(event.group_id) == current,
        _ => true,
    });
    Ok(())
}

async fn filter_group_start_frames(
    app: &Phase2App,
    session: StableRuntimeSession,
    frames: &mut Vec<ServerRealtimeFrameV1>,
) -> Result<(), u16> {
    if !frames
        .iter()
        .any(|frame| matches!(frame, ServerRealtimeFrameV1::GroupStarted(_)))
    {
        return Ok(());
    }
    let store = app.store.clone();
    let current = app
        .realtime
        .run_presence(move || {
            let _gate = store.lock_runtime_transition_gate();
            let now = store.now();
            store.read_transaction(|state| {
                let Some(lease) = state.leases.get(&session.character_id) else {
                    return Ok::<_, Phase2Error>(None);
                };
                if lease.released
                    || lease.contract.expires_at.value() <= now
                    || lease.contract.stable_runtime_session() != session
                {
                    return Ok::<_, Phase2Error>(None);
                }
                Ok::<_, Phase2Error>(
                    state
                        .active_group_by_member
                        .get(&session.character_id)
                        .copied(),
                )
            })
        })
        .await
        .map_err(|error| error.close_code())?
        .map_err(|_| 1011_u16)?;
    frames.retain(|frame| match frame {
        ServerRealtimeFrameV1::GroupStarted(event) => {
            event.session_epoch == session.session_epoch && Some(event.group_id) == current
        }
        _ => true,
    });
    Ok(())
}

fn server_frame(event: &PresenceOutboundV1) -> ServerRealtimeFrameV1 {
    match event {
        PresenceOutboundV1::Spawn(value) => {
            ServerRealtimeFrameV1::remote_player_spawn(value.clone())
        }
        PresenceOutboundV1::Update(value) => {
            ServerRealtimeFrameV1::remote_player_update(value.clone())
        }
        PresenceOutboundV1::Despawn(value) => {
            ServerRealtimeFrameV1::remote_player_despawn(value.clone())
        }
        PresenceOutboundV1::Interaction(value) => ServerRealtimeFrameV1::remote_interaction(*value),
        PresenceOutboundV1::Companion(value) => ServerRealtimeFrameV1::remote_companion(*value),
        PresenceOutboundV1::Signal(value) => ServerRealtimeFrameV1::remote_social_signal(*value),
    }
}

fn progress_event_id(frame: &ServerRealtimeFrameV1) -> Option<u64> {
    match frame {
        ServerRealtimeFrameV1::ProgressEvent(event) => Some(event.event_id),
        _ => None,
    }
}

/// Merges durable replay with the per-connection queue. Registering before
/// replay closes the loss window; event IDs remove the duplicate caused when
/// an event is persisted and queued between the two reads.
fn merge_progress_frames(
    replay: Vec<ServerRealtimeFrameV1>,
    queued: Vec<ServerRealtimeFrameV1>,
) -> Vec<ServerRealtimeFrameV1> {
    let mut by_id = BTreeMap::new();
    let mut notices = Vec::new();
    let mut seen_group_ends = HashSet::new();
    for frame in replay.into_iter().chain(queued) {
        if let Some(event_id) = progress_event_id(&frame) {
            by_id.entry(event_id).or_insert(frame);
        } else if let ServerRealtimeFrameV1::GroupEnded(event) = &frame {
            if seen_group_ends.insert(event.group_id) {
                notices.push(frame);
            }
        } else {
            notices.push(frame);
        }
    }
    let mut frames: Vec<_> = by_id.into_values().collect();
    frames.extend(notices);
    frames
}

#[derive(Default)]
struct InboundRate {
    frames: VecDeque<Instant>,
}

impl InboundRate {
    fn admit(&mut self) -> bool {
        let now = Instant::now();
        while self
            .frames
            .front()
            .is_some_and(|at| now.duration_since(*at) >= INBOUND_WINDOW)
        {
            self.frames.pop_front();
        }
        if self.frames.len() >= MAX_INBOUND_FRAMES {
            return false;
        }
        self.frames.push_back(now);
        true
    }
}

async fn next_before(
    socket: &mut WebSocket,
    deadline: TokioInstant,
) -> Option<Result<Message, axum::Error>> {
    time::timeout_at(deadline, socket.recv())
        .await
        .ok()
        .flatten()
}

async fn send_frames(socket: &mut WebSocket, frames: &[ServerRealtimeFrameV1]) -> Result<(), ()> {
    let deadline = TokioInstant::now() + OUTBOUND_DEADLINE;
    for frame in frames {
        let bytes = coop_cloud::encode_server_realtime_frame(frame).map_err(|_| ())?;
        if bytes.len() > MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES
            || bytes.len() > MAX_WRITE_BUFFER_BYTES
        {
            return Err(());
        }
        let text = String::from_utf8(bytes).map_err(|_| ())?;
        time::timeout_at(deadline, socket.send(Message::text(text)))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
    }
    Ok(())
}

async fn send_message(socket: &mut WebSocket, message: Message) -> Result<(), ()> {
    time::timeout(OUTBOUND_DEADLINE, socket.send(message))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

async fn close_socket(socket: &mut WebSocket, code: u16) -> Result<(), ()> {
    let frame = axum::extract::ws::CloseFrame {
        code,
        reason: "".into(),
    };
    time::timeout(CLOSE_DEADLINE, socket.send(Message::Close(Some(frame))))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

fn websocket_error_close_code(error: &axum::Error) -> u16 {
    // Axum intentionally exposes its WebSocket receive failures only through
    // its boxed error wrapper.  Preserve the observable assembled-message
    // policy close without depending on tungstenite as a production crate
    // dependency; all other transport/parser failures remain generic policy
    // closes.
    if error.to_string().contains("Message too long:") {
        1009
    } else {
        1008
    }
}

async fn close_presence_error(
    socket: &mut WebSocket,
    error: PresenceServiceError,
) -> Result<(), ()> {
    close_socket(socket, presence_close_code(&error)).await
}

/// Maps a soft interaction failure to its directed not-accepted reason.
/// Returns `None` for genuine protocol, authentication, lease, capacity,
/// or internal violations, which keep their close behavior.
fn interaction_rejection(
    error: &PresenceServiceError,
) -> Option<coop_cloud::InteractionRejectReason> {
    match error {
        PresenceServiceError::InteractionTargetUnavailable => {
            Some(coop_cloud::InteractionRejectReason::TargetUnavailable)
        }
        PresenceServiceError::InteractionObservationMismatch => {
            Some(coop_cloud::InteractionRejectReason::ObservationMismatch)
        }
        PresenceServiceError::InteractionOutOfRange => {
            Some(coop_cloud::InteractionRejectReason::OutOfRange)
        }
        _ => None,
    }
}

/// Folds one interaction validation result into the shape the connected
/// session loop already matches on. Soft rejections (`stale observation`,
/// `target unavailable`, `out of range`) are answered with a directed
/// `INTERACTION_REJECTED` reply and folded into `Ok(Ok(()))` so the
/// connection stays open and the rejected interaction is dropped. Genuine
/// violations pass through untouched so the caller still closes. A failed
/// reply send collapses to a worker failure close.
async fn classify_interaction_result(
    socket: &mut WebSocket,
    target: coop_protocol::PresenceHandle,
    result: Result<Result<(), PresenceServiceError>, PresenceWorkError>,
) -> Result<Result<(), PresenceServiceError>, PresenceWorkError> {
    match result {
        Ok(Ok(_)) => Ok(Ok(())),
        Ok(Err(error)) => {
            let Some(reason) = interaction_rejection(&error) else {
                return Ok(Err(error));
            };
            let frame = ServerRealtimeFrameV1::interaction_rejected(
                coop_cloud::InteractionRejectedV1::new(target, reason),
            );
            if send_frames(socket, std::slice::from_ref(&frame))
                .await
                .is_err()
            {
                return Err(PresenceWorkError::Failed);
            }
            Ok(Ok(()))
        }
        Err(error) => Err(error),
    }
}

fn presence_close_code(error: &PresenceServiceError) -> u16 {
    match error {
        PresenceServiceError::GlobalCapacity | PresenceServiceError::PartitionCapacity => 1013,
        PresenceServiceError::Authentication
        | PresenceServiceError::LeaseInactive
        | PresenceServiceError::LeaseFenceMismatch
        | PresenceServiceError::IncompatibleBuild
        | PresenceServiceError::UnsupportedZone
        | PresenceServiceError::InvalidState
        | PresenceServiceError::NotConnected
        | PresenceServiceError::InteractionTargetUnavailable
        | PresenceServiceError::InteractionObservationMismatch
        | PresenceServiceError::InteractionOutOfRange => 1008,
        PresenceServiceError::HandleAllocation | PresenceServiceError::Internal => 1011,
    }
}

// Keep this helper referenced by tests and documentation so the client cap is
// not accidentally changed without a compiler-visible use.
#[allow(dead_code)]
const fn client_frame_cap() -> usize {
    MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES
}

#[cfg(test)]
mod tests {
    use super::super::{ArgonPasswordEngine, Phase2Config};
    use super::*;
    use coop_cloud::{
        AcquireLeaseRequest, CharacterId, ClientInstanceId, IdempotencyKey, InvitationCode,
        LoginRequest, Password, ProgressFeedEventV1, RealtimeTicket, RegisterRequest, SessionEpoch,
        SessionId, UnixTimestampMillis, UserId, Username,
    };
    use coop_protocol::{
        AnimationId, AvatarId, CanonicalUsername, DespawnReason, Direction, LocalPresenceStateV1,
        MovementMode, PlayerState, PresenceHandle, PresencePoseV1, ProgressKindV1,
        ProgressObservationV1, RemotePlayerDespawnV1, RemotePlayerSpawnV1, RemotePlayerUpdateV1,
        WorldLocation,
    };
    use http_body_util::BodyExt;
    use std::sync::{Arc, Barrier};
    use tower::ServiceExt;

    fn three_world_catalog() -> (Vec<u8>, Vec<coop_cloud::RuntimeBuildIdentity>) {
        let builds = ["main", "cormoria", "third"]
            .into_iter()
            .map(|name| {
                coop_cloud::RuntimeBuildIdentity::new(
                    coop_cloud::GameBuildId::new(format!("test-{name}-v2")).unwrap(),
                    coop_cloud::Sha256Digest::of_bytes(name.as_bytes()),
                    coop_cloud::MgbaVersion::new("0.10.5").unwrap(),
                    coop_cloud::BridgeAbiVersion::new(1).unwrap(),
                    coop_cloud::ProtocolVersion::new(1).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let worlds = [1_u16, 2, 7]
            .into_iter()
            .zip(&builds)
            .map(|(world_id, build)| serde_json::json!({"world_id": world_id, "build": build}))
            .collect::<Vec<_>>();
        (
            serde_json::to_vec(&serde_json::json!({"schema_version": 1, "worlds": worlds}))
                .unwrap(),
            builds,
        )
    }

    #[test]
    fn missing_release_catalog_cannot_admit_a_runtime() {
        let (app, headers, request) = ticket_fixture();
        let mut config = (*app.store.config).clone();
        config.release_catalog = None;
        config.fixture_auto_bind = false;
        let config = config.with_adapters(app.store.repository.clone(), app.store.objects.clone());
        let unconfigured = Phase2App::new(config).unwrap();
        assert_eq!(
            mint_ticket(&unconfigured, &headers, &request),
            Err(Phase2Error::Internal)
        );
    }

    #[test]
    fn active_main_world_mints_only_main_and_prepare_rejects_spoofed_world() {
        let (catalog_bytes, builds) = three_world_catalog();
        for (index, world_number) in [1_u16].into_iter().enumerate() {
            let config = Phase2Config::local(
                vec![0x55; 32],
                coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
                "world-binding-test",
            )
            .unwrap()
            .with_release_catalog_bytes(
                &catalog_bytes,
                coop_cloud::Sha256Digest::of_bytes(&catalog_bytes),
            )
            .unwrap()
            .with_password_engine(Arc::new(ArgonPasswordEngine::new(8_192, 1, 1).unwrap()));
            let app = Phase2App::new(config).unwrap();
            app.add_invitation("world-binding-invite").unwrap();
            let registration = app
                .register(
                    RegisterRequest::new(
                        "WorldBindingUser",
                        Password::new("world-binding-password").unwrap(),
                        InvitationCode::new("world-binding-invite").unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            let login = app
                .login(
                    LoginRequest::new(
                        "worldbindinguser",
                        Password::new("world-binding-password").unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            let headers = HeaderMap::from_iter([(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", login.access_token.expose_secret())
                    .parse()
                    .unwrap(),
            )]);
            let actor = auth::actor_from_headers(&app.store, &headers).unwrap();
            let client = ClientInstanceId::new(uuid::Uuid::from_u128(101)).unwrap();
            let lease = app
                .acquire(
                    actor,
                    AcquireLeaseRequest::new(
                        registration.character_id,
                        client,
                        IdempotencyKey::new(uuid::Uuid::from_u128(102)).unwrap(),
                    ),
                )
                .unwrap();
            let world = coop_protocol::RomWorldId::new(world_number).unwrap();
            let sav = coop_cloud::SnapshotFile::from_bytes(
                coop_cloud::ArtifactIdentity::CharacterSav,
                &vec![0xff; coop_save::FLASH_IMAGE_SIZE],
            )
            .unwrap();
            let pending = coop_cloud::SnapshotFile::from_bytes(
                coop_cloud::ArtifactIdentity::PendingCommits,
                b"{}",
            )
            .unwrap();
            let prepare = |claimed_world| {
                coop_cloud::SnapshotPrepareRequest::new(
                    coop_cloud::SnapshotId::new(uuid::Uuid::new_v4()).unwrap(),
                    claimed_world,
                    coop_cloud::SnapshotPrepareFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        IdempotencyKey::new(uuid::Uuid::new_v4()).unwrap(),
                    ),
                    vec![sav.clone(), pending.clone()],
                    pending.sha256,
                )
                .unwrap()
            };
            assert_eq!(
                app.prepare(actor, prepare(world)),
                Err(Phase2Error::Authentication),
                "world {world_number} requires an authenticated runtime first"
            );
            for alternate in &builds[1..] {
                assert_eq!(
                    mint_ticket(
                        &app,
                        &headers,
                        &MintRealtimeTicketRequest::v1(RuntimeLeaseFence::new(
                            lease.stable_runtime_session(),
                            alternate.clone(),
                        )),
                    ),
                    Err(Phase2Error::Authentication),
                    "a fresh character must begin in Main"
                );
            }
            assert_eq!(
                app.store
                    .inspect_state(|state| {
                        (
                            state.leases[&actor.character_id].runtime_binding.is_none(),
                            state.realtime_tickets.len(),
                        )
                    })
                    .unwrap(),
                (true, 0)
            );
            // A later fresh lease is unbound, but its first ticket must still
            // follow the last durable snapshot rather than the client's ROM.
            let snapshot_id = coop_cloud::SnapshotId::new(uuid::Uuid::new_v4()).unwrap();
            let pending_digest = pending.sha256;
            let snapshot = coop_cloud::SnapshotRecord::new(
                snapshot_id,
                world,
                coop_cloud::SnapshotFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.session_epoch,
                ),
                coop_cloud::Revision::initial(),
                coop_cloud::Revision::initial().next().unwrap(),
                vec![sav.clone(), pending.clone()],
                pending_digest,
                None,
                Store::unix_timestamp(app.store.now()).unwrap(),
            )
            .unwrap();
            app.store
                .write_transaction(|state| {
                    state.snapshots.insert(snapshot_id, snapshot);
                    state
                        .characters
                        .get_mut(&actor.character_id)
                        .unwrap()
                        .active_snapshot = Some(snapshot_id);
                    Ok::<_, Phase2Error>(())
                })
                .unwrap();
            for alternate in &builds[1..] {
                assert_eq!(
                    mint_ticket(
                        &app,
                        &headers,
                        &MintRealtimeTicketRequest::v1(RuntimeLeaseFence::new(
                            lease.stable_runtime_session(),
                            alternate.clone(),
                        )),
                    ),
                    Err(Phase2Error::Authentication),
                    "a saved Main character cannot bind an alternate world"
                );
            }
            assert_eq!(
                app.store
                    .inspect_state(|state| {
                        (
                            state.leases[&actor.character_id].runtime_binding.is_none(),
                            state.realtime_tickets.len(),
                        )
                    })
                    .unwrap(),
                (true, 0)
            );
            let runtime =
                RuntimeLeaseFence::new(lease.stable_runtime_session(), builds[index].clone());
            mint_ticket(&app, &headers, &MintRealtimeTicketRequest::v1(runtime)).unwrap();
            let alternate = builds[(index + 1) % builds.len()].clone();
            assert_eq!(
                mint_ticket(
                    &app,
                    &headers,
                    &MintRealtimeTicketRequest::v1(RuntimeLeaseFence::new(
                        lease.stable_runtime_session(),
                        alternate,
                    )),
                ),
                Err(Phase2Error::Authentication),
                "an active lease cannot silently switch ROM worlds"
            );
            let wrong =
                coop_protocol::RomWorldId::new(if world_number == 1 { 2 } else { 1 }).unwrap();
            assert_eq!(
                app.prepare(actor, prepare(wrong)),
                Err(Phase2Error::Authentication),
                "world {world_number} rejects a client world spoof"
            );
            assert!(app.prepare(actor, prepare(world)).is_ok());
            let bound = app
                .store
                .inspect_state(|state| {
                    state.leases[&actor.character_id]
                        .runtime_binding
                        .as_ref()
                        .unwrap()
                        .world_id
                })
                .unwrap();
            assert_eq!(bound, world);
        }
    }

    fn ticket_fixture() -> (Phase2App, HeaderMap, MintRealtimeTicketRequest) {
        let app = Phase2App::test();
        app.add_invitation("realtime-unit-invite").unwrap();
        let registration = app
            .register(
                RegisterRequest::new(
                    "RealtimeUnitUser",
                    Password::new("realtime-unit-password").unwrap(),
                    InvitationCode::new("realtime-unit-invite").unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let login = app
            .login(
                LoginRequest::new(
                    "realtimeunituser",
                    Password::new("realtime-unit-password").unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let actor = auth::actor_from_headers(
            &app.store,
            &HeaderMap::from_iter([(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", login.access_token.expose_secret())
                    .parse()
                    .unwrap(),
            )]),
        )
        .unwrap();
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(
                    registration.character_id,
                    ClientInstanceId::new(uuid::Uuid::from_u128(101)).unwrap(),
                    IdempotencyKey::new(uuid::Uuid::from_u128(102)).unwrap(),
                ),
            )
            .unwrap();
        let runtime = RuntimeLeaseFence::new(
            lease.stable_runtime_session(),
            super::super::saves::current_runtime_build_identity().unwrap(),
        );
        let request = MintRealtimeTicketRequest::v1(runtime);
        let headers = HeaderMap::from_iter([(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", login.access_token.expose_secret())
                .parse()
                .unwrap(),
        )]);
        (app, headers, request)
    }

    fn local_state(x: i16, y: i16, source_sequence: u32) -> LocalPresenceStateV1 {
        let location = WorldLocation::new(coop_protocol::RegionId::Hoenn, 0, 9, x, y).unwrap();
        let pose = PresencePoseV1::new(
            location,
            0,
            Direction::South,
            1,
            1,
            MovementMode::Idle,
            AnimationId::Idle,
            AvatarId::Brendan,
            PlayerState::Overworld,
        )
        .unwrap();
        LocalPresenceStateV1::new(pose, source_sequence).unwrap()
    }

    fn presence_fixture(
        app: &Phase2App,
        invitation: &str,
        username: &str,
        id_base: u128,
    ) -> (AuthenticatedActor, RuntimeLeaseFence) {
        app.add_invitation(invitation).unwrap();
        let registration = app
            .register(
                RegisterRequest::new(
                    username,
                    Password::new("realtime-presence-password").unwrap(),
                    InvitationCode::new(invitation).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let login = app
            .login(
                LoginRequest::new(
                    username,
                    Password::new("realtime-presence-password").unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let headers = HeaderMap::from_iter([(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", login.access_token.expose_secret())
                .parse()
                .unwrap(),
        )]);
        let actor = auth::actor_from_headers(&app.store, &headers).unwrap();
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(
                    registration.character_id,
                    ClientInstanceId::new(uuid::Uuid::from_u128(id_base)).unwrap(),
                    IdempotencyKey::new(uuid::Uuid::from_u128(id_base + 1)).unwrap(),
                ),
            )
            .unwrap();
        (
            actor,
            RuntimeLeaseFence::new(
                lease.stable_runtime_session(),
                super::super::saves::current_runtime_build_identity().unwrap(),
            ),
        )
    }

    fn synthetic_ticket_record(
        index: usize,
        now: u64,
    ) -> ((UserId, StableRuntimeSession), RealtimeTicketRecord) {
        let index = u128::try_from(index).unwrap();
        let user_id = UserId::new(uuid::Uuid::from_u128(0x1000 + index)).unwrap();
        let character_id = CharacterId::new(uuid::Uuid::from_u128(0x2000 + index)).unwrap();
        let session = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(0x3000 + index)).unwrap(),
            character_id,
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(0x4000 + index)).unwrap(),
        );
        let runtime = RuntimeLeaseFence::new(
            session,
            super::super::saves::current_runtime_build_identity().unwrap(),
        );
        (
            (user_id, session),
            RealtimeTicketRecord {
                user_id,
                character_id,
                session,
                runtime,
                expires_at: now + 1,
            },
        )
    }

    #[test]
    fn rate_limit_is_exactly_64_per_window() {
        let mut rate = InboundRate::default();
        assert!((0..MAX_INBOUND_FRAMES).all(|_| rate.admit()));
        assert!(!rate.admit());
    }

    #[test]
    fn ticket_mint_rate_limit_is_per_user_and_cleans_expired_buckets() {
        let state = RealtimeTransportState::new();
        let first_user = UserId::new(uuid::Uuid::from_u128(0x501)).unwrap();
        let second_user = UserId::new(uuid::Uuid::from_u128(0x502)).unwrap();
        for _ in 0..MAX_TICKET_MINTS_PER_WINDOW {
            assert!(state.admit_ticket_mint(first_user).is_ok());
        }
        assert_eq!(state.admit_ticket_mint(first_user), Err(Phase2Error::Busy));
        assert!(state.admit_ticket_mint(second_user).is_ok());

        let expired_user = UserId::new(uuid::Uuid::from_u128(0x503)).unwrap();
        state.ticket_mints.lock().unwrap().insert(
            expired_user,
            VecDeque::from([Instant::now() - TICKET_MINT_WINDOW - Duration::from_secs(1)]),
        );
        assert!(state.admit_ticket_mint(second_user).is_ok());
        assert!(
            !state
                .ticket_mints
                .lock()
                .unwrap()
                .contains_key(&expired_user)
        );
    }

    #[test]
    fn admission_releases_on_drop() {
        let state = Arc::new(RealtimeTransportState::new());
        {
            let mut reservation = state.reserve_global().unwrap();
            reservation
                .bind(
                    coop_cloud::UserId::new(uuid::Uuid::from_u128(1)).unwrap(),
                    StableRuntimeSession::new(
                        coop_cloud::SessionId::new(uuid::Uuid::from_u128(2)).unwrap(),
                        coop_cloud::CharacterId::new(uuid::Uuid::from_u128(3)).unwrap(),
                        coop_cloud::SessionEpoch::new(1).unwrap(),
                        coop_cloud::ClientInstanceId::new(uuid::Uuid::from_u128(4)).unwrap(),
                    ),
                )
                .unwrap();
        }
        let admission = state.admission.lock().unwrap();
        assert_eq!(admission.global, 0);
        assert!(admission.by_runtime.is_empty());
    }

    #[test]
    fn admission_enforces_two_sockets_per_runtime_and_global_busy_mapping() {
        let state = Arc::new(RealtimeTransportState::new());
        let user = UserId::new(uuid::Uuid::from_u128(11)).unwrap();
        let character = CharacterId::new(uuid::Uuid::from_u128(12)).unwrap();
        let session = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(13)).unwrap(),
            character,
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(14)).unwrap(),
        );
        let mut first = state.reserve_global().unwrap();
        let mut second = state.reserve_global().unwrap();
        let mut third = state.reserve_global().unwrap();
        first.bind(user, session).unwrap();
        second.bind(user, session).unwrap();
        assert_eq!(third.bind(user, session), Err(Phase2Error::Busy));
        drop(third);
        let mut admission = state.admission.lock().unwrap();
        admission.global = MAX_REALTIME_GLOBAL_SOCKETS;
        drop(admission);
        assert_eq!(state.reserve_global().err(), Some(Phase2Error::Busy));
        drop(first);
        drop(second);
    }

    #[test]
    fn mint_replaces_the_prior_runtime_capability_and_stores_only_fingerprint() {
        let (app, headers, request) = ticket_fixture();
        let first = mint_ticket(&app, &headers, &request).unwrap();
        let first_ticket = first.ticket().expose_secret().to_owned();
        let first_fingerprint = *first.ticket().fingerprint().as_bytes();
        let second = mint_ticket(&app, &headers, &request).unwrap();
        let second_fingerprint = *second.ticket().fingerprint().as_bytes();
        assert_ne!(first_fingerprint, second_fingerprint);
        assert_ne!(first_ticket, second.ticket().expose_secret());
        assert_eq!(
            app.store
                .inspect_state(|state| state.realtime_tickets.len())
                .unwrap(),
            1
        );
        assert!(
            !app.store
                .inspect_state(|state| state.realtime_tickets.contains_key(&first_fingerprint))
                .unwrap()
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.realtime_by_runtime.len())
                .unwrap(),
            1
        );
    }

    #[test]
    fn revoked_refresh_family_cannot_redeem_a_realtime_ticket() {
        let (app, headers, request) = ticket_fixture();
        let minted = mint_ticket(&app, &headers, &request).unwrap();
        let ticket = RealtimeTicket::parse(minted.ticket().expose_secret()).unwrap();
        let fingerprint = *ticket.fingerprint().as_bytes();
        let family_id = app
            .store
            .inspect_state(|state| state.realtime_ticket_families.get(&fingerprint).copied())
            .unwrap()
            .expect("new tickets retain their family binding");
        app.store
            .write_transaction(|state| {
                state.families.get_mut(&family_id).unwrap().revoked = true;
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert!(matches!(
            preflight_ticket(&app.store, &ticket),
            Err(Phase2Error::Authentication)
        ));
    }

    #[test]
    fn malformed_or_duplicate_ticket_headers_collapse_to_authentication_failed() {
        let ticket = RealtimeTicket::from_bytes([1; 32]).unwrap();
        let value = format!("Bearer {}", ticket.expose_secret());
        let mut duplicate = HeaderMap::new();
        duplicate.append(axum::http::header::AUTHORIZATION, value.parse().unwrap());
        duplicate.append(axum::http::header::AUTHORIZATION, value.parse().unwrap());
        let mut oversized = HeaderMap::new();
        oversized.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", "a".repeat(257)).parse().unwrap(),
        );
        let cases = [
            HeaderMap::new(),
            HeaderMap::from_iter([(
                axum::http::header::AUTHORIZATION,
                "Basic abc".parse().unwrap(),
            )]),
            HeaderMap::from_iter([(axum::http::header::AUTHORIZATION, "Bearer".parse().unwrap())]),
            HeaderMap::from_iter([(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {} ", ticket.expose_secret())
                    .parse()
                    .unwrap(),
            )]),
            duplicate,
            oversized,
        ];
        for headers in cases {
            assert_eq!(
                ticket_from_headers(&headers),
                Err(Phase2Error::Authentication)
            );
        }
        let valid =
            HeaderMap::from_iter([(axum::http::header::AUTHORIZATION, value.parse().unwrap())]);
        assert_eq!(
            ticket_from_headers(&valid).unwrap().expose_secret(),
            ticket.expose_secret()
        );
    }

    #[test]
    fn candidate_selection_is_bounded_and_mutation_free_on_collision_or_capacity() {
        let now = 10_000;
        let ticket = RealtimeTicket::from_bytes([1; 32]).unwrap();
        let fingerprint = *ticket.fingerprint().as_bytes();
        let candidates = (0..MAX_TICKET_CANDIDATES)
            .map(|_| (RealtimeTicket::from_bytes([1; 32]).unwrap(), fingerprint))
            .collect::<Vec<_>>();

        let mut collision_state = StorageState::default();
        let (key, record) = synthetic_ticket_record(0, now);
        collision_state.realtime_tickets.insert(fingerprint, record);
        collision_state.realtime_by_runtime.insert(key, fingerprint);
        let before = (
            collision_state.realtime_tickets.len(),
            collision_state.realtime_by_runtime.len(),
        );
        assert_eq!(
            select_realtime_candidate(&collision_state, &candidates, key, now),
            Err(Phase2Error::Internal)
        );
        assert_eq!(
            (
                collision_state.realtime_tickets.len(),
                collision_state.realtime_by_runtime.len()
            ),
            before
        );

        let mut full_state = StorageState::default();
        for index in 0..MAX_REALTIME_TICKETS_GLOBAL {
            let mut occupied = [0_u8; 32];
            occupied[..8].copy_from_slice(&u64::try_from(index).unwrap().to_le_bytes());
            let (occupied_key, record) = synthetic_ticket_record(index, now);
            full_state.realtime_tickets.insert(occupied, record);
            full_state
                .realtime_by_runtime
                .insert(occupied_key, occupied);
        }
        let replacement_key = full_state
            .realtime_by_runtime
            .keys()
            .next()
            .copied()
            .unwrap();
        assert_eq!(
            select_realtime_candidate(&full_state, &[(ticket, fingerprint)], replacement_key, now,),
            Ok(0),
            "an active replacement is permitted at the global cap"
        );
        let new_key = synthetic_ticket_record(MAX_REALTIME_TICKETS_GLOBAL + 1, now).0;
        assert_eq!(
            select_realtime_candidate(
                &full_state,
                &[(RealtimeTicket::from_bytes([2; 32]).unwrap(), [9; 32])],
                new_key,
                now,
            ),
            Err(Phase2Error::Busy)
        );

        let mut expired_state = StorageState::default();
        let (expired_key, mut expired_record) = synthetic_ticket_record(0, now);
        expired_record.expires_at = now;
        expired_state
            .realtime_tickets
            .insert(fingerprint, expired_record);
        expired_state
            .realtime_by_runtime
            .insert(expired_key, fingerprint);
        assert_eq!(
            select_realtime_candidate(
                &expired_state,
                &[(RealtimeTicket::from_bytes([1; 32]).unwrap(), fingerprint)],
                expired_key,
                now,
            ),
            Ok(0),
            "expiry is exact at now >= expires_at and permits safe reuse"
        );
    }

    #[test]
    fn corrupt_ticket_indexes_fail_closed_without_replacing_the_existing_ticket() {
        let (app, headers, request) = ticket_fixture();
        let minted = mint_ticket(&app, &headers, &request).unwrap();
        let before = app
            .store
            .inspect_state(|state| {
                (
                    state.realtime_tickets.len(),
                    state.realtime_by_runtime.len(),
                )
            })
            .unwrap();
        app.store
            .write_transaction(|state| {
                state.realtime_by_runtime.clear();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            mint_ticket(&app, &headers, &request),
            Err(Phase2Error::Internal)
        );
        assert_eq!(
            app.store
                .inspect_state(|state| (
                    state.realtime_tickets.len(),
                    state.realtime_by_runtime.len()
                ))
                .unwrap(),
            (before.0, 0)
        );
        assert_ne!(minted.ticket().expose_secret(), "");
    }

    #[test]
    fn concurrent_mint_serializes_to_one_runtime_capability() {
        let (app, headers, request) = ticket_fixture();
        let app = Arc::new(app);
        let barrier = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let app = Arc::clone(&app);
            let headers = headers.clone();
            let request = request.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                mint_ticket(&app, &headers, &request)
            }));
        }
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 8);
        assert_eq!(
            app.store
                .inspect_state(|state| state.realtime_by_runtime.len())
                .unwrap(),
            1
        );
    }

    #[test]
    fn concurrent_redeem_is_one_use_linearizable() {
        let (app, headers, request) = ticket_fixture();
        let minted = mint_ticket(&app, &headers, &request).unwrap();
        let ticket = minted.ticket().expose_secret().to_owned();
        let app = Arc::new(app);
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let app = Arc::clone(&app);
            let ticket = ticket.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                let ticket = RealtimeTicket::parse(&ticket).unwrap();
                let redemption = preflight_ticket(&app.store, &ticket).unwrap();
                barrier.wait();
                consume_ticket(&app.store, &ticket, &redemption)
            }));
        }
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| **outcome == Err(Phase2Error::Authentication))
                .count(),
            1
        );
    }

    #[test]
    fn every_presence_failure_has_a_collapsed_close_code() {
        let capacity = [
            PresenceServiceError::GlobalCapacity,
            PresenceServiceError::PartitionCapacity,
        ];
        assert!(
            capacity
                .iter()
                .all(|error| super::presence_close_code(error) == 1013)
        );
        assert_eq!(
            super::presence_close_code(&PresenceServiceError::HandleAllocation),
            1011
        );
        assert_eq!(
            super::presence_close_code(&PresenceServiceError::Internal),
            1011
        );
        let policy = [
            PresenceServiceError::Authentication,
            PresenceServiceError::LeaseInactive,
            PresenceServiceError::LeaseFenceMismatch,
            PresenceServiceError::IncompatibleBuild,
            PresenceServiceError::UnsupportedZone,
            PresenceServiceError::InvalidState,
            PresenceServiceError::NotConnected,
            PresenceServiceError::InteractionTargetUnavailable,
            PresenceServiceError::InteractionObservationMismatch,
            PresenceServiceError::InteractionOutOfRange,
        ];
        assert!(
            policy
                .iter()
                .all(|error| super::presence_close_code(error) == 1008)
        );
    }

    #[test]
    fn submission_and_interaction_validation_keep_internal_failures_distinct() {
        for error in [
            PresenceServiceError::InvalidState,
            PresenceServiceError::NotConnected,
            PresenceServiceError::InteractionTargetUnavailable,
            PresenceServiceError::InteractionObservationMismatch,
            PresenceServiceError::InteractionOutOfRange,
        ] {
            assert_eq!(super::presence_close_code(&error), 1008);
        }
        assert_eq!(
            super::presence_close_code(&PresenceServiceError::Internal),
            1011
        );
    }

    #[test]
    fn server_frame_preserves_fifo_lifecycle_variants() {
        let state = local_state(1, 1, 1);
        let handle = PresenceHandle::new(7).unwrap();
        let username = CanonicalUsername::new("remoteuser").unwrap();
        let spawn = RemotePlayerSpawnV1::new(handle, 1, state.clone(), username).unwrap();
        let update = RemotePlayerUpdateV1::new(handle, 2, local_state(2, 1, 2)).unwrap();
        let despawn = RemotePlayerDespawnV1::new(handle, 3, DespawnReason::Disconnected).unwrap();
        let frames = [
            server_frame(&PresenceOutboundV1::Spawn(spawn)),
            server_frame(&PresenceOutboundV1::Update(update)),
            server_frame(&PresenceOutboundV1::Despawn(despawn)),
        ];
        assert!(matches!(
            frames[0],
            ServerRealtimeFrameV1::RemotePlayerSpawn(_)
        ));
        assert!(matches!(
            frames[1],
            ServerRealtimeFrameV1::RemotePlayerUpdate(_)
        ));
        assert!(matches!(
            frames[2],
            ServerRealtimeFrameV1::RemotePlayerDespawn(_)
        ));
    }

    #[test]
    fn two_user_presence_lifecycle_is_symmetric_and_stale_capabilities_are_harmless() {
        let app = Phase2App::test();
        let (first_actor, first_runtime) =
            presence_fixture(&app, "presence-first-invite", "PresenceFirst", 0x10_000);
        let (second_actor, second_runtime) =
            presence_fixture(&app, "presence-second-invite", "PresenceSecond", 0x20_000);
        let presence = app.presence();
        let (first, first_initial) = presence
            .connect_and_drain(first_actor, first_runtime.clone(), local_state(1, 1, 1))
            .unwrap();
        assert!(first_initial.events.is_empty());
        let (second, second_initial) = presence
            .connect_and_drain(second_actor, second_runtime, local_state(1, 1, 1))
            .unwrap();
        assert!(matches!(
            second_initial.events.as_slice(),
            [PresenceOutboundV1::Spawn(spawn)] if spawn.handle() == first.handle()
        ));
        assert!(matches!(
            presence.drain(first).unwrap().events.as_slice(),
            [PresenceOutboundV1::Spawn(spawn)] if spawn.handle() == second.handle()
        ));

        let replacement = presence
            .connect(first_actor, first_runtime, local_state(1, 1, 1))
            .unwrap();
        assert_ne!(replacement, first);
        assert_eq!(
            presence.drain(first),
            Err(PresenceServiceError::NotConnected)
        );
        let replacement_events = presence.drain(second).unwrap().events;
        assert!(matches!(
            replacement_events.as_slice(),
            [PresenceOutboundV1::Despawn(despawn), PresenceOutboundV1::Spawn(spawn)]
                if despawn.handle() == first.handle() && spawn.handle() == replacement.handle()
        ));

        presence
            .submit_state(replacement, local_state(2, 1, 2))
            .unwrap();
        presence
            .tick_at(app.store.now() + PRESENCE_TICK_MS)
            .unwrap();
        assert!(matches!(
            presence.drain(second).unwrap().events.as_slice(),
            [PresenceOutboundV1::Update(update)] if update.handle() == replacement.handle()
        ));

        presence.disconnect(replacement).unwrap();
        assert!(matches!(
            presence.drain(second).unwrap().events.as_slice(),
            [PresenceOutboundV1::Despawn(despawn)] if despawn.handle() == replacement.handle()
        ));
        presence.disconnect(first).unwrap();
    }

    #[test]
    fn explicit_disconnect_cleans_up_the_presence_capability() {
        let app = Phase2App::test();
        let (actor, runtime) =
            presence_fixture(&app, "presence-guard-invite", "PresenceGuard", 0x30_000);
        let presence = app.presence();
        let (connection, _) = presence
            .connect_and_drain(actor, runtime, local_state(1, 1, 1))
            .unwrap();
        presence.disconnect(connection).unwrap();
        assert_eq!(
            presence.drain(connection),
            Err(PresenceServiceError::NotConnected)
        );
        presence.disconnect(connection).unwrap();
    }

    #[tokio::test]
    async fn router_collapses_internal_and_capacity_errors_with_no_store() {
        let (app, headers, request) = ticket_fixture();
        mint_ticket(&app, &headers, &request).unwrap();
        app.store
            .write_transaction(|state| {
                state.realtime_by_runtime.clear();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        let body = serde_json::to_vec(&request).unwrap();
        let http_request = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/realtime/tickets")
            .header("content-type", "application/json")
            .header(
                axum::http::header::AUTHORIZATION,
                headers
                    .get(axum::http::header::AUTHORIZATION)
                    .unwrap()
                    .clone(),
            )
            .body(Body::from(body))
            .unwrap();
        let response = app.router().oneshot(http_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
            "internal_error"
        );

        let (app, headers, request) = ticket_fixture();
        let minted = mint_ticket(&app, &headers, &request).unwrap();
        app.realtime.admission.lock().unwrap().global = MAX_REALTIME_GLOBAL_SOCKETS;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let router = app.router();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let mut websocket =
            tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
                format!("ws://{address}/v1/realtime"),
            )
            .unwrap();
        websocket.headers_mut().insert(
            "authorization",
            format!("Bearer {}", minted.ticket().expose_secret())
                .parse()
                .unwrap(),
        );
        match tokio_tungstenite::connect_async(websocket).await {
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(
                    response.headers().get(header::CACHE_CONTROL),
                    Some(&HeaderValue::from_static("no-store"))
                );
            }
            other => panic!("capacity unexpectedly upgraded: {other:?}"),
        }
        server.abort();
    }

    #[test]
    fn soft_interaction_rejections_map_to_replies_not_close() {
        assert_eq!(
            interaction_rejection(&PresenceServiceError::InteractionTargetUnavailable),
            Some(coop_cloud::InteractionRejectReason::TargetUnavailable)
        );
        assert_eq!(
            interaction_rejection(&PresenceServiceError::InteractionObservationMismatch),
            Some(coop_cloud::InteractionRejectReason::ObservationMismatch)
        );
        assert_eq!(
            interaction_rejection(&PresenceServiceError::InteractionOutOfRange),
            Some(coop_cloud::InteractionRejectReason::OutOfRange)
        );
        let target = coop_protocol::PresenceHandle::new(0x0123_4567_89ab_cdef).unwrap();
        for (error, reason) in [
            (
                PresenceServiceError::InteractionTargetUnavailable,
                coop_cloud::InteractionRejectReason::TargetUnavailable,
            ),
            (
                PresenceServiceError::InteractionObservationMismatch,
                coop_cloud::InteractionRejectReason::ObservationMismatch,
            ),
            (
                PresenceServiceError::InteractionOutOfRange,
                coop_cloud::InteractionRejectReason::OutOfRange,
            ),
        ] {
            let frame = ServerRealtimeFrameV1::interaction_rejected(
                coop_cloud::InteractionRejectedV1::new(
                    target,
                    interaction_rejection(&error).unwrap(),
                ),
            );
            assert_eq!(
                frame,
                ServerRealtimeFrameV1::interaction_rejected(
                    coop_cloud::InteractionRejectedV1::new(target, reason)
                )
            );
            let bytes = coop_cloud::encode_server_realtime_frame(&frame).unwrap();
            assert_eq!(
                coop_cloud::decode_server_realtime_frame(&bytes).unwrap(),
                frame
            );
        }
    }

    #[test]
    fn interaction_violations_still_close() {
        let violations = [
            PresenceServiceError::Authentication,
            PresenceServiceError::LeaseInactive,
            PresenceServiceError::LeaseFenceMismatch,
            PresenceServiceError::IncompatibleBuild,
            PresenceServiceError::UnsupportedZone,
            PresenceServiceError::InvalidState,
            PresenceServiceError::NotConnected,
            PresenceServiceError::GlobalCapacity,
            PresenceServiceError::PartitionCapacity,
            PresenceServiceError::HandleAllocation,
            PresenceServiceError::Internal,
        ];
        assert!(
            violations
                .iter()
                .all(|error| interaction_rejection(error).is_none())
        );
        assert!(
            [
                PresenceServiceError::Authentication,
                PresenceServiceError::LeaseInactive,
                PresenceServiceError::LeaseFenceMismatch,
                PresenceServiceError::IncompatibleBuild,
                PresenceServiceError::UnsupportedZone,
                PresenceServiceError::InvalidState,
                PresenceServiceError::NotConnected,
            ]
            .iter()
            .all(|error| super::presence_close_code(error) == 1008)
        );
        assert_eq!(
            super::presence_close_code(&PresenceServiceError::Internal),
            1011
        );
    }

    #[tokio::test]
    async fn rejected_interaction_replies_over_the_socket_and_keeps_it_open() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as ClientMessage;

        let (app, headers, request) = ticket_fixture();
        let minted = mint_ticket(&app, &headers, &request).unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let router = app.router();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let mut websocket =
            tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
                format!("ws://{address}/v1/realtime"),
            )
            .unwrap();
        websocket.headers_mut().insert(
            "authorization",
            format!("Bearer {}", minted.ticket().expose_secret())
                .parse()
                .unwrap(),
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(websocket).await.unwrap();
        let send = |frame: ClientRealtimeFrameV1| {
            ClientMessage::Text(
                String::from_utf8(coop_cloud::encode_client_realtime_frame(&frame).unwrap())
                    .unwrap()
                    .into(),
            )
        };
        socket
            .send(send(ClientRealtimeFrameV1::player_state(local_state(
                1, 1, 1,
            ))))
            .await
            .unwrap();

        // No partner owns this handle, so validation fails softly.
        let absent = coop_protocol::PresenceHandle::new(0x0123_4567_89ab_cdef).unwrap();
        let interaction = || {
            ClientRealtimeFrameV1::interact_remote_player(
                coop_protocol::PresenceInteractionV1::new(absent, 1, 1, 1, 2).unwrap(),
            )
        };
        let expected =
            ServerRealtimeFrameV1::interaction_rejected(coop_cloud::InteractionRejectedV1::new(
                absent,
                coop_cloud::InteractionRejectReason::TargetUnavailable,
            ));
        for (round, state) in [(0_u32, None), (1, Some(local_state(2, 1, 2)))] {
            if let Some(state) = state {
                // The session is still live: it accepts ordinary state updates.
                socket
                    .send(send(ClientRealtimeFrameV1::player_state(state)))
                    .await
                    .unwrap();
            }
            socket.send(send(interaction())).await.unwrap();
            let rejected = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match socket.next().await {
                        Some(Ok(ClientMessage::Text(text))) => {
                            let frame =
                                coop_cloud::decode_server_realtime_frame(text.as_bytes()).unwrap();
                            if frame == expected {
                                return frame;
                            }
                        }
                        Some(Ok(ClientMessage::Close(close))) => {
                            panic!("round {round}: socket closed after soft rejection: {close:?}")
                        }
                        Some(Ok(_)) => {}
                        other => panic!("round {round}: socket ended: {other:?}"),
                    }
                }
            })
            .await
            .expect("interaction reply");
            assert_eq!(rejected, expected);
        }
        socket.close(None).await.unwrap();
        server.abort();
    }

    #[test]
    fn idle_backstop_bounds_silent_sockets() {
        let now = Instant::now();
        assert!(!is_idle_expired(now, now));
        assert!(!is_idle_expired(
            now,
            now + IDLE_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(is_idle_expired(now, now + IDLE_TIMEOUT));
        assert!(is_idle_expired(
            now,
            now + IDLE_TIMEOUT + Duration::from_secs(1)
        ));
    }

    #[test]
    fn solo_progress_observation_is_ignored_without_closing_presence() {
        let (app, headers, request) = ticket_fixture();
        let actor = auth::actor_from_headers(&app.store, &headers).unwrap();
        let observation = ProgressObservationV1 {
            kind: ProgressKindV1::BadgeEarned,
            region_id: coop_protocol::RegionId::Hoenn,
            subject_id: 0,
            session_epoch: request.runtime().session.session_epoch.value(),
            source_sequence: 1,
        };
        let accepted =
            progress_feed::accept(&app.store, actor, request.runtime().clone(), observation)
                .unwrap();
        assert!(accepted.is_none());
        let count = app
            .store
            .inspect_state(|state| state.group_progress_feeds.len())
            .unwrap();
        assert_eq!(count, 0);
    }

    fn queued_progress_frame(event_id: u64) -> ServerRealtimeFrameV1 {
        ServerRealtimeFrameV1::progress_event(ProgressFeedEventV1 {
            event_id,
            source_character_id: CharacterId::new(uuid::Uuid::from_u128(0x700)).unwrap(),
            source_username: Username::new("partner").unwrap(),
            kind: ProgressKindV1::BadgeEarned,
            region_id: coop_protocol::RegionId::Hoenn,
            subject_id: 0,
            source_sequence: event_id as u32,
            occurred_at: UnixTimestampMillis::new(event_id),
        })
    }

    #[test]
    fn overlapping_progress_sockets_each_receive_their_own_queue() {
        let state = RealtimeTransportState::new();
        let recipient = CharacterId::new(uuid::Uuid::from_u128(0x701)).unwrap();
        let session = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(0x702)).unwrap(),
            recipient,
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(0x703)).unwrap(),
        );
        let first = state.register_progress_recipient(session);
        let second = state.register_progress_recipient(session);
        let first_frame = queued_progress_frame(1);
        state.queue_progress(recipient, first_frame.clone());
        assert_eq!(state.drain_progress(first), vec![first_frame.clone()]);
        assert_eq!(state.drain_progress(second), vec![first_frame]);

        let second_frame = queued_progress_frame(2);
        state.unregister_progress_recipient(recipient, first);
        state.queue_progress(recipient, second_frame.clone());
        assert!(state.drain_progress(first).is_empty());
        assert_eq!(state.drain_progress(second), vec![second_frame]);
        state.unregister_progress_recipient(recipient, second);
    }

    #[test]
    fn group_ended_goes_only_to_current_partner_session() {
        let state = RealtimeTransportState::new();
        let recipient = CharacterId::new(uuid::Uuid::from_u128(0x710)).unwrap();
        let current = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(0x711)).unwrap(),
            recipient,
            SessionEpoch::new(2).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(0x712)).unwrap(),
        );
        let stale = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(0x711)).unwrap(),
            recipient,
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(0x712)).unwrap(),
        );
        let active_socket = state.register_progress_recipient(current);
        let stale_socket = state.register_progress_recipient(stale);
        let group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x713)).unwrap();
        state.queue_group_ended(current, group_id);
        assert_eq!(
            state.drain_progress(active_socket),
            vec![ServerRealtimeFrameV1::group_ended(group_id)]
        );
        assert!(state.drain_progress(stale_socket).is_empty());
        state.unregister_progress_recipient(recipient, active_socket);
        state.unregister_progress_recipient(recipient, stale_socket);
    }

    #[test]
    fn group_started_fanout_is_fenced_to_current_runtime() {
        let state = RealtimeTransportState::new();
        let recipient = CharacterId::new(uuid::Uuid::from_u128(0x714)).unwrap();
        let current = StableRuntimeSession::new(
            SessionId::new(uuid::Uuid::from_u128(0x715)).unwrap(),
            recipient,
            SessionEpoch::new(2).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(0x716)).unwrap(),
        );
        let stale = StableRuntimeSession::new(
            current.session_id,
            recipient,
            SessionEpoch::new(1).unwrap(),
            current.client_instance_id,
        );
        let active_socket = state.register_progress_recipient(current);
        let stale_socket = state.register_progress_recipient(stale);
        let group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x717)).unwrap();
        state.queue_group_started(current, group_id);
        assert_eq!(
            state.drain_progress(active_socket),
            vec![ServerRealtimeFrameV1::group_started(
                group_id,
                current.session_epoch
            )]
        );
        assert!(state.drain_progress(stale_socket).is_empty());
    }

    #[tokio::test]
    async fn invitation_and_pairing_commits_notify_both_member_sessions() {
        for mode in 0..3 {
            let app = Phase2App::test();
            let (first, first_runtime) =
                presence_fixture(&app, "group-start-a", "GroupStartA", 0x800);
            let (second, second_runtime) =
                presence_fixture(&app, "group-start-b", "GroupStartB", 0x810);
            let first_socket = app
                .realtime
                .register_progress_recipient(first_runtime.session);
            let second_socket = app
                .realtime
                .register_progress_recipient(second_runtime.session);
            let (first_fence, second_fence) = app
                .store
                .inspect_state(|state| {
                    (
                        state.leases[&first.character_id].contract.fence(),
                        state.leases[&second.character_id].contract.fence(),
                    )
                })
                .unwrap();
            let group_id = if mode == 2 {
                let code = app
                    .create_pairing_code(
                        first,
                        coop_cloud::CreatePairingCodeRequest::new(first_fence),
                    )
                    .unwrap();
                app.redeem_pairing_code(
                    second,
                    coop_cloud::RedeemPairingCodeRequest::new(second_fence, code.code),
                )
                .unwrap()
                .group
                .group_id
            } else {
                let invitation = app
                    .create_group_invitation(
                        first,
                        coop_cloud::CreateGroupInvitationRequest::new(
                            first_fence,
                            second.character_id,
                            IdempotencyKey::new(uuid::Uuid::from_u128(0x818)).unwrap(),
                        ),
                    )
                    .unwrap();
                if mode == 1 {
                    let response = super::super::online::action(
                        &app,
                        second,
                        &coop_cloud::OnlineActionRequest {
                            api_version: coop_cloud::ApiVersion::V1,
                            fence: second_fence,
                            idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(0x819))
                                .unwrap(),
                            action: coop_cloud::OnlineAction::Accept {
                                invitation_id: invitation.invitation_id,
                            },
                        },
                    )
                    .unwrap();
                    let coop_cloud::OnlineActionResponse::Accepted { group } = response else {
                        panic!("expected accepted group")
                    };
                    group.group_id
                } else {
                    app.accept_group_invitation(
                        second,
                        invitation.invitation_id,
                        coop_cloud::AcceptGroupInvitationRequest::new(
                            second_fence,
                            IdempotencyKey::new(uuid::Uuid::from_u128(0x819)).unwrap(),
                        ),
                    )
                    .unwrap()
                    .group
                    .group_id
                }
            };
            assert_eq!(
                app.realtime.drain_progress(first_socket),
                vec![ServerRealtimeFrameV1::group_started(
                    group_id,
                    first_runtime.session.session_epoch
                )]
            );
            assert_eq!(
                app.realtime.drain_progress(second_socket),
                vec![ServerRealtimeFrameV1::group_started(
                    group_id,
                    second_runtime.session.session_epoch
                )]
            );
            let mut stale = vec![ServerRealtimeFrameV1::group_started(
                group_id,
                first_runtime.session.session_epoch,
            )];
            app.store
                .write_transaction(|state| {
                    state.active_group_by_member.remove(&first.character_id);
                    Ok::<_, Phase2Error>(())
                })
                .unwrap();
            filter_group_start_frames(&app, first_runtime.session, &mut stale)
                .await
                .unwrap();
            assert!(
                stale.is_empty(),
                "a queued start must not revive an ended group"
            );
        }
    }

    #[test]
    fn progress_replay_merge_closes_read_registration_race_without_duplicates() {
        let first = queued_progress_frame(1);
        let second = queued_progress_frame(2);
        let third = queued_progress_frame(3);
        let merged = merge_progress_frames(
            vec![first.clone(), second.clone()],
            vec![second, third.clone()],
        );
        assert_eq!(merged, vec![first, queued_progress_frame(2), third]);
    }

    #[test]
    fn replay_merge_retains_group_ended_during_registration() {
        let notice = ServerRealtimeFrameV1::group_ended(
            coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x720)).unwrap(),
        );
        assert_eq!(
            merge_progress_frames(vec![queued_progress_frame(1)], vec![notice.clone()]),
            vec![queued_progress_frame(1), notice]
        );
    }

    #[tokio::test]
    async fn expired_group_notice_survives_partner_reconnect_within_grace() {
        let app = Phase2App::test();
        let (expired, _) = presence_fixture(&app, "group-end-offline-a", "GroupEndOfflineA", 0x731);
        let (partner, runtime) =
            presence_fixture(&app, "group-end-offline-b", "GroupEndOfflineB", 0x733);
        let group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x735)).unwrap();
        let now = app.store.now();
        let old_fence = app
            .store
            .write_transaction(|state| {
                state.groups.insert(
                    group_id,
                    super::super::storage::GroupRecord {
                        group: coop_cloud::Group::new(expired.character_id, partner.character_id)
                            .unwrap(),
                        zone: coop_protocol::WorldZone::new(
                            coop_protocol::RegionId::Hoenn,
                            "LITTLEROOT_TOWN",
                            0,
                        )
                        .unwrap(),
                        status: super::super::storage::GroupStatus::Active,
                        zone_revision: 0,
                    },
                );
                state
                    .active_group_by_member
                    .insert(expired.character_id, group_id);
                state
                    .active_group_by_member
                    .insert(partner.character_id, group_id);
                state
                    .leases
                    .get_mut(&expired.character_id)
                    .unwrap()
                    .grace_until = now - 1;
                let lease = state.leases.get_mut(&partner.character_id).unwrap();
                let fence = lease.contract.fence();
                lease.contract.expires_at =
                    super::super::storage::Store::unix_timestamp(now - 1).unwrap();
                assert!(lease.grace_until > now);
                Ok::<_, Phase2Error>(fence)
            })
            .unwrap();

        let ended = super::super::sessions::expire_groups(&app.store).unwrap();
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].partner_session, Some(runtime.session));
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap(),
            None,
        );

        let rotated = super::super::sessions::reconnect(
            &app.store,
            partner,
            &coop_cloud::ReconnectLeaseRequest::new(
                old_fence,
                coop_cloud::IdempotencyKey::new(uuid::Uuid::from_u128(0x736)).unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(
                &app.store,
                rotated.stable_runtime_session(),
            )
            .unwrap(),
            Some(group_id),
        );
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap(),
            None,
        );
        let replaced = app
            .acquire(
                partner,
                coop_cloud::AcquireLeaseRequest::new(
                    partner.character_id,
                    runtime.session.client_instance_id,
                    coop_cloud::IdempotencyKey::new(uuid::Uuid::from_u128(0x737)).unwrap(),
                )
                .replacing_same_client(),
            )
            .unwrap();
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(
                &app.store,
                replaced.stable_runtime_session(),
            )
            .unwrap(),
            Some(group_id),
        );
    }

    #[tokio::test]
    async fn expired_group_replays_to_partner_after_socket_gap_only_on_same_lease() {
        let app = Phase2App::test();
        let (expired, _) = presence_fixture(&app, "group-end-expired", "GroupEndExpired", 0x721);
        let (partner, runtime) =
            presence_fixture(&app, "group-end-partner", "GroupEndPartner", 0x722);
        let group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x723)).unwrap();
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state.groups.insert(
                    group_id,
                    super::super::storage::GroupRecord {
                        group: coop_cloud::Group::new(expired.character_id, partner.character_id)
                            .unwrap(),
                        zone: coop_protocol::WorldZone::new(
                            coop_protocol::RegionId::Hoenn,
                            "LITTLEROOT_TOWN",
                            0,
                        )
                        .unwrap(),
                        status: super::super::storage::GroupStatus::Active,
                        zone_revision: 0,
                    },
                );
                state
                    .active_group_by_member
                    .insert(expired.character_id, group_id);
                state
                    .active_group_by_member
                    .insert(partner.character_id, group_id);
                state
                    .leases
                    .get_mut(&expired.character_id)
                    .unwrap()
                    .grace_until = now.saturating_sub(1);
                Ok::<_, Phase2Error>(())
            })
            .unwrap();

        // No recipient is registered when the durable expiry commits.
        let ended = super::super::sessions::expire_groups(&app.store).unwrap();
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].partner_session, Some(runtime.session));
        app.realtime.queue_group_ended(runtime.session, group_id);
        let socket = app.realtime.register_progress_recipient(runtime.session);
        assert!(app.realtime.drain_progress(socket).is_empty());
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap(),
            Some(group_id)
        );
        let replay = ServerRealtimeFrameV1::group_ended(group_id);
        assert_eq!(
            merge_progress_frames(vec![replay.clone()], vec![replay.clone()]),
            vec![replay]
        );
        assert!(
            super::super::sessions::expire_groups(&app.store)
                .unwrap()
                .is_empty()
        );

        // Expiry after an empty first drain, but before the durable read,
        // appears in both the late queue and replay. Startup sends it once.
        let first_drain = app.realtime.drain_progress(socket);
        assert!(first_drain.is_empty());
        app.realtime.queue_group_ended(runtime.session, group_id);
        let durable =
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap()
                .map(ServerRealtimeFrameV1::group_ended)
                .into_iter()
                .collect();
        let late_drain = app.realtime.drain_progress(socket);
        assert_eq!(
            merge_progress_frames(durable, merge_progress_frames(first_drain, late_drain)),
            vec![ServerRealtimeFrameV1::group_ended(group_id)]
        );

        // A new active group suppresses both stored replay and a queued old
        // closure. The real formation paths also remove the durable record.
        let new_group_id = coop_cloud::GroupId::new(uuid::Uuid::from_u128(0x724)).unwrap();
        app.store
            .write_transaction(|state| {
                state
                    .active_group_by_member
                    .insert(partner.character_id, new_group_id);
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap(),
            None
        );
        let mut queued = vec![ServerRealtimeFrameV1::group_ended(group_id)];
        filter_group_end_frames(&app, runtime.session, &mut queued)
            .await
            .unwrap();
        assert!(queued.is_empty());
        app.store
            .write_transaction(|state| {
                state.group_end_notices.remove(&partner.character_id);
                state.active_group_by_member.remove(&partner.character_id);
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, runtime.session)
                .unwrap(),
            None
        );

        let wrong_session = StableRuntimeSession::new(
            runtime.session.session_id,
            partner.character_id,
            SessionEpoch::new(runtime.session.session_epoch.value() + 1).unwrap(),
            runtime.session.client_instance_id,
        );
        assert_eq!(
            super::super::sessions::pending_group_end_for_session(&app.store, wrong_session)
                .unwrap(),
            None
        );
        app.realtime
            .unregister_progress_recipient(partner.character_id, socket);
    }
    #[test]
    fn progress_publication_is_fenced_by_either_members_rom_stage() {
        for paired in [false, true] {
            for staged in 0..2 {
                let app = Phase2App::test();
                let (a, ra) = presence_fixture(&app, "progress-a", "ProgressA", 0x9000);
                let (b, rb) = presence_fixture(&app, "progress-b", "ProgressB", 0x9100);
                let actors = [a, b];
                let runtimes = [ra, rb];
                let group_id = coop_cloud::GroupId::new(uuid::Uuid::new_v4()).unwrap();
                app.store
                    .write_transaction(|state| {
                        let group = coop_cloud::Group::new(a.character_id, b.character_id).unwrap();
                        state.groups.insert(
                            group_id,
                            super::super::storage::GroupRecord {
                                group,
                                zone: state.characters[&a.character_id].state.world_zone.clone(),
                                status: super::super::storage::GroupStatus::Active,
                                zone_revision: 0,
                            },
                        );
                        for actor in actors {
                            state
                                .active_group_by_member
                                .insert(actor.character_id, group_id);
                        }
                        super::super::storage::test_stage_handoff(
                            state,
                            actors[staged].character_id,
                            paired,
                            app.store.now(),
                        );
                        Ok::<_, Phase2Error>(())
                    })
                    .unwrap();
                let before = app
                    .store
                    .inspect_state(|state| {
                        let mut out = Vec::new();
                        ciborium::into_writer(state, &mut out).unwrap();
                        out
                    })
                    .unwrap();
                for caller in 0..2 {
                    let observation = ProgressObservationV1 {
                        kind: ProgressKindV1::BadgeEarned,
                        region_id: coop_protocol::RegionId::Hoenn,
                        subject_id: 0,
                        session_epoch: runtimes[caller].session.session_epoch.value(),
                        source_sequence: 1,
                    };
                    assert!(matches!(
                        progress_feed::accept(
                            &app.store,
                            actors[caller],
                            runtimes[caller].clone(),
                            observation
                        ),
                        Err(Phase2Error::Conflict)
                    ));
                }
                let after = app
                    .store
                    .inspect_state(|state| {
                        let mut out = Vec::new();
                        ciborium::into_writer(state, &mut out).unwrap();
                        out
                    })
                    .unwrap();
                assert_eq!(before, after);
            }
        }
    }

    #[test]
    fn progress_requires_catalog_build_and_exact_active_world_binding() {
        let (app, headers, request) = ticket_fixture();
        let actor = auth::actor_from_headers(&app.store, &headers).unwrap();
        let observation = ProgressObservationV1 {
            kind: ProgressKindV1::BadgeEarned,
            region_id: coop_protocol::RegionId::Hoenn,
            subject_id: 0,
            session_epoch: request.runtime().session.session_epoch.value(),
            source_sequence: 1,
        };
        let mut wrong_build = request.runtime().clone();
        wrong_build.build.rom_sha256 = coop_cloud::Sha256Digest::of_bytes(b"wrong build");
        assert!(matches!(
            progress_feed::accept(&app.store, actor, wrong_build, observation),
            Err(Phase2Error::Authentication)
        ));
        app.store
            .write_transaction(|state| {
                state
                    .leases
                    .get_mut(&actor.character_id)
                    .unwrap()
                    .runtime_binding
                    .as_mut()
                    .unwrap()
                    .world_id = coop_protocol::RomWorldId::new(2).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert!(matches!(
            progress_feed::accept(&app.store, actor, request.runtime().clone(), observation),
            Err(Phase2Error::Authentication)
        ));
        assert!(
            app.store
                .inspect_state(|state| state.group_progress_feeds.is_empty())
                .unwrap()
        );
    }
}

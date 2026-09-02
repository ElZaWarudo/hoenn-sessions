//! Authenticated, bounded loopback realtime transport.
//!
//! Realtime tickets are short-lived capabilities.  The repository stores only
//! a domain-separated fingerprint; the plaintext ticket exists solely while a
//! mint or upgrade request is being processed.  The WebSocket adapter is
//! intentionally kept in this module so transport admission and presence
//! cleanup have one small, auditable boundary.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
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
    ClientRealtimeFrameV1, MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES,
    MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES, MintRealtimeTicketRequest, MintRealtimeTicketResponse,
    REALTIME_TICKET_ENTROPY_BYTES, REALTIME_TICKET_REQUEST_BODY_MAX_BYTES, RealtimeTicket,
    RuntimeLeaseFence, ServerRealtimeFrameV1, StableRuntimeSession,
};
use tokio::time::{self, Instant as TokioInstant};
use zeroize::Zeroizing;

use super::auth;
use super::presence::{
    PRESENCE_TICK_MS, PresenceConnection, PresenceOutboundV1, PresenceServiceError,
};
use super::storage::{
    MAX_REALTIME_TICKETS_GLOBAL, RealtimeTicketRecord, State as StorageState, Store,
};
use super::{AuthenticatedActor, Phase2App, Phase2Error};

const MAX_REALTIME_GLOBAL_SOCKETS: usize = 1_024;
const MAX_REALTIME_SOCKETS_PER_RUNTIME: usize = 2;
const MAX_TICKET_CANDIDATES: usize = 16;
const FIRST_FRAME_DEADLINE: Duration = Duration::from_millis(3_000);
const OUTBOUND_DEADLINE: Duration = Duration::from_millis(500);
const CLOSE_DEADLINE: Duration = Duration::from_millis(250);
const INBOUND_WINDOW: Duration = Duration::from_millis(1_000);
const MAX_INBOUND_FRAMES: usize = 64;
const MAX_WRITE_BUFFER_BYTES: usize = 4_096;

/// Process-local transport admission shared by all clones of one app.
pub(crate) struct RealtimeTransportState {
    admission: Mutex<AdmissionState>,
}

impl RealtimeTransportState {
    pub(crate) fn new() -> Self {
        Self {
            admission: Mutex::new(AdmissionState::default()),
        }
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
}

#[derive(Default)]
struct AdmissionState {
    global: usize,
    by_runtime: HashMap<(coop_cloud::UserId, StableRuntimeSession), usize>,
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
    let build = super::saves::current_runtime_build_identity()?;
    if runtime.build != build {
        return Err(Phase2Error::Authentication);
    }
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

    let _gate = app
        .store
        .runtime_transition_gate
        .lock()
        .map_err(|_| Phase2Error::Internal)?;
    let actor = auth::actor_from_headers(&app.store, headers)?;
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
        // The repository callback is not rollback-safe on error.  All
        // fallible validation is complete before this mutation-only suffix.
        for expired_fingerprint in expired {
            if let Some(record) = state.realtime_tickets.remove(&expired_fingerprint) {
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
    let build = super::saves::current_runtime_build_identity()?;
    let _gate = store
        .runtime_transition_gate
        .lock()
        .map_err(|_| Phase2Error::Internal)?;
    let now = store.now();
    store.read_transaction(|state| {
        validate_realtime_indexes(state)?;
        let record = state
            .realtime_tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?;
        if record.expires_at <= now {
            return Err(Phase2Error::Authentication);
        }
        let actor = AuthenticatedActor {
            user_id: record.user_id,
            character_id: record.character_id,
        };
        validate_runtime_state_in_state(state, actor, &record.runtime, now, &build)?;
        Ok(Redemption {
            actor,
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
    let build = super::saves::current_runtime_build_identity()?;
    let _gate = store
        .runtime_transition_gate
        .lock()
        .map_err(|_| Phase2Error::Internal)?;
    let now = store.now();
    store.write_transaction(|state| {
        validate_realtime_indexes(state)?;
        let record = state
            .realtime_tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?
            .clone();
        if record.expires_at <= now
            || record.user_id != redemption.actor.user_id
            || record.character_id != redemption.actor.character_id
            || record.runtime != redemption.runtime
        {
            return Err(Phase2Error::Authentication);
        }
        validate_runtime_state_in_state(state, redemption.actor, &record.runtime, now, &build)?;
        let key = (record.user_id, record.session);
        if state.realtime_by_runtime.get(&key).copied() != Some(fingerprint) {
            return Err(Phase2Error::Internal);
        }
        state.realtime_tickets.remove(&fingerprint);
        state.realtime_by_runtime.remove(&key);
        Ok(())
    })
}

struct PresenceGuard {
    service: super::presence::PresenceService,
    connection: PresenceConnection,
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        let _ = self.service.disconnect(self.connection);
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
    let mut rate = InboundRate::default();
    let first_deadline = TokioInstant::now() + FIRST_FRAME_DEADLINE;
    let initial = loop {
        let Some(message) = next_before(&mut socket, first_deadline).await else {
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
                    Ok(ClientRealtimeFrameV1::InteractRemotePlayer(_)) | Err(_) => {
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

    let (connection, initial_drain) =
        match app
            .presence
            .connect_and_drain(redemption.actor, redemption.runtime.clone(), initial)
        {
            Ok(value) => value,
            Err(error) => {
                let _ = close_presence_error(&mut socket, error).await;
                return;
            }
        };
    let _guard = PresenceGuard {
        service: app.presence(),
        connection,
    };
    let mut initial_frames = Vec::with_capacity(initial_drain.events.len() + 1);
    initial_frames.push(ServerRealtimeFrameV1::presence_ready(connection.handle()));
    initial_frames.extend(initial_drain.events.iter().map(server_frame));
    if send_frames(&mut socket, &initial_frames).await.is_err() {
        return;
    }

    let mut ticker = time::interval(Duration::from_millis(PRESENCE_TICK_MS));
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = ticker.tick() => {
                if app.presence.tick().is_err() {
                    let _ = close_socket(&mut socket, 1011).await;
                    return;
                }
                let drain = match app.presence.drain(connection) {
                    Ok(drain) => drain,
                    Err(error) => {
                        let _ = close_presence_error(&mut socket, error).await;
                        return;
                    }
                };
                let frames = drain.events.iter().map(server_frame).collect::<Vec<_>>();
                if send_frames(&mut socket, &frames).await.is_err() {
                    return;
                }
            }
            message = socket.recv() => {
                let Some(message) = message else { return; };
                match message {
                    Ok(Message::Text(text)) => {
                        if !rate.admit() { let _ = close_socket(&mut socket, 1008).await; return; }
                        match coop_cloud::decode_client_realtime_frame(text.as_bytes()) {
                            Ok(ClientRealtimeFrameV1::PlayerState(state)) => {
                                match app.presence.submit_state(connection, state) {
                                    Ok(super::presence::PresenceSubmitOutcome::DisconnectedUnsupportedTravel) => {
                                        let _ = close_socket(&mut socket, 1008).await;
                                        return;
                                    }
                                    Ok(_) => {},
                                    Err(error) => { let _ = close_presence_error(&mut socket, error).await; return; }
                                }
                            }
                            Ok(ClientRealtimeFrameV1::InteractRemotePlayer(interaction)) => {
                                if let Err(error) =
                                    app.presence.validate_interaction(connection, interaction)
                                {
                                    let _ = close_presence_error(&mut socket, error).await;
                                    return;
                                }
                            }
                            Err(coop_cloud::RealtimeError::MessageTooLarge) => { let _ = close_socket(&mut socket, 1009).await; return; }
                            Err(_) => { let _ = close_socket(&mut socket, 1008).await; return; }
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        if !rate.admit() || send_message(&mut socket, Message::Pong(payload)).await.is_err() { let _ = close_socket(&mut socket, 1008).await; return; }
                    }
                    Ok(Message::Pong(_)) => {
                        if !rate.admit() { let _ = close_socket(&mut socket, 1008).await; return; }
                    }
                    Ok(Message::Binary(_)) => {
                        if !rate.admit() { let _ = close_socket(&mut socket, 1008).await; return; }
                        let _ = close_socket(&mut socket, 1003).await;
                        return;
                    }
                    Ok(Message::Close(_)) => return,
                    Err(error) => {
                        let _ = close_socket(&mut socket, websocket_error_close_code(&error)).await;
                        return;
                    }
                }
            }
        }
    }
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
    }
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
    use super::*;
    use coop_cloud::{
        AcquireLeaseRequest, CharacterId, ClientInstanceId, IdempotencyKey, InvitationCode,
        LoginRequest, Password, RealtimeTicket, RegisterRequest, SessionEpoch, SessionId, UserId,
    };
    use coop_protocol::{
        AnimationId, AvatarId, CanonicalUsername, DespawnReason, Direction, LocalPresenceStateV1,
        MovementMode, PlayerState, PresenceHandle, PresencePoseV1, RemotePlayerDespawnV1,
        RemotePlayerSpawnV1, RemotePlayerUpdateV1, WorldLocation,
    };
    use http_body_util::BodyExt;
    use std::sync::{Arc, Barrier};
    use tower::ServiceExt;

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
    fn presence_guard_disconnects_once_and_cleans_up_on_drop() {
        let app = Phase2App::test();
        let (actor, runtime) =
            presence_fixture(&app, "presence-guard-invite", "PresenceGuard", 0x30_000);
        let presence = app.presence();
        let (connection, _) = presence
            .connect_and_drain(actor, runtime, local_state(1, 1, 1))
            .unwrap();
        {
            let _guard = PresenceGuard {
                service: presence.clone(),
                connection,
            };
        }
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
}

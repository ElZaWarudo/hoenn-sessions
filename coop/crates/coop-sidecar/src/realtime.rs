//! One-attempt, bounded cloud presence transport for the local sidecar.
//!
//! This module intentionally stops at a typed owner/driver seam.  It owns the
//! short-lived upgrade capability for exactly one connection, but never owns
//! bridge framing or writes to the ROM TCP socket.  The public owner is a
//! bounded command/event handle; the driver is consumed by [`run_realtime`].

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    ClientRealtimeFrameV1, MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES, REALTIME_TICKET_TTL_MS,
    RealtimeTicket, ServerRealtimeFrameV1, UnixTimestampMillis, decode_server_realtime_frame,
    encode_client_realtime_frame,
};
use coop_protocol::{
    DespawnReason, LocalPresenceStateV1, PlayerState, PresenceHandle, PresenceInteractionV1,
    RemotePlayerSpawnV1, RemotePlayerUpdateV1, sequence_is_newer,
};
use futures_util::{SinkExt, StreamExt};
use thiserror::Error;
use tokio::{
    sync::{mpsc, watch},
    time::{self, Instant},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, Request, header::AUTHORIZATION},
        protocol::WebSocketConfig,
    },
};
use url::Url;

/// Maximum UTF-8 endpoint text accepted by the sidecar core.
pub const MAX_REALTIME_ENDPOINT_BYTES: usize = 256;
/// Maximum number of non-self remote handles tracked by one owner.
pub const MAX_REMOTE_PLAYERS: usize = 4;
/// Maximum number of remote lifecycle records, including hidden tombstones.
///
/// Hidden handles remain tracked so a later visible respawn cannot reset its
/// per-handle sequence.  Keeping this separate from the visible-player bound
/// makes the tombstone policy explicit and keeps memory bounded.
const MAX_TRACKED_REMOTE_HANDLES: usize = MAX_REMOTE_PLAYERS * 2;
/// Bounded interaction queue capacity.
pub const MAX_INTERACTION_QUEUE: usize = 16;
/// Bounded owner event queue capacity.
pub const MAX_OWNER_EVENT_QUEUE: usize = 32;
/// Local state transmission cadence.
pub const PRESENCE_TICK: Duration = Duration::from_millis(100);
/// Maximum time allowed for a WebSocket upgrade.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Maximum time allowed for each outbound WebSocket write.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// Maximum time allowed for the server's readiness frame.
pub const READY_TIMEOUT: Duration = Duration::from_secs(3);

const MAX_WEBSOCKET_WRITE_BUFFER_BYTES: usize = 4096;

/// Stable construction and input errors.  No variant carries parser or
/// peer-controlled text.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeError {
    #[error("realtime endpoint is invalid")]
    InvalidEndpoint,
    #[error("realtime grant is invalid")]
    InvalidGrant,
    #[error("realtime grant has expired")]
    ExpiredGrant,
    #[error("realtime state is invalid")]
    InvalidState,
}

/// Stable terminal outcomes for one realtime attempt.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeOutcome {
    #[error("realtime owner stopped")]
    OwnerStopped,
    #[error("realtime grant expired")]
    Expired,
    #[error("realtime connection timed out")]
    ConnectTimeout,
    #[error("realtime readiness timed out")]
    ReadyTimeout,
    #[error("realtime write failed")]
    WriteFailed,
    #[error("realtime peer closed")]
    PeerClosed,
    #[error("realtime transport failed")]
    TransportFailed,
    #[error("realtime protocol violation")]
    ProtocolViolation,
    #[error("realtime remote capacity exceeded")]
    CapacityExceeded,
    #[error("realtime owner event queue is full")]
    OwnerBackpressure,
}

/// Errors returned by owner-side bounded commands.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeInputError {
    #[error("realtime owner is stopped")]
    Stopped,
    #[error("realtime owner is not ready")]
    NotReady,
    #[error("realtime interaction queue is full")]
    QueueFull,
    #[error("realtime driver is closed")]
    Closed,
}

/// A canonical, validated realtime WebSocket endpoint.
#[derive(Clone, Eq, PartialEq)]
pub struct RealtimeEndpoint {
    url: Url,
}

impl RealtimeEndpoint {
    /// Parses and validates a realtime endpoint.
    ///
    /// `ws` is deliberately restricted to literal loopback addresses and an
    /// explicit nonzero port.  `wss` accepts canonical DNS/IP hosts and either
    /// an explicit nonzero port or its standard TLS port.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeError::InvalidEndpoint`] when the URL is not a
    /// canonical, bounded `/v1/realtime` endpoint.
    pub fn new(value: impl AsRef<str>) -> Result<Self, RealtimeError> {
        let value = value.as_ref();
        if value.len() > MAX_REALTIME_ENDPOINT_BYTES || value.is_empty() {
            return Err(RealtimeError::InvalidEndpoint);
        }
        let url = Url::parse(value).map_err(|_| RealtimeError::InvalidEndpoint)?;
        if url.as_str() != value
            || url.path() != "/v1/realtime"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host_str().is_none()
        {
            return Err(RealtimeError::InvalidEndpoint);
        }

        match url.scheme() {
            "ws" => {
                let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
                if !loopback || url.port().is_none_or(|port| port == 0) {
                    return Err(RealtimeError::InvalidEndpoint);
                }
            }
            "wss" => {
                if url.port().is_some_and(|port| port == 0) || url.port_or_known_default().is_none()
                {
                    return Err(RealtimeError::InvalidEndpoint);
                }
                if url.host_str().is_some_and(|host| host.ends_with('.')) {
                    return Err(RealtimeError::InvalidEndpoint);
                }
            }
            _ => return Err(RealtimeError::InvalidEndpoint),
        }
        Ok(Self { url })
    }

    /// Alias emphasizing that endpoint text is parsed, not concatenated.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeError::InvalidEndpoint`] for invalid endpoint text.
    pub fn parse(value: &str) -> Result<Self, RealtimeError> {
        Self::new(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    #[must_use]
    pub fn is_secure(&self) -> bool {
        self.url.scheme() == "wss"
    }

    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.url.port_or_known_default()
    }
}

impl fmt::Debug for RealtimeEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RealtimeEndpoint")
            .field(&self.url)
            .finish()
    }
}

impl fmt::Display for RealtimeEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A one-use realtime capability and its validated endpoint.
///
/// The grant is intentionally non-`Clone`, non-serializable, and has no
/// ticket accessor.  Moving it into [`run_realtime`] consumes the capability.
pub struct RealtimeGrant {
    ticket: RealtimeTicket,
    endpoint: RealtimeEndpoint,
    expires_at: UnixTimestampMillis,
}

impl RealtimeGrant {
    /// Constructs a grant using the current wall clock for TTL validation.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeError::ExpiredGrant`] when the expiry is in the past
    /// or [`RealtimeError::InvalidGrant`] when it exceeds the ticket TTL.
    pub fn new(
        ticket: RealtimeTicket,
        endpoint: RealtimeEndpoint,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, RealtimeError> {
        Self::with_now(ticket, endpoint, expires_at, unix_now())
    }

    /// Constructs a grant against an explicit timestamp, useful for callers
    /// that already have an authenticated server clock.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeError::ExpiredGrant`] when the expiry is in the past
    /// or [`RealtimeError::InvalidGrant`] when it exceeds the ticket TTL.
    pub fn with_now(
        ticket: RealtimeTicket,
        endpoint: RealtimeEndpoint,
        expires_at: UnixTimestampMillis,
        now: UnixTimestampMillis,
    ) -> Result<Self, RealtimeError> {
        let now = now.value();
        let expiry = expires_at.value();
        if expiry == 0 || expiry <= now || expiry.saturating_sub(now) > REALTIME_TICKET_TTL_MS {
            return Err(if expiry != 0 && expiry <= now {
                RealtimeError::ExpiredGrant
            } else {
                RealtimeError::InvalidGrant
            });
        }
        Ok(Self {
            ticket,
            endpoint,
            expires_at,
        })
    }

    /// Alias for callers handling a minted response's expiry explicitly.
    ///
    /// # Errors
    ///
    /// Returns the same validation errors as [`Self::new`].
    pub fn from_parts(
        ticket: RealtimeTicket,
        endpoint: RealtimeEndpoint,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, RealtimeError> {
        Self::new(ticket, endpoint, expires_at)
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixTimestampMillis {
        self.expires_at
    }

    #[must_use]
    pub fn endpoint(&self) -> &RealtimeEndpoint {
        &self.endpoint
    }
}

impl fmt::Debug for RealtimeGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RealtimeGrant")
            .field("endpoint", &self.endpoint)
            .field("expires_at", &self.expires_at)
            .field("ticket", &"[REDACTED]")
            .finish()
    }
}

/// Typed events delivered to the owner, with no transport details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RealtimeOwnerEvent {
    Ready(coop_cloud::PresenceReadyV1),
    Spawn(RemotePlayerSpawnV1),
    Update(RemotePlayerUpdateV1),
    Despawn(coop_protocol::RemotePlayerDespawnV1),
}

impl RealtimeOwnerEvent {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ready(_) => "PRESENCE_READY",
            Self::Spawn(_) => "REMOTE_PLAYER_SPAWN",
            Self::Update(_) => "REMOTE_PLAYER_UPDATE",
            Self::Despawn(_) => "REMOTE_PLAYER_DESPAWN",
        }
    }
}

/// Bounded commands and events owned by the local sidecar integration.
pub struct RealtimeOwner {
    state_tx: watch::Sender<LocalPresenceStateV1>,
    interaction_tx: mpsc::Sender<PresenceInteractionV1>,
    stop_tx: watch::Sender<bool>,
    event_rx: mpsc::Receiver<RealtimeOwnerEvent>,
    ready: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

impl RealtimeOwner {
    /// Publishes the latest local state.  The driver sends at most one latest
    /// value per 100 ms tick.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeInputError::Stopped`] or
    /// [`RealtimeInputError::Closed`] when the driver is no longer running.
    pub fn update_state(&self, state: LocalPresenceStateV1) -> Result<(), RealtimeInputError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(RealtimeInputError::Stopped);
        }
        self.state_tx
            .send(state)
            .map_err(|_| RealtimeInputError::Closed)
    }

    /// Queues one interaction after readiness.  FIFO capacity is fixed at 16.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeInputError::NotReady`] before the server readiness
    /// event, or a bounded queue/owner state error otherwise.
    pub fn interact(&self, interaction: PresenceInteractionV1) -> Result<(), RealtimeInputError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(RealtimeInputError::Stopped);
        }
        if !self.ready.load(Ordering::Acquire) {
            return Err(RealtimeInputError::NotReady);
        }
        self.interaction_tx
            .try_send(interaction)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RealtimeInputError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => RealtimeInputError::Closed,
            })
    }

    /// Requests a bounded, graceful stop.  Repeated calls are harmless.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.stop_tx.send(true);
    }

    /// Receives the next typed owner event.
    pub async fn recv_event(&mut self) -> Option<RealtimeOwnerEvent> {
        self.event_rx.recv().await
    }

    /// Alias for stream-oriented integrations.
    pub async fn next_event(&mut self) -> Option<RealtimeOwnerEvent> {
        self.recv_event().await
    }
}

impl Drop for RealtimeOwner {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.stop_tx.send(true);
    }
}

/// Driver half consumed by [`run_realtime`].
pub struct RealtimeDriver {
    state_rx: watch::Receiver<LocalPresenceStateV1>,
    interaction_rx: mpsc::Receiver<PresenceInteractionV1>,
    stop_rx: watch::Receiver<bool>,
    event_tx: mpsc::Sender<RealtimeOwnerEvent>,
    ready: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

/// Creates the bounded owner/driver seam around one validated cached state.
///
/// # Errors
///
/// Returns [`RealtimeError::InvalidState`] when the cached presence state is
/// not valid for the V1 wire contract.
pub fn realtime_channel(
    initial_state: LocalPresenceStateV1,
) -> Result<(RealtimeOwner, RealtimeDriver), RealtimeError> {
    initial_state
        .validate()
        .map_err(|_| RealtimeError::InvalidState)?;
    let (state_tx, state_rx) = watch::channel(initial_state);
    let (interaction_tx, interaction_rx) = mpsc::channel(MAX_INTERACTION_QUEUE);
    let (event_tx, event_rx) = mpsc::channel(MAX_OWNER_EVENT_QUEUE);
    let (stop_tx, stop_rx) = watch::channel(false);
    let ready = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    Ok((
        RealtimeOwner {
            state_tx,
            interaction_tx,
            stop_tx,
            event_rx,
            ready: Arc::clone(&ready),
            stopped: Arc::clone(&stopped),
        },
        RealtimeDriver {
            state_rx,
            interaction_rx,
            stop_rx,
            event_tx,
            ready,
            stopped,
        },
    ))
}

/// Runs one authenticated realtime attempt.  The grant and driver are both
/// consumed; no retry or reconnect path exists in this function.
#[expect(
    clippy::too_many_lines,
    reason = "the bounded transport state machine is kept together"
)]
pub async fn run_realtime(grant: RealtimeGrant, mut driver: RealtimeDriver) -> RealtimeOutcome {
    if driver.stopped.load(Ordering::Acquire) {
        return RealtimeOutcome::OwnerStopped;
    }
    let Some(expiry) = expiry_instant(grant.expires_at) else {
        return RealtimeOutcome::Expired;
    };
    if expiry <= Instant::now() {
        return RealtimeOutcome::Expired;
    }

    let Ok(request) = request_for(&grant) else {
        return RealtimeOutcome::ProtocolViolation;
    };
    // The request owns the only bearer header needed by the upgrade.  Drop
    // the capability before any network await so the ticket is zeroized as
    // soon as request construction is complete.
    drop(grant);
    let connect_deadline = std::cmp::min(Instant::now() + CONNECT_TIMEOUT, expiry);
    let websocket_config = WebSocketConfig::default()
        .read_buffer_size(MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_WEBSOCKET_WRITE_BUFFER_BYTES)
        .max_message_size(Some(MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES))
        .max_frame_size(Some(MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES));
    let connect = time::timeout_at(
        connect_deadline,
        connect_async_with_config(request, Some(websocket_config), false),
    );
    let (mut socket, _) = match connect.await {
        Ok(Ok(connection)) => connection,
        Ok(Err(_)) => return RealtimeOutcome::TransportFailed,
        Err(_) => {
            return if expiry <= Instant::now() {
                RealtimeOutcome::Expired
            } else {
                RealtimeOutcome::ConnectTimeout
            };
        }
    };

    let cached_state = driver.state_rx.borrow().clone();
    if send_client_frame(
        &mut socket,
        ClientRealtimeFrameV1::player_state(cached_state),
    )
    .await
        != Ok(())
    {
        return RealtimeOutcome::WriteFailed;
    }

    // Once the authenticated upgrade succeeds, ticket expiry no longer
    // governs the session.  Readiness has its own fixed transport deadline.
    let ready_deadline = Instant::now() + READY_TIMEOUT;
    let self_handle = loop {
        tokio::select! {
            changed = driver.stop_rx.changed() => {
                let _ = changed;
                driver.stopped.store(true, Ordering::Release);
                return RealtimeOutcome::OwnerStopped;
            }
            message = socket.next() => {
                match receive_frame(message, &mut socket).await {
                    Receive::Ready(handle) => break handle,
                    Receive::PeerClosed => return RealtimeOutcome::PeerClosed,
                    Receive::PingFailure => return RealtimeOutcome::WriteFailed,
                    Receive::Protocol | Receive::Lifecycle(_) => {
                        return RealtimeOutcome::ProtocolViolation;
                    }
                    Receive::Transport => return RealtimeOutcome::TransportFailed,
                    Receive::Ignored => {}
                }
            }
            () = time::sleep_until(ready_deadline) => {
                return RealtimeOutcome::ReadyTimeout;
            }
        }
    };

    driver.ready.store(true, Ordering::Release);
    if send_event(
        &driver.event_tx,
        RealtimeOwnerEvent::Ready(coop_cloud::PresenceReadyV1::new(self_handle)),
    )
    .is_err()
    {
        driver.ready.store(false, Ordering::Release);
        return RealtimeOutcome::OwnerBackpressure;
    }

    let mut remotes = BTreeMap::new();
    let mut last_sent = driver.state_rx.borrow().clone();
    let mut tick = time::interval_at(Instant::now() + PRESENCE_TICK, PRESENCE_TICK);
    tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            changed = driver.stop_rx.changed() => {
                let _ = changed;
                driver.stopped.store(true, Ordering::Release);
                driver.ready.store(false, Ordering::Release);
                return RealtimeOutcome::OwnerStopped;
            }
            _ = tick.tick() => {
                let latest = driver.state_rx.borrow().clone();
                if latest != last_sent {
                    if send_client_frame(&mut socket, ClientRealtimeFrameV1::player_state(latest.clone())).await != Ok(()) {
                        driver.ready.store(false, Ordering::Release);
                        return RealtimeOutcome::WriteFailed;
                    }
                    last_sent = latest;
                }
            }
            interaction = driver.interaction_rx.recv() => {
                let Some(interaction) = interaction else {
                    driver.ready.store(false, Ordering::Release);
                    return RealtimeOutcome::OwnerStopped;
                };
                if send_client_frame(&mut socket, ClientRealtimeFrameV1::interact_remote_player(interaction)).await != Ok(()) {
                    driver.ready.store(false, Ordering::Release);
                    return RealtimeOutcome::WriteFailed;
                }
            }
            message = socket.next() => {
                match receive_frame(message, &mut socket).await {
                    Receive::Ready(_) | Receive::Protocol => {
                        driver.ready.store(false, Ordering::Release);
                        return RealtimeOutcome::ProtocolViolation;
                    }
                    Receive::Ignored => {}
                    Receive::Lifecycle(frame) => {
                        let outcome = apply_lifecycle(
                            &frame,
                            self_handle,
                            &mut remotes,
                            &driver.event_tx,
                        );
                        if let Err(outcome) = outcome {
                            driver.ready.store(false, Ordering::Release);
                            return outcome;
                        }
                    }
                    Receive::PeerClosed => {
                        driver.ready.store(false, Ordering::Release);
                        return RealtimeOutcome::PeerClosed;
                    }
                    Receive::PingFailure => {
                        driver.ready.store(false, Ordering::Release);
                        return RealtimeOutcome::WriteFailed;
                    }
                    Receive::Transport => {
                        driver.ready.store(false, Ordering::Release);
                        return RealtimeOutcome::TransportFailed;
                    }
                }
            }
        }
    }
}

fn request_for(grant: &RealtimeGrant) -> Result<Request<()>, ()> {
    let authorization = format!("Bearer {}", grant.ticket.expose_secret());
    let mut request = grant
        .endpoint
        .as_str()
        .into_client_request()
        .map_err(|_| ())?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&authorization).map_err(|_| ())?,
    );
    Ok(request)
}

async fn send_client_frame<S>(socket: &mut S, frame: ClientRealtimeFrameV1) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let bytes = encode_client_realtime_frame(&frame).map_err(|_| ())?;
    let text = String::from_utf8(bytes).map_err(|_| ())?;
    time::timeout(WRITE_TIMEOUT, socket.send(Message::Text(text.into())))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

enum Receive {
    Ready(PresenceHandle),
    Lifecycle(ServerRealtimeFrameV1),
    Ignored,
    PeerClosed,
    PingFailure,
    Protocol,
    Transport,
}

async fn receive_frame<S>(
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    socket: &mut S,
) -> Receive
where
    S: SinkExt<Message> + Unpin,
{
    let message = match message {
        Some(Ok(message)) => message,
        Some(Err(error)) => {
            return if matches!(
                error,
                tokio_tungstenite::tungstenite::Error::Capacity(_)
                    | tokio_tungstenite::tungstenite::Error::Protocol(_)
            ) {
                Receive::Protocol
            } else {
                Receive::Transport
            };
        }
        None => return Receive::PeerClosed,
    };
    match message {
        Message::Text(text) => {
            if text.len() > MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES {
                return Receive::Protocol;
            }
            let Ok(frame) = decode_server_realtime_frame(text.as_bytes()) else {
                return Receive::Protocol;
            };
            match frame {
                ServerRealtimeFrameV1::PresenceReady(readiness) => {
                    Receive::Ready(readiness.self_handle())
                }
                frame @ (ServerRealtimeFrameV1::RemotePlayerSpawn(_)
                | ServerRealtimeFrameV1::RemotePlayerUpdate(_)
                | ServerRealtimeFrameV1::RemotePlayerDespawn(_)) => Receive::Lifecycle(frame),
            }
        }
        Message::Binary(_) | Message::Frame(_) => Receive::Protocol,
        Message::Ping(payload) => {
            if payload.len() > MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES {
                return Receive::Protocol;
            }
            match time::timeout(WRITE_TIMEOUT, socket.send(Message::Pong(payload))).await {
                Ok(Ok(())) => Receive::Ignored,
                _ => Receive::PingFailure,
            }
        }
        Message::Pong(_) => Receive::Ignored,
        Message::Close(_) => Receive::PeerClosed,
    }
}

fn send_event(
    event_tx: &mpsc::Sender<RealtimeOwnerEvent>,
    event: RealtimeOwnerEvent,
) -> Result<(), ()> {
    event_tx.try_send(event).map_err(|_| ())
}

fn apply_lifecycle(
    frame: &ServerRealtimeFrameV1,
    self_handle: PresenceHandle,
    remotes: &mut BTreeMap<PresenceHandle, RemoteState>,
    event_tx: &mpsc::Sender<RealtimeOwnerEvent>,
) -> Result<(), RealtimeOutcome> {
    let (handle, sequence) = match &frame {
        ServerRealtimeFrameV1::RemotePlayerSpawn(spawn) => {
            (spawn.handle(), spawn.server_sequence())
        }
        ServerRealtimeFrameV1::RemotePlayerUpdate(update) => {
            (update.handle(), update.server_sequence())
        }
        ServerRealtimeFrameV1::RemotePlayerDespawn(despawn) => {
            (despawn.handle(), despawn.server_sequence())
        }
        ServerRealtimeFrameV1::PresenceReady(_) => return Err(RealtimeOutcome::ProtocolViolation),
    };
    if handle == self_handle {
        return Err(RealtimeOutcome::ProtocolViolation);
    }

    // Server sequences are scoped to a remote handle.  A new remote may
    // legitimately begin at sequence 1 even when another remote has advanced
    // much farther, while stale/equal/half-range values remain invalid for
    // the affected handle.
    if let Some(previous) = remotes.get(&handle).map(|remote| remote.sequence)
        && !sequence_is_newer(sequence, previous)
    {
        return Err(RealtimeOutcome::ProtocolViolation);
    }

    match &frame {
        ServerRealtimeFrameV1::RemotePlayerSpawn(spawn) => {
            let visible = state_is_visible(spawn.state());
            match remotes.get(&handle) {
                // A visible spawn is the only valid respawn transition for a
                // hidden handle.  It must still advance that handle's
                // sequence and consumes one visible slot.
                Some(remote) if !remote.visible && visible => {
                    if visible_remote_count(remotes) >= MAX_REMOTE_PLAYERS {
                        return Err(RealtimeOutcome::CapacityExceeded);
                    }
                    send_event(event_tx, RealtimeOwnerEvent::Spawn(spawn.clone()))
                        .map_err(|()| RealtimeOutcome::OwnerBackpressure)?;
                    remotes.insert(handle, RemoteState::new(sequence, true));
                }
                // Duplicate spawn, including hidden-to-hidden spawn, is not
                // a lifecycle transition and is rejected for this handle.
                Some(_) => return Err(RealtimeOutcome::ProtocolViolation),
                None => {
                    if remotes.len() >= MAX_TRACKED_REMOTE_HANDLES {
                        return Err(RealtimeOutcome::CapacityExceeded);
                    }
                    if visible && visible_remote_count(remotes) >= MAX_REMOTE_PLAYERS {
                        return Err(RealtimeOutcome::CapacityExceeded);
                    }
                    if visible {
                        send_event(event_tx, RealtimeOwnerEvent::Spawn(spawn.clone()))
                            .map_err(|()| RealtimeOutcome::OwnerBackpressure)?;
                    }
                    remotes.insert(handle, RemoteState::new(sequence, visible));
                }
            }
        }
        ServerRealtimeFrameV1::RemotePlayerUpdate(update) => {
            let Some(remote) = remotes.get(&handle) else {
                return Err(RealtimeOutcome::ProtocolViolation);
            };
            let was_visible = remote.visible;
            let visible = state_is_visible(update.state());
            if visible != was_visible {
                return Err(RealtimeOutcome::ProtocolViolation);
            }
            if was_visible || visible {
                send_event(event_tx, RealtimeOwnerEvent::Update(update.clone()))
                    .map_err(|()| RealtimeOutcome::OwnerBackpressure)?;
            }
            if let Some(remote) = remotes.get_mut(&handle) {
                remote.sequence = sequence;
                remote.visible = visible;
            } else {
                // The immutable lookup above guarantees this cannot happen
                // unless this function is changed to mutate concurrently.
                return Err(RealtimeOutcome::ProtocolViolation);
            }
        }
        ServerRealtimeFrameV1::RemotePlayerDespawn(despawn) => {
            let Some(remote) = remotes.get(&handle) else {
                return Err(RealtimeOutcome::ProtocolViolation);
            };
            if despawn.reason() == DespawnReason::Hidden && !remote.visible {
                return Err(RealtimeOutcome::ProtocolViolation);
            }
            if remote.visible {
                send_event(event_tx, RealtimeOwnerEvent::Despawn(despawn.clone()))
                    .map_err(|()| RealtimeOutcome::OwnerBackpressure)?;
            }
            if despawn.reason() == DespawnReason::Hidden {
                // Preserve the sequence tombstone for a hidden handle so a
                // subsequent visible spawn must be newer than this removal.
                if let Some(remote) = remotes.get_mut(&handle) {
                    remote.sequence = sequence;
                    remote.visible = false;
                }
            } else {
                // Terminal reasons release the handle record and its slot.
                remotes.remove(&handle);
            }
        }
        ServerRealtimeFrameV1::PresenceReady(_) => unreachable!(),
    }
    Ok(())
}

struct RemoteState {
    sequence: u32,
    visible: bool,
}

impl RemoteState {
    const fn new(sequence: u32, visible: bool) -> Self {
        Self { sequence, visible }
    }
}

fn state_is_visible(state: &LocalPresenceStateV1) -> bool {
    state.pose().player_state() != PlayerState::Hidden
}

fn visible_remote_count(remotes: &BTreeMap<PresenceHandle, RemoteState>) -> usize {
    remotes.values().filter(|remote| remote.visible).count()
}

fn unix_now() -> UnixTimestampMillis {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    UnixTimestampMillis::new(u64::try_from(millis).unwrap_or(u64::MAX))
}

fn expiry_instant(expires_at: UnixTimestampMillis) -> Option<Instant> {
    let now = unix_now().value();
    let remaining = expires_at.value().checked_sub(now)?;
    Some(Instant::now() + Duration::from_millis(remaining))
}

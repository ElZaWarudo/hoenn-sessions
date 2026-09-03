//! Credential-safe HTTP minting for one authenticated realtime attempt.
//!
//! The launcher owns the HTTP request and derives the WebSocket endpoint from
//! its already validated cloud base.  The sidecar still owns the WebSocket
//! transport: this module only hands it one consuming [`RealtimeGrant`].

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    MintRealtimeTicketRequest, MintRealtimeTicketResponse, REALTIME_TICKET_REQUEST_BODY_MAX_BYTES,
    UnixTimestampMillis,
};
use coop_protocol::{LocalPresenceStateV1, PresenceInteractionV1};
use coop_sidecar::{
    RealtimeEndpoint, RealtimeGrant, RealtimeInputError, RealtimeOutcome, RealtimeOwner,
    RealtimeOwnerEvent, realtime_channel, run_realtime,
};
use reqwest::{Method, StatusCode, header::CONTENT_TYPE};
use thiserror::Error;
use tokio::task::JoinHandle;

use coop_sidecar::control::ControlCommand;

use crate::{AuthSession, HttpClientError, ReqwestCloudApi};

#[cfg(test)]
use base64::Engine as _;
#[cfg(test)]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// Maximum assembled realtime ticket response body, in bytes.
pub const REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES: usize = 8 * 1024;

/// Stable errors returned by the launcher realtime HTTP seam.
///
/// These variants intentionally do not carry response bodies, URLs, header
/// values, parser diagnostics, or transport error text.  In particular, a
/// minted ticket never appears in an error or debug representation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeHttpError {
    #[error("realtime endpoint is invalid")]
    InvalidEndpoint,
    #[error("authentication session is no longer active")]
    SessionClosed,
    #[error("realtime request is invalid")]
    InvalidRequest,
    #[error("realtime request failed")]
    RequestFailed,
    #[error("realtime response was unauthorized")]
    Unauthorized,
    #[error("realtime response was rejected")]
    ResponseRejected,
    #[error("realtime response was invalid or too large")]
    InvalidResponse,
    #[error("realtime response did not match its request")]
    CorrelationFailed,
    #[error("realtime grant is invalid")]
    InvalidGrant,
    #[error("realtime grant has expired")]
    ExpiredGrant,
}

/// A boxed future used by the additive realtime API trait.
pub type RealtimeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RealtimeHttpError>> + Send + 'a>>;

/// Additive launcher API for minting one realtime grant.
pub trait RealtimeApi: Send + Sync {
    /// Mints exactly one bounded HTTP ticket and consumes it into a grant.
    ///
    /// The grant clock is sampled after the response has been received and
    /// decoded, so request latency cannot make a valid server-issued 30-second
    /// grant appear too long.  No retry or refresh is performed by this
    /// method.
    fn mint_realtime<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: MintRealtimeTicketRequest,
    ) -> RealtimeFuture<'a, RealtimeGrant>;
}

/// Stable launcher-side failures for the single realtime coordinator.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum RealtimeCoordinatorError {
    #[error("realtime coordinator could not be created")]
    InvalidState,
    #[error("realtime coordinator input failed")]
    Input,
    #[error("realtime coordinator task failed")]
    Task,
    #[error("realtime attempt terminated")]
    Terminated,
}

/// Events retained by the launcher coordinator. Readiness is deliberately
/// internal and can never be serialized onto the ROM control channel.
pub(crate) enum RealtimeCoordinatorEvent {
    Ready,
    Lifecycle(ControlCommand),
    Terminal,
}

/// Owns exactly one sidecar realtime owner/driver pair and its task.
///
/// The fixed pump generation is captured before mint and never changes. The
/// lifecycle must explicitly stop and join this coordinator before settling
/// child state or beginning cleanup.
pub(crate) struct RealtimeCoordinator {
    owner: RealtimeOwner,
    task: Option<JoinHandle<RealtimeOutcome>>,
    generation: u32,
    server_ready: bool,
    interaction_ready: bool,
    interaction_sequence: u64,
    ready_interaction_watermark: Option<u64>,
    pending_interactions: VecDeque<(u64, PresenceInteractionV1)>,
    #[cfg(test)]
    ordering_probe: Option<std::sync::Arc<RealtimeOrderingProbe>>,
    #[cfg(test)]
    event_return_barrier: Option<std::sync::Arc<RealtimeEventReturnBarrier>>,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RealtimeOrderingProbe {
    pub(crate) interaction_observed: tokio::sync::Notify,
    pub(crate) ready_observed: tokio::sync::Notify,
}

#[cfg(test)]
#[derive(Default)]
struct RealtimeEventReturnBarrier {
    event_received: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl RealtimeCoordinator {
    pub(crate) fn start(
        grant: RealtimeGrant,
        generation: u32,
        initial_state: LocalPresenceStateV1,
    ) -> Result<Self, RealtimeCoordinatorError> {
        let (owner, driver) =
            realtime_channel(initial_state).map_err(|_| RealtimeCoordinatorError::InvalidState)?;
        let task = tokio::spawn(run_realtime(grant, driver));
        Ok(Self {
            owner,
            task: Some(task),
            generation,
            server_ready: false,
            interaction_ready: false,
            interaction_sequence: 0,
            ready_interaction_watermark: None,
            pending_interactions: VecDeque::new(),
            #[cfg(test)]
            ordering_probe: None,
            #[cfg(test)]
            event_return_barrier: None,
        })
    }

    pub(crate) const fn generation(&self) -> u32 {
        self.generation
    }

    pub(crate) const fn interaction_ready(&self) -> bool {
        self.interaction_ready
    }

    pub(crate) const fn server_ready(&self) -> bool {
        self.server_ready
    }

    /// Opens the interaction gate only after the launcher has established its
    /// control-queue cutover for the observed server Ready event.
    pub(crate) fn activate_interactions(&mut self) -> Result<(), RealtimeCoordinatorError> {
        if !self.server_ready || self.interaction_ready {
            return Err(RealtimeCoordinatorError::Terminated);
        }
        let watermark = self
            .ready_interaction_watermark
            .ok_or(RealtimeCoordinatorError::Terminated)?;
        while let Some((sequence, interaction)) = self.pending_interactions.pop_front() {
            if sequence <= watermark {
                continue;
            }
            self.owner.interact(interaction).map_err(map_input_error)?;
        }
        self.interaction_ready = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_ordering_probe(&mut self, probe: std::sync::Arc<RealtimeOrderingProbe>) {
        self.ordering_probe = Some(probe);
    }

    #[cfg(test)]
    fn set_event_return_barrier(&mut self, barrier: std::sync::Arc<RealtimeEventReturnBarrier>) {
        self.event_return_barrier = Some(barrier);
    }

    pub(crate) fn update_state(
        &self,
        state: LocalPresenceStateV1,
    ) -> Result<(), RealtimeCoordinatorError> {
        self.owner
            .update_state(state)
            .map_err(|_| RealtimeCoordinatorError::Input)
    }

    /// Drops pre-readiness interactions and preserves FIFO after readiness.
    pub(crate) fn interact(
        &mut self,
        interaction: PresenceInteractionV1,
    ) -> Result<(), RealtimeCoordinatorError> {
        self.interaction_sequence = self
            .interaction_sequence
            .checked_add(1)
            .ok_or(RealtimeCoordinatorError::Terminated)?;
        #[cfg(test)]
        if let Some(probe) = &self.ordering_probe {
            probe.interaction_observed.notify_one();
        }
        if !self.server_ready {
            return Ok(());
        }
        if !self.interaction_ready {
            if self.pending_interactions.len() == coop_sidecar::MAX_INTERACTION_QUEUE {
                return Err(RealtimeCoordinatorError::Input);
            }
            self.pending_interactions
                .push_back((self.interaction_sequence, interaction));
            return Ok(());
        }
        self.owner.interact(interaction).map_err(map_input_error)
    }

    pub(crate) async fn next_event(
        &mut self,
    ) -> Result<RealtimeCoordinatorEvent, RealtimeCoordinatorError> {
        let Some(task) = self.task.as_ref() else {
            return Ok(RealtimeCoordinatorEvent::Terminal);
        };
        // A completed driver is terminal even when its bounded owner queue
        // still contains lifecycle events. Never forward stale peer input
        // after the driver has already failed closed (for example, because
        // that same queue overflowed).
        if task.is_finished() {
            return self.reap_completed_task().await;
        }
        let task = self.task.as_mut().expect("unfinished task remains owned");
        tokio::select! {
            biased;
            outcome = task => {
                self.task = None;
                match outcome {
                    Ok(_) => Ok(RealtimeCoordinatorEvent::Terminal),
                    Err(_) => Err(RealtimeCoordinatorError::Task),
                }
            }
            event = self.owner.recv_event() => {
                let Some(event) = event else {
                    return Err(RealtimeCoordinatorError::Terminated);
                };
                #[cfg(test)]
                if let Some(barrier) = &self.event_return_barrier {
                    barrier.event_received.notify_one();
                    barrier.release.notified().await;
                }
                if self.task.as_ref().is_some_and(JoinHandle::is_finished) {
                    return self.reap_completed_task().await;
                }
                let mapped = map_owner_event(event, &mut self.server_ready);
                if matches!(mapped, Ok(RealtimeCoordinatorEvent::Ready)) {
                    self.ready_interaction_watermark = Some(self.interaction_sequence);
                    #[cfg(test)]
                    if let Some(probe) = &self.ordering_probe {
                        probe.ready_observed.notify_one();
                    }
                }
                mapped
            },
        }
    }

    async fn reap_completed_task(
        &mut self,
    ) -> Result<RealtimeCoordinatorEvent, RealtimeCoordinatorError> {
        let outcome = self
            .task
            .take()
            .expect("completed task remains owned")
            .await;
        match outcome {
            Ok(_) => Ok(RealtimeCoordinatorEvent::Terminal),
            Err(_) => Err(RealtimeCoordinatorError::Task),
        }
    }

    pub(crate) async fn stop_and_join(mut self) -> Result<(), RealtimeCoordinatorError> {
        self.server_ready = false;
        self.interaction_ready = false;
        self.pending_interactions.clear();
        self.owner.stop();
        let Some(task) = self.task.take() else {
            return Ok(());
        };
        match task.await {
            Ok(RealtimeOutcome::OwnerStopped) => Ok(()),
            Ok(_) => Err(RealtimeCoordinatorError::Terminated),
            Err(_) => Err(RealtimeCoordinatorError::Task),
        }
    }
}

impl Drop for RealtimeCoordinator {
    fn drop(&mut self) {
        self.owner.stop();
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
    }
}

fn map_input_error(_error: RealtimeInputError) -> RealtimeCoordinatorError {
    RealtimeCoordinatorError::Input
}

fn map_owner_event(
    event: RealtimeOwnerEvent,
    ready: &mut bool,
) -> Result<RealtimeCoordinatorEvent, RealtimeCoordinatorError> {
    match event {
        RealtimeOwnerEvent::Ready(_) if !*ready => {
            *ready = true;
            Ok(RealtimeCoordinatorEvent::Ready)
        }
        RealtimeOwnerEvent::Spawn(spawn) if *ready => Ok(RealtimeCoordinatorEvent::Lifecycle(
            ControlCommand::RemotePlayerSpawn(spawn),
        )),
        RealtimeOwnerEvent::Update(update) if *ready => Ok(RealtimeCoordinatorEvent::Lifecycle(
            ControlCommand::RemotePlayerUpdate(update),
        )),
        RealtimeOwnerEvent::Despawn(despawn) if *ready => Ok(RealtimeCoordinatorEvent::Lifecycle(
            ControlCommand::RemotePlayerDespawn(despawn),
        )),
        _ => Err(RealtimeCoordinatorError::Terminated),
    }
}

#[cfg(test)]
pub(crate) async fn accept_websocket_for_test(listener: TcpListener) -> TcpStream {
    let (mut stream, _) = listener.accept().await.expect("websocket accept");
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream
            .read_exact(&mut byte)
            .await
            .expect("websocket request");
        request.push(byte[0]);
        assert!(request.len() <= 8 * 1024, "bounded websocket request");
    }
    let request = String::from_utf8(request).expect("ASCII websocket request");
    let key = request
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("sec-websocket-key")
                    .then(|| value.trim())
            })
        })
        .expect("websocket key");
    let mut accept_input = key.as_bytes().to_vec();
    accept_input.extend_from_slice(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = base64::engine::general_purpose::STANDARD.encode(sha1_for_test(&accept_input));
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("websocket response");
    stream
}

#[cfg(test)]
pub(crate) async fn read_websocket_text_for_test(stream: &mut TcpStream) -> Vec<u8> {
    let mut head = [0_u8; 2];
    stream.read_exact(&mut head).await.expect("websocket frame");
    assert_eq!(head[0] & 0x0f, 1, "expected text frame");
    let masked = head[1] & 0x80 != 0;
    let mut length = u64::from(head[1] & 0x7f);
    if length == 126 {
        let mut bytes = [0_u8; 2];
        stream.read_exact(&mut bytes).await.unwrap();
        length = u64::from(u16::from_be_bytes(bytes));
    } else if length == 127 {
        let mut bytes = [0_u8; 8];
        stream.read_exact(&mut bytes).await.unwrap();
        length = u64::from_be_bytes(bytes);
    }
    let mut mask = [0_u8; 4];
    if masked {
        stream.read_exact(&mut mask).await.unwrap();
    }
    let mut payload = vec![0_u8; usize::try_from(length).expect("bounded frame")];
    stream.read_exact(&mut payload).await.unwrap();
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    payload
}

#[cfg(test)]
pub(crate) async fn send_websocket_text_for_test(stream: &mut TcpStream, payload: &[u8]) {
    let mut frame = vec![0x81];
    match payload.len() {
        length @ 0..=125 => frame.push(u8::try_from(length).unwrap()),
        length @ 126..=65_535 => {
            frame.push(126);
            frame.extend_from_slice(&u16::try_from(length).unwrap().to_be_bytes());
        }
        length => {
            frame.push(127);
            frame.extend_from_slice(&u64::try_from(length).unwrap().to_be_bytes());
        }
    }
    frame.extend_from_slice(payload);
    stream.write_all(&frame).await.expect("websocket send");
}

#[cfg(test)]
#[expect(
    clippy::many_single_char_names,
    reason = "SHA-1 round variables use the names fixed by the algorithm specification"
)]
fn sha1_for_test(bytes: &[u8]) -> [u8; 20] {
    let bit_len = u64::try_from(bytes.len()).unwrap().wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    let mut h = [
        0x6745_2301_u32,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    for chunk in padded.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in words[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (index, word) in words.into_iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = [0_u8; 20];
    for (chunk, word) in digest.chunks_exact_mut(4).zip(h) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

impl ReqwestCloudApi {
    /// Derives the canonical realtime WebSocket endpoint from this API's
    /// validated HTTP(S) base.
    ///
    /// HTTPS bases become WSS and literal-loopback HTTP bases become WS.  The
    /// authority and any explicit port are retained, while the path is fixed
    /// to `/v1/realtime` and cannot receive query or fragment data.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeHttpError::InvalidEndpoint`] if the endpoint cannot
    /// be represented by the sidecar's validated realtime endpoint type.
    pub fn realtime_endpoint(&self) -> Result<RealtimeEndpoint, RealtimeHttpError> {
        let mut endpoint = self.base.clone();
        let scheme = match endpoint.scheme() {
            "https" => "wss",
            "http" if matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]" | "::1")) => "ws",
            _ => return Err(RealtimeHttpError::InvalidEndpoint),
        };
        endpoint
            .set_scheme(scheme)
            .map_err(|()| RealtimeHttpError::InvalidEndpoint)?;
        endpoint.set_path("/v1/realtime");
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        RealtimeEndpoint::new(endpoint.as_str()).map_err(|_| RealtimeHttpError::InvalidEndpoint)
    }

    /// Mints and consumes one authenticated realtime grant.
    ///
    /// Exactly one POST is attempted.  The request uses the active access
    /// token as its Authorization bearer and the strict mint DTO as its JSON
    /// body.  The response is bounded before decoding, correlated before its
    /// non-Clone ticket is moved, and then validated against a clock sampled
    /// after the response is decoded.
    ///
    /// # Errors
    ///
    /// Returns [`RealtimeHttpError::Unauthorized`] only for HTTP 401.  All
    /// other HTTP, transport, parse, bound, correlation, and grant failures
    /// use stable, secret-free variants.
    pub async fn mint_realtime(
        &self,
        auth: &AuthSession,
        request: MintRealtimeTicketRequest,
    ) -> Result<RealtimeGrant, RealtimeHttpError> {
        let endpoint = self.realtime_endpoint()?;
        let url = self
            .url("v1/realtime/tickets")
            .map_err(|_| RealtimeHttpError::InvalidEndpoint)?;
        let body = serde_json::to_vec(&request).map_err(|_| RealtimeHttpError::InvalidRequest)?;
        if body.len() > REALTIME_TICKET_REQUEST_BODY_MAX_BYTES {
            return Err(RealtimeHttpError::InvalidRequest);
        }
        let http_request = self
            .authenticated(Method::POST, url, auth)
            .map_err(|error| map_http_error(&error))?
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        let response = http_request
            .send()
            .await
            .map_err(|_| RealtimeHttpError::RequestFailed)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(RealtimeHttpError::Unauthorized);
        }
        if !response.status().is_success() {
            return Err(RealtimeHttpError::ResponseRejected);
        }
        let bytes = crate::bounded_body(response, REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES)
            .await
            .map_err(|error| map_body_error(&error))?;
        let response: MintRealtimeTicketResponse =
            serde_json::from_slice(&bytes).map_err(|_| RealtimeHttpError::InvalidResponse)?;
        if !response.matches_request(&request) {
            return Err(RealtimeHttpError::CorrelationFailed);
        }
        // Sample only after the complete response has arrived and passed
        // strict decoding/correlation.  This keeps server-issued TTL bounds
        // independent of HTTP request and response latency.
        grant_from_response(endpoint, &request, response, unix_now())
    }

    /// Alias for [`Self::mint_realtime`] emphasizing the consuming result.
    ///
    /// # Errors
    ///
    /// Returns the same stable errors as [`Self::mint_realtime`].
    pub async fn mint_realtime_grant(
        &self,
        auth: &AuthSession,
        request: MintRealtimeTicketRequest,
    ) -> Result<RealtimeGrant, RealtimeHttpError> {
        self.mint_realtime(auth, request).await
    }
}

impl RealtimeApi for ReqwestCloudApi {
    fn mint_realtime<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: MintRealtimeTicketRequest,
    ) -> RealtimeFuture<'a, RealtimeGrant> {
        Box::pin(async move { self.mint_realtime(auth, request).await })
    }
}

fn grant_from_response(
    endpoint: RealtimeEndpoint,
    request: &MintRealtimeTicketRequest,
    response: MintRealtimeTicketResponse,
    now: UnixTimestampMillis,
) -> Result<RealtimeGrant, RealtimeHttpError> {
    // Correlation is deliberately repeated in this consuming seam so its
    // ownership contract remains explicit if another launcher caller uses it
    // later.  The public adapter already performs this check before calling
    // here, and this function never exposes the ticket.
    if !response.matches_request(request) {
        return Err(RealtimeHttpError::CorrelationFailed);
    }
    let (_, _, ticket, expires_at) = response.into_parts();
    RealtimeGrant::with_now(ticket, endpoint, expires_at, now).map_err(map_grant_error)
}

fn unix_now() -> UnixTimestampMillis {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    UnixTimestampMillis::new(u64::try_from(millis).unwrap_or(u64::MAX))
}

fn map_http_error(error: &HttpClientError) -> RealtimeHttpError {
    match error {
        HttpClientError::InvalidEndpoint => RealtimeHttpError::InvalidEndpoint,
        HttpClientError::SessionClosed => RealtimeHttpError::SessionClosed,
        HttpClientError::Transport(_) | HttpClientError::Response | HttpClientError::Status(_) => {
            RealtimeHttpError::RequestFailed
        }
    }
}

fn map_body_error(error: &HttpClientError) -> RealtimeHttpError {
    match error {
        HttpClientError::Transport(_) => RealtimeHttpError::RequestFailed,
        _ => RealtimeHttpError::InvalidResponse,
    }
}

fn map_grant_error(error: coop_sidecar::RealtimeError) -> RealtimeHttpError {
    match error {
        coop_sidecar::RealtimeError::ExpiredGrant => RealtimeHttpError::ExpiredGrant,
        coop_sidecar::RealtimeError::InvalidGrant
        | coop_sidecar::RealtimeError::InvalidEndpoint
        | coop_sidecar::RealtimeError::InvalidState => RealtimeHttpError::InvalidGrant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{
        BridgeAbiVersion, CharacterId, ClientInstanceId, GameBuildId, MgbaVersion,
        MintRealtimeTicketRequest, MintRealtimeTicketResponse, ProtocolVersion,
        REALTIME_TICKET_TTL_MS, RealtimeTicket, RuntimeBuildIdentity, RuntimeLeaseFence,
        ServerRealtimeFrameV1, SessionEpoch, SessionId, Sha256Digest, StableRuntimeSession,
        encode_server_realtime_frame,
    };
    use coop_protocol::{
        AnimationId, AvatarId, CanonicalUsername, DespawnReason, Direction, LocalPresenceStateV1,
        MovementMode, PlayerState, PresenceHandle, PresenceInteractionV1, PresencePoseV1, RegionId,
        RemotePlayerDespawnV1, RemotePlayerSpawnV1, RemotePlayerUpdateV1, WorldLocation,
    };
    use uuid::Uuid;

    fn runtime() -> RuntimeLeaseFence {
        let session = StableRuntimeSession::new(
            SessionId::new(Uuid::from_u128(101)).expect("session id"),
            CharacterId::new(Uuid::from_u128(102)).expect("character id"),
            SessionEpoch::new(103).expect("session epoch"),
            ClientInstanceId::new(Uuid::from_u128(104)).expect("client id"),
        );
        let build = RuntimeBuildIdentity::new(
            GameBuildId::new("launcher-realtime-test").expect("build id"),
            Sha256Digest::from_bytes([9; 32]),
            MgbaVersion::new("0.11.3").expect("mGBA version"),
            BridgeAbiVersion::new(1).expect("bridge ABI"),
            ProtocolVersion::new(1).expect("protocol version"),
        );
        RuntimeLeaseFence::new(session, build)
    }

    fn consume_at(now: u64, expires_at: u64) -> Result<RealtimeGrant, RealtimeHttpError> {
        let request = MintRealtimeTicketRequest::v1(runtime());
        let response = MintRealtimeTicketResponse::v1(
            request.runtime().clone(),
            RealtimeTicket::from_bytes([31; 32]).expect("ticket"),
            UnixTimestampMillis::new(expires_at),
        )
        .expect("response");
        let endpoint = RealtimeEndpoint::new("ws://127.0.0.1:3000/v1/realtime").expect("endpoint");
        grant_from_response(endpoint, &request, response, UnixTimestampMillis::new(now))
    }

    fn state(sequence: u32) -> LocalPresenceStateV1 {
        let pose = PresencePoseV1::new(
            WorldLocation::new(RegionId::Hoenn, 1, 0, 4, 5).unwrap(),
            0,
            Direction::South,
            sequence,
            1,
            MovementMode::Idle,
            AnimationId::Idle,
            AvatarId::Brendan,
            PlayerState::Overworld,
        )
        .unwrap();
        LocalPresenceStateV1::new(pose, sequence).unwrap()
    }

    #[test]
    fn fixed_clock_private_seam_covers_expired_exact_and_excessive_ttl() {
        let now = 1_000_000;
        assert!(matches!(
            consume_at(now, now - 1),
            Err(RealtimeHttpError::ExpiredGrant)
        ));
        let exact = consume_at(now, now + REALTIME_TICKET_TTL_MS).expect("exact TTL grant");
        assert_eq!(exact.expires_at().value(), now + REALTIME_TICKET_TTL_MS);
        assert!(matches!(
            consume_at(now, now + REALTIME_TICKET_TTL_MS + 1),
            Err(RealtimeHttpError::InvalidGrant)
        ));
    }

    #[test]
    fn coordinator_keeps_ready_internal_and_maps_lifecycle_exactly() {
        let handle = PresenceHandle::new(7).unwrap();
        let mut ready = false;
        assert!(matches!(
            map_owner_event(
                RealtimeOwnerEvent::Ready(coop_cloud::PresenceReadyV1::new(handle)),
                &mut ready,
            ),
            Ok(RealtimeCoordinatorEvent::Ready)
        ));
        assert!(ready);

        let spawn =
            RemotePlayerSpawnV1::new(handle, 1, state(1), CanonicalUsername::new("may").unwrap())
                .unwrap();
        let update = RemotePlayerUpdateV1::new(handle, 2, state(2)).unwrap();
        let despawn = RemotePlayerDespawnV1::new(handle, 3, DespawnReason::Disconnected).unwrap();
        assert!(matches!(
            map_owner_event(RealtimeOwnerEvent::Spawn(spawn), &mut ready),
            Ok(RealtimeCoordinatorEvent::Lifecycle(
                ControlCommand::RemotePlayerSpawn(_)
            ))
        ));
        assert!(matches!(
            map_owner_event(RealtimeOwnerEvent::Update(update), &mut ready),
            Ok(RealtimeCoordinatorEvent::Lifecycle(
                ControlCommand::RemotePlayerUpdate(_)
            ))
        ));
        assert!(matches!(
            map_owner_event(RealtimeOwnerEvent::Despawn(despawn), &mut ready),
            Ok(RealtimeCoordinatorEvent::Lifecycle(
                ControlCommand::RemotePlayerDespawn(_)
            ))
        ));
        assert!(matches!(
            map_owner_event(
                RealtimeOwnerEvent::Ready(coop_cloud::PresenceReadyV1::new(handle)),
                &mut ready,
            ),
            Err(RealtimeCoordinatorError::Terminated)
        ));
    }

    #[test]
    fn coordinator_rejects_lifecycle_before_ready() {
        let handle = PresenceHandle::new(8).unwrap();
        let spawn = RemotePlayerSpawnV1::new(
            handle,
            1,
            state(1),
            CanonicalUsername::new("brendan").unwrap(),
        )
        .unwrap();
        let mut ready = false;
        assert!(matches!(
            map_owner_event(RealtimeOwnerEvent::Spawn(spawn), &mut ready),
            Err(RealtimeCoordinatorError::Terminated)
        ));
    }

    #[tokio::test]
    async fn coordinator_drops_pre_ready_interaction_and_joins_its_task() {
        let now = unix_now().value();
        let grant = consume_at(now, now + REALTIME_TICKET_TTL_MS).unwrap();
        let mut coordinator = RealtimeCoordinator::start(grant, 11, state(1)).unwrap();
        let interaction =
            PresenceInteractionV1::new(PresenceHandle::new(9).unwrap(), 1, 1, 4, 5).unwrap();
        assert!(coordinator.interact(interaction).is_ok());
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            coordinator.stop_and_join(),
        )
        .await
        .expect("coordinator task must terminate without a synchronization sleep")
        .unwrap();
    }

    #[tokio::test]
    async fn coordinator_observes_finished_driver_before_saturated_owner_queue() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let remote = PresenceHandle::new(44).unwrap();
        let server = tokio::spawn(async move {
            let mut socket = accept_websocket_for_test(listener).await;
            let _cached = read_websocket_text_for_test(&mut socket).await;
            let ready = ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap());
            send_websocket_text_for_test(
                &mut socket,
                &encode_server_realtime_frame(&ready).unwrap(),
            )
            .await;
            let spawn = ServerRealtimeFrameV1::remote_player_spawn(
                RemotePlayerSpawnV1::new(
                    remote,
                    1,
                    state(1),
                    CanonicalUsername::new("misty").unwrap(),
                )
                .unwrap(),
            );
            send_websocket_text_for_test(
                &mut socket,
                &encode_server_realtime_frame(&spawn).unwrap(),
            )
            .await;
            let last_sequence = u32::try_from(coop_sidecar::MAX_OWNER_EVENT_QUEUE).unwrap() + 3;
            for sequence in 2..=last_sequence {
                let update = ServerRealtimeFrameV1::remote_player_update(
                    RemotePlayerUpdateV1::new(remote, sequence, state(sequence)).unwrap(),
                );
                send_websocket_text_for_test(
                    &mut socket,
                    &encode_server_realtime_frame(&update).unwrap(),
                )
                .await;
            }
        });
        let now = unix_now();
        let grant = RealtimeGrant::with_now(
            RealtimeTicket::from_bytes([45; 32]).unwrap(),
            RealtimeEndpoint::new(format!("ws://127.0.0.1:{port}/v1/realtime")).unwrap(),
            UnixTimestampMillis::new(now.value() + REALTIME_TICKET_TTL_MS),
            now,
        )
        .unwrap();
        let mut coordinator = RealtimeCoordinator::start(grant, 1, state(1)).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if coordinator.task.as_ref().unwrap().is_finished() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owner backpressure must terminate the driver");
        assert!(matches!(
            coordinator.next_event().await,
            Ok(RealtimeCoordinatorEvent::Terminal)
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn coordinator_rechecks_driver_after_owner_event_wins_poll_race() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let remote = PresenceHandle::new(45).unwrap();
        let (flood_tx, flood_rx) = tokio::sync::oneshot::channel();
        let (driver_closed_tx, driver_closed_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut socket = accept_websocket_for_test(listener).await;
            let _cached = read_websocket_text_for_test(&mut socket).await;
            let ready = ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap());
            send_websocket_text_for_test(
                &mut socket,
                &encode_server_realtime_frame(&ready).unwrap(),
            )
            .await;
            flood_rx.await.unwrap();
            let spawn = ServerRealtimeFrameV1::remote_player_spawn(
                RemotePlayerSpawnV1::new(
                    remote,
                    1,
                    state(1),
                    CanonicalUsername::new("misty").unwrap(),
                )
                .unwrap(),
            );
            send_websocket_text_for_test(
                &mut socket,
                &encode_server_realtime_frame(&spawn).unwrap(),
            )
            .await;
            let last_sequence = u32::try_from(coop_sidecar::MAX_OWNER_EVENT_QUEUE).unwrap() + 3;
            for sequence in 2..=last_sequence {
                let update = ServerRealtimeFrameV1::remote_player_update(
                    RemotePlayerUpdateV1::new(remote, sequence, state(sequence)).unwrap(),
                );
                send_websocket_text_for_test(
                    &mut socket,
                    &encode_server_realtime_frame(&update).unwrap(),
                )
                .await;
            }
            let mut remainder = Vec::new();
            socket.read_to_end(&mut remainder).await.unwrap();
            driver_closed_tx.send(()).unwrap();
        });
        let now = unix_now();
        let grant = RealtimeGrant::with_now(
            RealtimeTicket::from_bytes([46; 32]).unwrap(),
            RealtimeEndpoint::new(format!("ws://127.0.0.1:{port}/v1/realtime")).unwrap(),
            UnixTimestampMillis::new(now.value() + REALTIME_TICKET_TTL_MS),
            now,
        )
        .unwrap();
        let mut coordinator = RealtimeCoordinator::start(grant, 1, state(1)).unwrap();
        let barrier = std::sync::Arc::new(RealtimeEventReturnBarrier::default());
        coordinator.set_event_return_barrier(std::sync::Arc::clone(&barrier));
        let call = tokio::spawn(async move {
            let result = coordinator.next_event().await;
            (coordinator, result)
        });
        barrier.event_received.notified().await;
        flood_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), driver_closed_rx)
            .await
            .expect("owner backpressure must close the driver during the held return")
            .unwrap();
        barrier.release.notify_one();
        let (coordinator, result) = call.await.unwrap();
        assert!(matches!(result, Ok(RealtimeCoordinatorEvent::Terminal)));
        coordinator.stop_and_join().await.unwrap();
        server.await.unwrap();
    }
}

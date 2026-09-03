//! Credential-safe HTTP minting for one authenticated realtime attempt.
//!
//! The launcher owns the HTTP request and derives the WebSocket endpoint from
//! its already validated cloud base.  The sidecar still owns the WebSocket
//! transport: this module only hands it one consuming [`RealtimeGrant`].

use std::{
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    MintRealtimeTicketRequest, MintRealtimeTicketResponse, REALTIME_TICKET_REQUEST_BODY_MAX_BYTES,
    UnixTimestampMillis,
};
use coop_sidecar::{RealtimeEndpoint, RealtimeGrant};
use reqwest::{Method, StatusCode, header::CONTENT_TYPE};
use thiserror::Error;

use crate::{AuthSession, HttpClientError, ReqwestCloudApi};

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
        SessionEpoch, SessionId, Sha256Digest, StableRuntimeSession,
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
}

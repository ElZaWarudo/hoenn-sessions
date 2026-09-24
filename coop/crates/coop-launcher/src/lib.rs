//! Secure local orchestration for one fenced cloud-coop session.

#![forbid(unsafe_code)]

pub mod arrival_verifier;
pub mod auth;
pub mod compat;
pub mod desktop;
pub mod epoch;
pub mod group_travel;
pub mod keychain;
pub mod online;
pub mod process;
pub mod realtime;
pub mod recovery;
pub mod rom_travel;
pub mod session;
pub mod update;
#[cfg(windows)]
pub mod windows_mgba_supervisor;

pub use auth::{AuthApi, AuthError, AuthSession};
pub use compat::{BuildCompatibility, CompatibilityError, SelectedRomWorld};
pub use coop_cloud::TrustedManifestKey;
pub use desktop::{
    AuthFailure, AuthFlow, AuthRequest, BlockReason, BootstrapInput, Command, CommandRejection,
    Controller, Dispatch, Effect, RecoveryReason, RecoveryResult, ReleaseReadiness, RetryTarget,
    Secret, ServiceFailure, SignOutFailure, StartFailure, State, Status, UiModel, UpdateFailure,
};
pub use epoch::{EpochError, EpochRecord, EpochStore};
pub use group_travel::{GroupTravelError, GroupTravelFuture};
pub use keychain::{KeychainError, OsKeychain, RefreshTokenStore};
pub use process::{
    CommandSpec, ControlChannel, ControlShutdownEvidence, DescendantCompletionEvidence,
    JobTerminationEvidence, ProcessError, RecoveryDisposition, RootReapEvidence,
    ShutdownDisposition, ShutdownPath, SoftCloseDisposition, SupervisedChildren, SupervisorEvent,
    materialize_bridge_session,
};
pub use realtime::{
    REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES, RealtimeApi, RealtimeFuture, RealtimeHttpError,
};
pub use recovery::{
    RecoveryCandidate, RecoveryDiscovery, RecoveryError, RecoveryMarker, RecoveryMarkerV2,
    RecoveryOutcome, RecoveryReconciler, RecoverySession,
};
pub use session::{CloudApi, SessionConfig, SessionError, SessionLifecycle, SessionWorkspace};
pub use update::{
    AcceptedGeneration, ArtifactIdentity, ArtifactPayload, ArtifactSet, GenerationArtifact,
    GenerationHandoff, GenerationStore, InstalledGeneration, MAX_ARTIFACT_BYTES,
    MAX_ENVELOPE_BYTES, ReleaseDescriptor, ReleaseStore, SignedReleaseEnvelope, TrustedReleaseKey,
    UpdateError, VerifiedRelease,
};

use std::time::Duration;

use coop_cloud::{
    AcquireLeaseRequest, ArtifactIdentity as CloudArtifactIdentity, CharacterId,
    HeartbeatLeaseRequest, LeaseContract, LeaseFence, LoginRequest, LoginResponse, LogoutRequest,
    LogoutResponse, PrepareSnapshotRequest, ReconnectLeaseRequest, RefreshRequest, RefreshResponse,
    RegisterRequest, RegisterResponse, ReleaseLeaseRequest, Revision, RomHandoffCommitRequest,
    RomHandoffPrepareRequest, RomHandoffPrepareResponse, RomHandoffRecoveryRequest,
    RomHandoffRecoveryStatus, SignedManifestEnvelope, SnapshotFinalizeRequest, SnapshotListRequest,
    SnapshotListResponse, SnapshotPrepareResponse, SnapshotRecord, SnapshotRestoreRequest,
    SnapshotRestoreResponse, UploadTarget,
};
use reqwest::{Client, Method, StatusCode, Url};
use thiserror::Error;

const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const MAX_JSON_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum HttpClientError {
    #[error("API endpoint is not a permitted HTTPS or literal-loopback URL")]
    InvalidEndpoint,
    #[error("cloud request failed")]
    Transport(#[source] reqwest::Error),
    #[error("cloud response was invalid or too large")]
    Response,
    #[error("cloud response returned status {0}")]
    Status(StatusCode),
    #[error("authentication session is no longer active")]
    SessionClosed,
}

#[allow(clippy::needless_pass_by_value)]
fn map_cloud_error(error: HttpClientError) -> SessionError {
    match error {
        HttpClientError::Status(StatusCode::UNAUTHORIZED) => SessionError::Unauthorized,
        HttpClientError::SessionClosed => SessionError::Auth(AuthError::SessionClosed),
        _ => SessionError::Cloud,
    }
}

fn map_acquire_error(error: HttpClientError) -> SessionError {
    match error {
        HttpClientError::Status(StatusCode::CONFLICT) => SessionError::AcquireConflict,
        other => map_cloud_error(other),
    }
}

/// Strict no-redirect HTTP adapter for the certified local Phase 2 routes.
#[derive(Clone)]
pub struct ReqwestCloudApi {
    client: Client,
    base: Url,
}

impl std::fmt::Debug for ReqwestCloudApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReqwestCloudApi")
            .field("base", &self.base)
            .field("client", &"[CONFIGURED]")
            .finish_non_exhaustive()
    }
}

impl ReqwestCloudApi {
    /// Creates a client for a permitted HTTPS or literal-loopback endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for noncanonical, unsafe, or malformed endpoints.
    pub fn new(base: &str) -> Result<Self, HttpClientError> {
        let raw_base = base;
        let base = Url::parse(raw_base).map_err(|_| HttpClientError::InvalidEndpoint)?;
        if has_noncanonical_authority(raw_base)
            || explicit_port(raw_base)
                .is_some_and(|port| matches!((base.scheme(), port), ("https", 443) | ("http", 80)))
        {
            return Err(HttpClientError::InvalidEndpoint);
        }
        validate_endpoint(&base)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(HttpClientError::Transport)?;
        Ok(Self { client, base })
    }

    fn url(&self, path: &str) -> Result<Url, HttpClientError> {
        self.base
            .join(path)
            .map_err(|_| HttpClientError::InvalidEndpoint)
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        max: usize,
    ) -> Result<T, HttpClientError> {
        let response = request.send().await.map_err(HttpClientError::Transport)?;
        if !response.status().is_success() {
            return Err(HttpClientError::Status(response.status()));
        }
        let bytes = bounded_body(response, max).await?;
        serde_json::from_slice(&bytes).map_err(|_| HttpClientError::Response)
    }

    async fn send_empty(&self, request: reqwest::RequestBuilder) -> Result<(), HttpClientError> {
        let response = request.send().await.map_err(HttpClientError::Transport)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(HttpClientError::Status(response.status()))
        }
    }

    fn authenticated(
        &self,
        method: Method,
        url: Url,
        auth: &AuthSession,
    ) -> Result<reqwest::RequestBuilder, HttpClientError> {
        let access_token = auth.access_token().ok_or(HttpClientError::SessionClosed)?;
        Ok(self
            .client
            .request(method, url)
            .bearer_auth(access_token.expose_secret()))
    }

    fn authenticated_with_fence(
        &self,
        method: Method,
        url: Url,
        auth: &AuthSession,
        fence: LeaseFence,
    ) -> Result<reqwest::RequestBuilder, HttpClientError> {
        Ok(self
            .authenticated(method, url, auth)?
            .header("X-Coop-Session-Id", fence.session_id.to_string())
            .header(
                "X-Coop-Session-Epoch",
                fence.session_epoch.value().to_string(),
            )
            .header(
                "X-Coop-Client-Instance-Id",
                fence.client_instance_id.to_string(),
            ))
    }
}

fn raw_authority(raw_url: &str) -> Option<&str> {
    raw_url
        .split_once("://")?
        .1
        .split(['/', '?', '#', '\\'])
        .next()
}

fn has_noncanonical_authority(raw_url: &str) -> bool {
    raw_authority(raw_url)
        .is_none_or(|authority| authority.contains('@') || authority.ends_with(':'))
}

fn explicit_port(raw_url: &str) -> Option<u16> {
    let authority = raw_authority(raw_url)?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let port = if let Some(end) = authority.find(']') {
        authority.get(end + 1..)?.strip_prefix(':')?
    } else {
        authority.rsplit_once(':')?.1
    };
    port.parse().ok()
}

fn validate_endpoint(url: &Url) -> Result<(), HttpClientError> {
    if url.cannot_be_a_base()
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || !url.path().trim_matches('/').is_empty()
        || (url.scheme() == "https" && url.port().is_some_and(|port| port == 443))
    {
        return Err(HttpClientError::InvalidEndpoint);
    }
    match url.scheme() {
        "https" => {
            if url.host_str().is_none() {
                Err(HttpClientError::InvalidEndpoint)
            } else {
                Ok(())
            }
        }
        "http" if matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1")) => Ok(()),
        _ => Err(HttpClientError::InvalidEndpoint),
    }
}

async fn bounded_body(
    mut response: reqwest::Response,
    max: usize,
) -> Result<Vec<u8>, HttpClientError> {
    if response
        .content_length()
        .is_some_and(|size| size > max as u64)
    {
        return Err(HttpClientError::Response);
    }
    let mut body = Vec::with_capacity(
        usize::try_from(response.content_length().unwrap_or(0).min(max as u64)).unwrap_or(max),
    );
    while let Some(chunk) = response.chunk().await.map_err(HttpClientError::Transport)? {
        if body.len().saturating_add(chunk.len()) > max {
            return Err(HttpClientError::Response);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Correlates the finalized commit response with the request fence and
/// lineage before exposing it to the coordinator. The commit request does
/// not carry a destination world, so the coordinator must compare
/// `rom_world_id` with the prepared handoff response separately.
fn validate_committed_handoff(
    request: &RomHandoffCommitRequest,
    response: &SnapshotRecord,
) -> Result<(), SessionError> {
    response.validate().map_err(|_| SessionError::Cloud)?;
    let next_revision = request
        .expected_revision
        .next()
        .map_err(|_| SessionError::Cloud)?;
    if response.api_version != coop_cloud::ApiVersion::V1
        || response.snapshot_id != request.stage_id
        || response.character_id != request.character_id
        || response.session_id != request.session_id
        || response.session_epoch != request.session_epoch
        || response.parent_revision != request.expected_revision
        || response.revision != next_revision
    {
        return Err(SessionError::Cloud);
    }
    Ok(())
}

type AResult<'a, T> = auth::AuthFuture<'a, T>;

impl AuthApi for ReqwestCloudApi {
    fn register(&self, request: RegisterRequest) -> AResult<'_, RegisterResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/auth/register")
                .map_err(|_| AuthError::Transport)?;
            self.send_json(
                self.client.post(url).json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(|error| match error {
                HttpClientError::Status(StatusCode::UNAUTHORIZED) => AuthError::InvalidCredentials,
                _ => AuthError::Transport,
            })
        })
    }
    fn login(&self, request: LoginRequest) -> AResult<'_, LoginResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/auth/login")
                .map_err(|_| AuthError::Transport)?;
            self.send_json(
                self.client.post(url).json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(|_| AuthError::Transport)
        })
    }
    fn refresh(&self, request: RefreshRequest) -> AResult<'_, RefreshResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/auth/refresh")
                .map_err(|_| AuthError::Transport)?;
            self.send_json(
                self.client.post(url).json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(|_| AuthError::Transport)
        })
    }
    fn logout(&self, request: LogoutRequest) -> AResult<'_, LogoutResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/auth/logout")
                .map_err(|_| AuthError::Transport)?;
            self.send_json(
                self.client.post(url).json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(|_| AuthError::Transport)
        })
    }
}

impl CloudApi for ReqwestCloudApi {
    fn reconcile_rom_handoff<'a>(
        &'a self,
        auth: &'a AuthSession,
        character_id: CharacterId,
        idempotency_key: coop_cloud::IdempotencyKey,
    ) -> session::CloudFuture<'a, RomHandoffRecoveryStatus> {
        Box::pin(async move {
            let fence = auth.active_fence().ok_or(SessionError::Lease)?;
            if fence.character_id != character_id {
                return Err(SessionError::Lease);
            }
            let url = self
                .url(&format!(
                    "v1/characters/{character_id}/rom-handoff/reconcile"
                ))
                .map_err(|_| SessionError::Cloud)?;
            let request = RomHandoffRecoveryRequest {
                api_version: coop_cloud::ApiVersion::V1,
                character_id,
                idempotency_key,
            };
            let response: RomHandoffRecoveryStatus = self
                .send_json(
                    self.authenticated_with_fence(Method::POST, url, auth, fence)
                        .map_err(map_cloud_error)?
                        .json(&request),
                    MAX_JSON_RESPONSE_BYTES,
                )
                .await
                .map_err(map_cloud_error)?;
            let (revision, key) = match &response {
                RomHandoffRecoveryStatus::Staged {
                    expected_revision,
                    idempotency_key,
                    ..
                }
                | RomHandoffRecoveryStatus::Aborted {
                    expected_revision,
                    idempotency_key,
                    ..
                } => (expected_revision, idempotency_key),
            };
            if *revision != fence.current_revision || *key != idempotency_key {
                return Err(SessionError::Cloud);
            }
            Ok(response)
        })
    }
    fn prepare_rom_handoff<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: RomHandoffPrepareRequest,
    ) -> session::CloudFuture<'a, RomHandoffPrepareResponse> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/characters/{}/rom-handoff/prepare",
                    request.character_id
                ))
                .map_err(|_| SessionError::Cloud)?;
            let response: RomHandoffPrepareResponse = self
                .send_json(
                    self.authenticated_with_fence(
                        Method::POST,
                        url,
                        auth,
                        LeaseFence::new(
                            request.session_id,
                            request.character_id,
                            request.expected_revision,
                            request.session_epoch,
                            request.client_instance_id,
                        ),
                    )
                    .map_err(map_cloud_error)?
                    .json(&request),
                    MAX_JSON_RESPONSE_BYTES,
                )
                .await
                .map_err(map_cloud_error)?;
            if response.api_version != coop_cloud::ApiVersion::V1
                || response.destination_save_sha256
                    != coop_cloud::Sha256Digest::of_bytes(&response.destination_save)
            {
                return Err(SessionError::Cloud);
            }
            Ok(response)
        })
    }
    fn commit_rom_handoff<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: RomHandoffCommitRequest,
    ) -> session::CloudFuture<'a, SnapshotRecord> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/characters/{}/rom-handoff/commit",
                    request.character_id
                ))
                .map_err(|_| SessionError::Cloud)?;
            let response = self
                .send_json(
                    self.authenticated_with_fence(
                        Method::POST,
                        url,
                        auth,
                        LeaseFence::new(
                            request.session_id,
                            request.character_id,
                            request.expected_revision,
                            request.session_epoch,
                            request.client_instance_id,
                        ),
                    )
                    .map_err(map_cloud_error)?
                    .json(&request),
                    MAX_JSON_RESPONSE_BYTES,
                )
                .await
                .map_err(map_cloud_error)?;
            validate_committed_handoff(&request, &response)?;
            Ok(response)
        })
    }
    fn abort_rom_handoff<'a>(
        &'a self,
        auth: &'a AuthSession,
        character_id: CharacterId,
        stage_id: coop_cloud::SnapshotId,
    ) -> session::CloudFuture<'a, ()> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/characters/{character_id}/rom-handoff/{stage_id}"
                ))
                .map_err(|_| SessionError::Cloud)?;
            let fence = auth.active_fence().ok_or(SessionError::Lease)?;
            if fence.character_id != character_id {
                return Err(SessionError::Lease);
            }
            self.send_empty(
                self.authenticated_with_fence(Method::DELETE, url, auth, fence)
                    .map_err(map_cloud_error)?,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn group_travel_create(
        &self,
        token: coop_cloud::AccessToken,
        group_id: coop_cloud::GroupId,
        request: coop_cloud::GroupTravelProposalRequest,
    ) -> group_travel::GroupTravelFuture<'_, coop_cloud::GroupTravelProposalView> {
        self.group_travel_create_http(token, group_id, request)
    }

    fn group_travel_current(
        &self,
        token: coop_cloud::AccessToken,
        group_id: coop_cloud::GroupId,
        fence: coop_cloud::LeaseFence,
    ) -> group_travel::GroupTravelFuture<'_, Option<coop_cloud::GroupTravelProposalView>> {
        self.group_travel_current_http(token, group_id, fence)
    }

    fn group_travel_get(
        &self,
        token: coop_cloud::AccessToken,
        group_id: coop_cloud::GroupId,
        proposal_id: coop_cloud::GroupTravelProposalId,
        fence: coop_cloud::LeaseFence,
    ) -> group_travel::GroupTravelFuture<'_, coop_cloud::GroupTravelProposalView> {
        self.group_travel_get_http(token, group_id, proposal_id, fence)
    }

    fn group_travel_action(
        &self,
        token: coop_cloud::AccessToken,
        group_id: coop_cloud::GroupId,
        proposal_id: coop_cloud::GroupTravelProposalId,
        request: coop_cloud::GroupTravelActionRequest,
    ) -> group_travel::GroupTravelFuture<'_, coop_cloud::GroupTravelProposalView> {
        self.group_travel_action_http(token, group_id, proposal_id, request)
    }

    fn online_snapshot(
        &self,
        token: coop_cloud::AccessToken,
        request: coop_cloud::OnlineSnapshotRequest,
    ) -> online::OnlineFuture<'_, coop_cloud::OnlineSnapshotResponse> {
        self.online_snapshot_http(token, request)
    }

    fn online_action(
        &self,
        token: coop_cloud::AccessToken,
        request: coop_cloud::OnlineActionRequest,
    ) -> online::OnlineFuture<'_, coop_cloud::OnlineActionResponse> {
        self.online_action_http(token, request)
    }

    fn acquire<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: AcquireLeaseRequest,
    ) -> session::CloudFuture<'a, LeaseContract> {
        Box::pin(async move {
            let u = self
                .url("v1/sessions/acquire")
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated(Method::POST, u, auth)
                    .map_err(map_cloud_error)?
                    .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_acquire_error)
        })
    }
    fn heartbeat<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: HeartbeatLeaseRequest,
    ) -> session::CloudFuture<'a, LeaseContract> {
        Box::pin(async move {
            let u = self
                .url("v1/sessions/heartbeat")
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(Method::POST, u, auth, request.fence())
                    .map_err(map_cloud_error)?
                    .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn reconnect<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReconnectLeaseRequest,
    ) -> session::CloudFuture<'a, LeaseContract> {
        Box::pin(async move {
            let u = self
                .url("v1/sessions/reconnect")
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(Method::POST, u, auth, request.fence())
                    .map_err(map_cloud_error)?
                    .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn release<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReleaseLeaseRequest,
    ) -> session::CloudFuture<'a, LogoutResponse> {
        Box::pin(async move {
            let u = self
                .url("v1/sessions/release")
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(
                    Method::POST,
                    u,
                    auth,
                    LeaseFence::new(
                        request.session_id,
                        request.character_id,
                        request.current_revision,
                        request.session_epoch,
                        request.client_instance_id,
                    ),
                )
                .map_err(map_cloud_error)?
                .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn resume_package<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        revision: Revision,
    ) -> session::CloudFuture<'a, Option<SignedManifestEnvelope>> {
        Box::pin(async move {
            let u = self
                .url(&format!(
                    "v1/characters/{character}/resume-package?revision={}",
                    revision.value()
                ))
                .map_err(|_| SessionError::Cloud)?;
            let req = match auth.active_fence() {
                Some(fence) => self.authenticated_with_fence(Method::GET, u, auth, fence),
                None => self.authenticated(Method::GET, u, auth),
            }
            .map_err(map_cloud_error)?;
            match req.send().await.map_err(|_| SessionError::Cloud)? {
                r if r.status() == StatusCode::NOT_FOUND => Ok(None),
                r if r.status().is_success() => Ok(Some(
                    serde_json::from_slice(
                        &bounded_body(r, MAX_JSON_RESPONSE_BYTES)
                            .await
                            .map_err(|_| SessionError::Cloud)?,
                    )
                    .map_err(|_| SessionError::Cloud)?,
                )),
                r if r.status() == StatusCode::UNAUTHORIZED => Err(SessionError::Unauthorized),
                _ => Err(SessionError::Cloud),
            }
        })
    }
    fn artifact<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        artifact: CloudArtifactIdentity,
        revision: Revision,
    ) -> session::CloudFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let u = self
                .url(&format!(
                    "v1/characters/{character}/resume-package/artifacts/{}?revision={}",
                    artifact.as_str(),
                    revision.value()
                ))
                .map_err(|_| SessionError::Cloud)?;
            let request = match auth.active_fence() {
                Some(fence) => self.authenticated_with_fence(Method::GET, u, auth, fence),
                None => self.authenticated(Method::GET, u, auth),
            }
            .map_err(map_cloud_error)?;
            let r = request.send().await.map_err(|_| SessionError::Cloud)?;
            if r.status() == StatusCode::NOT_FOUND {
                return Err(SessionError::ArtifactNotFound);
            }
            if !r.status().is_success() {
                return Err(if r.status() == StatusCode::UNAUTHORIZED {
                    SessionError::Unauthorized
                } else {
                    SessionError::Cloud
                });
            }
            bounded_body(r, MAX_RESPONSE_BYTES)
                .await
                .map_err(|_| SessionError::Cloud)
        })
    }
    fn list_snapshots<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotListRequest,
    ) -> session::CloudFuture<'a, SnapshotListResponse> {
        Box::pin(async move {
            let u = self
                .url(&format!(
                    "v1/characters/{}/snapshots?limit={}",
                    request.character_id, request.limit
                ))
                .map_err(|_| SessionError::Cloud)?;
            let r = self
                .authenticated_with_fence(
                    Method::GET,
                    u,
                    auth,
                    LeaseFence::new(
                        request.session_id,
                        request.character_id,
                        Revision::initial(),
                        request.session_epoch,
                        request.client_instance_id,
                    ),
                )
                .map_err(map_cloud_error)?
                .send()
                .await
                .map_err(|_| SessionError::Cloud)?;
            if !r.status().is_success() {
                return Err(if r.status() == StatusCode::UNAUTHORIZED {
                    SessionError::Unauthorized
                } else {
                    SessionError::Cloud
                });
            }
            serde_json::from_slice(
                &bounded_body(r, MAX_JSON_RESPONSE_BYTES)
                    .await
                    .map_err(|_| SessionError::Cloud)?,
            )
            .map_err(|_| SessionError::Cloud)
        })
    }
    fn restore<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotRestoreRequest,
    ) -> session::CloudFuture<'a, SnapshotRestoreResponse> {
        Box::pin(async move {
            let u = self
                .url(&format!("v1/characters/{}/snapshots", request.character_id))
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(
                    Method::POST,
                    u,
                    auth,
                    LeaseFence::new(
                        request.session_id,
                        request.character_id,
                        request.expected_revision,
                        request.session_epoch,
                        request.client_instance_id,
                    ),
                )
                .map_err(map_cloud_error)?
                .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn prepare<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: PrepareSnapshotRequest,
    ) -> session::CloudFuture<'a, SnapshotPrepareResponse> {
        Box::pin(async move {
            let u = self
                .url(&format!(
                    "v1/characters/{}/snapshots/prepare",
                    request.character_id
                ))
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(
                    Method::POST,
                    u,
                    auth,
                    LeaseFence::new(
                        request.session_id,
                        request.character_id,
                        request.expected_parent_revision,
                        request.session_epoch,
                        request.client_instance_id,
                    ),
                )
                .map_err(map_cloud_error)?
                .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
    fn upload<'a>(
        &'a self,
        target: &'a UploadTarget,
        bytes: Vec<u8>,
    ) -> session::CloudFuture<'a, ()> {
        Box::pin(async move {
            target.validate().map_err(|_| SessionError::Cloud)?;
            let u = Url::parse(target.url().as_ref()).map_err(|_| SessionError::Cloud)?;
            if u.scheme() != "https" && !matches!(u.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
            {
                return Err(SessionError::Cloud);
            }
            self.send_empty(self.client.put(u).body(bytes))
                .await
                .map_err(|_| SessionError::Cloud)
        })
    }
    fn finalize<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotFinalizeRequest,
    ) -> session::CloudFuture<'a, SnapshotRecord> {
        Box::pin(async move {
            let u = self
                .url(&format!(
                    "v1/characters/{}/snapshots/finalize",
                    request.character_id
                ))
                .map_err(|_| SessionError::Cloud)?;
            self.send_json(
                self.authenticated_with_fence(
                    Method::POST,
                    u,
                    auth,
                    LeaseFence::new(
                        request.session_id,
                        request.character_id,
                        request.expected_parent_revision,
                        request.session_epoch,
                        request.client_instance_id,
                    ),
                )
                .map_err(map_cloud_error)?
                .json(&request),
                MAX_JSON_RESPONSE_BYTES,
            )
            .await
            .map_err(map_cloud_error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HttpClientError, ReqwestCloudApi, bounded_body, map_acquire_error, map_cloud_error,
    };
    use crate::{AuthError, AuthSession, CloudApi, RefreshTokenStore, SessionError};
    use coop_cloud::{
        AccessToken, ArtifactIdentity as CloudArtifactIdentity, CharacterId, ClientInstanceId,
        HeartbeatLeaseRequest, LeaseContract, LeaseFence, LoginResponse, Password, RefreshFamilyId,
        RefreshToken, RomHandoffCommitRequest, RomHandoffPrepareRequest, RomHandoffPrepareResponse,
        RomHandoffRecoveryStatus, SessionEpoch, SessionId, SnapshotFence, SnapshotFile, SnapshotId,
        SnapshotRecord, UnixTimestampMillis, UploadTarget, UserId,
    };
    use reqwest::StatusCode;
    use std::sync::Arc;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use uuid::Uuid;

    #[test]
    fn endpoint_policy_allows_only_https_or_literal_loopback_http() {
        assert!(ReqwestCloudApi::new("https://cloud.example").is_ok());
        assert!(ReqwestCloudApi::new("http://127.0.0.1:8080").is_ok());
        assert!(ReqwestCloudApi::new("http://[::1]:8080").is_ok());
        for endpoint in [
            "http://localhost:8080",
            "http://127.0.0.2:8080",
            "ftp://127.0.0.1:8080",
            "https://user:password@cloud.example",
            "https://@cloud.example",
            "https://cloud.example/api",
            "https://cloud.example:443",
            "https://cloud.example:0443",
            "https://cloud.example:",
            "https://cloud.example:\\",
            "http://127.0.0.1:80",
            "http://[::1]:80",
            "http://127.0.0.1:",
            "https://cloud.example/?redirect=http://evil",
            "https://cloud.example/#fragment",
        ] {
            assert!(matches!(
                ReqwestCloudApi::new(endpoint),
                Err(HttpClientError::InvalidEndpoint)
            ));
        }
    }

    #[test]
    fn unauthorized_is_preserved_for_exactly_one_auth_retry() {
        assert!(matches!(
            map_cloud_error(HttpClientError::Status(StatusCode::UNAUTHORIZED)),
            SessionError::Unauthorized
        ));
        assert!(matches!(
            map_cloud_error(HttpClientError::Status(StatusCode::FORBIDDEN)),
            SessionError::Cloud
        ));
        assert!(matches!(
            map_cloud_error(HttpClientError::SessionClosed),
            SessionError::Auth(AuthError::SessionClosed)
        ));
    }

    #[test]
    fn acquire_conflict_is_distinct_from_transport_failure() {
        assert!(matches!(
            map_acquire_error(HttpClientError::Status(StatusCode::CONFLICT)),
            SessionError::AcquireConflict
        ));
        assert!(matches!(
            map_acquire_error(HttpClientError::Status(StatusCode::SERVICE_UNAVAILABLE)),
            SessionError::Cloud
        ));
        assert!(matches!(
            map_cloud_error(HttpClientError::Status(StatusCode::CONFLICT)),
            SessionError::Cloud
        ));
    }

    #[derive(Default)]
    struct TestKeychain;

    impl RefreshTokenStore for TestKeychain {
        fn load(
            &self,
            _service: &str,
            _username: &str,
        ) -> Result<Option<RefreshToken>, crate::KeychainError> {
            Ok(None)
        }

        fn store(
            &self,
            _service: &str,
            _username: &str,
            _token: &RefreshToken,
        ) -> Result<(), crate::KeychainError> {
            Ok(())
        }

        fn delete(&self, _service: &str, _username: &str) -> Result<(), crate::KeychainError> {
            Ok(())
        }
    }

    async fn read_http_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
            assert!(request.len() <= 128 * 1024, "request header is bounded");
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header_end = request.len();
        let headers = String::from_utf8_lossy(&request);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        assert!(content_length <= 8 * 1024 * 1024, "request body is bounded");
        let mut body = vec![0_u8; content_length];
        stream.read_exact(&mut body).await.unwrap();
        request.truncate(header_end);
        request.extend_from_slice(&body);
        String::from_utf8_lossy(&request).into_owned()
    }

    async fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) {
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    }

    fn test_ids() -> (
        UserId,
        CharacterId,
        SessionId,
        ClientInstanceId,
        RefreshFamilyId,
    ) {
        (
            UserId::new(Uuid::from_u128(201)).unwrap(),
            CharacterId::new(Uuid::from_u128(202)).unwrap(),
            SessionId::new(Uuid::from_u128(203)).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(204)).unwrap(),
            RefreshFamilyId::new(Uuid::from_u128(205)).unwrap(),
        )
    }

    #[tokio::test]
    async fn loopback_http_routes_fence_and_upload_auth_isolation() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (user_id, character_id, session_id, client_instance_id, family_id) = test_ids();
        let lease = LeaseContract::new(
            LeaseFence::new(
                session_id,
                character_id,
                coop_cloud::Revision::initial(),
                SessionEpoch::new(1).unwrap(),
                client_instance_id,
            ),
            UnixTimestampMillis::new(4_000_000_000_000),
            1_000,
        )
        .unwrap();
        let login = LoginResponse::new(
            user_id,
            character_id,
            AccessToken::new("http-access").unwrap(),
            RefreshToken::new("http-refresh").unwrap(),
            family_id,
            UnixTimestampMillis::new(4_000_000_000_000),
            UnixTimestampMillis::new(4_000_000_100_000),
        )
        .unwrap();
        let login_body = serde_json::to_vec(&login).unwrap();
        let lease_body = serde_json::to_vec(&lease).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with("POST /v1/auth/login HTTP/1.1\r\n"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            write_response(&mut stream, "200 OK", &login_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let lower = request.to_ascii_lowercase();
            assert!(request.starts_with("POST /v1/sessions/heartbeat HTTP/1.1\r\n"));
            assert!(lower.contains("authorization: bearer http-access"));
            assert!(lower.contains(&format!("x-coop-session-id: {session_id}")));
            assert!(lower.contains("x-coop-session-epoch: 1"));
            assert!(lower.contains(&format!("x-coop-client-instance-id: {client_instance_id}")));
            write_response(&mut stream, "200 OK", &lease_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let lower = request.to_ascii_lowercase();
            assert!(request.starts_with("PUT /upload?capability=test HTTP/1.1\r\n"));
            assert!(!lower.contains("authorization:"));
            write_response(&mut stream, "200 OK", b"").await;
        });
        let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{}", address.port())).unwrap();
        let keychain = Arc::new(TestKeychain);
        let auth = AuthSession::login(
            &api,
            keychain.as_ref(),
            "ash",
            Password::new("password").unwrap(),
        )
        .await
        .unwrap();
        let heartbeat = api
            .heartbeat(&auth, HeartbeatLeaseRequest::new(lease.fence()))
            .await
            .unwrap();
        assert_eq!(heartbeat, lease);
        let target = UploadTarget::new_put(
            CloudArtifactIdentity::CharacterSav,
            format!("http://127.0.0.1:{}/upload?capability=test", address.port()),
            UnixTimestampMillis::new(4_000_000_000_000),
        )
        .unwrap();
        api.upload(&target, b"save-bytes".to_vec()).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rom_handoff_prepare_commit_and_abort_use_fenced_character_routes() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (user_id, character_id, session_id, client_instance_id, family_id) = test_ids();
        let stage_id = SnapshotId::new(Uuid::from_u128(206)).unwrap();
        let source_id = SnapshotId::new(Uuid::from_u128(207)).unwrap();
        let prepare_key = coop_cloud::IdempotencyKey::new(Uuid::from_u128(208)).unwrap();
        let digest = coop_cloud::Sha256Digest::of_bytes(b"destination");
        let login = LoginResponse::new(
            user_id,
            character_id,
            AccessToken::new("http-access").unwrap(),
            RefreshToken::new("http-refresh").unwrap(),
            family_id,
            UnixTimestampMillis::new(4_000_000_000_000),
            UnixTimestampMillis::new(4_000_000_100_000),
        )
        .unwrap();
        let response = RomHandoffPrepareResponse {
            api_version: coop_cloud::ApiVersion::V1,
            stage_id,
            destination_world_id: coop_protocol::RomWorldId::new(2).unwrap(),
            arrival_portal_id: "from_previous".to_owned(),
            destination_save_sha256: digest,
            destination_save: b"destination".to_vec(),
        };
        let login_body = serde_json::to_vec(&login).unwrap();
        let mut bad_response = response.clone();
        bad_response.destination_save_sha256 = coop_cloud::Sha256Digest::of_bytes(b"wrong");
        let bad_response_body = serde_json::to_vec(&bad_response).unwrap();
        let response_body = serde_json::to_vec(&response).unwrap();
        let recovery = RomHandoffRecoveryStatus::Aborted {
            stage_id,
            source_snapshot_id: source_id,
            source_world_id: coop_protocol::RomWorldId::new(1).unwrap(),
            expected_revision: coop_cloud::Revision::new(1),
            idempotency_key: prepare_key,
        };
        let recovery_body = serde_json::to_vec(&recovery).unwrap();
        let commit_record = |snapshot_id, parent_revision, revision| {
            SnapshotRecord::new(
                snapshot_id,
                coop_protocol::RomWorldId::new(2).unwrap(),
                SnapshotFence::new(session_id, character_id, SessionEpoch::new(1).unwrap()),
                parent_revision,
                revision,
                vec![
                    SnapshotFile::from_bytes(CloudArtifactIdentity::CharacterSav, b"destination")
                        .unwrap(),
                    SnapshotFile::from_bytes(CloudArtifactIdentity::PendingCommits, b"[]").unwrap(),
                ],
                coop_cloud::Sha256Digest::of_bytes(b"[]"),
                None,
                UnixTimestampMillis::new(4_000_000_000_000),
            )
            .unwrap()
        };
        let wrong_stage_body = serde_json::to_vec(&commit_record(
            SnapshotId::new(Uuid::from_u128(210)).unwrap(),
            coop_cloud::Revision::new(1),
            coop_cloud::Revision::new(2),
        ))
        .unwrap();
        let wrong_revision_body = serde_json::to_vec(&commit_record(
            stage_id,
            coop_cloud::Revision::new(2),
            coop_cloud::Revision::new(3),
        ))
        .unwrap();
        let committed_body = serde_json::to_vec(&commit_record(
            stage_id,
            coop_cloud::Revision::new(1),
            coop_cloud::Revision::new(2),
        ))
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_http_request(&mut stream).await;
            write_response(&mut stream, "200 OK", &login_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let lower = request.to_ascii_lowercase();
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/reconcile HTTP/1.1\r\n"
            )));
            assert!(lower.contains("authorization: bearer http-access"));
            assert!(lower.contains(&format!("x-coop-session-id: {session_id}")));
            assert!(lower.contains("x-coop-session-epoch: 1"));
            assert!(lower.contains(&format!("x-coop-client-instance-id: {client_instance_id}")));
            assert!(request.contains(&format!("\"idempotency_key\":\"{prepare_key}\"")));
            write_response(&mut stream, "200 OK", &recovery_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/prepare HTTP/1.1\r\n"
            )));
            write_response(&mut stream, "200 OK", &bad_response_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let lower = request.to_ascii_lowercase();
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/prepare HTTP/1.1\r\n"
            )));
            assert!(lower.contains("authorization: bearer http-access"));
            assert!(lower.contains(&format!("x-coop-session-id: {session_id}")));
            assert!(lower.contains("x-coop-session-epoch: 1"));
            assert!(lower.contains(&format!("x-coop-client-instance-id: {client_instance_id}")));
            assert!(request.contains("\"portal_id\":\"to_next\""));
            write_response(&mut stream, "200 OK", &response_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/commit HTTP/1.1\r\n"
            )));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer http-access")
            );
            write_response(&mut stream, "200 OK", &wrong_stage_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/commit HTTP/1.1\r\n"
            )));
            write_response(&mut stream, "200 OK", &wrong_revision_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "POST /v1/characters/{character_id}/rom-handoff/commit HTTP/1.1\r\n"
            )));
            write_response(&mut stream, "200 OK", &committed_body).await;

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with(&format!(
                "DELETE /v1/characters/{character_id}/rom-handoff/{stage_id} HTTP/1.1\r\n"
            )));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer http-access")
            );
            let lower = request.to_ascii_lowercase();
            assert!(lower.contains(&format!("x-coop-session-id: {session_id}")));
            assert!(lower.contains("x-coop-session-epoch: 1"));
            assert!(lower.contains(&format!("x-coop-client-instance-id: {client_instance_id}")));
            write_response(&mut stream, "204 No Content", b"").await;
        });
        let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{}", address.port())).unwrap();
        let mut auth = AuthSession::login(
            &api,
            &TestKeychain,
            "ash",
            Password::new("password").unwrap(),
        )
        .await
        .unwrap();
        auth.set_active_fence(coop_cloud::LeaseFence::new(
            session_id,
            character_id,
            coop_cloud::Revision::new(1),
            SessionEpoch::new(1).unwrap(),
            client_instance_id,
        ));
        assert_eq!(
            api.reconcile_rom_handoff(&auth, character_id, prepare_key)
                .await
                .unwrap(),
            recovery
        );
        let request = RomHandoffPrepareRequest {
            api_version: coop_cloud::ApiVersion::V1,
            character_id,
            session_id,
            session_epoch: SessionEpoch::new(1).unwrap(),
            client_instance_id,
            expected_revision: coop_cloud::Revision::new(1),
            source_snapshot_id: source_id,
            portal_id: "to_next".to_owned(),
            idempotency_key: prepare_key,
        };
        assert!(
            api.prepare_rom_handoff(&auth, request.clone())
                .await
                .is_err()
        );
        assert_eq!(
            api.prepare_rom_handoff(&auth, request).await.unwrap(),
            response
        );
        let commit_request = RomHandoffCommitRequest {
            api_version: coop_cloud::ApiVersion::V1,
            character_id,
            session_id,
            session_epoch: SessionEpoch::new(1).unwrap(),
            client_instance_id,
            expected_revision: coop_cloud::Revision::new(1),
            stage_id,
            destination_save_sha256: digest,
            idempotency_key: coop_cloud::IdempotencyKey::new(Uuid::from_u128(208)).unwrap(),
        };
        assert!(matches!(
            api.commit_rom_handoff(
                &auth,
                RomHandoffCommitRequest {
                    stage_id: SnapshotId::new(Uuid::from_u128(211)).unwrap(),
                    ..commit_request.clone()
                }
            )
            .await,
            Err(SessionError::Cloud)
        ));
        assert!(matches!(
            api.commit_rom_handoff(&auth, commit_request.clone()).await,
            Err(SessionError::Cloud)
        ));
        assert_eq!(
            api.commit_rom_handoff(&auth, commit_request)
                .await
                .unwrap()
                .snapshot_id,
            stage_id
        );
        api.abort_rom_handoff(&auth, character_id, stage_id)
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn loopback_http_body_limit_rejects_oversized_content_length() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_http_request(&mut stream).await;
            let response = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd";
            stream.write_all(response).await.unwrap();
            stream.shutdown().await.unwrap();
        });
        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/bounded", address.port()))
            .send()
            .await
            .unwrap();
        assert!(matches!(
            bounded_body(response, 3).await,
            Err(HttpClientError::Response)
        ));
        server.await.unwrap();
    }
}

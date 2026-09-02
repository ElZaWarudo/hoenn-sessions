//! Authenticated, additive Phase 2 application.

use std::{
    collections::HashMap,
    fmt,
    net::SocketAddr,
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, OnceLock},
};

use axum::{
    Json, Router,
    extract::{FromRequest, Path, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use coop_cloud::{
    AcquireLeaseRequest, CharacterId, HeartbeatLeaseRequest, ReconnectLeaseRequest,
    ReleaseLeaseRequest, SnapshotFinalizeRequest, SnapshotListRequest, SnapshotPrepareRequest,
    SnapshotRestoreRequest,
};
use http_body_util::{BodyExt, Limited};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const PASSWORD_HASH_WORKERS: usize = 4;
const PASSWORD_HASH_MAX_WAITERS: usize = 32;
const PASSWORD_HASH_MAX_WAIT_MS: u64 = 1_000;
const MAX_REQUEST_TARGET_BYTES: usize = 8 * 1024;
const MAX_REQUEST_QUERY_BYTES: usize = 2 * 1024;
static PASSWORD_HASH_LIMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
static PASSWORD_HASH_WAITERS: AtomicUsize = AtomicUsize::new(0);

fn password_hash_limit() -> Arc<tokio::sync::Semaphore> {
    PASSWORD_HASH_LIMIT
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(PASSWORD_HASH_WORKERS)))
        .clone()
}

struct PasswordWaiter;

impl Drop for PasswordWaiter {
    fn drop(&mut self) {
        PASSWORD_HASH_WAITERS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn admit_password_waiter() -> Result<PasswordWaiter, Phase2Error> {
    PASSWORD_HASH_WAITERS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < PASSWORD_HASH_MAX_WAITERS).then_some(count + 1)
        })
        .map(|_| PasswordWaiter)
        .map_err(|_| Phase2Error::Busy)
}

async fn run_password_operation<T, F>(operation: F) -> Result<T, Phase2Error>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, Phase2Error> + Send + 'static,
{
    let waiter = admit_password_waiter()?;
    let permit = tokio::time::timeout(
        std::time::Duration::from_millis(PASSWORD_HASH_MAX_WAIT_MS),
        password_hash_limit().acquire_owned(),
    )
    .await
    .map_err(|_| Phase2Error::Busy)?
    .map_err(|_| Phase2Error::Internal)?;
    drop(waiter);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await
    .map_err(|_| Phase2Error::Internal)?
}
use thiserror::Error;

pub mod auth;
pub(crate) mod group_travel;
pub mod presence;
pub(crate) mod realtime;
pub mod saves;
pub mod sessions;
pub mod storage;

pub use presence::{
    PRESENCE_HANDLE_CANDIDATES, PRESENCE_MAP, PRESENCE_MAP_GROUP, PRESENCE_MAP_NUMBER,
    PRESENCE_MAX_GLOBAL_CONNECTIONS, PRESENCE_MAX_PARTITION_CONNECTIONS, PRESENCE_MAX_REMOTES,
    PRESENCE_OUTBOUND_QUEUE_CAPACITY, PRESENCE_SHARD_ID, PRESENCE_STALE_MS, PRESENCE_TICK_MS,
    PresenceConnection, PresenceDrain, PresenceOutboundV1, PresenceService, PresenceServiceError,
    PresenceSubmitOutcome, PresenceTickReport, ValidatedInteraction,
};
pub use storage::{
    ArgonPasswordEngine, Clock, Entropy, FirebaseObjectStore, InMemoryObjectStore,
    InMemoryRepository, ObjectStore, OsEntropy, PasswordEngine, Phase2Config, PostgresRepository,
    ProductionConfig, Repository, StorageMode, SystemClock,
};
#[cfg(test)]
pub use storage::{FixedClock, FixedEntropy};
use storage::{StorageError, Store};

/// The authenticated actor resolved from a server-owned bearer token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedActor {
    pub(crate) user_id: coop_cloud::UserId,
    pub(crate) character_id: CharacterId,
}

/// Stable public errors.  Internal storage details are deliberately not
/// included in the response or formatting of this type.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Phase2Error {
    #[error("invalid request")]
    InvalidRequest,
    #[error("authentication failed")]
    Authentication,
    #[error("resource not found")]
    NotFound,
    #[error("request conflicts with current state")]
    Conflict,
    #[error("operation is not permitted")]
    Forbidden,
    #[error("request expired")]
    Expired,
    #[error("request body is too large")]
    PayloadTooLarge,
    #[error("service is busy")]
    Busy,
    #[error("internal service error")]
    Internal,
}

impl From<StorageError> for Phase2Error {
    fn from(_: StorageError) -> Self {
        Self::Internal
    }
}

impl IntoResponse for Phase2Error {
    fn into_response(self) -> Response {
        let status = match self {
            Self::InvalidRequest => StatusCode::BAD_REQUEST,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Busy => StatusCode::SERVICE_UNAVAILABLE,
            Self::Authentication | Self::Expired => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: ErrorCode { code: self.code() },
            }),
        )
            .into_response()
    }
}

impl Phase2Error {
    const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Authentication => "authentication_failed",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Forbidden => "forbidden",
            Self::Expired => "expired",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Busy => "service_busy",
            Self::Internal => "internal_error",
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorCode,
}
#[derive(Serialize)]
struct ErrorCode {
    code: &'static str,
}

/// A cloneable Phase 2 service handle.
#[derive(Clone)]
pub struct Phase2App {
    pub(crate) store: Store,
    presence: presence::PresenceService,
    realtime: Arc<realtime::RealtimeTransportState>,
}

impl fmt::Debug for Phase2App {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Phase2App([REDACTED])")
    }
}

impl Phase2App {
    /// Creates a Phase 2 service from explicit local adapter configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when a production adapter mode is requested before it
    /// has an implementation.
    pub fn new(config: Phase2Config) -> Result<Self, Phase2Error> {
        let store = Store::new(config)?;
        let presence = presence::PresenceService::new(store.clone())?;
        Ok(Self {
            store,
            presence,
            realtime: Arc::new(realtime::RealtimeTransportState::new()),
        })
    }

    /// Returns the ephemeral presence service shared by all clones of this
    /// application. Presence is intentionally not restored from a repository.
    #[must_use]
    pub fn presence(&self) -> presence::PresenceService {
        self.presence.clone()
    }

    /// A deterministic test service.  Production callers must use `new` with
    /// externally supplied secrets and the normal OS entropy adapters.
    /// Creates a deterministic convenience service for unit tests.
    ///
    /// # Panics
    ///
    /// Panics only if the built-in test configuration violates its own
    /// invariants.
    #[must_use]
    #[cfg(test)]
    pub fn test() -> Self {
        let config = Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ));
        Self::new(config).expect("test config is local")
    }

    pub fn router(&self) -> Router {
        Router::new()
            .merge(
                Router::new()
                    .route("/v1/auth/register", post(register))
                    .route("/v1/auth/login", post(login))
                    .route("/v1/auth/refresh", post(refresh))
                    .route("/v1/auth/logout", post(logout))
                    .route("/v1/sessions/acquire", post(acquire))
                    .route("/v1/sessions/heartbeat", post(heartbeat))
                    .route("/v1/sessions/reconnect", post(reconnect))
                    .route("/v1/sessions/release", post(release))
                    .layer(axum::extract::DefaultBodyLimit::max(8 * 1024)),
            )
            .merge(
                Router::new()
                    .route(
                        "/v1/characters/{character_id}/snapshots/prepare",
                        post(prepare),
                    )
                    .route(
                        "/v1/characters/{character_id}/snapshots/finalize",
                        post(finalize),
                    )
                    .route(
                        "/v1/characters/{character_id}/snapshots",
                        post(restore).get(list),
                    )
                    .route(
                        "/v1/characters/{character_id}/restore/{revision}",
                        post(restore_at),
                    )
                    .layer(axum::extract::DefaultBodyLimit::max(64 * 1024)),
            )
            .merge(
                Router::new()
                    .route(
                        "/v1/characters/{character_id}/resume-package",
                        get(resume_package),
                    )
                    .route(
                        "/v1/characters/{character_id}/resume-package/artifacts/{artifact}",
                        get(resume_artifact),
                    ),
            )
            .merge(
                Router::new()
                    .route("/v1/uploads/{ticket}", put(upload))
                    .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024)),
            )
            .merge(
                Router::new()
                    .route("/v1/groups/invitations", post(create_group_invitation))
                    .route(
                        "/v1/groups/invitations/{invitation_id}/accept",
                        post(accept_group_invitation),
                    )
                    .route("/v1/groups/{group_id}", get(inspect_group))
                    .route("/v1/groups/{group_id}/travel", post(travel_group))
                    .layer(axum::extract::DefaultBodyLimit::max(
                        coop_cloud::GROUP_REQUEST_BODY_MAX_BYTES,
                    ))
                    .layer(axum::middleware::from_fn(group_no_store)),
            )
            .merge(realtime::router())
            .layer(axum::middleware::from_fn(reject_oversized_request_target))
            .with_state(self.clone())
    }

    /// Registers one invite-gated account and its main character.
    ///
    /// # Errors
    ///
    /// Returns a generic authentication error for invalid or reused invites.
    pub fn register(
        &self,
        request: coop_cloud::RegisterRequest,
    ) -> Result<coop_cloud::RegisterResponse, Phase2Error> {
        if !auth::registration_admissible(&self.store, &request)? {
            return Err(Phase2Error::Authentication);
        }
        let _admission = self
            .store
            .try_account_admission(request.username.as_str())?
            .ok_or(Phase2Error::Busy)?;
        let result = auth::register(&self.store, &request);
        drop(request);
        result
    }
    /// Authenticates a user and issues opaque access and refresh values.
    ///
    /// # Errors
    ///
    /// Returns a generic authentication error for invalid credentials.
    pub fn login(
        &self,
        request: coop_cloud::LoginRequest,
    ) -> Result<coop_cloud::LoginResponse, Phase2Error> {
        let result = auth::login(&self.store, &request);
        drop(request);
        result
    }
    /// Atomically rotates a refresh family generation.
    ///
    /// # Errors
    ///
    /// Returns a generic authentication error for an invalid or reused token.
    pub fn refresh(
        &self,
        request: coop_cloud::RefreshRequest,
    ) -> Result<coop_cloud::RefreshResponse, Phase2Error> {
        let result = auth::refresh(&self.store, &request);
        drop(request);
        result
    }
    /// Revokes a refresh family idempotently.
    ///
    /// # Errors
    ///
    /// Returns an internal error only if the local state cannot be accessed.
    pub fn logout(
        &self,
        request: coop_cloud::LogoutRequest,
    ) -> Result<coop_cloud::LogoutResponse, Phase2Error> {
        let result = auth::logout(&self.store, &request);
        drop(request);
        result
    }
    /// Acquires an exclusive server-issued character lease.
    ///
    /// # Errors
    ///
    /// Returns an ownership, conflict, or infrastructure error.
    pub fn acquire(
        &self,
        actor: AuthenticatedActor,
        request: AcquireLeaseRequest,
    ) -> Result<coop_cloud::LeaseContract, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        let result = sessions::acquire(&self.store, actor, &request);
        if let Ok(contract) = &result {
            self.presence
                .reconcile_lease_success(actor.character_id, contract);
        }
        result
    }
    /// Extends an active lease after validating its complete fence.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, expiry, or infrastructure error.
    pub fn heartbeat(
        &self,
        actor: AuthenticatedActor,
        request: HeartbeatLeaseRequest,
    ) -> Result<coop_cloud::LeaseContract, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        let result = sessions::heartbeat(&self.store, actor, &request);
        if let Ok(contract) = &result {
            self.presence
                .reconcile_lease_success(actor.character_id, contract);
        }
        result
    }
    /// Reconnects during grace while rotating the server-owned epoch.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, grace, or infrastructure error.
    pub fn reconnect(
        &self,
        actor: AuthenticatedActor,
        request: ReconnectLeaseRequest,
    ) -> Result<coop_cloud::LeaseContract, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        let result = sessions::reconnect(&self.store, actor, &request);
        if let Ok(contract) = &result {
            self.presence
                .reconcile_lease_success(actor.character_id, contract);
        }
        result
    }
    /// Releases a fenced lease idempotently.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, or infrastructure error.
    pub fn release(
        &self,
        actor: AuthenticatedActor,
        request: ReleaseLeaseRequest,
    ) -> Result<coop_cloud::LogoutResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        let result = sessions::release(&self.store, actor, &request);
        if result.is_ok() {
            self.presence.reconcile_lease_release(actor.character_id);
        }
        result
    }
    /// Prepares fixed artifact uploads for the next revision.
    ///
    /// # Errors
    ///
    /// Returns an invalid declaration, stale fence, or infrastructure error.
    pub fn prepare(
        &self,
        actor: AuthenticatedActor,
        request: SnapshotPrepareRequest,
    ) -> Result<coop_cloud::SnapshotPrepareResponse, Phase2Error> {
        saves::prepare(&self.store, actor, request)
    }
    /// Accepts one capability-authorized artifact upload.
    ///
    /// # Errors
    ///
    /// Returns an invalid, expired, or already-consumed capability error.
    pub fn upload(&self, ticket: &str, body: Vec<u8>) -> Result<(), Phase2Error> {
        saves::upload(&self.store, ticket, body)
    }
    /// Finalizes an uploaded snapshot and advances the canonical revision.
    ///
    /// # Errors
    ///
    /// Returns a stale fence, incomplete upload, CAS, or infrastructure error.
    pub fn finalize(
        &self,
        actor: AuthenticatedActor,
        request: SnapshotFinalizeRequest,
    ) -> Result<coop_cloud::SnapshotRecord, Phase2Error> {
        let result = saves::finalize(&self.store, actor, &request);
        drop(request);
        result
    }
    /// Lists bounded finalized snapshots under the active lease fence.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, or infrastructure error.
    pub fn list(
        &self,
        actor: AuthenticatedActor,
        request: SnapshotListRequest,
    ) -> Result<coop_cloud::SnapshotListResponse, Phase2Error> {
        saves::list(&self.store, actor, request)
    }
    /// Restores a finalized snapshot into a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, CAS, or infrastructure error.
    pub fn restore(
        &self,
        actor: AuthenticatedActor,
        request: &SnapshotRestoreRequest,
    ) -> Result<coop_cloud::SnapshotRestoreResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        validate_restore_target(&self.store, actor, request.character_id)?;
        saves::restore(&self.store, actor, request)
    }

    /// Restores a selected finalized snapshot under the same runtime
    /// transition boundary as ordinary restore.
    ///
    /// # Errors
    ///
    /// Returns an ownership, stale-fence, CAS, or infrastructure error.
    pub fn restore_at(
        &self,
        actor: AuthenticatedActor,
        request: &SnapshotRestoreRequest,
        source_revision: u64,
    ) -> Result<coop_cloud::SnapshotRestoreResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        validate_restore_target(&self.store, actor, request.character_id)?;
        saves::restore_at(&self.store, actor, request, source_revision)
    }
    /// Adds a one-use invitation during local bootstrap.
    ///
    /// # Errors
    ///
    /// Returns an infrastructure error if local state cannot be accessed.
    pub fn add_invitation(&self, code: &str) -> Result<(), Phase2Error> {
        auth::add_invitation(&self.store, code)
    }

    /// Creates one pending invitation under the caller's active lease.
    ///
    /// # Errors
    ///
    /// Returns an authentication, policy, conflict, capacity, or storage error.
    pub fn create_group_invitation(
        &self,
        actor: AuthenticatedActor,
        request: coop_cloud::CreateGroupInvitationRequest,
    ) -> Result<coop_cloud::CreateGroupInvitationResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        group_travel::create_invitation(&self.store, actor, &request)
    }

    /// Consumes an invitation and creates a symmetric group atomically.
    ///
    /// # Errors
    ///
    /// Returns an authentication, policy, conflict, capacity, or storage error.
    pub fn accept_group_invitation(
        &self,
        actor: AuthenticatedActor,
        invitation_id: coop_cloud::GroupInvitationId,
        request: coop_cloud::AcceptGroupInvitationRequest,
    ) -> Result<coop_cloud::AcceptGroupInvitationResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        group_travel::accept_invitation(&self.store, actor, invitation_id, &request)
    }

    /// Returns an active group owned by the caller and fenced to its lease.
    ///
    /// # Errors
    ///
    /// Returns a hidden-not-found, fence, or storage error.
    pub fn inspect_group(
        &self,
        actor: AuthenticatedActor,
        group_id: coop_cloud::GroupId,
        fence: coop_cloud::LeaseFence,
    ) -> Result<coop_cloud::GroupView, Phase2Error> {
        group_travel::inspect_group(&self.store, actor, group_id, fence)
    }

    /// Moves both members through one server-owned route atomically.
    ///
    /// # Errors
    ///
    /// Returns an authentication, policy, conflict, capacity, or storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "public operation consumes the request at the service boundary"
    )]
    pub fn travel_group(
        &self,
        actor: AuthenticatedActor,
        group_id: coop_cloud::GroupId,
        request: coop_cloud::GroupTravelRequest,
    ) -> Result<coop_cloud::GroupTravelResponse, Phase2Error> {
        let _gate = self
            .store
            .runtime_transition_gate
            .lock()
            .map_err(|_| Phase2Error::Internal)?;
        group_travel::travel(&self.store, actor, group_id, &request)
    }
}

fn phase2_app_from_values(
    pepper: &str,
    signing: &str,
    key_id: &str,
    bootstrap: &str,
    upload_base_url: String,
) -> Result<Phase2App, Phase2Error> {
    let signing =
        coop_cloud::SigningPrivateKey::parse_hex(signing).map_err(|_| Phase2Error::Internal)?;
    let invitation =
        coop_cloud::InvitationCode::new(bootstrap).map_err(|_| Phase2Error::Internal)?;
    let config = Phase2Config::local(pepper.as_bytes().to_vec(), signing, key_id)?
        .with_upload_base_url(upload_base_url);
    let app = Phase2App::new(config)?;
    app.add_invitation(invitation.expose_secret())?;
    Ok(app)
}

fn validate_restore_target(
    store: &Store,
    actor: AuthenticatedActor,
    character_id: CharacterId,
) -> Result<(), Phase2Error> {
    store.read_transaction(|state| {
        let user = state
            .users_by_id
            .get(&actor.user_id)
            .ok_or(Phase2Error::Authentication)?;
        if user.user_id != actor.user_id || user.disabled {
            return Err(Phase2Error::Authentication);
        }
        let character = state
            .characters
            .get(&character_id)
            .ok_or(Phase2Error::NotFound)?;
        if actor.character_id != character_id
            || user.character_id != character_id
            || character.owner != actor.user_id
        {
            return Err(Phase2Error::NotFound);
        }
        if character.state.character_id != character_id {
            return Err(Phase2Error::Internal);
        }
        if state.active_group_by_member.contains_key(&character_id) {
            Err(Phase2Error::Conflict)
        } else {
            Ok(())
        }
    })
}

fn phase2_app_from_env(upload_base_url: String) -> Result<Phase2App, Phase2Error> {
    if let Ok(mode) = std::env::var("COOP_PHASE2_STORAGE_MODE")
        && !mode.eq_ignore_ascii_case("phase2-local")
    {
        return Err(Phase2Error::Internal);
    }
    let pepper = zeroize::Zeroizing::new(
        std::env::var("COOP_PHASE2_INVITE_PEPPER").map_err(|_| Phase2Error::Internal)?,
    );
    let signing = zeroize::Zeroizing::new(
        std::env::var("COOP_PHASE2_SIGNING_KEY_HEX").map_err(|_| Phase2Error::Internal)?,
    );
    let key_id = zeroize::Zeroizing::new(
        std::env::var("COOP_PHASE2_SIGNING_KEY_ID").map_err(|_| Phase2Error::Internal)?,
    );
    let bootstrap = zeroize::Zeroizing::new(
        std::env::var("COOP_PHASE2_BOOTSTRAP_INVITATION").map_err(|_| Phase2Error::Internal)?,
    );
    phase2_app_from_values(
        pepper.as_str(),
        signing.as_str(),
        key_id.as_str(),
        bootstrap.as_str(),
        upload_base_url,
    )
}

fn loopback_upload_base(address: SocketAddr) -> String {
    format!("http://{address}")
}

/// Runs the explicitly selected in-memory Phase 2 service on a loopback
/// listener. Port zero is supported; upload capabilities use the actual
/// listener address selected by the operating system.
///
/// # Errors
///
/// Returns a stable error when the address is not loopback, binding fails, or
/// required local secret configuration is missing or invalid.
pub async fn serve_phase2_local(address: SocketAddr) -> Result<(), Phase2Error> {
    if !address.ip().is_loopback() {
        return Err(Phase2Error::InvalidRequest);
    }
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|_| Phase2Error::Internal)?;
    let bound = listener.local_addr().map_err(|_| Phase2Error::Internal)?;
    let app = phase2_app_from_env(loopback_upload_base(bound))?;
    axum::serve(listener, app.router())
        .await
        .map_err(|_| Phase2Error::Internal)
}

#[derive(Deserialize)]
struct CharacterPath {
    character_id: CharacterId,
}
#[derive(Deserialize)]
struct ArtifactPath {
    character_id: CharacterId,
    artifact: String,
}
#[derive(Deserialize)]
struct RevisionQuery {
    revision: Option<u64>,
}
#[derive(Deserialize)]
struct ListQuery {
    limit: Option<u16>,
}
#[derive(Deserialize)]
struct RestorePath {
    character_id: CharacterId,
    revision: u64,
}

/// JSON extractor with a stable Phase 2 error envelope.  Route-level body
/// limits reject oversized input before deserialization; this adapter maps
/// that rejection without exposing framework internals.
struct Phase2Json<T>(T);

async fn reject_oversized_request_target(request: Request, next: Next) -> Response {
    let uri = request.uri();
    let query_len = uri.query().map_or(0, str::len);
    let target_len = uri
        .path()
        .len()
        .saturating_add(usize::from(uri.query().is_some()))
        .saturating_add(query_len);
    if query_len > MAX_REQUEST_QUERY_BYTES || target_len > MAX_REQUEST_TARGET_BYTES {
        let mut response = Phase2Error::PayloadTooLarge.into_response();
        if matches!(uri.path(), "/v1/realtime" | "/v1/realtime/tickets")
            || uri.path() == "/v1/groups"
            || uri.path().starts_with("/v1/groups/")
        {
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        }
        return response;
    }
    next.run(request).await
}

impl<S, T> FromRequest<S> for Phase2Json<T>
where
    S: Send + Sync,
    T: DeserializeOwned + 'static,
{
    type Rejection = Phase2Error;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| {
                if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    Phase2Error::PayloadTooLarge
                } else {
                    Phase2Error::InvalidRequest
                }
            })
    }
}

async fn register(
    State(app): State<Phase2App>,
    Phase2Json(request): Phase2Json<coop_cloud::RegisterRequest>,
) -> Result<(StatusCode, Json<coop_cloud::RegisterResponse>), Phase2Error> {
    if !auth::registration_admissible(&app.store, &request)? {
        return Err(Phase2Error::Authentication);
    }
    let response = run_password_operation(move || app.register(request)).await?;
    Ok((StatusCode::CREATED, Json(response)))
}
async fn login(
    State(app): State<Phase2App>,
    Phase2Json(request): Phase2Json<coop_cloud::LoginRequest>,
) -> Result<Json<coop_cloud::LoginResponse>, Phase2Error> {
    let admission = app
        .store
        .try_account_admission(request.username.as_str())?
        .ok_or(Phase2Error::Busy)?;
    let response = run_password_operation(move || {
        let _admission = admission;
        app.login(request)
    })
    .await?;
    Ok(Json(response))
}
async fn refresh(
    State(app): State<Phase2App>,
    Phase2Json(request): Phase2Json<coop_cloud::RefreshRequest>,
) -> Result<Json<coop_cloud::RefreshResponse>, Phase2Error> {
    Ok(Json(app.refresh(request)?))
}
async fn logout(
    State(app): State<Phase2App>,
    Phase2Json(request): Phase2Json<coop_cloud::LogoutRequest>,
) -> Result<Json<coop_cloud::LogoutResponse>, Phase2Error> {
    Ok(Json(app.logout(request)?))
}

fn actor(
    headers: &axum::http::HeaderMap,
    app: &Phase2App,
) -> Result<AuthenticatedActor, Phase2Error> {
    auth::actor_from_headers(&app.store, headers)
}
async fn acquire(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Phase2Json(request): Phase2Json<AcquireLeaseRequest>,
) -> Result<Json<coop_cloud::LeaseContract>, Phase2Error> {
    Ok(Json(app.acquire(actor(&headers, &app)?, request)?))
}
async fn heartbeat(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Phase2Json(request): Phase2Json<HeartbeatLeaseRequest>,
) -> Result<Json<coop_cloud::LeaseContract>, Phase2Error> {
    Ok(Json(app.heartbeat(actor(&headers, &app)?, request)?))
}
async fn reconnect(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Phase2Json(request): Phase2Json<ReconnectLeaseRequest>,
) -> Result<Json<coop_cloud::LeaseContract>, Phase2Error> {
    Ok(Json(app.reconnect(actor(&headers, &app)?, request)?))
}
async fn release(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Phase2Json(request): Phase2Json<ReleaseLeaseRequest>,
) -> Result<Json<coop_cloud::LogoutResponse>, Phase2Error> {
    Ok(Json(app.release(actor(&headers, &app)?, request)?))
}
async fn prepare(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<CharacterPath>,
    Phase2Json(request): Phase2Json<SnapshotPrepareRequest>,
) -> Result<Json<coop_cloud::SnapshotPrepareResponse>, Phase2Error> {
    if request.character_id != path.character_id {
        return Err(Phase2Error::NotFound);
    }
    Ok(Json(app.prepare(actor(&headers, &app)?, request)?))
}
async fn finalize(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<CharacterPath>,
    Phase2Json(request): Phase2Json<SnapshotFinalizeRequest>,
) -> Result<Json<coop_cloud::SnapshotRecord>, Phase2Error> {
    if request.character_id != path.character_id {
        return Err(Phase2Error::NotFound);
    }
    Ok(Json(app.finalize(actor(&headers, &app)?, request)?))
}
async fn list(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<CharacterPath>,
    Query(query): Query<ListQuery>,
) -> Result<Json<coop_cloud::SnapshotListResponse>, Phase2Error> {
    let actor = actor(&headers, &app)?;
    let fence = auth::fence_from_headers(&headers, path.character_id, &app)?;
    let request = SnapshotListRequest::new(
        fence.session_id,
        path.character_id,
        fence.session_epoch,
        fence.client_instance_id,
        query.limit.map_or(20, |v| v.min(100)),
    )
    .map_err(|_| Phase2Error::InvalidRequest)?;
    Ok(Json(app.list(actor, request)?))
}
async fn restore(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<CharacterPath>,
    Phase2Json(request): Phase2Json<SnapshotRestoreRequest>,
) -> Result<Json<coop_cloud::SnapshotRestoreResponse>, Phase2Error> {
    if request.character_id != path.character_id {
        return Err(Phase2Error::NotFound);
    }
    Ok(Json(app.restore(actor(&headers, &app)?, &request)?))
}
async fn restore_at(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<RestorePath>,
    Phase2Json(request): Phase2Json<SnapshotRestoreRequest>,
) -> Result<Json<coop_cloud::SnapshotRestoreResponse>, Phase2Error> {
    if request.character_id != path.character_id {
        return Err(Phase2Error::NotFound);
    }
    let actor = actor(&headers, &app)?;
    Ok(Json(app.restore_at(actor, &request, path.revision)?))
}
async fn upload(
    State(app): State<Phase2App>,
    Path(path): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    request: Request,
) -> Result<StatusCode, Phase2Error> {
    if query.len() != 1 {
        return Err(Phase2Error::Authentication);
    }
    let query_ticket = query.get("ticket").ok_or(Phase2Error::Authentication)?;
    let limit = saves::upload_limit(&app.store, &path, query_ticket)?;
    let body = Limited::new(request.into_body(), limit)
        .collect()
        .await
        .map_err(|_| Phase2Error::PayloadTooLarge)?
        .to_bytes();
    saves::upload_with_credential(&app.store, &path, query_ticket, body.to_vec())?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct GroupPath {
    group_id: coop_cloud::GroupId,
}

#[derive(Deserialize)]
struct GroupInvitationPath {
    invitation_id: coop_cloud::GroupInvitationId,
}

async fn group_no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn create_group_invitation(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Phase2Json(request): Phase2Json<coop_cloud::CreateGroupInvitationRequest>,
) -> Result<(StatusCode, Json<coop_cloud::CreateGroupInvitationResponse>), Phase2Error> {
    let response = app.create_group_invitation(actor(&headers, &app)?, request)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn accept_group_invitation(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<GroupInvitationPath>,
    Phase2Json(request): Phase2Json<coop_cloud::AcceptGroupInvitationRequest>,
) -> Result<Json<coop_cloud::AcceptGroupInvitationResponse>, Phase2Error> {
    let response =
        app.accept_group_invitation(actor(&headers, &app)?, path.invitation_id, request)?;
    Ok(Json(response))
}

async fn inspect_group(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<GroupPath>,
) -> Result<Json<coop_cloud::GroupView>, Phase2Error> {
    let actor = actor(&headers, &app)?;
    let fence = auth::fence_from_headers(&headers, actor.character_id, &app)?;
    Ok(Json(app.inspect_group(actor, path.group_id, fence)?))
}

async fn travel_group(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<GroupPath>,
    Phase2Json(request): Phase2Json<coop_cloud::GroupTravelRequest>,
) -> Result<Json<coop_cloud::GroupTravelResponse>, Phase2Error> {
    Ok(Json(app.travel_group(
        actor(&headers, &app)?,
        path.group_id,
        request,
    )?))
}

async fn resume_package(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<CharacterPath>,
    Query(query): Query<RevisionQuery>,
) -> Result<Json<coop_cloud::SignedManifestEnvelope>, Phase2Error> {
    let actor = actor(&headers, &app)?;
    let fence = auth::fence_from_headers(&headers, path.character_id, &app)?;
    Ok(Json(saves::resume_package(
        &app.store,
        actor,
        fence,
        query.revision,
    )?))
}
async fn resume_artifact(
    State(app): State<Phase2App>,
    headers: axum::http::HeaderMap,
    Path(path): Path<ArtifactPath>,
    Query(query): Query<RevisionQuery>,
) -> Result<axum::response::Response, Phase2Error> {
    let actor = actor(&headers, &app)?;
    let fence = auth::fence_from_headers(&headers, path.character_id, &app)?;
    let bytes = saves::resume_artifact(&app.store, actor, fence, &path.artifact, query.revision)?;
    Ok((StatusCode::OK, bytes).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{
        ArtifactIdentity, ClientInstanceId, IdempotencyKey, InvitationCode, LeaseFence,
        LoginRequest, LogoutRequest, LogoutResponse, Password, ReconnectLeaseRequest,
        RefreshRequest, RegisterRequest, Revision, SessionEpoch, SessionId, SignedManifestEnvelope,
        SigningPrivateKey, SnapshotFile, SnapshotFinalizeFence, SnapshotId, SnapshotListRequest,
        SnapshotPrepareFence, SnapshotRestoreRequest, TrustedManifestKey,
    };
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use std::time::Duration;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn id<T>(constructor: fn(Uuid) -> Result<T, coop_cloud::IdError>) -> T {
        constructor(Uuid::new_v4()).expect("non-nil test UUID")
    }

    fn password() -> Password {
        Password::new("correct horse battery staple").expect("password")
    }

    const TEST_SECTOR_ID_OFFSET: usize = 4_084;
    const TEST_SECTOR_CHECKSUM_OFFSET: usize = 4_086;
    const TEST_SECTOR_SIGNATURE_OFFSET: usize = 4_088;
    const TEST_SECTOR_COUNTER_OFFSET: usize = 4_092;
    const TEST_SECTOR_SIGNATURE: u32 = 0x0801_2025;

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn character_sav_with_generation(
        with_rtc: bool,
        generation: u32,
        status_flags: u32,
    ) -> Vec<u8> {
        let mut payload = [0_u8; coop_save::COOP_SAVE_V1_SIZE];
        write_u32(&mut payload, 0, coop_save::COOP_SAVE_V1_MAGIC);
        write_u16(&mut payload, 4, coop_save::COOP_SAVE_V1_SCHEMA_VERSION);
        write_u16(
            &mut payload,
            6,
            u16::try_from(coop_save::COOP_SAVE_V1_SIZE).expect("CSP1 size fits u16"),
        );
        write_u32(&mut payload, 8, coop_protocol::IDENTITY_REGISTRY_VERSION);
        payload[12..28].copy_from_slice(&coop_protocol::IDENTITY_REGISTRY_DIGEST);
        write_u32(&mut payload, 28, generation);
        write_u32(&mut payload, 32, status_flags);
        for (index, region) in [1_u8, 2, 3, 4].into_iter().enumerate() {
            let offset = 36 + index * 8;
            payload[offset] = region;
            // Registry v1 intentionally assigns no Sevii badges.
            write_u16(
                &mut payload,
                offset + 2,
                if index == 3 { 0 } else { 1 << index },
            );
            write_u32(
                &mut payload,
                offset + 4,
                100 + u32::try_from(index).expect("regional fixture index fits u32"),
            );
        }
        let crc = crc32fast::hash(&payload[..668]);
        write_u32(&mut payload, 668, crc);

        let mut save_block3 = [0xff; coop_save::SAVE_BLOCK3_CAPACITY];
        save_block3[coop_save::COOP_SAVE_OFFSET
            ..coop_save::COOP_SAVE_OFFSET + coop_save::COOP_SAVE_V1_SIZE]
            .copy_from_slice(&payload);
        let mut bytes = vec![0xff; coop_save::FLASH_IMAGE_SIZE];
        for (slot, counter, rotation) in [(0_usize, 20_u32, 4_usize), (1, 21, 11)] {
            for physical in 0..coop_save::SECTORS_PER_SLOT {
                let logical = (physical + rotation) % coop_save::SECTORS_PER_SLOT;
                let start =
                    (slot * coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                let sector = &mut bytes[start..start + coop_save::SECTOR_SIZE];
                let source = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
                sector[coop_save::SAVE_BLOCK3_CHUNK_OFFSET
                    ..coop_save::SAVE_BLOCK3_CHUNK_OFFSET + coop_save::SAVE_BLOCK3_CHUNK_SIZE]
                    .copy_from_slice(
                        &save_block3[source..source + coop_save::SAVE_BLOCK3_CHUNK_SIZE],
                    );
                write_u16(
                    sector,
                    TEST_SECTOR_ID_OFFSET,
                    u16::try_from(logical).expect("logical sector fits u16"),
                );
                let checksum = coop_save::sector_checksum(
                    &sector[..coop_save::LOGICAL_SECTOR_DATA_SIZES[logical]],
                );
                write_u16(sector, TEST_SECTOR_CHECKSUM_OFFSET, checksum);
                write_u32(sector, TEST_SECTOR_SIGNATURE_OFFSET, TEST_SECTOR_SIGNATURE);
                write_u32(sector, TEST_SECTOR_COUNTER_OFFSET, counter);
            }
        }
        if with_rtc {
            bytes
                .extend(0_u8..u8::try_from(coop_save::RTC_TRAILER_SIZE).expect("RTC size fits u8"));
        }
        bytes
    }

    fn character_sav_with_status(with_rtc: bool, status_flags: u32) -> Vec<u8> {
        character_sav_with_generation(with_rtc, 1, status_flags)
    }

    fn valid_character_sav(with_rtc: bool) -> Vec<u8> {
        character_sav_with_status(with_rtc, 0)
    }

    fn valid_character_sav_generation(with_rtc: bool, generation: u32) -> Vec<u8> {
        character_sav_with_generation(with_rtc, generation, 0)
    }

    fn mutate_selected_coop_byte(bytes: &mut [u8], payload_offset: usize) {
        let save_block3_offset = coop_save::COOP_SAVE_OFFSET + payload_offset;
        let logical = save_block3_offset / coop_save::SAVE_BLOCK3_CHUNK_SIZE;
        let within_chunk = save_block3_offset % coop_save::SAVE_BLOCK3_CHUNK_SIZE;
        let physical = (0..coop_save::SECTORS_PER_SLOT)
            .find(|physical| {
                let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                usize::from(read_u16(bytes, start + TEST_SECTOR_ID_OFFSET)) == logical
            })
            .expect("selected fixture slot contains every logical sector");
        let offset = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE
            + coop_save::SAVE_BLOCK3_CHUNK_OFFSET
            + within_chunk;
        bytes[offset] ^= 1;
    }

    fn mutate_selected_lineage(bytes: &mut [u8]) {
        let physical = (0..coop_save::SECTORS_PER_SLOT)
            .find(|physical| {
                let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                usize::from(read_u16(bytes, start + TEST_SECTOR_ID_OFFSET)) == 0
            })
            .expect("selected fixture slot contains logical sector zero");
        let sector_start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
        bytes[sector_start] ^= 1;
        let checksum = coop_save::sector_checksum(
            &bytes[sector_start..sector_start + coop_save::LOGICAL_SECTOR_DATA_SIZES[0]],
        );
        write_u16(bytes, sector_start + TEST_SECTOR_CHECKSUM_OFFSET, checksum);
    }

    fn snapshot_request_for_sav(
        lease: coop_cloud::LeaseContract,
        actor: AuthenticatedActor,
        client: ClientInstanceId,
        sav_bytes: &[u8],
    ) -> (SnapshotPrepareRequest, SnapshotFile, SnapshotFile) {
        let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, sav_bytes).expect("sav");
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending");
        let request = SnapshotPrepareRequest::new(
            id(SnapshotId::new),
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .expect("prepare request");
        (request, sav, pending)
    }

    fn deterministic_app() -> (Phase2App, Arc<FixedClock>) {
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let entropy = Arc::new(FixedEntropy::new((0_u8..=255).collect()));
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(clock.clone(), entropy)
        .with_password_engine(Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ));
        (Phase2App::new(config).expect("test config is local"), clock)
    }

    fn deterministic_app_with_store() -> (Phase2App, Arc<InMemoryObjectStore>) {
        let objects = Arc::new(InMemoryObjectStore::new());
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ))
        .with_adapters(Arc::new(InMemoryRepository::new()), objects.clone());
        (
            Phase2App::new(config).expect("test config is local"),
            objects,
        )
    }

    #[derive(Clone)]
    struct CountingPasswordEngine {
        hashes: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl PasswordEngine for CountingPasswordEngine {
        fn hash(&self, _password: &str) -> Result<String, storage::StorageError> {
            self.hashes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok("test-password-phc".to_owned())
        }

        fn verify(&self, _password: &str, _phc: &str) -> bool {
            true
        }

        fn dummy_phc(&self) -> &'static str {
            "test-password-phc"
        }
    }

    #[derive(Clone)]
    struct FailingObjectStore {
        inner: InMemoryObjectStore,
        fail_put: Arc<std::sync::atomic::AtomicBool>,
    }

    impl ObjectStore for FailingObjectStore {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, storage::StorageError> {
            self.inner.get(key)
        }

        fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), storage::StorageError> {
            if self.fail_put.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(storage::StorageError::Lock);
            }
            self.inner.put(key, bytes)
        }

        fn put_if_absent(
            &self,
            key: String,
            bytes: Vec<u8>,
        ) -> Result<bool, storage::StorageError> {
            if self.fail_put.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(storage::StorageError::Lock);
            }
            self.inner.put_if_absent(key, bytes)
        }

        fn contains(&self, key: &str) -> Result<bool, storage::StorageError> {
            self.inner.contains(key)
        }
    }

    #[derive(Clone)]
    struct PartialObjectStore {
        inner: InMemoryObjectStore,
        successful_puts: Arc<std::sync::atomic::AtomicUsize>,
        fail_after: Arc<std::sync::atomic::AtomicUsize>,
        delete_failures: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ObjectStore for PartialObjectStore {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, storage::StorageError> {
            self.inner.get(key)
        }

        fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), storage::StorageError> {
            self.inner.put(key, bytes)
        }

        fn put_if_absent(
            &self,
            key: String,
            bytes: Vec<u8>,
        ) -> Result<bool, storage::StorageError> {
            let index = self
                .successful_puts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if index >= self.fail_after.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(storage::StorageError::Lock);
            }
            self.inner.put_if_absent(key, bytes)
        }

        fn delete_if_present(&self, key: &str) -> Result<bool, storage::StorageError> {
            let failed = self
                .delete_failures
                .fetch_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |remaining| remaining.checked_sub(1),
                )
                .is_ok();
            if failed {
                return Err(storage::StorageError::Lock);
            }
            self.inner.delete_if_present(key)
        }

        fn contains(&self, key: &str) -> Result<bool, storage::StorageError> {
            self.inner.contains(key)
        }
    }

    fn deterministic_app_with_partial_store() -> (
        Phase2App,
        Arc<PartialObjectStore>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let successful_puts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fail_after = Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX));
        let delete_failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let object_store = Arc::new(PartialObjectStore {
            inner: InMemoryObjectStore::new(),
            successful_puts: successful_puts.clone(),
            fail_after,
            delete_failures,
        });
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ))
        .with_adapters(Arc::new(InMemoryRepository::new()), object_store.clone());
        (
            Phase2App::new(config).expect("test config is local"),
            object_store,
            successful_puts,
        )
    }

    fn deterministic_app_with_failing_store() -> (Phase2App, Arc<std::sync::atomic::AtomicBool>) {
        let fail_put = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let object_store = FailingObjectStore {
            inner: InMemoryObjectStore::new(),
            fail_put: fail_put.clone(),
        };
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("test config")
        .with_test_adapters(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(FixedEntropy::new((0_u8..=255).collect())),
        )
        .with_password_engine(Arc::new(
            ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
        ))
        .with_adapters(Arc::new(InMemoryRepository::new()), Arc::new(object_store));
        (
            Phase2App::new(config).expect("test config is local"),
            fail_put,
        )
    }

    fn account_and_lease(
        app: &Phase2App,
    ) -> (
        AuthenticatedActor,
        coop_cloud::LeaseContract,
        ClientInstanceId,
    ) {
        app.add_invitation("fixture-invite").expect("invite");
        let registered = app
            .register(
                RegisterRequest::new(
                    "FixtureUser",
                    password(),
                    InvitationCode::new("fixture-invite").expect("invite"),
                )
                .expect("registration request"),
            )
            .expect("registration");
        let login = app
            .login(LoginRequest::new("fixtureuser", password()).expect("login request"))
            .expect("login");
        let actor = AuthenticatedActor {
            user_id: login.user_id,
            character_id: registered.character_id,
        };
        let client = id(ClientInstanceId::new);
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(registered.character_id, client, id(IdempotencyKey::new)),
            )
            .expect("lease");
        (actor, lease, client)
    }

    fn prepared_snapshot(
        app: &Phase2App,
        actor: AuthenticatedActor,
        lease: coop_cloud::LeaseContract,
        client: ClientInstanceId,
    ) -> (
        coop_cloud::SnapshotPrepareResponse,
        SnapshotFile,
        SnapshotFile,
    ) {
        let generation = u32::try_from(
            lease
                .current_revision
                .next()
                .expect("fixture revision fits")
                .value(),
        )
        .expect("fixture generation fits");
        let (request, sav, pending) = snapshot_request_for_sav(
            lease,
            actor,
            client,
            &valid_character_sav_generation(false, generation),
        );
        (app.prepare(actor, request).expect("prepared"), sav, pending)
    }

    fn upload_prepared(app: &Phase2App, prepared: &coop_cloud::SnapshotPrepareResponse) {
        for target in &prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(
                ticket,
                if target.artifact == ArtifactIdentity::CharacterSav {
                    valid_character_sav_generation(
                        false,
                        u32::try_from(prepared.next_revision.value())
                            .expect("fixture generation fits"),
                    )
                } else {
                    b"{}".to_vec()
                },
            )
            .expect("upload");
        }
    }

    fn upload_ticket(
        prepared: &coop_cloud::SnapshotPrepareResponse,
        artifact: ArtifactIdentity,
    ) -> String {
        prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == artifact)
            .expect("declared artifact has an upload target")
            .url
            .as_str()
            .split("?ticket=")
            .nth(1)
            .expect("target contains capability query")
            .to_owned()
    }

    async fn login_http(app: &Phase2App, request: LoginRequest) -> axum::response::Response {
        app.router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&request).expect("login JSON"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response")
    }

    static PASSWORD_TEST_SERIAL: std::sync::OnceLock<tokio::sync::Mutex<()>> =
        std::sync::OnceLock::new();

    async fn password_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
        PASSWORD_TEST_SERIAL
            .get_or_init(tokio::sync::Mutex::default)
            .lock()
            .await
    }

    fn assert_detached_signature_binds_metadata(envelope: &SignedManifestEnvelope) {
        let trusted = TrustedManifestKey::new(
            "local-test-key",
            [
                0xea, 0x4a, 0x6c, 0x63, 0xe2, 0x9c, 0x52, 0x0a, 0xbe, 0xf5, 0x50, 0x7b, 0x13, 0x2e,
                0xc5, 0xf9, 0x95, 0x47, 0x76, 0xae, 0xbe, 0xbe, 0x7b, 0x92, 0x42, 0x1e, 0xea, 0x69,
                0x14, 0x46, 0xd2, 0x2c,
            ],
        )
        .expect("trusted key");
        assert!(envelope.verify(&trusted).is_ok());
        let mut tampered = envelope.clone();
        let mut signature = *tampered.signature.as_bytes();
        signature[0] ^= 1;
        tampered.signature = coop_cloud::ManifestSignature::from_bytes(signature);
        assert!(tampered.verify(&trusted).is_err());
        let signed_again = SignedManifestEnvelope::sign(
            envelope.manifest.clone(),
            &SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("detached signature");
        assert_eq!(envelope.signature, signed_again.signature);
        let mut changed_manifest = envelope.manifest.clone();
        changed_manifest.snapshot_id = id(SnapshotId::new);
        let changed = SignedManifestEnvelope::sign(
            changed_manifest,
            &SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("changed detached signature");
        assert_ne!(envelope.signature, changed.signature);
    }

    #[test]
    fn registration_login_lease_and_snapshot_bootstrap_are_atomic() {
        let app = Phase2App::test();
        app.add_invitation("invite-1").expect("invite");
        let registration = RegisterRequest::new(
            "Ash_Ketchum",
            password(),
            InvitationCode::new("invite-1").expect("invite"),
        )
        .expect("registration");
        let registered = app.register(registration).expect("registered");
        let logged_in = app
            .login(LoginRequest::new("ASH_KETCHUM", password()).expect("login request"))
            .expect("login");
        let actor = AuthenticatedActor {
            user_id: logged_in.user_id,
            character_id: registered.character_id,
        };
        let client = id(ClientInstanceId::new);
        let acquire = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(registered.character_id, client, id(IdempotencyKey::new)),
            )
            .expect("lease");
        assert_eq!(acquire.current_revision, Revision::initial());
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(false))
                .expect("sav");
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending");
        let snapshot_id = id(SnapshotId::new);
        let prepare = SnapshotPrepareRequest::new(
            snapshot_id,
            SnapshotPrepareFence::new(
                acquire.session_id,
                registered.character_id,
                Revision::initial(),
                acquire.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .expect("prepare");
        let prepared = app.prepare(actor, prepare.clone()).expect("prepared");
        for target in prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            let bytes = if target.artifact == ArtifactIdentity::CharacterSav {
                valid_character_sav(false)
            } else {
                b"{}".to_vec()
            };
            app.upload(ticket, bytes).expect("upload");
        }
        let finalize = SnapshotFinalizeRequest::new(
            snapshot_id,
            SnapshotFinalizeFence::new(
                acquire.session_id,
                registered.character_id,
                Revision::initial(),
                acquire.session_epoch,
                client,
                prepare.idempotency_key,
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        let record = app.finalize(actor, finalize).expect("finalized");
        assert_eq!(record.revision, Revision::new(1));
        let fence = LeaseFence::new(
            acquire.session_id,
            registered.character_id,
            Revision::new(1),
            acquire.session_epoch,
            client,
        );
        let envelope =
            saves::resume_package(&app.store, actor, fence, None).expect("resume package");
        assert_eq!(envelope.manifest.revision, Revision::new(1));
        assert_detached_signature_binds_metadata(&envelope);
        assert_eq!(
            saves::resume_artifact(&app.store, actor, fence, "character.sav", None)
                .expect("sav artifact"),
            valid_character_sav(false)
        );
    }

    #[test]
    fn invitations_are_single_use_and_refresh_reuse_revokes_access() {
        let app = Phase2App::test();
        app.add_invitation("one-use").expect("invite");
        let request = RegisterRequest::new(
            "Misty",
            password(),
            InvitationCode::new("one-use").expect("invite"),
        )
        .expect("request");
        app.register(request.clone()).expect("first registration");
        assert_eq!(app.register(request), Err(Phase2Error::Authentication));
        let first = app
            .login(LoginRequest::new("misty", password()).expect("login"))
            .expect("login");
        let rotated = app
            .refresh(RefreshRequest::new(first.refresh_token.clone()))
            .expect("rotate");
        assert_ne!(
            first.refresh_token.expose_secret(),
            rotated.refresh_token.expose_secret()
        );
        assert_eq!(
            app.refresh(RefreshRequest::new(first.refresh_token)),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            app.logout(LogoutRequest::new(rotated.refresh_token)),
            Ok(LogoutResponse::default())
        );
    }

    #[test]
    fn revoked_token_history_is_pruned_before_new_login() {
        let (app, _) = deterministic_app();
        app.add_invitation("token-retention").expect("invite");
        app.register(
            RegisterRequest::new(
                "TokenRetention",
                password(),
                InvitationCode::new("token-retention").expect("invite"),
            )
            .expect("registration"),
        )
        .expect("registration");
        let first = app
            .login(LoginRequest::new("tokenretention", password()).expect("login"))
            .expect("login");
        app.logout(LogoutRequest::new(first.refresh_token))
            .expect("logout");
        assert_eq!(
            app.store
                .inspect_state(|state| (
                    state.access.len(),
                    state.refresh.len(),
                    state.families.len()
                ))
                .expect("token state"),
            (1, 1, 1)
        );
        app.login(LoginRequest::new("tokenretention", password()).expect("login"))
            .expect("second login");
        assert_eq!(
            app.store
                .inspect_state(|state| (
                    state.access.len(),
                    state.refresh.len(),
                    state.families.len()
                ))
                .expect("token state"),
            (1, 1, 1)
        );
    }

    #[tokio::test]
    async fn wrong_login_attempts_are_indistinguishable() {
        let _guard = password_test_guard().await;
        let app = Phase2App::test();
        app.add_invitation("generic-login").expect("invite");
        app.register(
            RegisterRequest::new(
                "GenericLogin",
                password(),
                InvitationCode::new("generic-login").expect("invite"),
            )
            .expect("registration request"),
        )
        .expect("registration");

        let unknown_user = login_http(
            &app,
            LoginRequest::new("missing-user", password()).expect("unknown login request"),
        )
        .await;
        let wrong_password = login_http(
            &app,
            LoginRequest::new(
                "genericlogin",
                Password::new("a different password").expect("wrong password"),
            )
            .expect("wrong password request"),
        )
        .await;
        assert_eq!(unknown_user.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(unknown_user.status(), wrong_password.status());
        let unknown_body = unknown_user
            .into_body()
            .collect()
            .await
            .expect("unknown response body")
            .to_bytes();
        let wrong_body = wrong_password
            .into_body()
            .collect()
            .await
            .expect("wrong password response body")
            .to_bytes();
        assert_eq!(unknown_body, wrong_body);
    }

    #[test]
    fn expired_access_and_revoked_refresh_family_fail_generically() {
        let (app, clock) = deterministic_app();
        app.add_invitation("expired-access").expect("invite");
        app.register(
            RegisterRequest::new(
                "ExpiredAccess",
                password(),
                InvitationCode::new("expired-access").expect("invite"),
            )
            .expect("registration request"),
        )
        .expect("registration");
        let login = app
            .login(LoginRequest::new("expiredaccess", password()).expect("login request"))
            .expect("login");
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_str(&format!(
                "Bearer {}",
                login.access_token.expose_secret()
            ))
            .expect("authorization header"),
        );
        assert!(auth::actor_from_headers(&app.store, &headers).is_ok());
        clock.advance(storage::ACCESS_TTL_MS + 1);
        assert_eq!(
            auth::actor_from_headers(&app.store, &headers),
            Err(Phase2Error::Authentication)
        );

        app.logout(LogoutRequest::new(login.refresh_token.clone()))
            .expect("logout");
        assert_eq!(
            app.refresh(RefreshRequest::new(login.refresh_token)),
            Err(Phase2Error::Authentication)
        );
    }

    #[tokio::test]
    async fn auth_and_lease_body_caps_return_bounded_errors_before_work() {
        let app = Phase2App::test();
        let oversized_auth = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(vec![0_u8; 8 * 1024 + 1]))
                    .expect("auth request"),
            )
            .await
            .expect("auth response");
        assert_eq!(oversized_auth.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let auth_body = oversized_auth
            .into_body()
            .collect()
            .await
            .expect("auth body")
            .to_bytes();
        let auth_error: serde_json::Value =
            serde_json::from_slice(&auth_body).expect("typed auth error");
        assert_eq!(auth_error["error"]["code"], "payload_too_large");

        let oversized_lease = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/sessions/acquire")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(vec![0_u8; 8 * 1024 + 1]))
                    .expect("lease request"),
            )
            .await
            .expect("lease response");
        assert_eq!(oversized_lease.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let lease_body = oversized_lease
            .into_body()
            .collect()
            .await
            .expect("lease body")
            .to_bytes();
        let lease_error: serde_json::Value =
            serde_json::from_slice(&lease_body).expect("typed lease error");
        assert_eq!(lease_error["error"]["code"], "payload_too_large");

        let oversized_snapshot = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/characters/00000000-0000-4000-8000-000000000001/snapshots/prepare")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(vec![0_u8; 64 * 1024 + 1]))
                    .expect("snapshot request"),
            )
            .await
            .expect("snapshot response");
        assert_eq!(oversized_snapshot.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let oversized_query = format!(
            "/v1/characters/00000000-0000-4000-8000-000000000001/resume-package?revision={}",
            "9".repeat(MAX_REQUEST_QUERY_BYTES)
        );
        let oversized_query_response = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::GET)
                    .uri(oversized_query)
                    .body(axum::body::Body::empty())
                    .expect("query request"),
            )
            .await
            .expect("query response");
        assert_eq!(
            oversized_query_response.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn reconnect_supports_multiple_grace_cycles_and_fences_prior_epochs() {
        let (app, clock) = deterministic_app();
        let (actor, first, client) = account_and_lease(&app);
        clock.advance(storage::LEASE_TTL_MS + 1);
        let first_request = ReconnectLeaseRequest::new(first.fence(), id(IdempotencyKey::new));
        let second = app
            .reconnect(actor, first_request)
            .expect("first reconnect");
        assert_eq!(second.session_id, first.session_id);
        assert!(second.session_epoch > first.session_epoch);
        assert_eq!(app.reconnect(actor, first_request), Ok(second));
        let stale = ReconnectLeaseRequest::new(first.fence(), id(IdempotencyKey::new));
        assert_eq!(app.reconnect(actor, stale), Err(Phase2Error::Conflict));

        clock.advance(storage::LEASE_TTL_MS + 1);
        let second_request = ReconnectLeaseRequest::new(second.fence(), id(IdempotencyKey::new));
        let third = app
            .reconnect(actor, second_request)
            .expect("second reconnect");
        assert_eq!(third.session_id, first.session_id);
        assert!(third.session_epoch > second.session_epoch);
        assert_eq!(third.current_revision, first.current_revision);
        let _ = client;
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn heartbeat_fences_expiry_and_owner_without_mutation() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        clock.advance(1);
        let renewed = app
            .heartbeat(actor, HeartbeatLeaseRequest::new(lease.fence()))
            .expect("heartbeat");
        assert!(renewed.expires_at > lease.expires_at);

        let wrong_epoch = SessionEpoch::new(renewed.session_epoch.value() + 1).expect("epoch");
        let wrong_fence = LeaseFence::new(
            renewed.session_id,
            renewed.character_id,
            renewed.current_revision,
            wrong_epoch,
            renewed.client_instance_id,
        );
        let before_rejection = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].clone())
            .expect("state");
        assert_eq!(
            app.heartbeat(actor, HeartbeatLeaseRequest::new(wrong_fence)),
            Err(Phase2Error::Conflict)
        );
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.leases[&actor.character_id].contract,
                    before_rejection.contract
                );
                assert_eq!(
                    state.leases[&actor.character_id].released,
                    before_rejection.released
                );
            })
            .expect("state");

        app.add_invitation("heartbeat-foreign").expect("invite");
        let registered = app
            .register(
                RegisterRequest::new(
                    "HeartbeatForeign",
                    password(),
                    InvitationCode::new("heartbeat-foreign").expect("invite"),
                )
                .expect("registration request"),
            )
            .expect("registration");
        let foreign_login = app
            .login(LoginRequest::new("heartbeatforeign", password()).expect("login"))
            .expect("foreign login");
        let foreign_actor = AuthenticatedActor {
            user_id: foreign_login.user_id,
            character_id: actor.character_id,
        };
        assert_eq!(
            app.heartbeat(foreign_actor, HeartbeatLeaseRequest::new(renewed.fence())),
            Err(Phase2Error::NotFound)
        );
        assert!(registered.character_id != actor.character_id);

        clock.advance(storage::LEASE_TTL_MS + 1);
        assert_eq!(
            app.heartbeat(actor, HeartbeatLeaseRequest::new(renewed.fence())),
            Err(Phase2Error::Expired)
        );
        app.store
            .inspect_state(|state| {
                assert!(!state.leases[&actor.character_id].released);
                assert_eq!(
                    state.leases[&actor.character_id].contract,
                    before_rejection.contract
                );
            })
            .expect("state");
        let _ = client;
    }

    #[test]
    fn lease_replay_survives_release_and_reacquire() {
        let (app, clock) = deterministic_app();
        let (actor, first, first_client) = account_and_lease(&app);
        clock.advance(storage::LEASE_TTL_MS + 1);
        let reconnect = ReconnectLeaseRequest::new(first.fence(), id(IdempotencyKey::new));
        let reconnected = app.reconnect(actor, reconnect).expect("reconnect");
        assert_eq!(app.reconnect(actor, reconnect), Ok(reconnected));

        let release =
            coop_cloud::ReleaseLeaseRequest::new(reconnected.fence(), id(IdempotencyKey::new));
        app.release(actor, release).expect("release");
        assert_eq!(app.release(actor, release), Ok(LogoutResponse::default()));

        let new_client = id(ClientInstanceId::new);
        let new_key = id(IdempotencyKey::new);
        let reacquired = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, new_client, new_key),
            )
            .expect("reacquire");
        assert_ne!(reacquired.session_id, first.session_id);
        assert!(reacquired.session_epoch > reconnected.session_epoch);
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, new_client, new_key),
            ),
            Ok(reacquired)
        );
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, first_client, new_key),
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(app.reconnect(actor, reconnect), Err(Phase2Error::Conflict));
    }

    #[test]
    fn upload_tamper_expiry_and_capability_binding_are_atomic() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        let sav_target = prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == ArtifactIdentity::CharacterSav)
            .expect("sav target");
        let sav_ticket = sav_target
            .url
            .as_str()
            .split("?ticket=")
            .nth(1)
            .expect("ticket")
            .to_owned();
        assert_eq!(
            saves::upload_with_credential(
                &app.store,
                &sav_ticket,
                "wrong",
                valid_character_sav(false),
            ),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            app.upload(&sav_ticket, b"tampered".to_vec()),
            Err(Phase2Error::InvalidRequest)
        );
        app.upload(&sav_ticket, valid_character_sav(false))
            .expect("correct upload");
        assert_eq!(
            app.upload(&sav_ticket, valid_character_sav(false)),
            Err(Phase2Error::Conflict)
        );
        let pending_target = prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == ArtifactIdentity::PendingCommits)
            .expect("pending target");
        let pending_ticket = pending_target
            .url
            .as_str()
            .split("?ticket=")
            .nth(1)
            .expect("ticket")
            .to_owned();
        clock.advance(storage::UPLOAD_TTL_MS + 1);
        assert_eq!(
            app.upload(&pending_ticket, b"{}".to_vec()),
            Err(Phase2Error::Expired)
        );
    }

    #[test]
    fn character_sav_accepts_both_exact_lengths_and_preserves_rtc_bytes() {
        for with_rtc in [false, true] {
            let (app, _) = deterministic_app();
            let (actor, lease, client) = account_and_lease(&app);
            let bytes = valid_character_sav(with_rtc);
            let expected_len = if with_rtc {
                coop_save::FLASH_IMAGE_SIZE + coop_save::RTC_TRAILER_SIZE
            } else {
                coop_save::FLASH_IMAGE_SIZE
            };
            assert_eq!(bytes.len(), expected_len);
            let (request, sav, pending) = snapshot_request_for_sav(lease, actor, client, &bytes);
            let prepared = app.prepare(actor, request).expect("exact size prepares");
            app.upload(
                &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
                bytes.clone(),
            )
            .expect("valid CSP1 uploads");
            app.upload(
                &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
                b"{}".to_vec(),
            )
            .expect("pending commits upload");
            let finalize = SnapshotFinalizeRequest::new(
                prepared.snapshot_id,
                SnapshotFinalizeFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.current_revision,
                    lease.session_epoch,
                    client,
                    prepared.idempotency_key,
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .expect("finalize request");
            let record = app.finalize(actor, finalize).expect("valid CSP1 finalizes");
            let active_fence = LeaseFence::new(
                lease.session_id,
                actor.character_id,
                record.revision,
                lease.session_epoch,
                client,
            );
            assert_eq!(
                saves::resume_artifact(&app.store, actor, active_fence, "character.sav", None,)
                    .expect("resume artifact"),
                bytes
            );
        }
    }

    #[test]
    fn character_sav_rejects_noncanonical_lengths_at_prepare() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        for bytes in [
            vec![0_u8; coop_save::FLASH_IMAGE_SIZE - 1],
            vec![0_u8; coop_save::FLASH_IMAGE_SIZE + 1],
            vec![0_u8; coop_save::FLASH_IMAGE_SIZE + coop_save::RTC_TRAILER_SIZE + 1],
        ] {
            let (request, _, _) = snapshot_request_for_sav(lease, actor, client, &bytes);
            assert_eq!(
                app.prepare(actor, request),
                Err(Phase2Error::InvalidRequest)
            );
        }
        let state = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&actor.character_id].revision,
                    state.characters[&actor.character_id].active_snapshot,
                    state.prepared.len(),
                )
            })
            .expect("state");
        assert_eq!(state, (Revision::initial(), None, 0));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn malformed_character_sav_uploads_never_advance_the_head() {
        let mut corrupt_slots = valid_character_sav(false);
        for slot in 0..coop_save::SAVE_SLOT_COUNT {
            let offset = (slot * coop_save::SECTORS_PER_SLOT) * coop_save::SECTOR_SIZE
                + TEST_SECTOR_SIGNATURE_OFFSET;
            corrupt_slots[offset] ^= 1;
        }
        let mut corrupt_checksums = valid_character_sav(false);
        for slot in 0..coop_save::SAVE_SLOT_COUNT {
            let offset = (slot * coop_save::SECTORS_PER_SLOT) * coop_save::SECTOR_SIZE;
            corrupt_checksums[offset] ^= 1;
        }
        let mut corrupt_crc = valid_character_sav(false);
        mutate_selected_coop_byte(&mut corrupt_crc, 668);
        let mut corrupt_schema = valid_character_sav(false);
        mutate_selected_coop_byte(&mut corrupt_schema, 4);
        let mut corrupt_registry_version = valid_character_sav(false);
        mutate_selected_coop_byte(&mut corrupt_registry_version, 8);
        let mut corrupt_registry_digest = valid_character_sav(false);
        mutate_selected_coop_byte(&mut corrupt_registry_digest, 12);
        let migration_ambiguous =
            character_sav_with_status(false, coop_save::COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS);

        for (case, bytes) in [
            ("slot", corrupt_slots),
            ("checksum", corrupt_checksums),
            ("crc", corrupt_crc),
            ("schema", corrupt_schema),
            ("registry-version", corrupt_registry_version),
            ("registry-digest", corrupt_registry_digest),
            ("migration-ambiguous", migration_ambiguous),
            ("erased", coop_save::erased_revision_zero_image()),
        ] {
            let (app, _) = deterministic_app();
            let (actor, lease, client) = account_and_lease(&app);
            let (request, sav, pending) = snapshot_request_for_sav(lease, actor, client, &bytes);
            let prepared = app.prepare(actor, request).expect("exact size prepares");
            let sav_ticket = upload_ticket(&prepared, ArtifactIdentity::CharacterSav);
            let pending_ticket = upload_ticket(&prepared, ArtifactIdentity::PendingCommits);
            app.upload(&pending_ticket, b"{}".to_vec())
                .expect("pending commits upload");
            assert_eq!(
                app.upload(&sav_ticket, bytes.clone()),
                Err(Phase2Error::InvalidRequest),
                "{case} fixture must be rejected"
            );
            assert_eq!(
                app.upload(&sav_ticket, bytes.clone()),
                Err(Phase2Error::InvalidRequest),
                "{case} rejection must not consume the ticket"
            );
            ObjectStore::put(
                app.store.objects.as_ref(),
                Store::object_key(
                    actor.character_id,
                    prepared.snapshot_id,
                    ArtifactIdentity::CharacterSav,
                ),
                bytes,
            )
            .expect("simulate a legacy or bypassed invalid object");
            let finalize = SnapshotFinalizeRequest::new(
                prepared.snapshot_id,
                SnapshotFinalizeFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.current_revision,
                    lease.session_epoch,
                    client,
                    prepared.idempotency_key,
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .expect("finalize request");
            assert_eq!(
                app.finalize(actor, finalize),
                Err(Phase2Error::InvalidRequest),
                "{case} must fail defense-in-depth validation before head advance"
            );
            app.store
                .inspect_state(|state| {
                    let character = &state.characters[&actor.character_id];
                    assert_eq!(character.revision, Revision::initial(), "{case}");
                    assert_eq!(character.active_snapshot, None, "{case}");
                    assert!(state.prepared.contains_key(&prepared.snapshot_id), "{case}");
                    let ticket = state
                        .tickets
                        .values()
                        .find(|ticket| {
                            ticket.snapshot_id == prepared.snapshot_id
                                && ticket.artifact == ArtifactIdentity::CharacterSav
                        })
                        .expect("SAV ticket remains pending");
                    assert!(!ticket.used, "{case}");
                })
                .expect("state");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn finalize_binds_lineage_and_advances_generation_from_the_active_head() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let first = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("first finalize request"),
            )
            .expect("first finalize");
        assert_eq!(first.revision, Revision::new(1));

        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("active lease");
        let generation_two = valid_character_sav_generation(false, 2);
        let (request, sav, pending) =
            snapshot_request_for_sav(lease, actor, client, &generation_two);
        let prepared = app.prepare(actor, request).expect("second prepare");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
            generation_two,
        )
        .expect("second SAV upload");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
            b"{}".to_vec(),
        )
        .expect("second pending upload");
        let second_request = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("second finalize request");
        let second = app
            .finalize(actor, second_request)
            .expect("second finalize");
        assert_eq!(second.revision, Revision::new(2));

        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("active lease");
        let wrong_generation = valid_character_sav_generation(false, 4);
        let (request, sav, pending) =
            snapshot_request_for_sav(lease, actor, client, &wrong_generation);
        let prepared = app
            .prepare(actor, request)
            .expect("wrong-generation prepare");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
            wrong_generation,
        )
        .expect("wrong-generation SAV upload");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
            b"{}".to_vec(),
        )
        .expect("wrong-generation pending upload");
        let wrong_generation_request = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("wrong-generation finalize request");
        assert_eq!(
            app.finalize(actor, wrong_generation_request),
            Err(Phase2Error::Conflict)
        );

        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("active lease");
        let mut wrong_lineage = valid_character_sav_generation(false, 3);
        mutate_selected_lineage(&mut wrong_lineage);
        let (request, sav, pending) =
            snapshot_request_for_sav(lease, actor, client, &wrong_lineage);
        let prepared = app.prepare(actor, request).expect("wrong-lineage prepare");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
            wrong_lineage,
        )
        .expect("wrong-lineage SAV upload");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
            b"{}".to_vec(),
        )
        .expect("wrong-lineage pending upload");
        let wrong_lineage_request = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending],
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}")
                .expect("pending")
                .sha256,
            None,
        )
        .expect("wrong-lineage finalize request");
        assert_eq!(
            app.finalize(actor, wrong_lineage_request),
            Err(Phase2Error::Conflict)
        );
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.characters[&actor.character_id].revision,
                    Revision::new(2)
                );
            })
            .expect("head remains at revision two");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn finalize_and_restore_bind_full_idempotency_and_copy_objects() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        for target in &prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(
                ticket,
                if target.artifact == ArtifactIdentity::CharacterSav {
                    valid_character_sav(false)
                } else {
                    b"{}".to_vec()
                },
            )
            .expect("upload");
        }
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        let record = app.finalize(actor, finalize.clone()).expect("finalize");
        assert_eq!(app.finalize(actor, finalize.clone()), Ok(record.clone()));
        let before_finalize_conflict = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&actor.character_id].revision,
                    state.characters[&actor.character_id].active_snapshot,
                    state.snapshots.len(),
                )
            })
            .expect("state");
        let mut changed = finalize;
        changed.files[0] =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(true))
                .expect("changed file");
        assert_eq!(app.finalize(actor, changed), Err(Phase2Error::Conflict));
        let after_finalize_conflict = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&actor.character_id].revision,
                    state.characters[&actor.character_id].active_snapshot,
                    state.snapshots.len(),
                )
            })
            .expect("state");
        assert_eq!(after_finalize_conflict, before_finalize_conflict);

        let restore = SnapshotRestoreRequest::new(
            record.snapshot_id,
            record.session_id,
            actor.character_id,
            record.revision,
            record.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let restored = app.restore(actor, &restore).expect("restore");
        assert_eq!(restored.snapshot.revision, Revision::new(2));
        assert_eq!(app.restore(actor, &restore), Ok(restored.clone()));
        let before_restore_conflict = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&actor.character_id].revision,
                    state.characters[&actor.character_id].active_snapshot,
                    state.snapshots.len(),
                )
            })
            .expect("state");
        let mut changed_restore = restore;
        changed_restore.expected_revision = Revision::new(2);
        assert_eq!(
            app.restore(actor, &changed_restore),
            Err(Phase2Error::Conflict)
        );
        let after_restore_conflict = app
            .store
            .inspect_state(|state| {
                (
                    state.characters[&actor.character_id].revision,
                    state.characters[&actor.character_id].active_snapshot,
                    state.snapshots.len(),
                )
            })
            .expect("state");
        assert_eq!(after_restore_conflict, before_restore_conflict);
        let restored_fence = LeaseFence::new(
            restored.snapshot.session_id,
            actor.character_id,
            restored.snapshot.revision,
            restored.snapshot.session_epoch,
            client,
        );
        assert_eq!(
            saves::resume_artifact(&app.store, actor, restored_fence, "character.sav", None,)
                .expect("restored artifact"),
            valid_character_sav(false)
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn cross_tenant_finalize_and_restore_replays_are_denied_before_lookup() {
        let (app, _) = deterministic_app();
        let (owner, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, owner, lease, client);
        for target in &prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(
                ticket,
                if target.artifact == ArtifactIdentity::CharacterSav {
                    valid_character_sav(false)
                } else {
                    b"{}".to_vec()
                },
            )
            .expect("upload");
        }
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                owner.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        let record = app
            .finalize(owner, finalize.clone())
            .expect("finalized snapshot");
        let restore = SnapshotRestoreRequest::new(
            record.snapshot_id,
            record.session_id,
            owner.character_id,
            record.revision,
            record.session_epoch,
            client,
            id(IdempotencyKey::new),
        );

        app.add_invitation("replay-foreign").expect("invite");
        let foreign = app
            .register(
                RegisterRequest::new(
                    "ReplayForeign",
                    password(),
                    InvitationCode::new("replay-foreign").expect("invite"),
                )
                .expect("registration request"),
            )
            .expect("registration");
        let foreign_login = app
            .login(LoginRequest::new("replayforeign", password()).expect("login"))
            .expect("foreign login");
        let foreign_actor = AuthenticatedActor {
            user_id: foreign_login.user_id,
            character_id: owner.character_id,
        };
        let nonexistent_actor = AuthenticatedActor {
            user_id: foreign_login.user_id,
            character_id: id(CharacterId::new),
        };
        let grouped_character = coop_cloud::GroupId::new(Uuid::new_v4()).expect("group");
        app.store
            .write_transaction(|state| {
                state
                    .active_group_by_member
                    .insert(owner.character_id, grouped_character);
                Ok::<(), Phase2Error>(())
            })
            .expect("group marker");
        assert_eq!(
            app.finalize(foreign_actor, finalize.clone()),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.finalize(nonexistent_actor, finalize),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.restore(foreign_actor, &restore),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.restore(nonexistent_actor, &restore),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(app.restore(owner, &restore), Err(Phase2Error::Conflict));
        assert_eq!(
            app.restore_at(foreign_actor, &restore, record.revision.value()),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.restore_at(owner, &restore, record.revision.value()),
            Err(Phase2Error::Conflict)
        );
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.characters[&owner.character_id].revision,
                    Revision::new(1)
                );
                assert_eq!(
                    state.characters[&owner.character_id].active_snapshot,
                    Some(record.snapshot_id)
                );
            })
            .expect("state");
        assert_ne!(foreign.character_id, owner.character_id);
    }

    #[tokio::test]
    async fn upload_http_requires_query_capability_and_applies_dynamic_limit() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        let target = prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == ArtifactIdentity::CharacterSav)
            .expect("sav target");
        let stripped = target.url.as_str().split('?').next().expect("path");
        let missing_query = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::PUT)
                    .uri(stripped)
                    .body(axum::body::Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(missing_query.status(), StatusCode::UNAUTHORIZED);

        let oversized = app
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::PUT)
                    .uri(target.url.as_str())
                    .body(axum::body::Body::from(vec![
                        0_u8;
                        usize::try_from(
                            storage::MAX_CHARACTER_SAV
                        )
                        .expect("test target limit")
                        .saturating_add(1)
                    ]))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = oversized
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        assert!(!String::from_utf8_lossy(&body).contains("ticket="));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn password_work_has_bounded_immediate_admission() {
        let _guard = password_test_guard().await;
        let limit = password_hash_limit();
        let permits: Vec<_> = (0..PASSWORD_HASH_WORKERS)
            .map(|_| limit.clone().try_acquire_owned().expect("test permit"))
            .collect();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                run_password_operation(|| Ok::<_, Phase2Error>(()))
            )
            .await
            .is_err()
        );
        drop(permits);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn known_and_unknown_logins_share_fair_bounded_admission() {
        let _guard = password_test_guard().await;
        let app = Phase2App::test();
        app.add_invitation("fair-login").expect("invite");
        app.register(
            RegisterRequest::new(
                "FairLogin",
                password(),
                InvitationCode::new("fair-login").expect("invite"),
            )
            .expect("registration"),
        )
        .expect("registration");
        let limit = password_hash_limit();
        let permits: Vec<_> = (0..PASSWORD_HASH_WORKERS)
            .map(|_| limit.clone().try_acquire_owned().expect("test permit"))
            .collect();
        let known = tokio::time::timeout(
            Duration::from_millis(20),
            login_http(
                &app,
                LoginRequest::new("fairlogin", password()).expect("known login"),
            ),
        );
        let unknown = tokio::time::timeout(
            Duration::from_millis(20),
            login_http(
                &app,
                LoginRequest::new("unknown-fair-login", password()).expect("unknown login"),
            ),
        );
        let (known, unknown) = tokio::join!(known, unknown);
        assert!(known.is_err());
        assert!(unknown.is_err());
        drop(permits);
    }

    #[test]
    fn expired_release_is_rejected_without_mutation_and_acquire_keys_are_scoped() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let release = coop_cloud::ReleaseLeaseRequest::new(lease.fence(), id(IdempotencyKey::new));
        clock.advance(storage::LEASE_TTL_MS + 1);
        assert_eq!(app.release(actor, release), Err(Phase2Error::Expired));
        app.store
            .inspect_state(|state| {
                let current = state.leases.get(&actor.character_id).expect("lease");
                assert!(!current.released);
                assert!(current.release_keys.is_empty());
            })
            .expect("state");

        clock.advance(storage::RECONNECT_GRACE_MS + 1);
        let key = id(IdempotencyKey::new);
        let new_client = id(ClientInstanceId::new);
        let old = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, key),
            )
            .expect("acquire after grace");
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, new_client, key),
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, key),
            ),
            Ok(old)
        );
        let first_release =
            coop_cloud::ReleaseLeaseRequest::new(old.fence(), id(IdempotencyKey::new));
        app.release(actor, first_release)
            .expect("release active lease");
        clock.advance(storage::LEASE_TTL_MS + 1);
        let second_release =
            coop_cloud::ReleaseLeaseRequest::new(old.fence(), id(IdempotencyKey::new));
        assert_eq!(
            app.release(actor, second_release),
            Err(Phase2Error::Expired)
        );
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, new_client, key),
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, key),
            ),
            Ok(old)
        );
    }

    #[test]
    fn service_clock_never_moves_backwards() {
        let (app, clock) = deterministic_app();
        let first = app.store.now();
        clock.set(first + 10_000);
        let later = app.store.now();
        clock.set(1);
        assert_eq!(app.store.now(), later);
    }

    #[test]
    fn prepare_expiry_and_duplicate_history_are_fail_closed() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(false))
                .expect("sav");
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending");
        let snapshot_id = id(SnapshotId::new);
        let request = SnapshotPrepareRequest::new(
            snapshot_id,
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .expect("prepare");
        let prepared = app.prepare(actor, request.clone()).expect("prepared");
        assert_eq!(
            app.prepare(actor, request.clone())
                .expect("idempotent prepare"),
            prepared
        );
        clock.advance(storage::UPLOAD_TTL_MS + 1);
        assert_eq!(
            app.prepare(actor, request.clone()),
            Err(Phase2Error::Expired)
        );
        assert_eq!(app.prepare(actor, request), Err(Phase2Error::Conflict));

        let client = id(ClientInstanceId::new);
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, id(IdempotencyKey::new)),
            )
            .expect("new lease after expired prepare");
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        for target in &prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(
                ticket,
                if target.artifact == ArtifactIdentity::CharacterSav {
                    valid_character_sav(false)
                } else {
                    b"{}".to_vec()
                },
            )
            .expect("upload");
        }
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize");
        app.finalize(actor, finalize).expect("finalized");
        let duplicate = SnapshotPrepareRequest::new(
            prepared.snapshot_id,
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                Revision::new(1),
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![
                SnapshotFile::from_bytes(
                    ArtifactIdentity::CharacterSav,
                    &valid_character_sav(false),
                )
                .expect("sav"),
                SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending"),
            ],
            pending.sha256,
        )
        .expect("duplicate declaration");
        assert_eq!(app.prepare(actor, duplicate), Err(Phase2Error::Conflict));
    }

    #[test]
    fn concurrent_finalizers_have_one_winner() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        for target in &prepared.upload_targets {
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(
                ticket,
                if target.artifact == ArtifactIdentity::CharacterSav {
                    valid_character_sav(false)
                } else {
                    b"{}".to_vec()
                },
            )
            .expect("upload");
        }
        let request = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize");
        let mut other = request.clone();
        other.idempotency_key = id(IdempotencyKey::new);
        let left_app = app.clone();
        let right_app = app.clone();
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| left_app.finalize(actor, request));
            let right = scope.spawn(|| right_app.finalize(actor, other));
            (left.join().expect("left"), right.join().expect("right"))
        });
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.characters[&actor.character_id].revision,
                    Revision::new(1)
                );
            })
            .expect("state");
    }

    #[test]
    fn exact_finalize_and_restore_replays_survive_later_head_advances() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let first_finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("first finalize");
        let first_record = app
            .finalize(actor, first_finalize.clone())
            .expect("first finalize");

        let second_lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("current lease");
        let (second_prepared, second_sav, second_pending) =
            prepared_snapshot(&app, actor, second_lease, client);
        upload_prepared(&app, &second_prepared);
        let second_finalize = SnapshotFinalizeRequest::new(
            second_prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                second_lease.session_id,
                actor.character_id,
                second_lease.current_revision,
                second_lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![second_sav, second_pending.clone()],
            second_pending.sha256,
            None,
        )
        .expect("second finalize");
        app.finalize(actor, second_finalize)
            .expect("second finalize");
        assert_eq!(
            app.finalize(actor, first_finalize),
            Ok(first_record.clone())
        );

        let current = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("current lease");
        let first_restore = SnapshotRestoreRequest::new(
            first_record.snapshot_id,
            current.session_id,
            actor.character_id,
            current.current_revision,
            current.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let first_restore_response = app.restore(actor, &first_restore).expect("first restore");
        let current = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("current lease");
        let second_restore = SnapshotRestoreRequest::new(
            first_record.snapshot_id,
            current.session_id,
            actor.character_id,
            current.current_revision,
            current.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        app.restore(actor, &second_restore).expect("second restore");
        assert_eq!(
            app.restore(actor, &first_restore),
            Ok(first_restore_response)
        );
    }

    #[test]
    fn concurrent_restore_loser_creates_no_unreachable_object_copy() {
        let (app, objects) = deterministic_app_with_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let finalized = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("finalize"),
            )
            .expect("finalize");
        let before = objects.object_count().expect("object count");
        let first = SnapshotRestoreRequest::new(
            finalized.snapshot_id,
            finalized.session_id,
            actor.character_id,
            finalized.revision,
            finalized.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let mut second = first.clone();
        second.idempotency_key = id(IdempotencyKey::new);
        let left_app = app.clone();
        let right_app = app.clone();
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| left_app.restore(actor, &first));
            let right = scope.spawn(|| right_app.restore(actor, &second));
            (left.join().expect("left"), right.join().expect("right"))
        });
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        assert_eq!(objects.object_count().expect("object count"), before + 2);
    }

    #[test]
    fn foreign_and_nonexistent_lists_are_indistinguishable() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        app.add_invitation("second-tenant").expect("invite");
        let second = app
            .register(
                RegisterRequest::new(
                    "SecondTenant",
                    password(),
                    InvitationCode::new("second-tenant").expect("invite"),
                )
                .expect("request"),
            )
            .expect("second registration");
        let second_login = app
            .login(LoginRequest::new("secondtenant", password()).expect("login"))
            .expect("second login");
        let nonexistent = id(CharacterId::new);
        let foreign = SnapshotListRequest::new(
            id(SessionId::new),
            actor.character_id,
            lease.session_epoch,
            client,
            20,
        )
        .expect("list");
        let missing = SnapshotListRequest::new(
            id(SessionId::new),
            nonexistent,
            lease.session_epoch,
            client,
            20,
        )
        .expect("list");
        let foreign_result = app.list(
            AuthenticatedActor {
                user_id: second_login.user_id,
                character_id: second.character_id,
            },
            foreign,
        );
        assert_eq!(foreign_result, Err(Phase2Error::NotFound));
        assert_eq!(
            app.list(
                AuthenticatedActor {
                    user_id: actor.user_id,
                    character_id: actor.character_id,
                },
                missing,
            ),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn concurrent_prepare_reuses_one_operation_and_rejects_changed_body() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(false))
                .expect("sav");
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending");
        let snapshot_id = id(SnapshotId::new);
        let key = id(IdempotencyKey::new);
        let first = SnapshotPrepareRequest::new(
            snapshot_id,
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                key,
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .expect("prepare");
        let mut changed = first.clone();
        changed.files[0] =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(true))
                .expect("changed sav");
        let left_app = app.clone();
        let right_app = app.clone();
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| left_app.prepare(actor, first));
            let right = scope.spawn(|| right_app.prepare(actor, changed));
            (left.join().expect("left"), right.join().expect("right"))
        });
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        app.store
            .inspect_state(|state| {
                assert_eq!(state.prepared.len(), 1);
                assert_eq!(state.tickets.len(), 2);
            })
            .expect("state");
    }

    #[test]
    fn prepared_snapshot_quota_is_scoped_per_character() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (first, _, _) = prepared_snapshot(&app, actor, lease, client);
        let _: Result<(), Phase2Error> = app.store.write_transaction(|state| {
            let template = state.prepared.values().next().cloned().expect("template");
            for _ in 1..storage::MAX_PREPARED_SNAPSHOTS {
                let snapshot_id = id(SnapshotId::new);
                let mut copy = template.clone();
                copy.request.snapshot_id = snapshot_id;
                state.prepared.insert(snapshot_id, copy);
            }
            Ok(())
        });
        let mut blocked = SnapshotPrepareRequest::new(
            id(SnapshotId::new),
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            first.files.clone(),
            first.pending_commits_sha256,
        )
        .expect("blocked prepare");
        blocked.snapshot_id = id(SnapshotId::new);
        assert_eq!(app.prepare(actor, blocked), Err(Phase2Error::Busy));

        app.add_invitation("quota-other-tenant").expect("invite");
        let other = app
            .register(
                RegisterRequest::new(
                    "QuotaOther",
                    password(),
                    InvitationCode::new("quota-other-tenant").expect("invite"),
                )
                .expect("request"),
            )
            .expect("registration");
        let other_login = app
            .login(LoginRequest::new("quotaother", password()).expect("login"))
            .expect("login");
        let other_actor = AuthenticatedActor {
            user_id: other_login.user_id,
            character_id: other.character_id,
        };
        let other_client = id(ClientInstanceId::new);
        let other_lease = app
            .acquire(
                other_actor,
                AcquireLeaseRequest::new(other.character_id, other_client, id(IdempotencyKey::new)),
            )
            .expect("other lease");
        prepared_snapshot(&app, other_actor, other_lease, other_client);
    }

    #[test]
    fn finalize_counts_only_durable_snapshots_for_history_quota() {
        let (app, _) = deterministic_app();
        let (actor, _, client) = account_and_lease(&app);
        for _ in 0..(storage::MAX_SNAPSHOTS_PER_CHARACTER - 1) {
            let lease = app
                .store
                .inspect_state(|state| state.leases[&actor.character_id].contract)
                .expect("lease");
            let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
            upload_prepared(&app, &prepared);
            app.finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("finalize request"),
            )
            .expect("finalize");
        }
        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("lease");
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        app.finalize(
            actor,
            SnapshotFinalizeRequest::new(
                prepared.snapshot_id,
                SnapshotFinalizeFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.current_revision,
                    lease.session_epoch,
                    client,
                    id(IdempotencyKey::new),
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .expect("boundary finalize request"),
        )
        .expect("boundary finalize");
        assert_eq!(
            app.store
                .inspect_state(|state| state.snapshots.len())
                .expect("snapshot count"),
            storage::MAX_SNAPSHOTS_PER_CHARACTER
        );
    }

    #[test]
    fn bootstrap_builder_consumes_one_use_invitation_and_actual_port_is_preserved() {
        let signing_hex = "07".repeat(32);
        let app = phase2_app_from_values(
            "pepper-pepper-pepper-pepper",
            &signing_hex,
            "local-test-key",
            "bootstrap-one-use",
            "http://127.0.0.1:43127".to_owned(),
        )
        .expect("configured local app");
        let request = RegisterRequest::new(
            "BootstrapUser",
            password(),
            InvitationCode::new("bootstrap-one-use").expect("invite"),
        )
        .expect("request");
        app.register(request.clone()).expect("first registration");
        assert_eq!(app.register(request), Err(Phase2Error::Authentication));
        assert_eq!(
            loopback_upload_base("127.0.0.1:43127".parse().expect("address")),
            "http://127.0.0.1:43127"
        );
    }

    #[test]
    fn invalid_registration_is_rejected_before_password_work() {
        let hashes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("config")
        .with_password_engine(Arc::new(CountingPasswordEngine {
            hashes: hashes.clone(),
        }));
        let app = Phase2App::new(config).expect("app");
        let invalid = RegisterRequest::new(
            "BeforeHash",
            password(),
            InvitationCode::new("missing-invite").expect("invite"),
        )
        .expect("request");
        assert_eq!(app.register(invalid), Err(Phase2Error::Authentication));
        assert_eq!(hashes.load(std::sync::atomic::Ordering::SeqCst), 0);

        app.add_invitation("valid-invite").expect("invite");
        app.register(
            RegisterRequest::new(
                "AfterHash",
                password(),
                InvitationCode::new("valid-invite").expect("invite"),
            )
            .expect("request"),
        )
        .expect("registration");
        assert_eq!(hashes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn explicit_in_memory_repository_and_object_store_adapters_are_supported() {
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("config")
        .with_adapters(
            Arc::new(InMemoryRepository::new()),
            Arc::new(InMemoryObjectStore::new()),
        );
        let app = Phase2App::new(config).expect("in-memory adapters");
        app.add_invitation("adapter-invite").expect("invite");
    }

    #[test]
    fn release_replay_is_fenced_and_prepare_keys_are_operation_scoped() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let release_key = id(IdempotencyKey::new);
        let release = ReleaseLeaseRequest::new(
            LeaseFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
            ),
            release_key,
        );
        app.release(actor, release).expect("release");
        assert_eq!(app.release(actor, release), Ok(LogoutResponse::default()));
        let mut changed_fence = release;
        changed_fence.current_revision = Revision::new(1);
        assert_eq!(
            app.release(actor, changed_fence),
            Err(Phase2Error::Conflict)
        );
        let changed_key = ReleaseLeaseRequest::new(lease.fence(), id(IdempotencyKey::new));
        assert_eq!(app.release(actor, changed_key), Err(Phase2Error::Conflict));

        // A key is scoped to the character and operation, not merely to a
        // snapshot id. Reusing it for another snapshot must not alias the
        // first prepared response.
        let client = id(ClientInstanceId::new);
        let lease = app
            .acquire(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, id(IdempotencyKey::new)),
            )
            .expect("new lease after release");
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &valid_character_sav(false))
                .expect("sav");
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}").expect("pending");
        let key = id(IdempotencyKey::new);
        let first = SnapshotPrepareRequest::new(
            id(SnapshotId::new),
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                key,
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .expect("first prepare");
        app.prepare(actor, first.clone()).expect("first prepare");
        let mut reused = first;
        reused.snapshot_id = id(SnapshotId::new);
        assert_eq!(app.prepare(actor, reused), Err(Phase2Error::Conflict));
    }

    #[test]
    fn object_store_failures_are_retryable_without_partial_snapshot_metadata() {
        let (app, fail_put) = deterministic_app_with_failing_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        let sav_target = prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == ArtifactIdentity::CharacterSav)
            .expect("sav target");
        let sav_ticket = sav_target
            .url
            .as_str()
            .split("?ticket=")
            .nth(1)
            .expect("ticket");
        fail_put.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            app.upload(sav_ticket, valid_character_sav(false)),
            Err(Phase2Error::Internal)
        );
        let fingerprint = storage::Store::token_fingerprint(sav_ticket);
        assert!(
            !app.store
                .inspect_state(|state| state.tickets[&fingerprint].used)
                .expect("state")
        );
        fail_put.store(false, std::sync::atomic::Ordering::SeqCst);
        app.upload(sav_ticket, valid_character_sav(false))
            .expect("retry upload");
        for target in &prepared.upload_targets {
            if target.artifact == ArtifactIdentity::CharacterSav {
                continue;
            }
            let ticket = target
                .url
                .as_str()
                .split("?ticket=")
                .nth(1)
                .expect("ticket");
            app.upload(ticket, b"{}".to_vec()).expect("upload");
        }
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize");
        let record = app.finalize(actor, finalize).expect("finalize");
        let historical = saves::resume_package(
            &app.store,
            actor,
            LeaseFence::new(
                record.session_id,
                actor.character_id,
                record.revision,
                record.session_epoch,
                client,
            ),
            Some(1),
        )
        .expect("historical resume package");
        assert_eq!(historical.manifest.revision, Revision::new(1));
        let restore = SnapshotRestoreRequest::new(
            record.snapshot_id,
            record.session_id,
            actor.character_id,
            record.revision,
            record.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let before = app
            .store
            .inspect_state(|state| state.snapshots.len())
            .expect("state");
        fail_put.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(app.restore(actor, &restore), Err(Phase2Error::Internal));
        assert_eq!(
            app.store
                .inspect_state(|state| state.snapshots.len())
                .expect("state"),
            before
        );
        fail_put.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            app.restore(actor, &restore)
                .expect("retry restore")
                .snapshot
                .revision,
            Revision::new(2)
        );
    }

    #[test]
    fn partial_restore_copy_is_cleaned_before_retry() {
        let (app, object_store, successful_puts) = deterministic_app_with_partial_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let finalized = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("finalize"),
            )
            .expect("finalize");
        let before = object_store.inner.object_count().expect("object count");
        let fail_after = successful_puts.load(std::sync::atomic::Ordering::SeqCst) + 1;
        object_store
            .fail_after
            .store(fail_after, std::sync::atomic::Ordering::SeqCst);
        let restore = SnapshotRestoreRequest::new(
            finalized.snapshot_id,
            finalized.session_id,
            actor.character_id,
            finalized.revision,
            finalized.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        assert_eq!(app.restore(actor, &restore), Err(Phase2Error::Internal));
        assert_eq!(
            object_store.inner.object_count().expect("object count"),
            before
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.snapshots.len())
                .expect("state"),
            1
        );
        object_store
            .fail_after
            .store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            app.restore(actor, &restore)
                .expect("retry restore")
                .snapshot
                .revision,
            Revision::new(2)
        );
    }

    #[test]
    fn stale_restore_cleanup_failure_remains_retryable() {
        let (app, object_store, _) = deterministic_app_with_partial_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let finalized = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("finalize request"),
            )
            .expect("finalize");
        let restore = SnapshotRestoreRequest::new(
            finalized.snapshot_id,
            finalized.session_id,
            actor.character_id,
            finalized.revision,
            finalized.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let stale_snapshot = id(SnapshotId::new);
        let stale_keys = [
            ArtifactIdentity::CharacterSav,
            ArtifactIdentity::PendingCommits,
            ArtifactIdentity::ResumeSs1,
        ]
        .map(|artifact| storage::Store::object_key(actor.character_id, stale_snapshot, artifact));
        for key in &stale_keys {
            ObjectStore::put(object_store.as_ref(), key.clone(), vec![0x55]).expect("stale object");
        }
        app.store
            .write_transaction(|state| {
                state.restore_staging.insert(
                    actor.character_id,
                    storage::RestoreStage {
                        request: restore.clone(),
                        snapshot_id: stale_snapshot,
                        expires_at: 0,
                        storage_bytes: 3,
                        created_objects: stale_keys.to_vec(),
                    },
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("stale stage");
        let before = object_store.inner.object_count().expect("object count");
        object_store
            .delete_failures
            .store(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(app.restore(actor, &restore), Err(Phase2Error::Internal));
        assert_eq!(
            object_store.inner.object_count().expect("object count"),
            before
        );
        assert!(
            app.store
                .inspect_state(|state| state.restore_staging.contains_key(&actor.character_id))
                .expect("stale stage retained")
        );

        assert_eq!(
            app.restore(actor, &restore)
                .expect("retry restore")
                .snapshot
                .revision,
            Revision::new(2)
        );
        assert_eq!(
            object_store.inner.object_count().expect("object count"),
            before - stale_keys.len() + 2
        );
        assert!(
            !app.store
                .inspect_state(|state| state.restore_staging.contains_key(&actor.character_id))
                .expect("stage cleared")
        );
    }

    #[test]
    fn upload_retries_exact_existing_object_but_rejects_digest_mismatch() {
        let (app, objects) = deterministic_app_with_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _sav, _pending) = prepared_snapshot(&app, actor, lease, client);
        let target = prepared
            .upload_targets
            .iter()
            .find(|target| target.artifact == ArtifactIdentity::CharacterSav)
            .expect("sav target");
        let ticket = target
            .url
            .as_str()
            .split("?ticket=")
            .nth(1)
            .expect("ticket");
        let fingerprint = storage::Store::token_fingerprint(ticket);
        let key = storage::Store::object_key(
            actor.character_id,
            prepared.snapshot_id,
            ArtifactIdentity::CharacterSav,
        );
        app.upload(ticket, valid_character_sav(false))
            .expect("first upload");
        let _: Result<(), Phase2Error> = app.store.write_transaction(|state| {
            state.tickets.get_mut(&fingerprint).expect("ticket").used = false;
            Ok(())
        });
        app.upload(ticket, valid_character_sav(false))
            .expect("exact object retry");
        let _: Result<(), Phase2Error> = app.store.write_transaction(|state| {
            state.tickets.get_mut(&fingerprint).expect("ticket").used = false;
            Ok(())
        });
        ObjectStore::put(objects.as_ref(), key, b"tampered".to_vec()).expect("tamper object");
        assert_eq!(
            app.upload(ticket, valid_character_sav(false)),
            Err(Phase2Error::Conflict)
        );
        assert!(
            !app.store
                .inspect_state(|state| state.tickets[&fingerprint].used)
                .expect("ticket state")
        );
    }

    #[test]
    fn a_cleanup_claim_fences_finalize_before_object_deletion() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let sav_key = storage::Store::object_key(
            actor.character_id,
            prepared.snapshot_id,
            ArtifactIdentity::CharacterSav,
        );
        app.store
            .write_transaction(|state| {
                state
                    .upload_objects
                    .get_mut(&sav_key)
                    .expect("owned upload")
                    .cleanup_claimed = true;
                Ok::<_, Phase2Error>(())
            })
            .expect("claim upload object");
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        assert_eq!(
            app.finalize(actor, finalize.clone()),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.snapshots.len())
                .expect("state"),
            0
        );
        app.store
            .write_transaction(|state| {
                state
                    .upload_objects
                    .get_mut(&sav_key)
                    .expect("claimed upload")
                    .cleanup_claimed = false;
                Ok::<_, Phase2Error>(())
            })
            .expect("release claim");
        app.finalize(actor, finalize).expect("unclaimed finalize");
    }

    #[test]
    fn production_and_non_loopback_local_adapters_fail_closed() {
        assert!(matches!(
            Phase2App::new(Phase2Config::postgres_firebase()),
            Err(Phase2Error::Internal)
        ));
        assert!(ProductionConfig::new("", "bucket.example").is_err());
        assert!(ProductionConfig::new("http://db.example", "bucket.example").is_err());
        assert!(ProductionConfig::new("postgres://db.example", "bad/bucket").is_err());
        let production = ProductionConfig::new("postgres://db.example", "bucket.example")
            .expect("production config");
        assert_eq!(format!("{production:?}"), "ProductionConfig([REDACTED])");
        assert_eq!(format!("{:?}", Phase2App::test()), "Phase2App([REDACTED])");
        assert!(
            Phase2Config::local(vec![0x55; 32], SigningPrivateKey::from_bytes([7; 32]), "")
                .is_err()
        );
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "local-test-key",
        )
        .expect("config")
        .with_upload_base_url("https://uploads.example.invalid");
        assert!(matches!(Phase2App::new(config), Err(Phase2Error::Internal)));
        for base in [
            "http://localhost",
            "http://localhost:3000",
            "http://127.0.0.1.evil",
            "http://127.0.0.1/path",
        ] {
            let config = Phase2Config::local(
                vec![0x55; 32],
                SigningPrivateKey::from_bytes([7; 32]),
                "local-test-key",
            )
            .expect("config")
            .with_upload_base_url(base);
            assert!(matches!(Phase2App::new(config), Err(Phase2Error::Internal)));
        }
    }

    #[test]
    fn expired_upload_retires_prepared_declaration_and_deletes_its_objects() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        let sav_ticket = upload_ticket(&prepared, ArtifactIdentity::CharacterSav);
        let pending_ticket = upload_ticket(&prepared, ArtifactIdentity::PendingCommits);
        app.upload(&sav_ticket, valid_character_sav(false))
            .expect("initial SAV upload");
        app.upload(&pending_ticket, b"{}".to_vec())
            .expect("initial pending upload");
        let sav_key = storage::Store::object_key(
            actor.character_id,
            prepared.snapshot_id,
            ArtifactIdentity::CharacterSav,
        );
        assert!(app.store.objects.contains(&sav_key).expect("object exists"));

        clock.advance(storage::UPLOAD_TTL_MS + 1);
        assert_eq!(
            app.upload(&sav_ticket, valid_character_sav(false)),
            Err(Phase2Error::Expired)
        );
        assert!(
            !app.store
                .objects
                .contains(&sav_key)
                .expect("object removed")
        );
        app.store
            .inspect_state(|state| {
                assert!(!state.prepared.contains_key(&prepared.snapshot_id));
                assert!(state.retired_snapshots.contains(&prepared.snapshot_id));
                assert!(state.upload_objects.is_empty());
            })
            .expect("expired declaration retired");
    }

    #[test]
    fn restore_rewinds_generation_and_next_finalize_advances_from_restored_source() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let first = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("first finalize request"),
            )
            .expect("first finalize");
        let restore = SnapshotRestoreRequest::new(
            first.snapshot_id,
            first.session_id,
            actor.character_id,
            first.revision,
            first.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let restored = app.restore(actor, &restore).expect("restore");
        assert_eq!(restored.snapshot.revision, Revision::new(2));
        let restored_fence = LeaseFence::new(
            restored.snapshot.session_id,
            actor.character_id,
            restored.snapshot.revision,
            restored.snapshot.session_epoch,
            client,
        );
        assert_eq!(
            saves::resume_artifact(&app.store, actor, restored_fence, "character.sav", None)
                .expect("restored SAV"),
            valid_character_sav(false)
        );

        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("active lease");
        let generation_two = valid_character_sav_generation(false, 2);
        let (request, sav, pending) =
            snapshot_request_for_sav(lease, actor, client, &generation_two);
        let prepared = app.prepare(actor, request).expect("post-restore prepare");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
            generation_two,
        )
        .expect("post-restore SAV upload");
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
            b"{}".to_vec(),
        )
        .expect("post-restore pending upload");
        let finalized = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("post-restore finalize request"),
            )
            .expect("post-restore finalize");
        assert_eq!(finalized.revision, Revision::new(3));
    }

    #[test]
    fn prepared_declarations_are_included_in_character_byte_quota() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        app.store
            .write_transaction(|state| {
                let active = state
                    .prepared
                    .get_mut(&prepared.snapshot_id)
                    .expect("prepared snapshot");
                active.request.files[0].size_bytes = storage::MAX_SNAPSHOT_STORAGE_BYTES;
                Ok::<_, Phase2Error>(())
            })
            .expect("inflate active declaration");
        let (request, _, _) = snapshot_request_for_sav(
            lease,
            actor,
            client,
            &valid_character_sav_generation(false, 1),
        );
        assert_eq!(app.prepare(actor, request), Err(Phase2Error::Busy));
        app.store
            .inspect_state(|state| {
                assert_eq!(state.prepared.len(), 1);
                assert_eq!(
                    state.characters[&actor.character_id].revision,
                    Revision::initial()
                );
            })
            .expect("quota rejection is atomic");
    }

    #[test]
    fn active_restore_reservation_bytes_are_included_in_character_quota() {
        let (app, _) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let restore = SnapshotRestoreRequest::new(
            id(SnapshotId::new),
            lease.session_id,
            actor.character_id,
            lease.current_revision,
            lease.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        app.store
            .write_transaction(|state| {
                state.restore_staging.insert(
                    actor.character_id,
                    storage::RestoreStage {
                        request: restore,
                        snapshot_id: id(SnapshotId::new),
                        expires_at: lease.expires_at.value(),
                        storage_bytes: storage::MAX_SNAPSHOT_STORAGE_BYTES,
                        created_objects: Vec::new(),
                    },
                );
                Ok::<_, Phase2Error>(())
            })
            .expect("restore reservation");
        let (request, _, _) = snapshot_request_for_sav(
            lease,
            actor,
            client,
            &valid_character_sav_generation(false, 1),
        );
        assert_eq!(app.prepare(actor, request), Err(Phase2Error::Busy));
        app.store
            .inspect_state(|state| assert!(state.prepared.is_empty()))
            .expect("quota rejection is atomic");
    }

    #[test]
    fn retired_snapshot_tombstones_are_bounded() {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        app.store
            .write_transaction(|state| {
                for _ in 0..storage::MAX_RETIRED_SNAPSHOTS {
                    state.retired_snapshots.insert(id(SnapshotId::new));
                }
                Ok::<_, Phase2Error>(())
            })
            .expect("fill bounded tombstone cache");
        clock.advance(storage::UPLOAD_TTL_MS + 1);
        let ticket = upload_ticket(&prepared, ArtifactIdentity::CharacterSav);
        assert_eq!(
            app.upload(&ticket, valid_character_sav(false)),
            Err(Phase2Error::Expired)
        );
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.retired_snapshots.len(),
                    storage::MAX_RETIRED_SNAPSHOTS
                );
            })
            .expect("bounded tombstone cache");
    }

    #[test]
    fn resume_package_verifies_mandatory_artifacts_before_signing() {
        for (artifact, corrupt) in [
            (ArtifactIdentity::CharacterSav, false),
            (ArtifactIdentity::PendingCommits, true),
        ] {
            let (app, _) = deterministic_app();
            let (actor, lease, client) = account_and_lease(&app);
            let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
            upload_prepared(&app, &prepared);
            let record = app
                .finalize(
                    actor,
                    SnapshotFinalizeRequest::new(
                        prepared.snapshot_id,
                        SnapshotFinalizeFence::new(
                            lease.session_id,
                            actor.character_id,
                            lease.current_revision,
                            lease.session_epoch,
                            client,
                            id(IdempotencyKey::new),
                        ),
                        vec![sav, pending.clone()],
                        pending.sha256,
                        None,
                    )
                    .expect("finalize request"),
                )
                .expect("finalize");
            let key = storage::Store::object_key(actor.character_id, record.snapshot_id, artifact);
            if corrupt {
                ObjectStore::put(app.store.objects.as_ref(), key, b"corrupt".to_vec())
                    .expect("corrupt mandatory object");
            } else {
                assert!(
                    app.store
                        .objects
                        .delete_if_present(&key)
                        .expect("remove mandatory object")
                );
            }
            let fence = LeaseFence::new(
                record.session_id,
                actor.character_id,
                record.revision,
                record.session_epoch,
                client,
            );
            assert_eq!(
                saves::resume_package(&app.store, actor, fence, None),
                Err(Phase2Error::Internal)
            );
        }
    }

    #[test]
    fn migration_binds_snapshot_provenance_and_full_refresh_generation_range() {
        let migration = include_str!("../migrations/0001_phase2_auth_save.sql");
        assert!(
            migration
                .contains("generation bigint NOT NULL CHECK (generation BETWEEN 0 AND 4294967295)")
        );
        assert!(migration.contains("phase2_finalized_snapshot_immutable"));
        assert!(migration.contains("finalized snapshot provenance is immutable"));
        assert!(migration.contains("parent_snapshot := OLD.snapshot_id"));
        assert!(migration.contains("artifact cannot be moved into a finalized snapshot"));
        assert!(migration.contains("NEW.snapshot_id IS DISTINCT FROM OLD.snapshot_id"));
        assert!(migration.contains("NEW.artifact IS DISTINCT FROM OLD.artifact"));
        assert!(migration.contains("snapshot artifact identity cannot be changed"));
        assert!(migration.contains("NEW.object_key IS DISTINCT FROM expected_key"));
        assert!(migration.contains("characters/%s/snapshots/%s/%s"));
    }
}

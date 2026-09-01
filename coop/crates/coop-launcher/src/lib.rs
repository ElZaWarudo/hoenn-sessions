//! Secure local orchestration for one fenced cloud-coop session.

#![forbid(unsafe_code)]

pub mod auth;
pub mod compat;
pub mod epoch;
pub mod keychain;
pub mod process;
pub mod session;

pub use auth::{AuthApi, AuthError, AuthSession};
pub use compat::{BuildCompatibility, CompatibilityError};
pub use epoch::{EpochError, EpochRecord, EpochStore};
pub use keychain::{KeychainError, OsKeychain, RefreshTokenStore};
pub use process::{
    CommandSpec, ControlChannel, ProcessError, SupervisedChildren, SupervisorEvent,
    materialize_bridge_session,
};
pub use session::{CloudApi, SessionConfig, SessionError, SessionLifecycle, SessionWorkspace};

use std::time::Duration;

use coop_cloud::{
    AcquireLeaseRequest, ArtifactIdentity, CharacterId, HeartbeatLeaseRequest, LeaseContract,
    LeaseFence, LoginRequest, LoginResponse, LogoutRequest, LogoutResponse, PrepareSnapshotRequest,
    ReconnectLeaseRequest, RefreshRequest, RefreshResponse, ReleaseLeaseRequest, Revision,
    SignedManifestEnvelope, SnapshotFinalizeRequest, SnapshotListRequest, SnapshotListResponse,
    SnapshotPrepareResponse, SnapshotRecord, SnapshotRestoreRequest, SnapshotRestoreResponse,
    UploadTarget,
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

type AResult<'a, T> = auth::AuthFuture<'a, T>;

impl AuthApi for ReqwestCloudApi {
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
            .map_err(map_cloud_error)
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
        artifact: ArtifactIdentity,
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
    use super::{HttpClientError, ReqwestCloudApi, bounded_body, map_cloud_error};
    use crate::{AuthError, AuthSession, CloudApi, RefreshTokenStore, SessionError};
    use coop_cloud::{
        AccessToken, ArtifactIdentity, CharacterId, ClientInstanceId, HeartbeatLeaseRequest,
        LeaseContract, LeaseFence, LoginResponse, Password, RefreshFamilyId, RefreshToken,
        SessionEpoch, SessionId, UnixTimestampMillis, UploadTarget, UserId,
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
            ArtifactIdentity::CharacterSav,
            format!("http://127.0.0.1:{}/upload?capability=test", address.port()),
            UnixTimestampMillis::new(4_000_000_000_000),
        )
        .unwrap();
        api.upload(&target, b"save-bytes".to_vec()).await.unwrap();
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

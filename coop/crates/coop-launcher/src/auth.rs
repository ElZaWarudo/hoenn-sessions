//! Username authentication and memory-only access-token management.

use std::{
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    AccessToken, CharacterId, LeaseFence, LoginRequest, LoginResponse, LogoutRequest,
    LogoutResponse, Password, RefreshRequest, RefreshResponse, RefreshToken, UserId, Username,
};
use thiserror::Error;

use crate::keychain::{KEYCHAIN_SERVICE, KeychainError, RefreshTokenStore};

pub type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AuthError>> + Send + 'a>>;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("authentication transport failed")]
    Transport,
    #[error("authentication response was invalid")]
    InvalidResponse,
    #[error("credential vault operation failed")]
    Keychain(#[from] KeychainError),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("refresh token expired")]
    RefreshExpired,
    #[error("authentication session is no longer active")]
    SessionClosed,
}

/// Authentication transport implemented by the cloud HTTP adapter or tests.
pub trait AuthApi: Send + Sync {
    /// Sends a username/password login request.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when transport or response validation fails.
    fn login(&self, request: LoginRequest) -> AuthFuture<'_, LoginResponse>;
    /// Rotates an opaque refresh token.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when the token is rejected or transport fails.
    fn refresh(&self, request: RefreshRequest) -> AuthFuture<'_, RefreshResponse>;
    /// Revokes a refresh family.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when the request cannot be delivered.
    fn logout(&self, request: LogoutRequest) -> AuthFuture<'_, LogoutResponse>;
}

/// Access is intentionally memory-only; only the rotating refresh token is
/// handed to the OS-keychain abstraction.
pub struct AuthSession {
    pub user_id: UserId,
    pub character_id: CharacterId,
    pub username: Username,
    access_token: Option<AccessToken>,
    refresh_token: Option<RefreshToken>,
    access_expires_at: u64,
    refresh_expires_at: u64,
    active_fence: Option<LeaseFence>,
}

impl std::fmt::Debug for AuthSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthSession")
            .field("user_id", &self.user_id)
            .field("character_id", &self.character_id)
            .field("username", &self.username)
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("access_expires_at", &self.access_expires_at)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .field("active_fence", &self.active_fence)
            .finish()
    }
}

impl AuthSession {
    /// Performs password login and immediately persists the rotated refresh token.
    ///
    /// # Errors
    ///
    /// Returns an error when the username/password, cloud response, or keychain
    /// operation is invalid.
    pub async fn login(
        api: &impl AuthApi,
        keychain: &(impl RefreshTokenStore + ?Sized),
        username: impl Into<String>,
        password: Password,
    ) -> Result<Self, AuthError> {
        let username = Username::new(username).map_err(|_| AuthError::InvalidCredentials)?;
        let request = LoginRequest::new(username.as_str(), password)
            .map_err(|_| AuthError::InvalidCredentials)?;
        let response = api.login(request).await?;
        response
            .validate()
            .map_err(|_| AuthError::InvalidResponse)?;
        keychain.store(KEYCHAIN_SERVICE, username.as_str(), &response.refresh_token)?;
        Ok(Self::from_login(username, response))
    }

    /// Rehydrates a session from an OS-keychain refresh token. The access
    /// token returned by the server is never persisted.
    ///
    /// # Errors
    ///
    /// Returns an error when the keychain, cloud response, or server token is invalid.
    pub async fn refresh_from_keychain(
        api: &impl AuthApi,
        keychain: &(impl RefreshTokenStore + ?Sized),
        username: impl Into<String>,
        user_id: UserId,
        character_id: CharacterId,
    ) -> Result<Self, AuthError> {
        let username = Username::new(username).map_err(|_| AuthError::InvalidCredentials)?;
        let refresh = keychain
            .load(KEYCHAIN_SERVICE, username.as_str())?
            .ok_or(AuthError::RefreshExpired)?;
        let response = api.refresh(RefreshRequest::new(refresh)).await?;
        response
            .validate()
            .map_err(|_| AuthError::InvalidResponse)?;
        keychain.store(KEYCHAIN_SERVICE, username.as_str(), &response.refresh_token)?;
        Ok(Self {
            user_id,
            character_id,
            username,
            access_token: Some(response.access_token),
            refresh_token: Some(response.refresh_token),
            access_expires_at: response.access_expires_at.value(),
            refresh_expires_at: response.refresh_expires_at.value(),
            active_fence: None,
        })
    }

    fn from_login(username: Username, response: LoginResponse) -> Self {
        Self {
            user_id: response.user_id,
            character_id: response.character_id,
            username,
            access_token: Some(response.access_token),
            refresh_token: Some(response.refresh_token),
            access_expires_at: response.access_expires_at.value(),
            refresh_expires_at: response.refresh_expires_at.value(),
            active_fence: None,
        }
    }

    #[must_use]
    /// Returns the live bearer token, or `None` after logout cleanup.
    pub fn access_token(&self) -> Option<&AccessToken> {
        self.access_token.as_ref()
    }

    #[must_use]
    /// Returns the live refresh token, or `None` after logout cleanup.
    pub fn refresh_token(&self) -> Option<&RefreshToken> {
        self.refresh_token.as_ref()
    }

    #[must_use]
    pub const fn access_expires_at(&self) -> u64 {
        self.access_expires_at
    }

    #[must_use]
    pub const fn refresh_expires_at(&self) -> u64 {
        self.refresh_expires_at
    }

    /// Associates the short-lived access token with the currently held cloud
    /// lease. The fence is non-secret and is used by the HTTP adapter on
    /// package/artifact requests that do not carry a DTO body.
    pub(crate) fn set_active_fence(&mut self, fence: LeaseFence) {
        self.active_fence = Some(fence);
    }

    #[must_use]
    pub(crate) const fn active_fence(&self) -> Option<LeaseFence> {
        self.active_fence
    }

    #[must_use]
    pub fn should_refresh_at(&self, now_millis: u64) -> bool {
        self.access_token.is_some() && now_millis.saturating_add(30_000) >= self.access_expires_at
    }

    /// Rotates access and refresh credentials and atomically updates the vault entry.
    ///
    /// # Errors
    ///
    /// Returns an error when refresh expiry, cloud response, or keychain storage fails.
    pub async fn refresh(
        &mut self,
        api: &impl AuthApi,
        keychain: &(impl RefreshTokenStore + ?Sized),
    ) -> Result<(), AuthError> {
        self.refresh_at(api, keychain, now_millis()).await
    }

    /// Rotates credentials using a caller-supplied clock.  The explicit clock
    /// keeps schedulers and deterministic tests from making expiry decisions
    /// from an untrusted client timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when refresh expiry, cloud response, or keychain
    /// storage fails.  The in-memory pair is changed only after the new
    /// refresh token has been durably accepted by the keychain.
    pub async fn refresh_at(
        &mut self,
        api: &impl AuthApi,
        keychain: &(impl RefreshTokenStore + ?Sized),
        now_millis: u64,
    ) -> Result<(), AuthError> {
        let refresh_token = self.refresh_token.clone().ok_or(AuthError::SessionClosed)?;
        if now_millis >= self.refresh_expires_at {
            return Err(AuthError::RefreshExpired);
        }
        let response = api.refresh(RefreshRequest::new(refresh_token)).await?;
        response
            .validate()
            .map_err(|_| AuthError::InvalidResponse)?;
        keychain.store(
            KEYCHAIN_SERVICE,
            self.username.as_str(),
            &response.refresh_token,
        )?;
        self.access_token = Some(response.access_token);
        self.refresh_token = Some(response.refresh_token);
        self.access_expires_at = response.access_expires_at.value();
        self.refresh_expires_at = response.refresh_expires_at.value();
        Ok(())
    }

    /// Logs out and removes the keychain entry even when server revocation
    /// cannot be delivered.  A failed remote revoke must never strand the
    /// persistent refresh credential locally.
    ///
    /// # Errors
    ///
    /// Returns an error when revocation or keychain deletion fails.
    pub async fn logout(
        &mut self,
        api: &impl AuthApi,
        keychain: &(impl RefreshTokenStore + ?Sized),
    ) -> Result<(), AuthError> {
        let refresh_token = self.refresh_token.clone().ok_or(AuthError::SessionClosed)?;
        // Delete persistent credentials and clear the in-memory wrappers
        // before the first cancellation point. A stalled or cancelled remote
        // revoke must not leave a locally usable session behind.
        let local = keychain.delete(KEYCHAIN_SERVICE, self.username.as_str());
        self.clear_credentials();
        let remote = api.logout(LogoutRequest::new(refresh_token)).await;
        match local {
            // A local deletion failure is the actionable result even when
            // remote revocation also failed: the persistent secret may remain.
            Err(error) => Err(AuthError::Keychain(error)),
            Ok(()) => remote.map(|_| ()),
        }
    }

    fn clear_credentials(&mut self) {
        self.access_token = None;
        self.refresh_token = None;
        self.access_expires_at = 0;
        self.refresh_expires_at = 0;
        self.active_fence = None;
    }

    /// Creates a request only after copying the zeroizing password into the
    /// cloud contract; dropping the request releases the password immediately.
    ///
    /// # Errors
    ///
    /// Returns an error when the password does not satisfy the cloud contract.
    pub fn password(value: impl Into<String>) -> Result<Password, AuthError> {
        Password::new(value.into()).map_err(|_| AuthError::InvalidCredentials)
    }
}

fn now_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use coop_cloud::{RefreshFamilyId, UnixTimestampMillis};
    use tokio::sync::Notify;
    use uuid::Uuid;

    use super::*;

    #[derive(Clone)]
    struct LogoutApi {
        fail_remote: bool,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl AuthApi for LogoutApi {
        fn login(&self, _request: LoginRequest) -> AuthFuture<'_, LoginResponse> {
            Box::pin(async { Err(AuthError::Transport) })
        }

        fn refresh(&self, _request: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
            Box::pin(async { Err(AuthError::Transport) })
        }

        fn logout(&self, request: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
            let event = format!("revoke:{}", request.refresh_token.expose_secret());
            let events = Arc::clone(&self.events);
            let fail_remote = self.fail_remote;
            Box::pin(async move {
                events
                    .lock()
                    .expect("test events lock poisoned")
                    .push(event);
                if fail_remote {
                    Err(AuthError::Transport)
                } else {
                    Ok(LogoutResponse::default())
                }
            })
        }
    }

    #[derive(Clone)]
    struct HangingLogoutApi {
        started: Arc<Notify>,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl AuthApi for HangingLogoutApi {
        fn login(&self, _request: LoginRequest) -> AuthFuture<'_, LoginResponse> {
            Box::pin(async { Err(AuthError::Transport) })
        }

        fn refresh(&self, _request: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
            Box::pin(async { Err(AuthError::Transport) })
        }

        fn logout(&self, request: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
            let started = Arc::clone(&self.started);
            let events = Arc::clone(&self.events);
            Box::pin(async move {
                events
                    .lock()
                    .expect("test events lock poisoned")
                    .push(format!(
                        "revoke-start:{}",
                        request.refresh_token.expose_secret()
                    ));
                started.notify_one();
                std::future::pending::<Result<LogoutResponse, AuthError>>().await
            })
        }
    }

    struct DeleteKeychain {
        fail_delete: bool,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl RefreshTokenStore for DeleteKeychain {
        fn load(
            &self,
            _service: &str,
            _username: &str,
        ) -> Result<Option<RefreshToken>, KeychainError> {
            Ok(None)
        }

        fn store(
            &self,
            _service: &str,
            _username: &str,
            _token: &RefreshToken,
        ) -> Result<(), KeychainError> {
            Ok(())
        }

        fn delete(&self, _service: &str, username: &str) -> Result<(), KeychainError> {
            self.events
                .lock()
                .expect("test events lock poisoned")
                .push(format!("delete:{username}"));
            if self.fail_delete {
                Err(KeychainError::Operation)
            } else {
                Ok(())
            }
        }
    }

    fn session() -> AuthSession {
        let response = LoginResponse::new(
            UserId::new(Uuid::from_u128(1)).expect("test user id is non-nil"),
            CharacterId::new(Uuid::from_u128(2)).expect("test character id is non-nil"),
            AccessToken::new("old-access").expect("test access token is valid"),
            RefreshToken::new("old-refresh").expect("test refresh token is valid"),
            RefreshFamilyId::new(Uuid::from_u128(3)).expect("test family id is non-nil"),
            UnixTimestampMillis::new(200_000),
            UnixTimestampMillis::new(300_000),
        )
        .expect("test login response is valid");
        AuthSession::from_login(
            Username::new("ash").expect("test username is valid"),
            response,
        )
    }

    fn assert_scrubbed(session: &AuthSession) {
        assert!(session.access_token.is_none());
        assert!(session.refresh_token.is_none());
        assert_eq!(session.access_expires_at, 0);
        assert_eq!(session.refresh_expires_at, 0);
        assert!(session.active_fence.is_none());
        assert!(!session.should_refresh_at(u64::MAX));
        assert!(session.access_token().is_none());
        assert!(session.refresh_token().is_none());
    }

    #[tokio::test]
    async fn logout_local_delete_failure_wins_over_remote_failure_and_scrubs() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let api = LogoutApi {
            fail_remote: true,
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: true,
            events: Arc::clone(&events),
        };
        let mut session = session();

        let error = session
            .logout(&api, &keychain)
            .await
            .expect_err("logout must fail");

        assert!(matches!(
            error,
            AuthError::Keychain(KeychainError::Operation)
        ));
        assert_eq!(
            events.lock().expect("test events lock poisoned").as_slice(),
            ["delete:ash".to_owned(), "revoke:old-refresh".to_owned()]
        );
        assert_scrubbed(&session);
    }

    #[tokio::test]
    async fn logout_remote_failure_still_deletes_and_scrubs_in_memory_credentials() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let api = LogoutApi {
            fail_remote: true,
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: false,
            events: Arc::clone(&events),
        };
        let mut session = session();

        let error = session
            .logout(&api, &keychain)
            .await
            .expect_err("revoke must fail");

        assert!(matches!(error, AuthError::Transport));
        assert_eq!(
            events.lock().expect("test events lock poisoned").as_slice(),
            ["delete:ash".to_owned(), "revoke:old-refresh".to_owned()]
        );
        assert_scrubbed(&session);
    }

    #[tokio::test]
    async fn cancelling_remote_logout_cannot_restore_local_credentials() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let started = Arc::new(Notify::new());
        let api = HangingLogoutApi {
            started: Arc::clone(&started),
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: false,
            events: Arc::clone(&events),
        };
        let mut session = session();
        let mut logout = Box::pin(session.logout(&api, &keychain));

        tokio::select! {
            result = &mut logout => panic!("remote logout unexpectedly completed: {result:?}"),
            () = started.notified() => {}
        }
        drop(logout);

        assert_eq!(
            events.lock().expect("test events lock poisoned").as_slice(),
            [
                "delete:ash".to_owned(),
                "revoke-start:old-refresh".to_owned()
            ]
        );
        assert_scrubbed(&session);
    }

    #[tokio::test]
    async fn logout_sends_the_current_refresh_token_before_local_delete() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let api = LogoutApi {
            fail_remote: false,
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: false,
            events: Arc::clone(&events),
        };
        let mut session = session();

        session
            .logout(&api, &keychain)
            .await
            .expect("logout succeeds");

        assert_eq!(
            events.lock().expect("test events lock poisoned").as_slice(),
            ["delete:ash".to_owned(), "revoke:old-refresh".to_owned()]
        );
        assert_scrubbed(&session);
    }

    #[tokio::test]
    async fn logout_after_credentials_are_cleared_returns_typed_session_closed() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let api = LogoutApi {
            fail_remote: false,
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: false,
            events,
        };
        let mut session = session();

        session
            .logout(&api, &keychain)
            .await
            .expect("first logout succeeds");
        let error = session
            .logout(&api, &keychain)
            .await
            .expect_err("second logout must not pretend to revoke a missing token");

        assert!(matches!(error, AuthError::SessionClosed));
        assert_scrubbed(&session);
    }

    #[tokio::test]
    async fn refresh_after_logout_reports_session_closed_before_expiry() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let api = LogoutApi {
            fail_remote: false,
            events: Arc::clone(&events),
        };
        let keychain = DeleteKeychain {
            fail_delete: false,
            events,
        };
        let mut session = session();

        session
            .logout(&api, &keychain)
            .await
            .expect("logout succeeds");
        let error = session
            .refresh_at(&api, &keychain, u64::MAX)
            .await
            .expect_err("closed credentials cannot refresh");

        assert!(matches!(error, AuthError::SessionClosed));
    }
}

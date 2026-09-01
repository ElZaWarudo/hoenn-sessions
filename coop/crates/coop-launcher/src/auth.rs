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
    access_token: AccessToken,
    refresh_token: RefreshToken,
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
            access_token: response.access_token,
            refresh_token: response.refresh_token,
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
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            access_expires_at: response.access_expires_at.value(),
            refresh_expires_at: response.refresh_expires_at.value(),
            active_fence: None,
        }
    }

    #[must_use]
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }

    #[must_use]
    pub fn refresh_token(&self) -> &RefreshToken {
        &self.refresh_token
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
        now_millis.saturating_add(30_000) >= self.access_expires_at
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
        if now_millis >= self.refresh_expires_at {
            return Err(AuthError::RefreshExpired);
        }
        let response = api
            .refresh(RefreshRequest::new(self.refresh_token.clone()))
            .await?;
        response
            .validate()
            .map_err(|_| AuthError::InvalidResponse)?;
        keychain.store(
            KEYCHAIN_SERVICE,
            self.username.as_str(),
            &response.refresh_token,
        )?;
        self.access_token = response.access_token;
        self.refresh_token = response.refresh_token;
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
        let remote = api
            .logout(LogoutRequest::new(self.refresh_token.clone()))
            .await;
        let local = keychain.delete(KEYCHAIN_SERVICE, self.username.as_str());
        match remote {
            Err(error) => Err(error),
            Ok(_) => local.map_err(AuthError::from),
        }
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

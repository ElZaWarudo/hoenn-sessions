//! Versioned, transport-neutral authentication contracts.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{
    AccessToken, CharacterId, InvitationCode, RefreshFamilyId, RefreshToken, SecretError,
    UnixTimestampMillis, UserId, ids::IdError, ids::deserialize_bounded_string, security::Password,
};

/// The only authentication API version currently accepted by this crate.
pub const AUTH_API_VERSION: u16 = 1;

/// A version marker that rejects unknown API versions during deserialization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApiVersion(u16);

impl ApiVersion {
    pub const V1: Self = Self(AUTH_API_VERSION);

    /// Creates the supported API version marker.
    ///
    /// # Errors
    ///
    /// Returns an error when an unknown version is supplied.
    pub fn new(value: u16) -> Result<Self, AuthError> {
        if value == AUTH_API_VERSION {
            Ok(Self(value))
        } else {
            Err(AuthError::UnknownApiVersion(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self::V1
    }
}

impl Serialize for ApiVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for ApiVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u16::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Authentication validation failures.
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum AuthError {
    #[error("unknown API version {0}")]
    UnknownApiVersion(u16),
    #[error("username must be 3..=32 ASCII bytes with alphanumeric boundaries")]
    InvalidUsername,
    #[error("secret validation failed: {0}")]
    Secret(#[from] SecretError),
    #[error("identifier validation failed: {0}")]
    Identifier(#[from] IdError),
    #[error("token expiry must be a non-zero timestamp")]
    InvalidExpiry,
}

/// A canonical, case-insensitive username identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Username(String);

impl Username {
    /// Validates and lowercases a username.
    ///
    /// # Errors
    ///
    /// Returns an error when the username violates its length, boundary, or
    /// ASCII alphabet invariant.
    pub fn new(value: impl Into<String>) -> Result<Self, AuthError> {
        let value = value.into();
        if !(3..=32).contains(&value.len())
            || !value.is_ascii()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            || !value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            return Err(AuthError::InvalidUsername);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for Username {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Username {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(deserializer, 32, "username")?)
            .map_err(serde::de::Error::custom)
    }
}

/// Invite-gated account registration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub api_version: ApiVersion,
    pub username: Username,
    pub password: Password,
    pub invitation_code: InvitationCode,
}

impl RegisterRequest {
    /// Builds a version-one invite-gated registration request.
    ///
    /// # Errors
    ///
    /// Returns an error when the username or secret wrappers are invalid.
    pub fn new(
        username: impl Into<String>,
        password: Password,
        invitation_code: InvitationCode,
    ) -> Result<Self, AuthError> {
        Ok(Self {
            api_version: ApiVersion::V1,
            username: Username::new(username)?,
            password,
            invitation_code,
        })
    }

    /// Revalidates all fields, including the explicit API version.
    ///
    /// # Errors
    ///
    /// Returns an error when any field violates its invariant.
    pub fn validate(&self) -> Result<(), AuthError> {
        let _ = Username::new(self.username.0.clone())?;
        Password::new(self.password.expose_secret().to_owned())?;
        InvitationCode::new(self.invitation_code.expose_secret().to_owned())?;
        ApiVersion::new(self.api_version.value())?;
        Ok(())
    }
}

/// Registration response carrying stable account and character IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterResponse {
    pub api_version: ApiVersion,
    pub user_id: UserId,
    pub character_id: CharacterId,
}

impl RegisterResponse {
    #[must_use]
    pub const fn new(user_id: UserId, character_id: CharacterId) -> Self {
        Self {
            api_version: ApiVersion::V1,
            user_id,
            character_id,
        }
    }
}

/// Password login request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub api_version: ApiVersion,
    pub username: Username,
    pub password: Password,
}

impl LoginRequest {
    /// Builds a version-one password login request.
    ///
    /// # Errors
    ///
    /// Returns an error when the username or password is invalid.
    pub fn new(username: impl Into<String>, password: Password) -> Result<Self, AuthError> {
        Ok(Self {
            api_version: ApiVersion::V1,
            username: Username::new(username)?,
            password,
        })
    }
}

/// Successful login response. Token rotation is owned by the server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginResponse {
    pub api_version: ApiVersion,
    pub user_id: UserId,
    pub character_id: CharacterId,
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub refresh_family_id: RefreshFamilyId,
    pub access_expires_at: UnixTimestampMillis,
    pub refresh_expires_at: UnixTimestampMillis,
}

#[derive(Serialize)]
struct SerializableLoginResponse<'a> {
    api_version: ApiVersion,
    user_id: UserId,
    character_id: CharacterId,
    access_token: &'a AccessToken,
    refresh_token: &'a RefreshToken,
    refresh_family_id: RefreshFamilyId,
    access_expires_at: UnixTimestampMillis,
    refresh_expires_at: UnixTimestampMillis,
}

impl Serialize for LoginResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableLoginResponse {
            api_version: self.api_version,
            user_id: self.user_id,
            character_id: self.character_id,
            access_token: &self.access_token,
            refresh_token: &self.refresh_token,
            refresh_family_id: self.refresh_family_id,
            access_expires_at: self.access_expires_at,
            refresh_expires_at: self.refresh_expires_at,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireLoginResponse {
    api_version: ApiVersion,
    user_id: UserId,
    character_id: CharacterId,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    refresh_family_id: RefreshFamilyId,
    access_expires_at: UnixTimestampMillis,
    refresh_expires_at: UnixTimestampMillis,
}

impl<'de> Deserialize<'de> for LoginResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireLoginResponse::deserialize(deserializer)?;
        let response = Self {
            api_version: wire.api_version,
            user_id: wire.user_id,
            character_id: wire.character_id,
            access_token: wire.access_token,
            refresh_token: wire.refresh_token,
            refresh_family_id: wire.refresh_family_id,
            access_expires_at: wire.access_expires_at,
            refresh_expires_at: wire.refresh_expires_at,
        };
        response.validate().map_err(serde::de::Error::custom)?;
        Ok(response)
    }
}

impl LoginResponse {
    /// Builds a response with server-authoritative token expiry timestamps.
    ///
    /// # Errors
    ///
    /// Returns an error when either expiry timestamp is zero.
    pub fn new(
        user_id: UserId,
        character_id: CharacterId,
        access_token: AccessToken,
        refresh_token: RefreshToken,
        refresh_family_id: RefreshFamilyId,
        access_expires_at: UnixTimestampMillis,
        refresh_expires_at: UnixTimestampMillis,
    ) -> Result<Self, AuthError> {
        let response = Self {
            api_version: ApiVersion::V1,
            user_id,
            character_id,
            access_token,
            refresh_token,
            refresh_family_id,
            access_expires_at,
            refresh_expires_at,
        };
        response.validate()?;
        Ok(response)
    }

    /// Validates server response metadata before a client consumes it.
    ///
    /// # Errors
    ///
    /// Returns an error when the API version is unknown or either expiry is
    /// zero.
    pub fn validate(&self) -> Result<(), AuthError> {
        ApiVersion::new(self.api_version.value())?;
        if self.access_expires_at.value() == 0 || self.refresh_expires_at.value() == 0 {
            return Err(AuthError::InvalidExpiry);
        }
        Ok(())
    }
}

/// Refresh-token rotation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    pub api_version: ApiVersion,
    pub refresh_token: RefreshToken,
}

impl RefreshRequest {
    #[must_use]
    pub fn new(refresh_token: RefreshToken) -> Self {
        Self {
            api_version: ApiVersion::V1,
            refresh_token,
        }
    }
}

/// Rotated access and refresh tokens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshResponse {
    pub api_version: ApiVersion,
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub refresh_family_id: RefreshFamilyId,
    pub access_expires_at: UnixTimestampMillis,
    pub refresh_expires_at: UnixTimestampMillis,
}

#[derive(Serialize)]
struct SerializableRefreshResponse<'a> {
    api_version: ApiVersion,
    access_token: &'a AccessToken,
    refresh_token: &'a RefreshToken,
    refresh_family_id: RefreshFamilyId,
    access_expires_at: UnixTimestampMillis,
    refresh_expires_at: UnixTimestampMillis,
}

impl Serialize for RefreshResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableRefreshResponse {
            api_version: self.api_version,
            access_token: &self.access_token,
            refresh_token: &self.refresh_token,
            refresh_family_id: self.refresh_family_id,
            access_expires_at: self.access_expires_at,
            refresh_expires_at: self.refresh_expires_at,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRefreshResponse {
    api_version: ApiVersion,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    refresh_family_id: RefreshFamilyId,
    access_expires_at: UnixTimestampMillis,
    refresh_expires_at: UnixTimestampMillis,
}

impl<'de> Deserialize<'de> for RefreshResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireRefreshResponse::deserialize(deserializer)?;
        let response = Self {
            api_version: wire.api_version,
            access_token: wire.access_token,
            refresh_token: wire.refresh_token,
            refresh_family_id: wire.refresh_family_id,
            access_expires_at: wire.access_expires_at,
            refresh_expires_at: wire.refresh_expires_at,
        };
        response.validate().map_err(serde::de::Error::custom)?;
        Ok(response)
    }
}

impl RefreshResponse {
    /// Builds a rotated token response with server-authoritative expiries.
    ///
    /// # Errors
    ///
    /// Returns an error when either expiry timestamp is zero.
    pub fn new(
        access_token: AccessToken,
        refresh_token: RefreshToken,
        refresh_family_id: RefreshFamilyId,
        access_expires_at: UnixTimestampMillis,
        refresh_expires_at: UnixTimestampMillis,
    ) -> Result<Self, AuthError> {
        let response = Self {
            api_version: ApiVersion::V1,
            access_token,
            refresh_token,
            refresh_family_id,
            access_expires_at,
            refresh_expires_at,
        };
        response.validate()?;
        Ok(response)
    }

    /// Validates server response metadata before a client consumes it.
    ///
    /// # Errors
    ///
    /// Returns an error when the API version is unknown or either expiry is
    /// zero.
    pub fn validate(&self) -> Result<(), AuthError> {
        ApiVersion::new(self.api_version.value())?;
        if self.access_expires_at.value() == 0 || self.refresh_expires_at.value() == 0 {
            return Err(AuthError::InvalidExpiry);
        }
        Ok(())
    }
}

/// Logout/revocation request. The server revokes the presented family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoutRequest {
    pub api_version: ApiVersion,
    pub refresh_token: RefreshToken,
}

impl LogoutRequest {
    #[must_use]
    pub fn new(refresh_token: RefreshToken) -> Self {
        Self {
            api_version: ApiVersion::V1,
            refresh_token,
        }
    }
}

/// A response with no body beyond the negotiated API version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoutResponse {
    pub api_version: ApiVersion,
}

impl Default for LogoutResponse {
    fn default() -> Self {
        Self {
            api_version: ApiVersion::V1,
        }
    }
}

/// Backwards-compatible spelling for callers that use “registration”.
pub type RegistrationRequest = RegisterRequest;
/// Backwards-compatible spelling for a registration response.
pub type RegistrationResponse = RegisterResponse;
/// Explicitly named refresh-token request alias.
pub type RefreshTokenRequest = RefreshRequest;
/// Explicitly named refresh-token response alias.
pub type RefreshTokenResponse = RefreshResponse;

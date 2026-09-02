//! Validated identifiers and scalar values used at the cloud boundary.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Deserialize text with a byte bound before taking ownership of borrowed
/// input.  For formats that already provide an owned string (for example an
/// escaped JSON value), the bound is checked before returning it so callers do
/// not make a second unbounded copy.  Consumers should still enforce an outer
/// request/package size limit before invoking serde.
pub(crate) fn deserialize_bounded_string<'de, D>(
    deserializer: D,
    maximum: usize,
    description: &'static str,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoundedStringVisitor {
        maximum: usize,
        description: &'static str,
    }

    impl<'de> serde::de::Visitor<'de> for BoundedStringVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "{} of at most {} bytes",
                self.description, self.maximum
            )
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.len() > self.maximum {
                return Err(E::custom(format!(
                    "{} exceeds {} bytes",
                    self.description, self.maximum
                )));
            }
            Ok(value.to_owned())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.len() > self.maximum {
                return Err(E::custom(format!(
                    "{} exceeds {} bytes",
                    self.description, self.maximum
                )));
            }
            Ok(value.to_owned())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.len() > self.maximum {
                return Err(E::custom(format!(
                    "{} exceeds {} bytes",
                    self.description, self.maximum
                )));
            }
            Ok(value)
        }
    }

    deserializer.deserialize_string(BoundedStringVisitor {
        maximum,
        description,
    })
}

macro_rules! scalar_serde {
    ($name:ident, $type:ty) => {
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_u64(u64::from(self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::new(<$type>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Errors returned by the validated cloud contract types.
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum IdError {
    #[error("UUID must be a non-nil UUID")]
    InvalidUuid,
    #[error("invalid UUID: {0}")]
    MalformedUuid(String),
    #[error("revision overflow")]
    RevisionOverflow,
    #[error("session epoch must be non-zero")]
    ZeroSessionEpoch,
    #[error("value must be non-zero")]
    ZeroValue,
    #[error("SHA-256 digest must contain exactly 64 lowercase hexadecimal characters")]
    InvalidSha256,
    #[error("version must be non-zero")]
    ZeroVersion,
    #[error("unsupported version {0}; only version 1 is implemented")]
    UnsupportedVersion(u16),
    #[error("game build identity must be 1..=128 ASCII build characters")]
    InvalidGameBuildId,
    #[error("mGBA version must be canonical major.minor.patch")]
    InvalidMgbaVersion,
}

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates an identifier from a non-nil UUID.
            ///
            /// # Errors
            ///
            /// Returns an error for a nil UUID.
            pub fn new(value: Uuid) -> Result<Self, IdError> {
                if value.is_nil() {
                    return Err(IdError::InvalidUuid);
                }
                Ok(Self(value))
            }

            /// Parses the canonical lowercase hyphenated UUID spelling.
            ///
            /// # Errors
            ///
            /// Returns an error for malformed or nil UUID text.
            pub fn parse(value: &str) -> Result<Self, IdError> {
                let uuid = Uuid::parse_str(value)
                    .map_err(|error| IdError::MalformedUuid(error.to_string()))?;
                if uuid.hyphenated().to_string() != value {
                    return Err(IdError::MalformedUuid(
                        "UUID is not canonical lowercase hyphenated form".to_owned(),
                    ));
                }
                Self::new(uuid)
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }

            /// Alias for `as_uuid` used by persistence adapters.
            #[must_use]
            pub const fn uuid(self) -> Uuid {
                self.as_uuid()
            }

            #[must_use]
            pub fn as_str(&self) -> String {
                self.0.hyphenated().to_string()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = deserialize_bounded_string(deserializer, 36, "UUID")?;
                Self::parse(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

uuid_id!(
    /// A stable account identifier.
    UserId
);
uuid_id!(
    /// A stable character identifier.
    CharacterId
);
uuid_id!(
    /// A launcher/server session identifier.
    SessionId
);
uuid_id!(
    /// A cloud snapshot identifier.
    SnapshotId
);
uuid_id!(
    /// A unique launcher installation identifier.
    ClientInstanceId
);
uuid_id!(
    /// The family identifier used to fence refresh-token rotation.
    RefreshFamilyId
);
uuid_id!(
    /// An idempotent commit identifier.
    CommitId
);
uuid_id!(
    /// A request idempotency key.
    IdempotencyKey
);
uuid_id!(
    /// A server-issued symmetric group identifier.
    GroupId
);
uuid_id!(
    /// A server-issued group invitation identifier.
    GroupInvitationId
);

/// A monotonically increasing snapshot or character revision.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(u64);

impl Revision {
    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn from_u64(value: u64) -> Self {
        Self::new(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Advances by exactly one revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision is already `u64::MAX`.
    pub fn next(self) -> Result<Self, IdError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(IdError::RevisionOverflow)
    }

    /// Returns whether this is the initial, pre-snapshot revision.
    #[must_use]
    pub const fn is_initial(self) -> bool {
        self.0 == 0
    }
}

impl Serialize for Revision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(u64::deserialize(deserializer)?))
    }
}

/// A non-zero launcher session epoch used for fencing stale processes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionEpoch(u32);

impl SessionEpoch {
    /// Creates a non-zero epoch.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero.
    pub fn new(value: u32) -> Result<Self, IdError> {
        ensure_nonzero(value)
            .map_err(|_| IdError::ZeroSessionEpoch)
            .map(Self)
    }

    /// Alias for `new` when reading a wire epoch.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero.
    pub fn from_u32(value: u32) -> Result<Self, IdError> {
        Self::new(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

impl Serialize for SessionEpoch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for SessionEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A UTC Unix timestamp represented as milliseconds.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnixTimestampMillis(u64);

impl UnixTimestampMillis {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl Serialize for UnixTimestampMillis {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for UnixTimestampMillis {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(u64::deserialize(deserializer)?))
    }
}

/// Short alias used by snapshot and session consumers.
pub type Timestamp = UnixTimestampMillis;
/// Alias clarifying that a revision identifies a finalized snapshot.
pub type SnapshotRevision = Revision;

/// A strict, lowercase hexadecimal SHA-256 digest.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Hashes bytes with SHA-256.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Creates a digest from exactly 32 raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parses exactly 64 lowercase hexadecimal characters.
    ///
    /// # Errors
    ///
    /// Returns an error for incomplete, uppercase, or non-hexadecimal text.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(IdError::InvalidSha256);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        Ok(Self(bytes))
    }

    /// Alias for `parse` used by JSON adapters.
    ///
    /// # Errors
    ///
    /// Returns an error when the text is not a strict lowercase digest.
    pub fn from_hex(value: &str) -> Result<Self, IdError> {
        Self::parse(value)
    }

    #[must_use]
    pub fn as_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            output.push(char::from(HEX[(byte >> 4) as usize]));
            output.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
        output
    }

    #[must_use]
    pub fn as_str(&self) -> String {
        self.as_hex()
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Sha256Digest")
            .field(&self.as_hex())
            .finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_hex())
    }
}

impl FromStr for Sha256Digest {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(&deserialize_bounded_string(
            deserializer,
            64,
            "SHA-256 digest",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

/// Bounded textual ROM build identity (for example `emerald-coop-0.1.0+abc1234`).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GameBuildId(String);

impl GameBuildId {
    /// Creates a bounded textual build identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the text is empty, oversized, or contains a
    /// character outside the build identity alphabet.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+:-".contains(&byte))
        {
            return Err(IdError::InvalidGameBuildId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GameBuildId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for GameBuildId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GameBuildId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(
            deserializer,
            128,
            "game build ID",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

/// Bridge ABI version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BridgeAbiVersion(u16);

impl BridgeAbiVersion {
    /// Creates a non-zero bridge ABI version.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero.
    pub fn new(value: u16) -> Result<Self, IdError> {
        if value == 1 {
            Ok(Self(value))
        } else {
            Err(IdError::UnsupportedVersion(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

scalar_serde!(BridgeAbiVersion, u16);

/// Co-op protocol version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolVersion(u16);

impl ProtocolVersion {
    /// Creates a non-zero protocol version.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero.
    pub fn new(value: u16) -> Result<Self, IdError> {
        if value == 1 {
            Ok(Self(value))
        } else {
            Err(IdError::UnsupportedVersion(value))
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

scalar_serde!(ProtocolVersion, u16);

/// mGBA version pinned as canonical `major.minor.patch` text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MgbaVersion(String);

impl MgbaVersion {
    /// Creates a canonical `major.minor.patch` version.
    ///
    /// # Errors
    ///
    /// Returns an error when the version is not three numeric components.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        let mut components = value.split('.');
        let valid = components.clone().count() == 3
            && components.all(|component| {
                !component.is_empty()
                    && (component == "0" || !component.starts_with('0'))
                    && component.bytes().all(|byte| byte.is_ascii_digit())
            });
        if value.len() > 128 || !valid {
            return Err(IdError::InvalidMgbaVersion);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MgbaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for MgbaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MgbaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(
            deserializer,
            128,
            "mGBA version",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

fn ensure_nonzero<T>(value: T) -> Result<T, IdError>
where
    T: Default + PartialEq,
{
    if value == T::default() {
        Err(IdError::ZeroValue)
    } else {
        Ok(value)
    }
}

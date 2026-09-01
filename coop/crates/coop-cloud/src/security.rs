//! Secret material wrappers. Secrets can be serialized for an authenticated
//! wire request, but their formatting implementations are always redacted.

use std::{fmt, str::FromStr};

use ed25519_dalek::SigningKey as DalekSigningKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::ids::deserialize_bounded_string;

/// Validation failures for secret values.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SecretError {
    #[error("secret is empty")]
    Empty,
    #[error("secret is too short")]
    TooShort,
    #[error("secret exceeds its maximum length")]
    TooLong,
    #[error("secret contains invalid characters")]
    InvalidCharacters,
    #[error("secret must be exactly 32 lowercase hexadecimal characters")]
    InvalidLoopbackSecret,
    #[error("signing key must be exactly 32 bytes")]
    InvalidSigningKey,
}

macro_rules! text_secret {
    ($(#[$meta:meta])* $name:ident, $minimum:expr, $maximum:expr, $validator:expr) => {
        $(#[$meta])*
        #[derive(Eq, PartialEq)]
        pub struct $name(Zeroizing<String>);

        impl Clone for $name {
            fn clone(&self) -> Self {
                Self(Zeroizing::new(self.0.as_str().to_owned()))
            }
        }

        impl $name {
            /// Validates and wraps secret text.
            ///
            /// # Errors
            ///
            /// Returns an error when the secret is empty, outside its size
            /// bound, or contains characters forbidden by this wrapper.
            pub fn new(value: impl Into<String>) -> Result<Self, SecretError> {
                let value = Zeroizing::new(value.into());
                let length = value.len();
                if length == 0 {
                    return Err(SecretError::Empty);
                }
                if length < $minimum {
                    return Err(SecretError::TooShort);
                }
                if length > $maximum {
                    return Err(SecretError::TooLong);
                }
                if !($validator)(value.as_str()) {
                    return Err(SecretError::InvalidCharacters);
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn expose_secret(&self) -> &str {
                self.0.as_str()
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.expose_secret()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("[REDACTED]")
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("[REDACTED]")
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.expose_secret())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::new(deserialize_bounded_string(deserializer, $maximum, stringify!($name))?)
                    .map_err(serde::de::Error::custom)
            }
        }

        impl FromStr for $name {
            type Err = SecretError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

fn password_characters(value: &str) -> bool {
    value.chars().all(|character| !character.is_control())
}

fn invitation_characters(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

fn token_characters(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\')
}

text_secret!(
    /// A password accepted by registration or login.
    Password,
    8,
    128,
    password_characters
);
text_secret!(
    /// An invite code accepted by registration.
    InvitationCode,
    4,
    128,
    invitation_characters
);
text_secret!(
    /// An opaque bearer access token.
    AccessToken,
    1,
    512,
    token_characters
);
text_secret!(
    /// An opaque refresh token.
    RefreshToken,
    1,
    512,
    token_characters
);

/// A launcher-owned 32-character lowercase hexadecimal loopback secret.
#[derive(Clone, Eq, PartialEq)]
pub struct LoopbackSecret(Zeroizing<String>);

impl LoopbackSecret {
    /// Validates and wraps a launcher loopback secret.
    ///
    /// # Errors
    ///
    /// Returns an error unless `value` is exactly 32 lowercase hexadecimal
    /// characters.
    pub fn new(value: impl Into<String>) -> Result<Self, SecretError> {
        let value = Zeroizing::new(value.into());
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(SecretError::InvalidLoopbackSecret);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.expose_secret()
    }
}

impl fmt::Debug for LoopbackSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for LoopbackSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for LoopbackSecret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for LoopbackSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(
            deserializer,
            32,
            "loopback secret",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

impl FromStr for LoopbackSecret {
    type Err = SecretError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Ed25519 private signing material held in zeroizing memory.
pub struct SigningPrivateKey(Zeroizing<[u8; 32]>);

impl SigningPrivateKey {
    /// Wraps raw Ed25519 secret key bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Parses a 64-character lowercase hexadecimal private key.
    ///
    /// # Errors
    ///
    /// Returns an error when the text is not exactly 64 lowercase hexadecimal
    /// characters.
    pub fn parse_hex(value: &str) -> Result<Self, SecretError> {
        let value = Zeroizing::new(value.to_owned());
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(SecretError::InvalidSigningKey);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        Ok(Self::from_bytes(bytes))
    }

    #[must_use]
    pub(crate) fn to_dalek(&self) -> DalekSigningKey {
        DalekSigningKey::from_bytes(&self.0)
    }
}

impl Clone for SigningPrivateKey {
    fn clone(&self) -> Self {
        Self::from_bytes(*self.0)
    }
}

impl fmt::Debug for SigningPrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for SigningPrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Drop for SigningPrivateKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

/// Alias used by callers that refer to the signing material as a key.
pub type SigningKey = SigningPrivateKey;

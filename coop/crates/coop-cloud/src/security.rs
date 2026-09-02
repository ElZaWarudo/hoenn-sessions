//! Secret material wrappers. Secrets can be serialized for an authenticated
//! wire request, but their formatting implementations are always redacted.

use std::{fmt, str::FromStr};

use ed25519_dalek::SigningKey as DalekSigningKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::ids::{Sha256Digest, deserialize_bounded_string};

const REALTIME_TICKET_ENTROPY_BYTES: usize = 32;
const REALTIME_TICKET_ENCODED_LEN: usize = 43;
const REALTIME_TICKET_DOMAIN: &[u8] = b"pokecrossroads/realtime-ticket/v1";

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
    #[error("realtime ticket must be canonical base64url for non-zero 32-byte entropy")]
    InvalidRealtimeTicket,
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

/// A short-lived capability used to authorize a realtime upgrade.
///
/// The wrapper owns its canonical text in zeroizing memory.  It intentionally
/// does not implement `Clone`, `Hash`, ordering, or `AsRef<str>`: callers must
/// make the security-sensitive choice to expose the ticket explicitly.
#[derive(Eq, PartialEq)]
pub struct RealtimeTicket(Zeroizing<String>);

impl RealtimeTicket {
    /// Wraps canonical, unpadded base64url text for exactly 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::InvalidRealtimeTicket`] for malformed,
    /// non-canonical, or all-zero ticket text.
    pub fn new(value: impl Into<String>) -> Result<Self, SecretError> {
        let value = Zeroizing::new(value.into());
        if !is_canonical_realtime_ticket(value.as_bytes()) {
            return Err(SecretError::InvalidRealtimeTicket);
        }
        let decoded =
            decode_realtime_ticket(value.as_bytes()).ok_or(SecretError::InvalidRealtimeTicket)?;
        if decoded.iter().all(|byte| *byte == 0) {
            return Err(SecretError::InvalidRealtimeTicket);
        }
        Ok(Self(value))
    }

    /// Parses canonical, unpadded base64url text.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::InvalidRealtimeTicket`] for malformed,
    /// non-canonical, or all-zero ticket text.
    pub fn parse(value: &str) -> Result<Self, SecretError> {
        Self::new(value)
    }

    /// Encodes and wraps exactly 32 bytes of entropy.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::InvalidRealtimeTicket`] for all-zero entropy.
    pub fn from_bytes(bytes: [u8; REALTIME_TICKET_ENTROPY_BYTES]) -> Result<Self, SecretError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(SecretError::InvalidRealtimeTicket);
        }
        let encoded = encode_realtime_ticket(&bytes);
        Self::new(encoded)
    }

    /// Alias documenting that the bytes are entropy rather than plaintext.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::InvalidRealtimeTicket`] for all-zero entropy.
    pub fn from_entropy(bytes: [u8; REALTIME_TICKET_ENTROPY_BYTES]) -> Result<Self, SecretError> {
        Self::from_bytes(bytes)
    }

    /// Explicitly exposes the canonical ticket text.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    /// Computes the domain-separated fingerprint used by ticket storage.
    #[must_use]
    pub fn fingerprint(&self) -> Sha256Digest {
        let mut digest = Sha256::new();
        digest.update(REALTIME_TICKET_DOMAIN);
        digest.update([0]);
        digest.update(self.0.as_bytes());
        Sha256Digest::from_bytes(digest.finalize().into())
    }
}

impl fmt::Debug for RealtimeTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for RealtimeTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for RealtimeTicket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

impl<'de> Deserialize<'de> for RealtimeTicket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(deserialize_bounded_string(
            deserializer,
            REALTIME_TICKET_ENCODED_LEN,
            "realtime ticket",
        )?)
        .map_err(serde::de::Error::custom)
    }
}

impl FromStr for RealtimeTicket {
    type Err = SecretError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

const REALTIME_TICKET_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_canonical_realtime_ticket(value: &[u8]) -> bool {
    value.len() == REALTIME_TICKET_ENCODED_LEN
        && value[..REALTIME_TICKET_ENCODED_LEN - 1]
            .iter()
            .all(|byte| base64url_value(*byte).is_some())
        && matches!(
            value[REALTIME_TICKET_ENCODED_LEN - 1],
            b'A' | b'E'
                | b'I'
                | b'M'
                | b'Q'
                | b'U'
                | b'Y'
                | b'c'
                | b'g'
                | b'k'
                | b'o'
                | b's'
                | b'w'
                | b'0'
                | b'4'
                | b'8'
        )
}

fn encode_realtime_ticket(bytes: &[u8; REALTIME_TICKET_ENTROPY_BYTES]) -> String {
    let mut output = String::with_capacity(REALTIME_TICKET_ENCODED_LEN);
    for chunk in bytes.chunks_exact(3) {
        output.push(char::from(
            REALTIME_TICKET_ALPHABET[(chunk[0] >> 2) as usize],
        ));
        output.push(char::from(
            REALTIME_TICKET_ALPHABET[((chunk[0] & 0x03) << 4 | chunk[1] >> 4) as usize],
        ));
        output.push(char::from(
            REALTIME_TICKET_ALPHABET[((chunk[1] & 0x0f) << 2 | chunk[2] >> 6) as usize],
        ));
        output.push(char::from(
            REALTIME_TICKET_ALPHABET[(chunk[2] & 0x3f) as usize],
        ));
    }
    let tail: [u8; 2] = bytes[30..].try_into().expect("32-byte ticket tail");
    output.push(char::from(
        REALTIME_TICKET_ALPHABET[(tail[0] >> 2) as usize],
    ));
    output.push(char::from(
        REALTIME_TICKET_ALPHABET[((tail[0] & 0x03) << 4 | tail[1] >> 4) as usize],
    ));
    output.push(char::from(
        REALTIME_TICKET_ALPHABET[((tail[1] & 0x0f) << 2) as usize],
    ));
    output
}

fn decode_realtime_ticket(value: &[u8]) -> Option<[u8; REALTIME_TICKET_ENTROPY_BYTES]> {
    if !is_canonical_realtime_ticket(value) {
        return None;
    }
    let mut output = [0_u8; REALTIME_TICKET_ENTROPY_BYTES];
    let mut output_index = 0;
    for chunk in value[..40].chunks_exact(4) {
        let a = base64url_value(chunk[0])?;
        let b = base64url_value(chunk[1])?;
        let c = base64url_value(chunk[2])?;
        let d = base64url_value(chunk[3])?;
        output[output_index] = (a << 2) | (b >> 4);
        output[output_index + 1] = (b << 4) | (c >> 2);
        output[output_index + 2] = (c << 6) | d;
        output_index += 3;
    }
    let a = base64url_value(value[40])?;
    let b = base64url_value(value[41])?;
    let c = base64url_value(value[42])?;
    output[30] = (a << 2) | (b >> 4);
    output[31] = (b << 4) | (c >> 2);
    Some(output)
}

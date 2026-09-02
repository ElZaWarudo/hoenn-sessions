//! Strict, region-qualified wire contracts for online player presence.
//!
//! The bridge payloads in this module are intentionally independent of Rust's
//! in-memory layout.  They are encoded field-by-field as little-endian bytes,
//! which makes the byte contracts suitable for the ROM and Lua adapters that
//! will consume them in a later protocol revision.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};
use thiserror::Error;

use crate::{ProtocolError, RegionId, WorldLocation};

/// The fixed-width wire representation of a region-qualified location.
pub const WORLD_LOCATION_V1_SIZE: usize = 10;
pub const WORLD_LOCATION_V1_REGION_OFFSET: usize = 0;
pub const WORLD_LOCATION_V1_MAP_GROUP_OFFSET: usize = 1;
pub const WORLD_LOCATION_V1_MAP_NUMBER_OFFSET: usize = 3;
pub const WORLD_LOCATION_V1_X_OFFSET: usize = 5;
pub const WORLD_LOCATION_V1_Y_OFFSET: usize = 7;
pub const WORLD_LOCATION_V1_RESERVED_OFFSET: usize = 9;

/// Presence pose payload layout (24 bytes).
pub const PRESENCE_POSE_V1_SIZE: usize = 24;
pub const PRESENCE_POSE_V1_LOCATION_OFFSET: usize = 0;
pub const PRESENCE_POSE_V1_ELEVATION_OFFSET: usize = 10;
pub const PRESENCE_POSE_V1_DIRECTION_OFFSET: usize = 11;
pub const PRESENCE_POSE_V1_CLIENT_TICK_OFFSET: usize = 12;
pub const PRESENCE_POSE_V1_WARP_SEQUENCE_OFFSET: usize = 16;
pub const PRESENCE_POSE_V1_MOVEMENT_MODE_OFFSET: usize = 20;
pub const PRESENCE_POSE_V1_ANIMATION_ID_OFFSET: usize = 21;
pub const PRESENCE_POSE_V1_AVATAR_ID_OFFSET: usize = 22;
pub const PRESENCE_POSE_V1_PLAYER_STATE_OFFSET: usize = 23;

/// Local state payload layout (28 bytes).
pub const LOCAL_PRESENCE_STATE_V1_SIZE: usize = 28;
pub const LOCAL_PRESENCE_STATE_V1_POSE_OFFSET: usize = 0;
pub const LOCAL_PRESENCE_STATE_V1_SOURCE_SEQUENCE_OFFSET: usize = 24;

/// Remote spawn payload layout (72 bytes).
pub const REMOTE_PLAYER_SPAWN_V1_SIZE: usize = 72;
pub const REMOTE_PLAYER_SPAWN_V1_HANDLE_OFFSET: usize = 0;
pub const REMOTE_PLAYER_SPAWN_V1_SERVER_SEQUENCE_OFFSET: usize = 8;
pub const REMOTE_PLAYER_SPAWN_V1_STATE_OFFSET: usize = 12;
pub const REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET: usize = 40;
pub const REMOTE_PLAYER_SPAWN_V1_USERNAME_SIZE: usize = 32;

/// Remote update payload layout (40 bytes).
pub const REMOTE_PLAYER_UPDATE_V1_SIZE: usize = 40;
pub const REMOTE_PLAYER_UPDATE_V1_HANDLE_OFFSET: usize = 0;
pub const REMOTE_PLAYER_UPDATE_V1_SERVER_SEQUENCE_OFFSET: usize = 8;
pub const REMOTE_PLAYER_UPDATE_V1_STATE_OFFSET: usize = 12;

/// Remote despawn payload layout (16 bytes).
pub const REMOTE_PLAYER_DESPAWN_V1_SIZE: usize = 16;
pub const REMOTE_PLAYER_DESPAWN_V1_HANDLE_OFFSET: usize = 0;
pub const REMOTE_PLAYER_DESPAWN_V1_SERVER_SEQUENCE_OFFSET: usize = 8;
pub const REMOTE_PLAYER_DESPAWN_V1_REASON_OFFSET: usize = 12;
pub const REMOTE_PLAYER_DESPAWN_V1_RESERVED_OFFSET: usize = 13;

/// Remote interaction payload layout (20 bytes).
pub const PRESENCE_INTERACTION_V1_SIZE: usize = 20;
pub const PRESENCE_INTERACTION_V1_HANDLE_OFFSET: usize = 0;
pub const PRESENCE_INTERACTION_V1_SERVER_SEQUENCE_OFFSET: usize = 8;
pub const PRESENCE_INTERACTION_V1_WARP_SEQUENCE_OFFSET: usize = 12;
pub const PRESENCE_INTERACTION_V1_X_OFFSET: usize = 16;
pub const PRESENCE_INTERACTION_V1_Y_OFFSET: usize = 18;

// Readable aliases used by adapters that call these records "payloads".
pub const PRESENCE_POSE_V1_LEN: usize = PRESENCE_POSE_V1_SIZE;
pub const LOCAL_PRESENCE_STATE_V1_LEN: usize = LOCAL_PRESENCE_STATE_V1_SIZE;
pub const REMOTE_PLAYER_SPAWN_V1_LEN: usize = REMOTE_PLAYER_SPAWN_V1_SIZE;
pub const REMOTE_PLAYER_UPDATE_V1_LEN: usize = REMOTE_PLAYER_UPDATE_V1_SIZE;
pub const REMOTE_PLAYER_DESPAWN_V1_LEN: usize = REMOTE_PLAYER_DESPAWN_V1_SIZE;
pub const PRESENCE_INTERACTION_V1_LEN: usize = PRESENCE_INTERACTION_V1_SIZE;

/// Errors returned by strict presence value and codec operations.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PresenceError {
    #[error("invalid {kind} payload length: expected {expected}, got {actual}")]
    InvalidLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("invalid location: {0}")]
    InvalidLocation(#[from] ProtocolError),
    #[error("non-zero reserved byte at offset {offset}: {value:#x}")]
    NonZeroReserved { offset: usize, value: u8 },
    #[error("unknown {field} ordinal: {value:#x}")]
    UnknownEnum { field: &'static str, value: u8 },
    #[error("{field} must not be zero")]
    ZeroValue { field: &'static str },
    #[error("invalid canonical username: {value}")]
    InvalidUsername { value: String },
    #[error("invalid presence handle: {value}")]
    InvalidHandle { value: String },
    #[error("presence handle must be non-zero")]
    ZeroHandle,
}
fn invalid_len(kind: &'static str, expected: usize, actual: usize) -> PresenceError {
    PresenceError::InvalidLength {
        kind,
        expected,
        actual,
    }
}

fn require_len<'a>(
    bytes: &'a [u8],
    expected: usize,
    kind: &'static str,
) -> Result<&'a [u8], PresenceError> {
    if bytes.len() == expected {
        Ok(bytes)
    } else {
        Err(invalid_len(kind, expected, bytes.len()))
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_i16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn write_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_i16(output: &mut [u8], offset: usize, value: i16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn ensure_nonzero(value: u32, field: &'static str) -> Result<u32, PresenceError> {
    if value == 0 {
        Err(PresenceError::ZeroValue { field })
    } else {
        Ok(value)
    }
}

/// Cardinal direction ordinals used on the presence wire.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Direction {
    South = 1,
    North = 2,
    West = 3,
    East = 4,
}

impl Direction {
    #[must_use]
    pub const fn wire(self) -> u8 {
        self as u8
    }

    /// Converts a direction ordinal from the wire.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::UnknownEnum`] for an unassigned ordinal.
    pub const fn from_wire(value: u8) -> Result<Self, PresenceError> {
        match value {
            1 => Ok(Self::South),
            2 => Ok(Self::North),
            3 => Ok(Self::West),
            4 => Ok(Self::East),
            value => Err(PresenceError::UnknownEnum {
                field: "direction",
                value,
            }),
        }
    }

    const fn token(self) -> &'static str {
        match self {
            Self::South => "SOUTH",
            Self::North => "NORTH",
            Self::West => "WEST",
            Self::East => "EAST",
        }
    }

    fn parse_token(value: &str) -> Result<Self, PresenceError> {
        match value {
            "SOUTH" => Ok(Self::South),
            "NORTH" => Ok(Self::North),
            "WEST" => Ok(Self::West),
            "EAST" => Ok(Self::East),
            _ => Err(PresenceError::UnknownEnum {
                field: "direction",
                value: 0,
            }),
        }
    }
}

macro_rules! wire_enum {
    ($name:ident, $field:literal, { $($wire:literal => $variant:ident => $token:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        #[repr(u8)]
        pub enum $name {
            $($variant = $wire),+
        }

        impl $name {
            #[must_use]
            pub const fn wire(self) -> u8 { self as u8 }

            /// Converts a wire ordinal into this closed enum.
            ///
            /// # Errors
            ///
            /// Returns [`PresenceError::UnknownEnum`] for an unassigned ordinal.
            pub const fn from_wire(value: u8) -> Result<Self, PresenceError> {
                match value {
                    $($wire => Ok(Self::$variant),)+
                    value => Err(PresenceError::UnknownEnum { field: $field, value }),
                }
            }

            const fn token(self) -> &'static str {
                match self { $(Self::$variant => $token,)+ }
            }

            fn parse_token(value: &str) -> Result<Self, PresenceError> {
                match value {
                    $($token => Ok(Self::$variant),)+
                    _ => Err(PresenceError::UnknownEnum { field: $field, value: 0 }),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where S: Serializer {
                serializer.serialize_str(self.token())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where D: Deserializer<'de> {
                let token = String::deserialize(deserializer)?;
                Self::parse_token(&token).map_err(serde::de::Error::custom)
            }
        }
    };
}

impl Serialize for Direction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for Direction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        Self::parse_token(&token).map_err(serde::de::Error::custom)
    }
}

wire_enum!(MovementMode, "movement_mode", {
    0 => Idle => "IDLE",
    1 => Walk => "WALK",
    2 => Run => "RUN",
});
wire_enum!(AnimationId, "animation_id", {
    0 => Idle => "IDLE",
    1 => Locomotion => "LOCOMOTION",
});
wire_enum!(AvatarId, "avatar_id", {
    1 => Brendan => "BRENDAN",
    2 => May => "MAY",
});
wire_enum!(PlayerState, "player_state", {
    0 => Hidden => "HIDDEN",
    1 => Overworld => "OVERWORLD",
});
wire_enum!(DespawnReason, "despawn_reason", {
    1 => Hidden => "HIDDEN",
    2 => Stale => "STALE",
    3 => Disconnected => "DISCONNECTED",
    4 => LeaseInvalid => "LEASE_INVALID",
    5 => Replaced => "REPLACED",
    6 => PartitionLeft => "PARTITION_LEFT",
});

/// A bounded, canonical player username.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanonicalUsername(String);

/// Alias retained for callers that use the shorter protocol name.
pub type Username = CanonicalUsername;

impl CanonicalUsername {
    /// Creates a canonical username (3 to 32 lowercase ASCII bytes).
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::InvalidUsername`] when the value is not a
    /// canonical bounded username.
    pub fn new(value: impl AsRef<str>) -> Result<Self, PresenceError> {
        let value = value.as_ref();
        let bytes = value.as_bytes();
        let valid_length = (3..=32).contains(&bytes.len());
        let valid_bytes = bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        });
        let valid_edges = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && bytes.last().is_some_and(u8::is_ascii_alphanumeric);
        if valid_length && valid_bytes && valid_edges {
            Ok(Self(value.to_owned()))
        } else {
            Err(PresenceError::InvalidUsername {
                value: value.to_owned(),
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn decode_fixed(bytes: &[u8]) -> Result<Self, PresenceError> {
        let first_zero = bytes.iter().position(|byte| *byte == 0);
        let (name, padding) = match first_zero {
            Some(index) => (&bytes[..index], &bytes[index + 1..]),
            None => (bytes, &[][..]),
        };
        if padding.iter().any(|byte| *byte != 0) {
            return Err(PresenceError::InvalidUsername {
                value: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        let value = std::str::from_utf8(name).map_err(|_| PresenceError::InvalidUsername {
            value: String::from_utf8_lossy(name).into_owned(),
        })?;
        Self::new(value)
    }
}

impl AsRef<str> for CanonicalUsername {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CanonicalUsername {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CanonicalUsername {
    type Err = PresenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for CanonicalUsername {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CanonicalUsername {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Opaque, non-zero server identity for a remote player.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresenceHandle(u64);

impl PresenceHandle {
    /// Creates a non-zero opaque handle.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::ZeroHandle`] for zero.
    pub const fn new(value: u64) -> Result<Self, PresenceError> {
        if value == 0 {
            Err(PresenceError::ZeroHandle)
        } else {
            Ok(Self(value))
        }
    }

    /// Validates and wraps a wire handle.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::ZeroHandle`] for zero.
    pub const fn from_wire(value: u64) -> Result<Self, PresenceError> {
        Self::new(value)
    }

    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Parses exactly 16 lowercase hexadecimal digits.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::InvalidHandle`] for malformed text or
    /// [`PresenceError::ZeroHandle`] for the all-zero handle.
    pub fn from_hex(value: &str) -> Result<Self, PresenceError> {
        if value.len() != 16
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(PresenceError::InvalidHandle {
                value: value.to_owned(),
            });
        }
        let mut decoded = 0u64;
        for byte in value.bytes() {
            decoded = decoded
                .checked_mul(16)
                .and_then(|value| {
                    value.checked_add(u64::from(match byte {
                        b'0'..=b'9' => byte - b'0',
                        b'a'..=b'f' => byte - b'a' + 10,
                        _ => return None,
                    }))
                })
                .ok_or_else(|| PresenceError::InvalidHandle {
                    value: value.to_owned(),
                })?;
        }
        Self::new(decoded)
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

impl fmt::Display for PresenceHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

impl Serialize for PresenceHandle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for PresenceHandle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(serde::de::Error::custom)
    }
}

/// Compares two non-zero serial numbers using RFC 1982 serial arithmetic.
///
/// The exact half-range is deliberately treated as not newer because it is
/// ambiguous.  Zero is not a valid presence sequence and is never newer.
#[must_use]
pub const fn sequence_is_newer(candidate: u32, reference: u32) -> bool {
    if candidate == 0 || reference == 0 || candidate == reference {
        return false;
    }
    let delta = candidate.wrapping_sub(reference);
    delta < 0x8000_0000
}

/// Alias for [`sequence_is_newer`] used by server-side callers.
#[must_use]
pub const fn is_newer_sequence(candidate: u32, reference: u32) -> bool {
    sequence_is_newer(candidate, reference)
}

/// Advances a serial number and skips the reserved zero value.
#[must_use]
pub const fn next_sequence(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

/// Alias for [`next_sequence`].
#[must_use]
pub const fn increment_sequence(current: u32) -> u32 {
    next_sequence(current)
}

fn encode_location(location: &WorldLocation, output: &mut [u8], offset: usize) {
    output[offset + WORLD_LOCATION_V1_REGION_OFFSET] = location.region.wire();
    write_u16(
        output,
        offset + WORLD_LOCATION_V1_MAP_GROUP_OFFSET,
        location.map_group,
    );
    write_u16(
        output,
        offset + WORLD_LOCATION_V1_MAP_NUMBER_OFFSET,
        location.map_number,
    );
    write_i16(output, offset + WORLD_LOCATION_V1_X_OFFSET, location.x);
    write_i16(output, offset + WORLD_LOCATION_V1_Y_OFFSET, location.y);
    output[offset + WORLD_LOCATION_V1_RESERVED_OFFSET] = 0;
}

fn decode_location(bytes: &[u8], offset: usize) -> Result<WorldLocation, PresenceError> {
    let reserved = bytes[offset + WORLD_LOCATION_V1_RESERVED_OFFSET];
    if reserved != 0 {
        return Err(PresenceError::NonZeroReserved {
            offset: offset + WORLD_LOCATION_V1_RESERVED_OFFSET,
            value: reserved,
        });
    }
    let region = RegionId::from_wire(bytes[offset + WORLD_LOCATION_V1_REGION_OFFSET])?;
    WorldLocation::new(
        region,
        read_u16(bytes, offset + WORLD_LOCATION_V1_MAP_GROUP_OFFSET),
        read_u16(bytes, offset + WORLD_LOCATION_V1_MAP_NUMBER_OFFSET),
        read_i16(bytes, offset + WORLD_LOCATION_V1_X_OFFSET),
        read_i16(bytes, offset + WORLD_LOCATION_V1_Y_OFFSET),
    )
    .map_err(PresenceError::InvalidLocation)
}

/// Player pose at one map coordinate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresencePoseV1 {
    location: WorldLocation,
    elevation: u8,
    direction: Direction,
    client_tick: u32,
    warp_sequence: u32,
    movement_mode: MovementMode,
    animation_id: AnimationId,
    avatar_id: AvatarId,
    player_state: PlayerState,
}

impl PresencePoseV1 {
    /// Constructs and validates a pose.
    ///
    /// # Errors
    ///
    /// Returns an error when the location is not catalogued or the warp
    /// sequence is zero.
    #[expect(
        clippy::too_many_arguments,
        reason = "the public constructor mirrors the fixed wire fields"
    )]
    pub fn new(
        location: WorldLocation,
        elevation: u8,
        direction: Direction,
        client_tick: u32,
        warp_sequence: u32,
        movement_mode: MovementMode,
        animation_id: AnimationId,
        avatar_id: AvatarId,
        player_state: PlayerState,
    ) -> Result<Self, PresenceError> {
        location
            .validate()
            .map_err(PresenceError::InvalidLocation)?;
        ensure_nonzero(warp_sequence, "warp_sequence")?;
        Ok(Self {
            location,
            elevation,
            direction,
            client_tick,
            warp_sequence,
            movement_mode,
            animation_id,
            avatar_id,
            player_state,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when a location or warp sequence is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(
            self.location,
            self.elevation,
            self.direction,
            self.client_tick,
            self.warp_sequence,
            self.movement_mode,
            self.animation_id,
            self.avatar_id,
            self.player_state,
        )
        .map(|_| ())
    }

    #[must_use]
    pub const fn location(&self) -> WorldLocation {
        self.location
    }

    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.elevation
    }

    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    #[must_use]
    pub const fn client_tick(&self) -> u32 {
        self.client_tick
    }

    #[must_use]
    pub const fn warp_sequence(&self) -> u32 {
        self.warp_sequence
    }

    #[must_use]
    pub const fn movement_mode(&self) -> MovementMode {
        self.movement_mode
    }

    #[must_use]
    pub const fn animation_id(&self) -> AnimationId {
        self.animation_id
    }

    #[must_use]
    pub const fn avatar_id(&self) -> AvatarId {
        self.avatar_id
    }

    #[must_use]
    pub const fn player_state(&self) -> PlayerState {
        self.player_state
    }

    #[must_use]
    pub fn encode(&self) -> [u8; PRESENCE_POSE_V1_SIZE] {
        let mut output = [0u8; PRESENCE_POSE_V1_SIZE];
        encode_location(
            &self.location,
            &mut output,
            PRESENCE_POSE_V1_LOCATION_OFFSET,
        );
        output[PRESENCE_POSE_V1_ELEVATION_OFFSET] = self.elevation;
        output[PRESENCE_POSE_V1_DIRECTION_OFFSET] = self.direction.wire();
        write_u32(
            &mut output,
            PRESENCE_POSE_V1_CLIENT_TICK_OFFSET,
            self.client_tick,
        );
        write_u32(
            &mut output,
            PRESENCE_POSE_V1_WARP_SEQUENCE_OFFSET,
            self.warp_sequence,
        );
        output[PRESENCE_POSE_V1_MOVEMENT_MODE_OFFSET] = self.movement_mode.wire();
        output[PRESENCE_POSE_V1_ANIMATION_ID_OFFSET] = self.animation_id.wire();
        output[PRESENCE_POSE_V1_AVATAR_ID_OFFSET] = self.avatar_id.wire();
        output[PRESENCE_POSE_V1_PLAYER_STATE_OFFSET] = self.player_state.wire();
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; PRESENCE_POSE_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian pose payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length, reserved byte, unknown enum,
    /// invalid map, or zero warp sequence.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, PRESENCE_POSE_V1_SIZE, "presence pose")?;
        Self::new(
            decode_location(bytes, PRESENCE_POSE_V1_LOCATION_OFFSET)?,
            bytes[PRESENCE_POSE_V1_ELEVATION_OFFSET],
            Direction::from_wire(bytes[PRESENCE_POSE_V1_DIRECTION_OFFSET])?,
            read_u32(bytes, PRESENCE_POSE_V1_CLIENT_TICK_OFFSET),
            read_u32(bytes, PRESENCE_POSE_V1_WARP_SEQUENCE_OFFSET),
            MovementMode::from_wire(bytes[PRESENCE_POSE_V1_MOVEMENT_MODE_OFFSET])?,
            AnimationId::from_wire(bytes[PRESENCE_POSE_V1_ANIMATION_ID_OFFSET])?,
            AvatarId::from_wire(bytes[PRESENCE_POSE_V1_AVATAR_ID_OFFSET])?,
            PlayerState::from_wire(bytes[PRESENCE_POSE_V1_PLAYER_STATE_OFFSET])?,
        )
    }
}

impl Serialize for PresencePoseV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("PresencePoseV1", 9)?;
        output.serialize_field("location", &self.location)?;
        output.serialize_field("elevation", &self.elevation)?;
        output.serialize_field("direction", &self.direction)?;
        output.serialize_field("client_tick", &self.client_tick)?;
        output.serialize_field("warp_sequence", &self.warp_sequence)?;
        output.serialize_field("movement_mode", &self.movement_mode)?;
        output.serialize_field("animation_id", &self.animation_id)?;
        output.serialize_field("avatar_id", &self.avatar_id)?;
        output.serialize_field("player_state", &self.player_state)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for PresencePoseV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WirePose {
            location: WorldLocation,
            elevation: u8,
            direction: Direction,
            client_tick: u32,
            warp_sequence: u32,
            movement_mode: MovementMode,
            animation_id: AnimationId,
            avatar_id: AvatarId,
            player_state: PlayerState,
        }
        let wire = WirePose::deserialize(deserializer)?;
        Self::new(
            wire.location,
            wire.elevation,
            wire.direction,
            wire.client_tick,
            wire.warp_sequence,
            wire.movement_mode,
            wire.animation_id,
            wire.avatar_id,
            wire.player_state,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// A local pose plus its source sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPresenceStateV1 {
    pose: PresencePoseV1,
    source_sequence: u32,
}

impl LocalPresenceStateV1 {
    /// Constructs and validates local state.
    ///
    /// # Errors
    ///
    /// Returns an error when the pose is invalid or the source sequence is
    /// zero.
    pub fn new(pose: PresencePoseV1, source_sequence: u32) -> Result<Self, PresenceError> {
        pose.validate()?;
        ensure_nonzero(source_sequence, "source_sequence")?;
        Ok(Self {
            pose,
            source_sequence,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when the pose or source sequence is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(self.pose.clone(), self.source_sequence).map(|_| ())
    }

    #[must_use]
    pub const fn pose(&self) -> &PresencePoseV1 {
        &self.pose
    }

    #[must_use]
    pub const fn source_sequence(&self) -> u32 {
        self.source_sequence
    }

    #[must_use]
    pub fn encode(&self) -> [u8; LOCAL_PRESENCE_STATE_V1_SIZE] {
        let mut output = [0u8; LOCAL_PRESENCE_STATE_V1_SIZE];
        output[..PRESENCE_POSE_V1_SIZE].copy_from_slice(&self.pose.encode());
        write_u32(
            &mut output,
            LOCAL_PRESENCE_STATE_V1_SOURCE_SEQUENCE_OFFSET,
            self.source_sequence,
        );
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; LOCAL_PRESENCE_STATE_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian local-state payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length or any invalid nested pose or
    /// source sequence.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, LOCAL_PRESENCE_STATE_V1_SIZE, "local presence state")?;
        Self::new(
            PresencePoseV1::decode(&bytes[..PRESENCE_POSE_V1_SIZE])?,
            read_u32(bytes, LOCAL_PRESENCE_STATE_V1_SOURCE_SEQUENCE_OFFSET),
        )
    }
}

impl Serialize for LocalPresenceStateV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("LocalPresenceStateV1", 2)?;
        output.serialize_field("pose", &self.pose)?;
        output.serialize_field("source_sequence", &self.source_sequence)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for LocalPresenceStateV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireState {
            pose: PresencePoseV1,
            source_sequence: u32,
        }
        let wire = WireState::deserialize(deserializer)?;
        Self::new(wire.pose, wire.source_sequence).map_err(serde::de::Error::custom)
    }
}

/// A server-issued remote player spawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemotePlayerSpawnV1 {
    handle: PresenceHandle,
    server_sequence: u32,
    state: LocalPresenceStateV1,
    username: CanonicalUsername,
}

impl RemotePlayerSpawnV1 {
    /// Constructs and validates a remote spawn.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle, server sequence, state, or username
    /// is invalid.
    pub fn new(
        handle: PresenceHandle,
        server_sequence: u32,
        state: LocalPresenceStateV1,
        username: CanonicalUsername,
    ) -> Result<Self, PresenceError> {
        state.validate()?;
        ensure_nonzero(server_sequence, "server_sequence")?;
        Ok(Self {
            handle,
            server_sequence,
            state,
            username,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when a nested value or server sequence is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(
            self.handle,
            self.server_sequence,
            self.state.clone(),
            self.username.clone(),
        )
        .map(|_| ())
    }

    #[must_use]
    pub const fn handle(&self) -> PresenceHandle {
        self.handle
    }

    #[must_use]
    pub const fn server_sequence(&self) -> u32 {
        self.server_sequence
    }

    #[must_use]
    pub const fn state(&self) -> &LocalPresenceStateV1 {
        &self.state
    }

    #[must_use]
    pub const fn username(&self) -> &CanonicalUsername {
        &self.username
    }

    #[must_use]
    pub fn encode(&self) -> [u8; REMOTE_PLAYER_SPAWN_V1_SIZE] {
        let mut output = [0u8; REMOTE_PLAYER_SPAWN_V1_SIZE];
        write_u64(
            &mut output,
            REMOTE_PLAYER_SPAWN_V1_HANDLE_OFFSET,
            self.handle.as_u64(),
        );
        write_u32(
            &mut output,
            REMOTE_PLAYER_SPAWN_V1_SERVER_SEQUENCE_OFFSET,
            self.server_sequence,
        );
        output[REMOTE_PLAYER_SPAWN_V1_STATE_OFFSET
            ..REMOTE_PLAYER_SPAWN_V1_STATE_OFFSET + LOCAL_PRESENCE_STATE_V1_SIZE]
            .copy_from_slice(&self.state.encode());
        output[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET
            ..REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET + self.username.as_bytes().len()]
            .copy_from_slice(self.username.as_bytes());
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; REMOTE_PLAYER_SPAWN_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian remote-spawn payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length or any invalid nested field.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, REMOTE_PLAYER_SPAWN_V1_SIZE, "remote player spawn")?;
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, REMOTE_PLAYER_SPAWN_V1_HANDLE_OFFSET))?,
            ensure_nonzero(
                read_u32(bytes, REMOTE_PLAYER_SPAWN_V1_SERVER_SEQUENCE_OFFSET),
                "server_sequence",
            )?,
            LocalPresenceStateV1::decode(
                &bytes[REMOTE_PLAYER_SPAWN_V1_STATE_OFFSET
                    ..REMOTE_PLAYER_SPAWN_V1_STATE_OFFSET + LOCAL_PRESENCE_STATE_V1_SIZE],
            )?,
            CanonicalUsername::decode_fixed(
                &bytes[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET
                    ..REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET
                        + REMOTE_PLAYER_SPAWN_V1_USERNAME_SIZE],
            )?,
        )
    }
}

impl Serialize for RemotePlayerSpawnV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("RemotePlayerSpawnV1", 4)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("server_sequence", &self.server_sequence)?;
        output.serialize_field("state", &self.state)?;
        output.serialize_field("username", &self.username)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for RemotePlayerSpawnV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireSpawn {
            handle: PresenceHandle,
            server_sequence: u32,
            state: LocalPresenceStateV1,
            username: CanonicalUsername,
        }
        let wire = WireSpawn::deserialize(deserializer)?;
        Self::new(wire.handle, wire.server_sequence, wire.state, wire.username)
            .map_err(serde::de::Error::custom)
    }
}

/// A subsequent remote player state update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemotePlayerUpdateV1 {
    handle: PresenceHandle,
    server_sequence: u32,
    state: LocalPresenceStateV1,
}

impl RemotePlayerUpdateV1 {
    /// Constructs and validates a remote update.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle, server sequence, or state is invalid.
    pub fn new(
        handle: PresenceHandle,
        server_sequence: u32,
        state: LocalPresenceStateV1,
    ) -> Result<Self, PresenceError> {
        state.validate()?;
        ensure_nonzero(server_sequence, "server_sequence")?;
        Ok(Self {
            handle,
            server_sequence,
            state,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when a nested value or server sequence is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(self.handle, self.server_sequence, self.state.clone()).map(|_| ())
    }

    #[must_use]
    pub const fn handle(&self) -> PresenceHandle {
        self.handle
    }

    #[must_use]
    pub const fn server_sequence(&self) -> u32 {
        self.server_sequence
    }

    #[must_use]
    pub const fn state(&self) -> &LocalPresenceStateV1 {
        &self.state
    }

    #[must_use]
    pub fn encode(&self) -> [u8; REMOTE_PLAYER_UPDATE_V1_SIZE] {
        let mut output = [0u8; REMOTE_PLAYER_UPDATE_V1_SIZE];
        write_u64(
            &mut output,
            REMOTE_PLAYER_UPDATE_V1_HANDLE_OFFSET,
            self.handle.as_u64(),
        );
        write_u32(
            &mut output,
            REMOTE_PLAYER_UPDATE_V1_SERVER_SEQUENCE_OFFSET,
            self.server_sequence,
        );
        output[REMOTE_PLAYER_UPDATE_V1_STATE_OFFSET
            ..REMOTE_PLAYER_UPDATE_V1_STATE_OFFSET + LOCAL_PRESENCE_STATE_V1_SIZE]
            .copy_from_slice(&self.state.encode());
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; REMOTE_PLAYER_UPDATE_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian remote-update payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length or any invalid nested field.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, REMOTE_PLAYER_UPDATE_V1_SIZE, "remote player update")?;
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, REMOTE_PLAYER_UPDATE_V1_HANDLE_OFFSET))?,
            read_u32(bytes, REMOTE_PLAYER_UPDATE_V1_SERVER_SEQUENCE_OFFSET),
            LocalPresenceStateV1::decode(
                &bytes[REMOTE_PLAYER_UPDATE_V1_STATE_OFFSET
                    ..REMOTE_PLAYER_UPDATE_V1_STATE_OFFSET + LOCAL_PRESENCE_STATE_V1_SIZE],
            )?,
        )
    }
}

impl Serialize for RemotePlayerUpdateV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("RemotePlayerUpdateV1", 3)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("server_sequence", &self.server_sequence)?;
        output.serialize_field("state", &self.state)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for RemotePlayerUpdateV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireUpdate {
            handle: PresenceHandle,
            server_sequence: u32,
            state: LocalPresenceStateV1,
        }
        let wire = WireUpdate::deserialize(deserializer)?;
        Self::new(wire.handle, wire.server_sequence, wire.state).map_err(serde::de::Error::custom)
    }
}

/// A remote player removal notification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemotePlayerDespawnV1 {
    handle: PresenceHandle,
    server_sequence: u32,
    reason: DespawnReason,
}

impl RemotePlayerDespawnV1 {
    /// Constructs and validates a remote despawn.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle or server sequence is invalid.
    pub fn new(
        handle: PresenceHandle,
        server_sequence: u32,
        reason: DespawnReason,
    ) -> Result<Self, PresenceError> {
        ensure_nonzero(server_sequence, "server_sequence")?;
        Ok(Self {
            handle,
            server_sequence,
            reason,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle or server sequence is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(self.handle, self.server_sequence, self.reason).map(|_| ())
    }

    #[must_use]
    pub const fn handle(&self) -> PresenceHandle {
        self.handle
    }

    #[must_use]
    pub const fn server_sequence(&self) -> u32 {
        self.server_sequence
    }

    #[must_use]
    pub const fn reason(&self) -> DespawnReason {
        self.reason
    }

    #[must_use]
    pub fn encode(&self) -> [u8; REMOTE_PLAYER_DESPAWN_V1_SIZE] {
        let mut output = [0u8; REMOTE_PLAYER_DESPAWN_V1_SIZE];
        write_u64(
            &mut output,
            REMOTE_PLAYER_DESPAWN_V1_HANDLE_OFFSET,
            self.handle.as_u64(),
        );
        write_u32(
            &mut output,
            REMOTE_PLAYER_DESPAWN_V1_SERVER_SEQUENCE_OFFSET,
            self.server_sequence,
        );
        output[REMOTE_PLAYER_DESPAWN_V1_REASON_OFFSET] = self.reason.wire();
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; REMOTE_PLAYER_DESPAWN_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian remote-despawn payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length, reserved byte, unknown reason,
    /// zero handle, or zero server sequence.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(
            bytes,
            REMOTE_PLAYER_DESPAWN_V1_SIZE,
            "remote player despawn",
        )?;
        for (offset, value) in bytes[REMOTE_PLAYER_DESPAWN_V1_RESERVED_OFFSET..]
            .iter()
            .enumerate()
        {
            if *value != 0 {
                return Err(PresenceError::NonZeroReserved {
                    offset: REMOTE_PLAYER_DESPAWN_V1_RESERVED_OFFSET + offset,
                    value: *value,
                });
            }
        }
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, REMOTE_PLAYER_DESPAWN_V1_HANDLE_OFFSET))?,
            read_u32(bytes, REMOTE_PLAYER_DESPAWN_V1_SERVER_SEQUENCE_OFFSET),
            DespawnReason::from_wire(bytes[REMOTE_PLAYER_DESPAWN_V1_REASON_OFFSET])?,
        )
    }
}

impl Serialize for RemotePlayerDespawnV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("RemotePlayerDespawnV1", 3)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("server_sequence", &self.server_sequence)?;
        output.serialize_field("reason", &self.reason)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for RemotePlayerDespawnV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireDespawn {
            handle: PresenceHandle,
            server_sequence: u32,
            reason: DespawnReason,
        }
        let wire = WireDespawn::deserialize(deserializer)?;
        Self::new(wire.handle, wire.server_sequence, wire.reason).map_err(serde::de::Error::custom)
    }
}

/// An interaction observed against a remote player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceInteractionV1 {
    handle: PresenceHandle,
    observed_server_sequence: u32,
    observed_warp_sequence: u32,
    x: i16,
    y: i16,
}

impl PresenceInteractionV1 {
    /// Constructs and validates an observed interaction.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle or either observed sequence is zero.
    pub fn new(
        handle: PresenceHandle,
        observed_server_sequence: u32,
        observed_warp_sequence: u32,
        x: i16,
        y: i16,
    ) -> Result<Self, PresenceError> {
        ensure_nonzero(observed_server_sequence, "observed_server_sequence")?;
        ensure_nonzero(observed_warp_sequence, "observed_warp_sequence")?;
        Ok(Self {
            handle,
            observed_server_sequence,
            observed_warp_sequence,
            x,
            y,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle or either observed sequence is zero.
    pub fn validate(&self) -> Result<(), PresenceError> {
        Self::new(
            self.handle,
            self.observed_server_sequence,
            self.observed_warp_sequence,
            self.x,
            self.y,
        )
        .map(|_| ())
    }

    #[must_use]
    pub const fn handle(&self) -> PresenceHandle {
        self.handle
    }

    #[must_use]
    pub const fn observed_server_sequence(&self) -> u32 {
        self.observed_server_sequence
    }

    #[must_use]
    pub const fn observed_warp_sequence(&self) -> u32 {
        self.observed_warp_sequence
    }

    #[must_use]
    pub const fn x(&self) -> i16 {
        self.x
    }

    #[must_use]
    pub const fn y(&self) -> i16 {
        self.y
    }

    #[must_use]
    pub fn encode(&self) -> [u8; PRESENCE_INTERACTION_V1_SIZE] {
        let mut output = [0u8; PRESENCE_INTERACTION_V1_SIZE];
        write_u64(
            &mut output,
            PRESENCE_INTERACTION_V1_HANDLE_OFFSET,
            self.handle.as_u64(),
        );
        write_u32(
            &mut output,
            PRESENCE_INTERACTION_V1_SERVER_SEQUENCE_OFFSET,
            self.observed_server_sequence,
        );
        write_u32(
            &mut output,
            PRESENCE_INTERACTION_V1_WARP_SEQUENCE_OFFSET,
            self.observed_warp_sequence,
        );
        write_i16(&mut output, PRESENCE_INTERACTION_V1_X_OFFSET, self.x);
        write_i16(&mut output, PRESENCE_INTERACTION_V1_Y_OFFSET, self.y);
        output
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; PRESENCE_INTERACTION_V1_SIZE] {
        self.encode()
    }

    /// Decodes the exact little-endian interaction payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length, zero handle, or zero observed
    /// sequence.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, PRESENCE_INTERACTION_V1_SIZE, "presence interaction")?;
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, PRESENCE_INTERACTION_V1_HANDLE_OFFSET))?,
            read_u32(bytes, PRESENCE_INTERACTION_V1_SERVER_SEQUENCE_OFFSET),
            read_u32(bytes, PRESENCE_INTERACTION_V1_WARP_SEQUENCE_OFFSET),
            read_i16(bytes, PRESENCE_INTERACTION_V1_X_OFFSET),
            read_i16(bytes, PRESENCE_INTERACTION_V1_Y_OFFSET),
        )
    }
}

impl Serialize for PresenceInteractionV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("PresenceInteractionV1", 5)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("observed_server_sequence", &self.observed_server_sequence)?;
        output.serialize_field("observed_warp_sequence", &self.observed_warp_sequence)?;
        output.serialize_field("x", &self.x)?;
        output.serialize_field("y", &self.y)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for PresenceInteractionV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireInteraction {
            handle: PresenceHandle,
            observed_server_sequence: u32,
            observed_warp_sequence: u32,
            x: i16,
            y: i16,
        }
        let wire = WireInteraction::deserialize(deserializer)?;
        Self::new(
            wire.handle,
            wire.observed_server_sequence,
            wire.observed_warp_sequence,
            wire.x,
            wire.y,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldLocation;

    fn location() -> WorldLocation {
        WorldLocation::new(RegionId::Hoenn, 1, 3, -4, 9).expect("catalog location")
    }

    fn pose() -> PresencePoseV1 {
        PresencePoseV1::new(
            location(),
            7,
            Direction::East,
            0x1122_3344,
            0x5566_7788,
            MovementMode::Run,
            AnimationId::Locomotion,
            AvatarId::May,
            PlayerState::Overworld,
        )
        .unwrap()
    }

    fn state() -> LocalPresenceStateV1 {
        LocalPresenceStateV1::new(pose(), 0x99aa_bbcc).unwrap()
    }

    #[test]
    fn layout_constants_and_pose_golden_vector_are_stable() {
        assert_eq!(WORLD_LOCATION_V1_SIZE, 10);
        assert_eq!(PRESENCE_POSE_V1_SIZE, 24);
        let encoded = pose().encode();
        assert_eq!(
            encoded,
            [
                1, 1, 0, 3, 0, 252, 255, 9, 0, 0, 7, 4, 68, 51, 34, 17, 136, 119, 102, 85, 2, 1, 2,
                1,
            ]
        );
        assert_eq!(PresencePoseV1::decode(&encoded).unwrap(), pose());

        let readable = pose();
        assert_eq!(readable.location(), location());
        assert_eq!(readable.elevation(), 7);
        assert_eq!(readable.direction(), Direction::East);
        assert_eq!(readable.client_tick(), 0x1122_3344);
        assert_eq!(readable.warp_sequence(), 0x5566_7788);
        assert_eq!(readable.movement_mode(), MovementMode::Run);
        assert_eq!(readable.animation_id(), AnimationId::Locomotion);
        assert_eq!(readable.avatar_id(), AvatarId::May);
        assert_eq!(readable.player_state(), PlayerState::Overworld);
    }

    #[test]
    fn all_payloads_round_trip_exact_lengths() {
        let handle = PresenceHandle::new(0x0123_4567_89ab_cdef).unwrap();
        let username = CanonicalUsername::new("ash-kanto").unwrap();
        let local_state = state();
        let local_bytes = local_state.encode();
        assert_eq!(
            local_bytes,
            [
                1, 1, 0, 3, 0, 252, 255, 9, 0, 0, 7, 4, 68, 51, 34, 17, 136, 119, 102, 85, 2, 1, 2,
                1, 204, 187, 170, 153,
            ]
        );
        let spawn = RemotePlayerSpawnV1::new(handle, 3, local_state.clone(), username).unwrap();
        assert_eq!(spawn.encode().len(), REMOTE_PLAYER_SPAWN_V1_SIZE);
        assert_eq!(RemotePlayerSpawnV1::decode(&spawn.encode()).unwrap(), spawn);
        let spawn_bytes = spawn.encode();
        assert_eq!(
            &spawn_bytes[..12],
            &[239, 205, 171, 137, 103, 69, 35, 1, 3, 0, 0, 0]
        );
        assert_eq!(&spawn_bytes[12..40], &local_bytes);
        assert_eq!(&spawn_bytes[40..49], b"ash-kanto");
        assert!(spawn_bytes[49..].iter().all(|byte| *byte == 0));

        let update = RemotePlayerUpdateV1::new(handle, 4, local_state).unwrap();
        assert_eq!(update.encode().len(), REMOTE_PLAYER_UPDATE_V1_SIZE);
        assert_eq!(
            RemotePlayerUpdateV1::decode(&update.encode()).unwrap(),
            update
        );
        assert_eq!(
            update.encode(),
            [
                239, 205, 171, 137, 103, 69, 35, 1, 4, 0, 0, 0, 1, 1, 0, 3, 0, 252, 255, 9, 0, 0,
                7, 4, 68, 51, 34, 17, 136, 119, 102, 85, 2, 1, 2, 1, 204, 187, 170, 153,
            ]
        );
        let despawn = RemotePlayerDespawnV1::new(handle, 5, DespawnReason::Replaced).unwrap();
        assert_eq!(despawn.encode().len(), REMOTE_PLAYER_DESPAWN_V1_SIZE);
        assert_eq!(
            RemotePlayerDespawnV1::decode(&despawn.encode()).unwrap(),
            despawn
        );
        assert_eq!(
            despawn.encode(),
            [239, 205, 171, 137, 103, 69, 35, 1, 5, 0, 0, 0, 5, 0, 0, 0]
        );
        let interaction = PresenceInteractionV1::new(handle, 4, 2, -4, 9).unwrap();
        assert_eq!(interaction.encode().len(), PRESENCE_INTERACTION_V1_SIZE);
        assert_eq!(
            PresenceInteractionV1::decode(&interaction.encode()).unwrap(),
            interaction
        );
        assert_eq!(
            interaction.encode(),
            [
                239, 205, 171, 137, 103, 69, 35, 1, 4, 0, 0, 0, 2, 0, 0, 0, 252, 255, 9, 0,
            ]
        );
    }

    #[test]
    fn malformed_wire_values_fail_closed() {
        assert!(matches!(
            PresencePoseV1::decode(&[0; 23]),
            Err(PresenceError::InvalidLength { .. })
        ));
        let mut bytes = pose().encode();
        bytes[PRESENCE_POSE_V1_LOCATION_OFFSET + WORLD_LOCATION_V1_RESERVED_OFFSET] = 1;
        assert!(matches!(
            PresencePoseV1::decode(&bytes),
            Err(PresenceError::NonZeroReserved { .. })
        ));
        let mut bytes = pose().encode();
        bytes[PRESENCE_POSE_V1_WARP_SEQUENCE_OFFSET..PRESENCE_POSE_V1_WARP_SEQUENCE_OFFSET + 4]
            .fill(0);
        assert!(matches!(
            PresencePoseV1::decode(&bytes),
            Err(PresenceError::ZeroValue { .. })
        ));
        let mut bytes =
            RemotePlayerDespawnV1::new(PresenceHandle::new(1).unwrap(), 1, DespawnReason::Hidden)
                .unwrap()
                .encode();
        bytes[REMOTE_PLAYER_DESPAWN_V1_RESERVED_OFFSET] = 1;
        assert!(matches!(
            RemotePlayerDespawnV1::decode(&bytes),
            Err(PresenceError::NonZeroReserved { .. })
        ));
        assert!(CanonicalUsername::new("ab_").is_err());
        assert!(CanonicalUsername::new("Abc").is_err());
        assert!(PresenceHandle::new(0).is_err());
        assert!(PresenceHandle::from_hex("0123456789ABCDEF").is_err());
        assert!(PresenceHandle::from_hex("1").is_err());
        assert!(PresenceHandle::from_hex("0000000000000000").is_err());
    }

    #[test]
    fn enum_ordinals_are_closed() {
        assert_eq!(Direction::from_wire(1), Ok(Direction::South));
        assert_eq!(MovementMode::from_wire(2), Ok(MovementMode::Run));
        assert_eq!(AnimationId::from_wire(1), Ok(AnimationId::Locomotion));
        assert_eq!(AvatarId::from_wire(2), Ok(AvatarId::May));
        assert_eq!(PlayerState::from_wire(0), Ok(PlayerState::Hidden));
        assert_eq!(
            DespawnReason::from_wire(6),
            Ok(DespawnReason::PartitionLeft)
        );
        assert!(Direction::from_wire(0).is_err());
        assert!(DespawnReason::from_wire(0).is_err());
        assert!(serde_json::from_str::<Direction>("\"UP\"").is_err());
    }

    #[test]
    fn rfc1982_sequence_comparison_and_increment_skip_zero() {
        assert!(sequence_is_newer(2, 1));
        assert!(!sequence_is_newer(1, 1));
        assert!(!sequence_is_newer(1, 2));
        assert!(sequence_is_newer(1, u32::MAX));
        assert!(!sequence_is_newer(u32::MAX, 1));
        assert!(!sequence_is_newer(0, 1));
        assert!(!sequence_is_newer(1, 0));
        assert!(!sequence_is_newer(0x8000_0001, 1));
        assert_eq!(next_sequence(0), 1);
        assert_eq!(next_sequence(u32::MAX), 1);
        assert_eq!(increment_sequence(u32::MAX - 1), u32::MAX);
    }

    #[test]
    fn handles_and_structures_use_strict_human_json() {
        let handle = PresenceHandle::new(0x0123_4567_89ab_cdef).unwrap();
        assert_eq!(
            serde_json::to_string(&handle).unwrap(),
            "\"0123456789abcdef\""
        );
        assert_eq!(
            serde_json::from_str::<PresenceHandle>("\"0123456789abcdef\"").unwrap(),
            handle
        );
        for invalid in [
            "1",
            "0123456789ABCDEf",
            "0123456789abcdef0",
            "0123456789abcdeG",
            "0000000000000000",
        ] {
            assert!(serde_json::from_str::<PresenceHandle>(&format!("\"{invalid}\"")).is_err());
        }
        assert!(serde_json::from_str::<PresenceHandle>("1").is_err());

        let json = serde_json::to_string(&pose()).unwrap();
        assert!(json.contains("\"location\""));
        assert!(
            serde_json::from_str::<PresencePoseV1>(&json.replace(
                "\"player_state\":\"OVERWORLD\"",
                "\"player_state\":\"OVERWORLD\",\"extra\":1"
            ))
            .is_err()
        );
        assert!(serde_json::from_str::<PresencePoseV1>(
            r#"{"location":{"region":"HOENN","map_group":1,"map_number":3,"x":-4,"y":9,"extra":true},"elevation":7,"direction":"EAST","client_tick":287454020,"warp_sequence":1432778632,"movement_mode":"RUN","animation_id":"LOCOMOTION","avatar_id":"MAY","player_state":"OVERWORLD"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<RemotePlayerDespawnV1>(
            r#"{"handle":"0000000000000001","server_sequence":1,"reason":"HIDDEN","extra":true}"#
        )
        .is_err());

        let invalid_pose = PresencePoseV1 {
            location: location(),
            elevation: 0,
            direction: Direction::South,
            client_tick: 0,
            warp_sequence: 0,
            movement_mode: MovementMode::Idle,
            animation_id: AnimationId::Idle,
            avatar_id: AvatarId::Brendan,
            player_state: PlayerState::Hidden,
        };
        assert!(serde_json::to_string(&invalid_pose).is_err());
    }

    #[test]
    fn username_fixed_padding_is_validated() {
        let handle = PresenceHandle::new(1).unwrap();
        let spawn =
            RemotePlayerSpawnV1::new(handle, 1, state(), CanonicalUsername::new("abc").unwrap())
                .unwrap();
        let encoded = spawn.encode();
        assert_eq!(
            &encoded[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET
                ..REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET + 3],
            b"abc"
        );
        assert!(
            encoded[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET + 3..]
                .iter()
                .all(|byte| *byte == 0)
        );
        let mut malformed = encoded;
        malformed[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET + 4] = 1;
        assert!(matches!(
            RemotePlayerSpawnV1::decode(&malformed),
            Err(PresenceError::InvalidUsername { .. })
        ));
        let mut malformed = encoded;
        malformed[REMOTE_PLAYER_SPAWN_V1_USERNAME_OFFSET] = b'A';
        assert!(matches!(
            RemotePlayerSpawnV1::decode(&malformed),
            Err(PresenceError::InvalidUsername { .. })
        ));
    }
}

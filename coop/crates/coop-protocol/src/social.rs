//! Companion (follower Pokemon) and social-signal (ping/emote) wire contracts.
//!
//! These payloads are intentionally independent of Rust's in-memory layout.
//! They are encoded field-by-field as little-endian bytes, matching the
//! style of [`crate::presence`], so the ROM and Lua adapters can consume
//! them with fixed offsets.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::{PresenceError, PresenceHandle};

/// Local companion payload layout (8 bytes).
pub const LOCAL_COMPANION_V1_SIZE: usize = 8;
/// Remote companion payload layout (16 bytes).
pub const REMOTE_COMPANION_V1_SIZE: usize = 16;
/// Local social-signal payload layout (12 bytes).
pub const LOCAL_SIGNAL_V1_SIZE: usize = 12;
/// Remote social-signal payload layout (20 bytes).
pub const REMOTE_SIGNAL_V1_SIZE: usize = 20;

/// Readable aliases used by adapters that call these records "payloads".
pub const LOCAL_COMPANION_V1_LEN: usize = LOCAL_COMPANION_V1_SIZE;
pub const REMOTE_COMPANION_V1_LEN: usize = REMOTE_COMPANION_V1_SIZE;
pub const LOCAL_SIGNAL_V1_LEN: usize = LOCAL_SIGNAL_V1_SIZE;
pub const REMOTE_SIGNAL_V1_LEN: usize = REMOTE_SIGNAL_V1_SIZE;

/// Species zero is reserved; the ROM additionally clamps to `NUM_SPECIES`.
pub const COMPANION_SPECIES_NONE: u16 = 0;
/// Only bit 0 (shiny) is defined; all other flag bits are reserved.
pub const COMPANION_FLAG_SHINY: u8 = 0x01;
/// Form ids are bounded so a corrupt byte cannot address open-ended graphics.
pub const COMPANION_FORM_MAX: u8 = 63;
/// Emote zero means "no emote" and is only legal on ping signals.
pub const EMOTE_NONE: u8 = 0;
/// Highest assigned emote ordinal.
pub const EMOTE_MAX: u8 = 8;

fn require_len<'a>(
    bytes: &'a [u8],
    expected: usize,
    kind: &'static str,
) -> Result<&'a [u8], PresenceError> {
    if bytes.len() == expected {
        Ok(bytes)
    } else {
        Err(PresenceError::InvalidLength {
            kind,
            expected,
            actual: bytes.len(),
        })
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

fn check_companion_fields(species: u16, form: u8, flags: u8) -> Result<(), PresenceError> {
    if species == COMPANION_SPECIES_NONE {
        return Err(PresenceError::ZeroValue { field: "species" });
    }
    if form > COMPANION_FORM_MAX {
        return Err(PresenceError::UnknownEnum {
            field: "form",
            value: form,
        });
    }
    if flags & !COMPANION_FLAG_SHINY != 0 {
        return Err(PresenceError::NonZeroReserved {
            offset: 0,
            value: flags,
        });
    }
    Ok(())
}

/// Closed social-signal kinds used on the wire.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SignalKind {
    /// A map-location ping; carries tile coordinates.
    Ping = 1,
    /// A chat-bubble emote; carries an emote ordinal.
    Emote = 2,
}

impl SignalKind {
    /// Returns the wire ordinal.
    #[must_use]
    pub const fn wire(self) -> u8 {
        self as u8
    }

    /// Converts a wire ordinal into this closed enum.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::UnknownEnum`] for an unassigned ordinal.
    pub const fn from_wire(value: u8) -> Result<Self, PresenceError> {
        match value {
            1 => Ok(Self::Ping),
            2 => Ok(Self::Emote),
            value => Err(PresenceError::UnknownEnum {
                field: "signal_kind",
                value,
            }),
        }
    }

    const fn token(self) -> &'static str {
        match self {
            Self::Ping => "PING",
            Self::Emote => "EMOTE",
        }
    }

    fn parse_token(value: &str) -> Result<Self, PresenceError> {
        match value {
            "PING" => Ok(Self::Ping),
            "EMOTE" => Ok(Self::Emote),
            _ => Err(PresenceError::UnknownEnum {
                field: "signal_kind",
                value: 0,
            }),
        }
    }
}

impl Serialize for SignalKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for SignalKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        Self::parse_token(&token).map_err(serde::de::Error::custom)
    }
}

/// Closed emote ordinals used on the wire.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum EmoteId {
    /// No emote; only legal on ping signals.
    None = 0,
    /// `!` surprise bubble.
    Exclaim = 1,
    /// `?` question bubble.
    Question = 2,
    /// Affection bubble.
    Heart = 3,
    /// Music-notes bubble.
    Music = 4,
    /// Nervous-sweat bubble.
    Sweat = 5,
    /// Anger bubble.
    Anger = 6,
    /// Sleeping bubble.
    Sleep = 7,
    /// Sparkle bubble.
    Star = 8,
}

impl EmoteId {
    /// Returns the wire ordinal.
    #[must_use]
    pub const fn wire(self) -> u8 {
        self as u8
    }

    /// Converts a wire ordinal into this closed enum.
    ///
    /// # Errors
    ///
    /// Returns [`PresenceError::UnknownEnum`] for an unassigned ordinal.
    pub const fn from_wire(value: u8) -> Result<Self, PresenceError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Exclaim),
            2 => Ok(Self::Question),
            3 => Ok(Self::Heart),
            4 => Ok(Self::Music),
            5 => Ok(Self::Sweat),
            6 => Ok(Self::Anger),
            7 => Ok(Self::Sleep),
            8 => Ok(Self::Star),
            value => Err(PresenceError::UnknownEnum {
                field: "emote_id",
                value,
            }),
        }
    }

    const fn token(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Exclaim => "EXCLAIM",
            Self::Question => "QUESTION",
            Self::Heart => "HEART",
            Self::Music => "MUSIC",
            Self::Sweat => "SWEAT",
            Self::Anger => "ANGER",
            Self::Sleep => "SLEEP",
            Self::Star => "STAR",
        }
    }

    fn parse_token(value: &str) -> Result<Self, PresenceError> {
        match value {
            "NONE" => Ok(Self::None),
            "EXCLAIM" => Ok(Self::Exclaim),
            "QUESTION" => Ok(Self::Question),
            "HEART" => Ok(Self::Heart),
            "MUSIC" => Ok(Self::Music),
            "SWEAT" => Ok(Self::Sweat),
            "ANGER" => Ok(Self::Anger),
            "SLEEP" => Ok(Self::Sleep),
            "STAR" => Ok(Self::Star),
            _ => Err(PresenceError::UnknownEnum {
                field: "emote_id",
                value: 0,
            }),
        }
    }
}

impl Serialize for EmoteId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for EmoteId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        Self::parse_token(&token).map_err(serde::de::Error::custom)
    }
}

/// Local follower companion plus its source sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalCompanionV1 {
    species: u16,
    form: u8,
    flags: u8,
    source_sequence: u32,
}

impl LocalCompanionV1 {
    /// Constructs and validates local companion state.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero species, out-of-range form, reserved
    /// flag bits, or a zero source sequence.
    pub fn new(
        species: u16,
        form: u8,
        flags: u8,
        source_sequence: u32,
    ) -> Result<Self, PresenceError> {
        if species == COMPANION_SPECIES_NONE {
            return Err(PresenceError::ZeroValue { field: "species" });
        }
        if form > COMPANION_FORM_MAX {
            return Err(PresenceError::UnknownEnum {
                field: "form",
                value: form,
            });
        }
        if flags & !COMPANION_FLAG_SHINY != 0 {
            return Err(PresenceError::NonZeroReserved {
                offset: 0,
                value: flags,
            });
        }
        if source_sequence == 0 {
            return Err(PresenceError::ZeroValue {
                field: "source_sequence",
            });
        }
        Ok(Self {
            species,
            form,
            flags,
            source_sequence,
        })
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when any field is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        match Self::new(self.species, self.form, self.flags, self.source_sequence) {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Returns the lead-party species id.
    #[must_use]
    pub const fn species(self) -> u16 {
        self.species
    }

    /// Returns the bounded form id.
    #[must_use]
    pub const fn form(self) -> u8 {
        self.form
    }

    /// Returns the flag bits (only bit 0 is defined).
    #[must_use]
    pub const fn flags(self) -> u8 {
        self.flags
    }

    /// Returns whether the shiny flag bit is set.
    #[must_use]
    pub const fn shiny(self) -> bool {
        self.flags & COMPANION_FLAG_SHINY != 0
    }

    /// Returns the source sequence.
    #[must_use]
    pub const fn source_sequence(self) -> u32 {
        self.source_sequence
    }

    /// Encodes the exact little-endian local-companion payload.
    #[must_use]
    pub fn encode(self) -> [u8; LOCAL_COMPANION_V1_SIZE] {
        let mut output = [0u8; LOCAL_COMPANION_V1_SIZE];
        write_u16(&mut output, 0, self.species);
        output[2] = self.form;
        output[3] = self.flags;
        write_u32(&mut output, 4, self.source_sequence);
        output
    }

    /// Decodes the exact little-endian local-companion payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length or any invalid field.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, LOCAL_COMPANION_V1_SIZE, "local companion")?;
        Self::new(read_u16(bytes, 0), bytes[2], bytes[3], read_u32(bytes, 4))
    }
}

impl Serialize for LocalCompanionV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("LocalCompanionV1", 4)?;
        output.serialize_field("species", &self.species)?;
        output.serialize_field("form", &self.form)?;
        output.serialize_field("flags", &self.flags)?;
        output.serialize_field("source_sequence", &self.source_sequence)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for LocalCompanionV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            species: u16,
            form: u8,
            flags: u8,
            source_sequence: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.species, wire.form, wire.flags, wire.source_sequence)
            .map_err(serde::de::Error::custom)
    }
}

/// A remote follower companion fanned out by the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteCompanionV1 {
    handle: PresenceHandle,
    server_sequence: u32,
    species: u16,
    form: u8,
    flags: u8,
}

impl RemoteCompanionV1 {
    /// Constructs and validates a remote companion record.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero handle or sequence, or any invalid
    /// companion field.
    pub fn new(
        handle: PresenceHandle,
        server_sequence: u32,
        species: u16,
        form: u8,
        flags: u8,
    ) -> Result<Self, PresenceError> {
        if server_sequence == 0 {
            return Err(PresenceError::ZeroValue {
                field: "server_sequence",
            });
        }
        match check_companion_fields(species, form, flags) {
            Ok(()) => Ok(Self {
                handle,
                server_sequence,
                species,
                form,
                flags,
            }),
            Err(error) => Err(error),
        }
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when any field is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        match Self::new(
            self.handle,
            self.server_sequence,
            self.species,
            self.form,
            self.flags,
        ) {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Returns the remote handle.
    #[must_use]
    pub const fn handle(self) -> PresenceHandle {
        self.handle
    }

    /// Returns the server sequence.
    #[must_use]
    pub const fn server_sequence(self) -> u32 {
        self.server_sequence
    }

    /// Returns the lead-party species id.
    #[must_use]
    pub const fn species(self) -> u16 {
        self.species
    }

    /// Returns the bounded form id.
    #[must_use]
    pub const fn form(self) -> u8 {
        self.form
    }

    /// Returns the flag bits (only bit 0 is defined).
    #[must_use]
    pub const fn flags(self) -> u8 {
        self.flags
    }

    /// Returns whether the shiny flag bit is set.
    #[must_use]
    pub const fn shiny(self) -> bool {
        self.flags & COMPANION_FLAG_SHINY != 0
    }

    /// Encodes the exact little-endian remote-companion payload.
    #[must_use]
    pub fn encode(self) -> [u8; REMOTE_COMPANION_V1_SIZE] {
        let mut output = [0u8; REMOTE_COMPANION_V1_SIZE];
        write_u64(&mut output, 0, self.handle.as_u64());
        write_u32(&mut output, 8, self.server_sequence);
        write_u16(&mut output, 12, self.species);
        output[14] = self.form;
        output[15] = self.flags;
        output
    }

    /// Decodes the exact little-endian remote-companion payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length or any invalid field.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, REMOTE_COMPANION_V1_SIZE, "remote companion")?;
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, 0))?,
            read_u32(bytes, 8),
            read_u16(bytes, 12),
            bytes[14],
            bytes[15],
        )
    }
}

impl Serialize for RemoteCompanionV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("RemoteCompanionV1", 5)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("server_sequence", &self.server_sequence)?;
        output.serialize_field("species", &self.species)?;
        output.serialize_field("form", &self.form)?;
        output.serialize_field("flags", &self.flags)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for RemoteCompanionV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            handle: PresenceHandle,
            server_sequence: u32,
            species: u16,
            form: u8,
            flags: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.handle,
            wire.server_sequence,
            wire.species,
            wire.form,
            wire.flags,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn check_signal_fields(
    kind: SignalKind,
    emote: EmoteId,
    x: i16,
    y: i16,
) -> Result<(), PresenceError> {
    match kind {
        SignalKind::Ping => {
            if emote != EmoteId::None {
                return Err(PresenceError::UnknownEnum {
                    field: "emote_id",
                    value: emote.wire(),
                });
            }
            Ok(())
        }
        SignalKind::Emote => {
            if emote == EmoteId::None {
                return Err(PresenceError::ZeroValue { field: "emote_id" });
            }
            if x != 0 || y != 0 {
                return Err(PresenceError::NonZeroReserved {
                    offset: 0,
                    value: 1,
                });
            }
            Ok(())
        }
    }
}

/// A local ping/emote signal plus its source sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSignalV1 {
    kind: SignalKind,
    emote: EmoteId,
    x: i16,
    y: i16,
    source_sequence: u32,
}

impl LocalSignalV1 {
    /// Constructs and validates a local signal.
    ///
    /// Pings carry tile coordinates and no emote; emotes carry an emote
    /// ordinal and no coordinates.
    ///
    /// # Errors
    ///
    /// Returns an error for a mixed ping/emote body or a zero source
    /// sequence.
    pub fn new(
        kind: SignalKind,
        emote: EmoteId,
        x: i16,
        y: i16,
        source_sequence: u32,
    ) -> Result<Self, PresenceError> {
        if source_sequence == 0 {
            return Err(PresenceError::ZeroValue {
                field: "source_sequence",
            });
        }
        match check_signal_fields(kind, emote, x, y) {
            Ok(()) => Ok(Self {
                kind,
                emote,
                x,
                y,
                source_sequence,
            }),
            Err(error) => Err(error),
        }
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when any field is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        match Self::new(self.kind, self.emote, self.x, self.y, self.source_sequence) {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Returns the signal kind.
    #[must_use]
    pub const fn kind(self) -> SignalKind {
        self.kind
    }

    /// Returns the emote ordinal (`NONE` on pings).
    #[must_use]
    pub const fn emote(self) -> EmoteId {
        self.emote
    }

    /// Returns the ping tile x (zero on emotes).
    #[must_use]
    pub const fn x(self) -> i16 {
        self.x
    }

    /// Returns the ping tile y (zero on emotes).
    #[must_use]
    pub const fn y(self) -> i16 {
        self.y
    }

    /// Returns the source sequence.
    #[must_use]
    pub const fn source_sequence(self) -> u32 {
        self.source_sequence
    }

    /// Encodes the exact little-endian local-signal payload.
    #[must_use]
    pub fn encode(self) -> [u8; LOCAL_SIGNAL_V1_SIZE] {
        let mut output = [0u8; LOCAL_SIGNAL_V1_SIZE];
        output[0] = self.kind.wire();
        output[1] = self.emote.wire();
        write_i16(&mut output, 2, self.x);
        write_i16(&mut output, 4, self.y);
        write_u32(&mut output, 8, self.source_sequence);
        output
    }

    /// Decodes the exact little-endian local-signal payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length, reserved bytes, unknown
    /// ordinals, a mixed body, or a zero source sequence.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, LOCAL_SIGNAL_V1_SIZE, "local signal")?;
        if bytes[6] != 0 || bytes[7] != 0 {
            return Err(PresenceError::NonZeroReserved {
                offset: 6,
                value: bytes[6],
            });
        }
        Self::new(
            SignalKind::from_wire(bytes[0])?,
            EmoteId::from_wire(bytes[1])?,
            read_i16(bytes, 2),
            read_i16(bytes, 4),
            read_u32(bytes, 8),
        )
    }
}

impl Serialize for LocalSignalV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("LocalSignalV1", 5)?;
        output.serialize_field("kind", &self.kind)?;
        output.serialize_field("emote", &self.emote)?;
        output.serialize_field("x", &self.x)?;
        output.serialize_field("y", &self.y)?;
        output.serialize_field("source_sequence", &self.source_sequence)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for LocalSignalV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            kind: SignalKind,
            emote: EmoteId,
            x: i16,
            y: i16,
            source_sequence: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.kind, wire.emote, wire.x, wire.y, wire.source_sequence)
            .map_err(serde::de::Error::custom)
    }
}

/// A remote ping/emote signal fanned out by the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteSignalV1 {
    handle: PresenceHandle,
    server_sequence: u32,
    kind: SignalKind,
    emote: EmoteId,
    x: i16,
    y: i16,
}

impl RemoteSignalV1 {
    /// Constructs and validates a remote signal record.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero handle or sequence, reserved bytes, or
    /// a mixed ping/emote body.
    pub fn new(
        handle: PresenceHandle,
        server_sequence: u32,
        kind: SignalKind,
        emote: EmoteId,
        x: i16,
        y: i16,
    ) -> Result<Self, PresenceError> {
        if server_sequence == 0 {
            return Err(PresenceError::ZeroValue {
                field: "server_sequence",
            });
        }
        match check_signal_fields(kind, emote, x, y) {
            Ok(()) => Ok(Self {
                handle,
                server_sequence,
                kind,
                emote,
                x,
                y,
            }),
            Err(error) => Err(error),
        }
    }

    /// Validates all invariants without changing the value.
    ///
    /// # Errors
    ///
    /// Returns an error when any field is invalid.
    pub fn validate(&self) -> Result<(), PresenceError> {
        match Self::new(
            self.handle,
            self.server_sequence,
            self.kind,
            self.emote,
            self.x,
            self.y,
        ) {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Returns the remote handle.
    #[must_use]
    pub const fn handle(self) -> PresenceHandle {
        self.handle
    }

    /// Returns the server sequence.
    #[must_use]
    pub const fn server_sequence(self) -> u32 {
        self.server_sequence
    }

    /// Returns the signal kind.
    #[must_use]
    pub const fn kind(self) -> SignalKind {
        self.kind
    }

    /// Returns the emote ordinal (`NONE` on pings).
    #[must_use]
    pub const fn emote(self) -> EmoteId {
        self.emote
    }

    /// Returns the ping tile x (zero on emotes).
    #[must_use]
    pub const fn x(self) -> i16 {
        self.x
    }

    /// Returns the ping tile y (zero on emotes).
    #[must_use]
    pub const fn y(self) -> i16 {
        self.y
    }

    /// Encodes the exact little-endian remote-signal payload.
    #[must_use]
    pub fn encode(self) -> [u8; REMOTE_SIGNAL_V1_SIZE] {
        let mut output = [0u8; REMOTE_SIGNAL_V1_SIZE];
        write_u64(&mut output, 0, self.handle.as_u64());
        write_u32(&mut output, 8, self.server_sequence);
        output[12] = self.kind.wire();
        output[13] = self.emote.wire();
        write_i16(&mut output, 14, self.x);
        write_i16(&mut output, 16, self.y);
        output
    }

    /// Decodes the exact little-endian remote-signal payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong length, reserved bytes, unknown
    /// ordinals, or a mixed body.
    pub fn decode(bytes: &[u8]) -> Result<Self, PresenceError> {
        let bytes = require_len(bytes, REMOTE_SIGNAL_V1_SIZE, "remote signal")?;
        if bytes[18] != 0 || bytes[19] != 0 {
            return Err(PresenceError::NonZeroReserved {
                offset: 18,
                value: bytes[18],
            });
        }
        Self::new(
            PresenceHandle::from_wire(read_u64(bytes, 0))?,
            read_u32(bytes, 8),
            SignalKind::from_wire(bytes[12])?,
            EmoteId::from_wire(bytes[13])?,
            read_i16(bytes, 14),
            read_i16(bytes, 16),
        )
    }
}

impl Serialize for RemoteSignalV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_struct("RemoteSignalV1", 6)?;
        output.serialize_field("handle", &self.handle)?;
        output.serialize_field("server_sequence", &self.server_sequence)?;
        output.serialize_field("kind", &self.kind)?;
        output.serialize_field("emote", &self.emote)?;
        output.serialize_field("x", &self.x)?;
        output.serialize_field("y", &self.y)?;
        output.end()
    }
}

impl<'de> Deserialize<'de> for RemoteSignalV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            handle: PresenceHandle,
            server_sequence: u32,
            kind: SignalKind,
            emote: EmoteId,
            x: i16,
            y: i16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.handle,
            wire.server_sequence,
            wire.kind,
            wire.emote,
            wire.x,
            wire.y,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Compatibility alias for callers that shorten the local companion name.
pub type CompanionStateV1 = LocalCompanionV1;
/// Compatibility alias for callers that shorten the remote companion name.
pub type CompanionUpdateV1 = RemoteCompanionV1;
/// Compatibility alias for callers that shorten the local signal name.
pub type SocialSignalStateV1 = LocalSignalV1;
/// Compatibility alias for callers that shorten the remote signal name.
pub type SocialSignalUpdateV1 = RemoteSignalV1;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handle() -> PresenceHandle {
        PresenceHandle::new(0x0102_0304_0506_0708).expect("test handle is non-zero")
    }

    #[test]
    fn local_companion_round_trip_and_rejects_reserved_bits() {
        let value = LocalCompanionV1::new(25, 0, COMPANION_FLAG_SHINY, 7).expect("valid");
        assert!(value.shiny());
        assert_eq!(LocalCompanionV1::decode(&value.encode()), Ok(value));
        assert_eq!(value.encode().len(), LOCAL_COMPANION_V1_SIZE);

        assert!(LocalCompanionV1::new(0, 0, 0, 1).is_err());
        assert!(LocalCompanionV1::new(1, COMPANION_FORM_MAX + 1, 0, 1).is_err());
        assert!(LocalCompanionV1::new(1, 0, 0x02, 1).is_err());
        assert!(LocalCompanionV1::new(1, 0, 0, 0).is_err());
        assert!(LocalCompanionV1::decode(&[0; 7]).is_err());
    }

    #[test]
    fn remote_companion_round_trip_and_rejects_zero_sequence() {
        let value = RemoteCompanionV1::new(handle(), 9, 25, 1, 0).expect("valid");
        assert_eq!(value.handle(), handle());
        assert_eq!(RemoteCompanionV1::decode(&value.encode()), Ok(value));
        assert_eq!(value.encode().len(), REMOTE_COMPANION_V1_SIZE);

        assert!(RemoteCompanionV1::new(handle(), 0, 25, 0, 0).is_err());
        assert!(RemoteCompanionV1::new(handle(), 1, 0, 0, 0).is_err());
        assert!(RemoteCompanionV1::decode(&[0; 15]).is_err());
    }

    #[test]
    fn local_signal_ping_and_emote_bodies_are_exclusive() {
        let ping = LocalSignalV1::new(SignalKind::Ping, EmoteId::None, 10, -4, 3).expect("valid");
        assert_eq!(LocalSignalV1::decode(&ping.encode()), Ok(ping));

        let emote = LocalSignalV1::new(SignalKind::Emote, EmoteId::Heart, 0, 0, 4).expect("valid");
        assert_eq!(LocalSignalV1::decode(&emote.encode()), Ok(emote));

        assert!(LocalSignalV1::new(SignalKind::Ping, EmoteId::Heart, 1, 1, 1).is_err());
        assert!(LocalSignalV1::new(SignalKind::Emote, EmoteId::None, 0, 0, 1).is_err());
        assert!(LocalSignalV1::new(SignalKind::Emote, EmoteId::Star, 1, 0, 1).is_err());
        assert!(LocalSignalV1::new(SignalKind::Ping, EmoteId::None, 0, 0, 0).is_err());

        let mut bytes = ping.encode();
        bytes[6] = 1;
        assert!(LocalSignalV1::decode(&bytes).is_err());
        assert!(LocalSignalV1::decode(&[0; 11]).is_err());
    }

    #[test]
    fn remote_signal_round_trip_and_rejects_reserved_tail() {
        let ping = RemoteSignalV1::new(handle(), 11, SignalKind::Ping, EmoteId::None, 6, 7)
            .expect("valid");
        assert_eq!(RemoteSignalV1::decode(&ping.encode()), Ok(ping));

        let emote = RemoteSignalV1::new(handle(), 12, SignalKind::Emote, EmoteId::Exclaim, 0, 0)
            .expect("valid");
        assert_eq!(RemoteSignalV1::decode(&emote.encode()), Ok(emote));

        let mut bytes = ping.encode();
        bytes[19] = 1;
        assert!(RemoteSignalV1::decode(&bytes).is_err());
        assert!(RemoteSignalV1::decode(&[0; 19]).is_err());
    }

    #[test]
    fn social_json_round_trip_rejects_unknown_fields() {
        let companion = LocalCompanionV1::new(150, 2, 0, 5).expect("valid");
        let value = serde_json::to_value(companion).expect("serializes");
        assert_eq!(
            value,
            json!({"species": 150, "form": 2, "flags": 0, "source_sequence": 5})
        );
        assert_eq!(
            serde_json::from_value::<LocalCompanionV1>(value).expect("deserializes"),
            companion
        );

        let signal = LocalSignalV1::new(SignalKind::Emote, EmoteId::Music, 0, 0, 6).expect("valid");
        let mut value = serde_json::to_value(signal).expect("serializes");
        assert_eq!(value["kind"], json!("EMOTE"));
        assert_eq!(value["emote"], json!("MUSIC"));
        value["unknown"] = json!(1);
        assert!(serde_json::from_value::<LocalSignalV1>(value).is_err());
    }

    #[test]
    fn signal_kind_and_emote_reject_unknown_ordinals() {
        assert!(SignalKind::from_wire(0).is_err());
        assert!(SignalKind::from_wire(3).is_err());
        assert!(EmoteId::from_wire(EMOTE_MAX + 1).is_err());
        assert_eq!(EmoteId::from_wire(0), Ok(EmoteId::None));
    }
}

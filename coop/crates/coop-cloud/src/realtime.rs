//! Strict, bounded JSON contracts for authenticated realtime presence.
//!
//! This module deliberately stops at DTOs and codecs.  Ticket redemption,
//! transport, persistence, and lease revalidation remain server-owned work.

use std::{fmt, str};

use coop_protocol::{
    LocalPresenceStateV1, PresenceHandle, PresenceInteractionV1, RemotePlayerDespawnV1,
    RemotePlayerSpawnV1, RemotePlayerUpdateV1,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{RealtimeTicket, RuntimeLeaseFence, UnixTimestampMillis};

/// The only realtime wire version currently implemented.
pub const CURRENT_REALTIME_VERSION: u16 = 1;

/// Number of entropy bytes in a realtime ticket.
pub const REALTIME_TICKET_ENTROPY_BYTES: usize = 32;
/// Number of unpadded base64url bytes in a realtime ticket.
pub const REALTIME_TICKET_ENCODED_LEN: usize = 43;
/// Ticket lifetime issued by the later realtime server, in milliseconds.
pub const REALTIME_TICKET_TTL_MS: u64 = 30_000;
/// Maximum assembled mint request body, in bytes.
pub const REALTIME_TICKET_REQUEST_BODY_MAX_BYTES: usize = 8 * 1024;
/// Fixed server-to-client presence send rate.
pub const PRESENCE_SEND_RATE_HZ: u16 = 10;
/// Fixed client interpolation delay.
pub const PRESENCE_INTERPOLATION_DELAY_MS: u32 = 100;
/// Fixed client stale-presence threshold.
pub const PRESENCE_STALE_MS: u32 = 1_500;
/// Maximum assembled client realtime text frame, in bytes.
pub const MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES: usize = 1_024;
/// Maximum assembled server realtime text frame, in bytes.
pub const MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES: usize = 2_048;

/// Stable errors returned by realtime DTO and codec operations.
///
/// Variants intentionally carry no parser or attacker-controlled text.  This
/// keeps their `Display` output suitable for public protocol boundaries.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeError {
    #[error("invalid realtime version")]
    InvalidVersion,
    #[error("invalid realtime ticket")]
    InvalidTicket,
    #[error("realtime ticket expiry is invalid")]
    InvalidExpiry,
    #[error("realtime ticket expiry overflow")]
    ExpiryOverflow,
    #[error("realtime message exceeds its size limit")]
    MessageTooLarge,
    #[error("realtime message is not valid UTF-8")]
    InvalidUtf8,
    #[error("realtime message is malformed")]
    MalformedMessage,
    #[error("realtime message could not be encoded within its size limit")]
    EncodedMessageTooLarge,
}

/// Numeric V1 realtime version with no open-ended future values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RealtimeVersion(u16);

impl RealtimeVersion {
    /// Constructs the only supported realtime version.
    ///
    /// # Errors
    ///
    /// Returns an error for every value other than V1.
    pub const fn new(value: u16) -> Result<Self, RealtimeVersionError> {
        if value == CURRENT_REALTIME_VERSION {
            Ok(Self(value))
        } else {
            Err(RealtimeVersionError::Unsupported)
        }
    }

    /// Returns the V1 wire version.
    #[must_use]
    pub const fn v1() -> Self {
        Self(CURRENT_REALTIME_VERSION)
    }

    /// Alias for [`Self::v1`].
    #[must_use]
    pub const fn current() -> Self {
        Self::v1()
    }

    /// Parses a numeric wire version.
    ///
    /// # Errors
    ///
    /// Returns an error for every value other than V1.
    pub const fn from_u16(value: u16) -> Result<Self, RealtimeVersionError> {
        Self::new(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Version construction failure without reflecting an untrusted value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RealtimeVersionError {
    #[error("unsupported realtime version")]
    Unsupported,
}

impl Serialize for RealtimeVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for RealtimeVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u16::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A request to mint a capability for one complete runtime lease fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintRealtimeTicketRequest {
    realtime_version: RealtimeVersion,
    runtime: RuntimeLeaseFence,
}

#[derive(Serialize)]
struct MintRealtimeTicketRequestWire<'a> {
    realtime_version: RealtimeVersion,
    runtime: &'a RuntimeLeaseFence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MintRealtimeTicketRequestWireOwned {
    realtime_version: RealtimeVersion,
    runtime: RuntimeLeaseFence,
}

impl MintRealtimeTicketRequest {
    /// Constructs a V1 mint request.
    #[must_use]
    pub const fn new(realtime_version: RealtimeVersion, runtime: RuntimeLeaseFence) -> Self {
        Self {
            realtime_version,
            runtime,
        }
    }

    /// Constructs a V1 mint request without accepting a caller-selected
    /// version.
    #[must_use]
    pub const fn v1(runtime: RuntimeLeaseFence) -> Self {
        Self::new(RealtimeVersion::v1(), runtime)
    }

    #[must_use]
    pub const fn realtime_version(&self) -> RealtimeVersion {
        self.realtime_version
    }

    #[must_use]
    pub const fn runtime(&self) -> &RuntimeLeaseFence {
        &self.runtime
    }

    /// Alias emphasizing that this is the complete lease fence.
    #[must_use]
    pub const fn runtime_lease_fence(&self) -> &RuntimeLeaseFence {
        self.runtime()
    }
}

impl Serialize for MintRealtimeTicketRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.realtime_version != RealtimeVersion::v1() {
            return Err(serde::ser::Error::custom("invalid realtime version"));
        }
        MintRealtimeTicketRequestWire {
            realtime_version: self.realtime_version,
            runtime: &self.runtime,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MintRealtimeTicketRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MintRealtimeTicketRequestWireOwned::deserialize(deserializer)?;
        Ok(Self::new(wire.realtime_version, wire.runtime))
    }
}

/// A server response carrying one ticket and its nonzero expiry.
#[derive(Debug, Eq, PartialEq)]
pub struct MintRealtimeTicketResponse {
    realtime_version: RealtimeVersion,
    runtime: RuntimeLeaseFence,
    ticket: RealtimeTicket,
    expires_at: UnixTimestampMillis,
}

#[derive(Serialize)]
struct MintRealtimeTicketResponseWire<'a> {
    realtime_version: RealtimeVersion,
    runtime: &'a RuntimeLeaseFence,
    ticket: &'a RealtimeTicket,
    expires_at: UnixTimestampMillis,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MintRealtimeTicketResponseWireOwned {
    realtime_version: RealtimeVersion,
    runtime: RuntimeLeaseFence,
    ticket: RealtimeTicket,
    expires_at: UnixTimestampMillis,
}

impl MintRealtimeTicketResponse {
    /// Constructs a response and rejects a zero expiry.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero expiry or a non-V1 version.
    pub fn new(
        realtime_version: RealtimeVersion,
        runtime: RuntimeLeaseFence,
        ticket: RealtimeTicket,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, RealtimeError> {
        if expires_at.value() == 0 {
            return Err(RealtimeError::InvalidExpiry);
        }
        if realtime_version != RealtimeVersion::v1() {
            return Err(RealtimeError::InvalidVersion);
        }
        Ok(Self {
            realtime_version,
            runtime,
            ticket,
            expires_at,
        })
    }

    /// Constructs a V1 response.
    ///
    /// # Errors
    ///
    /// Returns an error when the expiry is zero.
    pub fn v1(
        runtime: RuntimeLeaseFence,
        ticket: RealtimeTicket,
        expires_at: UnixTimestampMillis,
    ) -> Result<Self, RealtimeError> {
        Self::new(RealtimeVersion::v1(), runtime, ticket, expires_at)
    }

    #[must_use]
    pub const fn realtime_version(&self) -> RealtimeVersion {
        self.realtime_version
    }

    #[must_use]
    pub const fn runtime(&self) -> &RuntimeLeaseFence {
        &self.runtime
    }

    #[must_use]
    pub const fn runtime_lease_fence(&self) -> &RuntimeLeaseFence {
        self.runtime()
    }

    #[must_use]
    pub const fn ticket(&self) -> &RealtimeTicket {
        &self.ticket
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixTimestampMillis {
        self.expires_at
    }

    /// Correlates only the exact V1 and revision-independent runtime fence.
    #[must_use]
    pub fn matches_request(&self, request: &MintRealtimeTicketRequest) -> bool {
        self.realtime_version == request.realtime_version
            && self.runtime == request.runtime
            && self.expires_at.value() != 0
    }

    /// Adds the fixed TTL with overflow checking.
    ///
    /// # Errors
    ///
    /// Returns an error when the addition would overflow `u64`.
    pub fn checked_expires_at(
        issued_at: UnixTimestampMillis,
    ) -> Result<UnixTimestampMillis, RealtimeError> {
        issued_at
            .value()
            .checked_add(REALTIME_TICKET_TTL_MS)
            .map(UnixTimestampMillis::new)
            .ok_or(RealtimeError::ExpiryOverflow)
    }

    /// Alias for [`Self::checked_expires_at`].
    ///
    /// # Errors
    ///
    /// Returns an error when the addition would overflow `u64`.
    pub fn expires_at_for(
        issued_at: UnixTimestampMillis,
    ) -> Result<UnixTimestampMillis, RealtimeError> {
        Self::checked_expires_at(issued_at)
    }
}

impl Serialize for MintRealtimeTicketResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.realtime_version != RealtimeVersion::v1() || self.expires_at.value() == 0 {
            return Err(serde::ser::Error::custom("invalid realtime response"));
        }
        MintRealtimeTicketResponseWire {
            realtime_version: self.realtime_version,
            runtime: &self.runtime,
            ticket: &self.ticket,
            expires_at: self.expires_at,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MintRealtimeTicketResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MintRealtimeTicketResponseWireOwned::deserialize(deserializer)?;
        Self::new(
            wire.realtime_version,
            wire.runtime,
            wire.ticket,
            wire.expires_at,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Server readiness parameters and the handle assigned to this client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresenceReadyV1 {
    self_handle: PresenceHandle,
}

#[derive(Serialize)]
struct PresenceReadyWire {
    self_handle: PresenceHandle,
    send_rate_hz: u16,
    interpolation_delay_ms: u32,
    stale_presence_ms: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresenceReadyWireOwned {
    self_handle: PresenceHandle,
    send_rate_hz: u16,
    interpolation_delay_ms: u32,
    stale_presence_ms: u32,
}

impl PresenceReadyV1 {
    #[must_use]
    pub const fn new(self_handle: PresenceHandle) -> Self {
        Self { self_handle }
    }

    #[must_use]
    pub const fn self_handle(self) -> PresenceHandle {
        self.self_handle
    }

    #[must_use]
    pub const fn send_rate_hz(self) -> u16 {
        PRESENCE_SEND_RATE_HZ
    }

    #[must_use]
    pub const fn interpolation_delay_ms(self) -> u32 {
        PRESENCE_INTERPOLATION_DELAY_MS
    }

    #[must_use]
    pub const fn stale_presence_ms(self) -> u32 {
        PRESENCE_STALE_MS
    }
}

impl Serialize for PresenceReadyV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PresenceReadyWire {
            self_handle: self.self_handle,
            send_rate_hz: PRESENCE_SEND_RATE_HZ,
            interpolation_delay_ms: PRESENCE_INTERPOLATION_DELAY_MS,
            stale_presence_ms: PRESENCE_STALE_MS,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PresenceReadyV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PresenceReadyWireOwned::deserialize(deserializer)?;
        if wire.send_rate_hz != PRESENCE_SEND_RATE_HZ
            || wire.interpolation_delay_ms != PRESENCE_INTERPOLATION_DELAY_MS
            || wire.stale_presence_ms != PRESENCE_STALE_MS
        {
            return Err(serde::de::Error::custom(
                "invalid realtime readiness timing",
            ));
        }
        Ok(Self::new(wire.self_handle))
    }
}

/// Client-to-server V1 realtime presence frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientRealtimeFrameV1 {
    PlayerState(LocalPresenceStateV1),
    InteractRemotePlayer(PresenceInteractionV1),
}

#[derive(Serialize)]
struct ClientFrameWire<'a, T> {
    realtime_version: RealtimeVersion,
    #[serde(rename = "type")]
    kind: &'static str,
    payload: &'a T,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ClientFramePayload {
    PlayerState(LocalPresenceStateV1),
    InteractRemotePlayer(PresenceInteractionV1),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientFrameWireOwned {
    realtime_version: RealtimeVersion,
    #[serde(rename = "type", deserialize_with = "deserialize_frame_type")]
    kind: String,
    payload: ClientFramePayload,
}

impl ClientRealtimeFrameV1 {
    #[must_use]
    pub fn player_state(state: LocalPresenceStateV1) -> Self {
        Self::PlayerState(state)
    }

    #[must_use]
    pub fn interact_remote_player(interaction: PresenceInteractionV1) -> Self {
        Self::InteractRemotePlayer(interaction)
    }

    #[must_use]
    pub const fn realtime_version(&self) -> RealtimeVersion {
        RealtimeVersion::v1()
    }
}

impl Serialize for ClientRealtimeFrameV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::PlayerState(state) => ClientFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "PLAYER_STATE",
                payload: state,
            }
            .serialize(serializer),
            Self::InteractRemotePlayer(interaction) => ClientFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "INTERACT_REMOTE_PLAYER",
                payload: interaction,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ClientRealtimeFrameV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ClientFrameWireOwned::deserialize(deserializer)?;
        if wire.realtime_version != RealtimeVersion::v1() {
            return Err(serde::de::Error::custom("invalid realtime version"));
        }
        match (wire.kind.as_str(), wire.payload) {
            ("PLAYER_STATE", ClientFramePayload::PlayerState(state)) => {
                Ok(Self::PlayerState(state))
            }
            ("INTERACT_REMOTE_PLAYER", ClientFramePayload::InteractRemotePlayer(interaction)) => {
                Ok(Self::InteractRemotePlayer(interaction))
            }
            _ => Err(serde::de::Error::custom("invalid client realtime frame")),
        }
    }
}

/// Server-to-client V1 realtime presence frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerRealtimeFrameV1 {
    PresenceReady(PresenceReadyV1),
    RemotePlayerSpawn(RemotePlayerSpawnV1),
    RemotePlayerUpdate(RemotePlayerUpdateV1),
    RemotePlayerDespawn(RemotePlayerDespawnV1),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ServerFramePayload {
    PresenceReady(PresenceReadyV1),
    RemotePlayerSpawn(RemotePlayerSpawnV1),
    RemotePlayerUpdate(RemotePlayerUpdateV1),
    RemotePlayerDespawn(RemotePlayerDespawnV1),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerFrameWireOwned {
    realtime_version: RealtimeVersion,
    #[serde(rename = "type", deserialize_with = "deserialize_frame_type")]
    kind: String,
    payload: ServerFramePayload,
}

impl ServerRealtimeFrameV1 {
    #[must_use]
    pub fn presence_ready(handle: PresenceHandle) -> Self {
        Self::PresenceReady(PresenceReadyV1::new(handle))
    }

    #[must_use]
    pub fn remote_player_spawn(spawn: RemotePlayerSpawnV1) -> Self {
        Self::RemotePlayerSpawn(spawn)
    }

    #[must_use]
    pub fn remote_player_update(update: RemotePlayerUpdateV1) -> Self {
        Self::RemotePlayerUpdate(update)
    }

    #[must_use]
    pub fn remote_player_despawn(despawn: RemotePlayerDespawnV1) -> Self {
        Self::RemotePlayerDespawn(despawn)
    }

    #[must_use]
    pub const fn realtime_version(&self) -> RealtimeVersion {
        RealtimeVersion::v1()
    }
}

impl Serialize for ServerRealtimeFrameV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::PresenceReady(payload) => ServerFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "PRESENCE_READY",
                payload,
            }
            .serialize(serializer),
            Self::RemotePlayerSpawn(payload) => ServerFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "REMOTE_PLAYER_SPAWN",
                payload,
            }
            .serialize(serializer),
            Self::RemotePlayerUpdate(payload) => ServerFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "REMOTE_PLAYER_UPDATE",
                payload,
            }
            .serialize(serializer),
            Self::RemotePlayerDespawn(payload) => ServerFrameWire {
                realtime_version: RealtimeVersion::v1(),
                kind: "REMOTE_PLAYER_DESPAWN",
                payload,
            }
            .serialize(serializer),
        }
    }
}

#[derive(Serialize)]
struct ServerFrameWire<'a, T> {
    realtime_version: RealtimeVersion,
    #[serde(rename = "type")]
    kind: &'static str,
    payload: &'a T,
}

impl<'de> Deserialize<'de> for ServerRealtimeFrameV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ServerFrameWireOwned::deserialize(deserializer)?;
        if wire.realtime_version != RealtimeVersion::v1() {
            return Err(serde::de::Error::custom("invalid realtime version"));
        }
        match (wire.kind.as_str(), wire.payload) {
            ("PRESENCE_READY", ServerFramePayload::PresenceReady(readiness)) => {
                Ok(Self::PresenceReady(readiness))
            }
            ("REMOTE_PLAYER_SPAWN", ServerFramePayload::RemotePlayerSpawn(spawn)) => {
                Ok(Self::RemotePlayerSpawn(spawn))
            }
            ("REMOTE_PLAYER_UPDATE", ServerFramePayload::RemotePlayerUpdate(update)) => {
                Ok(Self::RemotePlayerUpdate(update))
            }
            ("REMOTE_PLAYER_DESPAWN", ServerFramePayload::RemotePlayerDespawn(despawn)) => {
                Ok(Self::RemotePlayerDespawn(despawn))
            }
            _ => Err(serde::de::Error::custom("invalid server realtime frame")),
        }
    }
}

fn deserialize_frame_type<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    // The assembled-message cap is enforced by the directional entry points;
    // this additional bound prevents an unbounded standalone serde string.
    crate::ids::deserialize_bounded_string(deserializer, 64, "realtime frame type")
}

fn decode_json<'a, T>(bytes: &'a [u8], maximum: usize) -> Result<T, RealtimeError>
where
    T: Deserialize<'a>,
{
    if bytes.len() > maximum {
        return Err(RealtimeError::MessageTooLarge);
    }
    str::from_utf8(bytes).map_err(|_| RealtimeError::InvalidUtf8)?;
    serde_json::from_slice(bytes).map_err(|_| RealtimeError::MalformedMessage)
}

fn encode_json<T>(value: &T, maximum: usize) -> Result<Vec<u8>, RealtimeError>
where
    T: Serialize,
{
    let encoded = serde_json::to_vec(value).map_err(|_| RealtimeError::MalformedMessage)?;
    if encoded.len() > maximum {
        return Err(RealtimeError::EncodedMessageTooLarge);
    }
    Ok(encoded)
}

/// Decodes one assembled client-to-server text application message.
///
/// # Errors
///
/// Returns an error for oversized, invalid UTF-8, or malformed messages.
pub fn decode_client_realtime_frame(bytes: &[u8]) -> Result<ClientRealtimeFrameV1, RealtimeError> {
    decode_json(bytes, MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES)
}

/// Encodes one client-to-server text application message.
///
/// # Errors
///
/// Returns an error when serialization fails or exceeds the client cap.
pub fn encode_client_realtime_frame(
    frame: &ClientRealtimeFrameV1,
) -> Result<Vec<u8>, RealtimeError> {
    encode_json(frame, MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES)
}

/// Decodes one assembled server-to-client text application message.
///
/// # Errors
///
/// Returns an error for oversized, invalid UTF-8, or malformed messages.
pub fn decode_server_realtime_frame(bytes: &[u8]) -> Result<ServerRealtimeFrameV1, RealtimeError> {
    decode_json(bytes, MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES)
}

/// Encodes one server-to-client text application message.
///
/// # Errors
///
/// Returns an error when serialization fails or exceeds the server cap.
pub fn encode_server_realtime_frame(
    frame: &ServerRealtimeFrameV1,
) -> Result<Vec<u8>, RealtimeError> {
    encode_json(frame, MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES)
}

/// Short aliases used by adapters that already name the direction.
pub use decode_client_realtime_frame as decode_client_frame;
pub use decode_server_realtime_frame as decode_server_frame;
pub use encode_client_realtime_frame as encode_client_frame;
pub use encode_server_realtime_frame as encode_server_frame;

impl fmt::Display for ClientRealtimeFrameV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PlayerState(_) => "PLAYER_STATE",
            Self::InteractRemotePlayer(_) => "INTERACT_REMOTE_PLAYER",
        })
    }
}

impl fmt::Display for ServerRealtimeFrameV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PresenceReady(_) => "PRESENCE_READY",
            Self::RemotePlayerSpawn(_) => "REMOTE_PLAYER_SPAWN",
            Self::RemotePlayerUpdate(_) => "REMOTE_PLAYER_UPDATE",
            Self::RemotePlayerDespawn(_) => "REMOTE_PLAYER_DESPAWN",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        BridgeAbiVersion, CharacterId, ClientInstanceId, GameBuildId, LeaseFence, MgbaVersion,
        ProtocolVersion, SessionEpoch, SessionId, Sha256Digest,
    };
    use coop_protocol::{
        AnimationId, AvatarId, CanonicalUsername, DespawnReason, Direction, MovementMode,
        PlayerState, PresencePoseV1, RegionId, WorldLocation,
    };
    use serde_json::{Value, json};
    use uuid::Uuid;

    fn id<T>(constructor: fn(Uuid) -> Result<T, crate::IdError>, value: u128) -> T {
        constructor(Uuid::from_u128(value)).unwrap()
    }

    fn runtime() -> RuntimeLeaseFence {
        let save_fence = LeaseFence::new(
            id(SessionId::new, 1),
            id(CharacterId::new, 2),
            crate::Revision::new(3),
            SessionEpoch::new(4).unwrap(),
            id(ClientInstanceId::new, 5),
        );
        RuntimeLeaseFence::new(
            crate::StableRuntimeSession::from_lease_fence(&save_fence),
            crate::RuntimeBuildIdentity::new(
                GameBuildId::new("pokeemerald-coop-0.1.0").unwrap(),
                Sha256Digest::of_bytes(b"rom"),
                MgbaVersion::new("0.10.5").unwrap(),
                BridgeAbiVersion::new(1).unwrap(),
                ProtocolVersion::new(1).unwrap(),
            ),
        )
    }

    fn state() -> LocalPresenceStateV1 {
        let location = WorldLocation::new(RegionId::Hoenn, 1, 0, 4, 5).unwrap();
        let pose = PresencePoseV1::new(
            location,
            0,
            Direction::South,
            3,
            1,
            MovementMode::Idle,
            AnimationId::Idle,
            AvatarId::Brendan,
            PlayerState::Overworld,
        )
        .unwrap();
        LocalPresenceStateV1::new(pose, 1).unwrap()
    }

    fn max_state() -> LocalPresenceStateV1 {
        let location = WorldLocation::new(RegionId::Sevii, 35, 122, i16::MIN, i16::MIN).unwrap();
        let pose = PresencePoseV1::new(
            location,
            u8::MAX,
            Direction::South,
            u32::MAX,
            u32::MAX,
            MovementMode::Idle,
            AnimationId::Locomotion,
            AvatarId::Brendan,
            PlayerState::Overworld,
        )
        .unwrap();
        LocalPresenceStateV1::new(pose, u32::MAX).unwrap()
    }

    fn handle() -> PresenceHandle {
        PresenceHandle::new(0x0123_4567_89ab_cdef).unwrap()
    }

    fn interaction() -> PresenceInteractionV1 {
        PresenceInteractionV1::new(handle(), 2, 3, -1, 1).unwrap()
    }

    fn spawn() -> RemotePlayerSpawnV1 {
        RemotePlayerSpawnV1::new(handle(), 2, state(), CanonicalUsername::new("abc").unwrap())
            .unwrap()
    }

    fn update() -> RemotePlayerUpdateV1 {
        RemotePlayerUpdateV1::new(handle(), 3, state()).unwrap()
    }

    fn despawn() -> RemotePlayerDespawnV1 {
        RemotePlayerDespawnV1::new(handle(), 4, DespawnReason::Stale).unwrap()
    }

    fn client_frames() -> [ClientRealtimeFrameV1; 2] {
        [
            ClientRealtimeFrameV1::player_state(state()),
            ClientRealtimeFrameV1::interact_remote_player(interaction()),
        ]
    }

    fn server_frames() -> [ServerRealtimeFrameV1; 4] {
        [
            ServerRealtimeFrameV1::presence_ready(handle()),
            ServerRealtimeFrameV1::remote_player_spawn(spawn()),
            ServerRealtimeFrameV1::remote_player_update(update()),
            ServerRealtimeFrameV1::remote_player_despawn(despawn()),
        ]
    }

    fn keys(value: &Value) -> BTreeSet<String> {
        value.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn golden_ticket_is_canonical_and_domain_separated() {
        let ticket =
            RealtimeTicket::from_bytes(core::array::from_fn(|index| u8::try_from(index).unwrap()))
                .unwrap();
        assert_eq!(
            ticket.expose_secret(),
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
        );
        assert_eq!(
            ticket.fingerprint().as_hex(),
            "6a2e6257fae4c2cb8ca876142aa61c4ec641f5de9035241c88d8fa9f5a837884"
        );
        assert_eq!(ticket.to_string(), "[REDACTED]");
        assert!(!format!("{ticket:?}").contains(ticket.expose_secret()));
    }

    #[test]
    fn mint_has_exact_fields_and_revision_independent_correlation() {
        let request = MintRealtimeTicketRequest::v1(runtime());
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            value.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["realtime_version", "runtime"]
        );
        let ticket = RealtimeTicket::from_bytes([1; 32]).unwrap();
        let response =
            MintRealtimeTicketResponse::v1(runtime(), ticket, UnixTimestampMillis::new(30_001))
                .unwrap();
        assert!(response.matches_request(&request));
        let mut changed = runtime();
        changed.session.session_epoch = SessionEpoch::new(5).unwrap();
        let other = MintRealtimeTicketRequest::v1(changed);
        assert!(!response.matches_request(&other));
        assert_eq!(
            serde_json::to_value(&response).unwrap()["expires_at"],
            json!(30_001)
        );
    }

    #[test]
    fn frames_are_directional_three_key_envelopes() {
        let client = ClientRealtimeFrameV1::player_state(state());
        let encoded = encode_client_realtime_frame(&client).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&encoded)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            vec!["payload", "realtime_version", "type"]
        );
        assert_eq!(decode_client_realtime_frame(&encoded).unwrap(), client);
        assert!(decode_server_realtime_frame(&encoded).is_err());
        assert!(decode_client_realtime_frame(
            br#"{"realtime_version":1,"type":"PLAYER_STATE","payload":{"pose":{},"source_sequence":1}}"#
        )
        .is_err());
    }

    #[test]
    fn readiness_timing_is_fixed() {
        let ready = ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap());
        let wire = serde_json::to_value(&ready).unwrap();
        assert_eq!(wire["payload"]["send_rate_hz"], json!(10));
        let tampered = serde_json::json!({
            "realtime_version": 1,
            "type": "PRESENCE_READY",
            "payload": {
                "self_handle": "0000000000000001",
                "send_rate_hz": 9,
                "interpolation_delay_ms": 100,
                "stale_presence_ms": 1500
            }
        });
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(tampered).is_err());
    }

    #[test]
    fn bounds_are_enforced_before_and_after_json() {
        assert_eq!(
            decode_client_realtime_frame(&vec![b' '; MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES + 1]),
            Err(RealtimeError::MessageTooLarge)
        );
        assert_eq!(
            decode_client_realtime_frame(&[0xff]),
            Err(RealtimeError::InvalidUtf8)
        );
        assert_eq!(
            decode_client_realtime_frame(&vec![0xff; MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES + 1]),
            Err(RealtimeError::MessageTooLarge)
        );
        assert_eq!(
            decode_server_realtime_frame(&vec![0xff; MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES + 1]),
            Err(RealtimeError::MessageTooLarge)
        );
        assert!(
            MintRealtimeTicketResponse::checked_expires_at(UnixTimestampMillis::new(u64::MAX,))
                .is_err()
        );
        assert_eq!(
            encode_json(&"x".repeat(2), 1),
            Err(RealtimeError::EncodedMessageTooLarge)
        );
    }

    #[test]
    fn ticket_serde_and_all_canonicality_boundaries_are_strict() {
        let golden = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        let ticket = RealtimeTicket::parse(golden).unwrap();
        let encoded = serde_json::to_string(&ticket).unwrap();
        assert_eq!(
            serde_json::from_str::<RealtimeTicket>(&encoded)
                .unwrap()
                .fingerprint(),
            ticket.fingerprint()
        );

        for invalid in [
            "A",
            &"A".repeat(42),
            &"A".repeat(44),
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh=",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh+",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh/",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh ",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh\n",
        ] {
            assert!(
                RealtimeTicket::parse(invalid).is_err(),
                "accepted malformed ticket"
            );
        }
        assert!(RealtimeTicket::parse("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHhé").is_err());
        let all_zero = "A".repeat(REALTIME_TICKET_ENCODED_LEN);
        assert!(RealtimeTicket::parse(&all_zero).is_err());
        assert!(serde_json::from_str::<RealtimeTicket>(&format!("\"{all_zero}\"")).is_err());
        let mut invalid_prefix = golden.as_bytes().to_vec();
        invalid_prefix[0] = b'!';
        assert!(RealtimeTicket::parse(str::from_utf8(&invalid_prefix).unwrap()).is_err());
        for value in [json!(1), json!(null), json!({}), json!([golden])] {
            assert!(serde_json::from_value::<RealtimeTicket>(value).is_err());
        }
        assert!(RealtimeTicket::from_bytes([0; REALTIME_TICKET_ENTROPY_BYTES]).is_err());

        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let prefix = &golden[..REALTIME_TICKET_ENCODED_LEN - 1];
        let allowed = b"AEIMQUYcgkosw048";
        for &byte in alphabet {
            if !allowed.contains(&byte) {
                let mut alias = prefix.to_owned();
                alias.push(char::from(byte));
                assert!(RealtimeTicket::parse(&alias).is_err());
            }
        }
    }

    #[test]
    fn realtime_versions_reject_every_non_v1_wire_class() {
        for wire in [
            "0",
            "2",
            "\"1\"",
            "1.0",
            "-1",
            "65536",
            "18446744073709551615",
        ] {
            assert!(
                serde_json::from_str::<RealtimeVersion>(wire).is_err(),
                "accepted {wire}"
            );
        }
        assert_eq!(
            RealtimeVersion::new(CURRENT_REALTIME_VERSION)
                .unwrap()
                .value(),
            1
        );
        assert!(RealtimeVersion::new(0).is_err());
        assert!(RealtimeVersion::new(2).is_err());
    }

    #[test]
    fn mint_dtos_reject_missing_unknown_nested_and_expiry_variants() {
        let request = MintRealtimeTicketRequest::v1(runtime());
        let request_value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            keys(&request_value),
            BTreeSet::from(["realtime_version".into(), "runtime".into()])
        );

        let mut missing = request_value.clone();
        missing.as_object_mut().unwrap().remove("runtime");
        assert!(serde_json::from_value::<MintRealtimeTicketRequest>(missing).is_err());
        let mut unknown = request_value.clone();
        unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<MintRealtimeTicketRequest>(unknown).is_err());
        for field in ["session", "build"] {
            let mut nested = request_value.clone();
            nested["runtime"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<MintRealtimeTicketRequest>(nested).is_err());
        }
        let mut nested_unknown = request_value.clone();
        nested_unknown["runtime"]["extra"] = json!(true);
        assert!(serde_json::from_value::<MintRealtimeTicketRequest>(nested_unknown).is_err());
        let mut session_unknown = request_value.clone();
        session_unknown["runtime"]["session"]["extra"] = json!(true);
        assert!(serde_json::from_value::<MintRealtimeTicketRequest>(session_unknown).is_err());
        let mut build_unknown = request_value.clone();
        build_unknown["runtime"]["build"]["extra"] = json!(true);
        assert!(serde_json::from_value::<MintRealtimeTicketRequest>(build_unknown).is_err());

        let response = MintRealtimeTicketResponse::v1(
            runtime(),
            RealtimeTicket::from_bytes([1; 32]).unwrap(),
            UnixTimestampMillis::new(30_001),
        )
        .unwrap();
        let response_value = serde_json::to_value(&response).unwrap();
        assert_eq!(
            keys(&response_value),
            BTreeSet::from([
                "expires_at".into(),
                "realtime_version".into(),
                "runtime".into(),
                "ticket".into(),
            ])
        );
        let mut response_missing = response_value.clone();
        response_missing.as_object_mut().unwrap().remove("ticket");
        assert!(serde_json::from_value::<MintRealtimeTicketResponse>(response_missing).is_err());
        let mut response_unknown = response_value.clone();
        response_unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<MintRealtimeTicketResponse>(response_unknown).is_err());
        assert!(
            MintRealtimeTicketResponse::v1(
                runtime(),
                RealtimeTicket::from_bytes([1; 32]).unwrap(),
                UnixTimestampMillis::new(0),
            )
            .is_err()
        );
        assert!(
            MintRealtimeTicketResponse::checked_expires_at(UnixTimestampMillis::new(u64::MAX,))
                .is_err()
        );
        assert_eq!(
            MintRealtimeTicketResponse::checked_expires_at(UnixTimestampMillis::new(1))
                .unwrap()
                .value(),
            30_001
        );
    }

    #[test]
    fn every_frame_variant_has_golden_roundtrip_and_exact_envelope() {
        let client_tags = ["PLAYER_STATE", "INTERACT_REMOTE_PLAYER"];
        for (frame, expected_tag) in client_frames().into_iter().zip(client_tags) {
            let encoded = encode_client_realtime_frame(&frame).unwrap();
            let wire: Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                keys(&wire),
                BTreeSet::from(["payload".into(), "realtime_version".into(), "type".into(),])
            );
            assert_eq!(wire["type"], json!(expected_tag));
            assert_eq!(decode_client_realtime_frame(&encoded).unwrap(), frame);
            assert!(decode_server_realtime_frame(&encoded).is_err());
        }
        let server_tags = [
            "PRESENCE_READY",
            "REMOTE_PLAYER_SPAWN",
            "REMOTE_PLAYER_UPDATE",
            "REMOTE_PLAYER_DESPAWN",
        ];
        for (frame, expected_tag) in server_frames().into_iter().zip(server_tags) {
            let encoded = encode_server_realtime_frame(&frame).unwrap();
            let wire: Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                keys(&wire),
                BTreeSet::from(["payload".into(), "realtime_version".into(), "type".into(),])
            );
            assert_eq!(wire["type"], json!(expected_tag));
            assert_eq!(decode_server_realtime_frame(&encoded).unwrap(), frame);
            assert!(decode_client_realtime_frame(&encoded).is_err());
        }
    }

    #[test]
    fn frame_top_level_and_type_fail_closed() {
        let valid = serde_json::to_value(ServerRealtimeFrameV1::presence_ready(handle())).unwrap();
        for invalid_type in [
            "presence_ready",
            "REMOTE_PLAYER_SPAWN",
            "PLAYER_STATE",
            "NOPE",
        ] {
            let mut value = valid.clone();
            value["type"] = json!(invalid_type);
            assert!(serde_json::from_value::<ServerRealtimeFrameV1>(value).is_err());
        }
        let mut missing_payload = valid.clone();
        missing_payload.as_object_mut().unwrap().remove("payload");
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(missing_payload).is_err());
        let mut extra = valid.clone();
        extra["extra"] = json!(true);
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(extra).is_err());
        let duplicate = br#"{"realtime_version":1,"type":"PRESENCE_READY","type":"PRESENCE_READY","payload":{"self_handle":"0123456789abcdef","send_rate_hz":10,"interpolation_delay_ms":100,"stale_presence_ms":1500}}"#;
        assert!(decode_server_realtime_frame(duplicate).is_err());
        let mut client =
            serde_json::to_value(ClientRealtimeFrameV1::player_state(state())).unwrap();
        client["type"] = json!("PLAYER_STATE");
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(client).is_err());
        let mut server = valid;
        server["type"] = json!("PRESENCE_READY");
        assert!(serde_json::from_value::<ClientRealtimeFrameV1>(server).is_err());
    }

    #[test]
    fn nested_unknown_fields_and_readiness_timing_are_rejected() {
        let ready = serde_json::to_value(ServerRealtimeFrameV1::presence_ready(handle())).unwrap();
        for field in [
            "send_rate_hz",
            "interpolation_delay_ms",
            "stale_presence_ms",
        ] {
            let mut tampered = ready.clone();
            tampered["payload"][field] = json!(0);
            assert!(serde_json::from_value::<ServerRealtimeFrameV1>(tampered).is_err());
        }
        let mut ready_extra = ready.clone();
        ready_extra["payload"]["extra"] = json!(true);
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(ready_extra).is_err());

        let client = serde_json::to_value(ClientRealtimeFrameV1::player_state(state())).unwrap();
        let paths: &[&[&str]] = &[
            &["payload", "extra"],
            &["payload", "pose", "extra"],
            &["payload", "pose", "location", "extra"],
        ];
        for path in paths {
            let mut tampered = client.clone();
            let (last, parents) = path.split_last().unwrap();
            let mut target = &mut tampered;
            for parent in parents {
                target = &mut target[*parent];
            }
            target[*last] = json!(true);
            assert!(serde_json::from_value::<ClientRealtimeFrameV1>(tampered).is_err());
        }

        let mut interaction_extra =
            serde_json::to_value(ClientRealtimeFrameV1::interact_remote_player(interaction()))
                .unwrap();
        interaction_extra["payload"]["extra"] = json!(true);
        assert!(serde_json::from_value::<ClientRealtimeFrameV1>(interaction_extra).is_err());
        let duplicate_state = br#"{"realtime_version":1,"type":"PLAYER_STATE","payload":{"pose":{"location":{"region":"HOENN","map_group":1,"map_number":0,"x":4,"y":5,"extra":true},"elevation":0,"direction":"SOUTH","client_tick":3,"warp_sequence":1,"movement_mode":"IDLE","animation_id":"IDLE","avatar_id":"BRENDAN","player_state":"OVERWORLD"},"source_sequence":1,"source_sequence":1}}"#;
        assert!(decode_client_realtime_frame(duplicate_state).is_err());
        let duplicate_interaction = br#"{"realtime_version":1,"type":"INTERACT_REMOTE_PLAYER","payload":{"handle":"0123456789abcdef","handle":"0123456789abcdef","observed_server_sequence":2,"observed_warp_sequence":3,"x":-1,"y":1}}"#;
        assert!(decode_client_realtime_frame(duplicate_interaction).is_err());

        for frame in [
            ServerRealtimeFrameV1::remote_player_spawn(spawn()),
            ServerRealtimeFrameV1::remote_player_update(update()),
            ServerRealtimeFrameV1::remote_player_despawn(despawn()),
        ] {
            let mut tampered = serde_json::to_value(frame).unwrap();
            tampered["payload"]["extra"] = json!(true);
            assert!(serde_json::from_value::<ServerRealtimeFrameV1>(tampered).is_err());
        }
    }

    #[test]
    fn strong_presence_fields_reject_handles_sequences_and_enums() {
        let mut zero_handle =
            serde_json::to_value(ServerRealtimeFrameV1::presence_ready(handle())).unwrap();
        zero_handle["payload"]["self_handle"] = json!("0000000000000000");
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(zero_handle).is_err());
        let mut uppercase_handle =
            serde_json::to_value(ServerRealtimeFrameV1::presence_ready(handle())).unwrap();
        uppercase_handle["payload"]["self_handle"] = json!("0123456789ABCDEF");
        assert!(serde_json::from_value::<ServerRealtimeFrameV1>(uppercase_handle).is_err());

        let mut zero_sequence =
            serde_json::to_value(ClientRealtimeFrameV1::player_state(state())).unwrap();
        zero_sequence["payload"]["source_sequence"] = json!(0);
        assert!(serde_json::from_value::<ClientRealtimeFrameV1>(zero_sequence).is_err());
        let mut zero_observed =
            serde_json::to_value(ClientRealtimeFrameV1::interact_remote_player(interaction()))
                .unwrap();
        zero_observed["payload"]["observed_server_sequence"] = json!(0);
        assert!(serde_json::from_value::<ClientRealtimeFrameV1>(zero_observed).is_err());
        let mut zero_warp =
            serde_json::to_value(ClientRealtimeFrameV1::interact_remote_player(interaction()))
                .unwrap();
        zero_warp["payload"]["observed_warp_sequence"] = json!(0);
        assert!(serde_json::from_value::<ClientRealtimeFrameV1>(zero_warp).is_err());

        for field in [
            "direction",
            "movement_mode",
            "animation_id",
            "avatar_id",
            "player_state",
        ] {
            let mut invalid =
                serde_json::to_value(ClientRealtimeFrameV1::player_state(state())).unwrap();
            invalid["payload"]["pose"][field] = json!("INVALID");
            assert!(serde_json::from_value::<ClientRealtimeFrameV1>(invalid).is_err());
        }
    }

    #[test]
    fn directional_codecs_enforce_preparse_caps_and_maximum_valid_shapes() {
        let client_at_limit = vec![b'x'; MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES];
        assert_eq!(
            decode_client_realtime_frame(&client_at_limit),
            Err(RealtimeError::MalformedMessage)
        );
        assert_eq!(
            decode_client_realtime_frame(&[b'x'; MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES + 1]),
            Err(RealtimeError::MessageTooLarge)
        );
        let server_at_limit = vec![b'x'; MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES];
        assert_eq!(
            decode_server_realtime_frame(&server_at_limit),
            Err(RealtimeError::MalformedMessage)
        );
        assert_eq!(
            decode_server_realtime_frame(&[b'x'; MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES + 1]),
            Err(RealtimeError::MessageTooLarge)
        );

        let max_client = ClientRealtimeFrameV1::player_state(max_state());
        assert_eq!(MAX_PRESENCE_CLIENT_TEXT_FRAME_BYTES, 1_024);
        assert_eq!(MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES, 2_048);
        assert_eq!(
            encode_client_realtime_frame(&max_client).unwrap().len(),
            366
        );
        let max_spawn = RemotePlayerSpawnV1::new(
            PresenceHandle::new(u64::MAX).unwrap(),
            u32::MAX,
            max_state(),
            CanonicalUsername::new("a".repeat(32)).unwrap(),
        )
        .unwrap();
        let max_server = [
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(u64::MAX).unwrap()),
            ServerRealtimeFrameV1::remote_player_spawn(max_spawn),
            ServerRealtimeFrameV1::remote_player_update(
                RemotePlayerUpdateV1::new(
                    PresenceHandle::new(u64::MAX).unwrap(),
                    u32::MAX,
                    max_state(),
                )
                .unwrap(),
            ),
            ServerRealtimeFrameV1::remote_player_despawn(
                RemotePlayerDespawnV1::new(
                    PresenceHandle::new(u64::MAX).unwrap(),
                    u32::MAX,
                    DespawnReason::PartitionLeft,
                )
                .unwrap(),
            ),
        ];
        assert_eq!(
            encode_server_realtime_frame(&max_server[1]).unwrap().len(),
            486
        );
        for frame in max_server {
            assert!(
                encode_server_realtime_frame(&frame).unwrap().len()
                    <= MAX_PRESENCE_SERVER_TEXT_FRAME_BYTES
            );
        }
    }

    #[test]
    fn gameplay_frames_never_contain_credentials_or_runtime_metadata() {
        let forbidden = [
            "ticket",
            "access_token",
            "session_id",
            "session_epoch",
            "lease",
            "build",
            "current_revision",
            "save_revision",
            "token",
            "protocol_version",
        ];
        for frame in client_frames() {
            let wire = String::from_utf8(encode_client_realtime_frame(&frame).unwrap()).unwrap();
            for field in forbidden {
                assert!(!wire.contains(field), "found forbidden field {field}");
            }
        }
        for frame in server_frames() {
            let wire = String::from_utf8(encode_server_realtime_frame(&frame).unwrap()).unwrap();
            for field in forbidden {
                assert!(!wire.contains(field), "found forbidden field {field}");
            }
        }
    }
}

//! Authenticated, symmetric two-member group travel contracts.

use crate::{
    ApiVersion, CharacterId, ClientInstanceId, IdError, IdempotencyKey, LeaseFence, Revision,
    SessionEpoch, SessionId, UnixTimestampMillis, ids::deserialize_bounded_string,
};
use coop_protocol::{GroupTravelDeparture, GroupTravelEndpoint, RegionId, WorldZone};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// Maximum JSON body accepted by group and invitation endpoints.
pub const GROUP_REQUEST_BODY_MAX_BYTES: usize = 8 * 1024;
/// Invitation lifetime, measured from the server's clock.
pub const GROUP_INVITATION_TTL_MS: u64 = 60_000;
/// Pairing links remain redeemable long enough to share out of band, but are
/// still short-lived bearer capabilities.
pub const GROUP_PAIRING_CODE_TTL_MS: u64 = 10 * 60_000;
pub const GROUP_PAIRING_CODE_LINK_PREFIX: &str = "hoenn-sessions://join/";
/// Maximum route identifier size on the wire.
pub const GROUP_ROUTE_ID_MAX_BYTES: usize = 128;
pub const MAX_WORLD_REVISION: u64 = i64::MAX as u64;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GroupError {
    #[error("invalid API version")]
    InvalidApiVersion,
    #[error("identifier is invalid: {0}")]
    Identifier(#[from] IdError),
    #[error("group must contain two distinct members")]
    InvalidMembers,
    #[error("group members are not canonical")]
    NonCanonicalMembers,
    #[error("route ID is not canonical")]
    InvalidRouteId,
    #[error("world zone is invalid: {0}")]
    InvalidZone(String),
    #[error("world revision is invalid")]
    InvalidWorldRevision,
    #[error("pairing code is invalid")]
    InvalidPairingCode,
}

/// A symmetric UUID-identified group.  The members are always sorted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Group {
    members: [CharacterId; 2],
}

impl Group {
    /// Creates a canonical group without a leader, host, or owner.
    ///
    /// # Errors
    ///
    /// Returns an error when both characters are equal.
    pub fn new(first: CharacterId, second: CharacterId) -> Result<Self, GroupError> {
        if first == second {
            return Err(GroupError::InvalidMembers);
        }
        Ok(Self {
            members: if first < second {
                [first, second]
            } else {
                [second, first]
            },
        })
    }

    #[must_use]
    pub const fn members(self) -> [CharacterId; 2] {
        self.members
    }

    #[must_use]
    pub fn contains(self, character_id: CharacterId) -> bool {
        self.members[0] == character_id || self.members[1] == character_id
    }

    /// # Errors
    ///
    /// Returns an error when the members are equal or not sorted.
    pub fn validate(&self) -> Result<(), GroupError> {
        if self.members[0] == self.members[1] {
            return Err(GroupError::InvalidMembers);
        }
        if self.members[0] > self.members[1] {
            return Err(GroupError::NonCanonicalMembers);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireGroup {
            members: [CharacterId; 2],
        }
        let wire = WireGroup::deserialize(deserializer)?;
        if wire.members[0] >= wire.members[1] {
            return Err(serde::de::Error::custom(GroupError::NonCanonicalMembers));
        }
        Group::new(wire.members[0], wire.members[1]).map_err(serde::de::Error::custom)
    }
}

/// A server-owned route identity.  It is deliberately opaque to clients.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RouteId(String);

impl RouteId {
    /// # Errors
    ///
    /// Returns an error when the route identity is not a bounded canonical
    /// region- or Seagallop-qualified uppercase value.
    pub fn new(value: impl Into<String>) -> Result<Self, GroupError> {
        let value = value.into();
        let mut qualified = value.split(':');
        let region = qualified.next().unwrap_or_default();
        let local_key = qualified.next().unwrap_or_default();
        if value.is_empty()
            || value.len() > GROUP_ROUTE_ID_MAX_BYTES
            || qualified.next().is_some()
            || (region != "SEAGALLOP"
                && RegionId::parse_token(region)
                    .ok()
                    .is_none_or(|region| region == RegionId::Unspecified))
            || local_key.is_empty()
            || !local_key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(GroupError::InvalidRouteId);
        }
        Ok(Self(value))
    }

    /// # Errors
    ///
    /// Returns an error when the route identity is not canonical.
    pub fn parse(value: &str) -> Result<Self, GroupError> {
        Self::new(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for RouteId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RouteId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_bounded_string(deserializer, GROUP_ROUTE_ID_MAX_BYTES, "route ID")?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Create an invitation for exactly one target character.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CreateGroupInvitationRequest {
    pub api_version: ApiVersion,
    pub invitee_character_id: CharacterId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl CreateGroupInvitationRequest {
    #[must_use]
    pub const fn new(
        fence: LeaseFence,
        invitee_character_id: CharacterId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            api_version: ApiVersion::V1,
            invitee_character_id,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key,
        }
    }

    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }

    /// # Errors
    ///
    /// Returns an error when the API version or member identity is invalid.
    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        if self.character_id == self.invitee_character_id {
            return Err(GroupError::InvalidMembers);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for CreateGroupInvitationRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            invitee_character_id: CharacterId,
            session_id: SessionId,
            character_id: CharacterId,
            current_revision: Revision,
            session_epoch: SessionEpoch,
            client_instance_id: ClientInstanceId,
            idempotency_key: IdempotencyKey,
        }
        let wire = Wire::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            invitee_character_id: wire.invitee_character_id,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// Accept an invitation.  The invited character is obtained from the bearer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptGroupInvitationRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl AcceptGroupInvitationRequest {
    #[must_use]
    pub const fn new(fence: LeaseFence, idempotency_key: IdempotencyKey) -> Self {
        Self {
            api_version: ApiVersion::V1,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key,
        }
    }
    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }
    /// # Errors
    ///
    /// Returns an error when the API version is unsupported.
    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for AcceptGroupInvitationRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            session_id: SessionId,
            character_id: CharacterId,
            current_revision: Revision,
            session_epoch: SessionEpoch,
            client_instance_id: ClientInstanceId,
            idempotency_key: IdempotencyKey,
        }
        let wire = Wire::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// Request for a server-catalogued atomic group transfer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupTravelRequest {
    pub api_version: ApiVersion,
    pub route_id: RouteId,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl GroupTravelRequest {
    /// # Errors
    ///
    /// Returns an error when the route identity is not canonical.
    pub fn new(
        fence: LeaseFence,
        route_id: impl Into<String>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, GroupError> {
        Ok(Self {
            api_version: ApiVersion::V1,
            route_id: RouteId::new(route_id)?,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key,
        })
    }
    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }
    #[must_use]
    pub fn route_id(&self) -> &str {
        self.route_id.as_str()
    }
    /// # Errors
    ///
    /// Returns an error when the API version is unsupported.
    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for GroupTravelRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            route_id: RouteId,
            session_id: SessionId,
            character_id: CharacterId,
            current_revision: Revision,
            session_epoch: SessionEpoch,
            client_instance_id: ClientInstanceId,
            idempotency_key: IdempotencyKey,
        }
        let wire = Wire::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            route_id: wire.route_id,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// A server-owned invitation and its exact immutable actors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupInvitationView {
    pub api_version: ApiVersion,
    pub invitation_id: crate::GroupInvitationId,
    pub inviter_character_id: CharacterId,
    pub invitee_character_id: CharacterId,
    pub expires_at: UnixTimestampMillis,
}

/// Public group state returned by inspect, accept, and travel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupMemberView {
    pub character_id: CharacterId,
    pub world_revision: u64,
}

/// A six-symbol pairing code rendered as `ABC-123` on the wire.
///
/// The server only persists an HMAC of this value.  Keeping the type in the
/// shared contract ensures all clients reject malformed codes before sending
/// them to the redeem endpoint.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PairingCode(String);

impl PairingCode {
    pub fn new(value: impl Into<String>) -> Result<Self, GroupError> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[3] != b'-' {
            return Err(GroupError::InvalidPairingCode);
        }
        if !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 3 || PAIRING_CODE_ALPHABET.contains(byte))
        {
            return Err(GroupError::InvalidPairingCode);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const PAIRING_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

impl Serialize for PairingCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PairingCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_bounded_string(deserializer, 7, "pairing code")?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Create a one-use remote pairing code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePairingCodeRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl CreatePairingCodeRequest {
    #[must_use]
    pub const fn new(fence: LeaseFence) -> Self {
        Self {
            api_version: ApiVersion::V1,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
        }
    }

    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }

    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        Ok(())
    }
}

/// Redeem a pairing code while bound to the redeeming player's lease.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedeemPairingCodeRequest {
    pub api_version: ApiVersion,
    pub code: PairingCode,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl RedeemPairingCodeRequest {
    #[must_use]
    pub fn new(fence: LeaseFence, code: PairingCode) -> Self {
        Self {
            api_version: ApiVersion::V1,
            code,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
        }
    }

    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }

    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        Ok(())
    }
}

/// The caller's current or most recent group partner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartnerStatus {
    pub username: crate::Username,
    pub online: bool,
    /// Time of the last successful session heartbeat, when one was observed.
    pub last_seen_at: Option<UnixTimestampMillis>,
    /// The location in the partner's last committed character state.
    pub world_zone: WorldZone,
    /// The partner's current authenticated presence location, when the
    /// partner has a fresh active presence connection. This is ephemeral and
    /// must never be treated as save or progression authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_world_zone: Option<WorldZone>,
    pub badge_count: u8,
    pub group_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartnerStatusResponse {
    pub api_version: ApiVersion,
    pub partner: Option<PartnerStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupView {
    pub api_version: ApiVersion,
    pub group_id: crate::GroupId,
    pub members: [GroupMemberView; 2],
    /// Each member's current location.  This is populated for remote pairing
    /// and lets clients keep both players in place until consent travel.
    pub member_world_zones: [WorldZone; 2],
    /// Legacy shared-zone field retained for old clients. It is the first
    /// canonical member's zone in durable group views; authenticated ONLINE
    /// snapshots may override it with the caller's partner live zone.
    pub world_zone: WorldZone,
}

impl GroupView {
    /// # Errors
    ///
    /// Returns an error when group, zone, or world revision invariants fail.
    pub fn new(
        group_id: crate::GroupId,
        group: Group,
        zone: WorldZone,
        revisions: [u64; 2],
    ) -> Result<Self, GroupError> {
        Self::new_with_member_zones(group_id, group, [zone.clone(), zone], revisions)
    }

    /// Builds a group view when members are allowed to remain on different
    /// maps, as with a redeemed pairing code.
    pub fn new_with_member_zones(
        group_id: crate::GroupId,
        group: Group,
        member_world_zones: [WorldZone; 2],
        revisions: [u64; 2],
    ) -> Result<Self, GroupError> {
        group.validate().map_err(|_| GroupError::InvalidMembers)?;
        for zone in &member_world_zones {
            zone.validate()
                .map_err(|error| GroupError::InvalidZone(error.to_string()))?;
        }
        if revisions
            .iter()
            .any(|revision| *revision > MAX_WORLD_REVISION)
        {
            return Err(GroupError::InvalidWorldRevision);
        }
        let ids = group.members();
        Ok(Self {
            api_version: ApiVersion::V1,
            group_id,
            members: [
                GroupMemberView {
                    character_id: ids[0],
                    world_revision: revisions[0],
                },
                GroupMemberView {
                    character_id: ids[1],
                    world_revision: revisions[1],
                },
            ],
            member_world_zones: member_world_zones.clone(),
            world_zone: member_world_zones[0].clone(),
        })
    }
}

impl<'de> Deserialize<'de> for GroupView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            group_id: crate::GroupId,
            members: [GroupMemberView; 2],
            #[serde(default)]
            member_world_zones: Option<[WorldZone; 2]>,
            world_zone: WireWorldZone,
        }
        let wire = Wire::deserialize(deserializer)?;
        if wire.api_version.value() != 1
            || wire.members[0].character_id >= wire.members[1].character_id
        {
            return Err(serde::de::Error::custom(GroupError::NonCanonicalMembers));
        }
        if wire
            .members
            .iter()
            .any(|member| member.world_revision > MAX_WORLD_REVISION)
        {
            return Err(serde::de::Error::custom(GroupError::InvalidWorldRevision));
        }
        let world_zone = WorldZone::new(
            wire.world_zone.region,
            wire.world_zone.map,
            wire.world_zone.channel,
        )
        .map_err(|error| serde::de::Error::custom(GroupError::InvalidZone(error.to_string())))?;
        let member_world_zones = wire
            .member_world_zones
            .unwrap_or_else(|| [world_zone.clone(), world_zone.clone()]);
        for zone in &member_world_zones {
            zone.validate().map_err(|error| {
                serde::de::Error::custom(GroupError::InvalidZone(error.to_string()))
            })?;
        }
        Ok(Self {
            api_version: wire.api_version,
            group_id: wire.group_id,
            members: wire.members,
            member_world_zones,
            world_zone,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWorldZone {
    region: RegionId,
    #[serde(deserialize_with = "deserialize_group_world_zone_map")]
    map: String,
    channel: u16,
}

fn deserialize_group_world_zone_map<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string(deserializer, 128, "group world-zone map")
}

/// Response returned by invitation creation.
pub type CreateGroupInvitationResponse = GroupInvitationView;
/// Response returned when a new pairing code is issued.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePairingCodeResponse {
    pub api_version: ApiVersion,
    pub code: PairingCode,
    pub join_link: String,
    pub expires_at: UnixTimestampMillis,
}

/// Response returned after a code forms a remote group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedeemPairingCodeResponse {
    pub api_version: ApiVersion,
    pub group: GroupView,
}
/// Response returned by invitation acceptance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptGroupInvitationResponse {
    pub api_version: ApiVersion,
    pub group: GroupView,
}
/// Response returned by atomic travel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelResponse {
    pub api_version: ApiVersion,
    pub group: GroupView,
}

/// Creates a consent-gated two-player travel proposal for an exact group state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupTravelProposalRequest {
    pub api_version: ApiVersion,
    pub route_id: RouteId,
    pub departure: GroupTravelDeparture,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
    /// Dynamic Dig/Escape Rope endpoints are omitted for legacy static routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<GroupTravelEndpoint>,
}

impl GroupTravelProposalRequest {
    /// # Errors
    ///
    /// Returns an error for a malformed route or non-canonical expected members.
    pub fn new(
        fence: LeaseFence,
        route_id: impl Into<String>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, GroupError> {
        let route_id = RouteId::new(route_id)?;
        let departure = if route_id.as_str().ends_with("_TRAIN") {
            GroupTravelDeparture::Train
        } else if route_id.as_str().ends_with("_FERRY") {
            GroupTravelDeparture::Ferry
        } else if route_id.as_str().ends_with("_ROUTE22") {
            GroupTravelDeparture::Gate
        } else if route_id.as_str() == "HOENN:FLY_LITTLEROOT" {
            GroupTravelDeparture::Fly
        } else if route_id.as_str() == "HOENN:DIG" {
            GroupTravelDeparture::Dig
        } else if route_id.as_str() == "HOENN:ESCAPE_ROPE" {
            GroupTravelDeparture::EscapeRope
        } else if route_id.as_str().ends_with("_CABLE_CAR") {
            GroupTravelDeparture::CableCar
        } else {
            return Err(GroupError::InvalidRouteId);
        };
        Self::new_with_departure(fence, route_id.as_str(), departure, idempotency_key)
    }

    /// # Errors
    ///
    /// Returns an error when `route_id` is not a valid canonical route identifier.
    pub fn new_with_departure(
        fence: LeaseFence,
        route_id: impl Into<String>,
        departure: GroupTravelDeparture,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, GroupError> {
        Self::new_with_departure_and_endpoint(fence, route_id, departure, None, idempotency_key)
    }

    /// Creates a proposal request with an optional dynamic endpoint.
    ///
    /// The endpoint is required for Dig and Escape Rope and must be omitted
    /// for all legacy static routes.
    pub fn new_with_departure_and_endpoint(
        fence: LeaseFence,
        route_id: impl Into<String>,
        departure: GroupTravelDeparture,
        endpoint: Option<GroupTravelEndpoint>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, GroupError> {
        let request = Self {
            api_version: ApiVersion::V1,
            route_id: RouteId::new(route_id)?,
            departure,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key,
            endpoint,
        };
        request.validate()?;
        Ok(request)
    }

    /// Alias retained for callers that already have an explicit endpoint.
    pub fn new_with_endpoint(
        fence: LeaseFence,
        route_id: impl Into<String>,
        departure: GroupTravelDeparture,
        endpoint: Option<GroupTravelEndpoint>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, GroupError> {
        Self::new_with_departure_and_endpoint(fence, route_id, departure, endpoint, idempotency_key)
    }

    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }

    /// # Errors
    ///
    /// Returns an error for an unsupported version or non-canonical revisions.
    pub fn validate(&self) -> Result<(), GroupError> {
        if self.api_version.value() != 1 {
            return Err(GroupError::InvalidApiVersion);
        }
        let dynamic = matches!(self.route_id.as_str(), "HOENN:DIG" | "HOENN:ESCAPE_ROPE");
        if dynamic != self.endpoint.is_some() {
            return Err(GroupError::InvalidRouteId);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for GroupTravelProposalRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            route_id: RouteId,
            departure: GroupTravelDeparture,
            session_id: SessionId,
            character_id: CharacterId,
            current_revision: Revision,
            session_epoch: SessionEpoch,
            client_instance_id: ClientInstanceId,
            idempotency_key: IdempotencyKey,
            #[serde(default)]
            endpoint: Option<GroupTravelEndpoint>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let request = Self {
            api_version: wire.api_version,
            route_id: wire.route_id,
            departure: wire.departure,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
            endpoint: wire.endpoint,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// A role-checked action on an existing travel proposal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupTravelAction {
    Accept,
    Decline,
    Cancel,
    Applied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct GroupTravelActionRequest {
    pub api_version: ApiVersion,
    pub action: GroupTravelAction,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl GroupTravelActionRequest {
    #[must_use]
    pub const fn new(
        fence: LeaseFence,
        action: GroupTravelAction,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            api_version: ApiVersion::V1,
            action,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            idempotency_key,
        }
    }

    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
}

impl<'de> Deserialize<'de> for GroupTravelActionRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            action: GroupTravelAction,
            session_id: SessionId,
            character_id: CharacterId,
            current_revision: Revision,
            session_epoch: SessionEpoch,
            client_instance_id: ClientInstanceId,
            idempotency_key: IdempotencyKey,
        }
        let wire = Wire::deserialize(deserializer)?;
        if wire.api_version.value() != 1 {
            return Err(serde::de::Error::custom(GroupError::InvalidApiVersion));
        }
        Ok(Self {
            api_version: wire.api_version,
            action: wire.action,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            client_instance_id: wire.client_instance_id,
            idempotency_key: wire.idempotency_key,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupTravelProposalStatus {
    Pending,
    AwaitingSceneReceipts,
    Suspended,
    Committed,
    Declined,
    Cancelled,
    Expired,
}

/// The caller's own unfinished first-Briney scene marker, discoverable after
/// its group and live proposal indexes have been closed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoryTravelRecoveryView {
    pub api_version: ApiVersion,
    pub proposal_id: crate::GroupTravelProposalId,
    pub group_id: crate::GroupId,
    pub status: GroupTravelProposalStatus,
    pub marker_fence: LeaseFence,
    pub scene_nonce: u32,
    pub marked_at: UnixTimestampMillis,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StoryTravelRecoveryAction {
    Reconcile,
    Abandon,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoryTravelRecoveryActionRequest {
    pub api_version: ApiVersion,
    pub action: StoryTravelRecoveryAction,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StoryTravelRecoveryOutcome {
    Reconciled,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoryTravelRecoveryResolutionView {
    pub api_version: ApiVersion,
    pub proposal_id: crate::GroupTravelProposalId,
    pub outcome: StoryTravelRecoveryOutcome,
}

/// Immutable delivery payload created by the atomic accept transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelCommit {
    pub group_zone_revision: u64,
    pub members: [GroupMemberView; 2],
    pub destination: WorldZone,
    /// Dynamic endpoint retained verbatim for replay and delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<GroupTravelEndpoint>,
}

/// Persistent proposal state delivered to either group member.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelProposalView {
    pub api_version: ApiVersion,
    pub proposal_id: crate::GroupTravelProposalId,
    pub group_id: crate::GroupId,
    pub requester_character_id: CharacterId,
    pub responder_character_id: CharacterId,
    pub route_id: RouteId,
    pub departure: GroupTravelDeparture,
    pub source: WorldZone,
    pub destination: WorldZone,
    /// Dynamic endpoint retained verbatim for replay and delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<GroupTravelEndpoint>,
    pub expected_group_zone_revision: u64,
    pub expected_members: [GroupMemberView; 2],
    pub status: GroupTravelProposalStatus,
    pub expires_at: UnixTimestampMillis,
    /// Server clock sampled when this response was produced. This is response
    /// metadata and is never persisted with the proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_now: Option<UnixTimestampMillis>,
    pub commit: Option<GroupTravelCommit>,
    pub applied_by: [bool; 2],
    /// Story routes advance only after each participant proves a completed
    /// scene and a distinct finalized post-scene save.
    #[serde(default)]
    pub scene_marked_by: [bool; 2],
    #[serde(default)]
    pub scene_receipted_by: [bool; 2],
}

/// A ROM-originated completion marker for one consented story scene.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelSceneMarkerRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub scene_nonce: u32,
}

impl GroupTravelSceneMarkerRequest {
    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
}

/// Names the exact finalized save containing the post-scene evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelSceneReceiptRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub snapshot_id: crate::SnapshotId,
}

impl GroupTravelSceneReceiptRequest {
    #[must_use]
    pub const fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn id<T>(constructor: fn(Uuid) -> Result<T, IdError>, value: u128) -> T {
        constructor(Uuid::from_u128(value)).expect("non-nil test UUID")
    }

    fn sample_view() -> GroupView {
        let first = id(CharacterId::new, 1);
        let second = id(CharacterId::new, 2);
        GroupView::new(
            id(crate::GroupId::new, 3),
            Group::new(first, second).expect("distinct members"),
            WorldZone::new(RegionId::Hoenn, "ROUTE101", 1).expect("catalogued map"),
            [4, 3],
        )
        .expect("valid group view")
    }

    fn sample_fence() -> LeaseFence {
        LeaseFence::new(
            id(SessionId::new, 10),
            id(CharacterId::new, 11),
            Revision::new(3),
            SessionEpoch::new(1).expect("epoch"),
            id(ClientInstanceId::new, 12),
        )
    }

    #[test]
    fn group_view_zone_wire_is_strict_bounded_and_round_trips() {
        let original = sample_view();
        let value = serde_json::to_value(&original).expect("serialize group view");
        let decoded: GroupView = serde_json::from_value(value.clone()).expect("round trip");
        assert_eq!(decoded, original);

        let mut unknown_zone_field = value.clone();
        unknown_zone_field["world_zone"]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<GroupView>(unknown_zone_field).is_err());

        let mut oversized_map = value;
        oversized_map["world_zone"]["map"] = json!("A".repeat(129));
        assert!(serde_json::from_value::<GroupView>(oversized_map).is_err());
    }

    #[test]
    fn proposal_requests_and_actions_are_strict_and_use_named_actions() {
        let proposal = GroupTravelProposalRequest::new_with_departure(
            sample_fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Train,
            id(IdempotencyKey::new, 13),
        )
        .expect("proposal");
        let mut value = serde_json::to_value(&proposal).expect("proposal json");
        assert_eq!(
            serde_json::from_value::<GroupTravelProposalRequest>(value.clone())
                .expect("proposal round trip"),
            proposal
        );
        value["unexpected"] = json!(true);
        assert!(serde_json::from_value::<GroupTravelProposalRequest>(value).is_err());

        let action = GroupTravelActionRequest::new(
            sample_fence(),
            GroupTravelAction::Applied,
            id(IdempotencyKey::new, 14),
        );
        let mut value = serde_json::to_value(action).expect("action json");
        assert_eq!(value["action"], "APPLIED");
        assert!(serde_json::from_value::<GroupTravelActionRequest>(value.clone()).is_ok());
        value["action"] = json!("applied");
        assert!(serde_json::from_value::<GroupTravelActionRequest>(value).is_err());
    }

    #[test]
    fn dynamic_proposal_endpoint_is_optional_only_for_legacy_routes() {
        let static_proposal = GroupTravelProposalRequest::new_with_departure(
            sample_fence(),
            "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
            GroupTravelDeparture::Train,
            id(IdempotencyKey::new, 15),
        )
        .expect("static proposal");
        let static_json = serde_json::to_value(&static_proposal).expect("static json");
        assert!(static_json.get("endpoint").is_none());

        let endpoint = GroupTravelEndpoint::new(0, 9, 0, 10, 4, 6);
        let dynamic = GroupTravelProposalRequest::new_with_endpoint(
            sample_fence(),
            "HOENN:DIG",
            GroupTravelDeparture::Dig,
            Some(endpoint),
            id(IdempotencyKey::new, 16),
        )
        .expect("dynamic proposal");
        let dynamic_json = serde_json::to_value(&dynamic).expect("dynamic json");
        assert_eq!(dynamic_json["endpoint"]["source_map_group"], 0);
        assert_eq!(
            serde_json::from_value::<GroupTravelProposalRequest>(dynamic_json).unwrap(),
            dynamic
        );
    }

    #[test]
    fn seagallop_route_ids_preserve_the_existing_ferry_catalog() {
        let route = RouteId::new("SEAGALLOP:VERMILION_ONE_FERRY").expect("ferry route");
        assert_eq!(route.as_str(), "SEAGALLOP:VERMILION_ONE_FERRY");
        assert!(RouteId::new("SEAGALLOP:vermilion_one_ferry").is_err());
        assert!(RouteId::new("OTHER:VERMILION_ONE_FERRY").is_err());
    }

    #[test]
    fn pairing_codes_are_fixed_format_and_group_views_keep_member_zones() {
        assert!(PairingCode::new("HX7-4QK").is_ok());
        assert!(PairingCode::new("hx7-4qk").is_err());
        assert!(PairingCode::new("HX7-4QKI").is_err());

        let first = WorldZone::new(RegionId::Hoenn, "ROUTE101", 1).expect("first map");
        let second = WorldZone::new(RegionId::Hoenn, "SLATEPORT_CITY", 1).expect("second map");
        let view = GroupView::new_with_member_zones(
            id(crate::GroupId::new, 4),
            Group::new(id(CharacterId::new, 5), id(CharacterId::new, 6)).expect("group"),
            [first.clone(), second.clone()],
            [1, 2],
        )
        .expect("view");
        let encoded = serde_json::to_value(&view).expect("view json");
        assert_eq!(encoded["member_world_zones"][0]["map"], "ROUTE101");
        assert_eq!(encoded["member_world_zones"][1]["map"], "SLATEPORT_CITY");
        let decoded: GroupView = serde_json::from_value(encoded).expect("view round trip");
        assert_eq!(decoded.member_world_zones, [first, second]);
    }
}

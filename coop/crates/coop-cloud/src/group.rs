//! Authenticated, symmetric two-member group travel contracts.

use crate::{
    ApiVersion, CharacterId, ClientInstanceId, IdError, IdempotencyKey, LeaseFence, Revision,
    SessionEpoch, SessionId, UnixTimestampMillis, ids::deserialize_bounded_string,
};
use coop_protocol::{RegionId, WorldZone};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// Maximum JSON body accepted by group and invitation endpoints.
pub const GROUP_REQUEST_BODY_MAX_BYTES: usize = 8 * 1024;
/// Invitation lifetime, measured from the server's clock.
pub const GROUP_INVITATION_TTL_MS: u64 = 30_000;
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
    /// region-qualified uppercase value.
    pub fn new(value: impl Into<String>) -> Result<Self, GroupError> {
        let value = value.into();
        let mut qualified = value.split(':');
        let region = qualified.next().unwrap_or_default();
        let local_key = qualified.next().unwrap_or_default();
        if value.is_empty()
            || value.len() > GROUP_ROUTE_ID_MAX_BYTES
            || qualified.next().is_some()
            || RegionId::parse_token(region)
                .ok()
                .is_none_or(|region| region == RegionId::Unspecified)
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupView {
    pub api_version: ApiVersion,
    pub group_id: crate::GroupId,
    pub members: [GroupMemberView; 2],
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
        group.validate().map_err(|_| GroupError::InvalidMembers)?;
        zone.validate()
            .map_err(|error| GroupError::InvalidZone(error.to_string()))?;
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
            world_zone: zone,
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
        Ok(Self {
            api_version: wire.api_version,
            group_id: wire.group_id,
            members: wire.members,
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
}

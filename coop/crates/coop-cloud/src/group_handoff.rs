//! Canonical, release-pinned source identity for a paired cross-ROM handoff.
//! This is a server-owned intent, not an independently authorized client route.

use coop_protocol::{RomWorldId, WorldZone};
use hmac::Mac;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{
    group::MAX_WORLD_REVISION, ApiVersion, GroupId, IdempotencyKey, LeaseFence,
    RuntimeBuildIdentity, Sha256Digest, SnapshotId, SnapshotRecord,
};

/// One independently authenticated member's consent to one portal handoff.
/// Both members submit the same group and portal; the server pins the group
/// revision and issues one attempt key for their rendezvous.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRomHandoffJoinRequest {
    pub api_version: ApiVersion,
    pub group_id: GroupId,
    pub fence: LeaseFence,
    pub source_snapshot_id: SnapshotId,
    pub portal_id: String,
    /// Client-generated durable intent identity. The launcher persists this
    /// before sending the join so a retry after a lost response can replay
    /// the same proposal or terminal receipt. A new key deliberately starts
    /// a new attempt after the bounded recovery window expires.
    pub client_intent_key: IdempotencyKey,
}

impl GroupRomHandoffJoinRequest {
    #[must_use]
    pub fn valid_portal_id(&self) -> bool {
        valid_portal_id(&self.portal_id)
    }
}

/// Lease-fenced status and recovery query for one exact handoff key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRomHandoffStatusRequest {
    pub api_version: ApiVersion,
    pub group_id: GroupId,
    pub fence: LeaseFence,
    pub idempotency_key: IdempotencyKey,
}

/// The destination ROM's authenticated report that it loaded its own exact
/// staged save. `acknowledgment_mac` binds the payload to the server-issued
/// per-member challenge; it does not attest execution on physical hardware.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRomHandoffArrivalRequest {
    pub api_version: ApiVersion,
    pub group_id: GroupId,
    pub fence: LeaseFence,
    pub idempotency_key: IdempotencyKey,
    pub stage_id: SnapshotId,
    pub destination_save_sha256: Sha256Digest,
    pub destination_build: RuntimeBuildIdentity,
    pub acknowledgment_mac: Sha256Digest,
}

impl GroupRomHandoffArrivalRequest {
    /// HMAC-SHA256 over a fixed, unambiguous v1 arrival transcript. The
    /// launcher must call this only after its ROM/nonce arrival verifier passes.
    ///
    /// # Panics
    ///
    /// Panics only if a validated runtime identity exceeds the transcript's
    /// two-byte length prefix, which is excluded by the identity validators.
    #[must_use]
    pub fn expected_mac(&self, challenge: Sha256Digest) -> Sha256Digest {
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(challenge.as_bytes())
            .expect("HMAC accepts a 32-byte challenge");
        mac.update(b"coop.group-rom-arrival.v1\0");
        mac.update(self.group_id.as_uuid().as_bytes());
        mac.update(self.fence.character_id.as_uuid().as_bytes());
        mac.update(self.fence.session_id.as_uuid().as_bytes());
        mac.update(&self.fence.session_epoch.value().to_be_bytes());
        mac.update(self.fence.client_instance_id.as_uuid().as_bytes());
        mac.update(&self.fence.current_revision.value().to_be_bytes());
        mac.update(self.idempotency_key.as_uuid().as_bytes());
        mac.update(self.stage_id.as_uuid().as_bytes());
        mac.update(self.destination_save_sha256.as_bytes());
        let build = self.destination_build.game_build_id.value().as_bytes();
        mac.update(
            &u16::try_from(build.len())
                .expect("validated game build identity fits transcript length")
                .to_be_bytes(),
        );
        mac.update(build);
        mac.update(self.destination_build.rom_sha256.as_bytes());
        let mgba = self.destination_build.mgba_version.as_str().as_bytes();
        mac.update(
            &u16::try_from(mgba.len())
                .expect("validated mGBA identity fits transcript length")
                .to_be_bytes(),
        );
        mac.update(mgba);
        mac.update(&self.destination_build.bridge_abi.value().to_be_bytes());
        mac.update(
            &self
                .destination_build
                .protocol_version
                .value()
                .to_be_bytes(),
        );
        Sha256Digest::from_bytes(mac.finalize().into_bytes().into())
    }
}

/// Either member may abort before commit. A committed result can only replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRomHandoffAbortRequest {
    pub api_version: ApiVersion,
    pub group_id: GroupId,
    pub fence: LeaseFence,
    pub idempotency_key: IdempotencyKey,
}

/// Each member sees only its own destination save and challenge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum GroupRomHandoffStatus {
    Pending {
        group_id: GroupId,
        idempotency_key: IdempotencyKey,
        portal_id: String,
        submitted_by: [bool; 2],
    },
    Staged {
        group_id: GroupId,
        idempotency_key: IdempotencyKey,
        stage_id: SnapshotId,
        destination_world_id: RomWorldId,
        arrival_portal_id: String,
        destination_save_sha256: Sha256Digest,
        destination_save: Vec<u8>,
        arrival_challenge: Sha256Digest,
        acknowledged_by: [bool; 2],
    },
    Committed {
        group_id: GroupId,
        idempotency_key: IdempotencyKey,
        destination_zone: WorldZone,
        group_zone_revision: u64,
        own_snapshot: SnapshotRecord,
    },
    Aborted {
        group_id: GroupId,
        idempotency_key: IdempotencyKey,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRomHandoffSource {
    pub fence: LeaseFence,
    pub source_snapshot_id: SnapshotId,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GroupRomHandoffError {
    #[error("invalid API version")]
    ApiVersion,
    #[error("members must be distinct and in canonical order")]
    Members,
    #[error("a finalized source revision is required")]
    SourceRevision,
    #[error("group zone revision is out of range")]
    GroupRevision,
    #[error("source and destination ROM worlds must differ")]
    Worlds,
    #[error("portal identity is invalid")]
    Portal,
}

/// Both members' exact finalized source heads and active lease fences, resolved
/// against one trusted catalog, one transfer descriptor, and one portal.
/// Member order is always ascending by character ID. An intent is immutable
/// after the server has persisted it; later requests must replay it exactly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupRomHandoffIntent {
    pub api_version: ApiVersion,
    pub group_id: GroupId,
    pub group_zone_revision: u64,
    pub source_zone: WorldZone,
    pub source_world_id: RomWorldId,
    pub destination_world_id: RomWorldId,
    pub portal_id: String,
    pub arrival_portal_id: String,
    pub catalog_sha256: Sha256Digest,
    pub descriptor_sha256: Sha256Digest,
    pub arrival_template_sha256: Sha256Digest,
    pub members: [GroupRomHandoffSource; 2],
    pub idempotency_key: IdempotencyKey,
}

impl GroupRomHandoffIntent {
    /// Validate structural invariants before checking live leases, snapshots,
    /// group membership, portal mapping, and catalog hashes on the server.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported version, invalid source or group
    /// revision, noncanonical member order, world pair, or portal identity.
    pub fn validate(&self) -> Result<(), GroupRomHandoffError> {
        if self.api_version != ApiVersion::V1 {
            return Err(GroupRomHandoffError::ApiVersion);
        }
        if self.group_zone_revision >= MAX_WORLD_REVISION {
            return Err(GroupRomHandoffError::GroupRevision);
        }
        if self.source_world_id == self.destination_world_id {
            return Err(GroupRomHandoffError::Worlds);
        }
        if !valid_portal_id(&self.portal_id) || !valid_portal_id(&self.arrival_portal_id) {
            return Err(GroupRomHandoffError::Portal);
        }
        if self.members[0].fence.character_id >= self.members[1].fence.character_id {
            return Err(GroupRomHandoffError::Members);
        }
        if self
            .members
            .iter()
            .any(|member| member.fence.current_revision.value() == 0)
        {
            return Err(GroupRomHandoffError::SourceRevision);
        }
        Ok(())
    }
}

fn valid_portal_id(value: &str) -> bool {
    value.len() <= 96
        && value
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

impl<'de> Deserialize<'de> for GroupRomHandoffIntent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            api_version: ApiVersion,
            group_id: GroupId,
            group_zone_revision: u64,
            source_zone: WorldZone,
            source_world_id: RomWorldId,
            destination_world_id: RomWorldId,
            portal_id: String,
            arrival_portal_id: String,
            catalog_sha256: Sha256Digest,
            descriptor_sha256: Sha256Digest,
            arrival_template_sha256: Sha256Digest,
            members: [GroupRomHandoffSource; 2],
            idempotency_key: IdempotencyKey,
        }
        let wire = Wire::deserialize(deserializer)?;
        let intent = Self {
            api_version: wire.api_version,
            group_id: wire.group_id,
            group_zone_revision: wire.group_zone_revision,
            source_zone: wire.source_zone,
            source_world_id: wire.source_world_id,
            destination_world_id: wire.destination_world_id,
            portal_id: wire.portal_id,
            arrival_portal_id: wire.arrival_portal_id,
            catalog_sha256: wire.catalog_sha256,
            descriptor_sha256: wire.descriptor_sha256,
            arrival_template_sha256: wire.arrival_template_sha256,
            members: wire.members,
            idempotency_key: wire.idempotency_key,
        };
        intent.validate().map_err(serde::de::Error::custom)?;
        Ok(intent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharacterId, ClientInstanceId, Revision, SessionEpoch, SessionId};
    use coop_protocol::RegionId;
    use uuid::Uuid;

    fn source(number: u128) -> GroupRomHandoffSource {
        GroupRomHandoffSource {
            fence: LeaseFence::new(
                SessionId::new(Uuid::from_u128(number + 10)).unwrap(),
                CharacterId::new(Uuid::from_u128(number)).unwrap(),
                Revision::new(1),
                SessionEpoch::new(1).unwrap(),
                ClientInstanceId::new(Uuid::from_u128(number + 20)).unwrap(),
            ),
            source_snapshot_id: SnapshotId::new(Uuid::from_u128(number + 30)).unwrap(),
        }
    }

    fn intent() -> GroupRomHandoffIntent {
        GroupRomHandoffIntent {
            api_version: ApiVersion::V1,
            group_id: GroupId::new(Uuid::from_u128(100)).unwrap(),
            group_zone_revision: 3,
            source_zone: WorldZone::new(RegionId::Hoenn, "LILYCOVE_CITY_HARBOR", 1).unwrap(),
            source_world_id: RomWorldId::new(1).unwrap(),
            destination_world_id: RomWorldId::new(2).unwrap(),
            portal_id: "to_cormoria".into(),
            arrival_portal_id: "rivetshore_harbor".into(),
            catalog_sha256: Sha256Digest::of_bytes(b"catalog"),
            descriptor_sha256: Sha256Digest::of_bytes(b"descriptor"),
            arrival_template_sha256: Sha256Digest::of_bytes(b"arrival"),
            members: [source(1), source(2)],
            idempotency_key: IdempotencyKey::new(Uuid::from_u128(101)).unwrap(),
        }
    }

    #[test]
    fn intent_json_round_trip_and_strict_schema() {
        let original = intent();
        let mut value = serde_json::to_value(&original).unwrap();
        assert_eq!(
            serde_json::from_value::<GroupRomHandoffIntent>(value.clone()).unwrap(),
            original
        );
        value["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<GroupRomHandoffIntent>(value).is_err());
    }

    #[test]
    fn intent_rejects_noncanonical_or_unbounded_identity() {
        let original = intent();
        for mutate in [
            |value: &mut serde_json::Value| value["members"].as_array_mut().unwrap().reverse(),
            |value: &mut serde_json::Value| value["destination_world_id"] = serde_json::json!(1),
            |value: &mut serde_json::Value| {
                value["group_zone_revision"] = serde_json::json!(u64::MAX)
            },
            |value: &mut serde_json::Value| value["portal_id"] = serde_json::json!("x".repeat(97)),
        ] {
            let mut value = serde_json::to_value(&original).unwrap();
            mutate(&mut value);
            assert!(serde_json::from_value::<GroupRomHandoffIntent>(value).is_err());
        }
        let mut value = serde_json::to_value(&original).unwrap();
        value["group_zone_revision"] = serde_json::json!(MAX_WORLD_REVISION);
        assert!(serde_json::from_value::<GroupRomHandoffIntent>(value).is_err());
    }
}

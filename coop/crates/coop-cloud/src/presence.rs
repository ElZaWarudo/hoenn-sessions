//! Revision-independent identities used by authenticated world presence.
//!
//! Save fencing intentionally remains revision-bearing.  Presence needs a
//! different identity: a runtime build must remain the same while a save
//! advances, and a session must remain the same while a save snapshot is
//! written.  These values therefore contain only the stable, validated
//! identifiers that the authenticated server binds to an active lease.

use serde::{Deserialize, Serialize};

use crate::{
    BridgeAbiVersion, CharacterId, ClientInstanceId, GameBuildId, MgbaVersion, ProtocolVersion,
    SessionEpoch, SessionId, Sha256Digest,
};

/// The exact build and protocol identity of a running game process.
///
/// `revision` is deliberately absent.  Save revisions identify persisted
/// state, not the executable/runtime that is allowed to publish presence.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBuildIdentity {
    pub game_build_id: GameBuildId,
    pub rom_sha256: Sha256Digest,
    pub mgba_version: MgbaVersion,
    pub bridge_abi: BridgeAbiVersion,
    pub protocol_version: ProtocolVersion,
}
impl RuntimeBuildIdentity {
    /// Creates an identity from already validated build and protocol values.
    #[must_use]
    pub const fn new(
        game_build_id: GameBuildId,
        rom_sha256: Sha256Digest,
        mgba_version: MgbaVersion,
        bridge_abi: BridgeAbiVersion,
        protocol_version: ProtocolVersion,
    ) -> Self {
        Self {
            game_build_id,
            rom_sha256,
            mgba_version,
            bridge_abi,
            protocol_version,
        }
    }

    /// Returns the build identity without copying a save revision.
    #[must_use]
    pub fn from_compatibility_target(target: &crate::CompatibilityTarget) -> Self {
        Self::new(
            target.game_build_id.clone(),
            target.rom_sha256,
            target.mgba_version.clone(),
            target.bridge_abi,
            target.protocol_version,
        )
    }
}

impl From<&crate::CompatibilityTarget> for RuntimeBuildIdentity {
    fn from(target: &crate::CompatibilityTarget) -> Self {
        Self::from_compatibility_target(target)
    }
}

/// The stable session identity that scopes an authenticated presence stream.
///
/// This value intentionally excludes save revision, lease expiry, heartbeat
/// cadence, and credentials.  Those remain part of the lease/save lifecycle
/// and must not partition an otherwise continuous runtime session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StableRuntimeSession {
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl StableRuntimeSession {
    /// Creates a stable session from already validated identifiers.
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        character_id: CharacterId,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
    ) -> Self {
        Self {
            session_id,
            character_id,
            session_epoch,
            client_instance_id,
        }
    }

    /// Derives the revision-independent session identity from save fencing.
    #[must_use]
    pub const fn from_lease_fence(fence: &crate::LeaseFence) -> Self {
        Self::new(
            fence.session_id,
            fence.character_id,
            fence.session_epoch,
            fence.client_instance_id,
        )
    }

    /// Derives the revision-independent session identity from an active lease.
    #[must_use]
    pub const fn from_lease_contract(contract: &crate::LeaseContract) -> Self {
        Self::new(
            contract.session_id,
            contract.character_id,
            contract.session_epoch,
            contract.client_instance_id,
        )
    }
}

impl From<&crate::LeaseFence> for StableRuntimeSession {
    fn from(fence: &crate::LeaseFence) -> Self {
        Self::from_lease_fence(fence)
    }
}

impl From<&crate::LeaseContract> for StableRuntimeSession {
    fn from(contract: &crate::LeaseContract) -> Self {
        Self::from_lease_contract(contract)
    }
}

/// The complete identity bound to an authenticated runtime lease.
///
/// This is a nested structure so its two trust-bearing components remain
/// visibly distinct in JSON and cannot be confused with the revision-bearing
/// `LeaseFence` used for save operations.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLeaseFence {
    pub session: StableRuntimeSession,
    pub build: RuntimeBuildIdentity,
}

impl RuntimeLeaseFence {
    /// Creates a runtime fence from stable, validated identity components.
    #[must_use]
    pub const fn new(session: StableRuntimeSession, build: RuntimeBuildIdentity) -> Self {
        Self { session, build }
    }

    /// Creates a runtime fence from the revision-bearing save fence and a
    /// validated compatibility target.
    #[must_use]
    pub fn from_lease_fence(
        fence: &crate::LeaseFence,
        target: &crate::CompatibilityTarget,
    ) -> Self {
        Self::new(
            StableRuntimeSession::from_lease_fence(fence),
            RuntimeBuildIdentity::from_compatibility_target(target),
        )
    }

    /// Creates a runtime fence from a server-issued lease and compatibility
    /// target without carrying expiry or save revision into the identity.
    #[must_use]
    pub fn from_lease_contract(
        contract: &crate::LeaseContract,
        target: &crate::CompatibilityTarget,
    ) -> Self {
        Self::new(
            StableRuntimeSession::from_lease_contract(contract),
            RuntimeBuildIdentity::from_compatibility_target(target),
        )
    }

    #[must_use]
    pub const fn stable_session(&self) -> StableRuntimeSession {
        self.session
    }

    #[must_use]
    pub const fn runtime_build(&self) -> &RuntimeBuildIdentity {
        &self.build
    }

    #[must_use]
    pub const fn runtime_build_identity(&self) -> &RuntimeBuildIdentity {
        self.runtime_build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompatibilityTarget, LeaseContract, LeaseFence, Revision};
    use serde_json::{Value, json};
    use uuid::Uuid;

    fn id<T>(constructor: fn(Uuid) -> Result<T, crate::IdError>, value: u128) -> T {
        constructor(Uuid::from_u128(value)).expect("test UUID is non-nil")
    }

    fn target(revision: Revision) -> CompatibilityTarget {
        CompatibilityTarget::new(
            GameBuildId::new("pokeemerald-coop-0.1.0").unwrap(),
            Sha256Digest::of_bytes(b"pokeemerald.gba"),
            MgbaVersion::new("0.10.5").unwrap(),
            BridgeAbiVersion::new(1).unwrap(),
            ProtocolVersion::new(1).unwrap(),
            revision,
        )
    }

    fn fence(revision: Revision) -> LeaseFence {
        LeaseFence::new(
            id(SessionId::new, 1),
            id(CharacterId::new, 2),
            revision,
            SessionEpoch::new(7).unwrap(),
            id(ClientInstanceId::new, 3),
        )
    }

    #[test]
    fn runtime_build_round_trips_strict_json_without_revision() {
        let identity = RuntimeBuildIdentity::from_compatibility_target(&target(Revision::new(4)));
        let value = serde_json::to_value(&identity).unwrap();
        assert_eq!(
            value,
            json!({
                "game_build_id": "pokeemerald-coop-0.1.0",
                "rom_sha256": Sha256Digest::of_bytes(b"pokeemerald.gba").as_hex(),
                "mgba_version": "0.10.5",
                "bridge_abi": 1,
                "protocol_version": 1,
            })
        );
        assert_eq!(
            serde_json::from_value::<RuntimeBuildIdentity>(value).unwrap(),
            identity
        );

        let mut unknown = serde_json::to_value(&identity).unwrap();
        unknown["revision"] = json!(4);
        assert!(serde_json::from_value::<RuntimeBuildIdentity>(unknown).is_err());
    }

    #[test]
    fn runtime_build_identity_ignores_revision_and_partitions_multivalued_fields() {
        let first = RuntimeBuildIdentity::from_compatibility_target(&target(Revision::new(1)));
        let same_build = RuntimeBuildIdentity::from_compatibility_target(&target(Revision::new(2)));
        assert_eq!(first, same_build);

        let mut changed = target(Revision::new(1));
        changed.game_build_id = GameBuildId::new("pokeemerald-coop-0.1.1").unwrap();
        assert_ne!(
            first,
            RuntimeBuildIdentity::from_compatibility_target(&changed)
        );
        let mut changed = target(Revision::new(1));
        changed.rom_sha256 = Sha256Digest::of_bytes(b"other.gba");
        assert_ne!(
            first,
            RuntimeBuildIdentity::from_compatibility_target(&changed)
        );
        let mut changed = target(Revision::new(1));
        changed.mgba_version = MgbaVersion::new("0.10.4").unwrap();
        assert_ne!(
            first,
            RuntimeBuildIdentity::from_compatibility_target(&changed)
        );

        // These versions currently have one accepted value each.  Verify that
        // conversion preserves that value and that it is present in the
        // strict JSON shape, without inventing an impossible inequality case.
        assert_eq!(first.bridge_abi, BridgeAbiVersion::new(1).unwrap());
        assert_eq!(first.protocol_version, ProtocolVersion::new(1).unwrap());
        let serialized = serde_json::to_value(first).unwrap();
        assert_eq!(
            serialized.get("bridge_abi"),
            Some(&json!(BridgeAbiVersion::new(1).unwrap().value()))
        );
        assert_eq!(
            serialized.get("protocol_version"),
            Some(&json!(ProtocolVersion::new(1).unwrap().value()))
        );
        assert!(BridgeAbiVersion::new(2).is_err());
        assert!(ProtocolVersion::new(2).is_err());
    }

    #[test]
    fn stable_session_round_trips_and_ignores_revision() {
        let first = StableRuntimeSession::from_lease_fence(&fence(Revision::new(1)));
        let later = StableRuntimeSession::from_lease_fence(&fence(Revision::new(99)));
        assert_eq!(first, later);
        assert_eq!(
            serde_json::from_value::<StableRuntimeSession>(serde_json::to_value(first).unwrap())
                .unwrap(),
            first
        );

        let mut unknown = serde_json::to_value(first).unwrap();
        unknown["current_revision"] = json!(99);
        assert!(serde_json::from_value::<StableRuntimeSession>(unknown).is_err());

        let mut changed = first;
        changed.session_id = id(SessionId::new, 4);
        assert_ne!(first, changed);
        let mut changed = first;
        changed.character_id = id(CharacterId::new, 5);
        assert_ne!(first, changed);
        let mut changed = first;
        changed.session_epoch = SessionEpoch::new(8).unwrap();
        assert_ne!(first, changed);
        let mut changed = first;
        changed.client_instance_id = id(ClientInstanceId::new, 6);
        assert_ne!(first, changed);
    }

    #[test]
    fn runtime_lease_fence_contains_only_stable_components() {
        let compatibility = target(Revision::new(3));
        let save_fence = fence(Revision::new(3));
        let runtime = RuntimeLeaseFence::from_lease_fence(&save_fence, &compatibility);
        let later = RuntimeLeaseFence::from_lease_fence(
            &fence(Revision::new(4)),
            &target(Revision::new(4)),
        );
        assert_eq!(runtime, later);
        assert_eq!(
            runtime.stable_session(),
            StableRuntimeSession::from_lease_fence(&save_fence)
        );
        assert_eq!(
            runtime.runtime_build(),
            &RuntimeBuildIdentity::from_compatibility_target(&compatibility)
        );

        let contract =
            LeaseContract::new(save_fence, crate::UnixTimestampMillis::new(100), 250).unwrap();
        assert_eq!(
            RuntimeLeaseFence::from_lease_contract(&contract, &compatibility),
            runtime
        );

        let mut value = serde_json::to_value(&runtime).unwrap();
        assert!(value.get("current_revision").is_none());
        assert!(value.get("expires_at").is_none());
        value["token"] = json!("secret");
        assert!(serde_json::from_value::<RuntimeLeaseFence>(value).is_err());
    }

    #[test]
    fn malformed_nested_strong_values_are_rejected() {
        let mut value = serde_json::to_value(RuntimeLeaseFence::from_lease_fence(
            &fence(Revision::initial()),
            &target(Revision::initial()),
        ))
        .unwrap();
        value["session"]["session_epoch"] = json!(0);
        assert!(serde_json::from_value::<RuntimeLeaseFence>(value).is_err());

        let mut value = serde_json::to_value(RuntimeBuildIdentity::from_compatibility_target(
            &target(Revision::initial()),
        ))
        .unwrap();
        value["rom_sha256"] = Value::String("not-a-sha256".to_owned());
        assert!(serde_json::from_value::<RuntimeBuildIdentity>(value).is_err());
    }
}

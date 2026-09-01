//! Generated, append-only identities used by persisted regional progress.

use crate::{
    BadgeId, EventId, FlyPointId, GymId, IdentityKind, ProtocolError, RegionId, TrainerInstanceId,
};

/// One stable identity assignment in the persistence registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentityCatalogEntry {
    pub kind: IdentityKind,
    pub region: RegionId,
    pub qualified_id: &'static str,
    /// Bit ordinal within this identity kind's frozen persisted capacity.
    /// Badges use `(region, badge_bit)` instead and therefore have no global
    /// ordinal.
    pub ordinal: Option<u16>,
    /// Engine opponent/flag value used only by the migration boundary.
    pub legacy_value: Option<u16>,
    /// Auditable C constant whose value is asserted by the ROM build.
    pub legacy_symbol: Option<&'static str>,
    /// Regional badge bit, present only for badge identities.
    pub badge_bit: Option<u8>,
}

include!("generated_identity_registry.rs");

/// Returns every assigned identity in append-only ledger order.
#[must_use]
pub const fn all_identities() -> &'static [IdentityCatalogEntry] {
    GENERATED_IDENTITY_REGISTRY
}

/// Resolves an exact kind and qualified ID.
///
/// # Errors
///
/// Returns [`ProtocolError::UnknownIdentity`] when the syntactically valid ID
/// has no stable persisted ordinal.
pub fn resolve_identity(
    kind: IdentityKind,
    qualified_id: &str,
) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    GENERATED_IDENTITY_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind && entry.qualified_id == qualified_id)
        .ok_or_else(|| ProtocolError::UnknownIdentity {
            kind,
            value: qualified_id.to_owned(),
        })
}

/// Resolves a persisted ordinal without interpreting it as another kind.
///
/// # Errors
///
/// Returns [`ProtocolError::UnknownIdentityOrdinal`] for an unassigned bit.
pub fn resolve_ordinal(
    kind: IdentityKind,
    ordinal: u16,
) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    GENERATED_IDENTITY_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind && entry.ordinal == Some(ordinal))
        .ok_or(ProtocolError::UnknownIdentityOrdinal { kind, ordinal })
}

/// Resolves an engine migration value in one concrete region.
///
/// Trainer values are opponent IDs; the other kinds use legacy event flags.
/// The region remains mandatory because Kanto, Hoenn, and future campaigns
/// intentionally reuse several raw values.
///
/// # Errors
///
/// Returns [`ProtocolError::UnknownLegacyIdentity`] when the migration value
/// is not assigned in this region and kind.
pub fn resolve_legacy_identity(
    kind: IdentityKind,
    region: RegionId,
    legacy_value: u16,
) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    let region = region.ensure_concrete()?;
    GENERATED_IDENTITY_REGISTRY
        .iter()
        .find(|entry| {
            entry.kind == kind && entry.region == region && entry.legacy_value == Some(legacy_value)
        })
        .ok_or(ProtocolError::UnknownLegacyIdentity {
            kind,
            region,
            legacy_value,
        })
}

/// Resolves one of the low-eight regional badge bits.
///
/// # Errors
///
/// Returns [`ProtocolError::UnknownRegionalBadgeBit`] when the region has no
/// assigned badge at this bit (for example Sevii in registry version 1).
pub fn resolve_badge_bit(
    region: RegionId,
    badge_bit: u8,
) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    let region = region.ensure_concrete()?;
    GENERATED_IDENTITY_REGISTRY
        .iter()
        .find(|entry| {
            entry.kind == IdentityKind::Badge
                && entry.region == region
                && entry.badge_bit == Some(badge_bit)
        })
        .ok_or(ProtocolError::UnknownRegionalBadgeBit { region, badge_bit })
}

/// Validates a trainer identity against the persisted registry.
///
/// # Errors
///
/// Returns an unknown-identity error when no ordinal is assigned.
pub fn trainer(
    identity: &TrainerInstanceId,
) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    resolve_identity(IdentityKind::Trainer, identity.as_str())
}

/// Validates a gym identity against the persisted registry.
///
/// # Errors
///
/// Returns an unknown-identity error when no ordinal is assigned.
pub fn gym(identity: &GymId) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    resolve_identity(IdentityKind::Gym, identity.as_str())
}

/// Validates a badge identity against the persisted registry.
///
/// # Errors
///
/// Returns an unknown-identity error when no ordinal is assigned.
pub fn badge(identity: &BadgeId) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    resolve_identity(IdentityKind::Badge, identity.as_str())
}

/// Validates a fly-point identity against the persisted registry.
///
/// # Errors
///
/// Returns an unknown-identity error when no ordinal is assigned.
pub fn fly_point(identity: &FlyPointId) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    resolve_identity(IdentityKind::FlyPoint, identity.as_str())
}

/// Validates an event identity against the persisted registry.
///
/// # Errors
///
/// Returns an unknown-identity error when no ordinal is assigned.
pub fn event(identity: &EventId) -> Result<&'static IdentityCatalogEntry, ProtocolError> {
    resolve_identity(IdentityKind::Event, identity.as_str())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn generated_registry_has_unique_bounded_ordinals() {
        assert_eq!(IDENTITY_REGISTRY_VERSION, 1);
        assert_eq!(IDENTITY_REGISTRY_DIGEST.len(), 16);
        assert_eq!(TRAINER_IDENTITY_CAPACITY, 2048);
        assert_eq!(EVENT_IDENTITY_CAPACITY, 2048);
        assert_eq!(FLY_POINT_IDENTITY_CAPACITY, 128);
        assert_eq!(GYM_IDENTITY_CAPACITY, 64);
        assert_eq!(BADGES_PER_REGION, 8);

        let mut identities = HashSet::new();
        let mut ordinals = HashSet::new();
        for entry in all_identities() {
            assert!(identities.insert(entry.qualified_id));
            if let Some(ordinal) = entry.ordinal {
                assert!(ordinals.insert((entry.kind, ordinal)));
                assert_eq!(resolve_ordinal(entry.kind, ordinal).unwrap(), entry);
            } else {
                assert_eq!(entry.kind, IdentityKind::Badge);
                assert!(entry.badge_bit.is_some());
            }
            assert_eq!(
                resolve_identity(entry.kind, entry.qualified_id).unwrap(),
                entry
            );
        }
    }

    #[test]
    fn hoenn_trainer_ledger_covers_the_complete_mvp_range() {
        let trainers = all_identities()
            .iter()
            .filter(|entry| entry.kind == IdentityKind::Trainer)
            .collect::<Vec<_>>();
        let hoenn = trainers
            .iter()
            .copied()
            .filter(|entry| entry.region == RegionId::Hoenn)
            .collect::<Vec<_>>();
        assert_eq!(trainers.len(), 856);
        assert_eq!(hoenn.len(), 854);
        assert_eq!(
            (
                hoenn[0].qualified_id,
                hoenn[0].ordinal,
                hoenn[0].legacy_value
            ),
            ("HOENN:TRAINER_SAWYER_1", Some(0), Some(1))
        );
        assert_eq!(
            (
                hoenn[853].qualified_id,
                hoenn[853].ordinal,
                hoenn[853].legacy_value,
            ),
            ("HOENN:TRAINER_MAY_PLACEHOLDER", Some(853), Some(854))
        );
        let brock = resolve_identity(IdentityKind::Trainer, "KANTO:TRAINER_BROCK").unwrap();
        let falkner = resolve_identity(IdentityKind::Trainer, "JOHTO:TRAINER_FALKNER").unwrap();
        assert_eq!(brock.ordinal, Some(854));
        assert_eq!(falkner.ordinal, Some(855));
    }

    #[test]
    fn region_disambiguates_reused_legacy_badge_flags() {
        let hoenn = resolve_legacy_identity(IdentityKind::Badge, RegionId::Hoenn, 2909)
            .expect("Hoenn badge flag");
        let kanto = resolve_legacy_identity(IdentityKind::Badge, RegionId::Kanto, 2909)
            .expect("Kanto badge flag");
        assert_eq!(hoenn.qualified_id, "HOENN:BADGE_STONE");
        assert_eq!(kanto.qualified_id, "KANTO:BADGE_BOULDER");
        assert_eq!(hoenn.ordinal, None);
        assert_eq!(kanto.ordinal, None);
        assert_eq!(resolve_badge_bit(RegionId::Hoenn, 0).unwrap(), hoenn);
        assert_eq!(resolve_badge_bit(RegionId::Kanto, 0).unwrap(), kanto);
        assert!(resolve_badge_bit(RegionId::Sevii, 0).is_err());
    }

    #[test]
    fn syntax_alone_never_assigns_a_persisted_bit() {
        let unregistered = TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_NOT_REGISTERED")
            .expect("identity is syntactically valid");
        assert!(matches!(
            trainer(&unregistered),
            Err(ProtocolError::UnknownIdentity {
                kind: IdentityKind::Trainer,
                ..
            })
        ));
        assert!(matches!(
            resolve_ordinal(IdentityKind::Trainer, 2047),
            Err(ProtocolError::UnknownIdentityOrdinal { .. })
        ));
    }
}

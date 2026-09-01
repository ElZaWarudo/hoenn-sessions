//! Region-safe shared contracts for `PokéCrossroads` cloud co-op.
//!
//! This crate deliberately has no dependency on the ROM's Region enum or on
//! any engine data structure. The values in this module are stable host
//! protocol values and remain independent when another region is added.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize, Serializer, de::Deserializer};
use thiserror::Error;

pub mod catalog;

pub use catalog::{MAP_CATALOG, MapCatalog, MapCatalogEntry, all_maps};

/// Regions understood by the co-op wire protocol.
///
/// Unspecified is reserved for low-level wire adapters; world and identity
/// constructors reject it. The explicit ordinals do not come from an engine
/// enum.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RegionId {
    Unspecified = 0,
    Hoenn = 1,
    Kanto = 2,
    Johto = 3,
    Sevii = 4,
}

impl RegionId {
    #[must_use]
    pub const fn wire(self) -> u8 {
        self as u8
    }

    /// Converts a protocol ordinal without consulting the game engine.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnknownRegionOrdinal`] for an ordinal that is
    /// not part of this protocol.
    pub const fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::Hoenn),
            2 => Ok(Self::Kanto),
            3 => Ok(Self::Johto),
            4 => Ok(Self::Sevii),
            ordinal => Err(ProtocolError::UnknownRegionOrdinal { ordinal }),
        }
    }

    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Unspecified => "UNSPECIFIED",
            Self::Hoenn => "HOENN",
            Self::Kanto => "KANTO",
            Self::Johto => "JOHTO",
            Self::Sevii => "SEVII",
        }
    }

    /// Parses a canonical uppercase protocol token.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::InvalidRegion`] when the token is unknown.
    pub fn parse_token(value: &str) -> Result<Self, ProtocolError> {
        match value {
            "UNSPECIFIED" => Ok(Self::Unspecified),
            "HOENN" => Ok(Self::Hoenn),
            "KANTO" => Ok(Self::Kanto),
            "JOHTO" => Ok(Self::Johto),
            "SEVII" => Ok(Self::Sevii),
            _ => Err(ProtocolError::InvalidRegion {
                value: value.to_owned(),
            }),
        }
    }

    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnspecifiedRegion`] for the reserved value.
    pub const fn ensure_concrete(self) -> Result<Self, ProtocolError> {
        match self {
            Self::Unspecified => Err(ProtocolError::UnspecifiedRegion),
            region => Ok(region),
        }
    }
}

impl fmt::Display for RegionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.token())
    }
}

impl FromStr for RegionId {
    type Err = ProtocolError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_token(value)
    }
}

impl Serialize for RegionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for RegionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        Self::parse_token(&token).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityKind {
    Trainer,
    Gym,
    Badge,
    FlyPoint,
    Event,
}

impl fmt::Display for IdentityKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Trainer => "trainer",
            Self::Gym => "gym",
            Self::Badge => "badge",
            Self::FlyPoint => "fly point",
            Self::Event => "event",
        };
        formatter.write_str(name)
    }
}

fn validate_local_key(key: &str, kind: IdentityKind) -> Result<(), ProtocolError> {
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ProtocolError::InvalidLocalKey {
            value: key.to_owned(),
        });
    }

    let correct_prefix = match kind {
        IdentityKind::Trainer => has_nonempty_suffix(key, "TRAINER_"),
        IdentityKind::Gym => has_nonempty_suffix(key, "GYM_"),
        IdentityKind::Badge => has_nonempty_suffix(key, "BADGE_"),
        IdentityKind::FlyPoint => has_nonempty_suffix(key, "FLY_"),
        IdentityKind::Event => has_nonempty_suffix(key, "EVENT_"),
    };
    if correct_prefix {
        Ok(())
    } else if let Some(actual) = identity_kind_for_key(key) {
        Err(ProtocolError::WrongIdentityKind {
            expected: kind,
            actual,
        })
    } else {
        Err(ProtocolError::InvalidLocalKey {
            value: key.to_owned(),
        })
    }
}

fn has_nonempty_suffix(key: &str, prefix: &str) -> bool {
    key.strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty())
}

fn identity_kind_for_key(key: &str) -> Option<IdentityKind> {
    if key.starts_with("TRAINER_") {
        Some(IdentityKind::Trainer)
    } else if key.starts_with("GYM_") {
        Some(IdentityKind::Gym)
    } else if key.starts_with("BADGE_") {
        Some(IdentityKind::Badge)
    } else if key.starts_with("FLY_") {
        Some(IdentityKind::FlyPoint)
    } else if key.starts_with("EVENT_") {
        Some(IdentityKind::Event)
    } else {
        None
    }
}

fn parse_qualified(
    value: &str,
    expected: IdentityKind,
) -> Result<(RegionId, String), ProtocolError> {
    let mut parts = value.split(':');
    let region_token = parts.next().unwrap_or_default();
    let local_key = parts
        .next()
        .ok_or_else(|| ProtocolError::InvalidQualifiedIdentity {
            value: value.to_owned(),
        })?;
    if parts.next().is_some() {
        return Err(ProtocolError::InvalidQualifiedIdentity {
            value: value.to_owned(),
        });
    }

    let region = RegionId::parse_token(region_token)?.ensure_concrete()?;
    validate_local_key(local_key, expected)?;
    Ok((region, local_key.to_owned()))
}

macro_rules! qualified_identity {
    ($name:ident, $kind:ident) => {
        /// A validated, region-qualified identity.
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name {
            value: String,
            region: RegionId,
        }

        impl $name {
            /// Creates an identity in canonical `REGION:LOCAL_KEY` form.
            ///
            /// # Errors
            ///
            /// Returns an error when the region is unspecified or the local
            /// key is not canonical for this identity kind.
            pub fn new(region: RegionId, local_key: &str) -> Result<Self, ProtocolError> {
                let region = region.ensure_concrete()?;
                validate_local_key(local_key, IdentityKind::$kind)?;
                Ok(Self {
                    value: format!("{region}:{local_key}"),
                    region,
                })
            }

            /// Parses and validates a canonical identity.
            ///
            /// # Errors
            ///
            /// Returns an error for a malformed, unknown-region, or
            /// cross-kind identity.
            pub fn parse(value: &str) -> Result<Self, ProtocolError> {
                let (region, local_key) = parse_qualified(value, IdentityKind::$kind)?;
                Ok(Self {
                    value: format!("{region}:{local_key}"),
                    region,
                })
            }

            #[must_use]
            pub const fn region(&self) -> RegionId {
                self.region
            }

            #[must_use]
            pub fn local_key(&self) -> &str {
                // The delimiter is an invariant of every constructed value.
                self.value
                    .split_once(':')
                    .map_or("", |(_, local_key)| local_key)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.value
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ProtocolError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

qualified_identity!(TrainerInstanceId, Trainer);
qualified_identity!(GymId, Gym);
qualified_identity!(BadgeId, Badge);
qualified_identity!(FlyPointId, FlyPoint);
qualified_identity!(EventId, Event);

/// A location inside a regional map, independent of raw engine structs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct WorldLocation {
    pub region: RegionId,
    pub map_group: u16,
    pub map_number: u16,
    pub x: i16,
    pub y: i16,
}

impl WorldLocation {
    /// Constructs a location with a concrete region.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnspecifiedRegion`] when no region is given.
    pub fn new(
        region: RegionId,
        map_group: u16,
        map_number: u16,
        x: i16,
        y: i16,
    ) -> Result<Self, ProtocolError> {
        let region = region.ensure_concrete()?;
        Ok(Self {
            region,
            map_group,
            map_number,
            x,
            y,
        })
    }

    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnspecifiedRegion`] for the reserved region.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.region.ensure_concrete().map(|_| ())
    }

    /// Resolves this location's exact map in the generated catalog.
    ///
    /// # Errors
    ///
    /// Returns a typed error when this region and numeric coordinate pair is
    /// absent or belongs to another region.
    pub fn map(&self) -> Result<&'static MapCatalogEntry, ProtocolError> {
        catalog::resolve_map_coordinates(self.region, self.map_group, self.map_number)
    }

    /// Translates this numeric location into the version-1 world-zone shape.
    ///
    /// # Errors
    ///
    /// Returns a typed catalog error when this location's map is unknown.
    pub fn to_zone(&self, channel: u16) -> Result<WorldZone, ProtocolError> {
        WorldZone::from_location(self, channel)
    }
}

impl<'de> Deserialize<'de> for WorldLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireLocation {
            region: RegionId,
            map_group: u16,
            map_number: u16,
            x: i16,
            y: i16,
        }

        let wire = WireLocation::deserialize(deserializer)?;
        Self::new(wire.region, wire.map_group, wire.map_number, wire.x, wire.y)
            .map_err(serde::de::Error::custom)
    }
}

fn validate_map_key(map: &str) -> Result<(), ProtocolError> {
    if map.is_empty()
        || !map
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        Err(ProtocolError::InvalidMapKey {
            value: map.to_owned(),
        })
    } else {
        Ok(())
    }
}

/// A logical online world zone. The same character/save can move between
/// zones; the region is part of the zone identity rather than a server shard.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorldZone {
    pub region: RegionId,
    pub map: String,
    pub channel: u16,
}

impl WorldZone {
    /// Constructs a logical zone with a canonical local map key.
    ///
    /// # Errors
    ///
    /// Returns an error when the region is unspecified or the map key is not
    /// uppercase ASCII.
    pub fn new(
        region: RegionId,
        map: impl Into<String>,
        channel: u16,
    ) -> Result<Self, ProtocolError> {
        let region = region.ensure_concrete()?;
        let map = map.into();
        validate_map_key(&map)?;
        catalog::resolve_map(region, &map)?;
        Ok(Self {
            region,
            map,
            channel,
        })
    }

    ///
    /// # Errors
    ///
    /// Returns an error when the region or map key is invalid.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.region.ensure_concrete()?;
        validate_map_key(&self.map)?;
        catalog::resolve_map(self.region, &self.map).map(|_| ())
    }

    /// Resolves this zone's exact map in the generated catalog.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the zone's region/map pair is absent or
    /// belongs to another region.
    pub fn map_entry(&self) -> Result<&'static MapCatalogEntry, ProtocolError> {
        catalog::resolve_map(self.region, &self.map)
    }

    /// Translates this zone into the numeric location used by the ROM bridge.
    ///
    /// # Errors
    ///
    /// Returns a typed catalog error when this zone's map is unknown.
    pub fn to_location(&self, x: i16, y: i16) -> Result<WorldLocation, ProtocolError> {
        let entry = self.map_entry()?;
        WorldLocation::new(self.region, entry.map_group, entry.map_number, x, y)
    }

    /// Alias for [`WorldZone::to_location`] for callers using the full name.
    ///
    /// # Errors
    ///
    /// Propagates the errors from [`WorldZone::to_location`].
    pub fn to_world_location(&self, x: i16, y: i16) -> Result<WorldLocation, ProtocolError> {
        self.to_location(x, y)
    }

    /// Translates an exact numeric location into a canonical world zone.
    ///
    /// # Errors
    ///
    /// Returns a typed catalog error when this location's map is unknown.
    pub fn from_location(location: &WorldLocation, channel: u16) -> Result<Self, ProtocolError> {
        let entry = location.map()?;
        Self::new(location.region, entry.map, channel)
    }

    /// Alias for [`WorldZone::from_location`] for callers using the full name.
    ///
    /// # Errors
    ///
    /// Propagates the errors from [`WorldZone::from_location`].
    pub fn from_world_location(
        location: &WorldLocation,
        channel: u16,
    ) -> Result<Self, ProtocolError> {
        Self::from_location(location, channel)
    }
}

impl<'de> Deserialize<'de> for WorldZone {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireZone {
            region: RegionId,
            map: String,
            channel: u16,
        }

        let wire = WireZone::deserialize(deserializer)?;
        Self::new(wire.region, wire.map, wire.channel).map_err(serde::de::Error::custom)
    }
}

/// Badge bits currently assigned by every protocol and ROM implementation.
///
/// `RegionalProgress::badge_mask` remains `u16` on the wire and in storage so
/// the contract can grow without a representation migration. Until such a
/// protocol revision exists, bits outside this low-eight-bit mask are reserved
/// and cannot contribute to a tier or satisfy an entitlement. This is the Rust
/// counterpart of the ROM's `COOP_PROGRESS_BADGE_MASK`.
pub const VALID_REGIONAL_BADGE_MASK: u16 = 0x00FF;

/// A player's progress for one concrete region.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RegionalProgress {
    pub region: RegionId,
    /// Raw `u16` wire/storage representation; only
    /// [`VALID_REGIONAL_BADGE_MASK`] bits have badge semantics.
    pub badge_mask: u16,
    pub story_checkpoint: u32,
    pub defeated_trainers: Vec<TrainerInstanceId>,
    pub unlocked_fly_points: Vec<FlyPointId>,
}

impl RegionalProgress {
    /// Creates normalized progress with sorted, duplicate-free identity lists.
    ///
    /// # Errors
    ///
    /// Returns an error when the region is unspecified or an identity belongs
    /// to another region.
    pub fn new(
        region: RegionId,
        badge_mask: u16,
        story_checkpoint: u32,
        mut defeated_trainers: Vec<TrainerInstanceId>,
        mut unlocked_fly_points: Vec<FlyPointId>,
    ) -> Result<Self, ProtocolError> {
        let region = region.ensure_concrete()?;
        for trainer in &defeated_trainers {
            if trainer.region() != region {
                return Err(ProtocolError::IdentityRegionMismatch {
                    expected: region,
                    actual: trainer.region(),
                });
            }
        }
        for fly_point in &unlocked_fly_points {
            if fly_point.region() != region {
                return Err(ProtocolError::IdentityRegionMismatch {
                    expected: region,
                    actual: fly_point.region(),
                });
            }
        }

        defeated_trainers.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        defeated_trainers.dedup();
        unlocked_fly_points.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        unlocked_fly_points.dedup();

        Ok(Self {
            region,
            badge_mask,
            story_checkpoint,
            defeated_trainers,
            unlocked_fly_points,
        })
    }

    /// Returns the number of assigned badge bits earned in this region only.
    /// Reserved high bits are ignored.
    #[must_use]
    pub fn badge_count(&self) -> u8 {
        u8::try_from((self.badge_mask & VALID_REGIONAL_BADGE_MASK).count_ones()).unwrap_or(0)
    }

    /// Returns whether every required assigned badge bit is present.
    ///
    /// A requirement containing any reserved bit fails closed, even if the
    /// same bit happens to be present in the raw stored mask.
    #[must_use]
    pub const fn has_badges(&self, required_mask: u16) -> bool {
        required_mask & !VALID_REGIONAL_BADGE_MASK == 0
            && self.badge_mask & VALID_REGIONAL_BADGE_MASK & required_mask == required_mask
    }

    /// Validates public fields and their regional identity invariants.
    ///
    /// # Errors
    ///
    /// Returns an error for an unspecified or mismatched region, or for a
    /// collection that is not sorted and duplicate-free.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.region.ensure_concrete()?;
        for trainer in &self.defeated_trainers {
            if trainer.region() != self.region {
                return Err(ProtocolError::IdentityRegionMismatch {
                    expected: self.region,
                    actual: trainer.region(),
                });
            }
        }
        for fly_point in &self.unlocked_fly_points {
            if fly_point.region() != self.region {
                return Err(ProtocolError::IdentityRegionMismatch {
                    expected: self.region,
                    actual: fly_point.region(),
                });
            }
        }
        if self
            .defeated_trainers
            .windows(2)
            .any(|window| window[0].as_str() >= window[1].as_str())
            || self
                .unlocked_fly_points
                .windows(2)
                .any(|window| window[0].as_str() >= window[1].as_str())
        {
            return Err(ProtocolError::NonCanonicalCollection);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RegionalProgress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireProgress {
            region: RegionId,
            badge_mask: u16,
            story_checkpoint: u32,
            defeated_trainers: Vec<TrainerInstanceId>,
            unlocked_fly_points: Vec<FlyPointId>,
        }

        let wire = WireProgress::deserialize(deserializer)?;
        Self::new(
            wire.region,
            wire.badge_mask,
            wire.story_checkpoint,
            wire.defeated_trainers,
            wire.unlocked_fly_points,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// A character and all of that character's regional progress records.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ParticipantProgress {
    character_id: u64,
    regional_progress: Vec<RegionalProgress>,
}

impl ParticipantProgress {
    /// Creates a participant record and rejects duplicate region records.
    ///
    /// # Errors
    ///
    /// Returns an error when regional progress is duplicated or invalid.
    pub fn new(
        character_id: u64,
        mut regional_progress: Vec<RegionalProgress>,
    ) -> Result<Self, ProtocolError> {
        regional_progress.sort_unstable_by_key(|progress| progress.region.wire());
        if regional_progress
            .windows(2)
            .any(|window| window[0].region == window[1].region)
        {
            return Err(ProtocolError::DuplicateRegionalProgress);
        }
        for progress in &regional_progress {
            progress.validate()?;
        }
        Ok(Self {
            character_id,
            regional_progress,
        })
    }

    #[must_use]
    pub fn progress_for(&self, region: RegionId) -> Option<&RegionalProgress> {
        self.regional_progress
            .iter()
            .find(|progress| progress.region == region)
    }

    #[must_use]
    pub const fn character_id(&self) -> u64 {
        self.character_id
    }

    #[must_use]
    pub fn regional_progress(&self) -> &[RegionalProgress] {
        &self.regional_progress
    }

    /// Validates the canonical ordering and every nested regional record.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate, unsorted, or invalid regional progress.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self
            .regional_progress
            .windows(2)
            .any(|window| window[0].region >= window[1].region)
        {
            return Err(ProtocolError::DuplicateRegionalProgress);
        }
        for progress in &self.regional_progress {
            progress.validate()?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ParticipantProgress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireParticipantProgress {
            character_id: u64,
            regional_progress: Vec<RegionalProgress>,
        }

        let wire = WireParticipantProgress::deserialize(deserializer)?;
        Self::new(wire.character_id, wire.regional_progress).map_err(serde::de::Error::custom)
    }
}

/// Computes the co-op battle tier in `battle_region`.
///
/// The result is the minimum regional badge count across all participants.
/// Progress from every other region is ignored, and a missing record fails
/// closed.
///
/// # Errors
///
/// Returns an error for an unspecified battle region, an empty participant
/// list, duplicate participants, or missing regional progress.
pub fn group_battle_tier(
    participants: &[ParticipantProgress],
    battle_region: RegionId,
) -> Result<u8, ProtocolError> {
    let battle_region = battle_region.ensure_concrete()?;
    if participants.is_empty() {
        return Err(ProtocolError::EmptyParticipants);
    }

    let mut minimum = u8::MAX;
    for (index, participant) in participants.iter().enumerate() {
        participant.validate()?;
        if participants[..index]
            .iter()
            .any(|previous| previous.character_id == participant.character_id)
        {
            return Err(ProtocolError::DuplicateParticipant {
                character_id: participant.character_id,
            });
        }
        let progress = participant.progress_for(battle_region).ok_or(
            ProtocolError::MissingRegionalProgress {
                character_id: participant.character_id,
                region: battle_region,
            },
        )?;
        minimum = minimum.min(progress.badge_count());
    }
    Ok(minimum)
}

/// Alias emphasizing that the tier is regional rather than global.
///
/// # Errors
///
/// Propagates the validation errors from [`group_battle_tier`].
pub fn cooperative_battle_tier(
    participants: &[ParticipantProgress],
    battle_region: RegionId,
) -> Result<u8, ProtocolError> {
    group_battle_tier(participants, battle_region)
}

/// A symmetric two-character co-op group.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Group {
    members: [u64; 2],
}

impl Group {
    /// Creates a group without a leader, owner, or host field.
    ///
    /// # Errors
    ///
    /// Returns an error when both character IDs are equal.
    pub const fn new(first: u64, second: u64) -> Result<Self, ProtocolError> {
        if first == second {
            return Err(ProtocolError::DuplicateParticipant {
                character_id: first,
            });
        }
        let members = if first < second {
            [first, second]
        } else {
            [second, first]
        };
        Ok(Self { members })
    }

    #[must_use]
    pub const fn contains(&self, character_id: u64) -> bool {
        self.members[0] == character_id || self.members[1] == character_id
    }

    #[must_use]
    pub const fn members(&self) -> [u64; 2] {
        self.members
    }

    ///
    /// # Errors
    ///
    /// Returns an error when both members are the same character.
    pub const fn validate(&self) -> Result<(), ProtocolError> {
        if self.members[0] == self.members[1] {
            Err(ProtocolError::DuplicateParticipant {
                character_id: self.members[0],
            })
        } else if self.members[0] > self.members[1] {
            Err(ProtocolError::NonCanonicalGroup)
        } else {
            Ok(())
        }
    }

    /// Atomically moves the group when every member is entitled by the route.
    ///
    /// # Errors
    ///
    /// Returns an error when the group, zones, route, membership, or any
    /// member's route progress is invalid. The current zone is unchanged.
    pub fn transfer(
        &self,
        current_zone: &mut WorldZone,
        destination: WorldZone,
        participants: &[ParticipantProgress],
        route: &TravelRoute,
    ) -> Result<(), ProtocolError> {
        atomic_group_travel(self, current_zone, destination, participants, route)
    }
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireGroup {
            members: [u64; 2],
        }

        let wire = WireGroup::deserialize(deserializer)?;
        Self::new(wire.members[0], wire.members[1]).map_err(serde::de::Error::custom)
    }
}

/// Name for callers that prefer to make co-op explicit.
pub type CoopGroup = Group;

/// A route between two logical regions and the progress required to use it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TravelRoute {
    from: RegionId,
    to: RegionId,
    minimum_badges: u8,
    minimum_story_checkpoint: u32,
}

impl TravelRoute {
    /// Creates a directed route with explicit progress requirements.
    ///
    /// # Errors
    ///
    /// Returns an error when an endpoint is unspecified or both endpoints are
    /// the same region.
    pub fn new(
        from: RegionId,
        to: RegionId,
        minimum_badges: u8,
        minimum_story_checkpoint: u32,
    ) -> Result<Self, ProtocolError> {
        let from = from.ensure_concrete()?;
        let to = to.ensure_concrete()?;
        if from == to {
            return Err(ProtocolError::SameRegionRoute { region: from });
        }
        Ok(Self {
            from,
            to,
            minimum_badges,
            minimum_story_checkpoint,
        })
    }

    ///
    /// # Errors
    ///
    /// Returns an error when an endpoint is unspecified or both endpoints are
    /// the same region.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match (self.from.ensure_concrete(), self.to.ensure_concrete()) {
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(from), Ok(to)) if from == to => {
                Err(ProtocolError::SameRegionRoute { region: from })
            }
            (Ok(_), Ok(_)) => Ok(()),
        }
    }

    #[must_use]
    pub const fn from(&self) -> RegionId {
        self.from
    }

    #[must_use]
    pub const fn to(&self) -> RegionId {
        self.to
    }

    #[must_use]
    pub const fn minimum_badges(&self) -> u8 {
        self.minimum_badges
    }

    #[must_use]
    pub const fn minimum_story_checkpoint(&self) -> u32 {
        self.minimum_story_checkpoint
    }
}

impl<'de> Deserialize<'de> for TravelRoute {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireTravelRoute {
            from: RegionId,
            to: RegionId,
            minimum_badges: u8,
            minimum_story_checkpoint: u32,
        }

        let wire = WireTravelRoute::deserialize(deserializer)?;
        Self::new(
            wire.from,
            wire.to,
            wire.minimum_badges,
            wire.minimum_story_checkpoint,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Moves a group only after validating every member, source zone, destination,
/// and route entitlement. Validation precedes the sole mutation.
///
/// # Errors
///
/// Returns an error when the group, zones, route, membership, or any member's
/// route progress is invalid. The current zone is unchanged on every error.
pub fn atomic_group_travel(
    group: &Group,
    current_zone: &mut WorldZone,
    destination: WorldZone,
    participants: &[ParticipantProgress],
    route: &TravelRoute,
) -> Result<(), ProtocolError> {
    group.validate()?;
    current_zone.validate()?;
    destination.validate()?;
    route.validate()?;
    if current_zone.region != route.from {
        return Err(ProtocolError::SourceZoneMismatch {
            expected: route.from,
            actual: current_zone.region,
        });
    }
    if destination.region != route.to {
        return Err(ProtocolError::DestinationZoneMismatch {
            expected: route.to,
            actual: destination.region,
        });
    }
    if participants.len() != 2 {
        return Err(ProtocolError::GroupMembershipMismatch);
    }

    let mut participant_ids = [participants[0].character_id, participants[1].character_id];
    if participant_ids[0] == participant_ids[1] {
        return Err(ProtocolError::DuplicateParticipant {
            character_id: participant_ids[0],
        });
    }
    participant_ids.sort_unstable();
    if participant_ids != group.members {
        return Err(ProtocolError::GroupMembershipMismatch);
    }

    for participant in participants {
        participant.validate()?;
        let progress =
            participant
                .progress_for(route.from)
                .ok_or(ProtocolError::MissingRegionalProgress {
                    character_id: participant.character_id,
                    region: route.from,
                })?;
        if progress.badge_count() < route.minimum_badges
            || progress.story_checkpoint < route.minimum_story_checkpoint
        {
            return Err(ProtocolError::TravelDenied {
                character_id: participant.character_id,
            });
        }
    }

    *current_zone = destination;
    Ok(())
}

/// The protocol versions accepted by this workspace.
pub const CURRENT_PROTOCOL_VERSION: u16 = 1;
/// The JSON schema version accepted by this workspace.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProtocolVersion {
    V1 = 1,
}

impl ProtocolVersion {
    #[must_use]
    pub const fn wire(self) -> u16 {
        self as u16
    }

    /// Parses a protocol version and rejects unknown versions.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnsupportedProtocolVersion`] for an unknown
    /// version.
    pub const fn from_wire(value: u16) -> Result<Self, ProtocolError> {
        match value {
            CURRENT_PROTOCOL_VERSION => Ok(Self::V1),
            version => Err(ProtocolError::UnsupportedProtocolVersion { version }),
        }
    }
}

impl From<ProtocolVersion> for u16 {
    fn from(value: ProtocolVersion) -> Self {
        value.wire()
    }
}

impl Serialize for ProtocolVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.wire())
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::from_wire(value).map_err(serde::de::Error::custom)
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SchemaVersion {
    V1 = 1,
}

impl SchemaVersion {
    #[must_use]
    pub const fn wire(self) -> u16 {
        self as u16
    }

    /// Parses a schema version and rejects unknown versions.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::UnsupportedSchemaVersion`] for an unknown
    /// version.
    pub const fn from_wire(value: u16) -> Result<Self, ProtocolError> {
        match value {
            CURRENT_SCHEMA_VERSION => Ok(Self::V1),
            version => Err(ProtocolError::UnsupportedSchemaVersion { version }),
        }
    }
}

impl From<SchemaVersion> for u16 {
    fn from(value: SchemaVersion) -> Self {
        value.wire()
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.wire())
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::from_wire(value).map_err(serde::de::Error::custom)
    }
}

/// A compatibility envelope for protocol messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Compatibility {
    pub protocol_version: u16,
    pub schema_version: u16,
}

impl Compatibility {
    /// Creates an envelope from numeric or enum versions.
    ///
    /// # Errors
    ///
    /// Returns an error when either supplied version is unsupported.
    pub fn new<P, S>(protocol_version: P, schema_version: S) -> Result<Self, ProtocolError>
    where
        P: Into<u16>,
        S: Into<u16>,
    {
        let compatibility = Self {
            protocol_version: protocol_version.into(),
            schema_version: schema_version.into(),
        };
        compatibility.validate()?;
        Ok(compatibility)
    }

    #[must_use]
    pub const fn current() -> Self {
        Self {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    ///
    /// # Errors
    ///
    /// Returns an error when either version is unsupported.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match ProtocolVersion::from_wire(self.protocol_version) {
            Ok(_) => {}
            Err(error) => return Err(error),
        }
        match SchemaVersion::from_wire(self.schema_version) {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl<'de> Deserialize<'de> for Compatibility {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireCompatibility {
            protocol_version: u16,
            schema_version: u16,
        }

        let wire = WireCompatibility::deserialize(deserializer)?;
        Self::new(wire.protocol_version, wire.schema_version).map_err(serde::de::Error::custom)
    }
}

/// Name for compatibility envelopes.
pub type ProtocolCompatibility = Compatibility;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    #[error("invalid region token: {value}")]
    InvalidRegion { value: String },
    #[error("unknown region ordinal: {ordinal}")]
    UnknownRegionOrdinal { ordinal: u8 },
    #[error("the region is unspecified")]
    UnspecifiedRegion,
    #[error("invalid qualified identity: {value}")]
    InvalidQualifiedIdentity { value: String },
    #[error("invalid local identity key: {value}")]
    InvalidLocalKey { value: String },
    #[error("expected {expected} identity but received {actual} identity")]
    WrongIdentityKind {
        expected: IdentityKind,
        actual: IdentityKind,
    },
    #[error("invalid map key: {value}")]
    InvalidMapKey { value: String },
    #[error("map {map} belongs to {actual}, expected {expected}")]
    MapRegionMismatch {
        map: String,
        expected: RegionId,
        actual: RegionId,
    },
    #[error("map coordinates {map_group}:{map_number} belong to {actual}, expected {expected}")]
    MapCoordinateRegionMismatch {
        map_group: u16,
        map_number: u16,
        expected: RegionId,
        actual: RegionId,
    },
    #[error("map {map} does not exist in {region}")]
    UnknownMap { region: RegionId, map: String },
    #[error("map coordinates {map_group}:{map_number} do not exist in {region}")]
    UnknownMapCoordinates {
        region: RegionId,
        map_group: u16,
        map_number: u16,
    },
    #[error("identity belongs to {actual}, expected {expected}")]
    IdentityRegionMismatch {
        expected: RegionId,
        actual: RegionId,
    },
    #[error("progress contains a non-canonical collection")]
    NonCanonicalCollection,
    #[error("participants cannot be empty")]
    EmptyParticipants,
    #[error("participant {character_id} is repeated")]
    DuplicateParticipant { character_id: u64 },
    #[error("a participant has duplicate progress for one region")]
    DuplicateRegionalProgress,
    #[error("a participant is missing progress for {region}")]
    MissingRegionalProgress { character_id: u64, region: RegionId },
    #[error("group membership does not match the two participants")]
    GroupMembershipMismatch,
    #[error("group members are not in canonical order")]
    NonCanonicalGroup,
    #[error("route endpoints must be different: {region}")]
    SameRegionRoute { region: RegionId },
    #[error("current zone is in {actual}, route starts in {expected}")]
    SourceZoneMismatch {
        expected: RegionId,
        actual: RegionId,
    },
    #[error("destination zone is in {actual}, route ends in {expected}")]
    DestinationZoneMismatch {
        expected: RegionId,
        actual: RegionId,
    },
    #[error("travel denied for participant {character_id}")]
    TravelDenied { character_id: u64 },
    #[error("unsupported protocol version: {version}")]
    UnsupportedProtocolVersion { version: u16 },
    #[error("unsupported schema version: {version}")]
    UnsupportedSchemaVersion { version: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn badges(count: u8) -> u16 {
        if count == 0 { 0 } else { (1u16 << count) - 1 }
    }

    fn regional(region: RegionId, count: u8) -> RegionalProgress {
        RegionalProgress::new(region, badges(count), u32::from(count), vec![], vec![])
            .expect("test progress is valid")
    }

    fn regional_with_mask(region: RegionId, badge_mask: u16) -> RegionalProgress {
        RegionalProgress::new(region, badge_mask, 8, vec![], vec![])
            .expect("test progress is valid")
    }

    fn participant(id: u64, records: Vec<RegionalProgress>) -> ParticipantProgress {
        ParticipantProgress::new(id, records).expect("test participant is valid")
    }

    #[test]
    fn region_ordinals_are_explicit_and_independent() {
        assert_eq!(RegionId::Unspecified.wire(), 0);
        assert_eq!(RegionId::Hoenn.wire(), 1);
        assert_eq!(RegionId::Kanto.wire(), 2);
        assert_eq!(RegionId::Johto.wire(), 3);
        assert_eq!(RegionId::Sevii.wire(), 4);
        assert_eq!(RegionId::from_wire(4), Ok(RegionId::Sevii));
        assert!(matches!(
            RegionId::from_wire(99),
            Err(ProtocolError::UnknownRegionOrdinal { .. })
        ));
        assert_eq!(
            serde_json::to_string(&RegionId::Kanto).unwrap(),
            "\"KANTO\""
        );
    }

    #[test]
    fn qualified_identities_round_trip_and_reject_confusion() {
        let trainer = TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").unwrap();
        assert_eq!(trainer.as_str(), "HOENN:TRAINER_WALLY_1");
        assert_eq!(TrainerInstanceId::parse(trainer.as_str()).unwrap(), trainer);
        assert_eq!(trainer.region(), RegionId::Hoenn);
        assert_eq!(trainer.local_key(), "TRAINER_WALLY_1");

        for invalid in [
            "TRAINER_WALLY_1",
            "hoenn:TRAINER_WALLY_1",
            "HOENN:trainer_wally_1",
            "MARS:TRAINER_WALLY_1",
            "HOENN:TRAINER_WALLY_1:EXTRA",
            "HOENN:BADGE_STONE",
            "HOENN:TRAINER_",
        ] {
            assert!(
                TrainerInstanceId::parse(invalid).is_err(),
                "{invalid} should fail"
            );
        }
        assert!(BadgeId::parse("HOENN:TRAINER_WALLY_1").is_err());
        let gym = GymId::parse("KANTO:GYM_PEWTER").unwrap();
        assert_eq!(gym.region(), RegionId::Kanto);
        assert!(BadgeId::parse(gym.as_str()).is_err());
        assert!(FlyPointId::parse("HOENN:FLYPOINT_LITTLEROOT").is_err());
        assert_eq!(
            serde_json::from_str::<BadgeId>("\"KANTO:BADGE_BOULDER\"")
                .unwrap()
                .as_str(),
            "KANTO:BADGE_BOULDER"
        );
    }

    #[test]
    fn world_contracts_are_fixed_width_and_deterministic() {
        let location = WorldLocation::new(RegionId::Kanto, 2, 7, -4, 9).unwrap();
        assert_eq!(
            serde_json::to_string(&location).unwrap(),
            r#"{"region":"KANTO","map_group":2,"map_number":7,"x":-4,"y":9}"#
        );
        let zone = WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).unwrap();
        assert_eq!(
            serde_json::to_string(&zone).unwrap(),
            r#"{"region":"KANTO","map":"PALLET_TOWN","channel":1}"#
        );
        assert!(WorldZone::new(RegionId::Unspecified, "PALLET_TOWN", 1).is_err());
        assert!(WorldZone::new(RegionId::Kanto, "pallet_town", 1).is_err());
        assert!(matches!(
            WorldZone::new(RegionId::Kanto, "LITTLEROOT_TOWN", 1),
            Err(ProtocolError::MapRegionMismatch { .. })
        ));

        let sevii_zone = WorldZone::new(RegionId::Sevii, "ONE_ISLAND", 2).unwrap();
        let location = sevii_zone.to_location(-3, 8).unwrap();
        assert_eq!((location.map_group, location.map_number), (37, 12));
        assert_eq!(WorldZone::from_location(&location, 2).unwrap(), sevii_zone);
        assert!(matches!(
            WorldZone::new(RegionId::Hoenn, "NOT_A_MAP", 1),
            Err(ProtocolError::UnknownMap { .. })
        ));
    }

    #[test]
    fn progress_normalizes_same_region_collections() {
        let trainer = TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").unwrap();
        let fly_point = FlyPointId::new(RegionId::Hoenn, "FLY_LITTLEROOT").unwrap();
        let progress = RegionalProgress::new(
            RegionId::Hoenn,
            badges(3),
            42,
            vec![trainer.clone(), trainer.clone()],
            vec![fly_point.clone(), fly_point],
        )
        .unwrap();
        assert_eq!(progress.defeated_trainers, vec![trainer]);
        assert_eq!(progress.unlocked_fly_points.len(), 1);
        assert!(progress.validate().is_ok());
        assert!(
            RegionalProgress::new(
                RegionId::Kanto,
                0,
                0,
                vec![TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").unwrap()],
                vec![],
            )
            .is_err()
        );
    }

    #[test]
    fn badge_entitlements_use_only_the_canonical_low_eight_bits() {
        let high_only = regional_with_mask(RegionId::Hoenn, 0xFF00);
        let wire = serde_json::to_string(&high_only).unwrap();
        let decoded: RegionalProgress = serde_json::from_str(&wire).unwrap();
        assert_eq!(decoded.badge_mask, 0xFF00);
        assert_eq!(high_only.badge_count(), 0);
        assert!(!high_only.has_badges(0x0100));

        let low_and_high = regional_with_mask(RegionId::Hoenn, 0xFF07);
        assert_eq!(low_and_high.badge_count(), 3);
        assert!(low_and_high.has_badges(0x0007));
        assert!(!low_and_high.has_badges(0x0107));
    }

    #[test]
    fn group_tier_and_travel_ignore_reserved_badge_bits() {
        let first = participant(10, vec![regional_with_mask(RegionId::Hoenn, 0xFF07)]);
        let second = participant(11, vec![regional_with_mask(RegionId::Hoenn, 0xFF03)]);
        assert_eq!(
            group_battle_tier(&[first.clone(), second.clone()], RegionId::Hoenn),
            Ok(2)
        );

        let high_only = [
            participant(10, vec![regional_with_mask(RegionId::Hoenn, 0xFF00)]),
            participant(11, vec![regional_with_mask(RegionId::Hoenn, 0xFF00)]),
        ];
        let group = Group::new(10, 11).unwrap();
        let mut source = WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        let original_source = source.clone();
        let destination = WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).unwrap();
        let route = TravelRoute::new(RegionId::Hoenn, RegionId::Kanto, 1, 0).unwrap();
        assert!(
            group
                .transfer(&mut source, destination, &high_only, &route)
                .is_err()
        );
        assert_eq!(source, original_source);
    }

    #[test]
    fn battle_tier_uses_the_minimum_in_the_battle_region() {
        let esteban = participant(
            10,
            vec![regional(RegionId::Hoenn, 8), regional(RegionId::Kanto, 3)],
        );
        let luis = participant(
            11,
            vec![regional(RegionId::Hoenn, 5), regional(RegionId::Kanto, 1)],
        );
        assert_eq!(
            group_battle_tier(&[esteban.clone(), luis.clone()], RegionId::Kanto),
            Ok(1)
        );
        assert_eq!(group_battle_tier(&[esteban, luis], RegionId::Hoenn), Ok(5));
        let missing_kanto = participant(12, vec![regional(RegionId::Hoenn, 8)]);
        assert!(matches!(
            group_battle_tier(&[missing_kanto], RegionId::Kanto),
            Err(ProtocolError::MissingRegionalProgress {
                region: RegionId::Kanto,
                ..
            })
        ));

        let duplicate_progress = r#"{
            "character_id":13,
            "regional_progress":[
                {"region":"KANTO","badge_mask":255,"story_checkpoint":8,"defeated_trainers":[],"unlocked_fly_points":[]},
                {"region":"KANTO","badge_mask":0,"story_checkpoint":0,"defeated_trainers":[],"unlocked_fly_points":[]}
            ]
        }"#;
        assert!(serde_json::from_str::<ParticipantProgress>(duplicate_progress).is_err());
    }

    #[test]
    fn group_is_symmetric_and_travel_is_atomic() {
        let group = Group::new(11, 10).unwrap();
        assert_eq!(group.members(), [10, 11]);
        let group_json = serde_json::to_string(&group).unwrap();
        assert_eq!(group_json, r#"{"members":[10,11]}"#);
        assert_eq!(serde_json::from_str::<Group>(&group_json).unwrap(), group);
        assert_eq!(
            serde_json::from_str::<Group>(r#"{"members":[11,10]}"#)
                .unwrap()
                .members(),
            [10, 11]
        );
        assert_eq!(
            Group { members: [11, 10] }.validate(),
            Err(ProtocolError::NonCanonicalGroup)
        );
        assert_eq!(
            Group::new(10, 10),
            Err(ProtocolError::DuplicateParticipant { character_id: 10 })
        );

        let route = TravelRoute::new(RegionId::Hoenn, RegionId::Kanto, 8, 8).unwrap();
        let mut source = WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        let destination = WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).unwrap();
        let entitled = [
            participant(10, vec![regional(RegionId::Hoenn, 8)]),
            participant(11, vec![regional(RegionId::Hoenn, 8)]),
        ];
        group
            .transfer(&mut source, destination.clone(), &entitled, &route)
            .unwrap();
        assert_eq!(source, destination);

        let mut denied_source = WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        let denied_destination = WorldZone::new(RegionId::Kanto, "PALLET_TOWN", 1).unwrap();
        let denied = [
            participant(10, vec![regional(RegionId::Hoenn, 8)]),
            participant(11, vec![regional(RegionId::Hoenn, 7)]),
        ];
        assert!(
            group
                .transfer(&mut denied_source, denied_destination, &denied, &route)
                .is_err()
        );
        assert_eq!(denied_source.region, RegionId::Hoenn);
        assert_eq!(denied_source.map, "LITTLEROOT_TOWN");
        assert_eq!(group.members(), [10, 11]);

        let route_json = serde_json::to_string(&route).unwrap();
        assert_eq!(
            serde_json::from_str::<TravelRoute>(&route_json).unwrap(),
            route
        );
        assert!(
            serde_json::from_str::<TravelRoute>(
                r#"{"from":"HOENN","to":"HOENN","minimum_badges":0,"minimum_story_checkpoint":0}"#,
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<TravelRoute>(
                r#"{"from":"UNSPECIFIED","to":"KANTO","minimum_badges":0,"minimum_story_checkpoint":0}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn compatibility_rejects_unknown_versions() {
        assert_eq!(
            Compatibility::current(),
            Compatibility::new(1u16, 1u16).unwrap()
        );
        assert!(Compatibility::new(2u16, 1u16).is_err());
        assert!(Compatibility::new(1u16, 2u16).is_err());
        assert_eq!(
            serde_json::to_string(&Compatibility::current()).unwrap(),
            r#"{"protocol_version":1,"schema_version":1}"#
        );
    }
}

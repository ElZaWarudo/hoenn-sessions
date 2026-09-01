//! Character state and exclusive lease fencing contracts.

use coop_protocol::{FlyPointId, RegionId, RegionalProgress, TrainerInstanceId, WorldZone};
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};
use thiserror::Error;

use crate::{
    ApiVersion, CharacterId, ClientInstanceId, IdempotencyKey, Revision, SessionEpoch, SessionId,
    UnixTimestampMillis, ids::IdError, ids::deserialize_bounded_string,
};

/// Errors produced while validating character state or leases.
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum SessionError {
    #[error("character state has no progress for its active region")]
    MissingActiveRegion,
    #[error("regional progress records contain a duplicate region")]
    DuplicateRegionalProgress,
    #[error("regional progress records are not in canonical region order")]
    NonCanonicalRegionalOrder,
    #[error("at most four regional progress records are allowed")]
    TooManyRegions,
    #[error("a regional progress collection exceeds its bound")]
    TooManyRegionalEntries,
    #[error("regional progress is invalid: {0}")]
    InvalidRegionalProgress(String),
    #[error("regional progress entries are not in canonical order")]
    NonCanonicalRegionalEntries,
    #[error("lease expiry must be a non-zero timestamp")]
    InvalidExpiry,
    #[error("heartbeat interval must be between 1 and 600000 milliseconds")]
    InvalidHeartbeatInterval,
    #[error("session epoch is invalid: {0}")]
    InvalidEpoch(#[from] IdError),
    #[error("API version is invalid")]
    InvalidApiVersion,
}

/// A character's region-safe cloud state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterCloudState {
    pub character_id: CharacterId,
    pub world_zone: WorldZone,
    pub regional_progress: Vec<RegionalProgress>,
}

#[derive(Serialize)]
struct SerializableCharacterCloudState<'a> {
    character_id: CharacterId,
    world_zone: &'a WorldZone,
    regional_progress: &'a [RegionalProgress],
}

impl Serialize for CharacterCloudState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableCharacterCloudState {
            character_id: self.character_id,
            world_zone: &self.world_zone,
            regional_progress: &self.regional_progress,
        }
        .serialize(serializer)
    }
}

impl CharacterCloudState {
    /// Constructs state with one canonical record per concrete region.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid zones, duplicate or invalid regional
    /// records, or a missing active-region record.
    pub fn new(
        character_id: CharacterId,
        world_zone: WorldZone,
        mut regional_progress: Vec<RegionalProgress>,
    ) -> Result<Self, SessionError> {
        world_zone
            .validate()
            .map_err(|error| SessionError::InvalidRegionalProgress(error.to_string()))?;
        if world_zone.map.len() > 128 {
            return Err(SessionError::InvalidRegionalProgress(
                "world-zone map exceeds 128 bytes".to_owned(),
            ));
        }
        if regional_progress.len() > 4 {
            return Err(SessionError::TooManyRegions);
        }
        regional_progress.sort_unstable_by_key(|progress| progress.region.wire());
        if regional_progress
            .windows(2)
            .any(|window| window[0].region == window[1].region)
        {
            return Err(SessionError::DuplicateRegionalProgress);
        }
        for progress in &regional_progress {
            if progress.defeated_trainers.len() > 4096 || progress.unlocked_fly_points.len() > 256 {
                return Err(SessionError::TooManyRegionalEntries);
            }
            progress
                .validate()
                .map_err(|error| SessionError::InvalidRegionalProgress(error.to_string()))?;
        }
        if !regional_progress
            .iter()
            .any(|progress| progress.region == world_zone.region)
        {
            return Err(SessionError::MissingActiveRegion);
        }
        Ok(Self {
            character_id,
            world_zone,
            regional_progress,
        })
    }

    /// Validates canonical ordering and the active-region requirement.
    ///
    /// # Errors
    ///
    /// Returns an error when the state violates any regional invariant.
    pub fn validate(&self) -> Result<(), SessionError> {
        self.world_zone
            .validate()
            .map_err(|error| SessionError::InvalidRegionalProgress(error.to_string()))?;
        if self.world_zone.map.len() > 128 {
            return Err(SessionError::InvalidRegionalProgress(
                "world-zone map exceeds 128 bytes".to_owned(),
            ));
        }
        if self.regional_progress.len() > 4 {
            return Err(SessionError::TooManyRegions);
        }
        if self
            .regional_progress
            .windows(2)
            .any(|window| window[0].region >= window[1].region)
        {
            return Err(SessionError::NonCanonicalRegionalOrder);
        }
        for progress in &self.regional_progress {
            if progress.defeated_trainers.len() > 4096 || progress.unlocked_fly_points.len() > 256 {
                return Err(SessionError::TooManyRegionalEntries);
            }
            progress
                .validate()
                .map_err(|error| SessionError::InvalidRegionalProgress(error.to_string()))?;
        }
        if !self
            .regional_progress
            .iter()
            .any(|progress| progress.region == self.world_zone.region)
        {
            return Err(SessionError::MissingActiveRegion);
        }
        Ok(())
    }

    #[must_use]
    pub fn progress_for(&self, region: RegionId) -> Option<&RegionalProgress> {
        self.regional_progress
            .iter()
            .find(|progress| progress.region == region)
    }

    /// Returns only the active-region badge count. Other regions are ignored.
    ///
    /// # Errors
    ///
    /// Returns an error when the active region has no progress record.
    pub fn active_region_badge_tier(&self) -> Result<u8, SessionError> {
        self.progress_for(self.world_zone.region)
            .map(RegionalProgress::badge_count)
            .ok_or(SessionError::MissingActiveRegion)
    }

    #[must_use]
    pub fn active_region(&self) -> RegionId {
        self.world_zone.region
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCharacterCloudState {
    character_id: CharacterId,
    world_zone: WireWorldZone,
    #[serde(deserialize_with = "deserialize_regional_progress")]
    regional_progress: Vec<WireRegionalProgress>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWorldZone {
    region: RegionId,
    #[serde(deserialize_with = "deserialize_map_key")]
    map: String,
    channel: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRegionalProgress {
    region: RegionId,
    badge_mask: u16,
    story_checkpoint: u32,
    #[serde(deserialize_with = "deserialize_trainers")]
    defeated_trainers: Vec<TrainerInstanceId>,
    #[serde(deserialize_with = "deserialize_fly_points")]
    unlocked_fly_points: Vec<FlyPointId>,
}

fn deserialize_bounded_vec<'de, D, T>(
    deserializer: D,
    maximum: usize,
    description: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedVisitor<T> {
        maximum: usize,
        description: &'static str,
        marker: PhantomData<fn() -> T>,
    }

    impl<'de, T> Visitor<'de> for BoundedVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "{} with at most {} items",
                self.description, self.maximum
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.maximum));
            while let Some(value) = sequence.next_element()? {
                if values.len() == self.maximum {
                    return Err(de::Error::custom(format!(
                        "{} exceeds {} items",
                        self.description, self.maximum
                    )));
                }
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(BoundedVisitor {
        maximum,
        description,
        marker: PhantomData,
    })
}

fn deserialize_regional_progress<'de, D>(
    deserializer: D,
) -> Result<Vec<WireRegionalProgress>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, 4, "regional progress")
}

fn deserialize_trainers<'de, D>(deserializer: D) -> Result<Vec<TrainerInstanceId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_bounded_identity_vec(
        deserializer,
        4096,
        TrainerInstanceId::parse,
        "defeated trainer identity",
    )
}

fn deserialize_fly_points<'de, D>(deserializer: D) -> Result<Vec<FlyPointId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_bounded_identity_vec(
        deserializer,
        256,
        FlyPointId::parse,
        "unlocked fly-point identity",
    )
}

fn deserialize_map_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_bounded_string(deserializer, 128, "world-zone map")
}

fn deserialize_qualified_identity<'de, D, T>(
    deserializer: D,
    parse: fn(&str) -> Result<T, coop_protocol::ProtocolError>,
    description: &'static str,
) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct IdentityVisitor<T> {
        parse: fn(&str) -> Result<T, coop_protocol::ProtocolError>,
        description: &'static str,
    }

    impl<'de, T> Visitor<'de> for IdentityVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "{} of at most 128 bytes", self.description)
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > 128 {
                return Err(E::custom(format!("{} exceeds 128 bytes", self.description)));
            }
            (self.parse)(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > 128 {
                return Err(E::custom(format!("{} exceeds 128 bytes", self.description)));
            }
            (self.parse)(value).map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > 128 {
                return Err(E::custom(format!("{} exceeds 128 bytes", self.description)));
            }
            (self.parse)(&value).map_err(E::custom)
        }
    }

    deserializer.deserialize_string(IdentityVisitor { parse, description })
}

fn deserialize_bounded_identity_vec<'de, D, T>(
    deserializer: D,
    maximum: usize,
    parse: fn(&str) -> Result<T, coop_protocol::ProtocolError>,
    description: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct IdentitySeed<T> {
        parse: fn(&str) -> Result<T, coop_protocol::ProtocolError>,
        description: &'static str,
    }

    impl<'de, T> serde::de::DeserializeSeed<'de> for IdentitySeed<T> {
        type Value = T;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserialize_qualified_identity(deserializer, self.parse, self.description)
        }
    }

    struct IdentitiesVisitor<T> {
        maximum: usize,
        parse: fn(&str) -> Result<T, coop_protocol::ProtocolError>,
        description: &'static str,
        marker: PhantomData<fn() -> T>,
    }

    impl<'de, T> Visitor<'de> for IdentitiesVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "{} with at most {} items",
                self.description, self.maximum
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.maximum));
            while let Some(value) = sequence.next_element_seed(IdentitySeed {
                parse: self.parse,
                description: self.description,
            })? {
                if values.len() == self.maximum {
                    return Err(de::Error::custom(format!(
                        "{} exceeds {} items",
                        self.description, self.maximum
                    )));
                }
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(IdentitiesVisitor {
        maximum,
        parse,
        description,
        marker: PhantomData,
    })
}

impl<'de> Deserialize<'de> for CharacterCloudState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireCharacterCloudState::deserialize(deserializer)?;
        let progress = wire
            .regional_progress
            .into_iter()
            .map(|record| {
                if record
                    .defeated_trainers
                    .windows(2)
                    .any(|window| window[0] >= window[1])
                    || record
                        .unlocked_fly_points
                        .windows(2)
                        .any(|window| window[0] >= window[1])
                {
                    return Err(serde::de::Error::custom(
                        SessionError::NonCanonicalRegionalEntries,
                    ));
                }
                RegionalProgress::new(
                    record.region,
                    record.badge_mask,
                    record.story_checkpoint,
                    record.defeated_trainers,
                    record.unlocked_fly_points,
                )
                .map_err(serde::de::Error::custom)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if progress.len() > 1
            && progress
                .windows(2)
                .any(|window| window[0].region >= window[1].region)
        {
            return Err(serde::de::Error::custom(
                SessionError::NonCanonicalRegionalOrder,
            ));
        }
        let world_zone = WorldZone::new(
            wire.world_zone.region,
            wire.world_zone.map,
            wire.world_zone.channel,
        )
        .map_err(serde::de::Error::custom)?;
        Self::new(wire.character_id, world_zone, progress).map_err(serde::de::Error::custom)
    }
}

/// The fencing data required by every lease operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseFence {
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl LeaseFence {
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        character_id: CharacterId,
        current_revision: Revision,
        session_epoch: SessionEpoch,
        client_instance_id: ClientInstanceId,
    ) -> Self {
        Self {
            session_id,
            character_id,
            current_revision,
            session_epoch,
            client_instance_id,
        }
    }
}

/// Server-owned active lease descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseContract {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub expires_at: UnixTimestampMillis,
    pub heartbeat_interval_ms: u32,
    pub client_instance_id: ClientInstanceId,
}

#[derive(Serialize)]
struct SerializableLeaseContract {
    api_version: ApiVersion,
    session_id: SessionId,
    character_id: CharacterId,
    current_revision: Revision,
    session_epoch: SessionEpoch,
    expires_at: UnixTimestampMillis,
    heartbeat_interval_ms: u32,
    client_instance_id: ClientInstanceId,
}

impl Serialize for LeaseContract {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.validate().map_err(serde::ser::Error::custom)?;
        SerializableLeaseContract {
            api_version: self.api_version,
            session_id: self.session_id,
            character_id: self.character_id,
            current_revision: self.current_revision,
            session_epoch: self.session_epoch,
            expires_at: self.expires_at,
            heartbeat_interval_ms: self.heartbeat_interval_ms,
            client_instance_id: self.client_instance_id,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireLeaseContract {
    api_version: ApiVersion,
    session_id: SessionId,
    character_id: CharacterId,
    current_revision: Revision,
    session_epoch: SessionEpoch,
    expires_at: UnixTimestampMillis,
    heartbeat_interval_ms: u32,
    client_instance_id: ClientInstanceId,
}

impl<'de> Deserialize<'de> for LeaseContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireLeaseContract::deserialize(deserializer)?;
        let contract = Self {
            api_version: wire.api_version,
            session_id: wire.session_id,
            character_id: wire.character_id,
            current_revision: wire.current_revision,
            session_epoch: wire.session_epoch,
            expires_at: wire.expires_at,
            heartbeat_interval_ms: wire.heartbeat_interval_ms,
            client_instance_id: wire.client_instance_id,
        };
        contract.validate().map_err(serde::de::Error::custom)?;
        Ok(contract)
    }
}

impl LeaseContract {
    /// Constructs a server-issued lease descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error for zero expiry or an out-of-range heartbeat interval.
    pub fn new(
        fence: LeaseFence,
        expires_at: UnixTimestampMillis,
        heartbeat_interval_ms: u32,
    ) -> Result<Self, SessionError> {
        if expires_at.value() == 0 {
            return Err(SessionError::InvalidExpiry);
        }
        if !(1..=600_000).contains(&heartbeat_interval_ms) {
            return Err(SessionError::InvalidHeartbeatInterval);
        }
        Ok(Self {
            api_version: ApiVersion::V1,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            expires_at,
            heartbeat_interval_ms,
            client_instance_id: fence.client_instance_id,
        })
    }

    /// Revalidates the server lease descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error when its API, epoch, expiry, or heartbeat interval is invalid.
    pub fn validate(&self) -> Result<(), SessionError> {
        ApiVersion::new(self.api_version.value()).map_err(|_| SessionError::InvalidApiVersion)?;
        SessionEpoch::new(self.session_epoch.value())?;
        if self.expires_at.value() == 0 {
            return Err(SessionError::InvalidExpiry);
        }
        if !(1..=600_000).contains(&self.heartbeat_interval_ms) {
            return Err(SessionError::InvalidHeartbeatInterval);
        }
        Ok(())
    }

    #[must_use]
    pub fn fence(&self) -> LeaseFence {
        LeaseFence::new(
            self.session_id,
            self.character_id,
            self.current_revision,
            self.session_epoch,
            self.client_instance_id,
        )
    }
}

/// Acquire an exclusive character lease. Expiry remains server-owned.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquireLeaseRequest {
    pub api_version: ApiVersion,
    pub character_id: CharacterId,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl AcquireLeaseRequest {
    #[must_use]
    pub const fn new(
        character_id: CharacterId,
        client_instance_id: ClientInstanceId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            api_version: ApiVersion::V1,
            character_id,
            client_instance_id,
            idempotency_key,
        }
    }

    /// Returns the operation key used to make an acquire retry-safe.
    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }
}

/// Heartbeat for an already acquired lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatLeaseRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
}

impl HeartbeatLeaseRequest {
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
}

/// Reconnect request with the same stale-client fencing data as heartbeat.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconnectLeaseRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl ReconnectLeaseRequest {
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

    /// Returns the operation identity used to make epoch rotation retry-safe.
    /// The server records the request fingerprint with this key: an exact
    /// retry returns the same rotated lease, a changed fingerprint conflicts,
    /// and any other key presented with the consumed prior epoch is stale.
    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }
}

/// Release request carrying fencing data and an idempotency key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLeaseRequest {
    pub api_version: ApiVersion,
    pub session_id: SessionId,
    pub character_id: CharacterId,
    pub current_revision: Revision,
    pub session_epoch: SessionEpoch,
    pub client_instance_id: ClientInstanceId,
    pub idempotency_key: IdempotencyKey,
}

impl ReleaseLeaseRequest {
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
}

/// Aliases for clients that omit the “Lease” infix.
pub type AcquireRequest = AcquireLeaseRequest;
pub type HeartbeatRequest = HeartbeatLeaseRequest;
pub type ReconnectRequest = ReconnectLeaseRequest;
pub type ReleaseRequest = ReleaseLeaseRequest;

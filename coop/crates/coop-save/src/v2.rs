//! Schema-two validation and an inactive, crate-private sector writer.
//!
//! Schema two keeps the version-one fields byte-identical and consumes the
//! former reserved area for a fifth, Cormoria progress record.  This module
//! deliberately keeps projection private to this crate until transfer
//! ownership and rekey rules are complete.

use coop_protocol::{
    IdentityKind, RegionId,
    identity_catalog::{resolve_badge_bit, resolve_ordinal},
};
use thiserror::Error;

use super::{
    COOP_CRC_OFFSET, COOP_EVENT_BITS_OFFSET, COOP_EVENT_BITS_SIZE, COOP_FLY_BITS_OFFSET,
    COOP_FLY_BITS_SIZE, COOP_GENERATION_OFFSET, COOP_GYM_BITS_OFFSET, COOP_GYM_BITS_SIZE,
    COOP_MAGIC_OFFSET, COOP_REGIONAL_PROGRESS_OFFSET, COOP_REGIONAL_PROGRESS_SIZE,
    COOP_REGISTRY_DIGEST_OFFSET, COOP_REGISTRY_VERSION_OFFSET, COOP_SAVE_BADGE_MASK,
    COOP_SAVE_V1_MAGIC, COOP_SCHEMA_OFFSET, COOP_STATUS_FLAGS_OFFSET, COOP_STRUCT_SIZE_OFFSET,
    COOP_TRAINER_BITS_OFFSET, COOP_TRAINER_BITS_SIZE, RegionalProgress, RegistryContract,
    read_array, read_u16, read_u32,
};

/// Schema-two payload size, unchanged from schema one.
pub const COOP_SAVE_V2_SIZE: usize = 672;
/// Schema-two wire version.
pub const COOP_SAVE_V2_SCHEMA_VERSION: u16 = 2;
/// Cormoria's fifth progress record offset.
pub const COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET: usize = 604;
/// Start of the schema-two zero-filled tail.
pub const COOP_SAVE_V2_RESERVED_TAIL_OFFSET: usize = 612;
/// Length of the schema-two zero-filled tail.
pub const COOP_SAVE_V2_RESERVED_TAIL_SIZE: usize = 56;
/// Status bit set only after every legacy Pokémon met-location is normalized.
pub const COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED: u32 = 1 << 1;
/// Status bits understood by schema two.
pub const COOP_SAVE_V2_STATUS_KNOWN_MASK: u32 =
    super::COOP_SAVE_STATUS_KNOWN_MASK | COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED;

/// A byte-for-byte retained image whose ROM-selected slot contains a valid
/// schema-two co-op extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedSaveV2 {
    raw: Box<[u8]>,
    selected_slot: super::SaveSlot,
    counter: u32,
    logical_sector_offsets: [usize; super::SECTORS_PER_SLOT],
    save_block3: [u8; super::SAVE_BLOCK3_CAPACITY],
    character_lineage: super::CharacterLineage,
    coop: CoopSaveV2,
}

impl ValidatedSaveV2 {
    /// Original bytes, including an optional RTC trailer.
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Exact 128 KiB `Flash1M` image.
    #[must_use]
    pub fn flash_bytes(&self) -> &[u8] {
        &self.raw[..super::FLASH_IMAGE_SIZE]
    }

    /// Original optional mGBA RTC trailer.
    #[must_use]
    pub fn rtc_trailer(&self) -> Option<&[u8; super::RTC_TRAILER_SIZE]> {
        self.raw
            .get(super::FLASH_IMAGE_SIZE..)
            .and_then(|trailer| trailer.try_into().ok())
    }

    /// Consumes the validated representation without changing a byte.
    #[must_use]
    pub fn into_raw_bytes(self) -> Box<[u8]> {
        self.raw
    }

    /// Selected physical slot.
    #[must_use]
    pub const fn selected_slot(&self) -> super::SaveSlot {
        self.selected_slot
    }

    /// Uniform save counter from the selected slot.
    #[must_use]
    pub const fn counter(&self) -> u32 {
        self.counter
    }

    /// Checksummed payload for a logical sector in the ROM-selected slot.
    #[must_use]
    pub fn logical_sector_payload(&self, logical_id: u8) -> Option<&[u8]> {
        let logical_index = usize::from(logical_id);
        let sector_offset = *self.logical_sector_offsets.get(logical_index)?;
        let payload_size = *super::LOGICAL_SECTOR_DATA_SIZES.get(logical_index)?;
        let payload_end = sector_offset.checked_add(payload_size)?;
        self.raw.get(sector_offset..payload_end)
    }

    /// `SaveBlock3` chunks reassembled by logical sector ID.
    #[must_use]
    pub const fn save_block3(&self) -> &[u8; super::SAVE_BLOCK3_CAPACITY] {
        &self.save_block3
    }

    /// In-game identity carried by the selected committed save slot.
    #[must_use]
    pub const fn character_lineage(&self) -> super::CharacterLineage {
        self.character_lineage
    }

    /// Validated schema-two cloud extension.
    #[must_use]
    pub const fn coop(&self) -> &CoopSaveV2 {
        &self.coop
    }

    /// Clone this image and patch only explicitly approved, checksummed bytes
    /// of its ROM-selected logical sectors. This is a low-level projection
    /// primitive; it does not decide field ownership or authorize travel.
    ///
    /// The caller must supply an audited allowlist. Sector footers, the other
    /// slot, SaveBlock3, and the optional RTC trailer are never patch targets.
    /// The result is reparsed and must retain the destination character and
    /// schema-two co-op state.
    ///
    /// # Errors
    ///
    /// Rejects empty, overlapping, out-of-range or unapproved patches, and
    /// any result that fails full save validation or changes protected state.
    #[allow(dead_code)] // Dormant until the audited transfer adapter exists.
    pub(crate) fn project_selected_sectors(
        &self,
        approved: &[ApprovedSectorSpan],
        patches: &[SelectedSectorPatch<'_>],
    ) -> Result<Self, SectorProjectionError> {
        if patches.is_empty() || approved.is_empty() {
            return Err(SectorProjectionError::InvalidPatch);
        }
        let mut touched = [false; super::SECTORS_PER_SLOT];
        let mut ranges = Vec::with_capacity(patches.len());
        for patch in patches {
            let logical = usize::from(patch.logical_id);
            let Some(&payload_size) = super::LOGICAL_SECTOR_DATA_SIZES.get(logical) else {
                return Err(SectorProjectionError::InvalidPatch);
            };
            let Some(end) = patch.offset.checked_add(patch.bytes.len()) else {
                return Err(SectorProjectionError::InvalidPatch);
            };
            if patch.bytes.is_empty() || end > payload_size {
                return Err(SectorProjectionError::InvalidPatch);
            }
            let authorized = approved.iter().any(|span| {
                span.logical_id == patch.logical_id
                    && span.offset <= patch.offset
                    && span
                        .offset
                        .checked_add(span.len)
                        .is_some_and(|limit| end <= limit && limit <= payload_size)
            });
            if !authorized
                || ranges.iter().any(|&(id, start, stop)| {
                    id == patch.logical_id && patch.offset < stop && start < end
                })
            {
                return Err(SectorProjectionError::UnapprovedOrOverlapping);
            }
            ranges.push((patch.logical_id, patch.offset, end));
            touched[logical] = true;
        }

        let mut raw = self.raw.to_vec();
        for patch in patches {
            let sector = self.logical_sector_offsets[usize::from(patch.logical_id)];
            let start = sector + patch.offset;
            raw[start..start + patch.bytes.len()].copy_from_slice(patch.bytes);
        }
        for (logical, changed) in touched.into_iter().enumerate() {
            if changed {
                let sector = self.logical_sector_offsets[logical];
                let checksum = super::sector_checksum(
                    &raw[sector..sector + super::LOGICAL_SECTOR_DATA_SIZES[logical]],
                );
                raw[sector + super::SECTOR_CHECKSUM_OFFSET
                    ..sector + super::SECTOR_CHECKSUM_OFFSET + 2]
                    .copy_from_slice(&checksum.to_le_bytes());
            }
        }

        let reparsed = parse_v2(&raw, self.coop.registry)?;
        if reparsed.selected_slot != self.selected_slot
            || reparsed.counter != self.counter
            || reparsed.logical_sector_offsets != self.logical_sector_offsets
            || !reparsed
                .character_lineage
                .same_trainer(self.character_lineage)
            || reparsed.coop != self.coop
        {
            return Err(SectorProjectionError::ProtectedStateChanged);
        }
        Ok(reparsed)
    }

    /// Copy one already validated co-op record into this image's selected
    /// SaveBlock3 chunks. Only a journaled caller may decide that `source` is
    /// authoritative; this primitive does not make that decision or write a
    /// file. The source CRC is already sealed by `parse_v2` and is validated
    /// again after the logical chunks are mapped into destination sectors.
    pub(crate) fn project_coop_from(&self, source: &Self) -> Result<Self, SectorProjectionError> {
        if !self
            .character_lineage
            .same_trainer(source.character_lineage)
            || self.coop.registry != source.coop.registry
        {
            return Err(SectorProjectionError::SourceMismatch);
        }
        let mut raw = self.raw.to_vec();
        let start = super::COOP_SAVE_OFFSET;
        for (index, &byte) in source.save_block3[start..start + COOP_SAVE_V2_SIZE]
            .iter()
            .enumerate()
        {
            let save_block3_offset = start + index;
            let logical = save_block3_offset / super::SAVE_BLOCK3_CHUNK_SIZE;
            let chunk_offset = save_block3_offset % super::SAVE_BLOCK3_CHUNK_SIZE;
            let sector = self.logical_sector_offsets[logical];
            raw[sector + super::SAVE_BLOCK3_CHUNK_OFFSET + chunk_offset] = byte;
        }
        let reparsed = parse_v2(&raw, self.coop.registry)?;
        if reparsed.selected_slot != self.selected_slot
            || reparsed.counter != self.counter
            || reparsed.logical_sector_offsets != self.logical_sector_offsets
            || reparsed.character_lineage != self.character_lineage
            || reparsed.coop != source.coop
        {
            return Err(SectorProjectionError::ProtectedStateChanged);
        }
        Ok(reparsed)
    }

    /// Bind a newly created destination-world save template to the source
    /// trainer before the first shared-player projection. The travel
    /// coordinator must prove that this template has never been active for a
    /// character; this primitive does not make that authority decision.
    /// Existing destination saves must go straight to shared projection.
    /// Only the selected slot's stable name, gender, and trainer ID change.
    #[allow(dead_code)] // Dormant until the one-time travel coordinator fences first arrival.
    pub(crate) fn bind_fresh_template_trainer(
        &self,
        source: &Self,
    ) -> Result<Self, SectorProjectionError> {
        if self.coop.registry != source.coop.registry {
            return Err(SectorProjectionError::SourceMismatch);
        }
        let source_identity = source.character_lineage;
        let mut raw = self.raw.to_vec();
        let sector = self.logical_sector_offsets[0];
        raw[sector + super::PLAYER_NAME_OFFSET
            ..sector + super::PLAYER_NAME_OFFSET + super::PLAYER_NAME_SIZE]
            .copy_from_slice(&source_identity.player_name);
        raw[sector + super::PLAYER_GENDER_OFFSET] = source_identity.player_gender;
        raw[sector + super::PLAYER_TRAINER_ID_OFFSET
            ..sector + super::PLAYER_TRAINER_ID_OFFSET + super::PLAYER_TRAINER_ID_SIZE]
            .copy_from_slice(&source_identity.player_trainer_id);
        let checksum =
            super::sector_checksum(&raw[sector..sector + super::LOGICAL_SECTOR_DATA_SIZES[0]]);
        raw[sector + super::SECTOR_CHECKSUM_OFFSET..sector + super::SECTOR_CHECKSUM_OFFSET + 2]
            .copy_from_slice(&checksum.to_le_bytes());
        let reparsed = parse_v2(&raw, self.coop.registry)?;
        if reparsed.selected_slot != self.selected_slot
            || reparsed.counter != self.counter
            || reparsed.logical_sector_offsets != self.logical_sector_offsets
            || reparsed.coop != self.coop
            || !reparsed.character_lineage.same_trainer(source_identity)
            || reparsed.character_lineage.player_region != self.character_lineage.player_region
        {
            return Err(SectorProjectionError::ProtectedStateChanged);
        }
        Ok(reparsed)
    }

    /// Advance the projected co-op checkpoint generation exactly once for a
    /// server-owned world handoff. The caller must first project the active
    /// source's shared state, then submit this result under the same fenced
    /// handoff; this method does not authorize either operation.
    #[allow(dead_code)] // Dormant until the fenced handoff uses this primitive.
    pub(crate) fn advance_transfer_generation(&self) -> Result<Self, SectorProjectionError> {
        let generation = self
            .coop
            .save_generation
            .checked_add(1)
            .ok_or(SectorProjectionError::GenerationExhausted)?;
        let start = super::COOP_SAVE_OFFSET;
        let mut record = self.save_block3[start..start + COOP_SAVE_V2_SIZE].to_vec();
        record[COOP_GENERATION_OFFSET..COOP_GENERATION_OFFSET + 4]
            .copy_from_slice(&generation.to_le_bytes());
        let crc = crc32fast::hash(&record[..COOP_CRC_OFFSET]);
        record[COOP_CRC_OFFSET..COOP_CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
        let mut raw = self.raw.to_vec();
        for (index, &byte) in record.iter().enumerate() {
            let save_block3_offset = start + index;
            let logical = save_block3_offset / super::SAVE_BLOCK3_CHUNK_SIZE;
            let chunk_offset = save_block3_offset % super::SAVE_BLOCK3_CHUNK_SIZE;
            let sector = self.logical_sector_offsets[logical];
            raw[sector + super::SAVE_BLOCK3_CHUNK_OFFSET + chunk_offset] = byte;
        }
        let reparsed = parse_v2(&raw, self.coop.registry)?;
        let mut expected_coop = self.coop.clone();
        expected_coop.save_generation = generation;
        expected_coop.crc32 = crc;
        if reparsed.selected_slot != self.selected_slot
            || reparsed.counter != self.counter
            || reparsed.logical_sector_offsets != self.logical_sector_offsets
            || reparsed.character_lineage != self.character_lineage
            || reparsed.coop != expected_coop
        {
            return Err(SectorProjectionError::ProtectedStateChanged);
        }
        Ok(reparsed)
    }
}

/// An audited writable interval in one selected logical sector's normal
/// checksummed payload. Approval must come from a separate ownership policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Dormant projection contract.
pub(crate) struct ApprovedSectorSpan {
    pub logical_id: u8,
    pub offset: usize,
    pub len: usize,
}

/// Replacement bytes for a selected logical sector's normal payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Dormant projection contract.
pub(crate) struct SelectedSectorPatch<'a> {
    pub logical_id: u8,
    pub offset: usize,
    pub bytes: &'a [u8],
}

/// Selected-slot projection failure. A failed projection returns no image.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SectorProjectionError {
    #[error("co-op source has a different character or identity registry")]
    SourceMismatch,
    #[error("patch is empty or outside a checksummed logical-sector payload")]
    InvalidPatch,
    #[error("patch is unapproved or overlaps another patch")]
    UnapprovedOrOverlapping,
    #[error("projected image failed schema-two validation: {0}")]
    Validation(#[from] SaveV2Error),
    #[error("co-op save generation cannot advance")]
    GenerationExhausted,
    #[error("projection changed the selected slot, character lineage, or regional co-op state")]
    ProtectedStateChanged,
}

/// Whole-image schema-two validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SaveV2Error {
    /// Failure while validating the Flash1M container or selecting its slot.
    #[error(transparent)]
    Container(#[from] super::SaveError),
    /// Failure while validating the selected schema-two co-op extension.
    #[error(transparent)]
    Coop(#[from] CoopSaveV2Error),
}

/// Validated schema-two cloud extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoopSaveV2 {
    /// Registry identity checked against the caller's contract.
    pub registry: RegistryContract,
    /// Monotonic generation sealed by the ROM before a checkpoint.
    pub save_generation: u32,
    /// Schema-two migration and compatibility flags.
    pub status_flags: u32,
    /// Hoenn, Kanto, Johto, Sevii, and Cormoria in fixed wire order.
    pub regional_progress: [RegionalProgress; 5],
    /// Append-only trainer-identity ordinals, stored as a 2048-bit set.
    pub defeated_trainers: [u8; COOP_TRAINER_BITS_SIZE],
    /// Append-only event-identity ordinals, stored as a 2048-bit set.
    pub events: [u8; COOP_EVENT_BITS_SIZE],
    /// Append-only Fly-point ordinals, stored as a 128-bit set.
    pub unlocked_fly_points: [u8; COOP_FLY_BITS_SIZE],
    /// Append-only gym/badge ordinals, stored as a 64-bit set.
    pub gyms: [u8; COOP_GYM_BITS_SIZE],
    /// CRC-32/ISO-HDLC over bytes 0 through 667.
    pub crc32: u32,
}

impl CoopSaveV2 {
    /// Whether legacy migration found an identity collision.
    #[must_use]
    pub const fn migration_ambiguous(&self) -> bool {
        self.status_flags & super::COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS != 0
    }

    /// Whether all Pokémon met-location values were normalized by migration.
    #[must_use]
    pub const fn met_locations_normalized(&self) -> bool {
        self.status_flags & COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED != 0
    }

    /// Whether this payload can participate in online authority immediately.
    ///
    /// The migration ambiguity bit is intentionally independent from the
    /// normalization bit: both flags may be present, but neither ambiguity nor
    /// incomplete Pokémon normalization is eligible for sharing.
    #[must_use]
    pub const fn online_eligible(&self) -> bool {
        self.met_locations_normalized() && !self.migration_ambiguous()
    }
}

/// Failure while validating a schema-two payload.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CoopSaveV2Error {
    /// Payload must be exactly the fixed 672-byte extension.
    #[error("invalid CoopSaveV2 payload length {actual}; expected {COOP_SAVE_V2_SIZE}")]
    InvalidLength { actual: usize },
    /// Payload does not have the frozen `CSP1` marker.
    #[error("invalid CoopSaveV2 magic {actual:#010x}, expected {COOP_SAVE_V1_MAGIC:#010x}")]
    Magic { actual: u32 },
    /// Only schema version two is understood by this additive parser.
    #[error("unsupported CoopSaveV2 schema {actual}; expected {COOP_SAVE_V2_SCHEMA_VERSION}")]
    SchemaVersion { actual: u16 },
    /// Embedded size must match the fixed ABI exactly.
    #[error("invalid CoopSaveV2 size {actual}; expected {COOP_SAVE_V2_SIZE}")]
    StructSize { actual: u16 },
    /// Registry version does not match the ROM/server contract.
    #[error("identity registry version {actual} does not match expected version {expected}")]
    RegistryVersion { expected: u32, actual: u32 },
    /// Registry digest does not bind to the expected canonical registry.
    #[error("identity registry digest {actual:02x?} does not match expected {expected:02x?}")]
    RegistryDigest {
        expected: [u8; 16],
        actual: [u8; 16],
    },
    /// Status contains semantics not frozen for schema two.
    #[error("CoopSaveV2 status {actual:#010x} contains unknown bits {unknown:#010x}")]
    UnknownStatusFlags { actual: u32, unknown: u32 },
    /// Payload was not sealed by the ROM or was changed after sealing.
    #[error("CoopSaveV2 CRC {actual:#010x} does not match calculated CRC {expected:#010x}")]
    Crc32 { expected: u32, actual: u32 },
    /// A regional record used an unspecified or unknown ordinal.
    #[error("regional progress record {record} has unsupported region ordinal {ordinal}")]
    RegionOrdinal { record: usize, ordinal: u8 },
    /// Records must remain in their fixed schema-two order.
    #[error(
        "regional progress record {record} has region {actual}, expected ordered region {expected}"
    )]
    RegionOrder {
        record: usize,
        expected: RegionId,
        actual: RegionId,
    },
    /// A per-record reserved byte was changed.
    #[error("reserved CoopSaveV2 byte at offset {offset} is nonzero ({value:#04x})")]
    ReservedByte { offset: usize, value: u8 },
    /// Only the low eight badge bits are canonical.
    #[error("regional progress record {record} has noncanonical badge mask {actual:#06x}")]
    BadgeMask { record: usize, actual: u16 },
    /// A persisted ordinal has no assignment in the bound registry.
    #[error("{kind} bitset contains unassigned ordinal {ordinal}")]
    UnassignedIdentityOrdinal { kind: IdentityKind, ordinal: u16 },
    /// A region-local badge bit has no assignment in the bound registry.
    #[error("region {region} contains unassigned badge bit {badge_bit}")]
    UnassignedBadgeBit { region: RegionId, badge_bit: u8 },
}

/// Validates a complete Flash1M image and decodes the ROM-selected schema-two
/// co-op extension.
///
/// Slot validation and counter selection deliberately reuse the same private
/// helpers as [`super::parse`]. A structurally valid newer V2 slot is therefore
/// never replaced by an older slot when its payload fails V2 validation.
///
/// # Errors
///
/// Returns [`SaveV2Error::Container`] for invalid image or slot data and
/// [`SaveV2Error::Coop`] for an invalid selected schema-two payload.
pub fn parse_v2(
    bytes: &[u8],
    expected_registry: RegistryContract,
) -> Result<ValidatedSaveV2, SaveV2Error> {
    super::validate_image_length(bytes)?;
    let flash = &bytes[..super::FLASH_IMAGE_SIZE];

    let first = super::validate_slot(flash, super::SaveSlot::First);
    let second = super::validate_slot(flash, super::SaveSlot::Second);
    let selected = match (first, second) {
        (Ok(first), Ok(second)) => super::select_rom_slot(first, second)?,
        (Ok(first), Err(second)) => super::select_only_valid_slot(first, second)?,
        (Err(first), Ok(second)) => super::select_only_valid_slot(second, first)?,
        (Err(first), Err(second)) => {
            return Err(SaveV2Error::Container(super::SaveError::NoValidSlot {
                first,
                second,
            }));
        }
    };

    let payload =
        &selected.save_block3[super::COOP_SAVE_OFFSET..super::COOP_SAVE_OFFSET + COOP_SAVE_V2_SIZE];
    let coop = parse_payload(payload, expected_registry)?;

    Ok(ValidatedSaveV2 {
        raw: bytes.into(),
        selected_slot: selected.slot,
        counter: selected.counter,
        logical_sector_offsets: selected.logical_sector_offsets,
        save_block3: selected.save_block3,
        character_lineage: selected.character_lineage,
        coop,
    })
}

/// Parses one exact, already reassembled schema-two co-op payload.
///
/// This function is intentionally independent from [`super::parse`]. A V1
/// whole-image caller therefore continues to reject schema two until the ROM
/// migration and selected-slot integration are explicitly activated.
///
/// # Errors
///
/// Returns [`CoopSaveV2Error`] when any fixed layout, registry, canonical
/// identity, reserved-byte, status, or CRC invariant fails.
pub fn parse_payload(
    bytes: &[u8],
    expected_registry: RegistryContract,
) -> Result<CoopSaveV2, CoopSaveV2Error> {
    if bytes.len() != COOP_SAVE_V2_SIZE {
        return Err(CoopSaveV2Error::InvalidLength {
            actual: bytes.len(),
        });
    }

    let magic = read_u32(bytes, COOP_MAGIC_OFFSET);
    if magic != COOP_SAVE_V1_MAGIC {
        return Err(CoopSaveV2Error::Magic { actual: magic });
    }

    let schema_version = read_u16(bytes, COOP_SCHEMA_OFFSET);
    if schema_version != COOP_SAVE_V2_SCHEMA_VERSION {
        return Err(CoopSaveV2Error::SchemaVersion {
            actual: schema_version,
        });
    }

    let struct_size = read_u16(bytes, COOP_STRUCT_SIZE_OFFSET);
    if usize::from(struct_size) != COOP_SAVE_V2_SIZE {
        return Err(CoopSaveV2Error::StructSize {
            actual: struct_size,
        });
    }

    let expected_crc = crc32fast::hash(&bytes[..COOP_CRC_OFFSET]);
    let actual_crc = read_u32(bytes, COOP_CRC_OFFSET);
    if actual_crc != expected_crc {
        return Err(CoopSaveV2Error::Crc32 {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    let registry_version = read_u32(bytes, COOP_REGISTRY_VERSION_OFFSET);
    if registry_version != expected_registry.version {
        return Err(CoopSaveV2Error::RegistryVersion {
            expected: expected_registry.version,
            actual: registry_version,
        });
    }
    let registry_digest = read_array::<16>(bytes, COOP_REGISTRY_DIGEST_OFFSET);
    if registry_digest != expected_registry.digest {
        return Err(CoopSaveV2Error::RegistryDigest {
            expected: expected_registry.digest,
            actual: registry_digest,
        });
    }

    let status_flags = read_u32(bytes, COOP_STATUS_FLAGS_OFFSET);
    let unknown_status_flags = status_flags & !COOP_SAVE_V2_STATUS_KNOWN_MASK;
    if unknown_status_flags != 0 {
        return Err(CoopSaveV2Error::UnknownStatusFlags {
            actual: status_flags,
            unknown: unknown_status_flags,
        });
    }

    let ordered_regions = [
        RegionId::Hoenn,
        RegionId::Kanto,
        RegionId::Johto,
        RegionId::Sevii,
        RegionId::Cormoria,
    ];
    let mut regional_progress = [RegionalProgress {
        region: RegionId::Hoenn,
        badge_mask: 0,
        story_checkpoint: 0,
    }; 5];
    for (record, expected_region) in ordered_regions.into_iter().enumerate() {
        let offset = if record < 4 {
            COOP_REGIONAL_PROGRESS_OFFSET + record * COOP_REGIONAL_PROGRESS_SIZE
        } else {
            COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET
        };
        regional_progress[record] = parse_progress_record(bytes, record, offset, expected_region)?;
    }

    if let Some((index, value)) = bytes[COOP_SAVE_V2_RESERVED_TAIL_OFFSET
        ..COOP_SAVE_V2_RESERVED_TAIL_OFFSET + COOP_SAVE_V2_RESERVED_TAIL_SIZE]
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value != 0)
    {
        return Err(CoopSaveV2Error::ReservedByte {
            offset: COOP_SAVE_V2_RESERVED_TAIL_OFFSET + index,
            value,
        });
    }

    let defeated_trainers = read_array::<COOP_TRAINER_BITS_SIZE>(bytes, COOP_TRAINER_BITS_OFFSET);
    let events = read_array::<COOP_EVENT_BITS_SIZE>(bytes, COOP_EVENT_BITS_OFFSET);
    let unlocked_fly_points = read_array::<COOP_FLY_BITS_SIZE>(bytes, COOP_FLY_BITS_OFFSET);
    let gyms = read_array::<COOP_GYM_BITS_SIZE>(bytes, COOP_GYM_BITS_OFFSET);
    validate_assigned_bits(&defeated_trainers, IdentityKind::Trainer)?;
    validate_assigned_bits(&events, IdentityKind::Event)?;
    validate_assigned_bits(&unlocked_fly_points, IdentityKind::FlyPoint)?;
    validate_assigned_bits(&gyms, IdentityKind::Gym)?;

    Ok(CoopSaveV2 {
        registry: expected_registry,
        save_generation: read_u32(bytes, COOP_GENERATION_OFFSET),
        status_flags,
        regional_progress,
        defeated_trainers,
        events,
        unlocked_fly_points,
        gyms,
        crc32: actual_crc,
    })
}

fn parse_progress_record(
    bytes: &[u8],
    record: usize,
    offset: usize,
    expected_region: RegionId,
) -> Result<RegionalProgress, CoopSaveV2Error> {
    let ordinal = bytes[offset];
    let region = RegionId::from_wire(ordinal)
        .ok()
        .and_then(|region| region.ensure_concrete().ok())
        .ok_or(CoopSaveV2Error::RegionOrdinal { record, ordinal })?;
    if region != expected_region {
        return Err(CoopSaveV2Error::RegionOrder {
            record,
            expected: expected_region,
            actual: region,
        });
    }

    let reserved = bytes[offset + 1];
    if reserved != 0 {
        return Err(CoopSaveV2Error::ReservedByte {
            offset: offset + 1,
            value: reserved,
        });
    }
    let badge_mask = read_u16(bytes, offset + 2);
    if badge_mask & !COOP_SAVE_BADGE_MASK != 0 {
        return Err(CoopSaveV2Error::BadgeMask {
            record,
            actual: badge_mask,
        });
    }
    for badge_bit in 0..8_u8 {
        if badge_mask & (1_u16 << badge_bit) != 0 && resolve_badge_bit(region, badge_bit).is_err() {
            return Err(CoopSaveV2Error::UnassignedBadgeBit { region, badge_bit });
        }
    }

    Ok(RegionalProgress {
        region,
        badge_mask,
        story_checkpoint: read_u32(bytes, offset + 4),
    })
}

fn validate_assigned_bits<const LENGTH: usize>(
    bits: &[u8; LENGTH],
    kind: IdentityKind,
) -> Result<(), CoopSaveV2Error> {
    for (byte_index, value) in bits.iter().copied().enumerate() {
        for bit_index in 0..8_u8 {
            if value & (1 << bit_index) == 0 {
                continue;
            }
            let ordinal = u16::try_from(byte_index * 8 + usize::from(bit_index))
                .expect("CoopSave bitset capacities fit in u16");
            if resolve_ordinal(kind, ordinal).is_err() {
                return Err(CoopSaveV2Error::UnassignedIdentityOrdinal { kind, ordinal });
            }
        }
    }
    Ok(())
}

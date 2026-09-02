//! Byte-accurate validation for `PokéCrossroads` `Flash1M` character saves.
//!
//! The game alternates between two rotated save slots. Each slot contains 15
//! physical 4 KiB sectors whose footer identifies the logical sector. This
//! crate validates that container before exposing the cloud co-op extension in
//! `SaveBlock3`; callers never have to trust physical sector order.

use std::array;

use coop_protocol::{
    IdentityKind, RegionId,
    identity_catalog::{resolve_badge_bit, resolve_ordinal},
};
use thiserror::Error;

/// `Flash1M` save data, excluding mGBA's optional real-time-clock trailer.
pub const FLASH_IMAGE_SIZE: usize = 128 * 1024;
/// Optional mGBA RTC trailer length.
pub const RTC_TRAILER_SIZE: usize = 16;
/// Size of one physical flash sector.
pub const SECTOR_SIZE: usize = 4096;
/// Number of logical sectors in each rotating character-save slot.
pub const SECTORS_PER_SLOT: usize = 15;
/// Number of alternating character-save slots.
pub const SAVE_SLOT_COUNT: usize = 2;
/// Offset of the `SaveBlock3` chunk in every physical sector.
pub const SAVE_BLOCK3_CHUNK_OFFSET: usize = 3968;
/// Length of the `SaveBlock3` chunk in every physical sector.
pub const SAVE_BLOCK3_CHUNK_SIZE: usize = 116;
/// Total reassembled `SaveBlock3` capacity in a slot.
pub const SAVE_BLOCK3_CAPACITY: usize = SAVE_BLOCK3_CHUNK_SIZE * SECTORS_PER_SLOT;

/// Offset of the cloud extension inside `SaveBlock3`.
pub const COOP_SAVE_OFFSET: usize = 4;
/// Frozen size of the version-one cloud extension.
pub const COOP_SAVE_V1_SIZE: usize = 672;
/// Little-endian ASCII `CSP1`.
pub const COOP_SAVE_V1_MAGIC: u32 = 0x3150_5343;
/// Frozen version-one schema ordinal.
pub const COOP_SAVE_V1_SCHEMA_VERSION: u16 = 1;
/// The ROM detected a legacy cross-region identity collision while migrating.
pub const COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS: u32 = 1;
/// Status bits understood by schema version one.
pub const COOP_SAVE_STATUS_KNOWN_MASK: u32 = COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
/// Canonical eight-badge subset of the 16-bit storage field.
pub const COOP_SAVE_BADGE_MASK: u16 = 0x00ff;

const SECTOR_DATA_OFFSET: usize = 0;
const SECTOR_ID_OFFSET: usize = 4084;
const SECTOR_CHECKSUM_OFFSET: usize = 4086;
const SECTOR_SIGNATURE_OFFSET: usize = 4088;
const SECTOR_COUNTER_OFFSET: usize = 4092;
const SECTOR_SIGNATURE: u32 = 0x0801_2025;

const COOP_MAGIC_OFFSET: usize = 0;
const COOP_SCHEMA_OFFSET: usize = 4;
const COOP_STRUCT_SIZE_OFFSET: usize = 6;
const COOP_REGISTRY_VERSION_OFFSET: usize = 8;
const COOP_REGISTRY_DIGEST_OFFSET: usize = 12;
const COOP_GENERATION_OFFSET: usize = 28;
const COOP_STATUS_FLAGS_OFFSET: usize = 32;
const COOP_REGIONAL_PROGRESS_OFFSET: usize = 36;
const COOP_REGIONAL_PROGRESS_SIZE: usize = 8;
const COOP_TRAINER_BITS_OFFSET: usize = 68;
const COOP_TRAINER_BITS_SIZE: usize = 256;
const COOP_EVENT_BITS_OFFSET: usize = 324;
const COOP_EVENT_BITS_SIZE: usize = 256;
const COOP_FLY_BITS_OFFSET: usize = 580;
const COOP_FLY_BITS_SIZE: usize = 16;
const COOP_GYM_BITS_OFFSET: usize = 596;
const COOP_GYM_BITS_SIZE: usize = 8;
const COOP_RESERVED_OFFSET: usize = 604;
const COOP_RESERVED_SIZE: usize = 64;
const COOP_CRC_OFFSET: usize = 668;

// Frozen `SaveBlock2` identity offsets in PokéCrossroads Beta-1.4. These
// bytes are not an authorization secret; they let the server bind revisions
// after the first accepted snapshot to one in-game character lineage.
const PLAYER_NAME_OFFSET: usize = 0;
const PLAYER_NAME_SIZE: usize = 8;
const PLAYER_GENDER_OFFSET: usize = 16;
const PLAYER_REGION_OFFSET: usize = 17;
const PLAYER_TRAINER_ID_OFFSET: usize = 19;
const PLAYER_TRAINER_ID_SIZE: usize = 4;

/// Number of bytes from the normal sector payload covered by the game's
/// additive checksum for each logical sector.
pub const LOGICAL_SECTOR_DATA_SIZES: [usize; SECTORS_PER_SLOT] = [
    3884, 3968, 3968, 3968, 3664, 0, 3968, 3968, 3968, 3968, 3968, 3968, 3968, 3968, 2400,
];

/// Expected identity-registry metadata embedded in `CoopSaveV1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryContract {
    /// Append-only registry version.
    pub version: u32,
    /// First 16 bytes of the canonical registry's SHA-256 digest.
    pub digest: [u8; 16],
}

impl RegistryContract {
    /// Creates an expected registry contract.
    #[must_use]
    pub const fn new(version: u32, digest: [u8; 16]) -> Self {
        Self { version, digest }
    }
}

/// One region's durable campaign progress from `CoopSaveV1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionalProgress {
    /// Campaign owning every identifier represented by this record.
    pub region: RegionId,
    /// Region-local badge mask. The storage ABI is intentionally 16 bits.
    pub badge_mask: u16,
    /// Region-local story checkpoint.
    pub story_checkpoint: u32,
}

/// Validated version-one cloud extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoopSaveV1 {
    /// Registry identity that was checked against the caller's contract.
    pub registry: RegistryContract,
    /// Monotonic generation sealed by the ROM before a checkpoint.
    pub save_generation: u32,
    /// ROM-defined migration and compatibility flags.
    pub status_flags: u32,
    /// Exactly one record for Hoenn, Kanto, Johto, and Sevii.
    pub regional_progress: [RegionalProgress; 4],
    /// Append-only trainer-identity ordinals, stored as a 2048-bit set.
    pub defeated_trainers: [u8; COOP_TRAINER_BITS_SIZE],
    /// Append-only event-identity ordinals, stored as a 2048-bit set.
    pub events: [u8; COOP_EVENT_BITS_SIZE],
    /// Append-only Fly-point ordinals, stored as a 128-bit set.
    pub unlocked_fly_points: [u8; COOP_FLY_BITS_SIZE],
    /// Append-only gym/badge ordinals, stored as a 64-bit set.
    pub gyms: [u8; COOP_GYM_BITS_SIZE],
    /// CRC-32/ISO-HDLC (`crc32fast`) over bytes 0 through 667.
    pub crc32: u32,
}

/// Stable in-game identity bytes read from logical `SaveBlock2` sector zero.
///
/// This value detects accidental or deliberate cross-character save swaps
/// after a server has accepted the first snapshot. It is not a cryptographic
/// identity and cannot by itself prevent cloning at the initial upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterLineage {
    /// Engine-encoded player name, including its terminator/padding byte.
    pub player_name: [u8; PLAYER_NAME_SIZE],
    /// Engine player gender ordinal.
    pub player_gender: u8,
    /// `PokéCrossroads` player campaign-region ordinal.
    pub player_region: u8,
    /// Four-byte immutable trainer ID generated with the character.
    pub player_trainer_id: [u8; PLAYER_TRAINER_ID_SIZE],
}

impl CoopSaveV1 {
    /// Whether legacy migration found an identity collision that requires
    /// explicit resolution before online play.
    #[must_use]
    pub const fn migration_ambiguous(&self) -> bool {
        self.status_flags & COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS != 0
    }

    /// Whether this otherwise valid payload can participate in online
    /// authority immediately.
    #[must_use]
    pub const fn online_eligible(&self) -> bool {
        !self.migration_ambiguous()
    }
}

/// Physical slot selected after both candidates were validated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveSlot {
    /// Physical sectors 0 through 14.
    First,
    /// Physical sectors 15 through 29.
    Second,
}

impl SaveSlot {
    const fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }

    const fn from_counter(counter: u32) -> Self {
        if counter & 1 == 0 {
            Self::First
        } else {
            Self::Second
        }
    }
}

/// A byte-for-byte retained save whose newest slot and co-op payload passed all
/// validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedSave {
    raw: Box<[u8]>,
    selected_slot: SaveSlot,
    counter: u32,
    save_block3: [u8; SAVE_BLOCK3_CAPACITY],
    character_lineage: CharacterLineage,
    coop: CoopSaveV1,
}

impl ValidatedSave {
    /// Original bytes, including an optional RTC trailer.
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// The exact 128 KiB `Flash1M` image.
    #[must_use]
    pub fn flash_bytes(&self) -> &[u8] {
        &self.raw[..FLASH_IMAGE_SIZE]
    }

    /// The original optional mGBA RTC trailer.
    #[must_use]
    pub fn rtc_trailer(&self) -> Option<&[u8; RTC_TRAILER_SIZE]> {
        self.raw
            .get(FLASH_IMAGE_SIZE..)
            .and_then(|trailer| trailer.try_into().ok())
    }

    /// Consumes the validated representation without changing a byte.
    #[must_use]
    pub fn into_raw_bytes(self) -> Box<[u8]> {
        self.raw
    }

    /// Selected physical slot.
    #[must_use]
    pub const fn selected_slot(&self) -> SaveSlot {
        self.selected_slot
    }

    /// Uniform save counter from the selected slot.
    #[must_use]
    pub const fn counter(&self) -> u32 {
        self.counter
    }

    /// `SaveBlock3` chunks reassembled by logical sector ID, never by physical
    /// sector order.
    #[must_use]
    pub const fn save_block3(&self) -> &[u8; SAVE_BLOCK3_CAPACITY] {
        &self.save_block3
    }

    /// In-game identity carried by the selected committed save slot.
    #[must_use]
    pub const fn character_lineage(&self) -> CharacterLineage {
        self.character_lineage
    }

    /// Validated cloud extension.
    #[must_use]
    pub const fn coop(&self) -> &CoopSaveV1 {
        &self.coop
    }
}

/// Validated canonical erased image. Its bytes are private so callers cannot
/// construct a fake revision-zero result without validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErasedRevisionZero {
    raw: Box<[u8]>,
}

impl ErasedRevisionZero {
    /// Exact 128 KiB erased bytes retained by validation.
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }
}

/// Character-save result that makes the only accepted revision-zero form
/// explicit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CharacterSave {
    /// Exact 128 KiB erased image used before the first successful save.
    ErasedRevisionZero(ErasedRevisionZero),
    /// Nonzero cloud revision with a valid `CoopSaveV1`.
    Version1(Box<ValidatedSave>),
}

impl CharacterSave {
    /// Returns the retained original bytes.
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        match self {
            Self::ErasedRevisionZero(save) => save.raw_bytes(),
            Self::Version1(save) => save.raw_bytes(),
        }
    }
}

/// Whole-image or co-op extension validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SaveError {
    /// Only a raw `Flash1M` image or that image plus mGBA's RTC trailer is valid.
    #[error(
        "invalid save length {actual}; expected {FLASH_IMAGE_SIZE} or {} bytes",
        FLASH_IMAGE_SIZE + RTC_TRAILER_SIZE
    )]
    InvalidLength {
        /// Actual byte count.
        actual: usize,
    },
    /// Neither alternating slot is structurally valid.
    #[error("neither character-save slot is valid (first: {first}; second: {second})")]
    NoValidSlot {
        /// Failure in physical slot zero.
        first: SlotError,
        /// Failure in physical slot one.
        second: SlotError,
    },
    /// The ROM-selected counter points at a physical slot that failed
    /// structural validation. Accepting the other slot would diverge from the
    /// bytes the game attempts to load.
    #[error("save counter {counter} selects {slot:?}, but that physical slot is invalid: {reason}")]
    RomSelectedSlotInvalid {
        /// Counter selected with the ROM's wraparound comparison.
        counter: u32,
        /// Physical slot selected by the counter's low bit.
        slot: SaveSlot,
        /// Structural failure in that physical slot.
        reason: SlotError,
    },
    /// Both slots are structurally valid, but the ROM-selected physical slot
    /// does not carry the counter that selected it.
    #[error(
        "save counter {counter} selects {slot:?}, whose sectors instead carry counter {slot_counter}"
    )]
    RomSelectedSlotCounterMismatch {
        /// Counter selected with the ROM's wraparound comparison.
        counter: u32,
        /// Physical slot selected by the counter's low bit.
        slot: SaveSlot,
        /// Uniform counter actually stored in that physical slot.
        slot_counter: u32,
    },
    /// The selected slot is structurally sound but its co-op payload is not.
    #[error(transparent)]
    Coop(#[from] CoopSaveError),
    /// Revision zero is deliberately narrower than the general parser.
    #[error("cloud revision zero must be an exact 128 KiB erased image")]
    RevisionZeroNotErased,
}

/// Failure while validating one physical save slot.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SlotError {
    /// Footer signature did not identify a committed game sector.
    #[error(
        "physical sector {physical_sector} has signature {actual:#010x}, expected {SECTOR_SIGNATURE:#010x}"
    )]
    Signature {
        /// Physical index within this slot.
        physical_sector: usize,
        /// Footer value.
        actual: u32,
    },
    /// Footer logical ID is outside the slot's frozen range.
    #[error("physical sector {physical_sector} has out-of-range logical ID {logical_id}")]
    LogicalIdOutOfRange {
        /// Physical index within this slot.
        physical_sector: usize,
        /// Footer value.
        logical_id: u16,
    },
    /// Normal payload checksum does not match `PokéCrossroads`' additive sum.
    #[error(
        "physical sector {physical_sector} (logical {logical_id}) has checksum {actual:#06x}, expected {expected:#06x}"
    )]
    Checksum {
        /// Physical index within this slot.
        physical_sector: usize,
        /// Footer logical ID.
        logical_id: u16,
        /// Calculated checksum.
        expected: u16,
        /// Footer checksum.
        actual: u16,
    },
    /// Every sector in a candidate slot must belong to one save operation.
    #[error(
        "physical sector {physical_sector} has counter {actual}, expected uniform counter {expected}"
    )]
    MixedCounter {
        /// Physical index within this slot.
        physical_sector: usize,
        /// Counter selected from the first physical sector.
        expected: u32,
        /// Mismatching footer counter.
        actual: u32,
    },
    /// The set of logical IDs is not exactly 0 through 14 once each.
    #[error("logical sector set is invalid (missing {missing:?}, duplicates {duplicates:?})")]
    LogicalIdSet {
        /// IDs with no physical sector.
        missing: Vec<u8>,
        /// IDs represented by more than one physical sector.
        duplicates: Vec<u8>,
    },
}

/// Failure inside the frozen `CoopSaveV1` payload.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CoopSaveError {
    /// Payload does not have the frozen `CSP1` marker.
    #[error("invalid CoopSaveV1 magic {actual:#010x}, expected {COOP_SAVE_V1_MAGIC:#010x}")]
    Magic {
        /// Stored magic.
        actual: u32,
    },
    /// Only schema version one is currently understood.
    #[error("unsupported co-op save schema {actual}; expected {COOP_SAVE_V1_SCHEMA_VERSION}")]
    SchemaVersion {
        /// Stored schema version.
        actual: u16,
    },
    /// Embedded size must match the frozen ABI exactly.
    #[error("invalid CoopSaveV1 size {actual}; expected {COOP_SAVE_V1_SIZE}")]
    StructSize {
        /// Stored size.
        actual: u16,
    },
    /// Registry version does not match the ROM/server contract.
    #[error("identity registry version {actual} does not match expected version {expected}")]
    RegistryVersion {
        /// Expected append-only registry version.
        expected: u32,
        /// Stored version.
        actual: u32,
    },
    /// Registry digest does not bind to the expected canonical registry.
    #[error("identity registry digest {actual:02x?} does not match expected {expected:02x?}")]
    RegistryDigest {
        /// Expected truncated SHA-256 digest.
        expected: [u8; 16],
        /// Stored digest.
        actual: [u8; 16],
    },
    /// Version one rejects status bits whose semantics are not frozen.
    #[error("CoopSaveV1 status {actual:#010x} contains unknown bits {unknown:#010x}")]
    UnknownStatusFlags {
        /// Entire stored status field.
        actual: u32,
        /// Bits outside [`COOP_SAVE_STATUS_KNOWN_MASK`].
        unknown: u32,
    },
    /// The payload was not sealed by the ROM or was changed after sealing.
    #[error("CoopSaveV1 CRC {actual:#010x} does not match calculated CRC {expected:#010x}")]
    Crc32 {
        /// Calculated CRC.
        expected: u32,
        /// Stored CRC.
        actual: u32,
    },
    /// A regional record used the wire-only unspecified ordinal or an unknown
    /// future value.
    #[error("regional progress record {record} has unsupported region ordinal {ordinal}")]
    RegionOrdinal {
        /// Record index.
        record: usize,
        /// Stored ordinal.
        ordinal: u8,
    },
    /// Version one fixes regional-record order to make the C and Rust ABIs
    /// deterministic.
    #[error(
        "regional progress record {record} has region {actual}, expected ordered region {expected}"
    )]
    RegionOrder {
        /// Record index.
        record: usize,
        /// Expected concrete region.
        expected: RegionId,
        /// Stored concrete region.
        actual: RegionId,
    },
    /// Only the low eight badge bits are canonical in version one.
    #[error("regional progress record {record} has noncanonical badge mask {actual:#06x}")]
    BadgeMask {
        /// Record index.
        record: usize,
        /// Stored 16-bit mask.
        actual: u16,
    },
    /// A persisted ordinal bit has no assignment in the registry bound into
    /// this schema. The registry digest alone cannot make unknown bits safe.
    #[error("{kind} bitset contains unassigned ordinal {ordinal}")]
    UnassignedIdentityOrdinal {
        /// Registry namespace owning the persisted bitset.
        kind: IdentityKind,
        /// Set bit with no append-only registry assignment.
        ordinal: u16,
    },
    /// A region-local badge bit has no badge identity in that campaign.
    #[error("region {region} contains unassigned badge bit {badge_bit}")]
    UnassignedBadgeBit {
        /// Region whose badge mask was decoded.
        region: RegionId,
        /// Set bit with no regional badge identity.
        badge_bit: u8,
    },
    /// All reserved bytes are zero in version one.
    #[error("reserved CoopSaveV1 byte at offset {offset} is nonzero ({value:#04x})")]
    ReservedByte {
        /// Offset relative to the start of `CoopSaveV1`.
        offset: usize,
        /// Stored byte.
        value: u8,
    },
}

#[derive(Debug)]
struct ValidatedSlot {
    slot: SaveSlot,
    counter: u32,
    save_block3: [u8; SAVE_BLOCK3_CAPACITY],
    character_lineage: CharacterLineage,
}

/// Creates the only canonical revision-zero image.
#[must_use]
pub fn erased_revision_zero_image() -> Vec<u8> {
    vec![0xff; FLASH_IMAGE_SIZE]
}

/// Validates a save according to its cloud revision.
///
/// Revision zero accepts only [`erased_revision_zero_image`]. Every nonzero
/// revision must contain a structurally valid slot and a matching
/// `CoopSaveV1`; an erased image can therefore never be uploaded as a later
/// revision.
///
/// # Errors
///
/// Returns [`SaveError`] for invalid lengths, a noncanonical revision-zero
/// image, corrupt slots, or an incompatible co-op payload.
pub fn validate_character_save(
    bytes: &[u8],
    cloud_revision: u64,
    expected_registry: RegistryContract,
) -> Result<CharacterSave, SaveError> {
    validate_image_length(bytes)?;

    if cloud_revision == 0 {
        if bytes.len() == FLASH_IMAGE_SIZE && bytes.iter().all(|byte| *byte == 0xff) {
            return Ok(CharacterSave::ErasedRevisionZero(ErasedRevisionZero {
                raw: bytes.into(),
            }));
        }
        return Err(SaveError::RevisionZeroNotErased);
    }

    parse(bytes, expected_registry).map(|save| CharacterSave::Version1(Box::new(save)))
}

/// Validates both physical slots, selects the newest committed slot, and
/// decodes its `CoopSaveV1` extension.
///
/// The optional 16-byte RTC trailer is retained exactly but is not interpreted.
/// If the newest structurally valid slot contains an invalid co-op payload, the
/// function fails instead of silently rolling back to an older cloud state.
///
/// # Errors
///
/// Returns [`SaveError`] when the image length, both slot candidates, or the
/// newest slot's co-op payload is invalid.
pub fn parse(
    bytes: &[u8],
    expected_registry: RegistryContract,
) -> Result<ValidatedSave, SaveError> {
    validate_image_length(bytes)?;
    let flash = &bytes[..FLASH_IMAGE_SIZE];

    let first = validate_slot(flash, SaveSlot::First);
    let second = validate_slot(flash, SaveSlot::Second);
    let selected = match (first, second) {
        (Ok(first), Ok(second)) => select_rom_slot(first, second)?,
        (Ok(first), Err(second)) => select_only_valid_slot(first, second)?,
        (Err(first), Ok(second)) => select_only_valid_slot(second, first)?,
        (Err(first), Err(second)) => return Err(SaveError::NoValidSlot { first, second }),
    };

    let coop = parse_coop_save(&selected.save_block3, expected_registry)?;
    Ok(ValidatedSave {
        raw: bytes.into(),
        selected_slot: selected.slot,
        counter: selected.counter,
        save_block3: selected.save_block3,
        character_lineage: selected.character_lineage,
        coop,
    })
}

/// Calculates the additive checksum used by `PokéCrossroads` sector footers.
///
/// Only complete little-endian 32-bit words contribute, matching the ROM.
#[must_use]
pub fn sector_checksum(data: &[u8]) -> u16 {
    let sum = data.chunks_exact(4).fold(0_u32, |sum, word| {
        sum.wrapping_add(u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
    });
    let folded = sum.wrapping_add(sum >> 16).to_le_bytes();
    u16::from_le_bytes([folded[0], folded[1]])
}

fn validate_image_length(bytes: &[u8]) -> Result<(), SaveError> {
    if bytes.len() == FLASH_IMAGE_SIZE || bytes.len() == FLASH_IMAGE_SIZE + RTC_TRAILER_SIZE {
        Ok(())
    } else {
        Err(SaveError::InvalidLength {
            actual: bytes.len(),
        })
    }
}

fn validate_slot(flash: &[u8], slot: SaveSlot) -> Result<ValidatedSlot, SlotError> {
    let slot_base = slot.index() * SECTORS_PER_SLOT * SECTOR_SIZE;
    let mut counts = [0_u8; SECTORS_PER_SLOT];
    let mut logical_sector_offsets = [0_usize; SECTORS_PER_SLOT];
    let mut counter = None;

    for physical_sector in 0..SECTORS_PER_SLOT {
        let offset = slot_base + physical_sector * SECTOR_SIZE;
        let sector = &flash[offset..offset + SECTOR_SIZE];
        let signature = read_u32(sector, SECTOR_SIGNATURE_OFFSET);
        if signature != SECTOR_SIGNATURE {
            return Err(SlotError::Signature {
                physical_sector,
                actual: signature,
            });
        }

        let logical_id = read_u16(sector, SECTOR_ID_OFFSET);
        let logical_index = usize::from(logical_id);
        if logical_index >= SECTORS_PER_SLOT {
            return Err(SlotError::LogicalIdOutOfRange {
                physical_sector,
                logical_id,
            });
        }

        let checksum_size = LOGICAL_SECTOR_DATA_SIZES[logical_index];
        let expected_checksum =
            sector_checksum(&sector[SECTOR_DATA_OFFSET..SECTOR_DATA_OFFSET + checksum_size]);
        let actual_checksum = read_u16(sector, SECTOR_CHECKSUM_OFFSET);
        if actual_checksum != expected_checksum {
            return Err(SlotError::Checksum {
                physical_sector,
                logical_id,
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        let sector_counter = read_u32(sector, SECTOR_COUNTER_OFFSET);
        if let Some(expected) = counter {
            if sector_counter != expected {
                return Err(SlotError::MixedCounter {
                    physical_sector,
                    expected,
                    actual: sector_counter,
                });
            }
        } else {
            counter = Some(sector_counter);
        }

        counts[logical_index] = counts[logical_index].saturating_add(1);
        logical_sector_offsets[logical_index] = offset;
    }

    let missing = counts
        .iter()
        .zip(0_u8..)
        .filter_map(|(count, id)| (*count == 0).then_some(id))
        .collect::<Vec<_>>();
    let duplicates = counts
        .iter()
        .zip(0_u8..)
        .filter_map(|(count, id)| (*count > 1).then_some(id))
        .collect::<Vec<_>>();
    if !missing.is_empty() || !duplicates.is_empty() {
        return Err(SlotError::LogicalIdSet {
            missing,
            duplicates,
        });
    }

    let counter = counter.expect("a slot always contains sectors");

    let mut save_block3 = [0_u8; SAVE_BLOCK3_CAPACITY];
    for (logical_id, sector_offset) in logical_sector_offsets.into_iter().enumerate() {
        let source_start = sector_offset + SAVE_BLOCK3_CHUNK_OFFSET;
        let destination_start = logical_id * SAVE_BLOCK3_CHUNK_SIZE;
        save_block3[destination_start..destination_start + SAVE_BLOCK3_CHUNK_SIZE]
            .copy_from_slice(&flash[source_start..source_start + SAVE_BLOCK3_CHUNK_SIZE]);
    }

    let save_block2 = &flash[logical_sector_offsets[0]..logical_sector_offsets[0] + SECTOR_SIZE];
    let character_lineage = CharacterLineage {
        player_name: read_array::<PLAYER_NAME_SIZE>(save_block2, PLAYER_NAME_OFFSET),
        player_gender: save_block2[PLAYER_GENDER_OFFSET],
        player_region: save_block2[PLAYER_REGION_OFFSET],
        player_trainer_id: read_array::<PLAYER_TRAINER_ID_SIZE>(
            save_block2,
            PLAYER_TRAINER_ID_OFFSET,
        ),
    };

    Ok(ValidatedSlot {
        slot,
        counter,
        save_block3,
        character_lineage,
    })
}

fn select_rom_slot(
    first: ValidatedSlot,
    second: ValidatedSlot,
) -> Result<ValidatedSlot, SaveError> {
    // This deliberately mirrors GetSaveValidStatus in the ROM. Its only
    // special wrap case is UINT32_MAX adjacent to zero; all other pairs use a
    // normal numeric comparison. CopySaveSlotData then chooses the physical
    // slot from the selected counter's low bit, including for equal counters.
    let counter = if first.counter == u32::MAX && second.counter == 0 {
        second.counter
    } else if first.counter == 0 && second.counter == u32::MAX {
        first.counter
    } else if first.counter < second.counter {
        second.counter
    } else {
        first.counter
    };
    let slot = SaveSlot::from_counter(counter);
    let selected = match slot {
        SaveSlot::First => first,
        SaveSlot::Second => second,
    };
    if selected.counter != counter {
        return Err(SaveError::RomSelectedSlotCounterMismatch {
            counter,
            slot,
            slot_counter: selected.counter,
        });
    }
    Ok(selected)
}

fn select_only_valid_slot(
    candidate: ValidatedSlot,
    other_error: SlotError,
) -> Result<ValidatedSlot, SaveError> {
    let selected_slot = SaveSlot::from_counter(candidate.counter);
    if candidate.slot == selected_slot {
        Ok(candidate)
    } else {
        Err(SaveError::RomSelectedSlotInvalid {
            counter: candidate.counter,
            slot: selected_slot,
            reason: other_error,
        })
    }
}

fn parse_coop_save(
    save_block3: &[u8; SAVE_BLOCK3_CAPACITY],
    expected_registry: RegistryContract,
) -> Result<CoopSaveV1, CoopSaveError> {
    let bytes = &save_block3[COOP_SAVE_OFFSET..COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE];
    let magic = read_u32(bytes, COOP_MAGIC_OFFSET);
    if magic != COOP_SAVE_V1_MAGIC {
        return Err(CoopSaveError::Magic { actual: magic });
    }

    let schema_version = read_u16(bytes, COOP_SCHEMA_OFFSET);
    if schema_version != COOP_SAVE_V1_SCHEMA_VERSION {
        return Err(CoopSaveError::SchemaVersion {
            actual: schema_version,
        });
    }

    let struct_size = read_u16(bytes, COOP_STRUCT_SIZE_OFFSET);
    if usize::from(struct_size) != COOP_SAVE_V1_SIZE {
        return Err(CoopSaveError::StructSize {
            actual: struct_size,
        });
    }

    let expected_crc = crc32fast::hash(&bytes[..COOP_CRC_OFFSET]);
    let actual_crc = read_u32(bytes, COOP_CRC_OFFSET);
    if actual_crc != expected_crc {
        return Err(CoopSaveError::Crc32 {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    let registry_version = read_u32(bytes, COOP_REGISTRY_VERSION_OFFSET);
    if registry_version != expected_registry.version {
        return Err(CoopSaveError::RegistryVersion {
            expected: expected_registry.version,
            actual: registry_version,
        });
    }
    let registry_digest = read_array::<16>(bytes, COOP_REGISTRY_DIGEST_OFFSET);
    if registry_digest != expected_registry.digest {
        return Err(CoopSaveError::RegistryDigest {
            expected: expected_registry.digest,
            actual: registry_digest,
        });
    }

    let status_flags = read_u32(bytes, COOP_STATUS_FLAGS_OFFSET);
    let unknown_status_flags = status_flags & !COOP_SAVE_STATUS_KNOWN_MASK;
    if unknown_status_flags != 0 {
        return Err(CoopSaveError::UnknownStatusFlags {
            actual: status_flags,
            unknown: unknown_status_flags,
        });
    }

    let regional_progress = parse_regional_progress(bytes)?;

    if let Some((index, value)) = bytes
        [COOP_RESERVED_OFFSET..COOP_RESERVED_OFFSET + COOP_RESERVED_SIZE]
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value != 0)
    {
        return Err(CoopSaveError::ReservedByte {
            offset: COOP_RESERVED_OFFSET + index,
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

    Ok(CoopSaveV1 {
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

fn validate_assigned_bits<const LENGTH: usize>(
    bits: &[u8; LENGTH],
    kind: IdentityKind,
) -> Result<(), CoopSaveError> {
    for (byte_index, value) in bits.iter().copied().enumerate() {
        for bit_index in 0..8_u8 {
            if value & (1 << bit_index) == 0 {
                continue;
            }
            let ordinal = u16::try_from(byte_index * 8 + usize::from(bit_index))
                .expect("CoopSaveV1 bitset capacities fit in u16");
            if resolve_ordinal(kind, ordinal).is_err() {
                return Err(CoopSaveError::UnassignedIdentityOrdinal { kind, ordinal });
            }
        }
    }
    Ok(())
}

fn parse_regional_progress(bytes: &[u8]) -> Result<[RegionalProgress; 4], CoopSaveError> {
    const ORDERED_REGIONS: [RegionId; 4] = [
        RegionId::Hoenn,
        RegionId::Kanto,
        RegionId::Johto,
        RegionId::Sevii,
    ];
    let mut progress = array::from_fn(|_| RegionalProgress {
        region: RegionId::Hoenn,
        badge_mask: 0,
        story_checkpoint: 0,
    });
    for (record, entry) in progress.iter_mut().enumerate() {
        let offset = COOP_REGIONAL_PROGRESS_OFFSET + record * COOP_REGIONAL_PROGRESS_SIZE;
        let ordinal = bytes[offset];
        let region = RegionId::from_wire(ordinal)
            .ok()
            .and_then(|region| region.ensure_concrete().ok())
            .ok_or(CoopSaveError::RegionOrdinal { record, ordinal })?;
        let expected_region = ORDERED_REGIONS[record];
        if region != expected_region {
            return Err(CoopSaveError::RegionOrder {
                record,
                expected: expected_region,
                actual: region,
            });
        }
        let reserved = bytes[offset + 1];
        if reserved != 0 {
            return Err(CoopSaveError::ReservedByte {
                offset: offset + 1,
                value: reserved,
            });
        }
        let badge_mask = read_u16(bytes, offset + 2);
        if badge_mask & !COOP_SAVE_BADGE_MASK != 0 {
            return Err(CoopSaveError::BadgeMask {
                record,
                actual: badge_mask,
            });
        }
        for badge_bit in 0..8_u8 {
            if badge_mask & (1_u16 << badge_bit) != 0
                && resolve_badge_bit(region, badge_bit).is_err()
            {
                return Err(CoopSaveError::UnassignedBadgeBit { region, badge_bit });
            }
        }
        *entry = RegionalProgress {
            region,
            badge_mask,
            story_checkpoint: read_u32(bytes, offset + 4),
        };
    }
    Ok(progress)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(read_array(bytes, offset))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(read_array(bytes, offset))
}

fn read_array<const LENGTH: usize>(bytes: &[u8], offset: usize) -> [u8; LENGTH] {
    bytes[offset..offset + LENGTH]
        .try_into()
        .expect("format offsets are compile-time bounded")
}

#[cfg(test)]
mod tests;

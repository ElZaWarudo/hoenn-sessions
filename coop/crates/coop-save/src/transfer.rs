//! In-memory selected-slot player projection. The caller must authenticate the
//! ROM manifests before supplying their compiler-emitted descriptors, and a
//! journal must establish which source co-op record is authoritative. This
//! module never chooses a world or commits a save to disk.

use thiserror::Error;

use super::v2::{ApprovedSectorSpan, SectorProjectionError, SelectedSectorPatch};
use super::{COOP_SAVE_OFFSET, LOGICAL_SECTOR_DATA_SIZES, SAVE_BLOCK3_CHUNK_SIZE, ValidatedSaveV2};

const MAGIC: u32 = 0x3154_5043;
const HEADER_LEN: usize = 44;
const FIELD_LEN: usize = 16;
const SHARED: u8 = 1;
const LOCAL: u8 = 2;
const PENDING: u8 = 3;
const MONEY: u16 = 0x0102;
const COINS: u16 = 0x0103;
const BAG: u16 = 0x0106;
const GAME_STATS: u16 = 0x0113;
const KEY: u16 = 0x020b;
const BERRY_POWDER: u16 = 0x020d;
const COOP_RECORD: u16 = 0x0403;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Field {
    id: u16,
    storage: u8,
    owner: u8,
    offset: usize,
    size: usize,
}

/// Source and destination descriptors supplied by the travel coordinator.
///
/// The coordinator must authenticate the ROM build descriptor before creating
/// this pair. The save crate only checks that the two supplied
/// descriptors are byte-identical. It cannot establish build provenance,
/// prove that a destination template is fresh, or fence the active source
/// save.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferDescriptorPair<'a> {
    /// Source ROM's compiler-emitted descriptor.
    pub source: &'a [u8],
    /// Destination ROM's compiler-emitted descriptor.
    pub destination: &'a [u8],
}

impl<'a> TransferDescriptorPair<'a> {
    fn resolve(self) -> Result<&'a [u8], TransferError> {
        if self.source == self.destination {
            Ok(self.source)
        } else {
            Err(TransferError::DescriptorMismatch)
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum TransferError {
    #[error("source and destination player-transfer descriptors differ")]
    DescriptorMismatch,
    #[error("player-transfer descriptor is incomplete or invalid")]
    InvalidDescriptor,
    #[error("player-transfer field {0:#06x} still has unresolved ownership")]
    PendingField(u16),
    #[error("source and destination characters differ")]
    CharacterMismatch,
    #[error("selected-slot projection failed: {0}")]
    Projection(#[from] SectorProjectionError),
}

/// Project a player's shared state into an arriving world's save.
///
/// This is the only public operation that combines trainer binding, the
/// descriptor-driven shared-field projection, and the co-op generation bump.
/// It always advances the generation exactly once with checked overflow and
/// reparses the resulting image before returning it. `first_arrival` must be
/// true only after the caller has proved that `destination` is a never-active
/// template for this player. The caller must authenticate both descriptors
/// and fence the source save's active lease/journal record; those
/// authority checks require server state and cannot be established here.
pub fn project_arrival(
    source: &ValidatedSaveV2,
    destination: &ValidatedSaveV2,
    descriptor: TransferDescriptorPair<'_>,
    first_arrival: bool,
) -> Result<ValidatedSaveV2, TransferError> {
    let descriptor = descriptor.resolve()?;
    let destination = if first_arrival {
        destination.bind_fresh_template_trainer(source)?
    } else {
        destination.clone()
    };
    let projected = project_shared_player(source, &destination, descriptor, descriptor)?;
    Ok(projected.advance_transfer_generation()?)
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn parse_descriptor(bytes: &[u8]) -> Result<(Vec<Field>, [usize; 4]), TransferError> {
    let invalid = || TransferError::InvalidDescriptor;
    if bytes.len() < HEADER_LEN
        || u32_at(bytes, 0) != Some(MAGIC)
        || u16_at(bytes, 4) != Some(3)
        || u32_at(bytes, 8) != Some(u32::try_from(bytes.len()).map_err(|_| invalid())?)
        || u32_at(bytes, 12) != Some(44)
        || u32_at(bytes, 32) != Some(44)
        || u32_at(bytes, 36) != Some(16)
        || u32_at(bytes, 40) != Some(0)
    {
        return Err(invalid());
    }
    let count = usize::from(u16_at(bytes, 6).ok_or_else(invalid)?);
    if count == 0 || HEADER_LEN.checked_add(count * FIELD_LEN) != Some(bytes.len()) {
        return Err(invalid());
    }
    let spans = [16, 20, 24, 28].map(|offset| u32_at(bytes, offset).unwrap_or(0) as usize);
    let capacities = [
        0x4220,
        LOGICAL_SECTOR_DATA_SIZES[0],
        LOGICAL_SECTOR_DATA_SIZES[6..].iter().sum(),
        SAVE_BLOCK3_CHUNK_SIZE * 15,
    ];
    if spans.contains(&0) || spans.iter().zip(capacities).any(|(&span, cap)| span > cap) {
        return Err(invalid());
    }
    let mut fields = Vec::with_capacity(count);
    let mut cursor = [0; 4];
    for index in 0..count {
        let base = HEADER_LEN + index * FIELD_LEN;
        let id = u16_at(bytes, base).ok_or_else(invalid)?;
        let storage = bytes[base + 2];
        let owner = bytes[base + 3];
        let offset = u32_at(bytes, base + 4).ok_or_else(invalid)? as usize;
        let size = u32_at(bytes, base + 8).ok_or_else(invalid)? as usize;
        let storage_index = usize::from(storage);
        if storage_index >= 4
            || id == 0
            || size == 0
            || u32_at(bytes, base + 12) != Some(0)
            || ![SHARED, LOCAL, PENDING].contains(&owner)
            || offset != cursor[storage_index]
            || size > spans[storage_index].saturating_sub(offset)
            || fields.iter().any(|field: &Field| field.id == id)
        {
            return Err(invalid());
        }
        if owner == PENDING {
            return Err(TransferError::PendingField(id));
        }
        if (id == BERRY_POWDER && (storage != 1 || size != 4))
            || (id == GAME_STATS && (storage != 0 || size % 4 != 0))
        {
            return Err(invalid());
        }
        cursor[storage_index] += size;
        fields.push(Field {
            id,
            storage,
            owner,
            offset,
            size,
        });
    }
    if cursor != spans {
        return Err(invalid());
    }
    for (id, storage, owner, offset, size) in [
        (MONEY, 0, SHARED, 0x490, 4),
        (COINS, 0, SHARED, 0x494, 2),
        (BAG, 0, SHARED, 0x560, 0x400),
        (KEY, 1, LOCAL, 0xb4, 4),
    ] {
        if !fields.iter().any(|field| {
            field.id == id
                && field.storage == storage
                && field.owner == owner
                && field.offset == offset
                && field.size == size
        }) {
            return Err(invalid());
        }
    }
    if !fields.iter().any(|field| {
        field.id == COOP_RECORD
            && field.storage == 3
            && field.owner == SHARED
            && field.offset == COOP_SAVE_OFFSET
            && field.size == super::v2::COOP_SAVE_V2_SIZE
    }) || fields
        .iter()
        .any(|field| field.storage == 3 && field.owner == SHARED && field.id != COOP_RECORD)
    {
        return Err(invalid());
    }
    Ok((fields, spans))
}

fn read_field(save: &ValidatedSaveV2, field: Field) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(field.size);
    for index in field.offset..field.offset + field.size {
        let (logical, offset) = locate(field.storage, index)?;
        result.push(*save.logical_sector_payload(logical)?.get(offset)?);
    }
    Some(result)
}

fn locate(storage: u8, offset: usize) -> Option<(u8, usize)> {
    match storage {
        0 => {
            let logical = 1 + offset / 3968;
            Some((u8::try_from(logical).ok()?, offset % 3968))
        }
        1 => Some((0, offset)),
        2 => {
            let logical = 6 + offset / 3968;
            Some((u8::try_from(logical).ok()?, offset % 3968))
        }
        _ => None,
    }
}

fn rekey(field: Field, bytes: &mut [u8], source: u32, destination: u32) {
    let delta = source ^ destination;
    match field.id {
        MONEY | BERRY_POWDER => {
            let value =
                u32::from_le_bytes(bytes.try_into().expect("validated money width")) ^ delta;
            bytes.copy_from_slice(&value.to_le_bytes());
        }
        COINS => {
            let value = u16::from_le_bytes(bytes.try_into().expect("validated coin width"))
                ^ u16::from_le_bytes([delta.to_le_bytes()[0], delta.to_le_bytes()[1]]);
            bytes.copy_from_slice(&value.to_le_bytes());
        }
        BAG => {
            // ItemSlot is a pair of u16 values; only quantity is encrypted.
            for slot in bytes.chunks_exact_mut(4) {
                let value = u16::from_le_bytes([slot[2], slot[3]])
                    ^ u16::from_le_bytes([delta.to_le_bytes()[0], delta.to_le_bytes()[1]]);
                slot[2..4].copy_from_slice(&value.to_le_bytes());
            }
        }
        GAME_STATS => {
            for stat in bytes.chunks_exact_mut(4) {
                let value =
                    u32::from_le_bytes(stat.try_into().expect("validated stat width")) ^ delta;
                stat.copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, GAME_STATS, rekey};

    #[test]
    fn game_statistics_rekey_every_word() {
        let source_key = 0x1234_5678;
        let destination_key = 0x9abc_def0;
        let mut bytes = [
            (123_u32 ^ source_key).to_le_bytes(),
            (45_678_u32 ^ source_key).to_le_bytes(),
        ]
        .concat();
        let field = Field {
            id: GAME_STATS,
            storage: 0,
            owner: 1,
            offset: 0,
            size: bytes.len(),
        };
        rekey(field, &mut bytes, source_key, destination_key);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()) ^ destination_key,
            123
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) ^ destination_key,
            45_678
        );
    }
}

/// Project one authenticated, fully classified shared schema into a destination
/// save. A pending ownership record rejects the entire transfer before writes.
pub(crate) fn project_shared_player(
    source: &ValidatedSaveV2,
    destination: &ValidatedSaveV2,
    source_descriptor: &[u8],
    destination_descriptor: &[u8],
) -> Result<ValidatedSaveV2, TransferError> {
    if source_descriptor != destination_descriptor {
        return Err(TransferError::DescriptorMismatch);
    }
    let (fields, _) = parse_descriptor(source_descriptor)?;
    if !source
        .character_lineage()
        .same_trainer(destination.character_lineage())
    {
        return Err(TransferError::CharacterMismatch);
    }
    let destination = destination.project_coop_from(source)?;
    let key = fields
        .iter()
        .find(|field| field.id == KEY)
        .ok_or(TransferError::InvalidDescriptor)?;
    let source_key = u32::from_le_bytes(
        read_field(source, *key)
            .ok_or(TransferError::InvalidDescriptor)?
            .try_into()
            .map_err(|_| TransferError::InvalidDescriptor)?,
    );
    let destination_key = u32::from_le_bytes(
        read_field(&destination, *key)
            .ok_or(TransferError::InvalidDescriptor)?
            .try_into()
            .map_err(|_| TransferError::InvalidDescriptor)?,
    );
    let mut data = Vec::new();
    let mut ranges = Vec::new();
    for field in fields
        .into_iter()
        .filter(|field| field.owner == SHARED && field.storage != 3)
    {
        let mut bytes = read_field(source, field).ok_or(TransferError::InvalidDescriptor)?;
        rekey(field, &mut bytes, source_key, destination_key);
        for (index, byte) in bytes.into_iter().enumerate() {
            let (logical, offset) = locate(field.storage, field.offset + index)
                .ok_or(TransferError::InvalidDescriptor)?;
            if offset >= LOGICAL_SECTOR_DATA_SIZES[usize::from(logical)] {
                return Err(TransferError::InvalidDescriptor);
            }
            data.push((logical, offset, byte));
        }
    }
    data.sort_unstable_by_key(|&(logical, offset, _)| (logical, offset));
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    for (logical, offset, byte) in data {
        if let Some(last) = ranges.last_mut() {
            let (last_logical, start, len): &mut (u8, usize, usize) = last;
            if *last_logical == logical && *start + *len == offset {
                chunks.last_mut().expect("range has chunk").push(byte);
                *len += 1;
                continue;
            }
        }
        ranges.push((logical, offset, 1));
        chunks.push(vec![byte]);
    }
    let approved: Vec<_> = ranges
        .iter()
        .map(|&(logical_id, offset, len)| ApprovedSectorSpan {
            logical_id,
            offset,
            len,
        })
        .collect();
    let patches: Vec<_> = ranges
        .iter()
        .zip(&chunks)
        .map(|(&(logical_id, offset, _), bytes)| SelectedSectorPatch {
            logical_id,
            offset,
            bytes,
        })
        .collect();
    Ok(destination.project_selected_sectors(&approved, &patches)?)
}

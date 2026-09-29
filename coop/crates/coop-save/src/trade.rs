//! Offline, byte preserving one-for-one party record exchange.

use crate::pokemon::{PARTY_POKEMON_SIZE, PARTY_SIZE, PartyPokemon};
use thiserror::Error;

use crate::{
    COOP_CRC_OFFSET, COOP_GENERATION_OFFSET, COOP_SAVE_OFFSET, COOP_SAVE_V1_SIZE,
    LOGICAL_SECTOR_DATA_SIZES, PokemonError, PokemonSlot, SAVE_BLOCK3_CHUNK_OFFSET,
    SAVE_BLOCK3_CHUNK_SIZE, SECTOR_CHECKSUM_OFFSET, SECTOR_COUNTER_OFFSET, SECTOR_SIZE,
    SECTORS_PER_SLOT, SaveError, SaveSlot, ValidatedSave, parse, sector_checksum,
};

const PARTY_OFFSET: usize = 0x238;
const FIRST_MAIL_ITEM: u16 = 199;
const LAST_MAIL_ITEM: u16 = 210;
const MAIL_NONE: u8 = 0xff;
const PARTY_MAIL_OFFSET: usize = 85;

/// Failure before either output is returned. Inputs are borrowed and never changed.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TradeError {
    #[error("{side} party record is invalid: {reason}")]
    Pokemon {
        side: &'static str,
        reason: PokemonError,
    },
    #[error("{side} party position {index} is outside its saved party count {count}")]
    OutsideParty {
        side: &'static str,
        index: usize,
        count: u8,
    },
    #[error("{side} party position {index} is empty")]
    Empty { side: &'static str, index: usize },
    #[error("{side} party position {index} holds mail or references a mail entry")]
    Mail { side: &'static str, index: usize },
    #[error("{side} save generation cannot be incremented")]
    GenerationOverflow { side: &'static str },
    #[error("{side} transformed save did not validate: {reason}")]
    OutputInvalid {
        side: &'static str,
        reason: SaveError,
    },
}

/// Swaps exact 100-byte occupied party records and returns two newly validated
/// save images. The next ROM slot and sector rotation are written for each
/// character; no friendship, evolution, Pokédex, or mail effects are modeled.
///
/// The caller must supply validated finalized images and enforce any trade
/// authorization, character ownership, and cloud revision concurrency rules.
pub fn trade_party_pokemon(
    left: &ValidatedSave,
    left_index: usize,
    right: &ValidatedSave,
    right_index: usize,
) -> Result<(ValidatedSave, ValidatedSave), TradeError> {
    let left_record = offered_record(left, left_index, "left")?;
    let right_record = offered_record(right, right_index, "right")?;
    let left_generation = left
        .coop()
        .save_generation
        .checked_add(1)
        .ok_or(TradeError::GenerationOverflow { side: "left" })?;
    let right_generation = right
        .coop()
        .save_generation
        .checked_add(1)
        .ok_or(TradeError::GenerationOverflow { side: "right" })?;

    let left_bytes = next_image(left, left_index, &right_record, left_generation);
    let right_bytes = next_image(right, right_index, &left_record, right_generation);
    let left_output =
        parse(&left_bytes, left.coop().registry).map_err(|reason| TradeError::OutputInvalid {
            side: "left",
            reason,
        })?;
    let right_output =
        parse(&right_bytes, right.coop().registry).map_err(|reason| TradeError::OutputInvalid {
            side: "right",
            reason,
        })?;
    Ok((left_output, right_output))
}

fn offered_record(
    save: &ValidatedSave,
    index: usize,
    side: &'static str,
) -> Result<[u8; PARTY_POKEMON_SIZE], TradeError> {
    let count = save
        .party_count()
        .map_err(|reason| TradeError::Pokemon { side, reason })?;
    if index >= PARTY_SIZE {
        return Err(TradeError::Pokemon {
            side,
            reason: PokemonError::PartyIndex { index },
        });
    }
    if index >= usize::from(count) {
        return Err(TradeError::OutsideParty { side, index, count });
    }
    let slot = save
        .party_pokemon(index)
        .map_err(|reason| TradeError::Pokemon { side, reason })?;
    let PokemonSlot::Occupied(record) = slot else {
        return Err(TradeError::Empty { side, index });
    };
    if party_record_holds_mail(&record) {
        return Err(TradeError::Mail { side, index });
    }
    Ok(record.raw)
}

/// Whether a party record holds a mail item or references a mail entry. Mail
/// lives in the sender's SaveBlock1 mail table, so such a record cannot move
/// to another save byte for byte; the ROM refuses it in a trade commit.
#[must_use]
pub fn party_record_holds_mail(record: &PartyPokemon) -> bool {
    record.raw[PARTY_MAIL_OFFSET] != MAIL_NONE
        || (FIRST_MAIL_ITEM..=LAST_MAIL_ITEM).contains(&record.identity.held_item)
}

fn next_image(
    save: &ValidatedSave,
    index: usize,
    incoming: &[u8; PARTY_POKEMON_SIZE],
    generation: u32,
) -> Vec<u8> {
    let mut image = save.raw_bytes().to_vec();
    let next_counter = save.counter().wrapping_add(1);
    let next_slot = SaveSlot::from_counter(next_counter);
    let selected_base = save.selected_slot().index() * SECTORS_PER_SLOT * SECTOR_SIZE;
    let current_zero = (save.logical_sector_offsets[0] - selected_base) / SECTOR_SIZE;
    let next_zero = (current_zero + 1) % SECTORS_PER_SLOT;
    let destination_base = next_slot.index() * SECTORS_PER_SLOT * SECTOR_SIZE;
    let destination_offsets = std::array::from_fn(|logical| {
        destination_base + ((next_zero + logical) % SECTORS_PER_SLOT) * SECTOR_SIZE
    });

    for (logical, &destination) in destination_offsets.iter().enumerate() {
        let source = save.logical_sector_offsets[logical];
        image[destination..destination + SECTOR_SIZE]
            .copy_from_slice(&save.raw_bytes()[source..source + SECTOR_SIZE]);
        image[destination + SECTOR_COUNTER_OFFSET..destination + SECTOR_COUNTER_OFFSET + 4]
            .copy_from_slice(&next_counter.to_le_bytes());
    }

    write_logical(
        &mut image,
        &destination_offsets,
        1,
        PARTY_OFFSET + index * PARTY_POKEMON_SIZE,
        incoming,
    );
    let generation_offset = COOP_SAVE_OFFSET + COOP_GENERATION_OFFSET;
    write_block3(
        &mut image,
        &destination_offsets,
        generation_offset,
        &generation.to_le_bytes(),
    );
    let mut coop = [0_u8; COOP_SAVE_V1_SIZE];
    for (index, byte) in coop.iter_mut().enumerate() {
        let offset = COOP_SAVE_OFFSET + index;
        *byte = image[destination_offsets[offset / SAVE_BLOCK3_CHUNK_SIZE]
            + SAVE_BLOCK3_CHUNK_OFFSET
            + offset % SAVE_BLOCK3_CHUNK_SIZE];
    }
    let crc = crc32fast::hash(&coop[..COOP_CRC_OFFSET]);
    write_block3(
        &mut image,
        &destination_offsets,
        COOP_SAVE_OFFSET + COOP_CRC_OFFSET,
        &crc.to_le_bytes(),
    );

    for (logical, &destination) in destination_offsets.iter().enumerate() {
        let checksum =
            sector_checksum(&image[destination..destination + LOGICAL_SECTOR_DATA_SIZES[logical]]);
        image[destination + SECTOR_CHECKSUM_OFFSET..destination + SECTOR_CHECKSUM_OFFSET + 2]
            .copy_from_slice(&checksum.to_le_bytes());
    }
    image
}

fn write_logical(
    image: &mut [u8],
    sectors: &[usize; SECTORS_PER_SLOT],
    first_sector: usize,
    offset: usize,
    data: &[u8],
) {
    for (index, byte) in data.iter().copied().enumerate() {
        let position = offset + index;
        image[sectors[first_sector + position / SAVE_BLOCK3_CHUNK_OFFSET]
            + position % SAVE_BLOCK3_CHUNK_OFFSET] = byte;
    }
}

fn write_block3(image: &mut [u8], sectors: &[usize; SECTORS_PER_SLOT], offset: usize, data: &[u8]) {
    for (index, byte) in data.iter().copied().enumerate() {
        let position = offset + index;
        image[sectors[position / SAVE_BLOCK3_CHUNK_SIZE]
            + SAVE_BLOCK3_CHUNK_OFFSET
            + position % SAVE_BLOCK3_CHUNK_SIZE] = byte;
    }
}

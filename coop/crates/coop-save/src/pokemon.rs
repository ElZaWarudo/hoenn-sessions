//! Read-only Pokémon records from the selected committed save slot.
//!
//! Layouts follow `include/global.h`, `include/pokemon.h`,
//! `include/pokemon_storage_system.h`, and the permutation in `src/pokemon.c`.

use thiserror::Error;

use crate::ValidatedSave;

/// Serialized `BoxPokemon` size.
pub const BOX_POKEMON_SIZE: usize = 80;
/// Serialized party `Pokemon` size.
pub const PARTY_POKEMON_SIZE: usize = 100;
/// Number of party positions.
pub const PARTY_SIZE: usize = 6;
/// Number of PC boxes.
pub const BOX_COUNT: usize = 14;
/// Number of positions in one PC box.
pub const BOX_SIZE: usize = 30;

const PARTY_OFFSET: usize = 0x238;
const PARTY_COUNT_OFFSET: usize = 0x234;
// PokemonStorage.currentBox is one byte; BoxPokemon's u32 fields align boxes to 4.
// The C layout puts boxNames at 0x8344, after 14*30*80 bytes of boxes.
const PC_BOXES_OFFSET: usize = 4;
const SECURE_OFFSET: usize = 32;
const SUBSTRUCT_SIZE: usize = 12;

// sSubstructOffsets in src/pokemon.c; rows are logical substruct types 0..3.
const SUBSTRUCT_OFFSETS: [[usize; 24]; 4] = [
    [
        0, 0, 0, 0, 0, 0, 1, 1, 2, 3, 2, 3, 1, 1, 2, 3, 2, 3, 1, 1, 2, 3, 2, 3,
    ],
    [
        1, 1, 2, 3, 2, 3, 0, 0, 0, 0, 0, 0, 2, 3, 1, 1, 3, 2, 2, 3, 1, 1, 3, 2,
    ],
    [
        2, 3, 1, 1, 3, 2, 2, 3, 1, 1, 3, 2, 0, 0, 0, 0, 0, 0, 3, 2, 3, 2, 1, 1,
    ],
    [
        3, 2, 3, 2, 1, 1, 3, 2, 3, 2, 1, 1, 3, 2, 3, 2, 1, 1, 0, 0, 0, 0, 0, 0,
    ],
];

/// Decoded values that identify and compare a Pokémon without rewriting it.
/// Names retain the game's original encoding and padding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PokemonIdentity {
    pub personality: u32,
    pub ot_id: u32,
    pub nickname: [u8; 10],
    pub ot_name: [u8; 7],
    pub species: u16,
    pub held_item: u16,
    pub moves: [u16; 4],
    pub is_egg: bool,
    pub is_bad_egg: bool,
    /// Stored checksum, verified against all four decrypted substructs.
    pub checksum: u16,
    /// Four decrypted 12-byte substructs in logical type order.
    pub substructs: [[u8; SUBSTRUCT_SIZE]; 4],
}

/// An occupied record keeps the exact bytes alongside decoded identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PokemonRecord<const N: usize> {
    pub raw: [u8; N],
    pub identity: PokemonIdentity,
}

pub type BoxPokemon = PokemonRecord<BOX_POKEMON_SIZE>;
pub type PartyPokemon = PokemonRecord<PARTY_POKEMON_SIZE>;

/// Empty slots retain their exact bytes too.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PokemonSlot<const N: usize> {
    Empty { raw: [u8; N] },
    Occupied(PokemonRecord<N>),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PokemonError {
    #[error("saved party count {count} exceeds {PARTY_SIZE}")]
    PartyCount { count: u8 },
    #[error("party position {index} is outside 0..{PARTY_SIZE}")]
    PartyIndex { index: usize },
    #[error("PC box {index} is outside 0..{BOX_COUNT}")]
    BoxIndex { index: usize },
    #[error("PC position {index} is outside 0..{BOX_SIZE}")]
    BoxPosition { index: usize },
    #[error("nonempty Pokémon has checksum {actual:#06x}, expected {expected:#06x}")]
    Checksum { expected: u16, actual: u16 },
    #[error("nonzero Pokémon record has no species flag or species value")]
    MissingSpecies,
}

impl ValidatedSave {
    /// Reads the saved active-party count. A trade offer must use an index
    /// below this count as well as an occupied, checksum-valid record.
    pub fn party_count(&self) -> Result<u8, PokemonError> {
        let count = self
            .save_block1_range(PARTY_COUNT_OFFSET, 1)
            .expect("party count fits SaveBlock1")[0];
        if usize::from(count) > PARTY_SIZE {
            return Err(PokemonError::PartyCount { count });
        }
        Ok(count)
    }

    /// Reads one saved party position, including its exact 100-byte record.
    pub fn party_pokemon(
        &self,
        index: usize,
    ) -> Result<PokemonSlot<PARTY_POKEMON_SIZE>, PokemonError> {
        if index >= PARTY_SIZE {
            return Err(PokemonError::PartyIndex { index });
        }
        let offset = PARTY_OFFSET + index * PARTY_POKEMON_SIZE;
        let raw: [u8; PARTY_POKEMON_SIZE] = self
            .save_block1_range(offset, PARTY_POKEMON_SIZE)
            .expect("bounded party layout fits SaveBlock1")
            .try_into()
            .unwrap();
        decode_record(raw)
    }

    /// Reads one PC position, including its exact 80-byte record.
    pub fn pc_pokemon(
        &self,
        box_index: usize,
        position: usize,
    ) -> Result<PokemonSlot<BOX_POKEMON_SIZE>, PokemonError> {
        if box_index >= BOX_COUNT {
            return Err(PokemonError::BoxIndex { index: box_index });
        }
        if position >= BOX_SIZE {
            return Err(PokemonError::BoxPosition { index: position });
        }
        let offset = PC_BOXES_OFFSET + (box_index * BOX_SIZE + position) * BOX_POKEMON_SIZE;
        let raw: [u8; BOX_POKEMON_SIZE] = self
            .pc_storage_range(offset, BOX_POKEMON_SIZE)
            .expect("bounded PC layout fits storage sectors")
            .try_into()
            .unwrap();
        decode_record(raw)
    }
}

/// Decodes one standalone 100-byte party record exactly as a saved party
/// slot is decoded: checksum-verified decryption, species presence, and the
/// egg flags. The bytes are never rewritten.
///
/// # Errors
///
/// Returns [`PokemonError::Checksum`] or [`PokemonError::MissingSpecies`] for
/// a nonempty record that is not structurally valid.
pub fn decode_party_record(
    raw: [u8; PARTY_POKEMON_SIZE],
) -> Result<PokemonSlot<PARTY_POKEMON_SIZE>, PokemonError> {
    decode_record(raw)
}

/// Current HP of a decoded party record (`struct Pokemon.hp`, offset 86).
#[must_use]
pub fn party_record_hp(raw: &[u8; PARTY_POKEMON_SIZE]) -> u16 {
    u16::from_le_bytes([raw[PARTY_HP_OFFSET], raw[PARTY_HP_OFFSET + 1]])
}

const PARTY_HP_OFFSET: usize = 86;

fn decode_record<const N: usize>(raw: [u8; N]) -> Result<PokemonSlot<N>, PokemonError> {
    // ZeroMonData clears the boxed portion and sets party mail to MAIL_NONE
    // (0xff). An unused party slot is therefore not necessarily all zero.
    if raw[..BOX_POKEMON_SIZE].iter().all(|byte| *byte == 0)
        && raw[BOX_POKEMON_SIZE..]
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == 0 || (index == 5 && *byte == 0xff))
    {
        return Ok(PokemonSlot::Empty { raw });
    }
    let box_data = &raw[..BOX_POKEMON_SIZE];
    let personality = u32::from_le_bytes(box_data[0..4].try_into().unwrap());
    let ot_id = u32::from_le_bytes(box_data[4..8].try_into().unwrap());
    let key = personality ^ ot_id;
    let mut decrypted = [0_u8; 48];
    let mut checksum = 0_u16;
    for (source, target) in box_data[SECURE_OFFSET..BOX_POKEMON_SIZE]
        .chunks_exact(4)
        .zip(decrypted.chunks_exact_mut(4))
    {
        let word = u32::from_le_bytes(source.try_into().unwrap()) ^ key;
        target.copy_from_slice(&word.to_le_bytes());
        checksum = checksum
            .wrapping_add(word as u16)
            .wrapping_add((word >> 16) as u16);
    }
    let actual = u16::from_le_bytes(box_data[28..30].try_into().unwrap());
    if checksum != actual {
        return Err(PokemonError::Checksum {
            expected: checksum,
            actual,
        });
    }

    let mut substructs = [[0_u8; SUBSTRUCT_SIZE]; 4];
    let permutation = (personality % 24) as usize;
    for (kind, substruct) in substructs.iter_mut().enumerate() {
        let physical = SUBSTRUCT_OFFSETS[kind][permutation];
        let start = physical * SUBSTRUCT_SIZE;
        substruct.copy_from_slice(&decrypted[start..start + SUBSTRUCT_SIZE]);
    }
    let growth = &substructs[0];
    let attacks = &substructs[1];
    let species = u16::from_le_bytes(growth[0..2].try_into().unwrap()) & 0x7ff;
    if box_data[19] & 0x02 == 0 || species == 0 {
        return Err(PokemonError::MissingSpecies);
    }
    let moves = std::array::from_fn(|slot| {
        u16::from_le_bytes(attacks[slot * 2..slot * 2 + 2].try_into().unwrap()) & 0x7ff
    });
    let identity = PokemonIdentity {
        personality,
        ot_id,
        nickname: box_data[8..18].try_into().unwrap(),
        ot_name: box_data[20..27].try_into().unwrap(),
        species,
        held_item: u16::from_le_bytes(growth[2..4].try_into().unwrap()) & 0x3ff,
        moves,
        is_egg: box_data[19] & 0x04 != 0
            || u32::from_le_bytes(substructs[3][4..8].try_into().unwrap()) & (1 << 30) != 0,
        is_bad_egg: box_data[19] & 0x01 != 0,
        checksum: actual,
        substructs,
    };
    Ok(PokemonSlot::Occupied(PokemonRecord { raw, identity }))
}

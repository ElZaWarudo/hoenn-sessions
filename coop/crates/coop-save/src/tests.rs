use super::*;

const TEST_REGISTRY: RegistryContract = RegistryContract::new(7, [0xa5; 16]);

#[test]
fn stock_mgba_first_save_matches_linked_rom_sector_checksums() {
    // Genuine first save through the ordinary menu on the Character build,
    // with stock mGBA 0.10.5. Unlike write_slot, this fixture does not derive
    // its checksums from the validator's own size table.
    let bytes = include_bytes!("../tests/fixtures/stock-mgba-first-save.sav");
    let registry = RegistryContract::new(
        1,
        [
            0x43, 0x91, 0x88, 0x33, 0xde, 0xc6, 0x46, 0xd6, 0xa5, 0x83, 0xd1, 0x24, 0x68, 0x6c,
            0x85, 0x40,
        ],
    );
    let save = parse(bytes, registry).expect("stock ROM save must validate");
    assert_eq!(save.coop().save_generation, 1);
    assert!(save.coop().online_eligible());
    assert!(save.rtc_trailer().is_some());
    // A new game starts with 3000; this pins the money and key offsets to a
    // genuine save rather than to the validator's own synthetic layout.
    assert_eq!(save.money(), Some(3000));
    assert_eq!(save.party_count(), Ok(0));
}

#[test]
fn canonical_progress_identifies_trainer_clears_for_battle_eligibility() {
    let save = parse(&valid_image(20, 21), TEST_REGISTRY).expect("valid save");
    let first = resolve_ordinal(IdentityKind::Trainer, 0).expect("registered trainer");
    let second = resolve_ordinal(IdentityKind::Trainer, 1).expect("registered trainer");
    let first = TrainerInstanceId::parse(first.qualified_id).expect("qualified trainer");
    let second = TrainerInstanceId::parse(second.qualified_id).expect("qualified trainer");

    assert_eq!(save.coop().defeated_trainer(&first), Ok(true));
    assert_eq!(save.coop().defeated_trainer(&second), Ok(false));
    assert_eq!(
        save.coop()
            .progress_for(RegionId::Hoenn)
            .map(|record| record.story_checkpoint),
        Some(100),
    );
    assert!(save.coop().progress_for(RegionId::Unspecified).is_none());
    let unknown = TrainerInstanceId::parse("HOENN:TRAINER_NOT_REGISTERED")
        .expect("well-formed unknown trainer");
    assert!(save.coop().defeated_trainer(&unknown).is_err());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn coop_payload(generation: u32) -> [u8; COOP_SAVE_V1_SIZE] {
    let mut payload = [0_u8; COOP_SAVE_V1_SIZE];
    write_u32(&mut payload, COOP_MAGIC_OFFSET, COOP_SAVE_V1_MAGIC);
    write_u16(
        &mut payload,
        COOP_SCHEMA_OFFSET,
        COOP_SAVE_V1_SCHEMA_VERSION,
    );
    write_u16(
        &mut payload,
        COOP_STRUCT_SIZE_OFFSET,
        u16::try_from(COOP_SAVE_V1_SIZE).unwrap(),
    );
    write_u32(
        &mut payload,
        COOP_REGISTRY_VERSION_OFFSET,
        TEST_REGISTRY.version,
    );
    payload[COOP_REGISTRY_DIGEST_OFFSET..COOP_REGISTRY_DIGEST_OFFSET + 16]
        .copy_from_slice(&TEST_REGISTRY.digest);
    write_u32(&mut payload, COOP_GENERATION_OFFSET, generation);
    write_u32(
        &mut payload,
        COOP_STATUS_FLAGS_OFFSET,
        COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS,
    );

    for record in 0..4 {
        let offset = COOP_REGIONAL_PROGRESS_OFFSET + record * COOP_REGIONAL_PROGRESS_SIZE;
        payload[offset] = u8::try_from(record + 1).unwrap();
        let badge_mask = if record == 3 { 0 } else { 1 << record };
        write_u16(&mut payload, offset + 2, badge_mask);
        write_u32(
            &mut payload,
            offset + 4,
            100 + u32::try_from(record).unwrap(),
        );
    }
    payload[COOP_TRAINER_BITS_OFFSET] = 0x81;
    payload[COOP_EVENT_BITS_OFFSET] = 0x0a;
    payload[COOP_FLY_BITS_OFFSET] = 0x05;
    payload[COOP_GYM_BITS_OFFSET + 2] = 0x80;
    seal_coop_payload(&mut payload);
    payload
}

fn seal_coop_payload(payload: &mut [u8; COOP_SAVE_V1_SIZE]) {
    let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
    write_u32(payload, COOP_CRC_OFFSET, crc);
}

fn write_slot(
    flash: &mut [u8],
    slot: SaveSlot,
    counter: u32,
    rotation: usize,
    payload: &[u8; COOP_SAVE_V1_SIZE],
) {
    let mut save_block3 = [0xff; SAVE_BLOCK3_CAPACITY];
    save_block3[COOP_SAVE_OFFSET..COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE].copy_from_slice(payload);
    let slot_base = slot.index() * SECTORS_PER_SLOT * SECTOR_SIZE;

    for physical in 0..SECTORS_PER_SLOT {
        let logical = (physical + rotation) % SECTORS_PER_SLOT;
        let offset = slot_base + physical * SECTOR_SIZE;
        let sector = &mut flash[offset..offset + SECTOR_SIZE];
        sector.fill(0xff);
        for (index, byte) in sector[..SAVE_BLOCK3_CHUNK_OFFSET].iter_mut().enumerate() {
            *byte = u8::try_from(logical).unwrap().wrapping_mul(17) ^ index.to_le_bytes()[0];
        }
        let source = logical * SAVE_BLOCK3_CHUNK_SIZE;
        sector[SAVE_BLOCK3_CHUNK_OFFSET..SAVE_BLOCK3_CHUNK_OFFSET + SAVE_BLOCK3_CHUNK_SIZE]
            .copy_from_slice(&save_block3[source..source + SAVE_BLOCK3_CHUNK_SIZE]);
        write_u16(sector, SECTOR_ID_OFFSET, u16::try_from(logical).unwrap());
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[logical]]);
        write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
        write_u32(sector, SECTOR_SIGNATURE_OFFSET, SECTOR_SIGNATURE);
        write_u32(sector, SECTOR_COUNTER_OFFSET, counter);
    }
}

fn valid_image(first_counter: u32, second_counter: u32) -> Vec<u8> {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let payload = coop_payload(41);
    write_slot(&mut bytes, SaveSlot::First, first_counter, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, second_counter, 11, &payload);
    bytes
}

fn physical_sector_mut(bytes: &mut [u8], slot: SaveSlot, physical: usize) -> &mut [u8] {
    let offset = (slot.index() * SECTORS_PER_SLOT + physical) * SECTOR_SIZE;
    &mut bytes[offset..offset + SECTOR_SIZE]
}

fn logical_sector_mut(bytes: &mut [u8], slot: SaveSlot, logical: usize) -> &mut [u8] {
    let physical = (0..SECTORS_PER_SLOT)
        .find(|physical| {
            let offset = (slot.index() * SECTORS_PER_SLOT + physical) * SECTOR_SIZE;
            usize::from(read_u16(bytes, offset + SECTOR_ID_OFFSET)) == logical
        })
        .expect("fixture contains every logical sector");
    physical_sector_mut(bytes, slot, physical)
}

fn write_logical_range(
    bytes: &mut [u8],
    slot: SaveSlot,
    first_sector: usize,
    offset: usize,
    data: &[u8],
) {
    for (index, byte) in data.iter().enumerate() {
        let position = offset + index;
        let logical = first_sector + position / SAVE_BLOCK3_CHUNK_OFFSET;
        let sector = logical_sector_mut(bytes, slot, logical);
        sector[position % SAVE_BLOCK3_CHUNK_OFFSET] = *byte;
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[logical]]);
        write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
    }
}

fn write_save_block3_range(bytes: &mut [u8], slot: SaveSlot, offset: usize, data: &[u8]) {
    for (index, byte) in data.iter().enumerate() {
        let position = offset + index;
        let logical = position / SAVE_BLOCK3_CHUNK_SIZE;
        let sector = logical_sector_mut(bytes, slot, logical);
        sector[SAVE_BLOCK3_CHUNK_OFFSET + position % SAVE_BLOCK3_CHUNK_SIZE] = *byte;
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[logical]]);
        write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
    }
}

fn write_csp1_trainer_bit(bytes: &mut [u8], slot: SaveSlot, ordinal: u16, defeated: bool) {
    let ordinal = usize::from(ordinal);
    let byte_offset = COOP_SAVE_OFFSET + COOP_TRAINER_BITS_OFFSET + ordinal / 8;
    let logical = byte_offset / SAVE_BLOCK3_CHUNK_SIZE;
    let sector = logical_sector_mut(bytes, slot, logical);
    let inside = SAVE_BLOCK3_CHUNK_OFFSET + byte_offset % SAVE_BLOCK3_CHUNK_SIZE;
    let mut byte = sector[inside];
    if defeated {
        byte |= 1 << (ordinal % 8);
    } else {
        byte &= !(1 << (ordinal % 8));
    }
    write_save_block3_range(bytes, slot, byte_offset, &[byte]);

    let mut payload = [0_u8; COOP_SAVE_V1_SIZE];
    for (index, byte) in payload.iter_mut().enumerate() {
        let position = COOP_SAVE_OFFSET + index;
        let logical = position / SAVE_BLOCK3_CHUNK_SIZE;
        let sector = logical_sector_mut(bytes, slot, logical);
        *byte = sector[SAVE_BLOCK3_CHUNK_OFFSET + position % SAVE_BLOCK3_CHUNK_SIZE];
    }
    let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
    write_save_block3_range(
        bytes,
        slot,
        COOP_SAVE_OFFSET + COOP_CRC_OFFSET,
        &crc.to_le_bytes(),
    );
}

fn write_wally_evidence(
    bytes: &mut [u8],
    slot: SaveSlot,
    defeated_wally_flag: bool,
    victory_road_1f_state: u16,
    entrance_wally_hidden: bool,
) {
    write_logical_range(
        bytes,
        slot,
        1,
        SAVE_BLOCK1_VARS_OFFSET + 2 * (VAR_VICTORY_ROAD_1F_STATE - 0x4000),
        &victory_road_1f_state.to_le_bytes(),
    );
    for (flag_id, value) in [
        (FLAG_DEFEATED_WALLY_VICTORY_ROAD, defeated_wally_flag),
        (FLAG_HIDE_VICTORY_ROAD_ENTRANCE_WALLY, entrance_wally_hidden),
    ] {
        let offset = SAVE_BLOCK1_FLAGS_OFFSET + flag_id / 8;
        let mut byte = logical_sector_mut(bytes, slot, 1 + offset / SAVE_BLOCK3_CHUNK_OFFSET)
            [offset % SAVE_BLOCK3_CHUNK_OFFSET];
        if value {
            byte |= 1 << (flag_id % 8);
        } else {
            byte &= !(1 << (flag_id % 8));
        }
        write_logical_range(bytes, slot, 1, offset, &[byte]);
    }
}

fn write_briney_evidence(
    bytes: &mut [u8],
    slot: SaveSlot,
    map_group: u8,
    map_num: u8,
    board_state: u16,
    norman_match_call: bool,
    dewford_briney_hidden: bool,
    dewford_boat_hidden: bool,
    route104_boat_hidden: bool,
) {
    write_logical_range(
        bytes,
        slot,
        1,
        SAVE_BLOCK1_LOCATION_OFFSET,
        &[map_group, map_num],
    );
    write_logical_range(
        bytes,
        slot,
        1,
        SAVE_BLOCK1_BOARD_BRINEY_BOAT_STATE_OFFSET,
        &board_state.to_le_bytes(),
    );
    let norman_flags = norman_match_call
        .then_some(FLAG_NORMAN_MATCH_CALL_MASK)
        .unwrap_or(0);
    write_logical_range(
        bytes,
        slot,
        1,
        FLAG_NORMAN_MATCH_CALL_BYTE_OFFSET,
        &[norman_flags],
    );
    let dewford_flags = (dewford_briney_hidden
        .then_some(FLAG_HIDE_MR_BRINEY_DEWFORD_MASK)
        .unwrap_or(0))
        | (dewford_boat_hidden
            .then_some(FLAG_HIDE_MR_BRINEY_BOAT_DEWFORD_MASK)
            .unwrap_or(0))
        | (route104_boat_hidden
            .then_some(FLAG_HIDE_ROUTE_104_MR_BRINEY_BOAT_MASK)
            .unwrap_or(0));
    write_logical_range(
        bytes,
        slot,
        1,
        FLAG_BRINEY_DEWFORD_BYTE_OFFSET,
        &[dewford_flags],
    );
}

fn write_bill_evidence(
    bytes: &mut [u8],
    slot: SaveSlot,
    map: (u8, u8),
    scenes: (u16, u16, u16),
    flags: (bool, bool, bool, bool),
) {
    write_logical_range(bytes, slot, 1, SAVE_BLOCK1_LOCATION_OFFSET, &[map.0, map.1]);
    for (id, value) in [(0x4171, scenes.0), (0x4175, scenes.1), (0x4176, scenes.2)] {
        write_logical_range(
            bytes,
            slot,
            1,
            SAVE_BLOCK1_VARS_OFFSET + 2 * (id - 0x4000),
            &value.to_le_bytes(),
        );
    }
    for (id, value) in [
        (0x15f, flags.0),
        (0x15e, flags.1),
        (0x852, flags.2),
        (0x853, flags.3),
    ] {
        let offset = SAVE_BLOCK1_FLAGS_OFFSET + id / 8;
        let mut byte = logical_sector_mut(bytes, slot, 1 + offset / SAVE_BLOCK3_CHUNK_OFFSET)
            [offset % SAVE_BLOCK3_CHUNK_OFFSET];
        if value {
            byte |= 1 << (id % 8);
        } else {
            byte &= !(1 << (id % 8));
        }
        write_logical_range(bytes, slot, 1, offset, &[byte]);
    }
}

#[test]
fn bill_story_evidence_requires_selected_slot_final_scene_and_exact_map() {
    let mut bytes = valid_image(20, 21);
    write_bill_evidence(
        &mut bytes,
        SaveSlot::First,
        (64, 0),
        (2, 3, 1),
        (true, true, false, true),
    );
    write_bill_evidence(
        &mut bytes,
        SaveSlot::Second,
        (64, 4),
        (2, 2, 0),
        (true, false, false, true),
    );
    let selected = parse(&bytes, TEST_REGISTRY).unwrap().bill_voyage_evidence();
    assert!(!selected.is_cinnabar_to_one_post_scene());
    write_bill_evidence(
        &mut bytes,
        SaveSlot::Second,
        (64, 0),
        (2, 3, 1),
        (true, true, false, true),
    );
    assert!(
        parse(&bytes, TEST_REGISTRY)
            .unwrap()
            .bill_voyage_evidence()
            .is_cinnabar_to_one_post_scene()
    );
    write_bill_evidence(
        &mut bytes,
        SaveSlot::Second,
        (37, 8),
        (4, 3, 3),
        (true, true, true, false),
    );
    assert!(
        parse(&bytes, TEST_REGISTRY)
            .unwrap()
            .bill_voyage_evidence()
            .is_one_to_cinnabar_post_scene()
    );
}

#[test]
fn wally_victory_road_evidence_uses_selected_slot_and_canonical_csp1_trainer() {
    let wally = resolve_ordinal(IdentityKind::Trainer, WALLY_VICTORY_ROAD_TRAINER_ORDINAL)
        .expect("Wally trainer is registered");
    assert_eq!(wally.ordinal, Some(518));
    assert_eq!(wally.qualified_id, "HOENN:TRAINER_WALLY_1");

    let mut bytes = valid_image(20, 21);
    // A stale completed first slot must not be used when the ROM selects the
    // newer second slot. This also proves the fixture has valid sector and
    // CSP1 checksums after evidence writes.
    write_wally_evidence(&mut bytes, SaveSlot::First, true, 1, false);
    write_csp1_trainer_bit(
        &mut bytes,
        SaveSlot::First,
        WALLY_VICTORY_ROAD_TRAINER_ORDINAL,
        true,
    );
    write_wally_evidence(&mut bytes, SaveSlot::Second, false, 0, true);

    let save = parse(&bytes, TEST_REGISTRY).expect("valid pre-battle save");
    assert_eq!(save.selected_slot(), SaveSlot::Second);
    let pre_battle = save.wally_victory_road_evidence();
    assert_eq!(pre_battle.victory_road_1f_state, 0);
    assert!(!pre_battle.defeated_wally_flag);
    assert!(pre_battle.entrance_wally_hidden);
    assert!(!pre_battle.canonical_trainer_defeated);
    assert!(!pre_battle.is_post_battle());

    // The ROM's story flag and state alone are insufficient. Set those on the
    // selected slot first, while leaving canonical CSP1 ordinal 518 clear.
    write_wally_evidence(&mut bytes, SaveSlot::Second, true, 1, false);
    let legacy_only = parse(&bytes, TEST_REGISTRY)
        .expect("valid legacy-only save")
        .wally_victory_road_evidence();
    assert!(legacy_only.defeated_wally_flag);
    assert!(!legacy_only.canonical_trainer_defeated);
    assert!(!legacy_only.is_post_battle());

    write_csp1_trainer_bit(
        &mut bytes,
        SaveSlot::Second,
        WALLY_VICTORY_ROAD_TRAINER_ORDINAL,
        true,
    );
    let post_battle = parse(&bytes, TEST_REGISTRY)
        .expect("valid post-battle save")
        .wally_victory_road_evidence();
    assert_eq!(post_battle.victory_road_1f_state, 1);
    assert!(post_battle.defeated_wally_flag);
    assert!(!post_battle.entrance_wally_hidden);
    assert!(post_battle.canonical_trainer_defeated);
    assert!(post_battle.is_post_battle());
}

#[test]
fn extracts_first_briney_voyage_post_scene_evidence_from_selected_slot() {
    let mut bytes = valid_image(20, 21);
    write_briney_evidence(
        &mut bytes,
        SaveSlot::Second,
        7,
        13,
        0,
        true,
        false,
        false,
        true,
    );

    let save = parse(&bytes, TEST_REGISTRY).expect("valid save");
    let evidence = save.briney_voyage_evidence();
    assert_eq!(evidence.map_group, 7);
    assert_eq!(evidence.map_num, 13);
    assert_eq!(evidence.board_briney_boat_state, 0);
    assert!(evidence.norman_match_call_enabled);
    assert!(!evidence.dewford_briney_hidden);
    assert!(!evidence.dewford_boat_hidden);
    assert!(evidence.route104_boat_hidden);
    assert!(evidence.is_first_voyage_post_scene_at(7, 13));
    assert!(!evidence.is_first_voyage_post_scene_at(7, 14));
}

#[test]
fn rejects_incomplete_briney_scene_evidence() {
    let mut bytes = valid_image(20, 21);
    write_briney_evidence(
        &mut bytes,
        SaveSlot::Second,
        7,
        13,
        1,
        false,
        true,
        true,
        false,
    );

    let evidence = parse(&bytes, TEST_REGISTRY)
        .expect("valid save")
        .briney_voyage_evidence();
    assert_eq!(evidence.board_briney_boat_state, 1);
    assert!(!evidence.norman_match_call_enabled);
    assert!(evidence.dewford_briney_hidden);
    assert!(evidence.dewford_boat_hidden);
    assert!(!evidence.route104_boat_hidden);
    assert!(!evidence.is_first_voyage_post_scene_at(7, 13));
}

#[test]
fn reads_briney_evidence_only_from_the_rom_selected_slot() {
    let mut bytes = valid_image(20, 21);
    write_briney_evidence(
        &mut bytes,
        SaveSlot::First,
        7,
        13,
        0,
        true,
        false,
        false,
        true,
    );
    // The newer second slot is the mid-scene state. A stale completed first
    // slot must not be used as evidence for the selected save.
    write_briney_evidence(
        &mut bytes,
        SaveSlot::Second,
        8,
        14,
        1,
        false,
        true,
        true,
        false,
    );

    let save = parse(&bytes, TEST_REGISTRY).expect("valid save");
    assert_eq!(save.selected_slot(), SaveSlot::Second);
    let evidence = save.briney_voyage_evidence();
    assert_eq!((evidence.map_group, evidence.map_num), (8, 14));
    assert!(!evidence.is_first_voyage_post_scene_at(7, 13));
}

#[test]
fn corrupt_save_has_no_briney_evidence() {
    let mut bytes = valid_image(20, 21);
    for slot in [SaveSlot::First, SaveSlot::Second] {
        write_u32(
            physical_sector_mut(&mut bytes, slot, 0),
            SECTOR_SIGNATURE_OFFSET,
            0,
        );
    }

    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::NoValidSlot { .. })
    ));
}

fn golden_box_pokemon() -> [u8; 80] {
    // Raw BoxPokemon from the ROM's "BoxPokemon encryption works" test.
    let words = [
        990384375_u32,
        2948624514,
        3907508686,
        14410461,
        35316705,
        3907508686,
        64742109,
        718729,
        3102307966,
        2160206402,
        49956971,
        2495766612,
        1424318580,
        273408756,
        2371630199,
        2708871082,
        3059937332,
        2529190026,
        2290634828,
        2870614922,
    ];
    let mut raw = [0_u8; 80];
    for (index, word) in words.iter().enumerate() {
        raw[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    raw
}

#[test]
fn decodes_rom_golden_vector_from_rotated_selected_party_and_pc() {
    let mut bytes = valid_image(20, 21);
    let box_raw = golden_box_pokemon();
    let mut party_raw = [0_u8; 100];
    party_raw[..80].copy_from_slice(&box_raw);
    party_raw[80..100].copy_from_slice(&[1; 20]);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &party_raw);
    // Linear PC position 49 straddles logical storage sectors 6 and 7.
    write_logical_range(&mut bytes, SaveSlot::Second, 6, 4 + 49 * 80, &box_raw);
    write_logical_range(&mut bytes, SaveSlot::First, 1, 0x238, &[0; 100]);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.selected_slot(), SaveSlot::Second);
    let PokemonSlot::Occupied(party) = save.party_pokemon(0).unwrap() else {
        panic!("party record must be occupied")
    };
    let PokemonSlot::Occupied(boxed) = save.pc_pokemon(1, 19).unwrap() else {
        panic!("PC record must be occupied")
    };
    assert_eq!(party.raw, party_raw);
    assert_eq!(boxed.raw, box_raw);
    assert_eq!(party.identity, boxed.identity);
    assert_eq!(boxed.identity.species, 255); // Torchic
    assert_eq!(boxed.identity.held_item, 520); // Oran Berry
    assert_eq!(boxed.identity.moves, [33, 10, 1, 45]);
    assert_eq!(save.pc_storage_range(4 + 49 * 80, 80).unwrap(), box_raw);
}

#[test]
fn distinguishes_empty_slots_and_rejects_corrupt_nonempty_pokemon() {
    let mut bytes = valid_image(20, 21);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &[0; 100]);
    write_logical_range(&mut bytes, SaveSlot::Second, 6, 4, &[0; 80]);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(save.party_pokemon(0), Ok(PokemonSlot::Empty { raw }) if raw == [0; 100]));
    assert!(matches!(save.pc_pokemon(0, 0), Ok(PokemonSlot::Empty { raw }) if raw == [0; 80]));

    // ZeroMonData writes MAIL_NONE (0xff) into an otherwise cleared party slot.
    let mut empty_party = [0_u8; 100];
    empty_party[0x55] = 0xff;
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &empty_party);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(save.party_pokemon(0), Ok(PokemonSlot::Empty { raw }) if raw == empty_party));

    let mut bad = golden_box_pokemon();
    bad[32] ^= 1;
    write_logical_range(&mut bytes, SaveSlot::Second, 6, 4, &bad);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(
        save.pc_pokemon(0, 0),
        Err(PokemonError::Checksum { .. })
    ));
}

#[test]
fn pokemon_and_logical_range_bounds_are_checked() {
    let save = parse(&valid_image(20, 21), TEST_REGISTRY).unwrap();
    assert_eq!(
        save.party_pokemon(6),
        Err(PokemonError::PartyIndex { index: 6 })
    );
    assert_eq!(
        save.pc_pokemon(14, 0),
        Err(PokemonError::BoxIndex { index: 14 })
    );
    assert_eq!(
        save.pc_pokemon(0, 30),
        Err(PokemonError::BoxPosition { index: 30 })
    );
    assert!(save.save_block1_range(SAVE_BLOCK1_SIZE, 1).is_none());
    assert_eq!(PC_STORAGE_CAPACITY, 34_144);
    assert!(save.pc_storage_range(PC_STORAGE_CAPACITY - 1, 1).is_some());
    assert!(save.pc_storage_range(PC_STORAGE_CAPACITY - 1, 2).is_none());
    assert!(save.pc_storage_range(usize::MAX, 2).is_none());
}

#[test]
fn party_count_bounds_trade_eligibility() {
    let mut bytes = valid_image(20, 21);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x234, &[1]);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.party_count(), Ok(1));

    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x234, &[7]);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(
        save.party_count(),
        Err(PokemonError::PartyCount { count: 7 })
    );
}

fn trade_fixture(counter: u32, tag: u8) -> ValidatedSave {
    let mut bytes = valid_image(counter.wrapping_sub(1), counter);
    let slot = SaveSlot::from_counter(counter);
    let mut record = [0_u8; 100];
    record[..80].copy_from_slice(&golden_box_pokemon());
    record[8] = tag;
    record[85] = 0xff;
    record[86] = tag;
    write_logical_range(&mut bytes, slot, 1, 0x234, &[1]);
    write_logical_range(&mut bytes, slot, 1, 0x238, &record);
    bytes.extend_from_slice(&[tag; RTC_TRAILER_SIZE]);
    parse(&bytes, TEST_REGISTRY).unwrap()
}

#[test]
fn trade_swaps_exact_records_and_preserves_other_logical_bytes() {
    let left = trade_fixture(21, 7);
    let right = trade_fixture(31, 9);
    let left_raw = left.raw_bytes().to_vec();
    let right_raw = right.raw_bytes().to_vec();
    let left_record = match left.party_pokemon(0).unwrap() {
        PokemonSlot::Occupied(record) => record.raw,
        _ => panic!("occupied fixture"),
    };
    let right_record = match right.party_pokemon(0).unwrap() {
        PokemonSlot::Occupied(record) => record.raw,
        _ => panic!("occupied fixture"),
    };
    assert_ne!(left_record[8], right_record[8]);
    let (new_left, new_right) = trade_party_pokemon(&left, 0, &right, 0).unwrap();
    assert_eq!(left.raw_bytes(), left_raw);
    assert_eq!(right.raw_bytes(), right_raw);
    for (before, after, expected) in [
        (&left, &new_left, right_record),
        (&right, &new_right, left_record),
    ] {
        assert_eq!(after.counter(), before.counter().wrapping_add(1));
        assert_ne!(after.selected_slot(), before.selected_slot());
        assert_eq!(
            after.coop().save_generation,
            before.coop().save_generation + 1
        );
        assert_eq!(after.rtc_trailer(), before.rtc_trailer());
        assert!(
            matches!(after.party_pokemon(0), Ok(PokemonSlot::Occupied(record)) if record.raw == expected)
        );
        assert!(parse(after.raw_bytes(), TEST_REGISTRY).is_ok());
        for logical in 0..SECTORS_PER_SLOT {
            let old = before.logical_sector_offsets[logical];
            let new = after.logical_sector_offsets[logical];
            for inside in 0..SECTOR_SIZE {
                let logical_party_position =
                    (logical.checked_sub(1)).map(|id| id * SAVE_BLOCK3_CHUNK_OFFSET + inside);
                let is_party = logical_party_position
                    .is_some_and(|position| (0x238..0x238 + 100).contains(&position));
                let block3_position = logical * SAVE_BLOCK3_CHUNK_SIZE
                    + inside.saturating_sub(SAVE_BLOCK3_CHUNK_OFFSET);
                let is_coop_integrity = inside >= SAVE_BLOCK3_CHUNK_OFFSET
                    && ((COOP_SAVE_OFFSET + COOP_GENERATION_OFFSET
                        ..COOP_SAVE_OFFSET + COOP_GENERATION_OFFSET + 4)
                        .contains(&block3_position)
                        || (COOP_SAVE_OFFSET + COOP_CRC_OFFSET
                            ..COOP_SAVE_OFFSET + COOP_CRC_OFFSET + 4)
                            .contains(&block3_position));
                let is_footer = (SECTOR_CHECKSUM_OFFSET..SECTOR_CHECKSUM_OFFSET + 2)
                    .contains(&inside)
                    || (SECTOR_COUNTER_OFFSET..SECTOR_COUNTER_OFFSET + 4).contains(&inside);
                if !(is_party || is_coop_integrity || is_footer) {
                    assert_eq!(
                        after.raw_bytes()[new + inside],
                        before.raw_bytes()[old + inside],
                        "logical sector {logical}, byte {inside}"
                    );
                }
            }
        }
    }
}

#[test]
fn trade_rejects_empty_corrupt_and_mail_without_mutating_inputs() {
    let left = trade_fixture(21, 7);
    let right = trade_fixture(31, 9);
    let original = left.raw_bytes().to_vec();
    assert!(matches!(
        trade_party_pokemon(&left, 1, &right, 0),
        Err(TradeError::OutsideParty { .. })
    ));
    assert!(matches!(
        trade_party_pokemon(&left, 6, &right, 0),
        Err(TradeError::Pokemon {
            reason: PokemonError::PartyIndex { .. },
            ..
        })
    ));

    let mut bytes = left.raw_bytes().to_vec();
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &[0; 100]);
    let empty = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(
        trade_party_pokemon(&empty, 0, &right, 0),
        Err(TradeError::Empty { .. })
    ));

    let mut bytes = left.raw_bytes().to_vec();
    let party = logical_sector_mut(&mut bytes, SaveSlot::Second, 1);
    party[0x238 + 32] ^= 1;
    write_u16(
        party,
        SECTOR_CHECKSUM_OFFSET,
        sector_checksum(&party[..LOGICAL_SECTOR_DATA_SIZES[1]]),
    );
    let corrupt = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(
        trade_party_pokemon(&corrupt, 0, &right, 0),
        Err(TradeError::Pokemon {
            reason: PokemonError::Checksum { .. },
            ..
        })
    ));

    let mut bytes = left.raw_bytes().to_vec();
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238 + 85, &[0]);
    let mail = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(
        trade_party_pokemon(&mail, 0, &right, 0),
        Err(TradeError::Mail { .. })
    ));
    let PokemonSlot::Occupied(with_mail) = mail.party_pokemon(0).unwrap() else {
        panic!("mail fixture slot is occupied");
    };
    let PokemonSlot::Occupied(without_mail) = left.party_pokemon(0).unwrap() else {
        panic!("fixture slot is occupied");
    };
    assert!(crate::party_record_holds_mail(&with_mail));
    assert!(!crate::party_record_holds_mail(&without_mail));
    assert_eq!(left.raw_bytes(), original);
    assert_eq!(right.rtc_trailer(), Some(&[9; RTC_TRAILER_SIZE]));
}

fn rewrite_payload(
    bytes: &mut [u8],
    slot: SaveSlot,
    change: impl FnOnce(&mut [u8; COOP_SAVE_V1_SIZE]),
) {
    let slot_base = slot.index() * SECTORS_PER_SLOT * SECTOR_SIZE;
    let mut payload = [0_u8; COOP_SAVE_V1_SIZE];
    for physical in 0..SECTORS_PER_SLOT {
        let offset = slot_base + physical * SECTOR_SIZE;
        let logical = usize::from(read_u16(bytes, offset + SECTOR_ID_OFFSET));
        let destination = logical * SAVE_BLOCK3_CHUNK_SIZE;
        if destination < COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE {
            let source = offset + SAVE_BLOCK3_CHUNK_OFFSET;
            let copy_start = COOP_SAVE_OFFSET.max(destination);
            let copy_end =
                (COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE).min(destination + SAVE_BLOCK3_CHUNK_SIZE);
            if copy_start < copy_end {
                payload[copy_start - COOP_SAVE_OFFSET..copy_end - COOP_SAVE_OFFSET]
                    .copy_from_slice(
                        &bytes[source + copy_start - destination..source + copy_end - destination],
                    );
            }
        }
    }

    change(&mut payload);

    for physical in 0..SECTORS_PER_SLOT {
        let offset = slot_base + physical * SECTOR_SIZE;
        let logical = usize::from(read_u16(bytes, offset + SECTOR_ID_OFFSET));
        let destination = logical * SAVE_BLOCK3_CHUNK_SIZE;
        if destination < COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE {
            let target = offset + SAVE_BLOCK3_CHUNK_OFFSET;
            let copy_start = COOP_SAVE_OFFSET.max(destination);
            let copy_end =
                (COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE).min(destination + SAVE_BLOCK3_CHUNK_SIZE);
            if copy_start < copy_end {
                bytes[target + copy_start - destination..target + copy_end - destination]
                    .copy_from_slice(
                        &payload[copy_start - COOP_SAVE_OFFSET..copy_end - COOP_SAVE_OFFSET],
                    );
            }
        }
    }
}

#[test]
fn parses_rotated_slots_and_exposes_frozen_payload() {
    let bytes = valid_image(20, 21);
    let parsed = parse(&bytes, TEST_REGISTRY).unwrap();

    assert_eq!(parsed.selected_slot(), SaveSlot::Second);
    assert_eq!(parsed.counter(), 21);
    assert_eq!(parsed.coop().save_generation, 41);
    assert_eq!(
        parsed.coop().status_flags,
        COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS
    );
    assert!(parsed.coop().migration_ambiguous());
    assert!(!parsed.coop().online_eligible());
    assert_eq!(parsed.coop().regional_progress[0].region, RegionId::Hoenn);
    assert_eq!(parsed.coop().regional_progress[3].region, RegionId::Sevii);
    assert_eq!(parsed.coop().defeated_trainers[0], 0x81);
    assert_eq!(parsed.coop().events[0], 0x0a);
    assert_eq!(parsed.coop().unlocked_fly_points[0], 0x05);
    assert_eq!(parsed.coop().gyms[2], 0x80);
    assert_eq!(
        parsed.character_lineage(),
        CharacterLineage {
            player_name: [0, 1, 2, 3, 4, 5, 6, 7],
            player_gender: 16,
            player_region: 17,
            player_trainer_id: [19, 20, 21, 22],
        }
    );
    assert_eq!(parsed.raw_bytes(), bytes);
    assert!(parsed.rtc_trailer().is_none());
}

#[test]
fn lineage_comes_from_the_rom_selected_logical_saveblock2_sector() {
    let mut bytes = valid_image(20, 21);
    let selected_sector = logical_sector_mut(&mut bytes, SaveSlot::Second, 0);
    selected_sector[PLAYER_NAME_OFFSET..PLAYER_NAME_OFFSET + PLAYER_NAME_SIZE]
        .copy_from_slice(b"ESTEBAN\xff");
    selected_sector[PLAYER_GENDER_OFFSET] = 1;
    selected_sector[PLAYER_REGION_OFFSET] = 2;
    selected_sector[PLAYER_TRAINER_ID_OFFSET..PLAYER_TRAINER_ID_OFFSET + PLAYER_TRAINER_ID_SIZE]
        .copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    let checksum = sector_checksum(&selected_sector[..LOGICAL_SECTOR_DATA_SIZES[0]]);
    write_u16(selected_sector, SECTOR_CHECKSUM_OFFSET, checksum);

    assert_eq!(
        parse(&bytes, TEST_REGISTRY).unwrap().character_lineage(),
        CharacterLineage {
            player_name: *b"ESTEBAN\xff",
            player_gender: 1,
            player_region: 2,
            player_trainer_id: [0x12, 0x34, 0x56, 0x78],
        }
    );
}

#[test]
fn accepts_every_physical_rotation() {
    for first_rotation in 0..SECTORS_PER_SLOT {
        for second_rotation in 0..SECTORS_PER_SLOT {
            let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
            let payload = coop_payload(41);
            write_slot(&mut bytes, SaveSlot::First, 20, first_rotation, &payload);
            write_slot(&mut bytes, SaveSlot::Second, 21, second_rotation, &payload);
            let parsed = parse(&bytes, TEST_REGISTRY).unwrap();
            assert_eq!(parsed.selected_slot(), SaveSlot::Second);
            assert_eq!(parsed.coop().save_generation, 41);
        }
    }
}

#[test]
fn accepts_one_valid_slot_when_the_other_is_corrupt() {
    let mut bytes = valid_image(20, 21);
    write_u32(
        physical_sector_mut(&mut bytes, SaveSlot::Second, 0),
        SECTOR_SIGNATURE_OFFSET,
        0,
    );

    let parsed = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(parsed.selected_slot(), SaveSlot::First);
}

#[test]
fn never_rolls_back_when_newest_slot_has_an_invalid_coop_payload() {
    let mut bytes = valid_image(20, 21);
    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        payload[COOP_CRC_OFFSET] ^= 1;
    });

    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::Coop(CoopSaveError::Crc32 { .. }))
    ));
}

#[test]
fn unambiguous_status_is_online_eligible() {
    let mut bytes = valid_image(20, 21);
    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        write_u32(payload, COOP_STATUS_FLAGS_OFFSET, 0);
        seal_coop_payload(payload);
    });

    let parsed = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(!parsed.coop().migration_ambiguous());
    assert!(parsed.coop().online_eligible());
}

#[test]
fn rejects_duplicate_and_missing_logical_ids() {
    let mut bytes = valid_image(20, 21);
    for slot in [SaveSlot::First, SaveSlot::Second] {
        let sector = physical_sector_mut(&mut bytes, slot, 0);
        let original = usize::from(read_u16(sector, SECTOR_ID_OFFSET));
        let replacement = (original + 1) % SECTORS_PER_SLOT;
        write_u16(
            sector,
            SECTOR_ID_OFFSET,
            u16::try_from(replacement).unwrap(),
        );
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[replacement]]);
        write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
    }

    let SaveError::NoValidSlot { first, second } = parse(&bytes, TEST_REGISTRY).unwrap_err() else {
        panic!("expected invalid slots");
    };
    for error in [first, second] {
        let SlotError::LogicalIdSet {
            missing,
            duplicates,
        } = error
        else {
            panic!("expected logical ID set error");
        };
        assert_eq!(missing.len(), 1);
        assert_eq!(duplicates.len(), 1);
    }
}

#[test]
fn rejects_mixed_counters() {
    let mut bytes = valid_image(20, 21);
    for slot in [SaveSlot::First, SaveSlot::Second] {
        let sector = physical_sector_mut(&mut bytes, slot, 3);
        let counter = read_u32(sector, SECTOR_COUNTER_OFFSET);
        write_u32(sector, SECTOR_COUNTER_OFFSET, counter + 1);
    }
    let SaveError::NoValidSlot { first, second } = parse(&bytes, TEST_REGISTRY).unwrap_err() else {
        panic!("expected invalid slots");
    };
    assert!(matches!(first, SlotError::MixedCounter { .. }));
    assert!(matches!(second, SlotError::MixedCounter { .. }));
}

#[test]
fn rejects_bad_signature_and_checksum() {
    let mut bytes = valid_image(20, 21);
    write_u32(
        physical_sector_mut(&mut bytes, SaveSlot::First, 2),
        SECTOR_SIGNATURE_OFFSET,
        0xdead_beef,
    );
    physical_sector_mut(&mut bytes, SaveSlot::Second, 2)[0] ^= 1;

    let SaveError::NoValidSlot { first, second } = parse(&bytes, TEST_REGISTRY).unwrap_err() else {
        panic!("expected invalid slots");
    };
    assert!(matches!(first, SlotError::Signature { .. }));
    assert!(matches!(second, SlotError::Checksum { .. }));
}

#[test]
fn checksums_each_logical_sector_over_its_frozen_data_length() {
    for (logical, size) in LOGICAL_SECTOR_DATA_SIZES.into_iter().enumerate() {
        if size != 0 {
            let mut bytes = valid_image(20, 21);
            for slot in [SaveSlot::First, SaveSlot::Second] {
                logical_sector_mut(&mut bytes, slot, logical)[size - 1] ^= 1;
            }
            assert!(matches!(
                parse(&bytes, TEST_REGISTRY),
                Err(SaveError::NoValidSlot {
                    first: SlotError::Checksum { .. },
                    second: SlotError::Checksum { .. },
                })
            ));
        }

        if size < SAVE_BLOCK3_CHUNK_OFFSET {
            let mut bytes = valid_image(20, 21);
            logical_sector_mut(&mut bytes, SaveSlot::Second, logical)[size] ^= 1;
            assert!(parse(&bytes, TEST_REGISTRY).is_ok());
        }
    }
}

#[test]
fn matches_rom_counter_wrap_and_uses_counter_parity_for_ties() {
    // MAX is odd and therefore belongs to the second physical slot; the next
    // counter, zero, belongs to the first slot and is selected after wrap.
    let wrapped = parse(&valid_image(0, u32::MAX), TEST_REGISTRY).unwrap();
    assert_eq!(wrapped.selected_slot(), SaveSlot::First);

    let tied = parse(&valid_image(8, 8), TEST_REGISTRY).unwrap();
    assert_eq!(tied.selected_slot(), SaveSlot::First);

    let odd_tie = parse(&valid_image(9, 9), TEST_REGISTRY).unwrap();
    assert_eq!(odd_tie.selected_slot(), SaveSlot::Second);

    let ordinary = parse(&valid_image(0, 1), TEST_REGISTRY).unwrap();
    assert_eq!(ordinary.selected_slot(), SaveSlot::Second);
}

#[test]
fn fails_closed_when_selected_counter_points_at_different_slot_data() {
    let bytes = valid_image(3, 4);
    assert_eq!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::RomSelectedSlotCounterMismatch {
            counter: 4,
            slot: SaveSlot::First,
            slot_counter: 3,
        })
    );
}

#[test]
fn fails_closed_when_only_valid_candidate_points_at_invalid_slot() {
    let mut bytes = valid_image(20, 21);
    write_u32(
        physical_sector_mut(&mut bytes, SaveSlot::First, 0),
        SECTOR_SIGNATURE_OFFSET,
        0,
    );
    for physical in 0..SECTORS_PER_SLOT {
        write_u32(
            physical_sector_mut(&mut bytes, SaveSlot::Second, physical),
            SECTOR_COUNTER_OFFSET,
            22,
        );
    }

    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::RomSelectedSlotInvalid {
            counter: 22,
            slot: SaveSlot::First,
            reason: SlotError::Signature { .. },
        })
    ));
}

#[test]
fn preserves_optional_rtc_trailer() {
    let mut bytes = valid_image(20, 21);
    let trailer =
        array::from_fn::<_, RTC_TRAILER_SIZE, _>(|index| u8::try_from(index).unwrap() ^ 0x5a);
    bytes.extend_from_slice(&trailer);

    let parsed = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(parsed.rtc_trailer(), Some(&trailer));
    assert_eq!(parsed.into_raw_bytes().as_ref(), bytes);
}

#[test]
fn validates_revision_zero_explicitly_and_rejects_it_later() {
    let erased = erased_revision_zero_image();
    let revision_zero = validate_character_save(&erased, 0, TEST_REGISTRY).unwrap();
    assert!(matches!(
        revision_zero,
        CharacterSave::ErasedRevisionZero(_)
    ));
    assert!(matches!(
        validate_character_save(&erased, 1, TEST_REGISTRY),
        Err(SaveError::NoValidSlot { .. })
    ));

    let mut noncanonical = erased;
    noncanonical[1] = 0;
    assert_eq!(
        validate_character_save(&noncanonical, 0, TEST_REGISTRY),
        Err(SaveError::RevisionZeroNotErased)
    );
}

#[test]
fn rejects_invalid_lengths() {
    for length in [0, FLASH_IMAGE_SIZE - 1, FLASH_IMAGE_SIZE + 1] {
        assert_eq!(
            parse(&vec![0; length], TEST_REGISTRY),
            Err(SaveError::InvalidLength { actual: length })
        );
    }
}

#[test]
fn validates_crc_schema_size_registry_and_reserved_bytes() {
    enum Mutation {
        Crc,
        Schema,
        Size,
        RegistryVersion,
        RegistryDigest,
        StatusFlags,
        RegionalReserved,
        BadgeMask,
        TopReserved,
    }

    for mutation in [
        Mutation::Crc,
        Mutation::Schema,
        Mutation::Size,
        Mutation::RegistryVersion,
        Mutation::RegistryDigest,
        Mutation::StatusFlags,
        Mutation::RegionalReserved,
        Mutation::BadgeMask,
        Mutation::TopReserved,
    ] {
        let mut bytes = valid_image(20, 21);
        rewrite_payload(&mut bytes, SaveSlot::Second, |payload| match mutation {
            Mutation::Crc => payload[COOP_CRC_OFFSET] ^= 1,
            Mutation::Schema => {
                write_u16(payload, COOP_SCHEMA_OFFSET, 2);
                seal_coop_payload(payload);
            }
            Mutation::Size => {
                write_u16(payload, COOP_STRUCT_SIZE_OFFSET, 671);
                seal_coop_payload(payload);
            }
            Mutation::RegistryVersion => {
                write_u32(payload, COOP_REGISTRY_VERSION_OFFSET, 8);
                seal_coop_payload(payload);
            }
            Mutation::RegistryDigest => {
                payload[COOP_REGISTRY_DIGEST_OFFSET] ^= 1;
                seal_coop_payload(payload);
            }
            Mutation::StatusFlags => {
                write_u32(payload, COOP_STATUS_FLAGS_OFFSET, 2);
                seal_coop_payload(payload);
            }
            Mutation::RegionalReserved => {
                payload[COOP_REGIONAL_PROGRESS_OFFSET + 1] = 1;
                seal_coop_payload(payload);
            }
            Mutation::BadgeMask => {
                write_u16(payload, COOP_REGIONAL_PROGRESS_OFFSET + 2, 0x100);
                seal_coop_payload(payload);
            }
            Mutation::TopReserved => {
                payload[COOP_RESERVED_OFFSET + COOP_RESERVED_SIZE - 1] = 1;
                seal_coop_payload(payload);
            }
        });

        let error = parse(&bytes, TEST_REGISTRY).unwrap_err();
        let SaveError::Coop(error) = error else {
            panic!("expected co-op error, got {error:?}");
        };
        match mutation {
            Mutation::Crc => assert!(matches!(error, CoopSaveError::Crc32 { .. })),
            Mutation::Schema => assert!(matches!(error, CoopSaveError::SchemaVersion { .. })),
            Mutation::Size => assert!(matches!(error, CoopSaveError::StructSize { .. })),
            Mutation::RegistryVersion => {
                assert!(matches!(error, CoopSaveError::RegistryVersion { .. }));
            }
            Mutation::RegistryDigest => {
                assert!(matches!(error, CoopSaveError::RegistryDigest { .. }));
            }
            Mutation::StatusFlags => {
                assert!(matches!(error, CoopSaveError::UnknownStatusFlags { .. }));
            }
            Mutation::BadgeMask => {
                assert!(matches!(error, CoopSaveError::BadgeMask { .. }));
            }
            Mutation::RegionalReserved | Mutation::TopReserved => {
                assert!(matches!(error, CoopSaveError::ReservedByte { .. }));
            }
        }
    }
}

#[test]
fn validates_region_ordinals_and_frozen_record_order() {
    let mut bytes = valid_image(20, 21);
    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        payload[COOP_REGIONAL_PROGRESS_OFFSET] = RegionId::Kanto.wire();
        seal_coop_payload(payload);
    });
    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::Coop(CoopSaveError::RegionOrder { .. }))
    ));

    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        payload[COOP_REGIONAL_PROGRESS_OFFSET] = RegionId::Unspecified.wire();
        seal_coop_payload(payload);
    });
    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::Coop(CoopSaveError::RegionOrdinal { .. }))
    ));
}

#[test]
fn accepts_assigned_identity_boundary_ordinals() {
    let mut bytes = valid_image(20, 21);
    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        // Last assigned v1 ordinals: trainer 855, event 3, Fly point 3,
        // and gym 23. These are intentionally not inferred from capacity.
        payload[COOP_TRAINER_BITS_OFFSET + 106] |= 0x80;
        payload[COOP_EVENT_BITS_OFFSET] |= 0x08;
        payload[COOP_FLY_BITS_OFFSET] |= 0x08;
        payload[COOP_GYM_BITS_OFFSET + 2] |= 0x80;
        seal_coop_payload(payload);
    });

    assert!(parse(&bytes, TEST_REGISTRY).is_ok());
}

#[test]
fn rejects_unassigned_identity_ordinals_for_every_persisted_kind() {
    for (kind, offset, ordinal) in [
        (IdentityKind::Trainer, COOP_TRAINER_BITS_OFFSET, 856_u16),
        (IdentityKind::Event, COOP_EVENT_BITS_OFFSET, 4_u16),
        (IdentityKind::FlyPoint, COOP_FLY_BITS_OFFSET, 4_u16),
        (IdentityKind::Gym, COOP_GYM_BITS_OFFSET, 24_u16),
    ] {
        let mut bytes = valid_image(20, 21);
        rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
            let ordinal = usize::from(ordinal);
            payload[offset + ordinal / 8] |= 1 << (ordinal % 8);
            seal_coop_payload(payload);
        });

        assert_eq!(
            parse(&bytes, TEST_REGISTRY),
            Err(SaveError::Coop(CoopSaveError::UnassignedIdentityOrdinal {
                kind,
                ordinal
            }))
        );
    }
}

#[test]
fn validates_badges_against_each_regional_assignment() {
    let mut valid = valid_image(20, 21);
    rewrite_payload(&mut valid, SaveSlot::Second, |payload| {
        let johto = COOP_REGIONAL_PROGRESS_OFFSET + 2 * COOP_REGIONAL_PROGRESS_SIZE;
        write_u16(payload, johto + 2, 0x80);
        seal_coop_payload(payload);
    });
    assert!(parse(&valid, TEST_REGISTRY).is_ok());

    let mut invalid = valid_image(20, 21);
    rewrite_payload(&mut invalid, SaveSlot::Second, |payload| {
        let sevii = COOP_REGIONAL_PROGRESS_OFFSET + 3 * COOP_REGIONAL_PROGRESS_SIZE;
        write_u16(payload, sevii + 2, 1);
        seal_coop_payload(payload);
    });
    assert_eq!(
        parse(&invalid, TEST_REGISTRY),
        Err(SaveError::Coop(CoopSaveError::UnassignedBadgeBit {
            region: RegionId::Sevii,
            badge_bit: 0,
        }))
    );
}

#[test]
fn checksum_ignores_partial_words_and_uses_end_around_fold() {
    assert_eq!(sector_checksum(&[1, 2, 3]), 0);
    assert_eq!(sector_checksum(&[0xff; 4]), 0xfffe);
    assert_eq!(sector_checksum(&[0xff; 8]), 0xfffd);
}

#[test]
fn reads_hoenn_badge_flags_in_gym_order() {
    let mut bytes = valid_image(20, 21);
    // Stone, Knuckle and Heat: badges 1, 2 and 4 (bits 0, 1 and 3). The
    // synthetic filler may hold any bits, so every badge flag is written.
    for badge in 0..8 {
        let flag = crate::FLAG_BADGE01_GET + badge;
        let offset = crate::SAVE_BLOCK1_FLAGS_OFFSET + flag / 8;
        let save = parse(&bytes, TEST_REGISTRY).unwrap();
        let current = save.save_block1_range(offset, 1).unwrap()[0];
        let mask = 1 << (flag % 8);
        let value = if [0, 1, 3].contains(&badge) {
            current | mask
        } else {
            current & !mask
        };
        write_logical_range(&mut bytes, SaveSlot::Second, 1, offset, &[value]);
    }
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.hoenn_badges(), Some(0b0000_1011));
    assert_eq!(save.event_flag(crate::FLAG_BADGE01_GET + 2), Some(false));
    assert_eq!(save.event_flag(crate::SAVE_BLOCK1_FLAG_BYTES * 8), None);
}

#[test]
fn reads_money_trainer_flags_experience_and_locates_pokemon() {
    let mut bytes = valid_image(20, 21);
    // Money is stored XOR the SaveBlock2 encryption key, as GetMoney reads it.
    write_logical_range(
        &mut bytes,
        SaveSlot::Second,
        0,
        crate::SAVE_BLOCK2_ENCRYPTION_KEY_OFFSET,
        &0x5a5a_1234_u32.to_le_bytes(),
    );
    write_logical_range(
        &mut bytes,
        SaveSlot::Second,
        1,
        crate::SAVE_BLOCK1_MONEY_OFFSET,
        &(3000_u32 ^ 0x5a5a_1234).to_le_bytes(),
    );
    // Trainer 0x2A is flag TRAINER_FLAGS_START + 0x2A.
    let flag = crate::TRAINER_FLAGS_START + 0x2a;
    write_logical_range(
        &mut bytes,
        SaveSlot::Second,
        1,
        crate::SAVE_BLOCK1_FLAGS_OFFSET + flag / 8,
        &[1 << (flag % 8)],
    );
    let box_raw = golden_box_pokemon();
    let mut party_raw = [0_u8; 100];
    party_raw[..80].copy_from_slice(&box_raw);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &party_raw);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x234, &[1]);
    // Empty PC slots are zeroed in real saves; clear the synthetic filler.
    write_logical_range(
        &mut bytes,
        SaveSlot::Second,
        6,
        4,
        &vec![0; crate::pokemon::BOX_COUNT * crate::pokemon::BOX_SIZE * 80],
    );
    write_logical_range(&mut bytes, SaveSlot::Second, 6, 4 + 49 * 80, &box_raw);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.selected_slot(), SaveSlot::Second);

    assert_eq!(save.money(), Some(3000));
    assert_eq!(save.trainer_flag(0x2a), Some(true));
    assert_eq!(save.trainer_flag(0x2b), Some(false));
    let past_end =
        u16::try_from(crate::TRAINER_FLAGS_END - crate::TRAINER_FLAGS_START + 1).unwrap();
    assert_eq!(save.trainer_flag(past_end), None);

    let PokemonSlot::Occupied(party) = save.party_pokemon(0).unwrap() else {
        panic!("party record must be occupied")
    };
    // The ROM's golden vector asserts MON_DATA_EXP == 12345.
    assert_eq!(party.identity.experience(), 12_345);
    assert_eq!(
        save.locate_pokemon(party.identity.personality, party.identity.ot_id)
            .unwrap(),
        vec![
            crate::PokemonLocation::Party(0),
            crate::PokemonLocation::Pc {
                box_index: 1,
                position: 19
            }
        ]
    );
    assert!(save.locate_pokemon(1, 2).unwrap().is_empty());
}

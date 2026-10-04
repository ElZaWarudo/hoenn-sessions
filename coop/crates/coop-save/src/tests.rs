use super::*;

#[test]
fn rom_written_harbor_saves_project_out_and_back_without_losing_world_locations() {
    let registry = RegistryContract::new(
        coop_protocol::IDENTITY_REGISTRY_VERSION,
        coop_protocol::IDENTITY_REGISTRY_DIGEST,
    );
    let main = parse_v2(
        include_bytes!("../../../../tools/tests/fixtures/arrival-v3-main-lilycove.sav"),
        registry,
    )
    .unwrap();
    let cormoria = parse_v2(
        include_bytes!("../../../../tools/tests/fixtures/arrival-v3-cormoria-rivetshore.sav"),
        registry,
    )
    .unwrap();
    let descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    let pair = TransferDescriptorPair {
        source: descriptor,
        destination: descriptor,
    };

    let arrival = project_arrival(&main, &cormoria, pair, true).unwrap();
    assert!(
        main.character_lineage()
            .same_trainer(arrival.character_lineage())
    );
    assert_eq!(
        arrival.coop().save_generation,
        main.coop().save_generation + 1
    );
    assert_eq!(
        &arrival.logical_sector_payload(1).unwrap()[..8],
        &cormoria.logical_sector_payload(1).unwrap()[..8],
        "the Cormoria harbor location remains local",
    );
    assert_eq!(
        &arrival.logical_sector_payload(1).unwrap()[0x32..0x34],
        &1314_u16.to_le_bytes(),
    );

    let returned = project_arrival(&arrival, &main, pair, false).unwrap();
    assert!(
        main.character_lineage()
            .same_trainer(returned.character_lineage())
    );
    assert_eq!(
        returned.coop().save_generation,
        arrival.coop().save_generation + 1
    );
    assert_eq!(
        &returned.logical_sector_payload(1).unwrap()[..8],
        &main.logical_sector_payload(1).unwrap()[..8],
        "the existing Main harbor location survives the return",
    );
    assert_eq!(
        &returned.logical_sector_payload(1).unwrap()[0x32..0x34],
        &88_u16.to_le_bytes(),
    );
    assert!(parse_v2(returned.raw_bytes(), registry).is_ok());
}

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
    assert_eq!(save.raw_bytes(), bytes);
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
fn pre_fix_zero_mon_data_party_tail_is_outside_every_party_walk() {
    // Before the ROM's ZeroMonData fix, an emptied party slot kept the previous
    // occupant's maxHP in box.hpLost (offset 30); the fixed ROM leaves only
    // MAIL_NONE. Party consumers stop at the saved count, so saves from either
    // ROM validate the same way. Only a direct read of the stale slot sees it,
    // as a nonempty record without species, never as a Pokemon.
    let mut bytes = valid_image(20, 21);
    let mut occupied = [0_u8; 100];
    occupied[..80].copy_from_slice(&golden_box_pokemon());
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238, &occupied);
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x234, &[1]);
    write_logical_range(
        &mut bytes,
        SaveSlot::Second,
        6,
        4,
        &vec![0; crate::pokemon::BOX_COUNT * crate::pokemon::BOX_SIZE * 80],
    );
    let mut stale = [0_u8; 100];
    stale[30] = 23;
    stale[0x55] = 0xff;
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238 + 100, &stale);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.party_count(), Ok(1));
    let PokemonSlot::Occupied(party) = save.party_pokemon(0).unwrap() else {
        panic!("party record must be occupied")
    };
    assert_eq!(
        save.locate_pokemon(party.identity.personality, party.identity.ot_id)
            .unwrap(),
        vec![crate::PokemonLocation::Party(0)]
    );
    assert_eq!(save.party_pokemon(1), Err(PokemonError::MissingSpecies));

    let mut clean = [0_u8; 100];
    clean[0x55] = 0xff;
    write_logical_range(&mut bytes, SaveSlot::Second, 1, 0x238 + 100, &clean);
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert!(matches!(save.party_pokemon(1), Ok(PokemonSlot::Empty { raw }) if raw == clean));
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
fn exposes_selected_logical_sector_payload_after_rotation() {
    let mut bytes = valid_image(20, 21);

    // Give the same logical sector different valid bytes in each physical
    // slot. The newer second slot must be the only source for the view.
    let first_sector = logical_sector_mut(&mut bytes, SaveSlot::First, 7);
    first_sector[123] = 0x11;
    let checksum = sector_checksum(&first_sector[..LOGICAL_SECTOR_DATA_SIZES[7]]);
    write_u16(first_sector, SECTOR_CHECKSUM_OFFSET, checksum);

    let second_sector = logical_sector_mut(&mut bytes, SaveSlot::Second, 7);
    second_sector[123] = 0x22;
    let checksum = sector_checksum(&second_sector[..LOGICAL_SECTOR_DATA_SIZES[7]]);
    write_u16(second_sector, SECTOR_CHECKSUM_OFFSET, checksum);

    let parsed = parse(&bytes, TEST_REGISTRY).unwrap();
    for logical in 0..SECTORS_PER_SLOT {
        let logical_id = u8::try_from(logical).unwrap();
        let payload = parsed
            .logical_sector_payload(logical_id)
            .expect("validated logical sector must be available");
        assert_eq!(payload.len(), LOGICAL_SECTOR_DATA_SIZES[logical]);
        assert_eq!(payload[0], logical_id.wrapping_mul(17));
    }
    assert_eq!(parsed.logical_sector_payload(7).unwrap()[123], 0x22);
    assert!(
        parsed
            .logical_sector_payload(SECTORS_PER_SLOT as u8)
            .is_none()
    );
    assert!(parsed.logical_sector_payload(u8::MAX).is_none());
    assert_eq!(parsed.raw_bytes(), bytes);
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
        (IdentityKind::Trainer, COOP_TRAINER_BITS_OFFSET, 2047_u16),
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

fn v2_payload(normalized: bool, migration_ambiguous: bool) -> Vec<u8> {
    let mut payload = coop_payload(41);
    write_u16(
        &mut payload,
        COOP_SCHEMA_OFFSET,
        v2::COOP_SAVE_V2_SCHEMA_VERSION,
    );
    let mut status_flags = 0;
    if normalized {
        status_flags |= v2::COOP_SAVE_STATUS_MET_LOCATION_NORMALIZED;
    }
    if migration_ambiguous {
        status_flags |= COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
    }
    write_u32(&mut payload, COOP_STATUS_FLAGS_OFFSET, status_flags);
    payload[v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET] = RegionId::Cormoria.wire();
    write_u32(
        &mut payload,
        v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET + 4,
        500,
    );
    let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
    write_u32(&mut payload, COOP_CRC_OFFSET, crc);
    payload.to_vec()
}

fn v2_payload_array(normalized: bool, migration_ambiguous: bool) -> [u8; COOP_SAVE_V1_SIZE] {
    v2_payload(normalized, migration_ambiguous)
        .try_into()
        .expect("schema-two payload has the frozen ABI size")
}

#[test]
fn parse_v2_selects_newest_rotated_v2_slot_and_exposes_views() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let older_v2 = v2_payload_array(true, false);
    let newer_v2 = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 13, &older_v2);
    write_slot(&mut bytes, SaveSlot::Second, 21, 2, &newer_v2);

    let parsed = parse_v2(&bytes, TEST_REGISTRY).unwrap();

    assert_eq!(parsed.selected_slot(), SaveSlot::Second);
    assert_eq!(parsed.counter(), 21);
    assert_eq!(
        parsed.coop().regional_progress[4].region,
        RegionId::Cormoria
    );
    assert_eq!(parsed.coop().regional_progress[4].story_checkpoint, 500);
    assert!(parsed.coop().met_locations_normalized());
    assert_eq!(parsed.raw_bytes(), bytes);
    assert_eq!(
        parsed.logical_sector_payload(2).unwrap()[0],
        2_u8.wrapping_mul(17)
    );
}

#[test]
fn parse_v2_does_not_fallback_when_newer_slot_is_v1() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let older_v2 = v2_payload_array(true, false);
    let newer_v1 = coop_payload(42);
    write_slot(&mut bytes, SaveSlot::First, 20, 5, &older_v2);
    write_slot(&mut bytes, SaveSlot::Second, 21, 8, &newer_v1);

    assert_eq!(
        parse_v2(&bytes, TEST_REGISTRY),
        Err(SaveV2Error::Coop(v2::CoopSaveV2Error::SchemaVersion {
            actual: 1
        }))
    );
}

#[test]
fn parse_v2_fails_closed_on_newer_bad_crc_without_rollback() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let older_v2 = v2_payload_array(true, false);
    let newer_v2 = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 5, &older_v2);
    write_slot(&mut bytes, SaveSlot::Second, 21, 8, &newer_v2);
    rewrite_payload(&mut bytes, SaveSlot::Second, |payload| {
        payload[COOP_CRC_OFFSET] ^= 1;
    });

    assert!(matches!(
        parse_v2(&bytes, TEST_REGISTRY),
        Err(SaveV2Error::Coop(v2::CoopSaveV2Error::Crc32 { .. }))
    ));
}

#[test]
fn parse_v2_retains_rtc_trailer_and_v1_api_rejects_schema_two() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let payload = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 5, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 8, &payload);
    let trailer = array::from_fn::<_, RTC_TRAILER_SIZE, _>(|index| 0xa0 ^ index as u8);
    bytes.extend_from_slice(&trailer);

    let parsed = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(parsed.rtc_trailer(), Some(&trailer));
    assert_eq!(parsed.into_raw_bytes().as_ref(), bytes);
    assert!(matches!(
        parse(&bytes, TEST_REGISTRY),
        Err(SaveError::Coop(CoopSaveError::SchemaVersion { actual: 2 }))
    ));
}

#[test]
fn parses_v2_payload_with_five_ordered_regions_and_registry_contract() {
    let payload = v2_payload(true, false);
    let parsed = v2::parse_payload(&payload, TEST_REGISTRY).unwrap();

    assert_eq!(parsed.regional_progress.len(), 5);
    assert_eq!(
        parsed
            .regional_progress
            .iter()
            .map(|progress| progress.region)
            .collect::<Vec<_>>(),
        vec![
            RegionId::Hoenn,
            RegionId::Kanto,
            RegionId::Johto,
            RegionId::Sevii,
            RegionId::Cormoria,
        ]
    );
    assert_eq!(parsed.regional_progress[4].story_checkpoint, 500);
    assert!(parsed.met_locations_normalized());
    assert!(!parsed.migration_ambiguous());
    assert!(parsed.online_eligible());
}

#[test]
fn v2_rejects_schema_versions_and_payload_lengths() {
    for schema in [1, 3] {
        let mut payload = v2_payload(true, false);
        write_u16(&mut payload, COOP_SCHEMA_OFFSET, schema);
        let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
        write_u32(&mut payload, COOP_CRC_OFFSET, crc);
        assert_eq!(
            v2::parse_payload(&payload, TEST_REGISTRY),
            Err(v2::CoopSaveV2Error::SchemaVersion { actual: schema })
        );
    }

    let mut embedded_size = v2_payload(true, false);
    write_u16(&mut embedded_size, COOP_STRUCT_SIZE_OFFSET, 671);
    let crc = crc32fast::hash(&embedded_size[..COOP_CRC_OFFSET]);
    write_u32(&mut embedded_size, COOP_CRC_OFFSET, crc);
    assert_eq!(
        v2::parse_payload(&embedded_size, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::StructSize { actual: 671 })
    );

    let payload = v2_payload(true, false);
    for invalid in [&payload[..0], &payload[..COOP_SAVE_V1_SIZE - 1]] {
        assert_eq!(
            v2::parse_payload(invalid, TEST_REGISTRY),
            Err(v2::CoopSaveV2Error::InvalidLength {
                actual: invalid.len()
            })
        );
    }
    let mut oversized = payload.clone();
    oversized.push(0);
    assert_eq!(
        v2::parse_payload(&oversized, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::InvalidLength {
            actual: COOP_SAVE_V1_SIZE + 1
        })
    );
}

#[test]
fn v2_rejects_crc_unknown_status_and_cormoria_record_errors() {
    let mut crc = v2_payload(true, false);
    crc[COOP_CRC_OFFSET] ^= 1;
    assert!(matches!(
        v2::parse_payload(&crc, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::Crc32 { .. })
    ));

    let mut unknown_status = v2_payload(true, false);
    write_u32(
        &mut unknown_status,
        COOP_STATUS_FLAGS_OFFSET,
        v2::COOP_SAVE_V2_STATUS_KNOWN_MASK | (1 << 2),
    );
    let crc = crc32fast::hash(&unknown_status[..COOP_CRC_OFFSET]);
    write_u32(&mut unknown_status, COOP_CRC_OFFSET, crc);
    assert!(matches!(
        v2::parse_payload(&unknown_status, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::UnknownStatusFlags { .. })
    ));

    let mut wrong_region = v2_payload(true, false);
    wrong_region[v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET] = RegionId::Sevii.wire();
    let crc = crc32fast::hash(&wrong_region[..COOP_CRC_OFFSET]);
    write_u32(&mut wrong_region, COOP_CRC_OFFSET, crc);
    assert!(matches!(
        v2::parse_payload(&wrong_region, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::RegionOrder {
            record: 4,
            expected: RegionId::Cormoria,
            actual: RegionId::Sevii,
        })
    ));
}

#[test]
fn v2_rejects_reserved_fifth_record_and_tail_bytes() {
    let mut fifth_reserved = v2_payload(true, false);
    fifth_reserved[v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET + 1] = 1;
    let crc = crc32fast::hash(&fifth_reserved[..COOP_CRC_OFFSET]);
    write_u32(&mut fifth_reserved, COOP_CRC_OFFSET, crc);
    assert_eq!(
        v2::parse_payload(&fifth_reserved, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::ReservedByte {
            offset: v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET + 1,
            value: 1,
        })
    );

    let mut tail = v2_payload(true, false);
    tail[v2::COOP_SAVE_V2_RESERVED_TAIL_OFFSET + v2::COOP_SAVE_V2_RESERVED_TAIL_SIZE - 1] = 1;
    let crc = crc32fast::hash(&tail[..COOP_CRC_OFFSET]);
    write_u32(&mut tail, COOP_CRC_OFFSET, crc);
    assert_eq!(
        v2::parse_payload(&tail, TEST_REGISTRY),
        Err(v2::CoopSaveV2Error::ReservedByte {
            offset: COOP_CRC_OFFSET - 1,
            value: 1,
        })
    );
}

#[test]
fn v2_online_eligibility_requires_normalization_and_no_ambiguity() {
    let missing = v2::parse_payload(&v2_payload(false, false), TEST_REGISTRY).unwrap();
    assert!(!missing.met_locations_normalized());
    assert!(!missing.online_eligible());

    let ambiguous = v2::parse_payload(&v2_payload(true, true), TEST_REGISTRY).unwrap();
    assert!(ambiguous.met_locations_normalized());
    assert!(ambiguous.migration_ambiguous());
    assert!(!ambiguous.online_eligible());
}

#[test]
fn v2_projection_changes_only_approved_selected_bytes_and_touched_checksum() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let payload = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let trailer = array::from_fn::<_, RTC_TRAILER_SIZE, _>(|index| index as u8 ^ 0x92);
    bytes.extend_from_slice(&trailer);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    let physical = (0..SECTORS_PER_SLOT)
        .map(|index| (SaveSlot::Second.index() * SECTORS_PER_SLOT + index) * SECTOR_SIZE)
        .find(|&offset| read_u16(&bytes, offset + SECTOR_ID_OFFSET) == 5)
        .unwrap();
    let replacement = [0x12, 0x34, 0x56, 0x78];
    let projected = save
        .project_selected_sectors(
            &[v2::ApprovedSectorSpan {
                logical_id: 5,
                offset: 2972,
                len: 4,
            }],
            &[v2::SelectedSectorPatch {
                logical_id: 5,
                offset: 2972,
                bytes: &replacement,
            }],
        )
        .unwrap();
    assert_eq!(
        &projected.raw_bytes()[physical + 2972..physical + 2976],
        &replacement
    );
    assert_eq!(projected.rtc_trailer(), Some(&trailer));
    assert_eq!(projected.coop(), save.coop());
    assert_eq!(projected.character_lineage(), save.character_lineage());
    for index in 0..bytes.len() {
        if !(physical + 2972..physical + 2976).contains(&index)
            && !(physical + SECTOR_CHECKSUM_OFFSET..physical + SECTOR_CHECKSUM_OFFSET + 2)
                .contains(&index)
        {
            assert_eq!(projected.raw_bytes()[index], bytes[index], "byte {index}");
        }
    }
    assert!(parse_v2(projected.raw_bytes(), TEST_REGISTRY).is_ok());
}

#[test]
fn v2_projection_rejects_unapproved_overlap_and_out_of_bounds() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let payload = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    let approval = [v2::ApprovedSectorSpan {
        logical_id: 5,
        offset: 100,
        len: 8,
    }];
    assert_eq!(
        save.project_selected_sectors(&approval, &[]),
        Err(v2::SectorProjectionError::InvalidPatch)
    );
    let bytes = [0x7a; 4];
    assert_eq!(
        save.project_selected_sectors(
            &approval,
            &[v2::SelectedSectorPatch {
                logical_id: 5,
                offset: 99,
                bytes: &bytes,
            }]
        ),
        Err(v2::SectorProjectionError::UnapprovedOrOverlapping)
    );
    assert_eq!(
        save.project_selected_sectors(
            &approval,
            &[v2::SelectedSectorPatch {
                logical_id: 5,
                offset: 2975,
                bytes: &bytes,
            }]
        ),
        Err(v2::SectorProjectionError::InvalidPatch)
    );
    assert_eq!(
        save.project_selected_sectors(
            &approval,
            &[
                v2::SelectedSectorPatch {
                    logical_id: 5,
                    offset: 100,
                    bytes: &bytes
                },
                v2::SelectedSectorPatch {
                    logical_id: 5,
                    offset: 102,
                    bytes: &bytes
                },
            ]
        ),
        Err(v2::SectorProjectionError::UnapprovedOrOverlapping)
    );
    assert_eq!(
        save.project_selected_sectors(
            &approval,
            &[v2::SelectedSectorPatch {
                logical_id: 15,
                offset: 0,
                bytes: &bytes,
            }]
        ),
        Err(v2::SectorProjectionError::InvalidPatch)
    );
}

#[test]
fn v2_projection_reparses_and_rejects_lineage_changes() {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let payload = v2_payload_array(true, false);
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    let replacement = [0x42];
    assert_eq!(
        save.project_selected_sectors(
            &[v2::ApprovedSectorSpan {
                logical_id: 0,
                offset: PLAYER_NAME_OFFSET,
                len: 1
            }],
            &[v2::SelectedSectorPatch {
                logical_id: 0,
                offset: PLAYER_NAME_OFFSET,
                bytes: &replacement,
            }],
        ),
        Err(v2::SectorProjectionError::ProtectedStateChanged)
    );
    assert_eq!(save.raw_bytes(), bytes);
}

fn transfer_descriptor(pending: bool, coop_shared: bool) -> Vec<u8> {
    // A compact, complete synthetic descriptor with the real encrypted field
    // positions. Production uses the compiler-emitted 45-field descriptor.
    let fields: [(u16, u8, u8, u32, u32); 15] = [
        (0x0100, 0, 2, 0, 0x490),
        (0x0102, 0, 1, 0x490, 4),
        (0x0103, 0, 1, 0x494, 2),
        (0x0104, 0, if pending { 3 } else { 2 }, 0x496, 0xca),
        (0x0106, 0, 1, 0x560, 0x400),
        (0x0200, 1, 2, 0, 17),
        (0x0203, 1, 1, 17, 1),
        (0x0204, 1, 2, 18, 0xb4 - 18),
        (0x020b, 1, 2, 0xb4, 4),
        (0x020c, 1, 2, 0xb8, 0x1fc - 0xb8),
        (0x020d, 1, 1, 0x1fc, 4),
        (0x0300, 2, 1, 0, 1),
        (0x0301, 2, 2, 1, 1),
        (0x0400, 3, 2, 0, COOP_SAVE_OFFSET as u32),
        (
            0x0403,
            3,
            if coop_shared { 1 } else { 2 },
            COOP_SAVE_OFFSET as u32,
            COOP_SAVE_V1_SIZE as u32,
        ),
    ];
    let mut bytes = vec![0; 44 + 16 * fields.len()];
    write_u32(&mut bytes, 0, 0x3154_5043);
    write_u16(&mut bytes, 4, 3);
    write_u16(&mut bytes, 6, u16::try_from(fields.len()).unwrap());
    let length = u32::try_from(bytes.len()).unwrap();
    write_u32(&mut bytes, 8, length);
    write_u32(&mut bytes, 12, 44);
    for (offset, span) in [
        (16, 0x960),
        (20, 0x200),
        (24, 2),
        (28, (COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE) as u32),
    ] {
        write_u32(&mut bytes, offset, span);
    }
    write_u32(&mut bytes, 32, 44);
    write_u32(&mut bytes, 36, 16);
    for (index, (id, storage, owner, offset, size)) in fields.into_iter().enumerate() {
        let base = 44 + index * 16;
        write_u16(&mut bytes, base, id);
        bytes[base + 2] = storage;
        bytes[base + 3] = owner;
        write_u32(&mut bytes, base + 4, offset);
        write_u32(&mut bytes, base + 8, size);
    }
    bytes
}

fn write_selected_payload(
    bytes: &mut [u8],
    slot: SaveSlot,
    logical: usize,
    offset: usize,
    value: &[u8],
) {
    let sector = logical_sector_mut(bytes, slot, logical);
    sector[offset..offset + value.len()].copy_from_slice(value);
    let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[logical]]);
    write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
}

#[test]
fn fresh_world_template_binds_only_selected_trainer_identity_before_projection() {
    let payload = v2_payload_array(true, false);
    let mut source = vec![0xff; FLASH_IMAGE_SIZE];
    let mut destination = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut source, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut source, SaveSlot::Second, 21, 11, &payload);
    write_slot(&mut destination, SaveSlot::First, 22, 7, &payload);
    write_slot(&mut destination, SaveSlot::Second, 21, 2, &payload);
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        0,
        PLAYER_NAME_OFFSET,
        b"NEWNAME\xff",
    );
    write_selected_payload(&mut source, SaveSlot::Second, 0, PLAYER_GENDER_OFFSET, &[1]);
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        0,
        PLAYER_TRAINER_ID_OFFSET,
        &[0x12, 0x34, 0x56, 0x78],
    );
    let source = parse_v2(&source, TEST_REGISTRY).unwrap();
    let template = parse_v2(&destination, TEST_REGISTRY).unwrap();
    assert!(
        !source
            .character_lineage()
            .same_trainer(template.character_lineage())
    );
    let bound = template.bind_fresh_template_trainer(&source).unwrap();
    assert!(
        source
            .character_lineage()
            .same_trainer(bound.character_lineage())
    );
    assert_eq!(bound.coop(), template.coop());
    assert_eq!(bound.selected_slot(), template.selected_slot());
    assert_eq!(bound.counter(), template.counter());
    assert_eq!(
        bound.logical_sector_payload(1),
        template.logical_sector_payload(1)
    );
    assert_eq!(
        &bound.raw_bytes()[SECTORS_PER_SLOT * SECTOR_SIZE..],
        &destination[SECTORS_PER_SLOT * SECTOR_SIZE..]
    );
    let descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    let projected =
        super::transfer::project_shared_player(&source, &bound, descriptor, descriptor).unwrap();
    assert_eq!(projected.character_lineage(), source.character_lineage());
    assert_eq!(projected.coop(), source.coop());
    assert!(parse_v2(projected.raw_bytes(), TEST_REGISTRY).is_ok());
}

#[test]
fn v2_sector_projection_rekeys_selected_rotated_slots_and_preserves_world_bytes() {
    let mut source_payload = v2_payload_array(true, false);
    write_u32(&mut source_payload, COOP_GENERATION_OFFSET, 42);
    seal_coop_payload(&mut source_payload);
    let destination_payload = v2_payload_array(true, false);
    let mut source = vec![0xff; FLASH_IMAGE_SIZE];
    let mut destination = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut source, SaveSlot::First, 20, 4, &source_payload);
    write_slot(&mut source, SaveSlot::Second, 21, 11, &source_payload);
    write_slot(
        &mut destination,
        SaveSlot::First,
        22,
        7,
        &destination_payload,
    );
    write_slot(
        &mut destination,
        SaveSlot::Second,
        21,
        2,
        &destination_payload,
    );
    write_selected_payload(&mut source, SaveSlot::Second, 0, PLAYER_REGION_OFFSET, &[1]);
    write_selected_payload(
        &mut destination,
        SaveSlot::First,
        0,
        PLAYER_REGION_OFFSET,
        &[2],
    );
    let source_key = 0x1256_3487_u32;
    let destination_key = 0xa9bc_02d1_u32;
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        0,
        0xb4,
        &source_key.to_le_bytes(),
    );
    write_selected_payload(
        &mut destination,
        SaveSlot::First,
        0,
        0xb4,
        &destination_key.to_le_bytes(),
    );
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        0,
        0x1fc,
        &(7_654_u32 ^ source_key).to_le_bytes(),
    );
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        1,
        0x490,
        &(500_000_u32 ^ source_key).to_le_bytes(),
    );
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        2,
        2220,
        &(33_u32 ^ source_key).to_le_bytes(),
    );
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        1,
        0x494,
        &(345_u16 ^ u16::from_le_bytes([source_key.to_le_bytes()[0], source_key.to_le_bytes()[1]]))
            .to_le_bytes(),
    );
    write_selected_payload(&mut source, SaveSlot::Second, 1, 0x560, &[0x34, 0x12]);
    write_selected_payload(
        &mut source,
        SaveSlot::Second,
        1,
        0x562,
        &(9_u16 ^ u16::from_le_bytes([source_key.to_le_bytes()[0], source_key.to_le_bytes()[1]]))
            .to_le_bytes(),
    );
    write_selected_payload(&mut source, SaveSlot::Second, 6, 0, &[0x57]);
    // These offsets come from the linked schema-v3 ROM descriptor fixture.
    // Day Care Pokémon follow the player; room furnishings remain regional.
    write_selected_payload(&mut source, SaveSlot::Second, 4, 1600, &[0xab]);
    write_selected_payload(&mut destination, SaveSlot::First, 3, 3268, &[0x77]);
    write_selected_payload(&mut destination, SaveSlot::First, 1, 0, &[0x91]);
    write_selected_payload(&mut destination, SaveSlot::First, 6, 1, &[0x62]);
    let trailer = [0x5a; RTC_TRAILER_SIZE];
    destination.extend_from_slice(&trailer);
    let src = parse_v2(&source, TEST_REGISTRY).unwrap();
    let dst = parse_v2(&destination, TEST_REGISTRY).unwrap();
    let linked_descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    let linked_projected =
        super::transfer::project_shared_player(&src, &dst, linked_descriptor, linked_descriptor)
            .unwrap();
    let arrival = linked_projected.advance_transfer_generation().unwrap();
    assert_eq!(
        arrival.coop().save_generation,
        linked_projected.coop().save_generation + 1
    );
    for logical in 0..SECTORS_PER_SLOT {
        assert_eq!(
            arrival.logical_sector_payload(logical as u8),
            linked_projected.logical_sector_payload(logical as u8)
        );
    }
    let mut expected_block3 = *linked_projected.save_block3();
    for offset in [COOP_GENERATION_OFFSET, COOP_CRC_OFFSET] {
        let start = COOP_SAVE_OFFSET + offset;
        expected_block3[start..start + 4].copy_from_slice(&arrival.save_block3()[start..start + 4]);
    }
    assert_eq!(arrival.save_block3(), &expected_block3);
    assert!(parse_v2(arrival.raw_bytes(), TEST_REGISTRY).is_ok());
    assert_eq!(linked_projected.coop(), src.coop());
    assert_eq!(linked_projected.character_lineage().player_region, 1);
    assert_eq!(
        linked_projected.logical_sector_payload(4).unwrap()[1600],
        0xab
    );
    assert_eq!(
        linked_projected.logical_sector_payload(3).unwrap()[3268],
        0x77
    );
    assert_eq!(
        read_u32(linked_projected.logical_sector_payload(2).unwrap(), 2220) ^ destination_key,
        33
    );
    assert_eq!(
        read_u32(linked_projected.logical_sector_payload(0).unwrap(), 0x1fc) ^ destination_key,
        7_654
    );
    let descriptor = transfer_descriptor(false, true);
    let projected =
        super::transfer::project_shared_player(&src, &dst, &descriptor, &descriptor).unwrap();
    assert_eq!(projected.selected_slot(), SaveSlot::First);
    assert_eq!(projected.rtc_trailer(), Some(&trailer));
    assert_eq!(projected.character_lineage().player_region, 1);
    assert_eq!(projected.coop(), src.coop());
    assert_eq!(
        &projected.logical_sector_payload(0).unwrap()[0xb4..0xb8],
        &destination_key.to_le_bytes()
    );
    assert_eq!(
        read_u32(projected.logical_sector_payload(1).unwrap(), 0x490) ^ destination_key,
        500_000
    );
    assert_eq!(
        read_u16(projected.logical_sector_payload(1).unwrap(), 0x494)
            ^ u16::from_le_bytes([
                destination_key.to_le_bytes()[0],
                destination_key.to_le_bytes()[1]
            ]),
        345
    );
    assert_eq!(
        read_u16(projected.logical_sector_payload(1).unwrap(), 0x562)
            ^ u16::from_le_bytes([
                destination_key.to_le_bytes()[0],
                destination_key.to_le_bytes()[1]
            ]),
        9
    );
    assert_eq!(
        read_u32(projected.logical_sector_payload(0).unwrap(), 0x1fc) ^ destination_key,
        7_654
    );
    assert_eq!(projected.logical_sector_payload(1).unwrap()[0], 0x91);
    assert_eq!(projected.logical_sector_payload(6).unwrap()[1], 0x62);
    assert_eq!(projected.logical_sector_payload(6).unwrap()[0], 0x57);
    let other_slot = SECTORS_PER_SLOT * SECTOR_SIZE..2 * SECTORS_PER_SLOT * SECTOR_SIZE;
    assert_eq!(
        &projected.raw_bytes()[other_slot.clone()],
        &destination[other_slot]
    );
    assert!(parse_v2(projected.raw_bytes(), TEST_REGISTRY).is_ok());
    let returned =
        super::transfer::project_shared_player(&projected, &src, &descriptor, &descriptor).unwrap();
    assert_eq!(
        read_u32(returned.logical_sector_payload(1).unwrap(), 0x490) ^ source_key,
        500_000
    );
    assert_eq!(
        &returned.logical_sector_payload(0).unwrap()[0xb4..0xb8],
        &source_key.to_le_bytes()
    );
}

#[test]
fn transfer_generation_rejects_wraparound_without_changing_the_save() {
    let mut payload = v2_payload_array(true, false);
    write_u32(&mut payload, COOP_GENERATION_OFFSET, u32::MAX);
    seal_coop_payload(&mut payload);
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(
        save.advance_transfer_generation(),
        Err(v2::SectorProjectionError::GenerationExhausted)
    );
    assert_eq!(save.raw_bytes(), bytes);
}

#[test]
fn public_arrival_projection_round_trips_existing_world_and_preserves_regional_bytes() {
    let mut source_payload = v2_payload_array(true, false);
    write_u32(&mut source_payload, COOP_GENERATION_OFFSET, 8);
    seal_coop_payload(&mut source_payload);
    let destination_payload = v2_payload_array(true, false);
    let mut source_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let mut destination_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut source_bytes, SaveSlot::First, 20, 4, &source_payload);
    write_slot(&mut source_bytes, SaveSlot::Second, 21, 11, &source_payload);
    write_slot(
        &mut destination_bytes,
        SaveSlot::First,
        22,
        7,
        &destination_payload,
    );
    write_slot(
        &mut destination_bytes,
        SaveSlot::Second,
        21,
        2,
        &destination_payload,
    );
    // This byte belongs to a destination-local regional field and must remain
    // untouched by the shared-player projection.
    write_selected_payload(&mut destination_bytes, SaveSlot::First, 3, 3268, &[0x7d]);
    let source = parse_v2(&source_bytes, TEST_REGISTRY).unwrap();
    let destination = parse_v2(&destination_bytes, TEST_REGISTRY).unwrap();
    let descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    let arrival = project_arrival(
        &source,
        &destination,
        TransferDescriptorPair {
            source: descriptor,
            destination: descriptor,
        },
        false,
    )
    .unwrap();
    assert_eq!(arrival.coop().save_generation, 9);
    assert_eq!(arrival.logical_sector_payload(3).unwrap()[3268], 0x7d);
    assert_eq!(arrival.character_lineage(), source.character_lineage());
    assert!(parse_v2(arrival.raw_bytes(), TEST_REGISTRY).is_ok());
}

#[test]
fn public_first_arrival_binds_fresh_template_and_supports_a_third_world() {
    let payload = v2_payload_array(true, false);
    let mut source_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let mut destination_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut source_bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut source_bytes, SaveSlot::Second, 21, 11, &payload);
    write_slot(&mut destination_bytes, SaveSlot::First, 22, 7, &payload);
    write_slot(&mut destination_bytes, SaveSlot::Second, 21, 2, &payload);
    write_selected_payload(
        &mut source_bytes,
        SaveSlot::Second,
        0,
        PLAYER_REGION_OFFSET,
        &[3],
    );
    write_selected_payload(
        &mut destination_bytes,
        SaveSlot::First,
        0,
        PLAYER_REGION_OFFSET,
        &[3],
    );
    write_selected_payload(
        &mut source_bytes,
        SaveSlot::Second,
        0,
        PLAYER_NAME_OFFSET,
        &[b'T', b'H', b'I', b'R', b'D', 0xff, 0xff, 0xff],
    );
    write_selected_payload(
        &mut source_bytes,
        SaveSlot::Second,
        0,
        PLAYER_GENDER_OFFSET,
        &[1],
    );
    write_selected_payload(
        &mut source_bytes,
        SaveSlot::Second,
        0,
        PLAYER_TRAINER_ID_OFFSET,
        &[0x12, 0x34, 0x56, 0x78],
    );
    write_selected_payload(&mut destination_bytes, SaveSlot::First, 3, 3268, &[0xa1]);
    let source = parse_v2(&source_bytes, TEST_REGISTRY).unwrap();
    let destination = parse_v2(&destination_bytes, TEST_REGISTRY).unwrap();
    assert!(
        !source
            .character_lineage()
            .same_trainer(destination.character_lineage())
    );
    let descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    let arrival = project_arrival(
        &source,
        &destination,
        TransferDescriptorPair {
            source: descriptor,
            destination: descriptor,
        },
        true,
    )
    .unwrap();
    assert!(
        source
            .character_lineage()
            .same_trainer(arrival.character_lineage())
    );
    assert_eq!(arrival.character_lineage().player_region, 3);
    assert_eq!(
        arrival.coop().save_generation,
        source.coop().save_generation + 1
    );
    assert_eq!(arrival.logical_sector_payload(3).unwrap()[3268], 0xa1);
}

#[test]
fn public_arrival_rejects_malformed_descriptor_and_generation_exhaustion() {
    let payload = v2_payload_array(true, false);
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(
        project_arrival(
            &save,
            &save,
            TransferDescriptorPair {
                source: &[],
                destination: &[],
            },
            false,
        ),
        Err(TransferError::InvalidDescriptor)
    );

    let mut max_payload = v2_payload_array(true, false);
    write_u32(&mut max_payload, COOP_GENERATION_OFFSET, u32::MAX);
    seal_coop_payload(&mut max_payload);
    let mut max_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut max_bytes, SaveSlot::First, 20, 4, &max_payload);
    write_slot(&mut max_bytes, SaveSlot::Second, 21, 11, &max_payload);
    let max_save = parse_v2(&max_bytes, TEST_REGISTRY).unwrap();
    let descriptor = include_bytes!("fixtures/player_transfer_v3.bin");
    assert_eq!(
        project_arrival(
            &max_save,
            &max_save,
            TransferDescriptorPair {
                source: descriptor,
                destination: descriptor,
            },
            false,
        ),
        Err(TransferError::Projection(
            v2::SectorProjectionError::GenerationExhausted
        ))
    );
    assert_eq!(max_save.raw_bytes(), max_bytes);
}

#[test]
fn v2_transfer_rejects_pending_and_mismatched_descriptors_without_writing() {
    let payload = v2_payload_array(true, false);
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut bytes, SaveSlot::First, 20, 4, &payload);
    write_slot(&mut bytes, SaveSlot::Second, 21, 11, &payload);
    let save = parse_v2(&bytes, TEST_REGISTRY).unwrap();
    let pending = transfer_descriptor(true, false);
    assert_eq!(
        super::transfer::project_shared_player(&save, &save, &pending, &pending),
        Err(super::transfer::TransferError::PendingField(0x0104))
    );
    let resolved = transfer_descriptor(false, false);
    assert_eq!(
        super::transfer::project_shared_player(&save, &save, &pending, &resolved),
        Err(super::transfer::TransferError::DescriptorMismatch)
    );
    assert_eq!(save.raw_bytes(), bytes);
    let missing_shared_coop = transfer_descriptor(false, false);
    assert_eq!(
        super::transfer::project_shared_player(
            &save,
            &save,
            &missing_shared_coop,
            &missing_shared_coop
        ),
        Err(super::transfer::TransferError::InvalidDescriptor)
    );
}

#[test]
fn v2_coop_projection_maps_validated_record_into_rotated_destination_only() {
    let mut source_payload = v2_payload_array(true, false);
    write_u32(&mut source_payload, COOP_GENERATION_OFFSET, 42);
    write_u32(
        &mut source_payload,
        v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET + 4,
        501,
    );
    seal_coop_payload(&mut source_payload);
    let destination_payload = v2_payload_array(true, false);
    let mut source_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let mut destination_bytes = vec![0xff; FLASH_IMAGE_SIZE];
    write_slot(&mut source_bytes, SaveSlot::First, 20, 4, &source_payload);
    write_slot(&mut source_bytes, SaveSlot::Second, 21, 11, &source_payload);
    write_slot(
        &mut destination_bytes,
        SaveSlot::First,
        22,
        7,
        &destination_payload,
    );
    write_slot(
        &mut destination_bytes,
        SaveSlot::Second,
        21,
        2,
        &destination_payload,
    );
    destination_bytes.extend_from_slice(&[0x5a; RTC_TRAILER_SIZE]);
    let source = parse_v2(&source_bytes, TEST_REGISTRY).unwrap();
    let destination = parse_v2(&destination_bytes, TEST_REGISTRY).unwrap();

    let projected = destination.project_coop_from(&source).unwrap();
    assert_eq!(projected.coop(), source.coop());
    assert_eq!(projected.selected_slot(), SaveSlot::First);
    assert_eq!(projected.counter(), destination.counter());
    assert_eq!(projected.rtc_trailer(), destination.rtc_trailer());
    assert!(parse_v2(projected.raw_bytes(), TEST_REGISTRY).is_ok());
    let mut changed = 0;
    for (offset, (&before, &after)) in destination_bytes
        .iter()
        .zip(projected.raw_bytes())
        .enumerate()
    {
        if before != after {
            changed += 1;
            let physical = offset / SECTOR_SIZE;
            let sector_offset = offset % SECTOR_SIZE;
            assert!(physical < SECTORS_PER_SLOT);
            assert!(
                (SAVE_BLOCK3_CHUNK_OFFSET..SAVE_BLOCK3_CHUNK_OFFSET + SAVE_BLOCK3_CHUNK_SIZE)
                    .contains(&sector_offset)
            );
            let logical = usize::from(read_u16(
                &destination_bytes,
                physical * SECTOR_SIZE + SECTOR_ID_OFFSET,
            ));
            let block3_offset =
                logical * SAVE_BLOCK3_CHUNK_SIZE + sector_offset - SAVE_BLOCK3_CHUNK_OFFSET;
            assert!(
                (COOP_SAVE_OFFSET..COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE).contains(&block3_offset)
            );
        }
    }
    assert!(changed > 0);

    let mut other_character = source_bytes.clone();
    write_selected_payload(&mut other_character, SaveSlot::Second, 0, 0, &[0x77]);
    let mismatched = parse_v2(&other_character, TEST_REGISTRY).unwrap();
    assert_eq!(
        destination.project_coop_from(&mismatched),
        Err(v2::SectorProjectionError::SourceMismatch)
    );
    assert_eq!(destination.raw_bytes(), destination_bytes);
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
fn reads_script_vars_as_var_get_does() {
    let mut bytes = valid_image(20, 21);
    // VAR_ROUTE110_STATE (0x4069) and the last saved variable.
    let last = crate::VARS_START + crate::SAVE_BLOCK1_VAR_COUNT - 1;
    for (var, value) in [(0x4069_usize, 0x0102_u16), (last, 7)] {
        let offset = crate::SAVE_BLOCK1_VARS_OFFSET + 2 * (var - crate::VARS_START);
        write_logical_range(
            &mut bytes,
            SaveSlot::Second,
            1,
            offset,
            &value.to_le_bytes(),
        );
    }
    let save = parse(&bytes, TEST_REGISTRY).unwrap();
    assert_eq!(save.event_var(0x4069), Some(0x0102));
    assert_eq!(save.event_var(last), Some(7));
    assert_eq!(save.event_var(last + 1), None);
    assert_eq!(save.event_var(crate::VARS_START - 1), None);
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

// Independent synthetic CSP2 saves, not a legacy-save conversion path.
fn v2_trade_record(tag: u8, held_item: u16) -> [u8; 100] {
    let mut raw = [0_u8; 100];
    let personality = u32::from(tag) * 24; // GAEM physical order, permutation zero.
    let ot_id = u32::from_le_bytes([tag; 4]);
    write_u32(&mut raw, 0, personality);
    write_u32(&mut raw, 4, ot_id);
    raw[8..18].fill(tag);
    raw[19] = 2;
    raw[20..27].fill(tag);
    let mut secure = [0_u8; 48];
    write_u16(&mut secure, 0, u16::from(tag));
    write_u16(&mut secure, 2, held_item);
    write_u32(&mut secure, 4, 12345);
    write_u16(&mut secure, 12, 33);
    secure[20..24].copy_from_slice(&[10, 11, 12, 13]);
    let checksum = secure.chunks_exact(2).fold(0_u16, |sum, word| {
        sum.wrapping_add(u16::from_le_bytes(word.try_into().unwrap()))
    });
    write_u16(&mut raw, 28, checksum);
    for (index, word) in secure.chunks_exact(4).enumerate() {
        write_u32(
            &mut raw,
            32 + 4 * index,
            u32::from_le_bytes(word.try_into().unwrap()) ^ personality ^ ot_id,
        );
    }
    raw[85] = 0xff;
    raw[84] = 5;
    write_u16(&mut raw, 86, 20);
    write_u16(&mut raw, 88, 25);
    raw
}

fn v2_trade_fixture(counter: u32, tag: u8, rotation: usize) -> ValidatedSaveV2 {
    let mut bytes = vec![0xff; FLASH_IMAGE_SIZE];
    let selected = SaveSlot::from_counter(counter);
    let other = SaveSlot::from_counter(counter.wrapping_sub(1));
    let mut payload = v2_payload_array(true, true); // Preserve both known status bits.
    write_u16(
        &mut payload,
        v2::COOP_SAVE_V2_CORMORIA_PROGRESS_OFFSET + 2,
        0x15,
    );
    let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
    write_u32(&mut payload, COOP_CRC_OFFSET, crc);
    write_slot(&mut bytes, other, counter.wrapping_sub(1), 3, &payload);
    write_slot(&mut bytes, selected, counter, rotation, &payload);
    write_logical_range(
        &mut bytes,
        selected,
        0,
        PLAYER_NAME_OFFSET,
        &[tag; PLAYER_NAME_SIZE],
    );
    write_logical_range(&mut bytes, selected, 0, PLAYER_TRAINER_ID_OFFSET, &[tag; 4]);
    write_logical_range(&mut bytes, selected, 1, 0x234, &[2]);
    let mut party = [0; 600];
    party[..100].copy_from_slice(&v2_trade_record(tag, 0));
    party[100..200].copy_from_slice(&v2_trade_record(tag + 1, 0));
    write_logical_range(&mut bytes, selected, 1, 0x238, &party);
    // Initialize boxes in batches so fixture construction recomputes each
    // sector checksum once, not once per cleared byte.
    let boxes_end = 4 + pokemon::BOX_COUNT * pokemon::BOX_SIZE * 80;
    for logical in 6..SECTORS_PER_SLOT {
        let base = (logical - 6) * SAVE_BLOCK3_CHUNK_OFFSET;
        let begin = 4_usize.saturating_sub(base);
        let end = boxes_end.saturating_sub(base).min(SAVE_BLOCK3_CHUNK_OFFSET);
        if begin >= end {
            continue;
        }
        let sector = logical_sector_mut(&mut bytes, selected, logical);
        sector[begin..end].fill(0);
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[logical]]);
        write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
    }
    write_logical_range(
        &mut bytes,
        selected,
        6,
        4 + 49 * 80,
        &v2_trade_record(tag, 0)[..80],
    );
    bytes.extend_from_slice(&[tag; RTC_TRAILER_SIZE]);
    parse_v2(&bytes, TEST_REGISTRY).unwrap()
}

#[test]
fn v2_trade_preserves_every_nonparty_byte_and_fifth_campaign_across_counter_rollover() {
    let left = v2_trade_fixture(21, 7, 1);
    let right = v2_trade_fixture(u32::MAX, 9, 14);
    assert!(
        !left
            .character_lineage()
            .same_trainer(right.character_lineage())
    );
    let left_record = match left.party_pokemon(1).unwrap() {
        PokemonSlot::Occupied(record) => record.raw,
        _ => panic!("occupied"),
    };
    let right_record = match right.party_pokemon(0).unwrap() {
        PokemonSlot::Occupied(record) => record.raw,
        _ => panic!("occupied"),
    };
    let originals = (left.raw_bytes().to_vec(), right.raw_bytes().to_vec());
    let (new_left, new_right) = trade_party_pokemon_v2(&left, 1, &right, 0).unwrap();
    assert_eq!(left.raw_bytes(), originals.0);
    assert_eq!(right.raw_bytes(), originals.1);
    assert_eq!(new_right.counter(), 0);
    for (before, after, index, incoming) in [
        (&left, &new_left, 1, right_record),
        (&right, &new_right, 0, left_record),
    ] {
        assert_eq!(after.character_lineage(), before.character_lineage());
        assert_eq!(after.rtc_trailer(), before.rtc_trailer());
        assert_eq!(after.money(), before.money());
        assert_eq!(
            after.pc_storage_range(0, PC_STORAGE_CAPACITY),
            before.pc_storage_range(0, PC_STORAGE_CAPACITY)
        );
        assert_eq!(after.party_count(), before.party_count());
        assert_eq!(
            after.coop().regional_progress,
            before.coop().regional_progress
        );
        assert_eq!(after.coop().status_flags, before.coop().status_flags);
        assert_eq!(
            after.coop().defeated_trainers,
            before.coop().defeated_trainers
        );
        assert_eq!(after.coop().events, before.coop().events);
        assert_eq!(
            after.coop().unlocked_fly_points,
            before.coop().unlocked_fly_points
        );
        assert_eq!(after.coop().gyms, before.coop().gyms);
        assert_eq!(
            after.coop().save_generation,
            before.coop().save_generation + 1
        );
        assert_eq!(
            after
                .coop()
                .progress_for(RegionId::Cormoria)
                .unwrap()
                .badge_mask,
            0x15
        );
        assert!(
            matches!(after.party_pokemon(index), Ok(PokemonSlot::Occupied(record)) if record.raw == incoming)
        );
        assert_eq!(after.counter(), before.counter().wrapping_add(1));
        assert_ne!(after.selected_slot(), before.selected_slot());
        let source_base = before.selected_slot().index() * SECTORS_PER_SLOT * SECTOR_SIZE;
        assert_eq!(
            &after.raw_bytes()[source_base..source_base + SECTORS_PER_SLOT * SECTOR_SIZE],
            &before.raw_bytes()[source_base..source_base + SECTORS_PER_SLOT * SECTOR_SIZE]
        );
        assert_eq!(
            &after.raw_bytes()[SECTOR_SIZE * SECTORS_PER_SLOT * 2..],
            &before.raw_bytes()[SECTOR_SIZE * SECTORS_PER_SLOT * 2..]
        );
        for logical in 0..SECTORS_PER_SLOT {
            let old = before.logical_sector_offsets[logical];
            let new = after.logical_sector_offsets[logical];
            for inside in 0..SECTOR_SIZE {
                let party = logical
                    .checked_sub(1)
                    .map(|id| id * SAVE_BLOCK3_CHUNK_OFFSET + inside)
                    .is_some_and(|position| {
                        (0x238 + index * 100..0x238 + (index + 1) * 100).contains(&position)
                    });
                let block3 = logical * SAVE_BLOCK3_CHUNK_SIZE
                    + inside.saturating_sub(SAVE_BLOCK3_CHUNK_OFFSET);
                let coop_integrity = inside >= SAVE_BLOCK3_CHUNK_OFFSET
                    && ((COOP_SAVE_OFFSET + COOP_GENERATION_OFFSET
                        ..COOP_SAVE_OFFSET + COOP_GENERATION_OFFSET + 4)
                        .contains(&block3)
                        || (COOP_SAVE_OFFSET + COOP_CRC_OFFSET
                            ..COOP_SAVE_OFFSET + COOP_CRC_OFFSET + 4)
                            .contains(&block3));
                let footer = (SECTOR_CHECKSUM_OFFSET..SECTOR_CHECKSUM_OFFSET + 2).contains(&inside)
                    || (SECTOR_COUNTER_OFFSET..SECTOR_COUNTER_OFFSET + 4).contains(&inside);
                if !(party || coop_integrity || footer) {
                    assert_eq!(
                        after.raw_bytes()[new + inside],
                        before.raw_bytes()[old + inside],
                        "logical{logical} byte{inside}"
                    );
                }
            }
        }
        assert!(parse_v2(after.raw_bytes(), TEST_REGISTRY).is_ok());
        assert!(parse(after.raw_bytes(), TEST_REGISTRY).is_err());
    }
}

#[test]
fn v2_trade_rotates_logical_sectors_from_each_physical_start() {
    let right = v2_trade_fixture(31, 9, 3);
    for rotation in 0..SECTORS_PER_SLOT {
        let left = v2_trade_fixture(21, 7, rotation);
        let (output, _) = trade_party_pokemon_v2(&left, 0, &right, 1).unwrap();
        let old_base = left.selected_slot().index() * SECTOR_SIZE * SECTORS_PER_SLOT;
        let new_base = output.selected_slot().index() * SECTOR_SIZE * SECTORS_PER_SLOT;
        let old_zero = (left.logical_sector_offsets[0] - old_base) / SECTOR_SIZE;
        assert_eq!(
            (output.logical_sector_offsets[0] - new_base) / SECTOR_SIZE,
            (old_zero + 1) % SECTORS_PER_SLOT
        );
    }
}

#[test]
fn v2_trade_rejects_invalid_selected_records_without_either_output() {
    let left = v2_trade_fixture(21, 7, 3);
    let right = v2_trade_fixture(31, 9, 6);
    assert!(matches!(
        trade_party_pokemon_v2(&left, 2, &right, 0),
        Err(TradeError::OutsideParty { side: "left", .. })
    ));
    assert!(matches!(
        trade_party_pokemon_v2(&left, 6, &right, 0),
        Err(TradeError::Pokemon {
            reason: PokemonError::PartyIndex { .. },
            ..
        })
    ));
    for (record, expected) in [
        ([0; 100], "empty"),
        (
            {
                let mut r = v2_trade_record(7, 0);
                r[32] ^= 1;
                r
            },
            "checksum",
        ),
        (
            {
                let mut r = v2_trade_record(7, 0);
                r[85] = 0;
                r
            },
            "mail",
        ),
        (v2_trade_record(7, 200), "mail"),
    ] {
        let mut raw = right.raw_bytes().to_vec();
        write_logical_range(&mut raw, right.selected_slot(), 1, 0x238, &record);
        let bad = parse_v2(&raw, TEST_REGISTRY).unwrap();
        let result = trade_party_pokemon_v2(&left, 0, &bad, 0);
        assert!(match expected {
            "empty" => matches!(result, Err(TradeError::Empty { side: "right", .. })),
            "checksum" => matches!(
                result,
                Err(TradeError::Pokemon {
                    side: "right",
                    reason: PokemonError::Checksum { .. }
                })
            ),
            _ => matches!(result, Err(TradeError::Mail { side: "right", .. })),
        });
        assert_eq!(bad.raw_bytes(), raw);
    }
}

#[test]
fn v2_trade_rejects_generation_overflow_on_either_side() {
    let left = v2_trade_fixture(21, 7, 3);
    let right = v2_trade_fixture(31, 9, 6);
    for side in ["left", "right"] {
        let save = if side == "left" { &left } else { &right };
        let mut raw = save.raw_bytes().to_vec();
        rewrite_payload(&mut raw, save.selected_slot(), |payload| {
            write_u32(payload, COOP_GENERATION_OFFSET, u32::MAX);
            let crc = crc32fast::hash(&payload[..COOP_CRC_OFFSET]);
            write_u32(payload, COOP_CRC_OFFSET, crc);
        });
        let exhausted = parse_v2(&raw, TEST_REGISTRY).unwrap();
        let result = if side == "left" {
            trade_party_pokemon_v2(&exhausted, 0, &right, 0)
        } else {
            trade_party_pokemon_v2(&left, 0, &exhausted, 0)
        };
        assert!(matches!(result,Err(TradeError::GenerationOverflow{side:failed}) if failed==side));
        assert_eq!(exhausted.raw_bytes(), raw);
    }
}

#[test]
fn v2_gameplay_views_use_current_slot_and_canonical_trainer_bits() {
    let save = v2_trade_fixture(21, 7, 3);
    let mut raw = save.raw_bytes().to_vec();
    let slot = save.selected_slot();
    write_logical_range(
        &mut raw,
        slot,
        0,
        SAVE_BLOCK2_ENCRYPTION_KEY_OFFSET,
        &0x12345678_u32.to_le_bytes(),
    );
    write_logical_range(
        &mut raw,
        slot,
        1,
        SAVE_BLOCK1_MONEY_OFFSET,
        &(3000_u32 ^ 0x12345678).to_le_bytes(),
    );
    write_briney_evidence(&mut raw, slot, 0, 11, 0, true, false, false, true);
    write_wally_evidence(&mut raw, slot, true, 1, false);
    write_csp1_trainer_bit(&mut raw, slot, WALLY_VICTORY_ROAD_TRAINER_ORDINAL, true);
    let save = parse_v2(&raw, TEST_REGISTRY).unwrap();
    assert_eq!(save.money(), Some(3000));
    assert!(
        save.briney_voyage_evidence()
            .is_first_voyage_post_scene_at(0, 11)
    );
    assert!(save.wally_victory_road_evidence().is_post_battle());
    let ordinal =
        resolve_ordinal(IdentityKind::Trainer, WALLY_VICTORY_ROAD_TRAINER_ORDINAL).unwrap();
    let identity = TrainerInstanceId::parse(ordinal.qualified_id).unwrap();
    assert_eq!(save.coop().defeated_trainer(&identity), Ok(true));
    assert!(
        save.coop()
            .defeated_trainer(&TrainerInstanceId::parse("HOENN:TRAINER_NOT_REGISTERED").unwrap())
            .is_err()
    );
    assert!(save.coop().progress_for(RegionId::Unspecified).is_none());
    let PokemonSlot::Occupied(mon) = save.party_pokemon(0).unwrap() else {
        panic!("occupied")
    };
    assert_eq!(mon.identity.experience(), 12345);
    assert_eq!(
        save.locate_pokemon(mon.identity.personality, mon.identity.ot_id)
            .unwrap(),
        vec![
            PokemonLocation::Party(0),
            PokemonLocation::Pc {
                box_index: 1,
                position: 19
            }
        ]
    );
    assert_eq!(save.event_var(VARS_START - 1), None);
    assert_eq!(save.event_var(VARS_START + SAVE_BLOCK1_VAR_COUNT), None);
    assert_eq!(save.event_flag(SAVE_BLOCK1_FLAG_BYTES * 8), None);
    assert_eq!(save.save_block1_range(usize::MAX, 1), None);
    assert_eq!(save.pc_storage_range(PC_STORAGE_CAPACITY, 1), None);
    let mut raw = save.raw_bytes().to_vec();
    write_bill_evidence(
        &mut raw,
        slot,
        (64, 0),
        (2, 3, 1),
        (true, true, false, true),
    );
    let bill = parse_v2(&raw, TEST_REGISTRY).unwrap();
    assert!(bill.bill_voyage_evidence().is_cinnabar_to_one_post_scene());
}

#[test]
fn v2_trade_output_preserves_rom_slot_selection_and_rejects_tamper() {
    let left = v2_trade_fixture(21, 7, 3);
    let right = v2_trade_fixture(31, 9, 6);
    let (output, _) = trade_party_pokemon_v2(&left, 0, &right, 0).unwrap();
    let mut raw = output.raw_bytes().to_vec();
    let offset = output.logical_sector_offsets[1];
    raw[offset + 200] ^= 1;
    // A corrupt container follows the ROM's valid-other-slot fallback.
    let fallback = parse_v2(&raw, TEST_REGISTRY).unwrap();
    assert_eq!(fallback.counter(), left.counter());
    raw[left.logical_sector_offsets[1] + 200] ^= 1;
    assert!(parse_v2(&raw, TEST_REGISTRY).is_err());
    let mut raw = output.raw_bytes().to_vec();
    rewrite_payload(&mut raw, output.selected_slot(), |payload| {
        payload[COOP_CRC_OFFSET] ^= 1
    });
    assert!(parse_v2(&raw, TEST_REGISTRY).is_err());
}

#[test]
fn same_trainer_binding_ignores_only_campaign_region() {
    let save = v2_trade_fixture(21, 7, 3);
    let identity = save.character_lineage();
    let mut other = identity;
    other.player_region ^= 1;
    assert!(identity.same_trainer(other));
    other.player_trainer_id[0] ^= 1;
    assert!(!identity.same_trainer(other));
    other = identity;
    other.player_name[0] ^= 1;
    assert!(!identity.same_trainer(other));
    other = identity;
    other.player_gender ^= 1;
    assert!(!identity.same_trainer(other));
}

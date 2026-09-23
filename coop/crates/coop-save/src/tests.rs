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
    assert_eq!(save.raw_bytes(), bytes);
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

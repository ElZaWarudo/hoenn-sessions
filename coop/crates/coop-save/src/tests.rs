use super::*;

const TEST_REGISTRY: RegistryContract = RegistryContract::new(7, [0xa5; 16]);

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

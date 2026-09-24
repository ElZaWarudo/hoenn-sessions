fn handoff_test_descriptor() -> Vec<u8> {
    let coop_offset = u32::try_from(coop_save::COOP_SAVE_OFFSET).unwrap();
    let coop_size = u32::try_from(coop_save::v2::COOP_SAVE_V2_SIZE).unwrap();
    let fields: [(u16, u8, u8, u32, u32); 15] = [
        (0x0100, 0, 2, 0, 0x490),
        (0x0102, 0, 1, 0x490, 4),
        (0x0103, 0, 1, 0x494, 2),
        (0x0104, 0, 2, 0x496, 0xca),
        (0x0106, 0, 1, 0x560, 0x400),
        (0x0200, 1, 2, 0, 17),
        (0x0203, 1, 1, 17, 1),
        (0x0204, 1, 2, 18, 0xb4 - 18),
        (0x020b, 1, 2, 0xb4, 4),
        (0x020c, 1, 2, 0xb8, 0x1fc - 0xb8),
        (0x020d, 1, 1, 0x1fc, 4),
        (0x0300, 2, 1, 0, 1),
        (0x0301, 2, 2, 1, 1),
        (0x0400, 3, 2, 0, coop_offset),
        (0x0403, 3, 1, coop_offset, coop_size),
    ];
    let mut bytes = vec![0; 44 + fields.len() * 16];
    write_u32(&mut bytes, 0, 0x3154_5043);
    write_u16(&mut bytes, 4, 3);
    write_u16(&mut bytes, 6, fields.len() as u16);
    let length = bytes.len() as u32;
    write_u32(&mut bytes, 8, length);
    write_u32(&mut bytes, 12, 44);
    for (offset, span) in [
        (16, 0x960),
        (20, 0x200),
        (24, 2),
        (28, coop_offset + coop_size),
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

fn handoff_fixture() -> (
    Phase2App,
    AuthenticatedActor,
    coop_cloud::LeaseContract,
    ClientInstanceId,
    SnapshotId,
) {
    let bytes = valid_character_sav(false);
    let root = std::env::temp_dir().join(format!("coop-handoff-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let mut catalog: serde_json::Value =
        serde_json::from_slice(include_bytes!("saves/fixtures/travel-catalog-v3.json")).unwrap();
    let descriptor = handoff_test_descriptor();
    let encoded = descriptor
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    catalog["shared_player_descriptor_hex"] = serde_json::json!(encoded);
    catalog["shared_player_descriptor_sha256"] =
        serde_json::json!(coop_cloud::Sha256Digest::of_bytes(&descriptor).as_hex());
    for world in catalog["worlds"].as_array_mut().unwrap() {
        for arrival in world["arrivals"].as_array_mut().unwrap() {
            let file = arrival["template_sav_path"].as_str().unwrap();
            std::fs::write(root.join(file), &bytes).unwrap();
            arrival["template_sav_sha256"] =
                serde_json::json!(coop_cloud::Sha256Digest::of_bytes(&bytes).as_hex());
            arrival["map_group"] = serde_json::json!(255);
            arrival["map_number"] = serde_json::json!(255);
            arrival["warp_id"] = serde_json::json!(255);
        }
    }
    let catalog_bytes = serde_json::to_vec(&catalog).unwrap();
    let config = Phase2Config::local(
        vec![0x55; 32],
        SigningPrivateKey::from_bytes([7; 32]),
        "local-test-key",
    )
    .unwrap()
    .with_legacy_test_runtime()
    .with_release_catalog_and_arrival_saves(
        &catalog_bytes,
        coop_cloud::Sha256Digest::of_bytes(&catalog_bytes),
        &root,
    )
    .unwrap()
    .with_test_adapters(
        Arc::new(FixedClock::new(1_700_000_000_000)),
        Arc::new(FixedEntropy::new((0_u8..=255).collect())),
    )
    .with_password_engine(Arc::new(ArgonPasswordEngine::new(8_192, 1, 1).unwrap()));
    let app = Phase2App::new(config).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    let (actor, lease, client) = account_and_lease(&app);
    let world = coop_protocol::RomWorldId::new(1).unwrap();
    let build = app
        .store
        .config
        .release_catalog
        .as_ref()
        .unwrap()
        .for_snapshot(Some(world), Some(world))
        .unwrap()
        .clone();
    app.store
        .write_transaction(|state| {
            state
                .leases
                .get_mut(&actor.character_id)
                .unwrap()
                .runtime_binding = Some(storage::RuntimeWorldBinding {
                world_id: world,
                build: build.clone(),
                session: lease.stable_runtime_session(),
            });
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let (mut request, sav, _) = snapshot_request_for_sav(lease.clone(), actor, client, &bytes);
    let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
    request.files = vec![sav.clone(), pending.clone()];
    request.pending_commits_sha256 = pending.sha256;
    let prepared = app.prepare(actor, request.clone()).unwrap();
    for target in &prepared.upload_targets {
        let ticket = target.url.as_str().split("?ticket=").nth(1).unwrap();
        let body = if target.artifact == ArtifactIdentity::CharacterSav {
            bytes.clone()
        } else {
            b"[]".to_vec()
        };
        app.upload(ticket, body).unwrap();
    }
    let record = app
        .finalize(
            actor,
            SnapshotFinalizeRequest::new(
                request.snapshot_id,
                SnapshotFinalizeFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.current_revision,
                    lease.session_epoch,
                    client,
                    request.idempotency_key,
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(record.revision, Revision::new(1));
    (app, actor, lease, client, record.snapshot_id)
}

fn acquire_bound_world(
    app: &Phase2App,
    actor: AuthenticatedActor,
    world: coop_protocol::RomWorldId,
) -> (coop_cloud::LeaseContract, ClientInstanceId) {
    let client = id(ClientInstanceId::new);
    let lease = app
        .acquire(
            actor,
            AcquireLeaseRequest::new(actor.character_id, client, id(IdempotencyKey::new)),
        )
        .unwrap();
    let build = app
        .store
        .config
        .release_catalog
        .as_ref()
        .unwrap()
        .for_snapshot(Some(world), Some(world))
        .unwrap()
        .clone();
    app.store
        .write_transaction(|state| {
            state
                .leases
                .get_mut(&actor.character_id)
                .unwrap()
                .runtime_binding = Some(storage::RuntimeWorldBinding {
                world_id: world,
                build: build.clone(),
                session: lease.stable_runtime_session(),
            });
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    (lease, client)
}

fn travel_once(
    app: &Phase2App,
    actor: AuthenticatedActor,
    lease: &coop_cloud::LeaseContract,
    client: ClientInstanceId,
    source_id: SnapshotId,
    portal_id: &str,
) -> coop_cloud::SnapshotRecord {
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: lease.current_revision,
        source_snapshot_id: source_id,
        portal_id: portal_id.to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.commit_rom_handoff(
        actor,
        &RomHandoffCommitRequest {
            api_version: coop_cloud::ApiVersion::V1,
            character_id: actor.character_id,
            session_id: lease.session_id,
            session_epoch: lease.session_epoch,
            client_instance_id: client,
            expected_revision: lease.current_revision,
            stage_id: staged.stage_id,
            destination_save_sha256: staged.destination_save_sha256,
            idempotency_key: request.idempotency_key,
        },
    )
    .unwrap()
}

#[test]
fn rom_handoff_stages_then_commits_only_acknowledged_destination() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    assert_eq!(
        staged.destination_world_id,
        coop_protocol::RomWorldId::new(2).unwrap()
    );
    assert_eq!(app.prepare_rom_handoff(actor, &request).unwrap(), staged);
    app.store
        .read_transaction(|state| {
            let character = state.characters.get(&actor.character_id).unwrap();
            assert_eq!(character.active_snapshot, Some(source_id));
            assert_eq!(character.revision, Revision::new(1));
            assert!(!state.leases.get(&actor.character_id).unwrap().released);
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert_eq!(
        app.prepare(
            actor,
            snapshot_request_for_sav(
                lease.clone(),
                actor,
                client,
                &valid_character_sav_generation(false, 2)
            )
            .0
        ),
        Err(Phase2Error::Conflict)
    );
    let mut commit = RomHandoffCommitRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        stage_id: staged.stage_id,
        destination_save_sha256: coop_cloud::Sha256Digest::of_bytes(b"wrong"),
        idempotency_key: request.idempotency_key,
    };
    assert_eq!(
        app.commit_rom_handoff(actor, &commit),
        Err(Phase2Error::Conflict)
    );
    commit.destination_save_sha256 = staged.destination_save_sha256;
    let record = app.commit_rom_handoff(actor, &commit).unwrap();
    assert_eq!(record.snapshot_id, staged.stage_id);
    assert_eq!(record.rom_world_id, staged.destination_world_id);
    assert_eq!(record.revision, Revision::new(2));
    assert_eq!(app.commit_rom_handoff(actor, &commit).unwrap(), record);
    app.store
        .read_transaction(|state| {
            let character = state.characters.get(&actor.character_id).unwrap();
            assert_eq!(character.active_snapshot, Some(staged.stage_id));
            assert_eq!(
                character
                    .world_heads
                    .get(&coop_protocol::RomWorldId::new(1).unwrap()),
                Some(&source_id)
            );
            assert_eq!(
                character
                    .world_heads
                    .get(&coop_protocol::RomWorldId::new(2).unwrap()),
                Some(&staged.stage_id)
            );
            assert!(state.leases.get(&actor.character_id).unwrap().released);
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn rom_handoff_abort_keeps_source_active() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.abort_rom_handoff(
        actor,
        coop_cloud::LeaseFence::new(
            lease.session_id,
            actor.character_id,
            Revision::new(1),
            lease.session_epoch,
            client,
        ),
        staged.stage_id,
    )
    .unwrap();
    app.store
        .read_transaction(|state| {
            assert_eq!(
                state
                    .characters
                    .get(&actor.character_id)
                    .unwrap()
                    .active_snapshot,
                Some(source_id)
            );
            assert!(state.retired_snapshots.contains(&staged.stage_id));
            assert!(!state.rom_handoff_staging.contains_key(&actor.character_id));
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert_eq!(
        app.commit_rom_handoff(
            actor,
            &RomHandoffCommitRequest {
                api_version: coop_cloud::ApiVersion::V1,
                character_id: actor.character_id,
                session_id: lease.session_id,
                session_epoch: lease.session_epoch,
                client_instance_id: client,
                expected_revision: Revision::new(1),
                stage_id: staged.stage_id,
                destination_save_sha256: staged.destination_save_sha256,
                idempotency_key: request.idempotency_key,
            }
        ),
        Err(Phase2Error::NotFound)
    );
}

#[test]
fn rom_handoff_abort_rejects_stale_revision_fence() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    let stale = coop_cloud::LeaseFence::new(
        lease.session_id,
        actor.character_id,
        Revision::new(0),
        lease.session_epoch,
        client,
    );
    assert_eq!(
        app.abort_rom_handoff(actor, stale, staged.stage_id),
        Err(Phase2Error::Conflict)
    );
    assert!(
        app.store
            .read_transaction(|state| Ok::<_, Phase2Error>(
                state.rom_handoff_staging.contains_key(&actor.character_id)
            ))
            .unwrap()
    );
}

#[test]
fn rom_handoff_abort_is_idempotent_and_fences_delayed_prepare() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    let fence = coop_cloud::LeaseFence::new(
        lease.session_id,
        actor.character_id,
        Revision::new(1),
        lease.session_epoch,
        client,
    );
    app.abort_rom_handoff(actor, fence, staged.stage_id)
        .unwrap();
    // A lost 204 response can be retried safely after the stage has already
    // been removed and its objects sealed.
    app.abort_rom_handoff(actor, fence, staged.stage_id)
        .unwrap();
    // The old idempotency key is permanently fenced while this source head is
    // active; it cannot recreate an equivalent stage after the lost response.
    assert_eq!(
        app.prepare_rom_handoff(actor, &request),
        Err(Phase2Error::Conflict)
    );
    app.store
        .read_transaction(|state| {
            let tombstone = state
                .rom_handoff_aborts
                .get(&(actor.character_id, request.idempotency_key))
                .unwrap();
            assert_eq!(tombstone.stage_id, staged.stage_id);
            assert_eq!(tombstone.request, request);
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn rom_handoff_recovery_status_recovers_expired_prepare_from_key_only() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    // Model a client crash after the prepare was committed but before its
    // response reached the journal.  Recovery sees only the durable key.
    app.store
        .write_transaction(|state| {
            state
                .rom_handoff_staging
                .get_mut(&actor.character_id)
                .unwrap()
                .expires_at = 0;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    app.store
        .write_transaction(|state| {
            state.leases.get_mut(&actor.character_id).unwrap().released = true;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let (replacement, replacement_client) =
        acquire_bound_world(&app, actor, coop_protocol::RomWorldId::new(1).unwrap());
    let status = app
        .reconcile_rom_handoff(
            actor,
            coop_cloud::LeaseFence::new(
                replacement.session_id,
                actor.character_id,
                replacement.current_revision,
                replacement.session_epoch,
                replacement_client,
            ),
            &coop_cloud::RomHandoffRecoveryRequest {
                api_version: coop_cloud::ApiVersion::V1,
                character_id: actor.character_id,
                idempotency_key: request.idempotency_key,
            },
        )
        .unwrap();
    assert_eq!(
        status,
        coop_cloud::RomHandoffRecoveryStatus::Aborted {
            stage_id: staged.stage_id,
            source_snapshot_id: source_id,
            source_world_id: coop_protocol::RomWorldId::new(1).unwrap(),
            expected_revision: Revision::new(1),
            idempotency_key: request.idempotency_key,
        }
    );
    assert_eq!(
        app.prepare_rom_handoff(actor, &request),
        Err(Phase2Error::Conflict)
    );
}

#[test]
fn rom_handoff_recovery_status_keeps_live_stage_after_lease_rollover() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.store
        .write_transaction(|state| {
            state.leases.get_mut(&actor.character_id).unwrap().released = true;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let (replacement, replacement_client) =
        acquire_bound_world(&app, actor, coop_protocol::RomWorldId::new(1).unwrap());
    let status = app
        .reconcile_rom_handoff(
            actor,
            coop_cloud::LeaseFence::new(
                replacement.session_id,
                actor.character_id,
                replacement.current_revision,
                replacement.session_epoch,
                replacement_client,
            ),
            &coop_cloud::RomHandoffRecoveryRequest {
                api_version: coop_cloud::ApiVersion::V1,
                character_id: actor.character_id,
                idempotency_key: request.idempotency_key,
            },
        )
        .unwrap();
    assert_eq!(
        status,
        coop_cloud::RomHandoffRecoveryStatus::Staged {
            stage_id: staged.stage_id,
            source_snapshot_id: source_id,
            source_world_id: coop_protocol::RomWorldId::new(1).unwrap(),
            expected_revision: Revision::new(1),
            idempotency_key: request.idempotency_key,
        }
    );
    assert!(
        app.store
            .read_transaction(|state| {
                Ok::<_, Phase2Error>(state.rom_handoff_staging.contains_key(&actor.character_id))
            })
            .unwrap()
    );
}

#[test]
fn rom_handoff_abort_accepts_replaced_source_lease_at_same_head() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.store
        .write_transaction(|state| {
            state.leases.get_mut(&actor.character_id).unwrap().released = true;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let (replacement, replacement_client) =
        acquire_bound_world(&app, actor, coop_protocol::RomWorldId::new(1).unwrap());
    let replacement_fence = coop_cloud::LeaseFence::new(
        replacement.session_id,
        actor.character_id,
        replacement.current_revision,
        replacement.session_epoch,
        replacement_client,
    );
    app.abort_rom_handoff(actor, replacement_fence, staged.stage_id)
        .unwrap();
    // A second lost-response retry remains idempotent after the lease rollover.
    app.abort_rom_handoff(actor, replacement_fence, staged.stage_id)
        .unwrap();
    assert!(
        !app.store
            .read_transaction(|state| {
                Ok::<_, Phase2Error>(state.rom_handoff_staging.contains_key(&actor.character_id))
            })
            .unwrap()
    );
}

#[test]
fn rom_handoff_abort_rejects_wrong_stage_and_committed_destination() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    let fence = coop_cloud::LeaseFence::new(
        lease.session_id,
        actor.character_id,
        Revision::new(1),
        lease.session_epoch,
        client,
    );
    assert_eq!(
        app.abort_rom_handoff(actor, fence, id(SnapshotId::new)),
        Err(Phase2Error::Conflict)
    );
    let foreign = AuthenticatedActor {
        user_id: id(coop_cloud::UserId::new),
        character_id: actor.character_id,
    };
    assert_eq!(
        app.abort_rom_handoff(foreign, fence, staged.stage_id),
        Err(Phase2Error::NotFound)
    );
    app.commit_rom_handoff(
        actor,
        &RomHandoffCommitRequest {
            api_version: coop_cloud::ApiVersion::V1,
            character_id: actor.character_id,
            session_id: lease.session_id,
            session_epoch: lease.session_epoch,
            client_instance_id: client,
            expected_revision: Revision::new(1),
            stage_id: staged.stage_id,
            destination_save_sha256: staged.destination_save_sha256,
            idempotency_key: request.idempotency_key,
        },
    )
    .unwrap();
    // A committed destination has advanced the source head and released its
    // lease, so a delayed abort cannot tombstone or undo that commit.
    assert_eq!(
        app.abort_rom_handoff(actor, fence, staged.stage_id),
        Err(Phase2Error::Conflict)
    );
}

#[test]
fn rom_handoff_abort_tombstones_are_bounded_and_gc_on_commit() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let base = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let world = coop_protocol::RomWorldId::new(1).unwrap();
    app.store
        .write_transaction(|state| {
            for _ in 0..storage::MAX_ROM_HANDOFF_ABORT_TOMBSTONES_PER_CHARACTER {
                let mut request = base.clone();
                request.idempotency_key = id(IdempotencyKey::new);
                let stage_id = id(SnapshotId::new);
                state.rom_handoff_aborts.insert(
                    (actor.character_id, request.idempotency_key),
                    storage::RomHandoffAbortTombstone {
                        request,
                        stage_id,
                        source_world_id: world,
                    },
                );
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let request = base.clone();
    assert_eq!(
        app.prepare_rom_handoff(actor, &request),
        Err(Phase2Error::Busy)
    );
    // A fresh source head demonstrates that advancing through a successful
    // handoff collects its old replay fences in the same commit transaction.
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.abort_rom_handoff(
        actor,
        coop_cloud::LeaseFence::new(
            lease.session_id,
            actor.character_id,
            Revision::new(1),
            lease.session_epoch,
            client,
        ),
        staged.stage_id,
    )
    .unwrap();
    let retry = RomHandoffPrepareRequest {
        idempotency_key: id(IdempotencyKey::new),
        ..request.clone()
    };
    let staged = app.prepare_rom_handoff(actor, &retry).unwrap();
    app.commit_rom_handoff(
        actor,
        &RomHandoffCommitRequest {
            api_version: coop_cloud::ApiVersion::V1,
            character_id: actor.character_id,
            session_id: lease.session_id,
            session_epoch: lease.session_epoch,
            client_instance_id: client,
            expected_revision: Revision::new(1),
            stage_id: staged.stage_id,
            destination_save_sha256: staged.destination_save_sha256,
            idempotency_key: retry.idempotency_key,
        },
    )
    .unwrap();
    app.store
        .read_transaction(|state| {
            assert!(state.rom_handoff_aborts.is_empty());
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn expired_rom_handoff_stage_is_retired_before_new_prepare() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let mut request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let first = app.prepare_rom_handoff(actor, &request).unwrap();
    app.store
        .write_transaction(|state| {
            state
                .rom_handoff_staging
                .get_mut(&actor.character_id)
                .unwrap()
                .expires_at = 0;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    request.idempotency_key = id(IdempotencyKey::new);
    let second = app.prepare_rom_handoff(actor, &request).unwrap();
    assert_ne!(first.stage_id, second.stage_id);
    app.store
        .read_transaction(|state| {
            assert!(state.retired_snapshots.contains(&first.stage_id));
            assert_eq!(
                state
                    .characters
                    .get(&actor.character_id)
                    .unwrap()
                    .active_snapshot,
                Some(source_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn expired_snapshot_prepare_does_not_block_rom_handoff() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let (declaration, _, _) = snapshot_request_for_sav(
        app.store
            .read_transaction(|state| {
                Ok::<_, Phase2Error>(
                    state
                        .leases
                        .get(&actor.character_id)
                        .unwrap()
                        .contract
                        .clone(),
                )
            })
            .unwrap(),
        actor,
        client,
        &valid_character_sav_generation(false, 2),
    );
    app.prepare(actor, declaration.clone()).unwrap();
    app.store
        .write_transaction(|state| {
            state
                .prepared
                .get_mut(&declaration.snapshot_id)
                .unwrap()
                .expires_at = 0;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    assert_eq!(
        staged.destination_world_id,
        coop_protocol::RomWorldId::new(2).unwrap()
    );
    app.store
        .read_transaction(|state| {
            assert!(!state.prepared.contains_key(&declaration.snapshot_id));
            assert!(state.retired_snapshots.contains(&declaration.snapshot_id));
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn aborted_handoff_does_not_exceed_retired_snapshot_bound() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    let staged = app.prepare_rom_handoff(actor, &request).unwrap();
    app.store
        .write_transaction(|state| {
            while state.retired_snapshots.len() < storage::MAX_RETIRED_SNAPSHOTS {
                state.retired_snapshots.insert(id(SnapshotId::new));
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    app.abort_rom_handoff(
        actor,
        coop_cloud::LeaseFence::new(
            lease.session_id,
            actor.character_id,
            Revision::new(1),
            lease.session_epoch,
            client,
        ),
        staged.stage_id,
    )
    .unwrap();
    app.store
        .read_transaction(|state| {
            assert_eq!(
                state.retired_snapshots.len(),
                storage::MAX_RETIRED_SNAPSHOTS
            );
            assert!(!state.rom_handoff_staging.contains_key(&actor.character_id));
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn expired_restore_stage_does_not_block_rom_handoff() {
    let (app, actor, lease, client, source_id) = handoff_fixture();
    let restore = SnapshotRestoreRequest::new(
        source_id,
        lease.session_id,
        actor.character_id,
        Revision::new(1),
        lease.session_epoch,
        client,
        id(IdempotencyKey::new),
    );
    let abandoned_id = id(SnapshotId::new);
    app.store
        .write_transaction(|state| {
            state.restore_staging.insert(
                actor.character_id,
                storage::RestoreStage {
                    request: restore.clone(),
                    snapshot_id: abandoned_id,
                    expires_at: 0,
                    storage_bytes: 0,
                    created_objects: Vec::new(),
                },
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: Revision::new(1),
        source_snapshot_id: source_id,
        portal_id: "to_next".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    app.prepare_rom_handoff(actor, &request).unwrap();
    app.store
        .read_transaction(|state| {
            assert!(!state.restore_staging.contains_key(&actor.character_id));
            assert_eq!(
                state
                    .characters
                    .get(&actor.character_id)
                    .unwrap()
                    .active_snapshot,
                Some(source_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn three_world_trip_returns_to_dormant_main_head() {
    let (app, actor, _initial_lease, initial_client, main_head) = handoff_fixture();
    let main = coop_protocol::RomWorldId::new(1).unwrap();
    let cormoria = coop_protocol::RomWorldId::new(2).unwrap();
    let third = coop_protocol::RomWorldId::new(3).unwrap();
    let main_lease = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>(
                state
                    .leases
                    .get(&actor.character_id)
                    .unwrap()
                    .contract
                    .clone(),
            )
        })
        .unwrap();
    let cormoria_head = travel_once(
        &app,
        actor,
        &main_lease,
        initial_client,
        main_head,
        "to_next",
    );
    assert_eq!(cormoria_head.rom_world_id, cormoria);
    let (lease, client) = acquire_bound_world(&app, actor, cormoria);
    let third_head = travel_once(
        &app,
        actor,
        &lease,
        client,
        cormoria_head.snapshot_id,
        "to_next",
    );
    assert_eq!(third_head.rom_world_id, third);
    let (lease, client) = acquire_bound_world(&app, actor, third);
    let returned = travel_once(
        &app,
        actor,
        &lease,
        client,
        third_head.snapshot_id,
        "to_next",
    );
    assert_eq!(returned.rom_world_id, main);
    assert_eq!(returned.revision, Revision::new(4));
    app.store
        .read_transaction(|state| {
            let character = state.characters.get(&actor.character_id).unwrap();
            assert_eq!(character.active_snapshot, Some(returned.snapshot_id));
            assert_eq!(
                character.world_heads.get(&main),
                Some(&returned.snapshot_id)
            );
            assert_eq!(
                character.world_heads.get(&cormoria),
                Some(&cormoria_head.snapshot_id)
            );
            assert_eq!(
                character.world_heads.get(&third),
                Some(&third_head.snapshot_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn handoff_prunes_old_history_at_snapshot_cap_without_losing_world_heads() {
    let (app, actor, _initial_lease, client, first_id) = handoff_fixture();
    let main = coop_protocol::RomWorldId::new(1).unwrap();
    let cormoria = coop_protocol::RomWorldId::new(2).unwrap();
    let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
    let mut latest_id = first_id;
    for revision in 2..=100 {
        let lease = app
            .store
            .read_transaction(|state| {
                Ok::<_, Phase2Error>(
                    state
                        .leases
                        .get(&actor.character_id)
                        .unwrap()
                        .contract
                        .clone(),
                )
            })
            .unwrap();
        let bytes = valid_character_sav_generation(false, revision);
        let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &bytes).unwrap();
        let declaration = SnapshotPrepareRequest::new(
            id(SnapshotId::new),
            main,
            SnapshotPrepareFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav.clone(), pending.clone()],
            pending.sha256,
        )
        .unwrap();
        let prepared = app.prepare(actor, declaration.clone()).unwrap();
        for target in &prepared.upload_targets {
            let ticket = target.url.as_str().split("?ticket=").nth(1).unwrap();
            let body = if target.artifact == ArtifactIdentity::CharacterSav {
                bytes.clone()
            } else {
                b"[]".to_vec()
            };
            app.upload(ticket, body).unwrap();
        }
        latest_id = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    declaration.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        declaration.idempotency_key,
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .unwrap(),
            )
            .unwrap()
            .snapshot_id;
    }
    let lease = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>(
                state
                    .leases
                    .get(&actor.character_id)
                    .unwrap()
                    .contract
                    .clone(),
            )
        })
        .unwrap();
    assert_eq!(lease.current_revision, Revision::new(100));
    let cormoria_head = travel_once(&app, actor, &lease, client, latest_id, "to_next");
    assert_eq!(cormoria_head.rom_world_id, cormoria);
    let retired_id = app
        .store
        .read_transaction(|state| {
            let character = state.characters.get(&actor.character_id).unwrap();
            assert_eq!(character.world_heads.get(&main), Some(&latest_id));
            assert_eq!(
                character.world_heads.get(&cormoria),
                Some(&cormoria_head.snapshot_id)
            );
            assert!(state.snapshots.contains_key(&first_id));
            assert_eq!(
                state
                    .snapshots
                    .values()
                    .filter(|record| record.character_id == actor.character_id)
                    .count(),
                storage::MAX_SNAPSHOTS_PER_CHARACTER
            );
            assert_eq!(state.retiring_snapshots.len(), 1);
            Ok::<_, Phase2Error>(*state.retiring_snapshots.keys().next().unwrap())
        })
        .unwrap();
    let (lease, client) = acquire_bound_world(&app, actor, cormoria);
    let bytes = valid_character_sav_generation(false, 102);
    let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &bytes).unwrap();
    let declaration = SnapshotPrepareRequest::new(
        id(SnapshotId::new),
        cormoria,
        SnapshotPrepareFence::new(
            lease.session_id,
            actor.character_id,
            lease.current_revision,
            lease.session_epoch,
            client,
            id(IdempotencyKey::new),
        ),
        vec![sav.clone(), pending.clone()],
        pending.sha256,
    )
    .unwrap();
    let prepared = app.prepare(actor, declaration.clone()).unwrap();
    for target in &prepared.upload_targets {
        let ticket = target.url.as_str().split("?ticket=").nth(1).unwrap();
        app.upload(
            ticket,
            if target.artifact == ArtifactIdentity::CharacterSav {
                bytes.clone()
            } else {
                b"[]".to_vec()
            },
        )
        .unwrap();
    }
    let saved = app
        .finalize(
            actor,
            SnapshotFinalizeRequest::new(
                declaration.snapshot_id,
                SnapshotFinalizeFence::new(
                    lease.session_id,
                    actor.character_id,
                    lease.current_revision,
                    lease.session_epoch,
                    client,
                    declaration.idempotency_key,
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(saved.revision, Revision::new(102));
    let lease = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>(
                state
                    .leases
                    .get(&actor.character_id)
                    .unwrap()
                    .contract
                    .clone(),
            )
        })
        .unwrap();
    let return_request = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actor.character_id,
        session_id: lease.session_id,
        session_epoch: lease.session_epoch,
        client_instance_id: client,
        expected_revision: lease.current_revision,
        source_snapshot_id: saved.snapshot_id,
        portal_id: "to_previous".to_owned(),
        idempotency_key: id(IdempotencyKey::new),
    };
    app.prepare_rom_handoff(actor, &return_request).unwrap();
    app.store
        .read_transaction(|state| {
            assert!(state.retiring_snapshots.is_empty());
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    for artifact in [
        ArtifactIdentity::CharacterSav,
        ArtifactIdentity::PendingCommits,
    ] {
        let key = storage::Store::object_key(actor.character_id, retired_id, artifact);
        assert_eq!(app.store.objects.get(&key).unwrap(), None);
        assert!(
            !app.store
                .objects
                .put_if_absent(key, b"late write".to_vec())
                .unwrap()
        );
    }
}

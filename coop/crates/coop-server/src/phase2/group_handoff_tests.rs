fn paired_handoff_fixture() -> (
    Phase2App,
    [AuthenticatedActor; 2],
    coop_cloud::GroupRomHandoffIntent,
) {
    let (app, first, _, _, first_snapshot_id) = handoff_fixture();
    app.add_invitation("paired-second-invite").unwrap();
    let second_registered = app
        .register(
            RegisterRequest::new(
                "PairedSecond",
                password(),
                InvitationCode::new("paired-second-invite").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let second = AuthenticatedActor {
        user_id: second_registered.user_id,
        character_id: second_registered.character_id,
    };
    let second_client = id(ClientInstanceId::new);
    let second_lease = app
        .acquire(
            second,
            AcquireLeaseRequest::new(second.character_id, second_client, id(IdempotencyKey::new)),
        )
        .unwrap();
    let main = coop_protocol::RomWorldId::new(1).unwrap();
    let catalog = app.store.config.release_catalog.as_ref().unwrap();
    let build = catalog
        .for_snapshot(Some(main), Some(main))
        .unwrap()
        .clone();
    app.store
        .write_transaction(|state| {
            state
                .leases
                .get_mut(&second.character_id)
                .unwrap()
                .runtime_binding = Some(storage::RuntimeWorldBinding {
                world_id: main,
                build: build.clone(),
                session: second_lease.stable_runtime_session(),
            });
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let source_key = storage::Store::object_key(
        first.character_id,
        first_snapshot_id,
        ArtifactIdentity::CharacterSav,
    );
    let bytes = app.store.objects.get(&source_key).unwrap().unwrap();
    let (mut request, sav, _) =
        snapshot_request_for_sav(second_lease, second, second_client, &bytes);
    let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
    request.files = vec![sav.clone(), pending.clone()];
    request.pending_commits_sha256 = pending.sha256;
    let prepared = app.prepare(second, request.clone()).unwrap();
    for target in &prepared.upload_targets {
        let ticket = target.url.as_str().split("?ticket=").nth(1).unwrap();
        let body = if target.artifact == ArtifactIdentity::CharacterSav {
            bytes.clone()
        } else {
            b"[]".to_vec()
        };
        app.upload(ticket, body).unwrap();
    }
    let second_snapshot = app
        .finalize(
            second,
            SnapshotFinalizeRequest::new(
                request.snapshot_id,
                SnapshotFinalizeFence::new(
                    second_lease.session_id,
                    second.character_id,
                    second_lease.current_revision,
                    second_lease.session_epoch,
                    second_client,
                    request.idempotency_key,
                ),
                vec![sav, pending.clone()],
                pending.sha256,
                None,
            )
            .unwrap(),
        )
        .unwrap();
    let (first_lease, second_lease) = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>((
                state.leases[&first.character_id].contract,
                state.leases[&second.character_id].contract,
            ))
        })
        .unwrap();
    let group = coop_cloud::Group::new(first.character_id, second.character_id).unwrap();
    let group_id = id(coop_cloud::GroupId::new);
    let zone = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>(
                state.characters[&first.character_id]
                    .state
                    .world_zone
                    .clone(),
            )
        })
        .unwrap();
    app.store
        .write_transaction(|state| {
            state.groups.insert(
                group_id,
                storage::GroupRecord {
                    group,
                    zone: zone.clone(),
                    status: storage::GroupStatus::Active,
                    zone_revision: 0,
                },
            );
            for member in group.members() {
                state.active_group_by_member.insert(member, group_id);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let destination = coop_protocol::RomWorldId::new(2).unwrap();
    let arrival_digest = catalog
        .arrival_template(destination, "from_previous")
        .unwrap()
        .1;
    let descriptor_digest =
        coop_cloud::Sha256Digest::of_bytes(catalog.transfer_descriptor().unwrap());
    let member =
        |lease: coop_cloud::LeaseContract, snapshot_id| coop_cloud::GroupRomHandoffSource {
            fence: lease.fence(),
            source_snapshot_id: snapshot_id,
        };
    let ordered = if first.character_id < second.character_id {
        (
            [first, second],
            [
                member(first_lease, first_snapshot_id),
                member(second_lease, second_snapshot.snapshot_id),
            ],
        )
    } else {
        (
            [second, first],
            [
                member(second_lease, second_snapshot.snapshot_id),
                member(first_lease, first_snapshot_id),
            ],
        )
    };
    let intent = coop_cloud::GroupRomHandoffIntent {
        api_version: coop_cloud::ApiVersion::V1,
        group_id,
        group_zone_revision: 0,
        source_zone: zone,
        source_world_id: main,
        destination_world_id: destination,
        portal_id: "to_next".to_owned(),
        arrival_portal_id: "from_previous".to_owned(),
        catalog_sha256: catalog.digest(),
        descriptor_sha256: descriptor_digest,
        arrival_template_sha256: arrival_digest,
        members: ordered.1,
        idempotency_key: id(IdempotencyKey::new),
    };
    (app, ordered.0, intent)
}

#[test]
fn paired_prepare_stages_both_saves_without_advancing_heads_and_replays_exactly() {
    let (app, actors, intent) = paired_handoff_fixture();
    let stage = group_handoff::prepare_pair(&app.store, actors, &intent).unwrap();
    assert_eq!(stage.intent, intent);
    assert_eq!(
        group_handoff::prepare_pair(&app.store, actors, &intent).unwrap(),
        stage
    );
    app.store
        .read_transaction(|state| {
            assert_eq!(state.group_rom_handoff_stages[&intent.group_id], stage);
            assert_eq!(state.groups[&intent.group_id].zone, intent.source_zone);
            for (index, actor) in actors.iter().enumerate() {
                let character = &state.characters[&actor.character_id];
                assert_eq!(
                    character.active_snapshot,
                    Some(intent.members[index].source_snapshot_id)
                );
                assert_eq!(
                    character.revision,
                    intent.members[index].fence.current_revision
                );
                assert!(!state.leases[&actor.character_id].released);
                assert!(state.paired_handoff_for_member(actor.character_id));
                assert!(matches!(
                    group_travel::lease_matches(
                        state,
                        actor.character_id,
                        intent.members[index].fence,
                        app.store.now(),
                    ),
                    Err(Phase2Error::Conflict)
                ));
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    for (index, actor) in actors.iter().enumerate() {
        let key = storage::Store::object_key(
            actor.character_id,
            stage.stage_ids[index],
            ArtifactIdentity::CharacterSav,
        );
        let bytes = app.store.objects.get(&key).unwrap().unwrap();
        assert_eq!(
            coop_cloud::Sha256Digest::of_bytes(&bytes),
            stage.destination_save_sha256[index]
        );
    }
    let fence = intent.members[0].fence;
    assert!(matches!(
        app.release(
            actors[0],
            coop_cloud::ReleaseLeaseRequest::new(fence, id(IdempotencyKey::new)),
        ),
        Err(Phase2Error::Conflict)
    ));
    let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"save").unwrap();
    let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
    let save_request = SnapshotPrepareRequest::new(
        id(SnapshotId::new),
        intent.source_world_id,
        SnapshotPrepareFence::new(
            fence.session_id,
            fence.character_id,
            fence.current_revision,
            fence.session_epoch,
            fence.client_instance_id,
            id(IdempotencyKey::new),
        ),
        vec![sav, pending.clone()],
        pending.sha256,
    )
    .unwrap();
    assert!(matches!(
        app.prepare(actors[0], save_request),
        Err(Phase2Error::Conflict)
    ));
}

#[test]
fn paired_prepare_rejects_foreign_actor_and_stale_companion_head_without_stage() {
    let (app, actors, intent) = paired_handoff_fixture();
    assert!(matches!(
        group_handoff::prepare_pair(&app.store, [actors[0], actors[0]], &intent),
        Err(Phase2Error::Conflict)
    ));
    let mut stale = intent.clone();
    stale.members[1].source_snapshot_id = id(SnapshotId::new);
    assert!(matches!(
        group_handoff::prepare_pair(&app.store, actors, &stale),
        Err(Phase2Error::Conflict)
    ));
    let mut untrusted = intent.clone();
    untrusted.catalog_sha256 = coop_cloud::Sha256Digest::of_bytes(b"other catalog");
    assert!(matches!(
        group_handoff::prepare_pair(&app.store, actors, &untrusted),
        Err(Phase2Error::Conflict)
    ));
    app.store
        .write_transaction(|state| {
            state
                .groups
                .get_mut(&intent.group_id)
                .unwrap()
                .zone_revision += 1;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::prepare_pair(&app.store, actors, &intent),
        Err(Phase2Error::Conflict)
    ));
    assert!(
        app.store
            .inspect_state(|state| state.group_rom_handoff_stages.is_empty())
            .unwrap()
    );
}

fn paired_join_request(
    intent: &coop_cloud::GroupRomHandoffIntent,
    index: usize,
) -> coop_cloud::GroupRomHandoffJoinRequest {
    coop_cloud::GroupRomHandoffJoinRequest {
        api_version: coop_cloud::ApiVersion::V1,
        group_id: intent.group_id,
        fence: intent.members[index].fence,
        source_snapshot_id: intent.members[index].source_snapshot_id,
        portal_id: intent.portal_id.clone(),
        client_intent_key: coop_cloud::IdempotencyKey::new(Uuid::from_u128((index as u128) + 1))
            .unwrap(),
    }
}

fn paired_arrival_request(
    app: &Phase2App,
    intent: &coop_cloud::GroupRomHandoffIntent,
    index: usize,
    status: &coop_cloud::GroupRomHandoffStatus,
) -> coop_cloud::GroupRomHandoffArrivalRequest {
    let coop_cloud::GroupRomHandoffStatus::Staged {
        idempotency_key,
        stage_id,
        destination_save_sha256,
        arrival_challenge,
        ..
    } = status
    else {
        panic!("expected staged handoff")
    };
    let build = app
        .store
        .config
        .release_catalog
        .as_ref()
        .unwrap()
        .for_snapshot(
            Some(intent.destination_world_id),
            Some(intent.destination_world_id),
        )
        .unwrap()
        .clone();
    let mut request = coop_cloud::GroupRomHandoffArrivalRequest {
        api_version: coop_cloud::ApiVersion::V1,
        group_id: intent.group_id,
        fence: intent.members[index].fence,
        idempotency_key: *idempotency_key,
        stage_id: *stage_id,
        destination_save_sha256: *destination_save_sha256,
        destination_build: build,
        acknowledgment_mac: coop_cloud::Sha256Digest::of_bytes(b"unset"),
    };
    request.acknowledgment_mac = request.expected_mac(*arrival_challenge);
    request
}

#[test]
fn paired_rendezvous_requires_independent_join_and_both_arrivals_before_atomic_promotion() {
    let (app, actors, intent) = paired_handoff_fixture();
    let first =
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap();
    assert!(matches!(
        first,
        coop_cloud::GroupRomHandoffStatus::Pending {
            submitted_by: [true, false],
            ..
        }
    ));
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 1)),
        Err(Phase2Error::InvalidRequest)
    ));
    let second =
        group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    assert!(matches!(
        second,
        coop_cloud::GroupRomHandoffStatus::Staged { .. }
    ));
    let key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    let first_staged = group_handoff::status(
        &app.store,
        actors[0],
        &coop_cloud::GroupRomHandoffStatusRequest {
            api_version: coop_cloud::ApiVersion::V1,
            group_id: intent.group_id,
            fence: intent.members[0].fence,
            idempotency_key: key,
        },
    )
    .unwrap();
    let first_request = paired_arrival_request(&app, &intent, 0, &first_staged);
    let mut forged = first_request.clone();
    forged.acknowledgment_mac = coop_cloud::Sha256Digest::of_bytes(b"wrong");
    assert!(matches!(
        group_handoff::arrive(&app.store, actors[0], &forged),
        Err(Phase2Error::Conflict)
    ));
    let pending = group_handoff::arrive(&app.store, actors[0], &first_request).unwrap();
    assert!(matches!(
        pending,
        coop_cloud::GroupRomHandoffStatus::Staged {
            acknowledged_by: [true, false],
            ..
        }
    ));
    app.store
        .read_transaction(|state| {
            assert_eq!(state.groups[&intent.group_id].zone, intent.source_zone);
            let mut tampered = state.group_rom_handoff_stages[&intent.group_id].clone();
            tampered.verified_arrivals[0]
                .as_mut()
                .unwrap()
                .acknowledgment_sha256 = coop_cloud::Sha256Digest::of_bytes(b"tampered");
            assert!(tampered.validate().is_err());
            for (index, actor) in actors.iter().enumerate() {
                assert_eq!(
                    state.characters[&actor.character_id].active_snapshot,
                    Some(intent.members[index].source_snapshot_id)
                );
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let second_request = paired_arrival_request(&app, &intent, 1, &second);
    let committed = group_handoff::arrive(&app.store, actors[1], &second_request).unwrap();
    assert!(matches!(
        committed,
        coop_cloud::GroupRomHandoffStatus::Committed { .. }
    ));
    let replay = group_handoff::arrive(&app.store, actors[0], &first_request).unwrap();
    assert!(matches!(
        replay,
        coop_cloud::GroupRomHandoffStatus::Committed { .. }
    ));
    let recovered = group_handoff::status(
        &app.store,
        actors[0],
        &coop_cloud::GroupRomHandoffStatusRequest {
            api_version: coop_cloud::ApiVersion::V1,
            group_id: intent.group_id,
            fence: intent.members[0].fence,
            idempotency_key: key,
        },
    )
    .unwrap();
    assert!(matches!(
        recovered,
        coop_cloud::GroupRomHandoffStatus::Committed { .. }
    ));
    app.store
        .read_transaction(|state| {
            assert_eq!(state.groups[&intent.group_id].zone_revision, 1);
            assert!(
                !state
                    .group_rom_handoff_stages
                    .contains_key(&intent.group_id)
            );
            for actor in actors {
                let character = &state.characters[&actor.character_id];
                assert_eq!(character.revision.value(), 2);
                assert_eq!(character.world_revision, 1);
                assert_eq!(
                    character.active_snapshot,
                    character
                        .world_heads
                        .get(&intent.destination_world_id)
                        .copied()
                );
                assert!(state.leases[&actor.character_id].released);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn paired_abort_before_second_arrival_preserves_both_source_heads_and_replays() {
    let (app, actors, intent) = paired_handoff_fixture();
    let first =
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap();
    let second =
        group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    let key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    let arrival = paired_arrival_request(&app, &intent, 1, &second);
    group_handoff::arrive(&app.store, actors[1], &arrival).unwrap();
    let request = coop_cloud::GroupRomHandoffAbortRequest {
        api_version: coop_cloud::ApiVersion::V1,
        group_id: intent.group_id,
        fence: intent.members[0].fence,
        idempotency_key: key,
    };
    assert!(matches!(
        group_handoff::abort(&app.store, actors[0], &request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { .. }
    ));
    assert!(matches!(
        group_handoff::abort(&app.store, actors[0], &request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { .. }
    ));
    assert!(matches!(
        group_handoff::arrive(&app.store, actors[1], &arrival).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { .. }
    ));
    app.store
        .read_transaction(|state| {
            assert_eq!(state.groups[&intent.group_id].zone, intent.source_zone);
            for (index, actor) in actors.iter().enumerate() {
                assert_eq!(
                    state.characters[&actor.character_id].active_snapshot,
                    Some(intent.members[index].source_snapshot_id)
                );
                assert!(!state.leases[&actor.character_id].released);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn paired_abort_preserves_stage_objects_when_terminal_receipt_capacity_is_full() {
    let (app, actors, intent) = paired_handoff_fixture();
    let stage = group_handoff::prepare_pair(&app.store, actors, &intent).unwrap();
    let now = app.store.now();
    app.store
        .write_transaction(|state| {
            let receipt = storage::GroupRomHandoffReceipt::Aborted {
                intent: intent.clone(),
                stage_ids: stage.stage_ids,
                client_intent_keys: None,
                resolved_at: now,
            };
            state.group_rom_handoff_receipt_high_water = now;
            for index in 0..storage::MAX_GROUP_ROM_HANDOFF_RECEIPTS {
                let key = coop_cloud::IdempotencyKey::new(Uuid::from_u128(10_000 + index as u128))
                    .unwrap();
                state
                    .group_rom_handoff_receipts
                    .insert((intent.group_id, key), receipt.clone());
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();

    let request = coop_cloud::GroupRomHandoffAbortRequest {
        api_version: coop_cloud::ApiVersion::V1,
        group_id: intent.group_id,
        fence: intent.members[0].fence,
        idempotency_key: intent.idempotency_key,
    };
    assert_eq!(
        group_handoff::abort(&app.store, actors[0], &request),
        Err(Phase2Error::Busy)
    );
    app.store
        .read_transaction(|state| {
            assert_eq!(state.group_rom_handoff_stages[&intent.group_id], stage);
            for (index, actor) in actors.iter().enumerate() {
                let key = storage::Store::object_key(
                    actor.character_id,
                    stage.stage_ids[index],
                    ArtifactIdentity::CharacterSav,
                );
                assert!(app.store.objects.get(&key).unwrap().is_some());
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn paired_lost_join_response_replays_old_intent_after_source_lease_replacement() {
    let (app, actors, intent) = paired_handoff_fixture();
    let request = paired_join_request(&intent, 0);
    let first = group_handoff::join(&app.store, actors[0], &request).unwrap();
    let attempt_key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    app.store
        .write_transaction(|state| {
            state
                .leases
                .get_mut(&actors[0].character_id)
                .unwrap()
                .released = true;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Pending { idempotency_key, .. }
            if idempotency_key == attempt_key
    ));
    let mut altered = request.clone();
    altered.source_snapshot_id = id(SnapshotId::new);
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &altered),
        Err(Phase2Error::Conflict)
    ));
    let mut fresh = request;
    fresh.client_intent_key = id(IdempotencyKey::new);
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &fresh).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { idempotency_key, .. }
            if idempotency_key == fresh.client_intent_key
    ));
}

#[test]
fn paired_lost_staged_join_response_replays_after_source_lease_replacement() {
    let (app, actors, intent) = paired_handoff_fixture();
    let first_request = paired_join_request(&intent, 0);
    let first = group_handoff::join(&app.store, actors[0], &first_request).unwrap();
    let attempt_key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    app.store
        .write_transaction(|state| {
            state
                .leases
                .get_mut(&actors[0].character_id)
                .unwrap()
                .released = true;
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &first_request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Staged { idempotency_key, .. }
            if idempotency_key == attempt_key
    ));
}

#[test]
fn paired_lost_join_replays_abort_after_companion_leaves_group() {
    let (app, actors, intent) = paired_handoff_fixture();
    let request = paired_join_request(&intent, 0);
    let first = group_handoff::join(&app.store, actors[0], &request).unwrap();
    let attempt_key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    app.store
        .write_transaction(|state| {
            state.groups.get_mut(&intent.group_id).unwrap().status = storage::GroupStatus::Closed;
            for member in intent.members {
                state
                    .active_group_by_member
                    .remove(&member.fence.character_id);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { idempotency_key, .. }
            if idempotency_key == attempt_key
    ));
    let mut fresh = request;
    fresh.client_intent_key = id(IdempotencyKey::new);
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &fresh).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { idempotency_key, .. }
            if idempotency_key == fresh.client_intent_key
    ));
    app.store
        .read_transaction(|state| {
            assert!(
                !state
                    .group_rom_handoff_proposals
                    .contains_key(&intent.group_id)
            );
            assert!(
                !state
                    .group_rom_handoff_stages
                    .contains_key(&intent.group_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn paired_staged_join_replays_abort_after_companion_leaves_group() {
    let (app, actors, intent) = paired_handoff_fixture();
    let request = paired_join_request(&intent, 0);
    let first = group_handoff::join(&app.store, actors[0], &request).unwrap();
    group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    let attempt_key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    app.store
        .write_transaction(|state| {
            state.groups.get_mut(&intent.group_id).unwrap().status = storage::GroupStatus::Closed;
            for member in intent.members {
                state
                    .active_group_by_member
                    .remove(&member.fence.character_id);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &request).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { idempotency_key, .. }
            if idempotency_key == attempt_key
    ));
    app.store
        .read_transaction(|state| {
            assert!(
                !state
                    .group_rom_handoff_proposals
                    .contains_key(&intent.group_id)
            );
            assert!(
                !state
                    .group_rom_handoff_stages
                    .contains_key(&intent.group_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn paired_stage_owns_expiry_and_expired_stage_aborts_without_promoting_sources() {
    let (app, actors, intent) = paired_handoff_fixture();
    let first =
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap();
    assert_eq!(
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap(),
        first
    );
    group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    let key = match first {
        coop_cloud::GroupRomHandoffStatus::Pending {
            idempotency_key, ..
        } => idempotency_key,
        _ => unreachable!(),
    };
    let query = coop_cloud::GroupRomHandoffStatusRequest {
        api_version: coop_cloud::ApiVersion::V1,
        group_id: intent.group_id,
        fence: intent.members[0].fence,
        idempotency_key: key,
    };
    app.store
        .write_transaction(|state| {
            state
                .group_rom_handoff_proposals
                .get_mut(&intent.group_id)
                .unwrap()
                .expires_at = app.store.now();
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::status(&app.store, actors[0], &query).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Staged { .. }
    ));
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Staged { idempotency_key, .. } if idempotency_key == key
    ));
    app.store
        .write_transaction(|state| {
            state
                .group_rom_handoff_stages
                .get_mut(&intent.group_id)
                .unwrap()
                .expires_at = app.store.now();
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        group_handoff::status(&app.store, actors[0], &query).unwrap(),
        coop_cloud::GroupRomHandoffStatus::Aborted { .. }
    ));
    let replay =
        group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap();
    assert!(matches!(
        replay,
        coop_cloud::GroupRomHandoffStatus::Aborted { idempotency_key, .. }
            if idempotency_key == key
    ));
    let mut altered = paired_join_request(&intent, 0);
    altered.source_snapshot_id = id(SnapshotId::new);
    assert!(matches!(
        group_handoff::join(&app.store, actors[0], &altered),
        Err(Phase2Error::Conflict)
    ));
    let mut fresh_request = paired_join_request(&intent, 0);
    fresh_request.client_intent_key = id(IdempotencyKey::new);
    let fresh = group_handoff::join(&app.store, actors[0], &fresh_request).unwrap();
    assert!(
        matches!(fresh, coop_cloud::GroupRomHandoffStatus::Pending { idempotency_key, .. } if idempotency_key != key)
    );
    app.store
        .read_transaction(|state| {
            assert!(
                !state
                    .group_rom_handoff_stages
                    .contains_key(&intent.group_id)
            );
            assert_eq!(state.groups[&intent.group_id].zone, intent.source_zone);
            for (index, actor) in actors.iter().enumerate() {
                assert_eq!(
                    state.characters[&actor.character_id].active_snapshot,
                    Some(intent.members[index].source_snapshot_id)
                );
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

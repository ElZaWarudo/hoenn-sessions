fn paired_handoff_fixture() -> (
    Phase2App,
    [AuthenticatedActor; 2],
    coop_cloud::GroupRomHandoffIntent,
) {
    let (app, actors, intent, _) = paired_handoff_fixture_with_clock();
    (app, actors, intent)
}

fn paired_handoff_fixture_with_clock() -> (
    Phase2App,
    [AuthenticatedActor; 2],
    coop_cloud::GroupRomHandoffIntent,
    Arc<FixedClock>,
) {
    let (app, first, _, _, first_snapshot_id, clock) = handoff_fixture_with_clock();
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
    (app, ordered.0, intent, clock)
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
fn paired_handoff_is_refused_while_either_member_holds_a_battle_reservation() {
    let (app, actors, intent) = paired_handoff_fixture();
    let set_battle = |member: Option<CharacterId>| {
        app.store
            .write_transaction(|state| {
                state.active_battle_by_member.clear();
                if let Some(member) = member {
                    state
                        .active_battle_by_member
                        .insert(member, uuid::Uuid::from_u128(77));
                }
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
    };
    for actor in actors {
        set_battle(Some(actor.character_id));
        assert!(matches!(
            group_handoff::prepare_pair(&app.store, actors, &intent),
            Err(Phase2Error::Conflict)
        ));
        app.store
            .read_transaction(|state| {
                assert!(!state.group_rom_handoff_stages.contains_key(&intent.group_id));
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
    }
    // A reservation taken after staging also blocks arrival, so nothing commits.
    set_battle(None);
    group_handoff::join(&app.store, actors[0], &paired_join_request(&intent, 0)).unwrap();
    let second =
        group_handoff::join(&app.store, actors[1], &paired_join_request(&intent, 1)).unwrap();
    let coop_cloud::GroupRomHandoffStatus::Staged {
        idempotency_key, ..
    } = second
    else {
        panic!("expected staged handoff")
    };
    let first = group_handoff::status(
        &app.store,
        actors[0],
        &coop_cloud::GroupRomHandoffStatusRequest {
            api_version: coop_cloud::ApiVersion::V1,
            group_id: intent.group_id,
            fence: intent.members[0].fence,
            idempotency_key,
        },
    )
    .unwrap();
    set_battle(Some(actors[1].character_id));
    assert!(matches!(
        group_handoff::arrive(
            &app.store,
            actors[0],
            &paired_arrival_request(&app, &intent, 0, &first),
        ),
        Err(Phase2Error::Conflict)
    ));
    app.store
        .read_transaction(|state| {
            for (index, actor) in actors.iter().enumerate() {
                assert_eq!(
                    state.characters[&actor.character_id].revision,
                    intent.members[index].fence.current_revision
                );
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    set_battle(None);
    group_handoff::arrive(
        &app.store,
        actors[0],
        &paired_arrival_request(&app, &intent, 0, &first),
    )
    .unwrap();
    assert!(matches!(
        group_handoff::arrive(
            &app.store,
            actors[1],
            &paired_arrival_request(&app, &intent, 1, &second),
        )
        .unwrap(),
        coop_cloud::GroupRomHandoffStatus::Committed { .. }
    ));
    assert_group_active(&app, intent.group_id, actors);
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

/// Drives one paired ROM handoff through both arrivals to its atomic commit.
fn commit_paired_handoff(
    app: &Phase2App,
    actors: [AuthenticatedActor; 2],
    intent: &coop_cloud::GroupRomHandoffIntent,
) {
    group_handoff::join(&app.store, actors[0], &paired_join_request(intent, 0)).unwrap();
    let second =
        group_handoff::join(&app.store, actors[1], &paired_join_request(intent, 1)).unwrap();
    let coop_cloud::GroupRomHandoffStatus::Staged {
        idempotency_key, ..
    } = second
    else {
        panic!("expected staged handoff")
    };
    let first = group_handoff::status(
        &app.store,
        actors[0],
        &coop_cloud::GroupRomHandoffStatusRequest {
            api_version: coop_cloud::ApiVersion::V1,
            group_id: intent.group_id,
            fence: intent.members[0].fence,
            idempotency_key,
        },
    )
    .unwrap();
    group_handoff::arrive(
        &app.store,
        actors[0],
        &paired_arrival_request(app, intent, 0, &first),
    )
    .unwrap();
    assert!(matches!(
        group_handoff::arrive(
            &app.store,
            actors[1],
            &paired_arrival_request(app, intent, 1, &second),
        )
        .unwrap(),
        coop_cloud::GroupRomHandoffStatus::Committed { .. }
    ));
}

fn assert_group_active(
    app: &Phase2App,
    group_id: coop_cloud::GroupId,
    actors: [AuthenticatedActor; 2],
) {
    app.store
        .read_transaction(|state| {
            assert_eq!(state.groups[&group_id].status, storage::GroupStatus::Active);
            for actor in actors {
                assert_eq!(
                    state.active_group_by_member.get(&actor.character_id),
                    Some(&group_id)
                );
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

fn acquire_destination(
    app: &Phase2App,
    actor: AuthenticatedActor,
    destination: coop_protocol::RomWorldId,
) -> coop_cloud::LeaseContract {
    let response = app
        .acquire_world(
            actor,
            AcquireLeaseRequest::new(
                actor.character_id,
                id(ClientInstanceId::new),
                id(IdempotencyKey::new),
            ),
        )
        .unwrap();
    assert_eq!(response.active_world_id, destination);
    response.lease
}

#[test]
fn paired_handoff_commit_keeps_group_until_destination_reacquire_then_normal_rules_resume() {
    let (app, actors, intent, clock) = paired_handoff_fixture_with_clock();
    // Commit part-way through the source leases' TTL so the handoff window
    // provably outlasts the released source leases' own reconnect grace.
    clock.advance(storage::LEASE_TTL_MS - 10_000);
    commit_paired_handoff(&app, actors, &intent);
    let committed_at = app.store.now();
    let source_grace = app
        .store
        .read_transaction(|state| {
            for actor in actors {
                let lease = &state.leases[&actor.character_id];
                assert!(lease.released);
                let pending = state.rom_handoff_reacquire[&actor.character_id];
                assert_eq!(
                    pending.released_session,
                    lease.contract.stable_runtime_session()
                );
                assert_eq!(
                    pending.reacquire_by,
                    committed_at + storage::ROM_HANDOFF_REACQUIRE_GRACE_MS
                );
            }
            Ok::<_, Phase2Error>(
                actors
                    .map(|actor| state.leases[&actor.character_id].grace_until)
                    .into_iter()
                    .max()
                    .unwrap(),
            )
        })
        .unwrap();
    // Watchdog-cadence sweeps, then one sweep beyond every released source
    // lease's own reconnect grace: travelling members are not leaving.
    for _ in 0..3 {
        clock.advance(5_000);
        assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
        assert_group_active(&app, intent.group_id, actors);
    }
    clock.set(source_grace + 1);
    assert!(app.store.now() <= committed_at + storage::ROM_HANDOFF_REACQUIRE_GRACE_MS);
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    assert_group_active(&app, intent.group_id, actors);

    let leases = actors.map(|actor| acquire_destination(&app, actor, intent.destination_world_id));
    app.store
        .read_transaction(|state| {
            assert!(state.rom_handoff_reacquire.is_empty());
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    assert_group_active(&app, intent.group_id, actors);
    // The lapsed window is irrelevant once the destination lease is held.
    clock.set(committed_at + storage::ROM_HANDOFF_REACQUIRE_GRACE_MS + 1);
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    assert_group_active(&app, intent.group_id, actors);

    // A genuine release on the destination closes the group under the
    // ordinary rule, with the end notice fenced to the surviving partner.
    app.release(
        actors[0],
        ReleaseLeaseRequest::new(leases[0].fence(), id(IdempotencyKey::new)),
    )
    .unwrap();
    let ended = sessions::expire_groups(&app.store).unwrap();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].expired_member, actors[0].character_id);
    assert_eq!(ended[0].partner, actors[1].character_id);
    assert_eq!(
        ended[0].partner_session,
        Some(leases[1].stable_runtime_session())
    );
    assert_eq!(
        sessions::pending_group_end_for_session(&app.store, leases[1].stable_runtime_session())
            .unwrap(),
        Some(intent.group_id)
    );
}

#[test]
fn paired_handoff_commit_closes_group_with_partner_notice_when_member_never_reacquires() {
    let (app, actors, intent, clock) = paired_handoff_fixture_with_clock();
    commit_paired_handoff(&app, actors, &intent);
    let deadline = app.store.now() + storage::ROM_HANDOFF_REACQUIRE_GRACE_MS;
    clock.set(deadline - 10_000);
    let returned = acquire_destination(&app, actors[0], intent.destination_world_id);
    // The deadline itself is still inside the window.
    clock.set(deadline);
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    assert_group_active(&app, intent.group_id, actors);

    clock.set(deadline + 1);
    let ended = sessions::expire_groups(&app.store).unwrap();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].group_id, intent.group_id);
    assert_eq!(ended[0].expired_member, actors[1].character_id);
    assert_eq!(ended[0].partner, actors[0].character_id);
    assert_eq!(
        ended[0].partner_session,
        Some(returned.stable_runtime_session())
    );
    app.store
        .read_transaction(|state| {
            assert_eq!(
                state.groups[&intent.group_id].status,
                storage::GroupStatus::Closed
            );
            for actor in actors {
                assert!(
                    !state
                        .active_group_by_member
                        .contains_key(&actor.character_id)
                );
            }
            assert!(state.rom_handoff_reacquire.is_empty());
            assert_eq!(
                state.group_end_notices[&actors[0].character_id].session,
                returned.stable_runtime_session()
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert_eq!(
        sessions::pending_group_end_for_session(&app.store, returned.stable_runtime_session())
            .unwrap(),
        Some(intent.group_id)
    );
    // A late destination acquire cannot revive the closed group.
    acquire_destination(&app, actors[1], intent.destination_world_id);
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    app.store
        .read_transaction(|state| {
            assert!(
                !state
                    .active_group_by_member
                    .contains_key(&actors[1].character_id)
            );
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn genuine_release_without_handoff_closes_group_on_next_sweep() {
    let (app, actors, intent) = paired_handoff_fixture();
    app.release(
        actors[0],
        ReleaseLeaseRequest::new(intent.members[0].fence, id(IdempotencyKey::new)),
    )
    .unwrap();
    let ended = sessions::expire_groups(&app.store).unwrap();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].expired_member, actors[0].character_id);
    assert_eq!(ended[0].partner, actors[1].character_id);
    let partner_session = app
        .store
        .read_transaction(|state| {
            Ok::<_, Phase2Error>(
                state.leases[&actors[1].character_id]
                    .contract
                    .stable_runtime_session(),
            )
        })
        .unwrap();
    assert_eq!(ended[0].partner_session, Some(partner_session));
    app.store
        .read_transaction(|state| {
            assert_eq!(
                state.groups[&intent.group_id].status,
                storage::GroupStatus::Closed
            );
            assert!(state.rom_handoff_reacquire.is_empty());
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

#[test]
fn grouped_member_solo_handoff_is_refused_and_never_marks_or_ends_the_group() {
    let (app, actors, intent, clock) = paired_handoff_fixture_with_clock();
    let member = &intent.members[0];
    let prepare = RomHandoffPrepareRequest {
        api_version: coop_cloud::ApiVersion::V1,
        character_id: actors[0].character_id,
        session_id: member.fence.session_id,
        session_epoch: member.fence.session_epoch,
        client_instance_id: member.fence.client_instance_id,
        expected_revision: member.fence.current_revision,
        source_snapshot_id: member.source_snapshot_id,
        portal_id: intent.portal_id.clone(),
        idempotency_key: id(IdempotencyKey::new),
    };
    assert!(matches!(
        app.prepare_rom_handoff(actors[0], &prepare),
        Err(Phase2Error::Conflict)
    ));
    // A stage prepared before grouping cannot commit once grouped either.
    let members = actors.map(|actor| actor.character_id);
    app.store
        .write_transaction(|state| {
            for character_id in members {
                state.active_group_by_member.remove(&character_id);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    let staged = app.prepare_rom_handoff(actors[0], &prepare).unwrap();
    app.store
        .write_transaction(|state| {
            for character_id in members {
                state
                    .active_group_by_member
                    .insert(character_id, intent.group_id);
            }
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
    assert!(matches!(
        app.commit_rom_handoff(
            actors[0],
            &RomHandoffCommitRequest {
                api_version: coop_cloud::ApiVersion::V1,
                character_id: actors[0].character_id,
                session_id: member.fence.session_id,
                session_epoch: member.fence.session_epoch,
                client_instance_id: member.fence.client_instance_id,
                expected_revision: member.fence.current_revision,
                stage_id: staged.stage_id,
                destination_save_sha256: staged.destination_save_sha256,
                idempotency_key: prepare.idempotency_key,
            },
        ),
        Err(Phase2Error::Conflict)
    ));
    clock.advance(5_000);
    assert!(sessions::expire_groups(&app.store).unwrap().is_empty());
    assert_group_active(&app, intent.group_id, actors);
    app.store
        .read_transaction(|state| {
            assert!(!state.leases[&actors[0].character_id].released);
            assert!(state.rom_handoff_reacquire.is_empty());
            Ok::<(), Phase2Error>(())
        })
        .unwrap();
}

// States persisted before the post-handoff marker existed must still decode,
// and the marker itself must survive a checkpoint round trip.
#[test]
fn checkpoint_handoff_reacquire_marker_round_trips_and_is_optional() {
    let (app, actors, intent) = paired_handoff_fixture();
    commit_paired_handoff(&app, actors, &intent);
    let state = app
        .store
        .read_transaction(|state| Ok::<_, Phase2Error>(state.clone()))
        .unwrap();
    let mut bytes = Vec::new();
    ciborium::into_writer(&state, &mut bytes).expect("checkpoint");
    let decoded: storage::State = ciborium::from_reader(bytes.as_slice()).expect("decodes");
    assert_eq!(decoded.rom_handoff_reacquire, state.rom_handoff_reacquire);
    assert_eq!(decoded.rom_handoff_reacquire.len(), 2);

    let mut value = ciborium::Value::serialized(&state).expect("state value");
    let ciborium::Value::Map(fields) = &mut value else {
        panic!("state is a map");
    };
    let before = fields.len();
    fields.retain(
        |(key, _)| !matches!(key, ciborium::Value::Text(name) if name == "rom_handoff_reacquire"),
    );
    assert_eq!(fields.len(), before - 1);
    let mut legacy = Vec::new();
    ciborium::into_writer(&value, &mut legacy).expect("legacy checkpoint");
    let decoded: storage::State = ciborium::from_reader(legacy.as_slice()).expect("decodes");
    assert!(decoded.rom_handoff_reacquire.is_empty());
}

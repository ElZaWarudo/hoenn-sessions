// Fresh-start maintenance: a golden legacy checkpoint shaped like the schema
// persisted by origin/main (no `rom_world_id`, no world heads, no ROM handoff
// state) must be rejected by this build and accepted after the transform.
mod fresh_start_checkpoint {
    use super::*;
    use crate::phase2::fresh_start::{
        self, Decision, FreshStartError, FreshStartOptions, decide, hex, parse_args, sha256,
        transform,
    };
    use crate::phase2::persistent::{decode_state, encode};
    use ciborium::Value;

    /// Top-level `State` keys persisted by origin/main's storage.rs.
    const ORIGIN_MAIN_STATE_KEYS: [&str; 49] = [
        "users_by_name",
        "users_by_id",
        "characters",
        "invitations",
        "invitation_issuers",
        "invitation_expires_at",
        "access",
        "refresh",
        "families",
        "leases",
        "acquire_history",
        "prepared",
        "prepare_ops",
        "snapshots",
        "snapshot_by_revision",
        "finalize_ops",
        "restore_ops",
        "restore_staging",
        "retired_snapshots",
        "tickets",
        "realtime_tickets",
        "realtime_ticket_families",
        "realtime_by_runtime",
        "upload_objects",
        "groups",
        "active_group_by_member",
        "group_end_notices",
        "last_group_by_member",
        "last_seen_at",
        "group_invitations",
        "group_pairing_codes",
        "group_pairing_code_issuances",
        "group_pairing_code_attempts",
        "group_idempotency",
        "group_travel_proposals",
        "live_group_travel_by_group",
        "live_group_travel_by_member",
        "group_travel_proposal_idempotency",
        "trade_offers",
        "trade_offer_idempotency",
        "trade_staging",
        "trade_receipts",
        "group_member_world_zones",
        "group_progress_feeds",
        "battle_reservations",
        "active_battle_by_member",
        "battle_idempotency",
        "ledger_entries",
        "ledger_open_by_character",
    ];

    fn text(value: &str) -> Value {
        Value::Text(value.to_owned())
    }

    fn cbor_map(fields: Vec<(&str, Value)>) -> Value {
        Value::Map(
            fields
                .into_iter()
                .map(|(key, value)| (text(key), value))
                .collect(),
        )
    }

    fn strip_key(value: &mut Value, name: &str) {
        match value {
            Value::Map(entries) => {
                entries.retain(|(key, _)| !matches!(key, Value::Text(text) if text == name));
                for (key, value) in entries {
                    strip_key(key, name);
                    strip_key(value, name);
                }
            }
            Value::Array(items) => {
                for item in items {
                    strip_key(item, name);
                }
            }
            Value::Tag(_, inner) => strip_key(inner, name),
            _ => {}
        }
    }

    fn slot<'a>(root: &'a mut [(Value, Value)], name: &str) -> &'a mut Value {
        &mut root
            .iter_mut()
            .find(|(key, _)| matches!(key, Value::Text(text) if text == name))
            .unwrap_or_else(|| panic!("{name} key"))
            .1
    }

    fn cbor(value: &Value) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(value, &mut bytes).expect("encode CBOR");
        bytes
    }

    struct LegacyFixture {
        bytes: Vec<u8>,
        actor: AuthenticatedActor,
        second: CharacterId,
        old_refresh_token: coop_cloud::RefreshToken,
        legacy_snapshot: SnapshotId,
        pending_snapshot: SnapshotId,
        epochs: std::collections::HashMap<CharacterId, u32>,
        password_hashes: std::collections::HashMap<coop_cloud::UserId, String>,
    }

    #[allow(clippy::too_many_lines)]
    fn legacy_fixture() -> LegacyFixture {
        use crate::phase2::storage::{
            GroupRecord, GroupStatus, PreparedSnapshot, UploadObjectRecord,
        };
        let (app, _objects) = deterministic_app_with_store();
        let (actor, lease, client) = account_and_lease(&app);
        let session = app
            .login(LoginRequest::new("fixtureuser", password()).expect("login"))
            .expect("login");
        // A committed SnapshotRecord (snapshots, snapshot_by_revision, finalize_ops).
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        let record = app.finalize(actor, finalize).expect("finalize");
        // restore_ops and a second committed record.
        let restore = SnapshotRestoreRequest::new(
            record.snapshot_id,
            record.session_id,
            actor.character_id,
            record.revision,
            record.session_epoch,
            client,
            id(IdempotencyKey::new),
        );
        let restored = app.restore(actor, &restore).expect("restore");
        // A second account so a group, trade and battle have two members.
        app.add_invitation("second-invite").expect("invite");
        let second = app
            .register(
                RegisterRequest::new(
                    "SecondUser",
                    password(),
                    InvitationCode::new("second-invite").expect("invite"),
                )
                .expect("registration request"),
            )
            .expect("registration")
            .character_id;
        let (pending_request, _, _) = snapshot_request_for_sav(
            lease,
            actor,
            client,
            &valid_character_sav_generation(false, 3),
        );
        let pending_snapshot = pending_request.snapshot_id;
        app.store
            .write_transaction(|state| {
                let group_id = id(coop_cloud::GroupId::new);
                state.groups.insert(
                    group_id,
                    GroupRecord {
                        group: coop_cloud::Group::new(actor.character_id, second).expect("group"),
                        zone: coop_protocol::WorldZone::new(
                            coop_protocol::RegionId::Hoenn,
                            "ROUTE101",
                            1,
                        )
                        .expect("zone"),
                        status: GroupStatus::Active,
                        zone_revision: 0,
                    },
                );
                state
                    .active_group_by_member
                    .insert(actor.character_id, group_id);
                state.active_group_by_member.insert(second, group_id);
                state
                    .last_seen_at
                    .insert(actor.character_id, 1_700_000_000_000);
                state.prepare_ops.insert(
                    (actor.character_id, id(IdempotencyKey::new)),
                    pending_snapshot,
                );
                state.prepared.insert(
                    pending_snapshot,
                    PreparedSnapshot {
                        request: pending_request.clone(),
                        upload_targets: Vec::new(),
                        expires_at: 1_700_000_300_000,
                    },
                );
                state.upload_objects.insert(
                    format!(
                        "characters/{}/snapshots/{pending_snapshot}/character.sav",
                        actor.character_id
                    ),
                    UploadObjectRecord {
                        fingerprint: [1; 32],
                        cleanup_claimed: false,
                    },
                );
                Ok::<(), StorageError>(())
            })
            .expect("fixture metadata");
        let (epochs, password_hashes, mut value) = app
            .store
            .inspect_state(|state| {
                assert!(state.snapshots.len() >= 2);
                assert_eq!(state.finalize_ops.len(), 1);
                assert_eq!(state.restore_ops.len(), 1);
                (
                    state
                        .characters
                        .iter()
                        .map(|(id, record)| (*id, record.last_session_epoch))
                        .collect(),
                    state
                        .users_by_id
                        .iter()
                        .map(|(id, user)| (*id, user.password_phc.clone()))
                        .collect(),
                    Value::serialized(state).expect("state value"),
                )
            })
            .expect("state");
        // Rewrite the current encoding into origin/main's persisted shape.
        strip_key(&mut value, "rom_world_id");
        let Value::Map(root) = &mut value else {
            panic!("state map")
        };
        root.retain(
            |(key, _)| matches!(key, Value::Text(text) if ORIGIN_MAIN_STATE_KEYS.contains(&text.as_str())),
        );
        assert_eq!(root.len(), ORIGIN_MAIN_STATE_KEYS.len());
        if let Value::Map(characters) = slot(root, "characters") {
            for (_, character) in characters {
                strip_key(character, "world_heads");
            }
        }
        if let Value::Map(leases) = slot(root, "leases") {
            for (_, lease) in leases {
                strip_key(lease, "runtime_binding");
            }
        }
        // Hand-built records using origin/main field names for the feature
        // maps that a fresh start must discard wholesale.
        let actor_key = text(&actor.character_id.to_string());
        let second_key = text(&second.to_string());
        let commit = text(&Uuid::new_v4().to_string());
        let battle = text(&Uuid::new_v4().to_string());
        let offer = text(&Uuid::new_v4().to_string());
        *slot(root, "ledger_entries") = Value::Map(vec![(
            commit.clone(),
            cbor_map(vec![
                ("commit_id", commit.clone()),
                ("character_id", actor_key.clone()),
                (
                    "origin",
                    cbor_map(vec![(
                        "Battle",
                        cbor_map(vec![("battle_id", battle.clone())]),
                    )]),
                ),
                ("base_snapshot_id", text(&record.snapshot_id.to_string())),
                ("base_revision", Value::Integer(1.into())),
                (
                    "expected",
                    cbor_map(vec![("money_delta", Value::Integer(100.into()))]),
                ),
                ("status", text("Issued")),
                ("issued_at", Value::Integer(1_700_000_000_000_u64.into())),
            ]),
        )]);
        *slot(root, "ledger_open_by_character") = Value::Map(vec![(actor_key.clone(), commit)]);
        *slot(root, "battle_reservations") = Value::Map(vec![(
            battle.clone(),
            cbor_map(vec![
                ("view", cbor_map(vec![("reservation_id", battle.clone())])),
                ("expires_at", Value::Integer(1_700_000_030_000_u64.into())),
                ("retain_until", Value::Integer(1_700_000_060_000_u64.into())),
            ]),
        )]);
        *slot(root, "active_battle_by_member") = Value::Map(vec![
            (actor_key.clone(), battle.clone()),
            (second_key.clone(), battle),
        ]);
        *slot(root, "trade_offers") = Value::Map(vec![(
            offer.clone(),
            cbor_map(vec![
                ("view", cbor_map(vec![("offer_id", offer)])),
                ("fences", Value::Array(Vec::new())),
                (
                    "consents",
                    Value::Array(vec![Value::Bool(true), Value::Bool(false)]),
                ),
                ("expires_at", Value::Integer(1_700_000_030_000_u64.into())),
            ]),
        )]);
        LegacyFixture {
            bytes: cbor(&value),
            actor,
            second,
            old_refresh_token: session.refresh_token,
            legacy_snapshot: restored.snapshot.snapshot_id,
            pending_snapshot,
            epochs,
            password_hashes,
        }
    }

    fn fresh_app() -> Phase2App {
        // Same secrets as the fixture app, different deterministic entropy so
        // newly minted tokens cannot collide with preserved fingerprints.
        Phase2App::new(
            Phase2Config::local(
                vec![0x55; 32],
                SigningPrivateKey::from_bytes([7; 32]),
                "local-test-key",
            )
            .expect("test config")
            .with_legacy_test_runtime()
            .with_test_adapters(
                Arc::new(FixedClock::new(1_700_000_000_000)),
                Arc::new(FixedEntropy::new((3_u8..=253).rev().collect())),
            )
            .with_password_engine(Arc::new(
                ArgonPasswordEngine::new(8_192, 1, 1).expect("test Argon2 policy"),
            ))
            .with_adapters(
                Arc::new(InMemoryRepository::new()),
                Arc::new(InMemoryObjectStore::new()),
            ),
        )
        .expect("fresh app")
    }

    fn load(app: &Phase2App, state: storage::State) {
        let mut state = Some(state);
        app.store
            .write_transaction(|current| {
                *current = state.take().expect("load once");
                Ok::<(), StorageError>(())
            })
            .expect("load state");
    }

    #[test]
    fn legacy_checkpoint_is_rejected_by_this_build() {
        let fixture = legacy_fixture();
        assert!(decode_state(&fixture.bytes).is_err());
        let error = ciborium::from_reader::<storage::State, _>(fixture.bytes.as_slice())
            .err()
            .expect("legacy decode fails");
        assert!(
            error.to_string().contains("rom_world_id"),
            "the P0 is the missing rom_world_id, got: {error}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn transform_resets_characters_and_keeps_accounts() {
        let fixture = legacy_fixture();
        let Value::Map(legacy_root) =
            ciborium::from_reader::<Value, _>(fixture.bytes.as_slice()).unwrap()
        else {
            panic!("map")
        };
        let outcome = transform(&fixture.bytes, false).expect("transform");
        let state = decode_state(&outcome.payload).expect("transformed payload decodes");
        let Value::Map(new_root) =
            ciborium::from_reader::<Value, _>(outcome.payload.as_slice()).unwrap()
        else {
            panic!("map")
        };
        // Users and invitations are byte-identical, entry by entry.
        for key in [
            "users_by_name",
            "users_by_id",
            "invitations",
            "access",
            "refresh",
            "families",
        ] {
            let lookup = |root: &[(Value, Value)]| {
                root.iter()
                    .find(|(name, _)| matches!(name, Value::Text(text) if text == key))
                    .map(|(_, value)| value.clone())
                    .expect("key")
            };
            let (Value::Map(old), Value::Map(new)) = (lookup(&legacy_root), lookup(&new_root))
            else {
                panic!("{key} map");
            };
            assert_eq!(old.len(), new.len(), "{key}");
            assert!(!old.is_empty(), "{key} fixture is populated");
            for (entry_key, entry_value) in &old {
                let (_, kept) = new
                    .iter()
                    .find(|(candidate, _)| cbor(candidate) == cbor(entry_key))
                    .expect("entry kept");
                assert_eq!(
                    cbor(kept),
                    cbor(entry_value),
                    "{key} entry is byte-identical"
                );
            }
        }
        assert_eq!(state.users_by_id.len(), 2);
        for (user_id, hash) in &fixture.password_hashes {
            assert_eq!(&state.users_by_id[user_id].password_phc, hash);
        }
        assert_eq!(state.characters.len(), 2);
        for (character_id, character) in &state.characters {
            assert_eq!(character.revision, Revision::initial());
            assert_eq!(character.world_revision, 0);
            assert_eq!(character.active_snapshot, None);
            assert!(character.world_heads.is_empty());
            assert_eq!(character.last_session_epoch, fixture.epochs[character_id]);
            assert_eq!(
                character.state,
                storage::Store::initial_state(*character_id).expect("initial state")
            );
        }
        assert!(fixture.epochs[&fixture.actor.character_id] > 0);
        assert!(state.characters.contains_key(&fixture.second));
        assert!(state.snapshots.is_empty());
        assert!(state.snapshot_by_revision.is_empty());
        assert!(state.prepared.is_empty());
        assert!(state.prepare_ops.is_empty());
        assert!(state.finalize_ops.is_empty());
        assert!(state.restore_ops.is_empty());
        assert!(state.restore_staging.is_empty());
        assert!(state.leases.is_empty());
        assert!(state.acquire_history.is_empty());
        assert!(state.tickets.is_empty());
        assert!(state.realtime_tickets.is_empty());
        assert!(state.groups.is_empty());
        assert!(state.active_group_by_member.is_empty());
        assert!(state.last_seen_at.is_empty());
        assert!(state.ledger_entries.is_empty());
        assert!(state.ledger_open_by_character.is_empty());
        assert!(state.trade_offers.is_empty());
        assert!(state.battle_reservations.is_empty());
        assert!(state.active_battle_by_member.is_empty());
        assert!(state.retiring_snapshots.is_empty());
        assert!(state.upload_objects.is_empty());
        assert!(state.retired_snapshots.contains(&fixture.legacy_snapshot));
        assert!(state.retired_snapshots.contains(&fixture.pending_snapshot));

        let report = &outcome.report;
        let before = report.before.as_ref().expect("before");
        let after = report.after.as_ref().expect("after");
        assert_eq!((before.users, after.users), (2, 2));
        assert_eq!((before.characters, after.characters), (2, 2));
        assert!(before.snapshots >= 2 && after.snapshots == 0);
        assert_eq!((before.ledger_entries, after.ledger_entries), (1, 0));
        assert_eq!((before.groups, after.groups), (1, 0));
        assert_eq!((before.trade_offers, after.trade_offers), (1, 0));
        assert_eq!(
            (before.battle_reservations, after.battle_reservations),
            (1, 0)
        );
        assert_eq!(report.characters_at_revision_zero, 2);
        assert_eq!(report.retired_snapshots_overflow, 0);
        assert_eq!(report.unparsable_snapshot_ids, 0);
        assert_eq!(
            before.retired_snapshots + report.retired_snapshots_added,
            after.retired_snapshots
        );
        assert!(report.retired_snapshots_added >= 3);
        assert!(report.dropped_keys.contains_key("snapshots"));
        assert!(report.dropped_keys.contains_key("ledger_entries"));
        assert!(!report.dropped_keys.contains_key("users_by_id"));
        assert_eq!(
            report.archived_payload_sha256.as_deref(),
            Some(hex(&sha256(&fixture.bytes)).as_str())
        );
        assert_eq!(
            report.new_payload_sha256.as_deref(),
            Some(hex(&sha256(&outcome.payload)).as_str())
        );

        // Dropping sessions is explicit and only affects the token maps.
        let dropped = transform(&fixture.bytes, true).expect("drop sessions");
        let dropped = decode_state(&dropped.payload).expect("decodes");
        assert!(dropped.access.is_empty() && dropped.refresh.is_empty());
        assert!(dropped.families.is_empty());
        assert_eq!(dropped.users_by_id.len(), 2);
    }

    #[test]
    fn decision_gates_digest_format_and_idempotency() {
        let fixture = legacy_fixture();
        let legacy_sha = sha256(&fixture.bytes);
        assert_eq!(
            decide(1, &fixture.bytes, &legacy_sha, false),
            Ok(Decision::Transform)
        );
        assert_eq!(
            decide(1, &fixture.bytes, &[0; 32], false),
            Err(FreshStartError::ShaMismatch {
                actual: hex(&legacy_sha)
            })
        );
        assert_eq!(
            decide(2, &fixture.bytes, &legacy_sha, false),
            Err(FreshStartError::UnsupportedFormat(2))
        );
        assert_eq!(
            decide(1, &fixture.bytes, &legacy_sha, true),
            Err(FreshStartError::ArchivedButNotFresh)
        );
        let oversized = vec![0_u8; super::super::persistent::MAX_STATE_BYTES + 1];
        assert_eq!(
            decide(1, &oversized, &sha256(&oversized), false),
            Err(FreshStartError::TooLarge)
        );
        let fresh = transform(&fixture.bytes, false).expect("transform").payload;
        // Re-running with the original digest after a committed run.
        assert!(matches!(
            decide(1, &fresh, &legacy_sha, true),
            Ok(Decision::AlreadyFresh(_))
        ));
        // Re-running against the new payload itself is a no-op too.
        assert!(matches!(
            decide(1, &fresh, &sha256(&fresh), false),
            Ok(Decision::AlreadyFresh(_))
        ));
        // A transform of an already-fresh payload changes nothing material.
        let again = transform(&fresh, false).expect("second transform");
        let (first, second) = (
            decode_state(&fresh).unwrap(),
            decode_state(&again.payload).unwrap(),
        );
        assert_eq!(
            encode(&first).unwrap().len(),
            encode(&second).unwrap().len()
        );
        assert_eq!(again.report.retired_snapshots_added, 0);
        assert_eq!(again.report.before, again.report.after);
        // An empty default checkpoint written by this build is already fresh.
        let empty = encode(&storage::State::default()).unwrap();
        assert!(matches!(
            decide(1, &empty, &sha256(&empty), false),
            Ok(Decision::AlreadyFresh(_))
        ));
    }

    #[test]
    fn played_current_format_characters_are_not_legacy() {
        let (app, _) = deterministic_app_with_store();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let finalize = SnapshotFinalizeRequest::new(
            prepared.snapshot_id,
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                lease.current_revision,
                lease.session_epoch,
                client,
                id(IdempotencyKey::new),
            ),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
        )
        .expect("finalize request");
        app.finalize(actor, finalize).expect("finalize");
        let bytes = app
            .store
            .inspect_state(|state| {
                assert_eq!(fresh_start::legacy_character_count(state), 0);
                encode(state).unwrap()
            })
            .unwrap();
        // A checkpoint that already works is never wiped, even with its digest.
        assert!(matches!(
            decide(1, &bytes, &sha256(&bytes), false),
            Ok(Decision::AlreadyFresh(_))
        ));
        let mut state = decode_state(&bytes).unwrap();
        state
            .characters
            .get_mut(&actor.character_id)
            .unwrap()
            .world_heads
            .clear();
        assert_eq!(fresh_start::legacy_character_count(&state), 1);
    }

    #[test]
    fn transformed_checkpoint_serves_login_and_a_new_campaign() {
        let fixture = legacy_fixture();
        let outcome = transform(&fixture.bytes, false).expect("transform");
        let app = fresh_app();
        load(&app, decode_state(&outcome.payload).unwrap());
        let login = app
            .login(LoginRequest::new("fixtureuser", password()).expect("login"))
            .expect("old password still logs in");
        assert_eq!(login.user_id, fixture.actor.user_id);
        app.refresh(RefreshRequest::new(fixture.old_refresh_token.clone()))
            .expect("kept refresh token still rotates");
        let actor = fixture.actor;
        let client = id(ClientInstanceId::new);
        let response = app
            .acquire_world(
                actor,
                AcquireLeaseRequest::new(actor.character_id, client, id(IdempotencyKey::new)),
            )
            .expect("acquire world");
        assert_eq!(response.lease.current_revision, Revision::initial());
        assert_eq!(
            response.active_world_id,
            coop_protocol::RomWorldId::new(1).unwrap()
        );
        assert_eq!(response.active_snapshot_id, None);
        assert!(
            u64::from(response.lease.session_epoch.value())
                > u64::from(fixture.epochs[&actor.character_id]),
            "session epochs stay monotonic across the fresh start"
        );
        let lease = response.lease;
        // A legacy snapshot ID is tombstoned and cannot be prepared again.
        let (mut replay, _, _) = snapshot_request_for_sav(
            lease,
            actor,
            client,
            &valid_character_sav_generation(false, 1),
        );
        replay.snapshot_id = fixture.legacy_snapshot;
        assert_eq!(app.prepare(actor, replay), Err(Phase2Error::Conflict));
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        upload_prepared(&app, &prepared);
        let record = app
            .finalize(
                actor,
                SnapshotFinalizeRequest::new(
                    prepared.snapshot_id,
                    SnapshotFinalizeFence::new(
                        lease.session_id,
                        actor.character_id,
                        lease.current_revision,
                        lease.session_epoch,
                        client,
                        id(IdempotencyKey::new),
                    ),
                    vec![sav, pending.clone()],
                    pending.sha256,
                    None,
                )
                .expect("finalize request"),
            )
            .expect("first finalize after fresh start");
        let world = coop_protocol::RomWorldId::new(1).unwrap();
        app.store
            .inspect_state(|state| {
                let character = &state.characters[&actor.character_id];
                assert_eq!(character.world_heads.get(&world), Some(&record.snapshot_id));
                assert_eq!(character.active_snapshot, Some(record.snapshot_id));
                assert_eq!(fresh_start::legacy_character_count(state), 0);
            })
            .unwrap();

        // With --drop-sessions the password still works but old refresh does not.
        let dropped = transform(&fixture.bytes, true).expect("drop sessions");
        let app = fresh_app();
        load(&app, decode_state(&dropped.payload).unwrap());
        assert_eq!(
            app.refresh(RefreshRequest::new(fixture.old_refresh_token)),
            Err(Phase2Error::Authentication)
        );
        app.login(LoginRequest::new("fixtureuser", password()).expect("login"))
            .expect("password survives dropped sessions");
    }

    #[test]
    fn arguments_and_sql_are_explicit() {
        let digest = "Ab".repeat(32);
        let parsed = parse_args(&[
            "--expect-sha256".to_owned(),
            digest.clone(),
            "--dry-run".to_owned(),
        ])
        .expect("arguments");
        assert_eq!(
            parsed,
            FreshStartOptions {
                expected_sha256: [0xab; 32],
                drop_sessions: false,
                dry_run: true,
            }
        );
        assert!(
            parse_args(&[
                "--expect-sha256".to_owned(),
                digest.clone(),
                "--drop-sessions".to_owned()
            ])
            .unwrap()
            .drop_sessions
        );
        for invalid in [
            vec![],
            vec!["--drop-sessions".to_owned()],
            vec!["--expect-sha256".to_owned()],
            vec!["--expect-sha256".to_owned(), "abc".to_owned()],
            vec!["--expect-sha256".to_owned(), "zz".repeat(32)],
            vec![
                "--expect-sha256".to_owned(),
                digest.clone(),
                "--expect-sha256".to_owned(),
                digest.clone(),
            ],
            vec![
                "--expect-sha256".to_owned(),
                digest.clone(),
                "--force".to_owned(),
            ],
        ] {
            assert!(parse_args(&invalid).is_err(), "{invalid:?}");
        }
        assert_eq!(
            super::super::persistent::ADVISORY_LOCK_SQL,
            "SELECT pg_try_advisory_lock(1129271120, 1)"
        );
        assert!(fresh_start::UNLOCK_SQL.contains("(1129271120, 1)"));
        assert!(fresh_start::SELECT_FOR_UPDATE_SQL.ends_with("WHERE id = 1 FOR UPDATE"));
        assert!(fresh_start::UPDATE_SQL.ends_with("WHERE id = 1 AND format_version = 1"));
        assert!(fresh_start::ARCHIVE_INSERT_SQL.contains("payload_sha256"));
        for clause in [
            "CREATE TABLE IF NOT EXISTS coop_pilot_checkpoint_archive",
            "id bigserial PRIMARY KEY",
            "archived_at timestamptz NOT NULL DEFAULT now()",
            "reason text NOT NULL",
            "format_version integer NOT NULL",
            "CHECK (octet_length(payload) <= 33554432)",
            "payload_sha256 bytea NOT NULL UNIQUE",
        ] {
            assert!(fresh_start::MIGRATION_SQL.contains(clause), "{clause}");
        }
        assert!(
            super::super::persistent::DECODE_FAILURE_GUIDANCE.contains("coop-server fresh-start")
        );
    }

    // Runs only against a disposable database server; never production.
    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    #[allow(clippy::too_many_lines)]
    fn postgres_fresh_start_is_gated_archived_and_idempotent() {
        use crate::phase2::persistent::PostgresStateRepository;
        let base = std::env::var("COOP_TEST_DATABASE_URL").expect("COOP_TEST_DATABASE_URL");
        let mut admin = postgres::Client::connect(&base, postgres::NoTls).expect("admin");
        let name = format!("coop_fresh_{}", Uuid::new_v4().simple());
        admin
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .expect("create database");
        let mut url = url::Url::parse(&base).expect("url");
        url.set_path(&name);
        let url: String = url.into();
        let outcome = std::panic::catch_unwind(|| {
            let fixture = legacy_fixture();
            let legacy_sha = sha256(&fixture.bytes);
            let mut db = postgres::Client::connect(&url, postgres::NoTls).expect("db");
            let options = |expected_sha256, dry_run| FreshStartOptions {
                expected_sha256,
                drop_sessions: false,
                dry_run,
            };
            assert_eq!(
                fresh_start::run(&url, &options(legacy_sha, false)),
                Err(FreshStartError::NoCheckpoint)
            );
            db.batch_execute(include_str!("../../migrations/0003_pilot_checkpoint.sql"))
                .unwrap();
            db.execute(
                "INSERT INTO coop_pilot_checkpoint (id, format_version, payload) VALUES (1, 1, $1)",
                &[&fixture.bytes],
            )
            .unwrap();
            // The current server refuses the legacy checkpoint and stays closed.
            assert!(PostgresStateRepository::connect(&url).is_err());
            let stored = |db: &mut postgres::Client| -> Vec<u8> {
                db.query_one(
                    "SELECT payload FROM coop_pilot_checkpoint WHERE id = 1",
                    &[],
                )
                .unwrap()
                .get(0)
            };
            let archive_exists = |db: &mut postgres::Client| -> bool {
                db.query_one(
                    "SELECT to_regclass('coop_pilot_checkpoint_archive') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get(0)
            };
            // Wrong digest: nothing written, not even the archive table.
            assert!(matches!(
                fresh_start::run(&url, &options([0; 32], false)),
                Err(FreshStartError::ShaMismatch { .. })
            ));
            assert_eq!(stored(&mut db), fixture.bytes);
            assert!(!archive_exists(&mut db));
            // A running server (advisory lock holder) blocks the command.
            let held: bool = db
                .query_one("SELECT pg_try_advisory_lock(1129271120, 1)", &[])
                .unwrap()
                .get(0);
            assert!(held);
            assert_eq!(
                fresh_start::run(&url, &options(legacy_sha, false)),
                Err(FreshStartError::LockHeld)
            );
            db.query_one("SELECT pg_advisory_unlock(1129271120, 1)", &[])
                .unwrap();
            // Dry run reports without writing.
            let dry = fresh_start::run(&url, &options(legacy_sha, true)).expect("dry run");
            assert_eq!(dry.outcome, "dry_run");
            assert_eq!(stored(&mut db), fixture.bytes);
            assert!(!archive_exists(&mut db));
            // Real run archives the original and replaces the checkpoint.
            let report = fresh_start::run(&url, &options(legacy_sha, false)).expect("run");
            assert_eq!(report.outcome, "fresh_started");
            let new_payload = stored(&mut db);
            assert_eq!(
                report.new_payload_sha256.as_deref(),
                Some(hex(&sha256(&new_payload)).as_str())
            );
            let archive = db
                .query(
                    "SELECT reason, format_version, payload, payload_sha256 FROM coop_pilot_checkpoint_archive",
                    &[],
                )
                .unwrap();
            assert_eq!(archive.len(), 1);
            assert_eq!(archive[0].get::<_, String>(0), fresh_start::ARCHIVE_REASON);
            assert_eq!(archive[0].get::<_, i32>(1), 1);
            assert_eq!(archive[0].get::<_, Vec<u8>>(2), fixture.bytes);
            assert_eq!(archive[0].get::<_, Vec<u8>>(3), legacy_sha.to_vec());
            // Idempotent re-run with the original digest.
            let again = fresh_start::run(&url, &options(legacy_sha, false)).expect("re-run");
            assert_eq!(again.outcome, "already_fresh");
            assert_eq!(stored(&mut db), new_payload);
            // The server now starts and serves the preserved accounts.
            let repository = PostgresStateRepository::connect(&url).expect("server starts");
            repository
                .read_transaction(&mut |state| {
                    assert_eq!(state.users_by_id.len(), 2);
                    assert_eq!(fresh_start::legacy_character_count(state), 0);
                    Ok(())
                })
                .unwrap();
            drop(repository);
        });
        drop(admin.batch_execute(&format!("DROP DATABASE {name} WITH (FORCE)")));
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}

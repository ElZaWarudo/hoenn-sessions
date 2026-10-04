// Golden checkpoint written by an actual origin/main build (17153772bc);
// see tests/fixtures/README.md for how it was produced.
const GOLDEN: &[u8] = include_bytes!("../../tests/fixtures/origin-main-17153772bc-checkpoint.cbor");
const GOLDEN_SHA256: &str = "0a0d99a693f06e17d553b6dd753d87c5f47dfaaa1e13c46bee28b0de5ccdef33";
/// Accounts in the golden checkpoint: (login name, password).
const GOLDEN_ACCOUNTS: [(&str, &str); 3] = [
    ("battlealice", "password-a"),
    ("battlebob", "password-b"),
    ("fixtureuser", "correct horse battery staple"),
];
/// Top-level maps a fresh start keeps entry-for-entry (sessions included
/// unless `--drop-sessions`).
const GOLDEN_KEPT: [&str; 8] = [
    "users_by_name",
    "users_by_id",
    "invitations",
    "invitation_issuers",
    "invitation_expires_at",
    "access",
    "refresh",
    "families",
];

fn root_of(bytes: &[u8]) -> Vec<(Value, Value)> {
    let Value::Map(root) = ciborium::from_reader::<Value, _>(bytes).expect("CBOR") else {
        panic!("root map")
    };
    root
}

fn lookup<'a>(root: &'a [(Value, Value)], key: &str) -> &'a Value {
    &root
        .iter()
        .find(|(name, _)| matches!(name, Value::Text(text) if text == key))
        .unwrap_or_else(|| panic!("{key}"))
        .1
}

fn mutate_golden(change: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
    let mut root = root_of(GOLDEN);
    change(&mut root);
    cbor(&Value::Map(root))
}

fn snapshot_key(index: u128) -> Value {
    Value::serialized(
        &SnapshotId::new(Uuid::from_u128(
            0x7e57_0000_0000_4000_8000_0000_0000_0000 + index,
        ))
        .expect("snapshot id"),
    )
    .expect("snapshot id value")
}

fn login_all(app: &Phase2App) {
    for (name, secret) in GOLDEN_ACCOUNTS {
        app.login(
            LoginRequest::new(name, Password::new(secret).expect("password"))
                .expect("login request"),
        )
        .unwrap_or_else(|error| panic!("{name} logs in: {error:?}"));
    }
}

#[test]
fn golden_origin_main_checkpoint_is_refused_by_this_build() {
    assert_eq!(hex(&sha256(GOLDEN)), GOLDEN_SHA256, "fixture bytes changed");
    assert!(decode_state(GOLDEN).is_err());
    let error = ciborium::from_reader::<storage::State, _>(GOLDEN)
        .err()
        .expect("typed decode fails");
    assert!(error.to_string().contains("rom_world_id"), "{error}");
    let root = fresh_start::parse_origin_main_root(GOLDEN).expect("origin/main schema");
    assert_eq!(root.len(), ORIGIN_MAIN_STATE_KEYS.len());
    assert_eq!(
        decide(1, GOLDEN, &sha256(GOLDEN), false),
        Ok(Decision::Transform)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn golden_origin_main_checkpoint_fresh_starts() {
    let legacy = root_of(GOLDEN);
    let outcome = transform(GOLDEN, false, false).expect("transform");
    let state = decode_state(&outcome.payload).expect("fresh payload decodes");
    let fresh = root_of(&outcome.payload);
    for key in GOLDEN_KEPT {
        let (Value::Map(old), Value::Map(new)) = (lookup(&legacy, key), lookup(&fresh, key)) else {
            panic!("{key} map");
        };
        assert!(!old.is_empty(), "{key} is populated in the golden");
        assert_eq!(old.len(), new.len(), "{key}");
        for (entry_key, entry_value) in old {
            let (_, kept) = new
                .iter()
                .find(|(candidate, _)| cbor(candidate) == cbor(entry_key))
                .unwrap_or_else(|| panic!("{key} entry kept"));
            assert_eq!(
                cbor(kept),
                cbor(entry_value),
                "{key} entry is byte-identical"
            );
        }
    }
    // Characters keep owner and session epoch only.
    let Value::Map(legacy_characters) = lookup(&legacy, "characters") else {
        panic!("characters")
    };
    assert_eq!(state.characters.len(), legacy_characters.len());
    assert_eq!(state.characters.len(), 3);
    for (key, record) in legacy_characters {
        let character_id: CharacterId = key.deserialized().expect("character id");
        let character = &state.characters[&character_id];
        let owner: coop_cloud::UserId = field_of(record, "owner");
        let epoch: u32 = field_of(record, "last_session_epoch");
        assert_eq!(character.owner, owner);
        assert_eq!(character.last_session_epoch, epoch);
        assert_eq!(character.revision, Revision::initial());
        assert_eq!(character.world_revision, 0);
        assert_eq!(character.active_snapshot, None);
        assert!(character.world_heads.is_empty());
        assert_eq!(
            character.state,
            storage::Store::initial_state(character_id).expect("initial state")
        );
    }
    assert_eq!(fresh_start::legacy_character_count(&state), 0);
    assert!(state.snapshots.is_empty() && state.prepared.is_empty());
    assert!(state.finalize_ops.is_empty() && state.restore_ops.is_empty());
    assert!(state.leases.is_empty() && state.tickets.is_empty());
    assert!(state.groups.is_empty() && state.group_invitations.is_empty());
    assert!(state.battle_reservations.is_empty() && state.active_battle_by_member.is_empty());
    assert!(state.ledger_entries.is_empty() && state.ledger_open_by_character.is_empty());
    // Every in-flight and head snapshot ID is tombstoned.
    let Value::Map(prepared) = lookup(&legacy, "prepared") else {
        panic!("prepared")
    };
    assert_eq!(prepared.len(), 1);
    for (id, _) in prepared {
        assert!(
            state
                .retired_snapshots
                .contains(&id.deserialized().unwrap())
        );
    }
    for (_, record) in legacy_characters {
        let head: Option<SnapshotId> = field_of(record, "active_snapshot");
        assert!(
            state
                .retired_snapshots
                .contains(&head.expect("played head"))
        );
    }
    let Value::Map(snapshots) = lookup(&legacy, "snapshots") else {
        panic!("snapshots")
    };
    for (id, _) in snapshots {
        assert!(
            state
                .retired_snapshots
                .contains(&id.deserialized().unwrap())
        );
    }
    let report = &outcome.report;
    let before = report.before.as_ref().expect("before");
    let after = report.after.as_ref().expect("after");
    assert_eq!((before.users, after.users), (3, 3));
    assert_eq!((before.characters, after.characters), (3, 3));
    assert_eq!((before.snapshots, after.snapshots), (4, 0));
    assert_eq!(
        (before.prepared_snapshots, after.prepared_snapshots),
        (1, 0)
    );
    assert_eq!((before.ledger_entries, after.ledger_entries), (2, 0));
    assert_eq!((before.groups, after.groups), (1, 0));
    assert_eq!(
        (before.battle_reservations, after.battle_reservations),
        (1, 0)
    );
    assert_eq!((before.access_tokens, after.access_tokens), (3, 3));
    assert_eq!(report.characters_without_owner, 0);
    assert_eq!(report.retired_snapshots_overflow, 0);
    assert_eq!(report.unparsable_snapshot_ids, 0);
    assert_eq!(report.retired_snapshots_in_flight_and_heads, 4);
    assert_eq!(report.retired_snapshots_final, 5);
    assert_eq!(report.retired_snapshots_bound, TOMBSTONE_HEADROOM_BOUND);
    assert_eq!(
        report.retired_snapshots_headroom,
        storage::MAX_RETIRED_SNAPSHOTS - 5
    );
    assert!(report.dropped_keys.contains_key("battle_reservations"));
    assert!(!report.dropped_keys.contains_key("invitation_issuers"));
    // Every golden account logs in with its original password, and a new
    // campaign starts in world 1 at revision 0.
    let app = fresh_app();
    load(&app, state);
    login_all(&app);
    let login = app
        .login(LoginRequest::new("battlealice", Password::new("password-a").unwrap()).unwrap())
        .unwrap();
    let character_id = app
        .store
        .inspect_state(|state| state.users_by_id[&login.user_id].character_id)
        .unwrap();
    let actor = AuthenticatedActor {
        user_id: login.user_id,
        character_id,
    };
    let response = app
        .acquire_world(
            actor,
            AcquireLeaseRequest::new(
                character_id,
                id(ClientInstanceId::new),
                id(IdempotencyKey::new),
            ),
        )
        .expect("acquire world");
    assert_eq!(response.lease.current_revision, Revision::initial());
    assert_eq!(response.active_snapshot_id, None);
    // Sessions dropped on request; accounts and invitations stay.
    let dropped = transform(GOLDEN, true, false).expect("drop sessions");
    let dropped = decode_state(&dropped.payload).unwrap();
    assert!(dropped.access.is_empty() && dropped.refresh.is_empty());
    assert!(dropped.families.is_empty());
    assert_eq!(dropped.users_by_id.len(), 3);
    assert_eq!(dropped.invitation_issuers.len(), 1);
    assert_eq!(dropped.invitation_expires_at.len(), 1);
}

fn field_of<T: serde::de::DeserializeOwned>(record: &Value, name: &str) -> T {
    let Value::Map(entries) = record else {
        panic!("record map")
    };
    entries
        .iter()
        .find(|(key, _)| matches!(key, Value::Text(text) if text == name))
        .unwrap_or_else(|| panic!("{name}"))
        .1
        .deserialized()
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn assert_not_origin_main(payload: &[u8], needle: &str) {
    for result in [
        decide(1, payload, &sha256(payload), false).map(|_| ()),
        transform(payload, false, false).map(|_| ()),
    ] {
        match result {
            Err(FreshStartError::NotOriginMainCheckpoint(message)) => {
                assert!(message.contains(needle), "{message} lacks {needle}");
            }
            other => panic!("expected a NotOriginMainCheckpoint refusal, got {other:?}"),
        }
    }
}

#[test]
fn only_positively_matched_origin_main_checkpoints_are_transformed() {
    // A key origin/main never persisted (here: a multi-world key).
    assert_not_origin_main(
        &mutate_golden(|root| {
            root.push((text("rom_handoff_staging"), Value::Map(Vec::new())));
        }),
        "unknown to origin/main: rom_handoff_staging",
    );
    // A future build's key.
    assert_not_origin_main(
        &mutate_golden(|root| root.push((text("future_feature"), Value::Null))),
        "future_feature",
    );
    // A required origin/main key missing.
    assert_not_origin_main(
        &mutate_golden(|root| {
            root.retain(|(key, _)| !matches!(key, Value::Text(text) if text == "tickets"));
        }),
        "missing: tickets",
    );
    // A character that already has world heads.
    assert_not_origin_main(
        &mutate_golden(|root| {
            let Value::Map(characters) = slot(root, "characters") else {
                panic!("characters")
            };
            let Value::Map(record) = &mut characters[0].1 else {
                panic!("record")
            };
            record.push((text("world_heads"), Value::Map(Vec::new())));
        }),
        "world_heads",
    );
    // Snapshot and prepare records that already carry rom_world_id.
    for key in ["snapshots", "prepared"] {
        assert_not_origin_main(
            &mutate_golden(|root| {
                let Value::Map(records) = slot(root, key) else {
                    panic!("{key}")
                };
                let Value::Map(record) = &mut records[0].1 else {
                    panic!("record")
                };
                record.push((text("rom_world_id"), Value::Integer(1.into())));
            }),
            &format!("{key} holds a record with rom_world_id"),
        );
    }
    // Corruption: truncated, trailing bytes, not a map.
    assert_not_origin_main(&GOLDEN[..GOLDEN.len() - 7], "not a single CBOR value");
    let mut trailing = GOLDEN.to_vec();
    trailing.push(0);
    assert_not_origin_main(&trailing, "trailing bytes");
    assert_not_origin_main(&cbor(&Value::Array(Vec::new())), "not a CBOR map");
}

#[test]
fn assemble_drops_every_key_outside_the_allowlist() {
    let mut root: Vec<(String, Value)> = root_of(GOLDEN)
        .into_iter()
        .map(|(key, value)| {
            let Value::Text(key) = key else {
                panic!("text key")
            };
            (key, value)
        })
        .collect();
    root.push((
        "future_feature".to_owned(),
        Value::Map(vec![(text("a"), text("b"))]),
    ));
    let (output, dropped) =
        fresh_start::assemble(&root, Value::Map(Vec::new()), &[], false).expect("assemble");
    assert_eq!(dropped.get("future_feature"), Some(&1));
    assert!(dropped.contains_key("snapshots") && dropped.contains_key("groups"));
    let Value::Map(output) = output else {
        panic!("map")
    };
    assert!(
        !output
            .iter()
            .any(|(key, _)| matches!(key, Value::Text(text) if text == "future_feature"))
    );
    let mut bytes = Vec::new();
    ciborium::into_writer(&Value::Map(output), &mut bytes).unwrap();
    assert!(decode_state(&bytes).is_ok());
}

#[test]
fn tombstones_put_in_flight_and_heads_first_within_the_headroom_bound() {
    // 700 extra history IDs plus 600 old tombstones: > 1,024 legacy IDs.
    let payload = mutate_golden(|root| {
        let Value::Map(snapshots) = slot(root, "snapshots") else {
            panic!("snapshots")
        };
        for index in 0..700 {
            snapshots.push((snapshot_key(index), Value::Null));
        }
        *slot(root, "retired_snapshots") = Value::Array((1_000..1_600).map(snapshot_key).collect());
    });
    let outcome = transform(&payload, false, false).expect("transform");
    let state = decode_state(&outcome.payload).unwrap();
    let report = &outcome.report;
    assert_eq!(state.retired_snapshots.len(), TOMBSTONE_HEADROOM_BOUND);
    assert_eq!(report.retired_snapshots_final, 512);
    assert_eq!(report.retired_snapshots_bound, 512);
    assert_eq!(report.retired_snapshots_headroom, 512);
    assert_eq!(report.retired_snapshots_in_flight_and_heads, 4);
    // 5 golden IDs (4 in-flight/heads, 1 history) + 700 history + 600 old
    // tombstones, all unique; everything beyond the 512 kept overflows.
    assert_eq!(report.retired_snapshots_overflow, 5 + 700 + 600 - 512);
    let legacy = root_of(GOLDEN);
    let Value::Map(prepared) = lookup(&legacy, "prepared") else {
        panic!("prepared")
    };
    for (id, _) in prepared {
        assert!(
            state
                .retired_snapshots
                .contains(&id.deserialized().unwrap())
        );
    }
    let Value::Map(characters) = lookup(&legacy, "characters") else {
        panic!("characters")
    };
    for (_, record) in characters {
        let head: Option<SnapshotId> = field_of(record, "active_snapshot");
        assert!(state.retired_snapshots.contains(&head.unwrap()));
    }
    // History outranks old tombstones: none of the latter fit.
    assert!(
        (1_000..1_600)
            .map(|index| snapshot_key(index).deserialized::<SnapshotId>().unwrap())
            .all(|id| !state.retired_snapshots.contains(&id))
    );
}

#[test]
fn in_flight_tombstones_beyond_the_bound_need_an_explicit_flag() {
    let payload = mutate_golden(|root| {
        let Value::Map(prepared) = slot(root, "prepared") else {
            panic!("prepared")
        };
        for index in 0..600 {
            prepared.push((snapshot_key(index), Value::Null));
        }
    });
    assert_eq!(
        transform(&payload, false, false).err(),
        Some(FreshStartError::TombstoneOverflow {
            required: 604,
            bound: TOMBSTONE_HEADROOM_BOUND,
        })
    );
    let outcome = transform(&payload, false, true).expect("overflow allowed");
    let state = decode_state(&outcome.payload).unwrap();
    assert_eq!(
        outcome.report.retired_snapshots_bound,
        storage::MAX_RETIRED_SNAPSHOTS
    );
    assert_eq!(outcome.report.retired_snapshots_in_flight_and_heads, 604);
    assert_eq!(state.retired_snapshots.len(), 604 + 1);
    assert!(
        (0..600)
            .map(|index| snapshot_key(index).deserialized::<SnapshotId>().unwrap())
            .all(|id| state.retired_snapshots.contains(&id))
    );
    assert_eq!(
        outcome.report.retired_snapshots_headroom,
        storage::MAX_RETIRED_SNAPSHOTS - 605
    );
}

fn runbook() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../deploy/coop/FRESH_START.md");
    std::fs::read_to_string(path)
        .expect("deploy/coop/FRESH_START.md")
        .replace("\r\n", "\n")
}

/// Returns the fenced `sql` block that follows `<!-- fresh-start-sql:NAME -->`.
fn runbook_sql(document: &str, name: &str) -> String {
    let marker = format!("<!-- fresh-start-sql:{name} -->");
    let after = &document[document.find(&marker).unwrap_or_else(|| panic!("{marker}"))..];
    let start = after.find("```sql\n").expect("sql fence") + "```sql\n".len();
    let end = after[start..].find("\n```").expect("closing fence");
    after[start..start + end].to_owned()
}

#[test]
fn runbook_sql_is_digest_pinned_and_lock_guarded() {
    let document = runbook();
    for name in ["rollback-b", "delete-archive"] {
        let sql = runbook_sql(&document, name);
        assert!(
            sql.contains("pg_try_advisory_xact_lock(1129271120, 1)"),
            "{name}"
        );
        assert!(sql.contains("decode('<HEX>', 'hex')"), "{name}");
        assert!(sql.contains("GET DIAGNOSTICS"), "{name}");
        assert!(
            !sql.contains("LIMIT 1"),
            "{name} must pin by digest, not recency"
        );
    }
    assert!(runbook_sql(&document, "rollback-b").contains("decode('<CURRENT>', 'hex')"));
    for required in [
        "COOP_IMAGE=<NEW>",
        "COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT>",
        "fresh-start --expect-sha256 <HEX> --dry-run",
        "backups/pre-fresh-start/",
        "pg_restore --list",
        "--allow-tombstone-overflow",
        "First multi-world rollout",
    ] {
        assert!(document.contains(required), "{required}");
    }
    assert!(!document.contains("Deploy the new image"));
}

fn create_disposable_database() -> (postgres::Client, String, String) {
    let base = std::env::var("COOP_TEST_DATABASE_URL").expect("COOP_TEST_DATABASE_URL");
    let mut admin = postgres::Client::connect(&base, postgres::NoTls).expect("admin");
    let name = format!("coop_fresh_{}", Uuid::new_v4().simple());
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .expect("create database");
    let mut url = url::Url::parse(&base).expect("url");
    url.set_path(&name);
    (admin, name, url.into())
}

// Runs only against a disposable database server; never production.
#[test]
#[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
#[allow(clippy::too_many_lines)]
fn postgres_runbook_rollback_b_and_archived_but_not_fresh() {
    use crate::phase2::persistent::PostgresStateRepository;
    let (mut admin, name, url) = create_disposable_database();
    let outcome = std::panic::catch_unwind(|| {
        let document = runbook();
        let legacy_sha = sha256(GOLDEN);
        let legacy_hex = hex(&legacy_sha);
        let options = FreshStartOptions {
            expected_sha256: legacy_sha,
            drop_sessions: false,
            dry_run: false,
            allow_tombstone_overflow: false,
        };
        let mut db = postgres::Client::connect(&url, postgres::NoTls).expect("db");
        db.batch_execute(include_str!("../../migrations/0003_pilot_checkpoint.sql"))
            .unwrap();
        db.execute(
            "INSERT INTO coop_pilot_checkpoint (id, format_version, payload) VALUES (1, 1, $1)",
            &[&GOLDEN],
        )
        .unwrap();
        assert!(PostgresStateRepository::connect(&url).is_err());
        let stored = |db: &mut postgres::Client| -> Vec<u8> {
            db.query_one(
                "SELECT payload FROM coop_pilot_checkpoint WHERE id = 1",
                &[],
            )
            .unwrap()
            .get(0)
        };
        let archive_rows = |db: &mut postgres::Client| -> i64 {
            db.query_one("SELECT count(*) FROM coop_pilot_checkpoint_archive", &[])
                .unwrap()
                .get(0)
        };
        let report = fresh_start::run(&url, &options).expect("fresh start");
        assert_eq!(report.outcome, "fresh_started");
        let new_payload = stored(&mut db);
        let new_hex = hex(&sha256(&new_payload));
        assert_eq!(report.new_payload_sha256.as_deref(), Some(new_hex.as_str()));
        let rollback = |current: &str| {
            runbook_sql(&document, "rollback-b")
                .replace("<HEX>", &legacy_hex)
                .replace("<CURRENT>", current)
        };
        // Each attempt runs in its own session, like one psql invocation.
        let attempt = |sql: &str| -> Result<(), String> {
            let mut session = postgres::Client::connect(&url, postgres::NoTls).unwrap();
            session.batch_execute(sql).map_err(|error| {
                error
                    .as_db_error()
                    .map_or_else(|| error.to_string(), |db| db.message().to_owned())
            })
        };
        // A running server (advisory lock holder) blocks the rollback.
        let held: bool = db
            .query_one("SELECT pg_try_advisory_lock(1129271120, 1)", &[])
            .unwrap()
            .get(0);
        assert!(held);
        let error = attempt(&rollback(&new_hex)).expect_err("lock held");
        assert!(error.contains("advisory lock is held"), "{error}");
        db.query_one("SELECT pg_advisory_unlock(1129271120, 1)", &[])
            .unwrap();
        assert_eq!(stored(&mut db), new_payload);
        // A checkpoint that changed since inspection is not overwritten.
        let error = attempt(&rollback(&"0".repeat(64))).expect_err("digest pinned");
        assert!(error.contains("expected exactly 1"), "{error}");
        assert_eq!(stored(&mut db), new_payload);
        // The pinned rollback restores the archived original byte for byte.
        attempt(&rollback(&new_hex)).expect("rollback B");
        assert_eq!(stored(&mut db), GOLDEN);
        assert_eq!(archive_rows(&mut db), 1);
        assert!(PostgresStateRepository::connect(&url).is_err());
        // The archive still holds that digest: fresh-start refuses.
        assert_eq!(
            fresh_start::run(&url, &options),
            Err(FreshStartError::ArchivedButNotFresh)
        );
        assert_eq!(stored(&mut db), GOLDEN);
        // Exported and deleted (digest-pinned), a second fresh start works.
        let delete = runbook_sql(&document, "delete-archive").replace("<HEX>", &legacy_hex);
        let held: bool = db
            .query_one("SELECT pg_try_advisory_lock(1129271120, 1)", &[])
            .unwrap()
            .get(0);
        assert!(held);
        attempt(&delete).expect_err("delete refuses while the lock is held");
        db.query_one("SELECT pg_advisory_unlock(1129271120, 1)", &[])
            .unwrap();
        attempt(&delete.replace(&legacy_hex, &"0".repeat(64))).expect_err("pinned");
        assert_eq!(archive_rows(&mut db), 1);
        attempt(&delete).expect("delete archive row");
        assert_eq!(archive_rows(&mut db), 0);
        let again = fresh_start::run(&url, &options).expect("second fresh start");
        assert_eq!(again.outcome, "fresh_started");
        assert_eq!(archive_rows(&mut db), 1);
        let repository = PostgresStateRepository::connect(&url).expect("server starts");
        repository
            .read_transaction(&mut |state| {
                assert_eq!(state.users_by_id.len(), 3);
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

/// Writes this build's fresh-start outputs of the golden checkpoint for
/// the origin/main rollback-compatibility probe (tests/fixtures/README.md).
#[test]
#[ignore = "writes rollback probe inputs to COOP_FRESH_START_PROBE_DIR"]
fn write_rollback_probe_inputs() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("COOP_FRESH_START_PROBE_DIR").expect("COOP_FRESH_START_PROBE_DIR"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let fresh = transform(GOLDEN, false, false).expect("transform").payload;
    std::fs::write(directory.join("01-fresh.cbor"), &fresh).unwrap();
    let dropped = transform(GOLDEN, true, false).expect("transform").payload;
    std::fs::write(directory.join("02-fresh-drop-sessions.cbor"), &dropped).unwrap();
    // The new build serves a login and a world acquire, but no save.
    let app = fresh_app();
    load(&app, decode_state(&fresh).unwrap());
    let login = app
        .login(LoginRequest::new("fixtureuser", password()).unwrap())
        .unwrap();
    let character_id = app
        .store
        .inspect_state(|state| state.users_by_id[&login.user_id].character_id)
        .unwrap();
    let actor = AuthenticatedActor {
        user_id: login.user_id,
        character_id,
    };
    let client = id(ClientInstanceId::new);
    let lease = app
        .acquire_world(
            actor,
            AcquireLeaseRequest::new(character_id, client, id(IdempotencyKey::new)),
        )
        .expect("acquire world")
        .lease;
    let snapshot = |app: &Phase2App| {
        app.store
            .inspect_state(|state| encode(state).unwrap())
            .unwrap()
    };
    std::fs::write(
        directory.join("03-fresh-after-new-login-acquire.cbor"),
        snapshot(&app),
    )
    .unwrap();
    // The new build's first save creates a world head.
    let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
    upload_prepared(&app, &prepared);
    app.finalize(
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
    .expect("first save");
    std::fs::write(
        directory.join("04-fresh-after-new-first-save.cbor"),
        snapshot(&app),
    )
    .unwrap();
}

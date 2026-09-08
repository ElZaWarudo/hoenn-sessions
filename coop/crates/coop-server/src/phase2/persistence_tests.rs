// These tests create and drop only their own randomly named databases.
mod durable {
    use super::*;
    use crate::phase2::persistent::PostgresStateRepository;

    struct Database {
        admin: postgres::Client,
        name: String,
        url: String,
    }
    impl Database {
        fn new() -> Self {
            let base = std::env::var("COOP_TEST_DATABASE_URL")
                .expect("set dedicated COOP_TEST_DATABASE_URL");
            let mut admin =
                postgres::Client::connect(&base, postgres::NoTls).expect("test database");
            let name = format!("coop_test_{}", uuid::Uuid::new_v4().simple());
            admin
                .batch_execute(&format!("CREATE DATABASE {name}"))
                .expect("isolated database");
            let mut url = url::Url::parse(&base).expect("database URL");
            url.set_path(&name);
            Self {
                admin,
                name,
                url: url.into(),
            }
        }

        fn repository(&self) -> PostgresStateRepository {
            // Dropping a worker closes its socket asynchronously.
            for _ in 0..100 {
                if let Ok(repository) = PostgresStateRepository::connect(&self.url) {
                    return repository;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("repository did not become available");
        }

        fn app(&self, objects: Arc<InMemoryObjectStore>) -> Phase2App {
            Phase2App::new(
                Phase2Config::local(
                    vec![7; 32],
                    SigningPrivateKey::from_bytes([8; 32]),
                    "persistent-test",
                )
                .expect("config")
                .with_password_engine(Arc::new(
                    ArgonPasswordEngine::new(8192, 1, 1).expect("test hash"),
                ))
                .with_adapters(Arc::new(self.repository()), objects),
            )
            .expect("persistent app")
        }
    }
    impl Drop for Database {
        fn drop(&mut self) {
            let _ = self
                .admin
                .batch_execute(&format!("DROP DATABASE {} WITH (FORCE)", self.name));
        }
    }

    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    fn postgres_restart_preserves_auth_replay_and_interrupted_snapshot() {
        let database = Database::new();
        let objects = Arc::new(InMemoryObjectStore::new());
        let app = database.app(objects.clone());
        let (actor, lease, client) = account_and_lease(&app);
        let login = app
            .login(LoginRequest::new("fixtureuser", password()).expect("login"))
            .expect("authenticated");
        let rotated = app
            .refresh(RefreshRequest::new(login.refresh_token.clone()))
            .expect("rotation");
        assert_eq!(
            app.refresh(RefreshRequest::new(login.refresh_token)),
            Err(Phase2Error::Authentication)
        );
        let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
        // Simulate process death after the first artifact, before finalization.
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
            valid_character_sav_generation(false, 1),
        )
        .expect("first artifact");
        drop(app);
        let app = database.app(objects.clone());
        assert_eq!(
            app.refresh(RefreshRequest::new(rotated.refresh_token)),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            app.add_invitation("fixture-invite"),
            Err(Phase2Error::Conflict)
        );
        app.upload(
            &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
            b"{}".to_vec(),
        )
        .expect("remaining artifact");
        let request = SnapshotFinalizeRequest::new(
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
        .expect("finalize");
        let record = app.finalize(actor, request.clone()).expect("published");
        drop(app);
        let app = database.app(objects.clone());
        assert_eq!(app.finalize(actor, request), Ok(record.clone()));
        assert_eq!(
            app.store
                .inspect_state(|state| state.snapshots.len())
                .expect("state"),
            1
        );
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
        add_complex_state(&app, actor, lease, client, restore.clone());
        drop(app);
        let app = database.app(objects);
        app.store
            .inspect_state(|state| {
                assert_eq!(state.groups.len(), 1);
                assert_eq!(state.group_invitations.len(), 1);
                assert_eq!(state.group_idempotency.len(), 4);
                assert_eq!(state.realtime_tickets.len(), 1);
                assert_eq!(state.realtime_by_runtime.len(), 1);
                assert_eq!(state.restore_staging[&actor.character_id].request, restore);
                assert_eq!(state.restore_ops.len(), 1);
                assert_eq!(
                    state.snapshots[&restored.snapshot.snapshot_id],
                    restored.snapshot
                );
            })
            .expect("complex CBOR state survived restart");
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one fixture explicitly covers every nested co-op checkpoint response variant"
    )]
    fn add_complex_state(
        app: &Phase2App,
        actor: AuthenticatedActor,
        lease: coop_cloud::LeaseContract,
        client: ClientInstanceId,
        restore: SnapshotRestoreRequest,
    ) {
        use crate::phase2::storage::{
            GroupIdempotencyRecord, GroupIdempotencyResponse, GroupInvitationRecord, GroupRecord,
            GroupStatus, RealtimeTicketRecord, RestoreStage,
        };
        use coop_cloud::{
            ApiVersion, Group, GroupId, GroupInvitationId, GroupInvitationView, GroupView,
            RuntimeLeaseFence, StableRuntimeSession, UnixTimestampMillis,
        };
        let second = id(CharacterId::new);
        let group_id = id(GroupId::new);
        let group = Group::new(actor.character_id, second).expect("group");
        let zone = coop_protocol::WorldZone::new(coop_protocol::RegionId::Hoenn, "ROUTE101", 1)
            .expect("zone");
        let view = GroupView::new(group_id, group, zone.clone(), [0, 0]).expect("group view");
        let invitation_id = id(GroupInvitationId::new);
        let invitation = GroupInvitationView {
            api_version: ApiVersion::V1,
            invitation_id,
            inviter_character_id: actor.character_id,
            invitee_character_id: second,
            expires_at: UnixTimestampMillis::new(1_700_000_030_000),
        };
        let session = StableRuntimeSession::new(
            lease.session_id,
            actor.character_id,
            lease.session_epoch,
            client,
        );
        let runtime = RuntimeLeaseFence::new(
            session,
            saves::current_runtime_build_identity().expect("runtime build"),
        );
        app.store
            .write_transaction(|state| {
                state.groups.insert(
                    group_id,
                    GroupRecord {
                        group,
                        zone,
                        status: GroupStatus::Active,
                    },
                );
                state
                    .active_group_by_member
                    .insert(actor.character_id, group_id);
                state.group_invitations.insert(
                    invitation_id,
                    GroupInvitationRecord {
                        invitation_id,
                        inviter: actor.character_id,
                        invitee: second,
                        expires_at: 1_700_000_030_000,
                        consumed: false,
                    },
                );
                for response in [
                    GroupIdempotencyResponse::Invitation(invitation),
                    GroupIdempotencyResponse::Accept(coop_cloud::AcceptGroupInvitationResponse {
                        api_version: ApiVersion::V1,
                        group: view.clone(),
                    }),
                    GroupIdempotencyResponse::Travel(coop_cloud::GroupTravelResponse {
                        api_version: ApiVersion::V1,
                        group: view.clone(),
                    }),
                    GroupIdempotencyResponse::Online(coop_cloud::OnlineActionResponse::Accepted {
                        group: view.clone(),
                    }),
                ] {
                    state.group_idempotency.insert(
                        (
                            actor.character_id,
                            "fixture".into(),
                            id(IdempotencyKey::new),
                        ),
                        GroupIdempotencyRecord {
                            fingerprint: [3; 32],
                            response,
                            expires_at: 1_700_000_030_000,
                        },
                    );
                }
                state.realtime_tickets.insert(
                    [4; 32],
                    RealtimeTicketRecord {
                        user_id: actor.user_id,
                        character_id: actor.character_id,
                        session,
                        runtime,
                        expires_at: 1_700_000_030_000,
                    },
                );
                state
                    .realtime_by_runtime
                    .insert((actor.user_id, session), [4; 32]);
                state.restore_staging.insert(
                    actor.character_id,
                    RestoreStage {
                        request: restore,
                        snapshot_id: id(SnapshotId::new),
                        expires_at: 1_700_000_030_000,
                        storage_bytes: 123,
                        created_objects: vec!["characters/fixture/snapshot/character.sav".into()],
                    },
                );
                Ok::<(), StorageError>(())
            })
            .expect("persist complex metadata");
    }

    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    fn postgres_exclusive_owner_and_connection_loss_fence_cached_reads() {
        let mut database = Database::new();
        let repository = database.repository();
        assert!(PostgresStateRepository::connect(&database.url).is_err());
        repository
            .write_transaction(&mut |state| {
                state.invitations.insert([9; 32], true);
                Err(StorageError::Transaction)
            })
            .expect_err("callback error");
        database
            .admin
            .query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1",
                &[&database.name],
            )
            .expect("terminate owned connection");
        let mut called = false;
        assert_eq!(
            repository.read_transaction(&mut |_| {
                called = true;
                Ok(())
            }),
            Err(StorageError::Persistence)
        );
        assert!(!called, "must not serve stale cached auth");
        drop(repository);
        database
            .repository()
            .read_transaction(&mut |state| {
                assert_eq!(state.invitations.get(&[9; 32]), Some(&true));
                Ok(())
            })
            .expect("error mutation was durable");
    }

    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    fn postgres_commit_failure_never_returns_mutated_cache() {
        let database = Database::new();
        let repository = database.repository();
        let mut intruder =
            postgres::Client::connect(&database.url, postgres::NoTls).expect("test control");
        intruder.batch_execute("ALTER TABLE coop_pilot_checkpoint ADD CONSTRAINT force_failed_commit CHECK (false) NOT VALID").expect("inject update failure");
        let mut calls = 0;
        assert_eq!(
            repository.write_transaction(&mut |state| {
                calls += 1;
                state.invitations.insert([1; 32], true);
                Ok(())
            }),
            Err(StorageError::Persistence)
        );
        assert_eq!(calls, 1);
        assert_eq!(
            repository.read_transaction(&mut |_| Ok(())),
            Err(StorageError::Persistence)
        );
        drop(repository);
        intruder
            .batch_execute("ALTER TABLE coop_pilot_checkpoint DROP CONSTRAINT force_failed_commit")
            .expect("remove injection");
        database
            .repository()
            .read_transaction(&mut |state| {
                assert!(state.invitations.is_empty());
                Ok(())
            })
            .expect("old durable state");
    }

    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    fn postgres_production_http_health_tracks_database_ownership() {
        let mut database = Database::new();
        let app = database.app(Arc::new(InMemoryObjectStore::new()));
        let mut config = (*app.store.config).clone();
        config.mode = StorageMode::PostgresFirebase;
        config.production =
            Some(ProductionConfig::new(&database.url, "test-bucket").expect("production settings"));
        config.upload_base_url = "https://coop.example.test".into();
        let app = Phase2App::new(config).expect("production app");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let status = |app: &Phase2App| {
            runtime.block_on(async {
                app.router()
                    .oneshot(
                        axum::http::Request::builder()
                            .uri("/health/ready")
                            .body(axum::body::Body::empty())
                            .expect("request"),
                    )
                    .await
                    .expect("response")
                    .status()
            })
        };
        assert_eq!(status(&app), StatusCode::OK);
        database
            .admin
            .query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1",
                &[&database.name],
            )
            .expect("connection lost");
        assert_eq!(status(&app), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    #[ignore = "requires dedicated COOP_TEST_DATABASE_URL with CREATE DATABASE permission"]
    fn postgres_rejects_unknown_truncated_and_corrupt_checkpoint_without_reset() {
        let database = Database::new();
        drop(database.repository());
        let mut control =
            postgres::Client::connect(&database.url, postgres::NoTls).expect("test control");
        let original: Vec<u8> = control
            .query_one("SELECT payload FROM coop_pilot_checkpoint WHERE id=1", &[])
            .expect("row")
            .get(0);
        for (version, bytes) in [
            (2_i32, original.clone()),
            (1, original[..original.len() / 2].to_vec()),
            (1, vec![0xff]),
        ] {
            control
                .execute(
                    "UPDATE coop_pilot_checkpoint SET format_version=$1, payload=$2 WHERE id=1",
                    &[&version, &bytes],
                )
                .expect("inject bad checkpoint");
            assert!(PostgresStateRepository::connect(&database.url).is_err());
            let row = control
                .query_one(
                    "SELECT format_version, payload FROM coop_pilot_checkpoint WHERE id=1",
                    &[],
                )
                .expect("unchanged row");
            assert_eq!(row.get::<_, i32>(0), version);
            assert_eq!(row.get::<_, Vec<u8>>(1), bytes);
        }
    }
}

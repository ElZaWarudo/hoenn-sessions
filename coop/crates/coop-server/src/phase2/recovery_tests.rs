#[test]
fn blocked_restore_keeps_other_players_heartbeats_available() {
    check_blocked_restore(false);
}

#[test]
fn restore_revalidates_group_membership_after_remote_copy() {
    check_blocked_restore(true);
}

struct DelayedRead {
    inner: Arc<InMemoryObjectStore>,
    arrived: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    resume: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl ObjectStore for DelayedRead {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let sender = self.arrived.lock().expect("arrival").take();
        if let Some(sender) = sender {
            sender.send(()).expect("remote read started");
            self.resume
                .lock()
                .expect("resume")
                .recv_timeout(std::time::Duration::from_secs(30))
                .expect("resume read");
        }
        self.inner.get(key)
    }
    fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError> {
        self.inner.put(key, bytes)
    }
    fn contains(&self, key: &str) -> Result<bool, StorageError> {
        self.inner.contains(key)
    }
    fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
        self.inner.delete_if_present(key)
    }
    fn retire_if_absent(&self, key: &str) -> Result<bool, StorageError> {
        self.inner.retire_if_absent(key)
    }
    fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
        self.inner.put_if_absent(key, bytes)
    }
}
fn check_blocked_restore(join_group: bool) {
    let (mut app, objects) = deterministic_app_with_store();
    let (actor, lease, client) = account_and_lease(&app);
    let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
    upload_prepared(&app, &prepared);
    let first = app
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
            .expect("request"),
        )
        .expect("first save");
    let (second_actor, second_lease) = second_heartbeat_player(&app);
    let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    app.store.objects = Arc::new(DelayedRead {
        inner: objects,
        arrived: std::sync::Mutex::new(Some(arrived_tx)),
        resume: std::sync::Mutex::new(resume_rx),
    });
    let restoring = app.clone();
    let request = SnapshotRestoreRequest::new(
        first.snapshot_id,
        first.session_id,
        actor.character_id,
        first.revision,
        first.session_epoch,
        client,
        id(IdempotencyKey::new),
    );
    let restore = std::thread::spawn(move || restoring.restore(actor, &request));
    arrived_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("restore reading remote bytes");
    let heartbeat_app = app.clone();
    let (heartbeat_tx, heartbeat_rx) = std::sync::mpsc::channel();
    let heartbeat = std::thread::spawn(move || {
        heartbeat_tx
            .send(heartbeat_app.heartbeat(
                second_actor,
                HeartbeatLeaseRequest::new(second_lease.fence()),
            ))
            .expect("heartbeat result");
    });
    let timely = heartbeat_rx.recv_timeout(std::time::Duration::from_secs(5));
    if join_group {
        let invite = app
            .create_group_invitation(
                second_actor,
                coop_cloud::CreateGroupInvitationRequest::new(
                    second_lease.fence(),
                    actor.character_id,
                    id(IdempotencyKey::new),
                ),
            )
            .expect("invite while restore waits");
        let fence = LeaseFence::new(
            first.session_id,
            actor.character_id,
            first.revision,
            first.session_epoch,
            client,
        );
        app.accept_group_invitation(
            actor,
            invite.invitation_id,
            coop_cloud::AcceptGroupInvitationRequest::new(fence, id(IdempotencyKey::new)),
        )
        .expect("join group during restore");
    }
    resume_tx.send(()).expect("release remote read");
    heartbeat.join().expect("heartbeat thread");
    let result = restore.join().expect("restore thread");
    if join_group {
        assert_eq!(result, Err(Phase2Error::Conflict));
        assert_eq!(
            app.store
                .inspect_state(|state| state.characters[&actor.character_id].active_snapshot)
                .expect("active snapshot"),
            Some(first.snapshot_id)
        );
    } else {
        result.expect("restored");
    }
    timely
        .expect("heartbeat must complete before remote restore resumes")
        .expect("heartbeat remains valid");
}

fn second_heartbeat_player(app: &Phase2App) -> (AuthenticatedActor, coop_cloud::LeaseContract) {
    app.add_invitation("second-heartbeat-player")
        .expect("second invite");
    let second = app
        .register(
            RegisterRequest::new(
                "SecondPlayer",
                password(),
                InvitationCode::new("second-heartbeat-player").expect("invite"),
            )
            .expect("register request"),
        )
        .expect("second player");
    let second_actor = AuthenticatedActor {
        user_id: second.user_id,
        character_id: second.character_id,
    };
    let second_lease = app
        .acquire(
            second_actor,
            AcquireLeaseRequest::new(
                second.character_id,
                id(ClientInstanceId::new),
                id(IdempotencyKey::new),
            ),
        )
        .expect("second lease");
    (second_actor, second_lease)
}

#[test]
fn expired_cleanup_fences_remote_create_that_finishes_after_retirement() {
    struct DelayedPublication {
        inner: Arc<InMemoryObjectStore>,
        arrived: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
        resume: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl ObjectStore for DelayedPublication {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            self.inner.get(key)
        }
        fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError> {
            self.inner.put(key, bytes)
        }
        fn contains(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.contains(key)
        }
        fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.delete_if_present(key)
        }
        fn retire_if_absent(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.retire_if_absent(key)
        }
        fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
            let sender = self.arrived.lock().expect("arrival").take();
            if let Some(sender) = sender {
                sender.send(()).expect("pending remote request");
                self.resume
                    .lock()
                    .expect("resume")
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .expect("remote completion");
            }
            self.inner.put_if_absent(key, bytes)
        }
    }
    let (mut app, clock) = deterministic_app();
    let (actor, lease, client) = account_and_lease(&app);
    let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
    let objects = Arc::new(InMemoryObjectStore::new());
    let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    app.store.objects = Arc::new(DelayedPublication {
        inner: objects.clone(),
        arrived: std::sync::Mutex::new(Some(arrived_tx)),
        resume: std::sync::Mutex::new(resume_rx),
    });
    let ticket = upload_ticket(&prepared, ArtifactIdentity::CharacterSav);
    let original = app.clone();
    let original_ticket = ticket.clone();
    let upload =
        std::thread::spawn(move || original.upload(&original_ticket, valid_character_sav(false)));
    arrived_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("in flight");
    clock.advance(storage::UPLOAD_TTL_MS + 1);
    assert_eq!(
        app.upload(&ticket, valid_character_sav(false)),
        Err(Phase2Error::Expired)
    );
    resume_tx.send(()).expect("allow late remote create");
    assert_eq!(
        upload.join().expect("upload thread"),
        Err(Phase2Error::Conflict)
    );
    let key = storage::Store::object_key(
        actor.character_id,
        prepared.snapshot_id,
        ArtifactIdentity::CharacterSav,
    );
    assert_eq!(objects.object_count().expect("no leaked live data"), 0);
    assert!(
        !objects
            .delete_if_present(&key)
            .expect("cannot remove retirement fence")
    );
    assert!(
        !objects
            .put_if_absent(key, valid_character_sav(false))
            .expect("key remains sealed")
    );
}

#[test]
fn delayed_creator_cannot_delete_snapshot_published_by_recovering_retry() {
    struct DelayedCreate {
        inner: Arc<InMemoryObjectStore>,
        arrived: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
        resume: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl ObjectStore for DelayedCreate {
        fn retire_if_absent(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.retire_if_absent(key)
        }
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            self.inner.get(key)
        }
        fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError> {
            self.inner.put(key, bytes)
        }
        fn contains(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.contains(key)
        }
        fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
            self.inner.delete_if_present(key)
        }
        fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
            let created = self.inner.put_if_absent(key, bytes)?;
            if created {
                let sender = self.arrived.lock().expect("arrival lock").take();
                if let Some(sender) = sender {
                    sender.send(()).expect("creator arrived");
                    self.resume
                        .lock()
                        .expect("resume lock")
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .expect("resume creator");
                }
            }
            Ok(created)
        }
    }
    let (mut app, objects) = deterministic_app_with_store();
    let (actor, lease, client) = account_and_lease(&app);
    let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
    app.upload(
        &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
        b"{}".to_vec(),
    )
    .expect("pending upload");
    let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    app.store.objects = Arc::new(DelayedCreate {
        inner: objects.clone(),
        arrived: std::sync::Mutex::new(Some(arrived_tx)),
        resume: std::sync::Mutex::new(resume_rx),
    });
    let ticket = upload_ticket(&prepared, ArtifactIdentity::CharacterSav);
    let first_app = app.clone();
    let first_ticket = ticket.clone();
    let creator =
        std::thread::spawn(move || first_app.upload(&first_ticket, valid_character_sav(false)));
    arrived_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("object created");
    app.upload(&ticket, valid_character_sav(false))
        .expect("retry adopts exact object");
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
    .expect("finalize request");
    app.finalize(actor, request)
        .expect("retry publishes snapshot");
    resume_tx.send(()).expect("unblock original creator");
    assert_eq!(
        creator.join().expect("creator thread"),
        Err(Phase2Error::Conflict)
    );
    let key = storage::Store::object_key(
        actor.character_id,
        prepared.snapshot_id,
        ArtifactIdentity::CharacterSav,
    );
    assert_eq!(
        objects.get(&key).expect("canonical object"),
        Some(valid_character_sav(false))
    );
}

#[test]
fn uploaded_bytes_without_ownership_checkpoint_can_be_finalized_after_retry() {
    let (app, _) = deterministic_app();
    let (actor, lease, client) = account_and_lease(&app);
    let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
    let key = storage::Store::object_key(
        actor.character_id,
        prepared.snapshot_id,
        ArtifactIdentity::CharacterSav,
    );
    // Exact crash boundary: object committed remotely, database ownership absent.
    app.store
        .objects
        .put_if_absent(key.clone(), valid_character_sav(false))
        .expect("remote create");
    app.upload(
        &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
        valid_character_sav(false),
    )
    .expect("recover upload");
    app.upload(
        &upload_ticket(&prepared, ArtifactIdentity::PendingCommits),
        b"{}".to_vec(),
    )
    .expect("pending upload");
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
    .expect("finalize request");
    let first = app
        .finalize(actor, request.clone())
        .expect("finalize recovered upload");
    assert_eq!(first.revision, Revision::new(1));
    assert_eq!(
        app.finalize(actor, request).expect("idempotent finalize"),
        first
    );
    assert!(
        app.store
            .objects
            .contains(&key)
            .expect("canonical retained")
    );
}

#[test]
fn expired_uncheckpointed_upload_is_cleaned_only_when_bytes_match() {
    for matching in [true, false] {
        let (app, clock) = deterministic_app();
        let (actor, lease, client) = account_and_lease(&app);
        let (prepared, _, _) = prepared_snapshot(&app, actor, lease, client);
        let key = storage::Store::object_key(
            actor.character_id,
            prepared.snapshot_id,
            ArtifactIdentity::CharacterSav,
        );
        let bytes = if matching {
            valid_character_sav(false)
        } else {
            b"unrelated object".to_vec()
        };
        app.store
            .objects
            .put_if_absent(key.clone(), bytes)
            .expect("remote create");
        clock.advance(storage::UPLOAD_TTL_MS + 1);
        assert_eq!(
            app.upload(
                &upload_ticket(&prepared, ArtifactIdentity::CharacterSav),
                valid_character_sav(false)
            ),
            Err(Phase2Error::Expired)
        );
        assert_eq!(
            app.store.objects.contains(&key).expect("object state"),
            !matching
        );
        assert!(
            !app.store
                .inspect_state(|state| state.prepared.contains_key(&prepared.snapshot_id))
                .expect("retired")
        );
    }
}

#[test]
fn expired_restore_discovers_copy_created_before_ownership_checkpoint() {
    let (app, objects) = deterministic_app_with_store();
    let (actor, lease, client) = account_and_lease(&app);
    let (prepared, sav, pending) = prepared_snapshot(&app, actor, lease, client);
    upload_prepared(&app, &prepared);
    let first = app
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
            .expect("request"),
        )
        .expect("first save");
    let restore = SnapshotRestoreRequest::new(
        first.snapshot_id,
        first.session_id,
        actor.character_id,
        first.revision,
        first.session_epoch,
        client,
        id(IdempotencyKey::new),
    );
    let interrupted_snapshot = id(SnapshotId::new);
    let interrupted_key = storage::Store::object_key(
        actor.character_id,
        interrupted_snapshot,
        ArtifactIdentity::CharacterSav,
    );
    app.store
        .write_transaction(|state| {
            state.restore_staging.insert(
                actor.character_id,
                storage::RestoreStage {
                    request: restore.clone(),
                    snapshot_id: interrupted_snapshot,
                    expires_at: 0,
                    storage_bytes: first.files.iter().map(|file| file.size_bytes).sum(),
                    created_objects: vec![],
                },
            );
            Ok::<_, Phase2Error>(())
        })
        .expect("durable reservation");
    app.store
        .objects
        .put_if_absent(interrupted_key.clone(), valid_character_sav(false))
        .expect("uncheckpointed copy");
    let result = app
        .restore(actor, &restore)
        .expect("recover and retry restore");
    assert_eq!(result.snapshot.revision, Revision::new(2));
    assert_eq!(objects.object_count().expect("object count"), 4);
    assert!(
        !app.store
            .objects
            .contains(&interrupted_key)
            .expect("interrupted copy removed")
    );
    assert_eq!(
        app.restore(actor, &restore).expect("idempotent restore"),
        result
    );
}

//! A never-saved (revision 0) character starting through the world-aware
//! route on a device whose epoch history belongs to another character.
//!
//! This is the field state behind "Your game session could not start" after
//! the fresh-start reset: the previous character's device-wide epoch records
//! (including a versioned copy renamed aside) made every start fail at epoch
//! acceptance, before any ROM or save was staged. Revision zero itself needs
//! no SAV: the ROM creates its own save.

use super::*;
use crate::{EpochError, session::RomWorldId};

const NAME: &str = "freshstarter";

fn write_foreign_epoch(path: &Path, greatest_epoch: u32) {
    let record = serde_json::json!({
        "format_version": 1,
        "character_id": Uuid::new_v4(),
        "session_id": Uuid::new_v4(),
        "greatest_epoch": greatest_epoch,
    });
    std::fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
}

fn config(root: &Path, client: ClientInstanceId, epoch_store: EpochStore) -> SessionConfig {
    SessionConfig {
        client_instance_id: client,
        manifest: server_compatibility(),
        trusted_manifest_key: TrustedManifestKey::new(
            "test",
            ed25519_dalek::SigningKey::from_bytes(&[7; 32])
                .verifying_key()
                .to_bytes(),
        )
        .unwrap(),
        epoch_store,
        workspace_parent: root.join("sessions"),
        bridge_lua_dir: root.join("bridge"),
        rom_world_id: main_world(),
    }
}

async fn sign_in<A: CloudApi>(api: &A, keychain: &Arc<dyn RefreshTokenStore>) -> AuthSession {
    AuthSession::login(
        api,
        keychain.as_ref(),
        NAME,
        Password::new(PASSWORD).unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn never_saved_character_starts_despite_foreign_device_epoch_history() {
    let _serial = SERIAL.lock().await;
    let server = Server::start().await;
    register(&server.app, NAME);
    let api = ReqwestCloudApi::new(&server.base).unwrap();
    let root = tempdir().unwrap();
    let bridge = root.path().join("bridge");
    std::fs::create_dir_all(&bridge).unwrap();
    std::fs::write(bridge.join("generated_addresses.lua"), b"return {}\n").unwrap();
    // Another character's live versioned record and the renamed-aside copies
    // found on the affected desktop.
    write_foreign_epoch(&root.path().join("epoch.json.epoch-9"), 9);
    write_foreign_epoch(
        &root
            .path()
            .join("epoch.json.epoch-9.pre-account-switch-previous.bak"),
        9,
    );
    write_foreign_epoch(
        &root
            .path()
            .join("epoch.json.pre-account-switch-previous.bak"),
        1,
    );

    let keychain: Arc<dyn RefreshTokenStore> = Arc::new(TestKeychain::default());
    let mut auth = sign_in(&api, &keychain).await;
    let character = auth.character_id;
    let client = ClientInstanceId::new(Uuid::new_v4()).unwrap();
    // The durable intent: one request, replayed exactly on retry.
    let request = AcquireLeaseRequest::new(character, client, key());
    let first = SessionLifecycle::acquire_world_with_keychain(&api, &mut auth, request, &keychain)
        .await
        .unwrap();
    assert_eq!(first.active_world_id, main_world());
    assert_eq!(first.active_snapshot_id, None);
    assert_eq!(first.lease.current_revision, Revision::initial());

    // The pre-fix desktop shared one device-wide epoch file.
    let device_wide = EpochStore::new(root.path().join("epoch.json"));
    let refused = SessionLifecycle::from_world_lease_with_keychain(
        &api,
        auth,
        config(root.path(), client, device_wide),
        Arc::clone(&keychain),
        first,
    )
    .await;
    assert!(matches!(
        refused,
        Err(SessionError::Epoch(EpochError::IdentityMismatch))
    ));
    assert!(!root.path().join("epoch.json").exists());

    // Retry replays the persisted intent and gets the same lease back.
    let mut auth = sign_in(&api, &keychain).await;
    let replayed =
        SessionLifecycle::acquire_world_with_keychain(&api, &mut auth, request, &keychain)
            .await
            .unwrap();
    assert_eq!(replayed.lease.session_id, first.lease.session_id);
    assert_eq!(replayed.lease.session_epoch, first.lease.session_epoch);
    assert_eq!(replayed.lease.client_instance_id, client);

    let store = EpochStore::for_character(root.path(), character).unwrap();
    let session = SessionLifecycle::from_world_lease_with_keychain(
        &api,
        auth,
        config(root.path(), client, store.clone()),
        Arc::clone(&keychain),
        replayed,
    )
    .await
    .unwrap();
    assert_eq!(session.revision, Revision::initial());
    assert_eq!(session.rom_world_id(), RomWorldId::new(1).unwrap());
    assert_eq!(
        store
            .read(character, first.lease.session_id)
            .unwrap()
            .unwrap()
            .greatest_epoch,
        first.lease.session_epoch.value()
    );
    // Revision zero stages no launcher-authored SAV; the ROM creates it.
    let workspace = session.workspace.path().to_owned();
    assert!(!workspace.join("character.sav").exists());
    assert_eq!(
        std::fs::read(workspace.join("pending_commits.json")).unwrap(),
        b"[]"
    );
    session.release(&api).await.unwrap();
    server.stop().await;
}

//! Crash recovery against the real catalog-bound `coop-server`.
//!
//! The server binds a lease to a ROM world only through
//! `POST /v1/sessions/acquire-world` and rejects resume packages for an
//! unbound lease. These tests use no fixture that binds a world implicitly:
//! recovery must acquire the world itself, select its local configuration
//! from the server's `active_world_id`, and keep evidence untouched when that
//! selection does not match.

use std::path::PathBuf;

use coop_cloud::Sha256Digest;

use super::*;
use crate::recovery::{
    RecoveryDiscovery, RecoveryError, RecoveryMarkerV2, RecoveryOutcome, RecoveryReconciler,
};

const NAME: &str = "recoverer";

/// A crash after the server committed revision 1 but before the launcher
/// retired its recovery copy: the evidence names the revision-0 lease that
/// authorized the save and the exact committed SAV.
struct CommittedCrash {
    server: Server,
    api: ReqwestCloudApi,
    root: TempDir,
    prior_client: ClientInstanceId,
    evidence: PathBuf,
    save: Vec<u8>,
    marker: Vec<u8>,
}

impl CommittedCrash {
    async fn new() -> Self {
        let server = Server::start().await;
        register(&server.app, NAME);
        let api = ReqwestCloudApi::new(&server.base).unwrap();
        let mut member = start_member(&api, NAME, tempdir().unwrap()).await;
        let prior_fence = member.session.lease.fence();
        assert_eq!(prior_fence.current_revision, Revision::initial());
        let party = [mon(24, 1), mon(48, 4)];
        assert_eq!(
            member.checkpoint(&api, &party).await.unwrap(),
            Revision::new(1)
        );
        let save = party_save(1, &party);
        let Member { root, session, .. } = member;
        session.release(&api).await.unwrap();

        let evidence = root.path().join("sessions").join("coop-recovery-crash");
        std::fs::create_dir_all(&evidence).unwrap();
        let marker = RecoveryMarkerV2::new(
            prior_fence,
            Revision::initial(),
            1,
            Sha256Digest::of_bytes(&save),
        )
        .unwrap()
        .encode()
        .unwrap();
        std::fs::write(evidence.join("character.sav"), &save).unwrap();
        std::fs::write(evidence.join("recovery.marker"), &marker).unwrap();
        assert!(
            RecoveryDiscovery::discover(&root.path().join("sessions"))
                .unwrap()
                .candidate()
                .is_some()
        );
        Self {
            server,
            api,
            root,
            prior_client: prior_fence.client_instance_id,
            evidence,
            save,
            marker,
        }
    }

    fn config(&self, client: ClientInstanceId, world: coop_protocol::RomWorldId) -> SessionConfig {
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
            epoch_store: EpochStore::new(self.root.path().join("epoch.json")),
            workspace_parent: self.root.path().join("sessions"),
            bridge_lua_dir: self.root.path().join("bridge"),
            rom_world_id: world,
        }
    }

    async fn sign_in(&self) -> (AuthSession, Arc<dyn RefreshTokenStore>) {
        let keychain: Arc<dyn RefreshTokenStore> = Arc::new(TestKeychain::default());
        let auth = AuthSession::login(
            &self.api,
            keychain.as_ref(),
            NAME,
            Password::new(PASSWORD).unwrap(),
        )
        .await
        .unwrap();
        (auth, keychain)
    }

    async fn acquire_world(
        &self,
        auth: &mut AuthSession,
        keychain: &Arc<dyn RefreshTokenStore>,
        client: ClientInstanceId,
    ) -> coop_cloud::AcquireWorldLeaseResponse {
        let request = AcquireLeaseRequest::new(auth.character_id, client, key());
        SessionLifecycle::acquire_world_with_keychain(&self.api, auth, request, keychain)
            .await
            .unwrap()
    }

    fn assert_evidence_untouched(&self) {
        assert_eq!(
            std::fs::read(self.evidence.join("character.sav")).unwrap(),
            self.save
        );
        assert_eq!(
            std::fs::read(self.evidence.join("recovery.marker")).unwrap(),
            self.marker
        );
    }

    /// The server head is still the committed revision and the previous
    /// recovery lease is closed: a fresh world acquire succeeds at revision 1.
    async fn assert_server_head_unchanged(&self) {
        let (mut auth, keychain) = self.sign_in().await;
        let probe = ClientInstanceId::new(Uuid::new_v4()).unwrap();
        let response = self.acquire_world(&mut auth, &keychain, probe).await;
        assert_eq!(response.active_world_id, main_world());
        assert_eq!(response.lease.current_revision, Revision::new(1));
        SessionLifecycle::release_preacquired_world_lease(
            &self.api, &mut auth, response, &keychain,
        )
        .await
        .unwrap();
    }

    async fn retire_with_world_lease(&self) {
        let (mut auth, keychain) = self.sign_in().await;
        let response = self
            .acquire_world(&mut auth, &keychain, self.prior_client)
            .await;
        // Local selection follows the server's active world, never a
        // hard-coded main world.
        let config = self.config(self.prior_client, response.active_world_id);
        let recovered =
            RecoveryReconciler::reconcile_world_lease(&self.api, auth, config, keychain, response)
                .await
                .unwrap();
        assert_eq!(recovered.outcome, RecoveryOutcome::RetiredCommittedV2);
        assert!(!self.evidence.exists());
        assert!(recovered.auth.access_token().is_some());
    }
}

#[tokio::test]
async fn world_bound_recovery_retires_committed_evidence_and_releases_its_lease() {
    let _serial = SERIAL.lock().await;
    let crash = CommittedCrash::new().await;
    crash.retire_with_world_lease().await;
    crash.assert_server_head_unchanged().await;
    crash.server.stop().await;
}

#[tokio::test]
async fn world_or_instance_mismatch_releases_lease_and_preserves_evidence() {
    let _serial = SERIAL.lock().await;
    let crash = CommittedCrash::new().await;

    // The local selection names a world the server did not bind.
    let (mut auth, keychain) = crash.sign_in().await;
    let response = crash
        .acquire_world(&mut auth, &keychain, crash.prior_client)
        .await;
    assert_eq!(response.active_world_id, main_world());
    let other_world = coop_protocol::RomWorldId::new(main_world().get() + 1).unwrap();
    let config = crash.config(crash.prior_client, other_world);
    assert_eq!(
        RecoveryReconciler::reconcile_world_lease(&crash.api, auth, config, keychain, response)
            .await
            .err(),
        Some(RecoveryError::Blocked)
    );
    crash.assert_evidence_untouched();
    crash.assert_server_head_unchanged().await;

    // A v2 marker requires the prior client instance; a lease for any
    // other instance is released before session state is materialized.
    let (mut auth, keychain) = crash.sign_in().await;
    let stranger = ClientInstanceId::new(Uuid::new_v4()).unwrap();
    let response = crash.acquire_world(&mut auth, &keychain, stranger).await;
    let config = crash.config(stranger, response.active_world_id);
    assert_eq!(
        RecoveryReconciler::reconcile_world_lease(&crash.api, auth, config, keychain, response)
            .await
            .err(),
        Some(RecoveryError::Blocked)
    );
    crash.assert_evidence_untouched();
    crash.assert_server_head_unchanged().await;

    // The preserved evidence still reconciles once the selection matches.
    crash.retire_with_world_lease().await;
    crash.server.stop().await;
}

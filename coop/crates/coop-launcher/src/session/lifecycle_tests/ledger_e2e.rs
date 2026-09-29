//! Level 1 outcome ledger, end to end (plan step B7).
//!
//! The server is the real `coop-server` `Phase2App` over HTTP and the cloud
//! adapter is the production `ReqwestCloudApi`. Each member is a real
//! `SessionLifecycle`. The sidecar control channel is the checkpoint fixture
//! used by the other lifecycle tests, and the ROM is `FakeRom`, which applies
//! a `TradeCommit` with the same rules as `src/coop/trade_runtime.c`
//! (outgoing Pokémon found by personality and OT ID, the slot byte only a
//! hint; a repeat is acknowledged again without applying twice).
//!
//! What only the real ROM proves (see `test/coop/trade_commit.c`): the bridge
//! queue ordering of the acknowledgement before `CHECKPOINT_READY`, the field
//! control lock, checksum and mail validation of the record, and re-sending an
//! unconsumed acknowledgement after a same-epoch reconnect.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use coop_cloud::{
    AcquireLeaseRequest, ApiVersion, ArtifactIdentity, CharacterId, ClientInstanceId, CommitId,
    CreateGroupInvitationRequest, GroupId, GroupInvitationView, HeartbeatLeaseRequest,
    IdempotencyKey, InvitationCode, LeaseContract, LoginRequest, LoginResponse, LogoutRequest,
    LogoutResponse, OnlineAction, OnlineActionRequest, OnlineActionResponse, PartyPosition,
    Password, PrepareSnapshotRequest, ReconnectLeaseRequest, RefreshRequest, RefreshResponse,
    RegisterRequest, ReleaseLeaseRequest, Revision, SignedManifestEnvelope, SigningPrivateKey,
    SnapshotFinalizeRequest, SnapshotListRequest, SnapshotListResponse, SnapshotPrepareResponse,
    SnapshotRecord, SnapshotRestoreRequest, SnapshotRestoreResponse, TradeDecision,
    TradeDecisionRequest, TradeOfferRequest, TradeOfferView, TrustedManifestKey, UploadTarget,
};
use coop_protocol::{TradeCommitAppliedRecord, TradeCommitRecord};
use coop_server::{Phase2App, Phase2Config};
use coop_sidecar::control::{ControlCommand, ControlEvent};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

use super::{TestKeychain, compatibility, control_pair_with_generation, valid_save};
use crate::{
    AuthApi, AuthSession, CloudApi, EpochStore, RefreshTokenStore, ReqwestCloudApi, SessionConfig,
    SessionError, SessionLifecycle,
    auth::AuthFuture,
    ledger::{ExpectedDelta, LedgerEntryView, LedgerStatus},
    session::CloudFuture,
};

const PASSWORD: &str = "ledger e2e password";

/// Each test runs a real server whose password hashing is slow in debug
/// builds; running them one at a time keeps sign-in inside its timeout.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn key() -> IdempotencyKey {
    IdempotencyKey::new(Uuid::new_v4()).unwrap()
}

/// A checksum-valid party record (OT ID = personality leaves the substructs
/// unencrypted; a multiple of 24 keeps the growth substruct first). Byte 85
/// is `MAIL_NONE`, as `ZeroMonData` leaves it.
fn mon(personality: u32, species: u16) -> [u8; 100] {
    assert_eq!(personality % 24, 0);
    let mut raw = [0_u8; 100];
    raw[0..4].copy_from_slice(&personality.to_le_bytes());
    raw[4..8].copy_from_slice(&personality.to_le_bytes());
    raw[19] = 2;
    raw[28..30].copy_from_slice(&species.to_le_bytes());
    raw[32..34].copy_from_slice(&species.to_le_bytes());
    raw[85] = 0xff;
    raw
}

fn key_of(raw: &[u8; 100]) -> (u32, u32) {
    (
        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
        u32::from_le_bytes(raw[4..8].try_into().unwrap()),
    )
}

/// `valid_save(generation)` with `records` as the party of both slots.
fn party_save(generation: u32, records: &[[u8; 100]]) -> Vec<u8> {
    use coop_save::{LOGICAL_SECTOR_DATA_SIZES, SECTOR_SIZE, SECTORS_PER_SLOT, sector_checksum};
    let mut bytes = valid_save(generation);
    for slot in 0..2 {
        // The fixture stores logical sector N at physical sector N.
        let start = (slot * SECTORS_PER_SLOT + 1) * SECTOR_SIZE;
        let sector = &mut bytes[start..start + SECTOR_SIZE];
        sector[0x234] = u8::try_from(records.len()).unwrap();
        for (index, record) in records.iter().enumerate() {
            let offset = 0x238 + index * 100;
            sector[offset..offset + 100].copy_from_slice(record);
        }
        let checksum = sector_checksum(&sector[..LOGICAL_SECTOR_DATA_SIZES[1]]);
        sector[4086..4088].copy_from_slice(&checksum.to_le_bytes());
    }
    bytes
}

/// The ROM side of a `TradeCommit`, mirroring `CoopTradeRuntime`.
#[derive(Clone, Debug)]
struct FakeRom {
    party: Vec<[u8; 100]>,
    /// The header this boot applied (`applied_header`); cleared by a reboot.
    applied: Option<TradeCommitAppliedRecord>,
    applies: usize,
}

impl FakeRom {
    fn boot(party: &[[u8; 100]]) -> Self {
        Self {
            party: party.to_vec(),
            applied: None,
            applies: 0,
        }
    }

    /// Returns the 28-byte acknowledgement, or `None` for a rejected commit.
    fn receive(&mut self, record: &TradeCommitRecord) -> Option<TradeCommitAppliedRecord> {
        let header = record.applied();
        if self.applied == Some(header) {
            return Some(header);
        }
        let outgoing = (record.outgoing_personality, record.outgoing_ot_id);
        let hint = usize::from(record.slot);
        let slot = if self
            .party
            .get(hint)
            .is_some_and(|raw| key_of(raw) == outgoing)
        {
            Some(hint)
        } else {
            self.party.iter().position(|raw| key_of(raw) == outgoing)
        };
        match slot {
            Some(slot) => {
                self.party[slot] = record.incoming_record;
                self.applies += 1;
            }
            // A save that already holds the trade (reboot after applying).
            None if self.party.contains(&record.incoming_record) => {}
            None => return None,
        }
        self.applied = Some(header);
        Some(header)
    }
}

/// `ReqwestCloudApi` whose next finalize reaches the server but loses its
/// response, as a dropped connection after the commit would.
struct LossyCloud {
    inner: ReqwestCloudApi,
    lose_next_finalize: AtomicBool,
    lost: Mutex<Vec<SnapshotRecord>>,
    finalize_calls: Mutex<Vec<SnapshotFinalizeRequest>>,
}

impl LossyCloud {
    fn new(base: &str) -> Self {
        Self {
            inner: ReqwestCloudApi::new(base).unwrap(),
            lose_next_finalize: AtomicBool::new(false),
            lost: Mutex::new(Vec::new()),
            finalize_calls: Mutex::new(Vec::new()),
        }
    }
}

impl AuthApi for LossyCloud {
    fn register(&self, request: RegisterRequest) -> AuthFuture<'_, coop_cloud::RegisterResponse> {
        AuthApi::register(&self.inner, request)
    }
    fn login(&self, request: LoginRequest) -> AuthFuture<'_, LoginResponse> {
        AuthApi::login(&self.inner, request)
    }
    fn refresh(&self, request: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        AuthApi::refresh(&self.inner, request)
    }
    fn logout(&self, request: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
        AuthApi::logout(&self.inner, request)
    }
}

impl CloudApi for LossyCloud {
    fn ledger_open(
        &self,
        token: coop_cloud::AccessToken,
        character_id: CharacterId,
        fence: coop_cloud::LeaseFence,
    ) -> crate::ledger::LedgerFuture<'_, Option<LedgerEntryView>> {
        CloudApi::ledger_open(&self.inner, token, character_id, fence)
    }
    fn online_action(
        &self,
        token: coop_cloud::AccessToken,
        request: OnlineActionRequest,
    ) -> crate::online::OnlineFuture<'_, OnlineActionResponse> {
        CloudApi::online_action(&self.inner, token, request)
    }
    fn acquire<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: AcquireLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        CloudApi::acquire(&self.inner, auth, request)
    }
    fn heartbeat<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: HeartbeatLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        CloudApi::heartbeat(&self.inner, auth, request)
    }
    fn reconnect<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReconnectLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        CloudApi::reconnect(&self.inner, auth, request)
    }
    fn release<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: ReleaseLeaseRequest,
    ) -> CloudFuture<'a, LogoutResponse> {
        CloudApi::release(&self.inner, auth, request)
    }
    fn resume_package<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        revision: Revision,
    ) -> CloudFuture<'a, Option<SignedManifestEnvelope>> {
        CloudApi::resume_package(&self.inner, auth, character, revision)
    }
    fn artifact<'a>(
        &'a self,
        auth: &'a AuthSession,
        character: CharacterId,
        artifact: ArtifactIdentity,
        revision: Revision,
    ) -> CloudFuture<'a, Vec<u8>> {
        CloudApi::artifact(&self.inner, auth, character, artifact, revision)
    }
    fn list_snapshots<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotListRequest,
    ) -> CloudFuture<'a, SnapshotListResponse> {
        CloudApi::list_snapshots(&self.inner, auth, request)
    }
    fn restore<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotRestoreRequest,
    ) -> CloudFuture<'a, SnapshotRestoreResponse> {
        CloudApi::restore(&self.inner, auth, request)
    }
    fn prepare<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: PrepareSnapshotRequest,
    ) -> CloudFuture<'a, SnapshotPrepareResponse> {
        CloudApi::prepare(&self.inner, auth, request)
    }
    fn upload<'a>(&'a self, target: &'a UploadTarget, bytes: Vec<u8>) -> CloudFuture<'a, ()> {
        CloudApi::upload(&self.inner, target, bytes)
    }
    fn finalize<'a>(
        &'a self,
        auth: &'a AuthSession,
        request: SnapshotFinalizeRequest,
    ) -> CloudFuture<'a, SnapshotRecord> {
        Box::pin(async move {
            self.finalize_calls.lock().unwrap().push(request.clone());
            let result = CloudApi::finalize(&self.inner, auth, request).await;
            if self.lose_next_finalize.swap(false, Ordering::SeqCst) {
                self.lost.lock().unwrap().push(result?);
                return Err(SessionError::Cloud);
            }
            result
        })
    }
}

struct Server {
    base: String,
    app: Phase2App,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start() -> Self {
        // The launcher trusts key ID "test" with the [7; 32] signing key.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "test",
        )
        .unwrap()
        .with_upload_base_url(base.clone());
        let app = Phase2App::new(config).unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let router = app.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        Self {
            base,
            app,
            stop: Some(stop),
            task,
        }
    }

    async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = self.task.await;
    }
}

struct Member {
    name: String,
    root: TempDir,
    session: SessionLifecycle,
    ready_sequence: u32,
}

impl Member {
    fn character_id(&self) -> CharacterId {
        self.session.lease.character_id
    }

    fn token(&self) -> coop_cloud::AccessToken {
        self.session.auth.access_token().unwrap().clone()
    }

    /// Writes `party` as the next ROM save and runs one checkpoint through
    /// the real launcher finalize path.
    async fn checkpoint<A: CloudApi>(
        &mut self,
        api: &A,
        party: &[[u8; 100]],
    ) -> Result<Revision, SessionError> {
        let generation = self.session.save_generation.unwrap_or(0) + 1;
        let path = self.session.workspace.path().join("character.sav");
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("could not clear SAV: {error}"),
        }
        self.session
            .workspace
            .write_atomic("character.sav", &party_save(generation, party))
            .unwrap();
        self.ready_sequence += 1;
        let epoch = self.session.lease.session_epoch.value();
        let (mut control, sidecar) =
            control_pair_with_generation(epoch, self.ready_sequence, generation).await;
        let ready = ControlEvent::CheckpointReady {
            session_epoch: epoch,
            ready_sequence: self.ready_sequence,
        };
        let result = self.session.checkpoint(api, &mut control, ready).await;
        sidecar.await.unwrap();
        result
    }

    /// One session-loop turn after the idle ledger poll came due: poll and
    /// return the `TradeCommit` for this control generation, if any.
    async fn deliver<A: CloudApi>(
        &mut self,
        api: &A,
        generation: u32,
    ) -> Option<TradeCommitRecord> {
        // Stands in for `IDLE_POLL_INTERVAL` elapsing.
        self.session.ledger.request_poll();
        self.session.poll_ledger_if_due(api).await.unwrap();
        match self.session.take_ledger_delivery(generation) {
            Some(ControlCommand::TradeCommit {
                session_epoch,
                record,
            }) => {
                assert_eq!(session_epoch, self.session.lease.session_epoch.value());
                Some(record)
            }
            Some(other) => panic!("unexpected ledger delivery {other:?}"),
            None => None,
        }
    }

    fn acknowledge(&mut self, ack: TradeCommitAppliedRecord) {
        self.session.accept_trade_commit_applied(ack).unwrap();
    }

    async fn open_entry<A: CloudApi>(&self, api: &A) -> Option<LedgerEntryView> {
        api.ledger_open(
            self.token(),
            self.character_id(),
            self.session.lease.fence(),
        )
        .await
        .unwrap()
    }
}

async fn start_member<A: CloudApi>(api: &A, name: &str, root: TempDir) -> Member {
    let bridge = root.path().join("bridge");
    std::fs::create_dir_all(&bridge).unwrap();
    std::fs::write(bridge.join("generated_addresses.lua"), b"return {}\n").unwrap();
    let session = acquire(api, name, root.path()).await;
    Member {
        name: name.to_owned(),
        root,
        session,
        ready_sequence: 0,
    }
}

/// The test build compatibility, targeting the build identity the server pins
/// from `dist/bridge_manifest.json` so its resume packages verify.
fn server_compatibility() -> crate::BuildCompatibility {
    let dist: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../../dist/bridge_manifest.json")).unwrap();
    let mut compatibility = compatibility();
    compatibility.target = coop_cloud::CompatibilityTarget::new(
        coop_cloud::GameBuildId::new(dist["game_build"]["id"].as_str().unwrap()).unwrap(),
        coop_cloud::Sha256Digest::parse(dist["game_build"]["rom_sha256"].as_str().unwrap())
            .unwrap(),
        coop_cloud::MgbaVersion::new("0.10.5").unwrap(),
        coop_cloud::BridgeAbiVersion::new(
            u16::try_from(dist["net_bridge"]["abi_version"].as_u64().unwrap()).unwrap(),
        )
        .unwrap(),
        coop_cloud::ProtocolVersion::new(
            u16::try_from(
                dist["net_bridge"]["game_protocol_version"]
                    .as_u64()
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
        Revision::initial(),
    );
    compatibility
}

async fn acquire<A: CloudApi>(api: &A, name: &str, root: &Path) -> SessionLifecycle {
    let keychain: Arc<dyn RefreshTokenStore> = Arc::new(TestKeychain::default());
    let auth = AuthSession::login(
        api,
        keychain.as_ref(),
        name,
        Password::new(PASSWORD).unwrap(),
    )
    .await
    .unwrap();
    let config = SessionConfig {
        client_instance_id: ClientInstanceId::new(Uuid::new_v4()).unwrap(),
        manifest: server_compatibility(),
        trusted_manifest_key: TrustedManifestKey::new(
            "test",
            ed25519_dalek::SigningKey::from_bytes(&[7; 32])
                .verifying_key()
                .to_bytes(),
        )
        .unwrap(),
        epoch_store: EpochStore::new(root.join("epoch.json")),
        workspace_parent: root.join("sessions"),
        bridge_lua_dir: root.join("bridge"),
    };
    SessionLifecycle::acquire_with_keychain(api, auth, config, keychain)
        .await
        .unwrap()
}

/// Launcher restart: release the lease and sign in again from a new client.
async fn restart<A: CloudApi>(api: &A, member: Member) -> Member {
    let Member {
        name,
        root,
        session,
        ..
    } = member;
    session.release(api).await.unwrap();
    let session = acquire(api, &name, root.path()).await;
    Member {
        name,
        root,
        session,
        ready_sequence: 0,
    }
}

fn register(app: &Phase2App, name: &str) {
    app.add_invitation(name).unwrap();
    app.register(
        RegisterRequest::new(
            name,
            Password::new(PASSWORD).unwrap(),
            InvitationCode::new(name).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
}

async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
    base: &str,
    token: &coop_cloud::AccessToken,
    path: &str,
    body: &T,
) -> Result<R, (reqwest::StatusCode, serde_json::Value)> {
    let response = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .bearer_auth(token.expose_secret())
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    if status.is_success() {
        Ok(response.json().await.unwrap())
    } else {
        Err((status, response.json().await.unwrap_or_default()))
    }
}

/// Two players in one group, each at revision 1 with a two-Pokémon party.
struct World {
    server: Server,
    api: LossyCloud,
    one: Member,
    two: Member,
    group_id: GroupId,
}

impl World {
    async fn new(one_party: [[u8; 100]; 2], two_party: [[u8; 100]; 2]) -> Self {
        let server = Server::start().await;
        let api = LossyCloud::new(&server.base);
        register(&server.app, "ledgerone");
        register(&server.app, "ledgertwo");
        let mut one = start_member(&api, "ledgerone", tempdir().unwrap()).await;
        let mut two = start_member(&api, "ledgertwo", tempdir().unwrap()).await;
        assert_eq!(
            one.checkpoint(&api, &one_party).await.unwrap(),
            Revision::new(1)
        );
        assert_eq!(
            two.checkpoint(&api, &two_party).await.unwrap(),
            Revision::new(1)
        );

        let invitation: GroupInvitationView = post(
            &server.base,
            &one.token(),
            "/v1/groups/invitations",
            &CreateGroupInvitationRequest::new(
                one.session.lease.fence(),
                two.character_id(),
                key(),
            ),
        )
        .await
        .unwrap();
        let accepted = api
            .online_action(
                two.token(),
                OnlineActionRequest {
                    api_version: ApiVersion::V1,
                    fence: two.session.lease.fence(),
                    idempotency_key: key(),
                    action: OnlineAction::Accept {
                        invitation_id: invitation.invitation_id,
                    },
                },
            )
            .await
            .unwrap();
        let OnlineActionResponse::Accepted { group } = accepted else {
            panic!("group accepted");
        };
        Self {
            server,
            api,
            one,
            two,
            group_id: group.group_id,
        }
    }

    /// Player one offers its slot `own`, player two accepts with slot
    /// `partner`. Returns the decision result.
    async fn trade(
        &self,
        own: u8,
        partner: u8,
    ) -> Result<TradeOfferView, (reqwest::StatusCode, serde_json::Value)> {
        let offer: TradeOfferView = post(
            &self.server.base,
            &self.one.token(),
            &format!("/v1/groups/{}/trade-offers", self.group_id),
            &TradeOfferRequest {
                api_version: ApiVersion::V1,
                fence: self.one.session.lease.fence(),
                group_id: self.group_id,
                own_slot: PartyPosition::new(own).unwrap(),
                partner_slot: PartyPosition::new(partner).unwrap(),
                partner_expected_revision: self.two.session.revision,
                idempotency_key: key(),
            },
        )
        .await
        .unwrap();
        post(
            &self.server.base,
            &self.two.token(),
            &format!(
                "/v1/groups/{}/trade-offers/{}/decision",
                self.group_id, offer.offer_id
            ),
            &TradeDecisionRequest {
                api_version: ApiVersion::V1,
                fence: self.two.session.lease.fence(),
                offer_id: offer.offer_id,
                own_slot: PartyPosition::new(partner).unwrap(),
                partner_slot: PartyPosition::new(own).unwrap(),
                decision: TradeDecision::Accept,
                idempotency_key: key(),
            },
        )
        .await
    }

    async fn stop(self) {
        self.server.stop().await;
    }
}

fn parties() -> ([[u8; 100]; 2], [[u8; 100]; 2]) {
    ([mon(24, 1), mon(48, 2)], [mon(72, 3), mon(96, 4)])
}

fn commit_of(record: &TradeCommitRecord) -> CommitId {
    CommitId::new(Uuid::from_bytes(record.commit_id.0)).unwrap()
}

#[tokio::test]
async fn trade_is_issued_delivered_applied_declared_and_closed() {
    let _serial = SERIAL.lock().await;
    let (one_party, two_party) = parties();
    let mut world = World::new(one_party, two_party).await;
    assert_eq!(
        world.trade(0, 0).await.unwrap().status,
        coop_cloud::TradeOfferStatus::Accepted
    );

    // Ledger open: the entry names the partner's exact record.
    let record = world
        .one
        .deliver(&world.api, 1)
        .await
        .expect("trade commit");
    assert_eq!(record.slot, 0);
    assert_eq!(
        key_of(&one_party[0]),
        (record.outgoing_personality, record.outgoing_ot_id)
    );
    assert_eq!(record.incoming_record, two_party[0]);
    let open = world.one.open_entry(&world.api).await.expect("open entry");
    assert_eq!(open.commit_id, commit_of(&record));
    assert_eq!(open.status, LedgerStatus::Delivered);
    assert!(matches!(open.expected, ExpectedDelta::Trade { .. }));
    assert_eq!(
        world
            .one
            .session
            .workspace
            .read_fixed("pending_commits.json")
            .unwrap(),
        format!("[\"{}\"]", commit_of(&record)).into_bytes()
    );
    // Delivered once per control generation until acknowledged.
    assert!(world.one.deliver(&world.api, 1).await.is_none());

    let mut rom = FakeRom::boot(&one_party);
    let ack = rom.receive(&record).expect("ROM applies");
    assert_eq!(ack.slot, record.slot);
    world.one.acknowledge(ack);
    assert!(world.one.deliver(&world.api, 2).await.is_none());

    assert_eq!(
        world.one.checkpoint(&world.api, &rom.party).await.unwrap(),
        Revision::new(2)
    );
    let declared = world
        .api
        .finalize_calls
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(declared.last_applied_commit, Some(commit_of(&record)));
    assert_eq!(world.one.session.pending_applied_commit, None);
    assert_eq!(
        world
            .one
            .session
            .workspace
            .read_fixed("pending_commits.json")
            .unwrap(),
        b"[]"
    );
    // Applied: nothing is open or delivered any more.
    assert_eq!(world.one.open_entry(&world.api).await, None);
    assert!(world.one.deliver(&world.api, 3).await.is_none());
    assert_eq!(rom.applies, 1);

    // The partner's own entry is independent and still open.
    let partner = world
        .two
        .open_entry(&world.api)
        .await
        .expect("partner entry");
    assert_ne!(partner.commit_id, commit_of(&record));
    world.stop().await;
}

#[tokio::test]
async fn lost_acknowledgement_is_redelivered_and_reacknowledged_before_finalize() {
    let _serial = SERIAL.lock().await;
    let (one_party, two_party) = parties();
    let mut world = World::new(one_party, two_party).await;
    world.trade(0, 0).await.unwrap();

    // Same launcher, new control generation (ROM reboot or sidecar restart)
    // after the ROM applied but before the launcher saw the ack.
    let record = world.one.deliver(&world.api, 1).await.unwrap();
    let mut rom = FakeRom::boot(&one_party);
    let _lost = rom.receive(&record).unwrap();
    let redelivered = world.one.deliver(&world.api, 2).await.expect("redelivery");
    assert_eq!(redelivered, record);
    // Same boot: the remembered header answers without applying twice.
    let ack = rom.receive(&redelivered).unwrap();
    // After a reboot from a save that already holds the trade, party
    // contents prove it and the ROM acknowledges again.
    let mut rebooted = FakeRom::boot(&rom.party);
    assert_eq!(rebooted.receive(&redelivered), Some(ack));
    assert_eq!((rom.applies, rebooted.applies), (1, 0));
    assert_eq!(rebooted.party, rom.party);

    // The launcher restarts before any finalize: the acknowledgement it saw
    // is gone with it. The open entry is delivered again from GET
    // ledger/open to the ROM, which boots from the restored revision 1.
    world.one.acknowledge(ack);
    world.one = restart(&world.api, world.one).await;
    assert_eq!(world.one.session.revision, Revision::new(1));
    assert_eq!(world.one.session.pending_applied_commit, None);
    let after_restart = world.one.deliver(&world.api, 1).await.expect("redelivery");
    assert_eq!(after_restart, record);
    let mut restored = FakeRom::boot(&one_party);
    let ack = restored.receive(&after_restart).unwrap();
    world.one.acknowledge(ack);
    assert_eq!(
        world
            .one
            .checkpoint(&world.api, &restored.party)
            .await
            .unwrap(),
        Revision::new(2)
    );
    assert_eq!(
        world
            .api
            .finalize_calls
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .last_applied_commit,
        Some(commit_of(&record))
    );
    assert_eq!(world.one.open_entry(&world.api).await, None);
    world.stop().await;
}

#[tokio::test]
async fn lost_finalize_response_is_replayed_without_applying_twice() {
    let _serial = SERIAL.lock().await;
    let (one_party, two_party) = parties();
    let mut world = World::new(one_party, two_party).await;
    world.trade(0, 0).await.unwrap();
    let record = world.one.deliver(&world.api, 1).await.unwrap();
    let mut rom = FakeRom::boot(&one_party);
    let ack = rom.receive(&record).unwrap();
    world.one.acknowledge(ack);

    let before = world.api.finalize_calls.lock().unwrap().len();
    world.api.lose_next_finalize.store(true, Ordering::SeqCst);
    assert_eq!(
        world.one.checkpoint(&world.api, &rom.party).await.unwrap(),
        Revision::new(2)
    );
    let calls = world.api.finalize_calls.lock().unwrap()[before..].to_vec();
    // The exact request was sent twice and the server returned the record it
    // stored for the first one.
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(calls[0].last_applied_commit, Some(commit_of(&record)));
    let lost = world.api.lost.lock().unwrap().clone();
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].snapshot_id, calls[0].snapshot_id);
    assert_eq!(lost[0].revision, Revision::new(2));
    assert_eq!(world.one.session.revision, Revision::new(2));
    assert_eq!(world.one.open_entry(&world.api).await, None);

    // The next ordinary save is revision 3; the applied entry is not open
    // for a second declaration.
    assert_eq!(
        world.one.checkpoint(&world.api, &rom.party).await.unwrap(),
        Revision::new(3)
    );
    assert_eq!(
        world
            .api
            .finalize_calls
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .last_applied_commit,
        None
    );
    world.stop().await;
}

#[tokio::test]
async fn pre_apply_save_is_accepted_and_undeclared_evidence_is_rejected() {
    let _serial = SERIAL.lock().await;
    let (one_party, two_party) = parties();
    let mut world = World::new(one_party, two_party).await;
    world.trade(0, 0).await.unwrap();
    let record = world.one.deliver(&world.api, 1).await.unwrap();

    // A save taken before the ROM applied the commit is ordinary progress,
    // and the entry stays open.
    assert_eq!(
        world.one.checkpoint(&world.api, &one_party).await.unwrap(),
        Revision::new(2)
    );
    assert_eq!(
        world
            .api
            .finalize_calls
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .last_applied_commit,
        None
    );
    let open = world.one.open_entry(&world.api).await.expect("still open");
    assert_eq!(open.commit_id, commit_of(&record));

    // The ROM applied, but the save that shows it declares nothing.
    let mut rom = FakeRom::boot(&one_party);
    rom.receive(&record).unwrap();
    assert!(world.one.checkpoint(&world.api, &rom.party).await.is_err());
    assert_eq!(world.one.session.revision, Revision::new(2));
    let open = world.one.open_entry(&world.api).await.expect("still open");
    assert_eq!(open.commit_id, commit_of(&record));
    world.stop().await;
}

#[tokio::test]
async fn reordered_party_applies_to_the_moved_slot_and_finalizes() {
    let _serial = SERIAL.lock().await;
    let (one_party, two_party) = parties();
    let mut world = World::new(one_party, two_party).await;
    world.trade(0, 0).await.unwrap();
    let record = world.one.deliver(&world.api, 1).await.unwrap();
    assert_eq!(record.slot, 0);

    // The player swapped the two party slots before the commit arrived.
    let mut rom = FakeRom::boot(&[one_party[1], one_party[0]]);
    let ack = rom.receive(&record).unwrap();
    assert_eq!(rom.party, vec![one_party[1], two_party[0]]);
    // The acknowledgement still carries the original slot hint.
    assert_eq!(ack, record.applied());
    world.one.acknowledge(ack);
    assert_eq!(
        world.one.checkpoint(&world.api, &rom.party).await.unwrap(),
        Revision::new(2)
    );
    assert_eq!(world.one.open_entry(&world.api).await, None);
    world.stop().await;
}

#[tokio::test]
async fn trade_offering_a_pokemon_with_mail_is_refused_at_issuance() {
    let _serial = SERIAL.lock().await;
    let (one_party, mut two_party) = parties();
    two_party[0][85] = 0; // mail index 0
    let world = World::new(one_party, two_party).await;
    let (status, body) = world.trade(0, 0).await.unwrap_err();
    assert_eq!(status, reqwest::StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "trade_pokemon_holds_mail");
    assert_eq!(world.one.open_entry(&world.api).await, None);
    assert_eq!(world.two.open_entry(&world.api).await, None);
    world.stop().await;
}

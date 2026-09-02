//! Launcher/sidecar integration proof for the local Phase 2 checkpoint path.
//!
//! The cloud is a deterministic injectable adapter, while the sidecar and both
//! wire protocols are real.  The test intentionally drives the ROM-facing
//! bridge with validated `BridgeFrame`s instead of introducing test DTOs.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use coop_cloud::{
    AccessToken, ApiVersion, BridgeAbiVersion, CharacterId, ClientInstanceId, CompatibilityTarget,
    GameBuildId, HeartbeatLeaseRequest, LeaseContract, LeaseFence, LoginRequest, LoginResponse,
    LogoutRequest, LogoutResponse, MgbaVersion, Password, PrepareSnapshotRequest, ProtocolVersion,
    RefreshFamilyId, RefreshRequest, RefreshResponse, RefreshToken, Revision, SessionEpoch,
    SessionId, Sha256Digest, SnapshotFence, SnapshotFinalizeRequest, SnapshotListRequest,
    SnapshotListResponse, SnapshotPrepareResponse, SnapshotRestoreRequest, SnapshotRestoreResponse,
    TrustedManifestKey, UnixTimestampMillis, UploadTarget, UserId,
};
use coop_launcher::{
    AuthApi, AuthError, AuthSession, BuildCompatibility, CloudApi, ControlChannel, EpochStore,
    KeychainError, RefreshTokenStore, SessionConfig, SessionError, SessionLifecycle,
    auth::AuthFuture, session::CloudFuture,
};
use coop_save::{
    COOP_SAVE_OFFSET, COOP_SAVE_V1_MAGIC, COOP_SAVE_V1_SCHEMA_VERSION, COOP_SAVE_V1_SIZE,
    SAVE_BLOCK3_CAPACITY, SAVE_BLOCK3_CHUNK_OFFSET, SAVE_BLOCK3_CHUNK_SIZE, SECTOR_SIZE,
    SECTORS_PER_SLOT, sector_checksum,
};
use coop_sidecar::control::ControlEvent;
use coop_sidecar::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BridgeFrame, Direction, GAME_PROTOCOL_VERSION,
    HANDSHAKE_ACCEPTED_LINE, LocalSidecar, MessageType,
};
use tempfile::{TempDir, tempdir};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinHandle,
    time::{Duration, timeout},
};
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn valid_character_save(generation: u32) -> Vec<u8> {
    let mut payload = [0_u8; COOP_SAVE_V1_SIZE];
    write_u32(&mut payload, 0, COOP_SAVE_V1_MAGIC);
    write_u16(&mut payload, 4, COOP_SAVE_V1_SCHEMA_VERSION);
    write_u16(
        &mut payload,
        6,
        u16::try_from(COOP_SAVE_V1_SIZE).expect("frozen save ABI fits u16"),
    );
    write_u32(&mut payload, 8, 1);
    payload[12..28].copy_from_slice(&[
        0x43, 0x91, 0x88, 0x33, 0xde, 0xc6, 0x46, 0xd6, 0xa5, 0x83, 0xd1, 0x24, 0x68, 0x6c, 0x85,
        0x40,
    ]);
    write_u32(&mut payload, 28, generation);
    // The four records are ordered by the frozen protocol ABI.
    for (index, region) in [1_u8, 2, 3, 4].into_iter().enumerate() {
        let offset = 36 + index * 8;
        payload[offset] = region;
    }
    let payload_crc = crc32(&payload[..668]);
    write_u32(&mut payload, 668, payload_crc);

    let mut save_block3 = [0xff_u8; SAVE_BLOCK3_CAPACITY];
    save_block3[COOP_SAVE_OFFSET..COOP_SAVE_OFFSET + COOP_SAVE_V1_SIZE].copy_from_slice(&payload);
    let mut bytes = vec![0xff_u8; 128 * 1024];
    for (slot, counter) in [(0_usize, 0_u32), (1, 1)] {
        let base = slot * SECTORS_PER_SLOT * SECTOR_SIZE;
        for logical in 0..SECTORS_PER_SLOT {
            let offset = base + logical * SECTOR_SIZE;
            let sector = &mut bytes[offset..offset + SECTOR_SIZE];
            sector.fill(0);
            let chunk = logical * SAVE_BLOCK3_CHUNK_SIZE;
            sector[SAVE_BLOCK3_CHUNK_OFFSET..SAVE_BLOCK3_CHUNK_OFFSET + SAVE_BLOCK3_CHUNK_SIZE]
                .copy_from_slice(&save_block3[chunk..chunk + SAVE_BLOCK3_CHUNK_SIZE]);
            write_u16(
                sector,
                4084,
                u16::try_from(logical).expect("fixture logical ID fits u16"),
            );
            write_u16(
                sector,
                4086,
                sector_checksum(&sector[..coop_save::LOGICAL_SECTOR_DATA_SIZES[logical]]),
            );
            write_u32(sector, 4088, 0x0801_2025);
            write_u32(sector, 4092, counter);
        }
    }
    bytes
}

const SESSION_EPOCH: u32 = 7;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

struct SidecarTaskGuard {
    handle: Option<JoinHandle<Result<(), coop_sidecar::SidecarError>>>,
}

impl SidecarTaskGuard {
    async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for SidecarTaskGuard {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

#[derive(Default)]
struct TestKeychain {
    token: Mutex<Option<RefreshToken>>,
}

impl RefreshTokenStore for TestKeychain {
    fn load(&self, _service: &str, _username: &str) -> Result<Option<RefreshToken>, KeychainError> {
        Ok(self.token.lock().expect("keychain lock").clone())
    }

    fn store(
        &self,
        _service: &str,
        _username: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError> {
        *self.token.lock().expect("keychain lock") = Some(token.clone());
        Ok(())
    }

    fn delete(&self, _service: &str, _username: &str) -> Result<(), KeychainError> {
        *self.token.lock().expect("keychain lock") = None;
        Ok(())
    }
}

struct FakeCloud {
    lease: LeaseContract,
    login: LoginResponse,
    calls: Mutex<Vec<String>>,
    uploads: Mutex<Vec<(coop_cloud::ArtifactIdentity, Vec<u8>)>>,
}

impl FakeCloud {
    fn record(&self, call: impl Into<String>) {
        self.calls.lock().expect("cloud lock").push(call.into());
    }
}

impl AuthApi for FakeCloud {
    fn login(&self, _request: LoginRequest) -> AuthFuture<'_, LoginResponse> {
        self.record("login");
        let response = self.login.clone();
        Box::pin(async move { Ok(response) })
    }

    fn refresh(&self, _request: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }

    fn logout(&self, _request: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
        self.record("logout");
        Box::pin(async { Ok(LogoutResponse::default()) })
    }
}

impl CloudApi for FakeCloud {
    fn acquire<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _request: coop_cloud::AcquireLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        self.record("acquire");
        let lease = self.lease;
        Box::pin(async move { Ok(lease) })
    }

    fn heartbeat<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _request: HeartbeatLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        self.record("heartbeat");
        let lease = self.lease;
        Box::pin(async move { Ok(lease) })
    }

    fn reconnect<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _request: coop_cloud::ReconnectLeaseRequest,
    ) -> CloudFuture<'a, LeaseContract> {
        self.record("reconnect");
        let lease = self.lease;
        Box::pin(async move { Ok(lease) })
    }

    fn release<'a>(
        &'a self,
        _auth: &'a AuthSession,
        request: coop_cloud::ReleaseLeaseRequest,
    ) -> CloudFuture<'a, LogoutResponse> {
        self.record(format!(
            "release:{}:{}:{}",
            request.session_id,
            request.session_epoch.value(),
            request.current_revision.value()
        ));
        Box::pin(async { Ok(LogoutResponse::default()) })
    }

    fn resume_package<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _character: CharacterId,
        _revision: Revision,
    ) -> CloudFuture<'a, Option<coop_cloud::SignedManifestEnvelope>> {
        self.record("resume_package");
        Box::pin(async { Ok(None) })
    }

    fn artifact<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _character: CharacterId,
        _artifact: coop_cloud::ArtifactIdentity,
        _revision: Revision,
    ) -> CloudFuture<'a, Vec<u8>> {
        Box::pin(async { Err(SessionError::ArtifactNotFound) })
    }

    fn list_snapshots<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _request: SnapshotListRequest,
    ) -> CloudFuture<'a, SnapshotListResponse> {
        Box::pin(async { Err(SessionError::Cloud) })
    }

    fn restore<'a>(
        &'a self,
        _auth: &'a AuthSession,
        _request: SnapshotRestoreRequest,
    ) -> CloudFuture<'a, SnapshotRestoreResponse> {
        Box::pin(async { Err(SessionError::Cloud) })
    }

    fn prepare<'a>(
        &'a self,
        _auth: &'a AuthSession,
        request: PrepareSnapshotRequest,
    ) -> CloudFuture<'a, SnapshotPrepareResponse> {
        self.record("prepare");
        let files = request.files.clone();
        let targets = files
            .iter()
            .map(|file| {
                UploadTarget::new_put(
                    file.artifact,
                    "http://127.0.0.1:9/upload?capability=phase2-test",
                    UnixTimestampMillis::new(4_000_000_000_000),
                )
                .expect("valid test capability")
            })
            .collect();
        let response = SnapshotPrepareResponse {
            api_version: ApiVersion::V1,
            snapshot_id: request.snapshot_id,
            expected_parent_revision: request.expected_parent_revision,
            next_revision: request
                .expected_parent_revision
                .next()
                .expect("revision room"),
            session_epoch: request.session_epoch,
            idempotency_key: request.idempotency_key,
            files,
            pending_commits_sha256: request.pending_commits_sha256,
            upload_targets: targets,
        };
        Box::pin(async move { Ok(response) })
    }

    fn upload<'a>(&'a self, target: &'a UploadTarget, bytes: Vec<u8>) -> CloudFuture<'a, ()> {
        self.record(format!(
            "upload:{}:{}",
            target.artifact.as_str(),
            bytes.len()
        ));
        self.uploads
            .lock()
            .expect("upload lock")
            .push((target.artifact, bytes));
        Box::pin(async { Ok(()) })
    }

    fn finalize<'a>(
        &'a self,
        _auth: &'a AuthSession,
        request: SnapshotFinalizeRequest,
    ) -> CloudFuture<'a, coop_cloud::SnapshotRecord> {
        self.record("finalize");
        let record = coop_cloud::SnapshotRecord::new(
            request.snapshot_id,
            SnapshotFence::new(
                request.session_id,
                request.character_id,
                request.session_epoch,
            ),
            request.expected_parent_revision,
            request.revision,
            request.files,
            request.pending_commits_sha256,
            request.last_applied_commit,
            UnixTimestampMillis::new(4_000_000_000_000),
        )
        .expect("valid test record");
        Box::pin(async move { Ok(record) })
    }
}

fn fixed_ids() -> (
    CharacterId,
    SessionId,
    ClientInstanceId,
    UserId,
    RefreshFamilyId,
) {
    (
        CharacterId::new(Uuid::from_u128(0x101)).expect("character"),
        SessionId::new(Uuid::from_u128(0x102)).expect("session"),
        ClientInstanceId::new(Uuid::from_u128(0x103)).expect("client"),
        UserId::new(Uuid::from_u128(0x104)).expect("user"),
        RefreshFamilyId::new(Uuid::from_u128(0x105)).expect("family"),
    )
}

fn compatibility() -> BuildCompatibility {
    let manifest = serde_json::from_value(serde_json::json!({
        "schema_version": 2,
        "game_build": {
            "id": "pokeemerald-coop",
            "numeric_id": 65536,
            "rom_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
        },
        "net_bridge": {
            "symbol": "gCoopNetBridge",
            "address": 33_554_432,
            "size": 9244,
            "magic": 1_347_111_759,
            "abi_version": 1,
            "game_protocol_version": 1,
            "byte_order": "little",
            "checksum": {"algorithm": "CRC-32/IEEE", "covered_bytes": [0, 139], "stored_offset": 140},
            "offsets": {"magic": 0, "abi_version": 4, "game_protocol_version": 6, "game_build_id": 8, "status_flags": 12, "last_sidecar_heartbeat": 16, "game_to_network": 20, "network_to_game": 4632},
            "queue": {"capacity": 32, "size": 4612, "read_index_offset": 0, "write_index_offset": 2, "entries_offset": 4},
            "message": {"size": 144, "payload_size": 128, "offsets": {"type": 0, "length": 2, "sequence": 4, "session_epoch": 8, "payload": 12, "checksum": 140}}
        },
        "save": {
            "block3_address": 33_554_432,
            "coop_offset": 4,
            "generation_offset": 28,
            "generation_address": 33_554_464,
            "crc_offset": 668,
            "schema_version": 1,
            "struct_size": 672,
            "registry_version": 1,
            "registry_digest": "43918833dec646d6a583d124686c8540"
        }
    }))
    .expect("bridge manifest");
    BuildCompatibility {
        target: CompatibilityTarget::new(
            GameBuildId::new("pokeemerald-coop").expect("build"),
            Sha256Digest::of_bytes(b"rom"),
            MgbaVersion::new("0.10.5").expect("mGBA"),
            BridgeAbiVersion::new(1).expect("ABI"),
            ProtocolVersion::new(1).expect("protocol"),
            Revision::initial(),
        ),
        manifest,
        rom_path: "synthetic.gba".into(),
        mgba_path: "synthetic-mgba".into(),
    }
}

async fn fixture() -> TestResult<(TempDir, SessionLifecycle, Arc<FakeCloud>)> {
    let root = tempdir()?;
    let bridge_dir = root.path().join("bridge");
    std::fs::create_dir_all(&bridge_dir)?;
    std::fs::write(bridge_dir.join("generated_addresses.lua"), b"return {}\n")?;
    let (character, session_id, client, user, family) = fixed_ids();
    let lease = LeaseContract::new(
        LeaseFence::new(
            session_id,
            character,
            Revision::initial(),
            SessionEpoch::new(SESSION_EPOCH).expect("epoch"),
            client,
        ),
        UnixTimestampMillis::new(4_000_000_000_000),
        1,
    )?;
    let login = LoginResponse::new(
        user,
        character,
        AccessToken::new("phase2-access")?,
        RefreshToken::new("phase2-refresh")?,
        family,
        UnixTimestampMillis::new(4_000_000_000_000),
        UnixTimestampMillis::new(4_000_000_100_000),
    )?;
    let cloud = Arc::new(FakeCloud {
        lease,
        login,
        calls: Mutex::new(Vec::new()),
        uploads: Mutex::new(Vec::new()),
    });
    let keychain: Arc<dyn RefreshTokenStore> = Arc::new(TestKeychain::default());
    let auth = AuthSession::login(
        cloud.as_ref(),
        keychain.as_ref(),
        "SmokeUser",
        Password::new("phase2-local-password")?,
    )
    .await?;
    let config = SessionConfig {
        client_instance_id: client,
        manifest: compatibility(),
        trusted_manifest_key: TrustedManifestKey::new(
            "phase2-test-key",
            ed25519_dalek::SigningKey::from_bytes(&[7; 32])
                .verifying_key()
                .to_bytes(),
        )?,
        epoch_store: EpochStore::new(root.path().join("epoch.json")),
        workspace_parent: root.path().join("sessions"),
        bridge_lua_dir: bridge_dir,
    };
    let session =
        SessionLifecycle::acquire_with_keychain(cloud.as_ref(), auth, config, keychain).await?;
    assert!(!session.workspace.path().join("character.sav").exists());
    assert_eq!(
        std::fs::read(session.workspace.path().join("pending_commits.json"))?,
        b"[]"
    );
    Ok((root, session, cloud))
}

fn deadline_remaining(deadline: std::time::Instant) -> Duration {
    deadline.saturating_duration_since(std::time::Instant::now())
}

async fn bridge_handshake(
    stream: &mut TcpStream,
    descriptor: &coop_sidecar::SessionDescriptor,
    deadline: std::time::Instant,
) -> TestResult<BridgeFrame> {
    let handshake = format!(
        "{{\"secret\":\"{}\",\"bridge_abi\":{},\"protocol_version\":{}}}\n",
        descriptor.secret(),
        BRIDGE_ABI_VERSION,
        GAME_PROTOCOL_VERSION
    );
    timeout(
        deadline_remaining(deadline),
        stream.write_all(handshake.as_bytes()),
    )
    .await??;
    let mut accepted = vec![0_u8; HANDSHAKE_ACCEPTED_LINE.len()];
    timeout(
        deadline_remaining(deadline),
        stream.read_exact(&mut accepted),
    )
    .await??;
    assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
    read_sidecar_frame(stream, deadline).await
}

async fn read_sidecar_frame(
    stream: &mut TcpStream,
    deadline: std::time::Instant,
) -> TestResult<BridgeFrame> {
    let mut bytes = [0_u8; BRIDGE_FRAME_SIZE];
    timeout(deadline_remaining(deadline), stream.read_exact(&mut bytes)).await??;
    Ok(BridgeFrame::decode_for(&bytes, Direction::SidecarToRom)?)
}

async fn send_rom_frame(
    stream: &mut TcpStream,
    frame: &BridgeFrame,
    deadline: std::time::Instant,
) -> TestResult<()> {
    timeout(
        deadline_remaining(deadline),
        stream.write_all(&frame.encode()),
    )
    .await??;
    Ok(())
}

#[tokio::test]
// Keep the lifecycle, wire handoff, and cloud assertions in one readable
// sequence so the integration boundary remains auditable.
#[allow(clippy::too_many_lines)]
async fn phase2_launcher_sidecar_checkpoint_smoke() -> TestResult<()> {
    let (_root, mut lifecycle, cloud) = fixture().await?;
    lifecycle.heartbeat(cloud.as_ref()).await?;

    let sidecar = LocalSidecar::bind_with_epoch(SESSION_EPOCH).await?;
    let descriptor = sidecar.session_descriptor();
    assert_eq!(descriptor.bridge().host(), "127.0.0.1");
    assert_eq!(descriptor.control().host(), "127.0.0.1");
    if descriptor.secret() == descriptor.control_secret() {
        return Err("bridge and control secrets must be independent".into());
    }
    let sidecar_task = SidecarTaskGuard {
        handle: Some(tokio::spawn(sidecar.serve())),
    };

    // Control authentication is intentionally completed before the ROM bridge
    // is connected, matching the launcher's control-first lifecycle.
    let control_deadline = std::time::Instant::now() + IO_TIMEOUT;
    let mut control = timeout(
        deadline_remaining(control_deadline),
        ControlChannel::connect(&descriptor),
    )
    .await??;

    // Exercise the same production orchestration helper used by
    // ProcessSupervisor::start_with_bridge. Control authentication succeeds
    // before any bridge credential is materialized for mGBA.
    let session_lua_path = lifecycle.workspace.path().join("session.lua");
    let bridge_source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../bridge")
        .canonicalize()?;
    coop_launcher::materialize_bridge_session(
        &lifecycle.workspace,
        &bridge_source,
        &descriptor,
        SESSION_EPOCH,
    )?;
    let session_lua = std::fs::read_to_string(&session_lua_path)?;
    assert!(session_lua.contains("host = \"127.0.0.1\""));
    assert!(session_lua.contains(&format!("port = {}", descriptor.bridge().port())));
    assert!(session_lua.contains(descriptor.secret()));
    assert!(!session_lua.contains(descriptor.control_secret()));
    assert_eq!(
        std::fs::read(lifecycle.workspace.path().join("main.lua"))?,
        std::fs::read(bridge_source.join("main.lua"))?
    );

    let bridge_deadline = std::time::Instant::now() + IO_TIMEOUT;
    let mut bridge = timeout(
        deadline_remaining(bridge_deadline),
        TcpStream::connect(descriptor.address()),
    )
    .await??;
    let initial_ready = bridge_handshake(
        &mut bridge,
        &descriptor,
        std::time::Instant::now() + IO_TIMEOUT,
    )
    .await?;
    assert_eq!(initial_ready.message_type(), MessageType::SessionReady);
    assert_eq!(initial_ready.session_epoch(), SESSION_EPOCH);

    let rom_ready_deadline = std::time::Instant::now() + IO_TIMEOUT;
    send_rom_frame(
        &mut bridge,
        &BridgeFrame::new(MessageType::RomReady, 1, SESSION_EPOCH, &[])?,
        rom_ready_deadline,
    )
    .await?;
    let rom_ready_ack = read_sidecar_frame(&mut bridge, rom_ready_deadline).await?;
    assert_eq!(rom_ready_ack.message_type(), MessageType::SessionReady);
    assert_eq!(rom_ready_ack.session_epoch(), SESSION_EPOCH);

    let checkpoint_ready_deadline = std::time::Instant::now() + IO_TIMEOUT;
    send_rom_frame(
        &mut bridge,
        &BridgeFrame::new(MessageType::CheckpointReady, 2, SESSION_EPOCH, &[])?,
        checkpoint_ready_deadline,
    )
    .await?;
    let ready = timeout(
        deadline_remaining(checkpoint_ready_deadline),
        control.receive(),
    )
    .await??;
    assert_eq!(
        ready,
        ControlEvent::CheckpointReady {
            session_epoch: SESSION_EPOCH,
            ready_sequence: 2,
        }
    );

    let sav_path = lifecycle.workspace.path().join("character.sav");
    let revision = {
        let checkpoint_deadline = std::time::Instant::now() + IO_TIMEOUT;
        timeout(
            deadline_remaining(checkpoint_deadline),
            async {
                let checkpoint_result: TestResult<Revision> = async {
                    let checkpoint = lifecycle.checkpoint(cloud.as_ref(), &mut control, ready);
                    tokio::pin!(checkpoint);
                    let revision = loop {
                        tokio::select! {
                            result = &mut checkpoint => break result?,
                            frame = read_sidecar_frame(&mut bridge, checkpoint_deadline) => {
                                let frame = frame?;
                                assert_eq!(frame.message_type(), MessageType::CheckpointGranted);
                                assert_eq!(frame.session_epoch(), SESSION_EPOCH);
                                assert!(frame.payload().is_empty());
                                // This direct write is the deterministic mGBA/SAV seam;
                                // the lifecycle reads the same private fixed path below.
                                std::fs::write(&sav_path, valid_character_save(1))?;
                                send_rom_frame(
                                    &mut bridge,
                                    &BridgeFrame::new(MessageType::SaveDataUpdated, 3, SESSION_EPOCH, &1_u32.to_le_bytes())?,
                                    checkpoint_deadline,
                                ).await?;
                            }
                        }
                    };
                    Ok(revision)
                }.await;
                checkpoint_result
            },
        ).await??
    };
    assert_eq!(revision, Revision::new(1));
    assert_eq!(lifecycle.revision, revision);
    assert_eq!(std::fs::read(&sav_path)?, valid_character_save(1));
    let calls = cloud.calls.lock().expect("cloud lock").clone();
    assert_eq!(calls[0], "login");
    assert!(calls.iter().any(|call| call == "acquire"));
    assert!(calls.iter().any(|call| call == "heartbeat"));
    assert!(calls.iter().any(|call| call == "prepare"));
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("upload:character.sav"))
    );
    assert!(calls.iter().any(|call| call == "finalize"));
    assert_eq!(
        cloud.uploads.lock().expect("upload lock").as_slice(),
        &[
            (
                coop_cloud::ArtifactIdentity::CharacterSav,
                valid_character_save(1),
            ),
            (coop_cloud::ArtifactIdentity::PendingCommits, b"[]".to_vec(),),
        ]
    );

    drop(control);
    drop(bridge);
    sidecar_task.shutdown().await;
    lifecycle.release(cloud.as_ref()).await?;
    assert!(
        cloud
            .calls
            .lock()
            .expect("cloud lock")
            .iter()
            .any(|call| call == "release:00000000-0000-0000-0000-000000000102:7:1")
    );
    Ok(())
}

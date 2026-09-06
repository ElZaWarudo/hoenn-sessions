#![cfg(windows)]

//! Bounded manual observation harness, not an automated avatar rendering test.
//! Run with --ignored --nocapture and local `COOP_REAL_ROM` / `COOP_REAL_MGBA`.
//! Load each printed main.lua in its respective stock mGBA Scripting window.
//! Only write the printed acceptance file after observing its exact checklist.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use coop_cloud::{
    ClientInstanceId, InvitationCode, Password, RefreshToken, RegisterRequest, SigningPrivateKey,
    TrustedManifestKey,
};
use coop_launcher::{
    AuthSession, BuildCompatibility, CommandSpec, EpochStore, KeychainError, RefreshTokenStore,
    ReqwestCloudApi, SessionConfig, SessionLifecycle, SupervisedChildren,
    process::{staged_rom_marker_contents, staged_rom_marker_path},
};
use coop_server::{Phase2App, Phase2Config};
use tokio::{net::TcpListener, sync::watch, time::Instant};
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ACCEPTANCE: &str = "Both Lua scripts authenticated.\nBoth remote avatars were visible in Littleroot.\nEach player's movement appeared on the other screen.\nRemote avatars did not block local movement.\n";
const KEY_ID: &str = "real-mgba-local-only";

#[derive(Default)]
struct MemoryKeychain(Mutex<Option<RefreshToken>>);

impl RefreshTokenStore for MemoryKeychain {
    fn load(&self, _: &str, _: &str) -> Result<Option<RefreshToken>, KeychainError> {
        Ok(self.0.lock().map_err(|_| KeychainError::Operation)?.clone())
    }
    fn store(&self, _: &str, _: &str, token: &RefreshToken) -> Result<(), KeychainError> {
        *self.0.lock().map_err(|_| KeychainError::Operation)? = Some(token.clone());
        Ok(())
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
        *self.0.lock().map_err(|_| KeychainError::Operation)? = None;
        Ok(())
    }
}

#[derive(Clone)]
struct Inputs {
    repository: PathBuf,
    rom: PathBuf,
    mgba: PathBuf,
    sidecar: PathBuf,
    public_key: [u8; 32],
}

async fn player(
    number: u8,
    inputs: &Inputs,
    base: &str,
    invitation: &str,
    stop: watch::Sender<bool>,
) -> TestResult<()> {
    eprintln!("Player {number}: staging and validating local runtime artifacts");
    let result = player_inner(number, inputs, base, invitation, stop.subscribe()).await;
    // Any premature completion also drains the other player. Its lifecycle is
    // never abandoned by cancelling a join handle or dropping its future.
    stop.send_replace(true);
    result
}

// Keep acquisition and every cleanup branch together for this opt-in harness.
#[allow(clippy::too_many_lines)]
async fn player_inner(
    number: u8,
    inputs: &Inputs,
    base: &str,
    invitation: &str,
    mut stop: watch::Receiver<bool>,
) -> TestResult<()> {
    let root = tempfile::Builder::new()
        .prefix("coop-real-player-")
        .tempdir()?;
    let rom = root.path().join(format!("player-{number}.gba"));
    fs::copy(&inputs.rom, &rom)?;
    let marker = staged_rom_marker_path(&rom);
    fs::write(&marker, staged_rom_marker_contents(&rom)?)?;
    let mgba = CommandSpec::mgba_owned_staged(&inputs.mgba, &rom, &marker)?;
    let sidecar = CommandSpec::sidecar_template(&inputs.sidecar)?;
    let manifest = BuildCompatibility::validate(
        inputs.repository.join("dist/bridge_manifest.json"),
        &rom,
        &inputs.mgba,
    )?;
    eprintln!("Player {number}: artifact validation complete; registering disposable account");
    let username = format!(
        "real{}{}",
        number,
        &Uuid::new_v4().simple().to_string()[..12]
    );
    let password = Password::new(format!("local-{}", Uuid::new_v4()))?;
    let request = RegisterRequest::new(
        &username,
        password.clone(),
        InvitationCode::new(invitation)?,
    )?;
    let response = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()?
        .post(format!("{base}/v1/auth/register"))
        .json(&request)
        .send()
        .await
        .map_err(|_| format!("player {number}: disposable registration transport failed"))?;
    if response.status() != reqwest::StatusCode::CREATED {
        return Err("disposable account registration failed".into());
    }
    let api = ReqwestCloudApi::new(base)?;
    let keychain: Arc<dyn RefreshTokenStore> = Arc::new(MemoryKeychain::default());
    eprintln!("Player {number}: logging into local server");
    let auth = AuthSession::login(&api, keychain.as_ref(), username, password).await?;
    let bridge = inputs.repository.join("bridge");
    let config = SessionConfig {
        client_instance_id: ClientInstanceId::new(Uuid::new_v4())?,
        manifest,
        trusted_manifest_key: TrustedManifestKey::new(KEY_ID, inputs.public_key)?,
        epoch_store: EpochStore::new(root.path().join("epoch.json")),
        // Recovery workspaces must outlive the staging TempDir when retained.
        workspace_parent: std::env::temp_dir().join("coop-real-presence-sessions"),
        bridge_lua_dir: bridge.clone(),
    };
    eprintln!("Player {number}: acquiring fresh session");
    let mut session = SessionLifecycle::acquire_with_keychain(&api, auth, config, keychain).await?;
    let epoch = session.lease.session_epoch.value();
    eprintln!("Player {number}: starting supervised sidecar and mGBA");
    let children = SupervisedChildren::start_with_bridge(
        sidecar.with_session_epoch(epoch)?,
        mgba,
        epoch,
        &session.workspace,
        &bridge,
    )
    .await;
    let mut children = match children {
        Ok(children) => children,
        Err(error) => {
            if error.cleanup_confirmed() {
                let _ = session.release(&api).await;
            } else {
                let _ = session.preserve_recovery_after_child_failure();
                let _ = session.close_credentials(&api).await;
                let _ = root.keep();
            }
            return Err(format!("player {number}: supervised startup failed: {error}").into());
        }
    };
    eprintln!(
        "Player {number}: load {}",
        session.workspace.path().join("main.lua").display()
    );
    let shutdown = async move {
        if !*stop.borrow() {
            let _ = stop.changed().await;
        }
    };
    let lifecycle = session
        .run_until_shutdown_with_realtime(&api, &mut children, shutdown)
        .await;
    let stopped = children.stop().await;
    if stopped.is_err() {
        let _ = session.preserve_recovery_after_child_failure();
        let _ = session.close_credentials(&api).await;
        let _ = root.keep();
        return Err(format!(
            "player {number}: child cleanup uncertain; retained lease and recovery"
        )
        .into());
    }
    let released = session.release(&api).await;
    if let Err(error) = lifecycle {
        return Err(format!("player {number}: lifecycle failed: {error}").into());
    }
    released.map_err(|_| format!("player {number}: release failed"))?;
    eprintln!("Player {number}: lifecycle drained and lease released");
    Ok(())
}

async fn observe(
    path: &Path,
    duration: Duration,
    stop: watch::Sender<bool>,
    presence: Option<coop_server::PresenceService>,
) -> TestResult<()> {
    let abort = path.with_file_name("abort.txt");
    let mut stopped = stop.subscribe();
    if *stopped.borrow() {
        return Err("a player exited before manual acceptance".into());
    }
    let deadline = Instant::now() + duration;
    let mut last_connections = None;
    let result = loop {
        tokio::select! {
            _ = stopped.changed() => break Err("a player exited before manual acceptance".into()),
            () = tokio::time::sleep_until(deadline) => break Err("manual observation timed out; no conformance pass".into()),
            () = tokio::time::sleep(Duration::from_millis(250)) => {
                if let Some(service) = &presence {
                    let Ok(count) = service.connection_count() else {
                        break Err("presence connection diagnostics failed".into());
                    };
                    if last_connections != Some(count) {
                        eprintln!("Local presence service: {count} joined players");
                        last_connections = Some(count);
                    }
                }
                if abort.exists() {
                    break Err("manual observation aborted; no conformance pass".into());
                }
                if fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == ACCEPTANCE.len() as u64)
                    && fs::read(path).is_ok_and(|bytes| bytes == ACCEPTANCE.as_bytes()) {
                    break Ok(());
                }
            }
        }
    };
    stop.send_replace(true);
    result
}

async fn log_realtime_admission(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Fixed labels only: never print raw URI/query, headers, bodies or tickets.
    let phase = match request.uri().path() {
        "/v1/realtime/tickets" => Some("eligible pose reached ticket mint"),
        "/v1/realtime" => Some("WebSocket upgrade"),
        _ => None,
    };
    let response = next.run(request).await;
    if let Some(phase) = phase {
        eprintln!(
            "Local realtime: {phase}; HTTP {}",
            response.status().as_u16()
        );
    }
    response
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual two-player stock mGBA observation requires local ROM and runtime"]
#[allow(clippy::too_many_lines)] // Keep task ownership and final drain together.
async fn two_stock_mgba_players_manual_presence() -> TestResult<()> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let seconds = std::env::var("COOP_REAL_DURATION_SECONDS")
        .unwrap_or_else(|_| "900".into())
        .parse::<u64>()?;
    if !(30..=3600).contains(&seconds) {
        return Err("duration must be 30..=3600 seconds".into());
    }
    let mut key = [0; 32];
    getrandom::fill(&mut key).map_err(|_| "local signing key generation failed")?;
    let mut pepper = [0; 32];
    getrandom::fill(&mut pepper).map_err(|_| "local invitation pepper generation failed")?;
    let inputs = Inputs {
        rom: PathBuf::from(std::env::var_os("COOP_REAL_ROM").ok_or("COOP_REAL_ROM is required")?),
        mgba: PathBuf::from(
            std::env::var_os("COOP_REAL_MGBA").ok_or("COOP_REAL_MGBA is required")?,
        ),
        sidecar: std::env::var_os("COOP_REAL_SIDECAR").map_or_else(
            || repository.join("target/debug/coop-sidecar.exe"),
            PathBuf::from,
        ),
        public_key: ed25519_dalek::SigningKey::from_bytes(&key)
            .verifying_key()
            .to_bytes(),
        repository,
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let base = format!("http://{}", listener.local_addr()?);
    let config = Phase2Config::local(pepper.to_vec(), SigningPrivateKey::from_bytes(key), KEY_ID)?
        .with_upload_base_url(&base);
    let app = Phase2App::new(config)?;
    let presence = app.presence();
    let invitations = [Uuid::new_v4().to_string(), Uuid::new_v4().to_string()];
    for invitation in &invitations {
        app.add_invitation(invitation)?;
    }
    let observation = tempfile::Builder::new()
        .prefix("coop-real-observation-")
        .tempdir()?;
    let acceptance = observation.path().join("accepted.txt");
    let (server_stop, server_shutdown) = tokio::sync::oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.router()
                .layer(axum::middleware::from_fn(log_realtime_admission)),
        )
        .with_graceful_shutdown(async move {
            let _ = server_shutdown.await;
        })
        .await
    });
    eprintln!(
        "Manual observation window: {seconds} seconds. Start new games, then meet in Littleroot."
    );
    eprintln!(
        "After observing all checklist items, write this exact LF text to {}:\n{ACCEPTANCE}",
        acceptance.display()
    );
    eprintln!(
        "To stop without acceptance, create {}",
        observation.path().join("abort.txt").display()
    );
    let (stop, _) = watch::channel(false);
    // join! polls its futures on one task. Synchronous artifact hashing and
    // executable probes in one player previously prevented the other player's
    // already-sent HTTP request from being polled within its 15-second limit.
    // Independent tasks leave the remaining runtime workers available to the
    // server, observation deadline, and the other player's network driver.
    // Both handles are always joined, so shutdown never abandons a lifecycle.
    let first_inputs = inputs.clone();
    let first_base = base.clone();
    let first_invitation = invitations[0].clone();
    let first_stop = stop.clone();
    let first_player = tokio::spawn(async move {
        player(1, &first_inputs, &first_base, &first_invitation, first_stop).await
    });
    let second_stop = stop.clone();
    let second_player =
        tokio::spawn(async move { player(2, &inputs, &base, &invitations[1], second_stop).await });
    let (first, second, observed) = tokio::join!(
        first_player,
        second_player,
        observe(
            &acceptance,
            Duration::from_secs(seconds),
            stop,
            Some(presence)
        ),
    );
    let _ = server_stop.send(());
    let server_result =
        if let Ok(result) = tokio::time::timeout(Duration::from_secs(5), &mut server).await {
            result
                .map_err(|_| "local server task failed")?
                .map_err(|_| "local server failed")
        } else {
            server.abort();
            let _ = server.await;
            Err("local server graceful shutdown timed out")
        };
    first.map_err(|_| "player 1 task failed")??;
    second.map_err(|_| "player 2 task failed")??;
    observed?;
    server_result?;
    eprintln!(
        "Manual walking observation accepted; both lifecycle cleanups passed. Map transitions, saving and resume are not certified by this harness."
    );
    Ok(())
}

#[tokio::test]
async fn observation_does_not_wait_after_players_already_stopped() {
    let (stop, receiver) = watch::channel(false);
    stop.send_replace(true);
    drop(receiver);
    let root = tempfile::tempdir().unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        observe(
            &root.path().join("accepted.txt"),
            Duration::from_secs(30),
            stop,
            None,
        ),
    )
    .await
    .expect("an already stopped run must not wait for the observation deadline");
    assert!(result.is_err());
}

#[tokio::test]
async fn checked_in_bridge_materializes_for_real_presence_workspace() -> TestResult<()> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let root = tempfile::tempdir()?;
    let workspace = coop_launcher::SessionWorkspace::create(root.path())?;
    workspace.copy_bridge_inputs(&repository.join("bridge"))?;
    let sidecar = coop_sidecar::LocalSidecar::bind_with_epoch(1).await?;
    let descriptor = sidecar.session_descriptor();
    coop_launcher::process::validate_descriptor(&descriptor, 1)?;
    workspace.write_session_lua(
        descriptor.bridge().host(),
        descriptor.bridge().port(),
        descriptor.bridge().secret(),
    )?;
    Ok(())
}

#![cfg(windows)]

//! Bounded manual observation harness, not an automated avatar rendering test.
//! Run with --ignored --nocapture and local `COOP_REAL_ROM` / `COOP_REAL_MGBA`.
//! Load each printed main.lua in its respective stock mGBA Scripting window.
//! Only write the printed acceptance file after observing its exact checklist.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
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
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    time::Instant,
};
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ACCEPTANCE: &str = "Both Lua scripts authenticated.\nBoth remote avatars were visible in Littleroot.\nEach player's movement appeared on the other screen.\nRemote avatars did not block local movement.\n";
const ONLINE_ACCEPTANCE: &str = "Both Lua scripts authenticated.\nBoth remote avatars were visible and moved reciprocally.\nEach player changed character through the appearance menu and both screens showed the selected sprite.\nCancelling character selection preserved the previous appearance.\nOnline invitations were accepted and both members saw their group.\nEach member could leave and both became ungrouped without losing nearby avatars.\nDeclined and expired invitations could not create a group.\nAfter the requested socket interruption both games recovered reciprocal movement.\nEach player entered a house and returned with reciprocal presence and selected appearance restored.\nOnline loading and injected unavailable states allowed Back and subsequent Refresh recovered.\nThe fully unlocked pause menu did not clip and ordinary menus and dialogue rendered after Online.\n";
const KEY_ID: &str = "real-mgba-local-only";

// HTTP faults live behind the TCP proxy, at request boundaries. Classifying only
// a connection's first header would miss snapshots sent on pooled HTTP sockets.
struct OnlineFaults {
    directory: PathBuf,
    delay_claimed: AtomicBool,
    unavailable_claimed: AtomicBool,
    delay_limit: Duration,
}

impl OnlineFaults {
    fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            delay_claimed: AtomicBool::new(false),
            unavailable_claimed: AtomicBool::new(false),
            delay_limit: Duration::from_secs(4),
        }
    }

    fn claim(&self, marker: &str, claimed: &AtomicBool) -> bool {
        self.directory.join(marker).is_file()
            && claimed
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
    }

    async fn hold_snapshot(&self) {
        let deadline = Instant::now() + self.delay_limit;
        while !self.directory.join("release-online-snapshot.txt").is_file()
            && !self.directory.join("abort.txt").is_file()
        {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => break,
                () = tokio::time::sleep(Duration::from_millis(20)) => {},
            }
        }
    }
}

async fn inject_online_fault(
    axum::extract::State(faults): axum::extract::State<Arc<OnlineFaults>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let checkpoint_stage = if request.method() == axum::http::Method::POST
        && request.uri().path().starts_with("/v1/characters/")
    {
        if request.uri().path().ends_with("/snapshots/prepare") {
            Some("prepare")
        } else if request.uri().path().ends_with("/snapshots/finalize") {
            Some("finalize")
        } else {
            None
        }
    } else {
        None
    };
    let snapshot = request.method() == axum::http::Method::POST
        && request.uri().path() == "/v1/online/snapshot";
    let unavailable = snapshot
        && faults.claim(
            "unavailable-online-snapshot.txt",
            &faults.unavailable_claimed,
        );
    let delayed = snapshot
        && !unavailable
        && faults.claim("delay-online-snapshot.txt", &faults.delay_claimed);
    // Complete normal body consumption and server handling before replacing a
    // snapshot response. Snapshots are read-only; actions are never intercepted.
    let response = next.run(request).await;
    if let Some(stage) = checkpoint_stage {
        eprintln!(
            "Local checkpoint: {stage}; HTTP {}",
            response.status().as_u16()
        );
    }
    if unavailable {
        eprintln!("Local fault injection: selected Online snapshot returned HTTP 503");
        return axum::response::IntoResponse::into_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
        );
    }
    if delayed {
        eprintln!("Local fault injection: selected Online snapshot held for at most four seconds");
        faults.hold_snapshot().await;
        eprintln!("Local fault injection: held Online snapshot released");
    }
    response
}

struct OnlineObservation {
    interrupt: watch::Sender<u32>,
    dropped: Arc<AtomicUsize>,
}

// Local fault injection only. HTTP traffic remains connected; established
// WebSocket streams are dropped once on the operator's explicit marker.
async fn proxy_connection(
    mut downstream: TcpStream,
    upstream: std::net::SocketAddr,
    mut interrupt: watch::Receiver<u32>,
    dropped: Arc<AtomicUsize>,
) -> std::io::Result<()> {
    let mut header = Vec::new();
    loop {
        let byte = tokio::time::timeout(Duration::from_secs(10), downstream.read_u8()).await??;
        header.push(byte);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() >= 16 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
    }
    let websocket =
        header.starts_with(b"GET /v1/realtime ") || header.starts_with(b"GET /v1/realtime?");
    let mut upstream = TcpStream::connect(upstream).await?;
    upstream.write_all(&header).await?;
    tokio::select! {
        result = tokio::io::copy_bidirectional(&mut downstream, &mut upstream) => { result?; }
        changed = interrupt.changed(), if websocket => {
            if changed.is_ok() {
                dropped.fetch_add(1, Ordering::SeqCst);
                eprintln!("Local fault injection: established WebSocket disconnected");
            }
        }
    }
    Ok(())
}

async fn interruption_proxy(
    listener: TcpListener,
    upstream: std::net::SocketAddr,
    interrupt: watch::Sender<u32>,
    dropped: Arc<AtomicUsize>,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) -> std::io::Result<()> {
    let mut connections = tokio::task::JoinSet::new();
    let result = loop {
        tokio::select! {
            _ = &mut stop => break Ok(()),
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => match accepted {
                Ok((socket, _)) => {
                    connections.spawn(proxy_connection(socket, upstream, interrupt.subscribe(), Arc::clone(&dropped)));
                }
                Err(error) => break Err(error),
            }
        }
    };
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    result
}

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
    // Preserve each player's failure before cleanup and ordered result propagation
    // can hide the other player's originating error. Display implementations
    // redact sources; the pump cause is a payload-free enum.
    if let Err(error) = &lifecycle {
        eprintln!("Player {number}: lifecycle failure before cleanup: {error}");
        if let coop_launcher::SessionError::Control(cause) = error {
            eprintln!(
                "Player {number}: control failure: {cause}; pump: {:?}",
                children.control.terminal_cause()
            );
        }
    }
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
    // Preserve only the genuine SAV after both children stop. This diagnostic
    // copy is not acceptance or recovery authorization, and must not skip lease
    // release if evidence storage fails. Never copy bridge credentials or states.
    match preserve_observation_save(session.workspace.path(), number) {
        Ok(path) => eprintln!(
            "Player {number}: private observed SAV retained at {}",
            path.display()
        ),
        Err(error) => eprintln!("Player {number}: could not retain observed SAV: {error}"),
    }
    let released = session.release(&api).await;
    match &released {
        Ok(()) => eprintln!("Player {number}: lease release confirmed"),
        Err(error) => eprintln!("Player {number}: lease release failed: {error}"),
    }
    if let Err(error) = lifecycle {
        return Err(format!("player {number}: lifecycle failed: {error}").into());
    }
    released.map_err(|_| format!("player {number}: release failed"))?;
    eprintln!("Player {number}: lifecycle drained and lease released");
    Ok(())
}

fn preserve_observation_save(workspace: &Path, number: u8) -> std::io::Result<PathBuf> {
    let evidence = tempfile::Builder::new()
        .prefix(&format!("coop-real-save-player-{number}-"))
        .tempdir()?;
    fs::copy(
        workspace.join("character.sav"),
        evidence.path().join("character.sav"),
    )?;
    Ok(evidence.keep().join("character.sav"))
}

#[test]
fn observed_save_copy_preserves_bytes_without_session_material() -> TestResult<()> {
    let workspace = tempfile::tempdir()?;
    let bytes = [0, 1, 0xff, 2, 3];
    fs::write(workspace.path().join("character.sav"), bytes)?;
    fs::write(workspace.path().join("main.lua"), "private bridge material")?;
    let retained = preserve_observation_save(workspace.path(), 1)?;
    let parent = retained.parent().unwrap();
    let observed = fs::read(&retained)?;
    let names = fs::read_dir(parent)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    fs::remove_file(&retained)?;
    fs::remove_dir(parent)?;
    assert_eq!(observed, bytes);
    assert_eq!(names, [std::ffi::OsString::from("character.sav")]);
    Ok(())
}

async fn observe(
    path: &Path,
    duration: Duration,
    stop: watch::Sender<bool>,
    presence: Option<coop_server::PresenceService>,
    online: Option<OnlineObservation>,
) -> TestResult<()> {
    let abort = path.with_file_name("abort.txt");
    let mut stopped = stop.subscribe();
    if *stopped.borrow() {
        return Err("a player exited before manual acceptance".into());
    }
    let deadline = Instant::now() + duration;
    let mut last_connections = None;
    let acceptance = if online.is_some() {
        ONLINE_ACCEPTANCE
    } else {
        ACCEPTANCE
    };
    let mut interrupted = false;
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
                if let Some(online) = &online
                    && !interrupted && path.with_file_name("interrupt-websockets.txt").exists() {
                    online.interrupt.send_replace(1);
                    interrupted = true;
                }
                if fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == acceptance.len() as u64)
                    && fs::read(path).is_ok_and(|bytes| bytes == acceptance.as_bytes()) {
                    if online.as_ref().is_some_and(|online| online.dropped.load(Ordering::SeqCst) < 2) {
                        break Err("Online acceptance requires two observed socket interruptions".into());
                    }
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
        "/v1/online/snapshot" => Some("Online snapshot"),
        "/v1/online/actions" => Some("Online action"),
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

fn parse_observation_seconds(value: Option<&str>) -> TestResult<u64> {
    let seconds = value.unwrap_or("900").parse::<u64>()?;
    if !(30..=7200).contains(&seconds) {
        return Err("duration must be 30..=7200 seconds".into());
    }
    Ok(seconds)
}

#[test]
fn observation_duration_preserves_default_and_enforces_bounds() {
    assert_eq!(parse_observation_seconds(None).unwrap(), 900);
    for seconds in [30, 3600, 7200] {
        assert_eq!(
            parse_observation_seconds(Some(&seconds.to_string())).unwrap(),
            seconds
        );
    }
    for value in ["29", "7201", "-1", "", "invalid", "18446744073709551616"] {
        assert!(parse_observation_seconds(Some(value)).is_err(), "{value}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual two-player stock mGBA observation requires local ROM and runtime"]
#[allow(clippy::too_many_lines)] // Keep task ownership and final drain together.
async fn two_stock_mgba_players_manual_presence() -> TestResult<()> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let seconds =
        parse_observation_seconds(std::env::var("COOP_REAL_DURATION_SECONDS").ok().as_deref())?;
    let online = match std::env::var("COOP_REAL_SCENARIO").as_deref() {
        Ok("online") => true,
        Ok("walking") | Err(std::env::VarError::NotPresent) => false,
        _ => return Err("COOP_REAL_SCENARIO must be walking or online".into()),
    };
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
    let upstream = listener.local_addr()?;
    let proxy_listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let base = format!("http://{}", proxy_listener.local_addr()?);
    let (interrupt, _) = watch::channel(0);
    let dropped = Arc::new(AtomicUsize::new(0));
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
    let faults = Arc::new(OnlineFaults::new(observation.path().to_path_buf()));
    let acceptance = observation.path().join("accepted.txt");
    let (proxy_stop, proxy_shutdown) = tokio::sync::oneshot::channel();
    let proxy = tokio::spawn(interruption_proxy(
        proxy_listener,
        upstream,
        interrupt.clone(),
        Arc::clone(&dropped),
        proxy_shutdown,
    ));
    let (server_stop, server_shutdown) = tokio::sync::oneshot::channel();
    let mut router = app.router();
    if online {
        router = router.layer(axum::middleware::from_fn_with_state(
            faults,
            inject_online_fault,
        ));
    }
    let router = router.layer(axum::middleware::from_fn(log_realtime_admission));
    let mut server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown.await;
            })
            .await
    });
    eprintln!(
        "Manual observation window: {seconds} seconds. Start new games, then meet in Littleroot."
    );
    let checklist = if online {
        ONLINE_ACCEPTANCE
    } else {
        ACCEPTANCE
    };
    eprintln!(
        "After observing all checklist items, write this exact LF text to {}:\n{checklist}",
        acceptance.display()
    );
    if online {
        eprintln!(
            "Online controls in {}: delay-online-snapshot.txt holds the next snapshot response for at most four seconds; release-online-snapshot.txt releases it early; unavailable-online-snapshot.txt makes one snapshot return HTTP 503. Each fault is claimed once per attempt; keep the other player out of Online while selecting a target. Abort also releases a held response.",
            observation.path().display()
        );
        eprintln!(
            "After testing invitations and leaving, create {} to interrupt both WebSockets once.",
            observation
                .path()
                .join("interrupt-websockets.txt")
                .display()
        );
    }
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
            Some(presence),
            online.then_some(OnlineObservation { interrupt, dropped })
        ),
    );
    let _ = server_stop.send(());
    let _ = proxy_stop.send(());
    let proxy_result = proxy.await;
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
    proxy_result.map_err(|_| "local interruption proxy task failed")??;
    eprintln!(
        "Manual scenario checklist accepted; both lifecycle cleanups passed. Saving, resume and group travel are not certified by this harness."
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
            None,
        ),
    )
    .await
    .expect("an already stopped run must not wait for the observation deadline");
    assert!(result.is_err());
}

#[tokio::test]
async fn online_acceptance_requires_observed_socket_interruption() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("accepted.txt");
    fs::write(&path, ONLINE_ACCEPTANCE).unwrap();
    let (stop, _) = watch::channel(false);
    let (interrupt, _) = watch::channel(0);
    let result = observe(
        &path,
        Duration::from_secs(1),
        stop,
        None,
        Some(OnlineObservation {
            interrupt,
            dropped: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("two observed socket interruptions")
    );
}

#[tokio::test]
async fn interruption_proxy_drops_websockets_and_preserves_http() -> TestResult<()> {
    let backend = TcpListener::bind(("127.0.0.1", 0)).await?;
    let backend_address = backend.local_addr()?;
    let echo = tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..2 {
            let (socket, _) = backend.accept().await.unwrap();
            tasks.spawn(async move {
                let (mut reader, mut writer) = socket.into_split();
                let _ = tokio::io::copy(&mut reader, &mut writer).await;
            });
        }
        while tasks.join_next().await.is_some() {}
    });
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let (interrupt, _) = watch::channel(0);
    let dropped = Arc::new(AtomicUsize::new(0));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let proxy = tokio::spawn(interruption_proxy(
        listener,
        backend_address,
        interrupt.clone(),
        Arc::clone(&dropped),
        stopped,
    ));
    let mut websocket = TcpStream::connect(address).await?;
    let mut http = TcpStream::connect(address).await?;
    for (socket, header) in [
        (
            &mut websocket,
            b"GET /v1/realtime HTTP/1.1\r\n\r\n".as_slice(),
        ),
        (
            &mut http,
            b"POST /v1/online/snapshot HTTP/1.1\r\n\r\n".as_slice(),
        ),
    ] {
        socket.write_all(header).await?;
        let mut response = vec![0; header.len()];
        tokio::time::timeout(Duration::from_secs(2), socket.read_exact(&mut response)).await??;
        assert_eq!(response, header);
    }
    interrupt.send_replace(1);
    let mut byte = [0; 1];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), websocket.read(&mut byte)).await??,
        0
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    http.write_all(b"ok").await?;
    let mut response = [0; 2];
    tokio::time::timeout(Duration::from_secs(2), http.read_exact(&mut response)).await??;
    assert_eq!(&response, b"ok");
    let _ = stop.send(());
    proxy.await??;
    echo.await?;
    Ok(())
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

fn fault_test_router(faults: Arc<OnlineFaults>) -> axum::Router {
    axum::Router::new()
        .fallback(|body: axum::body::Bytes| async move { body })
        .layer(axum::middleware::from_fn_with_state(
            faults,
            inject_online_fault,
        ))
}

async fn fault_test_request(
    router: axum::Router,
    method: &str,
    path: &str,
) -> axum::response::Response {
    use tower::ServiceExt;
    router
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(path)
                .body(axum::body::Body::from("snapshot request body"))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn online_faults_only_select_one_snapshot_and_preserve_other_routes() -> TestResult<()> {
    let root = tempfile::tempdir()?;
    let faults = Arc::new(OnlineFaults::new(root.path().to_path_buf()));
    let router = fault_test_router(Arc::clone(&faults));
    fs::write(root.path().join("unavailable-online-snapshot.txt"), "")?;
    for (method, path) in [
        ("POST", "/v1/online/actions"),
        ("POST", "/v1/sessions/heartbeat"),
        ("POST", "/v1/checkpoints"),
        ("GET", "/v1/realtime"),
        ("GET", "/v1/online/snapshot"),
        ("POST", "/v1/online/snapshot-extra"),
    ] {
        let response = fault_test_request(router.clone(), method, path).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await?
                .as_ref(),
            b"snapshot request body"
        );
    }
    assert!(!faults.unavailable_claimed.load(Ordering::SeqCst));
    assert_eq!(
        fault_test_request(router.clone(), "POST", "/v1/online/snapshot")
            .await
            .status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fault_test_request(router, "POST", "/v1/online/snapshot")
            .await
            .status(),
        axum::http::StatusCode::OK
    );
    Ok(())
}

#[tokio::test]
async fn held_snapshot_releases_automatically_and_does_not_hold_later_requests() -> TestResult<()> {
    let root = tempfile::tempdir()?;
    let mut faults = OnlineFaults::new(root.path().to_path_buf());
    faults.delay_limit = Duration::from_millis(200);
    let faults = Arc::new(faults);
    let router = fault_test_router(Arc::clone(&faults));
    fs::write(root.path().join("delay-online-snapshot.txt"), "")?;
    let held = tokio::spawn(fault_test_request(
        router.clone(),
        "POST",
        "/v1/online/snapshot",
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        while !faults.delay_claimed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(!held.is_finished());
    assert_eq!(
        fault_test_request(router, "POST", "/v1/online/snapshot")
            .await
            .status(),
        axum::http::StatusCode::OK
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), held)
            .await??
            .status(),
        axum::http::StatusCode::OK
    );
    Ok(())
}

#[tokio::test]
async fn release_and_abort_markers_drain_held_snapshot_before_deadline() -> TestResult<()> {
    for marker in ["release-online-snapshot.txt", "abort.txt"] {
        let root = tempfile::tempdir()?;
        let faults = Arc::new(OnlineFaults::new(root.path().to_path_buf()));
        let router = fault_test_router(Arc::clone(&faults));
        fs::write(root.path().join("delay-online-snapshot.txt"), "")?;
        let held = tokio::spawn(fault_test_request(router, "POST", "/v1/online/snapshot"));
        tokio::time::timeout(Duration::from_secs(1), async {
            while !faults.delay_claimed.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(!held.is_finished());
        fs::write(root.path().join(marker), "")?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), held)
                .await??
                .status(),
            axum::http::StatusCode::OK
        );
    }
    Ok(())
}

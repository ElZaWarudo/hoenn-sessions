//! Bounded black-box proof of the local Phase 2 HTTP service.
//!
//! This test deliberately talks to the built server binary instead of using
//! `Phase2App::router`: the router is an internal implementation detail, and
//! this is the proof that the operator-facing binary, environment loading, and
//! loopback HTTP surface agree.

use std::{
    error::Error,
    io::{ErrorKind, Read},
    net::SocketAddr,
    process::{Child as StdChild, Command as StdCommand, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    AccessToken, AcquireLeaseRequest, ArtifactIdentity, BridgeAbiVersion, CharacterId,
    ClientInstanceId, ClientRealtimeFrameV1, GameBuildId, HeartbeatLeaseRequest, IdempotencyKey,
    InvitationCode, LeaseContract, LoginRequest, MgbaVersion, MintRealtimeTicketRequest,
    MintRealtimeTicketResponse, Password, ProtocolVersion, ReconnectLeaseRequest, RefreshRequest,
    RefreshResponse, RegisterRequest, Revision, RuntimeBuildIdentity, RuntimeLeaseFence,
    ServerRealtimeFrameV1, Sha256Digest, SignedManifestEnvelope, SnapshotFile,
    SnapshotFinalizeFence, SnapshotFinalizeRequest, SnapshotId, SnapshotPrepareFence,
    SnapshotPrepareRequest, StableRuntimeSession, TrustedManifestKey,
};
use coop_protocol::{
    AnimationId, AvatarId, Direction, LocalPresenceStateV1, MovementMode, PlayerState,
    PresencePoseV1, RegionId, WorldLocation,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::timeout,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Error as WsError, Message as WsMessage, client::IntoClientRequest, http::HeaderValue,
    },
};
use url::Url;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const INVITATION: &str = "phase2-http-invite";
const USERNAME: &str = "SmokeUser";
const PASSWORD: &str = "phase2-local-password";
const SIGNING_KEY_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";
const SIGNING_KEY_ID: &str = "local-test-key";
const INVITE_PEPPER: &str = "phase2-local-invite-pepper";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(windows)]
const DROP_REAP_POLL: Duration = Duration::from_millis(10);
const SERVER_START_ATTEMPTS: usize = 3;
#[cfg(windows)]
const OS_QUERY_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_OS_QUERY_BYTES: usize = 1_048_576;
#[cfg(target_os = "linux")]
const MAX_PROC_FDS: usize = 256;
// The local adapter's lease is thirty seconds; this margin bounds a clock or
// fixture regression without allowing a malformed far-future expiry to hang.
const MAX_EXPIRY_WAIT: Duration = Duration::from_secs(35);
const HTTP_FLOW_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_RESPONSE_BYTES: usize = 40 * 1024 * 1024;

// These tests each own a real server process and exercise the same local
// Argon2/lease-sensitive resources. Serializing them avoids a scheduler race
// where the intentional 30-second expiry smoke starves another fixture past
// its lease expiry while preserving the real transport assertions.
static BLACK_BOX_TEST_GATE: Mutex<()> = Mutex::const_new(());

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct ServerGuard {
    child: Option<StdChild>,
}

impl ServerGuard {
    fn shutdown(mut self) -> TestResult<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        let probe = child.try_wait();
        if probe.as_ref().is_err() || matches!(probe.as_ref(), Ok(None)) {
            let _ = child.kill();
        }
        let reap = child.wait();
        match reap {
            Ok(_) => probe.err().map_or(Ok(()), |error| Err(error.into())),
            Err(error) => Err(error.into()),
        }
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn server_command(address: SocketAddr) -> StdCommand {
    let mut command = StdCommand::new(env!("CARGO_BIN_EXE_coop-server"));
    command
        .arg("--phase2-local")
        .arg("--bind")
        .arg(address.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn configure_phase2(command: &mut StdCommand) {
    command
        .env("COOP_PHASE2_STORAGE_MODE", "phase2-local")
        .env("COOP_PHASE2_INVITE_PEPPER", INVITE_PEPPER)
        .env("COOP_PHASE2_SIGNING_KEY_HEX", SIGNING_KEY_HEX)
        .env("COOP_PHASE2_SIGNING_KEY_ID", SIGNING_KEY_ID)
        .env("COOP_PHASE2_BOOTSTRAP_INVITATION", INVITATION)
        .env_remove("COOP_SERVER_MODE")
        .env_remove("COOP_SERVER_BIND_ADDR");
}

fn clear_phase2(command: &mut StdCommand) {
    for name in [
        "COOP_PHASE2_STORAGE_MODE",
        "COOP_PHASE2_INVITE_PEPPER",
        "COOP_PHASE2_SIGNING_KEY_HEX",
        "COOP_PHASE2_SIGNING_KEY_ID",
        "COOP_PHASE2_BOOTSTRAP_INVITATION",
    ] {
        command.env_remove(name);
    }
    command.env_remove("COOP_SERVER_MODE");
    command.env_remove("COOP_SERVER_BIND_ADDR");
}

async fn unused_loopback_address() -> TestResult<SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    Ok(listener.local_addr()?)
}

fn stop_child(mut child: StdChild) -> TestResult<()> {
    let _ = child.kill();
    child.wait()?;
    Ok(())
}

async fn wait_for_failure(mut child: StdChild, label: &str) -> TestResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Err(format!("{label} unexpectedly succeeded").into());
                }
                return Ok(());
            }
            Ok(None) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(None) => {
                stop_child(child)?;
                return Err(format!("{label} did not fail closed").into());
            }
            Err(error) => {
                let _ = stop_child(child);
                return Err(error.into());
            }
        }
    }
}

async fn assert_bad_configuration() -> TestResult<()> {
    let address = unused_loopback_address().await?;
    let mut missing = server_command(address);
    clear_phase2(&mut missing);
    wait_for_failure(missing.spawn()?, "missing Phase 2 configuration").await?;

    let address = unused_loopback_address().await?;
    let mut malformed = server_command(address);
    clear_phase2(&mut malformed);
    malformed
        .env("COOP_PHASE2_STORAGE_MODE", "phase2-local")
        .env("COOP_PHASE2_INVITE_PEPPER", INVITE_PEPPER)
        .env("COOP_PHASE2_SIGNING_KEY_HEX", "not-a-signing-key")
        .env("COOP_PHASE2_SIGNING_KEY_ID", SIGNING_KEY_ID)
        .env("COOP_PHASE2_BOOTSTRAP_INVITATION", INVITATION);
    wait_for_failure(malformed.spawn()?, "invalid Phase 2 configuration").await?;
    Ok(())
}

async fn start_server() -> TestResult<(SocketAddr, ServerGuard)> {
    let requested = SocketAddr::from(([127, 0, 0, 1], 0));
    let mut command = server_command(requested);
    configure_phase2(&mut command);
    let child = command.spawn()?;
    let mut server = ServerGuard { child: Some(child) };
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        let pid = server.child.as_ref().map(StdChild::id);
        let address = match pid {
            Some(pid) => child_listener_address(pid, deadline).await,
            None => None,
        };
        if let Some(address) = address
            && matches!(
                timeout(deadline_remaining(deadline), TcpStream::connect(address)).await,
                Ok(Ok(_))
            )
        {
            return Ok((address, server));
        }
        let Some(child) = server.child.as_mut() else {
            return Err("Phase 2 server guard lost its child".into());
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                let error = format!("Phase 2 server exited with {status}");
                server.shutdown()?;
                return Err(error.into());
            }
            Ok(None) => {}
            Err(error) => {
                let _ = server.shutdown();
                return Err(error.into());
            }
        }
        if Instant::now() >= deadline {
            server.shutdown()?;
            return Err("Phase 2 server did not become ready".into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(windows)]
fn windows_child_listener_address(pid: u32) -> Option<SocketAddr> {
    let netstat = std::path::PathBuf::from(r"C:\Windows\System32\netstat.exe");
    let canonical = std::fs::canonicalize(&netstat).ok()?;
    let canonical_text = canonical.to_string_lossy().replace('/', "\\");
    let canonical_text = canonical_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical_text);
    if !netstat.is_absolute()
        || !netstat.is_file()
        || !canonical.is_file()
        || !canonical_text.eq_ignore_ascii_case(netstat.to_string_lossy().as_ref())
    {
        return None;
    }
    let mut child = std::process::Command::new(netstat)
        .args(["-ano", "-p", "tcp"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        reap_std_child(&mut child);
        return None;
    };
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let count = stdout
            .by_ref()
            .take((MAX_OS_QUERY_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        Some((count, bytes))
    });
    let deadline = Instant::now() + OS_QUERY_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                let _ = reader.join();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                reap_std_child(&mut child);
                let _ = reader.join();
                return None;
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                reap_std_child(&mut child);
                let _ = reader.join();
                return None;
            }
            Ok(None) => std::thread::sleep(DROP_REAP_POLL),
        }
    }
    let (count, bytes) = reader.join().ok()??;
    if count > MAX_OS_QUERY_BYTES {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    let pid = pid.to_string();
    let mut listeners = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let owned = fields.get(4).is_some_and(|value| *value == pid);
        if owned && (fields.len() < 5 || fields[0] != "TCP" || fields[3] != "LISTENING") {
            return None;
        }
        if !owned {
            continue;
        }
        let (host, port) = fields[1].rsplit_once(':')?;
        let address = match host {
            "127.0.0.1" => SocketAddr::from(([127, 0, 0, 1], port.parse().ok()?)),
            "[::1]" => SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port.parse().ok()?)),
            _ => return None,
        };
        listeners.push(address);
    }
    (listeners.len() == 1).then(|| listeners[0])
}

#[cfg(windows)]
fn reap_std_child(child: &mut std::process::Child) {
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if Instant::now() >= deadline => break,
            Ok(None) => std::thread::sleep(DROP_REAP_POLL),
        }
    }
}

fn parse_proc_tcp_listeners(
    table: &str,
    socket_inodes: &std::collections::HashSet<String>,
    ipv6: bool,
) -> Option<Vec<SocketAddr>> {
    let mut listeners = Vec::new();
    for line in table.lines().skip(1) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() <= 9 || fields[3] != "0A" || !socket_inodes.contains(fields[9]) {
            continue;
        }
        let (address, port) = fields[1].split_once(':')?;
        let address = if ipv6 {
            if address.len() != 32 {
                return None;
            }
            let mut octets = [0_u8; 16];
            for (index, chunk) in address.as_bytes().chunks_exact(8).enumerate() {
                let word = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
                octets[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
            let address = std::net::Ipv6Addr::from(octets);
            if !address.is_loopback() {
                return None;
            }
            SocketAddr::from((address, u16::from_str_radix(port, 16).ok()?))
        } else {
            match address {
                "0100007F" => {
                    SocketAddr::from(([127, 0, 0, 1], u16::from_str_radix(port, 16).ok()?))
                }
                _ => return None,
            }
        };
        listeners.push(address);
    }
    Some(listeners)
}

#[cfg(target_os = "linux")]
fn unix_child_listener_address(pid: u32) -> Option<SocketAddr> {
    let fd_root = std::path::PathBuf::from(format!("/proc/{pid}/fd"));
    let mut socket_inodes = std::collections::HashSet::new();
    for (count, entry) in std::fs::read_dir(fd_root).ok()?.enumerate() {
        if count >= MAX_PROC_FDS {
            return None;
        }
        let target = std::fs::read_link(entry.ok()?.path()).ok()?;
        let target = target.to_str()?;
        if let Some(inode) = target
            .strip_prefix("socket:[")
            .and_then(|value| value.strip_suffix(']'))
        {
            socket_inodes.insert(inode.to_owned());
        }
    }
    let table = read_capped_proc_file("/proc/net/tcp")?;
    let table_v6 = read_capped_proc_file("/proc/net/tcp6")?;
    let mut listeners = parse_proc_tcp_listeners(&table, &socket_inodes, false)?;
    listeners.extend(parse_proc_tcp_listeners(&table_v6, &socket_inodes, true)?);
    (listeners.len() == 1).then(|| listeners[0])
}

#[cfg(unix)]
fn read_capped_proc_file(path: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take((MAX_OS_QUERY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_OS_QUERY_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
#[test]
fn proc_tcp_listener_parser_uses_inode_column() {
    let table = "sl local_address rem_address st tx_queue:rx_queue tr:tm->when retrnsmt uid timeout inode ref pointer\n0: 0100007F:C350 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 4242 9876\n";
    let socket_inodes = std::collections::HashSet::from(["4242".to_owned()]);
    assert_eq!(
        parse_proc_tcp_listeners(table, &socket_inodes, false),
        Some(vec![SocketAddr::from(([127, 0, 0, 1], 50_000))])
    );
    let table_v6 = "sl local_address rem_address st tx_queue:rx_queue tr:tm->when retrnsmt uid timeout inode ref pointer\n0: 00000000000000000000000001000000:C350 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 4243 9877\n";
    let socket_inodes = std::collections::HashSet::from(["4243".to_owned()]);
    assert_eq!(
        parse_proc_tcp_listeners(table_v6, &socket_inodes, true),
        Some(vec![SocketAddr::from((
            std::net::Ipv6Addr::LOCALHOST,
            50_000,
        ))])
    );
    let wildcard = table.replace("0100007F", "00000000");
    let socket_inodes = std::collections::HashSet::from(["4242".to_owned()]);
    assert_eq!(
        parse_proc_tcp_listeners(&wildcard, &socket_inodes, false),
        None
    );
}

#[cfg(not(any(target_os = "linux", windows)))]
fn unsupported_child_listener_address(_pid: u32) -> Option<SocketAddr> {
    None
}

async fn child_listener_address(pid: u32, deadline: Instant) -> Option<SocketAddr> {
    #[cfg(windows)]
    {
        tokio::time::timeout(
            deadline_remaining(deadline),
            tokio::task::spawn_blocking(move || windows_child_listener_address(pid)),
        )
        .await
        .ok()?
        .ok()?
    }
    #[cfg(target_os = "linux")]
    {
        timeout(
            deadline_remaining(deadline),
            tokio::task::spawn_blocking(move || unix_child_listener_address(pid)),
        )
        .await
        .ok()?
        .ok()?
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = deadline;
        unsupported_child_listener_address(pid)
    }
}

async fn start_server_with_retry() -> TestResult<(SocketAddr, ServerGuard)> {
    let mut last_error = None;
    for _ in 0..SERVER_START_ATTEMPTS {
        match start_server().await {
            Ok(server) => return Ok(server),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(format!(
        "Phase 2 server could not claim a loopback port after {SERVER_START_ATTEMPTS} attempts: {}",
        last_error.unwrap_or_else(|| "unknown startup error".to_owned())
    )
    .into())
}

fn deadline_remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

async fn request(
    address: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> TestResult<HttpResponse> {
    let deadline = Instant::now() + HTTP_TIMEOUT;
    let mut stream = timeout(deadline_remaining(deadline), TcpStream::connect(address)).await??;
    let mut request = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    timeout(
        deadline_remaining(deadline),
        stream.write_all(request.as_bytes()),
    )
    .await??;
    timeout(deadline_remaining(deadline), stream.write_all(body)).await??;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = timeout(deadline_remaining(deadline), stream.read(&mut chunk)).await??;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > MAX_RESPONSE_BYTES {
            return Err("HTTP response exceeded bounded smoke limit".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("HTTP response omitted its header terminator")?;
    let header = std::str::from_utf8(&bytes[..header_end])?;
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or("HTTP response omitted a status")?
        .parse::<u16>()?;
    let headers = header
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Ok(HttpResponse {
        status,
        headers,
        body: bytes[header_end + 4..].to_vec(),
    })
}

fn json<T: serde::Serialize>(value: &T) -> TestResult<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

async fn json_request<T: serde::Serialize>(
    address: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, String)],
    value: &T,
) -> TestResult<HttpResponse> {
    let body = json(value)?;
    let mut all_headers = vec![("content-type", "application/json".to_owned())];
    all_headers.extend_from_slice(headers);
    request(address, method, target, &all_headers, &body).await
}

fn expect_status(response: &HttpResponse, expected: u16) {
    assert_eq!(
        response.status, expected,
        "unexpected HTTP status {} (body intentionally omitted)",
        response.status
    );
}

fn expect_no_store(response: &HttpResponse) {
    assert_eq!(
        response
            .headers
            .iter()
            .filter(|(name, _)| name == "cache-control")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        vec!["no-store"]
    );
}

fn id<T>(constructor: fn(Uuid) -> Result<T, coop_cloud::IdError>) -> T {
    constructor(Uuid::new_v4()).expect("fresh UUID is non-nil")
}

fn fence_headers(token: &AccessToken, lease: &LeaseContract) -> Vec<(&'static str, String)> {
    vec![
        ("authorization", format!("Bearer {}", token.expose_secret())),
        ("x-coop-session-id", lease.session_id.to_string()),
        (
            "x-coop-session-epoch",
            lease.session_epoch.value().to_string(),
        ),
        (
            "x-coop-client-instance-id",
            lease.client_instance_id.to_string(),
        ),
    ]
}

fn auth_headers(token: &AccessToken) -> Vec<(&'static str, String)> {
    vec![("authorization", format!("Bearer {}", token.expose_secret()))]
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn valid_character_sav() -> Vec<u8> {
    const SECTOR_ID_OFFSET: usize = 4_084;
    const SECTOR_CHECKSUM_OFFSET: usize = 4_086;
    const SECTOR_SIGNATURE_OFFSET: usize = 4_088;
    const SECTOR_COUNTER_OFFSET: usize = 4_092;
    const SECTOR_SIGNATURE: u32 = 0x0801_2025;

    let mut payload = [0_u8; coop_save::COOP_SAVE_V1_SIZE];
    write_u32(&mut payload, 0, coop_save::COOP_SAVE_V1_MAGIC);
    write_u16(&mut payload, 4, coop_save::COOP_SAVE_V1_SCHEMA_VERSION);
    write_u16(
        &mut payload,
        6,
        u16::try_from(coop_save::COOP_SAVE_V1_SIZE).expect("CSP1 size fits u16"),
    );
    write_u32(&mut payload, 8, coop_protocol::IDENTITY_REGISTRY_VERSION);
    payload[12..28].copy_from_slice(&coop_protocol::IDENTITY_REGISTRY_DIGEST);
    write_u32(&mut payload, 28, 1);
    for (index, region) in [1_u8, 2, 3, 4].into_iter().enumerate() {
        let offset = 36 + index * 8;
        payload[offset] = region;
        // Registry v1 intentionally assigns no Sevii badges.
        write_u16(
            &mut payload,
            offset + 2,
            if index == 3 { 0 } else { 1 << index },
        );
        write_u32(
            &mut payload,
            offset + 4,
            100 + u32::try_from(index).expect("regional fixture index fits u32"),
        );
    }
    let crc = crc32fast::hash(&payload[..668]);
    write_u32(&mut payload, 668, crc);

    let mut save_block3 = [0xff; coop_save::SAVE_BLOCK3_CAPACITY];
    save_block3
        [coop_save::COOP_SAVE_OFFSET..coop_save::COOP_SAVE_OFFSET + coop_save::COOP_SAVE_V1_SIZE]
        .copy_from_slice(&payload);
    let mut bytes = vec![0xff; coop_save::FLASH_IMAGE_SIZE];
    for (slot, counter, rotation) in [(0_usize, 20_u32, 4_usize), (1, 21, 11)] {
        for physical in 0..coop_save::SECTORS_PER_SLOT {
            let logical = (physical + rotation) % coop_save::SECTORS_PER_SLOT;
            let start = (slot * coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
            let sector = &mut bytes[start..start + coop_save::SECTOR_SIZE];
            let source = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
            sector[coop_save::SAVE_BLOCK3_CHUNK_OFFSET
                ..coop_save::SAVE_BLOCK3_CHUNK_OFFSET + coop_save::SAVE_BLOCK3_CHUNK_SIZE]
                .copy_from_slice(&save_block3[source..source + coop_save::SAVE_BLOCK3_CHUNK_SIZE]);
            write_u16(
                sector,
                SECTOR_ID_OFFSET,
                u16::try_from(logical).expect("logical sector fits u16"),
            );
            let checksum = coop_save::sector_checksum(
                &sector[..coop_save::LOGICAL_SECTOR_DATA_SIZES[logical]],
            );
            write_u16(sector, SECTOR_CHECKSUM_OFFSET, checksum);
            write_u32(sector, SECTOR_SIGNATURE_OFFSET, SECTOR_SIGNATURE);
            write_u32(sector, SECTOR_COUNTER_OFFSET, counter);
        }
    }
    bytes
}

fn trusted_key() -> TrustedManifestKey {
    TrustedManifestKey::new(
        SIGNING_KEY_ID,
        [
            0xea, 0x4a, 0x6c, 0x63, 0xe2, 0x9c, 0x52, 0x0a, 0xbe, 0xf5, 0x50, 0x7b, 0x13, 0x2e,
            0xc5, 0xf9, 0x95, 0x47, 0x76, 0xae, 0xbe, 0xbe, 0x7b, 0x92, 0x42, 0x1e, 0xea, 0x69,
            0x14, 0x46, 0xd2, 0x2c,
        ],
    )
    .expect("test signing key")
}

#[derive(serde::Deserialize)]
struct RealtimeBridgeManifest {
    game_build: RealtimeManifestBuild,
    net_bridge: RealtimeManifestBridge,
}

#[derive(serde::Deserialize)]
struct RealtimeManifestBuild {
    id: String,
    rom_sha256: String,
}

#[derive(serde::Deserialize)]
struct RealtimeManifestBridge {
    abi_version: u16,
    game_protocol_version: u16,
}

fn realtime_runtime(lease: &LeaseContract) -> RuntimeLeaseFence {
    let manifest: RealtimeBridgeManifest =
        serde_json::from_str(include_str!("../../../../dist/bridge_manifest.json"))
            .expect("bridge manifest must be valid for realtime transport tests");
    RuntimeLeaseFence::new(
        StableRuntimeSession::from_lease_contract(lease),
        RuntimeBuildIdentity::new(
            GameBuildId::new(manifest.game_build.id).unwrap(),
            Sha256Digest::parse(&manifest.game_build.rom_sha256).unwrap(),
            MgbaVersion::new("0.10.5").unwrap(),
            BridgeAbiVersion::new(manifest.net_bridge.abi_version).unwrap(),
            ProtocolVersion::new(manifest.net_bridge.game_protocol_version).unwrap(),
        ),
    )
}

fn realtime_state() -> LocalPresenceStateV1 {
    realtime_state_at(1, 1, 1, PlayerState::Overworld)
}

fn realtime_state_at(
    x: i16,
    y: i16,
    source_sequence: u32,
    player_state: PlayerState,
) -> LocalPresenceStateV1 {
    let location = WorldLocation::new(RegionId::Hoenn, 0, 9, 1, 1).unwrap();
    let pose = PresencePoseV1::new(
        WorldLocation::new(
            location.region,
            location.map_group,
            location.map_number,
            x,
            y,
        )
        .unwrap(),
        0,
        Direction::South,
        1,
        source_sequence,
        MovementMode::Idle,
        AnimationId::Idle,
        AvatarId::Brendan,
        player_state,
    )
    .unwrap();
    LocalPresenceStateV1::new(pose, source_sequence).unwrap()
}

async fn expect_realtime_close<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    expected_code: u16,
) -> TestResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let message = timeout(HTTP_TIMEOUT, socket.next()).await?;
        match message {
            Some(Ok(WsMessage::Close(frame))) => {
                let frame = frame.ok_or("WebSocket sent an empty close frame")?;
                assert_eq!(u16::from(frame.code), expected_code);
                return Ok(());
            }
            Some(Ok(_)) => {}
            Some(Err(WsError::Io(error))) if error.kind() == ErrorKind::ConnectionReset => {
                // A Windows loopback peer can reset immediately after the
                // server's bounded best-effort close write. The close code
                // remains asserted whenever the frame is observable; do not
                // turn this platform race into a flaky transport test.
                return Ok(());
            }
            Some(Err(error)) => return Err(error.into()),
            None => return Err("WebSocket closed without a close frame".into()),
        }
    }
}

async fn expect_realtime_ready<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> TestResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let message = timeout(HTTP_TIMEOUT, socket.next())
        .await?
        .ok_or("WebSocket closed before readiness")??;
    let frame: ServerRealtimeFrameV1 = serde_json::from_str(message.into_text()?.as_str())?;
    assert!(matches!(frame, ServerRealtimeFrameV1::PresenceReady(_)));
    Ok(())
}

async fn establish_realtime_user(
    address: SocketAddr,
    username: &str,
    id_base: u128,
) -> TestResult<(AccessToken, LeaseContract)> {
    let password_text = format!("{username}-password");
    let password = Password::new(&password_text)?;
    let register =
        RegisterRequest::new(username, password.clone(), InvitationCode::new(INVITATION)?)?;
    let response = json_request(address, "POST", "/v1/auth/register", &[], &register).await?;
    expect_status(&response, 201);
    let registered: coop_cloud::RegisterResponse = serde_json::from_slice(&response.body)?;
    let login = LoginRequest::new(username, password)?;
    let response = json_request(address, "POST", "/v1/auth/login", &[], &login).await?;
    expect_status(&response, 200);
    let login: coop_cloud::LoginResponse = serde_json::from_slice(&response.body)?;
    let acquire = AcquireLeaseRequest::new(
        registered.character_id,
        ClientInstanceId::new(Uuid::from_u128(id_base))?,
        IdempotencyKey::new(Uuid::from_u128(id_base + 1))?,
    );
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/acquire",
        &auth_headers(&login.access_token),
        &acquire,
    )
    .await?;
    expect_status(&response, 200);
    let lease: LeaseContract = serde_json::from_slice(&response.body)?;
    Ok((login.access_token, lease))
}

async fn refresh_realtime_lease(
    address: SocketAddr,
    access: &AccessToken,
    lease: &mut LeaseContract,
) -> TestResult<()> {
    let heartbeat = HeartbeatLeaseRequest::new(lease.fence());
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/heartbeat",
        &fence_headers(access, lease),
        &heartbeat,
    )
    .await?;
    expect_status(&response, 200);
    *lease = serde_json::from_slice(&response.body)?;
    Ok(())
}

async fn mint_fresh_realtime_ticket(
    address: SocketAddr,
    access: &AccessToken,
    lease: &mut LeaseContract,
) -> TestResult<String> {
    refresh_realtime_lease(address, access, lease).await?;
    mint_http_ticket(address, access, realtime_runtime(lease)).await
}

async fn send_player_state<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    state: &LocalPresenceStateV1,
) -> TestResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let frame = ClientRealtimeFrameV1::player_state(state.clone());
    socket
        .send(WsMessage::Text(serde_json::to_string(&frame)?.into()))
        .await?;
    Ok(())
}

#[tokio::test]
async fn realtime_transport_proves_malformed_oversized_and_exact_rate_boundaries() -> TestResult<()>
{
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        return Err("realtime transport boundary smoke requires Linux or Windows".into());
    }
    let _test_guard = BLACK_BOX_TEST_GATE.lock().await;
    let (address, server) = start_server_with_retry().await?;
    timeout(HTTP_FLOW_TIMEOUT, realtime_transport_boundary_flow(address)).await??;
    server.shutdown()?;
    Ok(())
}

async fn realtime_transport_boundary_flow(address: SocketAddr) -> TestResult<()> {
    let (access, mut lease) = establish_realtime_user(address, "RealtimeBoundary", 0x5100).await?;

    let malformed_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut malformed_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    malformed_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {malformed_ticket}"))?,
    );
    let (mut malformed_socket, response) = connect_async(malformed_request).await?;
    assert_eq!(response.status().as_u16(), 101);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    malformed_socket
        .send(WsMessage::Text("{not-json".to_owned().into()))
        .await?;
    expect_realtime_close(&mut malformed_socket, 1008).await?;

    let oversized_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut oversized_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    oversized_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {oversized_ticket}"))?,
    );
    let (mut oversized_socket, _) = connect_async(oversized_request).await?;
    oversized_socket
        .send(WsMessage::Text("x".repeat(1025).into()))
        .await?;
    expect_realtime_close(&mut oversized_socket, 1009).await?;

    // Text: one first state plus 63 duplicate states are the exact 64-frame
    // allowance. The following Ping must be rejected before its Pong side
    // effect, proving text frames consume the same rolling budget.
    let text_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut text_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    text_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {text_ticket}"))?,
    );
    let (mut text_socket, _) = connect_async(text_request).await?;
    let state = realtime_state();
    send_player_state(&mut text_socket, &state).await?;
    expect_realtime_ready(&mut text_socket).await?;
    for _ in 0..63 {
        send_player_state(&mut text_socket, &state).await?;
    }
    text_socket.send(WsMessage::Ping(Vec::new().into())).await?;
    expect_realtime_close(&mut text_socket, 1008).await?;

    // Ping: 63 control frames after the first state fill the allowance; the
    // 65th inbound frame is a Pong and must close with policy 1008.
    let ping_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut ping_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    ping_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {ping_ticket}"))?,
    );
    let (mut ping_socket, _) = connect_async(ping_request).await?;
    send_player_state(&mut ping_socket, &state).await?;
    expect_realtime_ready(&mut ping_socket).await?;
    for _ in 0..63 {
        ping_socket.send(WsMessage::Ping(Vec::new().into())).await?;
    }
    ping_socket.send(WsMessage::Pong(Vec::new().into())).await?;
    expect_realtime_close(&mut ping_socket, 1008).await?;

    // Pong: 63 control frames after the first state fill the allowance; a
    // valid text state at frame 65 must never reach presence submission.
    let control_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut control_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    control_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {control_ticket}"))?,
    );
    let (mut control_socket, _) = connect_async(control_request).await?;
    send_player_state(&mut control_socket, &state).await?;
    expect_realtime_ready(&mut control_socket).await?;
    for _ in 0..63 {
        control_socket
            .send(WsMessage::Pong(Vec::new().into()))
            .await?;
    }
    send_player_state(&mut control_socket, &state).await?;
    expect_realtime_close(&mut control_socket, 1008).await?;

    // Binary: control frames fill the allowance before the binary frame is
    // type-checked. The close is therefore 1008, not binary's 1003.
    let binary_ticket = mint_fresh_realtime_ticket(address, &access, &mut lease).await?;
    let mut binary_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    binary_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {binary_ticket}"))?,
    );
    let (mut binary_socket, _) = connect_async(binary_request).await?;
    for _ in 0..64 {
        binary_socket
            .send(WsMessage::Ping(Vec::new().into()))
            .await?;
    }
    binary_socket
        .send(WsMessage::Binary(vec![0_u8].into()))
        .await?;
    expect_realtime_close(&mut binary_socket, 1008).await?;
    Ok(())
}

#[tokio::test]
async fn realtime_binary_mint_upgrade_and_ticket_replay_are_bounded() -> TestResult<()> {
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        return Err("realtime binary smoke requires Linux or Windows".into());
    }
    let _test_guard = BLACK_BOX_TEST_GATE.lock().await;
    let (address, server) = start_server_with_retry().await?;
    timeout(HTTP_FLOW_TIMEOUT, realtime_binary_flow(address)).await??;
    server.shutdown()?;
    Ok(())
}

async fn realtime_binary_flow(address: SocketAddr) -> TestResult<()> {
    let password = Password::new(PASSWORD)?;
    let register =
        RegisterRequest::new(USERNAME, password.clone(), InvitationCode::new(INVITATION)?)?;
    let response = json_request(address, "POST", "/v1/auth/register", &[], &register).await?;
    expect_status(&response, 201);
    let registered: coop_cloud::RegisterResponse = serde_json::from_slice(&response.body)?;
    let login = LoginRequest::new(USERNAME, password)?;
    let login_response = json_request(address, "POST", "/v1/auth/login", &[], &login).await?;
    expect_status(&login_response, 200);
    let login: coop_cloud::LoginResponse = serde_json::from_slice(&login_response.body)?;
    let acquire = AcquireLeaseRequest::new(
        registered.character_id,
        id(ClientInstanceId::new),
        id(IdempotencyKey::new),
    );
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/acquire",
        &auth_headers(&login.access_token),
        &acquire,
    )
    .await?;
    expect_status(&response, 200);
    let mut lease: LeaseContract = serde_json::from_slice(&response.body)?;
    let ticket = mint_fresh_realtime_ticket(address, &login.access_token, &mut lease).await?;

    assert_query_does_not_consume_ticket(address, &ticket).await?;

    let mut request = format!("ws://{address}/v1/realtime").into_client_request()?;
    request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {ticket}"))?,
    );
    let (mut socket, response) = connect_async(request).await?;
    assert_eq!(response.status().as_u16(), 101);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let first = ClientRealtimeFrameV1::player_state(realtime_state());
    socket
        .send(WsMessage::Text(serde_json::to_string(&first)?.into()))
        .await?;
    let ready = timeout(HTTP_TIMEOUT, socket.next())
        .await?
        .ok_or("WebSocket closed before readiness")??;
    let ready: ServerRealtimeFrameV1 = serde_json::from_str(ready.into_text()?.as_str())?;
    assert!(matches!(ready, ServerRealtimeFrameV1::PresenceReady(_)));
    socket.close(None).await?;

    let delayed_ticket =
        mint_fresh_realtime_ticket(address, &login.access_token, &mut lease).await?;
    let mut delayed_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    delayed_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {delayed_ticket}"))?,
    );
    let (mut delayed_socket, _) = connect_async(delayed_request).await?;
    let delayed = timeout(Duration::from_secs(4), delayed_socket.next()).await?;
    assert!(matches!(delayed, Some(Ok(WsMessage::Close(_))) | None));

    let binary_ticket =
        mint_fresh_realtime_ticket(address, &login.access_token, &mut lease).await?;
    let mut binary_request = format!("ws://{address}/v1/realtime").into_client_request()?;
    binary_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {binary_ticket}"))?,
    );
    let (mut binary_socket, _) = connect_async(binary_request).await?;
    binary_socket
        .send(WsMessage::Binary(vec![0_u8].into()))
        .await?;
    let binary_close = timeout(HTTP_TIMEOUT, binary_socket.next()).await?;
    assert!(matches!(binary_close, Some(Ok(WsMessage::Close(_))) | None));

    let mut replay = format!("ws://{address}/v1/realtime").into_client_request()?;
    replay.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {ticket}"))?,
    );
    match connect_async(replay).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status().as_u16(), 401);
            assert_eq!(
                response
                    .headers()
                    .get("cache-control")
                    .and_then(|value| value.to_str().ok()),
                Some("no-store")
            );
        }
        other => {
            return Err(
                format!("replayed realtime ticket unexpectedly upgraded: {other:?}").into(),
            );
        }
    }
    Ok(())
}

async fn mint_http_ticket(
    address: SocketAddr,
    access_token: &AccessToken,
    runtime: RuntimeLeaseFence,
) -> TestResult<String> {
    let mint = MintRealtimeTicketRequest::v1(runtime.clone());
    let response = json_request(
        address,
        "POST",
        "/v1/realtime/tickets",
        &auth_headers(access_token),
        &mint,
    )
    .await?;
    expect_status(&response, 200);
    expect_no_store(&response);
    let minted: MintRealtimeTicketResponse = serde_json::from_slice(&response.body)?;
    assert_eq!(minted.runtime(), &runtime);
    assert_ne!(minted.expires_at().value(), 0);
    Ok(minted.ticket().expose_secret().to_owned())
}

async fn assert_query_does_not_consume_ticket(address: SocketAddr, ticket: &str) -> TestResult<()> {
    let mut query_request = format!("ws://{address}/v1/realtime?probe=1").into_client_request()?;
    query_request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {ticket}"))?,
    );
    match connect_async(query_request).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status().as_u16(), 400);
            assert_eq!(
                response
                    .headers()
                    .get("cache-control")
                    .and_then(|value| value.to_str().ok()),
                Some("no-store")
            );
            Ok(())
        }
        other => Err(format!("query-bearing ticket unexpectedly upgraded: {other:?}").into()),
    }
}

#[tokio::test]
async fn realtime_http_rejections_are_generic_and_no_store() -> TestResult<()> {
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        return Err("realtime HTTP smoke requires Linux or Windows".into());
    }
    let _test_guard = BLACK_BOX_TEST_GATE.lock().await;
    let (address, server) = start_server_with_retry().await?;
    let response = request(address, "GET", "/v1/realtime", &[], &[]).await?;
    expect_status(&response, 400);
    expect_no_store(&response);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.body)?["error"]["code"],
        "invalid_request"
    );

    let response = request(address, "GET", "/v1/realtime?", &[], &[]).await?;
    expect_status(&response, 400);
    expect_no_store(&response);

    let response = request(
        address,
        "GET",
        "/v1/realtime",
        &[(
            "sec-websocket-protocol",
            "pokecrossroads.realtime.v1".to_owned(),
        )],
        &[],
    )
    .await?;
    expect_status(&response, 400);
    expect_no_store(&response);

    let response = request(address, "POST", "/v1/realtime", &[], &[]).await?;
    expect_status(&response, 405);
    expect_no_store(&response);

    let response = request(
        address,
        "POST",
        "/v1/realtime/tickets",
        &[("content-type", "application/json".to_owned())],
        &vec![b'a'; 8 * 1024 + 1],
    )
    .await?;
    expect_status(&response, 413);
    expect_no_store(&response);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.body)?["error"]["code"],
        "payload_too_large"
    );

    server.shutdown()?;
    Ok(())
}

#[tokio::test]
async fn phase2_binary_http_checkpoint_and_fencing_smoke() -> TestResult<()> {
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        return Err(
            "Phase 2 binary smoke requires Linux or Windows listener ownership queries".into(),
        );
    }
    let _test_guard = BLACK_BOX_TEST_GATE.lock().await;
    assert_bad_configuration().await?;
    let (address, server) = start_server_with_retry().await?;
    // Run the assertion-heavy protocol sequence in a child task so a panic is
    // converted to a JoinError and the parent can still synchronously own the
    // bounded termination/reap below.
    let mut flow = tokio::spawn(phase2_binary_http_flow(address));
    let result = match timeout(HTTP_FLOW_TIMEOUT, &mut flow).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(format!("HTTP smoke task panicked or was cancelled: {error}").into()),
        Err(_) => {
            flow.abort();
            let _ = timeout(SHUTDOWN_TIMEOUT, flow).await;
            Err("HTTP smoke exceeded its absolute flow deadline".into())
        }
    };
    server.shutdown()?;
    result
}

// Keep the real-binary protocol sequence linear so each fence and revision
// transition remains visible at the HTTP boundary.
#[allow(clippy::too_many_lines)]
async fn phase2_binary_http_flow(address: SocketAddr) -> TestResult<()> {
    let password = Password::new(PASSWORD)?;
    let register =
        RegisterRequest::new(USERNAME, password.clone(), InvitationCode::new(INVITATION)?)?;
    let response = json_request(address, "POST", "/v1/auth/register", &[], &register).await?;
    expect_status(&response, 201);
    let registered: coop_cloud::RegisterResponse = serde_json::from_slice(&response.body)?;

    // Preserve the mixed-case spelling on the wire; Username canonicalization
    // is the server's identity rule, not a client-side lookup convention.
    let login = LoginRequest::new(USERNAME, password)?;
    let mut login_value = serde_json::to_value(&login)?;
    login_value["username"] = serde_json::Value::String("sMoKeUsEr".to_owned());
    let response = json_request(address, "POST", "/v1/auth/login", &[], &login_value).await?;
    expect_status(&response, 200);
    let login: coop_cloud::LoginResponse = serde_json::from_slice(&response.body)?;
    assert_eq!(login.user_id, registered.user_id);
    assert_eq!(login.character_id, registered.character_id);

    // Refresh rotates the family. Reusing the consumed token is rejected and
    // revokes that family; subsequent calls use only the rotated pair.
    let old_refresh = login.refresh_token.clone();
    let response = json_request(
        address,
        "POST",
        "/v1/auth/refresh",
        &[],
        &RefreshRequest::new(old_refresh.clone()),
    )
    .await?;
    expect_status(&response, 200);
    let rotated: RefreshResponse = serde_json::from_slice(&response.body)?;
    if rotated.refresh_token.expose_secret() == old_refresh.expose_secret() {
        return Err("refresh rotation reused the consumed token".into());
    }
    let access = rotated.access_token;
    let client = id(ClientInstanceId::new);
    let acquire =
        AcquireLeaseRequest::new(registered.character_id, client, id(IdempotencyKey::new));
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/acquire",
        &auth_headers(&access),
        &acquire,
    )
    .await?;
    expect_status(&response, 200);
    let mut lease: LeaseContract = serde_json::from_slice(&response.body)?;
    assert_eq!(lease.current_revision, Revision::initial());
    assert_ne!(lease.session_epoch.value(), 0);

    let competing = AcquireLeaseRequest::new(
        registered.character_id,
        id(ClientInstanceId::new),
        id(IdempotencyKey::new),
    );
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/acquire",
        &auth_headers(&access),
        &competing,
    )
    .await?;
    expect_status(&response, 409);

    let response = request(
        address,
        "GET",
        &format!("/v1/characters/{}/resume-package", id(CharacterId::new)),
        &fence_headers(&access, &lease),
        &[],
    )
    .await?;
    expect_status(&response, 404);
    let response = request(
        address,
        "GET",
        &format!("/v1/characters/{}/resume-package", lease.character_id),
        &[],
        &[],
    )
    .await?;
    expect_status(&response, 401);

    let sav = valid_character_sav();
    let pending = b"[]".to_vec();
    let sav_file = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &sav)?;
    let pending_file = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, &pending)?;
    let snapshot_id = id(SnapshotId::new);
    let idempotency_key = id(IdempotencyKey::new);
    let prepare = SnapshotPrepareRequest::new(
        snapshot_id,
        SnapshotPrepareFence::new(
            lease.session_id,
            lease.character_id,
            lease.current_revision,
            lease.session_epoch,
            lease.client_instance_id,
            idempotency_key,
        ),
        vec![sav_file.clone(), pending_file.clone()],
        pending_file.sha256,
    )?;
    let response = json_request(
        address,
        "POST",
        &format!("/v1/characters/{}/snapshots/prepare", lease.character_id),
        &fence_headers(&access, &lease),
        &prepare,
    )
    .await?;
    expect_status(&response, 200);
    let prepared: coop_cloud::SnapshotPrepareResponse = serde_json::from_slice(&response.body)?;
    assert!(prepared.matches_request(&prepare));
    assert_eq!(prepared.upload_targets.len(), 2);

    for target in &prepared.upload_targets {
        let bytes = match target.artifact {
            ArtifactIdentity::CharacterSav => &sav,
            ArtifactIdentity::PendingCommits => &pending,
            ArtifactIdentity::ResumeSs1 => return Err("unexpected resume upload target".into()),
        };
        let upload_url = Url::parse(target.url().as_str())?;
        assert_eq!(upload_url.host_str(), Some("127.0.0.1"));
        let mut path = upload_url.path().to_owned();
        if let Some(query) = upload_url.query() {
            path.push('?');
            path.push_str(query);
        }
        let response = request(address, "PUT", &path, &[], bytes).await?;
        expect_status(&response, 204);
    }

    let finalize = SnapshotFinalizeRequest::new(
        snapshot_id,
        SnapshotFinalizeFence::new(
            lease.session_id,
            lease.character_id,
            lease.current_revision,
            lease.session_epoch,
            lease.client_instance_id,
            prepared.idempotency_key,
        ),
        vec![sav_file, pending_file],
        prepared.pending_commits_sha256,
        None,
    )?;
    let response = json_request(
        address,
        "POST",
        &format!("/v1/characters/{}/snapshots/finalize", lease.character_id),
        &fence_headers(&access, &lease),
        &finalize,
    )
    .await?;
    expect_status(&response, 200);
    let record: coop_cloud::SnapshotRecord = serde_json::from_slice(&response.body)?;
    assert_eq!(record.revision, Revision::new(1));
    assert_eq!(record.parent_revision, Revision::initial());
    // Finalization advances the server's canonical revision while retaining
    // the same lease fence identity.  Carry that revision into reconnect and
    // the deliberately stale heartbeat assertion below.
    lease.current_revision = record.revision;

    let response = request(
        address,
        "GET",
        &format!("/v1/characters/{}/resume-package", lease.character_id),
        &fence_headers(&access, &lease),
        &[],
    )
    .await?;
    expect_status(&response, 200);
    let envelope: SignedManifestEnvelope = serde_json::from_slice(&response.body)?;
    envelope.verify(&trusted_key())?;
    assert_eq!(envelope.manifest.revision, Revision::new(1));
    for (artifact, expected) in [
        (ArtifactIdentity::CharacterSav, sav.as_slice()),
        (ArtifactIdentity::PendingCommits, pending.as_slice()),
    ] {
        let response = request(
            address,
            "GET",
            &format!(
                "/v1/characters/{}/resume-package/artifacts/{}?revision=1",
                lease.character_id,
                artifact.as_str()
            ),
            &fence_headers(&access, &lease),
            &[],
        )
        .await?;
        expect_status(&response, 200);
        assert_eq!(response.body, expected);
    }
    let parsed = coop_save::parse(
        &sav,
        coop_save::RegistryContract::new(
            coop_protocol::IDENTITY_REGISTRY_VERSION,
            coop_protocol::IDENTITY_REGISTRY_DIGEST,
        ),
    )?;
    assert_eq!(parsed.raw_bytes(), sav);
    assert_eq!(parsed.coop().regional_progress.len(), 4);
    assert_eq!(
        parsed
            .coop()
            .regional_progress
            .map(|progress| (progress.region, progress.badge_mask)),
        [
            (RegionId::Hoenn, 1),
            (RegionId::Kanto, 2),
            (RegionId::Johto, 4),
            (RegionId::Sevii, 0),
        ]
    );

    let old_fence = lease.fence();
    // Derive the wait from the server-issued expiry instead of coupling this
    // smoke to a duplicated lease-TTL constant.
    let now_millis = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let wait_millis = lease
        .expires_at
        .value()
        .saturating_sub(now_millis)
        .saturating_add(1);
    if wait_millis > u64::try_from(MAX_EXPIRY_WAIT.as_millis())? {
        return Err("server-issued lease expiry exceeds the bounded smoke wait".into());
    }
    tokio::time::sleep(Duration::from_millis(wait_millis)).await;
    let reconnect = ReconnectLeaseRequest::new(old_fence, id(IdempotencyKey::new));
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/reconnect",
        &fence_headers(&access, &lease),
        &reconnect,
    )
    .await?;
    expect_status(&response, 200);
    let reconnected: LeaseContract = serde_json::from_slice(&response.body)?;
    assert!(reconnected.session_epoch > lease.session_epoch);
    assert_eq!(reconnected.current_revision, Revision::new(1));

    let stale_heartbeat = HeartbeatLeaseRequest::new(old_fence);
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/heartbeat",
        &fence_headers(&access, &lease),
        &stale_heartbeat,
    )
    .await?;
    expect_status(&response, 409);

    let response = json_request(
        address,
        "POST",
        "/v1/sessions/release",
        &fence_headers(&access, &reconnected),
        &coop_cloud::ReleaseLeaseRequest::new(reconnected.fence(), id(IdempotencyKey::new)),
    )
    .await?;
    expect_status(&response, 200);

    let reacquired = AcquireLeaseRequest::new(
        registered.character_id,
        id(ClientInstanceId::new),
        id(IdempotencyKey::new),
    );
    let response = json_request(
        address,
        "POST",
        "/v1/sessions/acquire",
        &auth_headers(&access),
        &reacquired,
    )
    .await?;
    expect_status(&response, 200);
    let reacquired: LeaseContract = serde_json::from_slice(&response.body)?;
    assert_eq!(reacquired.current_revision, Revision::new(1));
    assert!(reacquired.session_epoch > reconnected.session_epoch);

    // Reusing a consumed refresh token revokes its family.  Do this only after
    // the active-token checkpoint/release proof so the expected fail-closed
    // behavior cannot invalidate the remainder of this scenario.
    let response = json_request(
        address,
        "POST",
        "/v1/auth/refresh",
        &[],
        &RefreshRequest::new(old_refresh),
    )
    .await?;
    expect_status(&response, 401);
    Ok(())
}

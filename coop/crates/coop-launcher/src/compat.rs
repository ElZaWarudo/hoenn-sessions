//! Local build and emulator compatibility validation.

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use coop_cloud::{
    BridgeAbiVersion, CompatibilityTarget, GameBuildId, MgbaVersion, ProtocolVersion, Revision,
    Sha256Digest,
};
use serde::Deserialize;
use thiserror::Error;

pub const EXPECTED_MGBA_VERSION: &str = "0.10.5";
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_MGBA_OUTPUT_BYTES: usize = 64 * 1024;
pub const MGBA_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
pub const BRIDGE_MANIFEST_SCHEMA: u16 = 1;
pub const BRIDGE_ABI: u16 = 1;
pub const GAME_PROTOCOL: u16 = 1;
pub const BRIDGE_MESSAGE_BYTES: u64 = 144;
pub const BRIDGE_PAYLOAD_BYTES: u64 = 128;
pub const BRIDGE_QUEUE_CAPACITY: u64 = 32;
pub const BRIDGE_SIZE: u64 = 9244;

#[derive(Debug, Error)]
pub enum CompatibilityError {
    #[error("bridge manifest cannot be read")]
    Read(#[source] io::Error),
    #[error("bridge manifest is invalid")]
    Manifest,
    #[error("ROM cannot be read")]
    Rom(#[source] io::Error),
    #[error("ROM SHA-256 does not match the bridge manifest")]
    RomHash,
    #[error("mGBA executable cannot be started")]
    Mgba(#[source] io::Error),
    #[error("mGBA output is too large")]
    MgbaOutput,
    #[error("mGBA version probe timed out")]
    MgbaTimeout,
    #[error("unsupported mGBA version")]
    MgbaVersion,
    #[error("unsupported bridge ABI or protocol")]
    Protocol,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildCompatibility {
    pub target: CompatibilityTarget,
    pub manifest: BridgeManifest,
    pub rom_path: PathBuf,
    pub mgba_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeManifest {
    pub schema_version: u16,
    pub game_build: GameBuildManifest,
    pub net_bridge: NetBridgeManifest,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameBuildManifest {
    pub id: String,
    pub numeric_id: u32,
    pub rom_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetBridgeManifest {
    pub symbol: String,
    pub address: u64,
    pub size: u64,
    pub magic: u32,
    pub abi_version: u16,
    pub game_protocol_version: u16,
    pub byte_order: String,
    pub checksum: ChecksumManifest,
    pub offsets: BridgeOffsets,
    pub queue: QueueManifest,
    pub message: MessageManifest,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecksumManifest {
    pub algorithm: String,
    pub covered_bytes: [u16; 2],
    pub stored_offset: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeOffsets {
    pub magic: u16,
    pub abi_version: u16,
    pub game_protocol_version: u16,
    pub game_build_id: u16,
    pub status_flags: u16,
    pub last_sidecar_heartbeat: u16,
    pub game_to_network: u16,
    pub network_to_game: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueManifest {
    pub capacity: u16,
    pub size: u16,
    pub read_index_offset: u16,
    pub write_index_offset: u16,
    pub entries_offset: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageManifest {
    pub size: u16,
    pub payload_size: u16,
    pub offsets: MessageOffsets,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageOffsets {
    #[serde(rename = "type")]
    pub message_type: u16,
    pub length: u16,
    pub sequence: u16,
    pub session_epoch: u16,
    pub payload: u16,
    pub checksum: u16,
}

impl BuildCompatibility {
    /// Validates every generated bridge constant and hashes the complete ROM.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest, ROM digest, or exact emulator
    /// version does not satisfy the bridge compatibility contract.
    pub fn validate(
        manifest_path: impl AsRef<Path>,
        rom_path: impl AsRef<Path>,
        mgba_path: impl AsRef<Path>,
    ) -> Result<Self, CompatibilityError> {
        let manifest_path = manifest_path.as_ref();
        let rom_path = rom_path.as_ref();
        let mgba_path = mgba_path.as_ref();
        let manifest_file = File::open(manifest_path).map_err(CompatibilityError::Read)?;
        let mut manifest_bytes = Vec::new();
        manifest_file
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut manifest_bytes)
            .map_err(CompatibilityError::Read)?;
        if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(CompatibilityError::Manifest);
        }
        let manifest: BridgeManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|_| CompatibilityError::Manifest)?;
        validate_manifest(&manifest)?;
        let rom_sha256 = hash_file(rom_path).map_err(CompatibilityError::Rom)?;
        let expected_hash = Sha256Digest::parse(&manifest.game_build.rom_sha256)
            .map_err(|_| CompatibilityError::Manifest)?;
        if rom_sha256 != expected_hash {
            return Err(CompatibilityError::RomHash);
        }
        probe_mgba(mgba_path)?;
        let game_build_id = GameBuildId::new(manifest.game_build.id.clone())
            .map_err(|_| CompatibilityError::Manifest)?;
        let target = CompatibilityTarget::new(
            game_build_id,
            rom_sha256,
            MgbaVersion::new(EXPECTED_MGBA_VERSION).map_err(|_| CompatibilityError::Manifest)?,
            BridgeAbiVersion::new(BRIDGE_ABI).map_err(|_| CompatibilityError::Protocol)?,
            ProtocolVersion::new(GAME_PROTOCOL).map_err(|_| CompatibilityError::Protocol)?,
            Revision::initial(),
        );
        Ok(Self {
            target,
            manifest,
            rom_path: rom_path.to_owned(),
            mgba_path: mgba_path.to_owned(),
        })
    }
}

fn validate_manifest(manifest: &BridgeManifest) -> Result<(), CompatibilityError> {
    let game = &manifest.game_build;
    let bridge = &manifest.net_bridge;
    GameBuildId::new(game.id.clone()).map_err(|_| CompatibilityError::Manifest)?;
    if manifest.schema_version != BRIDGE_MANIFEST_SCHEMA
        || game.id.is_empty()
        || game.id.len() > 128
        || !game.id.is_ascii()
        || game.numeric_id != 0x0001_0000
        || bridge.symbol != "gCoopNetBridge"
        || bridge.address < 0x0200_0000
        || bridge
            .address
            .checked_add(bridge.size)
            .is_none_or(|end| end > 0x0204_0000)
        || bridge.size != BRIDGE_SIZE
        || bridge.magic != 0x504B_434F
        || bridge.abi_version != BRIDGE_ABI
        || bridge.game_protocol_version != GAME_PROTOCOL
        || bridge.byte_order != "little"
        || bridge.checksum.algorithm != "CRC-32/IEEE"
        || bridge.checksum.covered_bytes != [0, 139]
        || bridge.checksum.stored_offset != 140
        || u64::from(bridge.queue.capacity) != BRIDGE_QUEUE_CAPACITY
        || bridge.queue.size != 4612
        || bridge.queue.read_index_offset != 0
        || bridge.queue.write_index_offset != 2
        || bridge.queue.entries_offset != 4
        || u64::from(bridge.message.size) != BRIDGE_MESSAGE_BYTES
        || u64::from(bridge.message.payload_size) != BRIDGE_PAYLOAD_BYTES
        || bridge.message.offsets.message_type != 0
        || bridge.message.offsets.length != 2
        || bridge.message.offsets.sequence != 4
        || bridge.message.offsets.session_epoch != 8
        || bridge.message.offsets.payload != 12
        || bridge.message.offsets.checksum != 140
    {
        return Err(CompatibilityError::Manifest);
    }
    if Sha256Digest::parse(&game.rom_sha256).is_err() {
        return Err(CompatibilityError::Manifest);
    }
    Ok(())
}

fn hash_file(path: &Path) -> io::Result<Sha256Digest> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(Sha256Digest::from_bytes(digest.finalize().into()))
}

fn probe_mgba(path: &Path) -> Result<(), CompatibilityError> {
    let mut command = Command::new(path);
    let path_variable = std::env::var_os("PATH");
    command.env_clear();
    if let Some(path_variable) = path_variable {
        command.env("PATH", path_variable);
    }
    let mut child = command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(CompatibilityError::Mgba)?;
    let stdout = child.stdout.take().ok_or(CompatibilityError::MgbaOutput)?;
    let stderr = child.stderr.take().ok_or(CompatibilityError::MgbaOutput)?;
    let (sender, receiver) = mpsc::channel();
    spawn_bounded_reader(stdout, false, sender.clone());
    spawn_bounded_reader(stderr, true, sender);

    let deadline = Instant::now() + MGBA_PROBE_TIMEOUT;
    let status = loop {
        if let Ok(result) = receiver.try_recv()
            && result.oversized
        {
            terminate_probe_child(&mut child);
            return Err(CompatibilityError::MgbaOutput);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // The root has already exited and its PID may be reused. Do
                // not invoke a PID-based tree terminator here; bounded output
                // collection below abandons descendant-held readers after a
                // deadline instead.
                break status;
            }
            Ok(None) if Instant::now() >= deadline => {
                terminate_probe_child(&mut child);
                return Err(CompatibilityError::MgbaTimeout);
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                terminate_probe_child(&mut child);
                return Err(CompatibilityError::Mgba(error));
            }
        }
    };
    let (stdout, stderr) = receive_probe_outputs(&receiver, deadline)?;
    if stdout.1 || stderr.1 {
        return Err(CompatibilityError::MgbaOutput);
    }
    let mut text = String::from_utf8_lossy(&stdout.0).into_owned();
    text.push_str(&String::from_utf8_lossy(&stderr.0));
    if !status.success()
        || !text.lines().any(|line| {
            let line = line.trim();
            line.contains("mGBA")
                && line
                    .split_whitespace()
                    .any(|token| token == EXPECTED_MGBA_VERSION)
        })
    {
        return Err(CompatibilityError::MgbaVersion);
    }
    Ok(())
}

type ProbeCapture = (Vec<u8>, bool);

struct ProbeOutput {
    stderr: bool,
    bytes: Vec<u8>,
    oversized: bool,
}

fn spawn_bounded_reader<R: Read + Send + 'static>(
    mut reader: R,
    stderr: bool,
    sender: mpsc::Sender<ProbeOutput>,
) {
    thread::spawn(move || {
        let mut output = Vec::with_capacity(MAX_MGBA_OUTPUT_BYTES);
        let mut buffer = [0_u8; 4096];
        let mut oversized = false;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if append_bounded_probe_bytes(&mut output, &buffer[..count]) {
                        oversized = true;
                        break;
                    }
                }
            }
        }
        let _ = sender.send(ProbeOutput {
            stderr,
            bytes: output,
            oversized,
        });
    });
}

fn receive_probe_outputs(
    receiver: &Receiver<ProbeOutput>,
    deadline: Instant,
) -> Result<(ProbeCapture, ProbeCapture), CompatibilityError> {
    // A descendant may retain an inherited pipe after the probe root exits.
    // Bound this join independently and abandon the reader rather than using
    // a potentially recycled root PID for tree termination.
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or_default()
        .saturating_add(Duration::from_secs(1));
    let mut stdout = None;
    let mut stderr = None;
    while stdout.is_none() || stderr.is_none() {
        let result = receiver
            .recv_timeout(remaining)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => CompatibilityError::MgbaTimeout,
                RecvTimeoutError::Disconnected => CompatibilityError::MgbaOutput,
            })?;
        if result.stderr {
            if stderr.replace((result.bytes, result.oversized)).is_some() {
                return Err(CompatibilityError::MgbaOutput);
            }
        } else if stdout.replace((result.bytes, result.oversized)).is_some() {
            return Err(CompatibilityError::MgbaOutput);
        }
    }
    Ok((
        stdout.expect("stdout result checked"),
        stderr.expect("stderr result checked"),
    ))
}

fn terminate_probe_child(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        // `Child::kill` only terminates the direct process on Windows.  Use
        // the OS-provided process-tree terminator with a bounded wait only
        // while the root is still live (timeout/output failure). Calling it
        // after `try_wait` observed exit could target a recycled PID.
        let root_exited = matches!(child.try_wait(), Ok(Some(_)));
        if !root_exited {
            let pid = child.id();
            if pid != 0
                && let Some((taskkill, _taskkill_guard, _system32_guard)) = trusted_taskkill()
            {
                let mut command = Command::new(taskkill);
                command
                    .env_clear()
                    .arg("/PID")
                    .arg(pid.to_string())
                    .arg("/T")
                    .arg("/F")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if let Ok(mut killer) = command.spawn() {
                    let deadline = Instant::now() + Duration::from_secs(1);
                    loop {
                        match killer.try_wait() {
                            Ok(Some(_)) | Err(_) => break,
                            Ok(None) if Instant::now() >= deadline => {
                                let _ = killer.kill();
                                let _ = killer.wait();
                                break;
                            }
                            Ok(None) => thread::sleep(Duration::from_millis(10)),
                        }
                    }
                }
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn trusted_taskkill() -> Option<(std::path::PathBuf, std::fs::File, std::fs::File)> {
    use std::os::windows::fs::OpenOptionsExt;
    // System32 is the only accepted process-tree terminator location.  A
    // missing or redirected installation fails closed to direct-child kill.
    let taskkill = std::path::PathBuf::from(r"C:\Windows\System32\taskkill.exe");
    let mut parent_options = std::fs::OpenOptions::new();
    parent_options
        .read(true)
        .share_mode(0x0000_0003)
        .custom_flags(0x0220_0000);
    let parent = parent_options.open(taskkill.parent()?).ok()?;
    let canonical = std::fs::canonicalize(&taskkill).ok()?;
    if canonical != taskkill {
        return None;
    }
    let mut file_options = std::fs::OpenOptions::new();
    file_options
        .read(true)
        .share_mode(0x0000_0001)
        .custom_flags(0x0020_0000);
    let file = file_options.open(&taskkill).ok()?;
    Some((taskkill, file, parent))
}

fn append_bounded_probe_bytes(output: &mut Vec<u8>, chunk: &[u8]) -> bool {
    if output.len().saturating_add(chunk.len()) > MAX_MGBA_OUTPUT_BYTES {
        return true;
    }
    output.extend_from_slice(chunk);
    false
}

#[cfg(test)]
mod tests {
    use super::{MAX_MGBA_OUTPUT_BYTES, append_bounded_probe_bytes};

    #[test]
    fn probe_output_is_bounded_without_storing_untrusted_bytes() {
        let input = vec![b'x'; MAX_MGBA_OUTPUT_BYTES];
        let mut output = Vec::new();
        assert!(!append_bounded_probe_bytes(&mut output, &input));
        assert!(append_bounded_probe_bytes(&mut output, b"x"));
        assert_eq!(output.len(), MAX_MGBA_OUTPUT_BYTES);
    }
}

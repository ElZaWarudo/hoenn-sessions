//! Local build and emulator compatibility validation.

use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(not(windows))]
use std::{
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Instant,
};

use coop_cloud::{
    BridgeAbiVersion, CompatibilityTarget, GameBuildId, MgbaVersion, ProtocolVersion, Revision,
    Sha256Digest,
};
use coop_protocol::{IDENTITY_REGISTRY_DIGEST, IDENTITY_REGISTRY_VERSION};
use coop_save::RegistryContract;
use serde::Deserialize;
use thiserror::Error;

pub const EXPECTED_MGBA_VERSION: &str = "0.10.5";
pub const EXPECTED_MGBA_PLATFORM: &str = "windows-x64";
pub const EXPECTED_MGBA_VARIANT: &str = "Qt";
pub const EXPECTED_MGBA_EXECUTABLE_SHA256: &str =
    "5a3c98c2984dd04bd0d7c9378cdfae937ae0d73a196c880bb2eecf3b254af247";
pub const EXPECTED_MGBA_ARCHIVE_SHA256: &str =
    "b497a57c7d9093834dadc64f33a90f7c411439c21fdb8a0143255a45ea37563a";
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Keep this in sync with the CLI's source-ROM admission limit.  The public
/// compatibility API is also callable without the CLI, so it must enforce
/// the same bound before hashing an untrusted path.
pub const MAX_ROM_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MGBA_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_MGBA_OUTPUT_BYTES: usize = 64 * 1024;
pub const MGBA_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
pub const BRIDGE_MANIFEST_SCHEMA: u16 = 3;
pub const BRIDGE_ABI: u16 = 1;
pub const GAME_PROTOCOL: u16 = 1;
pub const BRIDGE_MESSAGE_BYTES: u64 = 144;
pub const BRIDGE_PAYLOAD_BYTES: u64 = 128;
pub const BRIDGE_QUEUE_CAPACITY: u64 = 32;
pub const BRIDGE_SIZE: u64 = 9244;

pub(crate) fn expected_mgba_executable_digest() -> [u8; 32] {
    let encoded = EXPECTED_MGBA_EXECUTABLE_SHA256.as_bytes();
    let mut digest = [0_u8; 32];
    let mut index = 0;
    while index < digest.len() {
        digest[index] = (hex_value(encoded[index * 2]) << 4) | hex_value(encoded[index * 2 + 1]);
        index += 1;
    }
    digest
}

const fn hex_value(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

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
    #[error("official mGBA executable identity does not match the pinned artifact")]
    MgbaIdentity,
    #[error("mGBA output is too large")]
    MgbaOutput,
    #[error("mGBA probe descendant cleanup could not be confirmed")]
    MgbaCleanup,
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
    pub save: SaveManifest,
    #[serde(default)]
    pub emulator: Option<EmulatorManifest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmulatorManifest {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub variant: String,
    pub archive_sha256: String,
    pub executable_sha256: String,
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

/// The linked-ROM `SaveBlock3` contract. Keep these fields separate from the
/// bridge fields: a ROM can expose a valid network bridge while its persisted
/// co-op payload is absent or incompatible.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveManifest {
    pub block3_address: u64,
    pub coop_offset: u16,
    pub generation_offset: u16,
    pub generation_address: u64,
    pub crc_offset: u16,
    pub schema_version: u16,
    pub struct_size: u16,
    pub registry_version: u32,
    pub registry_digest: String,
}

impl SaveManifest {
    /// Returns the protocol identity registry contract after validating its
    /// canonical version and truncated SHA-256 digest.
    ///
    /// # Errors
    ///
    /// Returns [`CompatibilityError::Manifest`] when the version or digest is
    /// not the pinned protocol registry contract.
    pub fn registry_contract(&self) -> Result<RegistryContract, CompatibilityError> {
        let mut digest = [0_u8; 16];
        let bytes = self.registry_digest.as_bytes();
        if bytes.len() != 32 {
            return Err(CompatibilityError::Manifest);
        }
        for (index, chunk) in bytes.chunks_exact(2).enumerate() {
            digest[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
        }
        if self.registry_version != IDENTITY_REGISTRY_VERSION || digest != IDENTITY_REGISTRY_DIGEST
        {
            return Err(CompatibilityError::Manifest);
        }
        Ok(RegistryContract::new(self.registry_version, digest))
    }
}

fn hex_nibble(value: u8) -> Result<u8, CompatibilityError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(CompatibilityError::Manifest),
    }
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
        let manifest_file =
            open_bounded_regular_file(manifest_path, MAX_MANIFEST_BYTES).map_err(|error| {
                if error.kind() == io::ErrorKind::InvalidData {
                    CompatibilityError::Manifest
                } else {
                    CompatibilityError::Read(error)
                }
            })?;
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
        #[cfg(windows)]
        crate::process::with_mgba_executable_guard(mgba_path, |path, digest| {
            if digest != crate::compat::expected_mgba_executable_digest() {
                return Err(CompatibilityError::MgbaIdentity);
            }
            probe_mgba(path)
        })
        .map_err(|error| match error {
            crate::process::ProcessError::MgbaIdentity => CompatibilityError::MgbaIdentity,
            crate::process::ProcessError::Spawn(source) => CompatibilityError::Mgba(source),
            _ => CompatibilityError::Mgba(io::Error::other("mGBA executable binding failed")),
        })??;
        #[cfg(not(windows))]
        {
            let mgba_sha256 = hash_file_bounded(mgba_path, MAX_MGBA_EXECUTABLE_BYTES)
                .map_err(CompatibilityError::Mgba)?;
            let expected_mgba_sha256 = Sha256Digest::parse(EXPECTED_MGBA_EXECUTABLE_SHA256)
                .map_err(|_| CompatibilityError::Manifest)?;
            if mgba_sha256 != expected_mgba_sha256 {
                return Err(CompatibilityError::MgbaIdentity);
            }
            probe_mgba(mgba_path)?;
        }
        let rom_sha256 = hash_file(rom_path).map_err(CompatibilityError::Rom)?;
        let expected_hash = Sha256Digest::parse(&manifest.game_build.rom_sha256)
            .map_err(|_| CompatibilityError::Manifest)?;
        if rom_sha256 != expected_hash {
            return Err(CompatibilityError::RomHash);
        }
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
    let Some(emulator) = manifest.emulator.as_ref() else {
        return Err(CompatibilityError::Manifest);
    };
    if emulator.name != "mGBA"
        || emulator.version != EXPECTED_MGBA_VERSION
        || emulator.platform != EXPECTED_MGBA_PLATFORM
        || emulator.variant != EXPECTED_MGBA_VARIANT
        || Sha256Digest::parse(&emulator.archive_sha256).is_err()
        || Sha256Digest::parse(&emulator.executable_sha256).is_err()
        || emulator.archive_sha256 != EXPECTED_MGBA_ARCHIVE_SHA256
        || emulator.executable_sha256 != EXPECTED_MGBA_EXECUTABLE_SHA256
    {
        return Err(CompatibilityError::Manifest);
    }
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
        || bridge.offsets.magic != 0
        || bridge.offsets.abi_version != 4
        || bridge.offsets.game_protocol_version != 6
        || bridge.offsets.game_build_id != 8
        || bridge.offsets.status_flags != 12
        || bridge.offsets.last_sidecar_heartbeat != 16
        || bridge.offsets.game_to_network != 20
        || bridge.offsets.network_to_game != 4632
    {
        return Err(CompatibilityError::Manifest);
    }
    if Sha256Digest::parse(&game.rom_sha256).is_err() {
        return Err(CompatibilityError::Manifest);
    }
    let save = &manifest.save;
    if save.block3_address < 0x0200_0000
        || !save.block3_address.is_multiple_of(4)
        || save
            .block3_address
            .checked_add(u64::from(save.struct_size) + u64::from(save.coop_offset))
            .is_none_or(|end| end > 0x0204_0000)
        || save.coop_offset
            != u16::try_from(coop_save::COOP_SAVE_OFFSET).expect("frozen save ABI fits u16")
        || save.generation_offset != 28
        || save
            .block3_address
            .checked_add(u64::from(save.coop_offset))
            .and_then(|address| address.checked_add(u64::from(save.generation_offset)))
            != Some(save.generation_address)
        || save.crc_offset != 668
        || save.schema_version != coop_save::COOP_SAVE_V1_SCHEMA_VERSION
        || save.struct_size
            != u16::try_from(coop_save::COOP_SAVE_V1_SIZE).expect("frozen save ABI fits u16")
        || save.registry_contract().is_err()
    {
        return Err(CompatibilityError::Manifest);
    }
    Ok(())
}

fn hash_file(path: &Path) -> io::Result<Sha256Digest> {
    hash_file_bounded(path, MAX_ROM_BYTES)
}

fn hash_file_bounded(path: &Path, max_bytes: u64) -> io::Result<Sha256Digest> {
    use sha2::{Digest, Sha256};
    let mut file = open_bounded_regular_file(path, max_bytes)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ROM is too large"))?;
        if total > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ROM is too large",
            ));
        }
        digest.update(&buffer[..read]);
    }
    Ok(Sha256Digest::from_bytes(digest.finalize().into()))
}

/// Open a bounded, regular input without following the final path component
/// (on platforms with a no-follow primitive).  The metadata check is made on
/// the open handle as well as before opening so a FIFO, device, directory, or
/// a path replaced during validation cannot turn the bounded read into an
/// unbounded/special-file operation.
fn open_bounded_regular_file(path: &Path, max_bytes: u64) -> io::Result<File> {
    reject_symlink_components(path)?;
    let path_metadata = fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "compatibility input is not a regular file",
        ));
    }
    if path_metadata.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compatibility input is too large",
        ));
    }

    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT prevents the final component from
        // being followed through a reparse point.  The metadata check below
        // then rejects that component unless it is an ordinary file.
        options.custom_flags(0x0020_0000);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW closes the final-component symlink race.  O_NONBLOCK
        // ensures a replacement FIFO cannot make open block before its type
        // is checked on the resulting handle.
        options.custom_flags(0x0002_0000 | 0x0000_0800);
    }
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // BSD and Darwin expose O_NOFOLLOW with this platform family value.
        options.custom_flags(0x0000_0100);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "compatibility input is not a regular file",
        ));
    }
    if metadata.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compatibility input is too large",
        ));
    }
    Ok(file)
}

fn reject_symlink_components(path: &Path) -> io::Result<()> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "compatibility input contains a symlink",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(())
}

#[cfg(windows)]
fn probe_mgba(path: &Path) -> Result<(), CompatibilityError> {
    let output =
        crate::windows_mgba_supervisor::probe(path, &["--version".to_owned()], MGBA_PROBE_TIMEOUT)
            .map_err(|failure| match failure {
                crate::windows_mgba_supervisor::ProbeFailure::Spawn(error)
                | crate::windows_mgba_supervisor::ProbeFailure::Output(error) => {
                    CompatibilityError::Mgba(error)
                }
                crate::windows_mgba_supervisor::ProbeFailure::Cleanup(_) => {
                    CompatibilityError::MgbaCleanup
                }
                crate::windows_mgba_supervisor::ProbeFailure::OutputTooLarge => {
                    CompatibilityError::MgbaOutput
                }
                crate::windows_mgba_supervisor::ProbeFailure::Timeout => {
                    CompatibilityError::MgbaTimeout
                }
            })?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success()
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

#[cfg(not(windows))]
fn probe_mgba(path: &Path) -> Result<(), CompatibilityError> {
    let deadline = Instant::now() + MGBA_PROBE_TIMEOUT;
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
    let Some(stdout) = child.stdout.take() else {
        if !terminate_probe_child(&mut child, deadline) {
            return Err(CompatibilityError::MgbaCleanup);
        }
        return Err(CompatibilityError::MgbaOutput);
    };
    let Some(stderr) = child.stderr.take() else {
        if !terminate_probe_child(&mut child, deadline) {
            return Err(CompatibilityError::MgbaCleanup);
        }
        return Err(CompatibilityError::MgbaOutput);
    };
    let (sender, receiver) = mpsc::channel();
    spawn_bounded_reader(stdout, false, sender.clone());
    spawn_bounded_reader(stderr, true, sender);

    let mut early_outputs = Vec::with_capacity(2);
    let status = loop {
        while let Ok(result) = receiver.try_recv() {
            if result.oversized {
                if !terminate_probe_child(&mut child, deadline) {
                    return Err(CompatibilityError::MgbaCleanup);
                }
                return Err(CompatibilityError::MgbaOutput);
            }
            early_outputs.push(result);
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
                if !terminate_probe_child(&mut child, deadline) {
                    return Err(CompatibilityError::MgbaCleanup);
                }
                return Err(CompatibilityError::MgbaTimeout);
            }
            Ok(None) => {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or_default();
                if remaining.is_zero() {
                    if !terminate_probe_child(&mut child, deadline) {
                        return Err(CompatibilityError::MgbaCleanup);
                    }
                    return Err(CompatibilityError::MgbaTimeout);
                }
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
            Err(error) => {
                if !terminate_probe_child(&mut child, deadline) {
                    return Err(CompatibilityError::MgbaCleanup);
                }
                return Err(CompatibilityError::Mgba(error));
            }
        }
    };
    let (stdout, stderr) = match receive_probe_outputs(&receiver, deadline, early_outputs) {
        Ok(outputs) => outputs,
        Err(error) => {
            if !terminate_probe_child(&mut child, deadline) {
                return Err(CompatibilityError::MgbaCleanup);
            }
            return Err(error);
        }
    };
    if stdout.1 || stderr.1 {
        if !terminate_probe_child(&mut child, deadline) {
            return Err(CompatibilityError::MgbaCleanup);
        }
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

#[cfg(not(windows))]
type ProbeCapture = (Vec<u8>, bool);

#[cfg(not(windows))]
struct ProbeOutput {
    stderr: bool,
    bytes: Vec<u8>,
    oversized: bool,
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
fn receive_probe_outputs(
    receiver: &Receiver<ProbeOutput>,
    deadline: Instant,
    initial: Vec<ProbeOutput>,
) -> Result<(ProbeCapture, ProbeCapture), CompatibilityError> {
    // A descendant may retain an inherited pipe after the probe root exits.
    // Use the probe's one absolute deadline and abandon the reader rather than
    // using a potentially recycled root PID for tree termination.
    let mut stdout = None;
    let mut stderr = None;
    for result in initial {
        store_probe_output(result, &mut stdout, &mut stderr)?;
    }
    while stdout.is_none() || stderr.is_none() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        if remaining.is_zero() {
            return Err(CompatibilityError::MgbaTimeout);
        }
        let result = receiver
            .recv_timeout(remaining)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => CompatibilityError::MgbaTimeout,
                RecvTimeoutError::Disconnected => CompatibilityError::MgbaOutput,
            })?;
        if result.oversized {
            // Stop waiting for the sibling reader. The caller will terminate
            // the root while it is still live, or return MgbaCleanup when the
            // root has already exited and descendants cannot be proven gone.
            return Err(CompatibilityError::MgbaOutput);
        }
        store_probe_output(result, &mut stdout, &mut stderr)?;
    }
    Ok((
        stdout.expect("stdout result checked"),
        stderr.expect("stderr result checked"),
    ))
}

#[cfg(not(windows))]
fn store_probe_output(
    result: ProbeOutput,
    stdout: &mut Option<ProbeCapture>,
    stderr: &mut Option<ProbeCapture>,
) -> Result<(), CompatibilityError> {
    let destination = if result.stderr { stderr } else { stdout };
    if destination
        .replace((result.bytes, result.oversized))
        .is_some()
    {
        return Err(CompatibilityError::MgbaOutput);
    }
    Ok(())
}

#[cfg(not(windows))]
fn terminate_probe_child(child: &mut std::process::Child, deadline: Instant) -> bool {
    // If the root has already exited, its PID is no longer a safe process-tree
    // handle. A descendant may still own either pipe, and without a Job Object
    // there is no safe std-only way to prove those descendants are gone.
    let root_exited = matches!(child.try_wait(), Ok(Some(_)));
    // The portable standard library can reap the direct child, but cannot
    // prove that inherited pipe descendants were terminated. The production
    // Windows boundary uses a Job Object in `windows_mgba_supervisor`.
    let mut cleanup_confirmed = false;
    let _ = root_exited;
    if child.kill().is_err() && !matches!(child.try_wait(), Ok(Some(_))) {
        cleanup_confirmed = false;
    }
    let root_reaped = wait_for_probe_child(child, deadline).is_some();
    if !root_reaped {
        cleanup_confirmed = false;
    }
    cleanup_confirmed
}

#[cfg(not(windows))]
fn wait_for_probe_child(
    child: &mut std::process::Child,
    deadline: Instant,
) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or_default();
                if remaining.is_zero() {
                    return None;
                }
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
            Err(_) => return None,
        }
    }
}

#[cfg(not(windows))]
fn append_bounded_probe_bytes(output: &mut Vec<u8>, chunk: &[u8]) -> bool {
    if output.len().saturating_add(chunk.len()) > MAX_MGBA_OUTPUT_BYTES {
        return true;
    }
    output.extend_from_slice(chunk);
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(not(windows))]
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    use super::{CompatibilityError, MAX_MANIFEST_BYTES, open_bounded_regular_file};

    #[cfg(not(windows))]
    use super::{
        MAX_MGBA_OUTPUT_BYTES, ProbeOutput, append_bounded_probe_bytes, receive_probe_outputs,
    };

    #[cfg(not(windows))]
    #[test]
    fn probe_output_is_bounded_without_storing_untrusted_bytes() {
        let input = vec![b'x'; MAX_MGBA_OUTPUT_BYTES];
        let mut output = Vec::new();
        assert!(!append_bounded_probe_bytes(&mut output, &input));
        assert!(append_bounded_probe_bytes(&mut output, b"x"));
        assert_eq!(output.len(), MAX_MGBA_OUTPUT_BYTES);
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_output_collection_uses_one_absolute_deadline() {
        let (_sender, receiver) = mpsc::channel();
        let started = Instant::now();
        assert!(matches!(
            receive_probe_outputs(&receiver, Instant::now(), Vec::new()),
            Err(CompatibilityError::MgbaTimeout)
        ));
        assert!(started.elapsed() < Duration::from_millis(80));
    }

    #[cfg(not(windows))]
    #[test]
    fn oversized_probe_output_aborts_collection_immediately() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(ProbeOutput {
                stderr: false,
                bytes: Vec::new(),
                oversized: true,
            })
            .unwrap();
        assert!(matches!(
            receive_probe_outputs(
                &receiver,
                Instant::now() + Duration::from_secs(1),
                Vec::new(),
            ),
            Err(CompatibilityError::MgbaOutput)
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn early_probe_output_is_preserved_for_final_collection() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(ProbeOutput {
                stderr: true,
                bytes: b"mGBA 0.10.5".to_vec(),
                oversized: false,
            })
            .unwrap();
        let early = vec![ProbeOutput {
            stderr: false,
            bytes: b"version".to_vec(),
            oversized: false,
        }];
        let (stdout, stderr) =
            receive_probe_outputs(&receiver, Instant::now() + Duration::from_secs(1), early)
                .unwrap();
        assert_eq!(stdout.0, b"version");
        assert_eq!(stderr.0, b"mGBA 0.10.5");
    }

    #[test]
    fn bounded_input_rejects_non_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(open_bounded_regular_file(directory.path(), MAX_MANIFEST_BYTES).is_err());
    }

    #[test]
    fn bounded_input_rejects_oversized_files_before_reading() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("oversized");
        fs::write(&path, b"1234").unwrap();
        assert!(open_bounded_regular_file(&path, 3).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn bounded_input_rejects_symlinked_files() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let link = directory.path().join("link");
        fs::write(&target, b"target").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(open_bounded_regular_file(&link, MAX_MANIFEST_BYTES).is_err());
    }

    #[test]
    fn checked_manifest_requires_the_pinned_emulator_contract() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("dist")
            .join("bridge_manifest.json");
        let value: serde_json::Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
        for mutation in ["missing", "wrong-version", "unknown-field", "wrong-digest"] {
            let mut mutated = value.clone();
            match mutation {
                "missing" => {
                    mutated.as_object_mut().unwrap().remove("emulator");
                }
                "wrong-version" => {
                    mutated["emulator"]["version"] = serde_json::Value::from("0.10.4");
                }
                "unknown-field" => {
                    mutated["emulator"]["unexpected"] = serde_json::Value::from(true);
                }
                "wrong-digest" => {
                    mutated["emulator"]["executable_sha256"] =
                        serde_json::Value::from("00".repeat(32));
                }
                _ => unreachable!(),
            }
            let directory = tempfile::tempdir().unwrap();
            let manifest = directory.path().join("manifest.json");
            fs::write(&manifest, serde_json::to_vec(&mutated).unwrap()).unwrap();
            assert!(matches!(
                super::BuildCompatibility::validate(&manifest, "missing.gba", "missing.exe"),
                Err(CompatibilityError::Manifest)
            ));
        }
    }

    #[test]
    fn wrong_executable_digest_is_rejected_before_probe() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("dist")
            .join("bridge_manifest.json");
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("manifest.json");
        fs::copy(source, &manifest).unwrap();
        let executable = directory.path().join("mGBA.exe");
        fs::write(&executable, b"not the official mGBA executable").unwrap();
        assert!(matches!(
            super::BuildCompatibility::validate(&manifest, "missing.gba", &executable),
            Err(CompatibilityError::MgbaIdentity)
        ));
    }
}

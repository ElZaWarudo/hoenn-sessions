//! Argument-vector-only process supervision and sidecar control connection.

use std::{
    fmt::Write as _,
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::sync::Arc;

use coop_cloud::{BridgeAbiVersion, ProtocolVersion};
use coop_sidecar::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, GAME_PROTOCOL_VERSION, MAX_DESCRIPTOR_BYTES,
    SessionDescriptor,
    control::{
        CONTROL_PROTOCOL_VERSION, CommandId, CommandStatus, ControlCommand, ControlEvent,
        MAX_CONTROL_LINE_BYTES, ShutdownRequest,
    },
};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
    time::timeout,
};
use uuid::Uuid;

use crate::session::SessionWorkspace;
#[cfg(windows)]
use crate::windows_mgba_supervisor::MgbaSupervisor;

pub const PROCESS_IO_TIMEOUT: Duration = Duration::from_secs(5);
/// One wall-clock bound for the complete shutdown transaction.  It covers
/// control ACK, the optional helper, natural waits, Job termination, and both
/// root reaps; no phase receives a fresh independent timeout.
pub const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(10);
const SHUTDOWN_ACK_BOUND: Duration = Duration::from_millis(500);
const DROP_REAP_POLL: Duration = Duration::from_millis(10);
const STAGED_ROM_MARKER_PREFIX: &[u8] = b"pokecrossroads-coop-staged-rom-v1\n";
const MAX_STAGED_ROM_MARKER_BYTES: u64 = 4096;
const OWNED_MGBA_ARG_COUNT: usize = 13;
const OWNED_MGBA_ROM_ARG_INDEX: usize = OWNED_MGBA_ARG_COUNT - 1;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process path or argument is invalid")]
    InvalidArgument,
    #[cfg(not(windows))]
    #[error("secure executable binding is unavailable on this platform")]
    UnsupportedPlatform,
    #[error("child process failed to start")]
    Spawn(#[source] io::Error),
    #[error("official mGBA executable identity is invalid")]
    MgbaIdentity,
    #[error("sidecar descriptor is invalid")]
    Descriptor,
    #[error("sidecar descriptor read timed out")]
    DescriptorTimeout,
    #[error("sidecar control connection failed")]
    Control(#[source] io::Error),
    #[error("sidecar control protocol failed")]
    Protocol(#[source] serde_json::Error),
    #[error("child process termination failed")]
    Termination(#[source] io::Error),
    #[error("owned mGBA artifact cleanup failed ({artifact})")]
    Cleanup {
        artifact: CleanupArtifact,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("startup failed ({startup}) and child cleanup was not confirmed ({cleanup})")]
    StartupCleanup {
        startup: Box<ProcessError>,
        cleanup: Box<ProcessError>,
    },
    #[error("supervised event failed ({event}) and child cleanup was not confirmed ({cleanup})")]
    EventCleanup {
        event: Box<ProcessError>,
        cleanup: Box<ProcessError>,
    },
    #[error("a supervised child exited unsuccessfully")]
    ChildExited,
}

/// Whether the sidecar accepted the correlated shutdown command before the
/// launcher attempted the emulator close.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPath {
    Graceful,
    Forced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootReapEvidence {
    Reaped,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobTerminationEvidence {
    Initiated,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlShutdownEvidence {
    Accepted,
    NotAccepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftCloseDisposition {
    Requested,
    NotAttempted,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryDisposition {
    Clean,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescendantCompletionEvidence {
    NotAvailable,
}

/// A bounded, non-ambiguous shutdown result.  `descendant_completion_proven`
/// is permanently false because windows-spawn exposes no Job completion or
/// active-process-count oracle; root reaping plus `TerminateJobObject`
/// initiation is the strongest available evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownDisposition {
    pub path: ShutdownPath,
    pub sidecar: RootReapEvidence,
    pub mgba: RootReapEvidence,
    pub job_termination: JobTerminationEvidence,
    pub control: ControlShutdownEvidence,
    pub soft_close: SoftCloseDisposition,
    pub recovery: RecoveryDisposition,
    pub descendant_completion: DescendantCompletionEvidence,
}

impl ShutdownDisposition {
    #[must_use]
    pub const fn clean(&self) -> bool {
        matches!(self.recovery, RecoveryDisposition::Clean)
            && matches!(self.sidecar, RootReapEvidence::Reaped)
            && matches!(self.mgba, RootReapEvidence::Reaped)
            && matches!(self.job_termination, JobTerminationEvidence::Initiated)
    }

    #[must_use]
    pub const fn recovery_required(&self) -> bool {
        matches!(self.recovery, RecoveryDisposition::Required)
    }
}

/// A cleanup target retained in a [`ProcessError`] so recovery code can
/// identify the first artifact that could not be removed.  Paths are kept in
/// the typed error for diagnostics; the CLI maps process errors to its
/// generic runtime error and never prints them to the operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupArtifact {
    /// The save file mGBA may create through its implicit SRAM path.
    ImplicitSave,
    /// The private staged ROM copied by the launcher.
    Rom,
    /// The ownership marker pairing the ROM to this launch.
    Marker,
}

impl std::fmt::Display for CleanupArtifact {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ImplicitSave => "implicit save",
            Self::Rom => "staged ROM",
            Self::Marker => "ownership marker",
        })
    }
}

impl ProcessError {
    /// Returns false when startup compensation could not prove that the
    /// already-started child was terminated and reaped.  Callers holding a
    /// lease must keep it fenced in that case rather than releasing it.
    #[must_use]
    pub const fn cleanup_confirmed(&self) -> bool {
        !matches!(
            self,
            Self::StartupCleanup { .. } | Self::EventCleanup { .. }
        )
    }
}

#[derive(Clone)]
pub struct CommandSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
    mgba: bool,
    identity: Option<ExecutableIdentity>,
    rom_identity: Option<ExecutableIdentity>,
    rom_cleanup: Option<PathBuf>,
    rom_marker_cleanup: Option<PathBuf>,
    rom_implicit_save_path: Option<PathBuf>,
    rom_marker_identity: Option<ExecutableIdentity>,
    #[cfg(windows)]
    executable_guards: Option<ExecutableGuards>,
    #[cfg(windows)]
    rom_guards: Option<ExecutableGuards>,
    #[cfg(windows)]
    rom_marker_guards: Option<ExecutableGuards>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExecutableIdentity {
    canonical: PathBuf,
    length: u64,
    modified: Option<std::time::SystemTime>,
    digest: [u8; 32],
}

#[cfg(windows)]
#[derive(Clone)]
struct ExecutableGuards {
    file: Arc<fs::File>,
    ancestors: Vec<Arc<fs::File>>,
}

impl CommandSpec {
    fn new(executable: impl Into<PathBuf>, args: Vec<String>) -> Result<Self, ProcessError> {
        let executable = executable.into();
        if executable.as_os_str().is_empty()
            || executable.to_str().is_none()
            || args.iter().any(|arg| arg.contains('\0'))
        {
            return Err(ProcessError::InvalidArgument);
        }
        let (identity, executable_guards) = executable_binding(&executable)?;
        #[cfg(not(windows))]
        let _ = executable_guards;
        Ok(Self {
            executable,
            args,
            mgba: false,
            identity,
            rom_identity: None,
            rom_cleanup: None,
            rom_marker_cleanup: None,
            rom_implicit_save_path: None,
            rom_marker_identity: None,
            #[cfg(windows)]
            executable_guards,
            #[cfg(windows)]
            rom_guards: None,
            #[cfg(windows)]
            rom_marker_guards: None,
        })
    }

    /// # Errors
    ///
    /// Returns an error when the executable or epoch is invalid.
    pub fn sidecar(executable: impl Into<PathBuf>, epoch: u32) -> Result<Self, ProcessError> {
        if epoch == 0 {
            return Err(ProcessError::InvalidArgument);
        }
        Self::new(
            executable,
            vec!["--session-epoch".into(), epoch.to_string()],
        )
    }

    /// Captures the sidecar executable identity before asynchronous session
    /// setup.  The epoch is bound later without discarding that identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable path is invalid.
    pub fn sidecar_template(executable: impl Into<PathBuf>) -> Result<Self, ProcessError> {
        Self::new(executable, Vec::new())
    }

    /// Binds the server-issued epoch to a previously captured sidecar path.
    ///
    /// # Errors
    ///
    /// Returns an error when the epoch is zero.
    pub fn with_session_epoch(mut self, epoch: u32) -> Result<Self, ProcessError> {
        if epoch == 0 {
            return Err(ProcessError::InvalidArgument);
        }
        self.args = vec!["--session-epoch".into(), epoch.to_string()];
        Ok(self)
    }

    /// # Errors
    ///
    /// Returns an error when the executable or ROM path is invalid.
    pub fn mgba(
        executable: impl Into<PathBuf>,
        rom: impl AsRef<Path>,
    ) -> Result<Self, ProcessError> {
        let rom_path = rom.as_ref().to_path_buf();
        let rom = rom_path.to_str().ok_or(ProcessError::InvalidArgument)?;
        let mut spec = Self::new(executable, vec![rom.to_owned()])?;
        spec.mgba = true;
        let (rom_identity, rom_guards) = executable_binding(&rom_path)?;
        ensure_no_auxiliary_inputs(&rom_path)?;
        #[cfg(not(windows))]
        let _ = rom_guards;
        spec.rom_identity = rom_identity;
        #[cfg(windows)]
        {
            spec.rom_guards = rom_guards;
            if let Some(identity) = spec.rom_identity.as_ref() {
                identity
                    .canonical
                    .to_str()
                    .ok_or(ProcessError::InvalidArgument)?
                    .clone_into(&mut spec.args[0]);
            }
        }
        Ok(spec)
    }

    /// Binds cleanup ownership to the explicit marker emitted by the
    /// launcher's ROM staging routine. `mgba` deliberately does not infer
    /// ownership from a filename or directory, so an arbitrary caller-owned
    /// `rom-*.gba` is never removed as a side effect of supervision.
    ///
    /// The marker is checked before any child starts and is retained until the
    /// emulator has been reaped. The launcher publishes it with create-new
    /// semantics next to its staged ROM and includes the canonical ROM path in
    /// it.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable, ROM, or ownership marker is
    /// invalid.
    pub fn mgba_owned_staged(
        executable: impl Into<PathBuf>,
        rom: impl AsRef<Path>,
        marker: impl AsRef<Path>,
    ) -> Result<Self, ProcessError> {
        let rom_path = rom.as_ref().to_path_buf();
        let marker_path = marker.as_ref().to_path_buf();
        if !rom_path.is_absolute()
            || !marker_path.is_absolute()
            || !owned_rom_marker_matches(&rom_path, &marker_path)
        {
            return Err(ProcessError::InvalidArgument);
        }
        let mut spec = Self::new(
            executable,
            vec![
                rom_path
                    .to_str()
                    .ok_or(ProcessError::InvalidArgument)?
                    .into(),
            ],
        )?;
        spec.mgba = true;
        let (rom_identity, rom_guards) = owned_file_binding(&rom_path)?;
        let Some(rom_identity) = rom_identity else {
            return Err(ProcessError::InvalidArgument);
        };
        let (marker_identity, marker_guards) = owned_file_binding(&marker_path)?;
        let Some(marker_identity) = marker_identity else {
            return Err(ProcessError::InvalidArgument);
        };
        #[cfg(windows)]
        if let Some(marker_guards) = marker_guards.as_ref()
            && !owned_rom_marker_matches_held_identity(&rom_identity, &marker_guards.file)
                .unwrap_or(false)
        {
            // The initial path check and the two held bindings must agree at
            // the same instant. If either file changed during construction,
            // release the guards and retain both artifacts for recovery.
            return Err(ProcessError::InvalidArgument);
        }
        let implicit_save_path = rom_path.with_extension("sav");
        // A pre-existing sibling is never ours.  Only NotFound means absence;
        // ACL, I/O, and other metadata failures fail closed before mGBA starts.
        ensure_absent(&implicit_save_path)?;
        ensure_no_auxiliary_inputs(&rom_path)?;
        let canonical_rom = rom_identity
            .canonical
            .to_str()
            .ok_or(ProcessError::InvalidArgument)?;
        let save_dir = rom_identity
            .canonical
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or(ProcessError::InvalidArgument)?
            .to_str()
            .ok_or(ProcessError::InvalidArgument)?;
        // mGBA 0.10.5 accepts repeated `-C OPTION=VALUE` overrides. Pin the
        // implicit SRAM directory to the private staged-ROM directory and
        // disable automatic state/cheat loading and saving. The explicit NUL
        // patch path suppresses mCoreAutoloadPatch's fallback sibling scan;
        // canonical SAV capture remains driven by the authenticated bridge
        // safe-point.
        spec.args = vec![
            "-C".into(),
            format!("savegamePath={save_dir}"),
            "-C".into(),
            "autoload=0".into(),
            "-C".into(),
            "autosave=0".into(),
            "-C".into(),
            "cheatAutoload=0".into(),
            "-C".into(),
            "cheatAutosave=0".into(),
            "-p".into(),
            "NUL".into(),
            canonical_rom.into(),
        ];
        spec.rom_cleanup = Some(rom_path);
        spec.rom_identity = Some(rom_identity);
        spec.rom_marker_cleanup = Some(marker_path);
        spec.rom_implicit_save_path = Some(implicit_save_path);
        spec.rom_marker_identity = Some(marker_identity);
        #[cfg(windows)]
        {
            spec.rom_guards = rom_guards;
            spec.rom_marker_guards = marker_guards;
        }
        #[cfg(not(windows))]
        let _ = (marker_guards, rom_guards);
        Ok(spec)
    }

    /// Starts a validated mGBA spec through the same Windows Job boundary as
    /// the gameplay lifecycle. This narrow entry point is also used by the
    /// opt-in real-artifact conformance test; normal sessions use
    /// [`SupervisedChildren::start`] so the authenticated sidecar is paired.
    ///
    /// The returned owner retains this spec's executable, ROM, marker, and
    /// ancestor guards until the contained process is stopped and dropped.
    /// The receiver is disarmed so callers may drop it immediately without
    /// releasing the live-process guards or cleanup ownership.
    ///
    /// # Errors
    ///
    /// Returns an error when the mGBA executable, held ROM, ownership marker,
    /// or canonical argument vector no longer satisfies the launch contract.
    #[cfg(windows)]
    pub fn spawn_guarded_mgba(&mut self) -> Result<GuardedMgbaChild, ProcessError> {
        if !self.mgba
            || self.identity.is_none()
            || self.rom_identity.is_none()
            || !self.has_supported_argv()
        {
            return Err(ProcessError::InvalidArgument);
        }
        validate_executable_identity(self)?;
        let owner = std::mem::replace(self, Self::disarmed());
        let supervisor =
            MgbaSupervisor::spawn(&owner.executable, &owner.args).map_err(ProcessError::Spawn)?;
        Ok(GuardedMgbaChild {
            supervisor,
            _spec: owner,
        })
    }

    #[cfg(windows)]
    fn disarmed() -> Self {
        Self {
            executable: PathBuf::new(),
            args: Vec::new(),
            mgba: false,
            identity: None,
            rom_identity: None,
            rom_cleanup: None,
            rom_marker_cleanup: None,
            rom_implicit_save_path: None,
            rom_marker_identity: None,
            executable_guards: None,
            rom_guards: None,
            rom_marker_guards: None,
        }
    }

    fn has_supported_argv(&self) -> bool {
        if self.args.len() == 1 {
            return self.rom_implicit_save_path.is_none()
                && !self.args[0].is_empty()
                && !self.args[0].contains('\0')
                && Path::new(&self.args[0]).is_absolute();
        }
        if self.args.len() != OWNED_MGBA_ARG_COUNT {
            return false;
        }
        let Some(rom_cleanup) = self.rom_cleanup.as_deref() else {
            return false;
        };
        let Some(marker_cleanup) = self.rom_marker_cleanup.as_deref() else {
            return false;
        };
        let Some(implicit_save_path) = self.rom_implicit_save_path.as_deref() else {
            return false;
        };
        let Some(rom_identity) = self.rom_identity.as_ref() else {
            return false;
        };
        let Some(marker_identity) = self.rom_marker_identity.as_ref() else {
            return false;
        };
        let Some(save_dir) = self.args[1].strip_prefix("savegamePath=") else {
            return false;
        };
        let Some(expected_save_dir) = rom_identity.canonical.parent() else {
            return false;
        };
        let Ok(current_rom_identity) = path_identity(rom_cleanup) else {
            return false;
        };
        let Ok(current_marker_identity) = path_identity(marker_cleanup) else {
            return false;
        };
        marker_cleanup == staged_rom_marker_path(rom_cleanup)
            && implicit_save_path == rom_cleanup.with_extension("sav")
            && current_rom_identity == *rom_identity
            && current_marker_identity == *marker_identity
            && Path::new(save_dir) == expected_save_dir
            && Path::new(&self.args[OWNED_MGBA_ROM_ARG_INDEX]) == rom_identity.canonical
            && owned_rom_marker_matches_identity(rom_identity, marker_cleanup)
            && self.args[0] == "-C"
            && self.args[2] == "-C"
            && self.args[3] == "autoload=0"
            && self.args[4] == "-C"
            && self.args[5] == "autosave=0"
            && self.args[6] == "-C"
            && self.args[7] == "cheatAutoload=0"
            && self.args[8] == "-C"
            && self.args[9] == "cheatAutosave=0"
            && self.args[10] == "-p"
            && self.args[11] == "NUL"
            && !self.args[OWNED_MGBA_ROM_ARG_INDEX].is_empty()
            && !self.args[OWNED_MGBA_ROM_ARG_INDEX].contains('\0')
            && Path::new(&self.args[OWNED_MGBA_ROM_ARG_INDEX]).is_absolute()
            && !save_dir.contains('\0')
            && ensure_no_auxiliary_inputs(rom_cleanup).is_ok()
    }

    /// Explicitly removes launcher-owned artifacts. The ownership marker is
    /// last so an error leaves enough evidence for a later recovery attempt.
    fn cleanup_owned_rom(&mut self) -> Result<(), ProcessError> {
        if let Some(path) = self.rom_implicit_save_path.clone() {
            #[cfg(windows)]
            remove_owned_implicit_save(
                &path,
                self.rom_cleanup.as_deref(),
                self.rom_guards.as_ref(),
            )?;
            #[cfg(not(windows))]
            remove_owned_file(&path, CleanupArtifact::ImplicitSave)?;
            self.rom_implicit_save_path = None;
        }
        if let Some(path) = self.rom_cleanup.clone() {
            #[cfg(windows)]
            remove_owned_guarded_file(
                &path,
                CleanupArtifact::Rom,
                self.rom_identity.as_ref(),
                &mut self.rom_guards,
            )?;
            #[cfg(not(windows))]
            remove_owned_file(&path, CleanupArtifact::Rom)?;
            self.rom_cleanup = None;
        }
        if let Some(path) = self.rom_marker_cleanup.clone() {
            #[cfg(windows)]
            {
                if let (Some(identity), Some(marker_guards)) =
                    (self.rom_identity.as_ref(), self.rom_marker_guards.as_ref())
                    && !owned_rom_marker_matches_held_identity(identity, &marker_guards.file)
                        .unwrap_or(false)
                {
                    return Err(ProcessError::Cleanup {
                        artifact: CleanupArtifact::Marker,
                        path,
                        source: io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "ownership marker content changed",
                        ),
                    });
                }
                remove_owned_guarded_file(
                    &path,
                    CleanupArtifact::Marker,
                    self.rom_marker_identity.as_ref(),
                    &mut self.rom_marker_guards,
                )?;
            }
            #[cfg(not(windows))]
            remove_owned_file(&path, CleanupArtifact::Marker)?;
            self.rom_marker_cleanup = None;
            self.rom_identity = None;
            self.rom_marker_identity = None;
        }
        Ok(())
    }
}

/// Returns the deterministic sidecar marker path for a staged ROM.  The
/// marker is deliberately a hidden sibling, not a filename heuristic, and is
/// created only by the launcher's verified staging routine.
#[must_use]
pub fn staged_rom_marker_path(rom: impl AsRef<Path>) -> PathBuf {
    let rom = rom.as_ref();
    let marker_name = rom.file_name().map_or_else(
        || ".rom.owner".to_owned(),
        |name| format!(".{}.owner", name.to_string_lossy()),
    );
    if let Some(parent) = rom.parent() {
        parent.join(marker_name)
    } else {
        PathBuf::from(marker_name)
    }
}

/// Returns the exact marker bytes that pair with a canonical staged ROM.
/// Callers should publish this file with create-new semantics beside the ROM
/// before constructing [`mgba_owned_staged`].
///
/// # Errors
///
/// Returns an error when the ROM path cannot be canonicalized.
pub fn staged_rom_marker_contents(rom: impl AsRef<Path>) -> Result<Vec<u8>, ProcessError> {
    let identity = path_identity(rom.as_ref()).map_err(|_| ProcessError::InvalidArgument)?;
    let marker = staged_rom_marker_contents_from_identity(&identity);
    if marker.len() as u64 > MAX_STAGED_ROM_MARKER_BYTES {
        return Err(ProcessError::InvalidArgument);
    }
    Ok(marker)
}

fn staged_rom_marker_contents_from_identity(identity: &ExecutableIdentity) -> Vec<u8> {
    let mut digest = String::with_capacity(identity.digest.len() * 2);
    for byte in identity.digest {
        let _ = write!(&mut digest, "{byte:02x}");
    }
    format!(
        "{}path={}\nlength={}\nsha256={}\n",
        std::str::from_utf8(STAGED_ROM_MARKER_PREFIX).expect("static marker prefix is UTF-8"),
        identity.canonical.to_string_lossy(),
        identity.length,
        digest,
    )
    .into_bytes()
}

fn owned_rom_marker_matches(rom: &Path, marker: &Path) -> bool {
    if marker != staged_rom_marker_path(rom)
        || matches!(
            fs::symlink_metadata(marker),
            Ok(metadata) if metadata.file_type().is_symlink()
        )
    {
        return false;
    }
    let Ok(file) = open_read_nofollow(marker) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_STAGED_ROM_MARKER_BYTES {
        return false;
    }
    let Ok(capacity) = usize::try_from(metadata.len()) else {
        return false;
    };
    let mut contents = Vec::with_capacity(capacity);
    if file
        .take(MAX_STAGED_ROM_MARKER_BYTES.saturating_add(1))
        .read_to_end(&mut contents)
        .is_err()
        || contents.len() as u64 > MAX_STAGED_ROM_MARKER_BYTES
    {
        return false;
    }
    let Ok(identity) = path_identity(rom) else {
        return false;
    };
    let expected = staged_rom_marker_contents_from_identity(&identity);
    if expected.len() as u64 > MAX_STAGED_ROM_MARKER_BYTES {
        return false;
    }
    contents == expected
}

fn owned_rom_marker_matches_identity(identity: &ExecutableIdentity, marker: &Path) -> bool {
    if matches!(
        fs::symlink_metadata(marker),
        Ok(metadata) if metadata.file_type().is_symlink()
    ) {
        return false;
    }
    let Ok(file) = open_read_nofollow(marker) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_STAGED_ROM_MARKER_BYTES {
        return false;
    }
    let Ok(capacity) = usize::try_from(metadata.len()) else {
        return false;
    };
    let mut contents = Vec::with_capacity(capacity);
    if file
        .take(MAX_STAGED_ROM_MARKER_BYTES.saturating_add(1))
        .read_to_end(&mut contents)
        .is_err()
    {
        return false;
    }
    let expected = staged_rom_marker_contents_from_identity(identity);
    contents == expected
}

#[cfg(windows)]
fn owned_rom_marker_matches_held_identity(
    identity: &ExecutableIdentity,
    marker_file: &fs::File,
) -> io::Result<bool> {
    let metadata = marker_file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_STAGED_ROM_MARKER_BYTES {
        return Ok(false);
    }
    let mut file = marker_file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut contents = Vec::new();
    file.take(MAX_STAGED_ROM_MARKER_BYTES.saturating_add(1))
        .read_to_end(&mut contents)?;
    Ok(contents == staged_rom_marker_contents_from_identity(identity))
}

fn open_read_nofollow(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            // Identity reads may run while a launcher-owned guard is open
            // with DELETE access. Replacement protection comes from the
            // caller's held identity guard and revalidation, not this reader.
            .share_mode(0x0000_0007)
            .custom_flags(0x0020_0000);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x0002_0000);
    }
    options.open(path)
}

fn path_identity(path: &Path) -> io::Result<ExecutableIdentity> {
    let path_metadata = fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "owned path is not a regular file",
        ));
    }
    let file = open_read_nofollow(path)?;
    let metadata = file.metadata()?;
    Ok(ExecutableIdentity {
        canonical: fs::canonicalize(path)?,
        length: metadata.len(),
        modified: metadata.modified().ok(),
        digest: hash_file_handle(&file)?,
    })
}

/// Removes a staged ROM and its ownership marker only when the marker still
/// proves the exact ROM identity. This is used when startup fails before a
/// [`CommandSpec`] can take ownership; ambiguity intentionally leaves both
/// paths in place for recovery instead of deleting a replaced file.
///
/// # Errors
///
/// Returns a typed cleanup error when identity or marker validation fails.
pub fn cleanup_owned_staged_rom(
    rom: impl AsRef<Path>,
    marker: impl AsRef<Path>,
) -> Result<(), ProcessError> {
    let rom = rom.as_ref().to_path_buf();
    let marker = marker.as_ref().to_path_buf();
    if !rom.is_absolute() || !marker.is_absolute() || marker != staged_rom_marker_path(&rom) {
        return Err(ProcessError::InvalidArgument);
    }
    let rom_identity = path_identity(&rom).map_err(|source| ProcessError::Cleanup {
        artifact: CleanupArtifact::Rom,
        path: rom.clone(),
        source,
    })?;
    if !owned_rom_marker_matches_identity(&rom_identity, &marker) {
        return Err(ProcessError::Cleanup {
            artifact: CleanupArtifact::Marker,
            path: marker.clone(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "ownership marker does not prove the staged ROM identity",
            ),
        });
    }
    let (bound_rom_identity, rom_guards) = owned_file_binding(&rom)?;
    let Some(bound_rom_identity) = bound_rom_identity else {
        return Err(ProcessError::InvalidArgument);
    };
    let (marker_identity, marker_guards) = owned_file_binding(&marker)?;
    let Some(marker_identity) = marker_identity else {
        return Err(ProcessError::InvalidArgument);
    };
    let mut spec = CommandSpec {
        executable: PathBuf::new(),
        args: Vec::new(),
        mgba: false,
        identity: None,
        rom_identity: Some(bound_rom_identity),
        rom_cleanup: Some(rom),
        rom_marker_cleanup: Some(marker),
        rom_implicit_save_path: None,
        rom_marker_identity: Some(marker_identity),
        #[cfg(windows)]
        executable_guards: None,
        #[cfg(windows)]
        rom_guards,
        #[cfg(windows)]
        rom_marker_guards: marker_guards,
    };
    #[cfg(not(windows))]
    {
        let _ = (rom_guards, marker_guards);
    }
    spec.cleanup_owned_rom()
}

impl Drop for CommandSpec {
    fn drop(&mut self) {
        // Keep the marker until every owned payload is gone.  If any best
        // effort operation fails, the marker remains as recovery evidence.
        let _ = self.cleanup_owned_rom();
    }
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = formatter.debug_struct("CommandSpec");
        debug
            .field("executable", &self.executable)
            .field("args", &self.args)
            .field("mgba", &self.mgba)
            .field("identity", &self.identity.as_ref().map(|_| "[BOUND]"))
            .field(
                "rom_identity",
                &self.rom_identity.as_ref().map(|_| "[BOUND]"),
            )
            .field("rom_cleanup", &self.rom_cleanup.as_ref().map(|_| "[HELD]"));
        debug.field(
            "rom_marker_cleanup",
            &self.rom_marker_cleanup.as_ref().map(|_| "[HELD]"),
        );
        debug.field(
            "rom_implicit_save_path",
            &self.rom_implicit_save_path.as_ref().map(|_| "[HELD]"),
        );
        debug.field(
            "rom_marker_identity",
            &self.rom_marker_identity.as_ref().map(|_| "[BOUND]"),
        );
        #[cfg(windows)]
        debug.field(
            "executable_guards",
            &self.executable_guards.as_ref().map(|_| "[HELD]"),
        );
        #[cfg(windows)]
        debug.field("rom_guards", &self.rom_guards.as_ref().map(|_| "[HELD]"));
        #[cfg(windows)]
        debug.field(
            "rom_marker_guards",
            &self.rom_marker_guards.as_ref().map(|_| "[HELD]"),
        );
        debug.finish()
    }
}

impl PartialEq for CommandSpec {
    fn eq(&self, other: &Self) -> bool {
        self.executable == other.executable
            && self.args == other.args
            && self.identity == other.identity
            && self.rom_identity == other.rom_identity
    }
}

impl Eq for CommandSpec {}

/// Validates a descriptor emitted by a sidecar before any emulator process is started.
///
/// # Errors
///
/// Returns an error when endpoint, secret, epoch, or protocol validation fails.
pub fn validate_descriptor(
    descriptor: &SessionDescriptor,
    expected_epoch: u32,
) -> Result<(), ProcessError> {
    if expected_epoch == 0
        || descriptor.version() != 1
        || descriptor.session_epoch() != expected_epoch
    {
        return Err(ProcessError::Descriptor);
    }
    let bridge = descriptor.bridge();
    let control = descriptor.control();
    if bridge.transport() != "tcp"
        || control.transport() != "tcp"
        || bridge.host() != "127.0.0.1"
        || control.host() != "127.0.0.1"
        || bridge.port() == 0
        || control.port() == 0
        || bridge.port() == control.port()
        || bridge.bridge_abi() != BRIDGE_ABI_VERSION
        || bridge.protocol_version() != GAME_PROTOCOL_VERSION
        || bridge.frame_bytes() != BRIDGE_FRAME_SIZE
        || control.control_version() != CONTROL_PROTOCOL_VERSION
        || control.max_line_bytes() != MAX_CONTROL_LINE_BYTES
        || bridge.secret().len() != 32
        || control.secret().len() != 32
        || bridge.secret() == control.secret()
        || !bridge
            .secret()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !control
            .secret()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ProcessError::Descriptor);
    }
    Ok(())
}

/// Materializes the authenticated sidecar bridge inputs through the same
/// production path used immediately before mGBA startup. Only the bridge
/// endpoint fields are written to `session.lua`; control credentials never
/// cross this boundary.
///
/// # Errors
///
/// Returns an error when the descriptor is not valid for the expected epoch,
/// the checked-in bridge cannot be copied, or the session file cannot be
/// written.
pub fn materialize_bridge_session(
    workspace: &SessionWorkspace,
    bridge_source: &Path,
    descriptor: &SessionDescriptor,
    expected_epoch: u32,
) -> Result<(), ProcessError> {
    validate_descriptor(descriptor, expected_epoch)?;
    workspace
        .copy_bridge_inputs(bridge_source)
        .map_err(|_| ProcessError::Descriptor)?;
    workspace
        .write_session_lua(
            descriptor.bridge().host(),
            descriptor.bridge().port(),
            descriptor.bridge().secret(),
        )
        .map_err(|_| ProcessError::Descriptor)?;
    Ok(())
}

/// A control-only connection; the secret is never serialized outside its handshake.
pub struct ControlChannel {
    stream: TcpStream,
    read_buffer: Vec<u8>,
}

impl std::fmt::Debug for ControlChannel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ControlChannel([REDACTED])")
    }
}

impl ControlChannel {
    /// # Errors
    ///
    /// Returns an error when control authentication or bounded I/O fails.
    pub async fn connect(descriptor: &SessionDescriptor) -> Result<Self, ProcessError> {
        let mut stream = timeout(
            PROCESS_IO_TIMEOUT,
            TcpStream::connect(descriptor.control_address()),
        )
        .await
        .map_err(|_| {
            ProcessError::Control(io::Error::new(io::ErrorKind::TimedOut, "connect timeout"))
        })?
        .map_err(ProcessError::Control)?;
        let handshake = format!(
            "{{\"secret\":\"{}\",\"control_version\":{},\"session_epoch\":{}}}\n",
            descriptor.control_secret(),
            CONTROL_PROTOCOL_VERSION,
            descriptor.session_epoch()
        );
        if handshake.len() > MAX_CONTROL_LINE_BYTES {
            return Err(ProcessError::Descriptor);
        }
        timeout(PROCESS_IO_TIMEOUT, stream.write_all(handshake.as_bytes()))
            .await
            .map_err(|_| {
                ProcessError::Control(io::Error::new(io::ErrorKind::TimedOut, "write timeout"))
            })?
            .map_err(ProcessError::Control)?;
        let response = timeout(
            PROCESS_IO_TIMEOUT,
            read_line(&mut stream, MAX_CONTROL_LINE_BYTES),
        )
        .await
        .map_err(|_| {
            ProcessError::Control(io::Error::new(io::ErrorKind::TimedOut, "read timeout"))
        })??;
        if response != b"{\"ok\":true}" {
            return Err(ProcessError::Descriptor);
        }
        Ok(Self {
            stream,
            read_buffer: Vec::new(),
        })
    }

    /// # Errors
    ///
    /// Returns an error when the typed command cannot be serialized or sent.
    pub async fn send(&mut self, command: &ControlCommand) -> Result<(), ProcessError> {
        let mut bytes = serde_json::to_vec(command).map_err(ProcessError::Protocol)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_CONTROL_LINE_BYTES {
            return Err(ProcessError::Descriptor);
        }
        timeout(PROCESS_IO_TIMEOUT, self.stream.write_all(&bytes))
            .await
            .map_err(|_| {
                ProcessError::Control(io::Error::new(io::ErrorKind::TimedOut, "write timeout"))
            })?
            .map_err(ProcessError::Control)
    }

    /// Receives the next event without imposing an idle timeout.  The
    /// supervisor invokes this method in a cancellation-safe `select!`; a
    /// healthy sidecar may remain quiet for longer than the I/O bound.
    ///
    /// # Errors
    ///
    /// Returns an error when the event is incomplete, oversized, or malformed.
    pub async fn receive(&mut self) -> Result<ControlEvent, ProcessError> {
        let bytes = self.read_line().await?;
        serde_json::from_slice(&bytes).map_err(ProcessError::Protocol)
    }

    /// Receives one response with the explicit bounded wait required by
    /// request/response operations such as checkpoint grant and save update.
    /// Partial bytes survive cancellation and can be completed by a later
    /// call, which prevents a heartbeat tick from corrupting framing.
    ///
    /// # Errors
    ///
    /// Returns an error when the response is incomplete, oversized, malformed,
    /// or does not arrive before [`PROCESS_IO_TIMEOUT`].
    pub async fn receive_bounded(&mut self) -> Result<ControlEvent, ProcessError> {
        let bytes = timeout(PROCESS_IO_TIMEOUT, self.read_line())
            .await
            .map_err(|_| {
                ProcessError::Control(io::Error::new(io::ErrorKind::TimedOut, "read timeout"))
            })??;
        serde_json::from_slice(&bytes).map_err(ProcessError::Protocol)
    }

    async fn read_line(&mut self) -> Result<Vec<u8>, ProcessError> {
        loop {
            if let Some(newline) = self.read_buffer.iter().position(|byte| *byte == b'\n') {
                if newline >= MAX_CONTROL_LINE_BYTES {
                    return Err(ProcessError::Descriptor);
                }
                let mut line = self.read_buffer.drain(..=newline).collect::<Vec<_>>();
                debug_assert_eq!(line.pop(), Some(b'\n'));
                return Ok(line);
            }
            if self.read_buffer.len() >= MAX_CONTROL_LINE_BYTES {
                return Err(ProcessError::Descriptor);
            }
            let remaining = MAX_CONTROL_LINE_BYTES - self.read_buffer.len();
            let mut chunk = [0_u8; 1024];
            let read_size = remaining.min(chunk.len());
            let count = self
                .stream
                .read(&mut chunk[..read_size])
                .await
                .map_err(ProcessError::Control)?;
            if count == 0 {
                return Err(ProcessError::Descriptor);
            }
            self.read_buffer.extend_from_slice(&chunk[..count]);
        }
    }

    #[cfg(test)]
    pub(crate) fn from_stream_for_test(stream: TcpStream) -> Self {
        Self {
            stream,
            read_buffer: Vec::new(),
        }
    }
}

async fn read_line(
    stream: &mut (impl AsyncRead + Unpin),
    max: usize,
) -> Result<Vec<u8>, ProcessError> {
    let mut line = Vec::with_capacity(max.min(128));
    let mut byte = [0_u8; 1];
    loop {
        let count = stream
            .read(&mut byte)
            .await
            .map_err(ProcessError::Control)?;
        if count == 0 {
            return Err(ProcessError::Descriptor);
        }
        if line.len().saturating_add(1) >= max {
            if byte[0] == b'\n' && line.len().saturating_add(1) == max {
                return Ok(line);
            }
            return Err(ProcessError::Descriptor);
        }
        if byte[0] == b'\n' {
            return Ok(line);
        }
        line.push(byte[0]);
    }
}

/// Owns a contained mGBA process together with the executable, ROM, marker,
/// and ancestor guards that authenticate its launch and cleanup lifetime.
/// Fields are ordered so the supervisor is dropped before the retained spec,
/// ensuring the Job is closed before owned artifacts are cleaned up.
#[cfg(windows)]
pub struct GuardedMgbaChild {
    supervisor: MgbaSupervisor,
    _spec: CommandSpec,
}

#[cfg(windows)]
impl GuardedMgbaChild {
    /// Waits for the contained root process and requests Job termination.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor boundary closes or the root
    /// process cannot be reaped.
    pub async fn wait(&self) -> io::Result<std::process::ExitStatus> {
        self.supervisor.wait().await
    }

    /// Requests contained-process termination and root reaping.
    ///
    /// mGBA's native `TerminateJobObject` request is synchronous and
    /// uncancellable through the safe `windows-spawn` API; any returned
    /// cleanup error represents termination-initiation or root-reap
    /// uncertainty, not proof that descendants are empty.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when Job termination or root reaping fails.
    pub async fn stop(&mut self) -> io::Result<()> {
        self.supervisor.stop().await
    }

    /// Returns a nonblocking root-process status snapshot.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor status lock is poisoned.
    pub fn try_wait(&self) -> io::Result<Option<std::process::ExitStatus>> {
        self.supervisor.try_wait()
    }
}

enum MgbaChild {
    #[cfg(windows)]
    Contained(MgbaSupervisor),
    #[cfg(any(not(windows), test))]
    Tokio(Box<Child>),
}

impl MgbaChild {
    async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        match self {
            #[cfg(windows)]
            Self::Contained(supervisor) => supervisor.wait().await,
            #[cfg(any(not(windows), test))]
            Self::Tokio(child) => child.wait().await,
        }
    }

    async fn stop(&mut self) -> Result<(), ProcessError> {
        match self {
            #[cfg(windows)]
            Self::Contained(supervisor) => timeout(PROCESS_IO_TIMEOUT, supervisor.stop())
                .await
                .map_err(|_| {
                    ProcessError::Termination(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "mGBA supervisor stop timed out",
                    ))
                })?
                .map_err(ProcessError::Termination),
            #[cfg(any(not(windows), test))]
            Self::Tokio(child) => stop_child(child).await,
        }
    }

    async fn shutdown(
        &mut self,
        attempt_soft_close: bool,
        deadline: Instant,
    ) -> MgbaShutdownReport {
        #[cfg(windows)]
        {
            match self {
                Self::Contained(supervisor) => {
                    let evidence = supervisor.shutdown(attempt_soft_close, deadline).await;
                    MgbaShutdownReport {
                        soft_close: match evidence.soft_close {
                            crate::windows_mgba_supervisor::SoftCloseEvidence::Requested => {
                                SoftCloseDisposition::Requested
                            }
                            crate::windows_mgba_supervisor::SoftCloseEvidence::NotAttempted => {
                                SoftCloseDisposition::NotAttempted
                            }
                            crate::windows_mgba_supervisor::SoftCloseEvidence::Failed => {
                                SoftCloseDisposition::Failed
                            }
                        },
                        root_reap: if matches!(
                            evidence.root,
                            crate::windows_mgba_supervisor::RootReapEvidence::Reaped
                        ) {
                            RootReapEvidence::Reaped
                        } else {
                            RootReapEvidence::Unknown
                        },
                        job_termination: if matches!(
                            evidence.job,
                            crate::windows_mgba_supervisor::JobTerminationEvidence::Initiated
                        ) {
                            JobTerminationEvidence::Initiated
                        } else {
                            JobTerminationEvidence::Unknown
                        },
                        recovery: if matches!(
                            evidence.recovery,
                            crate::windows_mgba_supervisor::RecoveryEvidence::Required
                        ) {
                            RecoveryDisposition::Required
                        } else {
                            RecoveryDisposition::Clean
                        },
                    }
                }
                #[cfg(test)]
                Self::Tokio(child) => {
                    let root_reaped = reap_tokio_child_until(child, deadline).await;
                    MgbaShutdownReport {
                        soft_close: SoftCloseDisposition::NotAttempted,
                        root_reap: if root_reaped {
                            RootReapEvidence::Reaped
                        } else {
                            RootReapEvidence::Unknown
                        },
                        job_termination: if root_reaped {
                            JobTerminationEvidence::Initiated
                        } else {
                            JobTerminationEvidence::Unknown
                        },
                        recovery: if root_reaped {
                            RecoveryDisposition::Clean
                        } else {
                            RecoveryDisposition::Required
                        },
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = attempt_soft_close;
            let MgbaChild::Tokio(child) = self;
            let root_reaped = reap_tokio_child_until(child, deadline).await;
            return MgbaShutdownReport {
                soft_close: SoftCloseDisposition::NotAttempted,
                root_reap: if root_reaped {
                    RootReapEvidence::Reaped
                } else {
                    RootReapEvidence::Unknown
                },
                // The portable test boundary has no Windows Job.  This
                // report is useful for lifecycle tests but is never used by
                // the supported Windows CLI.
                job_termination: if root_reaped {
                    JobTerminationEvidence::Initiated
                } else {
                    JobTerminationEvidence::Unknown
                },
                recovery: if root_reaped {
                    RecoveryDisposition::Clean
                } else {
                    RecoveryDisposition::Required
                },
            };
        }
    }

    #[cfg(test)]
    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        match self {
            #[cfg(windows)]
            Self::Contained(supervisor) => supervisor.try_wait(),
            #[cfg(any(not(windows), test))]
            Self::Tokio(child) => child.try_wait(),
        }
    }

    fn stop_sync(&mut self) -> MgbaShutdownReport {
        match self {
            #[cfg(windows)]
            Self::Contained(supervisor) => {
                let evidence = supervisor.shutdown_sync();
                MgbaShutdownReport {
                    soft_close: match evidence.soft_close {
                        crate::windows_mgba_supervisor::SoftCloseEvidence::Requested => {
                            SoftCloseDisposition::Requested
                        }
                        crate::windows_mgba_supervisor::SoftCloseEvidence::NotAttempted => {
                            SoftCloseDisposition::NotAttempted
                        }
                        crate::windows_mgba_supervisor::SoftCloseEvidence::Failed => {
                            SoftCloseDisposition::Failed
                        }
                    },
                    root_reap: if matches!(
                        evidence.root,
                        crate::windows_mgba_supervisor::RootReapEvidence::Reaped
                    ) {
                        RootReapEvidence::Reaped
                    } else {
                        RootReapEvidence::Unknown
                    },
                    job_termination: if matches!(
                        evidence.job,
                        crate::windows_mgba_supervisor::JobTerminationEvidence::Initiated
                    ) {
                        JobTerminationEvidence::Initiated
                    } else {
                        JobTerminationEvidence::Unknown
                    },
                    recovery: if matches!(
                        evidence.recovery,
                        crate::windows_mgba_supervisor::RecoveryEvidence::Required
                    ) {
                        RecoveryDisposition::Required
                    } else {
                        RecoveryDisposition::Clean
                    },
                }
            }
            #[cfg(any(not(windows), test))]
            Self::Tokio(child) => {
                let root_reaped = terminate_and_reap_sync(child);
                MgbaShutdownReport {
                    soft_close: SoftCloseDisposition::NotAttempted,
                    root_reap: if root_reaped {
                        RootReapEvidence::Reaped
                    } else {
                        RootReapEvidence::Unknown
                    },
                    job_termination: if root_reaped {
                        JobTerminationEvidence::Initiated
                    } else {
                        JobTerminationEvidence::Unknown
                    },
                    recovery: if root_reaped {
                        RecoveryDisposition::Clean
                    } else {
                        RecoveryDisposition::Required
                    },
                }
            }
        }
    }
}

pub struct SupervisedChildren {
    sidecar: Child,
    mgba: MgbaChild,
    pub control: ControlChannel,
    rom_cleanup: Option<PathBuf>,
    rom_marker_cleanup: Option<PathBuf>,
    rom_implicit_save_path: Option<PathBuf>,
    rom_identity: Option<ExecutableIdentity>,
    rom_marker_identity: Option<ExecutableIdentity>,
    #[cfg(windows)]
    rom_guards: Option<ExecutableGuards>,
    #[cfg(windows)]
    rom_marker_guards: Option<ExecutableGuards>,
    #[cfg(windows)]
    _mgba_executable_guards: Option<ExecutableGuards>,
    shutdown_disposition: Option<ShutdownDisposition>,
    shutdown_control_attempt: Option<ShutdownControlAttempt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MgbaShutdownReport {
    soft_close: SoftCloseDisposition,
    root_reap: RootReapEvidence,
    job_termination: JobTerminationEvidence,
    recovery: RecoveryDisposition,
}

#[derive(Clone, Debug)]
struct ShutdownControlAttempt {
    command_id: CommandId,
    expected_epoch: u32,
    sent: bool,
}

/// Owns a sidecar child while asynchronous startup is in progress.  A
/// canceled `start_internal` future cannot simply drop a Tokio child: this
/// guard synchronously requests termination and boundedly reaps it before
/// relinquishing the process handle.  Normal startup failures additionally
/// attempt the async cleanup path so callers receive an explicit
/// [`ProcessError::StartupCleanup`] when that proof fails.
struct StartupChildGuard {
    child: Option<Child>,
}

impl StartupChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("startup child guard always owns its child")
    }

    fn into_child(mut self) -> Child {
        self.child
            .take()
            .expect("startup child guard always owns its child")
    }
}

impl Drop for StartupChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            terminate_and_reap_sync(child);
        }
    }
}

/// One bounded event from the authenticated control stream or supervised
/// process pair.  A child exit always ends the online session, even when the
/// process reported success, because the peer must not remain detached.
#[derive(Debug)]
pub enum SupervisorEvent {
    Control(ControlEvent),
    ChildExited,
}

impl std::fmt::Debug for SupervisedChildren {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupervisedChildren")
            .field("descriptor", &"[REDACTED]")
            .field("sidecar", &"[RUNNING]")
            .field("mgba", &"[RUNNING]")
            .field("control", &self.control)
            .finish_non_exhaustive()
    }
}

#[cfg(windows)]
fn spawn_mgba(spec: &CommandSpec) -> io::Result<MgbaChild> {
    MgbaSupervisor::spawn(&spec.executable, &spec.args).map(MgbaChild::Contained)
}

#[cfg(not(windows))]
fn spawn_mgba(spec: &CommandSpec) -> io::Result<MgbaChild> {
    let mut command = Command::new(&spec.executable);
    isolate_environment(&mut command);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
        .spawn()
        .map(|child| MgbaChild::Tokio(Box::new(child)))
}

impl SupervisedChildren {
    #[cfg(test)]
    pub(crate) fn for_test(sidecar: Child, mgba: Child, control: ControlChannel) -> Self {
        Self {
            sidecar,
            mgba: MgbaChild::Tokio(Box::new(mgba)),
            control,
            rom_cleanup: None,
            rom_marker_cleanup: None,
            rom_implicit_save_path: None,
            rom_identity: None,
            rom_marker_identity: None,
            #[cfg(windows)]
            rom_guards: None,
            #[cfg(windows)]
            rom_marker_guards: None,
            #[cfg(windows)]
            _mgba_executable_guards: None,
            shutdown_disposition: None,
            shutdown_control_attempt: None,
        }
    }

    /// Starts sidecar, authenticates control, then starts stock mGBA with the
    /// validated canonical ROM path and its fixed no-auxiliary-input argv
    /// (12 option tokens followed by the ROM). Stock mGBA 0.10.5 is
    /// intentionally not given an invented --script argument.
    ///
    /// # Errors
    ///
    /// Returns an error when sidecar startup, descriptor validation, control
    /// authentication, or emulator startup fails.
    pub async fn start(
        sidecar: CommandSpec,
        mgba: CommandSpec,
        expected_epoch: u32,
    ) -> Result<Self, ProcessError> {
        Self::start_internal(sidecar, mgba, expected_epoch, None).await
    }

    /// Starts both children after copying the checked-in bridge and writing
    /// the descriptor-derived session.lua into the private workspace.
    ///
    /// The bridge files and session descriptor are prepared after the
    /// sidecar's authenticated descriptor is validated but before mGBA is
    /// spawned, so a manually started stock mGBA can load the exact private
    /// session configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when bridge materialization or child startup fails.
    pub async fn start_with_bridge(
        sidecar: CommandSpec,
        mgba: CommandSpec,
        expected_epoch: u32,
        workspace: &SessionWorkspace,
        bridge_source: &Path,
    ) -> Result<Self, ProcessError> {
        Self::start_internal(
            sidecar,
            mgba,
            expected_epoch,
            Some((workspace, bridge_source)),
        )
        .await
    }

    async fn start_internal(
        sidecar: CommandSpec,
        mgba: CommandSpec,
        expected_epoch: u32,
        bridge: Option<(&SessionWorkspace, &Path)>,
    ) -> Result<Self, ProcessError> {
        let mut mgba = mgba;
        if expected_epoch == 0
            || sidecar.args != ["--session-epoch", &expected_epoch.to_string()]
            || !sidecar.executable.is_absolute()
            || !mgba.executable.is_absolute()
            || sidecar.identity.is_none()
            || mgba.identity.is_none()
            || mgba.rom_identity.is_none()
            || !mgba.has_supported_argv()
        {
            return Err(startup_cleanup(ProcessError::InvalidArgument, &mut mgba));
        }
        if let Err(error) = validate_executable_identity(&sidecar) {
            return Err(startup_cleanup(error, &mut mgba));
        }
        // Validate both executable bindings before any child is spawned. A
        // later mGBA identity failure must never strand a live sidecar while
        // descriptor/control startup is in progress.
        if let Err(error) = validate_executable_identity(&mgba) {
            return Err(startup_cleanup(error, &mut mgba));
        }
        let (mut startup_child, control) =
            Self::start_sidecar(&sidecar, expected_epoch, &mut mgba, bridge).await?;
        // Revalidate immediately before contained CreateProcessW. The
        // existing executable guard held by `mgba` closes the substitution
        // interval between compatibility probing and gameplay startup.
        if let Err(error) = validate_executable_identity(&mgba) {
            return Err(
                startup_failure_with_cleanup(error, startup_child.child_mut(), &mut mgba).await,
            );
        }
        let mgba_child = match spawn_mgba(&mgba) {
            Ok(child) => child,
            Err(error) => {
                return Err(startup_failure_with_cleanup(
                    ProcessError::Spawn(error),
                    startup_child.child_mut(),
                    &mut mgba,
                )
                .await);
            }
        };
        let rom_cleanup = mgba.rom_cleanup.take();
        let rom_marker_cleanup = mgba.rom_marker_cleanup.take();
        let rom_implicit_save_path = mgba.rom_implicit_save_path.take();
        let rom_identity = mgba.rom_identity.take();
        let rom_marker_identity = mgba.rom_marker_identity.take();
        #[cfg(windows)]
        let rom_guards = mgba.rom_guards.take();
        #[cfg(windows)]
        let rom_marker_guards = mgba.rom_marker_guards.take();
        #[cfg(windows)]
        let mgba_executable_guards = mgba.executable_guards.take();
        Ok(Self {
            sidecar: startup_child.into_child(),
            mgba: mgba_child,
            control,
            rom_cleanup,
            rom_marker_cleanup,
            rom_implicit_save_path,
            rom_identity,
            rom_marker_identity,
            #[cfg(windows)]
            rom_guards,
            #[cfg(windows)]
            rom_marker_guards,
            #[cfg(windows)]
            _mgba_executable_guards: mgba_executable_guards,
            shutdown_disposition: None,
            shutdown_control_attempt: None,
        })
    }

    async fn start_sidecar(
        sidecar: &CommandSpec,
        expected_epoch: u32,
        mgba: &mut CommandSpec,
        bridge: Option<(&SessionWorkspace, &Path)>,
    ) -> Result<(StartupChildGuard, ControlChannel), ProcessError> {
        let mut sidecar_command = Command::new(&sidecar.executable);
        isolate_environment(&mut sidecar_command);
        sidecar_command
            .args(&sidecar.args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let child = match sidecar_command.spawn() {
            Ok(child) => child,
            Err(error) => return Err(startup_cleanup(ProcessError::Spawn(error), mgba)),
        };
        let mut startup_child = StartupChildGuard::new(child);
        let Some(stdout) = startup_child.child_mut().stdout.take() else {
            return Err(startup_failure_with_cleanup(
                ProcessError::Descriptor,
                startup_child.child_mut(),
                mgba,
            )
            .await);
        };
        let mut stdout = tokio::io::BufReader::new(stdout);
        let line = timeout(
            PROCESS_IO_TIMEOUT,
            read_line(&mut stdout, MAX_DESCRIPTOR_BYTES),
        )
        .await
        .map_err(|_| ProcessError::DescriptorTimeout)
        .and_then(|result| result);
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                return Err(
                    startup_failure_with_cleanup(error, startup_child.child_mut(), mgba).await,
                );
            }
        };
        let descriptor: SessionDescriptor = if let Ok(value) = serde_json::from_slice(&line) {
            value
        } else {
            return Err(startup_failure_with_cleanup(
                ProcessError::Descriptor,
                startup_child.child_mut(),
                mgba,
            )
            .await);
        };
        if let Err(error) = validate_descriptor(&descriptor, expected_epoch) {
            return Err(startup_failure_with_cleanup(error, startup_child.child_mut(), mgba).await);
        }
        let control = match ControlChannel::connect(&descriptor).await {
            Ok(control) => control,
            Err(error) => {
                return Err(
                    startup_failure_with_cleanup(error, startup_child.child_mut(), mgba).await,
                );
            }
        };
        if let Some((workspace, bridge_source)) = bridge
            && let Err(error) =
                materialize_bridge_session(workspace, bridge_source, &descriptor, expected_epoch)
        {
            return Err(startup_failure_with_cleanup(error, startup_child.child_mut(), mgba).await);
        }
        Ok((startup_child, control))
    }

    /// Stops both children and attempts to reap their root processes.
    ///
    /// The mGBA Job-termination request is synchronous and uncancellable;
    /// root exit observation is not proof that the Job has no descendants.
    ///
    /// # Errors
    ///
    /// Returns an error when either child cannot be terminated or reaped.
    pub async fn stop(self) -> Result<(), ProcessError> {
        let mut this = self;
        this.stop_in_place().await
    }

    /// Stops and reaps both children while retaining the supervisor value for
    /// an enclosing lifecycle compensation path.
    ///
    /// # Errors
    ///
    /// Returns an error when either child cannot be terminated or reaped.
    pub async fn stop_in_place(&mut self) -> Result<(), ProcessError> {
        if let Some(disposition) = self.shutdown_disposition {
            return if disposition.clean() {
                Ok(())
            } else {
                Err(ProcessError::Termination(io::Error::other(
                    "shutdown evidence requires recovery",
                )))
            };
        }
        let mgba_result = self.mgba.stop().await;
        let sidecar_result = stop_child(&mut self.sidecar).await;
        mgba_result.and(sidecar_result)?;
        self.cleanup_owned_rom()
    }

    /// Completes the authenticated launcher shutdown transaction.  The
    /// control request is sent at most once, only after the caller has drained
    /// an already-authorized checkpoint.  Every malformed, rejected, stale,
    /// mismatched, EOF, or timed-out response selects the forced Job path.
    ///
    /// This method always returns typed evidence, including uncertainty, so a
    /// caller can retain recovery material without mistaking a cleanup error
    /// for proof of descendant exit.  Repeated calls replay the same cached
    /// disposition without sending another command or repeating cleanup.
    pub async fn shutdown(
        &mut self,
        expected_epoch: u32,
        checkpoint_drained: bool,
    ) -> ShutdownDisposition {
        if let Some(disposition) = self.shutdown_disposition {
            return disposition;
        }
        let deadline = tokio::time::Instant::now() + SHUTDOWN_DEADLINE;
        let control_acknowledged = checkpoint_drained
            && self
                .request_authenticated_shutdown(expected_epoch, deadline)
                .await;
        let std_deadline =
            Instant::now() + deadline.saturating_duration_since(tokio::time::Instant::now());
        let (mgba, sidecar_reaped) = tokio::join!(
            self.mgba.shutdown(control_acknowledged, std_deadline),
            reap_sidecar_until(&mut self.sidecar, deadline, control_acknowledged),
        );
        let disposition = ShutdownDisposition {
            path: if control_acknowledged {
                ShutdownPath::Graceful
            } else {
                ShutdownPath::Forced
            },
            sidecar: if sidecar_reaped {
                RootReapEvidence::Reaped
            } else {
                RootReapEvidence::Unknown
            },
            mgba: mgba.root_reap,
            job_termination: mgba.job_termination,
            control: if control_acknowledged {
                ControlShutdownEvidence::Accepted
            } else {
                ControlShutdownEvidence::NotAccepted
            },
            soft_close: mgba.soft_close,
            recovery: if sidecar_reaped && matches!(mgba.recovery, RecoveryDisposition::Clean) {
                RecoveryDisposition::Clean
            } else {
                RecoveryDisposition::Required
            },
            descendant_completion: DescendantCompletionEvidence::NotAvailable,
        };
        self.shutdown_disposition = Some(disposition);
        // Cleanup is intentionally gated by complete root evidence.  If it
        // fails, preserve the owned paths for the caller's recovery flow.
        if disposition.clean() && self.cleanup_owned_rom().is_err() {
            self.shutdown_disposition = Some(ShutdownDisposition {
                recovery: RecoveryDisposition::Required,
                ..disposition
            });
        }
        self.shutdown_disposition.unwrap_or(disposition)
    }

    #[must_use]
    pub const fn shutdown_disposition(&self) -> Option<ShutdownDisposition> {
        self.shutdown_disposition
    }

    async fn request_authenticated_shutdown(
        &mut self,
        expected_epoch: u32,
        deadline: tokio::time::Instant,
    ) -> bool {
        let ack_deadline = (tokio::time::Instant::now() + SHUTDOWN_ACK_BOUND).min(deadline);
        if expected_epoch == 0 || tokio::time::Instant::now() >= ack_deadline {
            return false;
        }
        let (command_id, sent) = if let Some(attempt) = self.shutdown_control_attempt.as_ref() {
            if attempt.expected_epoch != expected_epoch {
                return false;
            }
            (attempt.command_id, attempt.sent)
        } else {
            let attempt = ShutdownControlAttempt {
                command_id: new_command_id(),
                expected_epoch,
                // Set this before the write. A cancellation in the write
                // future must never cause a retry to send a second command.
                sent: true,
            };
            let command_id = attempt.command_id;
            self.shutdown_control_attempt = Some(attempt);
            (command_id, false)
        };
        let command = ControlCommand::ShutdownRequest(ShutdownRequest {
            command_id,
            session_epoch: expected_epoch,
        });
        if !sent {
            let written = tokio::time::timeout_at(ack_deadline, self.control.send(&command)).await;
            if !matches!(written, Ok(Ok(()))) {
                return false;
            }
        }
        loop {
            let Ok(Ok(event)) = tokio::time::timeout_at(ack_deadline, self.control.receive()).await
            else {
                return false;
            };
            match event {
                ControlEvent::CommandResult {
                    command_id: received_id,
                    status,
                    reason,
                } => {
                    // Command results are correlated strictly.  A result for
                    // any other command is not evidence for this shutdown.
                    return received_id == command_id
                        && matches!(status, CommandStatus::Applied | CommandStatus::Replayed)
                        && reason.is_none();
                }
                // Existing FIFO events may be ahead of the ACK. They are not
                // acknowledgements, but may be drained until the one global
                // deadline without granting them control over shutdown.
                ControlEvent::CheckpointReady { .. }
                | ControlEvent::SaveDataUpdated { .. }
                | ControlEvent::CheckpointExpired { .. } => {}
            }
        }
    }

    /// Waits for either child and terminates/reaps its peer before returning.
    ///
    /// This is the supervision path for a normal running session: a sidecar
    /// or emulator exit cannot leave the other process detached. Dropping the
    /// supervisor also requests termination as a cancellation safety net.
    ///
    /// # Errors
    ///
    /// Returns an error when the first child or its peer cannot be reaped.
    pub async fn wait(mut self) -> Result<(), ProcessError> {
        let (first, peer) = tokio::select! {
            result = self.sidecar.wait() => {
                let first = result.map_err(ProcessError::Termination).and_then(|status| {
                    if status.success() { Ok(()) } else { Err(ProcessError::ChildExited) }
                });
                let peer = self.mgba.stop().await;
                (first, peer)
            }
            result = self.mgba.wait() => {
                let first = result.map_err(ProcessError::Termination).and_then(|status| {
                    if status.success() { Ok(()) } else { Err(ProcessError::ChildExited) }
                });
                let peer = stop_child(&mut self.sidecar).await;
                (first, peer)
            }
        };
        match (first, peer) {
            (Ok(()), Ok(())) => self.cleanup_owned_rom(),
            // Both children are reaped even when the first one exits with a
            // failure status. Cleanup must still be explicit in that case;
            // preserve both failures if the filesystem also refuses removal.
            (Err(event), Ok(())) => match self.cleanup_owned_rom() {
                Ok(()) => Err(event),
                Err(cleanup) => Err(ProcessError::EventCleanup {
                    event: Box::new(event),
                    cleanup: Box::new(cleanup),
                }),
            },
            (Ok(()) | Err(_), Err(cleanup)) => Err(cleanup),
        }
    }

    /// Waits for one control event or for either child to exit.  The method
    /// owns the race so a caller can schedule lease heartbeats and checkpoint
    /// work without ever leaving a peer process orphaned.
    ///
    /// # Errors
    ///
    /// Returns an error when control I/O or child reaping fails. Any control
    /// or protocol error first stops both children and attempts root reaping;
    /// if that cleanup status is uncertain, the returned
    /// [`ProcessError::EventCleanup`] preserves both failures for
    /// lease-release gating.
    pub async fn next_event(&mut self) -> Result<SupervisorEvent, ProcessError> {
        let result = tokio::select! {
            result = self.control.receive() => result.map(SupervisorEvent::Control),
            result = self.sidecar.wait() => {
                match result {
                    Err(error) => Err(ProcessError::Termination(error)),
                    Ok(status) => match self.mgba.stop().await {
                        Err(error) => Err(error),
                        Ok(()) => self.child_exit_after_reap(status.success()),
                    },
                }
            }
            result = self.mgba.wait() => {
                match result {
                    Err(error) => Err(ProcessError::Termination(error)),
                    Ok(status) => match stop_child(&mut self.sidecar).await {
                        Err(error) => Err(error),
                        Ok(()) => self.child_exit_after_reap(status.success()),
                    },
                }
            }
        };
        match result {
            Ok(event) => Ok(event),
            // Child-exit branches have already reaped both children and have
            // performed the explicit artifact cleanup. Do not run the
            // fail-closed stop path a second time: that would obscure the
            // original cleanup failure (or wrap it in a duplicate error).
            Err(error @ (ProcessError::Cleanup { .. } | ProcessError::EventCleanup { .. })) => {
                Err(error)
            }
            Err(error) => Err(self.fail_closed(error).await),
        }
    }

    fn child_exit_after_reap(&mut self, success: bool) -> Result<SupervisorEvent, ProcessError> {
        let event = if success {
            Ok(SupervisorEvent::ChildExited)
        } else {
            Err(ProcessError::ChildExited)
        };
        match (event, self.cleanup_owned_rom()) {
            (Ok(event), Ok(())) => Ok(event),
            (Err(event), Ok(())) => Err(event),
            (Ok(_), Err(cleanup)) => Err(cleanup),
            (Err(event), Err(cleanup)) => Err(ProcessError::EventCleanup {
                event: Box::new(event),
                cleanup: Box::new(cleanup),
            }),
        }
    }

    async fn fail_closed(&mut self, event: ProcessError) -> ProcessError {
        match self.stop_in_place().await {
            Ok(()) => event,
            Err(cleanup) => ProcessError::EventCleanup {
                event: Box::new(event),
                cleanup: Box::new(cleanup),
            },
        }
    }

    /// Removes all launcher-owned mGBA artifacts after both root processes
    /// have reported termination. That observation is not proof that a Job
    /// has no descendants. The marker is deliberately last: when a
    /// filesystem operation fails, its path and any remaining files stay
    /// available for explicit recovery.
    fn cleanup_owned_rom(&mut self) -> Result<(), ProcessError> {
        if let Some(path) = self.rom_implicit_save_path.clone() {
            #[cfg(windows)]
            remove_owned_implicit_save(
                &path,
                self.rom_cleanup.as_deref(),
                self.rom_guards.as_ref(),
            )?;
            #[cfg(not(windows))]
            remove_owned_file(&path, CleanupArtifact::ImplicitSave)?;
            self.rom_implicit_save_path = None;
        }
        let rom_identity = self.rom_identity.clone();
        let rom_path = self.rom_cleanup.clone();
        if let Some(path) = rom_path.as_deref() {
            #[cfg(windows)]
            remove_owned_guarded_file(
                path,
                CleanupArtifact::Rom,
                rom_identity.as_ref(),
                &mut self.rom_guards,
            )?;
            #[cfg(not(windows))]
            remove_owned_file(path, CleanupArtifact::Rom)?;
            self.rom_cleanup = None;
        }
        if let Some(path) = self.rom_marker_cleanup.clone() {
            #[cfg(windows)]
            {
                if let (Some(identity), Some(marker_guards)) =
                    (rom_identity.as_ref(), self.rom_marker_guards.as_ref())
                    && !owned_rom_marker_matches_held_identity(identity, &marker_guards.file)
                        .unwrap_or(false)
                {
                    return Err(ProcessError::Cleanup {
                        artifact: CleanupArtifact::Marker,
                        path,
                        source: io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "ownership marker content changed",
                        ),
                    });
                }
                remove_owned_guarded_file(
                    &path,
                    CleanupArtifact::Marker,
                    self.rom_marker_identity.as_ref(),
                    &mut self.rom_marker_guards,
                )?;
            }
            #[cfg(not(windows))]
            remove_owned_file(&path, CleanupArtifact::Marker)?;
            self.rom_marker_cleanup = None;
            self.rom_identity = None;
            self.rom_marker_identity = None;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn executable_binding(
    path: &Path,
) -> Result<(Option<ExecutableIdentity>, Option<ExecutableGuards>), ProcessError> {
    binding_with_access(path, false)
}

#[cfg(windows)]
fn owned_file_binding(
    path: &Path,
) -> Result<(Option<ExecutableIdentity>, Option<ExecutableGuards>), ProcessError> {
    binding_with_access(path, true)
}

#[cfg(windows)]
fn binding_with_access(
    path: &Path,
    delete_access: bool,
) -> Result<(Option<ExecutableIdentity>, Option<ExecutableGuards>), ProcessError> {
    if matches!(
        fs::symlink_metadata(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    ) {
        // The public constructors remain useful for argv-only tests and for
        // the supervised spawn error path.  The production CLI canonicalizes
        // executable paths before constructing a spec, so a missing path can
        // never reach a real launch attempt.
        return Ok((None, None));
    }
    reject_symlink_ancestors(path)?;
    let parent = path.parent().ok_or(ProcessError::InvalidArgument)?;
    let ancestor_guards = open_directory_ancestor_guards(parent, delete_access)
        .map_err(|_| ProcessError::InvalidArgument)?;
    let Some(file) = open_executable_file(path, delete_access)? else {
        return Ok((None, None));
    };
    let file_guard = Arc::new(file);
    let handle_metadata = file_guard
        .metadata()
        .map_err(|_| ProcessError::InvalidArgument)?;
    if !handle_metadata.is_file() {
        return Err(ProcessError::InvalidArgument);
    }
    let canonical = fs::canonicalize(path).map_err(|_| ProcessError::InvalidArgument)?;
    let digest = hash_file_handle_bounded(&file_guard, crate::compat::MAX_MGBA_EXECUTABLE_BYTES)
        .map_err(|_| ProcessError::InvalidArgument)?;
    Ok((
        Some(ExecutableIdentity {
            canonical,
            length: handle_metadata.len(),
            modified: handle_metadata.modified().ok(),
            digest,
        }),
        Some(ExecutableGuards {
            file: file_guard,
            ancestors: ancestor_guards,
        }),
    ))
}

#[cfg(windows)]
pub(crate) fn with_mgba_executable_guard<T>(
    path: &Path,
    operation: impl FnOnce(&Path, [u8; 32]) -> T,
) -> Result<T, ProcessError> {
    let (identity, guards) = executable_binding(path)?;
    let Some(identity) = identity else {
        return Err(ProcessError::MgbaIdentity);
    };
    let Some(_guards) = guards else {
        return Err(ProcessError::MgbaIdentity);
    };
    let digest = crate::compat::expected_mgba_executable_digest();
    if identity.digest != digest {
        return Err(ProcessError::MgbaIdentity);
    }
    Ok(operation(identity.canonical.as_path(), identity.digest))
}

#[cfg(not(windows))]
fn executable_binding(
    path: &Path,
) -> Result<(Option<ExecutableIdentity>, Option<()>), ProcessError> {
    if matches!(
        fs::symlink_metadata(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    ) {
        return Ok((None, None));
    }
    reject_symlink_ancestors(path)?;
    match fs::symlink_metadata(path) {
        Ok(_) => {
            // The portable standard library cannot hold a deny-write/delete
            // executable handle across validation and spawn. Refuse existing
            // production executable paths instead of claiming that a
            // check-then-use race is safe. Missing paths remain useful for
            // argument-only tests and fail at spawn as expected. The typed
            // boundary makes the production Windows-only guarantee explicit.
            Err(ProcessError::UnsupportedPlatform)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((None, None)),
        Err(_) => Err(ProcessError::InvalidArgument),
    }
}

#[cfg(not(windows))]
fn owned_file_binding(
    path: &Path,
) -> Result<(Option<ExecutableIdentity>, Option<()>), ProcessError> {
    executable_binding(path)
}

#[cfg(windows)]
fn open_executable_file(
    path: &Path,
    delete_access: bool,
) -> Result<Option<fs::File>, ProcessError> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    let _ = delete_access;
    // mGBA opens the ROM through the read-only CRT. Requesting DELETE on this
    // long-lived handle makes that open fail when the CRT does not include
    // FILE_SHARE_DELETE. Cleanup takes a separate delete handle after this
    // read-only integrity guard is released.
    options.read(true).share_mode(0x0000_0001);
    options.custom_flags(0x0020_0000);
    match options.open(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ProcessError::InvalidArgument),
    }
}

#[cfg(windows)]
fn open_directory_guard(path: &Path, delete_access: bool) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        // Owned staged artifacts must share delete so DELETE_ON_CLOSE cleanup
        // can proceed. Executable bindings deny directory delete/rename while
        // the guarded path remains live, closing the ancestor substitution
        // interval before CreateProcessW.
        .share_mode(if delete_access {
            0x0000_0007
        } else {
            0x0000_0003
        })
        .custom_flags(0x0220_0000);
    options.open(path)
}

#[cfg(windows)]
fn open_directory_ancestor_guards(
    path: &Path,
    delete_access: bool,
) -> io::Result<Vec<Arc<fs::File>>> {
    let mut guards = Vec::new();
    let mut current = path;
    loop {
        guards.push(Arc::new(open_directory_guard(current, delete_access)?));
        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    Ok(guards)
}

fn hash_file_handle(handle: &fs::File) -> io::Result<[u8; 32]> {
    hash_file_handle_bounded(handle, u64::MAX)
}

fn hash_file_handle_bounded(handle: &fs::File, max_bytes: u64) -> io::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    if handle.metadata()?.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds hashing limit",
        ));
    }
    let mut file = handle.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file exceeds hashing limit",
            ));
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

fn validate_executable_identity(spec: &CommandSpec) -> Result<(), ProcessError> {
    #[cfg(windows)]
    let guarded_identity = if spec.mgba {
        let guards = spec
            .executable_guards
            .as_ref()
            .ok_or(ProcessError::MgbaIdentity)?;
        // Reading metadata through both held handles keeps the guards live
        // through this immediate validation-to-spawn boundary and documents
        // that they are security-critical, not incidental storage.
        let actual = file_identity(&spec.executable, &guards.file)
            .map_err(|_| ProcessError::MgbaIdentity)?;
        if actual.digest != crate::compat::expected_mgba_executable_digest() {
            return Err(ProcessError::MgbaIdentity);
        }
        for ancestor in &guards.ancestors {
            let _ = ancestor.metadata();
        }
        Some(actual)
    } else {
        None
    };
    if let Some(expected) = &spec.identity {
        #[cfg(windows)]
        let actual = if let Some(actual) = guarded_identity {
            Some(actual)
        } else {
            executable_binding(&spec.executable)?.0
        };
        #[cfg(not(windows))]
        let actual = executable_binding(&spec.executable)?.0;
        let Some(actual) = actual else {
            return Err(ProcessError::InvalidArgument);
        };
        if &actual != expected {
            return Err(ProcessError::InvalidArgument);
        }
    }
    if spec.args.len() == 1 || spec.args.len() == OWNED_MGBA_ARG_COUNT {
        let rom = if spec.args.len() == 1 {
            Path::new(&spec.args[0])
        } else {
            Path::new(&spec.args[OWNED_MGBA_ROM_ARG_INDEX])
        };
        if let Some(expected_rom) = &spec.rom_identity {
            #[cfg(windows)]
            let actual_rom = {
                let guards = spec.rom_guards.as_ref().ok_or(ProcessError::MgbaIdentity)?;
                file_identity(rom, &guards.file).map_err(|_| ProcessError::MgbaIdentity)?
            };
            #[cfg(not(windows))]
            let actual_rom = executable_binding(rom)?
                .0
                .ok_or(ProcessError::InvalidArgument)?;
            if &actual_rom != expected_rom {
                return Err(ProcessError::InvalidArgument);
            }
            ensure_no_auxiliary_inputs(rom)?;
            #[cfg(windows)]
            if spec.args.len() == OWNED_MGBA_ARG_COUNT {
                let marker = spec
                    .rom_marker_guards
                    .as_ref()
                    .ok_or(ProcessError::MgbaIdentity)?;
                if !owned_rom_marker_matches_held_identity(expected_rom, &marker.file)
                    .map_err(|_| ProcessError::MgbaIdentity)?
                    || ensure_no_auxiliary_inputs(&expected_rom.canonical).is_err()
                {
                    return Err(ProcessError::InvalidArgument);
                }
            }
        } else if fs::symlink_metadata(rom).is_ok() {
            // A missing ROM at binding time must not become an unbound input
            // just before spawn. This closes the create-after-validation race.
            return Err(ProcessError::InvalidArgument);
        }
    }
    Ok(())
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), ProcessError> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ProcessError::InvalidArgument);
            }
            Ok(metadata) if current != path && !metadata.is_dir() => {
                return Err(ProcessError::InvalidArgument);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound && current == path => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(_) => return Err(ProcessError::InvalidArgument),
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

fn ensure_absent(path: &Path) -> Result<(), ProcessError> {
    match fs::symlink_metadata(path) {
        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(ProcessError::InvalidArgument)
            }
        }
        Ok(_) => Err(ProcessError::InvalidArgument),
    }
}

fn ensure_no_auxiliary_inputs(rom: &Path) -> Result<(), ProcessError> {
    // mGBA discovers these same-base files before opening the ROM. A staged
    // namespace must therefore contain no unbound patch/cheat input that
    // could change the effective game image or execution behavior.
    for extension in ["ips", "ups", "bps", "cheats"] {
        ensure_absent(&rom.with_extension(extension))?;
    }
    Ok(())
}

fn remove_owned_file(path: &Path, artifact: CleanupArtifact) -> Result<(), ProcessError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ProcessError::Cleanup {
                artifact,
                path: path.to_path_buf(),
                source,
            });
        }
    };
    // Never follow a link while cleaning a path whose ownership was granted
    // by a marker. Refusing a replaced or unexpected entry retains the marker
    // for recovery instead of deleting a caller-owned target.
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ProcessError::Cleanup {
            artifact,
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "owned path is not a file"),
        });
    }
    fs::remove_file(path).map_err(|source| ProcessError::Cleanup {
        artifact,
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn remove_owned_guarded_file(
    path: &Path,
    artifact: CleanupArtifact,
    expected: Option<&ExecutableIdentity>,
    guards: &mut Option<ExecutableGuards>,
) -> Result<(), ProcessError> {
    let Some(expected) = expected else {
        // This is only reachable for test-only supervisors assembled without
        // the production identity handles. A real owned spec always binds a
        // identity/deletion guard before exposing cleanup ownership.
        return remove_owned_file(path, artifact);
    };
    let Some(guards_ref) = guards.as_ref() else {
        return Err(ProcessError::Cleanup {
            artifact,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "owned identity handle is unavailable",
            ),
        });
    };
    let actual = file_identity(path, &guards_ref.file).map_err(|source| ProcessError::Cleanup {
        artifact,
        path: path.to_path_buf(),
        source,
    })?;
    if &actual != expected {
        return Err(ProcessError::Cleanup {
            artifact,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "owned path identity changed",
            ),
        });
    }
    // Reopen the pathname without delete-on-close and compare it to the
    // identity that was bound at startup. This catches a replacement that
    // occurred while the process was running before any deletion handle is
    // created; only an exact current object may proceed.
    let current_identity = match path_identity(path) {
        Ok(identity) => identity,
        Err(source) => {
            return Err(ProcessError::Cleanup {
                artifact,
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if &current_identity != expected {
        return Err(ProcessError::Cleanup {
            artifact,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "owned path was replaced before delete",
            ),
        });
    }
    // The read-only guard intentionally cannot coexist with a DELETE access
    // request: the official CRT does not promise FILE_SHARE_DELETE. Release
    // it only after the final identity check, then open DELETE_ON_CLOSE. Safe
    // Rust cannot make this final pathname step atomic against a hostile
    // same-user process; that documented cleanup race is retained.
    let _ = guards.take();
    let delete_file = match open_delete_on_close(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ProcessError::Cleanup {
                artifact,
                path: path.to_path_buf(),
                source,
            });
        }
    };
    // Once opened, DELETE_ON_CLOSE removes the exact object referenced by the
    // handle; no path unlink is attempted after the guard is released.
    drop(delete_file);
    Ok(())
}

#[cfg(windows)]
fn remove_owned_implicit_save(
    path: &Path,
    rom_path: Option<&Path>,
    rom_guards: Option<&ExecutableGuards>,
) -> Result<(), ProcessError> {
    let Some(rom_path) = rom_path else {
        return Err(ProcessError::Cleanup {
            artifact: CleanupArtifact::ImplicitSave,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "implicit save ownership is unproven",
            ),
        });
    };
    let Some(rom_parent) = rom_path.parent() else {
        return Err(ProcessError::Cleanup {
            artifact: CleanupArtifact::ImplicitSave,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "implicit save parent is invalid",
            ),
        });
    };
    if path != rom_path.with_extension("sav") || path.parent() != Some(rom_parent) {
        return Err(ProcessError::Cleanup {
            artifact: CleanupArtifact::ImplicitSave,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "implicit save path is not paired with the staged ROM",
            ),
        });
    }
    let Some(_rom_guards) = rom_guards else {
        #[cfg(test)]
        return remove_owned_file(path, CleanupArtifact::ImplicitSave);
        #[cfg(not(test))]
        return Err(ProcessError::Cleanup {
            artifact: CleanupArtifact::ImplicitSave,
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private staged-ROM directory is not guarded",
            ),
        });
    };
    // Establish a no-follow identity before requesting DELETE_ON_CLOSE. The
    // fresh per-launch directory narrows accidental interference, while the
    // same-user atomic-replacement limitation remains a documented production
    // blocker.
    let _expected = match path_identity(path) {
        Ok(identity) => identity,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ProcessError::Cleanup {
                artifact: CleanupArtifact::ImplicitSave,
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let file = open_delete_on_close(path).map_err(|source| ProcessError::Cleanup {
        artifact: CleanupArtifact::ImplicitSave,
        path: path.to_path_buf(),
        source,
    })?;
    // Do not canonicalize after opening: Windows hides a delete-pending name.
    // The pre-open no-follow identity and per-launch directory bind normal
    // cleanup to the launcher-created save within the documented threat model.
    // The no-follow DELETE_ON_CLOSE handle is bound to the exact file object;
    // closing it is the explicit cleanup point.
    drop(file);
    Ok(())
}

#[cfg(windows)]
fn open_delete_on_close(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .access_mode(0x8000_0000 | 0x0001_0000)
        .share_mode(0x0000_0007)
        .custom_flags(0x0420_0000);
    options.open(path)
}

#[cfg(windows)]
fn file_identity(path: &Path, file: &fs::File) -> io::Result<ExecutableIdentity> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "owned path is not a regular file",
        ));
    }
    Ok(ExecutableIdentity {
        canonical: fs::canonicalize(path)?,
        length: metadata.len(),
        modified: metadata.modified().ok(),
        digest: hash_file_handle_bounded(file, crate::compat::MAX_MGBA_EXECUTABLE_BYTES)?,
    })
}

impl Drop for SupervisedChildren {
    fn drop(&mut self) {
        // Drop cannot await. It asks the mGBA supervisor to request Job
        // termination and join its owner; the native operation may exceed its
        // nominal polling deadline. It then boundedly kills and polls only
        // the sidecar root. Neither root observation proves an empty Job.
        let mgba = self.mgba.stop_sync();
        let sidecar_reaped = terminate_and_reap_sync(&mut self.sidecar);
        if drop_cleanup_allowed(mgba, sidecar_reaped) {
            let _ = self.cleanup_owned_rom();
        }
    }
}

fn drop_cleanup_allowed(mgba: MgbaShutdownReport, sidecar_reaped: bool) -> bool {
    sidecar_reaped
        && matches!(mgba.root_reap, RootReapEvidence::Reaped)
        && matches!(mgba.job_termination, JobTerminationEvidence::Initiated)
        && matches!(mgba.recovery, RecoveryDisposition::Clean)
}

fn terminate_and_reap_sync(child: &mut Child) -> bool {
    let already_exited = child.try_wait().ok().flatten().is_some();
    if !already_exited {
        let _ = child.start_kill();
    }
    let deadline = Instant::now() + PROCESS_IO_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Err(_) => return false,
            Ok(None) if Instant::now() >= deadline => return false,
            Ok(None) => std::thread::sleep(DROP_REAP_POLL),
        }
    }
}

fn isolate_environment(command: &mut Command) {
    // Child tools receive no inherited credentials or configuration. PATH is
    // retained solely for explicitly relative executable names.
    let path = std::env::var_os("PATH");
    command.env_clear();
    if let Some(path) = path {
        command.env("PATH", path);
    }
}

async fn startup_failure(startup: ProcessError, child: &mut Child) -> ProcessError {
    match stop_child(child).await {
        Ok(()) => startup,
        Err(cleanup) => ProcessError::StartupCleanup {
            startup: Box::new(startup),
            cleanup: Box::new(cleanup),
        },
    }
}

fn startup_cleanup(startup: ProcessError, mgba: &mut CommandSpec) -> ProcessError {
    match mgba.cleanup_owned_rom() {
        Ok(()) => startup,
        Err(cleanup) => ProcessError::StartupCleanup {
            startup: Box::new(startup),
            cleanup: Box::new(cleanup),
        },
    }
}

async fn startup_failure_with_cleanup(
    startup: ProcessError,
    child: &mut Child,
    mgba: &mut CommandSpec,
) -> ProcessError {
    let startup = startup_failure(startup, child).await;
    if !startup.cleanup_confirmed() {
        // Cleanup ownership remains with the marker when the sidecar could
        // not be confirmed reaped. Drop still performs a cancellation
        // fallback, but its mGBA TerminateJobObject request is synchronous
        // and may exceed the nominal deadline; never turn an unconfirmed
        // child into a silently successful artifact deletion here.
        return startup;
    }
    startup_cleanup(startup, mgba)
}

async fn stop_child(child: &mut Child) -> Result<(), ProcessError> {
    // This is the fail-closed fallback for startup failures or shutdown paths
    // where the authenticated control transaction was unavailable or not
    // accepted. Keep cancellation deterministic with the bounded kill-and-reap
    // operation below instead of inventing an implicit signal or stdin
    // convention.
    match child.try_wait().map_err(ProcessError::Termination)? {
        Some(_) => return Ok(()),
        None => {
            if let Err(error) = child.start_kill() {
                // A natural exit can race the kill request. Recheck before
                // reporting uncertainty so normal process termination is not
                // mistaken for a failed cleanup.
                if child
                    .try_wait()
                    .map_err(ProcessError::Termination)?
                    .is_some()
                {
                    return Ok(());
                }
                return Err(ProcessError::Termination(error));
            }
        }
    }
    let waited = timeout(PROCESS_IO_TIMEOUT, child.wait())
        .await
        .map_err(|_| {
            ProcessError::Termination(io::Error::new(
                io::ErrorKind::TimedOut,
                "child did not exit",
            ))
        })?;
    match waited {
        Ok(_) => Ok(()),
        Err(error) => {
            // Some platforms report a benign no-child race from `wait` after
            // the process exits between the kill and reap calls. Confirm the
            // terminal state before surfacing cleanup uncertainty.
            if child
                .try_wait()
                .map_err(ProcessError::Termination)?
                .is_some()
            {
                Ok(())
            } else {
                Err(ProcessError::Termination(error))
            }
        }
    }
}

async fn reap_sidecar_until(
    child: &mut Child,
    deadline: tokio::time::Instant,
    allow_natural_wait: bool,
) -> bool {
    let natural_deadline = if allow_natural_wait {
        (tokio::time::Instant::now() + Duration::from_secs(2)).min(deadline)
    } else {
        tokio::time::Instant::now()
    };
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Err(_) => return false,
            Ok(None) => {}
        }
        let now = tokio::time::Instant::now();
        if now >= natural_deadline {
            // start_kill is the only sidecar fallback; a race with natural
            // exit is confirmed through the retained Child handle below.
            if child.start_kill().is_err() && child.try_wait().ok().flatten().is_none() {
                return false;
            }
            let value = matches!(
                tokio::time::timeout_at(deadline, child.wait()).await,
                Ok(Ok(_))
            );
            return value;
        }
        if now >= deadline {
            return false;
        }
        let next_poll = (now + Duration::from_millis(10)).min(deadline);
        tokio::time::sleep_until(next_poll).await;
    }
}

#[cfg(any(not(windows), test))]
async fn reap_tokio_child_until(child: &mut Child, deadline: Instant) -> bool {
    if child.try_wait().ok().flatten().is_some() {
        return true;
    }
    if child.start_kill().is_err() && child.try_wait().ok().flatten().is_none() {
        return false;
    }
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Err(_) => return false,
            Ok(None) if Instant::now() >= deadline => return false,
            Ok(None) => {
                let next_poll = (Instant::now() + Duration::from_millis(10)).min(deadline);
                tokio::time::sleep_until(tokio::time::Instant::from_std(next_poll)).await;
            }
        }
    }
}

/// # Panics
///
/// This cannot panic because a UUID v4 is rendered in canonical form.
#[must_use]
pub fn new_command_id() -> coop_sidecar::control::CommandId {
    // UUID v4 is canonicalized by CommandId parsing and never exposed as a secret.
    coop_sidecar::control::CommandId::parse(&Uuid::new_v4().hyphenated().to_string())
        .expect("UUID v4 is canonical")
}

// Keep compile-time coupling explicit while allowing hidden tests to assert the
// launcher uses the same DTO constants as both cloud and sidecar.
const _: Option<BridgeAbiVersion> = None;
const _: Option<ProtocolVersion> = None;

#[cfg(test)]
mod tests {
    use super::{
        CleanupArtifact, CommandSpec, ControlChannel, ControlShutdownEvidence,
        DescendantCompletionEvidence, JobTerminationEvidence, MgbaShutdownReport,
        OWNED_MGBA_ROM_ARG_INDEX, PROCESS_IO_TIMEOUT, ProcessError, RecoveryDisposition,
        RootReapEvidence, ShutdownPath, SoftCloseDisposition, SupervisedChildren, SupervisorEvent,
        drop_cleanup_allowed, ensure_absent, new_command_id, terminate_and_reap_sync,
    };
    #[cfg(windows)]
    use super::{
        materialize_bridge_session, staged_rom_marker_contents, staged_rom_marker_path,
        validate_executable_identity,
    };
    #[cfg(windows)]
    use crate::session::SessionWorkspace;
    #[cfg(windows)]
    use coop_sidecar::LocalSidecar;
    use coop_sidecar::control::{
        CommandStatus, ControlCommand, ControlEvent, MAX_CONTROL_LINE_BYTES,
    };
    #[cfg(windows)]
    use std::fs;
    use std::{process::Stdio, time::Duration};
    #[cfg(windows)]
    use tempfile::tempdir;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        process::Command,
    };

    fn long_running_child() -> tokio::process::Child {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("ping.exe");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("long-running test child")
    }

    async fn unused_control_channel() -> ControlChannel {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        // No control operation is performed by cleanup tests. Keeping the
        // listener alive until the returned stream is dropped is sufficient
        // to make the local connection deterministic without a protocol peer.
        drop(listener);
        ControlChannel::from_stream_for_test(stream)
    }

    #[tokio::test]
    async fn cancellation_termination_reaps_child_within_bound() {
        let started = std::time::Instant::now();
        let mut child = long_running_child();
        terminate_and_reap_sync(&mut child);
        assert!(child.try_wait().expect("reap status").is_some());
        assert!(started.elapsed() < PROCESS_IO_TIMEOUT + std::time::Duration::from_secs(1));
    }

    #[cfg(windows)]
    #[test]
    fn executable_guard_denies_replacement_after_binding() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("tool.exe");
        fs::write(&executable, b"trusted").unwrap();
        let spec = CommandSpec::sidecar(&executable, 1).unwrap();
        let replacement = directory.path().join("replacement.exe");
        fs::write(&replacement, b"attacker").unwrap();
        assert!(fs::remove_file(&executable).is_err());
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&executable)
                .is_err()
        );
        assert!(validate_executable_identity(&spec).is_ok());
        drop(spec);
        fs::remove_file(&executable).unwrap();
        fs::rename(replacement, executable).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn rom_binding_denies_replacement_until_spec_is_dropped() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let spec = CommandSpec::mgba(&executable, &rom).unwrap();
        assert!(spec.rom_identity.is_some());
        assert!(fs::remove_file(&rom).is_err());
        drop(spec);
        fs::remove_file(&rom).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn staged_rom_is_removed_when_unspawned_spec_is_dropped() {
        let root = std::env::temp_dir()
            .join("pokecrossroads-coop-launcher")
            .join(format!("test-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = root.join(format!("rom-{}.gba", uuid::Uuid::new_v4().simple()));
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        {
            let marker = staged_rom_marker_path(&rom);
            fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
            let spec = CommandSpec::mgba_owned_staged(&executable, &rom, &marker).unwrap();
            assert!(spec.rom_cleanup.is_some());
        }
        assert!(!rom.exists());
        assert!(!staged_rom_marker_path(&rom).exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn materialize_bridge_session_uses_validated_descriptor_path() {
        let root = tempdir().unwrap();
        let workspace = SessionWorkspace::create(root.path()).unwrap();
        let sidecar = LocalSidecar::bind_with_epoch(7).await.unwrap();
        let descriptor = sidecar.session_descriptor();
        let bridge = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../bridge")
            .canonicalize()
            .unwrap();

        materialize_bridge_session(&workspace, &bridge, &descriptor, 7).unwrap();

        let session_lua = fs::read_to_string(workspace.path().join("session.lua")).unwrap();
        assert!(session_lua.contains(descriptor.bridge().port().to_string().as_str()));
        assert!(session_lua.contains(descriptor.bridge().secret()));
        for name in ["main.lua", "memory.lua", "protocol.lua"] {
            assert_eq!(
                fs::read(workspace.path().join(name)).unwrap(),
                fs::read(bridge.join(name)).unwrap()
            );
        }
        drop(sidecar);
    }

    #[cfg(not(windows))]
    #[test]
    fn existing_executable_binding_fails_closed_as_unsupported_platform() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tool");
        std::fs::write(&executable, b"trusted").unwrap();
        assert!(matches!(
            CommandSpec::mgba(&executable, "rom.gba"),
            Err(ProcessError::UnsupportedPlatform)
        ));
    }

    #[tokio::test]
    async fn identityless_specs_are_rejected_before_any_spawn() {
        let missing = std::env::temp_dir().join(format!(
            "coop-launcher-missing-{}.exe",
            uuid::Uuid::new_v4().simple()
        ));
        let sidecar = CommandSpec::sidecar(&missing, 1).unwrap();
        let mgba = CommandSpec::mgba(&missing, "rom.gba").unwrap();
        assert!(matches!(
            SupervisedChildren::start(sidecar, mgba, 1).await,
            Err(ProcessError::InvalidArgument)
        ));
    }

    #[tokio::test]
    async fn control_idle_and_fragmented_frames_survive_select_cancellation() {
        use tokio::io::AsyncWriteExt;
        use tokio::sync::oneshot;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (server_can_write_tx, server_can_write_rx) = oneshot::channel();
        let (first_written_tx, first_written_rx) = oneshot::channel();
        let (continue_tx, continue_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            server_can_write_rx.await.unwrap();
            let frame =
                b"{\"type\":\"checkpoint_expired\",\"session_epoch\":1,\"ready_sequence\":2}\n";
            let split = frame.len() / 2;
            stream.write_all(&frame[..split]).await.unwrap();
            first_written_tx.send(()).unwrap();
            continue_rx.await.unwrap();
            stream.write_all(&frame[split..]).await.unwrap();
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut control = ControlChannel::from_stream_for_test(stream);
        // Do not let the producer race the receiver setup; this barrier makes
        // the cancellation point below deterministic on both Windows and
        // Unix.
        server_can_write_tx.send(()).unwrap();
        first_written_rx.await.unwrap();

        // This models a heartbeat winning the supervisor's select while a
        // partial control frame is buffered. The frame must remain pending,
        // rather than being discarded by a canceled receive future.
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), control.receive())
                .await
                .is_err()
        );
        continue_tx.send(()).unwrap();
        assert_eq!(
            control.receive().await.unwrap(),
            coop_sidecar::control::ControlEvent::CheckpointExpired {
                session_epoch: 1,
                ready_sequence: 2,
            }
        );
        server.await.unwrap();
    }

    async fn shutdown_control_pair(
        response: impl FnOnce(coop_sidecar::control::CommandId) -> ControlEvent + Send + 'static,
    ) -> (ControlChannel, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = Vec::new();
            loop {
                let mut byte = [0_u8; 1];
                stream.read_exact(&mut byte).await.unwrap();
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            let command: ControlCommand = serde_json::from_slice(&line).unwrap();
            let ControlCommand::ShutdownRequest(request) = command else {
                panic!("shutdown test must receive the shutdown command")
            };
            let mut bytes = serde_json::to_vec(&response(request.command_id)).unwrap();
            bytes.push(b'\n');
            stream.write_all(&bytes).await.unwrap();
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        (ControlChannel::from_stream_for_test(stream), server)
    }

    async fn shutdown_raw_control_pair(
        response: Option<Vec<u8>>,
        delay: std::time::Duration,
    ) -> (ControlChannel, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = Vec::new();
            loop {
                let mut byte = [0_u8; 1];
                stream.read_exact(&mut byte).await.unwrap();
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            tokio::time::sleep(delay).await;
            if let Some(response) = response {
                let _ = stream.write_all(&response).await;
            }
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        (ControlChannel::from_stream_for_test(stream), server)
    }

    #[tokio::test]
    async fn shutdown_accepts_only_the_correlated_applied_result_and_is_idempotent() {
        let (control, server) = shutdown_control_pair(|command_id| ControlEvent::CommandResult {
            command_id,
            status: CommandStatus::Applied,
            reason: None,
        })
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Graceful);
        assert_eq!(disposition.control, ControlShutdownEvidence::Accepted);
        assert_eq!(disposition.sidecar, RootReapEvidence::Reaped);
        assert_eq!(disposition.mgba, RootReapEvidence::Reaped);
        assert_eq!(
            disposition.job_termination,
            JobTerminationEvidence::Initiated
        );
        assert_eq!(
            disposition.descendant_completion,
            DescendantCompletionEvidence::NotAvailable
        );
        assert_eq!(children.shutdown(1, true).await, disposition);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_wrong_command_result_enters_forced_path_without_retrying_control() {
        let (control, server) = shutdown_control_pair(|_| ControlEvent::CommandResult {
            command_id: new_command_id(),
            status: CommandStatus::Applied,
            reason: None,
        })
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        assert_eq!(disposition.sidecar, RootReapEvidence::Reaped);
        assert_eq!(disposition.mgba, RootReapEvidence::Reaped);
        assert_eq!(children.shutdown(1, true).await, disposition);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_accepts_an_exact_replayed_result() {
        let (control, server) = shutdown_control_pair(|command_id| ControlEvent::CommandResult {
            command_id,
            status: CommandStatus::Replayed,
            reason: None,
        })
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Graceful);
        assert_eq!(disposition.control, ControlShutdownEvidence::Accepted);
        assert!(disposition.clean());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_rejected_result_enters_forced_path() {
        let (control, server) = shutdown_control_pair(|command_id| ControlEvent::CommandResult {
            command_id,
            status: CommandStatus::Rejected,
            reason: Some(coop_sidecar::control::CommandReason::WrongState),
        })
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        assert!(disposition.clean());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_applied_with_a_reason_is_not_an_ack() {
        let (control, server) = shutdown_control_pair(|command_id| ControlEvent::CommandResult {
            command_id,
            status: CommandStatus::Applied,
            reason: Some(coop_sidecar::control::CommandReason::WrongState),
        })
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_malformed_ack_enters_forced_path() {
        let (control, server) =
            shutdown_raw_control_pair(Some(b"not-json\n".to_vec()), Duration::ZERO).await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_oversized_ack_enters_forced_path() {
        let (control, server) = shutdown_raw_control_pair(
            Some(vec![b'x'; MAX_CONTROL_LINE_BYTES + 32]),
            Duration::ZERO,
        )
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_eof_before_ack_enters_forced_path() {
        let (control, server) = shutdown_raw_control_pair(None, Duration::ZERO).await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_ack_timeout_enters_forced_path() {
        let (control, server) = shutdown_raw_control_pair(
            Some(b"{\"type\":\"checkpoint_ready\"}\n".to_vec()),
            Duration::from_millis(700),
        )
        .await;
        let mut children =
            SupervisedChildren::for_test(long_running_child(), long_running_child(), control);
        let disposition = children.shutdown(1, true).await;
        assert_eq!(disposition.path, ShutdownPath::Forced);
        assert_eq!(disposition.control, ControlShutdownEvidence::NotAccepted);
        server.await.unwrap();
    }

    #[test]
    fn drop_cleanup_requires_both_roots_and_job_evidence() {
        let clean = MgbaShutdownReport {
            soft_close: SoftCloseDisposition::NotAttempted,
            root_reap: RootReapEvidence::Reaped,
            job_termination: JobTerminationEvidence::Initiated,
            recovery: RecoveryDisposition::Clean,
        };
        assert!(drop_cleanup_allowed(clean, true));
        assert!(!drop_cleanup_allowed(clean, false));
        assert!(!drop_cleanup_allowed(
            MgbaShutdownReport {
                root_reap: RootReapEvidence::Unknown,
                ..clean
            },
            true
        ));
        assert!(!drop_cleanup_allowed(
            MgbaShutdownReport {
                job_termination: JobTerminationEvidence::Unknown,
                ..clean
            },
            true
        ));
        assert!(!drop_cleanup_allowed(
            MgbaShutdownReport {
                recovery: RecoveryDisposition::Required,
                ..clean
            },
            true
        ));
    }

    #[test]
    fn unmarked_staged_name_is_never_owned_for_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("rom-arbitrary.gba");
        std::fs::write(&rom, b"caller-owned").unwrap();
        let executable = directory.path().join("mgba.exe");
        std::fs::write(&executable, b"placeholder").unwrap();

        #[cfg(windows)]
        {
            let spec = CommandSpec::mgba(&executable, &rom).unwrap();
            assert!(spec.rom_cleanup.is_none());
            drop(spec);
            assert!(rom.exists());
        }
        #[cfg(not(windows))]
        {
            // Existing executable bindings intentionally fail closed on
            // portable hosts before a production process could start.
            assert!(matches!(
                CommandSpec::mgba(&executable, &rom),
                Err(ProcessError::UnsupportedPlatform)
            ));
            assert!(rom.exists());
        }
    }

    #[test]
    fn implicit_save_absence_accepts_only_not_found() {
        let directory = tempfile::tempdir().unwrap();
        let absent = directory.path().join("implicit.sav");
        assert!(ensure_absent(&absent).is_ok());
        std::fs::write(&absent, b"caller-owned").unwrap();
        assert!(matches!(
            ensure_absent(&absent),
            Err(ProcessError::InvalidArgument)
        ));
        std::fs::remove_file(&absent).unwrap();
        std::fs::create_dir(&absent).unwrap();
        assert!(matches!(
            ensure_absent(&absent),
            Err(ProcessError::InvalidArgument)
        ));
    }

    #[tokio::test]
    async fn explicit_cleanup_removes_save_rom_then_marker_after_reap() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("staged.gba");
        let save = directory.path().join("staged.sav");
        let marker = directory.path().join(".staged.gba.owner");
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::write(&save, b"save").unwrap();
        std::fs::write(&marker, b"marker").unwrap();
        let mut children = SupervisedChildren::for_test(
            long_running_child(),
            long_running_child(),
            unused_control_channel().await,
        );
        children.rom_implicit_save_path = Some(save.clone());
        children.rom_cleanup = Some(rom.clone());
        children.rom_marker_cleanup = Some(marker.clone());

        children.stop().await.unwrap();
        assert!(!save.exists());
        assert!(!rom.exists());
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn cleanup_failure_is_typed_and_retains_marker_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("staged.gba");
        let save = directory.path().join("staged.sav");
        let marker = directory.path().join(".staged.gba.owner");
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::create_dir(&save).unwrap();
        std::fs::write(&marker, b"marker").unwrap();
        let mut children = SupervisedChildren::for_test(
            long_running_child(),
            long_running_child(),
            unused_control_channel().await,
        );
        children.rom_implicit_save_path = Some(save.clone());
        children.rom_cleanup = Some(rom.clone());
        children.rom_marker_cleanup = Some(marker.clone());

        let error = children.stop().await.unwrap_err();
        assert!(matches!(
            error,
            ProcessError::Cleanup {
                artifact: CleanupArtifact::ImplicitSave,
                ..
            }
        ));
        // Drop's cancellation fallback must stop at the first failed target;
        // the marker and staged payload remain available for recovery.
        assert!(save.exists());
        assert!(rom.exists());
        assert!(marker.exists());
    }

    #[tokio::test]
    async fn marker_cleanup_failure_is_reported_after_payload_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("staged.gba");
        let save = directory.path().join("staged.sav");
        let marker = directory.path().join(".staged.gba.owner");
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::write(&save, b"save").unwrap();
        std::fs::create_dir(&marker).unwrap();
        let mut children = SupervisedChildren::for_test(
            long_running_child(),
            long_running_child(),
            unused_control_channel().await,
        );
        children.rom_implicit_save_path = Some(save.clone());
        children.rom_cleanup = Some(rom.clone());
        children.rom_marker_cleanup = Some(marker.clone());

        let error = children.stop().await.unwrap_err();
        assert!(matches!(
            error,
            ProcessError::Cleanup {
                artifact: CleanupArtifact::Marker,
                ..
            }
        ));
        assert!(!save.exists());
        assert!(!rom.exists());
        assert!(marker.exists());
    }

    #[cfg(windows)]
    #[test]
    fn owned_mgba_argv_pins_implicit_save_and_disables_all_automatic_loads() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let marker = staged_rom_marker_path(&rom);
        fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
        let spec = CommandSpec::mgba_owned_staged(&executable, &rom, &marker).unwrap();
        let canonical_rom = rom.canonicalize().unwrap();
        let canonical_parent = canonical_rom.parent().unwrap();
        assert_eq!(
            spec.args,
            vec![
                "-C".to_owned(),
                format!("savegamePath={}", canonical_parent.to_string_lossy()),
                "-C".to_owned(),
                "autoload=0".to_owned(),
                "-C".to_owned(),
                "autosave=0".to_owned(),
                "-C".to_owned(),
                "cheatAutoload=0".to_owned(),
                "-C".to_owned(),
                "cheatAutosave=0".to_owned(),
                "-p".to_owned(),
                "NUL".to_owned(),
                canonical_rom.to_string_lossy().into_owned(),
            ]
        );
        assert_eq!(
            spec.rom_implicit_save_path.as_deref(),
            Some(rom.with_extension("sav").as_path())
        );
        drop(spec);
    }

    #[cfg(windows)]
    #[test]
    fn owned_argv_rejects_mutation_of_any_bound_field() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let marker = staged_rom_marker_path(&rom);
        fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();

        let mut spec = CommandSpec::mgba_owned_staged(&executable, &rom, &marker).unwrap();
        let expected_rom = spec.rom_cleanup.clone();
        let expected_marker = spec.rom_marker_cleanup.clone();
        let expected_save = spec.rom_implicit_save_path.clone();
        let expected_rom_identity = spec.rom_identity.clone();
        let expected_marker_identity = spec.rom_marker_identity.clone();

        spec.rom_cleanup = Some(directory.path().join("other.gba"));
        assert!(!spec.has_supported_argv());
        spec.rom_cleanup = expected_rom;
        spec.rom_marker_cleanup = Some(directory.path().join("other.owner"));
        assert!(!spec.has_supported_argv());
        spec.rom_marker_cleanup = expected_marker;
        spec.rom_implicit_save_path = Some(directory.path().join("other.sav"));
        assert!(!spec.has_supported_argv());
        spec.rom_implicit_save_path = expected_save;
        let mut wrong_rom_identity = expected_rom_identity.clone().unwrap();
        wrong_rom_identity.digest[0] ^= 0xff;
        spec.rom_identity = Some(wrong_rom_identity);
        assert!(!spec.has_supported_argv());
        spec.rom_identity = expected_rom_identity;
        let mut wrong_marker_identity = expected_marker_identity.clone().unwrap();
        wrong_marker_identity.digest[0] ^= 0xff;
        spec.rom_marker_identity = Some(wrong_marker_identity);
        assert!(!spec.has_supported_argv());
        spec.rom_marker_identity = expected_marker_identity;

        spec.args[1] = format!(
            "savegamePath={}",
            directory.path().join("other").to_string_lossy()
        );
        assert!(!spec.has_supported_argv());
        spec.args[1] = format!("savegamePath={}", directory.path().to_string_lossy());
        spec.args[OWNED_MGBA_ROM_ARG_INDEX] = directory
            .path()
            .join("other.gba")
            .to_string_lossy()
            .into_owned();
        assert!(!spec.has_supported_argv());
    }

    #[cfg(windows)]
    #[test]
    fn staged_rom_rejects_auto_discovered_patch_and_cheat_siblings() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let marker = staged_rom_marker_path(&rom);
        fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
        for extension in ["ips", "ups", "bps", "cheats"] {
            fs::write(rom.with_extension(extension), b"unbound input").unwrap();
            assert!(matches!(
                CommandSpec::mgba_owned_staged(&executable, &rom, &marker),
                Err(ProcessError::InvalidArgument)
            ));
            fs::remove_file(rom.with_extension(extension)).unwrap();
        }
        assert!(rom.exists());
        assert!(marker.exists());
    }

    #[cfg(windows)]
    #[test]
    fn cleanup_rejects_a_replaced_rom_and_retains_the_marker() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let marker = staged_rom_marker_path(&rom);
        fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
        let mut spec = CommandSpec::mgba_owned_staged(&executable, &rom, &marker).unwrap();
        // The production guard intentionally denies DELETE so official mGBA's
        // read-only CRT open remains compatible. Simulate a hostile replacement
        // only after the guard is explicitly lost; cleanup must retain evidence.
        let _ = spec.rom_guards.take();
        fs::remove_file(&rom).unwrap();
        fs::write(&rom, b"foreign replacement").unwrap();

        drop(spec);
        assert!(rom.exists());
        assert!(marker.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn startup_rejection_explicitly_cleans_staged_rom_and_marker() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("mgba.exe");
        let rom = directory.path().join("rom.gba");
        fs::write(&executable, b"trusted emulator").unwrap();
        fs::write(&rom, b"trusted rom").unwrap();
        let marker = staged_rom_marker_path(&rom);
        fs::write(&marker, staged_rom_marker_contents(&rom).unwrap()).unwrap();
        let mgba = CommandSpec::mgba_owned_staged(&executable, &rom, &marker).unwrap();
        let missing_sidecar = directory.path().join("sidecar.exe");
        let sidecar = CommandSpec::sidecar(&missing_sidecar, 1).unwrap();

        let result = SupervisedChildren::start(sidecar, mgba, 1).await;
        assert!(
            matches!(result, Err(ProcessError::InvalidArgument)),
            "{result:?}"
        );
        assert!(!rom.exists());
        assert!(!marker.exists());
        assert!(!rom.with_extension("sav").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn invalid_mgba_identity_is_rejected_before_sidecar_spawn() {
        let directory = tempdir().unwrap();
        let mgba_path = std::env::current_exe().unwrap();
        let rom = directory.path().join("rom.gba");
        fs::write(&rom, b"trusted rom").unwrap();
        let mut mgba = CommandSpec::mgba(&mgba_path, &rom).unwrap();
        mgba.identity.as_mut().unwrap().digest[0] ^= 0xff;

        let sidecar = CommandSpec::sidecar(&mgba_path, 1).unwrap();
        assert!(matches!(
            SupervisedChildren::start(sidecar, mgba, 1).await,
            Err(ProcessError::MgbaIdentity)
        ));
    }

    #[tokio::test]
    async fn control_failure_reaps_both_supervised_children() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let control = ControlChannel::from_stream_for_test(stream);
        let sidecar = long_running_child();
        let mgba = long_running_child();
        let mut children = SupervisedChildren::for_test(sidecar, mgba, control);
        assert!(matches!(
            children.next_event().await,
            Err(ProcessError::Descriptor)
        ));
        assert!(children.sidecar.try_wait().unwrap().is_some());
        assert!(children.mgba.try_wait().unwrap().is_some());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn peer_exit_is_reported_and_the_other_child_is_reaped() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("staged.gba");
        let save = directory.path().join("staged.sav");
        let marker = directory.path().join(".staged.gba.owner");
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::write(&save, b"save").unwrap();
        std::fs::write(&marker, b"marker").unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let control = ControlChannel::from_stream_for_test(stream);
        #[cfg(windows)]
        let mut exited = {
            let mut command = Command::new("cmd.exe");
            command.args(["/C", "exit", "0"]);
            command
        };
        #[cfg(not(windows))]
        let mut exited = {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            command
        };
        let sidecar = exited
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut children = SupervisedChildren::for_test(sidecar, long_running_child(), control);
        children.rom_implicit_save_path = Some(save.clone());
        children.rom_cleanup = Some(rom.clone());
        children.rom_marker_cleanup = Some(marker.clone());
        assert!(matches!(
            children.next_event().await.unwrap(),
            SupervisorEvent::ChildExited
        ));
        assert!(!save.exists());
        assert!(!rom.exists());
        assert!(!marker.exists());
        server.abort();
    }

    #[tokio::test]
    async fn child_exit_cleanup_failure_is_returned_and_retains_recovery_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let rom = directory.path().join("staged.gba");
        let save = directory.path().join("staged.sav");
        let marker = directory.path().join(".staged.gba.owner");
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::create_dir(&save).unwrap();
        std::fs::write(&marker, b"marker").unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let control = ControlChannel::from_stream_for_test(stream);
        #[cfg(windows)]
        let mut exited = {
            let mut command = Command::new("cmd.exe");
            command.args(["/C", "exit", "0"]);
            command
        };
        #[cfg(not(windows))]
        let mut exited = {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            command
        };
        let sidecar = exited
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut children = SupervisedChildren::for_test(sidecar, long_running_child(), control);
        children.rom_implicit_save_path = Some(save.clone());
        children.rom_cleanup = Some(rom.clone());
        children.rom_marker_cleanup = Some(marker.clone());

        assert!(matches!(
            children.next_event().await,
            Err(ProcessError::Cleanup {
                artifact: CleanupArtifact::ImplicitSave,
                ..
            })
        ));
        assert!(children.sidecar.try_wait().unwrap().is_some());
        assert!(children.mgba.try_wait().unwrap().is_some());
        assert!(save.exists());
        assert!(rom.exists());
        assert!(marker.exists());
        server.abort();
    }
}

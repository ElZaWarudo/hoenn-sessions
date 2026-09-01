//! Argument-vector-only process supervision and sidecar control connection.

use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::io::{Seek, SeekFrom};
#[cfg(windows)]
use std::sync::Arc;

use coop_cloud::{BridgeAbiVersion, ProtocolVersion};
use coop_sidecar::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, GAME_PROTOCOL_VERSION, MAX_DESCRIPTOR_BYTES,
    SessionDescriptor,
    control::{CONTROL_PROTOCOL_VERSION, ControlCommand, ControlEvent, MAX_CONTROL_LINE_BYTES},
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

pub const PROCESS_IO_TIMEOUT: Duration = Duration::from_secs(5);
const DROP_REAP_POLL: Duration = Duration::from_millis(10);
const STAGED_ROM_MARKER_PREFIX: &[u8] = b"pokecrossroads-coop-staged-rom-v1\n";
const MAX_STAGED_ROM_MARKER_BYTES: u64 = 4096;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process path or argument is invalid")]
    InvalidArgument,
    #[cfg(not(windows))]
    #[error("secure executable binding is unavailable on this platform")]
    UnsupportedPlatform,
    #[error("child process failed to start")]
    Spawn(#[source] io::Error),
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
    identity: Option<ExecutableIdentity>,
    rom_identity: Option<ExecutableIdentity>,
    rom_cleanup: Option<PathBuf>,
    rom_marker_cleanup: Option<PathBuf>,
    #[cfg(windows)]
    executable_guards: Option<ExecutableGuards>,
    #[cfg(windows)]
    rom_guards: Option<ExecutableGuards>,
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
            identity,
            rom_identity: None,
            rom_cleanup: None,
            rom_marker_cleanup: None,
            #[cfg(windows)]
            executable_guards,
            #[cfg(windows)]
            rom_guards: None,
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
        let (rom_identity, rom_guards) = executable_binding(&rom_path)?;
        #[cfg(not(windows))]
        let _ = rom_guards;
        spec.rom_identity = rom_identity;
        #[cfg(windows)]
        {
            spec.rom_guards = rom_guards;
        }
        Ok(spec)
    }

    /// Binds cleanup ownership to the explicit marker emitted by the
    /// launcher's ROM staging routine. `mgba` deliberately does not infer
    /// ownership from a filename or directory, so an arbitrary caller-owned
    /// `rom-*.gba` is never removed as a side effect of supervision.
    ///
    /// The marker is checked before any child starts and is retained until the
    /// emulator has been reaped. The launcher writes the marker atomically
    /// next to its staged ROM and includes the canonical ROM path in it.
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
        let mut spec = Self::mgba(executable, &rom_path)?;
        spec.rom_cleanup = Some(rom_path);
        spec.rom_marker_cleanup = Some(marker_path);
        Ok(spec)
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
/// Callers should write this file atomically beside the ROM before constructing
/// [`mgba_owned_staged`].
///
/// # Errors
///
/// Returns an error when the ROM path cannot be canonicalized.
pub fn staged_rom_marker_contents(rom: impl AsRef<Path>) -> Result<Vec<u8>, ProcessError> {
    let canonical = fs::canonicalize(rom.as_ref()).map_err(|_| ProcessError::InvalidArgument)?;
    let mut marker = STAGED_ROM_MARKER_PREFIX.to_vec();
    marker.extend_from_slice(canonical.to_string_lossy().as_bytes());
    marker.push(b'\n');
    if marker.len() as u64 > MAX_STAGED_ROM_MARKER_BYTES {
        return Err(ProcessError::InvalidArgument);
    }
    Ok(marker)
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
    let Ok(file) = fs::File::open(marker) else {
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
    let Ok(expected) = staged_rom_marker_contents(rom) else {
        return false;
    };
    contents == expected
}

impl Drop for CommandSpec {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = self.rom_guards.take();
        if let Some(path) = self.rom_marker_cleanup.take() {
            let _ = fs::remove_file(path);
        }
        if let Some(path) = self.rom_cleanup.take() {
            let _ = fs::remove_file(path);
        }
    }
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = formatter.debug_struct("CommandSpec");
        debug
            .field("executable", &self.executable)
            .field("args", &self.args)
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
        #[cfg(windows)]
        debug.field(
            "executable_guards",
            &self.executable_guards.as_ref().map(|_| "[HELD]"),
        );
        #[cfg(windows)]
        debug.field("rom_guards", &self.rom_guards.as_ref().map(|_| "[HELD]"));
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

pub struct SupervisedChildren {
    sidecar: Child,
    mgba: Child,
    pub control: ControlChannel,
    rom_cleanup: Option<PathBuf>,
    rom_marker_cleanup: Option<PathBuf>,
    #[cfg(windows)]
    rom_guards: Option<ExecutableGuards>,
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

impl SupervisedChildren {
    #[cfg(test)]
    pub(crate) fn for_test(sidecar: Child, mgba: Child, control: ControlChannel) -> Self {
        Self {
            sidecar,
            mgba,
            control,
            rom_cleanup: None,
            rom_marker_cleanup: None,
            #[cfg(windows)]
            rom_guards: None,
        }
    }

    /// Starts sidecar, authenticates control, then starts stock mGBA with only
    /// the validated ROM path. Stock mGBA 0.10.5 is intentionally not given
    /// an invented --script argument.
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
        if expected_epoch == 0
            || sidecar.args != ["--session-epoch", &expected_epoch.to_string()]
            || mgba.args.len() != 1
            || mgba.args[0].is_empty()
            || mgba.args[0].contains('\0')
            || !Path::new(&mgba.args[0]).is_absolute()
            || !sidecar.executable.is_absolute()
            || !mgba.executable.is_absolute()
            || sidecar.identity.is_none()
            || mgba.identity.is_none()
            || mgba.rom_identity.is_none()
        {
            return Err(ProcessError::InvalidArgument);
        }
        validate_executable_identity(&sidecar)?;
        // Validate both executable bindings before any child is spawned. A
        // later mGBA identity failure must never strand a live sidecar while
        // descriptor/control startup is in progress.
        validate_executable_identity(&mgba)?;
        let mut sidecar_command = Command::new(&sidecar.executable);
        isolate_environment(&mut sidecar_command);
        sidecar_command
            .args(&sidecar.args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let child = sidecar_command.spawn().map_err(ProcessError::Spawn)?;
        let mut startup_child = StartupChildGuard::new(child);
        let Some(stdout) = startup_child.child_mut().stdout.take() else {
            return Err(startup_failure(ProcessError::Descriptor, startup_child.child_mut()).await);
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
                return Err(startup_failure(error, startup_child.child_mut()).await);
            }
        };
        let descriptor: SessionDescriptor = if let Ok(value) = serde_json::from_slice(&line) {
            value
        } else {
            return Err(startup_failure(ProcessError::Descriptor, startup_child.child_mut()).await);
        };
        if let Err(error) = validate_descriptor(&descriptor, expected_epoch) {
            return Err(startup_failure(error, startup_child.child_mut()).await);
        }
        let control = match ControlChannel::connect(&descriptor).await {
            Ok(control) => control,
            Err(error) => {
                return Err(startup_failure(error, startup_child.child_mut()).await);
            }
        };
        if let Some((workspace, bridge_source)) = bridge
            && let Err(error) =
                materialize_bridge_session(workspace, bridge_source, &descriptor, expected_epoch)
        {
            return Err(startup_failure(error, startup_child.child_mut()).await);
        }
        let mut mgba = mgba;
        let mut mgba_command = Command::new(&mgba.executable);
        isolate_environment(&mut mgba_command);
        mgba_command
            .args(&mgba.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mgba_child = match mgba_command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Err(
                    startup_failure(ProcessError::Spawn(error), startup_child.child_mut()).await,
                );
            }
        };
        // Keep cleanup ownership and Windows deny-delete handles in the
        // supervisor until mGBA has been reaped.  On spawn failure these stay
        // in `mgba`, so CommandSpec::drop removes only explicitly owned ROMs.
        let rom_cleanup = mgba.rom_cleanup.take();
        let rom_marker_cleanup = mgba.rom_marker_cleanup.take();
        #[cfg(windows)]
        let rom_guards = mgba.rom_guards.take();
        Ok(Self {
            sidecar: startup_child.into_child(),
            mgba: mgba_child,
            control,
            rom_cleanup,
            rom_marker_cleanup,
            #[cfg(windows)]
            rom_guards,
        })
    }

    /// Stops and reaps both children with a bounded wait.
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
        let mgba_result = stop_child(&mut self.mgba).await;
        let sidecar_result = stop_child(&mut self.sidecar).await;
        mgba_result.and(sidecar_result)
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
            result = self.sidecar.wait() => (result, &mut self.mgba),
            result = self.mgba.wait() => (result, &mut self.sidecar),
        };
        let first = first.map_err(ProcessError::Termination).and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(ProcessError::ChildExited)
            }
        });
        let peer = stop_child(peer).await;
        first.and(peer)
    }

    /// Waits for one control event or for either child to exit.  The method
    /// owns the race so a caller can schedule lease heartbeats and checkpoint
    /// work without ever leaving a peer process orphaned.
    ///
    /// # Errors
    ///
    /// Returns an error when control I/O or child reaping fails.  Any control
    /// or protocol error first stops and reaps both children; if that proof
    /// fails, the returned [`ProcessError::EventCleanup`] preserves both
    /// failures for lease-release gating.
    pub async fn next_event(&mut self) -> Result<SupervisorEvent, ProcessError> {
        let result = tokio::select! {
            result = self.control.receive() => result.map(SupervisorEvent::Control),
            result = self.sidecar.wait() => {
                match result {
                    Err(error) => Err(ProcessError::Termination(error)),
                    Ok(status) => match stop_child(&mut self.mgba).await {
                        Err(error) => Err(error),
                        Ok(()) if status.success() => Ok(SupervisorEvent::ChildExited),
                        Ok(()) => Err(ProcessError::ChildExited),
                    },
                }
            }
            result = self.mgba.wait() => {
                match result {
                    Err(error) => Err(ProcessError::Termination(error)),
                    Ok(status) => match stop_child(&mut self.sidecar).await {
                        Err(error) => Err(error),
                        Ok(()) if status.success() => Ok(SupervisorEvent::ChildExited),
                        Ok(()) => Err(ProcessError::ChildExited),
                    },
                }
            }
        };
        match result {
            Ok(event) => Ok(event),
            Err(error) => Err(self.fail_closed(error).await),
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
}

#[cfg(windows)]
fn executable_binding(
    path: &Path,
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
    let ancestor_guards =
        open_directory_ancestor_guards(parent).map_err(|_| ProcessError::InvalidArgument)?;
    let Some(file) = open_executable_file(path)? else {
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
    let digest = hash_file_handle(&file_guard).map_err(|_| ProcessError::InvalidArgument)?;
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

#[cfg(windows)]
fn open_executable_file(path: &Path) -> Result<Option<fs::File>, ProcessError> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(0x0000_0001)
        .custom_flags(0x0020_0000);
    match options.open(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ProcessError::InvalidArgument),
    }
}

#[cfg(windows)]
fn open_directory_guard(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(0x0000_0003)
        .custom_flags(0x0220_0000);
    options.open(path)
}

#[cfg(windows)]
fn open_directory_ancestor_guards(path: &Path) -> io::Result<Vec<Arc<fs::File>>> {
    let mut guards = Vec::new();
    let mut current = path;
    loop {
        guards.push(Arc::new(open_directory_guard(current)?));
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

#[cfg(windows)]
fn hash_file_handle(handle: &fs::File) -> io::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut file = handle.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

fn validate_executable_identity(spec: &CommandSpec) -> Result<(), ProcessError> {
    #[cfg(windows)]
    if let Some(guards) = &spec.executable_guards {
        // Reading metadata through both held handles keeps the guards live
        // through this immediate validation-to-spawn boundary and documents
        // that they are security-critical, not incidental storage.
        let _ = guards.file.metadata();
        for ancestor in &guards.ancestors {
            let _ = ancestor.metadata();
        }
    }
    if let Some(expected) = &spec.identity {
        let (actual, _) = executable_binding(&spec.executable)?;
        let Some(actual) = actual else {
            return Err(ProcessError::InvalidArgument);
        };
        if &actual != expected {
            return Err(ProcessError::InvalidArgument);
        }
    }
    if spec.args.len() == 1 {
        let rom = Path::new(&spec.args[0]);
        if let Some(expected_rom) = &spec.rom_identity {
            let Some(actual_rom) = executable_binding(rom)?.0 else {
                return Err(ProcessError::InvalidArgument);
            };
            if &actual_rom != expected_rom {
                return Err(ProcessError::InvalidArgument);
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

impl Drop for SupervisedChildren {
    fn drop(&mut self) {
        // Drop cannot await.  It still owns cancellation supervision: request
        // termination and synchronously reap both children within a bounded
        // deadline so a cancelled launcher future cannot orphan processes.
        terminate_and_reap_sync(&mut self.mgba);
        terminate_and_reap_sync(&mut self.sidecar);
        self.remove_owned_rom();
    }
}

impl SupervisedChildren {
    fn remove_owned_rom(&mut self) {
        #[cfg(windows)]
        drop(self.rom_guards.take());
        if let Some(path) = self.rom_cleanup.take() {
            let _ = fs::remove_file(path);
        }
        if let Some(path) = self.rom_marker_cleanup.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn terminate_and_reap_sync(child: &mut Child) {
    let already_exited = child.try_wait().ok().flatten().is_some();
    if !already_exited {
        let _ = child.start_kill();
    }
    let deadline = Instant::now() + PROCESS_IO_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if Instant::now() >= deadline => break,
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

async fn stop_child(child: &mut Child) -> Result<(), ProcessError> {
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
        CommandSpec, ControlChannel, PROCESS_IO_TIMEOUT, ProcessError, SupervisedChildren,
        SupervisorEvent, terminate_and_reap_sync,
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
    #[cfg(windows)]
    use std::fs;
    use std::process::Stdio;
    #[cfg(windows)]
    use tempfile::tempdir;
    use tokio::{net::TcpListener, process::Command};

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
        let spec = CommandSpec::mgba(&executable, "rom.gba").unwrap();
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
            Err(ProcessError::InvalidArgument)
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
        assert!(matches!(
            children.next_event().await.unwrap(),
            SupervisorEvent::ChildExited
        ));
        server.abort();
    }
}

use std::{
    collections::HashMap,
    fmt, io,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, sleep_until, timeout},
};
use uuid::Uuid;

use crate::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BridgeFrame, Direction, FrameCodecError,
    GAME_PROTOCOL_VERSION, MessageType,
    control::{
        CONTROL_PROTOCOL_VERSION, CheckpointAbort, CheckpointGrant, CheckpointKey, CommandId,
        CommandReason, CommandStatus, ControlCommand, ControlConnection, ControlError,
        ControlEvent, ControlListener, ControlWriter, MAX_CONTROL_LINE_BYTES,
    },
};

/// Includes the terminating LF. A client cannot make the sidecar buffer more than this.
pub const MAX_HANDSHAKE_BYTES: usize = 256;
pub const HANDSHAKE_ACCEPTED_LINE: &[u8] = b"{\"ok\":true}\n";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const BOOTSTRAP_SEQUENCE: u32 = 1;
const DECISION_TIMEOUT: Duration = Duration::from_secs(3);
const SAVE_DATA_TIMEOUT: Duration = Duration::from_secs(3);
const BRIDGE_FRAME_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_DESCRIPTOR_BYTES: usize = 512;
const MAX_COMMAND_HISTORY: usize = 1024;

#[derive(Clone)]
struct SessionSecret(String);

impl SessionSecret {
    fn generate() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn matches(&self, candidate: &str) -> bool {
        if candidate.len() != 32
            || !candidate
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return false;
        }

        let mut difference = 0_u8;
        for (expected, received) in self.0.bytes().zip(candidate.bytes()) {
            difference |= expected ^ received;
        }
        difference == 0
    }
}

impl fmt::Debug for SessionSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionSecret([REDACTED])")
    }
}

/// Metadata exposed to Lua. It contains only bridge credentials, never control credentials.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDescriptor {
    transport: String,
    host: String,
    port: u16,
    secret: String,
    bridge_abi: u16,
    protocol_version: u16,
    frame_bytes: usize,
}

impl BridgeDescriptor {
    #[must_use]
    pub fn transport(&self) -> &str {
        &self.transport
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn bridge_abi(&self) -> u16 {
        self.bridge_abi
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    #[must_use]
    pub const fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for BridgeDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeDescriptor")
            .field("transport", &self.transport)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("secret", &"[REDACTED]")
            .field("bridge_abi", &self.bridge_abi)
            .field("protocol_version", &self.protocol_version)
            .field("frame_bytes", &self.frame_bytes)
            .finish()
    }
}

/// Metadata used by the launcher to authenticate its control connection.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlDescriptor {
    transport: String,
    host: String,
    port: u16,
    secret: String,
    control_version: u16,
    max_line_bytes: usize,
}

impl ControlDescriptor {
    #[must_use]
    pub fn transport(&self) -> &str {
        &self.transport
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn control_version(&self) -> u16 {
        self.control_version
    }

    #[must_use]
    pub const fn max_line_bytes(&self) -> usize {
        self.max_line_bytes
    }

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for ControlDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlDescriptor")
            .field("transport", &self.transport)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("secret", &"[REDACTED]")
            .field("control_version", &self.control_version)
            .field("max_line_bytes", &self.max_line_bytes)
            .finish()
    }
}

/// Printed once by the CLI after both listeners have successfully bound.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDescriptor {
    version: u16,
    session_epoch: u32,
    bridge: BridgeDescriptor,
    control: ControlDescriptor,
}

impl fmt::Debug for SessionDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionDescriptor")
            .field("version", &self.version)
            .field("session_epoch", &self.session_epoch)
            .field("bridge", &self.bridge)
            .field("control", &self.control)
            .finish()
    }
}

impl SessionDescriptor {
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    #[must_use]
    pub const fn session_epoch(&self) -> u32 {
        self.session_epoch
    }

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.bridge.address()
    }

    #[must_use]
    pub fn secret(&self) -> &str {
        self.bridge.secret()
    }

    #[must_use]
    pub fn bridge(&self) -> &BridgeDescriptor {
        &self.bridge
    }

    #[must_use]
    pub fn control(&self) -> &ControlDescriptor {
        &self.control
    }

    #[must_use]
    pub fn control_address(&self) -> SocketAddr {
        self.control.address()
    }

    #[must_use]
    pub fn control_secret(&self) -> &str {
        self.control.secret()
    }

    /// Serializes the one-line launcher descriptor and enforces its hard bound.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails or the descriptor exceeds the
    /// bounded launcher discovery line.
    pub fn to_bounded_json_line(&self) -> Result<Vec<u8>, SidecarError> {
        let mut line = serde_json::to_vec(self).map_err(|error| {
            SidecarError::DescriptorSerialization(io::Error::new(io::ErrorKind::InvalidData, error))
        })?;
        line.push(b'\n');
        if line.len() > MAX_DESCRIPTOR_BYTES {
            return Err(SidecarError::DescriptorTooLarge { actual: line.len() });
        }
        Ok(line)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandshakeRequest {
    secret: String,
    bridge_abi: u16,
    protocol_version: u16,
}

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("failed to bind or accept on the loopback listener")]
    Listener(#[source] io::Error),
    #[error("loopback client I/O failed")]
    Connection(#[source] io::Error),
    #[error("bridge connection failed before a frame")]
    BridgeConnection(#[source] io::Error),
    #[error("bridge frame write failed")]
    BridgeWriteConnection(#[source] io::Error),
    #[error("bridge frame write timed out")]
    BridgeWriteTimeout,
    #[error("rejected non-loopback peer {0}")]
    NonLoopbackPeer(SocketAddr),
    #[error("handshake timed out")]
    HandshakeTimeout,
    #[error("bridge handshake response write timed out")]
    HandshakeWriteTimeout,
    #[error("handshake exceeds the {MAX_HANDSHAKE_BYTES}-byte limit")]
    HandshakeTooLarge,
    #[error("connection closed before a complete handshake")]
    HandshakeClosed,
    #[error("handshake is not the required JSON object")]
    MalformedHandshake(#[source] serde_json::Error),
    #[error("handshake authentication failed")]
    AuthenticationFailed,
    #[error("bridge ABI {received} is incompatible with required ABI {BRIDGE_ABI_VERSION}")]
    IncompatibleBridgeAbi { received: u16 },
    #[error(
        "game protocol {received} is incompatible with required protocol {GAME_PROTOCOL_VERSION}"
    )]
    IncompatibleProtocolVersion { received: u16 },
    #[error("session epoch zero is reserved")]
    SessionEpochZero,
    #[error("bridge frame was rejected")]
    Frame(#[from] FrameCodecError),
    #[error("control channel failed")]
    Control(#[from] ControlError),
    #[error("checkpoint input violated the authenticated state machine: {0}")]
    ProtocolViolation(&'static str),
    #[error("descriptor serialization failed")]
    DescriptorSerialization(#[source] io::Error),
    #[error("descriptor exceeds the {MAX_DESCRIPTOR_BYTES}-byte limit: {actual}")]
    DescriptorTooLarge { actual: usize },
    #[error("checkpoint save-data handoff timed out")]
    CheckpointTimeout,
    #[error("checkpoint command replay ledger reached its bounded capacity")]
    CommandLedgerFull,
}

/// The authenticated, frame-oriented view of a local TCP connection.
pub(crate) struct AuthenticatedConnection {
    stream: TcpStream,
}

impl AuthenticatedConnection {
    pub(crate) fn into_split(self) -> (BridgeReader, BridgeWriter) {
        let (reader, writer) = self.stream.into_split();
        (
            BridgeReader { stream: reader },
            BridgeWriter { stream: writer },
        )
    }
}

pub(crate) struct BridgeReader {
    stream: OwnedReadHalf,
}

impl BridgeReader {
    async fn receive(
        &mut self,
        expected_direction: Direction,
    ) -> Result<Option<BridgeFrame>, SidecarError> {
        // An authenticated bridge is allowed to remain idle while the
        // control plane decides a checkpoint.  Only a frame that has started
        // must make progress within the bounded I/O window; timing out the
        // first byte would tear down an otherwise healthy idle session.
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        let first_byte_count = self
            .stream
            .read(&mut bytes[..1])
            .await
            // A reset while waiting for the next frame is an idle bridge
            // disconnect and may reconnect before a grant. Once the first
            // byte has arrived, errors remain fatal: the authenticated frame
            // may have been partially received.
            .map_err(SidecarError::BridgeConnection)?;
        if first_byte_count == 0 {
            return Ok(None);
        }
        let frame = timeout(BRIDGE_FRAME_TIMEOUT, async {
            self.stream
                .read_exact(&mut bytes[1..])
                .await
                .map_err(SidecarError::Connection)?;
            BridgeFrame::decode_for(&bytes, expected_direction).map_err(SidecarError::from)
        })
        .await
        .map_err(|_| {
            SidecarError::Connection(io::Error::new(
                io::ErrorKind::TimedOut,
                "bridge frame read timed out",
            ))
        })??;
        Ok(Some(frame))
    }
}

pub(crate) struct BridgeWriter {
    stream: OwnedWriteHalf,
}

impl BridgeWriter {
    async fn send(
        &mut self,
        frame: &BridgeFrame,
        expected_direction: Direction,
    ) -> Result<(), SidecarError> {
        frame.ensure_direction(expected_direction)?;
        let bytes = frame.encode();
        timeout(BRIDGE_FRAME_TIMEOUT, self.stream.write_all(&bytes))
            .await
            .map_err(|_| SidecarError::BridgeWriteTimeout)?
            .map_err(SidecarError::BridgeWriteConnection)
    }
}

#[derive(Debug)]
struct SessionSequenceState {
    next_sidecar: u32,
    last_boot_rom: u32,
    last_session_rom: u32,
}

impl Default for SessionSequenceState {
    fn default() -> Self {
        Self {
            next_sidecar: BOOTSTRAP_SEQUENCE,
            last_boot_rom: 0,
            last_session_rom: 0,
        }
    }
}

impl SessionSequenceState {
    fn take_sidecar_sequence(&mut self) -> u32 {
        let sequence = self.next_sidecar;
        self.next_sidecar = sequence.wrapping_add(1);
        if self.next_sidecar == 0 {
            self.next_sidecar = 1;
        }
        sequence
    }

    fn inspect_rom_frame(&self, frame: &BridgeFrame, session_epoch: u32) -> bool {
        let previous = if frame.session_epoch() == 0 {
            if frame.message_type() != MessageType::RomReady {
                return false;
            }
            self.last_boot_rom
        } else if frame.session_epoch() == session_epoch {
            self.last_session_rom
        } else {
            return false;
        };
        is_sequence_newer(frame.sequence(), previous)
    }

    fn commit_rom_frame(&mut self, frame: &BridgeFrame) {
        if frame.session_epoch() == 0 {
            self.last_boot_rom = frame.sequence();
        } else {
            self.last_session_rom = frame.sequence();
        }
    }

    fn rearm_session_after_rom_reboot(&mut self, sequence: u32) {
        self.last_session_rom = sequence;
    }

    fn rearm_boot_after_bridge_reconnect(&mut self) {
        self.last_boot_rom = 0;
    }

    fn commit_checkpoint_ready(&mut self, key: CheckpointKey) {
        if is_sequence_newer(key.ready_sequence, self.last_session_rom) {
            self.last_session_rom = key.ready_sequence;
        }
    }

    fn accept_rom_frame(&mut self, frame: &BridgeFrame, session_epoch: u32) -> bool {
        if !self.inspect_rom_frame(frame, session_epoch) {
            return false;
        }
        self.commit_rom_frame(frame);
        true
    }
}

/// Owns the decoder tasks for an authenticated bridge session. The bridge
/// decoder intentionally outlives a reconnectable control loss so a frame in
/// flight cannot be discarded between socket read and channel enqueue. The
/// owner aborts every task on every drop path, including cancellation of the
/// serving future; normal shutdown additionally awaits them.
struct ReaderTasks {
    bridge: JoinHandle<()>,
    control: Option<JoinHandle<()>>,
}

impl ReaderTasks {
    fn new_bridge(bridge: JoinHandle<()>) -> Self {
        Self {
            bridge,
            control: None,
        }
    }

    fn set_control(&mut self, control: JoinHandle<()>) {
        debug_assert!(self.control.is_none());
        self.control = Some(control);
    }

    async fn shutdown_control(&mut self) {
        if let Some(handle) = self.control.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    async fn shutdown(&mut self) {
        self.shutdown_control().await;
        self.bridge.abort();
        let _ = (&mut self.bridge).await;
    }

    #[cfg(test)]
    fn new(bridge: JoinHandle<()>, control: JoinHandle<()>) -> Self {
        let mut tasks = Self::new_bridge(bridge);
        tasks.set_control(control);
        tasks
    }
}

impl Drop for ReaderTasks {
    fn drop(&mut self) {
        self.bridge.abort();
        if let Some(handle) = &self.control {
            handle.abort();
        }
    }
}

fn spawn_control_reader(
    control: ControlConnection,
) -> (
    ControlWriter,
    mpsc::Receiver<Result<ControlCommand, SidecarError>>,
    JoinHandle<()>,
) {
    let (mut control_reader, control_writer) = control.into_split();
    let (control_tx, control_rx) = mpsc::channel::<Result<ControlCommand, SidecarError>>(1);
    let control_task = tokio::spawn(async move {
        loop {
            match control_reader.receive_command().await {
                Ok(command) => {
                    if control_tx.send(Ok(command)).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = control_tx.send(Err(SidecarError::Control(error))).await;
                    break;
                }
            }
        }
    });
    (control_writer, control_rx, control_task)
}

fn is_sequence_newer(sequence: u32, previous: u32) -> bool {
    sequence != 0 && (previous == 0 || sequence.wrapping_sub(previous).cast_signed() > 0)
}

fn rotate_expired_tombstone(frame: &BridgeFrame, expired_checkpoint: &mut Option<CheckpointKey>) {
    if let Some(expired) = *expired_checkpoint
        && frame.session_epoch() == expired.session_epoch
        && is_sequence_newer(frame.sequence(), expired.ready_sequence)
    {
        // TCP ordering means every frame observed after the retired key
        // belongs to the current generation. Once a later serial is
        // committed, the old tombstone can no longer be needed to reject
        // that key. This also bounds tombstone lifetime across serial wrap.
        *expired_checkpoint = None;
    }
}

/// A loopback-only listener pair. `serve` accepts one authenticated bridge and
/// one authenticated control peer before entering the checkpoint session loop.
pub struct LocalSidecar {
    bridge_listener: TcpListener,
    bridge_address: SocketAddr,
    bridge_secret: SessionSecret,
    control_listener: ControlListener,
    session_epoch: u32,
    sequence_state: SessionSequenceState,
    command_history: HashMap<CommandId, CommandRecord>,
}

impl LocalSidecar {
    /// Binds both literal-loopback listeners before returning a descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error when the epoch is zero or either loopback listener
    /// cannot be bound.
    pub async fn bind_with_epoch(session_epoch: u32) -> Result<Self, SidecarError> {
        if session_epoch == 0 {
            return Err(SidecarError::SessionEpochZero);
        }
        let bridge_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(SidecarError::Listener)?;
        let bridge_address = bridge_listener
            .local_addr()
            .map_err(SidecarError::Listener)?;
        let control_listener = ControlListener::bind(session_epoch).await?;
        Ok(Self {
            bridge_listener,
            bridge_address,
            bridge_secret: SessionSecret::generate(),
            control_listener,
            session_epoch,
            sequence_state: SessionSequenceState::default(),
            command_history: HashMap::new(),
        })
    }

    #[must_use]
    pub fn session_descriptor(&self) -> SessionDescriptor {
        SessionDescriptor {
            version: 1,
            session_epoch: self.session_epoch,
            bridge: BridgeDescriptor {
                transport: "tcp".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: self.bridge_address.port(),
                secret: self.bridge_secret.expose().to_owned(),
                bridge_abi: BRIDGE_ABI_VERSION,
                protocol_version: GAME_PROTOCOL_VERSION,
                frame_bytes: BRIDGE_FRAME_SIZE,
            },
            control: ControlDescriptor {
                transport: "tcp".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: self.control_listener.address().port(),
                secret: self.control_listener.secret().expose().to_owned(),
                control_version: CONTROL_PROTOCOL_VERSION,
                max_line_bytes: MAX_CONTROL_LINE_BYTES,
            },
        }
    }

    /// Accepts exactly one authenticated bridge peer.
    ///
    /// # Errors
    ///
    /// Returns an error when accepting or authenticating the peer fails.
    pub(crate) async fn accept(&mut self) -> Result<AuthenticatedConnection, SidecarError> {
        let (stream, peer) = self
            .bridge_listener
            .accept()
            .await
            .map_err(SidecarError::Listener)?;
        self.authenticate(stream, peer).await
    }

    async fn accept_control(&mut self) -> Result<ControlConnection, SidecarError> {
        self.control_listener.accept().await.map_err(Into::into)
    }

    async fn accept_reconnect_control(&mut self) -> Result<ControlConnection, SidecarError> {
        // This wait is scoped to the authenticated bridge session. Each
        // candidate handshake has its own bounded control timeout, and the
        // bridge reader/channel stay fixed and bounded while invalid peers
        // are discarded.
        loop {
            match self.accept_control().await {
                Ok(connection) => return Ok(connection),
                Err(SidecarError::Control(ControlError::Listener(error))) => {
                    return Err(SidecarError::Control(ControlError::Listener(error)));
                }
                // Invalid replacement peers are isolated just like invalid
                // peers during the initial pair acquisition. Keep the
                // authenticated bridge session available for the next valid
                // control connection.
                Err(_) => {}
            }
        }
    }

    /// Serves authenticated bridge/control sessions. Invalid clients are
    /// disconnected while accepting; idle or pre-grant connection loss may
    /// reconnect with retained state, while a post-grant failure terminates
    /// the process so a checkpoint cannot be silently reset.
    ///
    /// # Errors
    ///
    /// Returns an error when a listener itself can no longer accept clients.
    pub async fn serve(mut self) -> Result<(), SidecarError> {
        let mut reconnect_state = ReconnectState::default();
        loop {
            let (bridge, control) = self.accept_pair().await?;
            match self
                .serve_authenticated_controlled_client(bridge, control, reconnect_state)
                .await?
            {
                SessionExit::Reconnect(state) => reconnect_state = state,
            }
        }
    }

    async fn accept_pair(
        &mut self,
    ) -> Result<(AuthenticatedConnection, ControlConnection), SidecarError> {
        // Control authentication is deliberately first: the launcher must own
        // the control secret before it starts mGBA and supplies the bridge Lua
        // script. The already-bound bridge listener can queue its peer meanwhile.
        let control = loop {
            match self.accept_control().await {
                Ok(connection) => break connection,
                Err(SidecarError::Control(ControlError::Listener(error))) => {
                    return Err(SidecarError::Control(ControlError::Listener(error)));
                }
                Err(_) => {}
            }
        };
        let bridge = loop {
            match self.accept().await {
                Ok(connection) => break connection,
                Err(SidecarError::Listener(error)) => return Err(SidecarError::Listener(error)),
                Err(_) => {}
            }
        };
        Ok((bridge, control))
    }

    async fn authenticate(
        &mut self,
        mut stream: TcpStream,
        peer: SocketAddr,
    ) -> Result<AuthenticatedConnection, SidecarError> {
        if !peer.ip().is_loopback() {
            return Err(SidecarError::NonLoopbackPeer(peer));
        }
        stream.set_nodelay(true).map_err(SidecarError::Connection)?;
        let line = timeout(HANDSHAKE_TIMEOUT, read_bounded_handshake(&mut stream))
            .await
            .map_err(|_| SidecarError::HandshakeTimeout)??;
        let request: HandshakeRequest =
            serde_json::from_slice(&line).map_err(SidecarError::MalformedHandshake)?;

        if request.bridge_abi != BRIDGE_ABI_VERSION {
            return Err(SidecarError::IncompatibleBridgeAbi {
                received: request.bridge_abi,
            });
        }
        if request.protocol_version != GAME_PROTOCOL_VERSION {
            return Err(SidecarError::IncompatibleProtocolVersion {
                received: request.protocol_version,
            });
        }
        if !self.bridge_secret.matches(&request.secret) {
            return Err(SidecarError::AuthenticationFailed);
        }

        timeout(HANDSHAKE_TIMEOUT, stream.write_all(HANDSHAKE_ACCEPTED_LINE))
            .await
            .map_err(|_| SidecarError::HandshakeWriteTimeout)?
            .map_err(SidecarError::Connection)?;
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(MessageType::SessionReady, sequence, self.session_epoch, &[])?;
        let frame_bytes = frame.encode();
        timeout(HANDSHAKE_TIMEOUT, stream.write_all(&frame_bytes))
            .await
            .map_err(|_| SidecarError::HandshakeWriteTimeout)?
            .map_err(SidecarError::Connection)?;
        Ok(AuthenticatedConnection { stream })
    }

    async fn send_session_ready_to_stream(
        &mut self,
        connection: &mut BridgeWriter,
    ) -> Result<(), SidecarError> {
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(MessageType::SessionReady, sequence, self.session_epoch, &[])?;
        connection.send(&frame, Direction::SidecarToRom).await
    }

    async fn serve_authenticated_controlled_client(
        &mut self,
        bridge: AuthenticatedConnection,
        first_control: ControlConnection,
        reconnect_state: ReconnectState,
    ) -> Result<SessionExit, SidecarError> {
        // Socket reads are owned by dedicated tasks. The bridge decoder and
        // its bounded channel remain alive across a reconnectable control
        // loss, so a decoded (or still unread) CHECKPOINT_READY cannot fall
        // into the gap between control shutdown and the next authentication.
        let (mut bridge_reader, bridge_writer) = bridge.into_split();
        let (bridge_tx, bridge_rx) = mpsc::channel::<Result<BridgeFrame, SidecarError>>(1);
        let bridge_task = tokio::spawn(async move {
            loop {
                match bridge_reader.receive(Direction::RomToSidecar).await {
                    Ok(Some(frame)) => {
                        if bridge_tx.send(Ok(frame)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = bridge_tx
                            .send(Err(SidecarError::ProtocolViolation("bridge disconnected")))
                            .await;
                        break;
                    }
                    Err(error) => {
                        let _ = bridge_tx.send(Err(error)).await;
                        break;
                    }
                }
            }
        });
        let mut reader_tasks = ReaderTasks::new_bridge(bridge_task);
        let mut bridge_writer = bridge_writer;
        let mut bridge_rx = bridge_rx;
        let mut reconnect_state = reconnect_state;
        let mut control = first_control;
        let mut new_bridge_connection = true;

        loop {
            let (control_writer, control_rx, control_task) = spawn_control_reader(control);
            reader_tasks.set_control(control_task);

            let (result, next_state, next_bridge_writer, next_bridge_rx) = self
                .run_checkpoint_session(
                    bridge_writer,
                    control_writer,
                    bridge_rx,
                    control_rx,
                    reconnect_state,
                    new_bridge_connection,
                )
                .await;
            bridge_writer = next_bridge_writer;
            bridge_rx = next_bridge_rx;
            reader_tasks.shutdown_control().await;

            match result {
                Ok(()) => {
                    reader_tasks.shutdown().await;
                    return Ok(SessionExit::Reconnect(next_state));
                }
                Err(error)
                    if can_reconnect_after(&error)
                        && is_control_reconnect_error(&error)
                        && matches!(
                            next_state.checkpoint_state,
                            CheckpointState::Idle
                                | CheckpointState::ReadyPendingHandoff { .. }
                                | CheckpointState::AwaitDecision { .. }
                                | CheckpointState::ExpiryPendingHandoff { .. }
                        ) =>
                {
                    // Keep the bridge reader, writer, and channel alive while
                    // a replacement control peer authenticates. Any frame
                    // that arrives during this interval remains bounded by
                    // the channel capacity and is processed by the next run.
                    reconnect_state = next_state;
                    control = self.accept_reconnect_control().await?;
                    new_bridge_connection = false;
                }
                Err(error)
                    if can_reconnect_after(&error)
                        && matches!(
                            next_state.checkpoint_state,
                            CheckpointState::Idle
                                | CheckpointState::ReadyPendingHandoff { .. }
                                | CheckpointState::AwaitDecision { .. }
                                | CheckpointState::ExpiryPendingHandoff { .. }
                        ) =>
                {
                    // The bridge itself is no longer usable. Return to the
                    // outer accept loop for a fresh authenticated pair;
                    // preserving this reader would only spin on its terminal
                    // channel error.
                    reader_tasks.shutdown().await;
                    return Ok(SessionExit::Reconnect(next_state));
                }
                Err(error) => {
                    reader_tasks.shutdown().await;
                    return Err(error);
                }
            }
        }
    }

    async fn run_checkpoint_session(
        &mut self,
        mut bridge_writer: BridgeWriter,
        mut control_writer: ControlWriter,
        mut bridge_rx: mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        mut control_rx: mpsc::Receiver<Result<ControlCommand, SidecarError>>,
        reconnect_state: ReconnectState,
        new_bridge_connection: bool,
    ) -> (
        Result<(), SidecarError>,
        ReconnectState,
        BridgeWriter,
        mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
    ) {
        let mut session =
            ActiveSessionState::from_reconnect(reconnect_state, new_bridge_connection);
        if new_bridge_connection {
            // A newly authenticated bridge must acknowledge its fresh
            // SESSION_READY before it can originate checkpoint traffic. This
            // is intentionally not inherited across bridge reconnects, while
            // a control-only reconnect keeps the existing bridge generation.
            self.sequence_state.rearm_boot_after_bridge_reconnect();
        }
        let result = async {
        self.expire_checkpoint_if_due(&mut session, &mut control_writer, Instant::now())
            .await?;
            self.resume_checkpoint_handoff(&mut session, &mut bridge_writer, &mut control_writer)
            .await?;
            loop {
                // Resolve a timer/input race before selecting another input source.
                self.expire_checkpoint_if_due(&mut session, &mut control_writer, Instant::now())
                    .await?;
                self.drain_queued_bridge_frames(
                    &mut bridge_rx,
                    &mut bridge_writer,
                    &mut control_writer,
                    &mut session,
                )
                .await?;
                let deadline = session.checkpoint_state.deadline();
                tokio::select! {
                    bridge_result = bridge_rx.recv() => {
                        let Some(result) = bridge_result else {
                            break Err(SidecarError::ProtocolViolation("bridge reader terminated"));
                        };
                        let frame = result?;
                        self.handle_bridge_frame(frame, &mut bridge_writer, &mut control_writer, &mut session).await?;
                    }
                    command_result = control_rx.recv() => {
                        match command_result {
                            None => {
                                // A complete bridge frame may already be
                                // queued when the control reader reports its
                                // loss. Drain it before classifying the
                                // session exit so a ready handoff is retained.
                                self.drain_queued_bridge_frames(
                                    &mut bridge_rx,
                                    &mut bridge_writer,
                                    &mut control_writer,
                                    &mut session,
                                ).await?;
                                break Err(SidecarError::Control(ControlError::Connection(
                                    io::Error::new(io::ErrorKind::UnexpectedEof, "control reader terminated"),
                                )));
                            }
                            Some(Err(error)) => {
                                self.drain_queued_bridge_frames(
                                    &mut bridge_rx,
                                    &mut bridge_writer,
                                    &mut control_writer,
                                    &mut session,
                                ).await?;
                                break Err(error);
                            }
                            Some(Ok(command)) => {
                                self.handle_control_command(
                                    command,
                                    &mut bridge_writer,
                                    &mut control_writer,
                                    Instant::now(),
                                    &mut session,
                                ).await?;
                            }
                        }
                    }
                    () = checkpoint_deadline(deadline) => {
                        self.expire_checkpoint_if_due(&mut session, &mut control_writer, Instant::now()).await?;
                    }
                }
            }
        }
        .await;
        (result, session.into_reconnect(), bridge_writer, bridge_rx)
    }

    async fn drain_queued_bridge_frames(
        &mut self,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        loop {
            let result = match bridge_rx.try_recv() {
                Ok(result) => result,
                Err(mpsc::error::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(SidecarError::ProtocolViolation("bridge reader terminated"));
                }
            };
            let frame = result?;
            self.handle_bridge_frame(frame, bridge, control, session)
                .await?;
        }
    }

    async fn resume_checkpoint_handoff(
        &mut self,
        session: &mut ActiveSessionState,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
    ) -> Result<(), SidecarError> {
        match session.checkpoint_state {
            CheckpointState::ReadyPendingHandoff { key } => {
                // A ready frame is staged until its critical control event has
                // made it to the authenticated launcher. The decision clock
                // begins only after that handoff succeeds.
                control
                    .send_event(&ControlEvent::CheckpointReady {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                    })
                    .await?;
                self.sequence_state.commit_checkpoint_ready(key);
                session.expired_checkpoint = None;
                session.checkpoint_state = CheckpointState::AwaitDecision {
                    key,
                    deadline: Instant::now() + DECISION_TIMEOUT,
                };
            }
            CheckpointState::ExpiryPendingHandoff { key } => {
                control
                    .send_event(&ControlEvent::CheckpointExpired {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                    })
                    .await?;
                session.checkpoint_state = CheckpointState::Idle;
                if session.rearm_after_reboot {
                    self.complete_reboot_rearm(bridge, session).await?;
                }
            }
            // An AwaitDecision event was already handed off before reconnect;
            // preserve its original deadline and avoid duplicate launcher
            // notifications. Only a failed ReadyPendingHandoff is replayed.
            CheckpointState::AwaitDecision { .. }
            | CheckpointState::Idle
            | CheckpointState::AwaitSaveData { .. } => {}
        }
        Ok(())
    }

    async fn complete_reboot_rearm(
        &mut self,
        bridge: &mut BridgeWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        self.sequence_state.rearm_session_after_rom_reboot(1);
        session.acknowledged_rom_ready = false;
        // This write is bounded and fatal on failure. Only after the ROM has
        // received its new SESSION_READY may the old exact-key tombstone be
        // rotated, allowing sequence one in the new generation.
        self.send_session_ready_to_stream(bridge).await?;
        session.acknowledged_rom_ready = true;
        session.rearm_after_reboot = false;
        session.expired_checkpoint = None;
        Ok(())
    }

    async fn handle_bridge_frame(
        &mut self,
        frame: BridgeFrame,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        self.expire_checkpoint_if_due(session, control, Instant::now())
            .await?;
        if self
            .handle_rom_reboot(&frame, bridge, control, session)
            .await?
        {
            return Ok(());
        }
        match frame.message_type() {
            MessageType::CheckpointReady => {
                if frame.session_epoch() != self.session_epoch || !frame.payload().is_empty() {
                    return Err(SidecarError::ProtocolViolation("invalid checkpoint ready"));
                }
                if !session.acknowledged_rom_ready {
                    return Err(SidecarError::ProtocolViolation(
                        "checkpoint ready before rom ready",
                    ));
                }
                let key = CheckpointKey::new(self.session_epoch, frame.sequence())
                    .ok_or(SidecarError::ProtocolViolation("invalid checkpoint key"))?;
                // A checkpoint retired during expiry or a ROM reboot remains a
                // stale exact key even when the ROM has been rearmed at
                // sequence one. This prevents the retired frame from opening
                // a new decision window after reconnect.
                if Some(key) == session.expired_checkpoint {
                    return Ok(());
                }
                if !self
                    .sequence_state
                    .inspect_rom_frame(&frame, self.session_epoch)
                {
                    return Ok(());
                }
                if !matches!(session.checkpoint_state, CheckpointState::Idle) {
                    return Err(SidecarError::ProtocolViolation(
                        "checkpoint already pending",
                    ));
                }
                session.checkpoint_state = CheckpointState::ReadyPendingHandoff { key };
                control
                    .send_event(&ControlEvent::CheckpointReady {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                    })
                    .await?;
                self.sequence_state.commit_checkpoint_ready(key);
                session.expired_checkpoint = None;
                session.checkpoint_state = CheckpointState::AwaitDecision {
                    key,
                    deadline: Instant::now() + DECISION_TIMEOUT,
                };
            }
            MessageType::SaveDataUpdated => {
                if frame.session_epoch() != self.session_epoch || !frame.payload().is_empty() {
                    return Err(SidecarError::ProtocolViolation("invalid save-data update"));
                }
                let CheckpointState::AwaitSaveData { key, .. } = session.checkpoint_state else {
                    return Err(SidecarError::ProtocolViolation(
                        "save-data update without grant",
                    ));
                };
                if !self
                    .sequence_state
                    .inspect_rom_frame(&frame, self.session_epoch)
                {
                    return Ok(());
                }
                control
                    .send_event(&ControlEvent::SaveDataUpdated {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                        save_sequence: frame.sequence(),
                    })
                    .await?;
                self.sequence_state.commit_rom_frame(&frame);
                rotate_expired_tombstone(&frame, &mut session.expired_checkpoint);
                session.checkpoint_state = CheckpointState::Idle;
            }
            _ => {
                if !self
                    .sequence_state
                    .accept_rom_frame(&frame, self.session_epoch)
                {
                    return Ok(());
                }
                if frame.message_type() == MessageType::RomReady && !session.acknowledged_rom_ready
                {
                    self.send_session_ready_to_stream(bridge).await?;
                    session.acknowledged_rom_ready = true;
                }
                rotate_expired_tombstone(&frame, &mut session.expired_checkpoint);
            }
        }
        Ok(())
    }

    async fn handle_rom_reboot(
        &mut self,
        frame: &BridgeFrame,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<bool, SidecarError> {
        let is_boot_epoch_reboot = frame.session_epoch() == 0 && session.acknowledged_rom_ready;
        let is_session_epoch_reboot = frame.session_epoch() == self.session_epoch;
        if frame.message_type() != MessageType::RomReady
            || (!is_boot_epoch_reboot && !is_session_epoch_reboot)
            || frame.sequence() != 1
            || !frame.payload().is_empty()
        {
            return Ok(false);
        }
        match session.checkpoint_state {
            CheckpointState::AwaitSaveData { .. } => Err(SidecarError::ProtocolViolation(
                "rom reboot during post-grant checkpoint",
            )),
            CheckpointState::ReadyPendingHandoff { key }
            | CheckpointState::AwaitDecision { key, .. } => {
                // Retire a staged/awaiting checkpoint before rearming the ROM
                // cursor. The exact-key tombstone below also blocks a replay
                // of this frame after the reboot resets the ROM sequence.
                session.checkpoint_state = CheckpointState::ExpiryPendingHandoff { key };
                session.expired_checkpoint = Some(key);
                session.rearm_after_reboot = true;
                self.sequence_state.commit_checkpoint_ready(key);
                control
                    .send_event(&ControlEvent::CheckpointExpired {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                    })
                    .await?;
                session.checkpoint_state = CheckpointState::Idle;
                self.complete_reboot_rearm(bridge, session).await?;
                Ok(true)
            }
            CheckpointState::Idle
                if is_boot_epoch_reboot || self.sequence_state.last_session_rom > 1 =>
            {
                session.rearm_after_reboot = true;
                self.complete_reboot_rearm(bridge, session).await?;
                Ok(true)
            }
            CheckpointState::ExpiryPendingHandoff { key } => {
                // Timer expiry may have staged the durable handoff before a
                // failed control write. If the queued epoch-zero READY is
                // drained after that failure, retain the reboot intent for
                // the reconnect path instead of treating the frame as an
                // unrelated duplicate. The replacement SESSION_READY and
                // cursor rearm must wait until Expired is handed off.
                session.expired_checkpoint = Some(key);
                session.rearm_after_reboot = true;
                Ok(true)
            }
            CheckpointState::Idle => Ok(false),
        }
    }

    async fn handle_control_command(
        &mut self,
        command: ControlCommand,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        now: Instant,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let (command_id, fingerprint, key, is_grant) = command_parts(&command);

        if self
            .replay_existing_command(command_id, fingerprint, control)
            .await?
        {
            return Ok(());
        }
        // Reserve ledger capacity before any state transition, bridge frame,
        // or result event. A full ledger must never allow a side effect that
        // cannot be replayed for the lifetime of this session.
        if self.command_history.len() >= MAX_COMMAND_HISTORY {
            return Err(SidecarError::CommandLedgerFull);
        }
        self.expire_checkpoint_if_due(session, control, now).await?;

        let mut status = CommandStatus::Rejected;
        let mut reason = if command_is_expired(key, session.expired_checkpoint) {
            Some(CommandReason::Expired)
        } else {
            Some(CommandReason::WrongState)
        };
        if let Some(key) = key {
            if key.session_epoch == self.session_epoch {
                match (session.checkpoint_state, is_grant) {
                    (CheckpointState::AwaitDecision { key: pending, .. }, true)
                        if pending == key && session.acknowledged_rom_ready =>
                    {
                        let sequence = self.sequence_state.take_sidecar_sequence();
                        let frame = BridgeFrame::new(
                            MessageType::CheckpointGranted,
                            sequence,
                            self.session_epoch,
                            &[],
                        )?;
                        bridge.send(&frame, Direction::SidecarToRom).await?;
                        session.checkpoint_state = CheckpointState::AwaitSaveData {
                            key,
                            deadline: Instant::now() + SAVE_DATA_TIMEOUT,
                        };
                        status = CommandStatus::Applied;
                        reason = None;
                    }
                    (CheckpointState::AwaitDecision { key: pending, .. }, false)
                        if pending == key =>
                    {
                        session.checkpoint_state = CheckpointState::Idle;
                        status = CommandStatus::Applied;
                        reason = None;
                    }
                    (CheckpointState::AwaitDecision { .. }, _) => {
                        reason = Some(CommandReason::StaleCheckpoint);
                    }
                    _ => {}
                }
            } else {
                reason = Some(CommandReason::WrongEpoch);
            }
        } else {
            reason = Some(CommandReason::InvalidPayload);
        }

        self.command_history.insert(
            command_id,
            CommandRecord {
                fingerprint,
                reason,
            },
        );
        control
            .send_event(&ControlEvent::CommandResult {
                command_id,
                status,
                reason,
            })
            .await?;
        Ok(())
    }

    async fn replay_existing_command(
        &self,
        command_id: CommandId,
        fingerprint: CommandFingerprint,
        control: &mut ControlWriter,
    ) -> Result<bool, SidecarError> {
        let Some(previous) = self.command_history.get(&command_id) else {
            return Ok(false);
        };
        let (status, reason) = if previous.fingerprint == fingerprint {
            (CommandStatus::Replayed, previous.reason)
        } else {
            (
                CommandStatus::Conflict,
                Some(CommandReason::CommandBodyConflict),
            )
        };
        control
            .send_event(&ControlEvent::CommandResult {
                command_id,
                status,
                reason,
            })
            .await?;
        Ok(true)
    }

    async fn expire_checkpoint_if_due(
        &mut self,
        session: &mut ActiveSessionState,
        control: &mut ControlWriter,
        now: Instant,
    ) -> Result<bool, SidecarError> {
        match session.checkpoint_state {
            CheckpointState::AwaitDecision { key, deadline } if now >= deadline => {
                session.checkpoint_state = CheckpointState::ExpiryPendingHandoff { key };
                session.expired_checkpoint = Some(key);
                // Retire the ROM frame before sending the critical expiry
                // event. If that send fails, reconnect will hand off this
                // pending expiry exactly once from the pending state.
                self.sequence_state.commit_checkpoint_ready(key);
                control
                    .send_event(&ControlEvent::CheckpointExpired {
                        session_epoch: key.session_epoch,
                        ready_sequence: key.ready_sequence,
                    })
                    .await?;
                session.checkpoint_state = CheckpointState::Idle;
                Ok(true)
            }
            CheckpointState::AwaitSaveData { deadline, .. } if now >= deadline => {
                Err(SidecarError::CheckpointTimeout)
            }
            _ => Ok(false),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckpointState {
    Idle,
    ReadyPendingHandoff {
        key: CheckpointKey,
    },
    AwaitDecision {
        key: CheckpointKey,
        deadline: Instant,
    },
    ExpiryPendingHandoff {
        key: CheckpointKey,
    },
    AwaitSaveData {
        key: CheckpointKey,
        deadline: Instant,
    },
}

impl CheckpointState {
    const fn deadline(self) -> Option<Instant> {
        match self {
            Self::Idle | Self::ReadyPendingHandoff { .. } | Self::ExpiryPendingHandoff { .. } => {
                None
            }
            Self::AwaitDecision { deadline, .. } | Self::AwaitSaveData { deadline, .. } => {
                Some(deadline)
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ReconnectState {
    checkpoint_state: CheckpointState,
    expired_checkpoint: Option<CheckpointKey>,
    acknowledged_rom_ready: bool,
    rearm_after_reboot: bool,
}

struct ActiveSessionState {
    checkpoint_state: CheckpointState,
    expired_checkpoint: Option<CheckpointKey>,
    acknowledged_rom_ready: bool,
    rearm_after_reboot: bool,
}

impl ActiveSessionState {
    fn from_reconnect(reconnect: ReconnectState, new_bridge_connection: bool) -> Self {
        Self {
            checkpoint_state: reconnect.checkpoint_state,
            expired_checkpoint: reconnect.expired_checkpoint,
            acknowledged_rom_ready: reconnect.acknowledged_rom_ready && !new_bridge_connection,
            rearm_after_reboot: reconnect.rearm_after_reboot,
        }
    }

    fn into_reconnect(self) -> ReconnectState {
        ReconnectState {
            checkpoint_state: self.checkpoint_state,
            expired_checkpoint: self.expired_checkpoint,
            acknowledged_rom_ready: self.acknowledged_rom_ready,
            rearm_after_reboot: self.rearm_after_reboot,
        }
    }
}

impl Default for ReconnectState {
    fn default() -> Self {
        Self {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: false,
            rearm_after_reboot: false,
        }
    }
}

enum SessionExit {
    Reconnect(ReconnectState),
}

fn can_reconnect_after(error: &SidecarError) -> bool {
    matches!(
        error,
        SidecarError::BridgeConnection(_)
            | SidecarError::ProtocolViolation("bridge disconnected")
            | SidecarError::Control(
                ControlError::LineClosed
                    | ControlError::Connection(_)
                    | ControlError::WriteConnection(_)
                    | ControlError::WriteTimeout,
            )
    )
}

fn is_control_reconnect_error(error: &SidecarError) -> bool {
    matches!(error, SidecarError::Control(_))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandFingerprint {
    Grant {
        session_epoch: u32,
        ready_sequence: u32,
    },
    Abort {
        session_epoch: u32,
        ready_sequence: u32,
    },
}

fn command_parts(
    command: &ControlCommand,
) -> (CommandId, CommandFingerprint, Option<CheckpointKey>, bool) {
    match command {
        ControlCommand::CheckpointGrant(CheckpointGrant {
            command_id,
            session_epoch,
            ready_sequence,
        }) => (
            *command_id,
            CommandFingerprint::Grant {
                session_epoch: *session_epoch,
                ready_sequence: *ready_sequence,
            },
            CheckpointKey::new(*session_epoch, *ready_sequence),
            true,
        ),
        ControlCommand::CheckpointAbort(CheckpointAbort {
            command_id,
            session_epoch,
            ready_sequence,
        }) => (
            *command_id,
            CommandFingerprint::Abort {
                session_epoch: *session_epoch,
                ready_sequence: *ready_sequence,
            },
            CheckpointKey::new(*session_epoch, *ready_sequence),
            false,
        ),
    }
}

fn command_is_expired(
    key: Option<CheckpointKey>,
    expired_checkpoint: Option<CheckpointKey>,
) -> bool {
    key.is_some_and(|key| Some(key) == expired_checkpoint)
}

#[derive(Clone, Copy, Debug)]
struct CommandRecord {
    fingerprint: CommandFingerprint,
    reason: Option<CommandReason>,
}

async fn checkpoint_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn read_bounded_handshake(stream: &mut TcpStream) -> Result<Vec<u8>, SidecarError> {
    let mut line = Vec::with_capacity(128);
    let mut byte = [0_u8; 1];
    loop {
        let count = stream
            .read(&mut byte)
            .await
            .map_err(SidecarError::Connection)?;
        if count == 0 {
            return Err(SidecarError::HandshakeClosed);
        }
        if line.len() + 1 > MAX_HANDSHAKE_BYTES {
            return Err(SidecarError::HandshakeTooLarge);
        }
        if byte[0] == b'\n' {
            return Ok(line);
        }
        line.push(byte[0]);
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::oneshot,
        time::{Duration, timeout},
    };

    use super::*;

    const TEST_SESSION_EPOCH: u32 = 41;

    #[derive(Serialize)]
    struct TestHandshake<'a> {
        secret: &'a str,
        bridge_abi: u16,
        protocol_version: u16,
    }

    #[derive(Serialize)]
    struct TestControlHandshake<'a> {
        secret: &'a str,
        control_version: u16,
        session_epoch: u32,
    }

    async fn write_handshake(
        stream: &mut TcpStream,
        descriptor: &SessionDescriptor,
        secret: &str,
    ) -> BridgeFrame {
        let mut line = serde_json::to_vec(&TestHandshake {
            secret,
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        line.push(b'\n');
        stream.write_all(&line).await.unwrap();
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        stream.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
        let mut bootstrap_bytes = [0; BRIDGE_FRAME_SIZE];
        stream.read_exact(&mut bootstrap_bytes).await.unwrap();
        let bootstrap = BridgeFrame::decode_for(&bootstrap_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(bootstrap.message_type(), MessageType::SessionReady);
        assert_eq!(bootstrap.session_epoch(), TEST_SESSION_EPOCH);
        assert_eq!(stream.peer_addr().unwrap(), descriptor.address());
        bootstrap
    }

    async fn write_fragmented_handshake(
        stream: &mut TcpStream,
        descriptor: &SessionDescriptor,
        secret: &str,
    ) {
        let mut line = serde_json::to_vec(&TestHandshake {
            secret,
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        line.push(b'\n');
        let split = line.len() / 2;
        stream.write_all(&line[..split]).await.unwrap();
        tokio::task::yield_now().await;
        stream.write_all(&line[split..]).await.unwrap();
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        stream.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
        let mut bootstrap_bytes = [0; BRIDGE_FRAME_SIZE];
        stream.read_exact(&mut bootstrap_bytes).await.unwrap();
        let bootstrap = BridgeFrame::decode_for(&bootstrap_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(bootstrap.message_type(), MessageType::SessionReady);
        assert_eq!(stream.peer_addr().unwrap(), descriptor.address());
    }

    async fn write_control_handshake(
        stream: &mut TcpStream,
        descriptor: &SessionDescriptor,
        secret: &str,
        epoch: u32,
    ) {
        let mut line = serde_json::to_vec(&TestControlHandshake {
            secret,
            control_version: CONTROL_PROTOCOL_VERSION,
            session_epoch: epoch,
        })
        .unwrap();
        line.push(b'\n');
        stream.write_all(&line).await.unwrap();
        let mut accepted = [0; crate::control::CONTROL_HANDSHAKE_ACCEPTED_LINE.len()];
        stream.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, crate::control::CONTROL_HANDSHAKE_ACCEPTED_LINE);
        assert_eq!(stream.peer_addr().unwrap(), descriptor.control_address());
    }

    async fn establish_rom_session(stream: &mut TcpStream) {
        let ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        stream.write_all(&ready.encode()).await.unwrap();
        let mut session_ready = [0; BRIDGE_FRAME_SIZE];
        stream.read_exact(&mut session_ready).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&session_ready, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
    }

    async fn bridge_writer_pair() -> (BridgeWriter, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (_, writer) = (AuthenticatedConnection { stream: server }).into_split();
        (writer, client)
    }

    async fn control_writer_pair() -> (ControlWriter, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (_, writer) = ControlConnection::from_authenticated_stream(server).into_split();
        (writer, client)
    }

    async fn read_event(stream: &mut TcpStream) -> ControlEvent {
        let mut line = Vec::new();
        let mut byte = [0];
        loop {
            stream.read_exact(&mut byte).await.unwrap();
            if byte[0] == b'\n' {
                return serde_json::from_slice(&line).unwrap();
            }
            line.push(byte[0]);
            assert!(line.len() < MAX_CONTROL_LINE_BYTES);
        }
    }

    fn grant(command_id: &str, epoch: u32, sequence: u32) -> ControlCommand {
        ControlCommand::CheckpointGrant(CheckpointGrant {
            command_id: CommandId::parse(command_id).unwrap(),
            session_epoch: epoch,
            ready_sequence: sequence,
        })
    }

    fn abort(command_id: &str, epoch: u32, sequence: u32) -> ControlCommand {
        ControlCommand::CheckpointAbort(CheckpointAbort {
            command_id: CommandId::parse(command_id).unwrap(),
            session_epoch: epoch,
            ready_sequence: sequence,
        })
    }

    async fn send_command(stream: &mut TcpStream, command: &ControlCommand) {
        let mut bytes = serde_json::to_vec(command).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).await.unwrap();
    }

    struct DropSignal(Option<oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    async fn fill_command_ledger(control: &mut TcpStream) {
        for index in 1..=MAX_COMMAND_HISTORY {
            let command_id = format!("00000000-0000-4000-8000-{index:012x}");
            send_command(control, &abort(&command_id, TEST_SESSION_EPOCH, 1)).await;
        }
        for _ in 1..=MAX_COMMAND_HISTORY {
            assert!(matches!(
                read_event(control).await,
                ControlEvent::CommandResult {
                    status: CommandStatus::Rejected,
                    ..
                }
            ));
        }
    }

    #[tokio::test]
    async fn reader_task_owner_aborts_both_tasks_when_cancelled() {
        let (bridge_signal, mut bridge_dropped) = oneshot::channel();
        let (control_signal, mut control_dropped) = oneshot::channel();
        let bridge_task = tokio::spawn(async move {
            let _signal = DropSignal(Some(bridge_signal));
            std::future::pending::<()>().await;
        });
        let control_task = tokio::spawn(async move {
            let _signal = DropSignal(Some(control_signal));
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        drop(ReaderTasks::new(bridge_task, control_task));
        timeout(Duration::from_secs(1), &mut bridge_dropped)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(1), &mut control_dropped)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn descriptor_binds_distinct_loopback_endpoints_and_redacts_debug() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        assert_eq!(descriptor.version(), 1);
        assert_eq!(descriptor.session_epoch(), TEST_SESSION_EPOCH);
        assert_eq!(descriptor.address().ip(), Ipv4Addr::LOCALHOST);
        assert_eq!(descriptor.control_address().ip(), Ipv4Addr::LOCALHOST);
        assert_ne!(descriptor.address(), descriptor.control_address());
        assert_ne!(descriptor.secret(), descriptor.control_secret());
        assert_eq!(descriptor.bridge().transport(), "tcp");
        assert_eq!(descriptor.bridge().host(), "127.0.0.1");
        assert_eq!(descriptor.bridge().port(), descriptor.address().port());
        assert_eq!(descriptor.bridge().bridge_abi(), BRIDGE_ABI_VERSION);
        assert_eq!(
            descriptor.bridge().protocol_version(),
            GAME_PROTOCOL_VERSION
        );
        assert_eq!(descriptor.bridge().frame_bytes(), BRIDGE_FRAME_SIZE);
        assert_eq!(descriptor.control().transport(), "tcp");
        assert_eq!(descriptor.control().host(), "127.0.0.1");
        assert_eq!(
            descriptor.control().port(),
            descriptor.control_address().port()
        );
        assert_eq!(
            descriptor.control().control_version(),
            CONTROL_PROTOCOL_VERSION
        );
        assert_eq!(
            descriptor.control().max_line_bytes(),
            MAX_CONTROL_LINE_BYTES
        );
        assert!(descriptor.to_bounded_json_line().unwrap().len() <= MAX_DESCRIPTOR_BYTES);
        let debug = format!("{descriptor:?}");
        assert!(!debug.contains(descriptor.secret()));
        assert!(!debug.contains(descriptor.control_secret()));
        assert!(
            !serde_json::to_string(descriptor.bridge())
                .unwrap()
                .contains("control")
        );
    }

    #[tokio::test]
    async fn serve_authenticates_control_before_releasing_bridge_peer() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut bridge_handshake = serde_json::to_vec(&TestHandshake {
            secret: descriptor.secret(),
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        bridge_handshake.push(b'\n');
        bridge.write_all(&bridge_handshake).await.unwrap();
        let mut byte = [0];
        assert!(
            timeout(Duration::from_millis(100), bridge.read(&mut byte))
                .await
                .is_err()
        );

        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        bridge.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
        let mut bootstrap = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut bootstrap).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&bootstrap, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn checkpoint_ready_before_rom_ready_terminates_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        let result = timeout(Duration::from_secs(1), server_task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            result,
            Err(SidecarError::ProtocolViolation(
                "checkpoint ready before rom ready"
            ))
        ));
    }

    #[tokio::test]
    async fn idle_reconnect_preserves_command_ledger_and_sequence_state() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());

        let mut first_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut first_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut first_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let first_bootstrap =
            write_handshake(&mut first_bridge, &descriptor, descriptor.secret()).await;
        assert_eq!(first_bootstrap.sequence(), BOOTSTRAP_SEQUENCE);

        let rom_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        first_bridge.write_all(&rom_ready.encode()).await.unwrap();
        let mut first_session_ready = [0; BRIDGE_FRAME_SIZE];
        first_bridge
            .read_exact(&mut first_session_ready)
            .await
            .unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&first_session_ready, Direction::SidecarToRom)
                .unwrap()
                .sequence(),
            BOOTSTRAP_SEQUENCE + 1
        );

        let command_id = "00000000-0000-4000-8000-000000000008";
        send_command(
            &mut first_control,
            &abort(command_id, TEST_SESSION_EPOCH, 1),
        )
        .await;
        assert!(matches!(
            read_event(&mut first_control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::WrongState),
                ..
            }
        ));

        drop(first_control);
        drop(first_bridge);

        // Idle connection loss is the only reconnectable authenticated exit.
        // The sidecar retains its command ledger and sequence cursor while it
        // waits for a fresh control-authenticated bridge pair.
        let mut second_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut second_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut second_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let second_bootstrap =
            write_handshake(&mut second_bridge, &descriptor, descriptor.secret()).await;
        assert_eq!(second_bootstrap.sequence(), BOOTSTRAP_SEQUENCE + 2);
        send_command(
            &mut second_control,
            &abort(command_id, TEST_SESSION_EPOCH, 1),
        )
        .await;
        assert!(matches!(
            read_event(&mut second_control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Replayed,
                reason: Some(CommandReason::WrongState),
                ..
            }
        ));

        drop(second_control);
        drop(second_bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn pregrant_reconnect_preserves_pending_ready_without_duplicate_event() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());

        let mut first_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut first_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut first_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut first_bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut first_bridge).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        first_bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut first_control).await,
            ControlEvent::CheckpointReady { .. }
        ));

        drop(first_control);
        drop(first_bridge);

        let mut second_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut second_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut second_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut second_bridge, &descriptor, descriptor.secret()).await;
        assert!(
            timeout(Duration::from_millis(100), read_event(&mut second_control))
                .await
                .is_err()
        );
        establish_rom_session(&mut second_bridge).await;
        send_command(
            &mut second_control,
            &grant(
                "00000000-0000-4000-8000-000000000011",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut second_control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        second_bridge.read_exact(&mut granted).await.unwrap();
        let save =
            BridgeFrame::new(MessageType::SaveDataUpdated, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        second_bridge.write_all(&save.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut second_control).await,
            ControlEvent::SaveDataUpdated { .. }
        ));

        drop(second_control);
        drop(second_bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn control_loss_before_ready_enqueue_preserves_exactly_one_ready() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());

        let mut first_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut first_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;

        // Close control before authenticating its replacement. Completing the
        // replacement handshake proves the sidecar observed the loss while
        // the original bridge reader is still alive; the complete frame is
        // then delivered through that preserved reader/channel.
        drop(first_control);
        let mut second_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut second_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            timeout(Duration::from_secs(1), read_event(&mut second_control))
                .await
                .unwrap(),
            ControlEvent::CheckpointReady {
                ready_sequence: 1,
                ..
            }
        ));
        assert!(
            timeout(Duration::from_millis(100), read_event(&mut second_control))
                .await
                .is_err()
        );

        send_command(
            &mut second_control,
            &abort(
                "00000000-0000-4000-8000-000000000013",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut second_control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));

        drop(second_control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn idle_boot_epoch_rom_ready_rearms_after_rom_reboot() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;

        let initial_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&initial_ready.encode()).await.unwrap();
        let mut initial_session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut initial_session_ready).await.unwrap();

        let active_ready =
            BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&active_ready.encode()).await.unwrap();
        let reboot = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&reboot.encode()).await.unwrap();
        let mut session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut session_ready).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&session_ready, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady {
                ready_sequence: 2,
                ..
            }
        ));
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn pregrant_boot_epoch_rom_ready_expires_and_rearms_checkpoint() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;

        let initial_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&initial_ready.encode()).await.unwrap();
        let mut initial_session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut initial_session_ready).await.unwrap();

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 2,
            }
        ));

        let reboot = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&reboot.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointExpired {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 2,
            }
        ));
        let mut session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut session_ready).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&session_ready, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );

        let next_ready =
            BridgeFrame::new(MessageType::CheckpointReady, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&next_ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 2,
            }
        ));

        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn control_handshake_rejects_cross_use_unknown_and_wrong_epoch() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let address = descriptor.control_address();
        let wrong_secret = descriptor.secret().to_owned();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let line = format!(
            "{{\"secret\":\"{wrong_secret}\",\"control_version\":1,\"session_epoch\":41}}\n"
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::AuthenticationFailed))
        ));

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        let line = format!(
            "{{\"secret\":\"{}\",\"control_version\":1,\"session_epoch\":40,\"extra\":false}}\n",
            descriptor.control_secret()
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::MalformedHandshake(_)))
        ));

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        let line = format!(
            "{{\"secret\":\"{}\",\"control_version\":1,\"session_epoch\":40}}\n",
            descriptor.control_secret()
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::InvalidSessionEpoch))
        ));
    }

    #[tokio::test]
    async fn ready_grant_save_relay_is_typed_and_one_shot() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;

        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000000",
                TEST_SESSION_EPOCH - 1,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::WrongEpoch),
                ..
            }
        ));

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert_eq!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 1
            }
        );
        let command_id = "00000000-0000-4000-8000-000000000001";
        send_command(&mut control, &grant(command_id, TEST_SESSION_EPOCH, 1)).await;
        let granted = read_event(&mut control).await;
        assert!(matches!(
            granted,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                reason: None,
                ..
            }
        ));
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut bytes).await.unwrap();
        let frame = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(frame.message_type(), MessageType::CheckpointGranted);
        assert!(frame.payload().is_empty());

        send_command(&mut control, &grant(command_id, TEST_SESSION_EPOCH, 1)).await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Replayed,
                ..
            }
        ));
        assert!(
            timeout(Duration::from_millis(100), bridge.read(&mut bytes))
                .await
                .is_err()
        );

        let save =
            BridgeFrame::new(MessageType::SaveDataUpdated, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&save.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::SaveDataUpdated {
                ready_sequence: 1,
                save_sequence: 2,
                ..
            }
        ));
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn fragmented_bridge_and_control_inputs_are_reassembled_and_interleaved() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_fragmented_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let ready_bytes = ready.encode();
        bridge.write_all(&ready_bytes[..17]).await.unwrap();
        // The bridge reader task owns the partial frame while control remains
        // independently readable; neither input can steal the other's bytes.
        tokio::task::yield_now().await;
        send_command(
            &mut control,
            &abort(
                "00000000-0000-4000-8000-000000000010",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::WrongState),
                ..
            }
        ));
        bridge.write_all(&ready_bytes[17..]).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));

        let command = grant(
            "00000000-0000-4000-8000-000000000005",
            TEST_SESSION_EPOCH,
            1,
        );
        let mut command_bytes = serde_json::to_vec(&command).unwrap();
        command_bytes.push(b'\n');
        let split = command_bytes.len() / 2;
        control.write_all(&command_bytes[..split]).await.unwrap();
        tokio::task::yield_now().await;

        // While the grant command is still fragmented, deliver a complete
        // competing bridge event. The sidecar must service it without
        // cancelling or losing the control decoder's partial line.
        let player_state =
            BridgeFrame::new(MessageType::PlayerState, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&player_state.encode()).await.unwrap();
        control.write_all(&command_bytes[split..]).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut granted).await.unwrap();

        let save =
            BridgeFrame::new(MessageType::SaveDataUpdated, 3, TEST_SESSION_EPOCH, &[]).unwrap();
        let save_bytes = save.encode();
        bridge.write_all(&save_bytes[..23]).await.unwrap();
        tokio::task::yield_now().await;
        bridge.write_all(&save_bytes[23..]).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::SaveDataUpdated {
                save_sequence: 3,
                ..
            }
        ));
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn pregrant_abort_emits_result_without_rom_frame() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        let _ = read_event(&mut control).await;
        send_command(
            &mut control,
            &abort(
                "00000000-0000-4000-8000-000000000002",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        assert!(
            timeout(Duration::from_millis(100), bridge.read(&mut bytes))
                .await
                .is_err()
        );
        drop(control);
        drop(bridge);

        server_task.abort();
    }

    #[tokio::test]
    async fn command_reuse_with_changed_body_conflicts_and_wrong_state_is_rejected() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        let _ = read_event(&mut control).await;

        let command_id = "00000000-0000-4000-8000-000000000003";
        send_command(&mut control, &grant(command_id, TEST_SESSION_EPOCH, 1)).await;
        let _ = read_event(&mut control).await;
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut bytes).await.unwrap();

        // Reusing the same UUID for a different command body is a conflict,
        // never a second state transition.
        send_command(&mut control, &abort(command_id, TEST_SESSION_EPOCH, 1)).await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Conflict,
                reason: Some(CommandReason::CommandBodyConflict),
                ..
            }
        ));

        // A new abort UUID is validly parsed but cannot abort after a grant.
        send_command(
            &mut control,
            &abort(
                "00000000-0000-4000-8000-000000000004",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::WrongState),
                ..
            }
        ));
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn decision_expiry_emits_event_and_returns_to_idle_without_rom_frame() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        let _ = read_event(&mut control).await;
        assert!(matches!(
            timeout(
                DECISION_TIMEOUT + Duration::from_secs(1),
                read_event(&mut control)
            )
            .await
            .unwrap(),
            ControlEvent::CheckpointExpired {
                ready_sequence: 1,
                ..
            }
        ));
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000009",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::Expired),
                ..
            }
        ));
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        assert!(
            timeout(Duration::from_millis(100), bridge.read(&mut bytes))
                .await
                .is_err()
        );
        drop(control);
        drop(bridge);

        // A delivered expiry is not replayed forever on a later
        // pre-grant reconnect. The tombstone still classifies late commands,
        // but the Expired event itself was already handed off.
        let mut second_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut second_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut second_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut second_bridge, &descriptor, descriptor.secret()).await;
        assert!(
            timeout(Duration::from_millis(100), read_event(&mut second_control))
                .await
                .is_err()
        );
        drop(second_control);
        drop(second_bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn postgrant_save_timeout_terminates_authenticated_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000006",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut granted).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&granted, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::CheckpointGranted
        );

        assert!(
            timeout(SAVE_DATA_TIMEOUT + Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn postgrant_control_loss_terminates_authenticated_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        let initial_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&initial_ready.encode()).await.unwrap();
        let mut initial_session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut initial_session_ready).await.unwrap();
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000007",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut granted).await.unwrap();
        drop(control);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn postgrant_rom_reboot_terminates_before_save_relay() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        let initial_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&initial_ready.encode()).await.unwrap();
        let mut initial_session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut initial_session_ready).await.unwrap();
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000012",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut granted).await.unwrap();

        let reboot = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        bridge.write_all(&reboot.encode()).await.unwrap();
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn partial_authenticated_bridge_frame_terminates_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let bytes = ready.encode();
        bridge.write_all(&bytes[..1]).await.unwrap();
        assert!(
            timeout(BRIDGE_FRAME_TIMEOUT + Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn command_ledger_is_bounded_and_saturation_terminates_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;

        fill_command_ledger(&mut control).await;
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        let overflow_id = format!("00000000-0000-4000-8000-{:012x}", MAX_COMMAND_HISTORY + 1);
        send_command(&mut control, &abort(&overflow_id, TEST_SESSION_EPOCH, 1)).await;
        // The capacity check happens before the abort side effect: the ready
        // checkpoint remains pending and no bridge frame is emitted.
        assert!(
            timeout(Duration::from_secs(2), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn full_command_ledger_rejects_grant_before_rom_side_effect() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;
        fill_command_ledger(&mut control).await;

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        let overflow_id = format!("00000000-0000-4000-8000-{:012x}", MAX_COMMAND_HISTORY + 2);
        send_command(&mut control, &grant(&overflow_id, TEST_SESSION_EPOCH, 1)).await;
        assert!(
            timeout(Duration::from_secs(2), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        assert_eq!(bridge.read(&mut bytes).await.unwrap(), 0);
    }

    #[test]
    fn split_sequence_inspection_does_not_commit_before_handoff() {
        let frame =
            BridgeFrame::new(MessageType::CheckpointReady, 4, TEST_SESSION_EPOCH, &[]).unwrap();
        let mut state = SessionSequenceState::default();
        assert!(state.inspect_rom_frame(&frame, TEST_SESSION_EPOCH));
        assert!(state.inspect_rom_frame(&frame, TEST_SESSION_EPOCH));
        state.commit_rom_frame(&frame);
        assert!(!state.inspect_rom_frame(&frame, TEST_SESSION_EPOCH));
    }

    #[test]
    fn sidecar_sequence_wrap_skips_reserved_zero() {
        let mut state = SessionSequenceState {
            next_sidecar: u32::MAX,
            ..SessionSequenceState::default()
        };
        assert_eq!(state.take_sidecar_sequence(), u32::MAX);
        assert_eq!(state.take_sidecar_sequence(), 1);
    }

    #[test]
    fn expired_command_classification_is_stable_for_timer_or_command_order() {
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        // The exact tombstone classifies the expired key regardless of whether
        // the timer or command branch observed the deadline first.
        assert!(command_is_expired(Some(key), Some(key)));
        assert!(!command_is_expired(Some(key), None));
        assert!(!command_is_expired(
            Some(CheckpointKey::new(TEST_SESSION_EPOCH, 8).unwrap()),
            Some(key),
        ));
    }

    #[test]
    fn expired_tombstone_rotates_after_newer_serial_even_across_wrap() {
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, u32::MAX).unwrap();
        let mut tombstone = Some(key);
        let wrapped =
            BridgeFrame::new(MessageType::PlayerState, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        rotate_expired_tombstone(&wrapped, &mut tombstone);
        assert_eq!(tombstone, None);
    }

    #[test]
    fn control_write_failures_are_reconnectable_before_grant() {
        assert!(can_reconnect_after(&SidecarError::Control(
            ControlError::WriteConnection(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "test control disconnect",
            )),
        )));
        assert!(can_reconnect_after(&SidecarError::Control(
            ControlError::WriteTimeout,
        )));
    }

    #[test]
    fn bridge_rst_before_first_frame_reconnects_but_partial_frame_loss_is_fatal() {
        assert!(can_reconnect_after(&SidecarError::BridgeConnection(
            io::Error::new(io::ErrorKind::ConnectionReset, "test bridge RST"),
        )));
        assert!(!can_reconnect_after(&SidecarError::Connection(
            io::Error::new(io::ErrorKind::ConnectionReset, "partial frame RST"),
        )));
    }

    #[test]
    fn bridge_write_failures_are_bounded_and_listener_failures_are_fatal() {
        assert!(!can_reconnect_after(&SidecarError::BridgeWriteConnection(
            io::Error::new(io::ErrorKind::BrokenPipe, "test bridge disconnect"),
        )));
        assert!(!can_reconnect_after(&SidecarError::BridgeWriteTimeout));
        assert!(!can_reconnect_after(&SidecarError::HandshakeWriteTimeout));
        assert!(!can_reconnect_after(&SidecarError::Control(
            ControlError::Listener(io::Error::other("test control listener failure")),
        )));
        assert!(!can_reconnect_after(&SidecarError::CheckpointTimeout));
    }

    #[test]
    fn staged_ready_waits_for_handoff_before_starting_decision_window() {
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        assert_eq!(
            CheckpointState::ReadyPendingHandoff { key }.deadline(),
            None
        );
        assert_eq!(
            CheckpointState::ExpiryPendingHandoff { key }.deadline(),
            None
        );
        assert!(
            CheckpointState::AwaitDecision {
                key,
                deadline: Instant::now(),
            }
            .deadline()
            .is_some()
        );
    }

    #[tokio::test]
    async fn pending_reboot_expiry_handoff_rearms_without_second_rom_ready() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::ExpiryPendingHandoff { key },
            expired_checkpoint: Some(key),
            acknowledged_rom_ready: false,
            rearm_after_reboot: true,
        };
        let (mut bridge, mut bridge_peer) = bridge_writer_pair().await;
        let (mut control, mut control_peer) = control_writer_pair().await;

        sidecar
            .resume_checkpoint_handoff(&mut session, &mut bridge, &mut control)
            .await
            .unwrap();
        assert!(matches!(
            read_event(&mut control_peer).await,
            ControlEvent::CheckpointExpired {
                ready_sequence: 7,
                ..
            }
        ));
        let mut frame_bytes = [0; BRIDGE_FRAME_SIZE];
        bridge_peer.read_exact(&mut frame_bytes).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&frame_bytes, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
        assert_eq!(session.checkpoint_state, CheckpointState::Idle);
        assert!(session.expired_checkpoint.is_none());
        assert!(!session.rearm_after_reboot);
        assert!(session.acknowledged_rom_ready);
        assert_eq!(sidecar.sequence_state.last_session_rom, 1);
    }

    #[tokio::test]
    async fn queued_epoch_zero_ready_rearms_pending_expiry_after_control_loss() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::ExpiryPendingHandoff { key },
            expired_checkpoint: Some(key),
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        let (mut bridge, mut bridge_peer) = bridge_writer_pair().await;
        let (mut control, mut control_peer) = control_writer_pair().await;
        let reboot = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();

        // This models the exact ordering in which timer expiry staged the
        // durable handoff, its control write failed, and the reader then
        // delivered the already-authenticated reboot from its bounded queue.
        sidecar
            .handle_bridge_frame(reboot, &mut bridge, &mut control, &mut session)
            .await
            .unwrap();
        assert_eq!(
            session.checkpoint_state,
            CheckpointState::ExpiryPendingHandoff { key }
        );
        assert_eq!(session.expired_checkpoint, Some(key));
        assert!(session.rearm_after_reboot);
        let mut unexpected = [0; BRIDGE_FRAME_SIZE];
        assert!(
            timeout(
                Duration::from_millis(100),
                bridge_peer.read(&mut unexpected)
            )
            .await
            .is_err()
        );

        // Reconnect replays Expired once, then emits the replacement
        // SESSION_READY and finally rotates the active ROM cursor.
        sidecar
            .resume_checkpoint_handoff(&mut session, &mut bridge, &mut control)
            .await
            .unwrap();
        assert!(matches!(
            read_event(&mut control_peer).await,
            ControlEvent::CheckpointExpired {
                ready_sequence: 7,
                ..
            }
        ));
        let mut frame_bytes = [0; BRIDGE_FRAME_SIZE];
        bridge_peer.read_exact(&mut frame_bytes).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&frame_bytes, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
        assert_eq!(session.checkpoint_state, CheckpointState::Idle);
        assert!(session.expired_checkpoint.is_none());
        assert!(!session.rearm_after_reboot);
        assert!(session.acknowledged_rom_ready);
        assert_eq!(sidecar.sequence_state.last_session_rom, 1);

        let replacement =
            BridgeFrame::new(MessageType::CheckpointReady, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        sidecar
            .handle_bridge_frame(replacement, &mut bridge, &mut control, &mut session)
            .await
            .unwrap();
        assert!(matches!(
            read_event(&mut control_peer).await,
            ControlEvent::CheckpointReady {
                ready_sequence: 2,
                ..
            }
        ));
    }

    #[test]
    fn checkpoint_ready_commit_never_rewinds_a_later_rom_cursor() {
        let mut sequence = SessionSequenceState {
            last_session_rom: 9,
            ..SessionSequenceState::default()
        };
        sequence.commit_checkpoint_ready(CheckpointKey::new(TEST_SESSION_EPOCH, 3).unwrap());
        assert_eq!(sequence.last_session_rom, 9);
        sequence.commit_checkpoint_ready(CheckpointKey::new(TEST_SESSION_EPOCH, 10).unwrap());
        assert_eq!(sequence.last_session_rom, 10);
    }

    #[tokio::test]
    async fn bounded_bridge_handshake_rejects_oversize_and_unknown_fields() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept().await });
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        stream
            .write_all(&vec![b'x'; MAX_HANDSHAKE_BYTES + 1])
            .await
            .unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::HandshakeTooLarge)
        ));

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept().await });
        let line = format!(
            "{{\"secret\":\"{}\",\"bridge_abi\":1,\"protocol_version\":1,\"extra\":true}}\n",
            descriptor.secret()
        );
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::MalformedHandshake(_))
        ));
    }

    #[tokio::test]
    async fn authenticated_control_oversize_and_partial_commands_terminate_session() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        control
            .write_all(&vec![b'x'; MAX_CONTROL_LINE_BYTES + 1])
            .await
            .unwrap();
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );

        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());
        let mut control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        control.write_all(b"{\"type\":").await.unwrap();
        // Keep the authenticated connection open: the decoder must report a
        // bounded read timeout rather than confusing an incomplete line with
        // an immediate peer disconnect.
        assert!(
            timeout(Duration::from_secs(4), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }
}

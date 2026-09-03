use std::{
    collections::{HashMap, VecDeque},
    fmt,
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use coop_protocol::{LocalPresenceStateV1, PresenceInteractionV1};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Notify, mpsc, oneshot, watch},
    task::{AbortHandle, Id, JoinHandle, JoinSet},
    time::{Instant, sleep_until, timeout, timeout_at},
};
use uuid::Uuid;

use crate::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BridgeFrame, Direction, FrameCodecError,
    GAME_PROTOCOL_VERSION, MessageType,
    control::{
        CONTROL_PROTOCOL_VERSION, CheckpointAbort, CheckpointGrant, CheckpointKey, CommandId,
        CommandReason, CommandStatus, ControlCommand, ControlConnection, ControlError,
        ControlEvent, ControlListener, ControlWriter, MAX_CONTROL_LINE_BYTES, ShutdownRequest,
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
const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_DESCRIPTOR_BYTES: usize = 512;
const MAX_COMMAND_HISTORY: usize = 1024;
const MAX_DEFERRED_ROUTINE_COMMANDS: usize = 16;
const MAX_DEFERRED_COMMANDS: usize = MAX_DEFERRED_ROUTINE_COMMANDS + 1;
/// The owner realtime pump is allowed to have at most one bounded batch of
/// lifecycle records in flight.  Lifecycle records are ordered and are never
/// coalesced or discarded at this boundary.
pub const MAX_PRESENCE_COMMANDS: usize = 32;
const MAX_CONTROL_AUTH_CANDIDATES: usize = 16;
const MAX_BRIDGE_AUTH_CANDIDATES: usize = 16;

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
    #[error("pre-bridge command queue reached its bounded capacity")]
    DeferredCommandQueueFull,
    #[error("authenticated presence mailbox reached its bounded capacity")]
    PresenceQueueFull,
    #[error("authenticated shutdown exceeded its five-second grace period")]
    ShutdownTimeout,
}

/// The authenticated, frame-oriented view of a local TCP connection.
pub(crate) struct AuthenticatedConnection {
    stream: TcpStream,
}

impl AuthenticatedConnection {
    async fn send_handshake_accepted(&mut self) -> Result<(), SidecarError> {
        timeout(
            HANDSHAKE_TIMEOUT,
            self.stream.write_all(HANDSHAKE_ACCEPTED_LINE),
        )
        .await
        .map_err(|_| SidecarError::HandshakeWriteTimeout)?
        .map_err(SidecarError::Connection)
    }

    pub(crate) fn into_split(self) -> (BridgeReader, BridgeWriter) {
        let (reader, writer) = self.stream.into_split();
        (BridgeReader { stream: reader }, BridgeWriter::new(writer))
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
        self.receive_observed(expected_direction, || {}).await
    }

    async fn receive_observed<F>(
        &mut self,
        expected_direction: Direction,
        prefix_observer: F,
    ) -> Result<Option<BridgeFrame>, SidecarError>
    where
        F: FnOnce() + Send,
    {
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
        prefix_observer();
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeWriteClass {
    Critical,
    Lifecycle,
    Cutover,
    GenerationFence,
}

struct BridgeWriteRequest {
    bytes: [u8; BRIDGE_FRAME_SIZE],
    class: BridgeWriteClass,
    readiness_generation: u64,
    completion: Option<oneshot::Sender<Result<(), SidecarError>>>,
}

struct BridgeWriterState {
    failed: AtomicBool,
    failure: Mutex<Option<SidecarError>>,
    failure_signal: watch::Sender<bool>,
    _failure_observer: Mutex<watch::Receiver<bool>>,
    poisoned: AtomicBool,
    in_flight: AtomicBool,
    poison_notify: Notify,
    fence: Mutex<()>,
    requested_generation: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    test_write_gate: Mutex<Option<Arc<Notify>>>,
    #[cfg(test)]
    test_write_observer: Mutex<Option<TestWriterObserver>>,
    #[cfg(test)]
    test_failure_gate: Mutex<Option<Arc<Notify>>>,
}

#[cfg(test)]
#[derive(Clone)]
struct TestWriterObserver {
    admitted: Arc<AtomicBool>,
    admission_signal: Arc<Notify>,
    dropped: Arc<AtomicBool>,
    drop_signal: Arc<Notify>,
}

#[cfg(test)]
#[derive(Clone)]
struct TestBlockedWriterGate {
    release: Arc<Notify>,
    observer: TestWriterObserver,
}

#[cfg(test)]
#[derive(Clone)]
struct TestWriterFailureGate {
    trigger: Arc<Notify>,
    registered: Arc<Notify>,
    registration_seen: Arc<AtomicBool>,
}

#[cfg(test)]
struct WriterTaskDropSignal(Arc<BridgeWriterState>);

#[cfg(test)]
impl Drop for WriterTaskDropSignal {
    fn drop(&mut self) {
        if let Some(observer) = self
            .0
            .test_write_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            observer.dropped.store(true, Ordering::Release);
            observer.drop_signal.notify_waiters();
        }
    }
}

pub(crate) struct BridgeWriter {
    requests: Option<mpsc::Sender<BridgeWriteRequest>>,
    state: Arc<BridgeWriterState>,
    task: Option<JoinHandle<()>>,
}

impl BridgeWriter {
    #[allow(clippy::too_many_lines)]
    fn new(stream: OwnedWriteHalf) -> Self {
        let (requests, mut request_rx) =
            mpsc::channel::<BridgeWriteRequest>(MAX_PRESENCE_COMMANDS + 1);
        let (failure_signal, failure_observer) = watch::channel(false);
        let state = Arc::new(BridgeWriterState {
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
            failure_signal,
            _failure_observer: Mutex::new(failure_observer),
            poisoned: AtomicBool::new(false),
            in_flight: AtomicBool::new(false),
            poison_notify: Notify::new(),
            fence: Mutex::new(()),
            requested_generation: std::sync::atomic::AtomicU64::new(0),
            #[cfg(test)]
            test_write_gate: Mutex::new(None),
            #[cfg(test)]
            test_write_observer: Mutex::new(None),
            #[cfg(test)]
            test_failure_gate: Mutex::new(None),
        });
        let actor_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            #[cfg(test)]
            let _task_drop_signal = WriterTaskDropSignal(Arc::clone(&actor_state));
            let mut stream = stream;
            loop {
                #[cfg(test)]
                let request = {
                    let failure_gate = actor_state
                        .test_failure_gate
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone();
                    tokio::select! {
                        biased;
                        () = async move {
                            if let Some(trigger) = failure_gate {
                                trigger.notified().await;
                            } else {
                                std::future::pending::<()>().await;
                            }
                        } => {
                            let error = BridgeWriter::writer_terminated_error();
                            actor_state.record_failure(&error);
                            break;
                        }
                        request = request_rx.recv() => request,
                    }
                };
                #[cfg(not(test))]
                let request = request_rx.recv().await;
                let Some(request) = request else {
                    break;
                };
                let should_write = {
                    let _fence = actor_state
                        .fence
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if actor_state.poisoned.load(Ordering::Acquire)
                        || (request.class == BridgeWriteClass::Lifecycle
                            && request.readiness_generation
                                != actor_state.requested_generation.load(Ordering::Acquire))
                    {
                        false
                    } else if request.class == BridgeWriteClass::GenerationFence {
                        actor_state
                            .requested_generation
                            .store(request.readiness_generation, Ordering::Release);
                        false
                    } else {
                        if request.class == BridgeWriteClass::Lifecycle {
                            actor_state.in_flight.store(true, Ordering::Release);
                            #[cfg(test)]
                            if let Some(observer) = actor_state
                                .test_write_observer
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .as_ref()
                            {
                                observer.admitted.store(true, Ordering::Release);
                                observer.admission_signal.notify_waiters();
                            }
                        }
                        true
                    }
                };

                if !should_write {
                    if let Some(completion) = request.completion {
                        let result = if actor_state.poisoned.load(Ordering::Acquire) {
                            Err(BridgeWriter::writer_terminated_error())
                        } else {
                            Ok(())
                        };
                        let _ = completion.send(result);
                    }
                    continue;
                }

                let in_flight = request.class == BridgeWriteClass::Lifecycle;
                #[cfg(test)]
                let test_write_gate = (in_flight).then(|| {
                    actor_state
                        .test_write_gate
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone()
                });
                #[cfg(test)]
                if let Some(Some(gate)) = test_write_gate {
                    tokio::select! {
                        () = gate.notified() => {}
                        () = actor_state.poison_notify.notified() => {
                            let error = BridgeWriter::writer_terminated_error();
                            actor_state.record_failure(&error);
                            if in_flight {
                                let _fence = actor_state
                                    .fence
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                actor_state.in_flight.store(false, Ordering::Release);
                                #[cfg(test)]
                                if let Some(observer) = actor_state
                                    .test_write_observer
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .as_ref()
                                {
                                    observer.admitted.store(false, Ordering::Release);
                                }
                            }
                            if let Some(completion) = request.completion {
                                let _ = completion.send(Err(error));
                            }
                            break;
                        }
                    }
                }
                let result = tokio::select! {
                    result = timeout(BRIDGE_FRAME_TIMEOUT, stream.write_all(&request.bytes)) => {
                        match result {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(error)) => Err(SidecarError::BridgeWriteConnection(error)),
                            Err(_) => Err(SidecarError::BridgeWriteTimeout),
                        }
                    }
                    () = actor_state.poison_notify.notified() => {
                        Err(BridgeWriter::writer_terminated_error())
                    }
                };
                if in_flight {
                    let _fence = actor_state
                        .fence
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    actor_state.in_flight.store(false, Ordering::Release);
                    #[cfg(test)]
                    if let Some(observer) = actor_state
                        .test_write_observer
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .as_ref()
                    {
                        observer.admitted.store(false, Ordering::Release);
                    }
                }
                if let Err(error) = &result {
                    actor_state.record_failure(error);
                }
                if let Some(completion) = request.completion {
                    let _ = completion.send(result);
                }
                if actor_state.failed.load(Ordering::Acquire) {
                    break;
                }
            }
        });
        Self {
            requests: Some(requests),
            state,
            task: Some(task),
        }
    }

    #[cfg(test)]
    fn new_blocked(stream: OwnedWriteHalf) -> (Self, Arc<Notify>) {
        let writer = Self::new(stream);
        let gate = Arc::new(Notify::new());
        *writer
            .state
            .test_write_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&gate));
        (writer, gate)
    }

    #[cfg(test)]
    fn install_test_write_gate(&self, gate: TestBlockedWriterGate) {
        *self
            .state
            .test_write_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(gate.release);
        *self
            .state
            .test_write_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(gate.observer);
    }

    #[cfg(test)]
    fn install_test_failure_gate(&self, gate: &TestWriterFailureGate) {
        *self
            .state
            .test_failure_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&gate.trigger));
    }

    fn writer_terminated_error() -> SidecarError {
        SidecarError::BridgeWriteConnection(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "bridge writer terminated",
        ))
    }

    fn write_failure(&self) -> Option<SidecarError> {
        self.state
            .failed
            .load(Ordering::Acquire)
            .then(Self::writer_terminated_error)
    }

    fn take_failure(&self) -> Option<SidecarError> {
        self.state
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    fn failure_receiver(&self) -> watch::Receiver<bool> {
        self.state.failure_signal.subscribe()
    }

    fn record_failure(&self, error: &SidecarError) {
        self.state.record_failure(error);
    }

    fn poison(&self, error: &SidecarError) {
        self.record_failure(error);
    }

    fn set_generation(&self, generation: u64) {
        let _fence = self
            .state
            .fence
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.state
            .requested_generation
            .store(generation, Ordering::Release);
    }

    async fn cutover(&mut self, generation: u64) -> Result<(), SidecarError> {
        {
            let _fence = self
                .state
                .fence
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.state.failed.load(Ordering::Acquire) {
                return Err(self
                    .take_failure()
                    .unwrap_or_else(Self::writer_terminated_error));
            }
            self.state
                .requested_generation
                .store(generation, Ordering::Release);
            if self.state.in_flight.load(Ordering::Acquire) {
                let error = Self::writer_terminated_error();
                self.poison(&error);
                return Err(self
                    .take_failure()
                    .unwrap_or_else(Self::writer_terminated_error));
            }
            if self.state.poisoned.load(Ordering::Acquire) {
                return Err(self
                    .take_failure()
                    .unwrap_or_else(Self::writer_terminated_error));
            }
        }
        let (completion_tx, completion_rx) = oneshot::channel();
        self.enqueue(
            BridgeWriteRequest {
                bytes: [0; BRIDGE_FRAME_SIZE],
                class: BridgeWriteClass::GenerationFence,
                readiness_generation: generation,
                completion: Some(completion_tx),
            },
            completion_rx,
        )
        .await
    }

    async fn send(
        &mut self,
        frame: &BridgeFrame,
        expected_direction: Direction,
    ) -> Result<(), SidecarError> {
        self.send_with_class(
            frame,
            expected_direction,
            BridgeWriteClass::Critical,
            self.state.requested_generation.load(Ordering::Acquire),
        )
        .await
    }

    async fn send_with_class(
        &mut self,
        frame: &BridgeFrame,
        expected_direction: Direction,
        class: BridgeWriteClass,
        readiness_generation: u64,
    ) -> Result<(), SidecarError> {
        frame.ensure_direction(expected_direction)?;
        let (completion_tx, completion_rx) = oneshot::channel();
        self.enqueue(
            BridgeWriteRequest {
                bytes: frame.encode(),
                class,
                readiness_generation,
                completion: Some(completion_tx),
            },
            completion_rx,
        )
        .await
    }

    fn send_presence_at(
        &mut self,
        frame: &BridgeFrame,
        expected_direction: Direction,
        readiness_generation: u64,
    ) -> Result<(), SidecarError> {
        frame.ensure_direction(expected_direction)?;
        if self.write_failure().is_some() {
            return Err(self
                .take_failure()
                .unwrap_or_else(Self::writer_terminated_error));
        }
        // Keep one slot reserved for critical bridge traffic. Presence is
        // transient and must never make a later SESSION_READY or checkpoint
        // frame wait for an unbounded lifecycle backlog.
        let requests = self
            .requests
            .as_ref()
            .ok_or_else(Self::writer_terminated_error)?
            .clone();
        if requests.capacity() <= 1 {
            return Err(SidecarError::PresenceQueueFull);
        }
        requests
            .try_send(BridgeWriteRequest {
                bytes: frame.encode(),
                class: BridgeWriteClass::Lifecycle,
                readiness_generation,
                completion: None,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => SidecarError::PresenceQueueFull,
                mpsc::error::TrySendError::Closed(_) => SidecarError::BridgeWriteConnection(
                    io::Error::new(io::ErrorKind::ConnectionAborted, "bridge writer terminated"),
                ),
            })
    }

    async fn enqueue(
        &mut self,
        request: BridgeWriteRequest,
        completion_rx: oneshot::Receiver<Result<(), SidecarError>>,
    ) -> Result<(), SidecarError> {
        let requests = self
            .requests
            .as_ref()
            .ok_or_else(Self::writer_terminated_error)?;
        if self.state.failed.load(Ordering::Acquire) {
            return Err(self
                .take_failure()
                .unwrap_or_else(Self::writer_terminated_error));
        }
        let deadline = Instant::now() + BRIDGE_FRAME_TIMEOUT;
        match timeout_at(deadline, requests.send(request)).await {
            Err(_) => {
                let error = SidecarError::BridgeWriteTimeout;
                self.poison(&error);
                return Err(SidecarError::BridgeWriteTimeout);
            }
            Ok(Err(_)) => return Err(Self::writer_terminated_error()),
            Ok(Ok(())) => {}
        }
        match timeout_at(deadline, completion_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Self::writer_terminated_error()),
            Err(_) => {
                let error = SidecarError::BridgeWriteTimeout;
                self.poison(&error);
                Err(SidecarError::BridgeWriteTimeout)
            }
        }
    }

    async fn shutdown(&mut self) {
        let error = Self::writer_terminated_error();
        self.poison(&error);
        self.requests.take();
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl BridgeWriterState {
    fn record_failure(&self, error: &SidecarError) {
        if !self.failed.swap(true, Ordering::AcqRel) {
            let mut failure = self
                .failure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if failure.is_none() {
                *failure = Some(match error {
                    SidecarError::BridgeWriteTimeout => SidecarError::BridgeWriteTimeout,
                    SidecarError::BridgeWriteConnection(source) => {
                        SidecarError::BridgeWriteConnection(io::Error::new(
                            source.kind(),
                            source.to_string(),
                        ))
                    }
                    _ => BridgeWriter::writer_terminated_error(),
                });
            }
            self.failure_signal.send_replace(true);
            self.poisoned.store(true, Ordering::Release);
            self.poison_notify.notify_waiters();
        }
    }
}

impl Drop for BridgeWriter {
    fn drop(&mut self) {
        let error = Self::writer_terminated_error();
        self.poison(&error);
        self.requests.take();
        if let Some(task) = &self.task {
            task.abort();
        }
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

/// Owns one authenticated control decoder from authentication until it is
/// transferred into the active bridge session. Keeping the task alive across
/// listener and handshake races makes partially received JSONL cancellation
/// safe; dropping the owner always aborts rather than detaches the task.
type ControlResult = Result<ControlCommand, SidecarError>;
type ControlReceiver = mpsc::Receiver<ControlResult>;
type PresenceResult = Result<StampedPresenceCommand, SidecarError>;
type PresenceReceiver = mpsc::Receiver<PresenceResult>;
#[cfg(test)]
type ControlReceiverPair<'a> = (&'a mut ControlReceiver, &'a mut PresenceReceiver);
type ControlIoParts = (
    ControlWriter,
    ControlReceiver,
    PresenceReceiver,
    Arc<AtomicBool>,
    Arc<Notify>,
    Arc<ControlTerminalState>,
    watch::Receiver<bool>,
    JoinHandle<()>,
);

struct ControlTerminalState {
    published: AtomicBool,
    error: Mutex<Option<SidecarError>>,
    signal: watch::Sender<bool>,
}

impl ControlTerminalState {
    fn new() -> Arc<Self> {
        let (signal, _) = watch::channel(false);
        Arc::new(Self {
            published: AtomicBool::new(false),
            error: Mutex::new(None),
            signal,
        })
    }

    fn publish(&self, error: SidecarError) {
        let mut slot = self
            .error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(error);
            self.published.store(true, Ordering::Release);
            let _ = self.signal.send(true);
        }
    }

    fn take_error(&self) -> Option<SidecarError> {
        self.error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    fn is_published(&self) -> bool {
        self.published.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
struct StampedPresenceCommand {
    command: ControlCommand,
    readiness_generation: u64,
}

struct ControlIo {
    writer: Option<ControlWriter>,
    receiver: Option<ControlReceiver>,
    presence_receiver: Option<PresenceReceiver>,
    presence_overflow: Arc<AtomicBool>,
    presence_overflow_notify: Arc<Notify>,
    terminal: Arc<ControlTerminalState>,
    terminal_receiver: watch::Receiver<bool>,
    task: Option<JoinHandle<()>>,
}

impl ControlIo {
    fn writer(&mut self) -> &mut ControlWriter {
        self.writer.as_mut().expect("control writer is owned")
    }

    #[cfg(test)]
    fn receiver(&mut self) -> &mut ControlReceiver {
        self.receiver.as_mut().expect("control receiver is owned")
    }

    #[cfg(test)]
    fn receivers(&mut self) -> ControlReceiverPair<'_> {
        let control = self.receiver.as_mut().expect("control receiver is owned");
        let presence = self
            .presence_receiver
            .as_mut()
            .expect("presence receiver is owned");
        (control, presence)
    }

    fn receivers_and_terminal(
        &mut self,
    ) -> (
        &mut ControlReceiver,
        &mut PresenceReceiver,
        &Arc<ControlTerminalState>,
        &mut watch::Receiver<bool>,
    ) {
        let control = self.receiver.as_mut().expect("control receiver is owned");
        let presence = self
            .presence_receiver
            .as_mut()
            .expect("presence receiver is owned");
        (
            control,
            presence,
            &self.terminal,
            &mut self.terminal_receiver,
        )
    }

    fn into_parts(mut self) -> ControlIoParts {
        (
            self.writer.take().expect("control writer is owned"),
            self.receiver.take().expect("control receiver is owned"),
            self.presence_receiver
                .take()
                .expect("presence receiver is owned"),
            self.presence_overflow.clone(),
            self.presence_overflow_notify.clone(),
            self.terminal.clone(),
            self.terminal_receiver.clone(),
            self.task.take().expect("control task is owned"),
        )
    }

    async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        self.writer = None;
        self.receiver = None;
        self.presence_receiver = None;
    }
}

impl Drop for ControlIo {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Owns an authenticated bridge decoder while the launcher control peer is
/// being acquired or replaced. Retained deadlines and bridge EOF remain
/// observable without ever cancelling a partially decoded frame.
struct BridgeIo {
    writer: Option<BridgeWriter>,
    receiver: Option<mpsc::Receiver<Result<BridgeFrame, SidecarError>>>,
    task: Option<JoinHandle<()>>,
    terminal: watch::Receiver<Option<BridgeTerminal>>,
    pending_frame: Option<Box<BridgeFrame>>,
}

struct BridgeIoParts {
    writer: BridgeWriter,
    receiver: mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
    task: JoinHandle<()>,
    terminal: watch::Receiver<Option<BridgeTerminal>>,
    pending_frame: Option<Box<BridgeFrame>>,
}

struct ActiveSessionIo {
    bridge_writer: BridgeWriter,
    control_writer: ControlWriter,
    bridge_rx: mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
    bridge_terminal: watch::Receiver<Option<BridgeTerminal>>,
    control_rx: mpsc::Receiver<Result<ControlCommand, SidecarError>>,
    presence_rx: PresenceReceiver,
    presence_overflow: Arc<AtomicBool>,
    presence_overflow_notify: Arc<Notify>,
    control_terminal: Arc<ControlTerminalState>,
    control_terminal_receiver: watch::Receiver<bool>,
}

type BridgeReacquisitionParts<'a> = (
    &'a mut BridgeWriter,
    &'a mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
    &'a mut watch::Receiver<Option<BridgeTerminal>>,
    &'a mut Option<Box<BridgeFrame>>,
);

enum ActiveSessionEvent {
    Deadline,
    BridgeWriterFailed,
    ControlTerminal,
    Control(Option<Result<ControlCommand, SidecarError>>),
    Presence(Option<PresenceResult>),
    PresenceOverflow,
    Bridge(Option<Result<BridgeFrame, SidecarError>>),
}

enum ControlGapEvent {
    BridgeWriterFailed,
    Control(Result<ControlConnection, SidecarError>),
    Bridge(Option<Result<BridgeFrame, SidecarError>>),
    BridgeTerminal(Option<BridgeTerminal>),
    Deadline,
}

enum ControlReacquisition {
    Control(ControlConnection),
    BridgeTerminated(SidecarError),
}

enum ControlRecovery {
    Control(ControlIo),
    Reconnect,
    Shutdown,
}

enum SessionProgress {
    Continue,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeTerminal {
    CleanEof,
    IdleDisconnect,
    Fatal,
}

impl BridgeTerminal {
    fn into_error(self) -> SidecarError {
        match self {
            Self::CleanEof => SidecarError::ProtocolViolation("bridge disconnected"),
            Self::IdleDisconnect => SidecarError::BridgeConnection(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "bridge disconnected while idle",
            )),
            Self::Fatal => SidecarError::Connection(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "bridge reader terminated",
            )),
        }
    }
}

impl BridgeIo {
    fn reacquisition_parts(&mut self) -> BridgeReacquisitionParts<'_> {
        (
            self.writer.as_mut().expect("bridge writer is owned"),
            self.receiver.as_mut().expect("bridge receiver is owned"),
            &mut self.terminal,
            &mut self.pending_frame,
        )
    }

    fn into_parts(mut self) -> BridgeIoParts {
        BridgeIoParts {
            writer: self.writer.take().expect("bridge writer is owned"),
            receiver: self.receiver.take().expect("bridge receiver is owned"),
            task: self.task.take().expect("bridge task is owned"),
            terminal: self.terminal.clone(),
            pending_frame: self.pending_frame.take(),
        }
    }

    async fn shutdown(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            writer.shutdown().await;
        }
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        self.writer = None;
        self.receiver = None;
        self.pending_frame = None;
    }
}

impl Drop for BridgeIo {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[cfg(test)]
fn spawn_control_reader(control: ControlConnection) -> ControlIo {
    spawn_control_reader_with_generation(control, Arc::new(std::sync::atomic::AtomicU64::new(0)))
}

fn spawn_control_reader_with_generation(
    control: ControlConnection,
    presence_generation: Arc<std::sync::atomic::AtomicU64>,
) -> ControlIo {
    let (mut control_reader, control_writer) = control.into_split();
    let (control_tx, control_rx) = mpsc::channel::<Result<ControlCommand, SidecarError>>(1);
    let (presence_tx, presence_rx) = mpsc::channel::<PresenceResult>(MAX_PRESENCE_COMMANDS);
    let presence_overflow = Arc::new(AtomicBool::new(false));
    let presence_overflow_notify = Arc::new(Notify::new());
    let overflow_signal = Arc::clone(&presence_overflow);
    let overflow_notify = Arc::clone(&presence_overflow_notify);
    let terminal = ControlTerminalState::new();
    let terminal_signal = Arc::clone(&terminal);
    let terminal_receiver = terminal.signal.subscribe();
    let control_task = tokio::spawn(async move {
        loop {
            match control_reader.receive_command().await {
                Ok(command) => {
                    if is_presence_command(&command) {
                        // Presence is ordered lifecycle state. A full lane is
                        // a terminal condition, not permission to lose one
                        // transition: the owner observes the signal after
                        // draining any already accepted records and tears
                        // down this authenticated session.
                        let stamped = StampedPresenceCommand {
                            command,
                            readiness_generation: presence_generation.load(Ordering::Acquire),
                        };
                        match presence_tx.try_send(Ok(stamped)) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                overflow_signal.store(true, Ordering::Release);
                                overflow_notify.notify_one();
                                break;
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                    } else if control_tx.send(Ok(command)).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    terminal_signal.publish(SidecarError::Control(error));
                    break;
                }
            }
        }
    });
    ControlIo {
        writer: Some(control_writer),
        receiver: Some(control_rx),
        presence_receiver: Some(presence_rx),
        presence_overflow,
        presence_overflow_notify,
        terminal,
        terminal_receiver,
        task: Some(control_task),
    }
}

#[cfg(test)]
fn spawn_control_reader_observed_with_generation<F>(
    control: ControlConnection,
    presence_generation: Arc<std::sync::atomic::AtomicU64>,
    mut prefix_observer: F,
) -> ControlIo
where
    F: FnMut() + Send + 'static,
{
    let (mut control_reader, control_writer) = control.into_split();
    let (control_tx, control_rx) = mpsc::channel::<Result<ControlCommand, SidecarError>>(1);
    let (presence_tx, presence_rx) = mpsc::channel::<PresenceResult>(MAX_PRESENCE_COMMANDS);
    let presence_overflow = Arc::new(AtomicBool::new(false));
    let presence_overflow_notify = Arc::new(Notify::new());
    let overflow_signal = Arc::clone(&presence_overflow);
    let overflow_notify = Arc::clone(&presence_overflow_notify);
    let terminal = ControlTerminalState::new();
    let terminal_signal = Arc::clone(&terminal);
    let terminal_receiver = terminal.signal.subscribe();
    let control_task = tokio::spawn(async move {
        loop {
            match control_reader
                .receive_command_observed(&mut prefix_observer)
                .await
            {
                Ok(command) => {
                    if is_presence_command(&command) {
                        // Keep this path nonblocking so a lifecycle flood
                        // cannot strand a later critical command behind the
                        // bounded lane. Saturation is fatal and observable.
                        let stamped = StampedPresenceCommand {
                            command,
                            readiness_generation: presence_generation.load(Ordering::Acquire),
                        };
                        match presence_tx.try_send(Ok(stamped)) {
                            Ok(()) => prefix_observer(),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                overflow_signal.store(true, Ordering::Release);
                                overflow_notify.notify_one();
                                break;
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                    } else if control_tx.send(Ok(command)).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    terminal_signal.publish(SidecarError::Control(error));
                    break;
                }
            }
        }
    });
    ControlIo {
        writer: Some(control_writer),
        receiver: Some(control_rx),
        presence_receiver: Some(presence_rx),
        presence_overflow,
        presence_overflow_notify,
        terminal,
        terminal_receiver,
        task: Some(control_task),
    }
}

fn spawn_bridge_reader(bridge: AuthenticatedConnection) -> BridgeIo {
    let (mut bridge_reader, bridge_writer) = bridge.into_split();
    let (bridge_tx, bridge_rx) = mpsc::channel::<Result<BridgeFrame, SidecarError>>(1);
    let (terminal_tx, terminal_rx) = watch::channel(None);
    let bridge_task = tokio::spawn(async move {
        loop {
            match bridge_reader.receive(Direction::RomToSidecar).await {
                Ok(Some(frame)) => {
                    if bridge_tx.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = terminal_tx.send(Some(BridgeTerminal::CleanEof));
                    let _ = bridge_tx
                        .send(Err(SidecarError::ProtocolViolation("bridge disconnected")))
                        .await;
                    break;
                }
                Err(error) => {
                    let terminal = if matches!(error, SidecarError::BridgeConnection(_)) {
                        BridgeTerminal::IdleDisconnect
                    } else {
                        BridgeTerminal::Fatal
                    };
                    let _ = terminal_tx.send(Some(terminal));
                    let _ = bridge_tx.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    BridgeIo {
        writer: Some(bridge_writer),
        receiver: Some(bridge_rx),
        task: Some(bridge_task),
        terminal: terminal_rx,
        pending_frame: None,
    }
}

#[cfg(test)]
fn spawn_bridge_reader_observed<F>(
    bridge: AuthenticatedConnection,
    mut prefix_observer: F,
) -> BridgeIo
where
    F: FnMut() + Send + 'static,
{
    let (mut bridge_reader, bridge_writer) = bridge.into_split();
    let (bridge_tx, bridge_rx) = mpsc::channel::<Result<BridgeFrame, SidecarError>>(1);
    let (terminal_tx, terminal_rx) = watch::channel(None);
    let bridge_task = tokio::spawn(async move {
        loop {
            match bridge_reader
                .receive_observed(Direction::RomToSidecar, &mut prefix_observer)
                .await
            {
                Ok(Some(frame)) => {
                    if bridge_tx.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = terminal_tx.send(Some(BridgeTerminal::CleanEof));
                    let _ = bridge_tx
                        .send(Err(SidecarError::ProtocolViolation("bridge disconnected")))
                        .await;
                    break;
                }
                Err(error) => {
                    let terminal = if matches!(error, SidecarError::BridgeConnection(_)) {
                        BridgeTerminal::IdleDisconnect
                    } else {
                        BridgeTerminal::Fatal
                    };
                    let _ = terminal_tx.send(Some(terminal));
                    let _ = bridge_tx.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    BridgeIo {
        writer: Some(bridge_writer),
        receiver: Some(bridge_rx),
        task: Some(bridge_task),
        terminal: terminal_rx,
        pending_frame: None,
    }
}

type BridgeAuthentication =
    Pin<Box<dyn Future<Output = Result<AuthenticatedConnection, SidecarError>> + Send + 'static>>;

type ControlAuthentication =
    Pin<Box<dyn Future<Output = Result<ControlConnection, SidecarError>> + Send + 'static>>;

enum LostControlRace {
    Control(ControlIo),
    Bridge(BridgeIo),
    InvalidBridge,
    Continue,
}

enum ReplacementArrival {
    Control(ControlIo),
    Bridge(BridgeIo),
    Retry,
}

enum ControlledAcquisitionStep {
    Bridge(BridgeIo),
    BridgeRejected,
    ControlLost,
    Continue,
    Shutdown,
}

enum PendingBridgeEvent {
    Control(Result<ControlConnection, SidecarError>),
    Bridge(Result<AuthenticatedConnection, SidecarError>),
    Deadline,
}

enum ListenerRaceEvent {
    Control(Result<ControlConnection, SidecarError>),
    Bridge(Result<AuthenticatedConnection, SidecarError>),
    Deadline,
}

enum ControlledBridgeEvent {
    Bridge(Result<AuthenticatedConnection, SidecarError>),
    ControlTerminal,
    Command(Option<Result<ControlCommand, SidecarError>>),
    Presence(Option<PresenceResult>),
    PresenceOverflow,
    Deadline,
}

#[cfg(test)]
enum ControlledListenerEvent {
    Bridge(Result<AuthenticatedConnection, SidecarError>),
    ControlTerminal,
    Command(Option<Result<ControlCommand, SidecarError>>),
    Presence(Option<PresenceResult>),
    PresenceOverflow,
    Deadline,
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
    bridge_listener: Arc<TcpListener>,
    bridge_address: SocketAddr,
    bridge_secret: SessionSecret,
    control_listener: ControlListener,
    session_epoch: u32,
    sequence_state: SessionSequenceState,
    command_history: HashMap<CommandId, CommandRecord>,
    applied_shutdown: Option<CommandId>,
    // A ROM reboot closes the launcher realtime generation.  This latch is
    // deliberately owned by the sidecar rather than a reconnectable session
    // state so a control-only or bridge reconnect cannot accidentally revive
    // stale lifecycle forwarding in the same local session.
    lifecycle_forwarding_disabled: bool,
    presence_generation: Arc<std::sync::atomic::AtomicU64>,
    #[cfg(test)]
    handshake_prefix_observer: Option<mpsc::Sender<()>>,
    #[cfg(test)]
    bridge_prefix_observer: Option<mpsc::Sender<()>>,
    #[cfg(test)]
    control_prefix_observer: Option<mpsc::Sender<()>>,
    #[cfg(test)]
    test_write_gate: Option<TestBlockedWriterGate>,
    #[cfg(test)]
    test_writer_failure_gate: Option<TestWriterFailureGate>,
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
            bridge_listener: Arc::new(bridge_listener),
            bridge_address,
            bridge_secret: SessionSecret::generate(),
            control_listener,
            session_epoch,
            sequence_state: SessionSequenceState::default(),
            command_history: HashMap::new(),
            applied_shutdown: None,
            lifecycle_forwarding_disabled: false,
            presence_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            #[cfg(test)]
            handshake_prefix_observer: None,
            #[cfg(test)]
            bridge_prefix_observer: None,
            #[cfg(test)]
            control_prefix_observer: None,
            #[cfg(test)]
            test_write_gate: None,
            #[cfg(test)]
            test_writer_failure_gate: None,
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

    #[cfg(test)]
    fn observe_reader_prefixes(
        &mut self,
    ) -> (mpsc::Receiver<()>, mpsc::Receiver<()>, mpsc::Receiver<()>) {
        let (handshake_tx, handshake_rx) = mpsc::channel(16);
        let (bridge_tx, bridge_rx) = mpsc::channel(16);
        let (control_tx, control_rx) = mpsc::channel(16);
        self.handshake_prefix_observer = Some(handshake_tx);
        self.bridge_prefix_observer = Some(bridge_tx);
        self.control_prefix_observer = Some(control_tx);
        (handshake_rx, bridge_rx, control_rx)
    }

    fn spawn_control_io(&self, control: ControlConnection) -> ControlIo {
        #[cfg(test)]
        if let Some(observer) = self.control_prefix_observer.clone() {
            return spawn_control_reader_observed_with_generation(
                control,
                Arc::clone(&self.presence_generation),
                move || {
                    let _ = observer.try_send(());
                },
            );
        }
        #[cfg(not(test))]
        debug_assert!(self.session_epoch != 0);
        spawn_control_reader_with_generation(control, Arc::clone(&self.presence_generation))
    }

    fn spawn_bridge_io(&self, bridge: AuthenticatedConnection) -> BridgeIo {
        #[cfg(test)]
        if let Some(observer) = self.bridge_prefix_observer.clone() {
            let io = spawn_bridge_reader_observed(bridge, move || {
                let _ = observer.try_send(());
            });
            if let Some(gate) = self.test_write_gate.clone() {
                io.writer
                    .as_ref()
                    .expect("bridge writer is owned")
                    .install_test_write_gate(gate);
            }
            if let Some(gate) = self.test_writer_failure_gate.as_ref() {
                io.writer
                    .as_ref()
                    .expect("bridge writer is owned")
                    .install_test_failure_gate(gate);
            }
            return io;
        }
        #[cfg(not(test))]
        debug_assert!(self.session_epoch != 0);
        let io = spawn_bridge_reader(bridge);
        #[cfg(test)]
        if let Some(gate) = self.test_write_gate.clone() {
            io.writer
                .as_ref()
                .expect("bridge writer is owned")
                .install_test_write_gate(gate);
        }
        #[cfg(test)]
        if let Some(gate) = self.test_writer_failure_gate.as_ref() {
            io.writer
                .as_ref()
                .expect("bridge writer is owned")
                .install_test_failure_gate(gate);
        }
        io
    }

    #[cfg(test)]
    fn install_test_blocked_writer_gate(&mut self) -> TestBlockedWriterGate {
        let gate = TestBlockedWriterGate {
            release: Arc::new(Notify::new()),
            observer: TestWriterObserver {
                admitted: Arc::new(AtomicBool::new(false)),
                admission_signal: Arc::new(Notify::new()),
                dropped: Arc::new(AtomicBool::new(false)),
                drop_signal: Arc::new(Notify::new()),
            },
        };
        self.test_write_gate = Some(gate.clone());
        gate
    }

    #[cfg(test)]
    fn install_test_writer_failure_gate(&mut self) -> TestWriterFailureGate {
        let gate = TestWriterFailureGate {
            trigger: Arc::new(Notify::new()),
            registered: Arc::new(Notify::new()),
            registration_seen: Arc::new(AtomicBool::new(false)),
        };
        self.test_writer_failure_gate = Some(gate.clone());
        gate
    }

    /// Accepts exactly one authenticated bridge peer.
    ///
    /// # Errors
    ///
    /// Returns an error when accepting or authenticating the peer fails.
    #[cfg(test)]
    pub(crate) async fn accept(&mut self) -> Result<AuthenticatedConnection, SidecarError> {
        let (stream, peer) = self
            .bridge_listener
            .accept()
            .await
            .map_err(SidecarError::Listener)?;
        self.authenticate(stream, peer, true).await
    }

    #[cfg(test)]
    async fn accept_control(&self) -> Result<ControlConnection, SidecarError> {
        self.control_listener.accept().await.map_err(Into::into)
    }

    fn start_control_authentication(&self) -> ControlAuthentication {
        let listener = self.control_listener.clone();
        Box::pin(async move { authenticate_control_candidates(listener).await })
    }

    fn start_bridge_authentication_pump(&self) -> BridgeAuthentication {
        self.start_bridge_authentication_from(None)
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
        let mut reconnect = ReconnectContext::default();
        loop {
            let pair = self.accept_pair(reconnect).await?;
            let InitialAccept::Pair(pair) = pair else {
                return Ok(());
            };
            match self
                .serve_authenticated_controlled_client(pair.bridge, pair.control, pair.reconnect)
                .await?
            {
                SessionExit::Reconnect(next) => reconnect = next,
                SessionExit::Shutdown => return Ok(()),
            }
        }
    }

    async fn accept_pair(
        &mut self,
        mut reconnect: ReconnectContext,
    ) -> Result<InitialAccept, SidecarError> {
        // Control authentication is deliberately first: the launcher must own
        // the control secret before it starts mGBA and supplies the bridge Lua
        // script. The already-bound bridge listener can queue its peer meanwhile.
        let mut control_authentication = self.start_control_authentication();
        let control = loop {
            let deadline = reconnect.state.checkpoint_state.deadline();
            tokio::select! {
                biased;
                () = checkpoint_deadline(deadline) => {
                    advance_reconnect_deadline(
                        &mut self.sequence_state,
                        &mut reconnect.state,
                        Instant::now(),
                    )?;
                }
                result = &mut control_authentication => match result {
                    Ok(connection) => break connection,
                    Err(error) => return Err(error),
                }
            }
        };
        self.accept_initial_bridge_or_shutdown(self.spawn_control_io(control), reconnect)
            .await
    }

    async fn handoff_pair_acquisition_expiry(
        &mut self,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<bool, SidecarError> {
        let CheckpointState::ExpiryPendingHandoff { key } = reconnect.state.checkpoint_state else {
            return Ok(true);
        };
        let result = control
            .writer()
            .send_event(&ControlEvent::CheckpointExpired {
                session_epoch: key.session_epoch,
                ready_sequence: key.ready_sequence,
            })
            .await
            .map_err(SidecarError::Control);
        match result {
            Ok(()) => {
                reconnect.state.checkpoint_state = CheckpointState::Idle;
                Ok(true)
            }
            Err(error) if can_reconnect_after(&error) && is_control_reconnect_error(&error) => {
                control.shutdown().await;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    async fn handle_pair_acquisition_deadline(
        &mut self,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<ControlledAcquisitionStep, SidecarError> {
        advance_reconnect_deadline(
            &mut self.sequence_state,
            &mut reconnect.state,
            Instant::now(),
        )?;
        Ok(
            if self
                .handoff_pair_acquisition_expiry(control, reconnect)
                .await?
            {
                ControlledAcquisitionStep::Continue
            } else {
                ControlledAcquisitionStep::ControlLost
            },
        )
    }

    async fn reacquire_control_with_bridge(
        &mut self,
        bridge: &mut BridgeIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<ControlRecovery, SidecarError> {
        let result = {
            let (bridge_writer, bridge_rx, bridge_terminal, pending_bridge_frame) =
                bridge.reacquisition_parts();
            self.recover_control_with_bridge(
                bridge_writer,
                bridge_rx,
                bridge_terminal,
                &mut reconnect.state,
                pending_bridge_frame,
                &mut reconnect.bridge_lifecycle,
            )
            .await
        };
        if result.is_err() {
            bridge.shutdown().await;
        }
        result
    }

    async fn race_lost_control_with_bridge_auth(
        &mut self,
        authentication: &mut BridgeAuthentication,
        reconnect: &mut ReconnectContext,
    ) -> Result<LostControlRace, SidecarError> {
        let event = {
            let replacement = self.start_control_authentication();
            tokio::pin!(replacement);
            tokio::select! {
                biased;
                () = checkpoint_deadline(reconnect.state.checkpoint_state.deadline()) => {
                    PendingBridgeEvent::Deadline
                }
                result = &mut replacement => PendingBridgeEvent::Control(result),
                result = authentication => PendingBridgeEvent::Bridge(result),
            }
        };
        match event {
            PendingBridgeEvent::Deadline => {
                advance_reconnect_deadline(
                    &mut self.sequence_state,
                    &mut reconnect.state,
                    Instant::now(),
                )?;
                Ok(LostControlRace::Continue)
            }
            PendingBridgeEvent::Control(result) => result
                .map(|control| self.spawn_control_io(control))
                .map(LostControlRace::Control),
            PendingBridgeEvent::Bridge(Err(_)) => Ok(LostControlRace::InvalidBridge),
            PendingBridgeEvent::Bridge(Ok(mut connection)) => {
                reconnect.bridge_lifecycle.mark_authenticated();
                if connection.send_handshake_accepted().await.is_err() {
                    return Ok(LostControlRace::InvalidBridge);
                }
                if !reconnect.state.checkpoint_state.is_quiescing()
                    && self
                        .send_initial_session_ready(&mut connection)
                        .await
                        .is_err()
                {
                    return Ok(LostControlRace::InvalidBridge);
                }
                Ok(LostControlRace::Bridge(self.spawn_bridge_io(connection)))
            }
        }
    }

    async fn race_replacement_control_with_bridge_listener(
        &mut self,
        reconnect: &mut ReconnectContext,
    ) -> Result<ReplacementArrival, SidecarError> {
        let event = {
            let control_accept = self.start_control_authentication();
            let bridge_accept = self.start_bridge_authentication_pump();
            tokio::pin!(control_accept, bridge_accept);
            tokio::select! {
                biased;
                () = checkpoint_deadline(reconnect.state.checkpoint_state.deadline()) => {
                    ListenerRaceEvent::Deadline
                }
                result = &mut control_accept => ListenerRaceEvent::Control(result),
                result = &mut bridge_accept => ListenerRaceEvent::Bridge(result),
            }
        };
        match event {
            ListenerRaceEvent::Deadline => {
                advance_reconnect_deadline(
                    &mut self.sequence_state,
                    &mut reconnect.state,
                    Instant::now(),
                )?;
                Ok(ReplacementArrival::Retry)
            }
            ListenerRaceEvent::Control(Ok(control)) => {
                Ok(ReplacementArrival::Control(self.spawn_control_io(control)))
            }
            ListenerRaceEvent::Control(Err(error)) | ListenerRaceEvent::Bridge(Err(error)) => {
                Err(error)
            }
            ListenerRaceEvent::Bridge(Ok(connection)) => {
                let mut connection = connection;
                reconnect.bridge_lifecycle.mark_authenticated();
                if connection.send_handshake_accepted().await.is_err() {
                    return Ok(ReplacementArrival::Retry);
                }
                if !reconnect.state.checkpoint_state.is_quiescing()
                    && self
                        .send_initial_session_ready(&mut connection)
                        .await
                        .is_err()
                {
                    return Ok(ReplacementArrival::Retry);
                }
                // The bridge has been authenticated by the concurrent pump;
                // hand it to the normal reader owner before accepting frames.
                Ok(ReplacementArrival::Bridge(self.spawn_bridge_io(connection)))
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn drive_control_with_bridge_auth(
        &mut self,
        authentication: &mut BridgeAuthentication,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<ControlledAcquisitionStep, SidecarError> {
        if control.terminal.is_published() {
            let error = control
                .terminal
                .take_error()
                .unwrap_or(SidecarError::ProtocolViolation("control reader terminated"));
            if self
                .process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                .await?
            {
                return Ok(ControlledAcquisitionStep::Shutdown);
            }
            return Ok(if control.task.is_none() {
                ControlledAcquisitionStep::ControlLost
            } else {
                ControlledAcquisitionStep::Continue
            });
        }
        if control.presence_overflow.load(Ordering::Acquire) {
            return Err(SidecarError::PresenceQueueFull);
        }
        let presence_overflow_notify = Arc::clone(&control.presence_overflow_notify);
        let terminal_state = Arc::clone(&control.terminal);
        let event = {
            let (control_rx, presence_rx, _terminal, terminal_receiver) =
                control.receivers_and_terminal();
            tokio::select! {
                biased;
                changed = terminal_receiver.changed() => {
                    let _ = changed;
                    ControlledBridgeEvent::ControlTerminal
                }
                () = presence_overflow_notify.notified() => ControlledBridgeEvent::PresenceOverflow,
                () = checkpoint_deadline(reconnect.state.checkpoint_state.deadline()) => {
                    ControlledBridgeEvent::Deadline
                }
                command = control_rx.recv(),
                    if can_receive_deferred_command(&reconnect.deferred_commands)
                        || reconnect.state.checkpoint_state.is_quiescing() =>
                {
                    ControlledBridgeEvent::Command(command)
                }
                result = authentication => ControlledBridgeEvent::Bridge(result),
                presence = presence_rx.recv() => {
                    ControlledBridgeEvent::Presence(presence)
                }
            }
        };
        match event {
            ControlledBridgeEvent::ControlTerminal => {
                let error = terminal_state
                    .take_error()
                    .unwrap_or(SidecarError::ProtocolViolation("control reader terminated"));
                if self
                    .process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                    .await?
                {
                    Ok(ControlledAcquisitionStep::Shutdown)
                } else {
                    Ok(ControlledAcquisitionStep::ControlLost)
                }
            }
            ControlledBridgeEvent::Deadline => {
                self.handle_pair_acquisition_deadline(control, reconnect)
                    .await
            }
            ControlledBridgeEvent::Bridge(Err(_)) => Ok(ControlledAcquisitionStep::BridgeRejected),
            ControlledBridgeEvent::Bridge(Ok(mut connection)) => {
                reconnect.bridge_lifecycle.mark_authenticated();
                if connection.send_handshake_accepted().await.is_err() {
                    return Ok(ControlledAcquisitionStep::BridgeRejected);
                }
                if !reconnect.state.checkpoint_state.is_quiescing()
                    && self
                        .send_initial_session_ready(&mut connection)
                        .await
                        .is_err()
                {
                    return Ok(ControlledAcquisitionStep::BridgeRejected);
                }
                Ok(ControlledAcquisitionStep::Bridge(
                    self.spawn_bridge_io(connection),
                ))
            }
            ControlledBridgeEvent::Command(result) => {
                if control.presence_overflow.load(Ordering::Acquire) {
                    return Err(SidecarError::PresenceQueueFull);
                }
                if self
                    .process_pre_bridge_control_result(result, control, reconnect)
                    .await?
                {
                    return Ok(ControlledAcquisitionStep::Shutdown);
                }
                Ok(if control.task.is_none() {
                    ControlledAcquisitionStep::ControlLost
                } else {
                    ControlledAcquisitionStep::Continue
                })
            }
            ControlledBridgeEvent::Presence(result) => {
                if self
                    .process_pre_bridge_presence_result(result, control, reconnect, &terminal_state)
                    .await?
                {
                    return Ok(ControlledAcquisitionStep::Shutdown);
                }
                Ok(if control.task.is_none() {
                    ControlledAcquisitionStep::ControlLost
                } else {
                    ControlledAcquisitionStep::Continue
                })
            }
            ControlledBridgeEvent::PresenceOverflow => {
                if let Some(error) = terminal_state.take_error() {
                    self.process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                        .await
                        .map(|shutdown| {
                            if shutdown {
                                ControlledAcquisitionStep::Shutdown
                            } else {
                                ControlledAcquisitionStep::ControlLost
                            }
                        })
                } else {
                    Err(SidecarError::PresenceQueueFull)
                }
            }
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_lines)]
    async fn drive_control_with_bridge_listener(
        &mut self,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<ControlledAcquisitionStep, SidecarError> {
        if control.terminal.is_published() {
            let error = control
                .terminal
                .take_error()
                .unwrap_or(SidecarError::ProtocolViolation("control reader terminated"));
            if self
                .process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                .await?
            {
                return Ok(ControlledAcquisitionStep::Shutdown);
            }
            return Ok(if control.task.is_none() {
                ControlledAcquisitionStep::ControlLost
            } else {
                ControlledAcquisitionStep::Continue
            });
        }
        if control.presence_overflow.load(Ordering::Acquire) {
            return Err(SidecarError::PresenceQueueFull);
        }
        let presence_overflow_notify = Arc::clone(&control.presence_overflow_notify);
        let terminal_state = Arc::clone(&control.terminal);
        let bridge_authentication = self.start_bridge_authentication_pump();
        tokio::pin!(bridge_authentication);
        let event = {
            let (control_rx, presence_rx, _terminal, terminal_receiver) =
                control.receivers_and_terminal();
            tokio::select! {
                biased;
                changed = terminal_receiver.changed() => {
                    let _ = changed;
                    ControlledListenerEvent::ControlTerminal
                }
                () = presence_overflow_notify.notified() => ControlledListenerEvent::PresenceOverflow,
                () = checkpoint_deadline(
                    reconnect.state.checkpoint_state.deadline()
                ) => ControlledListenerEvent::Deadline,
                command = control_rx.recv(),
                    if can_receive_deferred_command(&reconnect.deferred_commands)
                        || reconnect.state.checkpoint_state.is_quiescing() =>
                {
                    ControlledListenerEvent::Command(command)
                }
                result = &mut bridge_authentication => ControlledListenerEvent::Bridge(result),
                presence = presence_rx.recv() => {
                    ControlledListenerEvent::Presence(presence)
                }
            }
        };
        match event {
            ControlledListenerEvent::ControlTerminal => {
                let error = terminal_state
                    .take_error()
                    .unwrap_or(SidecarError::ProtocolViolation("control reader terminated"));
                if self
                    .process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                    .await?
                {
                    Ok(ControlledAcquisitionStep::Shutdown)
                } else {
                    Ok(ControlledAcquisitionStep::ControlLost)
                }
            }
            ControlledListenerEvent::Deadline => {
                self.handle_pair_acquisition_deadline(control, reconnect)
                    .await
            }
            ControlledListenerEvent::Bridge(Err(error)) if is_listener_failure(&error) => {
                Err(error)
            }
            ControlledListenerEvent::Bridge(Err(_)) => {
                Ok(ControlledAcquisitionStep::BridgeRejected)
            }
            ControlledListenerEvent::Bridge(Ok(connection)) => {
                let mut connection = connection;
                reconnect.bridge_lifecycle.mark_authenticated();
                if connection.send_handshake_accepted().await.is_err() {
                    return Ok(ControlledAcquisitionStep::BridgeRejected);
                }
                if !reconnect.state.checkpoint_state.is_quiescing()
                    && self
                        .send_initial_session_ready(&mut connection)
                        .await
                        .is_err()
                {
                    return Ok(ControlledAcquisitionStep::BridgeRejected);
                }
                Ok(ControlledAcquisitionStep::Bridge(
                    self.spawn_bridge_io(connection),
                ))
            }
            ControlledListenerEvent::Command(result) => {
                if control.presence_overflow.load(Ordering::Acquire) {
                    return Err(SidecarError::PresenceQueueFull);
                }
                if self
                    .process_pre_bridge_control_result(result, control, reconnect)
                    .await?
                {
                    return Ok(ControlledAcquisitionStep::Shutdown);
                }
                Ok(if control.task.is_none() {
                    ControlledAcquisitionStep::ControlLost
                } else {
                    ControlledAcquisitionStep::Continue
                })
            }
            ControlledListenerEvent::Presence(result) => {
                if self
                    .process_pre_bridge_presence_result(result, control, reconnect, &terminal_state)
                    .await?
                {
                    return Ok(ControlledAcquisitionStep::Shutdown);
                }
                Ok(if control.task.is_none() {
                    ControlledAcquisitionStep::ControlLost
                } else {
                    ControlledAcquisitionStep::Continue
                })
            }
            ControlledListenerEvent::PresenceOverflow => {
                if let Some(error) = terminal_state.take_error() {
                    self.process_pre_bridge_control_result(Some(Err(error)), control, reconnect)
                        .await
                        .map(|shutdown| {
                            if shutdown {
                                ControlledAcquisitionStep::Shutdown
                            } else {
                                ControlledAcquisitionStep::ControlLost
                            }
                        })
                } else {
                    Err(SidecarError::PresenceQueueFull)
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn accept_initial_bridge_or_shutdown(
        &mut self,
        mut control: ControlIo,
        mut reconnect: ReconnectContext,
    ) -> Result<InitialAccept, SidecarError> {
        // The launcher authenticates before starting Lua, but it may request
        // shutdown during the window in which no bridge peer exists yet.
        // Keep the decoder task alive across every listener and handshake
        // race so a fragmented authenticated JSONL command is never lost.
        let mut bridge_authentication: Option<BridgeAuthentication> = None;
        let mut bridge: Option<BridgeIo> = None;
        let mut has_control = true;
        loop {
            if has_control {
                match self
                    .handoff_pair_acquisition_expiry(&mut control, &mut reconnect)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        has_control = false;
                        continue;
                    }
                    Err(error) => {
                        if let Some(mut pending_bridge) = bridge.take() {
                            pending_bridge.shutdown().await;
                        }
                        return Err(error);
                    }
                }
            }
            if has_control && let Some(bridge) = bridge.take() {
                reconnect.bridge_lifecycle.mark_authenticated();
                return Ok(InitialAccept::Pair(InitialPair {
                    bridge,
                    control,
                    reconnect,
                }));
            }

            if !has_control {
                if bridge.is_some() {
                    let recovery_result = {
                        let pending_bridge = bridge
                            .as_mut()
                            .expect("bridge is present for control reacquisition");
                        self.reacquire_control_with_bridge(pending_bridge, &mut reconnect)
                            .await
                    };
                    let recovery = match recovery_result {
                        Ok(recovery) => recovery,
                        Err(error) => {
                            if let Some(mut pending_bridge) = bridge.take() {
                                pending_bridge.shutdown().await;
                            }
                            return Err(error);
                        }
                    };
                    match recovery {
                        ControlRecovery::Control(replacement) => {
                            control = replacement;
                            has_control = true;
                        }
                        ControlRecovery::Reconnect => {
                            if let Some(mut pending_bridge) = bridge.take() {
                                pending_bridge.shutdown().await;
                            }
                        }
                        ControlRecovery::Shutdown => {
                            if let Some(mut pending_bridge) = bridge.take() {
                                pending_bridge.shutdown().await;
                            }
                            return Ok(InitialAccept::Shutdown);
                        }
                    }
                } else if let Some(authentication) = bridge_authentication.as_mut() {
                    match self
                        .race_lost_control_with_bridge_auth(authentication, &mut reconnect)
                        .await?
                    {
                        LostControlRace::Control(replacement) => {
                            control = replacement;
                            has_control = true;
                        }
                        LostControlRace::Bridge(connection) => {
                            bridge_authentication = None;
                            bridge = Some(connection);
                        }
                        LostControlRace::InvalidBridge => bridge_authentication = None,
                        LostControlRace::Continue => {}
                    }
                } else {
                    match self
                        .race_replacement_control_with_bridge_listener(&mut reconnect)
                        .await?
                    {
                        ReplacementArrival::Control(replacement) => {
                            control = replacement;
                            has_control = true;
                        }
                        ReplacementArrival::Bridge(bridge_connection) => {
                            bridge = Some(bridge_connection);
                        }
                        ReplacementArrival::Retry => {}
                    }
                }
                continue;
            }

            let step = if let Some(authentication) = bridge_authentication.as_mut() {
                self.drive_control_with_bridge_auth(authentication, &mut control, &mut reconnect)
                    .await?
            } else {
                bridge_authentication = Some(self.start_bridge_authentication_pump());
                self.drive_control_with_bridge_auth(
                    bridge_authentication
                        .as_mut()
                        .expect("bridge authentication pump is owned"),
                    &mut control,
                    &mut reconnect,
                )
                .await?
            };
            match step {
                ControlledAcquisitionStep::Bridge(connection) => {
                    bridge_authentication = None;
                    bridge = Some(connection);
                }
                ControlledAcquisitionStep::BridgeRejected => bridge_authentication = None,
                ControlledAcquisitionStep::ControlLost => has_control = false,
                ControlledAcquisitionStep::Continue => {}
                ControlledAcquisitionStep::Shutdown => return Ok(InitialAccept::Shutdown),
            }
        }
    }

    async fn process_pre_bridge_control_result(
        &mut self,
        result: Option<Result<ControlCommand, SidecarError>>,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<bool, SidecarError> {
        match result {
            Some(Ok(command)) => {
                let command_id = command_parts(&command).0;
                let can_replay_without_bridge = self.command_history.contains_key(&command_id);
                let handled = if reconnect.state.checkpoint_state.is_quiescing() {
                    let deadline = reconnect
                        .state
                        .checkpoint_state
                        .quiescing_deadline()
                        .expect("quiescing has a deadline");
                    let handling = self.handle_control_command_without_bridge(
                        command,
                        control.writer(),
                        &mut reconnect.state.checkpoint_state,
                    );
                    tokio::pin!(handling);
                    tokio::select! {
                        biased;
                        () = sleep_until(deadline) => Err(SidecarError::ShutdownTimeout),
                        result = &mut handling => result,
                    }
                } else if !can_replay_without_bridge
                    && (!matches!(command, ControlCommand::ShutdownRequest(_))
                        || (!reconnect.deferred_commands.is_empty()
                            && reconnect.deferred_commands.len() < MAX_DEFERRED_ROUTINE_COMMANDS))
                {
                    enqueue_deferred_command(&mut reconnect.deferred_commands, command)
                        .map(|()| false)
                } else {
                    self.handle_control_command_without_bridge(
                        command,
                        control.writer(),
                        &mut reconnect.state.checkpoint_state,
                    )
                    .await
                };
                let shutdown_acknowledged = match handled {
                    Ok(shutdown_acknowledged) => shutdown_acknowledged,
                    Err(error) => {
                        if can_reconnect_after(&error) && is_control_reconnect_error(&error) {
                            control.shutdown().await;
                            return Ok(false);
                        }
                        return Err(error);
                    }
                };
                if shutdown_acknowledged && reconnect.bridge_lifecycle.take_bridge_exit_proof() {
                    return Ok(true);
                }
                Ok(false)
            }
            Some(Err(error)) => {
                self.process_pre_bridge_control_error(error, control, reconnect)
                    .await
            }
            None => {
                if let Some(error) = control.terminal.take_error() {
                    return self
                        .process_pre_bridge_control_error(error, control, reconnect)
                        .await;
                }
                if control.presence_overflow.load(Ordering::Acquire) {
                    return Err(SidecarError::PresenceQueueFull);
                }
                control.shutdown().await;
                // A vanished decoder task is not transport EOF proof and
                // cannot authorize reconnect or clean shutdown.
                Err(SidecarError::ProtocolViolation("control reader terminated"))
            }
        }
    }

    async fn process_pre_bridge_control_error(
        &mut self,
        error: SidecarError,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
    ) -> Result<bool, SidecarError> {
        let clean_pre_bridge_shutdown = reconnect.state.checkpoint_state.is_quiescing()
            && reconnect.bridge_lifecycle.control_eof_proves_shutdown()
            && matches!(error, SidecarError::Control(ControlError::LineClosed));
        if clean_pre_bridge_shutdown {
            control.shutdown().await;
            return Ok(true);
        }
        if can_reconnect_after(&error) && is_control_reconnect_error(&error) {
            control.shutdown().await;
            return Ok(false);
        }
        Err(error)
    }

    async fn process_pre_bridge_presence_result(
        &mut self,
        result: Option<PresenceResult>,
        control: &mut ControlIo,
        reconnect: &mut ReconnectContext,
        terminal: &ControlTerminalState,
    ) -> Result<bool, SidecarError> {
        if let Some(error) = terminal.take_error() {
            return self
                .process_pre_bridge_control_error(error, control, reconnect)
                .await;
        }
        if control.presence_overflow.load(Ordering::Acquire) {
            return Err(SidecarError::PresenceQueueFull);
        }
        match result {
            Some(Ok(_)) => Ok(false),
            Some(Err(error)) => {
                if can_reconnect_after(&error) && is_control_reconnect_error(&error) {
                    control.shutdown().await;
                    return Ok(false);
                }
                Err(error)
            }
            None => {
                control.shutdown().await;
                // Presence EOF is a sibling-channel observation only. It is
                // never authoritative control EOF and cannot prove shutdown
                // or authorize a reconnect.
                Err(SidecarError::ProtocolViolation("control reader terminated"))
            }
        }
    }

    #[cfg(test)]
    async fn authenticate(
        &mut self,
        stream: TcpStream,
        peer: SocketAddr,
        send_bootstrap: bool,
    ) -> Result<AuthenticatedConnection, SidecarError> {
        let mut connection =
            authenticate_bridge_candidate(stream, peer, self.bridge_secret.clone()).await?;
        connection.send_handshake_accepted().await?;
        if send_bootstrap {
            self.send_initial_session_ready(&mut connection).await?;
        }
        Ok(connection)
    }

    fn start_bridge_authentication_from(
        &self,
        initial: Option<(TcpStream, SocketAddr)>,
    ) -> BridgeAuthentication {
        let listener = self.bridge_listener.clone();
        let secret = self.bridge_secret.clone();
        #[cfg(test)]
        let observer = self.handshake_prefix_observer.clone();
        #[cfg(not(test))]
        let observer = None;
        Box::pin(async move {
            let mut candidates = JoinSet::new();
            let mut candidate_handles: VecDeque<(Id, AbortHandle)> = VecDeque::new();
            let mut pending_candidate = None;
            if let Some((stream, peer)) = initial {
                track_authentication_candidate(
                    &mut candidate_handles,
                    spawn_bridge_authentication_candidate(
                        &mut candidates,
                        stream,
                        peer,
                        &secret,
                        observer.as_ref(),
                    ),
                );
            }
            let mut accept = Box::pin(listener.accept());
            loop {
                tokio::select! {
                    biased;
                    result = candidates.join_next_with_id(), if !candidates.is_empty() => {
                        let result = result.ok_or_else(|| {
                            SidecarError::Connection(io::Error::other(
                                "bridge authentication pump terminated",
                            ))
                        })?;
                        let (candidate_id, result) = match result {
                            Ok((candidate_id, result)) => (candidate_id, result),
                            Err(error) if error.is_cancelled() => {
                                remove_authentication_candidate(&mut candidate_handles, error.id());
                                if let Some((stream, peer)) = pending_candidate.take() {
                                    track_authentication_candidate(
                                        &mut candidate_handles,
                                        spawn_bridge_authentication_candidate(
                                            &mut candidates,
                                            stream,
                                            peer,
                                            &secret,
                                            observer.as_ref(),
                                        ),
                                    );
                                    accept = Box::pin(listener.accept());
                                }
                                continue;
                            }
                            Err(_) => {
                                return Err(SidecarError::Connection(io::Error::other(
                                    "bridge authentication task failed")));
                            }
                        };
                        remove_authentication_candidate(&mut candidate_handles, candidate_id);
                        if let Some((stream, peer)) = pending_candidate.take() {
                            track_authentication_candidate(
                                &mut candidate_handles,
                                spawn_bridge_authentication_candidate(
                                    &mut candidates,
                                    stream,
                                    peer,
                                    &secret,
                                    observer.as_ref(),
                                ),
                            );
                            accept = Box::pin(listener.accept());
                        }
                        match result {
                            Ok(connection) => return Ok(connection),
                            Err(error) if is_listener_failure(&error) => return Err(error),
                            Err(_) => {}
                        }
                    }
                    accepted = &mut accept, if pending_candidate.is_none() => {
                        let (stream, peer) = accepted.map_err(SidecarError::Listener)?;
                        if candidates.len() >= MAX_BRIDGE_AUTH_CANDIDATES {
                            abort_oldest_authentication(&mut candidate_handles);
                            pending_candidate = Some((stream, peer));
                        } else {
                            track_authentication_candidate(
                                &mut candidate_handles,
                                spawn_bridge_authentication_candidate(
                                    &mut candidates,
                                    stream,
                                    peer,
                                    &secret,
                                    observer.as_ref(),
                                ),
                            );
                        }
                        if pending_candidate.is_none() {
                            accept = Box::pin(listener.accept());
                        }
                    }
                }
            }
        })
    }

    async fn send_initial_session_ready(
        &mut self,
        connection: &mut AuthenticatedConnection,
    ) -> Result<(), SidecarError> {
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(MessageType::SessionReady, sequence, self.session_epoch, &[])?;
        let frame_bytes = frame.encode();
        timeout(HANDSHAKE_TIMEOUT, connection.stream.write_all(&frame_bytes))
            .await
            .map_err(|_| SidecarError::HandshakeWriteTimeout)?
            .map_err(SidecarError::Connection)
    }

    async fn send_session_ready_to_stream(
        &mut self,
        connection: &mut BridgeWriter,
    ) -> Result<(), SidecarError> {
        self.send_session_ready_to_stream_at(
            connection,
            self.presence_generation.load(Ordering::Acquire),
            BridgeWriteClass::Critical,
        )
        .await
    }

    async fn send_session_ready_to_stream_at(
        &mut self,
        connection: &mut BridgeWriter,
        readiness_generation: u64,
        class: BridgeWriteClass,
    ) -> Result<(), SidecarError> {
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(MessageType::SessionReady, sequence, self.session_epoch, &[])?;
        connection
            .send_with_class(&frame, Direction::SidecarToRom, class, readiness_generation)
            .await
    }

    fn acknowledge_rom_ready(&self, bridge: &BridgeWriter, session: &mut ActiveSessionState) {
        session.acknowledged_rom_ready = true;
        let generation = self.presence_generation.fetch_add(1, Ordering::AcqRel) + 1;
        bridge.set_generation(generation);
    }

    #[allow(clippy::too_many_lines)]
    async fn serve_authenticated_controlled_client(
        &mut self,
        bridge: BridgeIo,
        first_control: ControlIo,
        reconnect: ReconnectContext,
    ) -> Result<SessionExit, SidecarError> {
        // Dedicated decoder tasks outlive reconnectable control loss, so
        // in-flight bridge frames remain in one bounded channel.
        let BridgeIoParts {
            writer: bridge_writer,
            receiver: bridge_rx,
            task: bridge_task,
            terminal: bridge_terminal,
            pending_frame: pending_bridge_frame,
        } = bridge.into_parts();
        let mut reader_tasks = ReaderTasks::new_bridge(bridge_task);
        let mut bridge_writer = bridge_writer;
        let mut bridge_rx = bridge_rx;
        let mut bridge_terminal = bridge_terminal;
        let mut bridge_lifecycle = reconnect.bridge_lifecycle;
        bridge_lifecycle.mark_authenticated();
        let mut reconnect_state = reconnect.state;
        let mut deferred_commands = reconnect.deferred_commands;
        let mut control = first_control;
        let mut new_bridge_connection = true;
        let mut pending_bridge_frame = pending_bridge_frame;

        loop {
            let (
                control_writer,
                control_rx,
                presence_rx,
                presence_overflow,
                presence_overflow_notify,
                control_terminal,
                control_terminal_receiver,
                control_task,
            ) = control.into_parts();
            reader_tasks.set_control(control_task);

            let io = ActiveSessionIo {
                bridge_writer,
                control_writer,
                bridge_rx,
                bridge_terminal,
                control_rx,
                presence_rx,
                presence_overflow,
                presence_overflow_notify,
                control_terminal,
                control_terminal_receiver,
            };
            let (result, next_state, next_io) = self
                .run_checkpoint_session(
                    io,
                    reconnect_state,
                    &mut deferred_commands,
                    new_bridge_connection,
                    &mut pending_bridge_frame,
                )
                .await;
            bridge_writer = next_io.bridge_writer;
            bridge_rx = next_io.bridge_rx;
            bridge_terminal = next_io.bridge_terminal;
            reader_tasks.shutdown_control().await;

            match result {
                Ok(()) => {
                    bridge_writer.shutdown().await;
                    reader_tasks.shutdown().await;
                    return Ok(completed_session_exit(
                        next_state,
                        deferred_commands,
                        bridge_lifecycle,
                    ));
                }
                Err(error) if can_reconnect_control_loss(&error, next_state.checkpoint_state) => {
                    // Keep the bridge I/O alive while replacement control
                    // authenticates; its bounded channel retains in-flight frames.
                    reconnect_state = next_state;
                    // If the retained bridge has already published its
                    // terminal state, retire the pair before starting a new
                    // control authentication. Otherwise that in-flight
                    // authentication can be cancelled by the bridge EOF and
                    // the next control peer observes a spurious reset.
                    let retained_bridge_terminal = bridge_terminal.borrow().as_ref().copied();
                    if let Some(terminal) = retained_bridge_terminal {
                        let bridge_error = terminal.into_error();
                        bridge_lifecycle.record_termination(&bridge_error);
                        bridge_writer.shutdown().await;
                        reader_tasks.shutdown().await;
                        if reconnect_state.checkpoint_state.is_quiescing()
                            && is_bridge_shutdown_eof(&bridge_error)
                        {
                            return Ok(SessionExit::Shutdown);
                        }
                        if can_reconnect_bridge_loss(
                            &bridge_error,
                            reconnect_state.checkpoint_state,
                        ) {
                            return Ok(SessionExit::Reconnect(ReconnectContext {
                                state: reconnect_state,
                                deferred_commands,
                                bridge_lifecycle,
                            }));
                        }
                        return Err(bridge_error);
                    }
                    let recovery = self
                        .recover_control_with_bridge(
                            &mut bridge_writer,
                            &mut bridge_rx,
                            &mut bridge_terminal,
                            &mut reconnect_state,
                            &mut pending_bridge_frame,
                            &mut bridge_lifecycle,
                        )
                        .await;
                    match recovery {
                        Ok(ControlRecovery::Control(replacement)) => control = replacement,
                        Ok(ControlRecovery::Reconnect) => {
                            bridge_writer.shutdown().await;
                            reader_tasks.shutdown().await;
                            return Ok(SessionExit::Reconnect(ReconnectContext {
                                state: reconnect_state,
                                deferred_commands,
                                bridge_lifecycle,
                            }));
                        }
                        Ok(ControlRecovery::Shutdown) => {
                            bridge_writer.shutdown().await;
                            reader_tasks.shutdown().await;
                            return Ok(SessionExit::Shutdown);
                        }
                        Err(error) => {
                            bridge_writer.shutdown().await;
                            reader_tasks.shutdown().await;
                            return Err(error);
                        }
                    }
                    new_bridge_connection = false;
                }
                Err(error) if can_reconnect_bridge_loss(&error, next_state.checkpoint_state) => {
                    // A terminal bridge reader must be replaced, never polled again.
                    bridge_lifecycle.record_termination(&error);
                    bridge_writer.shutdown().await;
                    reader_tasks.shutdown().await;
                    return Ok(SessionExit::Reconnect(ReconnectContext {
                        state: next_state,
                        deferred_commands,
                        bridge_lifecycle,
                    }));
                }
                Err(error) => {
                    bridge_writer.shutdown().await;
                    reader_tasks.shutdown().await;
                    return Err(error);
                }
            }
        }
    }

    #[cfg(test)]
    async fn await_control_or_bridge(
        &mut self,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge_terminal: &mut watch::Receiver<Option<BridgeTerminal>>,
        reconnect: &mut ReconnectState,
        pending_bridge_frame: &mut Option<Box<BridgeFrame>>,
    ) -> Result<ControlReacquisition, SidecarError> {
        // Keep the replacement authentication future alive when a decision
        // deadline fires. Expiry staging must not cancel a valid fragmented
        // control handshake that is already in progress.
        let replacement = self.start_control_authentication();
        tokio::pin!(replacement);
        loop {
            let event = tokio::select! {
                biased;
                () = checkpoint_deadline(reconnect.checkpoint_state.deadline()) => {
                    ControlGapEvent::Deadline
                }
                result = &mut replacement => ControlGapEvent::Control(result),
                result = bridge_rx.recv(), if pending_bridge_frame.is_none() => {
                    ControlGapEvent::Bridge(result)
                }
                changed = bridge_terminal.changed() => {
                    ControlGapEvent::BridgeTerminal(
                        changed.ok().and_then(|()| *bridge_terminal.borrow())
                    )
                }
            };
            match event {
                ControlGapEvent::BridgeWriterFailed => unreachable!(
                    "the writer-failure event is only emitted by the writer-aware reacquisition"
                ),
                ControlGapEvent::Deadline => {
                    advance_reconnect_deadline(
                        &mut self.sequence_state,
                        reconnect,
                        Instant::now(),
                    )?;
                }
                ControlGapEvent::Control(result) => {
                    return result.map(ControlReacquisition::Control);
                }
                ControlGapEvent::BridgeTerminal(Some(terminal)) => {
                    return Ok(ControlReacquisition::BridgeTerminated(
                        terminal.into_error(),
                    ));
                }
                ControlGapEvent::BridgeTerminal(None) | ControlGapEvent::Bridge(None) => {
                    return Err(SidecarError::ProtocolViolation("bridge reader terminated"));
                }
                ControlGapEvent::Bridge(Some(Ok(_)))
                    if reconnect.checkpoint_state.is_quiescing() => {}
                ControlGapEvent::Bridge(Some(Ok(frame))) => {
                    if pending_bridge_frame.is_none() {
                        *pending_bridge_frame = Some(Box::new(frame));
                    }
                }
                ControlGapEvent::Bridge(Some(Err(error))) => {
                    return Ok(ControlReacquisition::BridgeTerminated(error));
                }
            }
        }
    }

    async fn await_control_or_bridge_with_writer(
        &mut self,
        bridge_writer: &mut BridgeWriter,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge_terminal: &mut watch::Receiver<Option<BridgeTerminal>>,
        reconnect: &mut ReconnectState,
        pending_bridge_frame: &mut Option<Box<BridgeFrame>>,
    ) -> Result<ControlReacquisition, SidecarError> {
        // Register the durable writer-failure watch before checking its
        // current state. A failure between a check and subscribe must not
        // leave control reacquisition waiting forever for replacement input.
        let mut writer_failure_receiver = bridge_writer.failure_receiver();
        let replacement = self.start_control_authentication();
        tokio::pin!(replacement);
        loop {
            if let Some(error) = bridge_writer
                .take_failure()
                .or_else(|| bridge_writer.write_failure())
            {
                return Ok(ControlReacquisition::BridgeTerminated(error));
            }
            let event = tokio::select! {
                biased;
                changed = writer_failure_receiver.changed() => {
                    let _ = changed;
                    ControlGapEvent::BridgeWriterFailed
                }
                () = checkpoint_deadline(reconnect.checkpoint_state.deadline()) => {
                    ControlGapEvent::Deadline
                }
                result = &mut replacement => ControlGapEvent::Control(result),
                result = bridge_rx.recv(), if pending_bridge_frame.is_none() => {
                    ControlGapEvent::Bridge(result)
                }
                changed = bridge_terminal.changed() => {
                    ControlGapEvent::BridgeTerminal(
                        changed.ok().and_then(|()| *bridge_terminal.borrow())
                    )
                }
            };
            match event {
                ControlGapEvent::BridgeWriterFailed => {
                    return Ok(ControlReacquisition::BridgeTerminated(
                        bridge_writer
                            .take_failure()
                            .or_else(|| bridge_writer.write_failure())
                            .unwrap_or_else(BridgeWriter::writer_terminated_error),
                    ));
                }
                ControlGapEvent::Deadline => {
                    advance_reconnect_deadline(
                        &mut self.sequence_state,
                        reconnect,
                        Instant::now(),
                    )?;
                }
                ControlGapEvent::Control(result) => {
                    // A reconnectable control EOF must not win a simultaneous
                    // writer-failure race and revive a retired bridge.
                    if let Some(error) = bridge_writer
                        .take_failure()
                        .or_else(|| bridge_writer.write_failure())
                    {
                        return Ok(ControlReacquisition::BridgeTerminated(error));
                    }
                    return result.map(ControlReacquisition::Control);
                }
                ControlGapEvent::BridgeTerminal(Some(terminal)) => {
                    return Ok(ControlReacquisition::BridgeTerminated(
                        terminal.into_error(),
                    ));
                }
                ControlGapEvent::BridgeTerminal(None) | ControlGapEvent::Bridge(None) => {
                    return Err(SidecarError::ProtocolViolation("bridge reader terminated"));
                }
                ControlGapEvent::Bridge(Some(Ok(_)))
                    if reconnect.checkpoint_state.is_quiescing() => {}
                ControlGapEvent::Bridge(Some(Ok(frame))) => {
                    if pending_bridge_frame.is_none() {
                        *pending_bridge_frame = Some(Box::new(frame));
                    }
                }
                ControlGapEvent::Bridge(Some(Err(error))) => {
                    return Ok(ControlReacquisition::BridgeTerminated(error));
                }
            }
        }
    }

    async fn recover_control_with_bridge(
        &mut self,
        bridge_writer: &mut BridgeWriter,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge_terminal: &mut watch::Receiver<Option<BridgeTerminal>>,
        reconnect: &mut ReconnectState,
        pending_bridge_frame: &mut Option<Box<BridgeFrame>>,
        bridge_lifecycle: &mut BridgeLifecycle,
    ) -> Result<ControlRecovery, SidecarError> {
        match self
            .await_control_or_bridge_with_writer(
                bridge_writer,
                bridge_rx,
                bridge_terminal,
                reconnect,
                pending_bridge_frame,
            )
            .await?
        {
            ControlReacquisition::Control(replacement) => {
                Ok(ControlRecovery::Control(self.spawn_control_io(replacement)))
            }
            ControlReacquisition::BridgeTerminated(error) => {
                bridge_lifecycle.record_termination(&error);
                if reconnect.checkpoint_state.is_quiescing() && is_bridge_shutdown_eof(&error) {
                    return Ok(ControlRecovery::Shutdown);
                }
                if can_reconnect_bridge_loss(&error, reconnect.checkpoint_state) {
                    return Ok(ControlRecovery::Reconnect);
                }
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn run_checkpoint_session(
        &mut self,
        mut io: ActiveSessionIo,
        reconnect_state: ReconnectState,
        deferred_commands: &mut VecDeque<ControlCommand>,
        new_bridge_connection: bool,
        pending_bridge_frame: &mut Option<Box<BridgeFrame>>,
    ) -> (Result<(), SidecarError>, ReconnectState, ActiveSessionIo) {
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
            let ActiveSessionIo {
                bridge_writer,
                control_writer,
                bridge_rx,
                bridge_terminal: _bridge_terminal,
                control_rx,
                presence_rx,
                presence_overflow,
                presence_overflow_notify,
                control_terminal,
                control_terminal_receiver,
            } = &mut io;
            self.prepare_active_session(
                bridge_rx,
                bridge_writer,
                control_writer,
                deferred_commands,
                &mut session,
                pending_bridge_frame,
                )
                .await?;
                loop {
                self.prepare_active_iteration(
                    bridge_rx,
                    bridge_writer,
                    control_writer,
                    deferred_commands,
                    &mut session,
                    )
                .await?;
                let mut writer_failure_receiver = bridge_writer.failure_receiver();
                #[cfg(test)]
                if let Some(gate) = self.test_writer_failure_gate.as_ref() {
                    gate.registration_seen.store(true, Ordering::Release);
                    gate.registered.notify_waiters();
                }
                if control_terminal.is_published()
                    || presence_overflow.load(Ordering::Acquire)
                    || bridge_writer.write_failure().is_some()
                {
                    return Err(take_active_terminal_error(
                        bridge_writer,
                        control_terminal,
                        presence_overflow.load(Ordering::Acquire),
                    ));
                }
                let deadline = session.checkpoint_state.deadline();
                let event = tokio::select! {
                    biased;
                    changed = control_terminal_receiver.changed() => {
                        let _ = changed;
                        ActiveSessionEvent::ControlTerminal
                    }
                    () = presence_overflow_notify.notified() => ActiveSessionEvent::PresenceOverflow,
                    changed = writer_failure_receiver.changed() => {
                        let _ = changed;
                        ActiveSessionEvent::BridgeWriterFailed
                    }
                    () = checkpoint_deadline(deadline) => ActiveSessionEvent::Deadline,
                    result = control_rx.recv(),
                        if can_receive_deferred_command(deferred_commands)
                            || session.checkpoint_state.is_quiescing() => {
                        ActiveSessionEvent::Control(result)
                    }
                    result = bridge_rx.recv() => ActiveSessionEvent::Bridge(result),
                    result = presence_rx.recv() => ActiveSessionEvent::Presence(result),
                };
                if control_terminal.is_published()
                    || presence_overflow.load(Ordering::Acquire)
                    || bridge_writer.write_failure().is_some()
                {
                    return Err(take_active_terminal_error(
                        bridge_writer,
                        control_terminal,
                        presence_overflow.load(Ordering::Acquire),
                    ));
                }
                match event {
                    ActiveSessionEvent::ControlTerminal | ActiveSessionEvent::BridgeWriterFailed => {
                        return Err(take_active_terminal_error(
                            bridge_writer,
                            control_terminal,
                            presence_overflow.load(Ordering::Acquire),
                        ));
                    }
                    ActiveSessionEvent::PresenceOverflow => {
                        return Err(take_active_terminal_error(
                            bridge_writer,
                            control_terminal,
                            true,
                        ));
                    }
                    ActiveSessionEvent::Deadline => {
                        self.handle_active_session_deadline(
                            control_writer,
                            bridge_writer,
                            deferred_commands,
                            &mut session,
                        )
                        .await?;
                    }
                    ActiveSessionEvent::Control(result) => {
                        self.handle_active_control_event(
                            result,
                            bridge_writer,
                            control_writer,
                            deferred_commands,
                            &mut session,
                        )
                        .await?;
                    }
                    ActiveSessionEvent::Presence(result) => {
                        self.handle_active_presence_event(
                            result,
                            bridge_writer,
                            &mut session,
                        )?;
                    }
                    ActiveSessionEvent::Bridge(result) => {
                        if matches!(
                            self.handle_active_bridge_event(
                                result,
                                bridge_writer,
                                control_writer,
                                deferred_commands,
                                &mut session,
                            )
                            .await?,
                            SessionProgress::Complete
                        ) {
                            break Ok(());
                        }
                    }
                }
            }
        }
        .await;
        (result, session.into_reconnect(), io)
    }

    async fn prepare_active_session(
        &mut self,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        deferred_commands: &mut VecDeque<ControlCommand>,
        session: &mut ActiveSessionState,
        pending_bridge_frame: &mut Option<Box<BridgeFrame>>,
    ) -> Result<(), SidecarError> {
        self.expire_checkpoint_if_due(session, control, Instant::now())
            .await?;
        self.resume_checkpoint_handoff(session, bridge, control)
            .await?;
        if session.checkpoint_state.is_quiescing() {
            pending_bridge_frame.take();
        } else {
            if let Some(frame) = pending_bridge_frame.take() {
                self.handle_bridge_frame(*frame, bridge, control, session)
                    .await?;
            }
            self.drain_queued_bridge_frames(bridge_rx, bridge, control, session)
                .await?;
        }
        self.drain_deferred_commands(deferred_commands, bridge, control, session)
            .await
    }

    async fn prepare_active_iteration(
        &mut self,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        deferred_commands: &mut VecDeque<ControlCommand>,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        if session.checkpoint_state.is_quiescing() {
            return Ok(());
        }
        self.expire_checkpoint_if_due(session, control, Instant::now())
            .await?;
        self.drain_queued_bridge_frames(bridge_rx, bridge, control, session)
            .await?;
        self.drain_deferred_commands(deferred_commands, bridge, control, session)
            .await
    }

    async fn handle_active_session_deadline(
        &mut self,
        control: &mut ControlWriter,
        bridge: &mut BridgeWriter,
        deferred_commands: &mut VecDeque<ControlCommand>,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        if session.checkpoint_state.is_quiescing() {
            return Err(SidecarError::ShutdownTimeout);
        }
        self.expire_checkpoint_if_due(session, control, Instant::now())
            .await?;
        self.drain_deferred_commands(deferred_commands, bridge, control, session)
            .await
    }

    async fn handle_active_control_event(
        &mut self,
        result: Option<Result<ControlCommand, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        deferred_commands: &mut VecDeque<ControlCommand>,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let Some(result) = result else {
            return Err(SidecarError::ProtocolViolation("control reader terminated"));
        };
        let command = match result {
            Ok(command) => command,
            Err(error) => return Err(error),
        };
        if is_presence_command(&command) {
            return self.handle_presence_command(command, bridge, session);
        }
        let (command_id, _, _, _) = command_parts(&command);
        let can_replay = self.command_history.contains_key(&command_id);
        if !deferred_commands.is_empty() && !can_replay {
            let shutdown_at_capacity = deferred_commands.len() == MAX_DEFERRED_ROUTINE_COMMANDS
                && matches!(command, ControlCommand::ShutdownRequest(_));
            if !shutdown_at_capacity {
                enqueue_deferred_command(deferred_commands, command)?;
                return self
                    .drain_deferred_commands(deferred_commands, bridge, control, session)
                    .await;
            }
        }
        if let Some(deadline) = session.checkpoint_state.quiescing_deadline() {
            let handling =
                self.handle_control_command(command, bridge, control, Instant::now(), session);
            tokio::pin!(handling);
            return tokio::select! {
                biased;
                () = sleep_until(deadline) => Err(SidecarError::ShutdownTimeout),
                result = &mut handling => result,
            };
        }
        self.handle_control_command(command, bridge, control, Instant::now(), session)
            .await
    }

    fn handle_active_presence_event(
        &mut self,
        result: Option<PresenceResult>,
        bridge: &mut BridgeWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let Some(result) = result else {
            return Err(SidecarError::ProtocolViolation("control reader terminated"));
        };
        let stamped = result?;
        self.handle_stamped_presence_command(stamped, bridge, session)
    }

    async fn handle_active_bridge_event(
        &mut self,
        result: Option<Result<BridgeFrame, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        deferred_commands: &mut VecDeque<ControlCommand>,
        session: &mut ActiveSessionState,
    ) -> Result<SessionProgress, SidecarError> {
        let Some(result) = result else {
            return Err(SidecarError::ProtocolViolation("bridge reader terminated"));
        };
        match result {
            Ok(_) if session.checkpoint_state.is_quiescing() => Ok(SessionProgress::Continue),
            Ok(frame) => {
                self.handle_bridge_frame(frame, bridge, control, session)
                    .await?;
                self.drain_deferred_commands(deferred_commands, bridge, control, session)
                    .await?;
                Ok(SessionProgress::Continue)
            }
            Err(error)
                if session.checkpoint_state.is_quiescing() && is_bridge_shutdown_eof(&error) =>
            {
                Ok(SessionProgress::Complete)
            }
            Err(error) => Err(error),
        }
    }

    async fn drain_deferred_commands(
        &mut self,
        deferred_commands: &mut VecDeque<ControlCommand>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        while let Some(command) = deferred_commands.front().cloned() {
            if deferred_command_waits_for_bridge_state(&command, session, self.session_epoch) {
                return Ok(());
            }
            if let Some(deadline) = session.checkpoint_state.quiescing_deadline() {
                let handling =
                    self.handle_control_command(command, bridge, control, Instant::now(), session);
                tokio::pin!(handling);
                tokio::select! {
                    biased;
                    () = sleep_until(deadline) => return Err(SidecarError::ShutdownTimeout),
                    result = &mut handling => result?,
                }
            } else {
                self.handle_control_command(command, bridge, control, Instant::now(), session)
                    .await?;
            }
            deferred_commands.pop_front();
        }
        Ok(())
    }

    async fn drain_queued_bridge_frames(
        &mut self,
        bridge_rx: &mut mpsc::Receiver<Result<BridgeFrame, SidecarError>>,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        // One frame matches the channel capacity and bounds each pre-select
        // drain pass. A producer cannot keep this helper busy indefinitely
        // and starve authenticated control or an absolute deadline.
        let result = match bridge_rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::error::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return Err(SidecarError::ProtocolViolation("bridge reader terminated"));
            }
        };
        let frame = result?;
        self.handle_bridge_frame(frame, bridge, control, session)
            .await
    }

    fn handle_presence_command(
        &mut self,
        command: ControlCommand,
        bridge: &mut BridgeWriter,
        session: &ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let stamped = StampedPresenceCommand {
            command,
            readiness_generation: self.presence_generation.load(Ordering::Acquire),
        };
        self.handle_stamped_presence_command(stamped, bridge, session)
    }

    fn handle_stamped_presence_command(
        &mut self,
        stamped: StampedPresenceCommand,
        bridge: &mut BridgeWriter,
        session: &ActiveSessionState,
    ) -> Result<(), SidecarError> {
        // Presence is best effort. It is intentionally rejected before ROM
        // readiness and once quiescing begins, without touching any command
        // ledger or deferred queue and without allocating an outer sequence.
        if self.lifecycle_forwarding_disabled
            || session.checkpoint_state.is_quiescing()
            || !session.acknowledged_rom_ready
            || stamped.readiness_generation != self.presence_generation.load(Ordering::Acquire)
        {
            return Ok(());
        }

        let StampedPresenceCommand {
            command,
            readiness_generation,
        } = stamped;
        let (message_type, payload) = match command {
            ControlCommand::RemotePlayerSpawn(value) => {
                (MessageType::RemotePlayerSpawn, value.encode().to_vec())
            }
            ControlCommand::RemotePlayerUpdate(value) => {
                (MessageType::RemotePlayerUpdate, value.encode().to_vec())
            }
            ControlCommand::RemotePlayerDespawn(value) => {
                (MessageType::RemotePlayerDespawn, value.encode().to_vec())
            }
            _ => return Err(SidecarError::ProtocolViolation("invalid presence command")),
        };
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(message_type, sequence, self.session_epoch, &payload)?;
        bridge.send_presence_at(&frame, Direction::SidecarToRom, readiness_generation)
    }

    async fn resume_checkpoint_handoff(
        &mut self,
        session: &mut ActiveSessionState,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
    ) -> Result<(), SidecarError> {
        // SESSION_READY is sent before the reset event. If the bounded event
        // write was ambiguous and control reconnects, retain the reboot mark
        // so the launcher still receives the one generation notification.
        if session.rearm_after_reboot
            && matches!(session.checkpoint_state, CheckpointState::Idle)
            && session.acknowledged_rom_ready
        {
            control.send_event(&ControlEvent::RomPresenceReset).await?;
            session.rearm_after_reboot = false;
        }
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
                    self.complete_reboot_rearm(bridge, control, session).await?;
                }
            }
            // An AwaitDecision event was already handed off before reconnect;
            // preserve its original deadline and avoid duplicate launcher
            // notifications. Only a failed ReadyPendingHandoff is replayed.
            CheckpointState::AwaitDecision { .. }
            | CheckpointState::Idle
            | CheckpointState::AwaitSaveData { .. }
            | CheckpointState::Quiescing { .. } => {}
        }
        Ok(())
    }

    async fn complete_reboot_rearm(
        &mut self,
        bridge: &mut BridgeWriter,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let next_generation = self.presence_generation.load(Ordering::Acquire) + 1;
        // Set the admission fence before allocating the new SESSION_READY.
        // Queued lifecycle frames from the retired generation are discarded
        // by the sole writer; an in-flight frame poisons the bridge.
        bridge.cutover(next_generation).await?;
        self.sequence_state.rearm_session_after_rom_reboot(1);
        session.acknowledged_rom_ready = false;
        // This write is bounded and fatal on failure. Only after the ROM has
        // received its new SESSION_READY may the old exact-key tombstone be
        // rotated, allowing sequence one in the new generation.
        self.send_session_ready_to_stream_at(bridge, next_generation, BridgeWriteClass::Cutover)
            .await?;
        self.acknowledge_rom_ready(bridge, session);
        session.rearm_after_reboot = true;
        session.expired_checkpoint = None;
        control.send_event(&ControlEvent::RomPresenceReset).await?;
        session.rearm_after_reboot = false;
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
            MessageType::PlayerState | MessageType::InteractRemotePlayer => {
                self.handle_presence_frame(frame, control, session).await?;
            }
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
                if frame.session_epoch() != self.session_epoch {
                    return Err(SidecarError::ProtocolViolation("invalid save-data update"));
                }
                let save_generation =
                    u32::from_le_bytes(frame.payload().try_into().map_err(|_| {
                        SidecarError::ProtocolViolation("invalid save-data update")
                    })?);
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
                        save_generation,
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
                    self.acknowledge_rom_ready(bridge, session);
                }
                rotate_expired_tombstone(&frame, &mut session.expired_checkpoint);
            }
        }
        Ok(())
    }

    async fn handle_presence_frame(
        &mut self,
        frame: BridgeFrame,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        if frame.session_epoch() != self.session_epoch {
            return Err(SidecarError::ProtocolViolation(
                "presence frame has wrong session epoch",
            ));
        }
        if !session.acknowledged_rom_ready {
            return Err(SidecarError::ProtocolViolation(
                "presence frame before rom ready",
            ));
        }
        if !self
            .sequence_state
            .inspect_rom_frame(&frame, self.session_epoch)
        {
            return Ok(());
        }

        let event = match frame.message_type() {
            MessageType::PlayerState => ControlEvent::PlayerState(
                LocalPresenceStateV1::decode(frame.payload())
                    .map_err(|_| SidecarError::ProtocolViolation("invalid player state"))?,
            ),
            MessageType::InteractRemotePlayer => ControlEvent::InteractRemotePlayer(
                PresenceInteractionV1::decode(frame.payload()).map_err(|_| {
                    SidecarError::ProtocolViolation("invalid remote player interaction")
                })?,
            ),
            _ => return Err(SidecarError::ProtocolViolation("invalid presence frame")),
        };

        // Commit before the bounded control write. A write with ambiguous
        // delivery must not cause the same transient frame to be replayed.
        self.sequence_state.commit_rom_frame(&frame);
        rotate_expired_tombstone(&frame, &mut session.expired_checkpoint);
        control.send_event(&event).await?;
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
            CheckpointState::AwaitSaveData { .. } => {
                self.lifecycle_forwarding_disabled = true;
                Err(SidecarError::ProtocolViolation(
                    "rom reboot during post-grant checkpoint",
                ))
            }
            CheckpointState::ReadyPendingHandoff { key }
            | CheckpointState::AwaitDecision { key, .. } => {
                self.lifecycle_forwarding_disabled = true;
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
                self.complete_reboot_rearm(bridge, control, session).await?;
                Ok(true)
            }
            CheckpointState::Idle
                if is_boot_epoch_reboot || self.sequence_state.last_session_rom > 1 =>
            {
                self.lifecycle_forwarding_disabled = true;
                session.rearm_after_reboot = true;
                self.complete_reboot_rearm(bridge, control, session).await?;
                Ok(true)
            }
            CheckpointState::ExpiryPendingHandoff { key } => {
                self.lifecycle_forwarding_disabled = true;
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
            CheckpointState::Quiescing { .. } => {
                self.lifecycle_forwarding_disabled = true;
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
        let (command_id, fingerprint, key, command_kind) = command_parts(&command);

        if self
            .replay_existing_command(command_id, fingerprint, control)
            .await?
        {
            return Ok(());
        }

        // Shutdown is deliberately handled before checkpoint expiry. A
        // request observed while any checkpoint state is non-idle is rejected
        // rather than completing or cancelling that checkpoint as a side
        // effect. Once quiescing, only exact replay is allowed to succeed.
        if matches!(command_kind, CommandKind::Shutdown) || session.checkpoint_state.is_quiescing()
        {
            return self
                .handle_shutdown_command(command, control, session)
                .await;
        }

        // Reserve routine capacity before any state transition, bridge frame,
        // or result event. The independent lifecycle slot is never consumed
        // by routine or rejected commands.
        self.ensure_command_ledger_capacity(false)?;

        self.expire_checkpoint_if_due(session, control, now).await?;

        let mut status = CommandStatus::Rejected;
        let mut reason = if command_is_expired(key, session.expired_checkpoint) {
            Some(CommandReason::Expired)
        } else {
            Some(CommandReason::WrongState)
        };
        if let Some(key) = key {
            if key.session_epoch == self.session_epoch {
                match (
                    session.checkpoint_state,
                    matches!(command_kind, CommandKind::Grant),
                ) {
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

    async fn handle_shutdown_command(
        &mut self,
        command: ControlCommand,
        control: &mut ControlWriter,
        session: &mut ActiveSessionState,
    ) -> Result<(), SidecarError> {
        let (command_id, fingerprint, _key, command_kind) = command_parts(&command);
        let command_epoch = command_epoch(&command);
        let applies = matches!(command_kind, CommandKind::Shutdown)
            && command_epoch == self.session_epoch
            && matches!(session.checkpoint_state, CheckpointState::Idle);
        self.ensure_command_ledger_capacity(applies)?;

        let (status, reason) = if applies {
            (CommandStatus::Applied, None)
        } else if matches!(command_kind, CommandKind::Shutdown)
            && command_epoch != self.session_epoch
        {
            (CommandStatus::Rejected, Some(CommandReason::WrongEpoch))
        } else {
            (CommandStatus::Rejected, Some(CommandReason::WrongState))
        };
        self.command_history.insert(
            command_id,
            CommandRecord {
                fingerprint,
                reason,
            },
        );
        if applies {
            // The lifecycle record and reservation precede both the state
            // transition and ACK. An ambiguous write can therefore replay
            // without applying shutdown twice.
            self.applied_shutdown = Some(command_id);
            session.checkpoint_state = CheckpointState::Quiescing {
                deadline: Instant::now() + SHUTDOWN_GRACE_TIMEOUT,
            };
        }
        control
            .send_event(&ControlEvent::CommandResult {
                command_id,
                status,
                reason,
            })
            .await?;
        Ok(())
    }

    async fn handle_control_command_without_bridge(
        &mut self,
        command: ControlCommand,
        control: &mut ControlWriter,
        checkpoint_state: &mut CheckpointState,
    ) -> Result<bool, SidecarError> {
        let (command_id, fingerprint, key, command_kind) = command_parts(&command);
        let replays_applied_shutdown = self.applied_shutdown == Some(command_id)
            && matches!(command_kind, CommandKind::Shutdown)
            && self
                .command_history
                .get(&command_id)
                .is_some_and(|previous| previous.fingerprint == fingerprint);
        if self
            .replay_existing_command(command_id, fingerprint, control)
            .await?
        {
            return Ok(replays_applied_shutdown);
        }

        let command_epoch = command_epoch(&command);
        let applied = matches!(command_kind, CommandKind::Shutdown)
            && command_epoch == self.session_epoch
            && matches!(*checkpoint_state, CheckpointState::Idle);
        self.ensure_command_ledger_capacity(applied)?;
        let (status, reason) = if applied {
            (CommandStatus::Applied, None)
        } else if command_epoch != self.session_epoch
            && (key.is_some() || matches!(command_kind, CommandKind::Shutdown))
        {
            (CommandStatus::Rejected, Some(CommandReason::WrongEpoch))
        } else if matches!(command_kind, CommandKind::Shutdown) {
            (CommandStatus::Rejected, Some(CommandReason::WrongState))
        } else if key.is_none() {
            (CommandStatus::Rejected, Some(CommandReason::InvalidPayload))
        } else {
            (CommandStatus::Rejected, Some(CommandReason::WrongState))
        };
        self.command_history.insert(
            command_id,
            CommandRecord {
                fingerprint,
                reason,
            },
        );
        if applied {
            self.applied_shutdown = Some(command_id);
            *checkpoint_state = CheckpointState::Quiescing {
                deadline: Instant::now() + SHUTDOWN_GRACE_TIMEOUT,
            };
        }
        control
            .send_event(&ControlEvent::CommandResult {
                command_id,
                status,
                reason,
            })
            .await?;
        Ok(applied)
    }

    fn ensure_command_ledger_capacity(
        &self,
        use_shutdown_reservation: bool,
    ) -> Result<(), SidecarError> {
        let routine_records = self
            .command_history
            .len()
            .saturating_sub(usize::from(self.applied_shutdown.is_some()));
        if routine_records < MAX_COMMAND_HISTORY
            || (use_shutdown_reservation && self.applied_shutdown.is_none())
        {
            Ok(())
        } else {
            Err(SidecarError::CommandLedgerFull)
        }
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
    Quiescing {
        deadline: Instant,
    },
}

impl CheckpointState {
    const fn deadline(self) -> Option<Instant> {
        match self {
            Self::Idle | Self::ReadyPendingHandoff { .. } | Self::ExpiryPendingHandoff { .. } => {
                None
            }
            Self::AwaitDecision { deadline, .. }
            | Self::AwaitSaveData { deadline, .. }
            | Self::Quiescing { deadline } => Some(deadline),
        }
    }

    const fn is_quiescing(self) -> bool {
        matches!(self, Self::Quiescing { .. })
    }

    const fn quiescing_deadline(self) -> Option<Instant> {
        match self {
            Self::Quiescing { deadline } => Some(deadline),
            _ => None,
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

#[derive(Default)]
struct ReconnectContext {
    state: ReconnectState,
    deferred_commands: VecDeque<ControlCommand>,
    bridge_lifecycle: BridgeLifecycle,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum BridgeLifecycle {
    #[default]
    NeverAuthenticated,
    AwaitingCleanEof,
    CleanEofObserved,
}

impl BridgeLifecycle {
    fn mark_authenticated(&mut self) {
        *self = Self::AwaitingCleanEof;
    }

    fn record_termination(&mut self, error: &SidecarError) {
        *self = if is_bridge_shutdown_eof(error) {
            Self::CleanEofObserved
        } else {
            Self::AwaitingCleanEof
        };
    }

    const fn control_eof_proves_shutdown(self) -> bool {
        !matches!(self, Self::AwaitingCleanEof)
    }

    const fn bridge_exit_proven(self) -> bool {
        matches!(self, Self::CleanEofObserved)
    }

    /// Consumes the one clean bridge EOF proof when a pre-bridge shutdown has
    /// delivered its Applied or Replayed result. A proof cannot be reused by a
    /// later connection, even though the terminal path normally exits
    /// immediately after this call.
    fn take_bridge_exit_proof(&mut self) -> bool {
        if self.bridge_exit_proven() {
            *self = Self::NeverAuthenticated;
            true
        } else {
            false
        }
    }
}

struct InitialPair {
    bridge: BridgeIo,
    control: ControlIo,
    reconnect: ReconnectContext,
}

#[allow(clippy::large_enum_variant)]
enum InitialAccept {
    Pair(InitialPair),
    Shutdown,
}

enum SessionExit {
    Reconnect(ReconnectContext),
    Shutdown,
}

fn completed_session_exit(
    state: ReconnectState,
    deferred_commands: VecDeque<ControlCommand>,
    bridge_lifecycle: BridgeLifecycle,
) -> SessionExit {
    if state.checkpoint_state.is_quiescing() {
        SessionExit::Shutdown
    } else {
        SessionExit::Reconnect(ReconnectContext {
            state,
            deferred_commands,
            bridge_lifecycle,
        })
    }
}

async fn authenticate_control_candidates(
    control_listener: ControlListener,
) -> Result<ControlConnection, SidecarError> {
    // Keep a bounded set of independent handshake owners. One incomplete
    // loopback candidate must not monopolize the listener while a valid
    // launcher reconnects, and dropping this JoinSet cancels every remaining
    // candidate when the caller's absolute deadline wins.
    let mut candidates = JoinSet::new();
    let mut candidate_handles: VecDeque<(Id, AbortHandle)> = VecDeque::new();
    let mut pending_candidate = None;
    let mut accept = Box::pin(control_listener.accept_stream());
    loop {
        tokio::select! {
            biased;
                    result = candidates.join_next_with_id(), if !candidates.is_empty() => {
                        let result = result.ok_or_else(|| {
                            SidecarError::Connection(io::Error::other(
                                "control authentication pump terminated",
                            ))
                        })?;
                        let (candidate_id, result) = match result {
                            Ok((candidate_id, result)) => (candidate_id, result),
                            Err(error) if error.is_cancelled() => {
                                remove_authentication_candidate(&mut candidate_handles, error.id());
                                if let Some((stream, peer)) = pending_candidate.take() {
                                    track_authentication_candidate(
                                        &mut candidate_handles,
                                        spawn_control_candidate(
                                            &mut candidates,
                                            &control_listener,
                                            stream,
                                            peer,
                                        ),
                                    );
                                    accept = Box::pin(control_listener.accept_stream());
                                }
                                continue;
                            }
                            Err(_) => {
                                return Err(SidecarError::Connection(io::Error::other(
                                    "control authentication task failed",
                                )));
                            }
                        };
                        remove_authentication_candidate(&mut candidate_handles, candidate_id);
                        if let Some((stream, peer)) = pending_candidate.take() {
                            track_authentication_candidate(
                                &mut candidate_handles,
                                spawn_control_candidate(
                                    &mut candidates,
                                    &control_listener,
                                    stream,
                                    peer,
                                ),
                            );
                    // The accept future completed while the bounded set was
                    // full. It must be replaced before the next select poll.
                    accept = Box::pin(control_listener.accept_stream());
                }
                match result {
                    Ok(connection) => return Ok(connection),
                    Err(error) if is_listener_failure(&error) => return Err(error),
                    Err(_) => {}
                }
            }
            accepted = &mut accept, if pending_candidate.is_none() => {
                let (stream, peer) = accepted?;
                if candidates.len() >= MAX_CONTROL_AUTH_CANDIDATES {
                    abort_oldest_authentication(&mut candidate_handles);
                    pending_candidate = Some((stream, peer));
                } else {
                    track_authentication_candidate(
                        &mut candidate_handles,
                        spawn_control_candidate(
                            &mut candidates,
                            &control_listener,
                            stream,
                            peer,
                        ),
                    );
                }
                if pending_candidate.is_none() {
                    accept = Box::pin(control_listener.accept_stream());
                }
            }
        }
    }
}

fn spawn_control_candidate(
    candidates: &mut JoinSet<Result<ControlConnection, SidecarError>>,
    control_listener: &ControlListener,
    stream: TcpStream,
    peer: SocketAddr,
) -> AbortHandle {
    let listener = control_listener.clone();
    candidates.spawn(async move {
        listener
            .authenticate_stream(stream, peer)
            .await
            .map_err(SidecarError::Control)
    })
}

fn is_listener_failure(error: &SidecarError) -> bool {
    matches!(
        error,
        SidecarError::Control(ControlError::Listener(_)) | SidecarError::Listener(_)
    )
}

fn take_active_terminal_error(
    bridge_writer: &mut BridgeWriter,
    control_terminal: &ControlTerminalState,
    presence_overflow: bool,
) -> SidecarError {
    let control_error = control_terminal.take_error();
    if let Some(error) = control_error {
        if is_control_reconnect_error(&error) {
            if presence_overflow {
                return SidecarError::PresenceQueueFull;
            }
            if let Some(writer_error) = bridge_writer
                .take_failure()
                .or_else(|| bridge_writer.write_failure())
            {
                return writer_error;
            }
        }
        return error;
    }
    if presence_overflow {
        return SidecarError::PresenceQueueFull;
    }
    bridge_writer
        .take_failure()
        .or_else(|| bridge_writer.write_failure())
        .unwrap_or(SidecarError::ProtocolViolation("control reader terminated"))
}

fn advance_reconnect_deadline(
    sequence_state: &mut SessionSequenceState,
    reconnect: &mut ReconnectState,
    now: Instant,
) -> Result<(), SidecarError> {
    match reconnect.checkpoint_state {
        CheckpointState::AwaitDecision { key, deadline } if now >= deadline => {
            reconnect.checkpoint_state = CheckpointState::ExpiryPendingHandoff { key };
            reconnect.expired_checkpoint = Some(key);
            sequence_state.commit_checkpoint_ready(key);
            Ok(())
        }
        CheckpointState::AwaitSaveData { deadline, .. } if now >= deadline => {
            Err(SidecarError::CheckpointTimeout)
        }
        CheckpointState::Quiescing { deadline } if now >= deadline => {
            Err(SidecarError::ShutdownTimeout)
        }
        _ => Ok(()),
    }
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

fn can_reconnect_control_loss(error: &SidecarError, state: CheckpointState) -> bool {
    can_reconnect_after(error)
        && is_control_reconnect_error(error)
        && matches!(
            state,
            CheckpointState::Idle
                | CheckpointState::ReadyPendingHandoff { .. }
                | CheckpointState::AwaitDecision { .. }
                | CheckpointState::ExpiryPendingHandoff { .. }
                | CheckpointState::Quiescing { .. }
        )
}

fn can_reconnect_bridge_loss(error: &SidecarError, state: CheckpointState) -> bool {
    can_reconnect_after(error)
        && matches!(
            state,
            CheckpointState::Idle
                | CheckpointState::ReadyPendingHandoff { .. }
                | CheckpointState::AwaitDecision { .. }
                | CheckpointState::ExpiryPendingHandoff { .. }
        )
}

fn is_bridge_shutdown_eof(error: &SidecarError) -> bool {
    matches!(
        error,
        SidecarError::ProtocolViolation("bridge disconnected")
    )
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
    Shutdown {
        session_epoch: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandKind {
    Grant,
    Abort,
    Shutdown,
}

fn command_parts(
    command: &ControlCommand,
) -> (
    CommandId,
    CommandFingerprint,
    Option<CheckpointKey>,
    CommandKind,
) {
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
            CommandKind::Grant,
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
            CommandKind::Abort,
        ),
        ControlCommand::ShutdownRequest(ShutdownRequest {
            command_id,
            session_epoch,
        }) => (
            *command_id,
            CommandFingerprint::Shutdown {
                session_epoch: *session_epoch,
            },
            None,
            CommandKind::Shutdown,
        ),
        ControlCommand::RemotePlayerSpawn(_)
        | ControlCommand::RemotePlayerUpdate(_)
        | ControlCommand::RemotePlayerDespawn(_) => {
            unreachable!("presence commands bypass command ledgers")
        }
    }
}

fn command_epoch(command: &ControlCommand) -> u32 {
    match command {
        ControlCommand::CheckpointGrant(command) => command.session_epoch,
        ControlCommand::CheckpointAbort(command) => command.session_epoch,
        ControlCommand::ShutdownRequest(command) => command.session_epoch,
        ControlCommand::RemotePlayerSpawn(_)
        | ControlCommand::RemotePlayerUpdate(_)
        | ControlCommand::RemotePlayerDespawn(_) => {
            unreachable!("presence commands have no session epoch")
        }
    }
}

fn is_presence_command(command: &ControlCommand) -> bool {
    matches!(
        command,
        ControlCommand::RemotePlayerSpawn(_)
            | ControlCommand::RemotePlayerUpdate(_)
            | ControlCommand::RemotePlayerDespawn(_)
    )
}

fn command_is_expired(
    key: Option<CheckpointKey>,
    expired_checkpoint: Option<CheckpointKey>,
) -> bool {
    key.is_some_and(|key| Some(key) == expired_checkpoint)
}

fn deferred_command_waits_for_bridge_state(
    command: &ControlCommand,
    session: &ActiveSessionState,
    session_epoch: u32,
) -> bool {
    let (key, is_grant) = match command {
        ControlCommand::CheckpointGrant(command) => (Some(command.key()), true),
        ControlCommand::CheckpointAbort(command) => (Some(command.key()), false),
        ControlCommand::ShutdownRequest(_) => (None, false),
        ControlCommand::RemotePlayerSpawn(_)
        | ControlCommand::RemotePlayerUpdate(_)
        | ControlCommand::RemotePlayerDespawn(_) => return false,
    };
    let Some(key) = key else {
        return false;
    };
    if key.session_epoch != session_epoch
        || CheckpointKey::new(key.session_epoch, key.ready_sequence).is_none()
        || Some(key) == session.expired_checkpoint
    {
        return false;
    }
    match session.checkpoint_state {
        CheckpointState::Idle => !session.acknowledged_rom_ready,
        CheckpointState::AwaitDecision { key: pending, .. } => {
            is_grant && key == pending && !session.acknowledged_rom_ready
        }
        _ => false,
    }
}

fn can_receive_deferred_command(deferred_commands: &VecDeque<ControlCommand>) -> bool {
    deferred_commands.len() < MAX_DEFERRED_ROUTINE_COMMANDS
        || (deferred_commands.len() == MAX_DEFERRED_ROUTINE_COMMANDS
            && !deferred_commands
                .iter()
                .any(|command| matches!(command, ControlCommand::ShutdownRequest(_))))
}

fn enqueue_deferred_command(
    deferred_commands: &mut VecDeque<ControlCommand>,
    command: ControlCommand,
) -> Result<(), SidecarError> {
    let within_routine_capacity = deferred_commands.len() < MAX_DEFERRED_ROUTINE_COMMANDS;
    let uses_shutdown_reservation = deferred_commands.len() == MAX_DEFERRED_ROUTINE_COMMANDS
        && matches!(command, ControlCommand::ShutdownRequest(_))
        && deferred_commands
            .iter()
            .all(|queued| !matches!(queued, ControlCommand::ShutdownRequest(_)));
    if !within_routine_capacity && !uses_shutdown_reservation {
        return Err(SidecarError::DeferredCommandQueueFull);
    }
    deferred_commands.push_back(command);
    debug_assert!(deferred_commands.len() <= MAX_DEFERRED_COMMANDS);
    Ok(())
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
    read_bounded_handshake_observed(stream, || {}).await
}

async fn read_bounded_handshake_observed<F>(
    stream: &mut TcpStream,
    prefix_observer: F,
) -> Result<Vec<u8>, SidecarError>
where
    F: FnOnce() + Send,
{
    let mut line = Vec::with_capacity(128);
    let mut byte = [0_u8; 1];
    let mut prefix_observer = Some(prefix_observer);
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
        if let Some(prefix_observer) = prefix_observer.take() {
            prefix_observer();
        }
    }
}

async fn authenticate_bridge_candidate(
    mut stream: TcpStream,
    peer: SocketAddr,
    expected_secret: SessionSecret,
) -> Result<AuthenticatedConnection, SidecarError> {
    if !peer.ip().is_loopback() {
        return Err(SidecarError::NonLoopbackPeer(peer));
    }
    stream.set_nodelay(true).map_err(SidecarError::Connection)?;
    let line = timeout(HANDSHAKE_TIMEOUT, read_bounded_handshake(&mut stream))
        .await
        .map_err(|_| SidecarError::HandshakeTimeout)??;
    complete_bridge_authentication(stream, &expected_secret, &line)
}

async fn authenticate_bridge_candidate_with_observer(
    stream: TcpStream,
    peer: SocketAddr,
    expected_secret: SessionSecret,
    observer: Option<mpsc::Sender<()>>,
) -> Result<AuthenticatedConnection, SidecarError> {
    #[cfg(test)]
    if let Some(observer) = observer {
        return authenticate_bridge_candidate_observed(stream, peer, expected_secret, move || {
            let _ = observer.try_send(());
        })
        .await;
    }
    #[cfg(not(test))]
    let _ = observer;
    authenticate_bridge_candidate(stream, peer, expected_secret).await
}

fn spawn_bridge_authentication_candidate(
    candidates: &mut JoinSet<Result<AuthenticatedConnection, SidecarError>>,
    stream: TcpStream,
    peer: SocketAddr,
    secret: &SessionSecret,
    observer: Option<&mpsc::Sender<()>>,
) -> AbortHandle {
    let expected_secret = secret.clone();
    let observer = observer.cloned();
    candidates.spawn(async move {
        authenticate_bridge_candidate_with_observer(stream, peer, expected_secret, observer).await
    })
}

fn track_authentication_candidate(handles: &mut VecDeque<(Id, AbortHandle)>, handle: AbortHandle) {
    handles.push_back((handle.id(), handle));
}

fn remove_authentication_candidate(handles: &mut VecDeque<(Id, AbortHandle)>, id: Id) {
    if let Some(index) = handles
        .iter()
        .position(|(candidate_id, _)| *candidate_id == id)
    {
        let _ = handles.remove(index);
    }
}

fn abort_oldest_authentication(handles: &mut VecDeque<(Id, AbortHandle)>) {
    if let Some((_, handle)) = handles.pop_front() {
        handle.abort();
    }
}

#[cfg(test)]
async fn authenticate_bridge_candidate_observed<F>(
    mut stream: TcpStream,
    peer: SocketAddr,
    expected_secret: SessionSecret,
    prefix_observer: F,
) -> Result<AuthenticatedConnection, SidecarError>
where
    F: FnOnce() + Send,
{
    if !peer.ip().is_loopback() {
        return Err(SidecarError::NonLoopbackPeer(peer));
    }
    stream.set_nodelay(true).map_err(SidecarError::Connection)?;
    let line = timeout(
        HANDSHAKE_TIMEOUT,
        read_bounded_handshake_observed(&mut stream, prefix_observer),
    )
    .await
    .map_err(|_| SidecarError::HandshakeTimeout)??;
    complete_bridge_authentication(stream, &expected_secret, &line)
}

fn complete_bridge_authentication(
    stream: TcpStream,
    expected_secret: &SessionSecret,
    line: &[u8],
) -> Result<AuthenticatedConnection, SidecarError> {
    let handshake: HandshakeRequest =
        serde_json::from_slice(line).map_err(SidecarError::MalformedHandshake)?;
    if handshake.bridge_abi != BRIDGE_ABI_VERSION {
        return Err(SidecarError::IncompatibleBridgeAbi {
            received: handshake.bridge_abi,
        });
    }
    if handshake.protocol_version != GAME_PROTOCOL_VERSION {
        return Err(SidecarError::IncompatibleProtocolVersion {
            received: handshake.protocol_version,
        });
    }
    if !expected_secret.matches(&handshake.secret) {
        return Err(SidecarError::AuthenticationFailed);
    }
    Ok(AuthenticatedConnection { stream })
}

#[cfg(test)]
mod tests {
    use coop_protocol::{
        CanonicalUsername, DespawnReason, LocalPresenceStateV1, PresenceHandle,
        RemotePlayerDespawnV1, RemotePlayerSpawnV1, RemotePlayerUpdateV1,
    };
    use serde::Serialize;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::oneshot,
        time::{Duration, timeout},
    };

    use super::*;

    const TEST_SESSION_EPOCH: u32 = 41;

    fn save_update(sequence: u32, generation: u32) -> BridgeFrame {
        BridgeFrame::new(
            MessageType::SaveDataUpdated,
            sequence,
            TEST_SESSION_EPOCH,
            &generation.to_le_bytes(),
        )
        .unwrap()
    }

    fn valid_local_presence_state() -> [u8; 28] {
        [
            1, 1, 0, 3, 0, 252, 255, 9, 0, 0, 7, 4, 68, 51, 34, 17, 136, 119, 102, 85, 2, 1, 2, 1,
            204, 187, 170, 153,
        ]
    }

    fn remote_update_command() -> ControlCommand {
        let state = LocalPresenceStateV1::decode(&valid_local_presence_state()).unwrap();
        ControlCommand::RemotePlayerUpdate(
            RemotePlayerUpdateV1::new(PresenceHandle::new(1).unwrap(), 1, state).unwrap(),
        )
    }

    fn remote_presence_frame(sequence: u32) -> BridgeFrame {
        let ControlCommand::RemotePlayerUpdate(value) = remote_update_command() else {
            unreachable!("remote update helper must return an update")
        };
        BridgeFrame::new(
            MessageType::RemotePlayerUpdate,
            sequence,
            TEST_SESSION_EPOCH,
            &value.encode(),
        )
        .unwrap()
    }

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
        prefix_observed: &mut mpsc::Receiver<()>,
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
        await_prefix(prefix_observed, "bridge authentication").await;
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

    async fn blocked_bridge_writer_pair() -> (BridgeWriter, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (reader, writer) = server.into_split();
        let (writer, _gate) = BridgeWriter::new_blocked(writer);
        drop(reader);
        (writer, client)
    }

    async fn control_writer_pair() -> (ControlWriter, TcpStream) {
        let (_, writer, client) = control_reader_writer_pair().await;
        (writer, client)
    }

    async fn control_reader_writer_pair()
    -> (crate::control::ControlReader, ControlWriter, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (reader, writer) = ControlConnection::from_authenticated_stream(server).into_split();
        (reader, writer, client)
    }

    async fn spawned_control_io() -> (ControlIo, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        (
            spawn_control_reader(ControlConnection::from_authenticated_stream(server)),
            client,
        )
    }

    async fn preloaded_control_io(command: ControlCommand) -> (ControlIo, TcpStream) {
        let (writer, client) = control_writer_pair().await;
        let (sender, receiver) = mpsc::channel(1);
        let (_presence_sender, presence_receiver) = mpsc::channel(1);
        sender.send(Ok(command)).await.unwrap();
        let task = tokio::spawn(std::future::pending::<()>());
        let terminal = ControlTerminalState::new();
        let terminal_receiver = terminal.signal.subscribe();
        (
            ControlIo {
                writer: Some(writer),
                receiver: Some(receiver),
                presence_receiver: Some(presence_receiver),
                presence_overflow: Arc::new(AtomicBool::new(false)),
                presence_overflow_notify: Arc::new(Notify::new()),
                terminal,
                terminal_receiver,
                task: Some(task),
            },
            client,
        )
    }

    fn full_deferred_routine_queue() -> VecDeque<ControlCommand> {
        (0..MAX_DEFERRED_ROUTINE_COMMANDS)
            .map(|index| {
                abort(
                    &format!("10000000-0000-4000-8000-{index:012x}"),
                    TEST_SESSION_EPOCH - 1,
                    1,
                )
            })
            .collect()
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

    async fn assert_command_result(
        stream: &mut TcpStream,
        command_id: &str,
        status: CommandStatus,
        reason: Option<CommandReason>,
    ) {
        let event = timeout(Duration::from_secs(2), read_event(stream))
            .await
            .expect("command result must arrive within the test bound");
        match event {
            ControlEvent::CommandResult {
                command_id: actual_command_id,
                status: actual_status,
                reason: actual_reason,
            } => assert_eq!(
                (actual_command_id, actual_status, actual_reason),
                (CommandId::parse(command_id).unwrap(), status, reason)
            ),
            event => panic!("expected command result, received {event:?}"),
        }
    }

    async fn assert_presence_and_command_result(stream: &mut TcpStream) {
        let first_event = read_event(stream).await;
        if matches!(first_event, ControlEvent::PlayerState(_)) {
            assert!(matches!(
                read_event(stream).await,
                ControlEvent::CommandResult {
                    status: CommandStatus::Applied,
                    ..
                }
            ));
        } else {
            assert!(matches!(
                first_event,
                ControlEvent::CommandResult {
                    status: CommandStatus::Applied,
                    ..
                }
            ));
            assert!(matches!(
                read_event(stream).await,
                ControlEvent::PlayerState(_)
            ));
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

    fn shutdown(command_id: &str, epoch: u32) -> ControlCommand {
        ControlCommand::ShutdownRequest(ShutdownRequest {
            command_id: CommandId::parse(command_id).unwrap(),
            session_epoch: epoch,
        })
    }

    async fn send_command(stream: &mut TcpStream, command: &ControlCommand) {
        let mut bytes = serde_json::to_vec(command).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).await.unwrap();
    }

    async fn await_prefix(prefixes: &mut mpsc::Receiver<()>, description: &str) {
        timeout(Duration::from_secs(1), prefixes.recv())
            .await
            .unwrap_or_else(|_| panic!("{description} prefix was not consumed"))
            .unwrap_or_else(|| panic!("{description} prefix observer closed"));
    }

    async fn assert_no_bridge_data(stream: &mut TcpStream) {
        let mut unexpected = [0_u8; 1];
        assert!(
            timeout(Duration::from_millis(100), stream.read(&mut unexpected))
                .await
                .is_err()
        );
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
            // Interleave the two bounded directions. Several saturation tests
            // run in parallel, and batching every request before reading any
            // result can fill both Windows TCP buffers and deadlock the test
            // fixture without exercising product behavior.
            assert_command_result(
                control,
                &command_id,
                CommandStatus::Rejected,
                Some(CommandReason::WrongState),
            )
            .await;
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
    async fn elapsed_quiescing_deadline_is_a_typed_failure() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (_bridge_tx, mut bridge_rx) = mpsc::channel(1);
        let (_bridge_terminal_tx, mut bridge_terminal) = watch::channel(None);
        let mut reconnect = ReconnectState {
            checkpoint_state: CheckpointState::Quiescing {
                deadline: Instant::now(),
            },
            ..ReconnectState::default()
        };
        let mut pending_bridge_frame = None;
        assert!(matches!(
            sidecar
                .await_control_or_bridge(
                    &mut bridge_rx,
                    &mut bridge_terminal,
                    &mut reconnect,
                    &mut pending_bridge_frame,
                )
                .await,
            Err(SidecarError::ShutdownTimeout)
        ));
    }

    #[tokio::test]
    async fn ready_shutdown_precedes_ready_invalid_bridge_authentication() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let command = shutdown("00000000-0000-4000-8000-00000000002f", TEST_SESSION_EPOCH);
        let (mut control, mut control_peer) = preloaded_control_io(command).await;
        let mut reconnect = ReconnectContext::default();
        let mut authentication: BridgeAuthentication =
            Box::pin(async { Err(SidecarError::AuthenticationFailed) });

        let step = sidecar
            .drive_control_with_bridge_auth(&mut authentication, &mut control, &mut reconnect)
            .await
            .unwrap();

        assert!(matches!(step, ControlledAcquisitionStep::Continue));
        assert!(reconnect.state.checkpoint_state.is_quiescing());
        assert_command_result(
            &mut control_peer,
            "00000000-0000-4000-8000-00000000002f",
            CommandStatus::Applied,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn ready_shutdown_precedes_prebuffered_invalid_bridge_candidate() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let bridge_address = sidecar.session_descriptor().address();
        let command = shutdown("00000000-0000-4000-8000-000000000030", TEST_SESSION_EPOCH);
        let (mut control, mut control_peer) = preloaded_control_io(command).await;
        let mut reconnect = ReconnectContext::default();
        let mut invalid_bridge = TcpStream::connect(bridge_address).await.unwrap();
        invalid_bridge.write_all(b"{}\n").await.unwrap();

        let step = sidecar
            .drive_control_with_bridge_listener(&mut control, &mut reconnect)
            .await
            .unwrap();

        assert!(matches!(step, ControlledAcquisitionStep::Continue));
        assert!(reconnect.state.checkpoint_state.is_quiescing());
        assert_command_result(
            &mut control_peer,
            "00000000-0000-4000-8000-000000000030",
            CommandStatus::Applied,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn pre_bridge_presence_flood_does_not_strand_shutdown_reservation() {
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

        for index in 0..MAX_DEFERRED_ROUTINE_COMMANDS {
            send_command(
                &mut control,
                &abort(
                    &format!("10000000-0000-4000-8000-{index:012x}"),
                    TEST_SESSION_EPOCH - 1,
                    1,
                ),
            )
            .await;
        }
        let presence = remote_update_command();
        for _ in 0..32 {
            send_command(&mut control, &presence).await;
        }
        let shutdown_id = "00000000-0000-4000-8000-0000000000f1";
        send_command(&mut control, &shutdown(shutdown_id, TEST_SESSION_EPOCH)).await;
        assert_command_result(&mut control, shutdown_id, CommandStatus::Applied, None).await;

        drop(control);
        server_task.abort();
    }

    #[tokio::test]
    async fn initial_active_rom_ready_does_not_disable_lifecycle_forwarding() {
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

        // The first active-epoch READY is a legitimate initial session frame,
        // not a reboot. Lifecycle forwarding must remain enabled afterwards.
        let active_ready =
            BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&active_ready.encode()).await.unwrap();
        send_command(&mut control, &remote_update_command()).await;
        let mut bytes = [0_u8; BRIDGE_FRAME_SIZE];
        timeout(Duration::from_millis(500), bridge.read_exact(&mut bytes))
            .await
            .expect("post-ready lifecycle frame must be forwarded")
            .unwrap();
        let frame = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(frame.message_type(), MessageType::RemotePlayerUpdate);

        send_command(
            &mut control,
            &shutdown("00000000-0000-4000-8000-0000000000f3", TEST_SESSION_EPOCH),
        )
        .await;
        assert_command_result(
            &mut control,
            "00000000-0000-4000-8000-0000000000f3",
            CommandStatus::Applied,
            None,
        )
        .await;
        drop(bridge);
        drop(control);
        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn presence_overflow_terminates_serving_loop_as_presence_queue_full() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (control_io, mut control) = spawned_control_io().await;

        let presence = remote_update_command();
        for _ in 0..=MAX_PRESENCE_COMMANDS {
            send_command(&mut control, &presence).await;
        }
        timeout(Duration::from_secs(1), async {
            while !control_io.presence_overflow.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("overflow must be published before the receiver observes EOF");

        let result = timeout(
            Duration::from_millis(500),
            server.accept_initial_bridge_or_shutdown(control_io, ReconnectContext::default()),
        )
        .await
        .expect("overflow must wake the real serving loop");
        assert!(matches!(result, Err(SidecarError::PresenceQueueFull)));
        drop(control);
    }

    #[tokio::test]
    async fn stalled_presence_bridge_write_does_not_delay_shutdown_ack() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let blocked_writer = server.install_test_blocked_writer_gate();
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

        // Prove the sole writer has admitted a lifecycle record and is held
        // behind the server-owned gate before the critical shutdown arrives.
        let admission = blocked_writer.observer.admission_signal.notified();
        send_command(&mut control, &remote_update_command()).await;
        timeout(Duration::from_secs(1), admission)
            .await
            .expect("lifecycle output must be pending before shutdown");
        assert!(blocked_writer.observer.admitted.load(Ordering::Acquire));
        let shutdown_id = "00000000-0000-4000-8000-0000000000f4";
        send_command(&mut control, &shutdown(shutdown_id, TEST_SESSION_EPOCH)).await;
        timeout(
            Duration::from_millis(500),
            assert_command_result(&mut control, shutdown_id, CommandStatus::Applied, None),
        )
        .await
        .expect("stalled lifecycle output must not delay shutdown ACK");

        // The peer half-closes its write side to provide the authenticated
        // clean bridge EOF that completes the quiescing session. Keep the
        // read side so the test can observe the server's joined writer close.
        bridge.shutdown().await.unwrap();
        let result = timeout(Duration::from_secs(1), server_task)
            .await
            .expect("server must finish without task cancellation")
            .unwrap();
        assert!(result.is_ok());
        assert!(blocked_writer.observer.dropped.load(Ordering::Acquire));
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), bridge.read(&mut closed))
                .await
                .expect("bridge peer must close after writer teardown")
                .unwrap(),
            0
        );
        drop(control);
    }

    #[tokio::test]
    async fn control_terminal_cause_wins_over_presence_eof_when_deferred_lane_is_full() {
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

        // Fifteen routine records plus the reserved shutdown slot leave the
        // deferred lane full. The malformed line then closes the sibling
        // presence sender, but the authoritative terminal retains the exact
        // decoder cause and wakes the real serving loop.
        for index in 0..(MAX_DEFERRED_ROUTINE_COMMANDS - 1) {
            send_command(
                &mut control,
                &abort(
                    &format!("20000000-0000-4000-8000-{index:012x}"),
                    TEST_SESSION_EPOCH - 1,
                    1,
                ),
            )
            .await;
        }
        send_command(
            &mut control,
            &shutdown("20000000-0000-4000-8000-0000000000a1", TEST_SESSION_EPOCH),
        )
        .await;
        control
            .write_all(b"{\"type\":\"not-a-command\"}\n")
            .await
            .unwrap();

        let result = timeout(Duration::from_secs(2), server_task)
            .await
            .expect("decoder terminal must wake the serving loop")
            .unwrap();
        assert!(matches!(
            result,
            Err(SidecarError::Control(ControlError::MalformedCommand(_)))
        ));
        drop(control);
    }

    #[tokio::test]
    async fn bridge_writer_failure_wakes_idle_session_after_registration_race() {
        let (mut writer, peer) = bridge_writer_pair().await;
        peer.set_zero_linger().unwrap();
        drop(peer);
        let mut failure = writer.failure_receiver();
        let frame =
            BridgeFrame::new(MessageType::SessionReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let result = timeout(
            Duration::from_secs(1),
            writer.send(&frame, Direction::SidecarToRom),
        )
        .await
        .expect("writer failure must wake the completion");
        assert!(matches!(
            result,
            Err(SidecarError::BridgeWriteConnection(_) | SidecarError::BridgeWriteTimeout)
        ));
        assert!(writer.write_failure().is_some());
        assert!(failure.changed().await.is_ok());
        assert!(*failure.borrow());
        writer.shutdown().await;
    }

    #[tokio::test]
    async fn active_writer_failure_precedes_simultaneous_control_eof() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let failure_gate = server.install_test_writer_failure_gate();
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

        timeout(Duration::from_secs(1), async {
            while !failure_gate.registration_seen.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("active loop must register writer failure before waiting");

        // Trigger the durable writer failure and close control without
        // allowing reconnectable control EOF to revive the retained bridge.
        failure_gate.trigger.notify_one();
        drop(control);
        let result = timeout(Duration::from_secs(1), server_task)
            .await
            .expect("writer failure must terminate the active session")
            .unwrap();
        assert!(matches!(
            result,
            Err(SidecarError::BridgeWriteConnection(_) | SidecarError::BridgeWriteTimeout)
        ));
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), bridge.read(&mut closed))
                .await
                .expect("bridge socket must close with the failed writer")
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn reboot_cutover_discards_queued_old_generation_lifecycle() {
        let (mut writer, mut peer) = bridge_writer_pair().await;
        writer
            .send_presence_at(&remote_presence_frame(1), Direction::SidecarToRom, 0)
            .unwrap();
        writer.cutover(1).await.unwrap();
        assert_no_bridge_data(&mut peer).await;
        writer
            .send_presence_at(&remote_presence_frame(2), Direction::SidecarToRom, 1)
            .unwrap();
        let mut bytes = [0_u8; BRIDGE_FRAME_SIZE];
        timeout(Duration::from_secs(1), peer.read_exact(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        let frame = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(frame.sequence(), 2);
        writer.shutdown().await;
    }

    #[tokio::test]
    async fn reboot_cutover_retires_unsafe_inflight_write() {
        let (mut writer, _peer) = blocked_bridge_writer_pair().await;
        writer
            .send_presence_at(&remote_presence_frame(1), Direction::SidecarToRom, 0)
            .unwrap();
        timeout(Duration::from_secs(1), async {
            while !writer.state.in_flight.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("blocked lifecycle write must be in flight");
        let error = writer
            .cutover(1)
            .await
            .expect_err("unsafe in-flight lifecycle must retire bridge");
        assert!(matches!(
            error,
            SidecarError::BridgeWriteConnection(_) | SidecarError::BridgeWriteTimeout
        ));
        writer.shutdown().await;
    }

    #[tokio::test]
    async fn pre_ready_presence_is_not_forwarded_after_bridge_authentication() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (_handshake_prefixes, _bridge_prefixes, mut control_prefixes) =
            server.observe_reader_prefixes();
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

        send_command(&mut control, &remote_update_command()).await;
        await_prefix(&mut control_prefixes, "pre-ready lifecycle command").await;

        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut bridge).await;
        assert_no_bridge_data(&mut bridge).await;

        // A fresh command after readiness is stamped with the new generation
        // and is forwarded with the next outer sequence.
        send_command(&mut control, &remote_update_command()).await;
        let mut bytes = [0_u8; BRIDGE_FRAME_SIZE];
        timeout(Duration::from_millis(500), bridge.read_exact(&mut bytes))
            .await
            .expect("post-ready lifecycle command must be forwarded")
            .unwrap();
        let frame = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(frame.message_type(), MessageType::RemotePlayerUpdate);
        assert_eq!(frame.sequence(), 3);

        send_command(
            &mut control,
            &shutdown("00000000-0000-4000-8000-0000000000f5", TEST_SESSION_EPOCH),
        )
        .await;
        assert_command_result(
            &mut control,
            "00000000-0000-4000-8000-0000000000f5",
            CommandStatus::Applied,
            None,
        )
        .await;
        drop(control);
        drop(bridge);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn presence_lane_retains_capacity_and_never_strands_shutdown() {
        let (mut io, mut peer) = spawned_control_io().await;
        let presence = remote_update_command();
        for _ in 0..MAX_PRESENCE_COMMANDS {
            send_command(&mut peer, &presence).await;
        }
        let shutdown_id = "00000000-0000-4000-8000-0000000000f2";
        send_command(&mut peer, &shutdown(shutdown_id, TEST_SESSION_EPOCH)).await;

        let critical = timeout(Duration::from_secs(1), io.receiver().recv())
            .await
            .expect("critical command must not wait on lifecycle capacity")
            .expect("control reader must remain alive")
            .expect("critical command must be valid");
        assert!(matches!(critical, ControlCommand::ShutdownRequest(_)));
        let (_, presence_rx) = io.receivers();
        for _ in 0..MAX_PRESENCE_COMMANDS {
            assert!(matches!(
                presence_rx.recv().await,
                Some(Ok(StampedPresenceCommand {
                    command: ControlCommand::RemotePlayerUpdate(_),
                    ..
                }))
            ));
        }
        assert!(!io.presence_overflow.load(Ordering::Acquire));
        io.shutdown().await;
    }

    #[tokio::test]
    async fn presence_lane_overflow_is_a_stable_fatal_signal() {
        let (mut io, mut peer) = spawned_control_io().await;
        let presence = remote_update_command();
        for _ in 0..=MAX_PRESENCE_COMMANDS {
            send_command(&mut peer, &presence).await;
        }
        timeout(Duration::from_secs(1), async {
            while !io.presence_overflow.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("overflow must be surfaced instead of silently dropping a record");
        assert!(io.presence_overflow.load(Ordering::Acquire));
        io.shutdown().await;
    }

    #[tokio::test]
    async fn malformed_presence_payload_does_not_commit_rom_sequence() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut control, mut control_peer) = control_writer_pair().await;
        let (_, bridge_peer) = bridge_writer_pair().await;
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        let frame =
            BridgeFrame::new(MessageType::PlayerState, 4, TEST_SESSION_EPOCH, &[0; 27]).unwrap();
        let error = sidecar
            .handle_presence_frame(frame, &mut control, &mut session)
            .await
            .expect_err("truncated presence must be rejected");
        assert!(matches!(
            error,
            SidecarError::ProtocolViolation("invalid player state")
        ));
        assert_eq!(sidecar.sequence_state.last_session_rom, 0);
        let frame = BridgeFrame::new(
            MessageType::InteractRemotePlayer,
            5,
            TEST_SESSION_EPOCH,
            &[0; 19],
        )
        .unwrap();
        let error = sidecar
            .handle_presence_frame(frame, &mut control, &mut session)
            .await
            .expect_err("truncated interaction must be rejected");
        assert!(matches!(
            error,
            SidecarError::ProtocolViolation("invalid remote player interaction")
        ));
        assert_eq!(sidecar.sequence_state.last_session_rom, 0);
        assert!(
            timeout(
                Duration::from_millis(100),
                control_peer.read(&mut [0_u8; 1])
            )
            .await
            .is_err()
        );
        drop(bridge_peer);
    }

    #[tokio::test]
    async fn outbound_presence_preserves_inner_bytes_and_uses_independent_sequences() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut bridge, mut bridge_peer) = bridge_writer_pair().await;
        let state = LocalPresenceStateV1::decode(&valid_local_presence_state()).unwrap();
        let commands = [
            ControlCommand::RemotePlayerSpawn(
                RemotePlayerSpawnV1::new(
                    PresenceHandle::new(1).unwrap(),
                    1,
                    state.clone(),
                    CanonicalUsername::new("ash").unwrap(),
                )
                .unwrap(),
            ),
            ControlCommand::RemotePlayerUpdate(
                RemotePlayerUpdateV1::new(PresenceHandle::new(1).unwrap(), 2, state).unwrap(),
            ),
            ControlCommand::RemotePlayerDespawn(
                RemotePlayerDespawnV1::new(
                    PresenceHandle::new(1).unwrap(),
                    3,
                    DespawnReason::Disconnected,
                )
                .unwrap(),
            ),
        ];
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        for (index, command) in commands.into_iter().enumerate() {
            sidecar
                .handle_presence_command(command, &mut bridge, &session)
                .unwrap();
            let mut bytes = [0_u8; BRIDGE_FRAME_SIZE];
            bridge_peer.read_exact(&mut bytes).await.unwrap();
            let frame = BridgeFrame::decode_for(&bytes, Direction::SidecarToRom).unwrap();
            assert_eq!(frame.sequence(), u32::try_from(index).unwrap() + 1);
            assert_eq!(frame.session_epoch(), TEST_SESSION_EPOCH);
            let expected = match index {
                0 => RemotePlayerSpawnV1::new(
                    PresenceHandle::new(1).unwrap(),
                    1,
                    LocalPresenceStateV1::decode(&valid_local_presence_state()).unwrap(),
                    CanonicalUsername::new("ash").unwrap(),
                )
                .unwrap()
                .encode()
                .to_vec(),
                1 => RemotePlayerUpdateV1::new(
                    PresenceHandle::new(1).unwrap(),
                    2,
                    LocalPresenceStateV1::decode(&valid_local_presence_state()).unwrap(),
                )
                .unwrap()
                .encode()
                .to_vec(),
                _ => RemotePlayerDespawnV1::new(
                    PresenceHandle::new(1).unwrap(),
                    3,
                    DespawnReason::Disconnected,
                )
                .unwrap()
                .encode()
                .to_vec(),
            };
            assert_eq!(frame.payload(), expected.as_slice());
        }
        assert_eq!(sidecar.sequence_state.next_sidecar, 4);
        session.checkpoint_state = CheckpointState::Quiescing {
            deadline: Instant::now() + Duration::from_secs(1),
        };
        sidecar
            .handle_presence_command(remote_update_command(), &mut bridge, &session)
            .unwrap();
        assert_eq!(sidecar.sequence_state.next_sidecar, 4);
        assert_no_bridge_data(&mut bridge_peer).await;
    }

    #[tokio::test]
    async fn reboot_reset_fences_queued_presence_for_the_local_session() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut bridge, mut bridge_peer) = bridge_writer_pair().await;
        let (mut control, mut control_peer) = control_writer_pair().await;
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        sidecar.sequence_state.last_session_rom = 2;
        let reboot = BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        assert!(
            sidecar
                .handle_rom_reboot(&reboot, &mut bridge, &mut control, &mut session)
                .await
                .unwrap()
        );
        let mut ready_bytes = [0_u8; BRIDGE_FRAME_SIZE];
        bridge_peer.read_exact(&mut ready_bytes).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&ready_bytes, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
        assert!(matches!(
            read_event(&mut control_peer).await,
            ControlEvent::RomPresenceReset
        ));
        assert!(sidecar.lifecycle_forwarding_disabled);
        assert_eq!(sidecar.sequence_state.next_sidecar, 2);
        sidecar
            .handle_presence_command(remote_update_command(), &mut bridge, &session)
            .unwrap();
        assert_eq!(sidecar.sequence_state.next_sidecar, 2);
        assert_no_bridge_data(&mut bridge_peer).await;
    }

    #[tokio::test]
    async fn fieldless_reset_retry_requires_launcher_fatal_no_restart_policy() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut bridge, mut bridge_peer) = bridge_writer_pair().await;
        let (mut control, control_peer) = control_writer_pair().await;
        control_peer.set_zero_linger().unwrap();
        drop(control_peer);
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        sidecar.sequence_state.last_session_rom = 2;
        let reboot = BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let error = sidecar
            .handle_rom_reboot(&reboot, &mut bridge, &mut control, &mut session)
            .await
            .expect_err("ambiguous reset write must terminate the attempt");
        assert!(matches!(
            error,
            SidecarError::Control(ControlError::WriteConnection(_))
        ));
        let mut ready_bytes = [0_u8; BRIDGE_FRAME_SIZE];
        bridge_peer.read_exact(&mut ready_bytes).await.unwrap();
        assert!(sidecar.lifecycle_forwarding_disabled);
        assert!(session.rearm_after_reboot);

        // A replacement control channel may retry the fieldless marker only
        // while the launcher treats the ambiguous handoff as fatal and does
        // not start another realtime core in this local session.
        let (mut replacement, mut replacement_peer) = control_writer_pair().await;
        sidecar
            .resume_checkpoint_handoff(&mut session, &mut bridge, &mut replacement)
            .await
            .unwrap();
        assert!(matches!(
            read_event(&mut replacement_peer).await,
            ControlEvent::RomPresenceReset
        ));
        assert!(!session.rearm_after_reboot);
    }

    #[tokio::test]
    async fn full_deferred_routine_queue_still_admits_ready_shutdown_during_bridge_auth() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let command_id = "00000000-0000-4000-8000-00000000003a";
        let (mut control, mut control_peer) =
            preloaded_control_io(shutdown(command_id, TEST_SESSION_EPOCH)).await;
        let mut reconnect = ReconnectContext {
            deferred_commands: full_deferred_routine_queue(),
            ..ReconnectContext::default()
        };
        let mut authentication: BridgeAuthentication =
            Box::pin(async { Err(SidecarError::AuthenticationFailed) });

        let step = sidecar
            .drive_control_with_bridge_auth(&mut authentication, &mut control, &mut reconnect)
            .await
            .unwrap();

        assert!(matches!(step, ControlledAcquisitionStep::Continue));
        assert_eq!(
            reconnect.deferred_commands.len(),
            MAX_DEFERRED_ROUTINE_COMMANDS
        );
        assert!(reconnect.state.checkpoint_state.is_quiescing());
        assert_command_result(&mut control_peer, command_id, CommandStatus::Applied, None).await;

        let (mut bridge, _bridge_peer) = bridge_writer_pair().await;
        let mut session = ActiveSessionState {
            checkpoint_state: reconnect.state.checkpoint_state,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        sidecar
            .drain_deferred_commands(
                &mut reconnect.deferred_commands,
                &mut bridge,
                control.writer(),
                &mut session,
            )
            .await
            .unwrap();
        for index in 0..MAX_DEFERRED_ROUTINE_COMMANDS {
            assert_command_result(
                &mut control_peer,
                &format!("10000000-0000-4000-8000-{index:012x}"),
                CommandStatus::Rejected,
                Some(CommandReason::WrongState),
            )
            .await;
        }
        assert_eq!(reconnect.deferred_commands.len(), 0);
        assert!(session.checkpoint_state.is_quiescing());
    }

    #[tokio::test]
    async fn full_deferred_routine_queue_still_admits_ready_shutdown_before_bridge_listener() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let bridge_address = sidecar.session_descriptor().address();
        let command_id = "00000000-0000-4000-8000-00000000003b";
        let (mut control, _control_peer) =
            preloaded_control_io(shutdown(command_id, TEST_SESSION_EPOCH)).await;
        let mut reconnect = ReconnectContext {
            deferred_commands: full_deferred_routine_queue(),
            ..ReconnectContext::default()
        };
        let mut invalid_bridge = TcpStream::connect(bridge_address).await.unwrap();
        invalid_bridge.write_all(b"{}\n").await.unwrap();

        let step = sidecar
            .drive_control_with_bridge_listener(&mut control, &mut reconnect)
            .await
            .unwrap();

        assert!(matches!(step, ControlledAcquisitionStep::Continue));
        assert_eq!(
            reconnect.deferred_commands.len(),
            MAX_DEFERRED_ROUTINE_COMMANDS
        );
        assert!(reconnect.state.checkpoint_state.is_quiescing());
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
    async fn pre_bridge_shutdown_is_replay_safe_and_exits_on_control_eof() {
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

        let command_id = "00000000-0000-4000-8000-000000000020";
        let command = shutdown(command_id, TEST_SESSION_EPOCH);
        send_command(&mut control, &command).await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                reason: None,
                ..
            }
        ));
        send_command(&mut control, &command).await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Replayed,
                reason: None,
                ..
            }
        ));
        send_command(
            &mut control,
            &shutdown("00000000-0000-4000-8000-000000000021", TEST_SESSION_EPOCH),
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
        send_command(
            &mut control,
            &shutdown(
                "00000000-0000-4000-8000-000000000022",
                TEST_SESSION_EPOCH - 1,
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

        drop(control);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn fragmented_pre_bridge_command_survives_invalid_bridge_authentication() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (_handshake_prefixes, _bridge_prefixes, mut control_prefixes) =
            server.observe_reader_prefixes();
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

        let command = abort(
            "00000000-0000-4000-8000-00000000002a",
            TEST_SESSION_EPOCH - 1,
            1,
        );
        let mut command_line = serde_json::to_vec(&command).unwrap();
        command_line.push(b'\n');
        let split = command_line.len() / 2;
        control.write_all(&command_line[..split]).await.unwrap();
        await_prefix(&mut control_prefixes, "fragmented control command").await;

        let mut invalid_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut invalid_handshake = serde_json::to_vec(&TestHandshake {
            secret: "00000000000000000000000000000000",
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        invalid_handshake.push(b'\n');
        invalid_bridge.write_all(&invalid_handshake).await.unwrap();
        let mut closed = [0_u8; 1];
        assert_eq!(invalid_bridge.read(&mut closed).await.unwrap(), 0);

        control.write_all(&command_line[split..]).await.unwrap();
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut bridge, &descriptor, descriptor.secret()).await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Rejected,
                reason: Some(CommandReason::WrongEpoch),
                ..
            }
        ));

        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn slow_bridge_handshake_cannot_delay_authenticated_pre_bridge_shutdown() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut handshake_prefixes, _bridge_prefixes, mut control_prefixes) =
            server.observe_reader_prefixes();
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

        let mut slow_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        slow_bridge.write_all(b"{").await.unwrap();
        await_prefix(&mut handshake_prefixes, "slow bridge authentication").await;
        let command = shutdown("00000000-0000-4000-8000-00000000002b", TEST_SESSION_EPOCH);
        let mut command_line = serde_json::to_vec(&command).unwrap();
        command_line.push(b'\n');
        let split = command_line.len() / 2;
        control.write_all(&command_line[..split]).await.unwrap();
        await_prefix(&mut control_prefixes, "fragmented shutdown command").await;
        control.write_all(&command_line[split..]).await.unwrap();
        assert!(matches!(
            timeout(Duration::from_millis(250), read_event(&mut control))
                .await
                .unwrap(),
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                reason: None,
                ..
            }
        ));

        drop(control);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
        drop(slow_bridge);
    }

    #[tokio::test]
    async fn quiescing_ignores_bridge_effects_and_exits_on_bridge_eof() {
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

        send_command(
            &mut control,
            &shutdown("00000000-0000-4000-8000-000000000023", TEST_SESSION_EPOCH),
        )
        .await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));

        // A checkpoint frame arriving after quiescence must not create a
        // launcher event or open a decision window.
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        let mut unexpected = [0_u8; 1];
        assert!(
            timeout(Duration::from_millis(100), read_event(&mut control))
                .await
                .is_err()
        );
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000024",
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
        assert!(
            timeout(
                Duration::from_millis(100),
                bridge.read_exact(&mut unexpected)
            )
            .await
            .is_err()
        );

        drop(bridge);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
        drop(control);
    }

    #[tokio::test]
    async fn authenticated_bridge_requires_bridge_eof_after_control_reconnect() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (_handshake_prefixes, mut bridge_prefixes, mut control_prefixes) =
            server.observe_reader_prefixes();
        let descriptor = server.session_descriptor();
        let mut server_task = tokio::spawn(server.serve());
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

        let command = shutdown("00000000-0000-4000-8000-00000000002c", TEST_SESSION_EPOCH);
        send_command(&mut control, &command).await;
        await_prefix(&mut control_prefixes, "initial shutdown command").await;
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Applied,
                ..
            }
        ));
        drop(control);
        assert!(
            timeout(Duration::from_millis(150), &mut server_task)
                .await
                .is_err()
        );

        let mut replacement = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let mut replay_line = serde_json::to_vec(&command).unwrap();
        replay_line.push(b'\n');
        let split = replay_line.len() / 2;
        replacement.write_all(&replay_line[..split]).await.unwrap();
        await_prefix(&mut control_prefixes, "fragmented shutdown replay").await;
        for sequence in 9..41 {
            let ignored =
                BridgeFrame::new(MessageType::PlayerState, sequence, TEST_SESSION_EPOCH, &[])
                    .unwrap();
            bridge.write_all(&ignored.encode()).await.unwrap();
        }
        await_prefix(&mut bridge_prefixes, "competing player-state frame").await;
        replacement.write_all(&replay_line[split..]).await.unwrap();
        assert!(matches!(
            read_event(&mut replacement).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Replayed,
                reason: None,
                ..
            }
        ));
        drop(replacement);
        assert!(
            timeout(Duration::from_millis(150), &mut server_task)
                .await
                .is_err()
        );

        drop(bridge);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn shutdown_rejects_every_non_idle_checkpoint_state_without_mutation() {
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        let states = [
            CheckpointState::ReadyPendingHandoff { key },
            CheckpointState::AwaitDecision {
                key,
                deadline: Instant::now() + Duration::from_secs(1),
            },
            CheckpointState::ExpiryPendingHandoff { key },
            CheckpointState::AwaitSaveData {
                key,
                deadline: Instant::now() + Duration::from_secs(1),
            },
            CheckpointState::Quiescing {
                deadline: Instant::now() + Duration::from_secs(1),
            },
        ];
        for (index, state) in states.into_iter().enumerate() {
            let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
                .await
                .unwrap();
            let (mut bridge, _bridge_peer) = bridge_writer_pair().await;
            let (mut control, mut control_peer) = control_writer_pair().await;
            let mut session = ActiveSessionState {
                checkpoint_state: state,
                expired_checkpoint: None,
                acknowledged_rom_ready: true,
                rearm_after_reboot: false,
            };
            let command_id = format!("00000000-0000-4000-8000-{index:012x}");
            let command = shutdown(&command_id, TEST_SESSION_EPOCH);
            sidecar
                .handle_control_command(
                    command.clone(),
                    &mut bridge,
                    &mut control,
                    Instant::now(),
                    &mut session,
                )
                .await
                .unwrap();
            assert_eq!(session.checkpoint_state, state);
            assert_command_result(
                &mut control_peer,
                &command_id,
                CommandStatus::Rejected,
                Some(CommandReason::WrongState),
            )
            .await;

            session.checkpoint_state = CheckpointState::Idle;
            sidecar
                .handle_control_command(
                    command,
                    &mut bridge,
                    &mut control,
                    Instant::now(),
                    &mut session,
                )
                .await
                .unwrap();
            assert_eq!(session.checkpoint_state, CheckpointState::Idle);
            assert_eq!(sidecar.applied_shutdown, None);
            assert_command_result(
                &mut control_peer,
                &command_id,
                CommandStatus::Replayed,
                Some(CommandReason::WrongState),
            )
            .await;
        }
    }

    #[tokio::test]
    async fn shutdown_is_durable_before_ack_failure_and_replays_after_reconnect() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut bridge, _bridge_peer) = bridge_writer_pair().await;
        let (mut control_reader, mut control, control_peer) = control_reader_writer_pair().await;
        let command = shutdown("00000000-0000-4000-8000-000000000025", TEST_SESSION_EPOCH);
        let command_id = CommandId::parse("00000000-0000-4000-8000-000000000025").unwrap();
        control_peer.set_zero_linger().unwrap();
        drop(control_peer);
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        let reset = timeout(Duration::from_secs(1), control_reader.receive_command())
            .await
            .expect("reset peer must wake the server reader")
            .expect_err("zero-linger peer must not produce a command");
        assert!(matches!(
            reset,
            ControlError::Connection(_) | ControlError::LineClosed
        ));
        let error = sidecar
            .handle_control_command(
                command.clone(),
                &mut bridge,
                &mut control,
                Instant::now(),
                &mut session,
            )
            .await
            .expect_err("the shutdown ACK write to a reset peer must fail");
        assert!(matches!(
            error,
            SidecarError::Control(ControlError::WriteConnection(_))
        ));
        assert!(session.checkpoint_state.is_quiescing());
        let original_deadline = session
            .checkpoint_state
            .quiescing_deadline()
            .expect("shutdown stores an absolute deadline");
        assert!(sidecar.command_history.contains_key(&command_id));
        assert_eq!(sidecar.applied_shutdown, Some(command_id));

        let (mut replacement, mut replacement_peer) = control_writer_pair().await;
        sidecar
            .handle_control_command(
                command,
                &mut bridge,
                &mut replacement,
                Instant::now(),
                &mut session,
            )
            .await
            .unwrap();
        assert_eq!(
            session.checkpoint_state.quiescing_deadline(),
            Some(original_deadline)
        );
        assert!(matches!(
            read_event(&mut replacement_peer).await,
            ControlEvent::CommandResult {
                status: CommandStatus::Replayed,
                reason: None,
                ..
            }
        ));
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
    async fn pre_bridge_fifo_survives_bridge_replacement_and_applies_once() {
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
        drop(first_bridge);
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), first_control.read(&mut closed))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(first_control);

        let mut replacement_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let grant_command = grant(
            "00000000-0000-4000-8000-00000000002d",
            TEST_SESSION_EPOCH,
            1,
        );
        send_command(&mut replacement_control, &grant_command).await;
        send_command(
            &mut replacement_control,
            &shutdown("00000000-0000-4000-8000-00000000002e", TEST_SESSION_EPOCH),
        )
        .await;

        let mut replacement_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut replacement_bridge, &descriptor, descriptor.secret()).await;
        establish_rom_session(&mut replacement_bridge).await;
        assert_command_result(
            &mut replacement_control,
            "00000000-0000-4000-8000-00000000002d",
            CommandStatus::Applied,
            None,
        )
        .await;
        assert_command_result(
            &mut replacement_control,
            "00000000-0000-4000-8000-00000000002e",
            CommandStatus::Rejected,
            Some(CommandReason::WrongState),
        )
        .await;
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        replacement_bridge.read_exact(&mut granted).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&granted, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::CheckpointGranted
        );
        assert_no_bridge_data(&mut replacement_bridge).await;

        drop(replacement_control);
        drop(replacement_bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn idle_reconnect_fifo_resolves_grant_before_shutdown_after_rom_ready() {
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
        drop(first_bridge);
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), first_control.read(&mut closed))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(first_control);

        let mut replacement_control = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement_control,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        send_command(
            &mut replacement_control,
            &grant(
                "00000000-0000-4000-8000-000000000031",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        send_command(
            &mut replacement_control,
            &shutdown("00000000-0000-4000-8000-000000000032", TEST_SESSION_EPOCH),
        )
        .await;

        let mut replacement_bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut replacement_bridge, &descriptor, descriptor.secret()).await;
        assert!(
            timeout(
                Duration::from_millis(100),
                read_event(&mut replacement_control)
            )
            .await
            .is_err()
        );
        establish_rom_session(&mut replacement_bridge).await;
        assert_command_result(
            &mut replacement_control,
            "00000000-0000-4000-8000-000000000031",
            CommandStatus::Rejected,
            Some(CommandReason::WrongState),
        )
        .await;
        assert_command_result(
            &mut replacement_control,
            "00000000-0000-4000-8000-000000000032",
            CommandStatus::Applied,
            None,
        )
        .await;

        drop(replacement_bridge);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn bridge_reset_history_prevents_control_eof_from_proving_shutdown() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let mut server_task = tokio::spawn(server.serve());

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
        bridge.set_zero_linger().unwrap();
        drop(bridge);

        let mut closed = [0_u8; 1];
        let close = timeout(Duration::from_secs(1), first_control.read(&mut closed))
            .await
            .expect("bridge reset must release the old control session");
        assert_eq!(close.unwrap(), 0);
        drop(first_control);

        let mut replacement = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        send_command(
            &mut replacement,
            &shutdown("00000000-0000-4000-8000-000000000033", TEST_SESSION_EPOCH),
        )
        .await;
        assert_command_result(
            &mut replacement,
            "00000000-0000-4000-8000-000000000033",
            CommandStatus::Applied,
            None,
        )
        .await;
        drop(replacement);

        assert!(
            timeout(Duration::from_millis(200), &mut server_task)
                .await
                .is_err(),
            "control EOF must not replace missing clean bridge EOF evidence"
        );
        server_task.abort();
    }

    #[tokio::test]
    async fn clean_bridge_eof_completes_after_applied_shutdown_ack_with_control_open() {
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
        drop(bridge);

        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), first_control.read(&mut closed))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(first_control);

        let command_id = "00000000-0000-4000-8000-00000000003c";
        let mut replacement = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        send_command(&mut replacement, &shutdown(command_id, TEST_SESSION_EPOCH)).await;
        assert_command_result(&mut replacement, command_id, CommandStatus::Applied, None).await;

        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok(),
            "clean bridge EOF must make a delivered Applied ACK terminal"
        );
        drop(replacement);
    }

    #[tokio::test]
    async fn clean_bridge_eof_completes_after_replayed_shutdown_ack() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let command_id = "00000000-0000-4000-8000-00000000003d";
        let command = shutdown(command_id, TEST_SESSION_EPOCH);
        let (mut first_writer, mut first_peer) = control_writer_pair().await;
        let mut checkpoint_state = CheckpointState::Idle;
        assert!(
            sidecar
                .handle_control_command_without_bridge(
                    command.clone(),
                    &mut first_writer,
                    &mut checkpoint_state,
                )
                .await
                .unwrap()
        );
        assert_command_result(&mut first_peer, command_id, CommandStatus::Applied, None).await;

        let (mut control, mut control_peer) = preloaded_control_io(command).await;
        let mut reconnect = ReconnectContext {
            state: ReconnectState {
                checkpoint_state,
                ..ReconnectState::default()
            },
            bridge_lifecycle: BridgeLifecycle::CleanEofObserved,
            ..ReconnectContext::default()
        };
        let replay_is_terminal = sidecar
            .process_pre_bridge_control_result(
                control.receiver().recv().await,
                &mut control,
                &mut reconnect,
            )
            .await
            .unwrap();

        assert!(replay_is_terminal);
        assert_command_result(&mut control_peer, command_id, CommandStatus::Replayed, None).await;
    }

    #[tokio::test]
    async fn pre_bridge_replay_after_ack_write_failure_consumes_clean_eof() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let command_id = "00000000-0000-4000-8000-00000000003e";
        let command = shutdown(command_id, TEST_SESSION_EPOCH);
        let (mut control_reader, mut first_writer, first_peer) = control_reader_writer_pair().await;
        first_peer.set_zero_linger().unwrap();
        drop(first_peer);
        let reset = timeout(Duration::from_secs(1), control_reader.receive_command())
            .await
            .expect("reset peer must wake the server reader")
            .expect_err("zero-linger peer must not produce a command");
        assert!(matches!(
            reset,
            ControlError::Connection(_) | ControlError::LineClosed
        ));
        let mut checkpoint_state = CheckpointState::Idle;
        let error = sidecar
            .handle_control_command_without_bridge(
                command.clone(),
                &mut first_writer,
                &mut checkpoint_state,
            )
            .await
            .expect_err("the first shutdown ACK must fail on the reset peer");
        assert!(matches!(
            error,
            SidecarError::Control(ControlError::WriteConnection(_) | ControlError::WriteTimeout)
        ));
        drop(first_writer);
        assert!(checkpoint_state.is_quiescing());
        assert_eq!(
            sidecar.applied_shutdown,
            Some(CommandId::parse(command_id).unwrap())
        );

        let (mut replacement, mut replacement_peer) = preloaded_control_io(command).await;
        let mut reconnect = ReconnectContext {
            state: ReconnectState {
                checkpoint_state,
                ..ReconnectState::default()
            },
            bridge_lifecycle: BridgeLifecycle::CleanEofObserved,
            ..ReconnectContext::default()
        };
        let replay_is_terminal = sidecar
            .process_pre_bridge_control_result(
                replacement.receiver().recv().await,
                &mut replacement,
                &mut reconnect,
            )
            .await
            .unwrap();

        assert!(replay_is_terminal);
        assert_eq!(
            reconnect.bridge_lifecycle,
            BridgeLifecycle::NeverAuthenticated
        );
        assert_command_result(
            &mut replacement_peer,
            command_id,
            CommandStatus::Replayed,
            None,
        )
        .await;
        drop(replacement);
        drop(replacement_peer);
    }

    #[tokio::test]
    async fn elapsed_decision_deadline_advances_while_waiting_for_replacement_pair() {
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
        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        bridge.write_all(&ready.encode()).await.unwrap();
        assert!(matches!(
            read_event(&mut first_control).await,
            ControlEvent::CheckpointReady {
                ready_sequence: 1,
                ..
            }
        ));
        drop(bridge);
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), first_control.read(&mut closed))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(first_control);

        sleep_until(Instant::now() + DECISION_TIMEOUT + Duration::from_millis(50)).await;
        let mut replacement = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        assert!(matches!(
            timeout(Duration::from_secs(1), read_event(&mut replacement))
                .await
                .expect("elapsed decision deadline must hand off without a bridge"),
            ControlEvent::CheckpointExpired {
                ready_sequence: 1,
                ..
            }
        ));
        send_command(
            &mut replacement,
            &shutdown("00000000-0000-4000-8000-000000000034", TEST_SESSION_EPOCH),
        )
        .await;
        assert_command_result(
            &mut replacement,
            "00000000-0000-4000-8000-000000000034",
            CommandStatus::Applied,
            None,
        )
        .await;
        drop(replacement);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn control_only_loss_stages_decision_expiry_while_observing_bridge_eof() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        let started = Instant::now();
        let deadline = started + Duration::from_millis(50);
        let mut reconnect = ReconnectState {
            checkpoint_state: CheckpointState::AwaitDecision { key, deadline },
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        let (bridge_tx, mut bridge_rx) = mpsc::channel(1);
        let (_bridge_terminal_tx, mut bridge_terminal) = watch::channel(None);
        tokio::spawn(async move {
            sleep_until(deadline + Duration::from_millis(25)).await;
            bridge_tx
                .send(Err(SidecarError::ProtocolViolation("bridge disconnected")))
                .await
                .unwrap();
        });
        let mut pending_bridge_frame = None;

        let result = timeout(
            Duration::from_secs(1),
            sidecar.await_control_or_bridge(
                &mut bridge_rx,
                &mut bridge_terminal,
                &mut reconnect,
                &mut pending_bridge_frame,
            ),
        )
        .await
        .expect("the existing bridge EOF must remain visible without replacement control")
        .unwrap();

        assert!(started.elapsed() >= Duration::from_millis(50));
        assert!(matches!(
            result,
            ControlReacquisition::BridgeTerminated(SidecarError::ProtocolViolation(
                "bridge disconnected"
            ))
        ));
        assert_eq!(
            reconnect.checkpoint_state,
            CheckpointState::ExpiryPendingHandoff { key }
        );
        assert_eq!(reconnect.expired_checkpoint, Some(key));
        assert_eq!(pending_bridge_frame, None);
    }

    #[tokio::test]
    async fn control_only_loss_observes_bridge_eof_after_prefetched_frame() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 8).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut reconnect = ReconnectState {
            checkpoint_state: CheckpointState::AwaitDecision { key, deadline },
            expired_checkpoint: None,
            acknowledged_rom_ready: true,
            rearm_after_reboot: false,
        };
        let frame = BridgeFrame::new(MessageType::PlayerState, 8, TEST_SESSION_EPOCH, &[]).unwrap();
        let (bridge_tx, mut bridge_rx) = mpsc::channel(1);
        bridge_tx.send(Ok(frame.clone())).await.unwrap();
        let (bridge_terminal_tx, mut bridge_terminal) = watch::channel(None);
        tokio::spawn(async move {
            sleep_until(Instant::now() + Duration::from_millis(25)).await;
            bridge_terminal_tx
                .send(Some(BridgeTerminal::CleanEof))
                .unwrap();
            bridge_tx
                .send(Err(SidecarError::ProtocolViolation("bridge disconnected")))
                .await
                .unwrap();
        });
        let mut pending_bridge_frame = None;

        let result = timeout(
            Duration::from_millis(500),
            sidecar.await_control_or_bridge(
                &mut bridge_rx,
                &mut bridge_terminal,
                &mut reconnect,
                &mut pending_bridge_frame,
            ),
        )
        .await
        .expect("bridge EOF must remain observable after a prefetched frame")
        .unwrap();

        assert!(matches!(
            result,
            ControlReacquisition::BridgeTerminated(SidecarError::ProtocolViolation(
                "bridge disconnected"
            ))
        ));
        assert_eq!(pending_bridge_frame.as_deref(), Some(&frame));
        assert_eq!(
            reconnect.checkpoint_state,
            CheckpointState::AwaitDecision { key, deadline }
        );
        assert_eq!(reconnect.expired_checkpoint, None);
    }

    #[tokio::test]
    async fn replacement_bridge_auth_loss_stages_decision_expiry_and_observes_eof() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        let started = Instant::now();
        let deadline = started + Duration::from_millis(250);
        let mut reconnect = ReconnectContext {
            state: ReconnectState {
                checkpoint_state: CheckpointState::AwaitDecision { key, deadline },
                expired_checkpoint: None,
                acknowledged_rom_ready: true,
                rearm_after_reboot: false,
            },
            bridge_lifecycle: BridgeLifecycle::AwaitingCleanEof,
            ..ReconnectContext::default()
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let bridge_address = listener.local_addr().unwrap();
        let mut bridge_peer = TcpStream::connect(bridge_address).await.unwrap();
        let (bridge_stream, _) = listener.accept().await.unwrap();
        let mut authentication: BridgeAuthentication = Box::pin(async move {
            Ok(AuthenticatedConnection {
                stream: bridge_stream,
            })
        });
        let LostControlRace::Bridge(mut replacement_bridge) = sidecar
            .race_lost_control_with_bridge_auth(&mut authentication, &mut reconnect)
            .await
            .unwrap()
        else {
            panic!("replacement bridge authentication must complete after control loss");
        };
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        bridge_peer.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
        let mut session_ready = [0; BRIDGE_FRAME_SIZE];
        bridge_peer.read_exact(&mut session_ready).await.unwrap();
        assert_eq!(
            BridgeFrame::decode_for(&session_ready, Direction::SidecarToRom)
                .unwrap()
                .message_type(),
            MessageType::SessionReady
        );
        let bridge_close = tokio::spawn(async move {
            sleep_until(deadline + Duration::from_millis(25)).await;
            drop(bridge_peer);
        });

        // This is the exact pair-acquisition branch reached when replacement
        // bridge authentication wins only after the prior control reader ends.
        let result = timeout(
            Duration::from_secs(2),
            sidecar.reacquire_control_with_bridge(&mut replacement_bridge, &mut reconnect),
        )
        .await
        .expect("replacement bridge EOF must stay visible without replacement control")
        .unwrap();
        bridge_close.await.unwrap();

        assert!(matches!(result, ControlRecovery::Reconnect));
        assert!(started.elapsed() >= Duration::from_millis(250));
        assert_eq!(
            reconnect.state.checkpoint_state,
            CheckpointState::ExpiryPendingHandoff { key }
        );
        assert_eq!(reconnect.state.expired_checkpoint, Some(key));
        assert_eq!(
            reconnect.bridge_lifecycle,
            BridgeLifecycle::CleanEofObserved
        );
        assert_eq!(replacement_bridge.pending_frame, None);
    }

    #[tokio::test]
    async fn replacement_bridge_auth_loss_preserves_one_prefetched_frame() {
        let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = sidecar.session_descriptor();
        let first = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        let second =
            BridgeFrame::new(MessageType::PlayerState, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        let (bridge_writer, _bridge_peer) = bridge_writer_pair().await;
        let (bridge_tx, bridge_rx) = mpsc::channel(1);
        bridge_tx.send(Ok(first.clone())).await.unwrap();
        let (second_queued_tx, second_queued_rx) = oneshot::channel();
        let queued_second = second.clone();
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let bridge_task = tokio::spawn(async move {
            let _terminal_tx = terminal_tx;
            bridge_tx.send(Ok(queued_second)).await.unwrap();
            let _ = second_queued_tx.send(());
            std::future::pending::<()>().await;
        });
        let mut replacement_bridge = BridgeIo {
            writer: Some(bridge_writer),
            receiver: Some(bridge_rx),
            task: Some(bridge_task),
            terminal: terminal_rx,
            pending_frame: None,
        };
        let mut reconnect = ReconnectContext {
            bridge_lifecycle: BridgeLifecycle::AwaitingCleanEof,
            ..ReconnectContext::default()
        };
        let control_client = tokio::spawn(async move {
            sleep_until(Instant::now() + Duration::from_millis(25)).await;
            let mut stream = TcpStream::connect(descriptor.control_address())
                .await
                .unwrap();
            write_control_handshake(
                &mut stream,
                &descriptor,
                descriptor.control_secret(),
                TEST_SESSION_EPOCH,
            )
            .await;
            stream
        });

        let result = timeout(
            Duration::from_secs(1),
            sidecar.reacquire_control_with_bridge(&mut replacement_bridge, &mut reconnect),
        )
        .await
        .expect("replacement control must authenticate within the test bound")
        .unwrap();

        let ControlRecovery::Control(replacement_control) = result else {
            panic!("replacement control must win after one frame is prefetched");
        };
        timeout(Duration::from_secs(1), second_queued_rx)
            .await
            .expect("the bounded bridge channel must retain its next frame")
            .unwrap();
        assert_eq!(replacement_bridge.pending_frame.as_deref(), Some(&first));
        assert_eq!(
            replacement_bridge
                .receiver
                .as_mut()
                .expect("bridge receiver is owned")
                .try_recv()
                .unwrap()
                .unwrap(),
            second
        );
        let control_peer = control_client.await.unwrap();
        drop(replacement_control);
        drop(control_peer);
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
        let save = save_update(2, 17);
        second_bridge.write_all(&save.encode()).await.unwrap();
        assert_eq!(
            read_event(&mut second_control).await,
            ControlEvent::SaveDataUpdated {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 1,
                save_sequence: 2,
                save_generation: 17,
            }
        );

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
        assert_eq!(
            read_event(&mut control).await,
            ControlEvent::RomPresenceReset
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
        assert_eq!(
            read_event(&mut control).await,
            ControlEvent::RomPresenceReset
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
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let address = descriptor.control_address();
        let wrong_secret = descriptor.secret().to_owned();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let line = format!(
            "{{\"secret\":\"{wrong_secret}\",\"control_version\":{CONTROL_PROTOCOL_VERSION},\"session_epoch\":41}}\n"
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::AuthenticationFailed))
        ));

        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        let line = format!(
            "{{\"secret\":\"{}\",\"control_version\":{CONTROL_PROTOCOL_VERSION},\"session_epoch\":40,\"extra\":false}}\n",
            descriptor.control_secret()
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::MalformedHandshake(_)))
        ));

        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let task = tokio::spawn(async move { server.accept_control().await });
        let mut stream = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        let line = format!(
            "{{\"secret\":\"{}\",\"control_version\":{CONTROL_PROTOCOL_VERSION},\"session_epoch\":40}}\n",
            descriptor.control_secret()
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(SidecarError::Control(ControlError::InvalidSessionEpoch))
        ));
    }

    async fn assert_control_authentication_pump_survives_slowloris(slow_candidates: usize) {
        let listener = ControlListener::bind(TEST_SESSION_EPOCH).await.unwrap();
        let address = listener.address();
        let secret = listener.secret().expose().to_owned();
        let pump = tokio::spawn(authenticate_control_candidates(listener));
        let mut incomplete = Vec::new();
        for _ in 0..slow_candidates {
            let mut stream = TcpStream::connect(address).await.unwrap();
            stream.write_all(b"{").await.unwrap();
            incomplete.push(stream);
        }
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let mut valid = TcpStream::connect(address).await.unwrap();
        let mut line = serde_json::to_vec(&TestControlHandshake {
            secret: &secret,
            control_version: CONTROL_PROTOCOL_VERSION,
            session_epoch: TEST_SESSION_EPOCH,
        })
        .unwrap();
        line.push(b'\n');
        valid.write_all(&line).await.unwrap();
        let _connection = timeout(Duration::from_secs(1), pump)
            .await
            .expect("valid control peer must not wait for slow candidates")
            .unwrap()
            .unwrap();
        let mut accepted = [0; crate::control::CONTROL_HANDSHAKE_ACCEPTED_LINE.len()];
        valid.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, crate::control::CONTROL_HANDSHAKE_ACCEPTED_LINE);
        drop(incomplete);
    }

    #[tokio::test]
    async fn initial_control_authentication_pump_allows_valid_peer_past_slowloris() {
        assert_control_authentication_pump_survives_slowloris(MAX_CONTROL_AUTH_CANDIDATES).await;
    }

    #[tokio::test]
    async fn reconnect_control_authentication_pump_allows_valid_peer_past_slowloris() {
        assert_control_authentication_pump_survives_slowloris(MAX_CONTROL_AUTH_CANDIDATES * 2)
            .await;
    }

    #[tokio::test]
    async fn bridge_authentication_pump_allows_valid_peer_past_slowloris() {
        let sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = sidecar.session_descriptor();
        let pump = tokio::spawn(sidecar.start_bridge_authentication_pump());
        let mut incomplete = Vec::new();
        for _ in 0..(MAX_BRIDGE_AUTH_CANDIDATES * 2) {
            let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
            stream.write_all(b"{").await.unwrap();
            incomplete.push(stream);
        }
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let mut valid = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut line = serde_json::to_vec(&TestHandshake {
            secret: descriptor.secret(),
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        line.push(b'\n');
        valid.write_all(&line).await.unwrap();
        let mut connection = timeout(Duration::from_secs(1), pump)
            .await
            .expect("valid bridge peer must not wait for slow candidates")
            .unwrap()
            .unwrap();
        connection.send_handshake_accepted().await.unwrap();
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        valid.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);
        drop(incomplete);
    }

    #[tokio::test]
    async fn full_deferred_shutdown_completes_while_slow_bridge_candidates_are_pending() {
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
        for index in 0..MAX_DEFERRED_ROUTINE_COMMANDS {
            send_command(
                &mut control,
                &abort(
                    &format!("10000000-0000-4000-8000-{index:012x}"),
                    TEST_SESSION_EPOCH - 1,
                    1,
                ),
            )
            .await;
        }

        let mut incomplete = Vec::new();
        for _ in 0..(MAX_BRIDGE_AUTH_CANDIDATES / 2) {
            let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
            stream.write_all(b"{").await.unwrap();
            incomplete.push(stream);
        }
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let shutdown_id = "00000000-0000-4000-8000-00000000003c";
        send_command(&mut control, &shutdown(shutdown_id, TEST_SESSION_EPOCH)).await;

        assert_command_result(&mut control, shutdown_id, CommandStatus::Applied, None).await;
        let mut bridge = TcpStream::connect(descriptor.address()).await.unwrap();
        let mut line = serde_json::to_vec(&TestHandshake {
            secret: descriptor.secret(),
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        line.push(b'\n');
        bridge.write_all(&line).await.unwrap();
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        bridge.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);

        drop(bridge);
        let result = timeout(Duration::from_secs(2), server_task)
            .await
            .expect("quiescing shutdown must complete after bridge EOF")
            .unwrap();
        assert!(result.is_ok(), "server returned {result:?}");
        drop(incomplete);
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

        let save = save_update(2, 0x7856_3412);
        bridge.write_all(&save.encode()).await.unwrap();
        assert_eq!(
            read_event(&mut control).await,
            ControlEvent::SaveDataUpdated {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 1,
                save_sequence: 2,
                save_generation: 0x7856_3412,
            }
        );
        drop(control);
        drop(bridge);
        server_task.abort();
    }

    #[tokio::test]
    async fn save_generation_boundaries_are_lossless_and_one_grant_is_one_shot() {
        for save_generation in [0, u32::MAX] {
            let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
                .await
                .unwrap();
            let key = CheckpointKey::new(TEST_SESSION_EPOCH, 1).unwrap();
            sidecar.sequence_state.commit_checkpoint_ready(key);
            let mut session = ActiveSessionState {
                checkpoint_state: CheckpointState::AwaitSaveData {
                    key,
                    deadline: Instant::now() + SAVE_DATA_TIMEOUT,
                },
                expired_checkpoint: None,
                acknowledged_rom_ready: true,
                rearm_after_reboot: false,
            };
            let (mut bridge, _bridge_peer) = bridge_writer_pair().await;
            let (mut control, mut control_peer) = control_writer_pair().await;
            let frame = save_update(2, save_generation);

            sidecar
                .handle_bridge_frame(frame.clone(), &mut bridge, &mut control, &mut session)
                .await
                .unwrap();
            assert_eq!(
                read_event(&mut control_peer).await,
                ControlEvent::SaveDataUpdated {
                    session_epoch: TEST_SESSION_EPOCH,
                    ready_sequence: 1,
                    save_sequence: 2,
                    save_generation,
                }
            );
            assert_eq!(session.checkpoint_state, CheckpointState::Idle);
            assert_eq!(sidecar.sequence_state.last_session_rom, 2);

            assert!(matches!(
                sidecar
                    .handle_bridge_frame(frame, &mut bridge, &mut control, &mut session)
                    .await,
                Err(SidecarError::ProtocolViolation(
                    "save-data update without grant"
                ))
            ));
            assert!(
                timeout(Duration::from_millis(50), read_event(&mut control_peer))
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn save_update_rejects_every_non_u32_payload_without_advancing_state() {
        for payload in [
            Vec::new(),
            vec![1],
            vec![1, 2],
            vec![1, 2, 3],
            vec![1, 2, 3, 4, 5],
            vec![0; crate::BRIDGE_PAYLOAD_SIZE],
        ] {
            let mut sidecar = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
                .await
                .unwrap();
            let key = CheckpointKey::new(TEST_SESSION_EPOCH, 1).unwrap();
            sidecar.sequence_state.commit_checkpoint_ready(key);
            let mut session = ActiveSessionState {
                checkpoint_state: CheckpointState::AwaitSaveData {
                    key,
                    deadline: Instant::now() + SAVE_DATA_TIMEOUT,
                },
                expired_checkpoint: None,
                acknowledged_rom_ready: true,
                rearm_after_reboot: false,
            };
            let (mut bridge, _bridge_peer) = bridge_writer_pair().await;
            let (mut control, mut control_peer) = control_writer_pair().await;
            let frame = BridgeFrame::new(
                MessageType::SaveDataUpdated,
                2,
                TEST_SESSION_EPOCH,
                &payload,
            )
            .unwrap();

            assert!(matches!(
                sidecar
                    .handle_bridge_frame(frame, &mut bridge, &mut control, &mut session)
                    .await,
                Err(SidecarError::ProtocolViolation("invalid save-data update"))
            ));
            assert!(matches!(
                session.checkpoint_state,
                CheckpointState::AwaitSaveData {
                    key: retained_key,
                    ..
                } if retained_key == key
            ));
            assert_eq!(sidecar.sequence_state.last_session_rom, 1);
            assert!(
                timeout(Duration::from_millis(50), read_event(&mut control_peer))
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn malformed_postgrant_save_frame_terminates_without_a_control_event() {
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
        assert!(matches!(
            read_event(&mut control).await,
            ControlEvent::CheckpointReady { .. }
        ));
        send_command(
            &mut control,
            &grant(
                "00000000-0000-4000-8000-000000000099",
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

        let mut malformed = save_update(2, 33).encode();
        malformed[BRIDGE_FRAME_SIZE - 1] ^= 1;
        bridge.write_all(&malformed).await.unwrap();

        let mut unexpected = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), control.read(&mut unexpected))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(matches!(
            server_task.await.unwrap(),
            Err(SidecarError::Frame(
                FrameCodecError::ChecksumMismatch { .. }
            ))
        ));
    }

    #[tokio::test]
    async fn fragmented_bridge_and_control_inputs_are_reassembled_and_interleaved() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let (mut handshake_prefixes, mut bridge_prefixes, mut control_prefixes) =
            server.observe_reader_prefixes();
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
        write_fragmented_handshake(
            &mut bridge,
            &descriptor,
            descriptor.secret(),
            &mut handshake_prefixes,
        )
        .await;
        establish_rom_session(&mut bridge).await;
        await_prefix(&mut bridge_prefixes, "ROM_READY frame").await;

        let ready =
            BridgeFrame::new(MessageType::CheckpointReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let ready_bytes = ready.encode();
        bridge.write_all(&ready_bytes[..17]).await.unwrap();
        // The bridge reader task owns the partial frame while control remains
        // independently readable; neither input can steal the other's bytes.
        await_prefix(&mut bridge_prefixes, "fragmented CHECKPOINT_READY frame").await;
        send_command(
            &mut control,
            &abort(
                "00000000-0000-4000-8000-000000000010",
                TEST_SESSION_EPOCH,
                1,
            ),
        )
        .await;
        await_prefix(&mut control_prefixes, "abort command").await;
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
        await_prefix(&mut control_prefixes, "fragmented grant command").await;

        // While the grant command is still fragmented, deliver a complete
        // competing bridge event. The sidecar must service it without
        // cancelling or losing the control decoder's partial line.
        let player_state = BridgeFrame::new(
            MessageType::PlayerState,
            2,
            TEST_SESSION_EPOCH,
            &valid_local_presence_state(),
        )
        .unwrap();
        bridge.write_all(&player_state.encode()).await.unwrap();
        await_prefix(&mut bridge_prefixes, "competing player-state frame").await;
        control.write_all(&command_bytes[split..]).await.unwrap();
        assert_presence_and_command_result(&mut control).await;
        let mut granted = [0; BRIDGE_FRAME_SIZE];
        bridge.read_exact(&mut granted).await.unwrap();

        let save = save_update(3, 0x0403_0201);
        let save_bytes = save.encode();
        bridge.write_all(&save_bytes[..23]).await.unwrap();
        await_prefix(&mut bridge_prefixes, "fragmented save-data frame").await;
        bridge.write_all(&save_bytes[23..]).await.unwrap();
        assert_eq!(
            read_event(&mut control).await,
            ControlEvent::SaveDataUpdated {
                session_epoch: TEST_SESSION_EPOCH,
                ready_sequence: 1,
                save_sequence: 3,
                save_generation: 0x0403_0201,
            }
        );
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
        assert_command_result(
            &mut control,
            command_id,
            CommandStatus::Conflict,
            Some(CommandReason::CommandBodyConflict),
        )
        .await;

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
        assert_command_result(
            &mut control,
            "00000000-0000-4000-8000-000000000004",
            CommandStatus::Rejected,
            Some(CommandReason::WrongState),
        )
        .await;
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
    async fn full_routine_ledger_reserves_replayable_shutdown_capacity() {
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

        let command_id = "00000000-0000-4000-8000-000000000ffe";
        let command = shutdown(command_id, TEST_SESSION_EPOCH);
        send_command(&mut control, &command).await;
        assert_command_result(&mut control, command_id, CommandStatus::Applied, None).await;
        send_command(&mut control, &command).await;
        assert_command_result(&mut control, command_id, CommandStatus::Replayed, None).await;
        send_command(&mut control, &abort(command_id, TEST_SESSION_EPOCH, 1)).await;
        assert_command_result(
            &mut control,
            command_id,
            CommandStatus::Conflict,
            Some(CommandReason::CommandBodyConflict),
        )
        .await;

        drop(bridge);
        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn full_routine_ledger_reserves_shutdown_before_replacement_bridge() {
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
        drop(bridge);
        let mut closed = [0_u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(1), control.read(&mut closed))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(control);

        let mut replacement = TcpStream::connect(descriptor.control_address())
            .await
            .unwrap();
        write_control_handshake(
            &mut replacement,
            &descriptor,
            descriptor.control_secret(),
            TEST_SESSION_EPOCH,
        )
        .await;
        let retained_routine_id = "00000000-0000-4000-8000-000000000001";
        send_command(
            &mut replacement,
            &abort(retained_routine_id, TEST_SESSION_EPOCH, 1),
        )
        .await;
        assert_command_result(
            &mut replacement,
            retained_routine_id,
            CommandStatus::Replayed,
            Some(CommandReason::WrongState),
        )
        .await;
        send_command(
            &mut replacement,
            &grant(retained_routine_id, TEST_SESSION_EPOCH, 1),
        )
        .await;
        assert_command_result(
            &mut replacement,
            retained_routine_id,
            CommandStatus::Conflict,
            Some(CommandReason::CommandBodyConflict),
        )
        .await;

        let command = shutdown("00000000-0000-4000-8000-000000000ffd", TEST_SESSION_EPOCH);
        send_command(&mut replacement, &command).await;
        assert_command_result(
            &mut replacement,
            "00000000-0000-4000-8000-000000000ffd",
            CommandStatus::Applied,
            None,
        )
        .await;

        assert!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
        drop(replacement);
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
    fn deferred_checkpoint_command_waits_until_bridge_state_can_decide_it() {
        let command = grant(
            "00000000-0000-4000-8000-00000000002f",
            TEST_SESSION_EPOCH,
            7,
        );
        let mut session = ActiveSessionState {
            checkpoint_state: CheckpointState::Idle,
            expired_checkpoint: None,
            acknowledged_rom_ready: false,
            rearm_after_reboot: false,
        };
        assert!(deferred_command_waits_for_bridge_state(
            &command,
            &session,
            TEST_SESSION_EPOCH
        ));
        session.acknowledged_rom_ready = true;
        assert!(!deferred_command_waits_for_bridge_state(
            &command,
            &session,
            TEST_SESSION_EPOCH
        ));
        let key = CheckpointKey::new(TEST_SESSION_EPOCH, 7).unwrap();
        session.acknowledged_rom_ready = false;
        session.checkpoint_state = CheckpointState::AwaitDecision {
            key,
            deadline: Instant::now() + Duration::from_secs(1),
        };
        assert!(deferred_command_waits_for_bridge_state(
            &command,
            &session,
            TEST_SESSION_EPOCH
        ));
        session.acknowledged_rom_ready = true;
        assert!(!deferred_command_waits_for_bridge_state(
            &command,
            &session,
            TEST_SESSION_EPOCH
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
    fn bridge_lifecycle_requires_clean_eof_after_authentication() {
        let mut lifecycle = BridgeLifecycle::default();
        assert!(lifecycle.control_eof_proves_shutdown());
        assert!(!lifecycle.bridge_exit_proven());
        lifecycle.mark_authenticated();
        assert!(!lifecycle.control_eof_proves_shutdown());
        assert!(!lifecycle.bridge_exit_proven());
        lifecycle.record_termination(&SidecarError::BridgeConnection(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "test bridge RST",
        )));
        assert!(!lifecycle.control_eof_proves_shutdown());
        assert!(!lifecycle.bridge_exit_proven());
        lifecycle.record_termination(&SidecarError::ProtocolViolation("bridge disconnected"));
        assert!(lifecycle.control_eof_proves_shutdown());
        assert!(lifecycle.bridge_exit_proven());
        lifecycle.mark_authenticated();
        assert!(!lifecycle.control_eof_proves_shutdown());
        assert!(!lifecycle.bridge_exit_proven());
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
        assert_eq!(
            read_event(&mut control_peer).await,
            ControlEvent::RomPresenceReset
        );

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
        assert!(matches!(
            timeout(Duration::from_secs(1), server_task)
                .await
                .unwrap()
                .unwrap(),
            Err(SidecarError::Control(ControlError::LineTooLarge))
        ));

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
        assert!(matches!(
            timeout(Duration::from_secs(4), server_task)
                .await
                .unwrap()
                .unwrap(),
            Err(SidecarError::Control(ControlError::ReadTimeout))
        ));
    }
}

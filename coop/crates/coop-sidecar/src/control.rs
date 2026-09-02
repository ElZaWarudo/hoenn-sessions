//! Authenticated, bounded JSONL control protocol for checkpoint decisions.
//!
//! The control channel is intentionally separate from the ROM bridge channel.  It
//! accepts only the typed commands defined here and never exposes a
//! caller-controlled bridge frame API.

use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    time::timeout,
};
use uuid::Uuid;

pub const CONTROL_PROTOCOL_VERSION: u16 = 1;
/// The terminating LF is included in this limit.
pub const MAX_CONTROL_LINE_BYTES: usize = 512;
pub const CONTROL_HANDSHAKE_ACCEPTED_LINE: &[u8] = b"{\"ok\":true}\n";
const CONTROL_TIMEOUT: Duration = Duration::from_secs(3);

/// An independent per-process secret for the control endpoint.
#[derive(Clone)]
pub struct ControlSecret(String);

impl ControlSecret {
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    pub(crate) fn matches(&self, candidate: &str) -> bool {
        if candidate.len() != 32
            || !candidate
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return false;
        }

        // Keep comparison independent of the first differing byte.  This is a
        // local loopback secret, but avoiding a short-circuit compare is cheap.
        let mut difference = 0_u8;
        for (expected, received) in self.0.bytes().zip(candidate.bytes()) {
            difference |= expected ^ received;
        }
        difference == 0
    }
}

impl fmt::Debug for ControlSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ControlSecret([REDACTED])")
    }
}

/// The checkpoint key owned by the ROM bridge.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointKey {
    pub session_epoch: u32,
    pub ready_sequence: u32,
}

impl CheckpointKey {
    #[must_use]
    pub const fn new(session_epoch: u32, ready_sequence: u32) -> Option<Self> {
        if session_epoch == 0 || ready_sequence == 0 {
            return None;
        }
        Some(Self {
            session_epoch,
            ready_sequence,
        })
    }
}

/// UUID command identifiers are strict lowercase canonical UUID strings.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct CommandId(Uuid);

impl CommandId {
    /// Parses a lowercase canonical UUID command identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ControlError::MalformedCommandId`] for any non-canonical or
    /// malformed identifier.
    pub fn parse(value: &str) -> Result<Self, ControlError> {
        if value.len() != 36
            || value.bytes().enumerate().any(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte != b'-'
                } else {
                    !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }
            })
        {
            return Err(ControlError::MalformedCommandId);
        }
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ControlError::MalformedCommandId)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Debug for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for CommandId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.hyphenated().to_string())
    }
}

impl<'de> Deserialize<'de> for CommandId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Commands accepted from the launcher.  There is deliberately no generic
/// frame or byte payload variant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlCommand {
    #[serde(rename = "checkpoint_grant")]
    CheckpointGrant(CheckpointGrant),
    #[serde(rename = "checkpoint_abort")]
    CheckpointAbort(CheckpointAbort),
    #[serde(rename = "shutdown_request")]
    ShutdownRequest(ShutdownRequest),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointGrant {
    pub command_id: CommandId,
    pub session_epoch: u32,
    pub ready_sequence: u32,
}

impl CheckpointGrant {
    #[must_use]
    pub const fn key(&self) -> CheckpointKey {
        CheckpointKey {
            session_epoch: self.session_epoch,
            ready_sequence: self.ready_sequence,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointAbort {
    pub command_id: CommandId,
    pub session_epoch: u32,
    pub ready_sequence: u32,
}

/// Requests an authenticated, epoch-bound sidecar shutdown.  The sidecar
/// records the command before acknowledging it and does not claim that the
/// emulator has already exited.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownRequest {
    pub command_id: CommandId,
    pub session_epoch: u32,
}

impl CheckpointAbort {
    #[must_use]
    pub const fn key(&self) -> CheckpointKey {
        CheckpointKey {
            session_epoch: self.session_epoch,
            ready_sequence: self.ready_sequence,
        }
    }
}

/// Fixed result status; no attacker-controlled reason text crosses the channel.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Applied,
    Replayed,
    Rejected,
    Conflict,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandReason {
    WrongEpoch,
    WrongState,
    StaleCheckpoint,
    InvalidPayload,
    Expired,
    CommandBodyConflict,
}

/// Typed events emitted to the launcher in FIFO order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlEvent {
    #[serde(rename = "checkpoint_ready")]
    CheckpointReady {
        session_epoch: u32,
        ready_sequence: u32,
    },
    #[serde(rename = "save_data_updated")]
    SaveDataUpdated {
        session_epoch: u32,
        ready_sequence: u32,
        save_sequence: u32,
        save_generation: u32,
    },
    #[serde(rename = "checkpoint_expired")]
    CheckpointExpired {
        session_epoch: u32,
        ready_sequence: u32,
    },
    #[serde(rename = "command_result")]
    CommandResult {
        command_id: CommandId,
        status: CommandStatus,
        reason: Option<CommandReason>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlHandshake {
    secret: String,
    control_version: u16,
    session_epoch: u32,
}

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("control listener failed")]
    Listener(#[source] io::Error),
    #[error("control connection failed")]
    Connection(#[source] io::Error),
    #[error("control event write failed")]
    WriteConnection(#[source] io::Error),
    #[error("control peer is not loopback: {0}")]
    NonLoopbackPeer(SocketAddr),
    #[error("control handshake timed out")]
    HandshakeTimeout,
    #[error("control line exceeds the {MAX_CONTROL_LINE_BYTES}-byte limit")]
    LineTooLarge,
    #[error("control line ended before a complete message")]
    LineClosed,
    #[error("malformed control handshake")]
    MalformedHandshake(#[source] serde_json::Error),
    #[error("control authentication failed")]
    AuthenticationFailed,
    #[error("unsupported control protocol version {0}")]
    IncompatibleVersion(u16),
    #[error("control session epoch is invalid")]
    InvalidSessionEpoch,
    #[error("malformed control command")]
    MalformedCommand(#[source] serde_json::Error),
    #[error("malformed command identifier")]
    MalformedCommandId,
    #[error("control write timed out")]
    WriteTimeout,
    #[error("control command read timed out")]
    ReadTimeout,
}

/// A control-only loopback listener.  It has no bridge-frame methods.
#[derive(Clone)]
pub(crate) struct ControlListener {
    listener: Arc<TcpListener>,
    address: SocketAddr,
    secret: ControlSecret,
    session_epoch: u32,
}

impl ControlListener {
    pub(crate) async fn bind(session_epoch: u32) -> Result<Self, ControlError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(ControlError::Listener)?;
        let address = listener.local_addr().map_err(ControlError::Listener)?;
        Ok(Self {
            listener: Arc::new(listener),
            address,
            secret: ControlSecret::generate(),
            session_epoch,
        })
    }

    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn secret(&self) -> &ControlSecret {
        &self.secret
    }

    #[cfg(test)]
    pub(crate) async fn accept(&self) -> Result<ControlConnection, ControlError> {
        let (stream, peer) = self.accept_stream().await?;
        self.authenticate_stream(stream, peer).await
    }

    pub(crate) async fn accept_stream(&self) -> Result<(TcpStream, SocketAddr), ControlError> {
        self.listener.accept().await.map_err(ControlError::Listener)
    }

    pub(crate) async fn authenticate_stream(
        &self,
        stream: TcpStream,
        peer: SocketAddr,
    ) -> Result<ControlConnection, ControlError> {
        ControlConnection::authenticate(stream, peer, &self.secret, self.session_epoch).await
    }
}

/// Authenticated control stream.  Exactly one task owns writes to this stream.
pub(crate) struct ControlConnection {
    stream: TcpStream,
}

impl ControlConnection {
    #[cfg(test)]
    pub(crate) fn from_authenticated_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    async fn authenticate(
        mut stream: TcpStream,
        peer: SocketAddr,
        expected_secret: &ControlSecret,
        expected_epoch: u32,
    ) -> Result<Self, ControlError> {
        if !peer.ip().is_loopback() {
            return Err(ControlError::NonLoopbackPeer(peer));
        }
        stream.set_nodelay(true).map_err(ControlError::Connection)?;
        let line = timeout(CONTROL_TIMEOUT, read_bounded_line(&mut stream))
            .await
            .map_err(|_| ControlError::HandshakeTimeout)??;
        let handshake: ControlHandshake =
            serde_json::from_slice(&line).map_err(ControlError::MalformedHandshake)?;
        if handshake.control_version != CONTROL_PROTOCOL_VERSION {
            return Err(ControlError::IncompatibleVersion(handshake.control_version));
        }
        if handshake.session_epoch == 0 || handshake.session_epoch != expected_epoch {
            return Err(ControlError::InvalidSessionEpoch);
        }
        if !expected_secret.matches(&handshake.secret) {
            return Err(ControlError::AuthenticationFailed);
        }
        timeout(
            CONTROL_TIMEOUT,
            stream.write_all(CONTROL_HANDSHAKE_ACCEPTED_LINE),
        )
        .await
        .map_err(|_| ControlError::WriteTimeout)?
        .map_err(ControlError::Connection)?;
        Ok(Self { stream })
    }

    pub(crate) fn into_split(self) -> (ControlReader, ControlWriter) {
        let (reader, writer) = self.stream.into_split();
        (
            ControlReader { stream: reader },
            ControlWriter { stream: writer },
        )
    }
}

pub(crate) struct ControlReader {
    stream: OwnedReadHalf,
}

impl ControlReader {
    /// Read exactly one bounded command line.  A partial line is an error and
    /// cannot be retried on this connection.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when the line is incomplete, oversized, or
    /// does not deserialize to one of the typed commands.
    pub(crate) async fn receive_command(&mut self) -> Result<ControlCommand, ControlError> {
        receive_bounded_command(&mut self.stream).await
    }

    #[cfg(test)]
    pub(crate) async fn receive_command_observed<F>(
        &mut self,
        prefix_observer: F,
    ) -> Result<ControlCommand, ControlError>
    where
        F: FnOnce() + Send,
    {
        receive_bounded_command_observed(&mut self.stream, prefix_observer).await
    }
}

pub(crate) struct ControlWriter {
    stream: OwnedWriteHalf,
}

impl ControlWriter {
    /// Write one complete event line with a bounded timeout.  The sidecar has
    /// one writer owner, so event order cannot be interleaved.
    ///
    /// # Errors
    ///
    /// Returns a protocol error when serialization, bounded I/O, or the
    /// authenticated connection fails.
    pub(crate) async fn send_event(&mut self, event: &ControlEvent) -> Result<(), ControlError> {
        write_bounded_event(&mut self.stream, event).await
    }
}

async fn receive_bounded_command(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<ControlCommand, ControlError> {
    receive_bounded_command_observed(stream, || {}).await
}

async fn receive_bounded_command_observed<F>(
    stream: &mut (impl AsyncRead + Unpin),
    prefix_observer: F,
) -> Result<ControlCommand, ControlError>
where
    F: FnOnce() + Send,
{
    // Do not put an idle control connection on an inactivity deadline. A
    // deadline starts only after the first byte, making a genuinely
    // fragmented command fail closed without mistaking an idle launcher
    // for a lost peer.
    let mut line = Vec::with_capacity(128);
    let mut first = [0_u8; 1];
    let count = stream
        .read(&mut first)
        .await
        .map_err(ControlError::Connection)?;
    if count == 0 {
        return Err(ControlError::LineClosed);
    }
    if first[0] == b'\n' {
        return Err(ControlError::MalformedCommand(
            serde_json::from_slice::<ControlCommand>(&[]).unwrap_err(),
        ));
    }
    line.push(first[0]);
    prefix_observer();
    let line = timeout(CONTROL_TIMEOUT, read_bounded_line_with_prefix(stream, line))
        .await
        .map_err(|_| ControlError::ReadTimeout)??;
    serde_json::from_slice(&line).map_err(ControlError::MalformedCommand)
}

async fn write_bounded_event(
    stream: &mut (impl AsyncWrite + Unpin),
    event: &ControlEvent,
) -> Result<(), ControlError> {
    let mut line = serde_json::to_vec(event).map_err(|error| {
        ControlError::Connection(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    line.push(b'\n');
    if line.len() > MAX_CONTROL_LINE_BYTES {
        return Err(ControlError::LineTooLarge);
    }
    timeout(CONTROL_TIMEOUT, stream.write_all(&line))
        .await
        .map_err(|_| ControlError::WriteTimeout)?
        .map_err(ControlError::WriteConnection)
}

pub(crate) async fn read_bounded_line(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<Vec<u8>, ControlError> {
    read_bounded_line_with_prefix(stream, Vec::with_capacity(128)).await
}

async fn read_bounded_line_with_prefix(
    stream: &mut (impl AsyncRead + Unpin),
    mut line: Vec<u8>,
) -> Result<Vec<u8>, ControlError> {
    let mut byte = [0_u8; 1];
    loop {
        let count = stream
            .read(&mut byte)
            .await
            .map_err(ControlError::Connection)?;
        if count == 0 {
            return Err(ControlError::LineClosed);
        }
        if line.len() + 1 > MAX_CONTROL_LINE_BYTES {
            return Err(ControlError::LineTooLarge);
        }
        if byte[0] == b'\n' {
            return Ok(line);
        }
        line.push(byte[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    fn command_id() -> CommandId {
        CommandId::parse("00000000-0000-4000-8000-000000000001").unwrap()
    }

    #[test]
    fn secrets_are_redacted_and_command_ids_are_strict() {
        let secret = ControlSecret::generate();
        assert_eq!(secret.expose().len(), 32);
        assert_eq!(format!("{secret:?}"), "ControlSecret([REDACTED])");
        assert!(CommandId::parse("00000000-0000-4000-8000-000000000001").is_ok());
        assert!(CommandId::parse("00000000-0000-4000-8000-00000000000A").is_err());
        assert!(CommandId::parse("not-a-command").is_err());
    }

    #[test]
    fn commands_are_typed_and_have_no_arbitrary_frame_variant() {
        let command = ControlCommand::CheckpointGrant(CheckpointGrant {
            command_id: command_id(),
            session_epoch: 7,
            ready_sequence: 9,
        });
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("checkpoint_grant"));
        assert!(
            serde_json::from_str::<ControlCommand>(r#"{"type":"raw_frame","bytes":"AA"}"#).is_err()
        );
        assert!(serde_json::from_str::<ControlCommand>(&format!(
            r#"{{"type":"checkpoint_grant","command_id":"{}","session_epoch":7,"ready_sequence":9,"extra":true}}"#,
            command_id().0
        ))
        .is_err());

        let shutdown = ControlCommand::ShutdownRequest(ShutdownRequest {
            command_id: command_id(),
            session_epoch: 7,
        });
        let json = serde_json::to_string(&shutdown).unwrap();
        assert_eq!(
            json,
            r#"{"type":"shutdown_request","command_id":"00000000-0000-4000-8000-000000000001","session_epoch":7}"#
        );
        assert_eq!(
            serde_json::from_str::<ControlCommand>(&json).unwrap(),
            shutdown
        );
        assert!(serde_json::from_str::<ControlCommand>(
            r#"{"type":"shutdown_request","command_id":"00000000-0000-4000-8000-000000000001","session_epoch":7,"extra":true}"#
        )
        .is_err());
    }

    #[test]
    fn save_update_events_preserve_the_full_generation_domain() {
        for save_generation in [0, u32::MAX] {
            let event = ControlEvent::SaveDataUpdated {
                session_epoch: 7,
                ready_sequence: 9,
                save_sequence: 10,
                save_generation,
            };
            let json = serde_json::to_string(&event).unwrap();
            assert!(json.contains(&format!(r#""save_generation":{save_generation}"#)));
            assert_eq!(serde_json::from_str::<ControlEvent>(&json).unwrap(), event);
        }
    }

    #[tokio::test]
    async fn control_listener_rejects_cross_use_and_wrong_epoch() {
        let listener = ControlListener::bind(7).await.unwrap();
        let address = listener.address();
        let expected = listener.secret().expose().to_owned();
        let wrong = ControlSecret::generate().expose().to_owned();
        let task = tokio::spawn(async move { listener.accept().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let line = format!(
            "{{\"secret\":\"{wrong}\",\"control_version\":{CONTROL_PROTOCOL_VERSION},\"session_epoch\":7}}\n"
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(ControlError::AuthenticationFailed)
        ));
        drop(expected);
    }

    #[tokio::test]
    async fn bounded_control_handshake_rejects_version_oversize_and_partial_lines() {
        let listener = ControlListener::bind(7).await.unwrap();
        let address = listener.address();
        let secret = listener.secret().expose().to_owned();
        let task = tokio::spawn(async move { listener.accept().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let line =
            format!("{{\"secret\":\"{secret}\",\"control_version\":2,\"session_epoch\":7}}\n");
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(ControlError::IncompatibleVersion(2))
        ));

        let listener = ControlListener::bind(7).await.unwrap();
        let address = listener.address();
        let task = tokio::spawn(async move { listener.accept().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream
            .write_all(&vec![b'x'; MAX_CONTROL_LINE_BYTES + 1])
            .await
            .unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(ControlError::LineTooLarge)
        ));

        let listener = ControlListener::bind(7).await.unwrap();
        let address = listener.address();
        let task = tokio::spawn(async move { listener.accept().await });
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(b"{\"secret\":").await.unwrap();
        drop(stream);
        assert!(matches!(task.await.unwrap(), Err(ControlError::LineClosed)));
    }
}

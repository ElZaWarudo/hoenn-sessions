use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use uuid::Uuid;

use crate::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BridgeFrame, Direction, FrameCodecError,
    GAME_PROTOCOL_VERSION, MessageType,
};

/// Includes the terminating LF. A client cannot make the sidecar buffer more than this.
pub const MAX_HANDSHAKE_BYTES: usize = 256;
pub const HANDSHAKE_ACCEPTED_LINE: &[u8] = b"{\"ok\":true}\n";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const BOOTSTRAP_SEQUENCE: u32 = 1;

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

/// Printed once by the CLI so the launcher can configure the Lua bridge.
#[derive(Clone, Serialize)]
pub struct SessionDescriptor {
    transport: &'static str,
    host: &'static str,
    port: u16,
    secret: String,
    bridge_abi: u16,
    protocol_version: u16,
    frame_bytes: usize,
}

impl SessionDescriptor {
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
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
    #[error("rejected non-loopback peer {0}")]
    NonLoopbackPeer(SocketAddr),
    #[error("handshake timed out")]
    HandshakeTimeout,
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
}

/// The authenticated, frame-oriented view of a local TCP connection.
pub struct AuthenticatedConnection {
    stream: TcpStream,
    peer: SocketAddr,
}

impl AuthenticatedConnection {
    #[must_use]
    pub const fn peer_addr(&self) -> SocketAddr {
        self.peer
    }

    /// Reads exactly one fixed-size frame. No unbounded application queue is used: callers
    /// must process a frame before another is read, so TCP backpressure is preserved.
    ///
    /// # Errors
    ///
    /// Returns an error for connection I/O failures or any invalid/directionally illegal frame.
    pub async fn receive(
        &mut self,
        expected_direction: Direction,
    ) -> Result<Option<BridgeFrame>, SidecarError> {
        let mut bytes = [0; BRIDGE_FRAME_SIZE];
        let first_byte_count = self
            .stream
            .read(&mut bytes[..1])
            .await
            .map_err(SidecarError::Connection)?;
        if first_byte_count == 0 {
            return Ok(None);
        }
        self.stream
            .read_exact(&mut bytes[1..])
            .await
            .map_err(SidecarError::Connection)?;
        Ok(Some(BridgeFrame::decode_for(&bytes, expected_direction)?))
    }

    /// Writes one complete frame and awaits the socket, preserving TCP backpressure.
    ///
    /// # Errors
    ///
    /// Returns an error for a direction mismatch or connection I/O failure.
    pub async fn send(
        &mut self,
        frame: &BridgeFrame,
        expected_direction: Direction,
    ) -> Result<(), SidecarError> {
        frame.ensure_direction(expected_direction)?;
        self.stream
            .write_all(&frame.encode())
            .await
            .map_err(SidecarError::Connection)
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

    fn accept_rom_frame(&mut self, frame: &BridgeFrame, session_epoch: u32) -> bool {
        let previous = if frame.session_epoch() == 0 {
            if frame.message_type() != MessageType::RomReady {
                return false;
            }
            &mut self.last_boot_rom
        } else if frame.session_epoch() == session_epoch {
            &mut self.last_session_rom
        } else {
            return false;
        };

        if !is_sequence_newer(frame.sequence(), *previous) {
            return false;
        }
        *previous = frame.sequence();
        true
    }
}

fn is_sequence_newer(sequence: u32, previous: u32) -> bool {
    sequence != 0 && (previous == 0 || sequence.wrapping_sub(previous).cast_signed() > 0)
}

/// A loopback-only listener. `serve` handles clients serially, so at most one is active.
pub struct LocalSidecar {
    listener: TcpListener,
    address: SocketAddr,
    secret: SessionSecret,
    session_epoch: u32,
    sequence_state: SessionSequenceState,
}

impl LocalSidecar {
    /// Binds with an explicit non-zero session epoch supplied by the launcher.
    /// The launcher owns persistence and wrapping-monotonic increment semantics.
    ///
    /// # Errors
    ///
    /// Returns an error for epoch zero or when the loopback listener cannot be created.
    pub async fn bind_with_epoch(session_epoch: u32) -> Result<Self, SidecarError> {
        if session_epoch == 0 {
            return Err(SidecarError::SessionEpochZero);
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(SidecarError::Listener)?;
        let address = listener.local_addr().map_err(SidecarError::Listener)?;
        Ok(Self {
            listener,
            address,
            secret: SessionSecret::generate(),
            session_epoch,
            sequence_state: SessionSequenceState::default(),
        })
    }

    #[must_use]
    pub fn session_descriptor(&self) -> SessionDescriptor {
        SessionDescriptor {
            transport: "tcp",
            host: "127.0.0.1",
            port: self.address.port(),
            secret: self.secret.expose().to_owned(),
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
            frame_bytes: BRIDGE_FRAME_SIZE,
        }
    }

    /// Accepts exactly one peer and completes the bounded, secret-bearing handshake.
    ///
    /// # Errors
    ///
    /// Returns an error for listener failure, non-loopback peers, invalid authentication,
    /// incompatible versions, connection I/O, or bootstrap-frame construction.
    pub async fn accept(&mut self) -> Result<AuthenticatedConnection, SidecarError> {
        let (stream, peer) = self
            .listener
            .accept()
            .await
            .map_err(SidecarError::Listener)?;
        self.authenticate(stream, peer).await
    }

    /// Serves clients one at a time. Invalid handshakes and frame failures only disconnect
    /// that client; an error on the listener itself terminates the process.
    ///
    /// # Errors
    ///
    /// Returns an error only when the underlying listener can no longer accept clients.
    pub async fn serve(mut self) -> Result<(), SidecarError> {
        loop {
            let mut connection = match self.accept().await {
                Ok(connection) => connection,
                Err(error @ SidecarError::Listener(_)) => return Err(error),
                Err(_) => continue,
            };

            // Client failures cannot terminate the listener. Sequence and replay state
            // intentionally survive so a reconnect cannot rewind either sequence domain.
            let _client_result = self.serve_authenticated_client(&mut connection).await;
        }
    }

    /// Accepts and serves exactly one authenticated client. Primarily useful to embedders
    /// and deterministic integration tests.
    ///
    /// # Errors
    ///
    /// Returns any handshake, connection, or frame error raised by that client.
    pub async fn serve_one(&mut self) -> Result<(), SidecarError> {
        let mut connection = self.accept().await?;
        self.serve_authenticated_client(&mut connection).await
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
        if !self.secret.matches(&request.secret) {
            return Err(SidecarError::AuthenticationFailed);
        }

        stream
            .write_all(HANDSHAKE_ACCEPTED_LINE)
            .await
            .map_err(SidecarError::Connection)?;
        let mut connection = AuthenticatedConnection { stream, peer };
        self.send_session_ready(&mut connection).await?;
        Ok(connection)
    }

    async fn send_session_ready(
        &mut self,
        connection: &mut AuthenticatedConnection,
    ) -> Result<(), SidecarError> {
        let sequence = self.sequence_state.take_sidecar_sequence();
        let frame = BridgeFrame::new(MessageType::SessionReady, sequence, self.session_epoch, &[])?;
        connection.send(&frame, Direction::SidecarToRom).await
    }

    async fn serve_authenticated_client(
        &mut self,
        connection: &mut AuthenticatedConnection,
    ) -> Result<(), SidecarError> {
        let mut acknowledged_rom_ready = false;
        while let Some(frame) = connection.receive(Direction::RomToSidecar).await? {
            if !self
                .sequence_state
                .accept_rom_frame(&frame, self.session_epoch)
            {
                continue;
            }

            if frame.message_type() == MessageType::RomReady && !acknowledged_rom_ready {
                self.send_session_ready(connection).await?;
                acknowledged_rom_ready = true;
            }
        }
        Ok(())
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
    use tokio::net::TcpStream;

    use super::*;

    const TEST_SESSION_EPOCH: u32 = 41;

    #[derive(Serialize)]
    struct TestHandshake<'a> {
        secret: &'a str,
        bridge_abi: u16,
        protocol_version: u16,
    }

    async fn write_handshake(stream: &mut TcpStream, descriptor: &SessionDescriptor, secret: &str) {
        let mut line = serde_json::to_vec(&TestHandshake {
            secret,
            bridge_abi: BRIDGE_ABI_VERSION,
            protocol_version: GAME_PROTOCOL_VERSION,
        })
        .unwrap();
        assert!(line.len() < MAX_HANDSHAKE_BYTES);
        line.push(b'\n');
        stream.write_all(&line).await.unwrap();
        assert_eq!(stream.peer_addr().unwrap(), descriptor.address());
    }

    async fn connect_and_authenticate(
        descriptor: &SessionDescriptor,
        expected_epoch: u32,
        expected_bootstrap_sequence: u32,
    ) -> TcpStream {
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut stream, descriptor, descriptor.secret()).await;
        let mut accepted = [0; HANDSHAKE_ACCEPTED_LINE.len()];
        stream.read_exact(&mut accepted).await.unwrap();
        assert_eq!(accepted, HANDSHAKE_ACCEPTED_LINE);

        let mut bootstrap_bytes = [0; BRIDGE_FRAME_SIZE];
        stream.read_exact(&mut bootstrap_bytes).await.unwrap();
        let bootstrap = BridgeFrame::decode_for(&bootstrap_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(bootstrap.message_type(), MessageType::SessionReady);
        assert_eq!(bootstrap.sequence(), expected_bootstrap_sequence);
        assert_eq!(bootstrap.session_epoch(), expected_epoch);
        assert!(bootstrap.payload().is_empty());
        stream
    }

    #[tokio::test]
    async fn valid_handshake_is_accepted_and_bootstraps_session() {
        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        assert_eq!(descriptor.address().ip(), Ipv4Addr::LOCALHOST);
        assert_eq!(descriptor.secret().len(), 32);
        assert!(
            descriptor
                .secret()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );

        let server_task = tokio::spawn(async move { server.accept().await });
        let stream =
            connect_and_authenticate(&descriptor, TEST_SESSION_EPOCH, BOOTSTRAP_SEQUENCE).await;
        let connection = server_task.await.unwrap().unwrap();
        assert!(connection.peer_addr().ip().is_loopback());
        drop(stream);
    }

    #[tokio::test]
    async fn invalid_oversized_and_unknown_field_handshakes_fail_closed() {
        assert!(matches!(
            LocalSidecar::bind_with_epoch(0).await,
            Err(SidecarError::SessionEpochZero)
        ));

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(async move { server.accept().await });
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        write_handshake(&mut stream, &descriptor, "00000000000000000000000000000000").await;
        assert!(matches!(
            server_task.await.unwrap(),
            Err(SidecarError::AuthenticationFailed)
        ));
        assert_connection_closed(&mut stream).await;

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(async move { server.accept().await });
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        stream
            .write_all(&vec![b'x'; MAX_HANDSHAKE_BYTES + 1])
            .await
            .unwrap();
        assert!(matches!(
            server_task.await.unwrap(),
            Err(SidecarError::HandshakeTooLarge)
        ));

        let mut server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(async move { server.accept().await });
        let mut stream = TcpStream::connect(descriptor.address()).await.unwrap();
        let line = format!(
            "{{\"secret\":\"{}\",\"bridge_abi\":1,\"protocol_version\":1,\"extra\":true}}\n",
            descriptor.secret()
        );
        stream.write_all(line.as_bytes()).await.unwrap();
        assert!(matches!(
            server_task.await.unwrap(),
            Err(SidecarError::MalformedHandshake(_))
        ));
        assert_connection_closed(&mut stream).await;
    }

    async fn assert_connection_closed(stream: &mut TcpStream) {
        let mut byte = [0];
        match stream.read(&mut byte).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("expected a closed connection, got {result:?}"),
        }
    }

    #[test]
    fn sequence_state_wraps_without_zero_and_separates_allowed_rom_epochs() {
        let mut state = SessionSequenceState {
            next_sidecar: u32::MAX,
            ..SessionSequenceState::default()
        };
        assert_eq!(state.take_sidecar_sequence(), u32::MAX);
        assert_eq!(state.take_sidecar_sequence(), 1);

        let boot = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        let invalid_boot = BridgeFrame::new(MessageType::PlayerState, 2, 0, &[]).unwrap();
        let negotiated =
            BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        let unrelated =
            BridgeFrame::new(MessageType::RomReady, 2, TEST_SESSION_EPOCH - 1, &[]).unwrap();

        assert!(!state.accept_rom_frame(&invalid_boot, TEST_SESSION_EPOCH));
        assert!(state.accept_rom_frame(&boot, TEST_SESSION_EPOCH));
        assert!(state.accept_rom_frame(&negotiated, TEST_SESSION_EPOCH));
        assert!(!state.accept_rom_frame(&boot, TEST_SESSION_EPOCH));
        assert!(!state.accept_rom_frame(&negotiated, TEST_SESSION_EPOCH));
        assert!(!state.accept_rom_frame(&unrelated, TEST_SESSION_EPOCH));

        state.last_session_rom = u32::MAX;
        assert!(state.accept_rom_frame(&negotiated, TEST_SESSION_EPOCH));
    }

    #[tokio::test]
    async fn authenticated_loopback_exchanges_raw_frames_in_both_directions() {
        const TEST_EPOCH: u32 = 41;
        const FRESH_EPOCH: u32 = 42;

        let mut server = LocalSidecar::bind_with_epoch(TEST_EPOCH).await.unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(async move {
            let mut connection = server.accept().await.unwrap();
            let rom_ready = connection
                .receive(Direction::RomToSidecar)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(rom_ready.message_type(), MessageType::RomReady);
            assert_eq!(rom_ready.sequence(), 1);

            let fresh_session =
                BridgeFrame::new(MessageType::SessionReady, 2, FRESH_EPOCH, &[]).unwrap();
            connection
                .send(&fresh_session, Direction::SidecarToRom)
                .await
                .unwrap();

            let player_state = connection
                .receive(Direction::RomToSidecar)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(player_state.message_type(), MessageType::PlayerState);
            assert_eq!(player_state.payload(), &[1, 2, 3]);
        });

        let mut stream =
            connect_and_authenticate(&descriptor, TEST_EPOCH, BOOTSTRAP_SEQUENCE).await;
        let rom_ready = BridgeFrame::new(MessageType::RomReady, 1, 0, &[]).unwrap();
        stream.write_all(&rom_ready.encode()).await.unwrap();

        let mut fresh_bytes = [0; BRIDGE_FRAME_SIZE];
        stream.read_exact(&mut fresh_bytes).await.unwrap();
        let fresh = BridgeFrame::decode_for(&fresh_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(fresh.message_type(), MessageType::SessionReady);
        assert_eq!(fresh.sequence(), 2);
        assert_eq!(fresh.session_epoch(), FRESH_EPOCH);

        let player_state =
            BridgeFrame::new(MessageType::PlayerState, 2, FRESH_EPOCH, &[1, 2, 3]).unwrap();
        stream.write_all(&player_state.encode()).await.unwrap();
        drop(stream);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn reconnect_preserves_both_sequence_domains_for_the_same_epoch() {
        let server = LocalSidecar::bind_with_epoch(TEST_SESSION_EPOCH)
            .await
            .unwrap();
        let descriptor = server.session_descriptor();
        let server_task = tokio::spawn(server.serve());

        let mut first = connect_and_authenticate(&descriptor, TEST_SESSION_EPOCH, 1).await;
        let rom_ready =
            BridgeFrame::new(MessageType::RomReady, 1, TEST_SESSION_EPOCH, &[]).unwrap();
        first.write_all(&rom_ready.encode()).await.unwrap();

        let mut response_bytes = [0; BRIDGE_FRAME_SIZE];
        first.read_exact(&mut response_bytes).await.unwrap();
        let response = BridgeFrame::decode_for(&response_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(response.message_type(), MessageType::SessionReady);
        assert_eq!(response.sequence(), 2);
        assert_eq!(response.session_epoch(), TEST_SESSION_EPOCH);
        drop(first);

        let mut second = connect_and_authenticate(&descriptor, TEST_SESSION_EPOCH, 3).await;
        second.write_all(&rom_ready.encode()).await.unwrap();

        let mut unexpected_byte = [0];
        assert!(
            timeout(
                Duration::from_millis(250),
                second.read(&mut unexpected_byte)
            )
            .await
            .is_err(),
            "a duplicate ROM frame must be dropped without producing a response"
        );

        let newer_rom_ready =
            BridgeFrame::new(MessageType::RomReady, 2, TEST_SESSION_EPOCH, &[]).unwrap();
        second.write_all(&newer_rom_ready.encode()).await.unwrap();
        second.read_exact(&mut response_bytes).await.unwrap();
        let response = BridgeFrame::decode_for(&response_bytes, Direction::SidecarToRom).unwrap();
        assert_eq!(response.message_type(), MessageType::SessionReady);
        assert_eq!(response.sequence(), 4);
        assert_eq!(response.session_epoch(), TEST_SESSION_EPOCH);

        drop(second);
        server_task.abort();
        assert!(server_task.await.unwrap_err().is_cancelled());
    }
}

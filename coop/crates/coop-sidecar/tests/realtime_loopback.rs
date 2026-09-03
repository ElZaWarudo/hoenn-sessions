use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use coop_cloud::{
    ClientRealtimeFrameV1, RealtimeTicket, ServerRealtimeFrameV1, UnixTimestampMillis,
    encode_server_realtime_frame,
};
use coop_protocol::{
    AnimationId, AvatarId, CanonicalUsername, DespawnReason, Direction, LocalPresenceStateV1,
    MovementMode, PlayerState, PresenceHandle, PresenceInteractionV1, PresencePoseV1, RegionId,
    RemotePlayerDespawnV1, RemotePlayerSpawnV1, RemotePlayerUpdateV1, WorldLocation,
};
use coop_sidecar::realtime::{
    MAX_INTERACTION_QUEUE, RealtimeEndpoint, RealtimeError, RealtimeGrant, RealtimeOutcome,
    RealtimeOwnerEvent, realtime_channel, run_realtime,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{accept_hdr_async, tungstenite::Message};

fn state(source_sequence: u32, client_tick: u32) -> LocalPresenceStateV1 {
    state_with_player_state(source_sequence, client_tick, PlayerState::Overworld)
}

fn hidden_state(source_sequence: u32, client_tick: u32) -> LocalPresenceStateV1 {
    state_with_player_state(source_sequence, client_tick, PlayerState::Hidden)
}

fn state_with_player_state(
    source_sequence: u32,
    client_tick: u32,
    player_state: PlayerState,
) -> LocalPresenceStateV1 {
    let location = WorldLocation::new(RegionId::Hoenn, 1, 0, 4, 5).unwrap();
    let pose = PresencePoseV1::new(
        location,
        0,
        Direction::South,
        client_tick,
        1,
        MovementMode::Idle,
        AnimationId::Idle,
        AvatarId::Brendan,
        player_state,
    )
    .unwrap();
    LocalPresenceStateV1::new(pose, source_sequence).unwrap()
}

fn grant(port: u16) -> RealtimeGrant {
    grant_with_ttl(port, 30_000)
}

fn grant_with_ttl(port: u16, ttl_ms: u64) -> RealtimeGrant {
    let ticket = RealtimeTicket::from_bytes([7; 32]).unwrap();
    let endpoint = RealtimeEndpoint::new(format!("ws://127.0.0.1:{port}/v1/realtime")).unwrap();
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    RealtimeGrant::with_now(
        ticket,
        endpoint,
        UnixTimestampMillis::new(now + ttl_ms),
        UnixTimestampMillis::new(now),
    )
    .unwrap()
}

async fn accept_server(
    listener: TcpListener,
    observed: Arc<Mutex<Option<(bool, bool, bool)>>>,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    let (stream, _) = listener.accept().await.unwrap();
    let expected_authorization = format!(
        "Bearer {}",
        RealtimeTicket::from_bytes([7; 32]).unwrap().expose_secret()
    );
    accept_hdr_async(
        stream,
        |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
            let authorization_exact = request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value == expected_authorization);
            let has_query = request.uri().query().is_some();
            let has_subprotocol = request.headers().contains_key("sec-websocket-protocol");
            *observed.lock().unwrap() = Some((authorization_exact, has_query, has_subprotocol));
            Ok(response)
        },
    )
    .await
    .unwrap()
}

async fn send_frame(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    frame: ServerRealtimeFrameV1,
) {
    let bytes = encode_server_realtime_frame(&frame).unwrap();
    socket
        .send(Message::Text(String::from_utf8(bytes).unwrap().into()))
        .await
        .unwrap();
}

#[test]
fn realtime_endpoint_boundaries_are_canonical_and_loopback_restricted() {
    assert!(RealtimeEndpoint::new("ws://127.0.0.1:3000/v1/realtime").is_ok());
    assert!(RealtimeEndpoint::new("ws://[::1]:3000/v1/realtime").is_ok());
    assert!(RealtimeEndpoint::new("wss://example.com/v1/realtime").is_ok());
    let standard_loopback =
        RealtimeEndpoint::new("ws://127.0.0.1/v1/realtime").expect("standard ws port");
    assert_eq!(standard_loopback.port(), Some(80));
    for invalid in [
        "http://127.0.0.1:3000/v1/realtime",
        "ws://localhost:3000/v1/realtime",
        "ws://127.0.0.1:0/v1/realtime",
        "ws://127.0.0.1:3000/v1/realtime/",
        "ws://127.0.0.1:3000/v1/realtime?ticket=secret",
        "ws://user:password@127.0.0.1:3000/v1/realtime",
        "wss://example.com:0/v1/realtime",
        "wss://example.com./v1/realtime",
    ] {
        assert_eq!(
            RealtimeEndpoint::new(invalid),
            Err(RealtimeError::InvalidEndpoint)
        );
    }
    assert_eq!(
        RealtimeEndpoint::new(format!(
            "ws://127.0.0.1:3000/v1/realtime{}",
            "x".repeat(300)
        )),
        Err(RealtimeError::InvalidEndpoint)
    );
}

#[test]
fn realtime_grant_ttl_and_secret_debug_boundaries_are_stable() {
    let ticket = RealtimeTicket::from_bytes([3; 32]).unwrap();
    let endpoint = RealtimeEndpoint::new("ws://127.0.0.1:3000/v1/realtime").unwrap();
    assert!(
        RealtimeGrant::with_now(
            ticket,
            endpoint,
            UnixTimestampMillis::new(1_000_001),
            UnixTimestampMillis::new(1_000_000),
        )
        .is_ok()
    );
    let ticket = RealtimeTicket::from_bytes([4; 32]).unwrap();
    let endpoint = RealtimeEndpoint::new("ws://127.0.0.1:3000/v1/realtime").unwrap();
    assert_eq!(
        RealtimeGrant::with_now(
            ticket,
            endpoint,
            UnixTimestampMillis::new(1_040_001),
            UnixTimestampMillis::new(1_000_000),
        )
        .unwrap_err(),
        RealtimeError::InvalidGrant
    );
    let ticket = RealtimeTicket::from_bytes([5; 32]).unwrap();
    let endpoint = RealtimeEndpoint::new("ws://127.0.0.1:3000/v1/realtime").unwrap();
    let grant = RealtimeGrant::with_now(
        ticket,
        endpoint,
        UnixTimestampMillis::new(1_000_001),
        UnixTimestampMillis::new(1_000_000),
    )
    .unwrap();
    let rendered = format!("{grant:?}");
    assert!(rendered.contains("REDACTED"));
    assert!(!rendered.contains("BQ"));
}

#[tokio::test]
async fn realtime_one_attempt_sends_cached_state_first_and_decodes_lifecycle() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let observed_server = Arc::clone(&observed);
    let cached = state(1, 11);
    let remote_handle = PresenceHandle::new(0x22).unwrap();
    let remote_user = CanonicalUsername::new("misty").unwrap();
    let spawn = RemotePlayerSpawnV1::new(remote_handle, 1, cached.clone(), remote_user).unwrap();
    let update = RemotePlayerUpdateV1::new(remote_handle, 2, state(2, 12)).unwrap();
    let despawn =
        RemotePlayerDespawnV1::new(remote_handle, 3, DespawnReason::Disconnected).unwrap();
    let expected_cached = cached.clone();
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed_server).await;
        let first = timeout(Duration::from_secs(1), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(first) = first else {
            panic!("first frame was not text")
        };
        assert_eq!(
            serde_json::from_str::<ClientRealtimeFrameV1>(first.as_ref()).unwrap(),
            ClientRealtimeFrameV1::player_state(expected_cached)
        );
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap()),
        )
        .await;
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::remote_player_spawn(spawn),
        )
        .await;
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::remote_player_update(update),
        )
        .await;
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::remote_player_despawn(despawn),
        )
        .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let (mut owner, driver) = realtime_channel(cached.clone()).unwrap();
    let run = tokio::spawn(run_realtime(grant(port), driver));
    assert_eq!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Ready(coop_cloud::PresenceReadyV1::new(
            PresenceHandle::new(1).unwrap()
        )))
    );
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Spawn(_))
    ));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Update(_))
    ));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Despawn(_))
    ));
    owner.stop();
    let outcome = timeout(Duration::from_secs(1), run).await.unwrap().unwrap();
    assert_eq!(outcome, RealtimeOutcome::OwnerStopped);
    server.await.unwrap();
    let observed = (*observed.lock().unwrap()).unwrap();
    assert!(observed.0);
    assert!(!observed.1);
    assert!(!observed.2);
}

#[tokio::test]
async fn realtime_remote_sequences_are_scoped_per_handle_and_interleavable() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let cached = state(1, 11);
    let first_handle = PresenceHandle::new(0x31).unwrap();
    let second_handle = PresenceHandle::new(0x32).unwrap();
    let first_user = CanonicalUsername::new("misty").unwrap();
    let second_user = CanonicalUsername::new("brock").unwrap();
    let frames = vec![
        ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(first_handle, 1, state(1, 21), first_user).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(second_handle, 1, state(1, 31), second_user).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_update(
            RemotePlayerUpdateV1::new(first_handle, 2, state(2, 22)).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_update(
            RemotePlayerUpdateV1::new(second_handle, 2, state(2, 32)).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_despawn(
            RemotePlayerDespawnV1::new(first_handle, 3, DespawnReason::Disconnected).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_despawn(
            RemotePlayerDespawnV1::new(second_handle, 3, DespawnReason::Disconnected).unwrap(),
        ),
    ];
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed).await;
        let _ = socket.next().await.unwrap().unwrap();
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap()),
        )
        .await;
        for frame in frames {
            send_frame(&mut socket, frame).await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let (mut owner, driver) = realtime_channel(cached).unwrap();
    let run = tokio::spawn(run_realtime(grant(port), driver));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Ready(_))
    ));
    let mut lifecycle = Vec::new();
    for _ in 0..6 {
        let event = timeout(Duration::from_secs(1), owner.recv_event())
            .await
            .unwrap()
            .unwrap();
        lifecycle.push(event);
    }
    assert!(matches!(
        lifecycle[0],
        RealtimeOwnerEvent::Spawn(ref spawn) if spawn.handle() == first_handle
    ));
    assert!(matches!(
        lifecycle[1],
        RealtimeOwnerEvent::Spawn(ref spawn) if spawn.handle() == second_handle
    ));
    assert!(matches!(
        lifecycle[2],
        RealtimeOwnerEvent::Update(ref update) if update.handle() == first_handle
    ));
    assert!(matches!(
        lifecycle[3],
        RealtimeOwnerEvent::Update(ref update) if update.handle() == second_handle
    ));
    assert!(matches!(
        lifecycle[4],
        RealtimeOwnerEvent::Despawn(ref despawn) if despawn.handle() == first_handle
    ));
    assert!(matches!(
        lifecycle[5],
        RealtimeOwnerEvent::Despawn(ref despawn) if despawn.handle() == second_handle
    ));
    owner.stop();
    assert_eq!(
        timeout(Duration::from_secs(1), run).await.unwrap().unwrap(),
        RealtimeOutcome::OwnerStopped
    );
    server.await.unwrap();
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "the loopback fixture keeps the complete lifecycle sequence together"
)]
async fn realtime_hidden_tombstones_do_not_consume_visible_capacity() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let cached = state(1, 11);
    let hidden_handle = PresenceHandle::new(0x40).unwrap();
    let hidden_only_handle = PresenceHandle::new(0x46).unwrap();
    let visible_handles = [
        PresenceHandle::new(0x41).unwrap(),
        PresenceHandle::new(0x42).unwrap(),
        PresenceHandle::new(0x43).unwrap(),
        PresenceHandle::new(0x44).unwrap(),
    ];
    let hidden_user = CanonicalUsername::new("hidden").unwrap();
    let mut frames = vec![
        ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(hidden_only_handle, 1, hidden_state(1, 41), hidden_user)
                .unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_update(
            RemotePlayerUpdateV1::new(hidden_only_handle, 2, hidden_state(2, 42)).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(
                hidden_handle,
                1,
                state(1, 42),
                CanonicalUsername::new("resurfacing").unwrap(),
            )
            .unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_despawn(
            RemotePlayerDespawnV1::new(hidden_handle, 2, DespawnReason::Hidden).unwrap(),
        ),
        ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(
                hidden_handle,
                3,
                state(1, 43),
                CanonicalUsername::new("visible-again").unwrap(),
            )
            .unwrap(),
        ),
    ];
    for (index, handle) in visible_handles.iter().take(3).enumerate() {
        frames.push(ServerRealtimeFrameV1::remote_player_spawn(
            RemotePlayerSpawnV1::new(
                *handle,
                1,
                state(1, 50 + u32::try_from(index).unwrap()),
                CanonicalUsername::new(format!("user{index}")).unwrap(),
            )
            .unwrap(),
        ));
    }
    let over_capacity_handle = PresenceHandle::new(0x45).unwrap();
    frames.push(ServerRealtimeFrameV1::remote_player_spawn(
        RemotePlayerSpawnV1::new(
            over_capacity_handle,
            1,
            state(1, 60),
            CanonicalUsername::new("overflow").unwrap(),
        )
        .unwrap(),
    ));
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed).await;
        let _ = socket.next().await.unwrap().unwrap();
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap()),
        )
        .await;
        for frame in frames {
            send_frame(&mut socket, frame).await;
        }
    });

    let (mut owner, driver) = realtime_channel(cached).unwrap();
    let run = tokio::spawn(run_realtime(grant(port), driver));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Ready(_))
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), owner.recv_event())
            .await
            .unwrap(),
        Some(RealtimeOwnerEvent::Spawn(_))
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), owner.recv_event())
            .await
            .unwrap(),
        Some(RealtimeOwnerEvent::Despawn(_))
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), owner.recv_event())
            .await
            .unwrap(),
        Some(RealtimeOwnerEvent::Spawn(_))
    ));
    for _ in 0..3 {
        assert!(matches!(
            timeout(Duration::from_secs(1), owner.recv_event())
                .await
                .unwrap(),
            Some(RealtimeOwnerEvent::Spawn(_))
        ));
    }
    assert_eq!(
        timeout(Duration::from_secs(1), run).await.unwrap().unwrap(),
        RealtimeOutcome::CapacityExceeded
    );
    server.await.unwrap();
    owner.stop();
}

#[tokio::test]
async fn realtime_authenticated_session_survives_consumed_ticket_expiry() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed).await;
        let _ = socket.next().await.unwrap().unwrap();
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap()),
        )
        .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let (mut owner, driver) = realtime_channel(state(1, 11)).unwrap();
    let run = tokio::spawn(run_realtime(grant_with_ttl(port, 300), driver));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Ready(_))
    ));
    tokio::time::sleep(Duration::from_millis(700)).await;
    owner.stop();
    assert_eq!(
        timeout(Duration::from_secs(1), run).await.unwrap().unwrap(),
        RealtimeOutcome::OwnerStopped
    );
    server.await.unwrap();
}

#[tokio::test]
async fn realtime_interactions_are_not_accepted_before_ready_and_latest_state_is_bounded() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let cached = state(1, 11);
    let updated = state(2, 99);
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed).await;
        let _ = socket.next().await.unwrap().unwrap();
        send_frame(
            &mut socket,
            ServerRealtimeFrameV1::presence_ready(PresenceHandle::new(1).unwrap()),
        )
        .await;
        let mut seen = Vec::new();
        for _ in 0..2 {
            let message = timeout(Duration::from_secs(1), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if let Message::Text(text) = message {
                seen.push(serde_json::from_str::<ClientRealtimeFrameV1>(text.as_ref()).unwrap());
            }
        }
        seen_tx.send(seen).unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let (mut owner, driver) = realtime_channel(cached).unwrap();
    let interaction =
        PresenceInteractionV1::new(PresenceHandle::new(9).unwrap(), 1, 1, 4, 5).unwrap();
    assert_eq!(
        owner.interact(interaction.clone()),
        Err(coop_sidecar::realtime::RealtimeInputError::NotReady)
    );
    let run = tokio::spawn(run_realtime(grant(port), driver));
    assert!(matches!(
        owner.recv_event().await,
        Some(RealtimeOwnerEvent::Ready(_))
    ));
    assert!(owner.interact(interaction).is_ok());
    owner.update_state(updated.clone()).unwrap();
    owner.update_state(updated.clone()).unwrap();
    let seen = timeout(Duration::from_secs(2), seen_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .any(|frame| matches!(frame, ClientRealtimeFrameV1::InteractRemotePlayer(_)))
    );
    assert!(seen.iter().any(
        |frame| matches!(frame, ClientRealtimeFrameV1::PlayerState(state) if state == &updated)
    ));
    owner.stop();
    assert_eq!(
        timeout(Duration::from_secs(1), run).await.unwrap().unwrap(),
        RealtimeOutcome::OwnerStopped
    );
    server.await.unwrap();
}

#[tokio::test]
async fn realtime_malformed_server_data_is_redacted_protocol_failure() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let observed = Arc::new(Mutex::new(None));
    let server = tokio::spawn(async move {
        let mut socket = accept_server(listener, observed).await;
        let _ = socket.next().await.unwrap().unwrap();
        socket
            .send(Message::Text(
                "{peer-controlled-close-reason}".to_owned().into(),
            ))
            .await
            .unwrap();
    });
    let (mut owner, driver) = realtime_channel(state(1, 11)).unwrap();
    let run = tokio::spawn(run_realtime(grant(port), driver));
    let outcome = timeout(Duration::from_secs(1), run).await.unwrap().unwrap();
    assert_eq!(outcome, RealtimeOutcome::ProtocolViolation);
    assert!(format!("{outcome}").contains("protocol violation"));
    assert!(owner.recv_event().await.is_none());
    server.await.unwrap();
    owner.stop();
}

#[test]
fn realtime_interaction_queue_capacity_is_fixed() {
    assert_eq!(MAX_INTERACTION_QUEUE, 16);
}

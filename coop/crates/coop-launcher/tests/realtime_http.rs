use std::{
    env,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    AccessToken, BridgeAbiVersion, CharacterId, ClientInstanceId, GameBuildId, LoginRequest,
    LoginResponse, LogoutRequest, LogoutResponse, MgbaVersion, MintRealtimeTicketRequest,
    MintRealtimeTicketResponse, Password, ProtocolVersion, RealtimeTicket, RefreshFamilyId,
    RefreshRequest, RefreshResponse, RefreshToken, RuntimeBuildIdentity, RuntimeLeaseFence,
    SessionEpoch, SessionId, Sha256Digest, StableRuntimeSession, UnixTimestampMillis, UserId,
};
use coop_launcher::{
    AuthApi, AuthError, AuthSession, REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES, RealtimeHttpError,
    RefreshTokenStore, ReqwestCloudApi,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

#[derive(Default)]
struct TestKeychain;

impl RefreshTokenStore for TestKeychain {
    fn load(
        &self,
        _service: &str,
        _username: &str,
    ) -> Result<Option<RefreshToken>, coop_launcher::KeychainError> {
        Ok(None)
    }

    fn store(
        &self,
        _service: &str,
        _username: &str,
        _token: &RefreshToken,
    ) -> Result<(), coop_launcher::KeychainError> {
        Ok(())
    }

    fn delete(&self, _service: &str, _username: &str) -> Result<(), coop_launcher::KeychainError> {
        Ok(())
    }
}

#[derive(Clone)]
struct TestAuthApi {
    response: LoginResponse,
}

impl AuthApi for TestAuthApi {
    fn login(&self, _request: LoginRequest) -> coop_launcher::auth::AuthFuture<'_, LoginResponse> {
        let response = self.response.clone();
        Box::pin(async move { Ok(response) })
    }

    fn refresh(
        &self,
        _request: RefreshRequest,
    ) -> coop_launcher::auth::AuthFuture<'_, RefreshResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }

    fn logout(
        &self,
        _request: LogoutRequest,
    ) -> coop_launcher::auth::AuthFuture<'_, LogoutResponse> {
        Box::pin(async { Ok(LogoutResponse::default()) })
    }
}

fn runtime() -> RuntimeLeaseFence {
    let session = StableRuntimeSession::new(
        SessionId::new(Uuid::from_u128(1)).expect("session id"),
        CharacterId::new(Uuid::from_u128(2)).expect("character id"),
        SessionEpoch::new(3).expect("session epoch"),
        ClientInstanceId::new(Uuid::from_u128(4)).expect("client id"),
    );
    let build = RuntimeBuildIdentity::new(
        GameBuildId::new("emerald-coop-test").expect("build id"),
        Sha256Digest::from_bytes([7; 32]),
        MgbaVersion::new("0.11.3").expect("mGBA version"),
        BridgeAbiVersion::new(1).expect("bridge ABI"),
        ProtocolVersion::new(1).expect("protocol version"),
    );
    RuntimeLeaseFence::new(session, build)
}

fn login_response() -> LoginResponse {
    LoginResponse::new(
        UserId::new(Uuid::from_u128(5)).expect("user id"),
        CharacterId::new(Uuid::from_u128(2)).expect("character id"),
        AccessToken::new("access-secret").expect("access token"),
        RefreshToken::new("refresh-secret").expect("refresh token"),
        RefreshFamilyId::new(Uuid::from_u128(6)).expect("refresh family"),
        UnixTimestampMillis::new(9_000_000_000_000),
        UnixTimestampMillis::new(9_000_000_100_000),
    )
    .expect("login response")
}

async fn auth_session() -> AuthSession {
    let api = TestAuthApi {
        response: login_response(),
    };
    AuthSession::login(
        &api,
        &TestKeychain,
        "ash",
        Password::new("password").expect("password"),
    )
    .await
    .expect("auth session")
}

async fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut head = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).await.expect("request header");
        head.push(byte[0]);
        assert!(head.len() <= 16 * 1024, "request header bound");
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8(head.clone()).expect("request headers are ASCII");
    let length = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
        })
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    assert!(length <= 8 * 1024, "request body bound");
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).await.expect("request body");
    (text, body)
}

async fn response(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .expect("response header");
    stream.write_all(body).await.expect("response body");
}

async fn chunked_response(stream: &mut TcpStream, status: &str, chunks: &[Vec<u8>]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n"
    );
    stream
        .write_all(header.as_bytes())
        .await
        .expect("chunked response header");
    for chunk in chunks {
        stream
            .write_all(format!("{:X}\r\n", chunk.len()).as_bytes())
            .await
            .expect("chunk size");
        stream.write_all(chunk).await.expect("chunk body");
        stream.write_all(b"\r\n").await.expect("chunk terminator");
    }
    stream.write_all(b"0\r\n\r\n").await.expect("chunk end");
}

async fn redirect_response(stream: &mut TcpStream, target_port: u16) {
    let header = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/v1/realtime/tickets\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(header.as_bytes())
        .await
        .expect("redirect response");
}

fn header_values<'a>(headers: &'a str, name: &str) -> Vec<&'a str> {
    headers
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .filter(|(field, _)| field.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
        .collect()
}

fn expected_response(request: &MintRealtimeTicketRequest, now: u64) -> Vec<u8> {
    serde_json::to_vec(
        &MintRealtimeTicketResponse::v1(
            request.runtime().clone(),
            RealtimeTicket::from_bytes([11; 32]).expect("ticket"),
            UnixTimestampMillis::new(now),
        )
        .expect("mint response"),
    )
    .expect("response JSON")
}

fn padded_response(request: &MintRealtimeTicketRequest, now: u64, length: usize) -> Vec<u8> {
    let mut body = expected_response(request, now);
    assert!(
        body.len() <= length,
        "test response fixture must fit its bound"
    );
    body.resize(length, b' ');
    body
}

fn unix_now() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_millis(),
    )
    .expect("timestamp fits")
}

#[test]
fn endpoint_derivation_is_canonical_and_preserves_authority() {
    let secure = ReqwestCloudApi::new("https://cloud.example:8443").expect("secure base");
    assert_eq!(
        secure
            .realtime_endpoint()
            .expect("secure endpoint")
            .as_str(),
        "wss://cloud.example:8443/v1/realtime"
    );

    let loopback = ReqwestCloudApi::new("http://127.0.0.1:43127").expect("loopback base");
    assert_eq!(
        loopback
            .realtime_endpoint()
            .expect("loopback endpoint")
            .as_str(),
        "ws://127.0.0.1:43127/v1/realtime"
    );
    let default_loopback = ReqwestCloudApi::new("http://127.0.0.1").expect("default loopback");
    let default_endpoint = default_loopback.realtime_endpoint();
    assert!(
        default_endpoint.is_ok(),
        "default endpoint: {default_endpoint:?}"
    );
    assert_eq!(
        default_endpoint
            .expect("default loopback endpoint")
            .as_str(),
        "ws://127.0.0.1/v1/realtime"
    );
    assert!(ReqwestCloudApi::new("https://cloud.example?endpoint=evil").is_err());
}

#[tokio::test]
async fn mint_uses_one_exact_authenticated_json_post_and_consumes_grant() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("listener address").port();
    let request = MintRealtimeTicketRequest::v1(runtime());
    let expected_ticket = RealtimeTicket::from_bytes([11; 32]).expect("ticket");
    let expected_ticket_text = expected_ticket.expose_secret().to_owned();
    let response_request = request.clone();
    let expected_request = serde_json::to_value(&request).expect("request JSON");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("mint connection");
        let (headers, body_bytes) = read_request(&mut stream).await;
        assert!(headers.starts_with("POST /v1/realtime/tickets HTTP/1.1\r\n"));
        assert_eq!(
            header_values(&headers, "Authorization"),
            ["Bearer access-secret"]
        );
        assert_eq!(header_values(&headers, "Authorization").len(), 1);
        assert_eq!(
            header_values(&headers, "Content-Type"),
            ["application/json"]
        );
        assert!(header_values(&headers, "Sec-WebSocket-Protocol").is_empty());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body_bytes).expect("body JSON"),
            expected_request
        );
        assert!(
            !body_bytes
                .windows(b"access-secret".len())
                .any(|window| window == b"access-secret")
        );
        // The cloud samples issuance before its response is delayed. A
        // caller-side pre-await clock would see more than the fixed TTL.
        let issued_at = unix_now();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let body = expected_response(
            &response_request,
            issued_at + coop_cloud::REALTIME_TICKET_TTL_MS,
        );
        response(&mut stream, "200 OK", &body).await;
    });

    let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
    let auth = auth_session().await;
    let grant = api.mint_realtime(&auth, request).await.expect("grant");
    assert_eq!(
        grant.endpoint().as_str(),
        format!("ws://127.0.0.1:{port}/v1/realtime")
    );
    assert!(grant.expires_at().value() > unix_now());
    let rendered = format!("{grant:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("access-secret"));
    assert!(!rendered.contains(&expected_ticket_text));
    server.await.expect("server task");
}

#[tokio::test]
async fn ambient_proxy_does_not_intercept_loopback_mint() {
    if env::var_os("REALTIME_PROXY_CHILD").is_some() {
        let port = env::var("REALTIME_ORIGIN_PORT")
            .expect("origin port")
            .parse::<u16>()
            .expect("origin port number");
        let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
        let auth = auth_session().await;
        api.mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
            .await
            .expect("loopback mint should bypass configured proxy");
        return;
    }

    let origin = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("origin listener");
    let origin_port = origin.local_addr().expect("origin address").port();
    let proxy = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("proxy listener");
    let proxy_port = proxy.local_addr().expect("proxy address").port();
    let origin_server = tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.expect("origin connection");
        let (headers, _) = read_request(&mut stream).await;
        assert_eq!(
            header_values(&headers, "Authorization"),
            ["Bearer access-secret"]
        );
        let request = MintRealtimeTicketRequest::v1(runtime());
        let body = expected_response(&request, unix_now() + 5_000);
        response(&mut stream, "200 OK", &body).await;
    });
    let proxy_observer = tokio::spawn(async move {
        match tokio::time::timeout(Duration::from_secs(2), proxy.accept()).await {
            Ok(Ok((mut stream, _))) => {
                let _ = stream.shutdown().await;
                true
            }
            Ok(Err(_)) | Err(_) => false,
        }
    });

    let executable = env::current_exe().expect("test executable");
    let proxy_url = format!("http://127.0.0.1:{proxy_port}");
    let output = tokio::process::Command::new(executable)
        .arg("--exact")
        .arg("ambient_proxy_does_not_intercept_loopback_mint")
        .arg("--nocapture")
        .env("REALTIME_PROXY_CHILD", "1")
        .env("REALTIME_ORIGIN_PORT", origin_port.to_string())
        .env("HTTP_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("https_proxy", &proxy_url)
        .env("ALL_PROXY", &proxy_url)
        .env("all_proxy", &proxy_url)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .expect("proxy child");
    assert!(output.status.success(), "proxy child failed");
    origin_server.await.expect("origin task");
    assert!(!proxy_observer.await.expect("proxy observer"));
}

#[tokio::test]
async fn unauthorized_is_stable_and_does_not_retry() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("listener address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("connection");
        let (headers, _) = read_request(&mut stream).await;
        assert_eq!(
            header_values(&headers, "Authorization"),
            ["Bearer access-secret"]
        );
        response(&mut stream, "401 Unauthorized", b"peer-secret-body").await;
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "unauthorized mint must not retry"
        );
    });
    let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
    let auth = auth_session().await;
    let error = api
        .mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
        .await
        .expect_err("request should fail");
    assert_eq!(error, RealtimeHttpError::Unauthorized);
    assert!(!error.to_string().contains("peer-secret"));
    server.await.expect("server task");
}

#[tokio::test]
async fn valid_cross_authority_redirect_is_rejected_without_target_contact() {
    let origin = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("origin listener");
    let origin_port = origin.local_addr().expect("origin address").port();
    let target = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("target listener");
    let target_port = target.local_addr().expect("target address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.expect("origin connection");
        let (headers, _) = read_request(&mut stream).await;
        assert_eq!(
            header_values(&headers, "Authorization"),
            ["Bearer access-secret"]
        );
        redirect_response(&mut stream, target_port).await;
    });
    let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{origin_port}")).expect("API");
    let auth = auth_session().await;
    let error = api
        .mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
        .await
        .expect_err("redirect should fail");
    assert_eq!(error, RealtimeHttpError::ResponseRejected);
    assert!(!error.to_string().contains("peer-secret"));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), target.accept())
            .await
            .is_err(),
        "redirect target must not receive a connection"
    );
    server.await.expect("origin task");
}

#[tokio::test]
async fn malformed_oversized_and_mismatched_responses_fail_closed() {
    let cases: &[(&str, Vec<u8>, RealtimeHttpError)] = &[
        (
            "malformed",
            b"not-json".to_vec(),
            RealtimeHttpError::InvalidResponse,
        ),
        (
            "oversized",
            vec![b'x'; REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES + 1],
            RealtimeHttpError::InvalidResponse,
        ),
    ];
    for (name, response_body, expected) in cases {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
        let port = listener.local_addr().expect("listener address").port();
        let response_body = response_body.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection");
            let _ = read_request(&mut stream).await;
            response(&mut stream, "200 OK", &response_body).await;
        });
        let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
        let auth = auth_session().await;
        let error = api
            .mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
            .await
            .expect_err(name);
        assert_eq!(error, *expected);
        server.await.expect("server task");
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("listener address").port();
    let response_body = serde_json::to_vec(
        &MintRealtimeTicketResponse::v1(
            runtime(),
            RealtimeTicket::from_bytes([12; 32]).expect("ticket"),
            UnixTimestampMillis::new(9_000_000_002_000),
        )
        .expect("mismatched response"),
    )
    .expect("response JSON");
    let distinct_ticket = RealtimeTicket::from_bytes([12; 32]).expect("ticket");
    let distinct_ticket_text = distinct_ticket.expose_secret().to_owned();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("connection");
        let _ = read_request(&mut stream).await;
        response(&mut stream, "200 OK", &response_body).await;
    });
    let api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
    let auth = auth_session().await;
    let request = MintRealtimeTicketRequest::v1(RuntimeLeaseFence::new(
        StableRuntimeSession::new(
            SessionId::new(Uuid::from_u128(10)).expect("session id"),
            CharacterId::new(Uuid::from_u128(2)).expect("character id"),
            SessionEpoch::new(3).expect("session epoch"),
            ClientInstanceId::new(Uuid::from_u128(4)).expect("client id"),
        ),
        runtime().build.clone(),
    ));
    let error = api
        .mint_realtime(&auth, request)
        .await
        .expect_err("mismatched response should fail");
    assert_eq!(error, RealtimeHttpError::CorrelationFailed);
    assert!(!format!("{error:?}").contains(&distinct_ticket_text));
    assert!(!error.to_string().contains(&distinct_ticket_text));
    assert!(!format!("{error:?}").contains("access-secret"));
    server.await.expect("server task");
}

#[tokio::test]
async fn chunked_response_bound_accepts_exact_limit_and_rejects_one_byte_over() {
    let request = MintRealtimeTicketRequest::v1(runtime());
    let exact_body = padded_response(
        &request,
        unix_now() + 5_000,
        REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES,
    );
    let exact_chunks = vec![exact_body[..4_096].to_vec(), exact_body[4_096..].to_vec()];
    let exact_listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let exact_port = exact_listener
        .local_addr()
        .expect("listener address")
        .port();
    let exact_server = tokio::spawn(async move {
        let (mut stream, _) = exact_listener.accept().await.expect("connection");
        let _ = read_request(&mut stream).await;
        chunked_response(&mut stream, "200 OK", &exact_chunks).await;
    });
    let exact_api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{exact_port}")).expect("API");
    let auth = auth_session().await;
    exact_api
        .mint_realtime(&auth, request.clone())
        .await
        .expect("exactly bounded chunked response should succeed");
    exact_server.await.expect("exact server task");

    let over_body = padded_response(
        &request,
        unix_now() + 5_000,
        REALTIME_TICKET_RESPONSE_BODY_MAX_BYTES + 1,
    );
    let over_chunks = vec![over_body[..4_096].to_vec(), over_body[4_096..].to_vec()];
    let over_listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let over_port = over_listener.local_addr().expect("listener address").port();
    let over_server = tokio::spawn(async move {
        let (mut stream, _) = over_listener.accept().await.expect("connection");
        let _ = read_request(&mut stream).await;
        chunked_response(&mut stream, "200 OK", &over_chunks).await;
    });
    let over_api = ReqwestCloudApi::new(&format!("http://127.0.0.1:{over_port}")).expect("API");
    let auth = auth_session().await;
    let error = over_api
        .mint_realtime(&auth, request)
        .await
        .expect_err("one byte over the cumulative bound should fail");
    assert_eq!(error, RealtimeHttpError::InvalidResponse);
    over_server.await.expect("over server task");
}

#[tokio::test]
async fn grant_expiry_ttl_and_closed_session_fail_without_secret_text() {
    let now = unix_now();
    for (expires_at, expected) in [
        (now, RealtimeHttpError::ExpiredGrant),
        (
            now + coop_cloud::REALTIME_TICKET_TTL_MS + 1_000,
            RealtimeHttpError::InvalidGrant,
        ),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
        let port = listener.local_addr().expect("listener address").port();
        let response_body = serde_json::to_vec(
            &MintRealtimeTicketResponse::v1(
                runtime(),
                RealtimeTicket::from_bytes([13; 32]).expect("ticket"),
                UnixTimestampMillis::new(expires_at),
            )
            .expect("response"),
        )
        .expect("response JSON");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection");
            let _ = read_request(&mut stream).await;
            response(&mut stream, "200 OK", &response_body).await;
        });
        let cloud = ReqwestCloudApi::new(&format!("http://127.0.0.1:{port}")).expect("API");
        let auth = auth_session().await;
        let error = cloud
            .mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
            .await
            .expect_err("invalid grant");
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("ticket"));
        server.await.expect("server task");
    }

    let cloud = ReqwestCloudApi::new("http://127.0.0.1:43127").expect("API");
    let auth_api = TestAuthApi {
        response: login_response(),
    };
    let mut auth = AuthSession::login(
        &auth_api,
        &TestKeychain,
        "ash",
        Password::new("password").expect("password"),
    )
    .await
    .expect("auth session");
    auth.logout(&auth_api, &TestKeychain)
        .await
        .expect("local logout");
    let error = cloud
        .mint_realtime(&auth, MintRealtimeTicketRequest::v1(runtime()))
        .await
        .expect_err("closed session");
    assert_eq!(error, RealtimeHttpError::SessionClosed);
    assert!(!error.to_string().contains("access-secret"));
}

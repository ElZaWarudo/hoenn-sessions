use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use coop_cloud::{
    AcquireLeaseRequest, ClientInstanceId, CreatePairingCodeRequest, InvitationCode, LeaseContract,
    LoginRequest, LoginResponse, PairingCode, Password, RedeemPairingCodeRequest, RegisterRequest,
    RegisterResponse, ReleaseLeaseRequest, SigningPrivateKey,
};
use coop_server::{Phase2App, Phase2Config};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn call(
    router: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn account(
    app: &Phase2App,
    router: &Router,
    name: &str,
    invitation: &str,
) -> (RegisterResponse, LoginResponse, LeaseContract) {
    app.add_invitation(invitation)
        .expect("bootstrap invitation");
    let register = RegisterRequest::new(
        name,
        Password::new("correct horse battery staple").expect("password"),
        InvitationCode::new(invitation).expect("invitation"),
    )
    .expect("register");
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/auth/register",
        None,
        serde_json::to_value(register).expect("register json"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let registered: RegisterResponse = serde_json::from_value(body).expect("registered");
    let login = LoginRequest::new(
        name,
        Password::new("correct horse battery staple").expect("password"),
    )
    .expect("login");
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/auth/login",
        None,
        serde_json::to_value(login).expect("login json"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let logged_in: LoginResponse = serde_json::from_value(body).expect("login response");
    let acquire = AcquireLeaseRequest::new(
        registered.character_id,
        ClientInstanceId::new(Uuid::new_v4()).expect("client"),
        coop_cloud::IdempotencyKey::new(Uuid::new_v4()).expect("idempotency"),
    );
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/sessions/acquire",
        Some(logged_in.access_token.expose_secret()),
        serde_json::to_value(acquire).expect("acquire json"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    (
        registered,
        logged_in,
        serde_json::from_value(body).expect("lease response"),
    )
}

fn app() -> Phase2App {
    Phase2App::new(
        Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "pairing-test-key",
        )
        .expect("config"),
    )
    .expect("app")
}

async fn issue(
    router: &Router,
    login: &LoginResponse,
    lease: &LeaseContract,
) -> (StatusCode, Value) {
    call(
        router,
        Method::POST,
        "/v1/groups/pairing-codes",
        Some(login.access_token.expose_secret()),
        serde_json::to_value(CreatePairingCodeRequest::new(lease.fence())).expect("create json"),
    )
    .await
}

async fn redeem(
    router: &Router,
    login: &LoginResponse,
    lease: &LeaseContract,
    code: &str,
) -> (StatusCode, Value) {
    call(
        router,
        Method::POST,
        "/v1/groups/pairing-codes/redeem",
        Some(login.access_token.expose_secret()),
        serde_json::to_value(RedeemPairingCodeRequest::new(
            lease.fence(),
            PairingCode::new(code).expect("pairing code"),
        ))
        .expect("redeem json"),
    )
    .await
}

#[tokio::test]
async fn pairing_code_is_short_lived_single_use_and_forms_group_without_shared_zone() {
    let app = app();
    let router = app.router();
    let (one, login_one, lease_one) = account(&app, &router, "pairingalice", "PAIR-A").await;

    let create = CreatePairingCodeRequest::new(lease_one.fence());
    let (status, body) = call(
        &router,
        Method::POST,
        "/v1/groups/pairing-codes",
        Some(login_one.access_token.expose_secret()),
        serde_json::to_value(create).expect("create json"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let code = body["code"].as_str().expect("code");
    assert_eq!(code.len(), 7);
    assert_eq!(&code[3..4], "-");
    assert_eq!(body["join_link"], format!("hoenn-sessions://join/{code}"));

    let (two, login_two, lease_two) = account(&app, &router, "pairingbob", "PAIR-B").await;

    let redeem = json!({
        "api_version": 1,
        "code": code,
        "session_id": lease_two.session_id,
        "character_id": lease_two.character_id,
        "current_revision": lease_two.current_revision,
        "session_epoch": lease_two.session_epoch,
        "client_instance_id": lease_two.client_instance_id,
    });
    let (status, group) = call(
        &router,
        Method::POST,
        "/v1/groups/pairing-codes/redeem",
        Some(login_two.access_token.expose_secret()),
        redeem.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(group["group"]["members"].as_array().unwrap().len(), 2);
    assert_eq!(
        group["group"]["member_world_zones"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(group["group"]["group_id"].is_string(), true);

    let (three, login_three, lease_three) = account(&app, &router, "pairingcarol", "PAIR-C").await;

    let replay = json!({
        "api_version": 1,
        "code": code,
        "session_id": lease_three.session_id,
        "character_id": lease_three.character_id,
        "current_revision": lease_three.current_revision,
        "session_epoch": lease_three.session_epoch,
        "client_instance_id": lease_three.client_instance_id,
    });
    let (status, _) = call(
        &router,
        Method::POST,
        "/v1/groups/pairing-codes/redeem",
        Some(login_three.access_token.expose_secret()),
        replay,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(one.character_id, two.character_id);
    assert_ne!(one.character_id, three.character_id);
}

#[tokio::test]
async fn simultaneous_redemption_consumes_code_once_and_only_one_group_is_visible() {
    let app = app();
    let router = app.router();
    let (_, owner, owner_lease) = account(&app, &router, "raceowner", "PAIR-RACE-O").await;
    let (_, first, first_lease) = account(&app, &router, "racefirst", "PAIR-RACE-F").await;
    let (_, second, second_lease) = account(&app, &router, "racesecond", "PAIR-RACE-S").await;
    let (status, issued) = issue(&router, &owner, &owner_lease).await;
    assert_eq!(status, StatusCode::CREATED);
    let code = issued["code"].as_str().expect("code");

    let (first_result, second_result) = tokio::join!(
        redeem(&router, &first, &first_lease, code),
        redeem(&router, &second, &second_lease, code),
    );
    let statuses = [first_result.0, second_result.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|&&status| status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|&&status| status == StatusCode::NOT_FOUND)
            .count(),
        1
    );

    let owner_token = owner.access_token.expose_secret();
    let (status, owner_view) = call(
        &router,
        Method::POST,
        "/v1/online/snapshot",
        Some(owner_token),
        json!({"api_version":1,"fence":owner_lease.fence(),"incoming_after":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let winner = if first_result.0 == StatusCode::OK {
        "racefirst"
    } else {
        "racesecond"
    };
    assert_eq!(owner_view["group"]["username"], winner);
    let loser = if first_result.0 == StatusCode::OK {
        (&second, &second_lease)
    } else {
        (&first, &first_lease)
    };
    let (status, loser_view) = call(
        &router,
        Method::POST,
        "/v1/online/snapshot",
        Some(loser.0.access_token.expose_secret()),
        json!({"api_version":1,"fence":loser.1.fence(),"incoming_after":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(loser_view["group"].is_null());
    let (status, partner) = call(
        &router,
        Method::GET,
        "/v1/group/partner",
        Some(owner_token),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partner["partner"]["username"], winner);
    assert_eq!(partner["partner"]["group_active"], true);
}

#[tokio::test]
async fn stale_or_released_lease_cannot_issue_or_redeem_pairing_code() {
    let app = app();
    let router = app.router();
    let (_, owner, owner_lease) = account(&app, &router, "staleowner", "PAIR-STALE-O").await;
    let (_, joiner, joiner_lease) = account(&app, &router, "stalejoiner", "PAIR-STALE-J").await;
    let (status, issued) = issue(&router, &owner, &owner_lease).await;
    assert_eq!(status, StatusCode::CREATED);
    let code = issued["code"].as_str().expect("code");

    let release = ReleaseLeaseRequest::new(
        joiner_lease.fence(),
        coop_cloud::IdempotencyKey::new(Uuid::new_v4()).expect("key"),
    );
    let (status, _) = call(
        &router,
        Method::POST,
        "/v1/sessions/release",
        Some(joiner.access_token.expose_secret()),
        serde_json::to_value(release).expect("release json"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        redeem(&router, &joiner, &joiner_lease, code).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        issue(&router, &joiner, &joiner_lease).await.0,
        StatusCode::UNAUTHORIZED
    );

    let reacquire = AcquireLeaseRequest::new(
        joiner_lease.character_id,
        ClientInstanceId::new(Uuid::new_v4()).expect("client"),
        coop_cloud::IdempotencyKey::new(Uuid::new_v4()).expect("key"),
    );
    let (status, body) = call(
        &router,
        Method::POST,
        "/v1/sessions/acquire",
        Some(joiner.access_token.expose_secret()),
        serde_json::to_value(reacquire).expect("acquire json"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let new_lease: LeaseContract = serde_json::from_value(body).expect("new lease");
    assert_ne!(new_lease.fence(), joiner_lease.fence());
    assert_eq!(
        redeem(&router, &joiner, &joiner_lease, code).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        redeem(&router, &joiner, &new_lease, code).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn per_account_issuance_and_bad_code_attempts_are_bounded() {
    let app = app();
    let router = app.router();
    let (_, owner, owner_lease) = account(&app, &router, "rateowner", "PAIR-RATE-O").await;
    let (_, joiner, joiner_lease) = account(&app, &router, "ratejoiner", "PAIR-RATE-J").await;
    let mut valid_code = String::new();
    for _ in 0..3 {
        let (status, body) = issue(&router, &owner, &owner_lease).await;
        assert_eq!(status, StatusCode::CREATED);
        valid_code = body["code"].as_str().expect("code").to_owned();
    }
    let (status, body) = issue(&router, &owner, &owner_lease).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "service_busy");
    for _ in 0..12 {
        assert_eq!(
            redeem(&router, &joiner, &joiner_lease, "AAA-AAA").await.0,
            StatusCode::NOT_FOUND
        );
    }
    let (status, body) = redeem(&router, &joiner, &joiner_lease, &valid_code).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "service_busy");
    let (status, owner_view) = call(
        &router,
        Method::POST,
        "/v1/online/snapshot",
        Some(owner.access_token.expose_secret()),
        json!({"api_version":1,"fence":owner_lease.fence(),"incoming_after":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(owner_view["group"].is_null());
}

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use coop_cloud::{
    AcquireLeaseRequest, ClientInstanceId, CreateGroupInvitationRequest, CreatePairingCodeRequest,
    IdempotencyKey, InvitationCode, LeaseContract, LoginRequest, LoginResponse, PairingCode,
    Password, RedeemPairingCodeRequest, RegisterRequest, RegisterResponse, SigningPrivateKey,
};
use coop_server::{Phase2App, Phase2Config};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    router: &Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    body: Value,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if uri == "/v1/online/snapshot" {
        builder = builder.header("x-coop-online-remote-join", "1");
    }
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response")
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes(),
    )
    .expect("json")
}

fn app() -> Phase2App {
    let config = Phase2Config::local(
        vec![0x55; 32],
        SigningPrivateKey::from_bytes([7; 32]),
        "group-test-key",
    )
    .expect("config");
    Phase2App::new(config).expect("app")
}

async fn account(
    router: &Router,
    app: &Phase2App,
    username: &str,
    invitation: &str,
) -> (
    RegisterResponse,
    LoginResponse,
    LeaseContract,
    ClientInstanceId,
) {
    app.add_invitation(invitation)
        .expect("bootstrap invitation");
    let register = RegisterRequest::new(
        username,
        Password::new("correct horse battery staple").expect("password"),
        InvitationCode::new(invitation).expect("invitation"),
    )
    .expect("register request");
    let response = request(
        router,
        Method::POST,
        "/v1/auth/register",
        None,
        serde_json::to_value(register).expect("register json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let registered: RegisterResponse =
        serde_json::from_value(json_body(response).await).expect("registered");
    let login = LoginRequest::new(
        username,
        Password::new("correct horse battery staple").expect("password"),
    )
    .expect("login request");
    let response = request(
        router,
        Method::POST,
        "/v1/auth/login",
        None,
        serde_json::to_value(login).expect("login json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let logged_in: LoginResponse =
        serde_json::from_value(json_body(response).await).expect("login");
    let client = ClientInstanceId::new(Uuid::new_v4()).expect("client");
    let acquire = AcquireLeaseRequest::new(
        registered.character_id,
        client,
        IdempotencyKey::new(Uuid::new_v4()).expect("key"),
    );
    let response = request(
        router,
        Method::POST,
        "/v1/sessions/acquire",
        Some(logged_in.access_token.expose_secret()),
        serde_json::to_value(acquire).expect("acquire json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let lease: LeaseContract = serde_json::from_value(json_body(response).await).expect("lease");
    (registered, logged_in, lease, client)
}

#[tokio::test]
async fn online_snapshot_is_authenticated_fenced_and_bounded() {
    let app = app();
    let router = app.router();
    let (_, login, lease, _) = account(&router, &app, "onlineone", "ONLINE-ONE").await;
    let body = json!({"api_version":1,"fence":lease.fence(),"incoming_after":null});
    let response = request(
        &router,
        Method::POST,
        "/v1/online/snapshot",
        Some(login.access_token.expose_secret()),
        body.clone(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({"api_version":1,"nearby":[],"incoming":[],"outgoing":[],"incoming_next":null,"group":null,"remote_join_possible":false})
    );
    let legacy = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/online/snapshot")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    format!("Bearer {}", login.access_token.expose_secret()),
                )
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::OK);
    let legacy_body = json_body(legacy).await;
    assert!(legacy_body.get("remote_join_possible").is_none());
    let conservative: coop_cloud::OnlineSnapshotResponse =
        serde_json::from_value(legacy_body).unwrap();
    assert!(conservative.remote_join_possible);
    let response = request(&router, Method::POST, "/v1/online/snapshot", None, body).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

async fn online_action(
    router: &Router,
    login: &LoginResponse,
    lease: &LeaseContract,
    action: Value,
    key: Uuid,
) -> axum::response::Response {
    request(
        router,
        Method::POST,
        "/v1/online/actions",
        Some(login.access_token.expose_secret()),
        json!({"api_version":1,"fence":lease.fence(),"idempotency_key":key,"action":action}),
    )
    .await
}

async fn snapshot(
    router: &Router,
    login: &LoginResponse,
    lease: &LeaseContract,
    after: Value,
) -> Value {
    let response = request(
        router,
        Method::POST,
        "/v1/online/snapshot",
        Some(login.access_token.expose_secret()),
        json!({"api_version":1,"fence":lease.fence(),"incoming_after":after}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn invite(
    router: &Router,
    login: &LoginResponse,
    lease: &LeaseContract,
    target: coop_cloud::CharacterId,
) -> Value {
    let request_body = CreateGroupInvitationRequest::new(
        lease.fence(),
        target,
        IdempotencyKey::new(Uuid::new_v4()).unwrap(),
    );
    let response = request(
        router,
        Method::POST,
        "/v1/groups/invitations",
        Some(login.access_token.expose_secret()),
        serde_json::to_value(request_body).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await
}

#[tokio::test]
async fn incoming_names_accept_and_symmetric_leave_are_idempotent() {
    let app = app();
    let router = app.router();
    let (one, login_one, lease_one, _) = account(&router, &app, "onlinealice", "ONLINE-A").await;
    let (two, login_two, lease_two, _) = account(&router, &app, "onlinebob", "ONLINE-B").await;
    let invitation = invite(&router, &login_one, &lease_one, two.character_id).await;
    assert_eq!(
        snapshot(&router, &login_one, &lease_one, Value::Null).await["remote_join_possible"],
        true
    );
    assert_eq!(
        snapshot(&router, &login_two, &lease_two, Value::Null).await["remote_join_possible"],
        false
    );
    let inbox = snapshot(&router, &login_two, &lease_two, Value::Null).await;
    assert_eq!(inbox["incoming"][0]["username"], "onlinealice");
    let accept = json!({"operation":"accept","invitation_id":invitation["invitation_id"]});
    let key = Uuid::new_v4();
    let response = online_action(&router, &login_two, &lease_two, accept.clone(), key).await;
    assert_eq!(response.status(), StatusCode::OK);
    let accepted = json_body(response).await;
    let replay = online_action(&router, &login_two, &lease_two, accept, key).await;
    assert_eq!(json_body(replay).await, accepted);
    let first = snapshot(&router, &login_one, &lease_one, Value::Null).await;
    assert_eq!(first["group"]["username"], "onlinebob");
    assert_eq!(first["remote_join_possible"], false);
    assert!(
        accepted["group"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["character_id"] == json!(one.character_id))
    );
    let leave = json!({"operation":"leave","group_id":accepted["group"]["group_id"]});
    let leave_key = Uuid::new_v4();
    for _ in 0..2 {
        let response =
            online_action(&router, &login_one, &lease_one, leave.clone(), leave_key).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await, json!({"result":"left"}));
    }
    assert!(snapshot(&router, &login_one, &lease_one, Value::Null).await["group"].is_null());
    assert!(snapshot(&router, &login_two, &lease_two, Value::Null).await["group"].is_null());
    // A stale leave cannot close the next group, even when its lost response is retried.
    let next = invite(&router, &login_one, &lease_one, two.character_id).await;
    let response = online_action(
        &router,
        &login_two,
        &lease_two,
        json!({"operation":"accept","invitation_id":next["invitation_id"]}),
        Uuid::new_v4(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        online_action(&router, &login_one, &lease_one, leave, leave_key)
            .await
            .status(),
        StatusCode::OK
    );
    assert!(!snapshot(&router, &login_two, &lease_two, Value::Null).await["group"].is_null());
}

#[tokio::test]
async fn pairing_code_makes_remote_join_possible_until_redeemed() {
    let app = app();
    let router = app.router();
    let (_, creator, creator_lease, _) =
        account(&router, &app, "paironlineone", "PAIR-ONLINE-A").await;
    let (_, joiner, joiner_lease, _) =
        account(&router, &app, "paironlinetwo", "PAIR-ONLINE-B").await;
    assert_eq!(
        snapshot(&router, &creator, &creator_lease, Value::Null).await["remote_join_possible"],
        false
    );
    let response = request(
        &router,
        Method::POST,
        "/v1/groups/pairing-codes",
        Some(creator.access_token.expose_secret()),
        serde_json::to_value(CreatePairingCodeRequest::new(creator_lease.fence())).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(
        snapshot(&router, &creator, &creator_lease, Value::Null).await["remote_join_possible"],
        true
    );
    assert_eq!(
        snapshot(&router, &joiner, &joiner_lease, Value::Null).await["remote_join_possible"],
        false
    );
    let response = request(
        &router,
        Method::POST,
        "/v1/groups/pairing-codes/redeem",
        Some(joiner.access_token.expose_secret()),
        serde_json::to_value(RedeemPairingCodeRequest::new(
            joiner_lease.fence(),
            PairingCode::new(created["code"].as_str().unwrap().to_owned()).unwrap(),
        ))
        .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        snapshot(&router, &creator, &creator_lease, Value::Null).await["remote_join_possible"],
        false
    );
}

#[tokio::test]
async fn incoming_pagination_decline_and_foreign_invitation_are_safe() {
    let app = app();
    let router = app.router();
    let (_, sender, sender_lease, _) = account(&router, &app, "onlinesender", "ONLINE-S").await;
    let (_, other_sender, other_sender_lease, _) =
        account(&router, &app, "onlineother", "ONLINE-O").await;
    let (recipient_id, recipient, recipient_lease, _) =
        account(&router, &app, "onlinereceiver", "ONLINE-R").await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(invite(&router, &sender, &sender_lease, recipient_id.character_id).await["invitation_id"].clone());
    }
    for _ in 0..2 {
        ids.push(
            invite(
                &router,
                &other_sender,
                &other_sender_lease,
                recipient_id.character_id,
            )
            .await["invitation_id"]
                .clone(),
        );
    }
    let sent = snapshot(&router, &sender, &sender_lease, Value::Null).await;
    assert_eq!(sent["remote_join_possible"], true);
    assert_eq!(sent["outgoing"].as_array().unwrap().len(), 3);
    assert!(
        sent["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["username"] == "onlinereceiver")
    );
    let other_sent = snapshot(&router, &other_sender, &other_sender_lease, Value::Null).await;
    assert_eq!(other_sent["outgoing"].as_array().unwrap().len(), 2);
    assert_eq!(other_sent["remote_join_possible"], true);
    ids.sort_by_key(|id| id.as_str().unwrap().to_owned());
    let first = snapshot(&router, &recipient, &recipient_lease, Value::Null).await;
    assert!(first["outgoing"].as_array().unwrap().is_empty());
    assert_eq!(first["remote_join_possible"], false);
    assert_eq!(first["incoming"].as_array().unwrap().len(), 4);
    let second = snapshot(
        &router,
        &recipient,
        &recipient_lease,
        first["incoming_next"].clone(),
    )
    .await;
    assert_eq!(second["incoming"].as_array().unwrap().len(), 1);
    assert_eq!(second["incoming"][0]["invitation"]["invitation_id"], ids[4]);
    assert!(second["incoming_next"].is_null());
    let decline = json!({"operation":"decline","invitation_id":ids[0]});
    assert_eq!(
        online_action(
            &router,
            &sender,
            &sender_lease,
            decline.clone(),
            Uuid::new_v4()
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let key = Uuid::new_v4();
    for _ in 0..2 {
        assert_eq!(
            online_action(&router, &recipient, &recipient_lease, decline.clone(), key)
                .await
                .status(),
            StatusCode::OK
        );
    }
    let response = online_action(
        &router,
        &recipient,
        &recipient_lease,
        json!({"operation":"decline","invitation_id":ids[1]}),
        key,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = online_action(
        &router,
        &recipient,
        &recipient_lease,
        json!({"operation":"accept","invitation_id":ids[0]}),
        Uuid::new_v4(),
    )
    .await;
    assert_ne!(response.status(), StatusCode::OK);
}

#[test]
fn online_contract_rejects_unbounded_or_ambiguous_wire_data() {
    let peer = json!({"handle":"0000000000000001","generation":1,"username":"alice"});
    let base = json!({"api_version":1,"nearby":[],"incoming":[],"incoming_next":null,"group":null,"remote_join_possible":false});
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(base.clone()).is_ok());
    let mut missing = base.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("remote_join_possible");
    assert!(
        serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(missing)
            .unwrap()
            .remote_join_possible
    );
    let mut bounded = base.clone();
    bounded["nearby"] = json!([peer, peer, peer, peer]);
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(bounded).is_ok());
    let mut oversized = base.clone();
    oversized["nearby"] = json!([peer, peer, peer, peer, peer]);
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(oversized).is_err());
    let mut unknown = base;
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(unknown).is_err());
    assert!(
        serde_json::from_value::<coop_cloud::OnlineAction>(
            json!({"operation":"invite","handle":"0000000000000001","generation":1})
        )
        .is_ok()
    );
    assert!(
        serde_json::from_value::<coop_cloud::OnlineAction>(
            json!({"operation":"invite","handle":"0000000000000001","generation":0})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<coop_cloud::OnlineAction>(
            json!({"operation":"invite","handle":"0000000000000001","generation":1,"character_id":Uuid::new_v4()})
        )
        .is_err()
    );
}

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use coop_cloud::{
    AcquireLeaseRequest, ClientInstanceId, CreateGroupInvitationRequest, IdempotencyKey,
    InvitationCode, LeaseContract, LoginRequest, LoginResponse, Password, RegisterRequest,
    RegisterResponse, SigningPrivateKey,
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
        json!({"api_version":1,"nearby":[],"incoming":[],"incoming_next":null,"group":null})
    );
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
async fn incoming_pagination_decline_and_foreign_invitation_are_safe() {
    let app = app();
    let router = app.router();
    let (_, sender, sender_lease, _) = account(&router, &app, "onlinesender", "ONLINE-S").await;
    let (recipient_id, recipient, recipient_lease, _) =
        account(&router, &app, "onlinereceiver", "ONLINE-R").await;
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(invite(&router, &sender, &sender_lease, recipient_id.character_id).await["invitation_id"].clone());
    }
    ids.sort_by_key(|id| id.as_str().unwrap().to_owned());
    let first = snapshot(&router, &recipient, &recipient_lease, Value::Null).await;
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
    let peer = json!({"handle":1,"generation":1,"username":"alice"});
    let base = json!({"api_version":1,"nearby":[],"incoming":[],"incoming_next":null,"group":null});
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(base.clone()).is_ok());
    let mut oversized = base.clone();
    oversized["nearby"] = json!([peer, peer, peer, peer, peer]);
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(oversized).is_err());
    let mut unknown = base;
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<coop_cloud::OnlineSnapshotResponse>(unknown).is_err());
    assert!(
        serde_json::from_value::<coop_cloud::OnlineAction>(
            json!({"operation":"invite","handle":1,"generation":0})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<coop_cloud::OnlineAction>(
            json!({"operation":"invite","handle":1,"generation":1,"character_id":Uuid::new_v4()})
        )
        .is_err()
    );
}

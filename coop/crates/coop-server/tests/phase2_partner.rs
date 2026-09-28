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
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
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
    app.add_invitation(invitation).unwrap();
    let register = RegisterRequest::new(
        name,
        Password::new("correct horse battery staple").unwrap(),
        InvitationCode::new(invitation).unwrap(),
    )
    .unwrap();
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/auth/register",
        None,
        serde_json::to_value(register).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let registered: RegisterResponse = serde_json::from_value(body).unwrap();
    let login =
        LoginRequest::new(name, Password::new("correct horse battery staple").unwrap()).unwrap();
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/auth/login",
        None,
        serde_json::to_value(login).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let logged_in: LoginResponse = serde_json::from_value(body).unwrap();
    let acquire = AcquireLeaseRequest::new(
        registered.character_id,
        ClientInstanceId::new(Uuid::new_v4()).unwrap(),
        IdempotencyKey::new(Uuid::new_v4()).unwrap(),
    );
    let (status, body) = call(
        router,
        Method::POST,
        "/v1/sessions/acquire",
        Some(logged_in.access_token.expose_secret()),
        serde_json::to_value(acquire).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    (registered, logged_in, serde_json::from_value(body).unwrap())
}

#[tokio::test]
async fn partner_panel_status_is_private_and_retains_last_partner_after_leave() {
    let app = Phase2App::new(
        Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "partner-test-key",
        )
        .unwrap(),
    )
    .unwrap();
    let router = app.router();
    let (status, _) = call(&router, Method::GET, "/v1/group/partner", None, Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (one, login_one, lease_one) = account(&app, &router, "partneralice", "PARTNER-A").await;
    let (two, login_two, lease_two) = account(&app, &router, "partnerbob", "PARTNER-B").await;
    let token_one = login_one.access_token.expose_secret();
    let token_two = login_two.access_token.expose_secret();
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/group/partner",
        Some(token_one),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["partner"].is_null());
    let invite = CreateGroupInvitationRequest::new(
        lease_one.fence(),
        two.character_id,
        IdempotencyKey::new(Uuid::new_v4()).unwrap(),
    );
    let (status, body) = call(
        &router,
        Method::POST,
        "/v1/groups/invitations",
        Some(token_one),
        serde_json::to_value(invite).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, accepted) = call(&router, Method::POST, "/v1/online/actions", Some(token_two), json!({"api_version":1,"fence":lease_two.fence(),"idempotency_key":Uuid::new_v4(),"action":{"operation":"accept","invitation_id":body["invitation_id"]}})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/group/partner",
        Some(token_one),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["partner"]["username"], "partnerbob");
    assert_eq!(body["partner"]["online"], true);
    assert_eq!(body["partner"]["group_active"], true);
    assert!(body["partner"]["last_seen_at"].as_u64().unwrap() > 0);
    assert_eq!(body["partner"]["badge_count"], 0);
    assert!(body["partner"]["world_zone"]["map"].is_string());
    assert!(body["partner"]["live_world_zone"].is_null());
    let (status, _) = call(&router, Method::POST, "/v1/online/actions", Some(token_one), json!({"api_version":1,"fence":lease_one.fence(),"idempotency_key":Uuid::new_v4(),"action":{"operation":"leave","group_id":accepted["group"]["group_id"]}})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/group/partner",
        Some(token_one),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["partner"]["username"], "partnerbob");
    assert_eq!(body["partner"]["group_active"], false);
    assert!(body["partner"]["live_world_zone"].is_null());
    assert_ne!(one.character_id, two.character_id);
}

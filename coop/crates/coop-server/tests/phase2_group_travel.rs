use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use coop_cloud::{
    AcceptGroupInvitationRequest, AcceptGroupInvitationResponse, AcquireLeaseRequest,
    ClientInstanceId, CreateGroupInvitationRequest, Group, GroupTravelRequest, GroupView,
    IdempotencyKey, InvitationCode, LeaseContract, LoginRequest, LoginResponse, Password,
    RegisterRequest, RegisterResponse, SigningPrivateKey,
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

fn headers(lease: &LeaseContract, client: ClientInstanceId) -> Vec<(&'static str, String)> {
    vec![
        ("x-coop-session-id", lease.session_id.to_string()),
        (
            "x-coop-session-epoch",
            lease.session_epoch.value().to_string(),
        ),
        ("x-coop-client-instance-id", client.to_string()),
    ]
}

#[test]
fn group_dto_is_strict_and_members_are_canonical() {
    let first = coop_cloud::CharacterId::new(Uuid::from_u128(1)).expect("id");
    let second = coop_cloud::CharacterId::new(Uuid::from_u128(2)).expect("id");
    assert!(Group::new(first, first).is_err());
    assert!(serde_json::from_value::<Group>(json!({"members":[second,first]})).is_err());
    assert!(
        serde_json::from_value::<GroupTravelRequest>(json!({
            "api_version":1,"route_id":"HOENN_TO_SEVII","session_id":Uuid::new_v4(),
            "character_id":first,"current_revision":0,"session_epoch":1,
            "client_instance_id":Uuid::new_v4(),"idempotency_key":Uuid::new_v4()
        }))
        .is_err()
    );
    let group_id = coop_cloud::GroupId::new(Uuid::new_v4()).expect("group");
    let group = Group::new(second, first).expect("group");
    let zone = coop_protocol::WorldZone::new(coop_protocol::RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
        .expect("zone");
    let view = GroupView::new(group_id, group, zone, [4, 3]).expect("view");
    assert_eq!(view.members[0].character_id, first);
    assert_eq!(view.members[0].world_revision, 4);
    assert_eq!(view.members[1].character_id, second);
    assert_eq!(view.members[1].world_revision, 3);
    assert_eq!(
        serde_json::from_value::<GroupView>(serde_json::to_value(&view).expect("wire"))
            .expect("round trip"),
        view
    );
    let accept_response = AcceptGroupInvitationResponse {
        api_version: coop_cloud::ApiVersion::V1,
        group: view.clone(),
    };
    let accept_wire = serde_json::to_value(&accept_response).expect("accept wire");
    assert!(
        !accept_wire
            .as_object()
            .expect("accept response object")
            .contains_key("replayed")
    );
    assert!(
        serde_json::from_value::<AcceptGroupInvitationResponse>({
            let mut value = accept_wire.clone();
            value["replayed"] = json!(false);
            value
        })
        .is_err()
    );
    let travel_response = coop_cloud::GroupTravelResponse {
        api_version: coop_cloud::ApiVersion::V1,
        group: view,
    };
    let travel_wire = serde_json::to_value(&travel_response).expect("travel wire");
    assert!(
        !travel_wire
            .as_object()
            .expect("travel response object")
            .contains_key("replayed")
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "the real transport test keeps bounded request and lifecycle assertions together"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_http_invite_accept_inspect_and_bounded_rejections() {
    let app = app();
    let router = app.router();
    let (_first, first_login, first_lease, first_client) =
        account(&router, &app, "group_one", "group-invite-one").await;
    let (second, second_login, second_lease, second_client) =
        account(&router, &app, "group_two", "group-invite-two").await;
    let (third, third_login, third_lease, third_client) =
        account(&router, &app, "group_three", "group-invite-three").await;

    let first_fence = first_lease.fence();
    let create_key = IdempotencyKey::new(Uuid::new_v4()).expect("key");
    let create = CreateGroupInvitationRequest::new(first_fence, second.character_id, create_key);
    let mut create_wire = serde_json::to_value(create).expect("create json");
    create_wire["unknown"] = json!(true);
    let response = request(
        &router,
        Method::POST,
        "/v1/groups/invitations",
        Some(first_login.access_token.expose_secret()),
        create_wire,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    let oversized = json!({"padding":"x".repeat(9_000)});
    let response = request(
        &router,
        Method::POST,
        "/v1/groups/invitations",
        Some(first_login.access_token.expose_secret()),
        oversized,
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        json_body(response).await,
        json!({"error":{"code":"payload_too_large"}})
    );

    let oversized_group_query = format!("/v1/groups/invitations?padding={}", "x".repeat(2_048));
    let response = request(
        &router,
        Method::POST,
        &oversized_group_query,
        Some(first_login.access_token.expose_secret()),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.headers().get_all("cache-control").iter().count(),
        1
    );
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        json_body(response).await,
        json!({"error":{"code":"payload_too_large"}})
    );

    let oversized_realtime_query = format!("/v1/realtime?padding={}", "x".repeat(2_048));
    let response = request(
        &router,
        Method::GET,
        &oversized_realtime_query,
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.headers().get_all("cache-control").iter().count(),
        1
    );
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        json_body(response).await,
        json!({"error":{"code":"payload_too_large"}})
    );

    let oversized_unrelated_query = format!("/v1/auth/login?padding={}", "x".repeat(2_048));
    let response = request(
        &router,
        Method::POST,
        &oversized_unrelated_query,
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(response.headers().get("cache-control").is_none());

    let oversized_group_lookalike = format!("/v1/groups-not-a-group?padding={}", "x".repeat(2_048));
    let response = request(
        &router,
        Method::GET,
        &oversized_group_lookalike,
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(response.headers().get("cache-control").is_none());

    let oversized_realtime_lookalike = format!("/v1/realtime-ish?padding={}", "x".repeat(2_048));
    let response = request(
        &router,
        Method::GET,
        &oversized_realtime_lookalike,
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(response.headers().get("cache-control").is_none());

    let response = request(
        &router,
        Method::POST,
        "/v1/groups/invitations",
        Some(first_login.access_token.expose_secret()),
        serde_json::to_value(create).expect("create json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let invitation = json_body(response).await;
    let invitation_id = invitation["invitation_id"].as_str().expect("invitation ID");

    let mut changed_create = create;
    changed_create.invitee_character_id = third.character_id;
    let response = request(
        &router,
        Method::POST,
        "/v1/groups/invitations",
        Some(first_login.access_token.expose_secret()),
        serde_json::to_value(changed_create).expect("changed create json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let accept = AcceptGroupInvitationRequest::new(
        second_lease.fence(),
        IdempotencyKey::new(Uuid::new_v4()).expect("key"),
    );
    let uri = format!("/v1/groups/invitations/{invitation_id}/accept");
    let response = request(
        &router,
        Method::POST,
        &uri,
        Some(second_login.access_token.expose_secret()),
        serde_json::to_value(accept).expect("accept json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let accepted = json_body(response).await;
    assert!(
        !accepted
            .as_object()
            .expect("accept object")
            .contains_key("replayed")
    );
    let response = request(
        &router,
        Method::POST,
        &uri,
        Some(second_login.access_token.expose_secret()),
        serde_json::to_value(accept).expect("accept replay json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await, accepted);
    let group_id = accepted["group"]["group_id"].as_str().expect("group ID");
    assert_eq!(
        accepted["group"]["members"]
            .as_array()
            .expect("members")
            .len(),
        2
    );

    let inspect_uri = format!("/v1/groups/{group_id}");
    let mut foreign_builder = Request::builder().method(Method::GET).uri(&inspect_uri);
    foreign_builder = foreign_builder.header(
        "authorization",
        format!("Bearer {}", third_login.access_token.expose_secret()),
    );
    for (name, value) in headers(&third_lease, third_client) {
        foreign_builder = foreign_builder.header(name, value);
    }
    let response = router
        .clone()
        .oneshot(
            foreign_builder
                .body(Body::empty())
                .expect("foreign request"),
        )
        .await
        .expect("foreign response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let mut wrong_fence_builder = Request::builder().method(Method::GET).uri(&inspect_uri);
    wrong_fence_builder = wrong_fence_builder.header(
        "authorization",
        format!("Bearer {}", first_login.access_token.expose_secret()),
    );
    for (name, value) in headers(&second_lease, second_client) {
        wrong_fence_builder = wrong_fence_builder.header(name, value);
    }
    let response = router
        .clone()
        .oneshot(
            wrong_fence_builder
                .body(Body::empty())
                .expect("wrong fence request"),
        )
        .await
        .expect("wrong fence response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let mut request_builder = Request::builder().method(Method::GET).uri(&inspect_uri);
    request_builder = request_builder.header(
        "authorization",
        format!("Bearer {}", first_login.access_token.expose_secret()),
    );
    for (name, value) in headers(&first_lease, first_client) {
        request_builder = request_builder.header(name, value);
    }
    let response = router
        .clone()
        .oneshot(
            request_builder
                .body(Body::empty())
                .expect("inspect request"),
        )
        .await
        .expect("inspect response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let inspected = json_body(response).await;
    assert_eq!(inspected["group_id"], group_id);
    assert_eq!(inspected["world_zone"]["region"], "HOENN");

    let travel = GroupTravelRequest::new(
        first_lease.fence(),
        "HOENN:SLATEPORT_SEVII_FERRY",
        IdempotencyKey::new(Uuid::new_v4()).expect("key"),
    )
    .expect("travel");
    let travel_uri = format!("/v1/groups/{group_id}/travel");
    let response = request(
        &router,
        Method::POST,
        &travel_uri,
        Some(first_login.access_token.expose_secret()),
        serde_json::to_value(travel).expect("travel json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let _ = second_client;
}

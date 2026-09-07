//! Real authenticated HTTP inbox, accept/decline, replay, and symmetric leave.
use coop_cloud::{
    AcquireLeaseRequest, ApiVersion, ClientInstanceId, CreateGroupInvitationRequest,
    GroupInvitationView, IdempotencyKey, InvitationCode, LeaseContract, LoginRequest,
    LoginResponse, OnlineAction, OnlineActionRequest, OnlineActionResponse, OnlineSnapshotRequest,
    Password, RegisterRequest, SigningPrivateKey,
};
use coop_launcher::{CloudApi, ReqwestCloudApi};
use coop_server::{Phase2App, Phase2Config};
use uuid::Uuid;

fn key() -> IdempotencyKey {
    IdempotencyKey::new(Uuid::new_v4()).unwrap()
}

async fn account(app: &Phase2App, base: &str, name: &str) -> (LoginResponse, LeaseContract) {
    app.add_invitation(name).unwrap();
    let registered = app
        .register(
            RegisterRequest::new(
                name,
                Password::new("test strong password").unwrap(),
                InvitationCode::new(name).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let login = app
        .login(LoginRequest::new(name, Password::new("test strong password").unwrap()).unwrap())
        .unwrap();
    let lease = reqwest::Client::new()
        .post(format!("{base}/v1/sessions/acquire"))
        .bearer_auth(login.access_token.expose_secret())
        .json(&AcquireLeaseRequest::new(
            registered.character_id,
            ClientInstanceId::new(Uuid::new_v4()).unwrap(),
            key(),
        ))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    (login, lease)
}

async fn invite(
    base: &str,
    login: &LoginResponse,
    lease: &LeaseContract,
    recipient: &LeaseContract,
) -> GroupInvitationView {
    reqwest::Client::new()
        .post(format!("{base}/v1/groups/invitations"))
        .bearer_auth(login.access_token.expose_secret())
        .json(&CreateGroupInvitationRequest::new(
            lease.fence(),
            recipient.character_id,
            key(),
        ))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "two authenticated players exercise the complete invitation and symmetric group transaction chain"
)]
async fn online_http_inbox_accept_decline_and_symmetric_leave_use_real_server() {
    let config = Phase2Config::local(
        vec![0x55; 32],
        SigningPrivateKey::from_bytes([7; 32]),
        "online-test",
    )
    .unwrap();
    let app = Phase2App::new(config).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let router = app.router();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    let (one, one_lease) = account(&app, &base, "onlineone").await;
    let (two, two_lease) = account(&app, &base, "onlinetwo").await;
    let api = ReqwestCloudApi::new(&base).unwrap();
    let first = invite(&base, &one, &one_lease, &two_lease).await;
    let two_request = OnlineSnapshotRequest {
        api_version: ApiVersion::V1,
        fence: two_lease.fence(),
        incoming_after: None,
    };
    let inbox = api
        .online_snapshot(two.access_token.clone(), two_request.clone())
        .await
        .unwrap();
    assert_eq!(inbox.incoming.len(), 1);
    assert_eq!(inbox.incoming[0].username.as_str(), "onlineone");
    assert_eq!(
        inbox.incoming[0].invitation.invitation_id,
        first.invitation_id
    );
    let accept = OnlineActionRequest {
        api_version: ApiVersion::V1,
        fence: two_lease.fence(),
        idempotency_key: key(),
        action: OnlineAction::Accept {
            invitation_id: first.invitation_id,
        },
    };
    let accepted = api
        .online_action(two.access_token.clone(), accept.clone())
        .await
        .unwrap();
    assert_eq!(
        api.online_action(two.access_token.clone(), accept)
            .await
            .unwrap(),
        accepted
    );
    let OnlineActionResponse::Accepted { group } = accepted else {
        panic!("accepted group");
    };
    let one_request = OnlineSnapshotRequest {
        api_version: ApiVersion::V1,
        fence: one_lease.fence(),
        incoming_after: None,
    };
    assert_eq!(
        api.online_snapshot(one.access_token.clone(), one_request.clone())
            .await
            .unwrap()
            .group
            .unwrap()
            .username
            .as_str(),
        "onlinetwo"
    );
    assert_eq!(
        api.online_snapshot(two.access_token.clone(), two_request.clone())
            .await
            .unwrap()
            .group
            .unwrap()
            .username
            .as_str(),
        "onlineone"
    );
    let leave = OnlineActionRequest {
        api_version: ApiVersion::V1,
        fence: one_lease.fence(),
        idempotency_key: key(),
        action: OnlineAction::Leave {
            group_id: group.group_id,
        },
    };
    for _ in 0..2 {
        assert_eq!(
            api.online_action(one.access_token.clone(), leave.clone())
                .await
                .unwrap(),
            OnlineActionResponse::Left
        );
    }
    assert!(
        api.online_snapshot(one.access_token.clone(), one_request)
            .await
            .unwrap()
            .group
            .is_none()
    );
    assert!(
        api.online_snapshot(two.access_token.clone(), two_request.clone())
            .await
            .unwrap()
            .group
            .is_none()
    );
    let second = invite(&base, &one, &one_lease, &two_lease).await;
    assert_eq!(
        api.online_action(
            two.access_token.clone(),
            OnlineActionRequest {
                api_version: ApiVersion::V1,
                fence: two_lease.fence(),
                idempotency_key: key(),
                action: OnlineAction::Decline {
                    invitation_id: second.invitation_id
                }
            }
        )
        .await
        .unwrap(),
        OnlineActionResponse::Declined
    );
    assert!(
        api.online_snapshot(two.access_token, two_request)
            .await
            .unwrap()
            .incoming
            .is_empty()
    );
    stop_tx.send(()).unwrap();
    server.await.unwrap();
}

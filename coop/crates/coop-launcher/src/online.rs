//! Bounded Online work and immutable, generation-fenced menu selections.
use std::{future::Future, pin::Pin, time::Duration};

use coop_cloud::{
    AccessToken, ApiVersion, IdempotencyKey, LeaseFence, OnlineAction as CloudAction,
    OnlineActionRequest, OnlineActionResponse, OnlineSnapshotRequest, OnlineSnapshotResponse,
};
use coop_protocol::{
    OnlineAction, OnlineRequest, OnlineResult, OnlineStatus, PairingAction, PairingRequest,
    PairingResult, PairingStatus,
};
use reqwest::StatusCode;
use thiserror::Error;
use tokio::time::Instant;

use crate::{CloudApi, HttpClientError, ReqwestCloudApi, SessionError};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OnlineError {
    #[error("Online is unavailable")]
    Unavailable,
    #[error("Online selection is stale")]
    Stale,
    #[error("Online authorization failed")]
    Unauthorized,
    #[error("Online response is invalid")]
    InvalidResponse,
}

pub type OnlineFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, OnlineError>> + Send + 'a>>;

pub(crate) struct PairingCompletion {
    request_id: u32,
    fence: LeaseFence,
    generation: u32,
    result: Result<PairingStatus, OnlineError>,
}

#[derive(Default)]
pub(crate) struct PairingOwner<'a> {
    pending: Option<Pin<Box<dyn Future<Output = PairingCompletion> + Send + 'a>>>,
}

impl<'a> PairingOwner<'a> {
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn invalidate(&mut self) {
        self.pending = None;
    }
    pub(crate) fn start<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
        request: PairingRequest,
    ) -> Option<PairingStatus> {
        let request_id = request.request_id;
        if request.encode().is_err() {
            return Some(PairingStatus {
                request_id,
                result: PairingResult::Invalid,
                code: String::new(),
            });
        }
        if self.pending.is_some() {
            return Some(PairingStatus {
                request_id,
                result: PairingResult::Unavailable,
                code: String::new(),
            });
        }
        self.pending = Some(Box::pin(async move {
            let result = tokio::time::timeout(Duration::from_secs(5), async {
                match request.action {
                    PairingAction::Create => {
                        let response = api
                            .pairing_create(token, coop_cloud::CreatePairingCodeRequest::new(fence))
                            .await?;
                        if response.api_version != ApiVersion::V1 {
                            return Err(OnlineError::InvalidResponse);
                        }
                        Ok(PairingStatus {
                            request_id,
                            result: PairingResult::Created,
                            code: response.code.as_str().to_owned(),
                        })
                    }
                    PairingAction::Redeem => {
                        let code = coop_cloud::PairingCode::new(request.code)
                            .map_err(|_| OnlineError::InvalidResponse)?;
                        let response = api
                            .pairing_redeem(
                                token,
                                coop_cloud::RedeemPairingCodeRequest::new(fence, code),
                            )
                            .await?;
                        if response.api_version != ApiVersion::V1 {
                            return Err(OnlineError::InvalidResponse);
                        }
                        Ok(PairingStatus {
                            request_id,
                            result: PairingResult::Joined,
                            code: String::new(),
                        })
                    }
                }
            })
            .await
            .unwrap_or(Err(OnlineError::Unavailable));
            PairingCompletion {
                request_id,
                fence,
                generation,
                result,
            }
        }));
        None
    }
    pub(crate) async fn next(&mut self) -> PairingCompletion {
        match &mut self.pending {
            Some(future) => future.await,
            None => std::future::pending().await,
        }
    }
    pub(crate) fn finish(
        &mut self,
        completion: PairingCompletion,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Option<PairingStatus>, SessionError> {
        self.pending = None;
        if completion.fence != fence || completion.generation != generation {
            return Ok(None);
        }
        let status = match completion.result {
            Ok(status) => status,
            Err(OnlineError::Unauthorized) => return Err(SessionError::Unauthorized),
            Err(OnlineError::InvalidResponse) => return Err(SessionError::Realtime),
            Err(OnlineError::Stale) => PairingStatus {
                request_id: completion.request_id,
                result: PairingResult::Invalid,
                code: String::new(),
            },
            Err(OnlineError::Unavailable) => PairingStatus {
                request_id: completion.request_id,
                result: PairingResult::Unavailable,
                code: String::new(),
            },
        };
        status.encode().map_err(|_| SessionError::Realtime)?;
        Ok(Some(status))
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err consumes the HTTP error"
)]
fn map_http_error(error: HttpClientError) -> OnlineError {
    match error {
        HttpClientError::Status(StatusCode::UNAUTHORIZED) | HttpClientError::SessionClosed => {
            OnlineError::Unauthorized
        }
        HttpClientError::Status(
            StatusCode::CONFLICT | StatusCode::NOT_FOUND | StatusCode::GONE | StatusCode::FORBIDDEN,
        ) => OnlineError::Stale,
        HttpClientError::Status(StatusCode::TOO_MANY_REQUESTS) | HttpClientError::Transport(_) => {
            OnlineError::Unavailable
        }
        HttpClientError::Status(status) if status.is_server_error() => OnlineError::Unavailable,
        _ => OnlineError::InvalidResponse,
    }
}

impl ReqwestCloudApi {
    pub(crate) fn pairing_create_http(
        &self,
        token: AccessToken,
        request: coop_cloud::CreatePairingCodeRequest,
    ) -> OnlineFuture<'_, coop_cloud::CreatePairingCodeResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/groups/pairing-codes")
                .map_err(map_http_error)?;
            self.send_online(
                self.client
                    .post(url)
                    .bearer_auth(token.expose_secret())
                    .json(&request),
                2048,
            )
            .await
        })
    }
    pub(crate) fn pairing_redeem_http(
        &self,
        token: AccessToken,
        request: coop_cloud::RedeemPairingCodeRequest,
    ) -> OnlineFuture<'_, coop_cloud::RedeemPairingCodeResponse> {
        Box::pin(async move {
            let url = self
                .url("v1/groups/pairing-codes/redeem")
                .map_err(map_http_error)?;
            self.send_online(
                self.client
                    .post(url)
                    .bearer_auth(token.expose_secret())
                    .json(&request),
                4096,
            )
            .await
        })
    }
    async fn send_online<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        max: usize,
    ) -> Result<T, OnlineError> {
        let response = request.send().await.map_err(|_| OnlineError::Unavailable)?;
        let status = response.status();
        if !status.is_success() {
            if status.is_server_error() {
                return Err(OnlineError::Unavailable);
            }
            let bytes = crate::bounded_body(response, 1024)
                .await
                .map_err(map_http_error)?;
            // The server reserves `authentication_failed` for invalid leases
            // and credentials. `expired` here is an invitation-domain result.
            let body: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| OnlineError::InvalidResponse)?;
            if status == StatusCode::UNAUTHORIZED && body["error"]["code"] == "expired" {
                return Err(OnlineError::Stale);
            }
            return Err(map_http_error(HttpClientError::Status(status)));
        }
        let bytes = crate::bounded_body(response, max)
            .await
            .map_err(map_http_error)?;
        serde_json::from_slice(&bytes).map_err(|_| OnlineError::InvalidResponse)
    }
    pub(crate) fn online_snapshot_http(
        &self,
        token: AccessToken,
        request: OnlineSnapshotRequest,
    ) -> OnlineFuture<'_, OnlineSnapshotResponse> {
        Box::pin(async move {
            let url = self.url("v1/online/snapshot").map_err(map_http_error)?;
            self.send_online(
                self.client
                    .post(url)
                    .bearer_auth(token.expose_secret())
                    .json(&request),
                16 * 1024,
            )
            .await
        })
    }

    pub(crate) fn online_action_http(
        &self,
        token: AccessToken,
        request: OnlineActionRequest,
    ) -> OnlineFuture<'_, OnlineActionResponse> {
        Box::pin(async move {
            let url = self.url("v1/online/actions").map_err(map_http_error)?;
            self.send_online(
                self.client
                    .post(url)
                    .bearer_auth(token.expose_secret())
                    .json(&request),
                8 * 1024,
            )
            .await
        })
    }
}

struct View {
    id: u32,
    fence: LeaseFence,
    generation: u32,
    snapshot: OnlineSnapshotResponse,
}

pub(crate) struct OnlineCompletion {
    request: OnlineRequest,
    fence: LeaseFence,
    generation: u32,
    result: Result<OnlineSnapshotResponse, OnlineError>,
}

#[derive(Default)]
pub(crate) struct OnlineOwner<'a> {
    view: Option<View>,
    latest_request: u32,
    pending: Option<Pin<Box<dyn Future<Output = OnlineCompletion> + Send + 'a>>>,
}

impl<'a> OnlineOwner<'a> {
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn invalidate(&mut self) {
        self.view = None;
        // A mutation already received by the server may still finish. Drop
        // local work so a not-yet-polled request cannot start after cutover;
        // the next snapshot discovers the authoritative result.
        self.pending = None;
        self.latest_request = 0;
    }

    pub(crate) fn invalidate_for_group_change(&mut self) -> Option<OnlineStatus> {
        self.view = None;
        if self.pending.take().is_some() {
            Some(empty_status(self.latest_request, OnlineResult::Unavailable))
        } else {
            None
        }
    }

    pub(crate) fn start<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
        request: OnlineRequest,
    ) -> Option<OnlineStatus> {
        if request.encode().is_err() {
            return Some(empty_status(request.request_id, OnlineResult::Failed));
        }
        self.latest_request = request.request_id;
        if self.pending.is_some() {
            self.view = None;
            return Some(empty_status(request.request_id, OnlineResult::Unavailable));
        }
        let Ok(action) = self.selected_action(request, fence, generation) else {
            return Some(empty_status(request.request_id, OnlineResult::Stale));
        };
        self.view = None;
        self.pending = Some(Box::pin(async move {
            let result = tokio::time::timeout(Duration::from_secs(5), async {
                if let Some(action) = action {
                    let action_request = OnlineActionRequest {
                        api_version: ApiVersion::V1,
                        fence,
                        idempotency_key: IdempotencyKey::new(uuid::Uuid::new_v4())
                            .expect("v4 UUID is nonzero"),
                        action,
                    };
                    // A lost response is retried once with the exact same key
                    // and identities. An explicit rejection is never retried.
                    let response = match api
                        .online_action(token.clone(), action_request.clone())
                        .await
                    {
                        Err(OnlineError::Unavailable) => {
                            api.online_action(token.clone(), action_request.clone())
                                .await
                        }
                        result => result,
                    }?;
                    if !action_response_matches(&action_request.action, &response) {
                        return Err(OnlineError::InvalidResponse);
                    }
                }
                load_snapshot(api, token, fence).await
            })
            .await
            .unwrap_or(Err(OnlineError::Unavailable));
            OnlineCompletion {
                request,
                fence,
                generation,
                result,
            }
        }));
        None
    }

    fn selected_action(
        &self,
        request: OnlineRequest,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Option<CloudAction>, ()> {
        if request.action == OnlineAction::Refresh {
            return Ok(None);
        }
        let view = self.view.as_ref().ok_or(())?;
        if view.id != request.view_id || view.fence != fence || view.generation != generation {
            return Err(());
        }
        let page = usize::from(request.page);
        Ok(Some(match request.action {
            OnlineAction::Invite => {
                let peer = view.snapshot.nearby.get(page).ok_or(())?;
                CloudAction::Invite {
                    handle: peer.handle,
                    generation: peer.generation,
                }
            }
            OnlineAction::Accept => CloudAction::Accept {
                invitation_id: view
                    .snapshot
                    .incoming
                    .get(page)
                    .ok_or(())?
                    .invitation
                    .invitation_id,
            },
            OnlineAction::Decline => CloudAction::Decline {
                invitation_id: view
                    .snapshot
                    .incoming
                    .get(page)
                    .ok_or(())?
                    .invitation
                    .invitation_id,
            },
            OnlineAction::Cancel => CloudAction::Cancel {
                invitation_id: view
                    .snapshot
                    .outgoing
                    .get(page)
                    .ok_or(())?
                    .invitation
                    .invitation_id,
            },
            OnlineAction::Leave => CloudAction::Leave {
                group_id: view.snapshot.group.as_ref().ok_or(())?.group.group_id,
            },
            OnlineAction::InviteLastPartner => {
                if view.snapshot.group.is_some() || view.snapshot.last_partner.is_none() {
                    return Err(());
                }
                CloudAction::InviteLastPartner
            }
            OnlineAction::Refresh => unreachable!(),
        }))
    }

    pub(crate) async fn next(&mut self) -> OnlineCompletion {
        match &mut self.pending {
            Some(future) => future.await,
            None => std::future::pending().await,
        }
    }

    pub(crate) fn finish(
        &mut self,
        completion: OnlineCompletion,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Option<OnlineStatus>, SessionError> {
        self.pending = None;
        if completion.fence != fence || completion.generation != generation {
            return Ok(None);
        }
        // Authorization/protocol failures are terminal even if the menu closed
        // or another request superseded its output under this same fence.
        if matches!(completion.result, Err(OnlineError::Unauthorized)) {
            return Err(SessionError::Unauthorized);
        }
        if matches!(completion.result, Err(OnlineError::InvalidResponse)) {
            return Err(SessionError::Realtime);
        }
        if completion.request.request_id != self.latest_request {
            return Ok(None);
        }
        let result = if completion.request.action == OnlineAction::Refresh {
            OnlineResult::Ready
        } else {
            OnlineResult::Success
        };
        match completion.result {
            Ok(snapshot) => {
                let status = snapshot_status(
                    completion.request,
                    &snapshot,
                    result,
                    completion.fence.character_id,
                );
                status.encode().map_err(|_| SessionError::Realtime)?;
                self.view = Some(View {
                    id: completion.request.request_id,
                    fence,
                    generation,
                    snapshot,
                });
                Ok(Some(status))
            }
            Err(OnlineError::Stale) => Ok(Some(empty_status(
                completion.request.request_id,
                OnlineResult::Stale,
            ))),
            Err(_) => Ok(Some(empty_status(
                completion.request.request_id,
                OnlineResult::Unavailable,
            ))),
        }
    }
}

/// Polls the authoritative invitation list without disturbing an open menu
/// view. IDs remain in the launcher; the ROM receives only a notification.
pub(crate) enum InviteWatcherEvent {
    GroupStateChanged(bool),
    InviteReceived(String),
}

pub(crate) struct InviteWatcher<'a> {
    pending: Option<
        Pin<Box<dyn Future<Output = Result<OnlineSnapshotResponse, OnlineError>> + Send + 'a>>,
    >,
    next_poll: Instant,
    seen: Vec<(coop_cloud::GroupInvitationId, u64)>,
    announcements: std::collections::VecDeque<String>,
    membership_pending: Option<bool>,
}

impl<'a> Default for InviteWatcher<'a> {
    fn default() -> Self {
        Self {
            pending: None,
            next_poll: Instant::now(),
            seen: Vec::new(),
            announcements: std::collections::VecDeque::new(),
            membership_pending: None,
        }
    }
}

impl<'a> InviteWatcher<'a> {
    pub(crate) fn invalidate(&mut self) {
        self.pending = None;
        self.next_poll = Instant::now();
        self.seen.clear();
        self.announcements.clear();
        self.membership_pending = None;
    }

    /// Cancel a snapshot started before an Online menu request. A later poll
    /// may then report membership only after the menu action has completed.
    pub(crate) fn invalidate_poll(&mut self) {
        self.pending = None;
        self.membership_pending = None;
        self.next_poll = Instant::now();
    }

    pub(crate) fn next_poll(&self) -> Instant {
        self.next_poll
    }
    pub(crate) fn is_idle(&self) -> bool {
        self.pending.is_none() && self.announcements.is_empty() && self.membership_pending.is_none()
    }

    pub(crate) fn prepare<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
    ) {
        if self.pending.is_some()
            || !self.announcements.is_empty()
            || self.membership_pending.is_some()
            || Instant::now() < self.next_poll
        {
            return;
        }
        self.next_poll = Instant::now() + Duration::from_secs(4);
        self.pending = Some(Box::pin(async move {
            tokio::time::timeout(Duration::from_secs(3), load_snapshot(api, token, fence))
                .await
                .unwrap_or(Err(OnlineError::Unavailable))
        }));
    }

    pub(crate) async fn next(&mut self) -> Result<Option<InviteWatcherEvent>, OnlineError> {
        if let Some(grouped) = self.membership_pending.take() {
            return Ok(Some(InviteWatcherEvent::GroupStateChanged(grouped)));
        }
        if let Some(username) = self.announcements.pop_front() {
            return Ok(Some(InviteWatcherEvent::InviteReceived(username)));
        }
        let result = match &mut self.pending {
            Some(pending) => pending.await,
            None => std::future::pending().await,
        };
        self.pending = None;
        self.observe(result?, crate::session::now_millis());
        Ok(self
            .membership_pending
            .take()
            .map(InviteWatcherEvent::GroupStateChanged))
    }

    fn observe(&mut self, snapshot: OnlineSnapshotResponse, now: u64) {
        // Emit on every successful poll so a ROM reset within the same lease
        // can recover the authoritative membership without opening Online.
        self.membership_pending = Some(snapshot.group.is_some());
        self.seen.retain(|(_, expires)| *expires > now);
        for entry in snapshot.incoming {
            let invite = entry.invitation;
            // The server already omits expired invitations. Comparing its
            // expiry against the local PC clock would hide valid invitations
            // when the two clocks disagree.
            if self.seen.iter().any(|(id, _)| *id == invite.invitation_id) {
                continue;
            }
            // The control pipe can accept a notice while the sidecar is
            // quiescing or waiting for the ROM. Reannounce a still-pending
            // invitation so that a dropped notice does not hide it until
            // the invitation expires.
            self.seen
                .push((invite.invitation_id, now.saturating_add(12_000)));
            self.announcements
                .push_back(entry.username.as_str().to_owned());
        }
    }
}

async fn load_snapshot<A: CloudApi>(
    api: &A,
    token: AccessToken,
    fence: LeaseFence,
) -> Result<OnlineSnapshotResponse, OnlineError> {
    let mut snapshot = api
        .online_snapshot(
            token.clone(),
            OnlineSnapshotRequest {
                api_version: ApiVersion::V1,
                fence,
                incoming_after: None,
            },
        )
        .await?;
    let mut cursor = snapshot.incoming_next;
    while let Some(after) = cursor {
        if snapshot.incoming.len() >= 32 {
            break;
        }
        let page = api
            .online_snapshot(
                token.clone(),
                OnlineSnapshotRequest {
                    api_version: ApiVersion::V1,
                    fence,
                    incoming_after: Some(after),
                },
            )
            .await?;
        if (page.incoming.is_empty() && page.incoming_next.is_some())
            || page
                .incoming
                .iter()
                .any(|entry| entry.invitation.invitation_id <= after)
            || page.incoming_next.is_some_and(|next| next <= after)
        {
            return Err(OnlineError::InvalidResponse);
        }
        cursor = page.incoming_next;
        snapshot.incoming.extend(page.incoming);
    }
    snapshot.incoming.truncate(32);
    snapshot.incoming_next = None;
    Ok(snapshot)
}

fn action_response_matches(action: &CloudAction, response: &OnlineActionResponse) -> bool {
    matches!(
        (action, response),
        (
            CloudAction::Invite { .. },
            OnlineActionResponse::Invited { .. }
        ) | (
            CloudAction::Accept { .. },
            OnlineActionResponse::Accepted { .. }
        ) | (CloudAction::Decline { .. }, OnlineActionResponse::Declined)
            | (CloudAction::Leave { .. }, OnlineActionResponse::Left)
            | (CloudAction::Cancel { .. }, OnlineActionResponse::Cancelled)
            | (
                CloudAction::InviteLastPartner,
                OnlineActionResponse::Invited { .. }
            )
    )
}

pub(crate) fn empty_status(request_id: u32, result: OnlineResult) -> OnlineStatus {
    OnlineStatus {
        request_id,
        result,
        flags: 0,
        nearby_count: 0,
        incoming_count: 0,
        nearby_page: 0,
        incoming_page: 0,
        outgoing_count: 0,
        outgoing_page: 0,
        location_map_group: 0,
        location_map_number: 0,
        nearby_name: String::new(),
        incoming_name: String::new(),
        group_name: String::new(),
        last_partner_name: String::new(),
    }
}

fn snapshot_status(
    request: OnlineRequest,
    snapshot: &OnlineSnapshotResponse,
    result: OnlineResult,
    character_id: coop_cloud::CharacterId,
) -> OnlineStatus {
    let mut status = empty_status(request.request_id, result);
    status.nearby_count = u8::try_from(snapshot.nearby.len()).unwrap_or(32).min(32);
    status.incoming_count = u8::try_from(snapshot.incoming.len()).unwrap_or(32).min(32);
    status.outgoing_count = u8::try_from(snapshot.outgoing.len()).unwrap_or(4).min(4);
    status.nearby_page = request.page.min(status.nearby_count.saturating_sub(1));
    status.incoming_page = request.page.min(status.incoming_count.saturating_sub(1));
    status.outgoing_page = request.page.min(status.outgoing_count.saturating_sub(1));
    if let Some(peer) = snapshot.nearby.get(usize::from(status.nearby_page)) {
        status.flags |= 2;
        peer.username.as_str().clone_into(&mut status.nearby_name);
    }
    if let Some(invitation) = snapshot.incoming.get(usize::from(status.incoming_page)) {
        status.flags |= 4;
        invitation
            .username
            .as_str()
            .clone_into(&mut status.incoming_name);
    }
    if let Some(group) = &snapshot.group {
        status.flags |= 1;
        group.username.as_str().clone_into(&mut status.group_name);
        let caller_index = group
            .group
            .members
            .iter()
            .position(|member| member.character_id == character_id);
        let partner_index = match caller_index {
            Some(0) => Some(1),
            Some(1) => Some(0),
            _ => None,
        };
        let partner_zone =
            partner_index.and_then(|index| group.group.member_world_zones.get(index));
        if let Some(partner_zone) = partner_zone
            && let Ok(entry) = partner_zone.map_entry()
        {
            status.flags |= 16;
            status.location_map_group = entry.map_group;
            status.location_map_number = entry.map_number;
        }
    }
    if let Some(last_partner) = &snapshot.last_partner {
        status.flags |= 32;
        status.last_partner_name = last_partner.username.as_str().chars().take(16).collect();
    }
    if snapshot.group.is_none() {
        if let Some(invitation) = snapshot.outgoing.get(usize::from(status.outgoing_page)) {
            status.flags |= 8;
            invitation
                .username
                .as_str()
                .clone_into(&mut status.group_name);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{CharacterId, ClientInstanceId, Revision, SessionEpoch, SessionId};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use uuid::Uuid;

    fn fence() -> LeaseFence {
        LeaseFence::new(
            SessionId::new(Uuid::from_u128(1)).unwrap(),
            CharacterId::new(Uuid::from_u128(2)).unwrap(),
            Revision::new(0),
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(3)).unwrap(),
        )
    }
    fn snapshot() -> OnlineSnapshotResponse {
        OnlineSnapshotResponse {
            api_version: ApiVersion::V1,
            nearby: vec![coop_cloud::OnlinePeer {
                handle: coop_protocol::PresenceHandle::new(7).unwrap(),
                generation: std::num::NonZeroU64::new(11).unwrap(),
                username: coop_protocol::CanonicalUsername::new("brendan").unwrap(),
            }],
            incoming: vec![],
            outgoing: vec![],
            incoming_next: None,
            group: None,
            last_partner: None,
        }
    }
    #[test]
    fn invite_watcher_trusts_server_expiry_despite_local_clock_skew() {
        let mut watcher = InviteWatcher::default();
        let mut response = serde_json::to_value(snapshot()).unwrap();
        response["incoming"] = json!([
            {"invitation":{"api_version":1,"invitation_id":Uuid::from_u128(10),
                "inviter_character_id":Uuid::from_u128(3),"invitee_character_id":fence().character_id,
                "expires_at":1000},"username":"may"}
        ]);
        let response: OnlineSnapshotResponse = serde_json::from_value(response).unwrap();
        watcher.observe(response.clone(), 2000);
        assert_eq!(watcher.announcements.pop_front(), Some("may".to_owned()));
        watcher.observe(response.clone(), 2000);
        watcher.observe(response, 3000);
        assert!(watcher.announcements.is_empty());
        let mut response = snapshot();
        response.incoming = vec![serde_json::from_value(json!({
            "invitation":{"api_version":1,"invitation_id":Uuid::from_u128(10),
                "inviter_character_id":Uuid::from_u128(3),"invitee_character_id":fence().character_id,
                "expires_at":1000},"username":"may"
        })).unwrap()];
        watcher.observe(response, 14_001);
        assert_eq!(watcher.announcements.pop_front(), Some("may".to_owned()));
    }

    #[test]
    fn invite_watcher_queues_entire_inbox_and_tracks_every_id() {
        let mut watcher = InviteWatcher::default();
        let mut response = snapshot();
        response.incoming = (1..=33_u128)
            .map(|number| {
                serde_json::from_value(json!({
                    "invitation": {
                        "api_version": 1,
                        "invitation_id": Uuid::from_u128(number),
                        "inviter_character_id": Uuid::from_u128(100 + number),
                        "invitee_character_id": fence().character_id,
                        "expires_at": 1000
                    },
                    "username": "may"
                }))
                .unwrap()
            })
            .collect();
        watcher.observe(response.clone(), 2000);
        assert_eq!(watcher.announcements.len(), 33);
        assert_eq!(watcher.seen.len(), 33);
        watcher.announcements.clear();
        watcher.observe(response, 2001);
        assert!(watcher.announcements.is_empty());
    }

    #[tokio::test]
    async fn invite_watcher_reports_membership_and_cancels_pre_menu_snapshot() {
        let mut watcher = InviteWatcher::default();
        watcher.observe(snapshot(), 2000);
        assert!(matches!(
            watcher.next().await.unwrap(),
            Some(InviteWatcherEvent::GroupStateChanged(false))
        ));

        // A menu request discards a prior poll's membership before the menu
        // response can set a newer state in the ROM.
        watcher.observe(snapshot(), 2001);
        watcher.invalidate_poll();
        assert!(watcher.membership_pending.is_none());
        assert!(watcher.is_idle());

        let mut response = serde_json::to_value(snapshot()).unwrap();
        response["group"] = json!({
            "username":"may",
            "group":{
                "api_version":1,"group_id":Uuid::from_u128(20),
                "members":[
                    {"character_id":Uuid::from_u128(2),"world_revision":1},
                    {"character_id":Uuid::from_u128(3),"world_revision":1}
                ],
                "member_world_zones":[
                    {"region":"HOENN","map":"ROUTE101","channel":0},
                    {"region":"HOENN","map":"ROUTE101","channel":0}
                ],
                "world_zone":{"region":"HOENN","map":"ROUTE101","channel":0}
            }
        });
        watcher.observe(serde_json::from_value(response).unwrap(), 2002);
        assert!(matches!(
            watcher.next().await.unwrap(),
            Some(InviteWatcherEvent::GroupStateChanged(true))
        ));
    }

    #[test]
    fn group_change_cancels_older_online_snapshot_with_menu_reply() {
        let mut owner = OnlineOwner::default();
        owner.latest_request = 9;
        owner.pending = Some(Box::pin(std::future::pending()));
        let status = owner.invalidate_for_group_change().unwrap();
        assert_eq!(status.request_id, 9);
        assert_eq!(status.result, OnlineResult::Unavailable);
        assert!(!owner.is_pending());
        assert!(owner.invalidate_for_group_change().is_none());
    }

    #[test]
    fn sender_cancel_uses_only_the_displayed_outgoing_invitation() {
        let mut response = serde_json::to_value(snapshot()).unwrap();
        response["outgoing"] = json!([{"invitation":{
            "api_version":1,"invitation_id":Uuid::from_u128(44),
            "inviter_character_id":fence().character_id,
            "invitee_character_id":Uuid::from_u128(45),
            "expires_at":9_999_999_999_999_u64},"username":"may"}]);
        let snapshot: OnlineSnapshotResponse = serde_json::from_value(response).unwrap();
        let status = snapshot_status(
            request(1, 0, OnlineAction::Refresh),
            &snapshot,
            OnlineResult::Ready,
            fence().character_id,
        );
        assert_eq!(status.outgoing_count, 1);
        assert_eq!(status.group_name, "may");
        let owner = OnlineOwner {
            view: Some(View {
                id: 1,
                fence: fence(),
                generation: 1,
                snapshot,
            }),
            ..OnlineOwner::default()
        };
        assert!(
            matches!(owner.selected_action(request(2, 1, OnlineAction::Cancel), fence(), 1),
            Ok(Some(CloudAction::Cancel { invitation_id })) if invitation_id.as_uuid() == Uuid::from_u128(44))
        );
        assert!(
            owner
                .selected_action(request(2, 0, OnlineAction::Cancel), fence(), 1)
                .is_err()
        );
        assert!(
            owner
                .selected_action(request(2, 1, OnlineAction::Cancel), fence(), 2)
                .is_err()
        );
    }

    #[test]
    fn grouped_status_maps_catalogued_zone_to_rom_coordinates() {
        let mut response = serde_json::to_value(snapshot()).unwrap();
        response["group"] = json!({
            "username":"may",
            "group":{
                "api_version":1,"group_id":Uuid::from_u128(20),
                "members":[
                    {"character_id":Uuid::from_u128(2),"world_revision":1},
                    {"character_id":Uuid::from_u128(3),"world_revision":1}
                ],
                "member_world_zones":[
                    {"region":"HOENN","map":"ROUTE101","channel":0},
                    {"region":"HOENN","map":"SLATEPORT_CITY","channel":0}
                ],
                "world_zone":{"region":"HOENN","map":"ROUTE101","channel":0}
            }
        });
        let snapshot: OnlineSnapshotResponse = serde_json::from_value(response).unwrap();
        let status = snapshot_status(
            request(1, 0, OnlineAction::Refresh),
            &snapshot,
            OnlineResult::Ready,
            fence().character_id,
        );
        assert_eq!(status.flags & 17, 17);
        assert_eq!(
            (status.location_map_group, status.location_map_number),
            (0, 1)
        );
        let outsider = CharacterId::new(Uuid::from_u128(99)).unwrap();
        let outsider_status = snapshot_status(
            request(2, 0, OnlineAction::Refresh),
            &snapshot,
            OnlineResult::Ready,
            outsider,
        );
        assert_eq!(outsider_status.flags & 16, 0);
        assert_eq!(
            OnlineStatus::decode(&status.encode().unwrap()).unwrap(),
            status
        );
    }
    fn request(id: u32, view_id: u32, action: OnlineAction) -> OnlineRequest {
        OnlineRequest {
            request_id: id,
            view_id,
            action,
            page: 0,
        }
    }

    async fn server(
        responses: Vec<Option<(u16, Value)>>,
    ) -> (
        ReqwestCloudApi,
        Arc<Mutex<Vec<Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let api =
            ReqwestCloudApi::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).await.unwrap();
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break bytes.len();
                    }
                    assert!(bytes.len() < 8192);
                };
                let header = String::from_utf8(bytes.clone()).unwrap();
                assert!(
                    header
                        .to_ascii_lowercase()
                        .contains("authorization: bearer online-test")
                );
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .map(str::to_owned)
                    })
                    .unwrap()
                    .parse()
                    .unwrap();
                bytes.resize(header_end + length, 0);
                socket.read_exact(&mut bytes[header_end..]).await.unwrap();
                observed
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&bytes[header_end..]).unwrap());
                if let Some((status, body)) = response {
                    let body = body.to_string();
                    socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
            }
        });
        (api, requests, task)
    }

    #[tokio::test]
    async fn online_owner_pins_identity_and_retries_lost_action_with_same_key() {
        let invitation = json!({"api_version":1,"invitation_id":Uuid::from_u128(9), "inviter_character_id":fence().character_id,
            "invitee_character_id":Uuid::from_u128(10),"expires_at":9_999_999_999_999_u64});
        let (api, requests, task) = server(vec![
            Some((200, serde_json::to_value(snapshot()).unwrap())),
            None,
            Some((200, json!({"result":"invited","invitation":invitation}))),
            Some((200, serde_json::to_value(snapshot()).unwrap())),
        ])
        .await;
        let token = AccessToken::new("online-test").unwrap();
        let mut owner = OnlineOwner::default();
        assert!(
            owner
                .start(
                    &api,
                    token.clone(),
                    fence(),
                    1,
                    request(1, 0, OnlineAction::Refresh)
                )
                .is_none()
        );
        let completion = owner.next().await;
        assert_eq!(
            owner
                .finish(completion, fence(), 1)
                .unwrap()
                .unwrap()
                .nearby_name,
            "brendan"
        );
        assert!(
            owner
                .start(
                    &api,
                    token.clone(),
                    fence(),
                    1,
                    request(2, 1, OnlineAction::Invite)
                )
                .is_none()
        );
        let completion = owner.next().await;
        assert_eq!(
            owner
                .finish(completion, fence(), 1)
                .unwrap()
                .unwrap()
                .result,
            OnlineResult::Success
        );
        assert_eq!(
            owner
                .start(&api, token, fence(), 1, request(3, 1, OnlineAction::Invite))
                .unwrap()
                .result,
            OnlineResult::Stale
        );
        task.await.unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests[1], requests[2],
            "retry preserves key, fence, handle and generation"
        );
        assert_eq!(
            requests[1]["action"],
            json!({"operation":"invite","handle":"0000000000000007","generation":11})
        );
    }

    #[tokio::test]
    async fn reopened_view_discards_pending_success_and_refresh_recovers() {
        let (api, requests, task) = server(vec![
            Some((200, serde_json::to_value(snapshot()).unwrap())),
            Some((200, serde_json::to_value(snapshot()).unwrap())),
        ])
        .await;
        let token = AccessToken::new("online-test").unwrap();
        let mut owner = OnlineOwner::default();
        assert!(
            owner
                .start(
                    &api,
                    token.clone(),
                    fence(),
                    1,
                    request(1, 0, OnlineAction::Refresh)
                )
                .is_none()
        );
        let reopened = owner
            .start(
                &api,
                token.clone(),
                fence(),
                1,
                request(2, 0, OnlineAction::Refresh),
            )
            .unwrap();
        assert_eq!(reopened.request_id, 2);
        assert_eq!(reopened.result, OnlineResult::Unavailable);
        let old = owner.next().await;
        assert!(owner.finish(old, fence(), 1).unwrap().is_none());
        assert!(owner.view.is_none());
        assert!(
            owner
                .start(
                    &api,
                    token,
                    fence(),
                    1,
                    request(3, 0, OnlineAction::Refresh)
                )
                .is_none()
        );
        let fresh = owner.next().await;
        let status = owner.finish(fresh, fence(), 1).unwrap().unwrap();
        assert_eq!(status.request_id, 3);
        assert_eq!(status.result, OnlineResult::Ready);
        task.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn online_completion_cannot_cross_revision_or_presence_generation() {
        for generation_changed in [false, true] {
            for result in [Ok(snapshot()), Err(OnlineError::Unauthorized)] {
                let mut owner = OnlineOwner {
                    latest_request: 1,
                    ..OnlineOwner::default()
                };
                let mut current = fence();
                if !generation_changed {
                    current.current_revision = Revision::new(1);
                }
                let completion = OnlineCompletion {
                    request: request(1, 0, OnlineAction::Refresh),
                    fence: fence(),
                    generation: 1,
                    result,
                };
                assert!(
                    owner
                        .finish(completion, current, if generation_changed { 2 } else { 1 })
                        .unwrap()
                        .is_none()
                );
                assert!(owner.view.is_none());
            }
        }
    }

    #[tokio::test]
    async fn online_pagination_accepts_expired_terminal_page_but_rejects_empty_continuation() {
        for continues in [false, true] {
            let mut first = serde_json::to_value(snapshot()).unwrap();
            first["incoming"] = json!([{"invitation":{"api_version":1,"invitation_id":Uuid::from_u128(9),
                "inviter_character_id":Uuid::from_u128(10),"invitee_character_id":fence().character_id,
                "expires_at":9_999_999_999_999_u64},"username":"may"}]);
            first["incoming_next"] = json!(Uuid::from_u128(9));
            let mut terminal = serde_json::to_value(snapshot()).unwrap();
            if continues {
                terminal["incoming_next"] = json!(Uuid::from_u128(10));
            }
            let (api, requests, task) =
                server(vec![Some((200, first)), Some((200, terminal))]).await;
            let result =
                load_snapshot(&api, AccessToken::new("online-test").unwrap(), fence()).await;
            if continues {
                assert_eq!(result.unwrap_err(), OnlineError::InvalidResponse);
            } else {
                let result = result.unwrap();
                assert_eq!(result.incoming.len(), 1);
                assert!(result.incoming_next.is_none());
            }
            task.await.unwrap();
            assert_eq!(
                requests.lock().unwrap()[1]["incoming_after"],
                json!(Uuid::from_u128(9))
            );
        }
    }

    #[tokio::test]
    async fn online_pagination_stops_at_thirty_two_invitations() {
        let pages = (0_u128..8).map(|page| {
            let mut body = serde_json::to_value(snapshot()).unwrap();
            let entries: Vec<_> = (1..=4).map(|offset| json!({"invitation":{
                "api_version":1,"invitation_id":Uuid::from_u128(page * 4 + offset),
                "inviter_character_id":Uuid::from_u128(100),"invitee_character_id":fence().character_id,
                "expires_at":9_999_999_999_999_u64},"username":"may"})).collect();
            body["incoming"] = json!(entries);
            body["incoming_next"] = json!(Uuid::from_u128(page * 4 + 4));
            Some((200,body))
        }).collect();
        let (api, requests, task) = server(pages).await;
        let result = load_snapshot(&api, AccessToken::new("online-test").unwrap(), fence())
            .await
            .unwrap();
        assert_eq!(result.incoming.len(), 32);
        assert!(result.incoming_next.is_none());
        task.await.unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 8);
        for (index, request) in requests.iter().enumerate().skip(1) {
            assert_eq!(
                request["incoming_after"],
                json!(Uuid::from_u128(index as u128 * 4))
            );
        }
    }

    #[tokio::test]
    async fn online_server_failure_does_not_require_json() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let api =
            ReqwestCloudApi::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            for body in ["", "<html>Service unavailable</html>"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                }
                let header = String::from_utf8(request).unwrap();
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap();
                socket.read_exact(&mut vec![0; length]).await.unwrap();
                socket.write_all(format!("HTTP/1.1 503 Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        for _ in 0..2 {
            assert_eq!(
                api.online_snapshot(
                    AccessToken::new("online-test").unwrap(),
                    OnlineSnapshotRequest {
                        api_version: ApiVersion::V1,
                        fence: fence(),
                        incoming_after: None,
                    }
                )
                .await
                .unwrap_err(),
                OnlineError::Unavailable
            );
        }
        task.await.unwrap();
    }

    #[tokio::test]
    async fn online_http_distinguishes_invitation_expiry_from_failed_fence() {
        let (api, _, task) = server(vec![
            Some((401, json!({"error":{"code":"expired"}}))),
            Some((401, json!({"error":{"code":"authentication_failed"}}))),
        ])
        .await;
        for expected in [OnlineError::Stale, OnlineError::Unauthorized] {
            assert_eq!(
                api.online_snapshot(
                    AccessToken::new("online-test").unwrap(),
                    OnlineSnapshotRequest {
                        api_version: ApiVersion::V1,
                        fence: fence(),
                        incoming_after: None,
                    }
                )
                .await
                .unwrap_err(),
                expected
            );
        }
        task.await.unwrap();
    }

    #[test]
    fn online_selection_rejects_old_generation_even_with_reused_handle() {
        let owner = OnlineOwner {
            view: Some(View {
                id: 1,
                fence: fence(),
                generation: 1,
                snapshot: snapshot(),
            }),
            ..OnlineOwner::default()
        };
        assert!(
            owner
                .selected_action(request(2, 1, OnlineAction::Invite), fence(), 2)
                .is_err()
        );
        assert!(
            owner
                .selected_action(
                    OnlineRequest {
                        page: 1,
                        ..request(2, 1, OnlineAction::Invite)
                    },
                    fence(),
                    1
                )
                .is_err()
        );
    }
}

//! Consent-gated two-member travel orchestration and strict HTTP transport.

use std::{future::Future, pin::Pin, time::Duration};

use coop_cloud::{
    AccessToken, ApiVersion, GroupId, GroupTravelAction, GroupTravelActionRequest,
    GroupTravelProposalId, GroupTravelProposalRequest, GroupTravelProposalStatus,
    GroupTravelProposalView, GroupTravelSceneMarkerRequest, GroupTravelSceneReceiptRequest,
    IdempotencyKey, LeaseFence, OnlineSnapshotRequest, SnapshotRecord, StoryTravelRecoveryAction,
    StoryTravelRecoveryActionRequest, StoryTravelRecoveryOutcome,
    StoryTravelRecoveryResolutionView, StoryTravelRecoveryView,
};
use coop_protocol::{
    GroupTravelClientKind, GroupTravelClientRecord, GroupTravelDeparture, GroupTravelEndpoint,
    GroupTravelReason, GroupTravelResult, GroupTravelRoute, GroupTravelServerKind,
    GroupTravelServerRecord,
};
use reqwest::{Method, StatusCode, Url};
use thiserror::Error;

use crate::{CloudApi, HttpClientError, ReqwestCloudApi, SessionError};

const RESPONSE_MAX_BYTES: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const TRANSIENT_POLL_BACKOFF_BASE_MILLIS: u64 = 500;
const TRANSIENT_POLL_BACKOFF_MAX_MILLIS: u64 = 30_000;
const TRANSIENT_POLL_JITTER_MAX_MILLIS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GroupTravelError {
    #[error("group travel is temporarily unavailable")]
    Unavailable,
    #[error("group travel authorization failed")]
    Unauthorized,
    #[error("group travel state is stale")]
    Stale,
    #[error("group travel proposal was not found")]
    NotFound,
    #[error("group travel response is invalid")]
    InvalidResponse,
}

pub type GroupTravelFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, GroupTravelError>> + Send + 'a>>;

fn validate_story_recovery(
    view: StoryTravelRecoveryView,
    character_id: coop_cloud::CharacterId,
) -> Result<StoryTravelRecoveryView, GroupTravelError> {
    if view.api_version != ApiVersion::V1
        || view.marker_fence.character_id != character_id
        || view.scene_nonce == 0
        || !matches!(
            view.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts
                | GroupTravelProposalStatus::Suspended
                | GroupTravelProposalStatus::Cancelled
        )
    {
        return Err(GroupTravelError::InvalidResponse);
    }
    Ok(view)
}

fn validate_story_recovery_resolution(
    view: StoryTravelRecoveryResolutionView,
    proposal_id: GroupTravelProposalId,
    action: StoryTravelRecoveryAction,
) -> Result<StoryTravelRecoveryResolutionView, GroupTravelError> {
    let expected = match action {
        StoryTravelRecoveryAction::Reconcile => StoryTravelRecoveryOutcome::Reconciled,
        StoryTravelRecoveryAction::Abandon => StoryTravelRecoveryOutcome::Abandoned,
    };
    if view.api_version != ApiVersion::V1
        || view.proposal_id != proposal_id
        || view.outcome != expected
    {
        return Err(GroupTravelError::InvalidResponse);
    }
    Ok(view)
}

fn map_http_error(error: &HttpClientError) -> GroupTravelError {
    match error {
        HttpClientError::Status(StatusCode::UNAUTHORIZED) | HttpClientError::SessionClosed => {
            GroupTravelError::Unauthorized
        }
        HttpClientError::Status(
            StatusCode::CONFLICT | StatusCode::GONE | StatusCode::FORBIDDEN,
        ) => GroupTravelError::Stale,
        HttpClientError::Status(StatusCode::NOT_FOUND) => GroupTravelError::NotFound,
        HttpClientError::Transport(_) => GroupTravelError::Unavailable,
        HttpClientError::Status(status) if status.is_server_error() => {
            GroupTravelError::Unavailable
        }
        _ => GroupTravelError::InvalidResponse,
    }
}

impl ReqwestCloudApi {
    fn group_travel_request(
        &self,
        method: Method,
        url: Url,
        token: &AccessToken,
        fence: LeaseFence,
    ) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .bearer_auth(token.expose_secret())
            .header("X-Coop-Session-Id", fence.session_id.to_string())
            .header(
                "X-Coop-Session-Epoch",
                fence.session_epoch.value().to_string(),
            )
            .header(
                "X-Coop-Client-Instance-Id",
                fence.client_instance_id.to_string(),
            )
    }

    async fn send_group_travel(
        &self,
        request: reqwest::RequestBuilder,
        expected: StatusCode,
    ) -> Result<GroupTravelProposalView, GroupTravelError> {
        let response = request
            .send()
            .await
            .map_err(|_| GroupTravelError::Unavailable)?;
        if response.status() != expected {
            return Err(map_http_error(&HttpClientError::Status(response.status())));
        }
        let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
            .await
            .map_err(|error| map_http_error(&error))?;
        serde_json::from_slice(&bytes).map_err(|_| GroupTravelError::InvalidResponse)
    }

    pub(crate) fn group_travel_create_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        request: GroupTravelProposalRequest,
    ) -> GroupTravelFuture<'_, GroupTravelProposalView> {
        Box::pin(async move {
            let url = self
                .url(&format!("v1/groups/{group_id}/travel-proposals"))
                .map_err(|error| map_http_error(&error))?;
            self.send_group_travel(
                self.group_travel_request(Method::POST, url, &token, request.fence())
                    .json(&request),
                StatusCode::CREATED,
            )
            .await
        })
    }

    pub(crate) fn group_travel_current_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        fence: LeaseFence,
    ) -> GroupTravelFuture<'_, Option<GroupTravelProposalView>> {
        Box::pin(async move {
            let url = self
                .url(&format!("v1/groups/{group_id}/travel-proposals/current"))
                .map_err(|error| map_http_error(&error))?;
            let response = self
                .group_travel_request(Method::GET, url, &token, fence)
                .send()
                .await
                .map_err(|_| GroupTravelError::Unavailable)?;
            match response.status() {
                StatusCode::OK => {
                    let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
                        .await
                        .map_err(|error| map_http_error(&error))?;
                    serde_json::from_slice(&bytes)
                        .map(Some)
                        .map_err(|_| GroupTravelError::InvalidResponse)
                }
                StatusCode::NOT_FOUND => Ok(None),
                status => Err(map_http_error(&HttpClientError::Status(status))),
            }
        })
    }

    pub(crate) fn group_travel_get_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        fence: LeaseFence,
    ) -> GroupTravelFuture<'_, GroupTravelProposalView> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/groups/{group_id}/travel-proposals/{proposal_id}"
                ))
                .map_err(|error| map_http_error(&error))?;
            self.send_group_travel(
                self.group_travel_request(Method::GET, url, &token, fence),
                StatusCode::OK,
            )
            .await
        })
    }

    pub(crate) fn group_travel_action_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        request: GroupTravelActionRequest,
    ) -> GroupTravelFuture<'_, GroupTravelProposalView> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/groups/{group_id}/travel-proposals/{proposal_id}/actions"
                ))
                .map_err(|error| map_http_error(&error))?;
            self.send_group_travel(
                self.group_travel_request(Method::POST, url, &token, request.fence())
                    .json(&request),
                StatusCode::OK,
            )
            .await
        })
    }

    pub(crate) fn group_travel_scene_marker_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        request: GroupTravelSceneMarkerRequest,
    ) -> GroupTravelFuture<'_, GroupTravelProposalView> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/groups/{group_id}/travel-proposals/{proposal_id}/scene-markers"
                ))
                .map_err(|error| map_http_error(&error))?;
            self.send_group_travel(
                self.group_travel_request(Method::POST, url, &token, request.fence())
                    .json(&request),
                StatusCode::OK,
            )
            .await
        })
    }

    pub(crate) fn group_travel_scene_receipt_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        request: GroupTravelSceneReceiptRequest,
    ) -> GroupTravelFuture<'_, GroupTravelProposalView> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/groups/{group_id}/travel-proposals/{proposal_id}/scene-receipts"
                ))
                .map_err(|error| map_http_error(&error))?;
            self.send_group_travel(
                self.group_travel_request(Method::POST, url, &token, request.fence())
                    .json(&request),
                StatusCode::OK,
            )
            .await
        })
    }

    pub(crate) fn story_travel_recovery_http(
        &self,
        token: AccessToken,
        fence: LeaseFence,
    ) -> GroupTravelFuture<'_, Option<StoryTravelRecoveryView>> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/characters/{}/story-travel-recovery",
                    fence.character_id
                ))
                .map_err(|error| map_http_error(&error))?;
            let response = self
                .group_travel_request(Method::GET, url, &token, fence)
                .send()
                .await
                .map_err(|_| GroupTravelError::Unavailable)?;
            match response.status() {
                StatusCode::OK => {
                    let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
                        .await
                        .map_err(|error| map_http_error(&error))?;
                    let view = serde_json::from_slice(&bytes)
                        .map_err(|_| GroupTravelError::InvalidResponse)?;
                    validate_story_recovery(view, fence.character_id).map(Some)
                }
                StatusCode::NOT_FOUND => Ok(None),
                status => Err(map_http_error(&HttpClientError::Status(status))),
            }
        })
    }

    pub(crate) fn story_travel_recovery_action_http(
        &self,
        token: AccessToken,
        proposal_id: GroupTravelProposalId,
        fence: LeaseFence,
        action: StoryTravelRecoveryAction,
    ) -> GroupTravelFuture<'_, StoryTravelRecoveryResolutionView> {
        Box::pin(async move {
            let url = self
                .url(&format!(
                    "v1/characters/{}/story-travel-recovery/{proposal_id}/actions",
                    fence.character_id
                ))
                .map_err(|error| map_http_error(&error))?;
            let request = StoryTravelRecoveryActionRequest {
                api_version: ApiVersion::V1,
                action,
            };
            let response = self
                .group_travel_request(Method::POST, url, &token, fence)
                .json(&request)
                .send()
                .await
                .map_err(|_| GroupTravelError::Unavailable)?;
            if response.status() != StatusCode::OK {
                return Err(map_http_error(&HttpClientError::Status(response.status())));
            }
            let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
                .await
                .map_err(|error| map_http_error(&error))?;
            let view =
                serde_json::from_slice(&bytes).map_err(|_| GroupTravelError::InvalidResponse)?;
            validate_story_recovery_resolution(view, proposal_id, action)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Requester,
    Responder,
}
#[derive(Clone, Debug)]
struct PendingCreate {
    group_id: GroupId,
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    endpoint: Option<GroupTravelEndpoint>,
    request_id: u32,
    request: GroupTravelProposalRequest,
    cancel_requested: bool,
}
#[derive(Clone, Copy, Debug)]
struct PendingRequest {
    fence: LeaseFence,
    record: GroupTravelClientRecord,
    cancel_requested: bool,
}
#[derive(Clone, Copy, Debug)]
struct PendingAction {
    action: GroupTravelAction,
    request: GroupTravelActionRequest,
}
#[derive(Clone, Debug)]
struct TrackedProposal {
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    endpoint: Option<GroupTravelEndpoint>,
    request_id: u32,
    role: Role,
    status: GroupTravelProposalStatus,
    vote_deadline: Option<tokio::time::Instant>,
    pending_action: Option<PendingAction>,
    marker_requested: bool,
    pending_marker: bool,
    marker_accepted: bool,
    marker_fence: Option<LeaseFence>,
    scene_complete: bool,
    pending_receipt: Option<GroupTravelSceneReceiptRequest>,
    receipt_accepted: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoryCheckpointToken {
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    marker_fence: LeaseFence,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Outbound {
    record: GroupTravelServerRecord,
    disposition: DeliveryDisposition,
    retry_at: tokio::time::Instant,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeliveryDisposition {
    Retain,
    ClearTracked,
    ReplyOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalReplay {
    generation: u32,
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    endpoint: Option<GroupTravelEndpoint>,
    request_id: u32,
    proposal_id: [u8; 16],
    record: GroupTravelServerRecord,
    disposition: DeliveryDisposition,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingKind {
    Snapshot,
    Current,
    Get,
    Create,
    Action,
    Marker,
    Receipt,
}
struct PendingWork<'a> {
    kind: PendingKind,
    future: Pin<Box<dyn Future<Output = Completion> + Send + 'a>>,
}
#[derive(Debug)]
enum Completion {
    Snapshot {
        generation: u32,
        result: Result<Option<GroupId>, SessionError>,
    },
    Current {
        fence: LeaseFence,
        generation: u32,
        result: Result<Option<GroupTravelProposalView>, GroupTravelError>,
    },
    Get {
        fence: LeaseFence,
        generation: u32,
        proposal_id: GroupTravelProposalId,
        result: Result<GroupTravelProposalView, GroupTravelError>,
    },
    Create {
        fence: LeaseFence,
        generation: u32,
        pending: PendingCreate,
        result: Result<GroupTravelProposalView, GroupTravelError>,
    },
    Action {
        fence: LeaseFence,
        generation: u32,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        pending: PendingAction,
        result: Result<GroupTravelProposalView, GroupTravelError>,
    },
    Marker {
        fence: LeaseFence,
        generation: u32,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        result: Result<GroupTravelProposalView, GroupTravelError>,
    },
    Receipt {
        fence: LeaseFence,
        generation: u32,
        group_id: GroupId,
        proposal_id: GroupTravelProposalId,
        request: GroupTravelSceneReceiptRequest,
        result: Result<GroupTravelProposalView, GroupTravelError>,
    },
}
impl Completion {
    const fn generation(&self) -> u32 {
        match self {
            Self::Snapshot { generation, .. }
            | Self::Current { generation, .. }
            | Self::Get { generation, .. }
            | Self::Create { generation, .. }
            | Self::Action { generation, .. }
            | Self::Marker { generation, .. }
            | Self::Receipt { generation, .. } => *generation,
        }
    }
}
pub(crate) enum GroupTravelOwnerEvent {
    Wake,
    Complete,
    Deliver(GroupTravelServerRecord),
    Error(SessionError),
}
enum Ready {
    Poll,
    Delivery,
    Completion(Box<Completion>),
}
const DELIVERY_RETRY_INTERVAL: Duration = Duration::from_millis(750);
const TERMINAL_REPLAY_LIMIT: usize = 8;

pub(crate) struct GroupTravelOwner<'a> {
    group_id: Option<GroupId>,
    pending_request: Option<PendingRequest>,
    pending_create: Option<PendingCreate>,
    tracked: Option<TrackedProposal>,
    pending: Option<PendingWork<'a>>,
    outbound: Option<Outbound>,
    terminal: std::collections::VecDeque<TerminalReplay>,
    generation: Option<u32>,
    next_poll: tokio::time::Instant,
    transient_poll_failures: u8,
}
impl Default for GroupTravelOwner<'_> {
    fn default() -> Self {
        Self {
            group_id: None,
            pending_request: None,
            pending_create: None,
            tracked: None,
            pending: None,
            outbound: None,
            terminal: std::collections::VecDeque::new(),
            generation: None,
            next_poll: tokio::time::Instant::now() + IDLE_POLL_INTERVAL,
            transient_poll_failures: 0,
        }
    }
}
impl<'a> GroupTravelOwner<'a> {
    /// Capture eligibility before granting a checkpoint. An older finalized
    /// save cannot become story evidence when SceneComplete arrives later.
    pub(crate) fn story_checkpoint_token(&self, fence: LeaseFence) -> Option<StoryCheckpointToken> {
        let tracked = self.tracked.as_ref()?;
        let marker_fence = tracked.marker_fence?;
        (is_story_route(tracked.route)
            && matches!(
                tracked.status,
                GroupTravelProposalStatus::AwaitingSceneReceipts
                    | GroupTravelProposalStatus::Suspended
            )
            && tracked.marker_accepted
            && tracked.scene_complete
            && !tracked.receipt_accepted
            && tracked.pending_receipt.is_none()
            && marker_fence.session_id == fence.session_id
            && marker_fence.session_epoch == fence.session_epoch
            && marker_fence.character_id == fence.character_id
            && marker_fence.current_revision == fence.current_revision)
            .then_some(StoryCheckpointToken {
                group_id: tracked.group_id,
                proposal_id: tracked.proposal_id,
                marker_fence,
            })
    }

    pub(crate) fn story_checkpoint_finalized(
        &mut self,
        token: StoryCheckpointToken,
        snapshot: &SnapshotRecord,
        fence: LeaseFence,
    ) -> Result<(), SessionError> {
        let tracked = self.tracked.as_mut().ok_or(SessionError::Realtime)?;
        if tracked.group_id != token.group_id
            || tracked.proposal_id != token.proposal_id
            || tracked.marker_fence != Some(token.marker_fence)
            || !tracked.marker_accepted
            || !tracked.scene_complete
            || tracked.pending_receipt.is_some()
            || tracked.receipt_accepted
            || snapshot.character_id != fence.character_id
            || snapshot.session_id != token.marker_fence.session_id
            || snapshot.session_epoch != token.marker_fence.session_epoch
            || snapshot.parent_revision != token.marker_fence.current_revision
            || snapshot.revision != fence.current_revision
        {
            return Err(SessionError::Realtime);
        }
        tracked.pending_receipt = Some(GroupTravelSceneReceiptRequest {
            api_version: ApiVersion::V1,
            session_id: fence.session_id,
            character_id: fence.character_id,
            current_revision: fence.current_revision,
            session_epoch: fence.session_epoch,
            client_instance_id: fence.client_instance_id,
            snapshot_id: snapshot.snapshot_id,
        });
        // A GET prepared before the checkpoint carries the old revision and
        // must not complete ahead of the post-save receipt.
        self.pending = None;
        self.next_poll = tokio::time::Instant::now();
        Ok(())
    }

    fn enter_generation(&mut self, generation: u32) {
        if self.generation == Some(generation) {
            return;
        }
        self.generation = Some(generation);
        self.transient_poll_failures = 0;
        self.terminal.retain(|replay| replay.proposal_id != [0; 16]);
        let preserve_delivery = self.outbound.is_some_and(|outbound| {
            outbound.disposition == DeliveryDisposition::ClearTracked
                && outbound.record.proposal_id != [0; 16]
        });
        if preserve_delivery {
            if let Some(outbound) = &mut self.outbound {
                outbound.retry_at = tokio::time::Instant::now();
            }
        } else {
            self.outbound = None;
            self.queue_for_tracked_state();
        }
    }
    pub(crate) fn reset_poll(&mut self) {
        self.transient_poll_failures = 0;
        self.next_poll = tokio::time::Instant::now();
        self.queue_for_tracked_state();
    }

    fn schedule_poll_success(&mut self, delay: Duration) {
        self.transient_poll_failures = 0;
        self.next_poll = tokio::time::Instant::now() + delay;
    }

    fn schedule_transient_poll_failure(&mut self) {
        self.transient_poll_failures = self.transient_poll_failures.saturating_add(1);
        self.next_poll =
            tokio::time::Instant::now() + transient_poll_backoff(self.transient_poll_failures);
    }
    pub(crate) fn prepare<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
    ) {
        self.enter_generation(generation);
        if self.pending.is_some() {
            return;
        }
        if tokio::time::Instant::now() < self.next_poll {
            return;
        }
        if self.pending_request.is_some() && self.group_id.is_none() {
            self.pending = Some(PendingWork {
                kind: PendingKind::Snapshot,
                future: Box::pin(async move {
                    Completion::Snapshot {
                        generation,
                        result: active_group(api, token, fence).await,
                    }
                }),
            });
            return;
        }
        if let Some(operation) = self.pending_create.clone() {
            let pending = operation.clone();
            self.pending = Some(PendingWork {
                kind: PendingKind::Create,
                future: Box::pin(async move {
                    let result =
                        retry_create(api, token, operation.group_id, operation.request.clone())
                            .await;
                    Completion::Create {
                        fence,
                        generation,
                        pending,
                        result,
                    }
                }),
            });
            return;
        }
        if let Some(tracked) = &self.tracked
            && tracked.pending_marker
        {
            let group_id = tracked.group_id;
            let proposal_id = tracked.proposal_id;
            let request = GroupTravelSceneMarkerRequest {
                api_version: ApiVersion::V1,
                session_id: fence.session_id,
                character_id: fence.character_id,
                current_revision: fence.current_revision,
                session_epoch: fence.session_epoch,
                client_instance_id: fence.client_instance_id,
                scene_nonce: tracked.request_id,
            };
            self.pending = Some(PendingWork {
                kind: PendingKind::Marker,
                future: Box::pin(async move {
                    Completion::Marker {
                        fence,
                        generation,
                        group_id,
                        proposal_id,
                        result: retry_scene_marker(api, token, group_id, proposal_id, request)
                            .await,
                    }
                }),
            });
            return;
        }
        if let Some(tracked) = &self.tracked
            && let Some(request) = tracked.pending_receipt.clone()
        {
            let group_id = tracked.group_id;
            let proposal_id = tracked.proposal_id;
            self.pending = Some(PendingWork {
                kind: PendingKind::Receipt,
                future: Box::pin(async move {
                    let result =
                        retry_scene_receipt(api, token, group_id, proposal_id, request.clone())
                            .await;
                    Completion::Receipt {
                        fence,
                        generation,
                        group_id,
                        proposal_id,
                        request,
                        result,
                    }
                }),
            });
            return;
        }
        if let Some(tracked) = &self.tracked
            && let Some(action) = tracked.pending_action
        {
            let group_id = tracked.group_id;
            let proposal_id = tracked.proposal_id;
            self.pending = Some(PendingWork {
                kind: PendingKind::Action,
                future: Box::pin(async move {
                    let result =
                        retry_action(api, token, group_id, proposal_id, action.request).await;
                    Completion::Action {
                        fence,
                        generation,
                        group_id,
                        proposal_id,
                        pending: action,
                        result,
                    }
                }),
            });
            return;
        }
        if let Some(tracked) = &self.tracked {
            let group_id = tracked.group_id;
            let proposal_id = tracked.proposal_id;
            self.pending = Some(PendingWork {
                kind: PendingKind::Get,
                future: Box::pin(async move {
                    Completion::Get {
                        fence,
                        generation,
                        proposal_id,
                        result: retry_get(api, token, group_id, proposal_id, fence).await,
                    }
                }),
            });
        } else if let Some(group_id) = self.group_id {
            self.pending = Some(PendingWork {
                kind: PendingKind::Current,
                future: Box::pin(async move {
                    Completion::Current {
                        fence,
                        generation,
                        result: retry_current(api, token, group_id, fence).await,
                    }
                }),
            });
        } else {
            self.pending = Some(PendingWork {
                kind: PendingKind::Snapshot,
                future: Box::pin(async move {
                    Completion::Snapshot {
                        generation,
                        result: active_group(api, token, fence).await,
                    }
                }),
            });
        }
    }
    pub(crate) async fn next_event(&mut self) -> GroupTravelOwnerEvent {
        if let Some(outbound) = self.outbound
            && outbound.retry_at <= tokio::time::Instant::now()
        {
            return GroupTravelOwnerEvent::Deliver(outbound.record);
        }
        let poll_at = self.next_poll;
        let delivery_at = self.outbound.map(|v| v.retry_at);
        let pending = &mut self.pending;
        let ready = tokio::select! {
            biased;
            ()=async{tokio::time::sleep_until(delivery_at.expect("guarded")).await},if delivery_at.is_some()=>Ready::Delivery,
            c=async{pending.as_mut().expect("guarded").future.as_mut().await},if pending.is_some()=>Ready::Completion(Box::new(c)),
            ()=tokio::time::sleep_until(poll_at)=>Ready::Poll
        };
        match ready {
            Ready::Poll => GroupTravelOwnerEvent::Wake,
            Ready::Delivery => {
                GroupTravelOwnerEvent::Deliver(self.outbound.expect("present").record)
            }
            Ready::Completion(c) => {
                self.pending = None;
                match self.finish(*c) {
                    Ok(()) => GroupTravelOwnerEvent::Complete,
                    Err(error) => GroupTravelOwnerEvent::Error(error),
                }
            }
        }
    }
    pub(crate) fn acknowledge_delivery(
        &mut self,
        record: GroupTravelServerRecord,
    ) -> Result<(), SessionError> {
        let out = self.outbound.ok_or(SessionError::Realtime)?;
        if out.record != record {
            return Err(SessionError::Realtime);
        }
        match out.disposition {
            DeliveryDisposition::Retain => {
                self.outbound = Some(Outbound {
                    retry_at: tokio::time::Instant::now() + DELIVERY_RETRY_INTERVAL,
                    ..out
                });
            }
            DeliveryDisposition::ClearTracked | DeliveryDisposition::ReplyOnly => {
                self.remember_terminal(out);
                self.outbound = None;
                if out.disposition == DeliveryDisposition::ClearTracked {
                    self.tracked = None;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn handle(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        self.enter_generation(generation);
        record.encode().map_err(|_| SessionError::Realtime)?;
        if let Some((replay, disposition)) = self.find_terminal(generation, record) {
            self.queue_outbound(replay, disposition);
            return Ok(());
        }
        match record.kind {
            GroupTravelClientKind::Request => self.handle_request(fence, record),
            GroupTravelClientKind::Decision => self.handle_decision(fence, record),
            GroupTravelClientKind::Cancel => self.handle_cancel(fence, record),
            GroupTravelClientKind::Applied => self.handle_applied(fence, record),
            GroupTravelClientKind::SceneMarkerRequest => self.handle_scene_marker(record),
            GroupTravelClientKind::SceneComplete => self.handle_scene_complete(fence, record),
        }
    }
    fn handle_request(
        &mut self,
        fence: LeaseFence,
        r: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        if let Some(pending) = self.pending_request {
            if pending.record.route == r.route
                && pending.record.departure == r.departure
                && pending.record.request_id == r.request_id
                && pending.record.endpoint == r.endpoint
            {
                self.queue_outbound(
                    requesting_record(r.route, r.departure, r.request_id, r.endpoint),
                    DeliveryDisposition::Retain,
                );
            } else {
                self.queue_outbound(
                    abort_before_create(r, GroupTravelReason::Conflict),
                    DeliveryDisposition::ReplyOnly,
                );
            }
            return Ok(());
        }
        if let Some(p) = &self.pending_create {
            if p.route == r.route
                && p.departure == r.departure
                && p.request_id == r.request_id
                && p.endpoint == r.endpoint
            {
                self.queue_outbound(
                    requesting_record(r.route, r.departure, r.request_id, r.endpoint),
                    DeliveryDisposition::Retain,
                );
            } else {
                self.queue_outbound(
                    abort_before_create(r, GroupTravelReason::Conflict),
                    DeliveryDisposition::ReplyOnly,
                );
            }
            return Ok(());
        }
        if let Some(t) = &self.tracked {
            if t.role == Role::Requester
                && t.route == r.route
                && t.departure == r.departure
                && t.request_id == r.request_id
                && t.endpoint == r.endpoint
            {
                self.queue_for_tracked_state();
            } else {
                self.queue_outbound(
                    abort_before_create(r, GroupTravelReason::Conflict),
                    DeliveryDisposition::ReplyOnly,
                );
            }
            return Ok(());
        }
        let Some(group_id) = self.group_id else {
            self.pending_request = Some(PendingRequest {
                fence,
                record: r,
                cancel_requested: false,
            });
            self.queue_outbound(
                requesting_record(r.route, r.departure, r.request_id, r.endpoint),
                DeliveryDisposition::Retain,
            );
            self.next_poll = tokio::time::Instant::now();
            return Ok(());
        };
        let request = GroupTravelProposalRequest::new_with_departure_and_endpoint(
            fence,
            route_id(r.route),
            r.departure,
            r.endpoint,
            new_idempotency_key()?,
        )
        .map_err(|_| SessionError::Realtime)?;
        self.pending_create = Some(PendingCreate {
            group_id,
            route: r.route,
            departure: r.departure,
            endpoint: r.endpoint,
            request_id: r.request_id,
            request,
            cancel_requested: false,
        });
        self.next_poll = tokio::time::Instant::now();
        self.queue_outbound(
            requesting_record(r.route, r.departure, r.request_id, r.endpoint),
            DeliveryDisposition::Retain,
        );
        Ok(())
    }
    fn handle_decision(
        &mut self,
        fence: LeaseFence,
        r: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        let Some(t) = self.tracked.as_ref() else {
            self.next_poll = tokio::time::Instant::now();
            return Ok(());
        };
        if !record_matches(t, r, false) || t.role != Role::Responder {
            return Err(SessionError::Realtime);
        }
        let action = match r.result {
            GroupTravelResult::Accepted => GroupTravelAction::Accept,
            GroupTravelResult::Declined => GroupTravelAction::Decline,
            _ => return Err(SessionError::Realtime),
        };
        if t.pending_action.is_some_and(|p| p.action == action) {
            return Ok(());
        }
        if t.status == GroupTravelProposalStatus::Committed && action == GroupTravelAction::Accept {
            self.queue_for_tracked_state();
            return Ok(());
        }
        self.schedule_action(fence, action)
    }
    fn handle_cancel(
        &mut self,
        fence: LeaseFence,
        r: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        if let Some(pending) = &mut self.pending_request
            && pending.record.route == r.route
            && pending.record.departure == r.departure
            && pending.record.request_id == r.request_id
            && pending.record.endpoint == r.endpoint
        {
            pending.cancel_requested = true;
            return Ok(());
        }
        if let Some(p) = &mut self.pending_create
            && p.route == r.route
            && p.departure == r.departure
            && p.request_id == r.request_id
            && p.endpoint == r.endpoint
        {
            p.cancel_requested = true;
            return Ok(());
        }
        let Some(t) = self.tracked.as_ref() else {
            self.next_poll = tokio::time::Instant::now();
            return Ok(());
        };
        if t.role != Role::Requester || !record_matches(t, r, true) {
            return Err(SessionError::Realtime);
        }
        if t.pending_action
            .is_some_and(|p| p.action == GroupTravelAction::Cancel)
        {
            return Ok(());
        }
        self.schedule_action(fence, GroupTravelAction::Cancel)
    }
    fn handle_applied(
        &mut self,
        fence: LeaseFence,
        r: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        let Some(t) = self.tracked.as_ref() else {
            self.next_poll = tokio::time::Instant::now();
            return Ok(());
        };
        if !record_matches(t, r, false) {
            return Err(SessionError::Realtime);
        }
        if t.pending_action
            .is_some_and(|p| p.action == GroupTravelAction::Applied)
        {
            return Ok(());
        }
        self.schedule_action(fence, GroupTravelAction::Applied)
    }
    fn handle_scene_marker(&mut self, r: GroupTravelClientRecord) -> Result<(), SessionError> {
        let Some(t) = self.tracked.as_mut() else {
            return Err(SessionError::Realtime);
        };
        if !is_story_route(t.route)
            || t.status != GroupTravelProposalStatus::AwaitingSceneReceipts
            || !record_matches(t, r, false)
        {
            return Err(SessionError::Realtime);
        }
        t.marker_requested = true;
        if t.marker_accepted {
            self.queue_for_tracked_state();
            return Ok(());
        }
        t.pending_marker = true;
        self.next_poll = tokio::time::Instant::now();
        Ok(())
    }
    fn handle_scene_complete(
        &mut self,
        fence: LeaseFence,
        r: GroupTravelClientRecord,
    ) -> Result<(), SessionError> {
        let Some(t) = self.tracked.as_mut() else {
            return Err(SessionError::Realtime);
        };
        if t.scene_complete
            && t.marker_fence.is_some_and(|marker| {
                marker.session_id == fence.session_id
                    && marker.session_epoch == fence.session_epoch
                    && marker.character_id == fence.character_id
            })
            && record_matches(t, r, false)
        {
            return Ok(());
        }
        if !is_story_route(t.route)
            || t.status != GroupTravelProposalStatus::AwaitingSceneReceipts
            || !t.marker_accepted
            || t.marker_fence != Some(fence)
            || !record_matches(t, r, false)
        {
            return Err(SessionError::Realtime);
        }
        t.scene_complete = true;
        Ok(())
    }
    fn schedule_action(
        &mut self,
        fence: LeaseFence,
        action: GroupTravelAction,
    ) -> Result<(), SessionError> {
        let t = self.tracked.as_mut().ok_or(SessionError::Realtime)?;
        t.pending_action = Some(PendingAction {
            action,
            request: GroupTravelActionRequest::new(fence, action, new_idempotency_key()?),
        });
        self.next_poll = tokio::time::Instant::now();
        if matches!(
            self.pending.as_ref().map(|p| p.kind),
            Some(PendingKind::Snapshot | PendingKind::Current | PendingKind::Get)
        ) {
            self.pending = None;
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "completion handling is one state transition table"
    )]
    fn finish(&mut self, c: Completion) -> Result<(), SessionError> {
        let generation_changed = self
            .generation
            .is_some_and(|generation| generation != c.generation());
        match c {
            Completion::Snapshot { result, .. } => match result {
                Ok(id) => {
                    self.group_id = id;
                    if id.is_some()
                        && let Some(request) = self.pending_request.take()
                    {
                        let _ = self.handle_request(request.fence, request.record);
                        if request.cancel_requested {
                            let _ = self.handle_cancel(
                                request.fence,
                                GroupTravelClientRecord {
                                    kind: GroupTravelClientKind::Cancel,
                                    result: GroupTravelResult::None,
                                    reason: GroupTravelReason::RequesterCanceled,
                                    ..request.record
                                },
                            );
                        }
                    } else if id.is_none()
                        && let Some(request) = self.pending_request.take()
                    {
                        self.queue_outbound(
                            abort_before_create(request.record, GroupTravelReason::Unsafe),
                            DeliveryDisposition::ReplyOnly,
                        );
                    }
                    self.schedule_poll_success(if id.is_some() {
                        Duration::ZERO
                    } else {
                        IDLE_POLL_INTERVAL
                    });
                }
                Err(SessionError::Cloud) => {
                    self.schedule_transient_poll_failure();
                }
                Err(error @ (SessionError::Unauthorized | SessionError::Realtime)) => {
                    return Err(error);
                }
                Err(_) => self.schedule_transient_poll_failure(),
            },
            Completion::Current {
                fence,
                generation,
                result,
            } => self.finish_view_result(result, fence, generation, None)?,
            Completion::Get {
                fence,
                generation,
                proposal_id,
                result,
            } => self.finish_view_result(result.map(Some), fence, generation, Some(proposal_id))?,
            Completion::Create {
                fence,
                generation,
                pending,
                result,
            } => match result {
                Ok(view) => {
                    let cancel_requested = pending.cancel_requested
                        || self.pending_create.as_ref().is_some_and(|live| {
                            live.group_id == pending.group_id
                                && live.route == pending.route
                                && live.departure == pending.departure
                                && live.endpoint == pending.endpoint
                                && live.request_id == pending.request_id
                                && live.request.idempotency_key == pending.request.idempotency_key
                                && live.cancel_requested
                        });
                    self.pending_create = None;
                    self.install_created(&view, &pending, fence)?;
                    if cancel_requested {
                        self.schedule_poll_success(Duration::ZERO);
                        self.schedule_action(fence, GroupTravelAction::Cancel)?;
                    } else {
                        self.observe_view(&view, fence, generation)?;
                    }
                }
                Err(GroupTravelError::Unavailable) => {
                    self.schedule_transient_poll_failure();
                }
                Err(GroupTravelError::Unauthorized) => return Err(SessionError::Unauthorized),
                Err(GroupTravelError::InvalidResponse) => return Err(SessionError::Realtime),
                Err(GroupTravelError::Stale | GroupTravelError::NotFound) => {
                    if generation_changed {
                        self.next_poll = tokio::time::Instant::now();
                    } else {
                        self.pending_create = None;
                        self.queue_outbound(
                            abort_before_create(
                                GroupTravelClientRecord {
                                    kind: GroupTravelClientKind::Request,
                                    route: pending.route,
                                    departure: pending.departure,
                                    endpoint: pending.endpoint,
                                    request_id: pending.request_id,
                                    proposal_id: [0; 16],
                                    result: GroupTravelResult::None,
                                    reason: GroupTravelReason::None,
                                },
                                GroupTravelReason::Conflict,
                            ),
                            DeliveryDisposition::ReplyOnly,
                        );
                    }
                }
            },
            Completion::Action {
                fence,
                generation,
                group_id,
                proposal_id,
                pending,
                result,
            } => {
                if let Some(t) = &mut self.tracked
                    && t.group_id == group_id
                    && t.proposal_id == proposal_id
                    && t.pending_action
                        .is_some_and(|p| p.request == pending.request)
                {
                    match result {
                        Ok(view) => {
                            self.transient_poll_failures = 0;
                            t.pending_action = None;
                            if pending.action == GroupTravelAction::Applied
                                && validate_view(&view, group_id, fence).is_ok()
                                && view.proposal_id == proposal_id
                                && route_from_id(view.route_id.as_str()) == Some(t.route)
                                && ((t.role == Role::Requester
                                    && view.requester_character_id == fence.character_id)
                                    || (t.role == Role::Responder
                                        && view.responder_character_id == fence.character_id))
                                && view.status == GroupTravelProposalStatus::Committed
                            {
                                t.status = view.status;
                                let complete = server_record(
                                    GroupTravelServerKind::Complete,
                                    t,
                                    GroupTravelResult::Applied,
                                    GroupTravelReason::None,
                                );
                                self.queue_outbound(complete, DeliveryDisposition::ClearTracked);
                            } else {
                                self.observe_view(&view, fence, generation)?;
                            }
                        }
                        Err(GroupTravelError::Unavailable) => {
                            self.schedule_transient_poll_failure();
                        }
                        Err(GroupTravelError::Stale | GroupTravelError::NotFound) => {
                            t.pending_action = None;
                            self.next_poll = tokio::time::Instant::now();
                        }
                        Err(GroupTravelError::Unauthorized) => {
                            return Err(SessionError::Unauthorized);
                        }
                        Err(GroupTravelError::InvalidResponse) => {
                            return Err(SessionError::Realtime);
                        }
                    }
                }
            }
            Completion::Marker {
                fence,
                generation,
                group_id,
                proposal_id,
                result,
            } => {
                if let Some(t) = &mut self.tracked
                    && t.group_id == group_id
                    && t.proposal_id == proposal_id
                    && t.pending_marker
                {
                    match result {
                        Ok(view) => {
                            validate_view(&view, group_id, fence)?;
                            let member = view
                                .expected_members
                                .iter()
                                .position(|m| m.character_id == fence.character_id)
                                .ok_or(SessionError::Realtime)?;
                            if view.proposal_id != proposal_id
                                || view.route_id.as_str() != route_id(t.route)
                                || !view.scene_marked_by[member]
                            {
                                return Err(SessionError::Realtime);
                            }
                            t.pending_marker = false;
                            t.marker_accepted = true;
                            t.marker_fence = Some(fence);
                            self.observe_view(&view, fence, generation)?;
                        }
                        Err(GroupTravelError::Unavailable) => {
                            self.schedule_transient_poll_failure()
                        }
                        Err(GroupTravelError::Unauthorized) => {
                            return Err(SessionError::Unauthorized);
                        }
                        Err(GroupTravelError::Stale | GroupTravelError::NotFound) => {
                            t.pending_marker = false;
                            self.next_poll = tokio::time::Instant::now();
                        }
                        Err(GroupTravelError::InvalidResponse) => {
                            return Err(SessionError::Realtime);
                        }
                    }
                }
            }
            Completion::Receipt {
                fence,
                generation,
                group_id,
                proposal_id,
                request,
                result,
            } => {
                if let Some(t) = &mut self.tracked
                    && t.group_id == group_id
                    && t.proposal_id == proposal_id
                    && t.pending_receipt.as_ref() == Some(&request)
                {
                    match result {
                        Ok(view) => {
                            validate_view(&view, group_id, fence)?;
                            let member = view
                                .expected_members
                                .iter()
                                .position(|m| m.character_id == fence.character_id)
                                .ok_or(SessionError::Realtime)?;
                            if view.proposal_id != proposal_id
                                || view.route_id.as_str() != route_id(t.route)
                                || !view.scene_marked_by[member]
                                || !view.scene_receipted_by[member]
                                || !matches!(
                                    view.status,
                                    GroupTravelProposalStatus::AwaitingSceneReceipts
                                        | GroupTravelProposalStatus::Suspended
                                        | GroupTravelProposalStatus::Committed
                                )
                            {
                                return Err(SessionError::Realtime);
                            }
                            t.pending_receipt = None;
                            t.receipt_accepted = true;
                            self.observe_view(&view, fence, generation)?;
                        }
                        Err(GroupTravelError::Unavailable) => {
                            self.schedule_transient_poll_failure()
                        }
                        Err(GroupTravelError::Unauthorized) => {
                            return Err(SessionError::Unauthorized);
                        }
                        Err(GroupTravelError::Stale | GroupTravelError::NotFound) => {
                            // Keep the exact immutable snapshot request for
                            // recovery; a head change must not select another.
                            self.schedule_transient_poll_failure();
                        }
                        Err(GroupTravelError::InvalidResponse) => {
                            return Err(SessionError::Realtime);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn finish_view_result(
        &mut self,
        result: Result<Option<GroupTravelProposalView>, GroupTravelError>,
        fence: LeaseFence,
        generation: u32,
        expected: Option<GroupTravelProposalId>,
    ) -> Result<(), SessionError> {
        match result {
            Ok(Some(v)) if expected.is_none_or(|id| id == v.proposal_id) => {
                self.observe_view(&v, fence, generation)?;
            }
            Ok(Some(_)) | Err(GroupTravelError::InvalidResponse) => {
                return Err(SessionError::Realtime);
            }
            Err(GroupTravelError::Unavailable) => {
                self.schedule_transient_poll_failure();
            }
            Err(GroupTravelError::Unauthorized) => return Err(SessionError::Unauthorized),
            Err(GroupTravelError::Stale) if expected.is_some() => {
                self.queue_terminal_reason(GroupTravelReason::Conflict);
            }
            Err(GroupTravelError::Stale) => {
                self.group_id = None;
                self.schedule_poll_success(IDLE_POLL_INTERVAL);
            }
            Ok(None) | Err(GroupTravelError::NotFound) => {
                if expected.is_some() {
                    self.queue_terminal_reason(GroupTravelReason::Conflict);
                } else {
                    self.group_id = None;
                    self.schedule_poll_success(IDLE_POLL_INTERVAL);
                }
            }
        }
        Ok(())
    }
    fn install_created(
        &mut self,
        v: &GroupTravelProposalView,
        p: &PendingCreate,
        fence: LeaseFence,
    ) -> Result<(), SessionError> {
        validate_view(v, p.group_id, fence)?;
        if v.requester_character_id != fence.character_id
            || v.status != GroupTravelProposalStatus::Pending
            || route_from_id(v.route_id.as_str()) != Some(p.route)
            || v.departure != p.departure
            || v.endpoint != p.endpoint
        {
            return Err(SessionError::Realtime);
        }
        self.tracked = Some(TrackedProposal {
            group_id: p.group_id,
            proposal_id: v.proposal_id,
            route: p.route,
            departure: p.departure,
            endpoint: p.endpoint,
            request_id: p.request_id,
            role: Role::Requester,
            status: v.status,
            vote_deadline: vote_deadline(v),
            pending_action: None,
            marker_requested: false,
            pending_marker: false,
            marker_accepted: false,
            marker_fence: None,
            scene_complete: false,
            pending_receipt: None,
            receipt_accepted: false,
        });
        Ok(())
    }
    fn observe_view(
        &mut self,
        v: &GroupTravelProposalView,
        fence: LeaseFence,
        _generation: u32,
    ) -> Result<(), SessionError> {
        validate_view(v, v.group_id, fence)?;
        let route = route_from_id(v.route_id.as_str()).ok_or(SessionError::Realtime)?;
        if !v.departure.matches_route(route) {
            return Err(SessionError::Realtime);
        }
        if matches!(
            v.status,
            GroupTravelProposalStatus::AwaitingSceneReceipts | GroupTravelProposalStatus::Suspended
        ) && !is_story_route(route)
        {
            return Err(SessionError::Realtime);
        }
        if is_story_route(route)
            && v.status == GroupTravelProposalStatus::Committed
            && (v.scene_marked_by != [true, true] || v.scene_receipted_by != [true, true])
        {
            return Err(SessionError::Realtime);
        }
        let role = if v.requester_character_id == fence.character_id {
            Role::Requester
        } else {
            Role::Responder
        };
        if self.tracked.is_none() {
            self.tracked = Some(TrackedProposal {
                group_id: v.group_id,
                proposal_id: v.proposal_id,
                route,
                departure: v.departure,
                endpoint: v.endpoint,
                request_id: request_id_from_proposal(v.proposal_id),
                role,
                status: v.status,
                vote_deadline: vote_deadline(v),
                pending_action: None,
                marker_requested: false,
                pending_marker: false,
                marker_accepted: false,
                marker_fence: None,
                scene_complete: false,
                pending_receipt: None,
                receipt_accepted: false,
            });
        }
        let Some(t) = &mut self.tracked else {
            return Err(SessionError::Realtime);
        };
        if t.group_id != v.group_id
            || t.proposal_id != v.proposal_id
            || t.route != route
            || t.departure != v.departure
            || t.endpoint != v.endpoint
            || t.role != role
        {
            return Err(SessionError::Realtime);
        }
        t.status = v.status;
        t.vote_deadline = vote_deadline(v);
        if is_story_route(route)
            && v.status != GroupTravelProposalStatus::AwaitingSceneReceipts
            && self.outbound.is_some_and(|out| {
                matches!(
                    out.record.kind,
                    GroupTravelServerKind::SceneReady | GroupTravelServerKind::SceneMarkerAccepted
                )
            })
        {
            self.outbound = None;
        }
        match v.status {
            GroupTravelProposalStatus::Pending | GroupTravelProposalStatus::Committed => {
                self.queue_for_tracked_state();
            }
            GroupTravelProposalStatus::AwaitingSceneReceipts => self.queue_for_tracked_state(),
            GroupTravelProposalStatus::Suspended => {}
            GroupTravelProposalStatus::Declined => {
                self.queue_terminal_reason(GroupTravelReason::ParticipantDeclined);
            }
            GroupTravelProposalStatus::Cancelled => {
                if is_story_route(route) && t.marker_accepted {
                    return Err(SessionError::StoryTravelRecoveryPending);
                }
                self.queue_terminal_reason(GroupTravelReason::RequesterCanceled);
            }
            GroupTravelProposalStatus::Expired => {
                self.queue_terminal_reason(GroupTravelReason::Conflict);
            }
        }
        self.schedule_poll_success(POLL_INTERVAL);
        Ok(())
    }
    fn queue_for_tracked_state(&mut self) {
        let Some(t) = &self.tracked else { return };
        if is_story_route(t.route) && t.status == GroupTravelProposalStatus::Committed {
            // Both finalized scene receipts have committed. Complete clears
            // the ROM's scene session; the ROM checks its Dewford landing and
            // never treats this record as permission to warp.
            let complete = server_record(
                GroupTravelServerKind::Complete,
                t,
                GroupTravelResult::Applied,
                GroupTravelReason::None,
            );
            self.queue_outbound(complete, DeliveryDisposition::ClearTracked);
            return;
        }
        let kind = match t.status {
            GroupTravelProposalStatus::Pending if t.role == Role::Requester => {
                GroupTravelServerKind::Requesting
            }
            GroupTravelProposalStatus::Pending => GroupTravelServerKind::Offer,
            GroupTravelProposalStatus::Committed => GroupTravelServerKind::Commit,
            GroupTravelProposalStatus::AwaitingSceneReceipts
                if t.marker_requested && t.marker_accepted =>
            {
                GroupTravelServerKind::SceneMarkerAccepted
            }
            GroupTravelProposalStatus::AwaitingSceneReceipts => GroupTravelServerKind::SceneReady,
            _ => return,
        };
        let record = if kind == GroupTravelServerKind::Requesting {
            let mut record = requesting_record(t.route, t.departure, t.request_id, t.endpoint);
            record.remaining_seconds = remaining_vote_seconds(t);
            record
        } else {
            let mut record =
                server_record(kind, t, GroupTravelResult::None, GroupTravelReason::None);
            if kind == GroupTravelServerKind::Offer {
                record.remaining_seconds = remaining_vote_seconds(t);
            }
            record
        };
        self.queue_outbound(record, DeliveryDisposition::Retain);
    }
    fn queue_terminal_reason(&mut self, reason: GroupTravelReason) {
        if let Some(t) = &self.tracked {
            self.queue_outbound(
                server_record(
                    GroupTravelServerKind::Abort,
                    t,
                    GroupTravelResult::None,
                    reason,
                ),
                DeliveryDisposition::ClearTracked,
            );
        }
    }
    fn queue_outbound(
        &mut self,
        record: GroupTravelServerRecord,
        disposition: DeliveryDisposition,
    ) {
        if self.outbound.is_some_and(|v| v.record == record) {
            return;
        }
        self.outbound = Some(Outbound {
            record,
            disposition,
            retry_at: tokio::time::Instant::now(),
        });
    }
    fn remember_terminal(&mut self, outbound: Outbound) {
        let generation = self.generation.unwrap_or_default();
        let record = outbound.record;
        self.terminal.push_back(TerminalReplay {
            generation,
            route: record.route,
            departure: record.departure,
            endpoint: record.endpoint,
            request_id: record.request_id,
            proposal_id: record.proposal_id,
            record,
            disposition: outbound.disposition,
        });
        while self.terminal.len() > TERMINAL_REPLAY_LIMIT {
            self.terminal.pop_front();
        }
    }
    fn find_terminal(
        &self,
        generation: u32,
        r: GroupTravelClientRecord,
    ) -> Option<(GroupTravelServerRecord, DeliveryDisposition)> {
        self.terminal.iter().rev().find_map(|t| {
            let exact_proposal = r.proposal_id != [0; 16] && r.proposal_id == t.proposal_id;
            let scoped_zero = r.proposal_id == [0; 16]
                && t.proposal_id == [0; 16]
                && t.generation == generation
                && t.request_id == r.request_id;
            (t.route == r.route
                && t.departure == r.departure
                && t.endpoint == r.endpoint
                && (exact_proposal || scoped_zero))
                .then_some((t.record, t.disposition))
        })
    }
}
fn record_matches(t: &TrackedProposal, r: GroupTravelClientRecord, zero: bool) -> bool {
    t.route == r.route
        && t.departure == r.departure
        && t.endpoint == r.endpoint
        && t.request_id == r.request_id
        && (proposal_bytes(t.proposal_id) == r.proposal_id || (zero && r.proposal_id == [0; 16]))
}
async fn active_group<A: CloudApi>(
    api: &A,
    token: AccessToken,
    fence: LeaseFence,
) -> Result<Option<GroupId>, SessionError> {
    let req = OnlineSnapshotRequest {
        api_version: ApiVersion::V1,
        fence,
        incoming_after: None,
    };
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.online_snapshot(token.clone(), req.clone()),
    )
    .await
    .map_err(|_| SessionError::Cloud)?;
    let response = match first {
        Ok(v) => v,
        Err(crate::online::OnlineError::Unavailable) => {
            tokio::time::timeout(REQUEST_TIMEOUT, api.online_snapshot(token, req))
                .await
                .map_err(|_| SessionError::Cloud)?
                .map_err(map_online_error)?
        }
        Err(e) => return Err(map_online_error(e)),
    };
    if response.api_version != ApiVersion::V1 {
        return Err(SessionError::Realtime);
    }
    let group = response.group.map(|v| v.group);
    if group.as_ref().is_some_and(|g| {
        !g.members
            .iter()
            .any(|m| m.character_id == fence.character_id)
    }) {
        return Err(SessionError::Realtime);
    }
    Ok(group.map(|g| g.group_id))
}
fn map_online_error(e: crate::online::OnlineError) -> SessionError {
    match e {
        crate::online::OnlineError::Unauthorized => SessionError::Unauthorized,
        crate::online::OnlineError::InvalidResponse => SessionError::Realtime,
        crate::online::OnlineError::Unavailable | crate::online::OnlineError::Stale => {
            SessionError::Cloud
        }
    }
}
async fn retry_create<A: CloudApi>(
    api: &A,
    token: AccessToken,
    g: GroupId,
    r: GroupTravelProposalRequest,
) -> Result<GroupTravelProposalView, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_create(token.clone(), g, r.clone()),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => {
            tokio::time::timeout(REQUEST_TIMEOUT, api.group_travel_create(token, g, r))
                .await
                .unwrap_or(Err(GroupTravelError::Unavailable))
        }
        v => v,
    }
}
async fn retry_current<A: CloudApi>(
    api: &A,
    token: AccessToken,
    g: GroupId,
    f: LeaseFence,
) -> Result<Option<GroupTravelProposalView>, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_current(token.clone(), g, f),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => {
            tokio::time::timeout(REQUEST_TIMEOUT, api.group_travel_current(token, g, f))
                .await
                .unwrap_or(Err(GroupTravelError::Unavailable))
        }
        v => v,
    }
}
async fn retry_get<A: CloudApi>(
    api: &A,
    token: AccessToken,
    g: GroupId,
    p: GroupTravelProposalId,
    f: LeaseFence,
) -> Result<GroupTravelProposalView, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_get(token.clone(), g, p, f),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => {
            tokio::time::timeout(REQUEST_TIMEOUT, api.group_travel_get(token, g, p, f))
                .await
                .unwrap_or(Err(GroupTravelError::Unavailable))
        }
        v => v,
    }
}
async fn retry_action<A: CloudApi>(
    api: &A,
    token: AccessToken,
    g: GroupId,
    p: GroupTravelProposalId,
    r: GroupTravelActionRequest,
) -> Result<GroupTravelProposalView, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_action(token.clone(), g, p, r),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => {
            tokio::time::timeout(REQUEST_TIMEOUT, api.group_travel_action(token, g, p, r))
                .await
                .unwrap_or(Err(GroupTravelError::Unavailable))
        }
        v => v,
    }
}
async fn retry_scene_marker<A: CloudApi>(
    api: &A,
    token: AccessToken,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: GroupTravelSceneMarkerRequest,
) -> Result<GroupTravelProposalView, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_scene_marker(token.clone(), group_id, proposal_id, request.clone()),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => tokio::time::timeout(
            REQUEST_TIMEOUT,
            api.group_travel_scene_marker(token, group_id, proposal_id, request),
        )
        .await
        .unwrap_or(Err(GroupTravelError::Unavailable)),
        result => result,
    }
}
async fn retry_scene_receipt<A: CloudApi>(
    api: &A,
    token: AccessToken,
    group_id: GroupId,
    proposal_id: GroupTravelProposalId,
    request: GroupTravelSceneReceiptRequest,
) -> Result<GroupTravelProposalView, GroupTravelError> {
    let first = tokio::time::timeout(
        REQUEST_TIMEOUT,
        api.group_travel_scene_receipt(token.clone(), group_id, proposal_id, request.clone()),
    )
    .await
    .unwrap_or(Err(GroupTravelError::Unavailable));
    match first {
        Err(GroupTravelError::Unavailable) => tokio::time::timeout(
            REQUEST_TIMEOUT,
            api.group_travel_scene_receipt(token, group_id, proposal_id, request),
        )
        .await
        .unwrap_or(Err(GroupTravelError::Unavailable)),
        result => result,
    }
}
fn transient_poll_backoff(failures: u8) -> Duration {
    let exponent = failures.saturating_sub(1).min(6);
    let base_millis =
        (TRANSIENT_POLL_BACKOFF_BASE_MILLIS << exponent).min(TRANSIENT_POLL_BACKOFF_MAX_MILLIS);
    let jitter_limit = (base_millis / 4).min(TRANSIENT_POLL_JITTER_MAX_MILLIS);
    let jitter = if jitter_limit == 0 {
        0
    } else {
        let bytes = uuid::Uuid::new_v4().as_bytes().to_owned();
        let random = u64::from_le_bytes(
            bytes[..8]
                .try_into()
                .expect("a UUID always contains at least eight bytes"),
        );
        random % (jitter_limit + 1)
    };
    Duration::from_millis((base_millis + jitter).min(TRANSIENT_POLL_BACKOFF_MAX_MILLIS))
}
fn validate_view(
    v: &GroupTravelProposalView,
    g: GroupId,
    f: LeaseFence,
) -> Result<(), SessionError> {
    let Some(route) = route_from_id(v.route_id.as_str()) else {
        return Err(SessionError::Realtime);
    };
    if !v.departure.matches_route(route) {
        return Err(SessionError::Realtime);
    }
    if route.is_dynamic() != v.endpoint.is_some() {
        return Err(SessionError::Realtime);
    }
    if let Some(e) = v.endpoint
        && !(zone_is_map(&v.source, e.source_map_group, e.source_map_number)
            && zone_is_map(&v.destination, e.target_map_group, e.target_map_number))
    {
        return Err(SessionError::Realtime);
    }
    if v.api_version != ApiVersion::V1
        || v.group_id != g
        || (v.requester_character_id != f.character_id
            && v.responder_character_id != f.character_id)
        || v.requester_character_id == v.responder_character_id
        || v.expected_members[0].character_id >= v.expected_members[1].character_id
        || matches!(v.status, GroupTravelProposalStatus::Committed) != v.commit.is_some()
    {
        return Err(SessionError::Realtime);
    }
    if let Some(c) = &v.commit
        && (c.members[0].character_id >= c.members[1].character_id
            || c.group_zone_revision <= v.expected_group_zone_revision)
    {
        return Err(SessionError::Realtime);
    }
    if let Some(c) = &v.commit
        && (c.endpoint != v.endpoint || c.destination != v.destination)
    {
        return Err(SessionError::Realtime);
    }
    Ok(())
}
/// Whether a cloud zone names the numeric ROM map carried by a dynamic endpoint.
fn zone_is_map(zone: &coop_protocol::WorldZone, group: u8, number: u8) -> bool {
    zone.map_entry().is_ok_and(|entry| {
        entry.map_group == u16::from(group) && entry.map_number == u16::from(number)
    })
}
fn new_idempotency_key() -> Result<IdempotencyKey, SessionError> {
    IdempotencyKey::new(uuid::Uuid::new_v4()).map_err(|_| SessionError::Realtime)
}
fn request_id_from_proposal(p: GroupTravelProposalId) -> u32 {
    u32::from_le_bytes(proposal_bytes(p)[..4].try_into().unwrap()).max(1)
}
fn proposal_bytes(p: GroupTravelProposalId) -> [u8; 16] {
    *p.as_uuid().as_bytes()
}
fn vote_deadline(v: &GroupTravelProposalView) -> Option<tokio::time::Instant> {
    let server_now = v.server_now?.value();
    let remaining_ms = v.expires_at.value().saturating_sub(server_now).min(30_000);
    Some(tokio::time::Instant::now() + Duration::from_millis(remaining_ms))
}
fn remaining_vote_seconds(t: &TrackedProposal) -> u8 {
    t.vote_deadline
        .map(|deadline| {
            deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .as_secs()
                .min(30) as u8
        })
        .unwrap_or(0)
}
fn requesting_record(
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    request_id: u32,
    endpoint: Option<GroupTravelEndpoint>,
) -> GroupTravelServerRecord {
    GroupTravelServerRecord {
        kind: GroupTravelServerKind::Requesting,
        route,
        departure,
        request_id,
        proposal_id: [0; 16],
        result: GroupTravelResult::None,
        reason: GroupTravelReason::None,
        remaining_seconds: 0,
        endpoint,
    }
}
fn server_record(
    kind: GroupTravelServerKind,
    t: &TrackedProposal,
    result: GroupTravelResult,
    reason: GroupTravelReason,
) -> GroupTravelServerRecord {
    GroupTravelServerRecord {
        kind,
        route: t.route,
        departure: t.departure,
        request_id: t.request_id,
        proposal_id: proposal_bytes(t.proposal_id),
        result,
        reason,
        remaining_seconds: 0,
        endpoint: t.endpoint,
    }
}
fn abort_before_create(
    r: GroupTravelClientRecord,
    reason: GroupTravelReason,
) -> GroupTravelServerRecord {
    GroupTravelServerRecord {
        kind: GroupTravelServerKind::Abort,
        route: r.route,
        departure: r.departure,
        request_id: r.request_id,
        proposal_id: [0; 16],
        result: GroupTravelResult::None,
        reason,
        remaining_seconds: 0,
        endpoint: r.endpoint,
    }
}
const fn is_story_route(route: GroupTravelRoute) -> bool {
    matches!(
        route,
        GroupTravelRoute::FerryBrineyHouseDewford
            | GroupTravelRoute::SeagallopBillCinnabarOne
            | GroupTravelRoute::SeagallopBillOneCinnabar
    )
}

const fn route_id(r: GroupTravelRoute) -> &'static str {
    match r {
        GroupTravelRoute::Dig => "HOENN:DIG",
        GroupTravelRoute::EscapeRope => "HOENN:ESCAPE_ROPE",
        GroupTravelRoute::TrainOriginal => "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
        GroupTravelRoute::TrainLater => "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
        GroupTravelRoute::FerryOriginal => "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
        GroupTravelRoute::FerryLater => "JOHTO:OLIVINE_KANTO_LATER_FERRY",
        GroupTravelRoute::GateOriginal => "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
        GroupTravelRoute::GateLater => "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
        GroupTravelRoute::ReturnFerryOriginal => "KANTO:ORIGINAL_VERMILION_JOHTO_FERRY",
        GroupTravelRoute::ReturnFerryLater => "KANTO:LATER_VERMILION_JOHTO_FERRY",
        GroupTravelRoute::ReturnTrainOriginal => "KANTO:ORIGINAL_SAFFRON_JOHTO_TRAIN",
        GroupTravelRoute::ReturnTrainLater => "KANTO:LATER_SAFFRON_JOHTO_TRAIN",
        GroupTravelRoute::ReturnGateOriginal => "KANTO:ORIGINAL_ROUTE22_JOHTO_ROUTE22",
        GroupTravelRoute::ReturnGateLater => "KANTO:LATER_ROUTE22_JOHTO_ROUTE22",
        GroupTravelRoute::CableCarRoute112MtChimney => "HOENN:ROUTE112_MT_CHIMNEY_CABLE_CAR",
        GroupTravelRoute::CableCarMtChimneyRoute112 => "HOENN:MT_CHIMNEY_ROUTE112_CABLE_CAR",
        GroupTravelRoute::FerryOlivineSouthernIsland => "JOHTO:OLIVINE_SOUTHERN_ISLAND_FERRY",
        GroupTravelRoute::FerryOlivineBirthIsland => "JOHTO:OLIVINE_BIRTH_ISLAND_FERRY",
        GroupTravelRoute::FerryOlivineFarawayIsland => "JOHTO:OLIVINE_FARAWAY_ISLAND_FERRY",
        GroupTravelRoute::FerryOlivineBattleFrontier => "JOHTO:OLIVINE_BATTLE_FRONTIER_FERRY",
        GroupTravelRoute::FerryVermilionSouthernIsland => {
            "KANTO_LATER:VERMILION_SOUTHERN_ISLAND_FERRY"
        }
        GroupTravelRoute::FerryVermilionBirthIsland => "KANTO_LATER:VERMILION_BIRTH_ISLAND_FERRY",
        GroupTravelRoute::FerryVermilionFarawayIsland => {
            "KANTO_LATER:VERMILION_FARAWAY_ISLAND_FERRY"
        }
        GroupTravelRoute::FerryVermilionBattleFrontier => {
            "KANTO_LATER:VERMILION_BATTLE_FRONTIER_FERRY"
        }
        GroupTravelRoute::FerrySouthernIslandLilycove => "HOENN:SOUTHERN_ISLAND_LILYCOVE_FERRY",
        GroupTravelRoute::FerryBirthIslandLilycove => "HOENN:BIRTH_ISLAND_LILYCOVE_FERRY",
        GroupTravelRoute::FerryFarawayIslandLilycove => "HOENN:FARAWAY_ISLAND_LILYCOVE_FERRY",
        GroupTravelRoute::FerryBattleFrontierSlateport => "HOENN:BATTLE_FRONTIER_SLATEPORT_FERRY",
        GroupTravelRoute::FerryBattleFrontierLilycove => "HOENN:BATTLE_FRONTIER_LILYCOVE_FERRY",
        GroupTravelRoute::FerryLilycoveSouthernIsland => "HOENN:LILYCOVE_SOUTHERN_ISLAND_FERRY",
        GroupTravelRoute::FerryLilycoveNavelRock => "HOENN:LILYCOVE_NAVEL_ROCK_FERRY",
        GroupTravelRoute::FerryLilycoveBirthIsland => "HOENN:LILYCOVE_BIRTH_ISLAND_FERRY",
        GroupTravelRoute::FerryLilycoveFarawayIsland => "HOENN:LILYCOVE_FARAWAY_ISLAND_FERRY",
        GroupTravelRoute::FerryLilycoveBattleFrontier => "HOENN:LILYCOVE_BATTLE_FRONTIER_FERRY",
        GroupTravelRoute::FerrySlateportBattleFrontier => "HOENN:SLATEPORT_BATTLE_FRONTIER_FERRY",
        GroupTravelRoute::FerryNavelRockLilycove => "HOENN:NAVEL_ROCK_LILYCOVE_FERRY",
        GroupTravelRoute::FerrySSTidalSlateportBoard => "HOENN:SLATEPORT_SS_TIDAL_BOARD_FERRY",
        GroupTravelRoute::FerrySSTidalLilycoveBoard => "HOENN:LILYCOVE_SS_TIDAL_BOARD_FERRY",
        GroupTravelRoute::FerrySSTidalLilycoveExit => "HOENN:SS_TIDAL_LILYCOVE_EXIT_FERRY",
        GroupTravelRoute::FerrySSTidalSlateportExit => "HOENN:SS_TIDAL_SLATEPORT_EXIT_FERRY",
        GroupTravelRoute::FerryBrineyHouseDewford => "HOENN:BRINEY_HOUSE_DEWFORD_FERRY",
        GroupTravelRoute::FerryDewfordBrineyHouse => "HOENN:DEWFORD_BRINEY_HOUSE_FERRY",
        GroupTravelRoute::FerryDewfordRoute109 => "HOENN:DEWFORD_ROUTE109_FERRY",
        GroupTravelRoute::FerryRoute109Dewford => "HOENN:ROUTE109_DEWFORD_FERRY",
        GroupTravelRoute::SeagallopVermilionOne => "SEAGALLOP:VERMILION_ONE_FERRY",
        GroupTravelRoute::SeagallopVermilionTwo => "SEAGALLOP:VERMILION_TWO_FERRY",
        GroupTravelRoute::SeagallopVermilionThree => "SEAGALLOP:VERMILION_THREE_FERRY",
        GroupTravelRoute::SeagallopVermilionFour => "SEAGALLOP:VERMILION_FOUR_FERRY",
        GroupTravelRoute::SeagallopVermilionFive => "SEAGALLOP:VERMILION_FIVE_FERRY",
        GroupTravelRoute::SeagallopVermilionSix => "SEAGALLOP:VERMILION_SIX_FERRY",
        GroupTravelRoute::SeagallopVermilionSeven => "SEAGALLOP:VERMILION_SEVEN_FERRY",
        GroupTravelRoute::SeagallopOneVermilion => "SEAGALLOP:ONE_VERMILION_FERRY",
        GroupTravelRoute::SeagallopOneTwo => "SEAGALLOP:ONE_TWO_FERRY",
        GroupTravelRoute::SeagallopOneThree => "SEAGALLOP:ONE_THREE_FERRY",
        GroupTravelRoute::SeagallopOneFour => "SEAGALLOP:ONE_FOUR_FERRY",
        GroupTravelRoute::SeagallopOneFive => "SEAGALLOP:ONE_FIVE_FERRY",
        GroupTravelRoute::SeagallopOneSix => "SEAGALLOP:ONE_SIX_FERRY",
        GroupTravelRoute::SeagallopOneSeven => "SEAGALLOP:ONE_SEVEN_FERRY",
        GroupTravelRoute::SeagallopTwoVermilion => "SEAGALLOP:TWO_VERMILION_FERRY",
        GroupTravelRoute::SeagallopTwoOne => "SEAGALLOP:TWO_ONE_FERRY",
        GroupTravelRoute::SeagallopTwoThree => "SEAGALLOP:TWO_THREE_FERRY",
        GroupTravelRoute::SeagallopTwoFour => "SEAGALLOP:TWO_FOUR_FERRY",
        GroupTravelRoute::SeagallopTwoFive => "SEAGALLOP:TWO_FIVE_FERRY",
        GroupTravelRoute::SeagallopTwoSix => "SEAGALLOP:TWO_SIX_FERRY",
        GroupTravelRoute::SeagallopTwoSeven => "SEAGALLOP:TWO_SEVEN_FERRY",
        GroupTravelRoute::SeagallopThreeVermilion => "SEAGALLOP:THREE_VERMILION_FERRY",
        GroupTravelRoute::SeagallopThreeOne => "SEAGALLOP:THREE_ONE_FERRY",
        GroupTravelRoute::SeagallopThreeTwo => "SEAGALLOP:THREE_TWO_FERRY",
        GroupTravelRoute::SeagallopThreeFour => "SEAGALLOP:THREE_FOUR_FERRY",
        GroupTravelRoute::SeagallopThreeFive => "SEAGALLOP:THREE_FIVE_FERRY",
        GroupTravelRoute::SeagallopThreeSix => "SEAGALLOP:THREE_SIX_FERRY",
        GroupTravelRoute::SeagallopThreeSeven => "SEAGALLOP:THREE_SEVEN_FERRY",
        GroupTravelRoute::SeagallopFourVermilion => "SEAGALLOP:FOUR_VERMILION_FERRY",
        GroupTravelRoute::SeagallopFourOne => "SEAGALLOP:FOUR_ONE_FERRY",
        GroupTravelRoute::SeagallopFourTwo => "SEAGALLOP:FOUR_TWO_FERRY",
        GroupTravelRoute::SeagallopFourThree => "SEAGALLOP:FOUR_THREE_FERRY",
        GroupTravelRoute::SeagallopFourFive => "SEAGALLOP:FOUR_FIVE_FERRY",
        GroupTravelRoute::SeagallopFourSix => "SEAGALLOP:FOUR_SIX_FERRY",
        GroupTravelRoute::SeagallopFourSeven => "SEAGALLOP:FOUR_SEVEN_FERRY",
        GroupTravelRoute::SeagallopFiveVermilion => "SEAGALLOP:FIVE_VERMILION_FERRY",
        GroupTravelRoute::SeagallopFiveOne => "SEAGALLOP:FIVE_ONE_FERRY",
        GroupTravelRoute::SeagallopFiveTwo => "SEAGALLOP:FIVE_TWO_FERRY",
        GroupTravelRoute::SeagallopFiveThree => "SEAGALLOP:FIVE_THREE_FERRY",
        GroupTravelRoute::SeagallopFiveFour => "SEAGALLOP:FIVE_FOUR_FERRY",
        GroupTravelRoute::SeagallopFiveSix => "SEAGALLOP:FIVE_SIX_FERRY",
        GroupTravelRoute::SeagallopFiveSeven => "SEAGALLOP:FIVE_SEVEN_FERRY",
        GroupTravelRoute::SeagallopSixVermilion => "SEAGALLOP:SIX_VERMILION_FERRY",
        GroupTravelRoute::SeagallopSixOne => "SEAGALLOP:SIX_ONE_FERRY",
        GroupTravelRoute::SeagallopSixTwo => "SEAGALLOP:SIX_TWO_FERRY",
        GroupTravelRoute::SeagallopSixThree => "SEAGALLOP:SIX_THREE_FERRY",
        GroupTravelRoute::SeagallopSixFour => "SEAGALLOP:SIX_FOUR_FERRY",
        GroupTravelRoute::SeagallopSixFive => "SEAGALLOP:SIX_FIVE_FERRY",
        GroupTravelRoute::SeagallopSixSeven => "SEAGALLOP:SIX_SEVEN_FERRY",
        GroupTravelRoute::SeagallopSevenVermilion => "SEAGALLOP:SEVEN_VERMILION_FERRY",
        GroupTravelRoute::SeagallopSevenOne => "SEAGALLOP:SEVEN_ONE_FERRY",
        GroupTravelRoute::SeagallopSevenTwo => "SEAGALLOP:SEVEN_TWO_FERRY",
        GroupTravelRoute::SeagallopSevenThree => "SEAGALLOP:SEVEN_THREE_FERRY",
        GroupTravelRoute::SeagallopSevenFour => "SEAGALLOP:SEVEN_FOUR_FERRY",
        GroupTravelRoute::SeagallopSevenFive => "SEAGALLOP:SEVEN_FIVE_FERRY",
        GroupTravelRoute::SeagallopSevenSix => "SEAGALLOP:SEVEN_SIX_FERRY",
        GroupTravelRoute::SeagallopVermilionNavel => "SEAGALLOP:VERMILION_NAVEL_FERRY",
        GroupTravelRoute::SeagallopNavelVermilion => "SEAGALLOP:NAVEL_VERMILION_FERRY",
        GroupTravelRoute::SeagallopVermilionBirth => "SEAGALLOP:VERMILION_BIRTH_FERRY",
        GroupTravelRoute::SeagallopBirthVermilion => "SEAGALLOP:BIRTH_VERMILION_FERRY",
        GroupTravelRoute::SeagallopBillCinnabarOne => "KANTO:CINNABAR_ONE_BILL_FERRY",
        GroupTravelRoute::SeagallopBillOneCinnabar => "SEVII:ONE_CINNABAR_BILL_FERRY",
        GroupTravelRoute::FlyLittleroot => "HOENN:FLY_LITTLEROOT",
        GroupTravelRoute::FlyJohtoNewbark => "JOHTO:FLY_NEW_BARK_TOWN",
        GroupTravelRoute::FlyJohtoCherrygrove => "JOHTO:FLY_CHERRYGROVE_CITY",
        GroupTravelRoute::FlyJohtoViolet => "JOHTO:FLY_VIOLET_CITY",
        GroupTravelRoute::FlyJohtoAzalea => "JOHTO:FLY_AZALEA_TOWN",
        GroupTravelRoute::FlyJohtoGoldenrod => "JOHTO:FLY_GOLDENROD_CITY",
        GroupTravelRoute::FlyJohtoEcruteak => "JOHTO:FLY_ECRUTEAK_CITY",
        GroupTravelRoute::FlyJohtoOlivine => "JOHTO:FLY_OLIVINE_CITY",
        GroupTravelRoute::FlyJohtoCianwood => "JOHTO:FLY_CIANWOOD_CITY",
        GroupTravelRoute::FlyJohtoMahogany => "JOHTO:FLY_MAHOGANYTOWN",
        GroupTravelRoute::FlyJohtoBlackthorn => "JOHTO:FLY_BLACKTHORN_CITY",
        GroupTravelRoute::FlyHoennOldale => "HOENN:FLY_OLDALE_TOWN",
        GroupTravelRoute::FlyHoennDewford => "HOENN:FLY_DEWFORD_TOWN",
        GroupTravelRoute::FlyHoennLavaridge => "HOENN:FLY_LAVARIDGE_TOWN",
        GroupTravelRoute::FlyHoennFallarbor => "HOENN:FLY_FALLARBOR_TOWN",
        GroupTravelRoute::FlyHoennVerdanturf => "HOENN:FLY_VERDANTURF_TOWN",
        GroupTravelRoute::FlyHoennPacifidlog => "HOENN:FLY_PACIFIDLOG_TOWN",
        GroupTravelRoute::FlyHoennPetalburg => "HOENN:FLY_PETALBURG_CITY",
        GroupTravelRoute::FlyHoennSlateport => "HOENN:FLY_SLATEPORT_CITY",
        GroupTravelRoute::FlyHoennMauville => "HOENN:FLY_MAUVILLE_CITY",
        GroupTravelRoute::FlyHoennRustboro => "HOENN:FLY_RUSTBORO_CITY",
        GroupTravelRoute::FlyHoennFortree => "HOENN:FLY_FORTREE_CITY",
        GroupTravelRoute::FlyHoennLilycove => "HOENN:FLY_LILYCOVE_CITY",
        GroupTravelRoute::FlyHoennMossdeep => "HOENN:FLY_MOSSDEEP_CITY",
        GroupTravelRoute::FlyHoennSootopolis => "HOENN:FLY_SOOTOPOLIS_CITY",
        GroupTravelRoute::FlyKantoOriginalPallet => "KANTO:FLY_PALLET_TOWN",
        GroupTravelRoute::FlyKantoOriginalViridian => "KANTO:FLY_VIRIDIAN_CITY",
        GroupTravelRoute::FlyKantoOriginalPewter => "KANTO:FLY_PEWTER_CITY",
        GroupTravelRoute::FlyKantoOriginalCerulean => "KANTO:FLY_CERULEAN_CITY",
        GroupTravelRoute::FlyKantoOriginalLavender => "KANTO:FLY_LAVENDER_TOWN",
        GroupTravelRoute::FlyKantoOriginalVermilion => "KANTO:FLY_VERMILION_CITY",
        GroupTravelRoute::FlyKantoOriginalCeladon => "KANTO:FLY_CELADON_CITY",
        GroupTravelRoute::FlyKantoOriginalFuchsia => "KANTO:FLY_FUCHSIA_CITY",
        GroupTravelRoute::FlyKantoOriginalCinnabar => "KANTO:FLY_CINNABAR_ISLAND",
        GroupTravelRoute::FlyKantoOriginalIndigo => "KANTO:FLY_INDIGO_PLATEAU",
        GroupTravelRoute::FlyKantoOriginalSaffron => "KANTO:FLY_SAFFRON_CITY",
        GroupTravelRoute::FlyKantoLaterPallet => "KANTO_LATER:FLY_PALLET_TOWN",
        GroupTravelRoute::FlyKantoLaterViridian => "KANTO_LATER:FLY_VIRIDIAN_CITY",
        GroupTravelRoute::FlyKantoLaterPewter => "KANTO_LATER:FLY_PEWTER_CITY",
        GroupTravelRoute::FlyKantoLaterCerulean => "KANTO_LATER:FLY_CERULEAN_CITY",
        GroupTravelRoute::FlyKantoLaterLavender => "KANTO_LATER:FLY_LAVENDER_TOWN",
        GroupTravelRoute::FlyKantoLaterVermilion => "KANTO_LATER:FLY_VERMILION_CITY",
        GroupTravelRoute::FlyKantoLaterCeladon => "KANTO_LATER:FLY_CELADON_CITY",
        GroupTravelRoute::FlyKantoLaterFuchsia => "KANTO_LATER:FLY_FUCHSIA_CITY",
        GroupTravelRoute::FlyKantoLaterSaffron => "KANTO_LATER:FLY_SAFFRON_CITY",
        GroupTravelRoute::FlyKantoLaterCinnabar => "KANTO_LATER:FLY_CINNABAR_ISLAND",
        GroupTravelRoute::FlySeviiOneIsland => "SEVII:FLY_ONE_ISLAND",
        GroupTravelRoute::FlySeviiTwoIsland => "SEVII:FLY_TWO_ISLAND",
        GroupTravelRoute::FlySeviiThreeIsland => "SEVII:FLY_THREE_ISLAND",
        GroupTravelRoute::FlySeviiFourIsland => "SEVII:FLY_FOUR_ISLAND",
        GroupTravelRoute::FlySeviiFiveIsland => "SEVII:FLY_FIVE_ISLAND",
        GroupTravelRoute::FlySeviiSevenIsland => "SEVII:FLY_SEVEN_ISLAND",
        GroupTravelRoute::FlySeviiSixIsland => "SEVII:FLY_SIX_ISLAND",
        GroupTravelRoute::FlyKantoRoute4PokemonCenter => "KANTO:FLY_ROUTE_4_POKECENTER",
        GroupTravelRoute::FlyKantoRoute10PokemonCenter => "KANTO:FLY_ROUTE_10_POKECENTER",
        GroupTravelRoute::FlyHoennEverGrandeCenter => "HOENN:FLY_EVER_GRANDE_CITY_CENTER",
        GroupTravelRoute::FlyHoennEverGrandeLeague => "HOENN:FLY_EVER_GRANDE_CITY_LEAGUE",
        GroupTravelRoute::FlyHoennBattleFrontier => "HOENN:FLY_BATTLE_FRONTIER",
    }
}
fn route_from_id(id: &str) -> Option<GroupTravelRoute> {
    [
        GroupTravelRoute::Dig,
        GroupTravelRoute::EscapeRope,
        GroupTravelRoute::TrainOriginal,
        GroupTravelRoute::TrainLater,
        GroupTravelRoute::FerryOriginal,
        GroupTravelRoute::FerryLater,
        GroupTravelRoute::GateOriginal,
        GroupTravelRoute::GateLater,
        GroupTravelRoute::ReturnFerryOriginal,
        GroupTravelRoute::ReturnFerryLater,
        GroupTravelRoute::ReturnTrainOriginal,
        GroupTravelRoute::ReturnTrainLater,
        GroupTravelRoute::ReturnGateOriginal,
        GroupTravelRoute::ReturnGateLater,
        GroupTravelRoute::FerryOlivineSouthernIsland,
        GroupTravelRoute::FerryOlivineBirthIsland,
        GroupTravelRoute::FerryOlivineFarawayIsland,
        GroupTravelRoute::FerryOlivineBattleFrontier,
        GroupTravelRoute::FerryVermilionSouthernIsland,
        GroupTravelRoute::FerryVermilionBirthIsland,
        GroupTravelRoute::FerryVermilionFarawayIsland,
        GroupTravelRoute::FerryVermilionBattleFrontier,
        GroupTravelRoute::FerrySouthernIslandLilycove,
        GroupTravelRoute::FerryBirthIslandLilycove,
        GroupTravelRoute::FerryFarawayIslandLilycove,
        GroupTravelRoute::FerryBattleFrontierSlateport,
        GroupTravelRoute::FerryBattleFrontierLilycove,
        GroupTravelRoute::FerryLilycoveSouthernIsland,
        GroupTravelRoute::FerryLilycoveNavelRock,
        GroupTravelRoute::FerryLilycoveBirthIsland,
        GroupTravelRoute::FerryLilycoveFarawayIsland,
        GroupTravelRoute::FerryLilycoveBattleFrontier,
        GroupTravelRoute::FerrySlateportBattleFrontier,
        GroupTravelRoute::FerryNavelRockLilycove,
        GroupTravelRoute::FerrySSTidalSlateportBoard,
        GroupTravelRoute::FerrySSTidalLilycoveBoard,
        GroupTravelRoute::FerrySSTidalLilycoveExit,
        GroupTravelRoute::FerrySSTidalSlateportExit,
        GroupTravelRoute::FerryBrineyHouseDewford,
        GroupTravelRoute::FerryDewfordBrineyHouse,
        GroupTravelRoute::FerryDewfordRoute109,
        GroupTravelRoute::FerryRoute109Dewford,
        GroupTravelRoute::SeagallopVermilionOne,
        GroupTravelRoute::SeagallopVermilionTwo,
        GroupTravelRoute::SeagallopVermilionThree,
        GroupTravelRoute::SeagallopVermilionFour,
        GroupTravelRoute::SeagallopVermilionFive,
        GroupTravelRoute::SeagallopVermilionSix,
        GroupTravelRoute::SeagallopVermilionSeven,
        GroupTravelRoute::SeagallopOneVermilion,
        GroupTravelRoute::SeagallopOneTwo,
        GroupTravelRoute::SeagallopOneThree,
        GroupTravelRoute::SeagallopOneFour,
        GroupTravelRoute::SeagallopOneFive,
        GroupTravelRoute::SeagallopOneSix,
        GroupTravelRoute::SeagallopOneSeven,
        GroupTravelRoute::SeagallopTwoVermilion,
        GroupTravelRoute::SeagallopTwoOne,
        GroupTravelRoute::SeagallopTwoThree,
        GroupTravelRoute::SeagallopTwoFour,
        GroupTravelRoute::SeagallopTwoFive,
        GroupTravelRoute::SeagallopTwoSix,
        GroupTravelRoute::SeagallopTwoSeven,
        GroupTravelRoute::SeagallopThreeVermilion,
        GroupTravelRoute::SeagallopThreeOne,
        GroupTravelRoute::SeagallopThreeTwo,
        GroupTravelRoute::SeagallopThreeFour,
        GroupTravelRoute::SeagallopThreeFive,
        GroupTravelRoute::SeagallopThreeSix,
        GroupTravelRoute::SeagallopThreeSeven,
        GroupTravelRoute::SeagallopFourVermilion,
        GroupTravelRoute::SeagallopFourOne,
        GroupTravelRoute::SeagallopFourTwo,
        GroupTravelRoute::SeagallopFourThree,
        GroupTravelRoute::SeagallopFourFive,
        GroupTravelRoute::SeagallopFourSix,
        GroupTravelRoute::SeagallopFourSeven,
        GroupTravelRoute::SeagallopFiveVermilion,
        GroupTravelRoute::SeagallopFiveOne,
        GroupTravelRoute::SeagallopFiveTwo,
        GroupTravelRoute::SeagallopFiveThree,
        GroupTravelRoute::SeagallopFiveFour,
        GroupTravelRoute::SeagallopFiveSix,
        GroupTravelRoute::SeagallopFiveSeven,
        GroupTravelRoute::SeagallopSixVermilion,
        GroupTravelRoute::SeagallopSixOne,
        GroupTravelRoute::SeagallopSixTwo,
        GroupTravelRoute::SeagallopSixThree,
        GroupTravelRoute::SeagallopSixFour,
        GroupTravelRoute::SeagallopSixFive,
        GroupTravelRoute::SeagallopSixSeven,
        GroupTravelRoute::SeagallopSevenVermilion,
        GroupTravelRoute::SeagallopSevenOne,
        GroupTravelRoute::SeagallopSevenTwo,
        GroupTravelRoute::SeagallopSevenThree,
        GroupTravelRoute::SeagallopSevenFour,
        GroupTravelRoute::SeagallopSevenFive,
        GroupTravelRoute::SeagallopSevenSix,
        GroupTravelRoute::SeagallopVermilionNavel,
        GroupTravelRoute::SeagallopNavelVermilion,
        GroupTravelRoute::SeagallopVermilionBirth,
        GroupTravelRoute::SeagallopBirthVermilion,
        GroupTravelRoute::SeagallopBillCinnabarOne,
        GroupTravelRoute::SeagallopBillOneCinnabar,
        GroupTravelRoute::FlyLittleroot,
        GroupTravelRoute::FlyJohtoNewbark,
        GroupTravelRoute::FlyJohtoCherrygrove,
        GroupTravelRoute::FlyJohtoViolet,
        GroupTravelRoute::FlyJohtoAzalea,
        GroupTravelRoute::FlyJohtoGoldenrod,
        GroupTravelRoute::FlyJohtoEcruteak,
        GroupTravelRoute::FlyJohtoOlivine,
        GroupTravelRoute::FlyJohtoCianwood,
        GroupTravelRoute::FlyJohtoMahogany,
        GroupTravelRoute::FlyJohtoBlackthorn,
        GroupTravelRoute::FlyHoennOldale,
        GroupTravelRoute::FlyHoennDewford,
        GroupTravelRoute::FlyHoennLavaridge,
        GroupTravelRoute::FlyHoennFallarbor,
        GroupTravelRoute::FlyHoennVerdanturf,
        GroupTravelRoute::FlyHoennPacifidlog,
        GroupTravelRoute::FlyHoennPetalburg,
        GroupTravelRoute::FlyHoennSlateport,
        GroupTravelRoute::FlyHoennMauville,
        GroupTravelRoute::FlyHoennRustboro,
        GroupTravelRoute::FlyHoennFortree,
        GroupTravelRoute::FlyHoennLilycove,
        GroupTravelRoute::FlyHoennMossdeep,
        GroupTravelRoute::FlyHoennSootopolis,
        GroupTravelRoute::FlyKantoOriginalPallet,
        GroupTravelRoute::FlyKantoOriginalViridian,
        GroupTravelRoute::FlyKantoOriginalPewter,
        GroupTravelRoute::FlyKantoOriginalCerulean,
        GroupTravelRoute::FlyKantoOriginalLavender,
        GroupTravelRoute::FlyKantoOriginalVermilion,
        GroupTravelRoute::FlyKantoOriginalCeladon,
        GroupTravelRoute::FlyKantoOriginalFuchsia,
        GroupTravelRoute::FlyKantoOriginalCinnabar,
        GroupTravelRoute::FlyKantoOriginalIndigo,
        GroupTravelRoute::FlyKantoOriginalSaffron,
        GroupTravelRoute::FlyKantoLaterPallet,
        GroupTravelRoute::FlyKantoLaterViridian,
        GroupTravelRoute::FlyKantoLaterPewter,
        GroupTravelRoute::FlyKantoLaterCerulean,
        GroupTravelRoute::FlyKantoLaterLavender,
        GroupTravelRoute::FlyKantoLaterVermilion,
        GroupTravelRoute::FlyKantoLaterCeladon,
        GroupTravelRoute::FlyKantoLaterFuchsia,
        GroupTravelRoute::FlyKantoLaterSaffron,
        GroupTravelRoute::FlyKantoLaterCinnabar,
        GroupTravelRoute::FlySeviiOneIsland,
        GroupTravelRoute::FlySeviiTwoIsland,
        GroupTravelRoute::FlySeviiThreeIsland,
        GroupTravelRoute::FlySeviiFourIsland,
        GroupTravelRoute::FlySeviiFiveIsland,
        GroupTravelRoute::FlySeviiSevenIsland,
        GroupTravelRoute::FlySeviiSixIsland,
        GroupTravelRoute::FlyKantoRoute4PokemonCenter,
        GroupTravelRoute::FlyKantoRoute10PokemonCenter,
        GroupTravelRoute::FlyHoennEverGrandeCenter,
        GroupTravelRoute::FlyHoennEverGrandeLeague,
        GroupTravelRoute::FlyHoennBattleFrontier,
    ]
    .into_iter()
    .find(|r| route_id(*r) == id)
}
#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{
        CharacterId, ClientInstanceId, GroupMemberView, GroupTravelCommit, Revision, RouteId,
        SessionEpoch, SessionId,
    };
    use coop_protocol::{RegionId, WorldZone};
    use uuid::Uuid;
    fn cid(v: u128) -> CharacterId {
        CharacterId::new(Uuid::from_u128(v)).unwrap()
    }
    fn gid() -> GroupId {
        GroupId::new(Uuid::from_u128(30)).unwrap()
    }
    fn pid() -> GroupTravelProposalId {
        GroupTravelProposalId::new(Uuid::from_u128(40)).unwrap()
    }
    fn fence(a: CharacterId) -> LeaseFence {
        LeaseFence::new(
            SessionId::new(Uuid::from_u128(50)).unwrap(),
            a,
            Revision::new(7),
            SessionEpoch::new(8).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(60)).unwrap(),
        )
    }
    fn proposal(s: GroupTravelProposalStatus) -> GroupTravelProposalView {
        let a = cid(1);
        let b = cid(2);
        let d = WorldZone::new(RegionId::Kanto, "KANTO_LATER_SAFFRON_CITY", 1).unwrap();
        GroupTravelProposalView {
            api_version: ApiVersion::V1,
            proposal_id: pid(),
            group_id: gid(),
            requester_character_id: a,
            responder_character_id: b,
            route_id: RouteId::new(route_id(GroupTravelRoute::TrainLater)).unwrap(),
            departure: GroupTravelDeparture::Train,
            source: WorldZone::new(RegionId::Johto, "GOLDENROD_CITY", 1).unwrap(),
            destination: d.clone(),
            endpoint: None,
            expected_group_zone_revision: 4,
            expected_members: [
                GroupMemberView {
                    character_id: a,
                    world_revision: 9,
                },
                GroupMemberView {
                    character_id: b,
                    world_revision: 10,
                },
            ],
            status: s,
            expires_at: coop_cloud::UnixTimestampMillis::new(99_999),
            server_now: None,
            commit: (s == GroupTravelProposalStatus::Committed).then_some(GroupTravelCommit {
                group_zone_revision: 5,
                members: [
                    GroupMemberView {
                        character_id: a,
                        world_revision: 10,
                    },
                    GroupMemberView {
                        character_id: b,
                        world_revision: 11,
                    },
                ],
                destination: d,
                endpoint: None,
            }),
            applied_by: [false, false],
            scene_marked_by: [false, false],
            scene_receipted_by: [false, false],
        }
    }
    fn client(
        k: GroupTravelClientKind,
        result: GroupTravelResult,
        p: [u8; 16],
    ) -> GroupTravelClientRecord {
        GroupTravelClientRecord {
            kind: k,
            route: GroupTravelRoute::TrainLater,
            departure: GroupTravelDeparture::Train,
            request_id: 7,
            proposal_id: p,
            result,
            reason: if k == GroupTravelClientKind::Cancel {
                GroupTravelReason::RequesterCanceled
            } else {
                GroupTravelReason::None
            },
            endpoint: None,
        }
    }
    fn api() -> ReqwestCloudApi {
        ReqwestCloudApi::new("http://127.0.0.1:9").unwrap()
    }
    fn token() -> AccessToken {
        AccessToken::new("group-travel-test").unwrap()
    }
    fn tracked(role: Role, status: GroupTravelProposalStatus) -> TrackedProposal {
        TrackedProposal {
            group_id: gid(),
            proposal_id: pid(),
            route: GroupTravelRoute::TrainLater,
            departure: GroupTravelDeparture::Train,
            endpoint: None,
            request_id: 7,
            role,
            status,
            vote_deadline: None,
            pending_action: None,
            marker_requested: false,
            pending_marker: false,
            marker_accepted: false,
            marker_fence: None,
            scene_complete: false,
            pending_receipt: None,
            receipt_accepted: false,
        }
    }
    #[test]
    fn all_routes_map_exactly() {
        for r in [
            GroupTravelRoute::TrainOriginal,
            GroupTravelRoute::TrainLater,
            GroupTravelRoute::FerryOriginal,
            GroupTravelRoute::FerryLater,
            GroupTravelRoute::GateOriginal,
            GroupTravelRoute::GateLater,
            GroupTravelRoute::FlyLittleroot,
            GroupTravelRoute::FlyJohtoNewbark,
            GroupTravelRoute::FlyJohtoCherrygrove,
            GroupTravelRoute::FlyJohtoViolet,
            GroupTravelRoute::FlyJohtoAzalea,
            GroupTravelRoute::FlyJohtoGoldenrod,
            GroupTravelRoute::FlyJohtoEcruteak,
            GroupTravelRoute::FlyJohtoOlivine,
            GroupTravelRoute::FlyJohtoCianwood,
            GroupTravelRoute::FlyJohtoMahogany,
            GroupTravelRoute::FlyJohtoBlackthorn,
            GroupTravelRoute::FlyHoennOldale,
            GroupTravelRoute::FlyHoennDewford,
            GroupTravelRoute::FlyHoennLavaridge,
            GroupTravelRoute::FlyHoennFallarbor,
            GroupTravelRoute::FlyHoennVerdanturf,
            GroupTravelRoute::FlyHoennPacifidlog,
            GroupTravelRoute::FlyHoennPetalburg,
            GroupTravelRoute::FlyHoennSlateport,
            GroupTravelRoute::FlyHoennMauville,
            GroupTravelRoute::FlyHoennRustboro,
            GroupTravelRoute::FlyHoennFortree,
            GroupTravelRoute::FlyHoennLilycove,
            GroupTravelRoute::FlyHoennMossdeep,
            GroupTravelRoute::FlyHoennSootopolis,
            GroupTravelRoute::FlyKantoOriginalPallet,
            GroupTravelRoute::FlyKantoOriginalViridian,
            GroupTravelRoute::FlyKantoOriginalPewter,
            GroupTravelRoute::FlyKantoOriginalCerulean,
            GroupTravelRoute::FlyKantoOriginalLavender,
            GroupTravelRoute::FlyKantoOriginalVermilion,
            GroupTravelRoute::FlyKantoOriginalCeladon,
            GroupTravelRoute::FlyKantoOriginalFuchsia,
            GroupTravelRoute::FlyKantoOriginalCinnabar,
            GroupTravelRoute::FlyKantoOriginalIndigo,
            GroupTravelRoute::FlyKantoOriginalSaffron,
            GroupTravelRoute::FlyKantoLaterPallet,
            GroupTravelRoute::FlyKantoLaterViridian,
            GroupTravelRoute::FlyKantoLaterPewter,
            GroupTravelRoute::FlyKantoLaterCerulean,
            GroupTravelRoute::FlyKantoLaterLavender,
            GroupTravelRoute::FlyKantoLaterVermilion,
            GroupTravelRoute::FlyKantoLaterCeladon,
            GroupTravelRoute::FlyKantoLaterFuchsia,
            GroupTravelRoute::FlyKantoLaterSaffron,
            GroupTravelRoute::FlyKantoLaterCinnabar,
            GroupTravelRoute::FlySeviiOneIsland,
            GroupTravelRoute::FlySeviiTwoIsland,
            GroupTravelRoute::FlySeviiThreeIsland,
            GroupTravelRoute::FlySeviiFourIsland,
            GroupTravelRoute::FlySeviiFiveIsland,
            GroupTravelRoute::FlySeviiSevenIsland,
            GroupTravelRoute::FlySeviiSixIsland,
            GroupTravelRoute::FlyKantoRoute4PokemonCenter,
            GroupTravelRoute::FlyKantoRoute10PokemonCenter,
            GroupTravelRoute::FlyHoennEverGrandeCenter,
            GroupTravelRoute::FlyHoennEverGrandeLeague,
            GroupTravelRoute::FlyHoennBattleFrontier,
            GroupTravelRoute::ReturnFerryOriginal,
            GroupTravelRoute::ReturnFerryLater,
            GroupTravelRoute::ReturnTrainOriginal,
            GroupTravelRoute::ReturnTrainLater,
            GroupTravelRoute::ReturnGateOriginal,
            GroupTravelRoute::ReturnGateLater,
        ] {
            assert_eq!(route_from_id(route_id(r)), Some(r));
        }
    }

    #[test]
    fn transient_poll_backoff_is_bounded_and_exponential() {
        let first = transient_poll_backoff(1);
        let second = transient_poll_backoff(2);
        let third = transient_poll_backoff(3);
        assert!(first >= Duration::from_millis(500));
        assert!(first <= Duration::from_millis(625));
        assert!(second >= Duration::from_millis(1_000));
        assert!(second <= Duration::from_millis(1_250));
        assert!(third >= Duration::from_millis(2_000));
        assert!(third <= Duration::from_millis(2_500));
        assert!(transient_poll_backoff(u8::MAX) <= Duration::from_secs(30));
    }

    #[test]
    fn transient_poll_failure_backoff_resets_after_success() {
        let mut owner = GroupTravelOwner::default();
        owner.schedule_transient_poll_failure();
        owner.schedule_transient_poll_failure();
        assert_eq!(owner.transient_poll_failures, 2);
        owner.schedule_poll_success(POLL_INTERVAL);
        assert_eq!(owner.transient_poll_failures, 0);
        assert!(owner.next_poll > tokio::time::Instant::now());
    }

    #[test]
    fn zero_id_cancel_before_and_after_create() {
        let a = cid(1);
        let mut o = GroupTravelOwner {
            group_id: Some(gid()),
            ..GroupTravelOwner::default()
        };
        o.handle(
            fence(a),
            1,
            client(
                GroupTravelClientKind::Request,
                GroupTravelResult::None,
                [0; 16],
            ),
        )
        .unwrap();
        let create_key = o.pending_create.as_ref().unwrap().request.idempotency_key;
        o.handle(
            fence(a),
            1,
            client(
                GroupTravelClientKind::Request,
                GroupTravelResult::None,
                [0; 16],
            ),
        )
        .unwrap();
        assert_eq!(
            o.pending_create.as_ref().unwrap().request.idempotency_key,
            create_key
        );
        o.handle(
            fence(a),
            1,
            client(
                GroupTravelClientKind::Cancel,
                GroupTravelResult::None,
                [0; 16],
            ),
        )
        .unwrap();
        assert!(o.pending_create.as_ref().unwrap().cancel_requested);
        let p = o.pending_create.take().unwrap();
        o.install_created(&proposal(GroupTravelProposalStatus::Pending), &p, fence(a))
            .unwrap();
        o.handle(
            fence(a),
            1,
            client(
                GroupTravelClientKind::Cancel,
                GroupTravelResult::None,
                [0; 16],
            ),
        )
        .unwrap();
        assert_eq!(
            o.tracked.as_ref().unwrap().pending_action.unwrap().action,
            GroupTravelAction::Cancel
        );
    }
    #[test]
    fn duplicate_decision_and_applied_reuse_idempotency() {
        let a = cid(2);
        let mut o = GroupTravelOwner {
            tracked: Some(tracked(Role::Responder, GroupTravelProposalStatus::Pending)),
            generation: Some(1),
            ..GroupTravelOwner::default()
        };
        let d = client(
            GroupTravelClientKind::Decision,
            GroupTravelResult::Accepted,
            proposal_bytes(pid()),
        );
        o.handle(fence(a), 1, d).unwrap();
        let key = o
            .tracked
            .as_ref()
            .unwrap()
            .pending_action
            .unwrap()
            .request
            .idempotency_key;
        o.handle(fence(a), 1, d).unwrap();
        assert_eq!(
            o.tracked
                .as_ref()
                .unwrap()
                .pending_action
                .unwrap()
                .request
                .idempotency_key,
            key
        );
        o.tracked.as_mut().unwrap().status = GroupTravelProposalStatus::Committed;
        o.tracked.as_mut().unwrap().pending_action = None;
        let ap = client(
            GroupTravelClientKind::Applied,
            GroupTravelResult::Applied,
            proposal_bytes(pid()),
        );
        o.handle(fence(a), 1, ap).unwrap();
        let key = o
            .tracked
            .as_ref()
            .unwrap()
            .pending_action
            .unwrap()
            .request
            .idempotency_key;
        o.handle(fence(a), 1, ap).unwrap();
        assert_eq!(
            o.tracked
                .as_ref()
                .unwrap()
                .pending_action
                .unwrap()
                .request
                .idempotency_key,
            key
        );
    }
    #[tokio::test]
    async fn outbound_retry_and_terminal_ack() {
        let mut o = GroupTravelOwner {
            tracked: Some(tracked(Role::Responder, GroupTravelProposalStatus::Pending)),
            generation: Some(1),
            ..GroupTravelOwner::default()
        };
        o.queue_for_tracked_state();
        let GroupTravelOwnerEvent::Deliver(offer) = o.next_event().await else {
            panic!()
        };
        o.acknowledge_delivery(offer).unwrap();
        assert!(o.outbound.is_some());
        o.outbound.as_mut().unwrap().retry_at = tokio::time::Instant::now();
        let GroupTravelOwnerEvent::Deliver(retry) = o.next_event().await else {
            panic!()
        };
        assert_eq!(offer, retry);
        o.queue_terminal_reason(GroupTravelReason::Conflict);
        let abort = o.outbound.unwrap().record;
        o.acknowledge_delivery(abort).unwrap();
        assert!(o.outbound.is_none());
        assert!(o.tracked.is_none());
        let d = client(
            GroupTravelClientKind::Decision,
            GroupTravelResult::Accepted,
            proposal_bytes(pid()),
        );
        o.handle(fence(cid(2)), 1, d).unwrap();
        assert_eq!(o.outbound.unwrap().record, abort);
    }

    #[tokio::test]
    async fn held_semantic_http_does_not_block_other_select_work() {
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut o = GroupTravelOwner {
            next_poll: tokio::time::Instant::now() + Duration::from_secs(60),
            pending: Some(PendingWork {
                kind: PendingKind::Snapshot,
                future: Box::pin(async move { receive.await.unwrap() }),
            }),
            ..GroupTravelOwner::default()
        };
        tokio::select! {
            event = o.next_event() => panic!("held HTTP unexpectedly completed: {}", matches!(event, GroupTravelOwnerEvent::Complete)),
            () = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
        assert!(o.pending.is_some());
        send.send(Completion::Snapshot {
            generation: 1,
            result: Ok(None),
        })
        .unwrap();
        assert!(matches!(
            o.next_event().await,
            GroupTravelOwnerEvent::Complete
        ));
    }

    #[test]
    fn applied_completion_is_retained_until_delivery_ack() {
        let f = fence(cid(2));
        let action = PendingAction {
            action: GroupTravelAction::Applied,
            request: GroupTravelActionRequest::new(
                f,
                GroupTravelAction::Applied,
                new_idempotency_key().unwrap(),
            ),
        };
        let mut tracked = tracked(Role::Responder, GroupTravelProposalStatus::Committed);
        tracked.pending_action = Some(action);
        let mut o = GroupTravelOwner {
            tracked: Some(tracked),
            ..GroupTravelOwner::default()
        };
        o.finish(Completion::Action {
            fence: f,
            generation: 1,
            group_id: gid(),
            proposal_id: pid(),
            pending: action,
            result: Ok(proposal(GroupTravelProposalStatus::Committed)),
        })
        .unwrap();
        let complete = o.outbound.unwrap().record;
        assert_eq!(complete.kind, GroupTravelServerKind::Complete);
        o.acknowledge_delivery(complete).unwrap();
        assert!(o.tracked.is_none());
    }

    #[test]
    fn elapsed_idle_poll_discovers_group_then_current_proposal() {
        let api = api();
        let f = fence(cid(2));
        let mut o = GroupTravelOwner::default();
        assert!(o.pending.is_none());
        o.next_poll = tokio::time::Instant::now();
        o.prepare(&api, token(), f, 1);
        assert_eq!(o.pending.as_ref().unwrap().kind, PendingKind::Snapshot);
        o.pending = None;
        o.finish(Completion::Snapshot {
            generation: 1,
            result: Ok(Some(gid())),
        })
        .unwrap();
        o.prepare(&api, token(), f, 1);
        assert_eq!(o.pending.as_ref().unwrap().kind, PendingKind::Current);
    }

    #[test]
    fn cancel_during_inflight_create_merges_live_intent() {
        let f = fence(cid(1));
        let request = GroupTravelProposalRequest::new(
            f,
            route_id(GroupTravelRoute::TrainLater),
            new_idempotency_key().unwrap(),
        )
        .unwrap();
        let stale = PendingCreate {
            group_id: gid(),
            route: GroupTravelRoute::TrainLater,
            departure: GroupTravelDeparture::Train,
            endpoint: None,
            request_id: 7,
            request: request.clone(),
            cancel_requested: false,
        };
        let mut live = stale.clone();
        live.cancel_requested = true;
        let mut o = GroupTravelOwner {
            pending_create: Some(live),
            ..GroupTravelOwner::default()
        };
        o.finish(Completion::Create {
            fence: f,
            generation: 1,
            pending: stale,
            result: Ok(proposal(GroupTravelProposalStatus::Pending)),
        })
        .unwrap();
        assert_eq!(
            o.tracked.unwrap().pending_action.unwrap().action,
            GroupTravelAction::Cancel
        );
    }

    #[test]
    fn unrelated_conflict_reply_does_not_clear_tracked_proposal() {
        let mut o = GroupTravelOwner {
            tracked: Some(tracked(Role::Responder, GroupTravelProposalStatus::Pending)),
            generation: Some(1),
            ..GroupTravelOwner::default()
        };
        let unrelated = GroupTravelClientRecord {
            request_id: 99,
            ..client(
                GroupTravelClientKind::Request,
                GroupTravelResult::None,
                [0; 16],
            )
        };
        o.handle(fence(cid(2)), 1, unrelated).unwrap();
        let abort = o.outbound.unwrap().record;
        assert_eq!(abort.kind, GroupTravelServerKind::Abort);
        o.acknowledge_delivery(abort).unwrap();
        assert!(o.tracked.is_some());
    }

    #[test]
    fn terminal_zero_id_replay_is_cleared_on_generation_advance() {
        let f = fence(cid(1));
        let request = client(
            GroupTravelClientKind::Request,
            GroupTravelResult::None,
            [0; 16],
        );
        let mut o = GroupTravelOwner {
            group_id: Some(gid()),
            generation: Some(1),
            ..GroupTravelOwner::default()
        };
        o.queue_outbound(
            abort_before_create(request, GroupTravelReason::Conflict),
            DeliveryDisposition::ReplyOnly,
        );
        let abort = o.outbound.unwrap().record;
        o.acknowledge_delivery(abort).unwrap();
        assert_eq!(o.terminal.len(), 1);
        o.handle(f, 2, request).unwrap();
        assert!(o.terminal.is_empty());
        assert!(o.pending_create.is_some());
        assert_eq!(
            o.outbound.unwrap().record.kind,
            GroupTravelServerKind::Requesting
        );
    }

    #[tokio::test]
    async fn fatal_async_errors_surface_as_owner_errors() {
        for (cloud, expected) in [
            (GroupTravelError::Unauthorized, SessionError::Unauthorized),
            (GroupTravelError::InvalidResponse, SessionError::Realtime),
        ] {
            let mut o = GroupTravelOwner {
                pending: Some(PendingWork {
                    kind: PendingKind::Current,
                    future: Box::pin(async move {
                        Completion::Current {
                            fence: fence(cid(1)),
                            generation: 1,
                            result: Err(cloud),
                        }
                    }),
                }),
                ..GroupTravelOwner::default()
            };
            let GroupTravelOwnerEvent::Error(actual) = o.next_event().await else {
                panic!("fatal semantic error was swallowed")
            };
            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn semantic_retries_wait_for_poll_deadline() {
        let api = api();
        let f = fence(cid(1));
        let request = GroupTravelProposalRequest::new(
            f,
            route_id(GroupTravelRoute::TrainLater),
            new_idempotency_key().unwrap(),
        )
        .unwrap();
        let mut o = GroupTravelOwner {
            pending_create: Some(PendingCreate {
                group_id: gid(),
                route: GroupTravelRoute::TrainLater,
                departure: GroupTravelDeparture::Train,
                request_id: 7,
                endpoint: None,
                request,
                cancel_requested: false,
            }),
            next_poll: tokio::time::Instant::now() + Duration::from_secs(60),
            ..GroupTravelOwner::default()
        };
        o.prepare(&api, token(), f, 1);
        assert!(o.pending.is_none());
        o.next_poll = tokio::time::Instant::now();
        o.prepare(&api, token(), f, 1);
        assert_eq!(o.pending.as_ref().unwrap().kind, PendingKind::Create);
        o.pending = None;
        let action = PendingAction {
            action: GroupTravelAction::Cancel,
            request: GroupTravelActionRequest::new(
                f,
                GroupTravelAction::Cancel,
                new_idempotency_key().unwrap(),
            ),
        };
        let mut tracked = tracked(Role::Requester, GroupTravelProposalStatus::Pending);
        tracked.pending_action = Some(action);
        o.pending_create = None;
        o.tracked = Some(tracked);
        o.next_poll = tokio::time::Instant::now() + Duration::from_secs(60);
        o.prepare(&api, token(), f, 1);
        assert!(o.pending.is_none());
        o.next_poll = tokio::time::Instant::now();
        o.prepare(&api, token(), f, 1);
        assert_eq!(o.pending.as_ref().unwrap().kind, PendingKind::Action);
    }

    #[test]
    fn unacknowledged_complete_and_exact_replay_survive_generation_change() {
        let f = fence(cid(2));
        let action = PendingAction {
            action: GroupTravelAction::Applied,
            request: GroupTravelActionRequest::new(
                f,
                GroupTravelAction::Applied,
                new_idempotency_key().unwrap(),
            ),
        };
        let mut tracked = tracked(Role::Responder, GroupTravelProposalStatus::Committed);
        tracked.pending_action = Some(action);
        let mut o = GroupTravelOwner {
            tracked: Some(tracked),
            generation: Some(1),
            ..GroupTravelOwner::default()
        };
        o.finish(Completion::Action {
            fence: f,
            generation: 1,
            group_id: gid(),
            proposal_id: pid(),
            pending: action,
            result: Ok(proposal(GroupTravelProposalStatus::Committed)),
        })
        .unwrap();
        let complete = o.outbound.unwrap().record;
        o.enter_generation(2);
        assert_eq!(o.outbound.unwrap().record, complete);
        o.acknowledge_delivery(complete).unwrap();
        assert!(o.tracked.is_none());
        let repeated = GroupTravelClientRecord {
            request_id: 99,
            ..client(
                GroupTravelClientKind::Applied,
                GroupTravelResult::Applied,
                proposal_bytes(pid()),
            )
        };
        o.handle(f, 3, repeated).unwrap();
        assert_eq!(o.outbound.unwrap().record, complete);
    }

    #[test]
    fn stale_generation_successes_reconcile_create_and_applied() {
        let requester = fence(cid(1));
        let create_request = GroupTravelProposalRequest::new(
            requester,
            route_id(GroupTravelRoute::TrainLater),
            new_idempotency_key().unwrap(),
        )
        .unwrap();
        let create = PendingCreate {
            group_id: gid(),
            route: GroupTravelRoute::TrainLater,
            departure: GroupTravelDeparture::Train,
            request_id: 7,
            endpoint: None,
            request: create_request,
            cancel_requested: false,
        };
        let mut created = GroupTravelOwner {
            pending_create: Some(create.clone()),
            generation: Some(2),
            ..GroupTravelOwner::default()
        };
        created
            .finish(Completion::Create {
                fence: requester,
                generation: 1,
                pending: create,
                result: Ok(proposal(GroupTravelProposalStatus::Pending)),
            })
            .unwrap();
        assert_eq!(created.tracked.as_ref().unwrap().proposal_id, pid());
        assert_eq!(
            created.outbound.unwrap().record.kind,
            GroupTravelServerKind::Requesting
        );

        let responder = fence(cid(2));
        let action = PendingAction {
            action: GroupTravelAction::Applied,
            request: GroupTravelActionRequest::new(
                responder,
                GroupTravelAction::Applied,
                new_idempotency_key().unwrap(),
            ),
        };
        let mut state = tracked(Role::Responder, GroupTravelProposalStatus::Committed);
        state.pending_action = Some(action);
        let mut applied = GroupTravelOwner {
            tracked: Some(state),
            generation: Some(2),
            ..GroupTravelOwner::default()
        };
        applied
            .finish(Completion::Action {
                fence: responder,
                generation: 1,
                group_id: gid(),
                proposal_id: pid(),
                pending: action,
                result: Ok(proposal(GroupTravelProposalStatus::Committed)),
            })
            .unwrap();
        assert_eq!(
            applied.outbound.unwrap().record.kind,
            GroupTravelServerKind::Complete
        );
    }

    #[test]
    fn malformed_and_wrong_proposal_views_fail_closed() {
        let f = fence(cid(2));
        let mut malformed = proposal(GroupTravelProposalStatus::Pending);
        malformed.expected_members.swap(0, 1);
        let mut o = GroupTravelOwner::default();
        assert!(matches!(
            o.finish_view_result(Ok(Some(malformed)), f, 1, None),
            Err(SessionError::Realtime)
        ));

        let mut wrong = proposal(GroupTravelProposalStatus::Pending);
        wrong.proposal_id = GroupTravelProposalId::new(Uuid::from_u128(41)).unwrap();
        let mut current = GroupTravelOwner {
            tracked: Some(tracked(Role::Responder, GroupTravelProposalStatus::Pending)),
            ..GroupTravelOwner::default()
        };
        assert!(matches!(
            current.finish_view_result(Ok(Some(wrong.clone())), f, 1, None),
            Err(SessionError::Realtime)
        ));
        assert!(matches!(
            current.finish_view_result(Ok(Some(wrong)), f, 1, Some(pid())),
            Err(SessionError::Realtime)
        ));
    }

    #[test]
    fn story_recovery_response_requires_own_marked_attempt() {
        let own = cid(1);
        let view = StoryTravelRecoveryView {
            api_version: ApiVersion::V1,
            proposal_id: pid(),
            group_id: gid(),
            status: GroupTravelProposalStatus::Suspended,
            marker_fence: fence(own),
            scene_nonce: 7,
            marked_at: coop_cloud::UnixTimestampMillis::new(100),
        };
        assert_eq!(validate_story_recovery(view.clone(), own), Ok(view.clone()));
        assert_eq!(
            validate_story_recovery(view.clone(), cid(2)),
            Err(GroupTravelError::InvalidResponse)
        );
        let mut invalid = view.clone();
        invalid.scene_nonce = 0;
        assert_eq!(
            validate_story_recovery(invalid, own),
            Err(GroupTravelError::InvalidResponse)
        );
        let mut invalid = view;
        invalid.status = GroupTravelProposalStatus::Committed;
        assert_eq!(
            validate_story_recovery(invalid, own),
            Err(GroupTravelError::InvalidResponse)
        );
    }
    #[test]
    fn first_voyage_marker_waits_for_server_ack_and_completes_after_both_receipts() {
        let own = cid(1);
        let f = fence(own);
        let mut awaiting = proposal(GroupTravelProposalStatus::AwaitingSceneReceipts);
        awaiting.route_id =
            RouteId::new(route_id(GroupTravelRoute::FerryBrineyHouseDewford)).unwrap();
        awaiting.departure = GroupTravelDeparture::Ferry;
        let mut owner = GroupTravelOwner::default();
        owner.observe_view(&awaiting, f, 1).unwrap();
        assert_eq!(
            owner.outbound.unwrap().record.kind,
            GroupTravelServerKind::SceneReady
        );
        let marker = GroupTravelClientRecord {
            kind: GroupTravelClientKind::SceneMarkerRequest,
            route: GroupTravelRoute::FerryBrineyHouseDewford,
            departure: GroupTravelDeparture::Ferry,
            request_id: request_id_from_proposal(pid()),
            proposal_id: proposal_bytes(pid()),
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        };
        owner.handle(f, 1, marker).unwrap();
        assert!(owner.tracked.as_ref().unwrap().pending_marker);
        assert_eq!(
            owner.outbound.unwrap().record.kind,
            GroupTravelServerKind::SceneReady
        );
        let mut accepted = awaiting.clone();
        accepted.scene_marked_by[0] = true;
        owner
            .finish(Completion::Marker {
                fence: f,
                generation: 1,
                group_id: gid(),
                proposal_id: pid(),
                result: Ok(accepted),
            })
            .unwrap();
        assert_eq!(
            owner.outbound.unwrap().record.kind,
            GroupTravelServerKind::SceneMarkerAccepted
        );
        let complete = GroupTravelClientRecord {
            kind: GroupTravelClientKind::SceneComplete,
            ..marker
        };
        owner.handle(f, 1, complete).unwrap();
        assert!(owner.tracked.as_ref().unwrap().scene_complete);
        let mut unrelated = complete;
        unrelated.proposal_id = [8; 16];
        assert!(matches!(
            owner.handle(f, 1, unrelated),
            Err(SessionError::Realtime)
        ));
        let mut committed = awaiting;
        committed.status = GroupTravelProposalStatus::Committed;
        committed.commit = proposal(GroupTravelProposalStatus::Committed).commit;
        assert!(matches!(
            owner.observe_view(&committed, f, 1),
            Err(SessionError::Realtime)
        ));
        committed.scene_marked_by = [true, true];
        committed.scene_receipted_by = [true, true];
        owner.observe_view(&committed, f, 1).unwrap();
        let final_record = owner.outbound.unwrap().record;
        assert_eq!(final_record.kind, GroupTravelServerKind::Complete);
        assert_eq!(
            final_record.route,
            GroupTravelRoute::FerryBrineyHouseDewford
        );
        assert_eq!(final_record.result, GroupTravelResult::Applied);
        owner.acknowledge_delivery(final_record).unwrap();
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn bill_story_routes_wait_for_exact_scene_and_two_receipts() {
        let own = cid(1);
        let f = fence(own);
        for route in [
            GroupTravelRoute::SeagallopBillCinnabarOne,
            GroupTravelRoute::SeagallopBillOneCinnabar,
        ] {
            let mut awaiting = proposal(GroupTravelProposalStatus::AwaitingSceneReceipts);
            awaiting.route_id = RouteId::new(route_id(route)).unwrap();
            awaiting.departure = GroupTravelDeparture::Ferry;
            let mut owner = GroupTravelOwner::default();
            owner.observe_view(&awaiting, f, 1).unwrap();
            let marker = GroupTravelClientRecord {
                kind: GroupTravelClientKind::SceneMarkerRequest,
                route,
                departure: GroupTravelDeparture::Ferry,
                request_id: request_id_from_proposal(pid()),
                proposal_id: proposal_bytes(pid()),
                result: GroupTravelResult::None,
                reason: GroupTravelReason::None,
                endpoint: None,
            };
            owner.handle(f, 1, marker).unwrap();
            let mut accepted = awaiting.clone();
            accepted.scene_marked_by[0] = true;
            owner
                .finish(Completion::Marker {
                    fence: f,
                    generation: 1,
                    group_id: gid(),
                    proposal_id: pid(),
                    result: Ok(accepted),
                })
                .unwrap();
            assert!(owner.story_checkpoint_token(f).is_none());
            let complete = GroupTravelClientRecord {
                kind: GroupTravelClientKind::SceneComplete,
                ..marker
            };
            owner.handle(f, 1, complete).unwrap();
            assert!(owner.story_checkpoint_token(f).is_some());
            let mut committed = awaiting;
            committed.status = GroupTravelProposalStatus::Committed;
            committed.commit = proposal(GroupTravelProposalStatus::Committed).commit;
            committed.scene_marked_by = [true, true];
            committed.scene_receipted_by = [true, true];
            owner.observe_view(&committed, f, 1).unwrap();
            assert_eq!(
                owner.outbound.unwrap().record.kind,
                GroupTravelServerKind::Complete
            );
        }
    }

    #[test]
    fn first_voyage_receipt_names_only_the_checkpoint_started_after_scene_completion() {
        let own = cid(1);
        let marker_fence = fence(own);
        let mut tracked = tracked(
            Role::Requester,
            GroupTravelProposalStatus::AwaitingSceneReceipts,
        );
        tracked.route = GroupTravelRoute::FerryBrineyHouseDewford;
        tracked.departure = GroupTravelDeparture::Ferry;
        tracked.request_id = request_id_from_proposal(pid());
        tracked.marker_requested = true;
        tracked.marker_accepted = true;
        tracked.marker_fence = Some(marker_fence);
        let mut owner = GroupTravelOwner::default();
        owner.tracked = Some(tracked);
        owner.enter_generation(1);
        assert!(owner.story_checkpoint_token(marker_fence).is_none());
        let complete = GroupTravelClientRecord {
            kind: GroupTravelClientKind::SceneComplete,
            route: GroupTravelRoute::FerryBrineyHouseDewford,
            departure: GroupTravelDeparture::Ferry,
            request_id: request_id_from_proposal(pid()),
            proposal_id: proposal_bytes(pid()),
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        };
        owner.handle(marker_fence, 1, complete).unwrap();
        let token = owner.story_checkpoint_token(marker_fence).unwrap();
        let mut finalized_fence = marker_fence;
        finalized_fence.current_revision = Revision::new(8);
        let snapshot = SnapshotRecord {
            api_version: ApiVersion::V1,
            snapshot_id: coop_cloud::SnapshotId::new(Uuid::from_u128(70)).unwrap(),
            session_id: marker_fence.session_id,
            character_id: own,
            parent_revision: marker_fence.current_revision,
            revision: finalized_fence.current_revision,
            session_epoch: marker_fence.session_epoch,
            files: Vec::new(),
            pending_commits_sha256: coop_cloud::Sha256Digest::of_bytes(b"test"),
            last_applied_commit: None,
            created_at: coop_cloud::UnixTimestampMillis::new(1),
        };
        let mut old = snapshot.clone();
        old.parent_revision = Revision::new(6);
        assert!(
            owner
                .story_checkpoint_finalized(token, &old, finalized_fence)
                .is_err()
        );
        owner
            .story_checkpoint_finalized(token, &snapshot, finalized_fence)
            .unwrap();
        let request = owner
            .tracked
            .as_ref()
            .unwrap()
            .pending_receipt
            .clone()
            .unwrap();
        assert_eq!(request.snapshot_id, snapshot.snapshot_id);
        assert_eq!(request.current_revision, snapshot.revision);
        assert!(owner.story_checkpoint_token(finalized_fence).is_none());
        owner
            .finish(Completion::Receipt {
                fence: finalized_fence,
                generation: 1,
                group_id: gid(),
                proposal_id: pid(),
                request: request.clone(),
                result: Err(GroupTravelError::Unavailable),
            })
            .unwrap();
        assert_eq!(
            owner.tracked.as_ref().unwrap().pending_receipt,
            Some(request.clone())
        );
        owner.enter_generation(2);
        assert!(owner.handle(finalized_fence, 2, complete).is_ok());
        let mut accepted = proposal(GroupTravelProposalStatus::AwaitingSceneReceipts);
        accepted.route_id =
            RouteId::new(route_id(GroupTravelRoute::FerryBrineyHouseDewford)).unwrap();
        accepted.departure = GroupTravelDeparture::Ferry;
        accepted.scene_marked_by[0] = true;
        accepted.scene_receipted_by[0] = true;
        owner
            .finish(Completion::Receipt {
                fence: finalized_fence,
                generation: 2,
                group_id: gid(),
                proposal_id: pid(),
                request: request.clone(),
                result: Ok(accepted),
            })
            .unwrap();
        assert!(owner.tracked.as_ref().unwrap().receipt_accepted);
        assert!(owner.tracked.as_ref().unwrap().pending_receipt.is_none());
    }

    #[test]
    fn first_voyage_marker_rejects_unproven_response() {
        let own = cid(1);
        let f = fence(own);
        let mut awaiting = proposal(GroupTravelProposalStatus::AwaitingSceneReceipts);
        awaiting.route_id =
            RouteId::new(route_id(GroupTravelRoute::FerryBrineyHouseDewford)).unwrap();
        awaiting.departure = GroupTravelDeparture::Ferry;
        let mut owner = GroupTravelOwner::default();
        owner.observe_view(&awaiting, f, 1).unwrap();
        owner
            .handle(
                f,
                1,
                GroupTravelClientRecord {
                    kind: GroupTravelClientKind::SceneMarkerRequest,
                    route: GroupTravelRoute::FerryBrineyHouseDewford,
                    departure: GroupTravelDeparture::Ferry,
                    request_id: request_id_from_proposal(pid()),
                    proposal_id: proposal_bytes(pid()),
                    result: GroupTravelResult::None,
                    reason: GroupTravelReason::None,
                    endpoint: None,
                },
            )
            .unwrap();
        assert!(matches!(
            owner.finish(Completion::Marker {
                fence: f,
                generation: 1,
                group_id: gid(),
                proposal_id: pid(),
                result: Ok(awaiting),
            }),
            Err(SessionError::Realtime)
        ));
    }
    #[test]
    fn first_voyage_request_enters_normal_group_consent() {
        let mut owner = GroupTravelOwner::default();
        let record = GroupTravelClientRecord {
            kind: GroupTravelClientKind::Request,
            route: GroupTravelRoute::FerryBrineyHouseDewford,
            departure: GroupTravelDeparture::Ferry,
            request_id: 7,
            proposal_id: [0; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        };
        owner.handle(fence(cid(1)), 1, record).unwrap();
        assert_eq!(
            owner.outbound.unwrap().record.kind,
            GroupTravelServerKind::Requesting
        );
        assert!(owner.pending_request.is_some());
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn dynamic_view_must_agree_with_endpoint_and_commit() {
        let cave = WorldZone::new(RegionId::Hoenn, "GRANITE_CAVE_1F", 1).unwrap();
        let exit = WorldZone::new(RegionId::Hoenn, "ROUTE106", 1).unwrap();
        let (c, r) = (cave.map_entry().unwrap(), exit.map_entry().unwrap());
        let endpoint = GroupTravelEndpoint::new(
            u8::try_from(c.map_group).unwrap(),
            u8::try_from(c.map_number).unwrap(),
            u8::try_from(r.map_group).unwrap(),
            u8::try_from(r.map_number).unwrap(),
            48,
            17,
        );
        let mut view = proposal(GroupTravelProposalStatus::Committed);
        view.route_id = RouteId::new(route_id(GroupTravelRoute::EscapeRope)).unwrap();
        view.departure = GroupTravelDeparture::EscapeRope;
        view.source = cave;
        view.destination = exit.clone();
        view.endpoint = Some(endpoint);
        let commit = view.commit.as_mut().unwrap();
        commit.destination = exit;
        commit.endpoint = Some(endpoint);
        let f = fence(cid(1));
        assert!(validate_view(&view, gid(), f).is_ok());

        let mut wrong_destination = view.clone();
        wrong_destination.destination =
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        assert!(validate_view(&wrong_destination, gid(), f).is_err());

        let mut wrong_commit = view.clone();
        wrong_commit.commit.as_mut().unwrap().endpoint = Some(GroupTravelEndpoint {
            target_x: 47,
            ..endpoint
        });
        assert!(validate_view(&wrong_commit, gid(), f).is_err());

        let mut commit_elsewhere = view.clone();
        commit_elsewhere.commit.as_mut().unwrap().destination =
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        assert!(validate_view(&commit_elsewhere, gid(), f).is_err());

        let mut missing = view;
        missing.endpoint = None;
        assert!(validate_view(&missing, gid(), f).is_err());
    }

    #[test]
    fn dynamic_endpoint_is_preserved_in_pending_and_abort_records() {
        let endpoint = GroupTravelEndpoint::new(0, 9, 0, 10, 4, 6);
        let record = GroupTravelClientRecord {
            kind: GroupTravelClientKind::Request,
            route: GroupTravelRoute::Dig,
            departure: GroupTravelDeparture::Dig,
            request_id: 7,
            proposal_id: [0; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: Some(endpoint),
        };
        let mut owner = GroupTravelOwner::default();
        owner.handle(fence(cid(1)), 1, record).unwrap();
        assert_eq!(owner.outbound.unwrap().record.endpoint, Some(endpoint));

        let mut changed = record;
        changed.endpoint = Some(GroupTravelEndpoint::new(0, 9, 0, 11, 4, 6));
        owner.handle(fence(cid(1)), 1, changed).unwrap();
        let abort = owner.outbound.unwrap().record;
        assert_eq!(abort.kind, GroupTravelServerKind::Abort);
        assert_eq!(abort.endpoint, changed.endpoint);
    }

    #[test]
    fn story_recovery_action_response_matches_request() {
        let reconciled = StoryTravelRecoveryResolutionView {
            api_version: ApiVersion::V1,
            proposal_id: pid(),
            outcome: StoryTravelRecoveryOutcome::Reconciled,
        };
        assert_eq!(
            validate_story_recovery_resolution(
                reconciled.clone(),
                pid(),
                StoryTravelRecoveryAction::Reconcile
            ),
            Ok(reconciled.clone())
        );
        assert_eq!(
            validate_story_recovery_resolution(
                reconciled.clone(),
                pid(),
                StoryTravelRecoveryAction::Abandon
            ),
            Err(GroupTravelError::InvalidResponse)
        );
        assert_eq!(
            validate_story_recovery_resolution(
                reconciled,
                GroupTravelProposalId::new(Uuid::from_u128(41)).unwrap(),
                StoryTravelRecoveryAction::Reconcile
            ),
            Err(GroupTravelError::InvalidResponse)
        );
    }

    #[test]
    fn story_receipt_statuses_do_not_create_ordinary_travel_state() {
        let f = fence(cid(2));
        for status in [
            GroupTravelProposalStatus::AwaitingSceneReceipts,
            GroupTravelProposalStatus::Suspended,
        ] {
            let mut owner = GroupTravelOwner::default();
            assert!(matches!(
                owner.observe_view(&proposal(status), f, 1),
                Err(SessionError::Realtime)
            ));
            assert!(owner.tracked.is_none());
            assert!(owner.outbound.is_none());
        }
    }

    #[test]
    fn vote_seconds_follow_server_clock_even_when_local_clock_differs() {
        let mut view = proposal(GroupTravelProposalStatus::Pending);
        view.expires_at = coop_cloud::UnixTimestampMillis::new(1_000_030_000);
        view.server_now = Some(coop_cloud::UnixTimestampMillis::new(1_000_001_000));
        let decoded: GroupTravelProposalView =
            serde_json::from_slice(&serde_json::to_vec(&view).unwrap()).unwrap();
        assert_eq!(decoded.server_now, view.server_now);
        let mut owner = GroupTravelOwner::default();
        owner
            .observe_view(&view, fence(cid(2)), 1)
            .expect("valid offer");
        let record = owner.outbound.expect("offer record").record;
        assert_eq!(record.kind, GroupTravelServerKind::Offer);
        assert!((28..=29).contains(&record.remaining_seconds));
        assert_eq!(record.encode().unwrap()[28], record.remaining_seconds);

        view.server_now = Some(view.expires_at);
        owner
            .observe_view(&view, fence(cid(2)), 1)
            .expect("expired update");
        assert_eq!(owner.outbound.unwrap().record.remaining_seconds, 0);
    }
}

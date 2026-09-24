//! Consent-gated two-member travel orchestration and strict HTTP transport.

use std::{future::Future, pin::Pin, time::Duration};

use coop_cloud::{
    AccessToken, ApiVersion, GroupId, GroupTravelAction, GroupTravelActionRequest,
    GroupTravelProposalId, GroupTravelProposalRequest, GroupTravelProposalStatus,
    GroupTravelProposalView, IdempotencyKey, LeaseFence, OnlineSnapshotRequest,
};
use coop_protocol::{
    GroupTravelClientKind, GroupTravelClientRecord, GroupTravelDeparture, GroupTravelReason,
    GroupTravelResult, GroupTravelRoute, GroupTravelServerKind, GroupTravelServerRecord,
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
    request_id: u32,
    role: Role,
    status: GroupTravelProposalStatus,
    pending_action: Option<PendingAction>,
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
}
impl Completion {
    const fn generation(&self) -> u32 {
        match self {
            Self::Snapshot { generation, .. }
            | Self::Current { generation, .. }
            | Self::Get { generation, .. }
            | Self::Create { generation, .. }
            | Self::Action { generation, .. } => *generation,
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
            {
                self.queue_outbound(
                    requesting_record(r.route, r.departure, r.request_id),
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
            if p.route == r.route && p.departure == r.departure && p.request_id == r.request_id {
                self.queue_outbound(
                    requesting_record(r.route, r.departure, r.request_id),
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
                requesting_record(r.route, r.departure, r.request_id),
                DeliveryDisposition::Retain,
            );
            self.next_poll = tokio::time::Instant::now();
            return Ok(());
        };
        let request = GroupTravelProposalRequest::new_with_departure(
            fence,
            route_id(r.route),
            r.departure,
            new_idempotency_key()?,
        )
        .map_err(|_| SessionError::Realtime)?;
        self.pending_create = Some(PendingCreate {
            group_id,
            route: r.route,
            departure: r.departure,
            request_id: r.request_id,
            request,
            cancel_requested: false,
        });
        self.next_poll = tokio::time::Instant::now();
        self.queue_outbound(
            requesting_record(r.route, r.departure, r.request_id),
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
        {
            pending.cancel_requested = true;
            return Ok(());
        }
        if let Some(p) = &mut self.pending_create
            && p.route == r.route
            && p.departure == r.departure
            && p.request_id == r.request_id
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
        {
            return Err(SessionError::Realtime);
        }
        self.tracked = Some(TrackedProposal {
            group_id: p.group_id,
            proposal_id: v.proposal_id,
            route: p.route,
            departure: p.departure,
            request_id: p.request_id,
            role: Role::Requester,
            status: v.status,
            pending_action: None,
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
                request_id: request_id_from_proposal(v.proposal_id),
                role,
                status: v.status,
                pending_action: None,
            });
        }
        let Some(t) = &mut self.tracked else {
            return Err(SessionError::Realtime);
        };
        if t.group_id != v.group_id
            || t.proposal_id != v.proposal_id
            || t.route != route
            || t.departure != v.departure
            || t.role != role
        {
            return Err(SessionError::Realtime);
        }
        t.status = v.status;
        match v.status {
            GroupTravelProposalStatus::Pending | GroupTravelProposalStatus::Committed => {
                self.queue_for_tracked_state();
            }
            GroupTravelProposalStatus::Declined => {
                self.queue_terminal_reason(GroupTravelReason::ParticipantDeclined);
            }
            GroupTravelProposalStatus::Cancelled => {
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
        let kind = match t.status {
            GroupTravelProposalStatus::Pending if t.role == Role::Requester => {
                GroupTravelServerKind::Requesting
            }
            GroupTravelProposalStatus::Pending => GroupTravelServerKind::Offer,
            GroupTravelProposalStatus::Committed => GroupTravelServerKind::Commit,
            _ => return,
        };
        let record = if kind == GroupTravelServerKind::Requesting {
            requesting_record(t.route, t.departure, t.request_id)
        } else {
            server_record(kind, t, GroupTravelResult::None, GroupTravelReason::None)
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
            (t.route == r.route && t.departure == r.departure && (exact_proposal || scoped_zero))
                .then_some((t.record, t.disposition))
        })
    }
}
fn record_matches(t: &TrackedProposal, r: GroupTravelClientRecord, zero: bool) -> bool {
    t.route == r.route
        && t.departure == r.departure
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
    Ok(())
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
fn requesting_record(
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    request_id: u32,
) -> GroupTravelServerRecord {
    GroupTravelServerRecord {
        kind: GroupTravelServerKind::Requesting,
        route,
        departure,
        request_id,
        proposal_id: [0; 16],
        result: GroupTravelResult::None,
        reason: GroupTravelReason::None,
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
    }
}
const fn route_id(r: GroupTravelRoute) -> &'static str {
    match r {
        GroupTravelRoute::TrainOriginal => "JOHTO:GOLDENROD_KANTO_ORIGINAL_TRAIN",
        GroupTravelRoute::TrainLater => "JOHTO:GOLDENROD_KANTO_LATER_TRAIN",
        GroupTravelRoute::FerryOriginal => "JOHTO:OLIVINE_KANTO_ORIGINAL_FERRY",
        GroupTravelRoute::FerryLater => "JOHTO:OLIVINE_KANTO_LATER_FERRY",
        GroupTravelRoute::GateOriginal => "KANTO:RECEPTION_GATE_KANTO_ORIGINAL_ROUTE22",
        GroupTravelRoute::GateLater => "KANTO:RECEPTION_GATE_KANTO_LATER_ROUTE22",
    }
}
fn route_from_id(id: &str) -> Option<GroupTravelRoute> {
    [
        GroupTravelRoute::TrainOriginal,
        GroupTravelRoute::TrainLater,
        GroupTravelRoute::FerryOriginal,
        GroupTravelRoute::FerryLater,
        GroupTravelRoute::GateOriginal,
        GroupTravelRoute::GateLater,
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
            }),
            applied_by: [false, false],
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
            request_id: 7,
            role,
            status,
            pending_action: None,
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
}

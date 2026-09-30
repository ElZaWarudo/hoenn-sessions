//! Requests a host UI sends into a running session.
//!
//! While the game runs, the session owns the only authorized token and lease
//! fence. A host such as the desktop app therefore asks the session to make
//! these calls instead of holding credentials of its own. Each request is
//! answered on its own one-shot channel; the session never fails because a
//! host request failed.
use std::{future::Future, pin::Pin, time::Duration};

use coop_cloud::{AccessToken, ApiVersion, LeaseFence, PairingCode, PartnerStatusResponse};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::{CloudApi, online::OnlineError};

/// Capacity of the host-to-session request queue.
pub const LIVE_REQUEST_CAPACITY: usize = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LiveRequestError {
    /// The server or network is temporarily unavailable, or another request
    /// is already in flight.
    #[error("the service is unavailable")]
    Unavailable,
    /// The code is unknown, expired, consumed, or the caller is not eligible.
    #[error("the request was refused")]
    Refused,
    /// The session ended before answering.
    #[error("the session is not running")]
    NotRunning,
}

/// One host request with its reply channel.
#[derive(Debug)]
pub enum LiveRequest {
    PartnerStatus(oneshot::Sender<Result<PartnerStatusResponse, LiveRequestError>>),
    RedeemPairingCode {
        code: PairingCode,
        reply: oneshot::Sender<Result<(), LiveRequestError>>,
    },
}

/// Creates the host sender and the receiver handed to the session.
#[must_use]
pub fn live_request_channel() -> (mpsc::Sender<LiveRequest>, mpsc::Receiver<LiveRequest>) {
    mpsc::channel(LIVE_REQUEST_CAPACITY)
}

pub(crate) enum LiveStep {
    Request(LiveRequest),
    Finished,
}

/// Serves at most one host request at a time inside the session loop.
pub(crate) struct LiveRequestOwner<'a> {
    requests: Option<mpsc::Receiver<LiveRequest>>,
    pending: Option<Pin<Box<dyn Future<Output = ()> + Send + 'a>>>,
}

impl<'a> LiveRequestOwner<'a> {
    pub(crate) fn new(requests: Option<mpsc::Receiver<LiveRequest>>) -> Self {
        Self {
            requests,
            pending: None,
        }
    }

    /// Returns the receiver so a later run of the same session can reuse it.
    pub(crate) fn into_receiver(self) -> Option<mpsc::Receiver<LiveRequest>> {
        self.requests
    }

    /// Cancel-safe: the in-flight call stays owned here and receiving from
    /// the channel does not lose a request when another branch wins.
    pub(crate) async fn next(&mut self) -> LiveStep {
        if let Some(pending) = &mut self.pending {
            pending.await;
            self.pending = None;
            return LiveStep::Finished;
        }
        loop {
            let Some(requests) = &mut self.requests else {
                return std::future::pending().await;
            };
            match requests.recv().await {
                Some(request) => return LiveStep::Request(request),
                None => self.requests = None,
            }
        }
    }

    pub(crate) fn start<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        request: LiveRequest,
    ) {
        if self.pending.is_some() {
            reply_unavailable(request);
            return;
        }
        self.pending = Some(Box::pin(async move {
            match request {
                LiveRequest::PartnerStatus(reply) => {
                    let result = tokio::time::timeout(REQUEST_TIMEOUT, api.partner_status(token))
                        .await
                        .unwrap_or(Err(OnlineError::Unavailable))
                        .map_err(live_error)
                        .and_then(|response| {
                            if response.api_version == ApiVersion::V1 {
                                Ok(response)
                            } else {
                                Err(LiveRequestError::Unavailable)
                            }
                        });
                    let _ = reply.send(result);
                }
                LiveRequest::RedeemPairingCode { code, reply } => {
                    let result = tokio::time::timeout(
                        REQUEST_TIMEOUT,
                        api.pairing_redeem(
                            token,
                            coop_cloud::RedeemPairingCodeRequest::new(fence, code),
                        ),
                    )
                    .await
                    .unwrap_or(Err(OnlineError::Unavailable))
                    .map_err(live_error)
                    .and_then(|response| {
                        if response.api_version == ApiVersion::V1 {
                            Ok(())
                        } else {
                            Err(LiveRequestError::Unavailable)
                        }
                    });
                    let _ = reply.send(result);
                }
            }
        }));
    }
}

pub(crate) fn reply_unavailable(request: LiveRequest) {
    match request {
        LiveRequest::PartnerStatus(reply) => {
            let _ = reply.send(Err(LiveRequestError::Unavailable));
        }
        LiveRequest::RedeemPairingCode { reply, .. } => {
            let _ = reply.send(Err(LiveRequestError::Unavailable));
        }
    }
}

const fn live_error(error: OnlineError) -> LiveRequestError {
    match error {
        OnlineError::Stale => LiveRequestError::Refused,
        OnlineError::Unavailable | OnlineError::Unauthorized | OnlineError::InvalidResponse => {
            LiveRequestError::Unavailable
        }
    }
}

//! In-game trade offers, launcher side (game protocol 4).
//!
//! The ROM asks for a trade with `TradeOfferRequest` after its own
//! checkpoint, so the server's snapshot head holds the party the player
//! picked from. The launcher creates an *open* server offer (the partner
//! picks its own Pokémon when it accepts) and reports each step back as a
//! `TradeOfferStatus`. The partner's launcher polls the group's current
//! offer on a short cadence, shows it to its ROM as `TradeOfferReceived`, and
//! turns the ROM's `TradeOfferDecision` (sent after that ROM's checkpoint)
//! into the server decision. An accepted offer issues both ledger entries on
//! the server; the existing ledger delivery sends `TradeCommit` to each ROM.
//!
//! The server is the only authority. This owner keeps at most one offer per
//! role, never retries a decision with a different key, and on a ROM
//! restart withdraws an offer its ROM stopped waiting for.

use std::{collections::VecDeque, future::Future, pin::Pin, time::Duration};

use coop_cloud::{
    AccessToken, CharacterId, GroupId, LeaseFence, TradeDecisionRequest, TradeOfferCurrentView,
    TradeOfferId, TradeOfferRequest, TradeOfferStatus, TradeOfferView,
};
use coop_protocol::{
    TradeOfferOutcome, TradeOfferReceivedRecord, TradeOfferRole, TradeOfferStatusRecord,
};
use reqwest::{Method, StatusCode};
use thiserror::Error;
use tokio::time::Instant;

use crate::ReqwestCloudApi;

const RESPONSE_MAX_BYTES: usize = 16 * 1024;
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a waiting requester asks for its offer's outcome.
pub(crate) const REQUESTER_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// How often an idle grouped member looks for a partner's offer. The server
/// keeps an offer open for 60 s; the partner still needs a prompt, a party
/// selection and a checkpoint inside that window.
pub(crate) const RESPONDER_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// How long a resolved group is trusted before it is looked up again, and
/// the poll delay while this member is not grouped.
pub(crate) const GROUP_REFRESH_INTERVAL: Duration = Duration::from_secs(15);
/// Offers already shown to the ROM are never shown again.
const SEEN_OFFERS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TradeOfferError {
    #[error("the trade service is temporarily unavailable")]
    Unavailable,
    #[error("trade authorization failed")]
    Unauthorized,
    #[error("no such group or offer")]
    NotFound,
    #[error("the trade conflicts with the current state")]
    Conflict,
    #[error("an offered Pokémon holds mail")]
    Mail,
    #[error("the trade offer expired")]
    Expired,
    #[error("the partner cannot trade right now")]
    Forbidden,
    #[error("the trade service is busy")]
    Busy,
    #[error("the trade response is invalid")]
    Invalid,
}

pub type TradeOfferFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, TradeOfferError>> + Send + 'a>>;

/// Maps a non-success response. The server names mail refusals and expiry
/// in its error body; both share status codes with other failures.
fn map_status(status: StatusCode, code: Option<&str>) -> TradeOfferError {
    match (status, code) {
        (StatusCode::CONFLICT, Some("trade_pokemon_holds_mail")) => TradeOfferError::Mail,
        (StatusCode::UNAUTHORIZED, Some("expired")) => TradeOfferError::Expired,
        (StatusCode::UNAUTHORIZED, _) => TradeOfferError::Unauthorized,
        (StatusCode::CONFLICT, _) => TradeOfferError::Conflict,
        (StatusCode::NOT_FOUND, _) => TradeOfferError::NotFound,
        (StatusCode::FORBIDDEN, _) => TradeOfferError::Forbidden,
        (StatusCode::SERVICE_UNAVAILABLE | StatusCode::TOO_MANY_REQUESTS, _) => {
            TradeOfferError::Busy
        }
        (status, _) if status.is_server_error() => TradeOfferError::Unavailable,
        _ => TradeOfferError::Invalid,
    }
}

fn error_code(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get("error")?.get("code")?.as_str().map(str::to_owned)
}

impl ReqwestCloudApi {
    async fn trade_send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        not_found_is_none: bool,
    ) -> Result<Option<T>, TradeOfferError> {
        let response = request
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| TradeOfferError::Unavailable)?;
        let status = response.status();
        let body = crate::bounded_body(response, RESPONSE_MAX_BYTES)
            .await
            .map_err(|_| TradeOfferError::Unavailable)?;
        if status.is_success() {
            return serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| TradeOfferError::Invalid);
        }
        if not_found_is_none && status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Err(map_status(status, error_code(&body).as_deref()))
    }

    fn trade_get(
        &self,
        token: &AccessToken,
        path: &str,
        fence: LeaseFence,
    ) -> Result<reqwest::RequestBuilder, TradeOfferError> {
        let url = self.url(path).map_err(|_| TradeOfferError::Invalid)?;
        Ok(self
            .client
            .request(Method::GET, url)
            .bearer_auth(token.expose_secret())
            .header("X-Coop-Session-Id", fence.session_id.to_string())
            .header(
                "X-Coop-Session-Epoch",
                fence.session_epoch.value().to_string(),
            )
            .header(
                "X-Coop-Client-Instance-Id",
                fence.client_instance_id.to_string(),
            ))
    }

    /// `POST /v1/groups/{group_id}/trade-offers`.
    pub(crate) fn trade_offer_create_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        request: TradeOfferRequest,
    ) -> TradeOfferFuture<'_, TradeOfferView> {
        Box::pin(async move {
            let url = self
                .url(&format!("v1/groups/{group_id}/trade-offers"))
                .map_err(|_| TradeOfferError::Invalid)?;
            let view: TradeOfferView = self
                .trade_send(
                    self.client
                        .request(Method::POST, url)
                        .bearer_auth(token.expose_secret())
                        .json(&request),
                    false,
                )
                .await?
                .ok_or(TradeOfferError::Invalid)?;
            if view.group_id != group_id || view.initiator != request.fence.character_id {
                return Err(TradeOfferError::Invalid);
            }
            Ok(view)
        })
    }

    /// `GET /v1/groups/{group_id}/trade-offers/{offer_id}`.
    pub(crate) fn trade_offer_get_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        offer_id: TradeOfferId,
        fence: LeaseFence,
    ) -> TradeOfferFuture<'_, TradeOfferView> {
        Box::pin(async move {
            let request = self.trade_get(
                &token,
                &format!("v1/groups/{group_id}/trade-offers/{offer_id}"),
                fence,
            )?;
            let view: TradeOfferView = self
                .trade_send(request, false)
                .await?
                .ok_or(TradeOfferError::Invalid)?;
            if view.offer_id != offer_id || view.group_id != group_id {
                return Err(TradeOfferError::Invalid);
            }
            Ok(view)
        })
    }

    /// `GET /v1/groups/{group_id}/trade-offers/current`; `None` when no open
    /// offer is pending.
    pub(crate) fn trade_offer_current_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        fence: LeaseFence,
    ) -> TradeOfferFuture<'_, Option<TradeOfferCurrentView>> {
        Box::pin(async move {
            let request = self.trade_get(
                &token,
                &format!("v1/groups/{group_id}/trade-offers/current"),
                fence,
            )?;
            let view: Option<TradeOfferCurrentView> = self.trade_send(request, true).await?;
            if view.is_some_and(|view| view.offer.group_id != group_id) {
                return Err(TradeOfferError::Invalid);
            }
            Ok(view)
        })
    }

    /// `POST /v1/groups/{group_id}/trade-offers/{offer_id}/decision`.
    pub(crate) fn trade_offer_decide_http(
        &self,
        token: AccessToken,
        group_id: GroupId,
        request: TradeDecisionRequest,
    ) -> TradeOfferFuture<'_, TradeOfferView> {
        Box::pin(async move {
            let offer_id = request.offer_id;
            let url = self
                .url(&format!(
                    "v1/groups/{group_id}/trade-offers/{offer_id}/decision"
                ))
                .map_err(|_| TradeOfferError::Invalid)?;
            let view: TradeOfferView = self
                .trade_send(
                    self.client
                        .request(Method::POST, url)
                        .bearer_auth(token.expose_secret())
                        .json(&request),
                    false,
                )
                .await?
                .ok_or(TradeOfferError::Invalid)?;
            if view.offer_id != offer_id || view.group_id != group_id {
                return Err(TradeOfferError::Invalid);
            }
            Ok(view)
        })
    }
}

/// The bridge's name for a server offer. Derived from the offer UUID so a
/// restarted launcher names the same offer the same way; never zero.
#[must_use]
pub(crate) fn offer_token(offer_id: TradeOfferId) -> u32 {
    let uuid = offer_id.as_uuid();
    let bytes = uuid.as_bytes();
    match u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) {
        0 => 1,
        token => token,
    }
}

/// The outcome the ROM shows for a terminal server status.
#[must_use]
pub(crate) const fn outcome_of(status: TradeOfferStatus) -> Option<TradeOfferOutcome> {
    match status {
        TradeOfferStatus::Pending => None,
        TradeOfferStatus::Accepted => Some(TradeOfferOutcome::Accepted),
        TradeOfferStatus::Rejected => Some(TradeOfferOutcome::Declined),
        TradeOfferStatus::Expired => Some(TradeOfferOutcome::Expired),
    }
}

/// The outcome for a failed create or accept. `Conflict` is resolved by the
/// caller first (another live offer is `Busy`, a moved offer is terminal);
/// here it means the checkpoint did not hold the named Pokémon.
#[must_use]
pub(crate) const fn failure_outcome(error: TradeOfferError) -> TradeOfferOutcome {
    match error {
        TradeOfferError::Mail => TradeOfferOutcome::Mail,
        TradeOfferError::Expired => TradeOfferOutcome::Expired,
        TradeOfferError::NotFound | TradeOfferError::Forbidden => {
            TradeOfferOutcome::PartnerUnavailable
        }
        TradeOfferError::Busy => TradeOfferOutcome::Busy,
        TradeOfferError::Conflict => TradeOfferOutcome::Stale,
        TradeOfferError::Unavailable | TradeOfferError::Unauthorized | TradeOfferError::Invalid => {
            TradeOfferOutcome::Unavailable
        }
    }
}

#[must_use]
pub(crate) fn received_record(view: &TradeOfferCurrentView) -> Option<TradeOfferReceivedRecord> {
    let record = TradeOfferReceivedRecord {
        offer_token: offer_token(view.offer.offer_id),
        species: view.offered.species,
        level: view.offered.level,
        is_egg: view.offered.is_egg,
        nickname: view.offered.nickname,
    };
    record.encode().ok().map(|_| record)
}

#[must_use]
pub(crate) const fn requester_status(
    request_id: u32,
    offer_token: u32,
    outcome: TradeOfferOutcome,
) -> TradeOfferStatusRecord {
    TradeOfferStatusRecord {
        role: TradeOfferRole::Requester,
        outcome,
        request_id,
        offer_token,
    }
}

#[must_use]
pub(crate) const fn responder_status(
    offer_token: u32,
    outcome: TradeOfferOutcome,
) -> TradeOfferStatusRecord {
    TradeOfferStatusRecord {
        role: TradeOfferRole::Responder,
        outcome,
        request_id: 0,
        offer_token,
    }
}

/// An offer this member made and still waits on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Requester {
    pub request_id: u32,
    pub group_id: GroupId,
    pub view: TradeOfferView,
    /// The control generation of the ROM that is waiting.
    pub generation: u32,
}

impl Requester {
    #[must_use]
    pub(crate) fn token(&self) -> u32 {
        offer_token(self.view.offer_id)
    }

    /// The initiator's own slot, which a decision request must repeat.
    #[must_use]
    pub(crate) fn own_slot(&self, character_id: CharacterId) -> coop_cloud::PartyPosition {
        let index = usize::from(self.view.members[1] == character_id);
        self.view.slots[index]
    }
}

/// A partner's offer shown to this member's ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Responder {
    pub group_id: GroupId,
    pub view: TradeOfferView,
    pub received: TradeOfferReceivedRecord,
    pub delivered_generation: Option<u32>,
}

impl Responder {
    #[must_use]
    pub(crate) fn token(&self) -> u32 {
        self.received.offer_token
    }

    /// The initiator's anchored slot, which the accept must name.
    #[must_use]
    pub(crate) fn initiator_slot(&self) -> coop_cloud::PartyPosition {
        let index = usize::from(self.view.members[1] == self.view.initiator);
        self.view.slots[index]
    }
}

/// Per-session owner of the in-game trade offers.
#[derive(Debug)]
pub(crate) struct TradeOfferOwner {
    pub requester: Option<Requester>,
    pub responder: Option<Responder>,
    /// An offer to withdraw because its ROM stopped waiting (ROM restart).
    pub withdraw: Option<Requester>,
    group: Option<(GroupId, Instant)>,
    seen: VecDeque<TradeOfferId>,
    next_poll: Instant,
}

impl Default for TradeOfferOwner {
    fn default() -> Self {
        Self {
            requester: None,
            responder: None,
            withdraw: None,
            group: None,
            seen: VecDeque::new(),
            next_poll: Instant::now(),
        }
    }
}

impl TradeOfferOwner {
    /// When the session loop must wake for the next poll.
    #[must_use]
    pub(crate) const fn next_wake(&self) -> Instant {
        self.next_poll
    }

    #[must_use]
    pub(crate) fn poll_due(&self, now: Instant) -> bool {
        now >= self.next_poll
    }

    pub(crate) fn request_poll(&mut self) {
        self.next_poll = Instant::now();
    }

    /// Schedules the next poll on the cadence the current state needs.
    pub(crate) fn schedule(&mut self, now: Instant) {
        self.next_poll = now
            + if self.requester.is_some() || self.withdraw.is_some() {
                REQUESTER_POLL_INTERVAL
            } else if self.group.is_some() {
                RESPONDER_POLL_INTERVAL
            } else {
                GROUP_REFRESH_INTERVAL
            };
    }

    /// The resolved group while it is fresh.
    #[must_use]
    pub(crate) fn fresh_group(&self, now: Instant) -> Option<GroupId> {
        self.group
            .filter(|(_, at)| now.saturating_duration_since(*at) < GROUP_REFRESH_INTERVAL)
            .map(|(group, _)| group)
    }

    pub(crate) fn set_group(&mut self, group: Option<GroupId>, now: Instant) {
        self.group = group.map(|group| (group, now));
    }

    pub(crate) fn forget_group(&mut self) {
        self.group = None;
    }

    #[must_use]
    pub(crate) fn seen(&self, offer_id: TradeOfferId) -> bool {
        self.seen.contains(&offer_id)
    }

    pub(crate) fn mark_seen(&mut self, offer_id: TradeOfferId) {
        if !self.seen.contains(&offer_id) {
            if self.seen.len() == SEEN_OFFERS {
                self.seen.pop_front();
            }
            self.seen.push_back(offer_id);
        }
    }

    /// A new control generation means a restarted ROM: it no longer waits
    /// for its own offer (withdraw it) and must be shown a partner's pending
    /// offer again.
    pub(crate) fn observe_generation(&mut self, generation: u32) {
        if let Some(requester) = self.requester
            && requester.generation != generation
        {
            self.requester = None;
            self.withdraw = Some(requester);
            self.request_poll();
        }
        if let Some(responder) = &mut self.responder
            && responder
                .delivered_generation
                .is_some_and(|seen| seen != generation)
        {
            responder.delivered_generation = None;
        }
    }

    /// The `TradeOfferReceived` still owed to this control generation.
    pub(crate) fn take_received(&mut self, generation: u32) -> Option<TradeOfferReceivedRecord> {
        let responder = self.responder.as_mut()?;
        if responder.delivered_generation == Some(generation) {
            return None;
        }
        responder.delivered_generation = Some(generation);
        Some(responder.received)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use coop_cloud::{ApiVersion, PartyPosition, Revision, SnapshotId, UnixTimestampMillis};
    use uuid::Uuid;

    pub(crate) fn offer_view(members: [CharacterId; 2], initiator: CharacterId) -> TradeOfferView {
        TradeOfferView {
            api_version: ApiVersion::V1,
            offer_id: TradeOfferId::new(Uuid::from_u128(0x0102_0304_0000_0000_0000_0000_0000_0009))
                .unwrap(),
            group_id: GroupId::new(Uuid::from_u128(7)).unwrap(),
            members,
            initiator,
            slots: [
                PartyPosition::new(2).unwrap(),
                PartyPosition::new(0).unwrap(),
            ],
            revisions: [Revision::new(2), Revision::new(3)],
            snapshots: [
                SnapshotId::new(Uuid::from_u128(8)).unwrap(),
                SnapshotId::new(Uuid::from_u128(9)).unwrap(),
            ],
            status: TradeOfferStatus::Pending,
            expires_at: UnixTimestampMillis::new(60_000),
            partner_chooses: true,
        }
    }

    fn members() -> [CharacterId; 2] {
        [
            CharacterId::new(Uuid::from_u128(1)).unwrap(),
            CharacterId::new(Uuid::from_u128(2)).unwrap(),
        ]
    }

    #[test]
    fn tokens_are_stable_nonzero_and_derived_from_the_offer() {
        let view = offer_view(members(), members()[0]);
        // UUID octets are big endian; the token reads the first four octets
        // little endian.
        assert_eq!(offer_token(view.offer_id), 0x0403_0201);
        let zero = TradeOfferId::new(Uuid::from_u128(1)).unwrap();
        assert_eq!(offer_token(zero), 1);
    }

    #[test]
    fn server_errors_map_to_short_rom_outcomes() {
        assert_eq!(
            map_status(StatusCode::CONFLICT, Some("trade_pokemon_holds_mail")),
            TradeOfferError::Mail
        );
        assert_eq!(
            map_status(StatusCode::CONFLICT, Some("conflict")),
            TradeOfferError::Conflict
        );
        assert_eq!(
            map_status(StatusCode::UNAUTHORIZED, Some("expired")),
            TradeOfferError::Expired
        );
        assert_eq!(
            map_status(StatusCode::UNAUTHORIZED, Some("authentication_failed")),
            TradeOfferError::Unauthorized
        );
        assert_eq!(
            map_status(StatusCode::NOT_FOUND, None),
            TradeOfferError::NotFound
        );
        assert_eq!(
            map_status(StatusCode::SERVICE_UNAVAILABLE, None),
            TradeOfferError::Busy
        );
        assert_eq!(
            map_status(StatusCode::BAD_GATEWAY, None),
            TradeOfferError::Unavailable
        );
        assert_eq!(
            map_status(StatusCode::BAD_REQUEST, None),
            TradeOfferError::Invalid
        );
        assert_eq!(
            error_code(br#"{"error":{"code":"expired"}}"#).as_deref(),
            Some("expired")
        );
        assert_eq!(error_code(b"not json"), None);

        for (error, outcome) in [
            (TradeOfferError::Mail, TradeOfferOutcome::Mail),
            (TradeOfferError::Expired, TradeOfferOutcome::Expired),
            (
                TradeOfferError::NotFound,
                TradeOfferOutcome::PartnerUnavailable,
            ),
            (
                TradeOfferError::Forbidden,
                TradeOfferOutcome::PartnerUnavailable,
            ),
            (TradeOfferError::Busy, TradeOfferOutcome::Busy),
            (TradeOfferError::Conflict, TradeOfferOutcome::Stale),
            (TradeOfferError::Unavailable, TradeOfferOutcome::Unavailable),
            (TradeOfferError::Invalid, TradeOfferOutcome::Unavailable),
        ] {
            assert_eq!(failure_outcome(error), outcome);
        }
        assert_eq!(outcome_of(TradeOfferStatus::Pending), None);
        assert_eq!(
            outcome_of(TradeOfferStatus::Rejected),
            Some(TradeOfferOutcome::Declined)
        );
    }

    #[test]
    fn a_restarted_rom_withdraws_its_offer_and_sees_the_partners_again() {
        let [one, two] = members();
        let mut owner = TradeOfferOwner::default();
        owner.requester = Some(Requester {
            request_id: 5,
            group_id: GroupId::new(Uuid::from_u128(7)).unwrap(),
            view: offer_view([one, two], one),
            generation: 1,
        });
        assert_eq!(owner.requester.unwrap().own_slot(one).index(), 2);
        owner.observe_generation(1);
        assert!(owner.requester.is_some());
        owner.observe_generation(2);
        assert!(owner.requester.is_none());
        assert_eq!(owner.withdraw.map(|offer| offer.request_id), Some(5));

        let view = offer_view([one, two], one);
        let received = received_record(&TradeOfferCurrentView {
            offer: view,
            offered: coop_cloud::TradeOfferedPokemon {
                species: 263,
                level: 5,
                is_egg: false,
                nickname: [0xFF; 10],
            },
        })
        .unwrap();
        owner.responder = Some(Responder {
            group_id: view.group_id,
            view,
            received,
            delivered_generation: None,
        });
        assert_eq!(owner.responder.unwrap().initiator_slot().index(), 2);
        assert_eq!(owner.take_received(2), Some(received));
        assert_eq!(owner.take_received(2), None);
        owner.observe_generation(3);
        assert_eq!(owner.take_received(3), Some(received));
    }

    #[test]
    fn cadence_follows_the_offer_state_and_seen_offers_are_bounded() {
        let now = Instant::now();
        let mut owner = TradeOfferOwner::default();
        owner.schedule(now);
        assert_eq!(owner.next_wake(), now + GROUP_REFRESH_INTERVAL);
        owner.set_group(Some(GroupId::new(Uuid::from_u128(7)).unwrap()), now);
        assert!(owner.fresh_group(now).is_some());
        assert!(owner.fresh_group(now + GROUP_REFRESH_INTERVAL).is_none());
        owner.schedule(now);
        assert_eq!(owner.next_wake(), now + RESPONDER_POLL_INTERVAL);
        let [one, two] = members();
        owner.requester = Some(Requester {
            request_id: 5,
            group_id: GroupId::new(Uuid::from_u128(7)).unwrap(),
            view: offer_view([one, two], one),
            generation: 1,
        });
        owner.schedule(now);
        assert_eq!(owner.next_wake(), now + REQUESTER_POLL_INTERVAL);
        for value in 0..10_u128 {
            owner.mark_seen(TradeOfferId::new(Uuid::from_u128(100 + value)).unwrap());
        }
        assert!(!owner.seen(TradeOfferId::new(Uuid::from_u128(100)).unwrap()));
        assert!(owner.seen(TradeOfferId::new(Uuid::from_u128(109)).unwrap()));
    }
}

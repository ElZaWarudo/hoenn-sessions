//! Level 1 multiplayer outcome ledger, launcher side.
//!
//! The server keeps at most one open (`ISSUED` or `DELIVERED`) ledger entry
//! per character and serves it from `GET /v1/characters/{id}/ledger/open`.
//! This module mirrors that view exactly, fetches it under the lease fence,
//! and owns delivery of an open `TRADE` entry to the ROM as a
//! [`ControlCommand::TradeCommit`](coop_sidecar::control::ControlCommand).
//!
//! `TRAINER_WIN` entries (Wally) are deliberately ignored here: the existing
//! battle commit grant flow already delivers them as `BattleCommit`, and the
//! grant ID is the ledger commit ID, so delivering them twice would apply the
//! same outcome through two bridge paths.
//!
//! The server's open entry is the only authority. The launcher re-polls it on
//! every session start and after every commit acknowledgement or declaring
//! finalize, so an entry delivered by a crashed launcher is simply delivered
//! again. `pending_commits.json` mirrors the tracked (delivered but not yet
//! declared) trade commit as `[]` or `["<commit uuid>"]`, so a snapshot taken
//! between delivery and declaration records which outcome was in flight.

use std::{future::Future, pin::Pin, time::Duration};

use coop_cloud::{
    AccessToken, ApiVersion, CharacterId, CommitId, LeaseFence, PartyPosition, Revision,
    SnapshotId, TradeOfferId, UnixTimestampMillis,
};
use coop_protocol::{BattleId, TradeCommitAppliedRecord, TradeCommitRecord, TrainerInstanceId};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{HttpClientError, ReqwestCloudApi};

const RESPONSE_MAX_BYTES: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay before a failed poll is retried. The session loop wakes at least
/// once per heartbeat, so the effective retry interval is bounded by both.
pub(crate) const POLL_RETRY_DELAY: Duration = Duration::from_secs(5);

/// What produced a ledger entry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LedgerOrigin {
    Battle { battle_id: Uuid },
    Trade { offer_id: TradeOfferId },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LedgerStatus {
    Issued,
    Delivered,
    Applied,
    Voided,
}

impl LedgerStatus {
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Issued | Self::Delivered)
    }
}

/// Identifies one Pokémon across party and PC storage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PokemonKey {
    pub personality: u32,
    pub ot_id: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrainerStoryPolicy {
    None,
    WallyVictoryRoad,
}

/// The exact change the declaring snapshot must contain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ExpectedDelta {
    Trade {
        slot: PartyPosition,
        outgoing: PokemonKey,
        #[serde(with = "hex_record")]
        incoming_raw: [u8; 100],
        incoming_key: PokemonKey,
    },
    TrainerWin {
        trainer_id: TrainerInstanceId,
        trainer_ordinal: u16,
        vanilla_trainer_id: Option<u16>,
        story: TrainerStoryPolicy,
    },
}

/// `GET /v1/characters/{character_id}/ledger/open` response, field for field
/// the server's `LedgerEntryView`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerEntryView {
    pub api_version: ApiVersion,
    pub commit_id: CommitId,
    pub character_id: CharacterId,
    pub origin: LedgerOrigin,
    pub base_snapshot_id: SnapshotId,
    pub base_revision: Revision,
    pub expected: ExpectedDelta,
    pub status: LedgerStatus,
    pub issued_at: UnixTimestampMillis,
}

impl LedgerEntryView {
    /// The bridge record for an open trade entry, or `None` for any other
    /// entry kind.
    #[must_use]
    pub fn trade_commit(&self) -> Option<TradeCommitRecord> {
        let ExpectedDelta::Trade {
            slot,
            outgoing,
            incoming_raw,
            ..
        } = &self.expected
        else {
            return None;
        };
        Some(TradeCommitRecord {
            commit_id: BattleId(*self.commit_id.as_uuid().as_bytes()),
            slot: u8::from(*slot),
            outgoing_personality: outgoing.personality,
            outgoing_ot_id: outgoing.ot_id,
            incoming_record: *incoming_raw,
        })
    }

    fn validate(&self, character_id: CharacterId) -> Result<(), LedgerError> {
        let consistent = matches!(
            (&self.origin, &self.expected),
            (LedgerOrigin::Trade { .. }, ExpectedDelta::Trade { .. })
                | (
                    LedgerOrigin::Battle { .. },
                    ExpectedDelta::TrainerWin { .. }
                )
        );
        if self.api_version != ApiVersion::V1
            || self.character_id != character_id
            || !self.status.is_open()
            || !consistent
            || self
                .trade_commit()
                .is_some_and(|record| record.encode().is_err())
        {
            return Err(LedgerError::InvalidResponse);
        }
        Ok(())
    }
}

mod hex_record {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        raw: &[u8; 100],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(200);
        for byte in raw {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 15)]));
        }
        serializer.serialize_str(&text)
    }

    /// Accepts exactly the server's spelling: 200 lowercase hex digits.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 100], D::Error> {
        fn nibble(byte: u8) -> Option<u8> {
            match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            }
        }
        let text = String::deserialize(deserializer)?;
        let invalid = || serde::de::Error::custom("expected a 100-byte lowercase hex record");
        if text.len() != 200 {
            return Err(invalid());
        }
        let mut record = [0_u8; 100];
        for (slot, pair) in record.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
            *slot = (nibble(pair[0]).ok_or_else(invalid)? << 4)
                | nibble(pair[1]).ok_or_else(invalid)?;
        }
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LedgerError {
    #[error("the outcome ledger is temporarily unavailable")]
    Unavailable,
    #[error("outcome ledger authorization failed")]
    Unauthorized,
    #[error("the outcome ledger lease fence is stale")]
    Stale,
    #[error("the outcome ledger response is invalid")]
    InvalidResponse,
}

pub type LedgerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, LedgerError>> + Send + 'a>>;

fn map_http_error(error: &HttpClientError) -> LedgerError {
    match error {
        HttpClientError::Status(StatusCode::UNAUTHORIZED) | HttpClientError::SessionClosed => {
            LedgerError::Unauthorized
        }
        HttpClientError::Status(
            StatusCode::CONFLICT | StatusCode::GONE | StatusCode::FORBIDDEN,
        ) => LedgerError::Stale,
        HttpClientError::Transport(_) => LedgerError::Unavailable,
        HttpClientError::Status(status) if status.is_server_error() => LedgerError::Unavailable,
        _ => LedgerError::InvalidResponse,
    }
}

impl ReqwestCloudApi {
    /// Fetches the caller's open ledger entry under the exact lease fence.
    /// `404` means no open entry. Serving an entry marks it `DELIVERED`.
    pub(crate) fn ledger_open_http(
        &self,
        token: AccessToken,
        character_id: CharacterId,
        fence: LeaseFence,
    ) -> LedgerFuture<'_, Option<LedgerEntryView>> {
        Box::pin(async move {
            if fence.character_id != character_id {
                return Err(LedgerError::Stale);
            }
            let url = self
                .url(&format!("v1/characters/{character_id}/ledger/open"))
                .map_err(|error| map_http_error(&error))?;
            let response = self
                .client
                .request(Method::GET, url)
                .timeout(REQUEST_TIMEOUT)
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
                .send()
                .await
                .map_err(|_| LedgerError::Unavailable)?;
            match response.status() {
                StatusCode::OK => {
                    let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
                        .await
                        .map_err(|error| map_http_error(&error))?;
                    let view: LedgerEntryView =
                        serde_json::from_slice(&bytes).map_err(|_| LedgerError::InvalidResponse)?;
                    view.validate(character_id)?;
                    Ok(Some(view))
                }
                StatusCode::NOT_FOUND => Ok(None),
                status => Err(map_http_error(&HttpClientError::Status(status))),
            }
        })
    }
}

/// Canonical `pending_commits.json` bytes for the tracked commit.
#[must_use]
pub(crate) fn encode_pending_commits(commit: Option<CommitId>) -> Vec<u8> {
    match commit {
        None => b"[]".to_vec(),
        Some(commit) => format!("[\"{commit}\"]").into_bytes(),
    }
}

#[derive(Clone, Copy, Debug)]
struct TradeDelivery {
    commit_id: CommitId,
    record: TradeCommitRecord,
    delivered_generation: Option<u32>,
    acknowledged: bool,
}

/// Per-session owner of the open trade entry.
#[derive(Debug, Default)]
pub(crate) struct LedgerOwner {
    poll_due: bool,
    retry_at: Option<tokio::time::Instant>,
    /// Set by the first successful poll. Until then `pending_commits.json`
    /// keeps the restored bytes because nothing contradicts them yet.
    synced: bool,
    trade: Option<TradeDelivery>,
}

impl LedgerOwner {
    /// Schedules a poll for the next opportunity, cancelling any backoff.
    pub(crate) fn request_poll(&mut self) {
        self.poll_due = true;
        self.retry_at = None;
    }

    pub(crate) fn cancel_poll(&mut self) {
        self.poll_due = false;
        self.retry_at = None;
    }

    #[must_use]
    pub(crate) fn poll_ready(&self, now: tokio::time::Instant) -> bool {
        self.poll_due && self.retry_at.is_none_or(|at| now >= at)
    }

    pub(crate) fn poll_failed(&mut self, now: tokio::time::Instant) {
        self.poll_due = true;
        self.retry_at = Some(now + POLL_RETRY_DELAY);
    }

    #[must_use]
    pub(crate) const fn synced(&self) -> bool {
        self.synced
    }

    /// The trade commit delivered (or about to be delivered) and not yet
    /// declared by a finalized snapshot.
    #[must_use]
    pub(crate) fn tracked_commit(&self) -> Option<CommitId> {
        self.trade.map(|trade| trade.commit_id)
    }

    /// Applies a successful poll result.
    ///
    /// # Errors
    ///
    /// `InvalidResponse` when the view is malformed, not open, for another
    /// character, or reuses the tracked commit ID for a different record.
    pub(crate) fn accept(
        &mut self,
        view: Option<&LedgerEntryView>,
        character_id: CharacterId,
    ) -> Result<(), LedgerError> {
        if let Some(view) = view {
            view.validate(character_id)?;
        }
        self.poll_due = false;
        self.retry_at = None;
        self.synced = true;
        let Some(record) = view.and_then(LedgerEntryView::trade_commit) else {
            // No open trade. An acknowledged trade stays tracked until the
            // finalize that declares it; an unacknowledged one is gone.
            if !self.trade.is_some_and(|trade| trade.acknowledged) {
                self.trade = None;
            }
            return Ok(());
        };
        let commit_id = view.expect("a trade record implies a view").commit_id;
        match &self.trade {
            Some(tracked) if tracked.commit_id == commit_id => {
                if tracked.record != record {
                    return Err(LedgerError::InvalidResponse);
                }
            }
            Some(tracked) if tracked.acknowledged => {
                // At most one entry is open; the acknowledged one must be
                // declared before the server can issue another.
                return Err(LedgerError::InvalidResponse);
            }
            _ => {
                self.trade = Some(TradeDelivery {
                    commit_id,
                    record,
                    delivered_generation: None,
                    acknowledged: false,
                });
            }
        }
        Ok(())
    }

    /// Returns the tracked trade once per sidecar control generation until
    /// the ROM acknowledges it. A later generation (control restart, ROM
    /// reboot) receives it again; the ROM must treat a repeated commit ID as
    /// already applied and acknowledge it again.
    pub(crate) fn take_delivery(&mut self, generation: u32) -> Option<TradeCommitRecord> {
        let trade = self.trade.as_mut()?;
        if trade.acknowledged || trade.delivered_generation == Some(generation) {
            return None;
        }
        trade.delivered_generation = Some(generation);
        Some(trade.record)
    }

    /// Accepts only the exact acknowledgement of the tracked trade. A
    /// duplicate acknowledgement is accepted again.
    pub(crate) fn acknowledge(&mut self, ack: TradeCommitAppliedRecord) -> Option<CommitId> {
        let trade = self.trade.as_mut()?;
        if trade.record.applied() != ack {
            return None;
        }
        trade.acknowledged = true;
        Some(trade.commit_id)
    }

    /// A finalized snapshot declared `commit_id`; it is no longer open.
    pub(crate) fn finalized(&mut self, commit_id: CommitId) {
        if self.tracked_commit() == Some(commit_id) {
            self.trade = None;
        }
        self.request_poll();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    pub(crate) fn character() -> CharacterId {
        CharacterId::new(Uuid::from_u128(101)).unwrap()
    }

    pub(crate) fn incoming_raw() -> [u8; 100] {
        let mut raw = [0_u8; 100];
        for (index, byte) in raw.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap().wrapping_mul(7).wrapping_add(1);
        }
        raw
    }

    fn hex(raw: &[u8]) -> String {
        raw.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub(crate) fn trade_view_json(commit: u128, character_id: CharacterId) -> serde_json::Value {
        json!({
            "api_version": 1,
            "commit_id": Uuid::from_u128(commit),
            "character_id": character_id,
            "origin": {"kind": "TRADE", "offer_id": Uuid::from_u128(0x0ff3)},
            "base_snapshot_id": Uuid::from_u128(0x5a5),
            "base_revision": 1,
            "expected": {
                "kind": "TRADE",
                "slot": 3,
                "outgoing": {"personality": 0xDEAD_BEEF_u32, "ot_id": 0x0102_0304},
                "incoming_raw": hex(&incoming_raw()),
                "incoming_key": {"personality": 0x0A0B_0C0D, "ot_id": 0x1122_3344},
            },
            "status": "DELIVERED",
            "issued_at": 1_700_000_000_000_u64,
        })
    }

    pub(crate) fn trade_view(commit: u128) -> LedgerEntryView {
        serde_json::from_value(trade_view_json(commit, character())).unwrap()
    }

    pub(crate) fn wally_view(commit: u128) -> LedgerEntryView {
        serde_json::from_value(json!({
            "api_version": 1,
            "commit_id": Uuid::from_u128(commit),
            "character_id": character(),
            "origin": {"kind": "BATTLE", "battle_id": Uuid::from_u128(0xba77)},
            "base_snapshot_id": Uuid::from_u128(0x5a5),
            "base_revision": 1,
            "expected": {
                "kind": "TRAINER_WIN",
                "trainer_id": "HOENN:TRAINER_WALLY_1",
                "trainer_ordinal": 518,
                "vanilla_trainer_id": null,
                "story": "WALLY_VICTORY_ROAD",
            },
            "status": "ISSUED",
            "issued_at": 1_700_000_000_000_u64,
        }))
        .unwrap()
    }

    fn fence() -> LeaseFence {
        LeaseFence::new(
            coop_cloud::SessionId::new(Uuid::from_u128(102)).unwrap(),
            character(),
            Revision::new(1),
            coop_cloud::SessionEpoch::new(7).unwrap(),
            coop_cloud::ClientInstanceId::new(Uuid::from_u128(103)).unwrap(),
        )
    }

    #[test]
    fn trade_view_maps_to_the_exact_trade_commit_record_and_bytes() {
        let view = trade_view(0xc0);
        let record = view.trade_commit().unwrap();
        assert_eq!(record.commit_id.0, *Uuid::from_u128(0xc0).as_bytes());
        assert_eq!(record.slot, 3);
        assert_eq!(record.outgoing_personality, 0xDEAD_BEEF);
        assert_eq!(record.outgoing_ot_id, 0x0102_0304);
        assert_eq!(record.incoming_record, incoming_raw());
        let bytes = record.encode().unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(Uuid::from_u128(0xc0).as_bytes());
        expected.extend_from_slice(&[3, 0, 0, 0]);
        expected.extend_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
        expected.extend_from_slice(&0x0102_0304_u32.to_le_bytes());
        expected.extend_from_slice(&incoming_raw());
        assert_eq!(bytes, expected);
        assert_eq!(bytes.len(), 128);
        assert!(wally_view(0xc1).trade_commit().is_none());
    }

    #[test]
    fn view_mirror_is_strict() {
        let mut extra = trade_view_json(0xc0, character());
        extra["unexpected"] = json!(1);
        assert!(serde_json::from_value::<LedgerEntryView>(extra).is_err());
        let mut nested = trade_view_json(0xc0, character());
        nested["expected"]["unexpected"] = json!(1);
        assert!(serde_json::from_value::<LedgerEntryView>(nested).is_err());
        let mut upper = trade_view_json(0xc0, character());
        upper["expected"]["incoming_raw"] = json!(hex(&incoming_raw()).to_uppercase());
        assert!(serde_json::from_value::<LedgerEntryView>(upper).is_err());
        let mut slot = trade_view_json(0xc0, character());
        slot["expected"]["slot"] = json!(6);
        assert!(serde_json::from_value::<LedgerEntryView>(slot).is_err());
        let round_trip = serde_json::to_value(trade_view(0xc0)).unwrap();
        assert_eq!(round_trip, trade_view_json(0xc0, character()));
    }

    #[test]
    fn owner_delivers_once_per_generation_until_acknowledged() {
        let mut owner = LedgerOwner::default();
        owner.request_poll();
        let view = trade_view(0xc0);
        owner.accept(Some(&view), character()).unwrap();
        let record = view.trade_commit().unwrap();
        assert_eq!(owner.take_delivery(1), Some(record));
        assert_eq!(owner.take_delivery(1), None);
        assert_eq!(owner.take_delivery(2), Some(record));
        let mut wrong = record.applied();
        wrong.slot = 0;
        assert_eq!(owner.acknowledge(wrong), None);
        let commit = CommitId::new(Uuid::from_u128(0xc0)).unwrap();
        assert_eq!(owner.acknowledge(record.applied()), Some(commit));
        assert_eq!(owner.acknowledge(record.applied()), Some(commit));
        assert_eq!(owner.take_delivery(3), None);
        // Re-polling the still-open entry keeps the acknowledgement.
        owner.accept(Some(&view), character()).unwrap();
        assert_eq!(owner.take_delivery(4), None);
        assert_eq!(owner.tracked_commit(), Some(commit));
        owner.finalized(commit);
        assert_eq!(owner.tracked_commit(), None);
        assert!(owner.poll_ready(tokio::time::Instant::now()));
    }

    #[test]
    fn owner_ignores_trainer_wins_and_rejects_foreign_or_closed_entries() {
        let mut owner = LedgerOwner::default();
        owner.accept(Some(&wally_view(0xc1)), character()).unwrap();
        assert_eq!(owner.take_delivery(1), None);
        assert_eq!(owner.tracked_commit(), None);
        let other = CharacterId::new(Uuid::from_u128(999)).unwrap();
        assert_eq!(
            owner.accept(Some(&trade_view(0xc0)), other),
            Err(LedgerError::InvalidResponse)
        );
        let mut applied = trade_view(0xc0);
        applied.status = LedgerStatus::Applied;
        assert_eq!(
            owner.accept(Some(&applied), character()),
            Err(LedgerError::InvalidResponse)
        );
    }

    #[test]
    fn pending_commits_mirror_is_canonical() {
        assert_eq!(encode_pending_commits(None), b"[]");
        let commit = CommitId::new(Uuid::from_u128(0xc0)).unwrap();
        assert_eq!(
            encode_pending_commits(Some(commit)),
            b"[\"00000000-0000-0000-0000-0000000000c0\"]"
        );
    }

    /// One scripted HTTP exchange per entry; returns each raw request head.
    async fn serve(
        responses: Vec<(u16, String)>,
    ) -> (ReqwestCloudApi, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let api =
            ReqwestCloudApi::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            let mut heads = Vec::new();
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                }
                heads.push(String::from_utf8(request).unwrap());
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
            heads
        });
        (api, task)
    }

    #[tokio::test]
    async fn ledger_open_http_parses_200_and_404_and_sends_the_fence() {
        let body = trade_view_json(0xc0, character()).to_string();
        let (api, task) = serve(vec![(200, body), (404, String::new())]).await;
        let token = AccessToken::new("ledger-token").unwrap();
        let view = api
            .ledger_open_http(token.clone(), character(), fence())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view, trade_view(0xc0));
        assert_eq!(
            api.ledger_open_http(token, character(), fence())
                .await
                .unwrap(),
            None
        );
        let heads = task.await.unwrap();
        for head in heads {
            let lower = head.to_ascii_lowercase();
            assert!(head.starts_with(&format!(
                "GET /v1/characters/{}/ledger/open HTTP/1.1\r\n",
                character()
            )));
            assert!(lower.contains("authorization: bearer ledger-token\r\n"));
            assert!(lower.contains(&format!("x-coop-session-id: {}\r\n", Uuid::from_u128(102))));
            assert!(lower.contains("x-coop-session-epoch: 7\r\n"));
            assert!(lower.contains(&format!(
                "x-coop-client-instance-id: {}\r\n",
                Uuid::from_u128(103)
            )));
        }
    }

    #[tokio::test]
    async fn ledger_open_http_maps_fence_auth_and_malformed_responses() {
        let other = CharacterId::new(Uuid::from_u128(999)).unwrap();
        let mut extra = trade_view_json(0xc0, character());
        extra["extra"] = json!(true);
        let (api, task) = serve(vec![
            (409, String::new()),
            (401, String::new()),
            (503, String::new()),
            (200, trade_view_json(0xc0, other).to_string()),
            (200, extra.to_string()),
        ])
        .await;
        let token = AccessToken::new("ledger-token").unwrap();
        for expected in [
            LedgerError::Stale,
            LedgerError::Unauthorized,
            LedgerError::Unavailable,
            LedgerError::InvalidResponse,
            LedgerError::InvalidResponse,
        ] {
            assert_eq!(
                api.ledger_open_http(token.clone(), character(), fence())
                    .await
                    .unwrap_err(),
                expected
            );
        }
        task.await.unwrap();
        // A fence for another character is refused before any request.
        assert_eq!(
            api.ledger_open_http(token, other, fence())
                .await
                .unwrap_err(),
            LedgerError::Stale
        );
    }
}

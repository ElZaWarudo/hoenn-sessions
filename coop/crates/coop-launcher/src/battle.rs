//! Fenced HTTP transport for the server's battle reservation and consensus API.
//!
//! These calls coordinate clients. They do not execute or settle a battle.

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    time::Duration,
};

use coop_cloud::{
    AccessToken, ApiVersion, CharacterId, CommitId, GroupId, IdempotencyKey, LeaseFence, Revision,
    SnapshotId, UnixTimestampMillis,
};
use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::Instant;
use uuid::Uuid;

use crate::{CloudApi, HttpClientError, ReqwestCloudApi, SessionError};
use coop_protocol::{
    AbortBattleRecord, ActionIntent, BattleCommitRecord, BattleConsentOutcome,
    BattleConsentOutcomeRecord, BattleDecision, BattleDigest, BattleFinishedRecord,
    BattleFinishedResult, BattleId, BattleJoinOfferRecord, BattleJoinResponseRecord,
    BattleManifestRecord, BattleReadyRecord, BattleReserveRejectedRecord, BattleRole,
    BattleStartRecord, IdentityKind, PartySnapshotChunk, PauseForReconnectRecord, RegionId,
    TrainerBattleReserveRecord, TrainerInstanceId, TurnBundleRecord, TurnResultHash,
    identity_catalog,
};

// A consensus view includes up to 32 full turns. Each of the two 512-byte
// action strings can expand to six JSON bytes per control character.
const RESPONSE_MAX_BYTES: usize = 512 * 1024;
const PEER_RESPONSE_MAX_BYTES: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BattleApiError {
    #[error("battle service is temporarily unavailable")]
    Unavailable,
    #[error("battle authorization failed")]
    Unauthorized,
    #[error("battle state is stale or forbidden")]
    Stale,
    #[error("battle was not found")]
    NotFound,
    #[error("battle request or response is invalid")]
    Invalid,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleKind {
    CooperativeTrainer,
    Friendly,
}

/// Who applies a trainer battle's rewards. `Local` battles are applied by
/// each ROM and carry their staged party records to the server.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleRewardMode {
    #[default]
    Ledger,
    Local,
}

impl BattleRewardMode {
    const fn is_ledger(&self) -> bool {
        matches!(self, Self::Ledger)
    }
}

/// A member who already beat the trainer joins a local battle as a helper.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleMemberRole {
    Participant,
    Helper,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleStatus {
    Pending,
    Accepted,
    /// Internal launcher marker: the paired terminal hash is ready for the
    /// single finish POST. It is never accepted from server JSON.
    #[serde(skip)]
    FinishReady,
    Completed,
    CommitPending,
    Declined,
    Cancelled,
    Expired,
    Diverged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReservationView {
    pub api_version: ApiVersion,
    pub battle_id: Uuid,
    pub group_id: GroupId,
    pub initiator_character_id: CharacterId,
    pub member_character_ids: [CharacterId; 2],
    pub kind: BattleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_id: Option<TrainerInstanceId>,
    pub status: BattleStatus,
    pub expires_at: UnixTimestampMillis,
    #[serde(default, skip_serializing_if = "BattleRewardMode::is_ledger")]
    pub reward_mode: BattleRewardMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_roles: Option<[BattleMemberRole; 2]>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleManifest {
    pub battle_id: Uuid,
    pub member_character_ids: [CharacterId; 2],
    pub snapshot_hashes: [String; 2],
    pub seed: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleTurn {
    pub number: u16,
    pub actions: [Option<String>; 2],
    pub state_hashes: [Option<String>; 2],
    action_keys: [Option<IdempotencyKey>; 2],
    hash_keys: [Option<IdempotencyKey>; 2],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleConsensusView {
    pub reservation: BattleReservationView,
    pub commitments: [Option<String>; 2],
    pub manifest: Option<BattleManifest>,
    #[serde(default)]
    pub ready: [bool; 2],
    #[serde(default)]
    pub start_released: bool,
    pub turns: Vec<BattleTurn>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattlePeerPartyView {
    pub api_version: ApiVersion,
    pub battle_id: Uuid,
    pub peer_character_id: CharacterId,
    pub snapshot_revision: Revision,
    pub snapshot_hash: String,
    pub party_records: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleCommitGrantView {
    pub grant_id: CommitId,
    pub character_id: CharacterId,
    pub battle_id: Uuid,
    pub trainer_id: TrainerInstanceId,
    pub source_snapshot_id: SnapshotId,
    pub source_revision: Revision,
    pub source_party_digest: String,
    pub terminal_turn: u16,
    pub terminal_state_hash: String,
}

fn valid_commit_grant(
    grant: &BattleCommitGrantView,
    battle_id: Uuid,
    fence: LeaseFence,
    expected_trainer: &TrainerInstanceId,
    expected_source_digest: BattleDigest,
    expected_terminal_turn: u16,
    expected_terminal_hash: BattleDigest,
) -> bool {
    grant.character_id == fence.character_id
        && grant.battle_id == battle_id
        && grant.source_revision == fence.current_revision
        && grant.terminal_turn == expected_terminal_turn
        && expected_terminal_turn != 0
        && BattleDigest::parse(&grant.source_party_digest) == Ok(expected_source_digest)
        && BattleDigest::parse(&grant.terminal_state_hash) == Ok(expected_terminal_hash)
        && grant.trainer_id == *expected_trainer
        && identity_catalog::trainer(&grant.trainer_id).is_ok()
}

#[derive(Clone, Debug, Serialize)]
struct ReserveRequest {
    api_version: ApiVersion,
    kind: BattleKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    trainer_id: Option<TrainerInstanceId>,
    idempotency_key: IdempotencyKey,
}

#[derive(Clone, Debug, Serialize)]
struct ActionRequest {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotRequest<'a> {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
    snapshot_hash: &'a str,
    /// Exact staged party records, sent only for local-reward battles.
    #[serde(skip_serializing_if = "Option::is_none")]
    party_records: Option<&'a [String]>,
}

#[derive(Clone, Debug, Serialize)]
struct ReadyRequest<'a> {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
    snapshot_hash: &'a str,
}

#[derive(Clone, Debug, Serialize)]
struct IntentRequest<'a> {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
    turn: u16,
    action: &'a str,
}

#[derive(Clone, Debug, Serialize)]
struct HashRequest<'a> {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
    turn: u16,
    state_hash: &'a str,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum BattleFinishWireResult {
    Member0Won,
    Member1Won,
    Draw,
    Won,
    Lost,
}

#[derive(Clone, Debug, Serialize)]
struct FinishRequest {
    api_version: ApiVersion,
    idempotency_key: IdempotencyKey,
    result: BattleFinishWireResult,
    turn: u16,
    state_hash: String,
}

fn map_http_error(error: &HttpClientError) -> BattleApiError {
    match error {
        HttpClientError::Status(StatusCode::UNAUTHORIZED) | HttpClientError::SessionClosed => {
            BattleApiError::Unauthorized
        }
        HttpClientError::Status(
            StatusCode::CONFLICT | StatusCode::GONE | StatusCode::FORBIDDEN,
        ) => BattleApiError::Stale,
        HttpClientError::Status(StatusCode::NOT_FOUND) => BattleApiError::NotFound,
        HttpClientError::Status(StatusCode::TOO_MANY_REQUESTS) => BattleApiError::Unavailable,
        HttpClientError::Transport(_) => BattleApiError::Unavailable,
        HttpClientError::Status(status) if status.is_server_error() => BattleApiError::Unavailable,
        _ => BattleApiError::Invalid,
    }
}

impl ReqwestCloudApi {
    fn battle_url(
        &self,
        group_id: GroupId,
        battle_id: Option<Uuid>,
        suffix: &str,
    ) -> Result<Url, BattleApiError> {
        let mut path = format!("v1/groups/{group_id}/battles");
        if let Some(id) = battle_id {
            path.push('/');
            path.push_str(&id.to_string());
        }
        path.push_str(suffix);
        self.url(&path).map_err(|error| map_http_error(&error))
    }

    fn battle_request(
        &self,
        method: Method,
        url: Url,
        token: &AccessToken,
        fence: LeaseFence,
    ) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
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
    }

    async fn battle_response<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        expected: StatusCode,
    ) -> Result<T, BattleApiError> {
        let response = request
            .send()
            .await
            .map_err(|_| BattleApiError::Unavailable)?;
        if response.status() != expected {
            return Err(map_http_error(&HttpClientError::Status(response.status())));
        }
        let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
            .await
            .map_err(|error| map_http_error(&error))?;
        serde_json::from_slice(&bytes).map_err(|_| BattleApiError::Invalid)
    }

    pub async fn reserve_battle_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        fence: LeaseFence,
        kind: BattleKind,
        trainer_id: Option<TrainerInstanceId>,
        idempotency_key: IdempotencyKey,
    ) -> Result<BattleReservationView, BattleApiError> {
        let url = self.battle_url(group_id, None, "")?;
        let request = ReserveRequest {
            api_version: ApiVersion::V1,
            kind,
            trainer_id: trainer_id.clone(),
            idempotency_key,
        };
        let view: BattleReservationView = self
            .battle_response(
                self.battle_request(Method::POST, url, token, fence)
                    .json(&request),
                StatusCode::CREATED,
            )
            .await?;
        if view.group_id != group_id
            || view.api_version != ApiVersion::V1
            || view.kind != kind
            || view.trainer_id != trainer_id
        {
            return Err(BattleApiError::Invalid);
        }
        Ok(view)
    }

    pub async fn current_battle_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        fence: LeaseFence,
    ) -> Result<Option<BattleReservationView>, BattleApiError> {
        let url = self.battle_url(group_id, None, "/current")?;
        let response = self
            .battle_request(Method::GET, url, token, fence)
            .send()
            .await
            .map_err(|_| BattleApiError::Unavailable)?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::OK => {
                let bytes = crate::bounded_body(response, RESPONSE_MAX_BYTES)
                    .await
                    .map_err(|error| map_http_error(&error))?;
                let view: BattleReservationView =
                    serde_json::from_slice(&bytes).map_err(|_| BattleApiError::Invalid)?;
                if view.group_id != group_id || view.api_version != ApiVersion::V1 {
                    return Err(BattleApiError::Invalid);
                }
                Ok(Some(view))
            }
            status => Err(map_http_error(&HttpClientError::Status(status))),
        }
    }

    pub async fn battle_reservation_action_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        action: &str,
        idempotency_key: IdempotencyKey,
    ) -> Result<BattleReservationView, BattleApiError> {
        if !matches!(action, "accept" | "decline" | "cancel") {
            return Err(BattleApiError::Invalid);
        }
        let url = self.battle_url(group_id, Some(battle_id), &format!("/{action}"))?;
        let request = ActionRequest {
            api_version: ApiVersion::V1,
            idempotency_key,
        };
        let view: BattleReservationView = self
            .battle_response(
                self.battle_request(Method::POST, url, token, fence)
                    .json(&request),
                StatusCode::OK,
            )
            .await?;
        if view.group_id != group_id
            || view.battle_id != battle_id
            || view.api_version != ApiVersion::V1
        {
            return Err(BattleApiError::Invalid);
        }
        Ok(view)
    }

    pub async fn battle_finish_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        idempotency_key: IdempotencyKey,
        result: BattleFinishedResult,
        turn: u16,
        state_hash: BattleDigest,
    ) -> Result<BattleReservationView, BattleApiError> {
        let result = match result {
            BattleFinishedResult::Member0Won => BattleFinishWireResult::Member0Won,
            BattleFinishedResult::Member1Won => BattleFinishWireResult::Member1Won,
            BattleFinishedResult::Draw => BattleFinishWireResult::Draw,
            BattleFinishedResult::Won => BattleFinishWireResult::Won,
            BattleFinishedResult::Lost => BattleFinishWireResult::Lost,
        };
        let url = self.battle_url(group_id, Some(battle_id), "/finish")?;
        let request = FinishRequest {
            api_version: ApiVersion::V1,
            idempotency_key,
            result,
            turn,
            state_hash: hex_string(&state_hash.0),
        };
        let view: BattleReservationView = self
            .battle_response(
                self.battle_request(Method::POST, url, token, fence)
                    .json(&request),
                StatusCode::OK,
            )
            .await?;
        if view.group_id != group_id
            || view.battle_id != battle_id
            || view.api_version != ApiVersion::V1
            || !view.member_character_ids.contains(&fence.character_id)
        {
            return Err(BattleApiError::Invalid);
        }
        Ok(view)
    }

    pub async fn battle_commit_grant_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        expected_trainer: &TrainerInstanceId,
        expected_source_digest: BattleDigest,
        expected_terminal_turn: u16,
        expected_terminal_hash: BattleDigest,
    ) -> Result<BattleCommitGrantView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/commit-grant")?;
        let grant: BattleCommitGrantView = self
            .battle_response(
                self.battle_request(Method::GET, url, token, fence),
                StatusCode::OK,
            )
            .await?;
        if !valid_commit_grant(
            &grant,
            battle_id,
            fence,
            expected_trainer,
            expected_source_digest,
            expected_terminal_turn,
            expected_terminal_hash,
        ) {
            return Err(BattleApiError::Invalid);
        }
        Ok(grant)
    }

    pub async fn battle_commit_snapshot_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        idempotency_key: IdempotencyKey,
        snapshot_hash: &str,
        party_records: Option<&[String]>,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/snapshot-commitments")?;
        let request = SnapshotRequest {
            api_version: ApiVersion::V1,
            idempotency_key,
            snapshot_hash,
            party_records,
        };
        self.battle_consensus_response(
            self.battle_request(Method::POST, url, token, fence)
                .json(&request),
            group_id,
            battle_id,
        )
        .await
    }

    pub async fn battle_inspect_consensus_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/consensus")?;
        self.battle_consensus_response(
            self.battle_request(Method::GET, url, token, fence),
            group_id,
            battle_id,
        )
        .await
    }

    pub async fn battle_ready_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        idempotency_key: IdempotencyKey,
        party_digest: BattleDigest,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/ready")?;
        let hash = hex_string(&party_digest.0);
        self.battle_consensus_response(
            self.battle_request(Method::POST, url, token, fence)
                .json(&ReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key,
                    snapshot_hash: &hash,
                }),
            group_id,
            battle_id,
        )
        .await
    }

    pub async fn battle_peer_party_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
    ) -> Result<BattlePeerPartyView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/peer-party")?;
        let response = self
            .battle_request(Method::GET, url, token, fence)
            .send()
            .await
            .map_err(|_| BattleApiError::Unavailable)?;
        if response.status() != StatusCode::OK {
            return Err(map_http_error(&HttpClientError::Status(response.status())));
        }
        let bytes = crate::bounded_body(response, PEER_RESPONSE_MAX_BYTES)
            .await
            .map_err(|error| map_http_error(&error))?;
        let view: BattlePeerPartyView =
            serde_json::from_slice(&bytes).map_err(|_| BattleApiError::Invalid)?;
        if view.api_version != ApiVersion::V1 || view.battle_id != battle_id {
            return Err(BattleApiError::Invalid);
        }
        Ok(view)
    }

    pub async fn battle_submit_action_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        idempotency_key: IdempotencyKey,
        turn: u16,
        action: &str,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/actions")?;
        let request = IntentRequest {
            api_version: ApiVersion::V1,
            idempotency_key,
            turn,
            action,
        };
        self.battle_consensus_response(
            self.battle_request(Method::POST, url, token, fence)
                .json(&request),
            group_id,
            battle_id,
        )
        .await
    }

    pub async fn battle_acknowledge_hash_http(
        &self,
        token: &AccessToken,
        group_id: GroupId,
        battle_id: Uuid,
        fence: LeaseFence,
        idempotency_key: IdempotencyKey,
        turn: u16,
        state_hash: &str,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let url = self.battle_url(group_id, Some(battle_id), "/state-hashes")?;
        let request = HashRequest {
            api_version: ApiVersion::V1,
            idempotency_key,
            turn,
            state_hash,
        };
        self.battle_consensus_response(
            self.battle_request(Method::POST, url, token, fence)
                .json(&request),
            group_id,
            battle_id,
        )
        .await
    }

    async fn battle_consensus_response(
        &self,
        request: reqwest::RequestBuilder,
        group_id: GroupId,
        battle_id: Uuid,
    ) -> Result<BattleConsensusView, BattleApiError> {
        let view: BattleConsensusView = self.battle_response(request, StatusCode::OK).await?;
        if view.reservation.group_id != group_id
            || view.reservation.battle_id != battle_id
            || view.reservation.api_version != ApiVersion::V1
            || view
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.battle_id != battle_id)
        {
            return Err(BattleApiError::Invalid);
        }
        Ok(view)
    }
}

pub type BattleFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BattleApiError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug)]
enum Job {
    Poll,
    Cancel {
        battle_id: Uuid,
        group_id: GroupId,
        key: IdempotencyKey,
    },
    Finish {
        battle_id: Uuid,
        group_id: GroupId,
        record: BattleFinishedRecord,
        key: IdempotencyKey,
    },
    FinishProbe {
        battle_id: Uuid,
        group_id: GroupId,
        record: BattleFinishedRecord,
        key: IdempotencyKey,
    },
    Reserve {
        nonce: u32,
        kind: coop_protocol::BattleKind,
        trainer_region: Option<RegionId>,
        trainer_ordinal: Option<u16>,
        key: IdempotencyKey,
    },
    Respond {
        battle_id: Uuid,
        group_id: GroupId,
        decision: BattleDecision,
        key: IdempotencyKey,
    },
}

pub(crate) struct Completion {
    fence: LeaseFence,
    generation: u32,
    job: Job,
    result: Result<Option<BattleReservationView>, BattleApiError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Tracked {
    group_id: GroupId,
    battle_id: Uuid,
    members: [CharacterId; 2],
    kind: BattleKind,
    trainer_identity: Option<(RegionId, u16)>,
    role: BattleRole,
    nonce: u32,
    local_rewards: bool,
}

#[derive(Clone, Debug)]
enum ConsensusJob {
    Inspect,
    Commit {
        hash: String,
        key: IdempotencyKey,
        records: Option<Vec<String>>,
    },
    Ready {
        digest: BattleDigest,
        key: IdempotencyKey,
    },
    Action {
        turn: u16,
        action: String,
        key: IdempotencyKey,
    },
    Hash {
        turn: u16,
        hash: String,
        key: IdempotencyKey,
    },
}

fn new_key() -> IdempotencyKey {
    IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil")
}

fn hex_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

fn unhex_action(value: &str) -> Option<Vec<u8>> {
    if value.is_empty()
        || value.len() > coop_protocol::BATTLE_MAX_ACTION_SIZE * 2
        || value.len() % 2 != 0
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            Some((nibble(pair[0])? << 4) | nibble(pair[1])?)
        })
        .collect()
}

fn decode_party_record(value: &str) -> Option<Vec<u8>> {
    if value.len() != coop_protocol::BATTLE_PARTY_MON_SIZE * 2 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            Some((nibble(pair[0])? << 4) | nibble(pair[1])?)
        })
        .collect()
}

fn canonical_party_digest(records: &[Vec<u8>]) -> Option<BattleDigest> {
    if !(1..=6).contains(&records.len())
        || records
            .iter()
            .any(|record| record.len() != coop_protocol::BATTLE_PARTY_MON_SIZE)
    {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"coop-battle-party-v1\0");
    hasher.update([records.len() as u8]);
    for record in records {
        hasher.update(record);
    }
    Some(BattleDigest(hasher.finalize().into()))
}

fn validated_peer_chunks(
    view: BattlePeerPartyView,
    tracked: Tracked,
    fence: LeaseFence,
    expected: (CharacterId, BattleDigest),
) -> Result<Vec<PartySnapshotChunk>, BattleApiError> {
    if view.api_version != ApiVersion::V1
        || view.battle_id != tracked.battle_id
        || view.peer_character_id != expected.0
        || !tracked.members.contains(&view.peer_character_id)
        || !tracked.members.contains(&fence.character_id)
        || view.peer_character_id == fence.character_id
        || view.snapshot_revision == Revision::initial()
        || BattleDigest::parse(&view.snapshot_hash) != Ok(expected.1)
        || !(1..=6).contains(&view.party_records.len())
    {
        return Err(BattleApiError::Invalid);
    }
    let records: Vec<Vec<u8>> = view
        .party_records
        .iter()
        .map(|text| decode_party_record(text).ok_or(BattleApiError::Invalid))
        .collect::<Result<_, _>>()?;
    if canonical_party_digest(&records) != Some(expected.1) {
        return Err(BattleApiError::Invalid);
    }
    let count = records.len() as u8;
    records
        .into_iter()
        .enumerate()
        .map(|(index, mon)| {
            let record = PartySnapshotChunk {
                battle_id: bridge_id(tracked.battle_id),
                party_slot: index as u8,
                chunk_index: index as u8,
                chunk_count: count,
                mon,
            };
            record.encode().map_err(|_| BattleApiError::Invalid)?;
            Ok(record)
        })
        .collect()
}

pub(crate) struct ConsensusCompletion {
    fence: LeaseFence,
    generation: u32,
    job: ConsensusJob,
    result: Result<BattleConsensusView, BattleApiError>,
}

pub(crate) struct PeerCompletion {
    fence: LeaseFence,
    generation: u32,
    battle_id: Uuid,
    result: Result<Vec<PartySnapshotChunk>, BattleApiError>,
}

pub(crate) struct CommitGrantCompletion {
    fence: LeaseFence,
    generation: u32,
    battle_id: Uuid,
    result: Result<BattleCommitGrantView, BattleApiError>,
}

pub(crate) enum BattleCompletion {
    Reservation(Option<Completion>),
    Consensus(Option<ConsensusCompletion>),
    Peer(PeerCompletion),
    CommitGrant(CommitGrantCompletion),
}

/// Per-lease battle consent owner. It never commits a snapshot or starts a battle.
pub(crate) struct BattleOwner<'a> {
    pending: Option<Pin<Box<dyn Future<Output = Completion> + Send + 'a>>>,
    pending_job: Option<Job>,
    next_poll: Instant,
    queued_reserve: Option<TrainerBattleReserveRecord>,
    replacement_reserve: Option<TrainerBattleReserveRecord>,
    queued_cancel: Option<(Uuid, GroupId, IdempotencyKey)>,
    queued_finish: Option<(BattleFinishedRecord, IdempotencyKey)>,
    last_finish: Option<(BattleFinishedRecord, IdempotencyKey)>,
    finish_waiting: bool,
    finish_posted: bool,
    cold_reconcile: bool,
    cold_reconciled_once: bool,
    cold_cancel: Option<(Uuid, GroupId, IdempotencyKey)>,
    cold_abort_sent: bool,
    cold_retry_reserve: Option<(TrainerBattleReserveRecord, IdempotencyKey)>,
    cold_retry_used: bool,
    reconcile_before_reserve: bool,
    reserve_from_prior_generation: bool,
    queued_response: Option<BattleJoinResponseRecord>,
    reserve_keys: HashMap<u32, IdempotencyKey>,
    tracked: Option<Tracked>,
    requester_outcome: Option<BattleConsentOutcomeRecord>,
    redeliver_requester_outcome: bool,
    rejected_battle: Option<Uuid>,
    redeliver: bool,
    response_key: Option<IdempotencyKey>,
    answered: Option<(Uuid, BattleDecision)>,
    fence: Option<LeaseFence>,
    generation: u32,
    consensus_pending: Option<Pin<Box<dyn Future<Output = ConsensusCompletion> + Send + 'a>>>,
    peer_pending: Option<Pin<Box<dyn Future<Output = PeerCompletion> + Send + 'a>>>,
    commit_grant_pending: Option<Pin<Box<dyn Future<Output = CommitGrantCompletion> + Send + 'a>>>,
    next_consensus_poll: Instant,
    next_peer_poll: Instant,
    party: Vec<Vec<u8>>,
    party_count: Option<u8>,
    snapshot: Option<(String, IdempotencyKey)>,
    accepted_battle: Option<Uuid>,
    actions: BTreeMap<u16, (String, IdempotencyKey)>,
    hashes: BTreeMap<u16, (String, IdempotencyKey)>,
    manifest_delivered: bool,
    bundles_delivered: u16,
    paused_turn: Option<u16>,
    submitted_snapshot: bool,
    ready_proof: Option<(BattleDigest, IdempotencyKey)>,
    ready_from_rom: bool,
    submitted_ready: bool,
    start_delivered: bool,
    submitted_actions: u16,
    submitted_hashes: u16,
    peer_expected: Option<(CharacterId, BattleDigest)>,
    peer_cached: Option<Vec<PartySnapshotChunk>>,
    peer_delivered: bool,
    commit_ready: bool,
    commit_record: Option<BattleCommitRecord>,
    commit_acknowledged: bool,
    commit_delivered_generation: Option<u32>,
    next_commit_poll: Instant,
}

impl Default for BattleOwner<'_> {
    fn default() -> Self {
        Self {
            pending: None,
            pending_job: None,
            next_poll: Instant::now(),
            queued_reserve: None,
            replacement_reserve: None,
            queued_cancel: None,
            queued_finish: None,
            last_finish: None,
            finish_waiting: false,
            finish_posted: false,
            cold_reconcile: false,
            cold_reconciled_once: false,
            cold_cancel: None,
            cold_abort_sent: false,
            cold_retry_reserve: None,
            cold_retry_used: false,
            reconcile_before_reserve: false,
            reserve_from_prior_generation: false,
            queued_response: None,
            reserve_keys: HashMap::new(),
            tracked: None,
            requester_outcome: None,
            redeliver_requester_outcome: false,
            rejected_battle: None,
            redeliver: false,
            response_key: None,
            answered: None,
            fence: None,
            generation: 0,
            consensus_pending: None,
            peer_pending: None,
            commit_grant_pending: None,
            next_consensus_poll: Instant::now(),
            next_peer_poll: Instant::now(),
            party: Vec::new(),
            party_count: None,
            snapshot: None,
            accepted_battle: None,
            actions: BTreeMap::new(),
            hashes: BTreeMap::new(),
            manifest_delivered: false,
            bundles_delivered: 0,
            paused_turn: None,
            submitted_snapshot: false,
            ready_proof: None,
            ready_from_rom: false,
            submitted_ready: false,
            start_delivered: false,
            submitted_actions: 0,
            submitted_hashes: 0,
            peer_expected: None,
            peer_cached: None,
            peer_delivered: false,
            commit_ready: false,
            commit_record: None,
            commit_acknowledged: false,
            commit_delivered_generation: None,
            next_commit_poll: Instant::now(),
        }
    }
}

fn bridge_id(id: Uuid) -> BattleId {
    BattleId(*id.as_bytes())
}

fn bridge_kind(kind: BattleKind) -> coop_protocol::BattleKind {
    match kind {
        BattleKind::CooperativeTrainer => coop_protocol::BattleKind::CooperativeTrainer,
        BattleKind::Friendly => coop_protocol::BattleKind::Friendly,
    }
}

fn cloud_kind(kind: coop_protocol::BattleKind) -> BattleKind {
    match kind {
        coop_protocol::BattleKind::CooperativeTrainer => BattleKind::CooperativeTrainer,
        coop_protocol::BattleKind::Friendly => BattleKind::Friendly,
    }
}

fn valid_view(
    view: &BattleReservationView,
    group_id: GroupId,
    members: [CharacterId; 2],
    actor: CharacterId,
) -> bool {
    view.api_version == ApiVersion::V1
        && view.group_id == group_id
        && view.member_character_ids == members
        && members.contains(&actor)
        && members.contains(&view.initiator_character_id)
}

fn valid_reserve_view(
    view: &BattleReservationView,
    kind: coop_protocol::BattleKind,
    trainer_id: Option<&TrainerInstanceId>,
    actor: CharacterId,
) -> bool {
    view.kind == cloud_kind(kind)
        && view.trainer_id.as_ref() == trainer_id
        && view.initiator_character_id == actor
}

fn finish_probe_ready(view: &BattleConsensusView, record: BattleFinishedRecord) -> bool {
    let expected_hash = hex_string(&record.terminal_hash.0);
    view.manifest.as_ref().is_some_and(|manifest| {
        bridge_id(manifest.battle_id) == record.battle_id
            && manifest.member_character_ids == view.reservation.member_character_ids
            && view.turns.last().is_some_and(|turn| {
                turn.number == record.turn
                    && turn.actions.iter().all(Option::is_some)
                    && turn.state_hashes == [Some(expected_hash.clone()), Some(expected_hash)]
            })
    })
}

fn reserve_trainer_id(
    kind: coop_protocol::BattleKind,
    region: Option<RegionId>,
    ordinal: Option<u16>,
) -> Result<Option<TrainerInstanceId>, BattleApiError> {
    match (kind, region, ordinal) {
        (coop_protocol::BattleKind::Friendly, None, None) => Ok(None),
        (coop_protocol::BattleKind::CooperativeTrainer, Some(region), Some(ordinal)) => {
            let entry = identity_catalog::resolve_ordinal(IdentityKind::Trainer, ordinal)
                .map_err(|_| BattleApiError::Invalid)?;
            if entry.region != region {
                return Err(BattleApiError::Invalid);
            }
            TrainerInstanceId::parse(entry.qualified_id)
                .map(Some)
                .map_err(|_| BattleApiError::Invalid)
        }
        _ => Err(BattleApiError::Invalid),
    }
}

fn manifest_identity(
    reservation: &BattleReservationView,
    character_id: CharacterId,
) -> Option<(coop_protocol::BattleKind, u8, RegionId, u16)> {
    let slot = reservation
        .member_character_ids
        .iter()
        .position(|member| *member == character_id)? as u8;
    if reservation.member_character_ids[0] == reservation.member_character_ids[1] {
        return None;
    }
    match (reservation.kind, reservation.trainer_id.as_ref()) {
        (BattleKind::Friendly, None) => Some((
            coop_protocol::BattleKind::Friendly,
            slot,
            RegionId::Unspecified,
            0,
        )),
        (BattleKind::CooperativeTrainer, Some(trainer_id)) => {
            let entry =
                identity_catalog::resolve_identity(IdentityKind::Trainer, trainer_id.as_str())
                    .ok()?;
            if !matches!(
                entry.qualified_id,
                "HOENN:TRAINER_WALLY_1" | "KANTO:TRAINER_BROCK"
            ) {
                return None;
            }
            Some((
                coop_protocol::BattleKind::CooperativeTrainer,
                slot,
                entry.region,
                entry.ordinal?,
            ))
        }
        _ => None,
    }
}

impl<'a> BattleOwner<'a> {
    /// A fresh launcher has no battle engine state to resume. Settle any
    /// server-owned reservation before accepting a new ROM battle request.
    pub(crate) fn new_process() -> Self {
        Self {
            cold_reconcile: true,
            ..Self::default()
        }
    }

    /// Reissues a terminal abort once per control generation when the prior
    /// send may have failed after cold reconciliation opened the gate.
    pub(crate) fn take_replay_abort(
        &mut self,
        fence: LeaseFence,
        generation: u32,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        if self.cold_reconcile || self.cold_abort_sent {
            return None;
        }
        let (battle_id, _, _) = self.cold_cancel?;
        self.cold_abort_sent = true;
        Some(AbortBattleRecord {
            battle_id: bridge_id(battle_id),
            reason: 1,
        })
    }

    pub(crate) fn handle_bridge_event(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        event: coop_sidecar::control::ControlEvent,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        if self.cold_reconcile {
            return None;
        }
        if self.queued_cancel.is_some() || matches!(self.pending_job, Some(Job::Cancel { .. })) {
            return None;
        }
        match event {
            coop_sidecar::control::ControlEvent::PartySnapshot(record) => {
                self.party_chunk(fence, generation, record)
            }
            coop_sidecar::control::ControlEvent::BattleReady(record) => {
                self.battle_ready(fence, generation, record)
            }
            coop_sidecar::control::ControlEvent::ActionIntent(record) => {
                self.action_intent(fence, generation, record)
            }
            coop_sidecar::control::ControlEvent::TurnResultHash(record) => {
                self.turn_hash(fence, generation, record)
            }
            coop_sidecar::control::ControlEvent::BattleFinished(record) => {
                self.finish_request(fence, generation, record)
            }
            _ => None,
        }
    }

    /// Accept only the exact acknowledgement for the currently delivered
    /// server grant. A duplicate acknowledgement is harmless and remains
    /// accepted so a control restart cannot turn an already applied grant
    /// into a fatal protocol error.
    pub(crate) fn mark_commit_acknowledged(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: coop_protocol::CommitAppliedRecord,
    ) -> bool {
        self.bind(fence, generation);
        if self.commit_record != Some(record) {
            return false;
        }
        self.commit_acknowledged = true;
        true
    }

    /// Replay an accepted grant once for each sidecar control generation. The
    /// grant stays in launcher memory until the subsequent snapshot finalize
    /// proves that the server consumed it.
    pub(crate) fn take_replay_commit(
        &mut self,
        fence: LeaseFence,
        generation: u32,
    ) -> Option<BattleCommitRecord> {
        self.bind(fence, generation);
        if self.commit_acknowledged || self.commit_delivered_generation == Some(generation) {
            return None;
        }
        let record = self.commit_record?;
        self.commit_delivered_generation = Some(generation);
        Some(record)
    }

    pub(crate) fn finish_request(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: BattleFinishedRecord,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        let Some(tracked) = self.tracked else {
            return Some(AbortBattleRecord {
                battle_id: record.battle_id,
                reason: 5,
            });
        };
        if record.battle_id != bridge_id(tracked.battle_id) {
            // A late receipt from a prior battle cannot affect the current
            // reservation or cause a false abort.
            return None;
        }
        let abort = || AbortBattleRecord {
            battle_id: record.battle_id,
            reason: 5,
        };
        let terminal_hash = hex_string(&record.terminal_hash.0);
        let invalid = record.encode().is_err()
            || !matches!(
                (tracked.kind, record.result),
                (BattleKind::Friendly, BattleFinishedResult::Member0Won)
                    | (BattleKind::Friendly, BattleFinishedResult::Member1Won)
                    | (BattleKind::Friendly, BattleFinishedResult::Draw)
                    | (BattleKind::CooperativeTrainer, BattleFinishedResult::Won)
                    | (BattleKind::CooperativeTrainer, BattleFinishedResult::Lost)
                    | (BattleKind::CooperativeTrainer, BattleFinishedResult::Draw)
            )
            || self.hashes.get(&record.turn).map(|(hash, _)| hash.as_str())
                != Some(terminal_hash.as_str());
        if invalid {
            self.queue_finish_cancel(tracked);
            self.consensus_pending = None;
            self.peer_pending = None;
            return Some(abort());
        }
        if let Some((existing, _)) = self.queued_finish {
            if existing != record {
                self.queue_finish_cancel(tracked);
                return Some(abort());
            }
            return None;
        }
        if let Some((existing, _)) = self.last_finish {
            if existing != record {
                self.queue_finish_cancel(tracked);
                return Some(abort());
            }
        }
        if let Some(Job::Finish {
            battle_id,
            record: existing,
            ..
        }) = self.pending_job
        {
            if battle_id == tracked.battle_id {
                if existing != record {
                    self.queue_finish_cancel(tracked);
                    return Some(abort());
                }
                return None;
            }
        }
        let had_finish = self.last_finish.is_some();
        let key = match self.last_finish {
            Some((_, key)) => key,
            None => match self.pending_job {
                Some(Job::Finish {
                    key,
                    record: existing,
                    ..
                }) if existing == record => key,
                _ => IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"),
            },
        };
        self.last_finish = Some((record, key));
        self.queued_finish = Some((record, key));
        if !had_finish {
            self.finish_waiting = false;
        }
        self.next_poll = Instant::now();
        None
    }

    fn queue_finish_cancel(&mut self, tracked: Tracked) {
        if self.queued_cancel.is_none() && !matches!(self.pending_job, Some(Job::Cancel { .. })) {
            self.queued_cancel = Some((tracked.battle_id, tracked.group_id, new_key()));
        }
        self.consensus_pending = None;
        self.peer_pending = None;
        self.commit_grant_pending = None;
    }
    pub(crate) fn abort_request(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: AbortBattleRecord,
    ) {
        self.bind(fence, generation);
        if self.cold_reconcile {
            return;
        }
        let Some(tracked) = self.tracked else {
            return;
        };
        if record.encode().is_err()
            || record.battle_id.0.iter().all(|byte| *byte == 0)
            || record.battle_id != bridge_id(tracked.battle_id)
            || self.queued_cancel.is_some()
            || matches!(self.pending_job, Some(Job::Cancel { .. }))
        {
            return;
        }
        self.consensus_pending = None;
        self.peer_pending = None;
        self.queued_cancel = Some((tracked.battle_id, tracked.group_id, new_key()));
        self.next_poll = Instant::now();
    }
    fn clear_consensus(&mut self) {
        self.consensus_pending = None;
        self.peer_pending = None;
        self.party.clear();
        self.party_count = None;
        self.snapshot = None;
        self.accepted_battle = None;
        self.actions.clear();
        self.hashes.clear();
        self.manifest_delivered = false;
        self.bundles_delivered = 0;
        self.paused_turn = None;
        self.submitted_snapshot = false;
        self.ready_proof = None;
        self.ready_from_rom = false;
        self.submitted_ready = false;
        self.start_delivered = false;
        self.submitted_actions = 0;
        self.submitted_hashes = 0;
        self.peer_expected = None;
        self.peer_cached = None;
        self.peer_delivered = false;
        self.finish_waiting = false;
        self.finish_posted = false;
    }

    fn bad_record(&mut self, battle_id: BattleId) -> Option<AbortBattleRecord> {
        self.rejected_battle = self.tracked.map(|tracked| tracked.battle_id);
        self.clear_consensus();
        self.tracked = None;
        self.requester_outcome = None;
        self.redeliver_requester_outcome = false;
        self.finish_waiting = false;
        self.finish_posted = false;
        self.commit_ready = false;
        self.commit_record = None;
        self.commit_acknowledged = false;
        self.commit_delivered_generation = None;
        Some(AbortBattleRecord {
            battle_id,
            reason: 5,
        })
    }

    fn reject_current(&mut self) {
        self.rejected_battle = self.tracked.map(|tracked| tracked.battle_id);
        self.clear_consensus();
        self.tracked = None;
        self.requester_outcome = None;
        self.redeliver_requester_outcome = false;
        self.commit_ready = false;
        self.commit_record = None;
        self.commit_acknowledged = false;
        self.commit_delivered_generation = None;
    }

    fn maybe_deliver_requester_outcome(
        &mut self,
        record: BattleConsentOutcomeRecord,
        deliveries: &mut Vec<BattleDelivery>,
    ) {
        if let Some(existing) = self.requester_outcome {
            if existing.battle_id == record.battle_id
                && existing.request_nonce == record.request_nonce
                && existing.outcome != record.outcome
            {
                // A terminal result is immutable for a battle/nonce. In
                // particular, expiry after acceptance must not rewrite the
                // requester-visible acceptance decision.
                return;
            }
        }
        if self.requester_outcome != Some(record) || self.redeliver_requester_outcome {
            self.requester_outcome = Some(record);
            self.redeliver_requester_outcome = false;
            deliveries.push(BattleDelivery::ConsentOutcome(record));
        }
    }

    pub(crate) fn party_chunk(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: PartySnapshotChunk,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        let id = record.battle_id;
        if self.tracked.map(|tracked| bridge_id(tracked.battle_id)) != Some(id) {
            return None;
        }
        if record.encode().is_err() {
            return self.bad_record(id);
        }
        if self.snapshot.is_some() {
            return if self.party.get(usize::from(record.chunk_index)) == Some(&record.mon)
                && self.party_count == Some(record.chunk_count)
            {
                None
            } else {
                self.bad_record(id)
            };
        }
        if let Some(count) = self.party_count {
            if count != record.chunk_count {
                return self.bad_record(id);
            }
        } else {
            self.party_count = Some(record.chunk_count);
        }
        let index = usize::from(record.chunk_index);
        if index < self.party.len() {
            return if self.party[index] == record.mon {
                None
            } else {
                self.bad_record(id)
            };
        }
        if index != self.party.len() {
            return self.bad_record(id);
        }
        self.party.push(record.mon);
        if self.party.len() == usize::from(record.chunk_count) {
            let digest = canonical_party_digest(&self.party).expect("validated party chunks");
            if self.ready_proof.is_some_and(|(ready, _)| ready != digest) {
                return self.bad_record(id);
            }
            let hash = hex_string(&digest.0);
            self.snapshot = Some((hash, new_key()));
            self.next_consensus_poll = Instant::now();
        }
        None
    }

    pub(crate) fn action_intent(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: ActionIntent,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        let id = record.battle_id;
        if self.tracked.map(|tracked| bridge_id(tracked.battle_id)) != Some(id) {
            return None;
        }
        // The ROM may already be running when a same-lease control channel is
        // replaced. Keep bounded, validated intents until START is redelivered.
        if record.encode().is_err() || self.snapshot.is_none() {
            return self.bad_record(id);
        }
        let action = hex_string(&record.action);
        match self.actions.get(&record.turn) {
            Some((existing, _)) if existing == &action => return None,
            Some(_) => return self.bad_record(id),
            None => {}
        }
        if self.actions.len() >= 32
            || record.turn != self.actions.keys().next_back().copied().unwrap_or(0) + 1
        {
            return self.bad_record(id);
        }
        self.actions.insert(record.turn, (action, new_key()));
        self.next_consensus_poll = Instant::now();
        None
    }

    pub(crate) fn turn_hash(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: TurnResultHash,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        let id = record.battle_id;
        if self.tracked.map(|tracked| bridge_id(tracked.battle_id)) != Some(id) {
            return None;
        }
        if record.encode().is_err() || !self.actions.contains_key(&record.turn) {
            return self.bad_record(id);
        }
        let hash = hex_string(&record.digest.0);
        match self.hashes.get(&record.turn) {
            Some((existing, _)) if existing == &hash => return None,
            Some(_) => return self.bad_record(id),
            None => {}
        }
        if record.turn != self.hashes.keys().next_back().copied().unwrap_or(0) + 1 {
            return self.bad_record(id);
        }
        self.hashes.insert(record.turn, (hash, new_key()));
        self.next_consensus_poll = Instant::now();
        None
    }

    pub(crate) fn battle_ready(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: BattleReadyRecord,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        let id = record.battle_id;
        if self.tracked.map(|tracked| bridge_id(tracked.battle_id)) != Some(id) {
            return None;
        }
        if record.encode().is_err()
            || self
                .snapshot
                .as_ref()
                .is_some_and(|(hash, _)| *hash != hex_string(&record.party_digest.0))
            || self
                .ready_proof
                .is_some_and(|(known, _)| known != record.party_digest)
        {
            return self.bad_record(id);
        }
        self.ready_proof
            .get_or_insert_with(|| (record.party_digest, new_key()));
        self.ready_from_rom = true;
        self.next_consensus_poll = Instant::now();
        None
    }

    pub(crate) fn prepare_consensus<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
    ) {
        self.bind(fence, generation);
        let Some(tracked) = self.tracked else {
            return;
        };
        if self.queued_cancel.is_some()
            || matches!(self.pending_job, Some(Job::Cancel { .. }))
            || self.consensus_pending.is_some()
            || Instant::now() < self.next_consensus_poll
        {
            return;
        }
        let job = self.consensus_job(tracked);
        self.consensus_pending = Some(Box::pin(async move {
            let result = match &job {
                ConsensusJob::Inspect => {
                    api.battle_inspect_consensus(token, tracked.group_id, tracked.battle_id, fence)
                        .await
                }
                ConsensusJob::Commit { hash, key, records } => {
                    api.battle_commit_snapshot(
                        token,
                        tracked.group_id,
                        tracked.battle_id,
                        fence,
                        *key,
                        hash,
                        records.clone(),
                    )
                    .await
                }
                ConsensusJob::Ready { digest, key } => {
                    api.battle_ready(
                        token,
                        tracked.group_id,
                        tracked.battle_id,
                        fence,
                        *key,
                        *digest,
                    )
                    .await
                }
                ConsensusJob::Action { turn, action, key } => {
                    api.battle_submit_action(
                        token,
                        tracked.group_id,
                        tracked.battle_id,
                        fence,
                        *key,
                        *turn,
                        action,
                    )
                    .await
                }
                ConsensusJob::Hash { turn, hash, key } => {
                    api.battle_acknowledge_hash(
                        token,
                        tracked.group_id,
                        tracked.battle_id,
                        fence,
                        *key,
                        *turn,
                        hash,
                    )
                    .await
                }
            };
            ConsensusCompletion {
                fence,
                generation,
                job,
                result,
            }
        }));
    }

    pub(crate) fn prepare_commit_grant<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
    ) {
        self.bind(fence, generation);
        let Some(tracked) = self.tracked else {
            return;
        };
        if !self.commit_ready
            || self.commit_record.is_some()
            || self.commit_grant_pending.is_some()
            || self.commit_acknowledged
            || Instant::now() < self.next_commit_poll
            || tracked.kind != BattleKind::CooperativeTrainer
        {
            return;
        }
        let Some((region, ordinal)) = tracked.trainer_identity else {
            return;
        };
        let Some((source_hash, _)) = self.snapshot.as_ref() else {
            return;
        };
        let Ok(source_digest) = BattleDigest::parse(source_hash) else {
            return;
        };
        let Some((terminal, _)) = self.last_finish else {
            return;
        };
        if terminal.result != BattleFinishedResult::Won
            || terminal.turn == 0
            || terminal.terminal_hash.0 == [0; 32]
        {
            return;
        }
        let Ok(trainer) = reserve_trainer_id(
            coop_protocol::BattleKind::CooperativeTrainer,
            Some(region),
            Some(ordinal),
        ) else {
            return;
        };
        let Some(trainer) = trainer else {
            return;
        };
        self.commit_grant_pending = Some(Box::pin(async move {
            let result = api
                .battle_commit_grant(
                    token,
                    tracked.group_id,
                    tracked.battle_id,
                    fence,
                    trainer,
                    source_digest,
                    terminal.turn,
                    terminal.terminal_hash,
                )
                .await;
            CommitGrantCompletion {
                fence,
                generation,
                battle_id: tracked.battle_id,
                result,
            }
        }));
    }

    fn consensus_job(&self, tracked: Tracked) -> ConsensusJob {
        if let Some((hash, key)) = &self.snapshot {
            if !self.submitted_snapshot && self.accepted_battle == Some(tracked.battle_id) {
                ConsensusJob::Commit {
                    hash: hash.clone(),
                    key: *key,
                    records: tracked
                        .local_rewards
                        .then(|| self.party.iter().map(|mon| hex_string(mon)).collect()),
                }
            } else if self.ready_from_rom
                && !self.submitted_ready
                && self.accepted_battle == Some(tracked.battle_id)
                && self.manifest_delivered
                && self.peer_delivered
                && self.submitted_snapshot
                && self
                    .ready_proof
                    .is_some_and(|(digest, _)| *hash == hex_string(&digest.0))
                && let Some((digest, key)) = self.ready_proof
            {
                ConsensusJob::Ready { digest, key }
            } else if self.manifest_delivered
                && self.peer_delivered
                && self.start_delivered
                && let Some((&turn, (action, key))) = self.actions.iter().find(|(turn, _)| {
                    **turn > self.submitted_actions && **turn <= self.submitted_hashes + 1
                })
            {
                ConsensusJob::Action {
                    turn,
                    action: action.clone(),
                    key: *key,
                }
            } else if self.start_delivered
                && let Some((&turn, (hash, key))) = self.hashes.iter().find(|(turn, _)| {
                    **turn > self.submitted_hashes && **turn <= self.bundles_delivered
                })
            {
                ConsensusJob::Hash {
                    turn,
                    hash: hash.clone(),
                    key: *key,
                }
            } else {
                ConsensusJob::Inspect
            }
        } else {
            ConsensusJob::Inspect
        }
    }

    pub(crate) fn prepare_peer<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
    ) {
        self.bind(fence, generation);
        let (Some(tracked), Some(expected)) = (self.tracked, self.peer_expected) else {
            return;
        };
        if self.queued_cancel.is_some()
            || matches!(self.pending_job, Some(Job::Cancel { .. }))
            || !self.manifest_delivered
            || self.peer_delivered
            || self.peer_pending.is_some()
            || Instant::now() < self.next_peer_poll
        {
            return;
        }
        let cached = self.peer_cached.clone();
        self.peer_pending = Some(Box::pin(async move {
            let result = match cached {
                Some(records) => Ok(records),
                None => api
                    .battle_peer_party(token, tracked.group_id, tracked.battle_id, fence)
                    .await
                    .and_then(|view| validated_peer_chunks(view, tracked, fence, expected)),
            };
            PeerCompletion {
                fence,
                generation,
                battle_id: tracked.battle_id,
                result,
            }
        }));
    }

    pub(crate) fn finish_peer(
        &mut self,
        completion: PeerCompletion,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Vec<BattleDelivery>, SessionError> {
        if completion.fence != fence
            || completion.generation != generation
            || self.tracked.map(|tracked| tracked.battle_id) != Some(completion.battle_id)
        {
            return Ok(Vec::new());
        }
        self.peer_pending = None;
        self.next_peer_poll = Instant::now() + POLL_INTERVAL;
        match completion.result {
            Ok(records) => {
                if self.peer_delivered {
                    return Ok(Vec::new());
                }
                self.peer_cached = Some(records.clone());
                self.peer_delivered = true;
                Ok(records.into_iter().map(BattleDelivery::PeerParty).collect())
            }
            Err(BattleApiError::Unauthorized) => Err(SessionError::Unauthorized),
            Err(BattleApiError::Unavailable) => {
                self.next_peer_poll = Instant::now() + RETRY_DELAY;
                Ok(Vec::new())
            }
            Err(BattleApiError::Stale | BattleApiError::Invalid | BattleApiError::NotFound) => {
                self.reject_current();
                Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                    battle_id: bridge_id(completion.battle_id),
                    reason: 5,
                })])
            }
        }
    }

    pub(crate) fn finish_consensus(
        &mut self,
        completion: Option<ConsensusCompletion>,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Vec<BattleDelivery>, SessionError> {
        let Some(completion) = completion else {
            return Ok(Vec::new());
        };
        self.consensus_pending = None;
        if completion.fence != fence || completion.generation != generation {
            return Ok(Vec::new());
        }
        self.next_consensus_poll = Instant::now() + POLL_INTERVAL;
        let Some(tracked) = self.tracked else {
            return Ok(Vec::new());
        };
        let view = match completion.result {
            Ok(view) => view,
            Err(BattleApiError::Unauthorized) => return Err(SessionError::Unauthorized),
            Err(BattleApiError::Unavailable) => {
                self.next_consensus_poll = Instant::now() + RETRY_DELAY;
                return Ok(Vec::new());
            }
            Err(BattleApiError::Stale | BattleApiError::Invalid | BattleApiError::NotFound) => {
                self.reject_current();
                return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                    battle_id: bridge_id(tracked.battle_id),
                    reason: 5,
                })]);
            }
        };
        let abort = || {
            vec![BattleDelivery::Abort(AbortBattleRecord {
                battle_id: bridge_id(tracked.battle_id),
                reason: 5,
            })]
        };
        if view.reservation.battle_id != tracked.battle_id
            || view.reservation.group_id != tracked.group_id
            || view.reservation.api_version != ApiVersion::V1
            || view.reservation.member_character_ids != tracked.members
            || view.reservation.kind != tracked.kind
            || manifest_identity(&view.reservation, fence.character_id).map(
                |(kind, _, region, ordinal)| {
                    if kind == coop_protocol::BattleKind::Friendly {
                        None
                    } else {
                        Some((region, ordinal))
                    }
                },
            ) != Some(tracked.trainer_identity)
            || !view
                .reservation
                .member_character_ids
                .contains(&fence.character_id)
            || view.turns.len() > 32
            || view
                .commitments
                .iter()
                .flatten()
                .any(|hash| BattleDigest::parse(hash).is_err())
            || (view.start_released && view.ready != [true, true])
        {
            self.reject_current();
            return Ok(abort());
        }
        if view.reservation.status == BattleStatus::Completed
            && view.reservation.kind == BattleKind::Friendly
        {
            // A friendly completion is a server receipt only. There is no
            // bridge success command, so never synthesize one here.
            self.tracked = None;
            self.last_finish = None;
            self.queued_finish = None;
            self.clear_consensus();
            return Ok(Vec::new());
        }
        if view.reservation.status == BattleStatus::Completed
            && view.reservation.kind == BattleKind::CooperativeTrainer
        {
            // A trainer battle is complete only after the ROM acknowledged a
            // grant and a checkpoint finalize consumed that grant. Until that
            // evidence exists, keep the reservation inert and retry the
            // authorized path after reconnect.
            if self.commit_acknowledged {
                self.tracked = None;
                self.commit_record = None;
                self.commit_ready = false;
                self.commit_acknowledged = false;
                self.commit_delivered_generation = None;
                self.last_finish = None;
                self.queued_finish = None;
                self.clear_consensus();
            }
            return Ok(Vec::new());
        }
        if view.reservation.status == BattleStatus::CommitPending {
            // Trainer completion remains held by the pre-ledger server state;
            // it must not be converted into a local success or abort.
            self.queued_finish = None;
            self.finish_waiting = false;
            self.commit_ready = true;
            return Ok(Vec::new());
        }
        if !matches!(
            view.reservation.status,
            BattleStatus::Pending | BattleStatus::Accepted
        ) {
            let was_accepted = self.requester_outcome.is_some_and(|record| {
                record.battle_id == bridge_id(tracked.battle_id)
                    && record.outcome == BattleConsentOutcome::Accepted
            });
            let outcome =
                if tracked.role == BattleRole::Requester && tracked.nonce != 0 && !was_accepted {
                    match view.reservation.status {
                        BattleStatus::Declined => Some(BattleConsentOutcome::Declined),
                        BattleStatus::Expired => Some(BattleConsentOutcome::Expired),
                        _ => None,
                    }
                } else {
                    None
                };
            self.reject_current();
            let mut deliveries = Vec::new();
            if let Some(outcome) = outcome {
                self.maybe_deliver_requester_outcome(
                    BattleConsentOutcomeRecord {
                        battle_id: bridge_id(tracked.battle_id),
                        request_nonce: tracked.nonce,
                        outcome,
                    },
                    &mut deliveries,
                );
            }
            deliveries.push(BattleDelivery::Abort(AbortBattleRecord {
                battle_id: bridge_id(tracked.battle_id),
                reason: match view.reservation.status {
                    BattleStatus::Expired => 4,
                    BattleStatus::Diverged => 3,
                    _ => 1,
                },
            }));
            return Ok(deliveries);
        }
        self.accepted_battle =
            (view.reservation.status == BattleStatus::Accepted).then_some(tracked.battle_id);
        match completion.job {
            ConsensusJob::Commit { .. } => self.submitted_snapshot = true,
            ConsensusJob::Ready { .. } => self.submitted_ready = true,
            ConsensusJob::Action { turn, .. } => {
                self.submitted_actions = self.submitted_actions.max(turn)
            }
            ConsensusJob::Hash { turn, .. } => {
                self.submitted_hashes = self.submitted_hashes.max(turn)
            }
            ConsensusJob::Inspect => {}
        }
        let mut deliveries = Vec::new();
        if let Some(manifest) = &view.manifest {
            let Some((kind, local_member_slot, trainer_region, trainer_ordinal)) =
                manifest_identity(&view.reservation, fence.character_id)
            else {
                self.reject_current();
                return Ok(abort());
            };
            if manifest.battle_id != tracked.battle_id
                || manifest.member_character_ids != view.reservation.member_character_ids
            {
                self.reject_current();
                return Ok(abort());
            }
            let seed = BattleDigest::parse(&manifest.seed);
            let hashes = [
                BattleDigest::parse(&manifest.snapshot_hashes[0]),
                BattleDigest::parse(&manifest.snapshot_hashes[1]),
            ];
            let (Ok(seed), [Ok(first), Ok(second)]) = (seed, hashes) else {
                self.reject_current();
                return Ok(abort());
            };
            let peer_slot = usize::from(manifest.member_character_ids[0] == fence.character_id);
            let expected_peer = (
                manifest.member_character_ids[peer_slot],
                [first, second][peer_slot],
            );
            if self.peer_expected.is_some_and(|old| old != expected_peer) {
                self.reject_current();
                return Ok(abort());
            }
            self.peer_expected = Some(expected_peer);
            if let Some((local, _)) = &self.snapshot {
                let slot =
                    usize::from(view.reservation.member_character_ids[0] != fence.character_id);
                if local != &manifest.snapshot_hashes[slot] {
                    self.reject_current();
                    return Ok(abort());
                }
            }
            if !self.manifest_delivered {
                deliveries.push(BattleDelivery::Manifest(BattleManifestRecord {
                    battle_id: bridge_id(tracked.battle_id),
                    turn: 0,
                    seed,
                    snapshot_hashes: [first, second],
                    kind,
                    local_member_slot,
                    trainer_region,
                    trainer_ordinal,
                }));
                self.manifest_delivered = true;
            }
        }
        if !self.peer_delivered {
            return Ok(deliveries);
        }
        let local_slot = usize::from(tracked.members[0] != fence.character_id);
        if view.start_released
            && view.ready[local_slot]
            && self.ready_from_rom
            && self.submitted_ready
            && self.manifest_delivered
            && !self.start_delivered
        {
            deliveries.push(BattleDelivery::Start(BattleStartRecord {
                battle_id: bridge_id(tracked.battle_id),
            }));
            self.start_delivered = true;
        }
        if !self.start_delivered {
            return Ok(deliveries);
        }
        for (index, turn) in view.turns.iter().enumerate() {
            if turn.number != (index + 1) as u16 {
                self.reject_current();
                return Ok(abort());
            }
            if turn
                .state_hashes
                .iter()
                .flatten()
                .any(|hash| BattleDigest::parse(hash).is_err())
            {
                self.reject_current();
                return Ok(abort());
            }
            if let [Some(left), Some(right)] = &turn.state_hashes {
                if left != right {
                    self.reject_current();
                    return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                        battle_id: bridge_id(tracked.battle_id),
                        reason: 3,
                    })]);
                }
            }
            if turn.number <= self.bundles_delivered {
                continue;
            }
            if let [Some(left), Some(right)] = &turn.actions {
                let (Some(left), Some(right)) = (unhex_action(left), unhex_action(right)) else {
                    self.reject_current();
                    return Ok(abort());
                };
                let record = TurnBundleRecord {
                    battle_id: bridge_id(tracked.battle_id),
                    turn: turn.number,
                    actions: [left, right],
                };
                if record.encode().is_err() || !self.manifest_delivered {
                    self.reject_current();
                    return Ok(abort());
                }
                deliveries.push(BattleDelivery::Bundle(record));
                self.bundles_delivered = turn.number;
                self.paused_turn = None;
            } else if self.manifest_delivered
                && self.submitted_actions >= turn.number
                && self.paused_turn != Some(turn.number)
            {
                // The public view redacts both actions until the pair is complete.
                // A confirmed local submit proves that the other member is missing.
                let missing_slot = u8::from(tracked.members[0] == fence.character_id);
                deliveries.push(BattleDelivery::Pause(PauseForReconnectRecord {
                    battle_id: bridge_id(tracked.battle_id),
                    turn: turn.number,
                    missing_slot,
                }));
                self.paused_turn = Some(turn.number);
                break;
            } else {
                break;
            }
        }
        Ok(deliveries)
    }

    pub(crate) fn invalidate(&mut self) {
        if self.cold_cancel.is_some() {
            // A previous ABORT_BATTLE delivery may have died with the control
            // generation before the ROM observed it. Repeated aborts are safe.
            self.cold_abort_sent = false;
        }
        self.consensus_pending = None;
        self.peer_pending = None;
        self.accepted_battle = None;
        // A control restart may follow a peer save revision change or a
        // terminal reservation. Revalidate through the fenced endpoint.
        self.peer_cached = None;
        self.peer_delivered = false;
        self.ready_from_rom = false;
        self.submitted_ready = false;
        self.start_delivered = false;
        self.next_peer_poll = Instant::now();
        self.next_commit_poll = Instant::now();
        self.next_consensus_poll = Instant::now();
        self.manifest_delivered = false;
        self.bundles_delivered = 0;
        self.paused_turn = None;
        if let Some(Job::Reserve {
            nonce,
            kind,
            trainer_region,
            trainer_ordinal,
            ..
        }) = self.pending_job
        {
            self.queued_reserve = Some(TrainerBattleReserveRecord {
                request_nonce: nonce,
                kind,
                trainer_region,
                trainer_ordinal,
            });
        }
        if let Some(Job::Cancel {
            battle_id,
            group_id,
            key,
        }) = self.pending_job
        {
            self.queued_cancel = Some((battle_id, group_id, key));
        }
        if let Some(Job::Finish {
            battle_id,
            record,
            key,
            ..
        }) = self.pending_job
        {
            self.queued_finish = Some((record, key));
            debug_assert_eq!(bridge_id(battle_id), record.battle_id);
            self.finish_waiting = false;
            self.finish_posted = false;
        }
        if let Some(Job::FinishProbe {
            battle_id,
            record,
            key: _,
            ..
        }) = self.pending_job
        {
            self.queued_finish = None;
            debug_assert_eq!(bridge_id(battle_id), record.battle_id);
            self.finish_waiting = true;
        }
        if let Some(Job::Respond {
            battle_id,
            decision,
            key,
            ..
        }) = self.pending_job
        {
            self.queued_response = Some(BattleJoinResponseRecord {
                battle_id: bridge_id(battle_id),
                decision,
            });
            self.response_key = Some(key);
        }
        self.pending = None;
        self.pending_job = None;
        self.redeliver = self.tracked.is_some();
        self.redeliver_requester_outcome = self.requester_outcome.is_some();
        self.next_poll = Instant::now();
    }

    fn bind(&mut self, fence: LeaseFence, generation: u32) {
        if self.fence != Some(fence) || self.generation != generation {
            // Preserve nonce keys across a transport restart under one lease.
            if self.fence != Some(fence) {
                if self.fence.is_some() {
                    self.cold_reconcile = true;
                }
                self.cold_cancel = None;
                self.cold_abort_sent = false;
                self.cold_reconciled_once = false;
                self.cold_retry_reserve = None;
                self.cold_retry_used = false;
                self.pending = None;
                self.pending_job = None;
                self.reserve_keys.clear();
                self.queued_reserve = None;
                self.replacement_reserve = None;
                self.queued_cancel = None;
                self.queued_finish = None;
                self.last_finish = None;
                self.finish_waiting = false;
                self.finish_posted = false;
                self.reconcile_before_reserve = false;
                self.reserve_from_prior_generation = false;
                self.queued_response = None;
                self.tracked = None;
                self.requester_outcome = None;
                self.redeliver_requester_outcome = false;
                self.rejected_battle = None;
                self.answered = None;
                self.response_key = None;
                self.party.clear();
                self.party_count = None;
                self.snapshot = None;
                self.ready_proof = None;
                self.ready_from_rom = false;
                self.submitted_ready = false;
                self.start_delivered = false;
                self.actions.clear();
                self.hashes.clear();
                self.peer_expected = None;
                self.peer_cached = None;
                self.peer_delivered = false;
                self.commit_grant_pending = None;
                self.commit_ready = false;
                self.commit_record = None;
                self.commit_acknowledged = false;
                self.commit_delivered_generation = None;
            }
            self.invalidate();
            if self.fence == Some(fence) {
                self.reserve_from_prior_generation =
                    self.queued_reserve.is_some() && !self.reconcile_before_reserve;
                // The ROM restarts its nonce counter with the control lifecycle.
                // Only the original request needs a replay key. The newest
                // replacement gets its key after the old request settles.
                let unfinished = self.queued_reserve.map(|record| record.request_nonce);
                self.reserve_keys
                    .retain(|nonce, _| Some(*nonce) == unfinished);
            }
            self.fence = Some(fence);
            self.generation = generation;
        }
    }

    pub(crate) fn reserve(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: TrainerBattleReserveRecord,
    ) -> Option<BattleReserveRejectedRecord> {
        self.bind(fence, generation);
        if record.encode().is_err() {
            return None;
        }
        if self.cold_reconcile {
            // A replay from the previous process may still name an old ROM
            // consent request. Re-reserving it would mint a different UUID.
            return Some(BattleReserveRejectedRecord {
                request_nonce: record.request_nonce,
            });
        }
        if self.reconcile_before_reserve {
            // A second ROM restart can replace the still-waiting request while
            // the original HTTP reserve is being settled. Preserve only the
            // original replay key and the newest replacement key.
            if self
                .replacement_reserve
                .is_some_and(|old| old.request_nonce == record.request_nonce)
            {
                return None;
            }
            self.replacement_reserve = Some(record);
            return None;
        }
        if self.queued_reserve.is_some() {
            if self.reserve_from_prior_generation && self.tracked.is_none() {
                // Replay the old idempotency key to settle its in-flight HTTP
                // request before issuing a replacement. A /current poll can
                // race a late commit of that request.
                self.replacement_reserve = Some(record);
                self.reserve_from_prior_generation = false;
                self.reconcile_before_reserve = true;
                self.next_poll = Instant::now();
            }
            return None;
        }
        if self.reserve_keys.contains_key(&record.request_nonce) {
            return None;
        }
        if matches!(self.pending_job, Some(Job::Reserve { .. })) {
            return None;
        }
        // A ROM can have only one consent prompt. Bound replay keys to the lease.
        if self.reserve_keys.len() >= 64 {
            return Some(BattleReserveRejectedRecord {
                request_nonce: record.request_nonce,
            });
        }
        if let Some(tracked) = self.tracked {
            if tracked.role != BattleRole::Requester
                || self.queued_cancel.is_some()
                || matches!(self.pending_job, Some(Job::Cancel { .. }))
            {
                return None;
            }
            // A new ROM consent request cannot coexist with the requester's
            // old live reservation. Cancel it before reserving again.
            self.queued_cancel = Some((
                tracked.battle_id,
                tracked.group_id,
                IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"),
            ));
        }
        self.cold_retry_used = false;
        self.reserve_keys.insert(
            record.request_nonce,
            IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"),
        );
        // A new requester nonce starts a new consent lifecycle. Any terminal
        // outcome from the previous battle has already been made replayable
        // on the bridge; do not let it attach to this reservation.
        self.requester_outcome = None;
        self.redeliver_requester_outcome = false;
        self.queued_reserve = Some(record);
        self.next_poll = Instant::now();
        None
    }

    pub(crate) fn respond(
        &mut self,
        fence: LeaseFence,
        generation: u32,
        record: BattleJoinResponseRecord,
    ) -> Option<AbortBattleRecord> {
        self.bind(fence, generation);
        if self.cold_reconcile {
            return None;
        }
        let Some(tracked) = self.tracked else {
            return Some(AbortBattleRecord {
                battle_id: record.battle_id,
                reason: 5,
            });
        };
        if record.encode().is_err()
            || record.battle_id != bridge_id(tracked.battle_id)
            || tracked.role != BattleRole::Responder
        {
            return Some(AbortBattleRecord {
                battle_id: record.battle_id,
                reason: 5,
            });
        }
        if let Some((battle_id, decision)) = self.answered {
            if battle_id == tracked.battle_id {
                return (decision != record.decision).then_some(AbortBattleRecord {
                    battle_id: record.battle_id,
                    reason: 5,
                });
            }
        }
        if self
            .queued_response
            .is_some_and(|queued| queued.decision != record.decision)
        {
            return Some(AbortBattleRecord {
                battle_id: record.battle_id,
                reason: 5,
            });
        }
        self.response_key
            .get_or_insert_with(|| IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"));
        self.queued_response = Some(record);
        self.next_poll = Instant::now();
        None
    }

    pub(crate) fn prepare<A: CloudApi>(
        &mut self,
        api: &'a A,
        token: AccessToken,
        fence: LeaseFence,
        generation: u32,
    ) {
        self.bind(fence, generation);
        if self.pending.is_some() || Instant::now() < self.next_poll {
            return;
        }
        let job = if self.cold_reconcile && self.queued_cancel.is_none() {
            Job::Poll
        } else if let Some((battle_id, group_id, key)) = self.queued_cancel.take() {
            Job::Cancel {
                battle_id,
                group_id,
                key,
            }
        } else if self.finish_waiting {
            let Some((record, key)) = self.last_finish else {
                return;
            };
            let Some(tracked) = self.tracked else {
                return;
            };
            Job::FinishProbe {
                battle_id: tracked.battle_id,
                group_id: tracked.group_id,
                record,
                key,
            }
        } else if let Some((record, key)) = self.queued_finish.take() {
            let Some(tracked) = self.tracked else {
                return;
            };
            Job::Finish {
                battle_id: tracked.battle_id,
                group_id: tracked.group_id,
                record,
                key,
            }
        } else if let Some(record) = self.queued_response.take() {
            let Some(tracked) = self.tracked else {
                return;
            };
            Job::Respond {
                battle_id: tracked.battle_id,
                group_id: tracked.group_id,
                decision: record.decision,
                key: self.response_key.expect("response key set with decision"),
            }
        } else if let Some(record) = self.queued_reserve.take() {
            if !self.reconcile_before_reserve {
                self.reserve_from_prior_generation = false;
            }
            let key = self.reserve_keys[&record.request_nonce];
            Job::Reserve {
                nonce: record.request_nonce,
                kind: record.kind,
                trainer_region: record.trainer_region,
                trainer_ordinal: record.trainer_ordinal,
                key,
            }
        } else if Instant::now() >= self.next_poll {
            Job::Poll
        } else {
            return;
        };
        self.pending_job = Some(job);
        let tracked = self.tracked;
        let finish_posted = self.finish_posted;
        self.pending = Some(Box::pin(async move {
            let result = work(api, token, fence, job, tracked, finish_posted).await;
            Completion {
                fence,
                generation,
                job,
                result,
            }
        }));
    }

    pub(crate) async fn next(&mut self) -> BattleCompletion {
        let reservation = &mut self.pending;
        let consensus = &mut self.consensus_pending;
        let peer = &mut self.peer_pending;
        let commit_grant = &mut self.commit_grant_pending;
        let next_poll = self.next_poll;
        tokio::select! {
            result = async { match reservation { Some(future) => Some(future.await), None => { tokio::time::sleep_until(next_poll).await; None } } } => BattleCompletion::Reservation(result),
            result = async { match consensus { Some(future) => Some(future.await), None => std::future::pending().await } } => BattleCompletion::Consensus(result),
            result = async { match peer { Some(future) => future.await, None => std::future::pending().await } } => BattleCompletion::Peer(result),
            result = async { match commit_grant { Some(future) => future.await, None => std::future::pending().await } } => BattleCompletion::CommitGrant(result),
        }
    }

    pub(crate) fn finish_commit_grant(
        &mut self,
        completion: CommitGrantCompletion,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Vec<BattleDelivery>, SessionError> {
        self.commit_grant_pending = None;
        if completion.fence != fence
            || completion.generation != generation
            || self.tracked.map(|tracked| tracked.battle_id) != Some(completion.battle_id)
        {
            return Ok(Vec::new());
        }
        self.next_commit_poll = Instant::now() + POLL_INTERVAL;
        let tracked = self.tracked.expect("completion is bound to tracked battle");
        let Some((region, ordinal)) = tracked.trainer_identity else {
            return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                battle_id: bridge_id(completion.battle_id),
                reason: 5,
            })]);
        };
        let expected_trainer = reserve_trainer_id(
            coop_protocol::BattleKind::CooperativeTrainer,
            Some(region),
            Some(ordinal),
        )
        .map_err(|_| SessionError::Realtime)?
        .ok_or(SessionError::Realtime)?;
        let Some((source_hash, _)) = self.snapshot.as_ref() else {
            return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                battle_id: bridge_id(completion.battle_id),
                reason: 5,
            })]);
        };
        let source_digest = BattleDigest::parse(source_hash).map_err(|_| SessionError::Realtime)?;
        let Some((terminal, _)) = self.last_finish else {
            return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                battle_id: bridge_id(completion.battle_id),
                reason: 5,
            })]);
        };
        match completion.result {
            Err(BattleApiError::Unauthorized) => Err(SessionError::Unauthorized),
            Err(BattleApiError::Unavailable) => Ok(Vec::new()),
            Err(BattleApiError::Stale | BattleApiError::Invalid | BattleApiError::NotFound) => {
                self.reject_current();
                Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                    battle_id: bridge_id(completion.battle_id),
                    reason: 5,
                })])
            }
            Ok(grant)
                if valid_commit_grant(
                    &grant,
                    completion.battle_id,
                    fence,
                    &expected_trainer,
                    source_digest,
                    terminal.turn,
                    terminal.terminal_hash,
                ) =>
            {
                let record = BattleCommitRecord {
                    battle_id: bridge_id(grant.battle_id),
                    commit_id: bridge_id(grant.grant_id.uuid()),
                    trainer_region: region,
                    trainer_ordinal: ordinal,
                    source_revision: grant.source_revision.value(),
                };
                if record.encode().is_err() {
                    self.reject_current();
                    return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                        battle_id: bridge_id(completion.battle_id),
                        reason: 5,
                    })]);
                }
                self.commit_record = Some(record);
                self.commit_delivered_generation = Some(generation);
                Ok(vec![BattleDelivery::Commit(record)])
            }
            Ok(_) => {
                self.reject_current();
                Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                    battle_id: bridge_id(completion.battle_id),
                    reason: 5,
                })])
            }
        }
    }

    pub(crate) fn finish(
        &mut self,
        completion: Option<Completion>,
        fence: LeaseFence,
        generation: u32,
    ) -> Result<Vec<BattleDelivery>, SessionError> {
        let Some(completion) = completion else {
            return Ok(Vec::new());
        };
        self.pending = None;
        self.pending_job = None;
        if completion.fence != fence || completion.generation != generation {
            return Ok(Vec::new());
        }
        self.next_poll = Instant::now() + POLL_INTERVAL;
        if self.cold_reconciled_once
            && self.tracked.is_none()
            && matches!(completion.job, Job::Reserve { .. })
            && matches!(&completion.result, Err(BattleApiError::Stale))
        {
            // A reserve from the previous process may have committed after
            // the first empty /current. Reconcile the server lock before any
            // later ROM request can be honored.
            self.cold_reconcile = true;
            self.cold_cancel = None;
            self.cold_abort_sent = false;
            let rejected = if let Job::Reserve {
                nonce,
                kind,
                trainer_region,
                trainer_ordinal,
                key,
            } = completion.job
            {
                if !self.cold_retry_used {
                    self.cold_retry_reserve = Some((
                        TrainerBattleReserveRecord {
                            request_nonce: nonce,
                            kind,
                            trainer_region,
                            trainer_ordinal,
                        },
                        key,
                    ));
                    None
                } else {
                    Some(BattleDelivery::ReserveRejected(
                        BattleReserveRejectedRecord {
                            request_nonce: nonce,
                        },
                    ))
                }
            } else {
                None
            };
            self.next_poll = Instant::now();
            return Ok(rejected.into_iter().collect());
        }
        if self.cold_reconcile {
            match (completion.job, completion.result) {
                (Job::Poll, Ok(None) | Err(BattleApiError::NotFound)) => {
                    let deliveries = self
                        .cold_cancel
                        .filter(|_| !self.cold_abort_sent)
                        .map(|(battle_id, _, _)| {
                            BattleDelivery::Abort(AbortBattleRecord {
                                battle_id: bridge_id(battle_id),
                                reason: 1,
                            })
                        })
                        .into_iter()
                        .collect();
                    if self.cold_cancel.is_some() {
                        self.cold_abort_sent = true;
                    }
                    self.cold_reconcile = false;
                    self.cold_reconciled_once = true;
                    if let Some((record, key)) = self.cold_retry_reserve.take() {
                        self.reserve_keys.insert(record.request_nonce, key);
                        self.queued_reserve = Some(record);
                        self.cold_retry_used = true;
                    }
                    self.next_poll = Instant::now();
                    return Ok(deliveries);
                }
                (Job::Poll, Ok(Some(view)))
                    if matches!(
                        view.status,
                        BattleStatus::Pending
                            | BattleStatus::Accepted
                            | BattleStatus::CommitPending
                    ) =>
                {
                    if self
                        .cold_cancel
                        .is_none_or(|old| old.0 != view.battle_id || old.1 != view.group_id)
                    {
                        self.cold_abort_sent = false;
                    }
                    let cancel = self
                        .cold_cancel
                        .filter(|old| old.0 == view.battle_id && old.1 == view.group_id)
                        .unwrap_or_else(|| (view.battle_id, view.group_id, new_key()));
                    self.cold_cancel = Some(cancel);
                    self.queued_cancel = Some(cancel);
                    self.next_poll = Instant::now();
                }
                (
                    Job::Cancel {
                        battle_id,
                        group_id,
                        key,
                    },
                    Err(BattleApiError::Unavailable),
                ) => {
                    self.queued_cancel = Some((battle_id, group_id, key));
                    self.next_poll = Instant::now() + RETRY_DELAY;
                }
                (Job::Cancel { .. }, Ok(Some(view))) if view.status == BattleStatus::Cancelled => {
                    // Confirm absence through /current before opening the gate.
                    self.next_poll = Instant::now();
                    if !self.cold_abort_sent {
                        self.cold_abort_sent = true;
                        return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(view.battle_id),
                            reason: 1,
                        })]);
                    }
                }
                (_, Err(BattleApiError::Unauthorized)) => return Err(SessionError::Unauthorized),
                _ => self.next_poll = Instant::now() + RETRY_DELAY,
            }
            return Ok(Vec::new());
        }
        if let Job::Finish {
            battle_id,
            record,
            key,
            ..
        }
        | Job::FinishProbe {
            battle_id,
            record,
            key,
            ..
        } = completion.job
        {
            match completion.result {
                Err(BattleApiError::Unauthorized) => return Err(SessionError::Unauthorized),
                Err(BattleApiError::Unavailable) => {
                    if matches!(completion.job, Job::Finish { .. }) {
                        self.queued_finish = Some((record, key));
                        self.finish_waiting = false;
                    } else {
                        self.queued_finish = None;
                        self.finish_waiting = true;
                    }
                    self.next_poll = Instant::now() + RETRY_DELAY;
                    return Ok(Vec::new());
                }
                Err(BattleApiError::Stale) => {
                    if matches!(completion.job, Job::FinishProbe { .. }) {
                        if let Some(tracked) = self.tracked {
                            self.queued_cancel.get_or_insert((
                                tracked.battle_id,
                                tracked.group_id,
                                new_key(),
                            ));
                        }
                        self.reject_current();
                        return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(battle_id),
                            reason: 5,
                        })]);
                    }
                    // A conflict means the peer hash may not be paired yet;
                    // inspect consensus before issuing this key again.
                    self.queued_finish = None;
                    self.finish_waiting = true;
                    self.next_poll = Instant::now() + RETRY_DELAY;
                    return Ok(Vec::new());
                }
                Err(BattleApiError::Invalid | BattleApiError::NotFound) | Ok(None) => {
                    self.reject_current();
                    return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                        battle_id: bridge_id(battle_id),
                        reason: 5,
                    })]);
                }
                Ok(Some(view)) => {
                    let identity_ok = self.tracked.is_some_and(|tracked| {
                        let trainer_identity = manifest_identity(&view, fence.character_id).map(
                            |(kind, _, region, ordinal)| {
                                if kind == coop_protocol::BattleKind::Friendly {
                                    None
                                } else {
                                    Some((region, ordinal))
                                }
                            },
                        );
                        view.group_id == tracked.group_id
                            && view.member_character_ids == tracked.members
                            && view.kind == tracked.kind
                            && trainer_identity == Some(tracked.trainer_identity)
                    });
                    if !identity_ok
                        || view.battle_id != battle_id
                        || matches!(
                            view.status,
                            BattleStatus::Diverged
                                | BattleStatus::Expired
                                | BattleStatus::Cancelled
                        )
                    {
                        self.reject_current();
                        return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(battle_id),
                            reason: match view.status {
                                BattleStatus::Diverged => 3,
                                BattleStatus::Expired => 4,
                                _ => 5,
                            },
                        })]);
                    }
                    if view.status == BattleStatus::FinishReady {
                        self.queued_finish = Some((record, key));
                        self.finish_waiting = false;
                        self.finish_posted = false;
                        return Ok(Vec::new());
                    }
                    if view.kind == BattleKind::Friendly && view.status == BattleStatus::Completed {
                        // There is no safe bridge command for a terminal
                        // success receipt yet. Keep this as a server-side
                        // receipt only and do not claim success to the ROM.
                        self.tracked = None;
                        self.last_finish = None;
                        self.queued_finish = None;
                        self.finish_waiting = false;
                        self.clear_consensus();
                        return Ok(Vec::new());
                    }
                    if matches!(
                        view.status,
                        BattleStatus::Accepted | BattleStatus::CommitPending
                    ) {
                        // Accepted means the peer has not attested yet;
                        // CommitPending is deliberately held without any
                        // progression or save side effect.
                        if view.status == BattleStatus::Accepted {
                            self.queued_finish = None;
                            self.finish_waiting = true;
                            if matches!(completion.job, Job::Finish { .. }) {
                                self.finish_posted = true;
                            }
                        } else {
                            self.queued_finish = None;
                            self.finish_waiting = false;
                            if matches!(completion.job, Job::Finish { .. }) {
                                self.finish_posted = true;
                            }
                        }
                        return Ok(Vec::new());
                    }
                    self.reject_current();
                    return Ok(vec![BattleDelivery::Abort(AbortBattleRecord {
                        battle_id: bridge_id(battle_id),
                        reason: 5,
                    })]);
                }
            }
        } else {
            if let Job::Reserve { nonce, .. } = completion.job {
                if matches!(
                    &completion.result,
                    Err(BattleApiError::Invalid | BattleApiError::NotFound)
                ) {
                    return Ok(vec![BattleDelivery::ReserveRejected(
                        BattleReserveRejectedRecord {
                            request_nonce: nonce,
                        },
                    )]);
                }
            }
            let view = match completion.result {
                Ok(view) => view,
                Err(BattleApiError::Unauthorized) => return Err(SessionError::Unauthorized),
                Err(BattleApiError::Unavailable) => {
                    match completion.job {
                        Job::Cancel {
                            battle_id,
                            group_id,
                            key,
                        } => {
                            self.queued_cancel = Some((battle_id, group_id, key));
                        }
                        Job::Reserve {
                            nonce,
                            kind,
                            trainer_region,
                            trainer_ordinal,
                            ..
                        } => {
                            self.queued_reserve = Some(TrainerBattleReserveRecord {
                                request_nonce: nonce,
                                kind,
                                trainer_region,
                                trainer_ordinal,
                            })
                        }
                        Job::Respond {
                            battle_id,
                            decision,
                            key,
                            ..
                        } => {
                            self.queued_response = Some(BattleJoinResponseRecord {
                                battle_id: bridge_id(battle_id),
                                decision,
                            });
                            self.response_key = Some(key);
                        }
                        Job::Finish { .. } => unreachable!(),
                        Job::FinishProbe { .. } => unreachable!(),
                        Job::Poll => {}
                    }
                    self.next_poll = Instant::now() + RETRY_DELAY;
                    return Ok(Vec::new());
                }
                Err(BattleApiError::Invalid | BattleApiError::Stale | BattleApiError::NotFound) => {
                    None
                }
            };
            if matches!(completion.job, Job::Reserve { .. }) && self.reconcile_before_reserve {
                self.reconcile_before_reserve = false;
                if let Job::Reserve { nonce, .. } = completion.job {
                    self.reserve_keys.remove(&nonce);
                }
                self.queued_reserve = self.replacement_reserve.take();
                if let Some(record) = self.queued_reserve {
                    self.reserve_keys.insert(
                        record.request_nonce,
                        IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"),
                    );
                }
                self.next_poll = Instant::now();
                if let Some(old) = &view {
                    if old.initiator_character_id == fence.character_id
                        && matches!(old.status, BattleStatus::Pending | BattleStatus::Accepted)
                    {
                        self.queued_cancel = Some((
                            old.battle_id,
                            old.group_id,
                            IdempotencyKey::new(Uuid::new_v4()).expect("UUID is nonnil"),
                        ));
                        return Ok(Vec::new());
                    }
                }
                return Ok(Vec::new());
            }
            if let Job::Respond {
                battle_id,
                decision,
                ..
            } = completion.job
            {
                if view.is_some() {
                    self.answered = Some((battle_id, decision));
                    self.queued_response = None;
                }
            }
            let mut deliveries = Vec::new();
            if self.tracked.is_none()
                && self.redeliver_requester_outcome
                && let Some(record) = self.requester_outcome
            {
                self.maybe_deliver_requester_outcome(record, &mut deliveries);
            }
            match view {
                Some(view) => {
                    if self.rejected_battle == Some(view.battle_id) {
                        return Ok(deliveries);
                    }
                    self.rejected_battle = None;
                    if view.status == BattleStatus::Completed {
                        if view.kind == BattleKind::Friendly {
                            self.tracked = None;
                            self.last_finish = None;
                            self.clear_consensus();
                            return Ok(deliveries);
                        }
                        if self.commit_acknowledged {
                            self.tracked = None;
                            self.commit_record = None;
                            self.commit_ready = false;
                            self.commit_acknowledged = false;
                            self.commit_delivered_generation = None;
                            self.last_finish = None;
                            self.clear_consensus();
                        }
                        return Ok(deliveries);
                    }
                    if view.status == BattleStatus::CommitPending {
                        // This status is a server-held pre-ledger receipt. It
                        // carries no progression authority and is deliberately
                        // kept inert until a future ledger integration exists.
                        self.commit_ready = true;
                        return Ok(deliveries);
                    }
                    if !matches!(view.status, BattleStatus::Pending | BattleStatus::Accepted) {
                        let role = if view.initiator_character_id == fence.character_id {
                            BattleRole::Requester
                        } else {
                            BattleRole::Responder
                        };
                        let nonce = match completion.job {
                            Job::Reserve { nonce, .. } => nonce,
                            _ => self
                                .tracked
                                .filter(|old| old.battle_id == view.battle_id)
                                .map_or(0, |old| old.nonce),
                        };
                        let was_accepted = self.requester_outcome.is_some_and(|record| {
                            record.battle_id == bridge_id(view.battle_id)
                                && record.outcome == BattleConsentOutcome::Accepted
                        });
                        if was_accepted {
                            self.requester_outcome = None;
                            self.redeliver_requester_outcome = false;
                        }
                        if role == BattleRole::Requester
                            && nonce != 0
                            && matches!(completion.job, Job::Reserve { .. })
                            && self.tracked.map(|old| old.battle_id) != Some(view.battle_id)
                        {
                            // An idempotent reserve can return a terminal view
                            // before this ROM has received its battle ID. Bind
                            // the nonce first so the outcome can be validated.
                            deliveries.push(BattleDelivery::Offer(BattleJoinOfferRecord {
                                battle_id: bridge_id(view.battle_id),
                                kind: bridge_kind(view.kind),
                                role,
                                request_nonce: nonce,
                            }));
                        }
                        if role == BattleRole::Requester && nonce != 0 && !was_accepted {
                            let outcome = match view.status {
                                BattleStatus::Declined => Some(BattleConsentOutcome::Declined),
                                BattleStatus::Expired => Some(BattleConsentOutcome::Expired),
                                _ => None,
                            };
                            if let Some(outcome) = outcome {
                                self.maybe_deliver_requester_outcome(
                                    BattleConsentOutcomeRecord {
                                        battle_id: bridge_id(view.battle_id),
                                        request_nonce: nonce,
                                        outcome,
                                    },
                                    &mut deliveries,
                                );
                            }
                        }
                        let reason = match view.status {
                            BattleStatus::Expired => 4,
                            BattleStatus::Diverged => 3,
                            _ => 1,
                        };
                        deliveries.push(BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(view.battle_id),
                            reason,
                        }));
                        self.tracked = None;
                        self.redeliver = false;
                        self.response_key = None;
                        self.answered = None;
                        self.clear_consensus();
                        return Ok(deliveries);
                    }
                    let nonce = match completion.job {
                        Job::Reserve { nonce, .. } => nonce,
                        _ => self
                            .tracked
                            .filter(|old| old.battle_id == view.battle_id)
                            .map_or(0, |old| old.nonce),
                    };
                    let role = if view.initiator_character_id == fence.character_id {
                        BattleRole::Requester
                    } else {
                        BattleRole::Responder
                    };
                    let Some((kind, _, region, ordinal)) =
                        manifest_identity(&view, fence.character_id)
                    else {
                        return Ok(deliveries);
                    };
                    let tracked = Tracked {
                        group_id: view.group_id,
                        battle_id: view.battle_id,
                        members: view.member_character_ids,
                        kind: view.kind,
                        trainer_identity: if kind == coop_protocol::BattleKind::Friendly {
                            None
                        } else {
                            Some((region, ordinal))
                        },
                        role,
                        nonce,
                        local_rewards: view.reward_mode == BattleRewardMode::Local,
                    };
                    if let Some(old) = self
                        .tracked
                        .filter(|old| old.battle_id != tracked.battle_id)
                    {
                        deliveries.push(BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(old.battle_id),
                            reason: 1,
                        }));
                        self.response_key = None;
                        self.answered = None;
                        self.clear_consensus();
                    }
                    if role == BattleRole::Requester && nonce == 0 {
                        return Ok(deliveries);
                    }
                    let redeliver_offer = self.redeliver
                        && !(role == BattleRole::Requester && self.requester_outcome.is_some());
                    if self.tracked != Some(tracked) || redeliver_offer {
                        if self.tracked.map(|old| old.battle_id) != Some(tracked.battle_id) {
                            self.clear_consensus();
                            self.last_finish = None;
                            self.commit_ready = false;
                            self.commit_record = None;
                            self.commit_acknowledged = false;
                            self.commit_delivered_generation = None;
                        }
                        self.tracked = Some(tracked);
                        self.redeliver = false;
                        deliveries.push(BattleDelivery::Offer(BattleJoinOfferRecord {
                            battle_id: bridge_id(view.battle_id),
                            kind: bridge_kind(view.kind),
                            role,
                            request_nonce: nonce,
                        }));
                    }
                    // A responder can queue its local acceptance before the
                    // server records it. Buffer early party chunks, but only
                    // publish their digest after this fenced accepted view.
                    self.accepted_battle =
                        (view.status == BattleStatus::Accepted).then_some(view.battle_id);
                    if role == BattleRole::Requester
                        && nonce != 0
                        && view.status == BattleStatus::Accepted
                    {
                        self.maybe_deliver_requester_outcome(
                            BattleConsentOutcomeRecord {
                                battle_id: bridge_id(view.battle_id),
                                request_nonce: nonce,
                                outcome: BattleConsentOutcome::Accepted,
                            },
                            &mut deliveries,
                        );
                    }
                }
                None => {
                    if let Some(old) = self.tracked.take() {
                        let was_accepted = self.requester_outcome.is_some_and(|record| {
                            record.battle_id == bridge_id(old.battle_id)
                                && record.outcome == BattleConsentOutcome::Accepted
                        });
                        if was_accepted {
                            self.requester_outcome = None;
                            self.redeliver_requester_outcome = false;
                        }
                        if old.role == BattleRole::Requester && old.nonce != 0 && !was_accepted {
                            self.maybe_deliver_requester_outcome(
                                BattleConsentOutcomeRecord {
                                    battle_id: bridge_id(old.battle_id),
                                    request_nonce: old.nonce,
                                    outcome: BattleConsentOutcome::Expired,
                                },
                                &mut deliveries,
                            );
                        }
                        self.clear_consensus();
                        deliveries.push(BattleDelivery::Abort(AbortBattleRecord {
                            battle_id: bridge_id(old.battle_id),
                            reason: 4,
                        }));
                        self.response_key = None;
                        self.answered = None;
                    }
                }
            }
            Ok(deliveries)
        }
    }
}

pub(crate) enum BattleDelivery {
    Offer(BattleJoinOfferRecord),
    ReserveRejected(BattleReserveRejectedRecord),
    ConsentOutcome(BattleConsentOutcomeRecord),
    Abort(AbortBattleRecord),
    Manifest(BattleManifestRecord),
    Start(BattleStartRecord),
    PeerParty(PartySnapshotChunk),
    Bundle(TurnBundleRecord),
    Pause(PauseForReconnectRecord),
    Commit(BattleCommitRecord),
}

async fn work<A: CloudApi>(
    api: &A,
    token: AccessToken,
    fence: LeaseFence,
    job: Job,
    tracked: Option<Tracked>,
    finish_posted: bool,
) -> Result<Option<BattleReservationView>, BattleApiError> {
    if let Job::Cancel {
        battle_id,
        group_id,
        key,
    } = job
    {
        return api
            .battle_action(token, group_id, battle_id, fence, "cancel", key)
            .await
            .and_then(|view| {
                if view.api_version != ApiVersion::V1
                    || view.battle_id != battle_id
                    || view.group_id != group_id
                    || !view.member_character_ids.contains(&fence.character_id)
                    || view.status != BattleStatus::Cancelled
                {
                    Err(BattleApiError::Invalid)
                } else {
                    Ok(Some(view))
                }
            });
    }
    if let Job::Respond {
        battle_id,
        group_id,
        decision,
        key,
    } = job
    {
        let action = match decision {
            BattleDecision::Accept => "accept",
            BattleDecision::Decline => "decline",
        };
        for attempt in 0..2 {
            match api
                .battle_action(token.clone(), group_id, battle_id, fence, action, key)
                .await
            {
                Err(BattleApiError::Unavailable) if attempt == 0 => {
                    tokio::time::sleep(RETRY_DELAY).await
                }
                other => {
                    return other.and_then(|view| {
                        if view.api_version != ApiVersion::V1
                            || view.battle_id != battle_id
                            || view.group_id != group_id
                            || !view.member_character_ids.contains(&fence.character_id)
                        {
                            Err(BattleApiError::Invalid)
                        } else {
                            Ok(Some(view))
                        }
                    });
                }
            }
        }
        unreachable!();
    }
    if let Job::Finish {
        battle_id,
        group_id,
        record,
        key,
    } = job
    {
        return api
            .battle_finish(
                token,
                group_id,
                battle_id,
                fence,
                key,
                record.result,
                record.turn,
                record.terminal_hash,
            )
            .await
            .and_then(|view| {
                if view.api_version != ApiVersion::V1
                    || view.battle_id != battle_id
                    || view.group_id != group_id
                    || tracked.is_some_and(|tracked| view.member_character_ids != tracked.members)
                    || !view.member_character_ids.contains(&fence.character_id)
                {
                    Err(BattleApiError::Invalid)
                } else {
                    Ok(Some(view))
                }
            });
    }
    if let Job::FinishProbe {
        battle_id,
        group_id,
        record,
        key: _,
    } = job
    {
        let view = api
            .battle_inspect_consensus(token.clone(), group_id, battle_id, fence)
            .await?;
        if view.reservation.api_version != ApiVersion::V1
            || view.reservation.battle_id != battle_id
            || view.reservation.group_id != group_id
            || tracked.is_some_and(|tracked| {
                view.reservation.member_character_ids != tracked.members
                    || view.reservation.kind != tracked.kind
            })
        {
            return Err(BattleApiError::Invalid);
        }
        if matches!(
            view.reservation.status,
            BattleStatus::Completed
                | BattleStatus::CommitPending
                | BattleStatus::Diverged
                | BattleStatus::Expired
                | BattleStatus::Cancelled
        ) {
            return Ok(Some(view.reservation));
        }
        if !finish_probe_ready(&view, record) {
            return Ok(Some(view.reservation));
        }
        if finish_posted {
            return Ok(Some(view.reservation));
        }
        let mut ready = view.reservation;
        ready.status = BattleStatus::FinishReady;
        return Ok(Some(ready));
    }
    let snapshot = api
        .online_snapshot(
            token.clone(),
            coop_cloud::OnlineSnapshotRequest {
                api_version: ApiVersion::V1,
                fence,
                incoming_after: None,
            },
        )
        .await
        .map_err(|error| match error {
            crate::online::OnlineError::Unauthorized => BattleApiError::Unauthorized,
            crate::online::OnlineError::Stale => BattleApiError::Stale,
            crate::online::OnlineError::InvalidResponse => BattleApiError::Invalid,
            crate::online::OnlineError::Unavailable => BattleApiError::Unavailable,
        })?;
    if snapshot.api_version != ApiVersion::V1 {
        return Err(BattleApiError::Invalid);
    }
    let Some(group) = snapshot.group else {
        return Ok(None);
    };
    let group_id = group.group.group_id;
    let members = group.group.members.map(|member| member.character_id);
    if !members.contains(&fence.character_id) {
        return Err(BattleApiError::Invalid);
    }
    let requested_trainer_id = match job {
        Job::Reserve {
            kind,
            trainer_region,
            trainer_ordinal,
            ..
        } => reserve_trainer_id(kind, trainer_region, trainer_ordinal)?,
        _ => None,
    };
    let mut result = None;
    for attempt in 0..2 {
        let call = match job {
            Job::Poll => {
                // A terminal reservation is no longer /current. Query only
                // the exact battle already shown to this member, under the
                // current group and lease, before discovering a new offer.
                if let Some(old) = tracked.filter(|old| old.group_id == group_id) {
                    match api
                        .battle_inspect_consensus(token.clone(), group_id, old.battle_id, fence)
                        .await
                    {
                        Ok(view) if view.reservation.battle_id == old.battle_id => {
                            Ok(Some(view.reservation))
                        }
                        Ok(_) => Err(BattleApiError::Invalid),
                        Err(BattleApiError::NotFound) => {
                            api.battle_current(token.clone(), group_id, fence).await
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    api.battle_current(token.clone(), group_id, fence).await
                }
            }
            Job::Reserve { kind, key, .. } => api
                .battle_reserve(
                    token.clone(),
                    group_id,
                    fence,
                    cloud_kind(kind),
                    requested_trainer_id.clone(),
                    key,
                )
                .await
                .map(Some),
            Job::Respond { .. } | Job::Cancel { .. } => unreachable!(),
            Job::Finish { .. } | Job::FinishProbe { .. } => unreachable!(),
        };
        match call {
            Err(BattleApiError::Unavailable) if attempt == 0 => {
                tokio::time::sleep(RETRY_DELAY).await
            }
            other => {
                result = Some(other);
                break;
            }
        }
    }
    let view = result.expect("bounded retry returned")?;
    if view
        .as_ref()
        .is_some_and(|view| !valid_view(view, group_id, members, fence.character_id))
    {
        return Err(BattleApiError::Invalid);
    }
    if let (Job::Reserve { kind, .. }, Some(view)) = (job, view.as_ref()) {
        if !valid_reserve_view(
            view,
            kind,
            requested_trainer_id.as_ref(),
            fence.character_id,
        ) {
            return Err(BattleApiError::Invalid);
        }
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{BattleApiError, BattleConsensusView, BattleKind, BattleReservationView};

    fn reservation_json() -> serde_json::Value {
        json!({
            "api_version": 1,
            "battle_id": "0f8844a0-6b64-4561-9772-e8ae49783ca7",
            "group_id": "2c1d8aa2-1f08-473c-a147-49b191430e72",
            "initiator_character_id": "7ca5c789-57cb-4d06-b68d-176ff05f69c5",
            "member_character_ids": [
                "7ca5c789-57cb-4d06-b68d-176ff05f69c5",
                "7e84b14b-91b7-4a66-9866-c86033f78f4a"
            ],
            "kind": "COOPERATIVE_TRAINER",
            "trainer_id": "HOENN:TRAINER_WALLY_1",
            "status": "PENDING",
            "expires_at": 42
        })
    }

    #[test]
    fn battle_wire_shapes_match_server_contract() {
        let reservation: BattleReservationView =
            serde_json::from_value(reservation_json()).expect("reservation contract");
        assert_eq!(reservation.kind, BattleKind::CooperativeTrainer);

        let consensus: BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation_json(),
            "commitments": [null, null],
            "manifest": null,
            "turns": [{
                "number": 1,
                "actions": [null, null],
                "state_hashes": [null, null],
                "action_keys": [null, null],
                "hash_keys": [null, null]
            }]
        }))
        .expect("consensus contract");
        assert_eq!(consensus.turns[0].number, 1);
        assert!(consensus.turns[0].actions.iter().all(Option::is_none));
        let mut unexpected = reservation_json();
        unexpected["unexpected"] = json!("field");
        assert!(serde_json::from_value::<BattleReservationView>(unexpected).is_err());
    }

    #[test]
    fn commit_grant_binds_caller_battle_revision_and_terminal_evidence() {
        let fence = fence();
        let battle_id = uuid::Uuid::from_u128(500);
        let mut grant: super::BattleCommitGrantView = serde_json::from_value(json!({
            "grant_id": uuid::Uuid::from_u128(501),
            "character_id": fence.character_id,
            "battle_id": battle_id,
            "trainer_id": "HOENN:TRAINER_WALLY_1",
            "source_snapshot_id": uuid::Uuid::from_u128(502),
            "source_revision": fence.current_revision,
            "source_party_digest": "a".repeat(64),
            "terminal_turn": 1,
            "terminal_state_hash": "b".repeat(64)
        }))
        .expect("server grant shape");
        let trainer = grant.trainer_id.clone();
        let source = coop_protocol::BattleDigest::parse(&"a".repeat(64)).unwrap();
        let terminal = coop_protocol::BattleDigest::parse(&"b".repeat(64)).unwrap();
        let valid = |grant: &super::BattleCommitGrantView, battle, trainer| {
            super::valid_commit_grant(grant, battle, fence, trainer, source, 1, terminal)
        };
        assert!(valid(&grant, battle_id, &trainer));
        grant.source_revision = coop_cloud::Revision::new(2);
        assert!(!valid(&grant, battle_id, &trainer));
        grant.source_revision = fence.current_revision;
        grant.terminal_state_hash = "bad".to_owned();
        assert!(!valid(&grant, battle_id, &trainer));
        grant.terminal_state_hash = "b".repeat(64);
        assert!(!valid(&grant, uuid::Uuid::from_u128(503), &trainer));
        let other =
            coop_protocol::TrainerInstanceId::new(coop_protocol::RegionId::Kanto, "TRAINER_BROCK")
                .expect("registered trainer syntax");
        assert!(!valid(&grant, battle_id, &other));
        grant.source_party_digest = "c".repeat(64);
        assert!(!valid(&grant, battle_id, &trainer));
        grant.source_party_digest = "a".repeat(64);
        grant.terminal_turn = 2;
        assert!(!valid(&grant, battle_id, &trainer));
    }

    #[test]
    fn trainer_reserve_resolves_exact_identity_and_rejects_region_or_unknown_ordinal() {
        use coop_protocol::{BattleKind as BridgeKind, RegionId, TrainerInstanceId};
        let trainer = super::reserve_trainer_id(
            BridgeKind::CooperativeTrainer,
            Some(RegionId::Hoenn),
            Some(518),
        )
        .unwrap()
        .unwrap();
        assert_eq!(trainer.as_str(), "HOENN:TRAINER_WALLY_1");
        assert_eq!(
            super::reserve_trainer_id(
                BridgeKind::CooperativeTrainer,
                Some(RegionId::Kanto),
                Some(518)
            ),
            Err(BattleApiError::Invalid)
        );
        assert_eq!(
            super::reserve_trainer_id(
                BridgeKind::CooperativeTrainer,
                Some(RegionId::Hoenn),
                Some(u16::MAX)
            ),
            Err(BattleApiError::Invalid)
        );
        assert_eq!(
            super::reserve_trainer_id(BridgeKind::Friendly, None, None),
            Ok(None)
        );
        let mut view: BattleReservationView = serde_json::from_value(reservation_json()).unwrap();
        assert!(super::valid_reserve_view(
            &view,
            BridgeKind::CooperativeTrainer,
            Some(&trainer),
            view.initiator_character_id
        ));
        let other = TrainerInstanceId::parse("HOENN:TRAINER_WALLY_2").unwrap();
        view.trainer_id = Some(other);
        assert!(!super::valid_reserve_view(
            &view,
            BridgeKind::CooperativeTrainer,
            Some(&trainer),
            view.initiator_character_id
        ));
    }

    #[test]
    fn manifest_identity_uses_fenced_member_and_supported_reservation() {
        let mut view: BattleReservationView = serde_json::from_value(reservation_json()).unwrap();
        let first = view.member_character_ids[0];
        let second = view.member_character_ids[1];
        assert_eq!(
            super::manifest_identity(&view, first),
            Some((
                coop_protocol::BattleKind::CooperativeTrainer,
                0,
                coop_protocol::RegionId::Hoenn,
                518
            ))
        );
        assert_eq!(
            super::manifest_identity(&view, second),
            Some((
                coop_protocol::BattleKind::CooperativeTrainer,
                1,
                coop_protocol::RegionId::Hoenn,
                518
            ))
        );
        view.trainer_id =
            Some(coop_protocol::TrainerInstanceId::parse("HOENN:TRAINER_WALLY_2").unwrap());
        assert_eq!(super::manifest_identity(&view, first), None);
        view.kind = BattleKind::Friendly;
        assert_eq!(super::manifest_identity(&view, first), None);
        view.trainer_id = None;
        assert_eq!(
            super::manifest_identity(&view, second),
            Some((
                coop_protocol::BattleKind::Friendly,
                1,
                coop_protocol::RegionId::Unspecified,
                0
            ))
        );
        view.member_character_ids[1] = first;
        assert_eq!(super::manifest_identity(&view, first), None);
    }

    #[test]
    fn reserve_request_keeps_friendly_json_without_trainer_id() {
        let key = coop_cloud::IdempotencyKey::new(uuid::Uuid::from_u128(1)).unwrap();
        let friendly = serde_json::to_value(super::ReserveRequest {
            api_version: coop_cloud::ApiVersion::V1,
            kind: BattleKind::Friendly,
            trainer_id: None,
            idempotency_key: key,
        })
        .unwrap();
        assert!(friendly.get("trainer_id").is_none());
        let trainer = serde_json::to_value(super::ReserveRequest {
            api_version: coop_cloud::ApiVersion::V1,
            kind: BattleKind::CooperativeTrainer,
            trainer_id: Some(
                coop_protocol::TrainerInstanceId::parse("HOENN:TRAINER_WALLY_1").unwrap(),
            ),
            idempotency_key: key,
        })
        .unwrap();
        assert_eq!(trainer["trainer_id"], "HOENN:TRAINER_WALLY_1");
    }

    #[test]
    fn conflict_is_a_stale_state_error() {
        assert_eq!(
            super::map_http_error(&crate::HttpClientError::Status(
                reqwest::StatusCode::CONFLICT
            )),
            BattleApiError::Stale,
        );
    }

    fn fence() -> coop_cloud::LeaseFence {
        use coop_cloud::{CharacterId, ClientInstanceId, Revision, SessionEpoch, SessionId};
        coop_cloud::LeaseFence::new(
            SessionId::new(uuid::Uuid::from_u128(10)).unwrap(),
            CharacterId::new(
                uuid::Uuid::parse_str("7ca5c789-57cb-4d06-b68d-176ff05f69c5").unwrap(),
            )
            .unwrap(),
            Revision::new(1),
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(11)).unwrap(),
        )
    }

    #[test]
    fn requester_nonce_is_deduplicated_and_replayed_only_with_its_offer() {
        use super::{BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, BattleRole, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 42,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&42];
        owner.reserve(fence, 1, request);
        assert_eq!(owner.reserve_keys.len(), 1);
        assert_eq!(owner.reserve_keys[&42], key);
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 42,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(deliveries.as_slice(), [BattleDelivery::Offer(record)] if record.role == BattleRole::Requester && record.request_nonce == 42)
        );
        owner.invalidate();
        let replay = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Poll,
                    result: Ok(Some(view)),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(
            matches!(replay.as_slice(), [BattleDelivery::Offer(record)] if record.role == BattleRole::Requester && record.request_nonce == 42)
        );
    }

    #[test]
    fn requester_receives_idempotent_acceptance_outcome_bound_to_nonce() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 42,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        let key = {
            owner.reserve(fence, 1, request);
            owner.reserve_keys[&42]
        };
        let pending: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 42,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(pending.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        let mut accepted = pending.clone();
        accepted.status = super::BattleStatus::Accepted;
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(accepted.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(
            deliveries.as_slice(),
            [BattleDelivery::ConsentOutcome(record)]
                if record.battle_id == super::bridge_id(accepted.battle_id)
                    && record.request_nonce == 42
                    && record.outcome == BattleConsentOutcome::Accepted
        ));
        owner.invalidate();
        let replay = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Poll,
                    result: Ok(Some(accepted)),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(matches!(
            replay.as_slice(),
            [BattleDelivery::ConsentOutcome(record)]
                if record.request_nonce == 42
                    && record.outcome == BattleConsentOutcome::Accepted
        ));
    }

    #[test]
    fn accepted_reserve_delivers_offer_before_outcome() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 44,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&44];
        let mut accepted: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        accepted.status = super::BattleStatus::Accepted;
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 44,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(accepted)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(
            deliveries.as_slice(),
            [BattleDelivery::Offer(offer), BattleDelivery::ConsentOutcome(outcome)]
                if offer.request_nonce == 44
                    && outcome.request_nonce == 44
                    && outcome.battle_id == offer.battle_id
                    && outcome.outcome == BattleConsentOutcome::Accepted
        ));
    }

    #[test]
    fn requester_expiry_emits_outcome_before_legacy_abort() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 43,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&43];
        let pending: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 43,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(pending)),
                }),
                fence,
                1,
            )
            .unwrap();
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(
            deliveries.as_slice(),
            [BattleDelivery::ConsentOutcome(outcome), BattleDelivery::Abort(abort)]
                if outcome.request_nonce == 43
                    && outcome.outcome == BattleConsentOutcome::Expired
                    && abort.reason == 4
        ));
    }

    #[test]
    fn requester_decline_is_observable_and_replayable_without_active_reservation() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 44,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&44];
        let mut view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 44,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        view.status = super::BattleStatus::Declined;
        let delivered = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(delivered.as_slice(),
            [BattleDelivery::ConsentOutcome(outcome), BattleDelivery::Abort(_)]
                if outcome.request_nonce == 44
                    && outcome.outcome == BattleConsentOutcome::Declined));
        owner.invalidate();
        assert_eq!(
            owner.requester_outcome.unwrap().outcome,
            BattleConsentOutcome::Declined
        );
        assert!(owner.redeliver_requester_outcome);
        let replay = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Poll,
                    result: Ok(None),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(matches!(replay.as_slice(),
            [BattleDelivery::ConsentOutcome(outcome)]
                if outcome.request_nonce == 44
                    && outcome.outcome == BattleConsentOutcome::Declined));
    }

    #[test]
    fn terminal_reserve_replay_binds_offer_before_decline() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 46,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&46];
        let mut view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        view.status = super::BattleStatus::Declined;
        let delivered = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 46,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(delivered.as_slice(),
            [BattleDelivery::Offer(offer), BattleDelivery::ConsentOutcome(outcome),
             BattleDelivery::Abort(_)]
                if offer.request_nonce == 46
                    && outcome.request_nonce == 46
                    && outcome.outcome == BattleConsentOutcome::Declined));
    }

    #[test]
    fn accepted_result_is_not_replayed_after_matching_abort() {
        use super::{BattleConsentOutcome, BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 45,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&45];
        let mut view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 45,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        view.status = super::BattleStatus::Accepted;
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert_eq!(
            owner.requester_outcome.unwrap().outcome,
            BattleConsentOutcome::Accepted
        );
        view.status = super::BattleStatus::Expired;
        let delivered = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(delivered.as_slice(), [BattleDelivery::Abort(_)]));
        assert!(owner.requester_outcome.is_none());
    }

    #[test]
    fn responder_offer_matches_uuid_and_terminal_state_aborts() {
        use super::{BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleDecision, BattleJoinResponseRecord, BattleRole};
        let mut owner = BattleOwner::default();
        let mut fence = fence();
        fence.character_id = coop_cloud::CharacterId::new(
            uuid::Uuid::parse_str("7e84b14b-91b7-4a66-9866-c86033f78f4a").unwrap(),
        )
        .unwrap();
        owner.bind(fence, 1);
        let mut view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let offers = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(offers.as_slice(), [BattleDelivery::Offer(record)] if record.role == BattleRole::Responder && record.request_nonce == 0)
        );
        let wrong = BattleJoinResponseRecord {
            battle_id: coop_protocol::BattleId(*uuid::Uuid::from_u128(99).as_bytes()),
            decision: BattleDecision::Accept,
        };
        assert_eq!(
            owner.respond(fence, 1, wrong).unwrap().battle_id,
            wrong.battle_id
        );
        let right = BattleJoinResponseRecord {
            battle_id: coop_protocol::BattleId(*view.battle_id.as_bytes()),
            decision: BattleDecision::Accept,
        };
        assert!(owner.respond(fence, 1, right).is_none());
        assert_eq!(owner.queued_response, Some(right));
        view.status = super::BattleStatus::Expired;
        let terminal = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(terminal.as_slice(), [BattleDelivery::Abort(record)] if record.reason == 4)
        );
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn transient_reserve_failure_requeues_the_same_nonce_and_key() {
        use super::{BattleApiError, BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let record = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 7,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, record);
        let key = owner.reserve_keys[&7];
        assert_eq!(owner.queued_reserve.take(), Some(record));
        let job = Job::Reserve {
            nonce: 7,
            kind: record.kind,
            trainer_region: record.trainer_region,
            trainer_ordinal: record.trainer_ordinal,
            key,
        };
        assert!(
            owner
                .finish(
                    Some(Completion {
                        fence,
                        generation: 1,
                        job,
                        result: Err(BattleApiError::Unavailable)
                    }),
                    fence,
                    1
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(owner.queued_reserve, Some(record));
        assert_eq!(owner.reserve_keys[&7], key);
        owner.pending_job = Some(job);
        owner.queued_reserve = None;
        owner.invalidate();
        assert_eq!(owner.queued_reserve, Some(record));
        assert_eq!(owner.reserve_keys[&7], key);
    }

    #[test]
    fn second_battle_has_a_fresh_response_key_and_request_kind_is_checked() {
        use super::{BattleDelivery, BattleOwner, Completion, Job};
        use coop_protocol::{BattleDecision, BattleJoinResponseRecord, BattleKind as BridgeKind};
        let mut owner = BattleOwner::default();
        let mut fence = fence();
        fence.character_id = coop_cloud::CharacterId::new(
            uuid::Uuid::parse_str("7e84b14b-91b7-4a66-9866-c86033f78f4a").unwrap(),
        )
        .unwrap();
        owner.bind(fence, 1);
        let first: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(first.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        let response = BattleJoinResponseRecord {
            battle_id: super::bridge_id(first.battle_id),
            decision: BattleDecision::Accept,
        };
        assert!(owner.respond(fence, 1, response).is_none());
        let first_key = owner.response_key.unwrap();
        let terminal = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(terminal.as_slice(), [BattleDelivery::Abort(_)]));
        let mut second = first.clone();
        second.battle_id = uuid::Uuid::from_u128(123);
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(Some(second.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            owner
                .respond(
                    fence,
                    1,
                    BattleJoinResponseRecord {
                        battle_id: super::bridge_id(second.battle_id),
                        decision: BattleDecision::Accept
                    }
                )
                .is_none()
        );
        assert_ne!(owner.response_key.unwrap(), first_key);
        assert!(!super::valid_reserve_view(
            &second,
            BridgeKind::Friendly,
            None,
            first.initiator_character_id
        ));
        assert!(!super::valid_reserve_view(
            &second,
            BridgeKind::CooperativeTrainer,
            None,
            fence.character_id
        ));
    }

    #[test]
    fn new_lease_drops_inflight_consent_instead_of_replaying_it() {
        use super::{BattleOwner, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let old = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 91,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(old, 1, request);
        owner.pending_job = Some(Job::Reserve {
            nonce: 91,
            kind: request.kind,
            trainer_region: request.trainer_region,
            trainer_ordinal: request.trainer_ordinal,
            key: owner.reserve_keys[&91],
        });
        owner.queued_reserve = None;
        let mut replacement = old;
        replacement.session_epoch = coop_cloud::SessionEpoch::new(2).unwrap();
        owner.bind(replacement, 2);
        assert!(owner.pending_job.is_none());
        assert!(owner.queued_reserve.is_none());
        assert!(owner.reserve_keys.is_empty());
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn same_lease_rom_restart_reuses_nonce_with_new_key_after_settlement() {
        use super::{BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 1,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let old_key = owner.reserve_keys[&1];
        owner.queued_reserve = None; // The HTTP job has taken this request.
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 1,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key: old_key,
                    },
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        owner.reserve(fence, 2, request);
        assert_eq!(owner.queued_reserve, Some(request));
        assert_ne!(owner.reserve_keys[&1], old_key);
    }

    #[test]
    fn same_lease_rom_restart_replays_then_rekeys_reused_nonce() {
        use super::{BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, TrainerBattleReserveRecord};
        let mut owner = BattleOwner::default();
        let fence = fence();
        let request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 1,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&1];
        owner.pending_job = Some(Job::Reserve {
            nonce: 1,
            kind: request.kind,
            trainer_region: request.trainer_region,
            trainer_ordinal: request.trainer_ordinal,
            key,
        });
        owner.queued_reserve = None;
        owner.reserve(fence, 2, request);
        assert_eq!(owner.queued_reserve, Some(request));
        assert_eq!(owner.reserve_keys[&1], key);
        assert_eq!(owner.replacement_reserve, Some(request));
        let old_view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Reserve {
                        nonce: 1,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Ok(Some(old_view)),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(deliveries.is_empty());
        assert_eq!(owner.queued_reserve, Some(request));
        assert_ne!(owner.reserve_keys[&1], key);
    }

    #[test]
    fn same_lease_restart_reconciles_inflight_reserve_before_new_nonce() {
        use super::{BattleOwner, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, RegionId, TrainerBattleReserveRecord};

        let mut owner = BattleOwner::default();
        let fence = fence();
        let old_request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 41,
            trainer_region: Some(RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, old_request);
        let old_key = owner.reserve_keys[&41];
        owner.pending_job = Some(Job::Reserve {
            nonce: 41,
            kind: old_request.kind,
            trainer_region: old_request.trainer_region,
            trainer_ordinal: old_request.trainer_ordinal,
            key: old_key,
        });
        owner.queued_reserve = None;

        let fresh_request = TrainerBattleReserveRecord {
            request_nonce: 99,
            ..old_request
        };
        owner.reserve(fence, 2, fresh_request);
        assert_eq!(owner.queued_reserve, Some(old_request));
        assert_eq!(owner.replacement_reserve, Some(fresh_request));
        assert!(owner.reconcile_before_reserve);
        assert_eq!(owner.reserve_keys[&41], old_key);
        assert!(!owner.reserve_keys.contains_key(&99));
        owner.bind(fence, 3);
        assert_eq!(owner.reserve_keys[&41], old_key);
        assert_eq!(owner.replacement_reserve, Some(fresh_request));
        let latest_request = TrainerBattleReserveRecord {
            request_nonce: 100,
            ..old_request
        };
        owner.reserve(fence, 3, latest_request);
        assert_eq!(owner.replacement_reserve, Some(latest_request));
        assert!(!owner.reserve_keys.contains_key(&99));
        assert!(!owner.reserve_keys.contains_key(&100));

        let old_view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        // A current poll may still see no reservation while the original
        // request is in flight. It must not release the fresh request.
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 3,
                    job: Job::Poll,
                    result: Ok(None),
                }),
                fence,
                3,
            )
            .unwrap();
        assert!(owner.reconcile_before_reserve);
        assert_eq!(owner.queued_reserve, Some(old_request));
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 3,
                    job: Job::Reserve {
                        nonce: 41,
                        kind: old_request.kind,
                        trainer_region: old_request.trainer_region,
                        trainer_ordinal: old_request.trainer_ordinal,
                        key: old_key,
                    },
                    result: Ok(Some(old_view.clone())),
                }),
                fence,
                3,
            )
            .unwrap();
        assert!(
            deliveries.is_empty(),
            "stale nonce offer must not reach the fresh ROM"
        );
        assert_eq!(owner.queued_reserve, Some(latest_request));
        assert!(!owner.reserve_keys.contains_key(&41));
        assert!(owner.reserve_keys.contains_key(&100));
        assert_eq!(
            owner.queued_cancel.map(|cancel| cancel.0),
            Some(old_view.battle_id)
        );
    }

    #[test]
    fn same_lease_rom_restart_cancels_old_requester_before_new_reserve() {
        use super::{BattleDelivery, BattleOwner, BattleStatus, Completion, Job};
        use coop_protocol::{BattleKind as BridgeKind, RegionId, TrainerBattleReserveRecord};

        let mut owner = BattleOwner::default();
        let fence = fence();
        let old_request = TrainerBattleReserveRecord {
            kind: BridgeKind::CooperativeTrainer,
            request_nonce: 41,
            trainer_region: Some(RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, old_request);
        let mut old_view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let old_key = owner.reserve_keys[&41];
        owner.queued_reserve = None;
        owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 1,
                    job: Job::Reserve {
                        nonce: 41,
                        kind: old_request.kind,
                        trainer_region: old_request.trainer_region,
                        trainer_ordinal: old_request.trainer_ordinal,
                        key: old_key,
                    },
                    result: Ok(Some(old_view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();

        let fresh_request = TrainerBattleReserveRecord {
            request_nonce: 99,
            ..old_request
        };
        owner.reserve(fence, 2, fresh_request);
        let (battle_id, group_id, cancel_key) =
            owner.queued_cancel.expect("old requester must be canceled");
        assert_eq!(battle_id, old_view.battle_id);
        assert_eq!(owner.queued_reserve, Some(fresh_request));
        assert_ne!(owner.reserve_keys[&99], old_key);

        old_view.status = BattleStatus::Cancelled;
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Cancel {
                        battle_id,
                        group_id,
                        key: cancel_key,
                    },
                    result: Ok(Some(old_view)),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(matches!(deliveries.as_slice(), [BattleDelivery::Abort(_)]));
        assert!(owner.tracked.is_none());
        assert_eq!(owner.queued_reserve, Some(fresh_request));
    }

    fn tracked_owner() -> (
        super::BattleOwner<'static>,
        coop_cloud::LeaseFence,
        super::BattleReservationView,
    ) {
        let fence = fence();
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let mut owner = super::BattleOwner::default();
        owner.bind(fence, 1);
        owner.tracked = Some(super::Tracked {
            group_id: view.group_id,
            battle_id: view.battle_id,
            members: view.member_character_ids,
            kind: view.kind,
            trainer_identity: Some((coop_protocol::RegionId::Hoenn, 518)),
            role: coop_protocol::BattleRole::Requester,
            nonce: 1,
            local_rewards: false,
        });
        (owner, fence, view)
    }

    #[test]
    fn local_reward_views_parse_and_send_staged_records_only_when_local() {
        let mut local = reservation_json();
        local["trainer_id"] = json!("HOENN:TRAINER_CALVIN_1");
        local["reward_mode"] = json!("LOCAL");
        local["member_roles"] = json!(["PARTICIPANT", "HELPER"]);
        let view: super::BattleReservationView = serde_json::from_value(local.clone()).unwrap();
        assert_eq!(view.reward_mode, super::BattleRewardMode::Local);
        assert_eq!(
            view.member_roles,
            Some([
                super::BattleMemberRole::Participant,
                super::BattleMemberRole::Helper
            ])
        );
        assert_eq!(serde_json::to_value(&view).unwrap(), local);
        let ledger: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        assert_eq!(ledger.reward_mode, super::BattleRewardMode::Ledger);
        assert_eq!(serde_json::to_value(&ledger).unwrap(), reservation_json());

        let (mut owner, _fence, view) = tracked_owner();
        owner.party = vec![vec![0xAB; coop_protocol::BATTLE_PARTY_MON_SIZE]];
        owner.snapshot = Some(("11".repeat(32), super::new_key()));
        owner.accepted_battle = Some(view.battle_id);
        let tracked = owner.tracked.unwrap();
        let super::ConsensusJob::Commit { records, .. } = owner.consensus_job(tracked) else {
            panic!("snapshot commit expected");
        };
        assert_eq!(
            records, None,
            "ledger battles keep the anchored-save commit"
        );
        let local_tracked = super::Tracked {
            local_rewards: true,
            ..tracked
        };
        let super::ConsensusJob::Commit { records, .. } = owner.consensus_job(local_tracked) else {
            panic!("snapshot commit expected");
        };
        assert_eq!(
            records,
            Some(vec!["ab".repeat(coop_protocol::BATTLE_PARTY_MON_SIZE)])
        );
    }

    #[test]
    fn finished_record_is_strictly_bound_and_reuses_its_finish_key() {
        use super::{BattleDelivery, BattleFinishedRecord, BattleFinishedResult, Completion, Job};

        let (mut owner, fence, view) = tracked_owner();
        let digest = coop_protocol::BattleDigest([0xCD; 32]);
        owner
            .hashes
            .insert(1, (super::hex_string(&digest.0), super::new_key()));
        let invalid = BattleFinishedRecord {
            battle_id: super::bridge_id(view.battle_id),
            turn: 1,
            result: BattleFinishedResult::Member0Won,
            terminal_hash: digest,
        };
        assert!(matches!(
            owner.finish_request(fence, 1, invalid),
            Some(record) if record.reason == 5
        ));
        assert_eq!(
            owner
                .queued_cancel
                .map(|(battle_id, group_id, _)| (battle_id, group_id)),
            Some((view.battle_id, view.group_id))
        );
        owner.queued_cancel = None;
        let valid = BattleFinishedRecord {
            result: BattleFinishedResult::Won,
            ..invalid
        };
        assert!(owner.finish_request(fence, 1, valid).is_none());
        let (_, first_key) = owner.queued_finish.expect("finish queued");
        owner.invalidate();
        assert_eq!(owner.queued_finish, Some((valid, first_key)));
        assert!(owner.finish_request(fence, 2, valid).is_none());
        assert_eq!(owner.queued_finish, Some((valid, first_key)));

        let mut accepted = view.clone();
        accepted.status = super::BattleStatus::Accepted;
        let deliveries = owner
            .finish(
                Some(Completion {
                    fence,
                    generation: 2,
                    job: Job::Finish {
                        battle_id: view.battle_id,
                        group_id: view.group_id,
                        record: valid,
                        key: first_key,
                    },
                    result: Ok(Some(accepted)),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(deliveries.is_empty());
        assert!(owner.tracked.is_some());
        assert_eq!(owner.last_finish, Some((valid, first_key)));
        assert!(!matches!(deliveries.as_slice(), [BattleDelivery::Abort(_)]));
    }

    #[test]
    fn finished_wire_result_uses_server_screaming_snake_case_and_hex_hash() {
        let key = coop_cloud::IdempotencyKey::new(uuid::Uuid::from_u128(2)).unwrap();
        let request = super::FinishRequest {
            api_version: coop_cloud::ApiVersion::V1,
            idempotency_key: key,
            result: super::BattleFinishWireResult::Member1Won,
            turn: 7,
            state_hash: "ab".repeat(32),
        };
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["result"], "MEMBER1_WON");
        assert_eq!(json["state_hash"], "ab".repeat(32));
    }

    #[test]
    fn finish_probe_waits_for_both_matching_terminal_hashes() {
        let reservation: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let digest = coop_protocol::BattleDigest([0xCD; 32]);
        let record = super::BattleFinishedRecord {
            battle_id: super::bridge_id(reservation.battle_id),
            turn: 1,
            result: coop_protocol::BattleFinishedResult::Won,
            terminal_hash: digest,
        };
        let manifest = super::BattleManifest {
            battle_id: reservation.battle_id,
            member_character_ids: reservation.member_character_ids,
            snapshot_hashes: ["aa".repeat(32), "bb".repeat(32)],
            seed: "cc".repeat(32),
        };
        let mut view = super::BattleConsensusView {
            reservation,
            commitments: [None, None],
            manifest: Some(manifest),
            ready: [false; 2],
            start_released: false,
            turns: Vec::new(),
        };
        assert!(!super::finish_probe_ready(&view, record));
        let hash = super::hex_string(&digest.0);
        view.turns.push(super::BattleTurn {
            number: 1,
            actions: [Some("00".to_owned()), Some("00".to_owned())],
            state_hashes: [Some(hash.clone()), None],
            action_keys: [None, None],
            hash_keys: [None, None],
        });
        assert!(!super::finish_probe_ready(&view, record));
        view.turns[0].state_hashes = [Some(hash.clone()), Some("dd".repeat(32))];
        assert!(!super::finish_probe_ready(&view, record));
        view.turns[0].state_hashes = [Some(hash.clone()), Some(hash)];
        assert!(super::finish_probe_ready(&view, record));
    }

    #[test]
    fn finish_probe_divergence_aborts_without_replaying_the_finish_key() {
        let (mut owner, fence, mut view) = tracked_owner();
        view.status = super::BattleStatus::Diverged;
        let record = super::BattleFinishedRecord {
            battle_id: super::bridge_id(view.battle_id),
            turn: 1,
            result: coop_protocol::BattleFinishedResult::Won,
            terminal_hash: coop_protocol::BattleDigest([0xCD; 32]),
        };
        let key = super::new_key();
        let deliveries = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::FinishProbe {
                        battle_id: view.battle_id,
                        group_id: view.group_id,
                        record,
                        key,
                    },
                    result: Ok(Some(view)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(
            deliveries.as_slice(),
            [super::BattleDelivery::Abort(record)]
                if record.reason == 3
        ));
        assert!(owner.tracked.is_none());
        assert!(owner.queued_finish.is_none());
    }

    #[test]
    fn conflicting_finish_duplicate_queues_fenced_cancel() {
        let (mut owner, fence, view) = tracked_owner();
        let digest = coop_protocol::BattleDigest([0xCD; 32]);
        owner
            .hashes
            .insert(1, (super::hex_string(&digest.0), super::new_key()));
        let first = super::BattleFinishedRecord {
            battle_id: super::bridge_id(view.battle_id),
            turn: 1,
            result: coop_protocol::BattleFinishedResult::Won,
            terminal_hash: digest,
        };
        assert!(owner.finish_request(fence, 1, first).is_none());
        let conflicting = super::BattleFinishedRecord {
            result: coop_protocol::BattleFinishedResult::Lost,
            ..first
        };
        assert!(matches!(
            owner.finish_request(fence, 1, conflicting),
            Some(record) if record.reason == 5
        ));
        assert_eq!(
            owner
                .queued_cancel
                .map(|(battle_id, group_id, _)| (battle_id, group_id)),
            Some((view.battle_id, view.group_id))
        );
    }

    fn peer_fixture(
        view: &super::BattleReservationView,
    ) -> (super::BattlePeerPartyView, coop_protocol::BattleDigest) {
        let records = vec![vec![1; 100], vec![2; 100]];
        let digest = super::canonical_party_digest(&records).unwrap();
        (
            super::BattlePeerPartyView {
                api_version: coop_cloud::ApiVersion::V1,
                battle_id: view.battle_id,
                peer_character_id: view.member_character_ids[1],
                snapshot_revision: coop_cloud::Revision::new(1),
                snapshot_hash: super::hex_string(&digest.0),
                party_records: records
                    .iter()
                    .map(|record| super::hex_string(record))
                    .collect(),
            },
            digest,
        )
    }

    #[test]
    fn peer_party_validates_identity_digest_and_exact_record_bytes() {
        let (owner, fence, view) = tracked_owner();
        let (party, digest) = peer_fixture(&view);
        let expected = (view.member_character_ids[1], digest);
        let chunks =
            super::validated_peer_chunks(party.clone(), owner.tracked.unwrap(), fence, expected)
                .unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].mon, vec![1; 100]);
        assert_eq!(chunks[1].mon, vec![2; 100]);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
        let mut wrong = party.clone();
        wrong.peer_character_id = view.member_character_ids[0];
        assert_eq!(
            super::validated_peer_chunks(wrong, owner.tracked.unwrap(), fence, expected),
            Err(super::BattleApiError::Invalid)
        );
        let mut wrong = party.clone();
        wrong.snapshot_hash = "00".repeat(32);
        assert_eq!(
            super::validated_peer_chunks(wrong, owner.tracked.unwrap(), fence, expected),
            Err(super::BattleApiError::Invalid)
        );
        let mut wrong = party.clone();
        wrong.snapshot_revision = coop_cloud::Revision::initial();
        assert_eq!(
            super::validated_peer_chunks(wrong, owner.tracked.unwrap(), fence, expected),
            Err(super::BattleApiError::Invalid)
        );
        let mut wrong = party.clone();
        wrong.party_records[1].replace_range(0..2, "03");
        assert_eq!(
            super::validated_peer_chunks(wrong, owner.tracked.unwrap(), fence, expected),
            Err(super::BattleApiError::Invalid)
        );
        let mut wrong = party;
        wrong.party_records[0].replace_range(0..2, "FF");
        assert_eq!(
            super::validated_peer_chunks(wrong, owner.tracked.unwrap(), fence, expected),
            Err(super::BattleApiError::Invalid)
        );
    }

    #[test]
    fn peer_party_requires_revalidation_after_control_restart() {
        use super::{BattleDelivery, PeerCompletion};
        let (mut owner, fence, view) = tracked_owner();
        let (party, digest) = peer_fixture(&view);
        owner.peer_expected = Some((view.member_character_ids[1], digest));
        let chunks = super::validated_peer_chunks(
            party,
            owner.tracked.unwrap(),
            fence,
            owner.peer_expected.unwrap(),
        )
        .unwrap();
        let delivered = owner
            .finish_peer(
                PeerCompletion {
                    fence,
                    generation: 1,
                    battle_id: view.battle_id,
                    result: Ok(chunks.clone()),
                },
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(delivered.as_slice(), [BattleDelivery::PeerParty(a), BattleDelivery::PeerParty(b)] if a.chunk_index == 0 && b.chunk_index == 1)
        );
        assert!(
            owner
                .finish_peer(
                    PeerCompletion {
                        fence,
                        generation: 1,
                        battle_id: view.battle_id,
                        result: Ok(chunks.clone())
                    },
                    fence,
                    1
                )
                .unwrap()
                .is_empty()
        );
        owner.bind(fence, 2);
        assert!(!owner.peer_delivered);
        assert_eq!(owner.peer_cached, None);
        let replay = owner
            .finish_peer(
                PeerCompletion {
                    fence,
                    generation: 2,
                    battle_id: view.battle_id,
                    result: Ok(chunks),
                },
                fence,
                2,
            )
            .unwrap();
        assert!(
            matches!(replay.as_slice(), [BattleDelivery::PeerParty(a), BattleDelivery::PeerParty(b)] if a.chunk_index == 0 && b.chunk_index == 1)
        );
    }

    #[test]
    fn transient_peer_fetch_keeps_expected_party_for_retry() {
        use super::PeerCompletion;
        let (mut owner, fence, view) = tracked_owner();
        let (_, digest) = peer_fixture(&view);
        owner.peer_expected = Some((view.member_character_ids[1], digest));
        let result = owner
            .finish_peer(
                PeerCompletion {
                    fence,
                    generation: 1,
                    battle_id: view.battle_id,
                    result: Err(super::BattleApiError::Unavailable),
                },
                fence,
                1,
            )
            .unwrap();
        assert!(result.is_empty());
        assert!(owner.tracked.is_some());
        assert_eq!(
            owner.peer_expected,
            Some((view.member_character_ids[1], digest))
        );
        assert!(!owner.peer_delivered);
        assert!(owner.peer_cached.is_none());
        assert!(owner.next_peer_poll > tokio::time::Instant::now());
    }

    #[test]
    fn invalid_peer_response_aborts_but_stale_battle_completion_is_ignored() {
        use super::{BattleDelivery, PeerCompletion};
        let (mut owner, fence, view) = tracked_owner();
        let (_, digest) = peer_fixture(&view);
        owner.peer_expected = Some((view.member_character_ids[1], digest));
        assert!(
            owner
                .finish_peer(
                    PeerCompletion {
                        fence,
                        generation: 1,
                        battle_id: uuid::Uuid::from_u128(123),
                        result: Err(super::BattleApiError::Invalid)
                    },
                    fence,
                    1
                )
                .unwrap()
                .is_empty()
        );
        assert!(owner.tracked.is_some());
        let abort = owner
            .finish_peer(
                PeerCompletion {
                    fence,
                    generation: 1,
                    battle_id: view.battle_id,
                    result: Err(super::BattleApiError::Invalid),
                },
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(abort.as_slice(), [BattleDelivery::Abort(record)] if record.battle_id == super::bridge_id(view.battle_id) && record.reason == 5)
        );
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn party_digest_uses_count_and_ordered_raw_mons_and_replays_chunks() {
        let (mut owner, fence, view) = tracked_owner();
        let make = |index, value| coop_protocol::PartySnapshotChunk {
            battle_id: super::bridge_id(view.battle_id),
            party_slot: index,
            chunk_index: index,
            chunk_count: 2,
            mon: vec![value; 100],
        };
        assert!(owner.party_chunk(fence, 1, make(0, 1)).is_none());
        assert!(owner.party_chunk(fence, 1, make(0, 1)).is_none());
        assert!(owner.party_chunk(fence, 1, make(1, 2)).is_none());
        let (digest, key) = owner.snapshot.clone().unwrap();
        assert_eq!(
            digest,
            "31272fb92764f1350aba789e8f7b559f8e9578ec5617062aa219ab7faad1869a"
        );
        assert!(owner.party_chunk(fence, 1, make(1, 2)).is_none());
        assert_eq!(owner.snapshot.as_ref().unwrap().1, key);
        let (mut out_of_order, fence, _) = tracked_owner();
        assert_eq!(
            out_of_order
                .party_chunk(fence, 1, make(1, 2))
                .unwrap()
                .reason,
            5
        );
    }

    #[test]
    fn action_hex_and_conflicting_replay_are_bounded() {
        let (mut owner, fence, view) = tracked_owner();
        owner.party_chunk(
            fence,
            1,
            coop_protocol::PartySnapshotChunk {
                battle_id: super::bridge_id(view.battle_id),
                party_slot: 0,
                chunk_index: 0,
                chunk_count: 1,
                mon: vec![1; 100],
            },
        );
        owner.start_delivered = true;
        let action = coop_protocol::ActionIntent {
            battle_id: super::bridge_id(view.battle_id),
            turn: 1,
            action: vec![0, 0xAF, 0x10],
        };
        assert!(owner.action_intent(fence, 1, action.clone()).is_none());
        assert_eq!(owner.actions[&1].0, "00af10");
        let key = owner.actions[&1].1;
        assert!(owner.action_intent(fence, 1, action.clone()).is_none());
        assert_eq!(owner.actions[&1].1, key);
        assert_eq!(
            owner
                .action_intent(
                    fence,
                    1,
                    coop_protocol::ActionIntent {
                        action: vec![1],
                        ..action
                    }
                )
                .unwrap()
                .reason,
            5
        );
        assert!(
            owner
                .finish(
                    Some(super::Completion {
                        fence,
                        generation: 1,
                        job: super::Job::Poll,
                        result: Ok(Some(view)),
                    }),
                    fence,
                    1
                )
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rom_abort_uses_one_cancel_key_and_ignores_stale_battle() {
        let (mut owner, fence, view) = tracked_owner();
        let current = coop_protocol::AbortBattleRecord {
            battle_id: super::bridge_id(view.battle_id),
            reason: 1,
        };
        owner.abort_request(
            fence,
            1,
            coop_protocol::AbortBattleRecord {
                battle_id: super::bridge_id(uuid::Uuid::from_u128(123)),
                ..current
            },
        );
        assert!(owner.queued_cancel.is_none());
        owner.abort_request(fence, 1, current);
        let first = owner.queued_cancel.expect("cancel queued");
        owner.abort_request(fence, 1, current);
        assert_eq!(owner.queued_cancel, Some(first));
        assert!(owner.consensus_pending.is_none());
        assert!(owner.peer_pending.is_none());
    }

    #[test]
    fn fresh_process_cancels_live_reservation_before_opening_battle_gate() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        let rejected = owner.reserve(
            fence,
            1,
            coop_protocol::TrainerBattleReserveRecord {
                request_nonce: 42,
                kind: coop_protocol::BattleKind::CooperativeTrainer,
                trainer_region: Some(coop_protocol::RegionId::Hoenn),
                trainer_ordinal: Some(518),
            },
        );
        assert_eq!(
            rejected,
            Some(coop_protocol::BattleReserveRejectedRecord { request_nonce: 42 })
        );
        let fresh = owner.reserve(
            fence,
            1,
            coop_protocol::TrainerBattleReserveRecord {
                request_nonce: 43,
                kind: coop_protocol::BattleKind::CooperativeTrainer,
                trainer_region: Some(coop_protocol::RegionId::Hoenn),
                trainer_ordinal: Some(518),
            },
        );
        assert_eq!(
            fresh,
            Some(coop_protocol::BattleReserveRejectedRecord { request_nonce: 43 })
        );
        assert!(owner.cold_reconcile);
        assert!(owner.queued_reserve.is_none());
        let deliveries = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(deliveries.is_empty());
        assert!(owner.tracked.is_none());
        let cancel = owner
            .queued_cancel
            .expect("old reservation must be cancelled");
        assert_eq!((cancel.0, cancel.1), (view.battle_id, view.group_id));
        owner.queued_cancel = None;
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Cancel {
                        battle_id: cancel.0,
                        group_id: cancel.1,
                        key: cancel.2,
                    },
                    result: Err(super::BattleApiError::Unavailable),
                }),
                fence,
                1,
            )
            .unwrap();
        assert_eq!(owner.queued_cancel, Some(cancel));
        owner.queued_cancel = None;
        let mut cancelled = view;
        cancelled.status = super::BattleStatus::Cancelled;
        let local_abort = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Cancel {
                        battle_id: cancel.0,
                        group_id: cancel.1,
                        key: cancel.2,
                    },
                    result: Ok(Some(cancelled)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(local_abort.as_slice(), [super::BattleDelivery::Abort(record)]
            if record.battle_id == super::bridge_id(cancel.0))
        );
        assert!(owner.cold_reconcile);
        let after_absence = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Err(super::BattleApiError::NotFound),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(after_absence.is_empty());
        assert!(!owner.cold_reconcile);
        assert_eq!(owner.cold_cancel, Some(cancel));
        assert!(owner.queued_reserve.is_none());
        owner.bind(fence, 2);
        let replay = owner
            .take_replay_abort(fence, 2)
            .expect("retry abort after failed send");
        assert_eq!(replay.battle_id, super::bridge_id(cancel.0));
        assert!(owner.take_replay_abort(fence, 2).is_none());
    }

    #[test]
    fn fresh_process_opens_gate_only_after_absence_is_confirmed() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Err(super::BattleApiError::Unavailable),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(owner.cold_reconcile);
        for error in [super::BattleApiError::Invalid, super::BattleApiError::Stale] {
            owner
                .finish(
                    Some(super::Completion {
                        fence,
                        generation: 1,
                        job: super::Job::Poll,
                        result: Err(error),
                    }),
                    fence,
                    1,
                )
                .unwrap();
            assert!(owner.cold_reconcile);
        }
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(!owner.cold_reconcile);
        assert!(owner.queued_cancel.is_none());
    }

    #[test]
    fn fresh_process_cancels_commit_pending_before_opening_battle_gate() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        let mut view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        view.status = super::BattleStatus::CommitPending;
        let deliveries = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(deliveries.is_empty());
        assert!(owner.cold_reconcile);
        assert_eq!(
            owner.queued_cancel.map(|cancel| (cancel.0, cancel.1)),
            Some((view.battle_id, view.group_id))
        );
    }

    #[test]
    fn exhausted_reserve_keys_rejects_nonce_without_creating_reservation() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        for nonce in 1..=64 {
            owner.reserve_keys.insert(nonce, super::new_key());
        }
        let rejected = owner.reserve(
            fence,
            1,
            coop_protocol::TrainerBattleReserveRecord {
                request_nonce: 65,
                kind: coop_protocol::BattleKind::CooperativeTrainer,
                trainer_region: Some(coop_protocol::RegionId::Hoenn),
                trainer_ordinal: Some(518),
            },
        );
        assert_eq!(
            rejected,
            Some(coop_protocol::BattleReserveRejectedRecord { request_nonce: 65 })
        );
        assert!(owner.queued_reserve.is_none());
        assert!(owner.queued_cancel.is_none());
    }

    #[test]
    fn cold_reserve_conflict_rechecks_for_late_server_lock() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(owner.cold_reconciled_once);
        let request = coop_protocol::TrainerBattleReserveRecord {
            request_nonce: 42,
            kind: coop_protocol::BattleKind::CooperativeTrainer,
            trainer_region: Some(coop_protocol::RegionId::Hoenn),
            trainer_ordinal: Some(518),
        };
        owner.reserve(fence, 1, request);
        let key = owner.reserve_keys[&request.request_nonce];
        assert_eq!(owner.queued_reserve.take(), Some(request));
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Reserve {
                        nonce: request.request_nonce,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Err(super::BattleApiError::Stale),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(owner.cold_reconcile);
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        assert_eq!(
            owner.queued_cancel.map(|cancel| (cancel.0, cancel.1)),
            Some((view.battle_id, view.group_id))
        );
        assert!(owner.tracked.is_none());
        let cancel = owner.queued_cancel.take().unwrap();
        let mut cancelled = view;
        cancelled.status = super::BattleStatus::Cancelled;
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Cancel {
                        battle_id: cancel.0,
                        group_id: cancel.1,
                        key: cancel.2,
                    },
                    result: Ok(Some(cancelled)),
                }),
                fence,
                1,
            )
            .unwrap();
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(None),
                }),
                fence,
                1,
            )
            .unwrap();
        assert_eq!(owner.queued_reserve, Some(request));
        assert_eq!(owner.reserve_keys[&request.request_nonce], key);
        assert!(owner.cold_retry_used);
        assert_eq!(owner.queued_reserve.take(), Some(request));
        let rejected = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Reserve {
                        nonce: request.request_nonce,
                        kind: request.kind,
                        trainer_region: request.trainer_region,
                        trainer_ordinal: request.trainer_ordinal,
                        key,
                    },
                    result: Err(super::BattleApiError::Stale),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(rejected.as_slice(), [super::BattleDelivery::ReserveRejected(record)]
            if record.request_nonce == request.request_nonce)
        );
        assert!(owner.cold_reconcile);
        assert!(owner.cold_retry_reserve.is_none());
    }

    #[test]
    fn cold_abort_is_redelivered_after_control_generation_reset() {
        let fence = fence();
        let mut owner = super::BattleOwner::new_process();
        owner.bind(fence, 1);
        let view: super::BattleReservationView =
            serde_json::from_value(reservation_json()).unwrap();
        owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Poll,
                    result: Ok(Some(view.clone())),
                }),
                fence,
                1,
            )
            .unwrap();
        let cancel = owner.queued_cancel.take().unwrap();
        let mut cancelled = view;
        cancelled.status = super::BattleStatus::Cancelled;
        let first = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 1,
                    job: super::Job::Cancel {
                        battle_id: cancel.0,
                        group_id: cancel.1,
                        key: cancel.2,
                    },
                    result: Ok(Some(cancelled)),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(
            first.as_slice(),
            [super::BattleDelivery::Abort(_)]
        ));
        owner.bind(fence, 2);
        let replay = owner
            .finish(
                Some(super::Completion {
                    fence,
                    generation: 2,
                    job: super::Job::Poll,
                    result: Err(super::BattleApiError::NotFound),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(
            matches!(replay.as_slice(), [super::BattleDelivery::Abort(record)]
            if record.battle_id == super::bridge_id(cancel.0))
        );
    }

    #[test]
    fn stale_battle_frames_do_not_abort_the_current_battle() {
        let (mut owner, fence, view) = tracked_owner();
        let old = super::bridge_id(uuid::Uuid::from_u128(123));
        let current = super::bridge_id(view.battle_id);
        let chunk = coop_protocol::PartySnapshotChunk {
            battle_id: old,
            party_slot: 0,
            chunk_index: 0,
            chunk_count: 1,
            mon: vec![1; 100],
        };
        assert!(owner.party_chunk(fence, 1, chunk.clone()).is_none());
        assert!(
            owner
                .action_intent(
                    fence,
                    1,
                    coop_protocol::ActionIntent {
                        battle_id: old,
                        turn: 1,
                        action: vec![1]
                    }
                )
                .is_none()
        );
        assert!(
            owner
                .turn_hash(
                    fence,
                    1,
                    coop_protocol::TurnResultHash {
                        battle_id: old,
                        turn: 1,
                        digest: coop_protocol::BattleDigest([2; 32])
                    }
                )
                .is_none()
        );
        assert_eq!(owner.tracked.unwrap().battle_id, view.battle_id);
        assert!(owner.snapshot.is_none());
        assert!(
            owner
                .party_chunk(
                    fence,
                    1,
                    coop_protocol::PartySnapshotChunk {
                        battle_id: current,
                        ..chunk
                    }
                )
                .is_none()
        );
        assert!(owner.snapshot.is_some());
        assert_eq!(
            owner
                .action_intent(
                    fence,
                    1,
                    coop_protocol::ActionIntent {
                        battle_id: current,
                        turn: 0,
                        action: vec![1]
                    }
                )
                .unwrap()
                .reason,
            5
        );
        assert!(owner.tracked.is_none());
    }

    #[test]
    fn consensus_manifest_bundle_and_reconnect_pause_replay() {
        use super::{BattleDelivery, ConsensusCompletion, ConsensusJob};
        let (mut owner, fence, view) = tracked_owner();
        let digest = "00".repeat(32);
        let mut reservation = reservation_json();
        reservation["status"] = json!("ACCEPTED");
        let consensus: super::BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation,
            "commitments": [digest, "11".repeat(32)],
            "manifest": { "battle_id": view.battle_id, "member_character_ids": view.member_character_ids, "snapshot_hashes": ["00".repeat(32), "11".repeat(32)], "seed": "22".repeat(32) },
            "turns": [{ "number": 1, "actions": [null, null], "state_hashes": [null, null], "action_keys": [null, null], "hash_keys": [null, null] }]
        })).unwrap();
        let first = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(first.as_slice(), [BattleDelivery::Manifest(_)]));
        assert_eq!(owner.accepted_battle, Some(view.battle_id));
        owner.peer_delivered = true;
        owner.start_delivered = true;
        owner.submitted_actions = 1;
        let paused = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(paused.as_slice(), [BattleDelivery::Pause(record)] if record.missing_slot == 1)
        );
        owner.invalidate();
        assert!(owner.accepted_battle.is_none());
        let replay = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(replay.as_slice(), [BattleDelivery::Manifest(_)]));
        owner.peer_delivered = true;
        owner.start_delivered = true;
        let resumed = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(resumed.as_slice(), [BattleDelivery::Pause(record)] if record.missing_slot == 1)
        );
        let mut complete = consensus;
        complete.turns[0].actions[0] = Some("00af".into());
        complete.turns[0].actions[1] = Some("1011".into());
        let bundles = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(complete),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(
            matches!(bundles.as_slice(), [BattleDelivery::Bundle(record)] if record.actions == [vec![0, 0xaf], vec![0x10, 0x11]])
        );
    }

    #[test]
    fn responder_snapshot_waits_for_validated_server_acceptance() {
        let (mut owner, fence, view) = tracked_owner();
        let tracked = owner.tracked.as_mut().unwrap();
        tracked.role = coop_protocol::BattleRole::Responder;
        tracked.nonce = 0;
        owner.party_chunk(
            fence,
            1,
            coop_protocol::PartySnapshotChunk {
                battle_id: super::bridge_id(view.battle_id),
                party_slot: 0,
                chunk_index: 0,
                chunk_count: 1,
                mon: vec![1; 100],
            },
        );
        assert!(owner.snapshot.is_some());
        assert!(owner.accepted_battle.is_none());
        let mut reservation = reservation_json();
        reservation["status"] = json!("ACCEPTED");
        let consensus: super::BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation,
            "commitments": [null, null],
            "manifest": null,
            "turns": []
        }))
        .unwrap();
        owner
            .finish_consensus(
                Some(super::ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: super::ConsensusJob::Inspect,
                    result: Ok(consensus),
                }),
                fence,
                1,
            )
            .unwrap();
        assert_eq!(owner.accepted_battle, Some(view.battle_id));
    }

    #[test]
    fn battle_start_requires_matching_rom_proof_and_server_release() {
        use super::{BattleDelivery, ConsensusCompletion, ConsensusJob};
        let (mut owner, fence, view) = tracked_owner();
        let mut reservation = reservation_json();
        reservation["status"] = json!("ACCEPTED");
        let mut consensus: super::BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation,
            "commitments": ["00".repeat(32), "11".repeat(32)],
            "manifest": { "battle_id": view.battle_id, "member_character_ids": view.member_character_ids,
                "snapshot_hashes": ["00".repeat(32), "11".repeat(32)], "seed": "22".repeat(32) },
            "ready": [false, false], "start_released": false, "turns": []
        })).unwrap();
        owner.snapshot = Some(("00".repeat(32), super::new_key()));
        owner.submitted_snapshot = true;
        let manifest = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(manifest.as_slice(), [BattleDelivery::Manifest(_)]));
        owner.peer_delivered = true;
        let wrong = coop_protocol::BattleReadyRecord {
            battle_id: super::bridge_id(view.battle_id),
            party_digest: coop_protocol::BattleDigest([1; 32]),
        };
        assert!(owner.battle_ready(fence, 1, wrong).is_some());
        // The mismatched proof aborts this owner, so use a fresh accepted ROM lifecycle.
        let (mut owner, fence, view) = tracked_owner();
        owner.snapshot = Some(("00".repeat(32), super::new_key()));
        owner.submitted_snapshot = true;
        owner.accepted_battle = Some(view.battle_id);
        owner.manifest_delivered = true;
        owner.peer_delivered = true;
        let proof = coop_protocol::BattleReadyRecord {
            battle_id: super::bridge_id(view.battle_id),
            party_digest: coop_protocol::BattleDigest([0; 32]),
        };
        assert!(owner.battle_ready(fence, 1, proof).is_none());
        consensus.ready = [true, false];
        let ready_key = owner.ready_proof.unwrap().1;
        let early = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Ready {
                        digest: proof.party_digest,
                        key: ready_key,
                    },
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(early.is_empty());
        assert!(!owner.start_delivered);
        consensus.ready = [true, true];
        consensus.start_released = true;
        let release = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus.clone()),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(release.as_slice(), [BattleDelivery::Start(record)]
            if record.battle_id == super::bridge_id(view.battle_id)));
        let duplicate = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(duplicate.is_empty());
    }

    #[test]
    fn control_restart_requires_fresh_rom_ready_and_reuses_receipt_key() {
        let (mut owner, fence, view) = tracked_owner();
        let digest = coop_protocol::BattleDigest([0; 32]);
        owner.snapshot = Some((super::hex_string(&digest.0), super::new_key()));
        owner.accepted_battle = Some(view.battle_id);
        owner.manifest_delivered = true;
        owner.peer_delivered = true;
        let proof = coop_protocol::BattleReadyRecord {
            battle_id: super::bridge_id(view.battle_id),
            party_digest: digest,
        };
        assert!(owner.battle_ready(fence, 1, proof).is_none());
        let key = owner.ready_proof.unwrap().1;
        owner.submitted_ready = true;
        owner.start_delivered = true;
        owner.invalidate();
        assert!(!owner.ready_from_rom && !owner.submitted_ready && !owner.start_delivered);
        assert_eq!(owner.ready_proof.unwrap().1, key);
        owner.accepted_battle = Some(view.battle_id);
        owner.manifest_delivered = true;
        owner.peer_delivered = true;
        assert!(owner.battle_ready(fence, 1, proof).is_none());
        assert_eq!(owner.ready_proof.unwrap().1, key);
        assert!(owner.ready_from_rom);
    }

    #[test]
    fn early_ready_after_control_replacement_waits_for_manifest_and_peer() {
        use super::{ConsensusCompletion, ConsensusJob};
        let (mut owner, fence, view) = tracked_owner();
        let digest = coop_protocol::BattleDigest([0; 32]);
        owner.snapshot = Some((super::hex_string(&digest.0), super::new_key()));
        owner.submitted_snapshot = true;
        owner.invalidate();
        let proof = coop_protocol::BattleReadyRecord {
            battle_id: super::bridge_id(view.battle_id),
            party_digest: digest,
        };
        assert!(owner.battle_ready(fence, 2, proof).is_none());
        assert!(matches!(
            owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Inspect
        ));
        let mut reservation = reservation_json();
        reservation["status"] = json!("ACCEPTED");
        let consensus = serde_json::from_value(json!({
            "reservation": reservation,
            "commitments": ["00".repeat(32), "11".repeat(32)],
            "manifest": { "battle_id": view.battle_id,
                "member_character_ids": view.member_character_ids,
                "snapshot_hashes": ["00".repeat(32), "11".repeat(32)],
                "seed": "22".repeat(32) },
            "ready": [false, false], "start_released": false, "turns": []
        }))
        .unwrap();
        let delivered = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 2,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(matches!(
            delivered.as_slice(),
            [super::BattleDelivery::Manifest(_)]
        ));
        assert!(matches!(
            owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Inspect
        ));
        owner.peer_delivered = true;
        assert!(matches!(owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Ready { digest: observed, .. } if observed == digest));
    }

    #[test]
    fn queued_action_after_control_replacement_waits_for_start() {
        use super::ConsensusJob;
        let (mut owner, fence, view) = tracked_owner();
        owner.snapshot = Some(("00".repeat(32), super::new_key()));
        owner.submitted_snapshot = true;
        owner.start_delivered = true;
        owner.invalidate();
        let action = coop_protocol::ActionIntent {
            battle_id: super::bridge_id(view.battle_id),
            turn: 1,
            action: vec![0x12, 0x34],
        };
        assert!(owner.action_intent(fence, 2, action.clone()).is_none());
        assert_eq!(owner.actions[&1].0, "1234");
        assert!(matches!(
            owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Inspect
        ));
        owner.accepted_battle = Some(view.battle_id);
        owner.manifest_delivered = true;
        owner.peer_delivered = true;
        assert!(matches!(
            owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Inspect
        ));
        owner.start_delivered = true;
        assert!(matches!(
            owner.consensus_job(owner.tracked.unwrap()),
            ConsensusJob::Action { turn: 1, .. }
        ));
        assert!(owner.action_intent(fence, 2, action.clone()).is_none());
        assert!(
            owner
                .action_intent(
                    fence,
                    2,
                    coop_protocol::ActionIntent {
                        battle_id: coop_protocol::BattleId([9; 16]),
                        ..action.clone()
                    }
                )
                .is_none()
        );
        assert!(
            owner
                .action_intent(
                    fence,
                    2,
                    coop_protocol::ActionIntent {
                        action: vec![0x99],
                        ..action
                    }
                )
                .is_some()
        );
    }

    #[test]
    fn redacted_incomplete_turn_pauses_the_remote_slot_for_responder() {
        use super::{BattleDelivery, ConsensusCompletion, ConsensusJob};
        let (mut owner, mut fence, view) = tracked_owner();
        fence.character_id = view.member_character_ids[1];
        owner.bind(fence, 2);
        owner.tracked = Some(super::Tracked {
            group_id: view.group_id,
            battle_id: view.battle_id,
            members: view.member_character_ids,
            kind: view.kind,
            trainer_identity: Some((coop_protocol::RegionId::Hoenn, 518)),
            role: coop_protocol::BattleRole::Responder,
            nonce: 0,
            local_rewards: false,
        });
        owner.submitted_actions = 1;
        owner.peer_delivered = true;
        owner.start_delivered = true;
        let mut reservation = reservation_json();
        reservation["status"] = json!("ACCEPTED");
        let consensus: super::BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation,
            "commitments": ["00".repeat(32), "11".repeat(32)],
            "manifest": { "battle_id": view.battle_id, "member_character_ids": view.member_character_ids, "snapshot_hashes": ["00".repeat(32), "11".repeat(32)], "seed": "22".repeat(32) },
            "turns": [{ "number": 1, "actions": [null, null], "state_hashes": [null, null], "action_keys": [null, null], "hash_keys": [null, null] }]
        })).unwrap();
        let deliveries = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 2,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus),
                }),
                fence,
                2,
            )
            .unwrap();
        assert!(
            matches!(deliveries.as_slice(), [BattleDelivery::Manifest(_), BattleDelivery::Pause(record)] if record.missing_slot == 0)
        );
    }

    #[test]
    fn diverged_consensus_aborts_with_desync_reason() {
        use super::{BattleDelivery, ConsensusCompletion, ConsensusJob};
        let (mut owner, fence, _) = tracked_owner();
        let mut reservation = reservation_json();
        reservation["status"] = json!("DIVERGED");
        let consensus: super::BattleConsensusView = serde_json::from_value(json!({
            "reservation": reservation, "commitments": [null, null], "manifest": null, "turns": []
        }))
        .unwrap();
        let result = owner
            .finish_consensus(
                Some(ConsensusCompletion {
                    fence,
                    generation: 1,
                    job: ConsensusJob::Inspect,
                    result: Ok(consensus),
                }),
                fence,
                1,
            )
            .unwrap();
        assert!(matches!(result.as_slice(), [BattleDelivery::Abort(record)] if record.reason == 3));
    }
}

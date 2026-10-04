//! Authenticated battle reservation and bounded turn consensus.
//!
//! This module coordinates two clients after consent. It does not execute a
//! battle, mutate a save, or accept a client supplied result.

use std::collections::HashSet;

use coop_cloud::{
    ApiVersion, ArtifactIdentity, CharacterId, CommitId, GroupId, IdempotencyKey, LeaseFence,
    Revision, SnapshotFinalizeRequest, SnapshotId, SnapshotRecord, UnixTimestampMillis,
};
use coop_protocol::{
    FriendlyBattleRules, IdentityKind, RegionId, TrainerInstanceId, WorldZone, identity_catalog,
};
use coop_save::PokemonSlot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::storage::{
    BATTLE_ACCEPTED_IDLE_TTL_MS, BATTLE_IDEMPOTENCY_TTL_MS, BATTLE_RESERVATION_TTL_MS,
    MAX_BATTLE_ACTION_BYTES, MAX_BATTLE_IDEMPOTENCY, MAX_BATTLE_IDEMPOTENCY_PER_MEMBER,
    MAX_BATTLE_RESERVATIONS, MAX_BATTLE_TURNS, Store,
};
use super::trainer_rules::{HOENN_STORY_BATTLES, MemberStanding, TrainerRule, trainer_rule};
use super::{AuthenticatedActor, Phase2Error};

const OP_RESERVE: &str = "battle_reserve_v1";
const OP_ACCEPT: &str = "battle_accept_v1";
const OP_DECLINE: &str = "battle_decline_v1";
const OP_CANCEL: &str = "battle_cancel_v1";
const OP_FINISH: &str = "battle_finish_v1";

/// The kind is descriptive only in this slice.  No battle engine or
/// progression authority is invoked by a reservation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleReservationKind {
    CooperativeTrainer,
    Friendly,
}

/// Who applies the rewards of a cooperative trainer win.
///
/// `Ledger` is the original server progression handoff (Wally): a Won result
/// becomes `CommitPending` and issues one commit grant per member. `Local`
/// completes the battle on a matching Won result and issues no grant; each
/// ROM applies the vanilla rewards itself. Records persisted before this
/// field existed default to `Ledger`, so their behavior is unchanged.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleRewardMode {
    #[default]
    Ledger,
    Local,
    /// A friendly battle between the two grouped players: each member
    /// stages exactly the challenge's team size of battle-ready records (like
    /// `Local`, the snapshot is those records), and the result gives nothing
    /// to anyone: no ledger entry, no grant, no member roles.
    None,
}

impl BattleRewardMode {
    /// Members stage the exact party records they battle with.
    const fn stages_records(self) -> bool {
        matches!(self, Self::Local | Self::None)
    }
}

impl BattleRewardMode {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn is_ledger(&self) -> bool {
        *self == Self::Ledger
    }
}

/// A member's part in a local-reward trainer battle. The requester is always
/// a `Participant`. A partner who has already beaten a first-battle trainer,
/// has not beaten a rematch's first battle, or is not at a gym's story point
/// joins as a `Helper` (see `trainer_rules`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleMemberRole {
    Participant,
    Helper,
}

/// Number of Pokémon records a local-reward snapshot commit may carry.
const MAX_STAGED_PARTY_RECORDS: usize = 6;
/// Usable (non-egg, HP above zero) Pokémon each side may bring.
const MAX_STAGED_USABLE: usize = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReservationRequest {
    pub api_version: ApiVersion,
    pub kind: BattleReservationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_id: Option<TrainerInstanceId>,
    /// A friendly challenge (game protocol 5): format, level mode and team
    /// size. Friendly reservations without rules keep the original anchored
    /// ledger shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly_rules: Option<FriendlyBattleRules>,
    pub idempotency_key: IdempotencyKey,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReservationActionRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleReservationStatus {
    Pending,
    Accepted,
    /// Both members attested the same terminal result that needs no further
    /// progression commit. Friendly results and cooperative losses/draws use
    /// this terminal state.
    Completed,
    /// A trainer result is attested but awaits the separate progression
    /// authority. This phase never grants rewards or publishes a save.
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
    pub kind: BattleReservationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_id: Option<TrainerInstanceId>,
    pub status: BattleReservationStatus,
    pub expires_at: UnixTimestampMillis,
    /// Omitted on the wire for `Ledger` so friendly and Wally views keep
    /// their exact pre-existing shape.
    #[serde(default, skip_serializing_if = "BattleRewardMode::is_ledger")]
    pub reward_mode: BattleRewardMode,
    /// Present only for `Local` trainer battles, in `member_character_ids`
    /// order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_roles: Option<[BattleMemberRole; 2]>,
    /// The friendly challenge, shown to the partner before it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly_rules: Option<FriendlyBattleRules>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum BattleOperation {
    Reserve,
    Accept,
    Decline,
    Cancel,
    Finish,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BattleIdempotencyResponse {
    pub view: BattleReservationView,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BattleReservationRecord {
    pub view: BattleReservationView,
    pub expires_at: u64,
    pub retain_until: u64,
    #[serde(default)]
    party_anchors: Option<[PartyAnchor; 2]>,
    #[serde(default)]
    pub consensus: BattleConsensusState,
    /// A pair of durable, one-use-in-the-ledger capabilities. This remains
    /// optional so records written before the progression handoff existed
    /// deserialize safely and fail closed when a grant is requested.
    #[serde(default)]
    commit_grants: Option<[BattleCommitGrant; 2]>,
    /// Per-member consumption state for the grants above. Legacy records
    /// default to all-unapplied and still fail closed because they have no
    /// `commit_grants` value.
    #[serde(default)]
    commit_grant_applied: [bool; 2],
    /// Local-reward mode only: the exact party records each member staged in
    /// its snapshot commitment. `peer_party` serves these to the partner.
    #[serde(default)]
    staged_parties: [Option<StagedParty>; 2],
}

/// A validated set of 100-byte party records (lowercase hex) and the party
/// digest computed from them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StagedParty {
    digest: String,
    records: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PartyAnchor {
    snapshot_id: SnapshotId,
    revision: Revision,
    digest: String,
}

/// These records coordinate clients only. Neither a submitted action nor an
/// agreed hash is an authoritative battle result.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct BattleConsensusState {
    commitments: [Option<Commitment>; 2],
    manifest: Option<BattleManifest>,
    /// One durable receipt per canonical member. Missing on old persisted
    /// records and until that member has loaded the manifest.
    #[serde(default)]
    ready: [Option<ReadyReceipt>; 2],
    turns: Vec<BattleTurn>,
    /// At most one terminal attestation per canonical member. Retained in
    /// terminal tombstones so an exact retry can replay its response.
    #[serde(default)]
    finish_attestations: [Option<BattleFinishAttestation>; 2],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Commitment {
    snapshot_hash: String,
    idempotency_key: IdempotencyKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ReadyReceipt {
    snapshot_hash: String,
    idempotency_key: IdempotencyKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattleManifest {
    pub battle_id: Uuid,
    pub member_character_ids: [CharacterId; 2],
    pub snapshot_hashes: [String; 2],
    pub seed: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattleTurn {
    pub number: u16,
    pub actions: [Option<String>; 2],
    pub state_hashes: [Option<String>; 2],
    action_keys: [Option<IdempotencyKey>; 2],
    hash_keys: [Option<IdempotencyKey>; 2],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattleConsensusView {
    pub reservation: BattleReservationView,
    pub commitments: [Option<String>; 2],
    pub manifest: Option<BattleManifest>,
    pub ready: [bool; 2],
    pub start_released: bool,
    pub turns: Vec<BattleTurn>,
}

/// The peer's exact, validated party records from the finalized save pinned
/// by this battle. Each string is lowercase hex for one 100-byte record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattlePeerPartyView {
    pub api_version: ApiVersion,
    pub battle_id: Uuid,
    pub peer_character_id: CharacterId,
    pub snapshot_revision: Revision,
    pub snapshot_hash: String,
    pub party_records: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleSnapshotCommitRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
    pub snapshot_hash: String,
    /// Required in `Local` reward mode and rejected in `Ledger` mode: 1-6
    /// exact 100-byte party records, each as 200 lowercase hex characters
    /// (the same encoding `BattlePeerPartyView::party_records` uses). One to
    /// three of them must be usable (not an egg, HP above zero), and
    /// `snapshot_hash` must equal the party digest of these records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party_records: Option<Vec<String>>,
}

/// Confirms that this member's running ROM has loaded the published battle
/// manifest and is still using the exact party snapshot that was committed.
/// The server releases the battle only after both members have submitted this
/// receipt.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleReadyRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
    #[serde(alias = "party_digest")]
    pub snapshot_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleActionIntentRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
    pub turn: u16,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleStateHashRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
    pub turn: u16,
    pub state_hash: String,
}

/// A pre-ledger terminal battle attestation. It is accepted only after both
/// clients have acknowledged the same final turn hash. It never mutates a
/// save or awards trainer progress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BattleFinishRequest {
    pub api_version: ApiVersion,
    pub idempotency_key: IdempotencyKey,
    pub result: BattleFinishResult,
    pub turn: u16,
    pub state_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BattleFinishResult {
    /// Friendly result in the reservation's canonical member order.
    Member0Won,
    Member1Won,
    Draw,
    /// Cooperative trainer result shared by both members.
    Won,
    Lost,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct BattleFinishAttestation {
    result: BattleFinishResult,
    turn: u16,
    state_hash: String,
    idempotency_key: IdempotencyKey,
}

/// Internal handoff capability for the future progression ledger. It carries
/// enough immutable identity to prevent a grant from being replayed against a
/// different battle, character, trainer, or finalized source snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BattleCommitGrant {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BattleIdempotencyRecord {
    pub operation: BattleOperation,
    pub fingerprint: [u8; 32],
    pub response: BattleIdempotencyResponse,
    pub expires_at: u64,
}

impl BattleReservationRequest {
    fn validate(&self) -> Result<(), Phase2Error> {
        if self.api_version.value() != 1 {
            return Err(Phase2Error::InvalidRequest);
        }
        if matches!(self.kind, BattleReservationKind::CooperativeTrainer)
            != self.trainer_id.is_some()
        {
            return Err(Phase2Error::InvalidRequest);
        }
        match (self.kind, self.friendly_rules) {
            (BattleReservationKind::CooperativeTrainer, Some(_)) => {
                Err(Phase2Error::InvalidRequest)
            }
            (_, Some(rules)) if rules.validate().is_err() => Err(Phase2Error::InvalidRequest),
            _ => Ok(()),
        }
    }
}

impl BattleReservationActionRequest {
    fn validate(&self) -> Result<(), Phase2Error> {
        if self.api_version.value() != 1 {
            return Err(Phase2Error::InvalidRequest);
        }
        Ok(())
    }
}

fn fingerprint<T: Serialize>(operation: &str, values: &T) -> Result<[u8; 32], Phase2Error> {
    let bytes = serde_json::to_vec(&(operation, values)).map_err(|_| Phase2Error::Internal)?;
    Ok(Sha256::digest(bytes).into())
}

fn add_ms(now: u64, amount: u64) -> Result<u64, Phase2Error> {
    now.checked_add(amount).ok_or(Phase2Error::Internal)
}

fn lease_matches(
    state: &super::storage::State,
    character_id: CharacterId,
    fence: LeaseFence,
    now: u64,
) -> Result<(), Phase2Error> {
    if state.handoff_for_member(character_id) {
        return Err(Phase2Error::Conflict);
    }

    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Expired)?;
    if lease.contract.character_id != character_id
        || lease.released
        || lease.contract.expires_at.value() <= now
    {
        return Err(Phase2Error::Expired);
    }
    if lease.contract.fence() != fence {
        return Err(Phase2Error::Conflict);
    }
    Ok(())
}

fn actor_owns_character(
    state: &super::storage::State,
    actor: AuthenticatedActor,
) -> Result<(), Phase2Error> {
    if state
        .characters
        .get(&actor.character_id)
        .is_some_and(|character| character.owner == actor.user_id)
    {
        Ok(())
    } else {
        Err(Phase2Error::NotFound)
    }
}

/// Validates group membership and the server-owned leases. The caller's lease
/// remains strictly live and fenced. A companion whose lease TTL elapsed can
/// still participate in an already-active battle while its reconnect grace is
/// live; group expiry closes the battle once that grace ends. The companion's
/// fence is never accepted from the caller; the server reads the companion's
/// current lease from the same repository transaction.
fn group_and_current_members(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
    now: u64,
) -> Result<[CharacterId; 2], Phase2Error> {
    actor_owns_character(state, actor)?;
    let group = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if group.status != super::storage::GroupStatus::Active
        || !group.group.contains(actor.character_id)
    {
        return Err(Phase2Error::NotFound);
    }
    let members = group.group.members();
    if state.active_group_by_member.get(&members[0]) != Some(&group_id)
        || state.active_group_by_member.get(&members[1]) != Some(&group_id)
    {
        return Err(Phase2Error::NotFound);
    }
    lease_matches(state, actor.character_id, fence, now)?;
    for member in members {
        super::group_travel::validate_member(state, member)?;
        if member != actor.character_id {
            let lease = state.leases.get(&member).ok_or(Phase2Error::Expired)?;
            if lease.contract.character_id != member || lease.released || lease.grace_until <= now {
                return Err(Phase2Error::Expired);
            }
        }
    }
    Ok(members)
}

fn prune_locked(state: &mut super::storage::State, now: u64) {
    state
        .battle_idempotency
        .retain(|_, record| record.expires_at > now);

    let mut release_members = Vec::new();
    let mut remove_reservations = Vec::new();
    let mut void_battles = Vec::new();
    for (battle_id, record) in &mut state.battle_reservations {
        let group_active = state
            .groups
            .get(&record.view.group_id)
            .is_some_and(|group| {
                group.status == super::storage::GroupStatus::Active
                    && state
                        .active_group_by_member
                        .get(&record.view.member_character_ids[0])
                        == Some(&record.view.group_id)
                    && state
                        .active_group_by_member
                        .get(&record.view.member_character_ids[1])
                        == Some(&record.view.group_id)
            });
        if matches!(
            record.view.status,
            BattleReservationStatus::Pending
                | BattleReservationStatus::Accepted
                | BattleReservationStatus::CommitPending
        ) && (!group_active || record.expires_at <= now)
        {
            if record.view.status == BattleReservationStatus::CommitPending {
                void_battles.push(*battle_id);
            }
            record.view.status = BattleReservationStatus::Expired;
            release_members.extend(record.view.member_character_ids);
            record.retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS).unwrap_or(u64::MAX);
        } else if record.retain_until <= now {
            remove_reservations.push(*battle_id);
        }
    }
    for battle_id in remove_reservations {
        state.battle_reservations.remove(&battle_id);
    }
    for battle_id in void_battles {
        super::ledger::void_battle_entries(state, battle_id);
    }
    let live_ids: HashSet<_> = state
        .battle_reservations
        .iter()
        .filter(|(_, record)| {
            matches!(
                record.view.status,
                BattleReservationStatus::Pending
                    | BattleReservationStatus::Accepted
                    | BattleReservationStatus::CommitPending
            )
        })
        .map(|(id, _)| *id)
        .collect();
    state
        .active_battle_by_member
        .retain(|_, battle_id| live_ids.contains(battle_id));
    for member in release_members {
        if state
            .active_battle_by_member
            .get(&member)
            .is_some_and(|battle_id| !live_ids.contains(battle_id))
        {
            state.active_battle_by_member.remove(&member);
        }
    }
}

fn reserve_view(
    battle_id: Uuid,
    group_id: GroupId,
    members: [CharacterId; 2],
    initiator: CharacterId,
    kind: BattleReservationKind,
    trainer_id: Option<TrainerInstanceId>,
    status: BattleReservationStatus,
    expires_at: u64,
    reward_mode: BattleRewardMode,
    member_roles: Option<[BattleMemberRole; 2]>,
    friendly_rules: Option<FriendlyBattleRules>,
) -> Result<BattleReservationView, Phase2Error> {
    Ok(BattleReservationView {
        api_version: ApiVersion::V1,
        battle_id,
        group_id,
        initiator_character_id: initiator,
        member_character_ids: members,
        kind,
        trainer_id,
        status,
        expires_at: Store::unix_timestamp(expires_at).map_err(Phase2Error::from)?,
        reward_mode,
        member_roles,
        friendly_rules,
    })
}

fn check_idempotency_capacity(
    state: &super::storage::State,
    actor: CharacterId,
) -> Result<(), Phase2Error> {
    if state.battle_idempotency.len() >= MAX_BATTLE_IDEMPOTENCY
        || state
            .battle_idempotency
            .keys()
            .filter(|(member, _)| *member == actor)
            .count()
            >= MAX_BATTLE_IDEMPOTENCY_PER_MEMBER
    {
        Err(Phase2Error::Busy)
    } else {
        Ok(())
    }
}

/// SHA-256("coop-battle-party-v1\0" || count:u8 || each occupied party
/// Pokémon's exact 100-byte serialized record in party order), lowercase hex.
/// The source is the latest finalized character.sav, not live unsaved RAM.
fn party_digest(save: &coop_save::ValidatedSaveV2) -> Result<String, Phase2Error> {
    Ok(records_digest(&party_records(save)?))
}

/// The same party digest as [`party_digest`], over records supplied directly.
fn records_digest(records: &[[u8; 100]]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"coop-battle-party-v1\0");
    hash.update([records.len() as u8]);
    for record in records {
        hash.update(record);
    }
    hex_string(&hash.finalize())
}

pub(super) fn parse_hex_record(text: &str) -> Option<[u8; 100]> {
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
    let bytes = text.as_bytes();
    if bytes.len() != 200 {
        return None;
    }
    let mut record = [0_u8; 100];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        record[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(record)
}

/// Structurally validates staged party records: 1-6 occupied,
/// checksum-valid records. Returns the party and its usable count.
fn staged_party(records: &[String]) -> Result<(StagedParty, usize), Phase2Error> {
    if records.is_empty() || records.len() > MAX_STAGED_PARTY_RECORDS {
        return Err(Phase2Error::InvalidRequest);
    }
    let mut raw = Vec::with_capacity(records.len());
    let mut usable = 0_usize;
    for text in records {
        let record = parse_hex_record(text).ok_or(Phase2Error::InvalidRequest)?;
        match coop_save::decode_party_record(record) {
            Ok(PokemonSlot::Occupied(pokemon)) => {
                if !pokemon.identity.is_egg
                    && !pokemon.identity.is_bad_egg
                    && coop_save::party_record_hp(&record) > 0
                {
                    usable += 1;
                }
            }
            Ok(PokemonSlot::Empty { .. }) | Err(_) => return Err(Phase2Error::InvalidRequest),
        }
        raw.push(record);
    }
    Ok((
        StagedParty {
            digest: records_digest(&raw),
            records: raw.iter().map(|record| hex_string(record)).collect(),
        },
        usable,
    ))
}

/// The staging rule of a reservation: a local-reward side brings one to
/// three usable Pokémon among its records; a friendly side brings exactly
/// the challenge's team size, every one of them battle-ready.
fn staged_party_allowed(view: &BattleReservationView, party: &StagedParty, usable: usize) -> bool {
    match view.reward_mode {
        BattleRewardMode::Local => (1..=MAX_STAGED_USABLE).contains(&usable),
        BattleRewardMode::None => view.friendly_rules.is_some_and(|rules| {
            usize::from(rules.team_size) == party.records.len() && usable == party.records.len()
        }),
        BattleRewardMode::Ledger => false,
    }
}

/// The party digest a member's commitment, manifest slot, and ready receipt
/// must carry: the anchored save party in `Ledger` mode, or the staged
/// records in `Local` mode (none until that member commits).
fn expected_party_digest(record: &BattleReservationRecord, index: usize) -> Option<&str> {
    match record.view.reward_mode {
        BattleRewardMode::Ledger => record
            .party_anchors
            .as_ref()
            .map(|anchors| anchors[index].digest.as_str()),
        BattleRewardMode::Local | BattleRewardMode::None => record.staged_parties[index]
            .as_ref()
            .map(|party| party.digest.as_str()),
    }
}

fn party_records(save: &coop_save::ValidatedSaveV2) -> Result<Vec<[u8; 100]>, Phase2Error> {
    let count = save.party_count().map_err(|_| Phase2Error::Conflict)?;
    if count == 0 {
        return Err(Phase2Error::Conflict);
    }
    let mut records = Vec::with_capacity(usize::from(count));
    for index in 0..usize::from(count) {
        match save
            .party_pokemon(index)
            .map_err(|_| Phase2Error::Conflict)?
        {
            PokemonSlot::Occupied(pokemon) => records.push(pokemon.raw),
            PokemonSlot::Empty { .. } => return Err(Phase2Error::Conflict),
        }
    }
    Ok(records)
}

fn snapshot_for_party(
    state: &super::storage::State,
    character_id: CharacterId,
) -> Result<SnapshotRecord, Phase2Error> {
    let character = state
        .characters
        .get(&character_id)
        .ok_or(Phase2Error::NotFound)?;
    if character.revision == Revision::initial() {
        return Err(Phase2Error::Conflict);
    }
    let snapshot_id = character.active_snapshot.ok_or(Phase2Error::Conflict)?;
    if state
        .snapshot_by_revision
        .get(&(character_id, character.revision))
        != Some(&snapshot_id)
    {
        return Err(Phase2Error::Conflict);
    }
    let snapshot = state
        .snapshots
        .get(&snapshot_id)
        .ok_or(Phase2Error::Conflict)?;
    if snapshot.character_id != character_id || snapshot.revision != character.revision {
        return Err(Phase2Error::Conflict);
    }
    snapshot.validate().map_err(|_| Phase2Error::Conflict)?;
    Ok(snapshot.clone())
}

/// Returns the anchor and what this member's save says about the requested
/// trainer (the default standing without a trainer).
fn party_anchor(
    store: &Store,
    character_id: CharacterId,
    snapshot: &SnapshotRecord,
    trainer: Option<(&TrainerInstanceId, &TrainerEncounter)>,
) -> Result<(PartyAnchor, MemberStanding), Phase2Error> {
    let sav = snapshot
        .files
        .iter()
        .find(|file| file.artifact == ArtifactIdentity::CharacterSav)
        .ok_or(Phase2Error::Conflict)?;
    let key = Store::object_key(
        character_id,
        snapshot.snapshot_id,
        ArtifactIdentity::CharacterSav,
    );
    let bytes = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
    sav.verify_bytes(&bytes)
        .map_err(|_| Phase2Error::Conflict)?;
    let save = super::saves::validate_character_sav(&bytes, snapshot.revision)
        .map_err(|_| Phase2Error::Conflict)?;
    if !save.coop().online_eligible() {
        return Err(Phase2Error::Conflict);
    }
    let mut standing = MemberStanding::default();
    if let Some((trainer_id, encounter)) = trainer {
        if save
            .coop()
            .progress_for(encounter.region)
            .is_none_or(|progress| progress.story_checkpoint < encounter.minimum_story_checkpoint)
        {
            return Err(Phase2Error::Conflict);
        }
        standing.defeated = save
            .coop()
            .defeated_trainer(trainer_id)
            .map_err(|_| Phase2Error::Conflict)?;
        match &encounter.rule {
            TrainerRule::Ordinary => {}
            TrainerRule::Rematch { base } => {
                standing.base_defeated = save
                    .coop()
                    .defeated_trainer(base)
                    .map_err(|_| Phase2Error::Conflict)?;
            }
            // Badges live in the SaveBlock1 flags of the uploaded save.
            TrainerRule::Gym { .. } => {
                standing.badges = save.hoenn_badges().ok_or(Phase2Error::Conflict)?;
            }
            // The story flags and vars live in the same SaveBlock1 arrays.
            TrainerRule::Story { battle } => {
                standing.story_point = HOENN_STORY_BATTLES
                    .get(*battle)
                    .ok_or(Phase2Error::Internal)?
                    .at_story_point(|flag| save.event_flag(flag), |var| save.event_var(var))
                    .ok_or(Phase2Error::Conflict)?;
            }
        }
    }
    Ok((
        PartyAnchor {
            snapshot_id: snapshot.snapshot_id,
            revision: snapshot.revision,
            digest: party_digest(&save)?,
        },
        standing,
    ))
}

struct TrainerEncounter {
    region: RegionId,
    /// The exact logical map for a server-configured story encounter. `None`
    /// means the catalog has no map for this trainer, so the members only
    /// need to be on the same or directly connected maps.
    map: Option<&'static str>,
    minimum_story_checkpoint: u32,
    reward_mode: BattleRewardMode,
    rule: TrainerRule,
}

const WALLY_VICTORY_ROAD: &str = "HOENN:TRAINER_WALLY_1";

#[cfg(test)]
thread_local! {
    /// The progression-ledger path (`BattleRewardMode::Ledger`: a Won result
    /// becomes `CommitPending` with commit grants) is no longer chosen for
    /// any trainer, but records persisted by an older server can still be in
    /// it. Tests of that machinery put Wally back on it for their thread.
    static LEGACY_WALLY_LEDGER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn legacy_wally_ledger() -> bool {
    LEGACY_WALLY_LEDGER.with(std::cell::Cell::get)
}

#[cfg(not(test))]
const fn legacy_wally_ledger() -> bool {
    false
}

// Every trainer with a persisted ordinal in the identity catalog can become a
// cooperative battle with local rewards. Wally's Victory Road battle keeps his
// configured story map; since A9 it is a local-reward story battle like the
// others (the ROM's legacy Wally special stays off). The catalog carries no
// trainer map, so other encounters are located only by the two members'
// positions.
fn trainer_encounter(trainer_id: &TrainerInstanceId) -> Result<TrainerEncounter, Phase2Error> {
    let entry = identity_catalog::trainer(trainer_id).map_err(|_| Phase2Error::Conflict)?;
    if entry.kind != IdentityKind::Trainer || entry.ordinal.is_none() {
        return Err(Phase2Error::Conflict);
    }
    let region = entry
        .region
        .ensure_concrete()
        .map_err(|_| Phase2Error::Conflict)?;
    let wally = trainer_id.as_str() == WALLY_VICTORY_ROAD;
    if wally && legacy_wally_ledger() {
        return Ok(TrainerEncounter {
            region,
            map: Some("VICTORY_ROAD_1F"),
            minimum_story_checkpoint: 0,
            reward_mode: BattleRewardMode::Ledger,
            rule: TrainerRule::Ordinary,
        });
    }
    Ok(TrainerEncounter {
        region,
        map: wally.then_some("VICTORY_ROAD_1F"),
        minimum_story_checkpoint: 0,
        reward_mode: BattleRewardMode::Local,
        rule: trainer_rule(trainer_id),
    })
}

/// Whether two maps of one region share a cardinal map edge (either header).
fn maps_connected(region: RegionId, first: &str, second: &str) -> bool {
    let (Ok(a), Ok(b)) = (
        coop_protocol::catalog::resolve_map(region, first),
        coop_protocol::catalog::resolve_map(region, second),
    ) else {
        return false;
    };
    coop_protocol::catalog::maps_share_edge(a.map_group, a.map_number, b.map_group, b.map_number)
        || coop_protocol::catalog::maps_share_edge(
            b.map_group,
            b.map_number,
            a.map_group,
            a.map_number,
        )
}

fn trainer_member_zones(
    state: &super::storage::State,
    members: [CharacterId; 2],
    encounter: &TrainerEncounter,
) -> Result<[WorldZone; 2], Phase2Error> {
    let zones = members.map(|member| {
        state
            .characters
            .get(&member)
            .map(|character| character.state.world_zone.clone())
            .ok_or(Phase2Error::Conflict)
    });
    let zones = [zones[0].clone()?, zones[1].clone()?];
    match encounter.map {
        Some(map) => {
            if zones[0] != zones[1]
                || zones.iter().any(|zone| {
                    zone.region != encounter.region || zone.map != map || zone.channel != 1
                })
            {
                return Err(Phase2Error::Conflict);
            }
        }
        None => {
            if zones.iter().any(|zone| zone.region != encounter.region)
                || zones[0].channel != zones[1].channel
                || (zones[0].map != zones[1].map
                    && !maps_connected(encounter.region, &zones[0].map, &zones[1].map))
            {
                return Err(Phase2Error::Conflict);
            }
        }
    }
    Ok(zones)
}

fn anchor_is_current(
    state: &super::storage::State,
    character_id: CharacterId,
    anchor: &PartyAnchor,
) -> bool {
    state
        .characters
        .get(&character_id)
        .is_some_and(|character| {
            character.active_snapshot == Some(anchor.snapshot_id)
                && character.revision == anchor.revision
        })
        && state
            .snapshot_by_revision
            .get(&(character_id, anchor.revision))
            == Some(&anchor.snapshot_id)
        && state
            .snapshots
            .get(&anchor.snapshot_id)
            .is_some_and(|snapshot| {
                snapshot.character_id == character_id && snapshot.revision == anchor.revision
            })
}

/// Creates the two progression handoff capabilities at the exact transition
/// that records the second matching cooperative-trainer Won receipt. The
/// reservation record is already cloned by `finish`, so a failure before the
/// caller inserts it cannot leave a half-issued grant behind.
fn issue_commit_grants(
    store: &Store,
    state: &super::storage::State,
    record: &BattleReservationRecord,
) -> Result<[BattleCommitGrant; 2], Phase2Error> {
    if record.view.kind != BattleReservationKind::CooperativeTrainer
        || record.view.status != BattleReservationStatus::CommitPending
        || record.commit_grants.is_some()
    {
        return Err(Phase2Error::Conflict);
    }
    let trainer_id = record
        .view
        .trainer_id
        .as_ref()
        .ok_or(Phase2Error::Conflict)?;
    let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
    let [Some(first), Some(second)] = &record.consensus.finish_attestations else {
        return Err(Phase2Error::Conflict);
    };
    if first.result != BattleFinishResult::Won
        || second.result != BattleFinishResult::Won
        || first.turn != second.turn
        || first.state_hash != second.state_hash
    {
        return Err(Phase2Error::Conflict);
    }
    if record
        .view
        .member_character_ids
        .iter()
        .enumerate()
        .any(|(index, member)| !anchor_is_current(state, *member, &anchors[index]))
    {
        return Err(Phase2Error::Conflict);
    }

    // UUID entropy is the usual uniqueness boundary, but check the persisted
    // state too. This keeps a replay or a restored repository from ever
    // minting a duplicate capability ID.
    let mut used_ids = HashSet::new();
    for existing in state.battle_reservations.values() {
        if let Some(grants) = &existing.commit_grants {
            used_ids.extend(grants.iter().map(|grant| grant.grant_id));
        }
    }
    used_ids.extend(state.ledger_entries.keys().copied());

    let mut grants = Vec::with_capacity(2);
    for (index, member) in record.view.member_character_ids.iter().enumerate() {
        let mut grant_id = None;
        for _ in 0..32 {
            let candidate = CommitId::new(store.random_uuid().map_err(Phase2Error::from)?)
                .map_err(|_| Phase2Error::Internal)?;
            if used_ids.insert(candidate) {
                grant_id = Some(candidate);
                break;
            }
        }
        let grant_id = grant_id.ok_or(Phase2Error::Internal)?;
        let anchor = &anchors[index];
        grants.push(BattleCommitGrant {
            grant_id,
            character_id: *member,
            battle_id: record.view.battle_id,
            trainer_id: trainer_id.clone(),
            source_snapshot_id: anchor.snapshot_id,
            source_revision: anchor.revision,
            source_party_digest: anchor.digest.clone(),
            terminal_turn: first.turn,
            terminal_state_hash: first.state_hash.clone(),
        });
    }
    grants.try_into().map_err(|_| Phase2Error::Internal)
}

/// Reads a progression handoff only while the requesting member still holds
/// the current lease fence and the grant's source snapshot remains current.
/// This is intentionally crate-private; the future progression authority will
/// call it from inside its own atomic save-commit flow.
pub(crate) fn retrieve_commit_grant(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
) -> Result<BattleCommitGrant, Phase2Error> {
    let now = store.now();
    store.read_transaction(|state| {
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        let index = members
            .iter()
            .position(|member| *member == actor.character_id)
            .ok_or(Phase2Error::Forbidden)?;
        let record = state
            .battle_reservations
            .get(&battle_id)
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id
            || record.view.member_character_ids != members
            || record.view.status != BattleReservationStatus::CommitPending
            || record.expires_at <= now
        {
            return Err(Phase2Error::Conflict);
        }
        let grant = record
            .commit_grants
            .as_ref()
            .and_then(|grants| grants.get(index))
            .ok_or(Phase2Error::Conflict)?;
        let anchor = record
            .party_anchors
            .as_ref()
            .and_then(|anchors| anchors.get(index))
            .ok_or(Phase2Error::Conflict)?;
        let [Some(first), Some(second)] = &record.consensus.finish_attestations else {
            return Err(Phase2Error::Conflict);
        };
        if grant.character_id != actor.character_id
            || grant.battle_id != battle_id
            || record.commit_grant_applied[index]
            || record.view.trainer_id.as_ref() != Some(&grant.trainer_id)
            || first.result != BattleFinishResult::Won
            || second.result != BattleFinishResult::Won
            || first.turn != second.turn
            || first.state_hash != second.state_hash
            || grant.terminal_turn != first.turn
            || grant.terminal_state_hash != first.state_hash
            || !anchor_is_current(state, actor.character_id, anchor)
            || grant.source_snapshot_id != anchor.snapshot_id
            || grant.source_revision != anchor.revision
            || grant.source_party_digest != anchor.digest
            || state
                .ledger_entries
                .get(&grant.grant_id)
                .is_some_and(|entry| !entry.status.is_open())
        {
            return Err(Phase2Error::Conflict);
        }
        Ok(grant.clone())
    })
}

fn wally_trainer_id() -> Result<TrainerInstanceId, Phase2Error> {
    TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").map_err(|_| Phase2Error::Internal)
}

fn wally_ordinal() -> Result<usize, Phase2Error> {
    identity_catalog::trainer(&wally_trainer_id()?)
        .map_err(|_| Phase2Error::Internal)?
        .ordinal
        .map(usize::from)
        .ok_or(Phase2Error::Internal)
}

/// Checks the exact CSP1 change that a Wally battle is allowed to introduce.
/// Save generation and CSP1 CRC are deliberately excluded because they are
/// rewritten by the ROM for every finalized save. Every other canonical field
/// must remain byte-for-byte equivalent, with exactly one newly set Wally bit.
fn validate_wally_csp_delta(
    source: &coop_save::ValidatedSaveV2,
    incoming: &coop_save::ValidatedSaveV2,
) -> Result<(), Phase2Error> {
    let before = source.coop();
    let after = incoming.coop();
    if before.registry != after.registry
        || before.status_flags != after.status_flags
        || before.regional_progress != after.regional_progress
        || before.events != after.events
        || before.unlocked_fly_points != after.unlocked_fly_points
        || before.gyms != after.gyms
    {
        return Err(Phase2Error::Conflict);
    }
    let ordinal = wally_ordinal()?;
    let byte = ordinal / 8;
    let mask = 1_u8 << (ordinal % 8);
    if before.defeated_trainers[byte] & mask != 0 || after.defeated_trainers[byte] & mask == 0 {
        return Err(Phase2Error::Conflict);
    }
    let mut expected = before.defeated_trainers;
    expected[byte] |= mask;
    if after.defeated_trainers != expected {
        return Err(Phase2Error::Conflict);
    }
    Ok(())
}

/// A Wally grant covers only his Victory Road story transition. Battle
/// damage and item use may change other save areas, but unrelated persistent
/// flags or script variables cannot ride along on this capability.
fn validate_wally_story_delta(
    source: &coop_save::ValidatedSaveV2,
    incoming: &coop_save::ValidatedSaveV2,
) -> Result<(), Phase2Error> {
    const FLAGS_OFFSET: usize = coop_save::SAVE_BLOCK1_FLAGS_OFFSET;
    const FLAG_BYTES: usize = coop_save::SAVE_BLOCK1_FLAG_BYTES;
    const VARS_OFFSET: usize = coop_save::SAVE_BLOCK1_VARS_OFFSET;
    const VAR_COUNT: usize = coop_save::SAVE_BLOCK1_VAR_COUNT;
    let before_flags = source
        .save_block1_range(FLAGS_OFFSET, FLAG_BYTES)
        .ok_or(Phase2Error::Conflict)?;
    let after_flags = incoming
        .save_block1_range(FLAGS_OFFSET, FLAG_BYTES)
        .ok_or(Phase2Error::Conflict)?;
    for (index, (before, after)) in before_flags.iter().zip(&after_flags).enumerate() {
        let allowed = [0x7e_usize, 0x35a]
            .into_iter()
            .filter(|flag| flag / 8 == index)
            .fold(0_u8, |mask, flag| mask | (1 << (flag % 8)));
        if (before ^ after) & !allowed != 0 {
            return Err(Phase2Error::Conflict);
        }
    }
    let before_vars = source
        .save_block1_range(VARS_OFFSET, VAR_COUNT * 2)
        .ok_or(Phase2Error::Conflict)?;
    let after_vars = incoming
        .save_block1_range(VARS_OFFSET, VAR_COUNT * 2)
        .ok_or(Phase2Error::Conflict)?;
    for index in 0..VAR_COUNT {
        if index == 0x40c3 - 0x4000 {
            continue;
        }
        let bytes = index * 2..index * 2 + 2;
        if before_vars[bytes.clone()] != after_vars[bytes] {
            return Err(Phase2Error::Conflict);
        }
    }
    Ok(())
}

/// Atomically consumes the caller's Wally progression grant as part of the
/// snapshot finalize transaction. The caller supplies the immutable current
/// and incoming validated saves; this function rechecks all repository-side
/// bindings before marking the member applied.
pub(crate) fn apply_commit_grant(
    state: &mut super::storage::State,
    actor: AuthenticatedActor,
    request: &SnapshotFinalizeRequest,
    source_save: &coop_save::ValidatedSaveV2,
    incoming_save: &coop_save::ValidatedSaveV2,
    now: u64,
) -> Result<bool, Phase2Error> {
    let source_wally = source_save.wally_victory_road_evidence();
    let incoming_wally = incoming_save.wally_victory_road_evidence();
    if (source_wally.canonical_trainer_defeated && !incoming_wally.canonical_trainer_defeated)
        || (source_wally.defeated_wally_flag && !incoming_wally.defeated_wally_flag)
        || (matches!(source_wally.victory_road_1f_state, 1 | 2)
            && !matches!(incoming_wally.victory_road_1f_state, 1 | 2))
    {
        return Err(Phase2Error::Conflict);
    }
    let wally_added =
        !source_wally.canonical_trainer_defeated && incoming_wally.canonical_trainer_defeated;
    let wally_story_advanced = (!source_wally.defeated_wally_flag
        && incoming_wally.defeated_wally_flag)
        || (!matches!(source_wally.victory_road_1f_state, 1 | 2)
            && matches!(incoming_wally.victory_road_1f_state, 1 | 2))
        || (source_wally.entrance_wally_hidden && !incoming_wally.entrance_wally_hidden)
        || wally_added;
    let Some(commit_id) = request.last_applied_commit else {
        return if wally_story_advanced {
            Err(Phase2Error::Forbidden)
        } else {
            Ok(false)
        };
    };
    if !wally_story_advanced || !wally_added {
        return Err(Phase2Error::Conflict);
    }
    if !incoming_wally.is_post_battle() {
        return Err(Phase2Error::Conflict);
    }
    validate_wally_csp_delta(source_save, incoming_save)?;
    validate_wally_story_delta(source_save, incoming_save)?;
    let wally = wally_trainer_id()?;

    let mut found = None;
    for (battle_id, record) in &state.battle_reservations {
        let Some(grants) = &record.commit_grants else {
            continue;
        };
        for (index, grant) in grants.iter().enumerate() {
            if grant.grant_id == commit_id {
                if found.is_some()
                    || grant.character_id != actor.character_id
                    || grant.trainer_id != wally
                {
                    return Err(Phase2Error::Conflict);
                }
                found = Some((*battle_id, index));
            }
        }
    }
    let (battle_id, index) = found.ok_or(Phase2Error::Forbidden)?;
    let record = state
        .battle_reservations
        .get(&battle_id)
        .ok_or(Phase2Error::Conflict)?;
    if record.view.status != BattleReservationStatus::CommitPending
        || record.expires_at <= now
        || record.view.trainer_id.as_ref() != Some(&wally)
        || record.commit_grant_applied[index]
        || record.view.member_character_ids[index] != actor.character_id
    {
        return Err(Phase2Error::Conflict);
    }
    let group = state
        .groups
        .get(&record.view.group_id)
        .ok_or(Phase2Error::NotFound)?;
    if group.status != super::storage::GroupStatus::Active
        || state
            .active_group_by_member
            .get(&record.view.member_character_ids[0])
            != Some(&record.view.group_id)
        || state
            .active_group_by_member
            .get(&record.view.member_character_ids[1])
            != Some(&record.view.group_id)
    {
        return Err(Phase2Error::Conflict);
    }
    let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
    let anchor = &anchors[index];
    for (member_index, member) in record.view.member_character_ids.iter().enumerate() {
        if !record.commit_grant_applied[member_index]
            && !anchor_is_current(state, *member, &anchors[member_index])
        {
            return Err(Phase2Error::Conflict);
        }
    }
    let character = state
        .characters
        .get(&actor.character_id)
        .ok_or(Phase2Error::NotFound)?;
    if character.active_snapshot != Some(anchor.snapshot_id)
        || character.revision != anchor.revision
        || request.expected_parent_revision != anchor.revision
        || !anchor_is_current(state, actor.character_id, anchor)
        || party_digest(source_save)? != anchor.digest
    {
        return Err(Phase2Error::Conflict);
    }
    let grant = record
        .commit_grants
        .as_ref()
        .and_then(|grants| grants.get(index))
        .ok_or(Phase2Error::Conflict)?;
    if grant.source_snapshot_id != anchor.snapshot_id
        || grant.source_revision != anchor.revision
        || grant.source_party_digest != anchor.digest
    {
        return Err(Phase2Error::Conflict);
    }
    super::ledger::check_declarable(state, actor.character_id, commit_id)?;
    let retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?;
    let members = record.view.member_character_ids;
    let complete = {
        let record = state
            .battle_reservations
            .get_mut(&battle_id)
            .ok_or(Phase2Error::Conflict)?;
        record.commit_grant_applied[index] = true;
        let complete = record.commit_grant_applied == [true, true];
        if complete {
            record.view.status = BattleReservationStatus::Completed;
            record.retain_until = retain_until;
        }
        complete
    };
    super::ledger::mark_applied(state, commit_id, request.snapshot_id);
    if complete {
        release_battle_locks(state, battle_id, members);
    }
    Ok(true)
}

/// Whether a battle's commit grants can still be applied.
pub(crate) fn grant_is_live(state: &super::storage::State, battle_id: Uuid, now: u64) -> bool {
    state
        .battle_reservations
        .get(&battle_id)
        .is_some_and(|record| {
            record.view.status == BattleReservationStatus::CommitPending && record.expires_at > now
        })
}

pub(crate) fn reserve(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
    request: &BattleReservationRequest,
) -> Result<BattleReservationView, Phase2Error> {
    request.validate()?;
    let now = store.now();
    let request_fingerprint = if let Some(trainer_id) = &request.trainer_id {
        fingerprint(OP_RESERVE, &(group_id, request.kind, trainer_id))?
    } else if let Some(rules) = request.friendly_rules {
        fingerprint(OP_RESERVE, &(group_id, request.kind, rules))?
    } else {
        fingerprint(OP_RESERVE, &(group_id, request.kind))?
    };
    let encounter = request
        .trainer_id
        .as_ref()
        .map(trainer_encounter)
        .transpose()?;
    // A recorded retry only needs the caller's current authority over the
    // group. The finalized snapshots may have advanced since this response
    // was first returned, so consult the receipt before reading party data.
    if let Some(view) = store.read_transaction(|state| {
        group_and_current_members(state, actor, group_id, fence, now)?;
        match state
            .battle_idempotency
            .get(&(actor.character_id, request.idempotency_key))
            .filter(|record| record.expires_at > now)
        {
            Some(record)
                if record.operation == BattleOperation::Reserve
                    && record.fingerprint == request_fingerprint =>
            {
                Ok(Some(record.response.view.clone()))
            }
            Some(_) => Err(Phase2Error::Conflict),
            None => Ok(None),
        }
    })? {
        return Ok(view);
    }
    // Object-store reads are outside the repository write transaction. Its
    // callback does not roll back on Err, so all preconditions are rechecked
    // before the first reservation mutation below.
    let (expected_members, snapshots) = store.read_transaction(|state| {
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        if let Some(encounter) = &encounter {
            trainer_member_zones(state, members, encounter)?;
        }
        Ok::<_, Phase2Error>((
            members,
            members
                .map(|member| snapshot_for_party(state, member))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?,
        ))
    })?;
    let trainer = request.trainer_id.as_ref().zip(encounter.as_ref());
    let anchored: [(PartyAnchor, MemberStanding); 2] = expected_members
        .into_iter()
        .zip(snapshots.iter())
        .map(|(member, snapshot)| party_anchor(store, member, snapshot, trainer))
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| Phase2Error::Internal)?;
    let [
        (first_anchor, first_standing),
        (second_anchor, second_standing),
    ] = anchored;
    let standings = [first_standing, second_standing];
    let anchors = [first_anchor, second_anchor];
    let (reward_mode, member_roles) = match &encounter {
        // A friendly challenge stages its teams and rewards no one.
        None if request.friendly_rules.is_some() => (BattleRewardMode::None, None),
        None => (BattleRewardMode::Ledger, None),
        Some(encounter) if encounter.reward_mode == BattleRewardMode::Ledger => {
            // The progression ledger grants the trainer to both members, so
            // neither may have beaten it already.
            if standings.iter().any(|standing| standing.defeated) {
                return Err(Phase2Error::Conflict);
            }
            (BattleRewardMode::Ledger, None)
        }
        Some(encounter) => {
            let requester = expected_members
                .iter()
                .position(|member| *member == actor.character_id)
                .ok_or(Phase2Error::Forbidden)?;
            if !encounter.rule.requester_allowed(standings[requester]) {
                return Err(Phase2Error::Conflict);
            }
            (
                BattleRewardMode::Local,
                Some(std::array::from_fn(|index| {
                    if index == requester || encounter.rule.participates(standings[index]) {
                        BattleMemberRole::Participant
                    } else {
                        BattleMemberRole::Helper
                    }
                })),
            )
        }
    };
    store.write_transaction(|state| {
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        prune_locked(state, now);
        if let Some(record) = state
            .battle_idempotency
            .get(&(actor.character_id, request.idempotency_key))
        {
            if record.operation == BattleOperation::Reserve
                && record.fingerprint == request_fingerprint
            {
                return Ok(record.response.view.clone());
            }
            return Err(Phase2Error::Conflict);
        }
        if let Some(encounter) = &encounter {
            trainer_member_zones(state, members, encounter)?;
        }
        if members != expected_members
            || members
                .iter()
                .enumerate()
                .any(|(index, member)| !anchor_is_current(state, *member, &anchors[index]))
        {
            return Err(Phase2Error::Conflict);
        }
        let evict_id = if state.battle_reservations.len() >= MAX_BATTLE_RESERVATIONS {
            Some(
                state
                    .battle_reservations
                    .iter()
                    .filter(|(_, record)| {
                        !matches!(
                            record.view.status,
                            BattleReservationStatus::Pending
                                | BattleReservationStatus::Accepted
                                | BattleReservationStatus::CommitPending
                        )
                    })
                    .min_by_key(|(_, record)| record.retain_until)
                    .map(|(id, _)| *id)
                    .ok_or(Phase2Error::Busy)?,
            )
        } else {
            None
        };
        if members
            .iter()
            .any(|member| state.active_battle_by_member.contains_key(member))
        {
            return Err(Phase2Error::Conflict);
        }
        // A Ledger-mode win issues one ledger entry per member, and a member
        // may hold only one open entry.
        if request.kind == BattleReservationKind::CooperativeTrainer
            && reward_mode == BattleRewardMode::Ledger
            && members
                .iter()
                .any(|member| super::ledger::has_open_entry(state, *member))
        {
            return Err(Phase2Error::Conflict);
        }
        check_idempotency_capacity(state, actor.character_id)?;
        let battle_id = store.random_uuid().map_err(Phase2Error::from)?;
        let expires_at = add_ms(now, BATTLE_RESERVATION_TTL_MS)?;
        let retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?;
        let view = reserve_view(
            battle_id,
            group_id,
            members,
            actor.character_id,
            request.kind,
            request.trainer_id.clone(),
            BattleReservationStatus::Pending,
            expires_at,
            reward_mode,
            member_roles,
            request.friendly_rules,
        )?;
        if let Some(id) = evict_id {
            state.battle_reservations.remove(&id);
        }
        state.battle_reservations.insert(
            battle_id,
            BattleReservationRecord {
                view: view.clone(),
                expires_at,
                retain_until,
                party_anchors: Some(anchors),
                consensus: BattleConsensusState::default(),
                commit_grants: None,
                commit_grant_applied: [false, false],
                staged_parties: [None, None],
            },
        );
        for member in members {
            state.active_battle_by_member.insert(member, battle_id);
        }
        state.battle_idempotency.insert(
            (actor.character_id, request.idempotency_key),
            BattleIdempotencyRecord {
                operation: BattleOperation::Reserve,
                fingerprint: request_fingerprint,
                response: BattleIdempotencyResponse { view: view.clone() },
                expires_at: retain_until,
            },
        );
        Ok(view)
    })
}

/// Finds the one live reservation visible to either current group member.
/// Polling never extends its deadline or exposes the server's party anchors.
pub(crate) fn current(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<BattleReservationView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        prune_locked(state, now);
        let battle_id = state
            .active_battle_by_member
            .get(&actor.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if members
            .iter()
            .any(|member| state.active_battle_by_member.get(member) != Some(battle_id))
        {
            return Err(Phase2Error::NotFound);
        }
        let record = state
            .battle_reservations
            .get(battle_id)
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id
            || record.view.member_character_ids != members
            || record.expires_at <= now
            || !matches!(
                record.view.status,
                BattleReservationStatus::Pending
                    | BattleReservationStatus::Accepted
                    | BattleReservationStatus::CommitPending
            )
        {
            return Err(Phase2Error::NotFound);
        }
        Ok(record.view.clone())
    })
}

fn action(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleReservationActionRequest,
    operation: BattleOperation,
) -> Result<BattleReservationView, Phase2Error> {
    request.validate()?;
    let operation_name = match operation {
        BattleOperation::Accept => OP_ACCEPT,
        BattleOperation::Decline => OP_DECLINE,
        BattleOperation::Cancel => OP_CANCEL,
        BattleOperation::Reserve | BattleOperation::Finish => {
            return Err(Phase2Error::InvalidRequest);
        }
    };
    let now = store.now();
    let request_fingerprint = fingerprint(operation_name, &(group_id, battle_id))?;
    store.write_transaction(|state| {
        prune_locked(state, now);
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        if let Some(record) = state
            .battle_idempotency
            .get(&(actor.character_id, request.idempotency_key))
        {
            if record.operation == operation && record.fingerprint == request_fingerprint {
                return Ok(record.response.view.clone());
            }
            return Err(Phase2Error::Conflict);
        }
        let record = state
            .battle_reservations
            .get(&battle_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || record.view.member_character_ids != members {
            return Err(Phase2Error::NotFound);
        }
        if record.expires_at <= now {
            return Err(Phase2Error::Expired);
        }
        let is_initiator = actor.character_id == record.view.initiator_character_id;
        match operation {
            BattleOperation::Accept if is_initiator => return Err(Phase2Error::Forbidden),
            BattleOperation::Decline if is_initiator => return Err(Phase2Error::Forbidden),
            _ => {}
        }
        let next_status = match operation {
            BattleOperation::Accept if record.view.status == BattleReservationStatus::Pending => {
                BattleReservationStatus::Accepted
            }
            BattleOperation::Decline
                if matches!(
                    record.view.status,
                    BattleReservationStatus::Pending | BattleReservationStatus::Accepted
                ) =>
            {
                BattleReservationStatus::Declined
            }
            BattleOperation::Cancel
                if matches!(
                    record.view.status,
                    BattleReservationStatus::Pending
                        | BattleReservationStatus::Accepted
                        | BattleReservationStatus::CommitPending
                ) =>
            {
                BattleReservationStatus::Cancelled
            }
            BattleOperation::Accept => return Err(Phase2Error::Conflict),
            BattleOperation::Decline | BattleOperation::Cancel => {
                return Err(Phase2Error::Conflict);
            }
            BattleOperation::Reserve | BattleOperation::Finish => {
                return Err(Phase2Error::InvalidRequest);
            }
        };
        check_idempotency_capacity(state, actor.character_id)?;
        let retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?;
        let commit_pending = record.view.status == BattleReservationStatus::CommitPending;
        let mut view = record.view;
        view.status = next_status;
        let expires_at = if next_status == BattleReservationStatus::Accepted {
            let deadline = add_ms(now, BATTLE_ACCEPTED_IDLE_TTL_MS)?;
            view.expires_at = Store::unix_timestamp(deadline).map_err(Phase2Error::from)?;
            deadline
        } else {
            record.expires_at
        };
        if commit_pending {
            super::ledger::void_battle_entries(state, battle_id);
        }
        if !matches!(
            next_status,
            BattleReservationStatus::Pending | BattleReservationStatus::Accepted
        ) {
            for member in members {
                if state.active_battle_by_member.get(&member) == Some(&battle_id) {
                    state.active_battle_by_member.remove(&member);
                }
            }
        }
        state.battle_reservations.insert(
            battle_id,
            BattleReservationRecord {
                view: view.clone(),
                expires_at,
                retain_until,
                party_anchors: record.party_anchors,
                consensus: record.consensus,
                commit_grants: record.commit_grants,
                commit_grant_applied: record.commit_grant_applied,
                staged_parties: record.staged_parties,
            },
        );
        state.battle_idempotency.insert(
            (actor.character_id, request.idempotency_key),
            BattleIdempotencyRecord {
                operation,
                fingerprint: request_fingerprint,
                response: BattleIdempotencyResponse { view: view.clone() },
                expires_at: retain_until,
            },
        );
        Ok(view)
    })
}

pub(crate) fn accept(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleReservationActionRequest,
) -> Result<BattleReservationView, Phase2Error> {
    action(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        request,
        BattleOperation::Accept,
    )
}

pub(crate) fn decline(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleReservationActionRequest,
) -> Result<BattleReservationView, Phase2Error> {
    action(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        request,
        BattleOperation::Decline,
    )
}

pub(crate) fn cancel(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleReservationActionRequest,
) -> Result<BattleReservationView, Phase2Error> {
    action(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        request,
        BattleOperation::Cancel,
    )
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn consensus_view(record: &BattleReservationRecord) -> BattleConsensusView {
    let mut turns = record.consensus.turns.clone();
    for turn in &mut turns {
        if turn.actions.iter().any(Option::is_none) {
            turn.actions = [None, None];
            turn.action_keys = [None, None];
        }
        if turn.state_hashes.iter().any(Option::is_none) {
            turn.state_hashes = [None, None];
            turn.hash_keys = [None, None];
        }
    }
    let ready = record
        .consensus
        .ready
        .clone()
        .map(|receipt| receipt.is_some());
    BattleConsensusView {
        reservation: record.view.clone(),
        commitments: record
            .consensus
            .commitments
            .clone()
            .map(|item| item.map(|c| c.snapshot_hash)),
        manifest: record.consensus.manifest.clone(),
        start_released: ready == [true, true],
        ready,
        turns,
    }
}

/// Polling does not extend the deadline. Both participants can learn the
/// manifest and each completed pair of intents or hashes without a write.
pub(crate) fn inspect_consensus(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
) -> Result<BattleConsensusView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        prune_locked(state, now);
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        let record = state
            .battle_reservations
            .get(&battle_id)
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || record.view.member_character_ids != members {
            return Err(Phase2Error::NotFound);
        }
        Ok(consensus_view(record))
    })
}

/// Where the peer's party records come from: the anchored finalized save in
/// `Ledger` mode, or the records the peer staged in `Local` mode.
enum PeerPartySource {
    Anchored(SnapshotRecord),
    Staged(Vec<String>),
}

fn peer_party_source(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    now: u64,
) -> Result<(CharacterId, PartyAnchor, PeerPartySource, String), Phase2Error> {
    let members = group_and_current_members(state, actor, group_id, fence, now)?;
    let record = state
        .battle_reservations
        .get(&battle_id)
        .ok_or(Phase2Error::NotFound)?;
    if record.view.group_id != group_id || record.view.member_character_ids != members {
        return Err(Phase2Error::NotFound);
    }
    if record.view.status != BattleReservationStatus::Accepted
        || record.expires_at <= now
        || members
            .iter()
            .any(|member| state.active_battle_by_member.get(member) != Some(&battle_id))
    {
        return Err(Phase2Error::Conflict);
    }
    let actor_index = members
        .iter()
        .position(|member| *member == actor.character_id)
        .ok_or(Phase2Error::NotFound)?;
    let peer_index = 1 - actor_index;
    let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
    let manifest = record
        .consensus
        .manifest
        .as_ref()
        .ok_or(Phase2Error::Conflict)?;
    if manifest.battle_id != battle_id
        || manifest.member_character_ids != members
        || members.iter().enumerate().any(|(index, member)| {
            let Some(expected) = expected_party_digest(record, index) else {
                return true;
            };
            !anchor_is_current(state, *member, &anchors[index])
                || manifest.snapshot_hashes[index] != expected
                || record.consensus.commitments[index]
                    .as_ref()
                    .is_none_or(|commitment| commitment.snapshot_hash != expected)
        })
    {
        return Err(Phase2Error::Conflict);
    }
    let peer = members[peer_index];
    let source = match record.view.reward_mode {
        BattleRewardMode::Local | BattleRewardMode::None => PeerPartySource::Staged(
            record.staged_parties[peer_index]
                .as_ref()
                .ok_or(Phase2Error::Conflict)?
                .records
                .clone(),
        ),
        BattleRewardMode::Ledger => {
            let snapshot = snapshot_for_party(state, peer)?;
            if snapshot.snapshot_id != anchors[peer_index].snapshot_id
                || snapshot.revision != anchors[peer_index].revision
            {
                return Err(Phase2Error::Conflict);
            }
            PeerPartySource::Anchored(snapshot)
        }
    };
    Ok((
        peer,
        anchors[peer_index].clone(),
        source,
        manifest.snapshot_hashes[peer_index].clone(),
    ))
}

/// Returns only the other participant's canonical party after both snapshot
/// commitments form a manifest. No state or reservation deadline is changed.
pub(crate) fn peer_party(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
) -> Result<BattlePeerPartyView, Phase2Error> {
    let now = store.now();
    let (peer, anchor, source, manifest_hash) = store.read_transaction(|state| {
        peer_party_source(state, actor, group_id, battle_id, fence, now)
    })?;
    let snapshot = match source {
        // Local-reward records were validated when staged and are already
        // bound to the manifest digest; no object read is needed.
        PeerPartySource::Staged(party_records) => {
            return Ok(BattlePeerPartyView {
                api_version: ApiVersion::V1,
                battle_id,
                peer_character_id: peer,
                snapshot_revision: anchor.revision,
                snapshot_hash: manifest_hash,
                party_records,
            });
        }
        PeerPartySource::Anchored(snapshot) => snapshot,
    };
    let sav = snapshot
        .files
        .iter()
        .find(|file| file.artifact == ArtifactIdentity::CharacterSav)
        .ok_or(Phase2Error::Conflict)?;
    let key = Store::object_key(peer, anchor.snapshot_id, ArtifactIdentity::CharacterSav);
    let bytes = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
    sav.verify_bytes(&bytes)
        .map_err(|_| Phase2Error::Conflict)?;
    let save = super::saves::validate_character_sav(&bytes, anchor.revision)
        .map_err(|_| Phase2Error::Conflict)?;
    if !save.coop().online_eligible() || party_digest(&save)? != manifest_hash {
        return Err(Phase2Error::Conflict);
    }
    let party_records = party_records(&save)?
        .iter()
        .map(|record| hex_string(record))
        .collect();
    // Recheck after the object read so a concurrent revision or battle
    // transition cannot turn an old snapshot into a successful response.
    store.read_transaction(|state| {
        let (current_peer, current_anchor, _, current_hash) =
            peer_party_source(state, actor, group_id, battle_id, fence, store.now())?;
        if current_peer != peer || current_anchor != anchor || current_hash != manifest_hash {
            return Err(Phase2Error::Conflict);
        }
        Ok::<_, Phase2Error>(())
    })?;
    Ok(BattlePeerPartyView {
        api_version: ApiVersion::V1,
        battle_id,
        peer_character_id: peer,
        snapshot_revision: anchor.revision,
        snapshot_hash: manifest_hash,
        party_records,
    })
}

fn consensus_transition(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    allow_diverged_replay: bool,
    transition: impl FnOnce(
        &mut BattleReservationRecord,
        usize,
        &Store,
        &super::storage::State,
    ) -> Result<bool, Phase2Error>,
) -> Result<BattleConsensusView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        prune_locked(state, now);
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        let index = members
            .iter()
            .position(|member| *member == actor.character_id)
            .ok_or(Phase2Error::Forbidden)?;
        // The repository callback does not roll back on Err. Stage the whole
        // turn transition until random seed generation and deadline checks pass.
        let mut record = state
            .battle_reservations
            .get(&battle_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || record.view.member_character_ids != members {
            return Err(Phase2Error::NotFound);
        }
        if record.view.status != BattleReservationStatus::Accepted
            && !(allow_diverged_replay && record.view.status == BattleReservationStatus::Diverged)
        {
            return Err(Phase2Error::Conflict);
        }
        let changed = transition(&mut record, index, store, state)?;
        let diverged = record.view.status == BattleReservationStatus::Diverged;
        if diverged && changed {
            record.retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?;
        } else if changed {
            record.expires_at = add_ms(now, BATTLE_ACCEPTED_IDLE_TTL_MS)?;
            record.view.expires_at =
                Store::unix_timestamp(record.expires_at).map_err(Phase2Error::from)?;
        }
        let view = consensus_view(&record);
        if changed {
            state.battle_reservations.insert(battle_id, record);
        }
        if diverged {
            for member in members {
                if state.active_battle_by_member.get(&member) == Some(&battle_id) {
                    state.active_battle_by_member.remove(&member);
                }
            }
        }
        Ok(view)
    })
}

pub(crate) fn commit_snapshot(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleSnapshotCommitRequest,
) -> Result<BattleConsensusView, Phase2Error> {
    if request.api_version != ApiVersion::V1 || !valid_hash(&request.snapshot_hash) {
        return Err(Phase2Error::InvalidRequest);
    }
    let staged = request
        .party_records
        .as_deref()
        .map(staged_party)
        .transpose()?;
    if staged
        .as_ref()
        .is_some_and(|(party, _)| party.digest != request.snapshot_hash)
    {
        return Err(Phase2Error::InvalidRequest);
    }
    consensus_transition(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        false,
        |record, index, store, state| {
            let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
            // The anchor still fences ownership, lease, and revision in both
            // modes. Only Ledger mode binds the party to the finalized save.
            let expected = match record.view.reward_mode {
                BattleRewardMode::Ledger if staged.is_some() => {
                    return Err(Phase2Error::InvalidRequest);
                }
                BattleRewardMode::Ledger => anchors[index].digest.clone(),
                BattleRewardMode::Local | BattleRewardMode::None => {
                    let (party, usable) = staged.as_ref().ok_or(Phase2Error::InvalidRequest)?;
                    if !staged_party_allowed(&record.view, party, *usable) {
                        return Err(Phase2Error::InvalidRequest);
                    }
                    party.digest.clone()
                }
            };
            if record
                .view
                .member_character_ids
                .iter()
                .enumerate()
                .any(|(i, member)| !anchor_is_current(state, *member, &anchors[i]))
                || request.snapshot_hash != expected
            {
                return Err(Phase2Error::Conflict);
            }
            let slot = &mut record.consensus.commitments[index];
            if let Some(existing) = slot {
                if existing.snapshot_hash != request.snapshot_hash
                    || existing.idempotency_key != request.idempotency_key
                {
                    return Err(Phase2Error::Conflict);
                }
                return Ok(false);
            }
            if record.consensus.manifest.is_some() {
                return Err(Phase2Error::Conflict);
            }
            *slot = Some(Commitment {
                snapshot_hash: request.snapshot_hash.clone(),
                idempotency_key: request.idempotency_key,
            });
            if record.view.reward_mode.stages_records() {
                record.staged_parties[index] = staged.as_ref().map(|(party, _)| party.clone());
            }
            if let [Some(a), Some(b)] = &record.consensus.commitments {
                let nonce = store.random_uuid().map_err(Phase2Error::from)?;
                let seed: [u8; 32] = Sha256::digest(
                    serde_json::to_vec(&(
                        b"battle-seed-v1",
                        record.view.battle_id,
                        nonce,
                        &a.snapshot_hash,
                        &b.snapshot_hash,
                    ))
                    .map_err(|_| Phase2Error::Internal)?,
                )
                .into();
                record.consensus.manifest = Some(BattleManifest {
                    battle_id: record.view.battle_id,
                    member_character_ids: record.view.member_character_ids,
                    snapshot_hashes: [a.snapshot_hash.clone(), b.snapshot_hash.clone()],
                    seed: hex_string(&seed),
                });
            }
            Ok(true)
        },
    )
}

/// Records that one member has loaded the complete manifest and verified its
/// own live party against the server's anchored snapshot. The receipt is
/// accepted only while both commitments and the manifest remain current.
pub(crate) fn ready(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleReadyRequest,
) -> Result<BattleConsensusView, Phase2Error> {
    if request.api_version != ApiVersion::V1 || !valid_hash(&request.snapshot_hash) {
        return Err(Phase2Error::InvalidRequest);
    }
    consensus_transition(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        false,
        |record, index, _, state| {
            let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
            let manifest = record
                .consensus
                .manifest
                .as_ref()
                .ok_or(Phase2Error::Conflict)?;
            let expected = [0, 1]
                .map(|member_index| expected_party_digest(record, member_index).map(str::to_owned));
            let [Some(first), Some(second)] = expected else {
                return Err(Phase2Error::Conflict);
            };
            let expected = [first, second];
            if manifest.battle_id != battle_id
                || manifest.member_character_ids != record.view.member_character_ids
                || record.consensus.commitments.iter().enumerate().any(
                    |(member_index, commitment)| {
                        commitment.as_ref().is_none_or(|commitment| {
                            commitment.snapshot_hash != expected[member_index]
                                || manifest.snapshot_hashes[member_index] != expected[member_index]
                        })
                    },
                )
                || record.view.member_character_ids.iter().enumerate().any(
                    |(member_index, member)| {
                        !anchor_is_current(state, *member, &anchors[member_index])
                    },
                )
                || request.snapshot_hash != expected[index]
            {
                return Err(Phase2Error::Conflict);
            }
            let receipt = &mut record.consensus.ready[index];
            if let Some(existing) = receipt {
                if existing.snapshot_hash == request.snapshot_hash
                    && existing.idempotency_key == request.idempotency_key
                {
                    return Ok(false);
                }
                return Err(Phase2Error::Conflict);
            }
            *receipt = Some(ReadyReceipt {
                snapshot_hash: request.snapshot_hash.clone(),
                idempotency_key: request.idempotency_key,
            });
            Ok(true)
        },
    )
}

pub(super) fn hex_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}

pub(crate) fn submit_action(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleActionIntentRequest,
) -> Result<BattleConsensusView, Phase2Error> {
    if request.api_version != ApiVersion::V1
        || request.turn == 0
        || request.turn as usize > MAX_BATTLE_TURNS
        || request.action.is_empty()
        || request.action.len() > MAX_BATTLE_ACTION_BYTES
    {
        return Err(Phase2Error::InvalidRequest);
    }
    consensus_transition(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        false,
        |record, index, _, _| {
            if record.consensus.manifest.is_none() {
                return Err(Phase2Error::Conflict);
            }
            if record.consensus.ready.iter().any(Option::is_none) {
                return Err(Phase2Error::Conflict);
            }
            if record
                .consensus
                .finish_attestations
                .iter()
                .any(Option::is_some)
            {
                return Err(Phase2Error::Conflict);
            }
            let turns = &mut record.consensus.turns;
            let number = request.turn as usize;
            if number > turns.len() + 1 {
                return Err(Phase2Error::Conflict);
            }
            if number == turns.len() + 1 {
                if turns
                    .last()
                    .is_some_and(|turn| turn.state_hashes.iter().any(Option::is_none))
                {
                    return Err(Phase2Error::Conflict);
                }
                turns.push(BattleTurn {
                    number: request.turn,
                    actions: [None, None],
                    state_hashes: [None, None],
                    action_keys: [None, None],
                    hash_keys: [None, None],
                });
            }
            let is_latest = number == turns.len();
            let turn = &mut turns[number - 1];
            match &turn.actions[index] {
                Some(existing)
                    if existing == &request.action
                        && turn.action_keys[index] == Some(request.idempotency_key) =>
                {
                    Ok(false)
                }
                Some(_) => Err(Phase2Error::Conflict),
                None if !is_latest => Err(Phase2Error::Conflict),
                None => {
                    turn.actions[index] = Some(request.action.clone());
                    turn.action_keys[index] = Some(request.idempotency_key);
                    Ok(true)
                }
            }
        },
    )
}

pub(crate) fn acknowledge_hash(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleStateHashRequest,
) -> Result<BattleConsensusView, Phase2Error> {
    if request.api_version != ApiVersion::V1
        || request.turn == 0
        || request.turn as usize > MAX_BATTLE_TURNS
        || !valid_hash(&request.state_hash)
    {
        return Err(Phase2Error::InvalidRequest);
    }
    consensus_transition(
        store,
        actor,
        group_id,
        battle_id,
        fence,
        true,
        |record, index, _, _| {
            if record
                .consensus
                .finish_attestations
                .iter()
                .any(Option::is_some)
            {
                return Err(Phase2Error::Conflict);
            }
            let turns = &mut record.consensus.turns;
            let number = request.turn as usize;
            if number == 0 || number > turns.len() {
                return Err(Phase2Error::Conflict);
            }
            let is_latest = number == turns.len();
            let turn = &mut turns[number - 1];
            if turn.actions.iter().any(Option::is_none) {
                return Err(Phase2Error::Conflict);
            }
            match &turn.state_hashes[index] {
                Some(existing)
                    if existing == &request.state_hash
                        && turn.hash_keys[index] == Some(request.idempotency_key) =>
                {
                    return Ok(false);
                }
                Some(_) => return Err(Phase2Error::Conflict),
                None if !is_latest => return Err(Phase2Error::Conflict),
                None => {}
            }
            if record.view.status == BattleReservationStatus::Diverged {
                return Err(Phase2Error::Conflict);
            }
            turn.state_hashes[index] = Some(request.state_hash.clone());
            turn.hash_keys[index] = Some(request.idempotency_key);
            if let [Some(a), Some(b)] = &turn.state_hashes {
                if a != b {
                    record.view.status = BattleReservationStatus::Diverged;
                }
            }
            Ok(true)
        },
    )
}

fn release_battle_locks(
    state: &mut super::storage::State,
    battle_id: Uuid,
    members: [CharacterId; 2],
) {
    for member in members {
        if state.active_battle_by_member.get(&member) == Some(&battle_id) {
            state.active_battle_by_member.remove(&member);
        }
    }
}

/// Records one terminal attestation after the two members have already
/// acknowledged a paired terminal state hash. This is deliberately a bounded
/// pre-ledger receipt: it performs no save publication and no progression
/// mutation. A second member's mismatch makes the tombstone Diverged and
/// releases both reservation locks.
pub(crate) fn finish(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    battle_id: Uuid,
    fence: LeaseFence,
    request: &BattleFinishRequest,
) -> Result<BattleReservationView, Phase2Error> {
    if request.api_version != ApiVersion::V1
        || request.turn == 0
        || request.turn as usize > MAX_BATTLE_TURNS
        || !valid_hash(&request.state_hash)
    {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    let request_fingerprint = fingerprint(
        OP_FINISH,
        &(
            group_id,
            battle_id,
            request.result,
            request.turn,
            &request.state_hash,
        ),
    )?;
    store.write_transaction(|state| {
        prune_locked(state, now);
        // Fence and group membership are checked before idempotency replay so
        // a stale lease cannot replay a terminal receipt after reconnect.
        let members = group_and_current_members(state, actor, group_id, fence, now)?;
        if let Some(record) = state
            .battle_idempotency
            .get(&(actor.character_id, request.idempotency_key))
        {
            if record.operation == BattleOperation::Finish
                && record.fingerprint == request_fingerprint
            {
                return Ok(record.response.view.clone());
            }
            return Err(Phase2Error::Conflict);
        }
        let mut record = state
            .battle_reservations
            .get(&battle_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || record.view.member_character_ids != members {
            return Err(Phase2Error::NotFound);
        }
        if !matches!(
            record.view.status,
            BattleReservationStatus::Accepted | BattleReservationStatus::CommitPending
        ) || record.expires_at <= now
        {
            return Err(Phase2Error::Conflict);
        }
        let result_allowed = match record.view.kind {
            BattleReservationKind::Friendly => matches!(
                request.result,
                BattleFinishResult::Member0Won
                    | BattleFinishResult::Member1Won
                    | BattleFinishResult::Draw
            ),
            BattleReservationKind::CooperativeTrainer => matches!(
                request.result,
                BattleFinishResult::Won | BattleFinishResult::Lost | BattleFinishResult::Draw
            ),
        };
        if !result_allowed {
            return Err(Phase2Error::InvalidRequest);
        }
        let manifest = record
            .consensus
            .manifest
            .as_ref()
            .ok_or(Phase2Error::Conflict)?;
        if manifest.battle_id != battle_id || manifest.member_character_ids != members {
            return Err(Phase2Error::Conflict);
        }
        let anchors = record.party_anchors.as_ref().ok_or(Phase2Error::Conflict)?;
        if members
            .iter()
            .enumerate()
            .any(|(index, member)| !anchor_is_current(state, *member, &anchors[index]))
        {
            return Err(Phase2Error::Conflict);
        }
        let terminal = record.consensus.turns.last().ok_or(Phase2Error::Conflict)?;
        if terminal.number != request.turn
            || terminal.actions.iter().any(Option::is_none)
            || terminal.state_hashes.iter().any(Option::is_none)
            || terminal.state_hashes[0] != terminal.state_hashes[1]
            || terminal.state_hashes[0].as_deref() != Some(request.state_hash.as_str())
        {
            return Err(Phase2Error::Conflict);
        }
        let index = members
            .iter()
            .position(|member| *member == actor.character_id)
            .ok_or(Phase2Error::Forbidden)?;
        check_idempotency_capacity(state, actor.character_id)?;
        if let Some(existing) = &record.consensus.finish_attestations[index] {
            // A missing idempotency row indicates an inconsistent persisted
            // state; never accept a second key for the same member slot.
            if existing.idempotency_key != request.idempotency_key {
                return Err(Phase2Error::Conflict);
            }
            return Err(Phase2Error::Conflict);
        }
        record.consensus.finish_attestations[index] = Some(BattleFinishAttestation {
            result: request.result,
            turn: request.turn,
            state_hash: request.state_hash.clone(),
            idempotency_key: request.idempotency_key,
        });

        let mut release = false;
        let mut diverged = false;
        if let [Some(first), Some(second)] = &record.consensus.finish_attestations {
            if first.result != second.result
                || first.turn != second.turn
                || first.state_hash != second.state_hash
            {
                record.view.status = BattleReservationStatus::Diverged;
                diverged = true;
                release = true;
            } else {
                record.view.status = match record.view.kind {
                    BattleReservationKind::Friendly => BattleReservationStatus::Completed,
                    BattleReservationKind::CooperativeTrainer => {
                        if first.result == BattleFinishResult::Won
                            && record.view.reward_mode == BattleRewardMode::Ledger
                        {
                            BattleReservationStatus::CommitPending
                        } else {
                            BattleReservationStatus::Completed
                        }
                    }
                };
                // A matching Ledger-mode trainer Won stays fenced and keeps
                // both locks until the progression ledger (or expiry)
                // resolves it. Friendly battles, trainer losses/draws, and
                // Local-mode wins (each ROM applies its own vanilla rewards)
                // have no progression handoff, so their Completed tombstone
                // releases both locks immediately and issues no grant.
                release = record.view.status == BattleReservationStatus::Completed;
            }
        }

        if record.view.status == BattleReservationStatus::CommitPending
            && record.commit_grants.is_none()
        {
            let grants = issue_commit_grants(store, state, &record)?;
            record.commit_grants = Some(super::ledger::issue_battle_grants(state, grants, now)?);
        }

        if diverged {
            record.retain_until = add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?;
        } else if matches!(
            record.view.status,
            BattleReservationStatus::Accepted | BattleReservationStatus::CommitPending
        ) {
            record.expires_at = add_ms(now, BATTLE_ACCEPTED_IDLE_TTL_MS)?;
            record.view.expires_at =
                Store::unix_timestamp(record.expires_at).map_err(Phase2Error::from)?;
        }
        let view = record.view.clone();
        state.battle_reservations.insert(battle_id, record);
        if release {
            release_battle_locks(state, battle_id, members);
        }
        state.battle_idempotency.insert(
            (actor.character_id, request.idempotency_key),
            BattleIdempotencyRecord {
                operation: BattleOperation::Finish,
                fingerprint: request_fingerprint,
                response: BattleIdempotencyResponse { view: view.clone() },
                expires_at: add_ms(now, BATTLE_IDEMPOTENCY_TTL_MS)?,
            },
        );
        Ok(view)
    })
}

/// Releases battle locks after group expiry and bounds the persistent
/// reservation/idempotency maps. Called by the existing expiry watchdog.
pub(crate) fn prune_expired(store: &Store) -> Result<(), Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        prune_locked(state, now);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase2::Phase2App;
    use coop_cloud::{
        AcquireLeaseRequest, CreateGroupInvitationRequest, IdempotencyKey, InvitationCode,
        Password, RegisterRequest, SnapshotFence, SnapshotFile, SnapshotFinalizeFence, SnapshotId,
    };

    fn key(number: u128) -> IdempotencyKey {
        IdempotencyKey::new(uuid::Uuid::from_u128(number)).expect("key")
    }

    fn fixture_party_sav() -> Vec<u8> {
        let mut bytes = super::super::tests::valid_character_sav(false);
        let mut mon = [0_u8; 100];
        mon[19] = 2; // hasSpecies
        mon[85] = 0xff; // ROM MAIL_NONE
        mon[28] = 1; // checksum of decrypted species word
        mon[32] = 1; // species 1, personality and OT ID are zero
        let selected_slot = coop_save::SECTORS_PER_SLOT;
        let physical = (0..coop_save::SECTORS_PER_SLOT)
            .find(|physical| {
                let start = (selected_slot + physical) * coop_save::SECTOR_SIZE;
                u16::from_le_bytes(bytes[start + 4084..start + 4086].try_into().unwrap()) == 1
            })
            .expect("SaveBlock1 sector");
        let start = (selected_slot + physical) * coop_save::SECTOR_SIZE;
        bytes[start + 0x234] = 1;
        bytes[start + 0x238..start + 0x238 + mon.len()].copy_from_slice(&mon);
        let checksum = coop_save::sector_checksum(
            &bytes[start..start + coop_save::LOGICAL_SECTOR_DATA_SIZES[1]],
        );
        bytes[start + 4086..start + 4088].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    const TEST_SECTOR_ID_OFFSET: usize = 4_084;
    const TEST_SECTOR_CHECKSUM_OFFSET: usize = 4_086;

    fn selected_physical(bytes: &[u8], logical: usize) -> usize {
        (0..coop_save::SECTORS_PER_SLOT)
            .find(|physical| {
                let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                u16::from_le_bytes(
                    bytes[start + TEST_SECTOR_ID_OFFSET..start + TEST_SECTOR_ID_OFFSET + 2]
                        .try_into()
                        .unwrap(),
                ) == logical as u16
            })
            .expect("selected fixture slot contains logical sector")
    }

    fn rewrite_sector_checksum(bytes: &mut [u8], logical: usize) {
        let physical = selected_physical(bytes, logical);
        let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
        let checksum = coop_save::sector_checksum(
            &bytes[start..start + coop_save::LOGICAL_SECTOR_DATA_SIZES[logical]],
        );
        bytes[start + TEST_SECTOR_CHECKSUM_OFFSET..start + TEST_SECTOR_CHECKSUM_OFFSET + 2]
            .copy_from_slice(&checksum.to_le_bytes());
    }

    fn selected_save_block3(bytes: &[u8]) -> [u8; coop_save::SAVE_BLOCK3_CAPACITY] {
        let mut block = [0_u8; coop_save::SAVE_BLOCK3_CAPACITY];
        for logical in 0..coop_save::SECTORS_PER_SLOT {
            let physical = selected_physical(bytes, logical);
            let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
            let offset = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
            block[offset..offset + coop_save::SAVE_BLOCK3_CHUNK_SIZE].copy_from_slice(
                &bytes[start + coop_save::SAVE_BLOCK3_CHUNK_OFFSET
                    ..start
                        + coop_save::SAVE_BLOCK3_CHUNK_OFFSET
                        + coop_save::SAVE_BLOCK3_CHUNK_SIZE],
            );
        }
        block
    }

    fn write_selected_save_block3(bytes: &mut [u8], block: &[u8; coop_save::SAVE_BLOCK3_CAPACITY]) {
        for logical in 0..coop_save::SECTORS_PER_SLOT {
            let physical = selected_physical(bytes, logical);
            let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
            let offset = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
            bytes[start + coop_save::SAVE_BLOCK3_CHUNK_OFFSET
                ..start + coop_save::SAVE_BLOCK3_CHUNK_OFFSET + coop_save::SAVE_BLOCK3_CHUNK_SIZE]
                .copy_from_slice(&block[offset..offset + coop_save::SAVE_BLOCK3_CHUNK_SIZE]);
            rewrite_sector_checksum(bytes, logical);
        }
    }

    fn wally_story_save(canonical_delta: bool, post_battle_fields: bool) -> Vec<u8> {
        let mut bytes = fixture_party_sav();
        if post_battle_fields {
            for (flag, set) in [(0x7e_usize, true), (0x35a, false)] {
                let offset = coop_save::SAVE_BLOCK1_FLAGS_OFFSET + flag / 8;
                let logical = 1 + offset / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
                let physical = selected_physical(&bytes, logical);
                let position = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE
                    + offset % coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
                if set {
                    bytes[position] |= 1 << (flag % 8);
                } else {
                    bytes[position] &= !(1 << (flag % 8));
                }
                rewrite_sector_checksum(&mut bytes, logical);
            }
            let offset = coop_save::SAVE_BLOCK1_VARS_OFFSET + 2 * (0x40c3 - 0x4000);
            let logical = 1 + offset / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
            let physical = selected_physical(&bytes, logical);
            let position = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE
                + offset % coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
            bytes[position..position + 2].copy_from_slice(&1_u16.to_le_bytes());
            rewrite_sector_checksum(&mut bytes, logical);
        }

        if canonical_delta {
            let mut block = selected_save_block3(&bytes);
            let coop = coop_save::COOP_SAVE_OFFSET;
            block[coop + 28..coop + 32].copy_from_slice(&2_u32.to_le_bytes());
            let trainer =
                TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").expect("trainer");
            let ordinal = identity_catalog::trainer(&trainer)
                .expect("trainer registry")
                .ordinal
                .expect("trainer ordinal");
            block[coop + 68 + usize::from(ordinal / 8)] |= 1 << (ordinal % 8);
            let crc = crc32fast::hash(&block[coop..coop + 668]);
            block[coop + 668..coop + 672].copy_from_slice(&crc.to_le_bytes());
            write_selected_save_block3(&mut bytes, &block);
        }
        bytes
    }

    fn validated_fixture_save(bytes: &[u8], revision: u64) -> coop_save::ValidatedSaveV2 {
        super::super::saves::validate_character_sav(bytes, Revision::new(revision))
            .expect("validated V2 save")
    }

    fn finalize_request_for_grant(
        app: &Phase2App,
        actor: AuthenticatedActor,
        grant_id: CommitId,
        number: u128,
    ) -> SnapshotFinalizeRequest {
        let (lease, revision, files, pending) = app
            .store
            .inspect_state(|state| {
                let lease = state.leases[&actor.character_id].contract;
                let snapshot_id = state.characters[&actor.character_id]
                    .active_snapshot
                    .expect("active snapshot");
                let snapshot = &state.snapshots[&snapshot_id];
                (
                    lease,
                    state.characters[&actor.character_id].revision,
                    snapshot.files.clone(),
                    snapshot.pending_commits_sha256,
                )
            })
            .expect("source snapshot");
        SnapshotFinalizeRequest::new(
            SnapshotId::new(Uuid::new_v4()).expect("snapshot id"),
            SnapshotFinalizeFence::new(
                lease.session_id,
                actor.character_id,
                revision,
                lease.session_epoch,
                lease.client_instance_id,
                key(number),
            ),
            files,
            pending,
            Some(grant_id),
        )
        .expect("finalize request")
    }

    fn add_forged_event(bytes: &mut Vec<u8>) {
        let mut block = selected_save_block3(bytes);
        let coop = coop_save::COOP_SAVE_OFFSET;
        block[coop + 324] ^= 1;
        let crc = crc32fast::hash(&block[coop..coop + 668]);
        block[coop + 668..coop + 672].copy_from_slice(&crc.to_le_bytes());
        write_selected_save_block3(bytes, &block);
    }

    fn seed_finalized_party(app: &Phase2App, actor: AuthenticatedActor) -> String {
        seed_finalized_party_with_defeat(app, actor, false)
    }

    fn seed_finalized_party_with_defeat(
        app: &Phase2App,
        actor: AuthenticatedActor,
        defeated: bool,
    ) -> String {
        seed_finalized_party_revision(app, actor, 1, defeated)
    }

    fn seed_finalized_party_revision(
        app: &Phase2App,
        actor: AuthenticatedActor,
        revision: u64,
        defeated: bool,
    ) -> String {
        seed_finalized_party_defeating(
            app,
            actor,
            revision,
            defeated.then_some("HOENN:TRAINER_WALLY_1"),
        )
    }

    fn seed_finalized_party_defeating(
        app: &Phase2App,
        actor: AuthenticatedActor,
        revision: u64,
        defeated_trainer: Option<&str>,
    ) -> String {
        seed_finalized_progress(app, actor, revision, defeated_trainer.as_slice(), 0)
    }

    /// Sets one SaveBlock1 event flag in the selected (second) fixture slot.
    fn write_save_block1_flag(bytes: &mut [u8], flag: usize, set: bool) {
        let position = coop_save::SAVE_BLOCK1_FLAGS_OFFSET + flag / 8;
        let logical = 1 + position / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        let physical = selected_physical(bytes, logical);
        let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
        let index = start + position % coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        if set {
            bytes[index] |= 1 << (flag % 8);
        } else {
            bytes[index] &= !(1 << (flag % 8));
        }
        rewrite_sector_checksum(bytes, logical);
    }

    /// A finalized party save whose CSP1 records every trainer in
    /// `defeated_trainers` and whose flags hold exactly the Hoenn `badges`.
    fn seed_finalized_progress(
        app: &Phase2App,
        actor: AuthenticatedActor,
        revision: u64,
        defeated_trainers: &[&str],
        badges: u8,
    ) -> String {
        seed_finalized_story(app, actor, revision, defeated_trainers, badges, &[], &[])
    }

    /// Sets one SaveBlock1 script variable in the selected fixture slot.
    fn write_save_block1_var(bytes: &mut [u8], var: usize, value: u16) {
        let position = coop_save::SAVE_BLOCK1_VARS_OFFSET + 2 * (var - coop_save::VARS_START);
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            let position = position + index;
            let logical = 1 + position / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
            let physical = selected_physical(bytes, logical);
            let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
            bytes[start + position % coop_save::SAVE_BLOCK3_CHUNK_OFFSET] = byte;
            rewrite_sector_checksum(bytes, logical);
        }
    }

    /// `seed_finalized_progress` with story flags and vars written too.
    fn seed_finalized_story(
        app: &Phase2App,
        actor: AuthenticatedActor,
        revision: u64,
        defeated_trainers: &[&str],
        badges: u8,
        flags: &[(usize, bool)],
        vars: &[(usize, u16)],
    ) -> String {
        let lease = app
            .store
            .inspect_state(|state| state.leases[&actor.character_id].contract)
            .expect("lease");
        let mut bytes = fixture_party_sav();
        for badge in 0..8 {
            write_save_block1_flag(
                &mut bytes,
                coop_save::FLAG_BADGE01_GET + badge,
                badges & (1 << badge) != 0,
            );
        }
        for &(flag, set) in flags {
            write_save_block1_flag(&mut bytes, flag, set);
        }
        for &(var, value) in vars {
            write_save_block1_var(&mut bytes, var, value);
        }
        if !defeated_trainers.is_empty() || revision > 1 {
            let ordinals: Vec<u16> = defeated_trainers
                .iter()
                .map(|id| {
                    let trainer = TrainerInstanceId::parse(id).unwrap();
                    identity_catalog::trainer(&trainer)
                        .unwrap()
                        .ordinal
                        .unwrap()
                })
                .collect();
            let mut payload = [0_u8; coop_save::COOP_SAVE_V1_SIZE];
            for logical in 0..coop_save::SECTORS_PER_SLOT {
                let physical = (0..coop_save::SECTORS_PER_SLOT)
                    .find(|physical| {
                        let start =
                            (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                        u16::from_le_bytes(bytes[start + 4084..start + 4086].try_into().unwrap())
                            == logical as u16
                    })
                    .unwrap();
                let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                let chunk_start = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
                for (index, byte) in payload.iter_mut().enumerate() {
                    let source = coop_save::COOP_SAVE_OFFSET + index;
                    if source >= chunk_start
                        && source < chunk_start + coop_save::SAVE_BLOCK3_CHUNK_SIZE
                    {
                        *byte = bytes
                            [start + coop_save::SAVE_BLOCK3_CHUNK_OFFSET + source - chunk_start];
                    }
                }
            }
            payload[28..32].copy_from_slice(&(revision as u32).to_le_bytes());
            for ordinal in &ordinals {
                payload[68 + usize::from(ordinal / 8)] |= 1 << (ordinal % 8);
            }
            let crc = crc32fast::hash(&payload[..668]);
            payload[668..672].copy_from_slice(&crc.to_le_bytes());
            for logical in 0..coop_save::SECTORS_PER_SLOT {
                let physical = (0..coop_save::SECTORS_PER_SLOT)
                    .find(|physical| {
                        let start =
                            (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                        u16::from_le_bytes(bytes[start + 4084..start + 4086].try_into().unwrap())
                            == logical as u16
                    })
                    .unwrap();
                let start = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE;
                let chunk_start = logical * coop_save::SAVE_BLOCK3_CHUNK_SIZE;
                for (index, byte) in payload.iter().enumerate() {
                    let destination = coop_save::COOP_SAVE_OFFSET + index;
                    if destination >= chunk_start
                        && destination < chunk_start + coop_save::SAVE_BLOCK3_CHUNK_SIZE
                    {
                        bytes[start + coop_save::SAVE_BLOCK3_CHUNK_OFFSET + destination
                            - chunk_start] = *byte;
                    }
                }
                let checksum = coop_save::sector_checksum(
                    &bytes[start..start + coop_save::LOGICAL_SECTOR_DATA_SIZES[logical]],
                );
                bytes[start + 4086..start + 4088].copy_from_slice(&checksum.to_le_bytes());
            }
        }
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &bytes).expect("sav file");
        let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"{}")
            .expect("pending file");
        let snapshot_id = SnapshotId::new(Uuid::new_v4()).expect("snapshot id");
        let snapshot = SnapshotRecord::new(
            snapshot_id,
            coop_protocol::RomWorldId::new(1).unwrap(),
            SnapshotFence::new(lease.session_id, actor.character_id, lease.session_epoch),
            Revision::new(revision - 1),
            Revision::new(revision),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
            Store::unix_timestamp(app.store.now()).expect("time"),
        )
        .expect("snapshot");
        let key = Store::object_key(
            actor.character_id,
            snapshot_id,
            ArtifactIdentity::CharacterSav,
        );
        assert!(
            app.store
                .objects
                .put_if_absent(key, bytes.clone())
                .expect("put sav")
        );
        app.store
            .write_transaction(|state| {
                let character = state
                    .characters
                    .get_mut(&actor.character_id)
                    .expect("character");
                character.revision = Revision::new(revision);
                character.active_snapshot = Some(snapshot_id);
                state.snapshots.insert(snapshot_id, snapshot);
                state
                    .snapshot_by_revision
                    .insert((actor.character_id, Revision::new(revision)), snapshot_id);
                Ok::<_, Phase2Error>(())
            })
            .expect("finalized state");
        let save = validated_fixture_save(&bytes, revision);
        party_digest(&save).expect("party digest")
    }

    fn accepted() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        GroupId,
        LeaseFence,
        LeaseFence,
        Uuid,
    ) {
        let (app, a, b, group, fa, fb) = fixture();
        let reservation = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(100),
                },
            )
            .expect("reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(101),
            },
        )
        .expect("accept");
        (app, a, b, group, fa, fb, reservation.battle_id)
    }

    fn snapshots(
        app: &Phase2App,
        a: AuthenticatedActor,
        b: AuthenticatedActor,
        group: GroupId,
        fa: LeaseFence,
        fb: LeaseFence,
        battle: Uuid,
    ) -> BattleManifest {
        let hashes = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&battle]
                    .party_anchors
                    .as_ref()
                    .expect("anchors")
                    .clone()
                    .map(|anchor| anchor.digest)
            })
            .expect("hashes");
        let first = app
            .commit_battle_snapshot(
                a,
                group,
                battle,
                fa,
                BattleSnapshotCommitRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(102),
                    snapshot_hash: hashes[0].clone(),
                    party_records: None,
                },
            )
            .expect("first commitment");
        assert!(first.manifest.is_none());
        app.commit_battle_snapshot(
            b,
            group,
            battle,
            fb,
            BattleSnapshotCommitRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(103),
                snapshot_hash: hashes[1].clone(),
                party_records: None,
            },
        )
        .expect("second commitment")
        .manifest
        .expect("manifest")
    }

    fn release_start(
        app: &Phase2App,
        a: AuthenticatedActor,
        b: AuthenticatedActor,
        group: GroupId,
        fa: LeaseFence,
        fb: LeaseFence,
        battle: Uuid,
        manifest: &BattleManifest,
    ) -> BattleConsensusView {
        let first = app
            .ready_battle(
                a,
                group,
                battle,
                fa,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(170),
                    snapshot_hash: manifest.snapshot_hashes[0].clone(),
                },
            )
            .expect("first ready");
        assert_eq!(first.ready, [true, false]);
        app.ready_battle(
            b,
            group,
            battle,
            fb,
            BattleReadyRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(171),
                snapshot_hash: manifest.snapshot_hashes[1].clone(),
            },
        )
        .expect("second ready")
    }

    #[test]
    fn peer_party_requires_manifest_and_valid_participant_fence() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        assert_eq!(
            app.battle_peer_party(a, group, battle, fa),
            Err(Phase2Error::Conflict)
        );
        let wrong_group = GroupId::new(Uuid::new_v4()).expect("group");
        assert_eq!(
            app.battle_peer_party(a, wrong_group, battle, fa),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.battle_peer_party(a, group, Uuid::new_v4(), fa),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.battle_peer_party(a, group, battle, fb),
            Err(Phase2Error::Conflict)
        );
        let outsider = AuthenticatedActor {
            user_id: b.user_id,
            character_id: a.character_id,
        };
        assert_eq!(
            app.battle_peer_party(outsider, group, battle, fa),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn peer_party_returns_exact_records_and_manifest_peer_hash() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        let response = app
            .battle_peer_party(a, group, battle, fa)
            .expect("peer party");
        assert_eq!(response.api_version, ApiVersion::V1);
        assert_eq!(response.battle_id, battle);
        assert_eq!(response.peer_character_id, b.character_id);
        assert_eq!(response.snapshot_revision, Revision::new(1));
        assert_eq!(response.snapshot_hash, manifest.snapshot_hashes[1]);
        assert_eq!(response.party_records.len(), 1);
        assert_eq!(response.party_records[0].len(), 200);
        assert_eq!(
            response.party_records[0],
            hex_string(&fixture_party_sav_record())
        );
        let reverse = app
            .battle_peer_party(b, group, battle, fb)
            .expect("reverse party");
        assert_eq!(reverse.peer_character_id, a.character_id);
        assert_eq!(reverse.snapshot_hash, manifest.snapshot_hashes[0]);
    }

    fn fixture_party_sav_record() -> [u8; 100] {
        let mut mon = [0_u8; 100];
        mon[85] = 0xff; // exact ROM MAIL_NONE matches the authored fixture
        mon[19] = 2;
        mon[28] = 1;
        mon[32] = 1;
        mon
    }

    #[test]
    fn peer_party_rejects_stale_revision_and_terminal_battle() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        snapshots(&app, a, b, group, fa, fb, battle);
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&b.character_id)
                    .expect("peer")
                    .revision = Revision::new(2);
                Ok::<_, Phase2Error>(())
            })
            .expect("advance revision");
        assert_eq!(
            app.battle_peer_party(a, group, battle, fa),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&b.character_id)
                    .expect("peer")
                    .revision = Revision::new(1);
                Ok::<_, Phase2Error>(())
            })
            .expect("restore fixture");
        app.cancel_battle(
            a,
            group,
            battle,
            fa,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(105),
            },
        )
        .expect("cancel");
        assert_eq!(
            app.battle_peer_party(a, group, battle, fa),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn battle_ready_is_manifest_fenced_exactly_once_and_releases_start() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let anchor_hashes = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&battle]
                    .party_anchors
                    .as_ref()
                    .expect("anchors")
                    .clone()
                    .map(|anchor| anchor.digest)
            })
            .expect("anchors");
        assert_eq!(
            app.ready_battle(
                a,
                group,
                battle,
                fa,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(180),
                    snapshot_hash: anchor_hashes[0].clone(),
                },
            ),
            Err(Phase2Error::Conflict)
        );
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        assert_eq!(
            app.submit_battle_action(
                a,
                group,
                battle,
                fa,
                BattleActionIntentRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(181),
                    turn: 1,
                    action: "blocked-before-ready".into(),
                },
            ),
            Err(Phase2Error::Conflict)
        );
        let request = BattleReadyRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(182),
            snapshot_hash: manifest.snapshot_hashes[0].clone(),
        };
        let first = app
            .ready_battle(a, group, battle, fa, request.clone())
            .expect("first ready");
        assert_eq!(first.ready, [true, false]);
        assert!(!first.start_released);
        assert_eq!(
            app.ready_battle(a, group, battle, fa, request.clone()),
            Ok(first.clone())
        );
        assert_eq!(
            app.ready_battle(
                a,
                group,
                battle,
                fa,
                BattleReadyRequest {
                    idempotency_key: key(183),
                    ..request.clone()
                },
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.ready_battle(
                b,
                group,
                battle,
                fb,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(184),
                    snapshot_hash: "a".repeat(64),
                },
            ),
            Err(Phase2Error::Conflict)
        );
        let released = app
            .ready_battle(
                b,
                group,
                battle,
                fb,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(185),
                    snapshot_hash: manifest.snapshot_hashes[1].clone(),
                },
            )
            .expect("second ready");
        assert_eq!(released.ready, [true, true]);
        assert!(released.start_released);
    }

    #[test]
    fn battle_ready_rejects_stale_anchor_revision() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&a.character_id)
                    .expect("character")
                    .revision = Revision::new(2);
                Ok::<_, Phase2Error>(())
            })
            .expect("advance revision");
        assert_eq!(
            app.ready_battle(
                a,
                group,
                battle,
                fa,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(186),
                    snapshot_hash: manifest.snapshot_hashes[0].clone(),
                },
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&a.character_id)
                    .expect("character")
                    .revision = Revision::new(1);
                Ok::<_, Phase2Error>(())
            })
            .expect("restore revision");
        assert_eq!(
            app.ready_battle(
                a,
                group,
                battle,
                fa,
                BattleReadyRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(187),
                    snapshot_hash: manifest.snapshot_hashes[0].clone(),
                },
            )
            .expect("fresh anchor")
            .ready,
            [true, false]
        );
    }

    #[test]
    fn commitments_turn_order_matching_hash_and_exact_replay() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        let released = release_start(&app, a, b, group, fa, fb, battle, &manifest);
        assert!(released.start_released);
        assert_eq!(manifest.snapshot_hashes[0], manifest.snapshot_hashes[1]);
        assert_eq!(manifest.seed.len(), 64);
        assert_eq!(
            app.inspect_battle_consensus(a, group, battle, fa)
                .expect("poll")
                .manifest,
            Some(manifest)
        );
        let action_a = BattleActionIntentRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(104),
            turn: 1,
            action: "opaque-a".into(),
        };
        let action_b = BattleActionIntentRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(105),
            turn: 1,
            action: "opaque-b".into(),
        };
        let first = app
            .submit_battle_action(a, group, battle, fa, action_a.clone())
            .expect("action");
        assert_eq!(first.turns[0].actions, [None, None]);
        assert_eq!(
            app.inspect_battle_consensus(b, group, battle, fb)
                .expect("partial poll")
                .turns[0]
                .actions,
            [None, None]
        );
        let expires_at = app
            .store
            .inspect_state(|state| state.battle_reservations[&battle].expires_at)
            .expect("deadline");
        assert_eq!(
            app.submit_battle_action(a, group, battle, fa, action_a.clone())
                .expect("replay"),
            first
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.battle_reservations[&battle].expires_at)
                .expect("replay deadline"),
            expires_at
        );
        assert_eq!(
            app.submit_battle_action(
                a,
                group,
                battle,
                fa,
                BattleActionIntentRequest {
                    action: "changed".into(),
                    ..action_a
                }
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.submit_battle_action(
                b,
                group,
                battle,
                fb,
                BattleActionIntentRequest {
                    turn: 2,
                    ..action_b.clone()
                }
            ),
            Err(Phase2Error::Conflict)
        );
        let both_actions = app
            .submit_battle_action(b, group, battle, fb, action_b)
            .expect("second action");
        assert_eq!(
            both_actions.turns[0].actions,
            [Some("opaque-a".into()), Some("opaque-b".into())]
        );
        let ack_a = BattleStateHashRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(106),
            turn: 1,
            state_hash: "c".repeat(64),
        };
        let ack_b = BattleStateHashRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(107),
            turn: 1,
            state_hash: "c".repeat(64),
        };
        let partial_hash = app
            .acknowledge_battle_hash(a, group, battle, fa, ack_a)
            .expect("first hash");
        assert_eq!(partial_hash.turns[0].state_hashes, [None, None]);
        assert_eq!(
            app.inspect_battle_consensus(b, group, battle, fb)
                .expect("partial hash poll")
                .turns[0]
                .state_hashes,
            [None, None]
        );
        let agreed = app
            .acknowledge_battle_hash(b, group, battle, fb, ack_b)
            .expect("matching hash");
        assert_eq!(agreed.reservation.status, BattleReservationStatus::Accepted);
        assert_eq!(
            agreed.turns[0].state_hashes,
            [Some("c".repeat(64)), Some("c".repeat(64))]
        );
        app.submit_battle_action(
            a,
            group,
            battle,
            fa,
            BattleActionIntentRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(108),
                turn: 2,
                action: "next".into(),
            },
        )
        .expect("next turn");
    }

    #[test]
    fn mismatch_aborts_and_releases_locks() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        release_start(&app, a, b, group, fa, fb, battle, &manifest);
        for (actor, fence, n) in [(a, fa, 110), (b, fb, 111)] {
            app.submit_battle_action(
                actor,
                group,
                battle,
                fence,
                BattleActionIntentRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(n),
                    turn: 1,
                    action: "move".into(),
                },
            )
            .expect("action");
        }
        app.acknowledge_battle_hash(
            a,
            group,
            battle,
            fa,
            BattleStateHashRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(112),
                turn: 1,
                state_hash: "d".repeat(64),
            },
        )
        .expect("first hash");
        let mismatch = BattleStateHashRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(113),
            turn: 1,
            state_hash: "e".repeat(64),
        };
        let aborted = app
            .acknowledge_battle_hash(b, group, battle, fb, mismatch.clone())
            .expect("divergence receipt");
        assert_eq!(
            aborted.reservation.status,
            BattleReservationStatus::Diverged
        );
        assert_eq!(
            app.acknowledge_battle_hash(b, group, battle, fb, mismatch)
                .expect("terminal replay"),
            aborted
        );
        app.store
            .inspect_state(|state| {
                assert!(!state.active_battle_by_member.contains_key(&a.character_id));
                assert!(!state.active_battle_by_member.contains_key(&b.character_id));
            })
            .expect("released locks");
    }

    #[test]
    fn stale_lease_blocks_replay_and_expiry_cleans_up() {
        let (app, a, b, group, fa, _fb, battle) = accepted();
        let request = BattleSnapshotCommitRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(120),
            snapshot_hash: app
                .store
                .inspect_state(|state| {
                    state.battle_reservations[&battle]
                        .party_anchors
                        .as_ref()
                        .unwrap()[0]
                        .digest
                        .clone()
                })
                .expect("anchor"),
            party_records: None,
        };
        app.commit_battle_snapshot(a, group, battle, fa, request.clone())
            .expect("commit");
        app.store
            .write_transaction(|state| {
                state
                    .leases
                    .get_mut(&a.character_id)
                    .expect("lease")
                    .released = true;
                Ok::<_, Phase2Error>(())
            })
            .expect("expire lease");
        assert_eq!(
            app.commit_battle_snapshot(a, group, battle, fa, request),
            Err(Phase2Error::Expired)
        );
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&battle)
                    .expect("battle")
                    .expires_at = 0;
                Ok::<_, Phase2Error>(())
            })
            .expect("set timeout");
        prune_expired(&app.store).expect("sweep");
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.battle_reservations[&battle].view.status,
                    BattleReservationStatus::Expired
                );
                assert!(!state.active_battle_by_member.contains_key(&a.character_id));
                assert!(!state.active_battle_by_member.contains_key(&b.character_id));
            })
            .expect("timeout cleanup");
    }

    #[test]
    fn participant_abort_after_manifest_and_payload_limit() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        release_start(&app, a, b, group, fa, fb, battle, &manifest);
        assert_eq!(
            app.submit_battle_action(
                a,
                group,
                battle,
                fa,
                BattleActionIntentRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(130),
                    turn: 1,
                    action: "x".repeat(MAX_BATTLE_ACTION_BYTES + 1),
                }
            ),
            Err(Phase2Error::InvalidRequest)
        );
        let declined = app
            .decline_battle(
                b,
                group,
                battle,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(131),
                },
            )
            .expect("partner abort");
        assert_eq!(declined.status, BattleReservationStatus::Declined);
        assert_eq!(
            app.commit_battle_snapshot(
                a,
                group,
                battle,
                fa,
                BattleSnapshotCommitRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(102),
                    snapshot_hash: app
                        .store
                        .inspect_state(|state| state.battle_reservations[&battle]
                            .party_anchors
                            .as_ref()
                            .unwrap()[0]
                            .digest
                            .clone())
                        .expect("anchor"),
                    party_records: None,
                }
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .inspect_state(|state| {
                assert!(!state.active_battle_by_member.contains_key(&a.character_id));
                assert!(!state.active_battle_by_member.contains_key(&b.character_id));
            })
            .expect("abort cleanup");
    }

    #[test]
    fn terminal_tombstones_do_not_exhaust_live_reservation_capacity() {
        let (app, a, _b, group, fa, _fb) = fixture();
        let base = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(140),
                },
            )
            .expect("reserve");
        app.cancel_battle(
            a,
            group,
            base.battle_id,
            fa,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(141),
            },
        )
        .expect("cancel");
        app.store
            .write_transaction(|state| {
                let template = state.battle_reservations[&base.battle_id].clone();
                for number in 1..MAX_BATTLE_RESERVATIONS {
                    state
                        .battle_reservations
                        .insert(Uuid::from_u128(10_000 + number as u128), template.clone());
                }
                Ok::<_, Phase2Error>(())
            })
            .expect("fill terminal history");
        let next = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(142),
                },
            )
            .expect("new live reservation");
        assert_eq!(next.status, BattleReservationStatus::Pending);
        app.store
            .inspect_state(|state| {
                assert!(state.battle_reservations.len() <= MAX_BATTLE_RESERVATIONS)
            })
            .expect("bounded history");
    }

    #[test]
    fn responder_cancel_releases_both_locks_and_replays_exact_key() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let request = BattleReservationActionRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(220),
        };
        assert!(
            app.cancel_battle(a, group, battle, fb, request.clone())
                .is_err()
        );
        let cancelled = app
            .cancel_battle(b, group, battle, fb, request.clone())
            .expect("responder cancel");
        assert_eq!(cancelled.status, BattleReservationStatus::Cancelled);
        assert_eq!(
            app.cancel_battle(b, group, battle, fb, request).unwrap(),
            cancelled
        );
        assert_eq!(
            app.cancel_battle(
                b,
                group,
                Uuid::from_u128(123),
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(220),
                }
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(app.current_battle(a, group, fa), Err(Phase2Error::NotFound));
        assert_eq!(app.current_battle(b, group, fb), Err(Phase2Error::NotFound));
        assert_eq!(
            app.cancel_battle(
                a,
                group,
                battle,
                fa,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(221),
                }
            ),
            Err(Phase2Error::Conflict)
        );
    }

    fn fixture() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        coop_cloud::GroupId,
        LeaseFence,
        LeaseFence,
    ) {
        fixture_with_defeated_partner(false)
    }

    fn fixture_with_defeated_partner(
        defeated: bool,
    ) -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        coop_cloud::GroupId,
        LeaseFence,
        LeaseFence,
    ) {
        // Most tests below exercise the legacy ledger path through Wally;
        // `local_wally` switches this thread back to the production rule.
        LEGACY_WALLY_LEDGER.with(|legacy| legacy.set(true));
        let app = Phase2App::test();
        app.add_invitation("battle-a").expect("invite a");
        app.add_invitation("battle-b").expect("invite b");
        let alice = app
            .register(
                RegisterRequest::new(
                    "battlealice",
                    Password::new("password-a").expect("password"),
                    InvitationCode::new("battle-a").expect("code"),
                )
                .expect("register"),
            )
            .expect("alice");
        let bob = app
            .register(
                RegisterRequest::new(
                    "battlebob",
                    Password::new("password-b").expect("password"),
                    InvitationCode::new("battle-b").expect("code"),
                )
                .expect("register"),
            )
            .expect("bob");
        let actor_a = AuthenticatedActor {
            user_id: alice.user_id,
            character_id: alice.character_id,
        };
        let actor_b = AuthenticatedActor {
            user_id: bob.user_id,
            character_id: bob.character_id,
        };
        let lease_a = app
            .acquire(
                actor_a,
                AcquireLeaseRequest::new(
                    alice.character_id,
                    coop_cloud::ClientInstanceId::new(uuid::Uuid::from_u128(11)).expect("client"),
                    IdempotencyKey::new(uuid::Uuid::from_u128(21)).expect("key"),
                ),
            )
            .expect("lease a");
        let lease_b = app
            .acquire(
                actor_b,
                AcquireLeaseRequest::new(
                    bob.character_id,
                    coop_cloud::ClientInstanceId::new(uuid::Uuid::from_u128(12)).expect("client"),
                    IdempotencyKey::new(uuid::Uuid::from_u128(22)).expect("key"),
                ),
            )
            .expect("lease b");
        let invitation = app
            .create_group_invitation(
                actor_a,
                CreateGroupInvitationRequest::new(
                    lease_a.fence(),
                    bob.character_id,
                    IdempotencyKey::new(uuid::Uuid::from_u128(31)).expect("key"),
                ),
            )
            .expect("invitation");
        let group = app
            .accept_group_invitation(
                actor_b,
                invitation.invitation_id,
                coop_cloud::AcceptGroupInvitationRequest::new(
                    lease_b.fence(),
                    IdempotencyKey::new(uuid::Uuid::from_u128(32)).expect("key"),
                ),
            )
            .expect("group")
            .group;
        app.store
            .write_transaction(|state| {
                let zone = WorldZone::new(RegionId::Hoenn, "VICTORY_ROAD_1F", 1).unwrap();
                for member in [actor_a.character_id, actor_b.character_id] {
                    state.characters.get_mut(&member).unwrap().state.world_zone = zone.clone();
                }
                state.groups.get_mut(&group.group_id).unwrap().zone = zone;
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        seed_finalized_party(&app, actor_a);
        seed_finalized_party_with_defeat(&app, actor_b, defeated);
        (
            app,
            actor_a,
            actor_b,
            group.group_id,
            lease_a.fence(),
            lease_b.fence(),
        )
    }

    fn pending_wally() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        coop_cloud::GroupId,
        LeaseFence,
        LeaseFence,
        BattleReservationView,
        [BattleCommitGrant; 2],
    ) {
        let (app, a, b, group, fa, fb) = fixture();
        let reservation = app
            .reserve_battle(a, group, fa, trainer_request(600, "HOENN:TRAINER_WALLY_1"))
            .expect("reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(601),
            },
        )
        .expect("accept");
        let hash = terminal_hash(&app, a, b, group, fa, fb, reservation.battle_id, 610);
        app.finish_battle(
            a,
            group,
            reservation.battle_id,
            fa,
            BattleFinishRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(620),
                result: BattleFinishResult::Won,
                turn: 1,
                state_hash: hash.clone(),
            },
        )
        .expect("first finish");
        app.finish_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleFinishRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(621),
                result: BattleFinishResult::Won,
                turn: 1,
                state_hash: hash,
            },
        )
        .expect("second finish");
        let grants = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&reservation.battle_id]
                    .commit_grants
                    .clone()
            })
            .expect("grants")
            .expect("won grants");
        (app, a, b, group, fa, fb, reservation, grants)
    }

    #[test]
    fn reservation_locks_both_members_and_replays_idempotently() {
        let (app, actor_a, actor_b, group_id, fence_a, fence_b) = fixture();
        let key = IdempotencyKey::new(uuid::Uuid::from_u128(41)).expect("key");
        let request = BattleReservationRequest {
            api_version: ApiVersion::V1,
            kind: BattleReservationKind::CooperativeTrainer,
            trainer_id: Some(
                TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").expect("trainer"),
            ),
            friendly_rules: None,
            idempotency_key: key,
        };
        let first = app
            .reserve_battle(actor_a, group_id, fence_a, request.clone())
            .expect("reserve");
        let replay = app
            .reserve_battle(actor_a, group_id, fence_a, request)
            .expect("replay");
        assert_eq!(first, replay);
        assert_eq!(
            first.trainer_id,
            Some(TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").unwrap())
        );
        let conflicting = app.reserve_battle(
            actor_b,
            group_id,
            fence_b,
            BattleReservationRequest {
                api_version: ApiVersion::V1,
                kind: BattleReservationKind::Friendly,
                trainer_id: None,
                friendly_rules: None,
                idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(42)).expect("key"),
            },
        );
        assert_eq!(conflicting, Err(Phase2Error::Conflict));
    }

    fn trainer_request(number: u128, trainer_id: &str) -> BattleReservationRequest {
        BattleReservationRequest {
            api_version: ApiVersion::V1,
            kind: BattleReservationKind::CooperativeTrainer,
            trainer_id: Some(TrainerInstanceId::parse(trainer_id).expect("trainer syntax")),
            friendly_rules: None,
            idempotency_key: key(number),
        }
    }

    #[test]
    fn trainer_reservation_rejects_unknown_wrong_location_and_split_group() {
        let (app, a, b, group, fa, _) = fixture();
        assert_eq!(
            app.reserve_battle(
                a,
                group,
                fa,
                trainer_request(510, "HOENN:TRAINER_FABRICATED")
            ),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                let zone = WorldZone::new(RegionId::Hoenn, "ROUTE101", 1).unwrap();
                for member in [a.character_id, b.character_id] {
                    state.characters.get_mut(&member).unwrap().state.world_zone = zone.clone();
                }
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(511, "HOENN:TRAINER_WALLY_1")),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                let zone = WorldZone::new(RegionId::Kanto, "PEWTER_CITY_GYM", 1).unwrap();
                for member in [a.character_id, b.character_id] {
                    state.characters.get_mut(&member).unwrap().state.world_zone = zone.clone();
                }
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(516, "HOENN:TRAINER_WALLY_1")),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&a.character_id)
                    .unwrap()
                    .state
                    .world_zone = WorldZone::new(RegionId::Hoenn, "VICTORY_ROAD_1F", 1).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(512, "HOENN:TRAINER_WALLY_1")),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn trainer_reservation_rejects_defeated_partner_and_accepts_both_eligible() {
        let (app, a, _, group, fa, _) = fixture_with_defeated_partner(true);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(513, "HOENN:TRAINER_WALLY_1")),
            Err(Phase2Error::Conflict)
        );
        let (app, a, _, group, fa, _) = fixture();
        let view = app
            .reserve_battle(a, group, fa, trainer_request(514, "HOENN:TRAINER_WALLY_1"))
            .unwrap();
        assert_eq!(view.trainer_id.unwrap().as_str(), "HOENN:TRAINER_WALLY_1");
    }

    #[test]
    fn exact_retry_survives_new_defeated_snapshot_but_keeps_authorization_and_key_conflict() {
        let (app, a, b, group, fa, _) = fixture();
        let request = trainer_request(517, "HOENN:TRAINER_WALLY_1");
        let original = app.reserve_battle(a, group, fa, request.clone()).unwrap();
        seed_finalized_party_revision(&app, b, 2, true);
        assert_eq!(
            app.reserve_battle(a, group, fa, request.clone()),
            Ok(original)
        );

        let changed = BattleReservationRequest {
            kind: BattleReservationKind::Friendly,
            trainer_id: None,
            ..request.clone()
        };
        assert_eq!(
            app.reserve_battle(a, group, fa, changed),
            Err(Phase2Error::Conflict)
        );
        let wrong_owner = AuthenticatedActor {
            user_id: b.user_id,
            character_id: a.character_id,
        };
        assert_eq!(
            app.reserve_battle(wrong_owner, group, fa, request),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn reservation_json_keeps_friendly_wire_compatible_and_requires_trainer_identity() {
        let friendly = BattleReservationRequest {
            api_version: ApiVersion::V1,
            kind: BattleReservationKind::Friendly,
            trainer_id: None,
            friendly_rules: None,
            idempotency_key: key(515),
        };
        let json = serde_json::to_value(&friendly).unwrap();
        assert!(json.get("trainer_id").is_none());
        assert_eq!(
            serde_json::from_value::<BattleReservationRequest>(json).unwrap(),
            friendly
        );
        let mut invalid = friendly.clone();
        invalid.trainer_id =
            Some(TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1").unwrap());
        assert_eq!(invalid.validate(), Err(Phase2Error::InvalidRequest));
        invalid.kind = BattleReservationKind::CooperativeTrainer;
        assert!(invalid.validate().is_ok());
        invalid.trainer_id = None;
        assert_eq!(invalid.validate(), Err(Phase2Error::InvalidRequest));
    }

    #[test]
    fn current_reservation_is_visible_to_partner_without_extending_deadline() {
        let (app, a, b, group, fa, fb) = fixture();
        assert_eq!(app.current_battle(b, group, fb), Err(Phase2Error::NotFound));
        let reservation = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(160),
                },
            )
            .expect("reserve");
        assert_eq!(app.current_battle(b, group, fb), Ok(reservation.clone()));
        assert_eq!(app.current_battle(a, group, fa), Ok(reservation.clone()));
        let wire = serde_json::to_value(&reservation).expect("public view");
        assert!(wire.get("party_anchors").is_none());
        assert!(wire.get("snapshot_hashes").is_none());
        let accepted = app
            .accept_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(161),
                },
            )
            .expect("accept");
        assert_eq!(app.current_battle(a, group, fa), Ok(accepted.clone()));
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.battle_reservations[&reservation.battle_id].expires_at,
                    app.store.now() + BATTLE_ACCEPTED_IDLE_TTL_MS
                );
            })
            .expect("accepted deadline");
        assert!(accepted.expires_at > reservation.expires_at);
        let deadline = accepted.expires_at;
        assert_eq!(app.current_battle(b, group, fb), Ok(accepted.clone()));
        assert_eq!(
            app.inspect_battle_consensus(a, group, reservation.battle_id, fa)
                .expect("inspect")
                .reservation
                .expires_at,
            deadline
        );
        let replay = app
            .accept_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(161),
                },
            )
            .expect("replay");
        assert_eq!(replay.expires_at, deadline);
    }

    #[test]
    fn accepted_idle_deadline_renews_only_on_new_progress() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let now = app.store.now();
        let short_deadline = now + 1_000;
        app.store
            .write_transaction(|state| {
                let record = state.battle_reservations.get_mut(&battle).unwrap();
                record.expires_at = short_deadline;
                record.view.expires_at = Store::unix_timestamp(short_deadline).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            app.current_battle(a, group, fa).unwrap().expires_at.value(),
            short_deadline
        );
        assert_eq!(
            app.inspect_battle_consensus(b, group, battle, fb)
                .unwrap()
                .reservation
                .expires_at
                .value(),
            short_deadline
        );
        let hashes = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&battle]
                    .party_anchors
                    .as_ref()
                    .unwrap()
                    .clone()
                    .map(|anchor| anchor.digest)
            })
            .unwrap();
        let request = BattleSnapshotCommitRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(410),
            snapshot_hash: hashes[0].clone(),
            party_records: None,
        };
        let committed = app
            .commit_battle_snapshot(a, group, battle, fa, request.clone())
            .unwrap();
        assert_eq!(
            committed.reservation.expires_at.value(),
            now + BATTLE_ACCEPTED_IDLE_TTL_MS
        );
        let renewed = app
            .store
            .inspect_state(|state| state.battle_reservations[&battle].expires_at)
            .unwrap();
        assert_eq!(
            app.commit_battle_snapshot(a, group, battle, fa, request)
                .unwrap(),
            committed
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.battle_reservations[&battle].expires_at)
                .unwrap(),
            renewed
        );
    }

    #[test]
    fn accepted_battle_expires_at_idle_deadline_after_reads_and_exact_replay() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let now = app.store.now();
        let deadline = now + 1;
        app.store
            .write_transaction(|state| {
                let record = state.battle_reservations.get_mut(&battle).unwrap();
                record.expires_at = deadline;
                record.view.expires_at = Store::unix_timestamp(deadline).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        for _ in 0..3 {
            assert_eq!(
                app.current_battle(a, group, fa).unwrap().expires_at.value(),
                deadline
            );
            assert_eq!(
                app.inspect_battle_consensus(b, group, battle, fb)
                    .unwrap()
                    .reservation
                    .expires_at
                    .value(),
                deadline
            );
            app.accept_battle(
                b,
                group,
                battle,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(101),
                },
            )
            .expect("exact accept replay");
            assert_eq!(
                app.store
                    .inspect_state(|state| state.battle_reservations[&battle].expires_at)
                    .unwrap(),
                deadline
            );
        }
        app.store
            .write_transaction(|state| {
                let record = state.battle_reservations.get_mut(&battle).unwrap();
                record.expires_at = now;
                record.view.expires_at = Store::unix_timestamp(now).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        prune_expired(&app.store).unwrap();
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.battle_reservations[&battle].view.status,
                    BattleReservationStatus::Expired
                );
                assert!(!state.active_battle_by_member.contains_key(&a.character_id));
                assert!(!state.active_battle_by_member.contains_key(&b.character_id));
            })
            .unwrap();
        assert_eq!(app.current_battle(a, group, fa), Err(Phase2Error::NotFound));
    }

    #[test]
    fn stale_fence_cannot_renew_accepted_battle_after_reconnect() {
        let (app, a, _b, group, fa, _fb, battle) = accepted();
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                let lease = state.leases.get_mut(&a.character_id).unwrap();
                lease.contract = coop_cloud::LeaseContract::new(
                    lease.contract.fence(),
                    Store::unix_timestamp(now - 1).unwrap(),
                    super::super::storage::HEARTBEAT_INTERVAL_MS,
                )
                .unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        let fresh = app
            .reconnect(a, coop_cloud::ReconnectLeaseRequest::new(fa, key(420)))
            .expect("reconnect")
            .fence();
        let hash = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&battle]
                    .party_anchors
                    .as_ref()
                    .unwrap()[0]
                    .digest
                    .clone()
            })
            .unwrap();
        let request = BattleSnapshotCommitRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(421),
            snapshot_hash: hash,
            party_records: None,
        };
        let deadline = app
            .store
            .inspect_state(|state| state.battle_reservations[&battle].expires_at)
            .unwrap();
        assert_eq!(
            app.commit_battle_snapshot(a, group, battle, fa, request.clone()),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.store
                .inspect_state(|state| state.battle_reservations[&battle].expires_at)
                .unwrap(),
            deadline
        );
        assert!(
            app.commit_battle_snapshot(a, group, battle, fresh, request)
                .is_ok()
        );
    }

    #[test]
    fn group_expiry_releases_accepted_battle_locks_before_idle_deadline() {
        let (app, a, b, _group, _fa, _fb, battle) = accepted();
        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                state.leases.get_mut(&b.character_id).unwrap().grace_until = now - 1;
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            super::super::sessions::expire_groups(&app.store)
                .unwrap()
                .len(),
            1
        );
        prune_expired(&app.store).unwrap();
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.battle_reservations[&battle].view.status,
                    BattleReservationStatus::Expired
                );
                assert!(!state.active_battle_by_member.contains_key(&a.character_id));
                assert!(!state.active_battle_by_member.contains_key(&b.character_id));
            })
            .unwrap();
    }

    #[test]
    fn partner_ttl_expiry_keeps_local_battle_alive_until_reconnect_grace_ends() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let manifest = snapshots(&app, a, b, group, fa, fb, battle);
        assert!(release_start(&app, a, b, group, fa, fb, battle, &manifest).start_released);

        let now = app.store.now();
        app.store
            .write_transaction(|state| {
                let lease = state
                    .leases
                    .get_mut(&b.character_id)
                    .expect("partner lease");
                lease.contract = coop_cloud::LeaseContract::new(
                    lease.contract.fence(),
                    Store::unix_timestamp(now - 1).expect("expired timestamp"),
                    super::super::storage::HEARTBEAT_INTERVAL_MS,
                )
                .expect("lease contract");
                lease.grace_until = now
                    .saturating_add(super::super::storage::RECONNECT_GRACE_MS)
                    .saturating_sub(1);
                Ok::<_, Phase2Error>(())
            })
            .expect("expire partner TTL");

        app.submit_battle_action(
            a,
            group,
            battle,
            fa,
            BattleActionIntentRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(180),
                turn: 1,
                action: "local-progress".into(),
            },
        )
        .expect("local battle continues during reconnect grace");
        app.store
            .inspect_state(|state| {
                assert_eq!(
                    state.battle_reservations[&battle].consensus.turns[0].actions[0].as_deref(),
                    Some("local-progress")
                );
            })
            .expect("local action persisted");

        app.store
            .write_transaction(|state| {
                state
                    .leases
                    .get_mut(&b.character_id)
                    .expect("partner lease")
                    .grace_until = app.store.now().saturating_sub(1);
                Ok::<_, Phase2Error>(())
            })
            .expect("end reconnect grace");
        assert_eq!(
            app.submit_battle_action(
                a,
                group,
                battle,
                fa,
                BattleActionIntentRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(181),
                    turn: 2,
                    action: "too-late".into(),
                },
            ),
            Err(Phase2Error::Expired)
        );
    }

    #[test]
    fn declined_status_is_visible_only_by_exact_battle_id_to_current_members() {
        let (app, a, b, group, fa, fb) = fixture();
        app.add_invitation("battle-terminal-outsider")
            .expect("outsider invite");
        let foreign = app
            .register(
                RegisterRequest::new(
                    "terminaloutsider",
                    Password::new("password-c").expect("password"),
                    InvitationCode::new("battle-terminal-outsider").expect("code"),
                )
                .expect("register request"),
            )
            .expect("outsider");
        let outsider = AuthenticatedActor {
            user_id: foreign.user_id,
            character_id: foreign.character_id,
        };
        let reservation = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(165),
                },
            )
            .expect("reserve");
        let declined = app
            .decline_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(166),
                },
            )
            .expect("decline");
        assert_eq!(declined.status, BattleReservationStatus::Declined);
        assert_eq!(app.current_battle(a, group, fa), Err(Phase2Error::NotFound));
        assert_eq!(
            app.inspect_battle_consensus(a, group, reservation.battle_id, fa)
                .expect("requester exact lookup")
                .reservation
                .status,
            BattleReservationStatus::Declined
        );
        assert_eq!(
            app.inspect_battle_consensus(b, group, reservation.battle_id, fb)
                .expect("responder exact lookup")
                .reservation
                .status,
            BattleReservationStatus::Declined
        );
        assert_eq!(
            app.inspect_battle_consensus(outsider, group, reservation.battle_id, fb),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn current_reservation_hides_terminal_expired_and_foreign_battles() {
        let (app, a, b, group, fa, fb) = fixture();
        app.add_invitation("battle-c").expect("outsider invite");
        let foreign = app
            .register(
                RegisterRequest::new(
                    "battleoutsider",
                    Password::new("password-c").expect("password"),
                    InvitationCode::new("battle-c").expect("code"),
                )
                .expect("register request"),
            )
            .expect("outsider");
        let outsider = AuthenticatedActor {
            user_id: foreign.user_id,
            character_id: foreign.character_id,
        };
        let reservation = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(162),
                },
            )
            .expect("reserve");
        let wrong_group = GroupId::new(Uuid::new_v4()).expect("group id");
        assert_eq!(
            app.current_battle(b, wrong_group, fb),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            app.current_battle(outsider, group, fb),
            Err(Phase2Error::NotFound)
        );
        app.cancel_battle(
            a,
            group,
            reservation.battle_id,
            fa,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(163),
            },
        )
        .expect("cancel");
        assert_eq!(app.current_battle(b, group, fb), Err(Phase2Error::NotFound));
        let next = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(164),
                },
            )
            .expect("reserve again");
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&next.battle_id)
                    .unwrap()
                    .expires_at = 0;
                Ok::<_, Phase2Error>(())
            })
            .expect("expire");
        assert_eq!(app.current_battle(b, group, fb), Err(Phase2Error::NotFound));
    }

    #[test]
    fn reservation_requires_finalized_party_for_both_members() {
        let (app, actor_a, actor_b, group_id, fence_a, _) = fixture();
        app.store
            .write_transaction(|state| {
                let character = state.characters.get_mut(&actor_b.character_id).unwrap();
                character.revision = Revision::initial();
                character.active_snapshot = None;
                Ok::<_, Phase2Error>(())
            })
            .expect("remove finalized snapshot");
        assert_eq!(
            app.reserve_battle(
                actor_a,
                group_id,
                fence_a,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(150),
                }
            ),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn snapshot_commit_rejects_wrong_hash_and_stale_party_anchor() {
        let (app, actor_a, actor_b, group_id, fence_a, fence_b, battle) = accepted();
        let wrong = BattleSnapshotCommitRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(151),
            snapshot_hash: "a".repeat(64),
            party_records: None,
        };
        assert_eq!(
            app.commit_battle_snapshot(actor_a, group_id, battle, fence_a, wrong),
            Err(Phase2Error::Conflict)
        );
        app.store
            .inspect_state(|state| {
                assert!(
                    state.battle_reservations[&battle]
                        .consensus
                        .commitments
                        .iter()
                        .all(Option::is_none)
                )
            })
            .expect("no commitment");
        let correct = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&battle]
                    .party_anchors
                    .as_ref()
                    .unwrap()[0]
                    .digest
                    .clone()
            })
            .expect("anchor");
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&actor_b.character_id)
                    .unwrap()
                    .revision = Revision::new(2);
                Ok::<_, Phase2Error>(())
            })
            .expect("advance partner revision");
        assert_eq!(
            app.commit_battle_snapshot(
                actor_a,
                group_id,
                battle,
                fence_a,
                BattleSnapshotCommitRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(152),
                    snapshot_hash: correct,
                    party_records: None,
                }
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            app.inspect_battle_consensus(actor_b, group_id, battle, fence_b)
                .expect("poll")
                .commitments,
            [None, None]
        );
    }

    #[test]
    fn accept_requires_partner_and_decline_releases_both_locks() {
        let (app, actor_a, actor_b, group_id, fence_a, fence_b) = fixture();
        let reservation = app
            .reserve_battle(
                actor_a,
                group_id,
                fence_a,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::CooperativeTrainer,
                    trainer_id: Some(
                        TrainerInstanceId::new(RegionId::Hoenn, "TRAINER_WALLY_1")
                            .expect("trainer"),
                    ),
                    friendly_rules: None,
                    idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(51)).expect("key"),
                },
            )
            .expect("reserve");
        let denied = app.accept_battle(
            actor_a,
            group_id,
            reservation.battle_id,
            fence_a,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(52)).expect("key"),
            },
        );
        assert_eq!(denied, Err(Phase2Error::Forbidden));
        let declined = app
            .decline_battle(
                actor_b,
                group_id,
                reservation.battle_id,
                fence_b,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(53)).expect("key"),
                },
            )
            .expect("decline");
        assert_eq!(declined.status, BattleReservationStatus::Declined);
        let next = app.reserve_battle(
            actor_a,
            group_id,
            fence_a,
            BattleReservationRequest {
                api_version: ApiVersion::V1,
                kind: BattleReservationKind::Friendly,
                trainer_id: None,
                friendly_rules: None,
                idempotency_key: IdempotencyKey::new(uuid::Uuid::from_u128(54)).expect("key"),
            },
        );
        assert!(next.is_ok());
    }

    fn terminal_hash(
        app: &Phase2App,
        a: AuthenticatedActor,
        b: AuthenticatedActor,
        group: GroupId,
        fa: LeaseFence,
        fb: LeaseFence,
        battle: Uuid,
        key_base: u128,
    ) -> String {
        let manifest = snapshots(app, a, b, group, fa, fb, battle);
        play_terminal_turn(app, a, b, group, fa, fb, battle, &manifest, key_base)
    }

    fn play_terminal_turn(
        app: &Phase2App,
        a: AuthenticatedActor,
        b: AuthenticatedActor,
        group: GroupId,
        fa: LeaseFence,
        fb: LeaseFence,
        battle: Uuid,
        manifest: &BattleManifest,
        key_base: u128,
    ) -> String {
        release_start(app, a, b, group, fa, fb, battle, manifest);
        for (actor, fence, number, action) in [
            (a, fa, key_base, "member-0"),
            (b, fb, key_base + 1, "member-1"),
        ] {
            app.submit_battle_action(
                actor,
                group,
                battle,
                fence,
                BattleActionIntentRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(number),
                    turn: 1,
                    action: action.to_owned(),
                },
            )
            .expect("action");
        }
        let hash = "a".repeat(64);
        app.acknowledge_battle_hash(
            a,
            group,
            battle,
            fa,
            BattleStateHashRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(key_base + 2),
                turn: 1,
                state_hash: hash.clone(),
            },
        )
        .expect("first terminal hash");
        app.acknowledge_battle_hash(
            b,
            group,
            battle,
            fb,
            BattleStateHashRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(key_base + 3),
                turn: 1,
                state_hash: hash.clone(),
            },
        )
        .expect("second terminal hash");
        hash
    }

    #[test]
    fn trainer_finish_rejects_a_party_head_changed_after_terminal_hash() {
        let (app, a, b, group, fa, fb) = fixture();
        let reservation = app
            .reserve_battle(a, group, fa, trainer_request(800, "HOENN:TRAINER_WALLY_1"))
            .expect("reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(801),
            },
        )
        .expect("accept");
        let hash = terminal_hash(&app, a, b, group, fa, fb, reservation.battle_id, 810);
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&a.character_id)
                    .expect("character")
                    .revision = Revision::new(2);
                Ok::<_, Phase2Error>(())
            })
            .expect("advance party head");
        assert_eq!(
            app.finish_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(820),
                    result: BattleFinishResult::Won,
                    turn: 1,
                    state_hash: hash,
                },
            ),
            Err(Phase2Error::Conflict)
        );
        assert!(
            app.store
                .inspect_state(|state| state.battle_reservations[&reservation.battle_id]
                    .consensus
                    .finish_attestations
                    .iter()
                    .all(Option::is_none))
                .expect("no terminal receipt")
        );
    }

    #[test]
    fn friendly_finish_requires_paired_hashes_and_releases_on_matching_terminal_result() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let first_request = BattleFinishRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(700),
            result: BattleFinishResult::Member0Won,
            turn: 1,
            state_hash: "a".repeat(64),
        };
        assert_eq!(
            app.finish_battle(a, group, battle, fa, first_request.clone()),
            Err(Phase2Error::Conflict)
        );
        let hash = terminal_hash(&app, a, b, group, fa, fb, battle, 710);
        let first = app
            .finish_battle(
                a,
                group,
                battle,
                fa,
                BattleFinishRequest {
                    state_hash: hash.clone(),
                    ..first_request
                },
            )
            .expect("first finish");
        assert_eq!(first.status, BattleReservationStatus::Accepted);
        assert_eq!(
            app.finish_battle(
                a,
                group,
                battle,
                fa,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(700),
                    result: BattleFinishResult::Member0Won,
                    turn: 1,
                    state_hash: hash.clone(),
                },
            )
            .expect("exact retry"),
            first
        );
        let completed = app
            .finish_battle(
                b,
                group,
                battle,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(701),
                    result: BattleFinishResult::Member0Won,
                    turn: 1,
                    state_hash: hash,
                },
            )
            .expect("matching partner finish");
        assert_eq!(completed.status, BattleReservationStatus::Completed);
        assert!(
            app.store
                .inspect_state(|state| state.active_battle_by_member.is_empty())
                .expect("locks")
        );
        assert_eq!(
            app.finish_battle(
                b,
                group,
                battle,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(701),
                    result: BattleFinishResult::Member0Won,
                    turn: 1,
                    state_hash: "a".repeat(64),
                },
            )
            .expect("terminal retry"),
            completed
        );
    }

    #[test]
    fn friendly_result_mismatch_diverges_and_releases_locks() {
        let (app, a, b, group, fa, fb, battle) = accepted();
        let hash = terminal_hash(&app, a, b, group, fa, fb, battle, 730);
        app.finish_battle(
            a,
            group,
            battle,
            fa,
            BattleFinishRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(740),
                result: BattleFinishResult::Member0Won,
                turn: 1,
                state_hash: hash.clone(),
            },
        )
        .expect("first finish");
        let diverged = app
            .finish_battle(
                b,
                group,
                battle,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(741),
                    result: BattleFinishResult::Member1Won,
                    turn: 1,
                    state_hash: hash,
                },
            )
            .expect("mismatch finish");
        assert_eq!(diverged.status, BattleReservationStatus::Diverged);
        assert!(
            app.store
                .inspect_state(|state| state.active_battle_by_member.is_empty())
                .expect("locks")
        );
    }

    #[test]
    fn trainer_finish_is_commit_pending_without_progress_or_lock_release() {
        let (app, a, b, group, fa, fb) = fixture();
        let reservation = app
            .reserve_battle(a, group, fa, trainer_request(750, "HOENN:TRAINER_WALLY_1"))
            .expect("reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(751),
            },
        )
        .expect("accept");
        let before = app
            .store
            .inspect_state(|state| {
                [
                    state.characters[&a.character_id].state.clone(),
                    state.characters[&b.character_id].state.clone(),
                ]
            })
            .expect("progress snapshot");
        let hash = terminal_hash(&app, a, b, group, fa, fb, reservation.battle_id, 760);
        let first = app
            .finish_battle(
                a,
                group,
                reservation.battle_id,
                fa,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(770),
                    result: BattleFinishResult::Won,
                    turn: 1,
                    state_hash: hash.clone(),
                },
            )
            .expect("first trainer finish");
        assert_eq!(first.status, BattleReservationStatus::Accepted);
        let final_view = app
            .finish_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(771),
                    result: BattleFinishResult::Won,
                    turn: 1,
                    state_hash: hash,
                },
            )
            .expect("second trainer finish");
        assert_eq!(final_view.status, BattleReservationStatus::CommitPending);
        let grants = app
            .store
            .inspect_state(|state| {
                state.battle_reservations[&reservation.battle_id]
                    .commit_grants
                    .clone()
            })
            .expect("commit grants")
            .expect("won grants");
        assert_ne!(grants[0].grant_id, grants[1].grant_id);
        assert_eq!(grants[0].battle_id, reservation.battle_id);
        assert_eq!(grants[1].battle_id, reservation.battle_id);
        assert_eq!(grants[0].character_id, a.character_id);
        assert_eq!(grants[1].character_id, b.character_id);
        assert_eq!(grants[0].trainer_id.as_str(), "HOENN:TRAINER_WALLY_1");
        assert_eq!(grants[1].trainer_id, grants[0].trainer_id);
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, reservation.battle_id, fa),
            Ok(grants[0].clone())
        );
        assert_eq!(
            retrieve_commit_grant(&app.store, b, group, reservation.battle_id, fb),
            Ok(grants[1].clone())
        );
        let replayed = app
            .finish_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(771),
                    result: BattleFinishResult::Won,
                    turn: 1,
                    state_hash: "a".repeat(64),
                },
            )
            .expect("exact finish replay");
        assert_eq!(replayed, final_view);
        assert_eq!(
            app.store
                .inspect_state(|state| state.battle_reservations[&reservation.battle_id]
                    .commit_grants
                    .as_ref()
                    .unwrap()
                    .clone())
                .expect("grants stay stable"),
            grants
        );

        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&reservation.battle_id)
                    .expect("reservation")
                    .expires_at = app.store.now();
                Ok::<_, Phase2Error>(())
            })
            .expect("expire grant");
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, reservation.battle_id, fa),
            Err(Phase2Error::Conflict)
        );
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&reservation.battle_id)
                    .expect("reservation")
                    .expires_at = final_view.expires_at.value();
                Ok::<_, Phase2Error>(())
            })
            .expect("restore grant deadline");

        // A legacy CommitPending tombstone can decode without the new
        // optional field, but its grant retrieval must fail closed.
        app.store
            .write_transaction(|state| {
                let record = state
                    .battle_reservations
                    .get(&reservation.battle_id)
                    .cloned()
                    .expect("record");
                let mut json = serde_json::to_value(record).expect("record json");
                json.as_object_mut()
                    .expect("record object")
                    .remove("commit_grants");
                let legacy: BattleReservationRecord =
                    serde_json::from_value(json).expect("legacy record");
                assert!(legacy.commit_grants.is_none());
                state
                    .battle_reservations
                    .insert(reservation.battle_id, legacy);
                Ok::<_, Phase2Error>(())
            })
            .expect("install legacy record");
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, reservation.battle_id, fa),
            Err(Phase2Error::Conflict)
        );
        assert!(
            !app.store
                .inspect_state(|state| state.active_battle_by_member.is_empty())
                .expect("locks")
        );
        assert_eq!(
            app.current_battle(a, group, fa)
                .expect("commit-pending current battle")
                .status,
            BattleReservationStatus::CommitPending
        );
        assert_eq!(
            app.store
                .inspect_state(|state| [
                    state.characters[&a.character_id].state.clone(),
                    state.characters[&b.character_id].state.clone(),
                ])
                .expect("progress"),
            before
        );
        let cancelled = app
            .cancel_battle(
                a,
                group,
                reservation.battle_id,
                fa,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(772),
                },
            )
            .expect("cancel pre-ledger reservation");
        assert_eq!(cancelled.status, BattleReservationStatus::Cancelled);
        assert!(
            app.store
                .inspect_state(|state| state.active_battle_by_member.is_empty())
                .expect("cancel releases locks")
        );
        let replacement = app
            .reserve_battle(
                a,
                group,
                fa,
                BattleReservationRequest {
                    api_version: ApiVersion::V1,
                    kind: BattleReservationKind::Friendly,
                    trainer_id: None,
                    friendly_rules: None,
                    idempotency_key: key(773),
                },
            )
            .expect("reserve after cancellation");
        app.cancel_battle(
            b,
            group,
            replacement.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(774),
            },
        )
        .expect("clear replacement reservation");

        let expiring = app
            .reserve_battle(a, group, fa, trainer_request(780, "HOENN:TRAINER_WALLY_1"))
            .expect("reserve expiring trainer");
        app.accept_battle(
            b,
            group,
            expiring.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(781),
            },
        )
        .expect("accept expiring trainer");
        let expiring_hash = terminal_hash(&app, a, b, group, fa, fb, expiring.battle_id, 790);
        app.finish_battle(
            a,
            group,
            expiring.battle_id,
            fa,
            BattleFinishRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(800),
                result: BattleFinishResult::Won,
                turn: 1,
                state_hash: expiring_hash.clone(),
            },
        )
        .expect("first expiring finish");
        app.finish_battle(
            b,
            group,
            expiring.battle_id,
            fb,
            BattleFinishRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(801),
                result: BattleFinishResult::Won,
                turn: 1,
                state_hash: expiring_hash,
            },
        )
        .expect("second expiring finish");
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&expiring.battle_id)
                    .expect("reservation")
                    .expires_at = 0;
                Ok::<_, Phase2Error>(())
            })
            .expect("expire pending ledger");
        prune_expired(&app.store).expect("prune");
        assert_eq!(
            app.store
                .inspect_state(|state| {
                    state.battle_reservations[&expiring.battle_id].view.status
                })
                .expect("status"),
            BattleReservationStatus::Expired
        );
        assert!(
            app.store
                .inspect_state(|state| state.active_battle_by_member.is_empty())
                .expect("released locks")
        );
    }

    #[test]
    fn wally_commit_grant_consumption_is_exact_and_one_shot() {
        let (app, a, b, group, fa, _fb, reservation, grants) = pending_wally();
        let source = validated_fixture_save(&fixture_party_sav(), 1);
        let incoming = validated_fixture_save(&wally_story_save(true, true), 2);
        let request_a = finalize_request_for_grant(&app, a, grants[0].grant_id, 950);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request_a, &source, &incoming, app.store.now())
            }),
            Ok(true)
        );
        assert_eq!(
            app.store
                .inspect_state(|state| {
                    (
                        state.battle_reservations[&reservation.battle_id].commit_grant_applied,
                        state.battle_reservations[&reservation.battle_id]
                            .view
                            .status,
                    )
                })
                .expect("applied state"),
            ([true, false], BattleReservationStatus::CommitPending)
        );
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request_a, &source, &incoming, app.store.now())
            }),
            Err(Phase2Error::Conflict)
        );
        let request_b = finalize_request_for_grant(&app, b, grants[1].grant_id, 951);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, b, &request_b, &source, &incoming, app.store.now())
            }),
            Ok(true)
        );
        assert_eq!(
            app.store
                .inspect_state(|state| {
                    (
                        state.battle_reservations[&reservation.battle_id].commit_grant_applied,
                        state.battle_reservations[&reservation.battle_id]
                            .view
                            .status,
                        state.active_battle_by_member.is_empty(),
                    )
                })
                .expect("completed state"),
            ([true, true], BattleReservationStatus::Completed, true)
        );
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, reservation.battle_id, fa),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn wally_commit_grant_rejects_unknown_forged_and_stale_sources() {
        let (app, a, _b, _group, _fa, _fb, _reservation, _grants) = pending_wally();
        let source = validated_fixture_save(&fixture_party_sav(), 1);
        let incoming = validated_fixture_save(&wally_story_save(true, true), 2);
        let unknown = CommitId::new(Uuid::from_u128(0xfeed)).expect("commit id");
        let unknown_request = finalize_request_for_grant(&app, a, unknown, 960);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(
                    state,
                    a,
                    &unknown_request,
                    &source,
                    &incoming,
                    app.store.now(),
                )
            }),
            Err(Phase2Error::Forbidden)
        );

        let (app, a, _b, _group, _fa, _fb, _reservation, grants) = pending_wally();
        let request = finalize_request_for_grant(&app, a, grants[0].grant_id, 961);
        let mut forged_bytes = wally_story_save(true, true);
        add_forged_event(&mut forged_bytes);
        let forged = validated_fixture_save(&forged_bytes, 2);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request, &source, &forged, app.store.now())
            }),
            Err(Phase2Error::Conflict)
        );

        let mut forged_bytes = wally_story_save(true, true);
        let offset = coop_save::SAVE_BLOCK1_FLAGS_OFFSET + 0x100 / 8;
        let logical = 1 + offset / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        let physical = selected_physical(&forged_bytes, logical);
        let position = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE
            + offset % coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        forged_bytes[position] ^= 1;
        rewrite_sector_checksum(&mut forged_bytes, logical);
        let forged_story = validated_fixture_save(&forged_bytes, 2);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request, &source, &forged_story, app.store.now())
            }),
            Err(Phase2Error::Conflict)
        );
        let mut forged_bytes = wally_story_save(true, true);
        let offset = coop_save::SAVE_BLOCK1_VARS_OFFSET + 2 * (0x4020 - 0x4000);
        let logical = 1 + offset / coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        let physical = selected_physical(&forged_bytes, logical);
        let position = (coop_save::SECTORS_PER_SLOT + physical) * coop_save::SECTOR_SIZE
            + offset % coop_save::SAVE_BLOCK3_CHUNK_OFFSET;
        forged_bytes[position] ^= 1;
        rewrite_sector_checksum(&mut forged_bytes, logical);
        let forged_story = validated_fixture_save(&forged_bytes, 2);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request, &source, &forged_story, app.store.now())
            }),
            Err(Phase2Error::Conflict)
        );

        let (app, a, _b, _group, _fa, _fb, _reservation, grants) = pending_wally();
        let request = finalize_request_for_grant(&app, a, grants[0].grant_id, 962);
        app.store
            .write_transaction(|state| {
                state.characters.get_mut(&a.character_id).unwrap().revision = Revision::new(2);
                Ok::<_, Phase2Error>(())
            })
            .expect("stale source");
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(state, a, &request, &source, &incoming, app.store.now())
            }),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn wally_story_fields_require_a_grant_and_one_sided_expiry_keeps_applied_state() {
        let (app, a, _b, _group, _fa, _fb, _reservation, _grants) = pending_wally();
        let source = validated_fixture_save(&fixture_party_sav(), 1);
        let legacy_only = validated_fixture_save(&wally_story_save(false, true), 2);
        let no_commit_request = SnapshotFinalizeRequest {
            last_applied_commit: None,
            ..finalize_request_for_grant(
                &app,
                a,
                CommitId::new(Uuid::from_u128(0xbeef)).expect("commit id"),
                970,
            )
        };
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(
                    state,
                    a,
                    &no_commit_request,
                    &source,
                    &legacy_only,
                    app.store.now(),
                )
            }),
            Err(Phase2Error::Forbidden)
        );
        let committed = validated_fixture_save(&wally_story_save(true, true), 1);
        let regressed = validated_fixture_save(&fixture_party_sav(), 2);
        assert_eq!(
            app.store.write_transaction(|state| {
                apply_commit_grant(
                    state,
                    a,
                    &no_commit_request,
                    &committed,
                    &regressed,
                    app.store.now(),
                )
            }),
            Err(Phase2Error::Conflict)
        );

        let (app, a, _b, _group, _fa, _fb, reservation, grants) = pending_wally();
        let source = validated_fixture_save(&fixture_party_sav(), 1);
        let incoming = validated_fixture_save(&wally_story_save(true, true), 2);
        let request = finalize_request_for_grant(&app, a, grants[0].grant_id, 971);
        app.store
            .write_transaction(|state| {
                apply_commit_grant(state, a, &request, &source, &incoming, app.store.now())
            })
            .expect("first member applied");
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&reservation.battle_id)
                    .expect("reservation")
                    .expires_at = 0;
                Ok::<_, Phase2Error>(())
            })
            .expect("expire reservation");
        prune_expired(&app.store).expect("prune");
        assert_eq!(
            app.store
                .inspect_state(|state| {
                    let record = &state.battle_reservations[&reservation.battle_id];
                    (record.commit_grant_applied, record.view.status)
                })
                .expect("expired record"),
            ([true, false], BattleReservationStatus::Expired)
        );
    }

    #[test]
    fn trainer_loss_and_draw_settle_without_commit_grants() {
        for (key_base, result) in [
            (900_u128, BattleFinishResult::Lost),
            (930_u128, BattleFinishResult::Draw),
        ] {
            let (app, a, b, group, fa, fb) = fixture();
            let reservation = app
                .reserve_battle(
                    a,
                    group,
                    fa,
                    trainer_request(key_base, "HOENN:TRAINER_WALLY_1"),
                )
                .expect("reserve");
            app.accept_battle(
                b,
                group,
                reservation.battle_id,
                fb,
                BattleReservationActionRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(key_base + 1),
                },
            )
            .expect("accept");
            let hash = terminal_hash(
                &app,
                a,
                b,
                group,
                fa,
                fb,
                reservation.battle_id,
                key_base + 10,
            );
            app.finish_battle(
                a,
                group,
                reservation.battle_id,
                fa,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(key_base + 20),
                    result,
                    turn: 1,
                    state_hash: hash.clone(),
                },
            )
            .expect("first terminal result");
            let final_view = app
                .finish_battle(
                    b,
                    group,
                    reservation.battle_id,
                    fb,
                    BattleFinishRequest {
                        api_version: ApiVersion::V1,
                        idempotency_key: key(key_base + 21),
                        result,
                        turn: 1,
                        state_hash: hash,
                    },
                )
                .expect("second terminal result");
            assert_eq!(final_view.status, BattleReservationStatus::Completed);
            assert!(
                app.store
                    .inspect_state(|state| state.battle_reservations[&reservation.battle_id]
                        .commit_grants
                        .is_none())
                    .expect("no grant")
            );
            assert!(
                app.store
                    .inspect_state(|state| state.active_battle_by_member.is_empty())
                    .expect("locks released")
            );
        }
    }

    // ---- Local-reward trainer battles -------------------------------------

    const LOCAL_TRAINER: &str = "HOENN:TRAINER_SAWYER_1";

    /// A checksum-valid party record with personality and OT ID zero, so the
    /// growth substruct is stored unencrypted at offset 32.
    fn party_record(species: u8, hp: u16, egg: bool) -> [u8; 100] {
        let mut mon = [0_u8; 100];
        mon[19] = if egg { 2 | 4 } else { 2 };
        mon[28] = species;
        mon[32] = species;
        mon[86..88].copy_from_slice(&hp.to_le_bytes());
        mon
    }

    fn usable(species: u8) -> [u8; 100] {
        party_record(species, 20, false)
    }

    fn local_commit(records: &[[u8; 100]], number: u128) -> BattleSnapshotCommitRequest {
        BattleSnapshotCommitRequest {
            api_version: ApiVersion::V1,
            idempotency_key: key(number),
            snapshot_hash: records_digest(records),
            party_records: Some(records.iter().map(|record| hex_string(record)).collect()),
        }
    }

    fn place(
        app: &Phase2App,
        actor: AuthenticatedActor,
        region: RegionId,
        map: &str,
        channel: u16,
    ) {
        app.store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&actor.character_id)
                    .unwrap()
                    .state
                    .world_zone = WorldZone::new(region, map, channel).unwrap();
                Ok::<_, Phase2Error>(())
            })
            .unwrap();
    }

    /// Two grouped members on connected Hoenn routes (Route 104 and 105).
    fn local_fixture() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        GroupId,
        LeaseFence,
        LeaseFence,
    ) {
        let (app, a, b, group, fa, fb) = fixture();
        place(&app, a, RegionId::Hoenn, "ROUTE104", 1);
        place(&app, b, RegionId::Hoenn, "ROUTE105", 1);
        (app, a, b, group, fa, fb)
    }

    fn local_accepted() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        GroupId,
        LeaseFence,
        LeaseFence,
        Uuid,
    ) {
        let (app, a, b, group, fa, fb) = local_fixture();
        let reservation = app
            .reserve_battle(a, group, fa, trainer_request(900, LOCAL_TRAINER))
            .expect("local reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(901),
            },
        )
        .expect("accept");
        (app, a, b, group, fa, fb, reservation.battle_id)
    }

    fn role_of(view: &BattleReservationView, actor: AuthenticatedActor) -> BattleMemberRole {
        let index = view
            .member_character_ids
            .iter()
            .position(|member| *member == actor.character_id)
            .expect("member");
        view.member_roles.expect("roles")[index]
    }

    #[test]
    fn local_trainer_reserves_with_partner_on_connected_map() {
        let (app, a, b, group, fa, _) = local_fixture();
        let view = app
            .reserve_battle(a, group, fa, trainer_request(910, LOCAL_TRAINER))
            .expect("connected maps reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Local);
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
        assert_eq!(role_of(&view, b), BattleMemberRole::Participant);
        assert_eq!(view.status, BattleReservationStatus::Pending);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["reward_mode"], "LOCAL");
        assert_eq!(
            json["member_roles"],
            serde_json::json!(["PARTICIPANT", "PARTICIPANT"])
        );
        assert_eq!(
            serde_json::from_value::<BattleReservationView>(json).unwrap(),
            view
        );

        // The same map also qualifies.
        let (app, a, b, group, fa, _) = local_fixture();
        place(&app, b, RegionId::Hoenn, "ROUTE104", 1);
        assert!(
            app.reserve_battle(a, group, fa, trainer_request(911, LOCAL_TRAINER))
                .is_ok()
        );
    }

    #[test]
    fn local_trainer_rejects_unconnected_map_channel_and_region() {
        let (app, a, b, group, fa, _) = local_fixture();
        place(&app, b, RegionId::Hoenn, "LITTLEROOT_TOWN", 1);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(920, LOCAL_TRAINER)),
            Err(Phase2Error::Conflict)
        );
        place(&app, b, RegionId::Hoenn, "ROUTE105", 2);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(921, LOCAL_TRAINER)),
            Err(Phase2Error::Conflict)
        );
        place(&app, a, RegionId::Kanto, "PALLET_TOWN", 1);
        place(&app, b, RegionId::Kanto, "PALLET_TOWN", 1);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(922, LOCAL_TRAINER)),
            Err(Phase2Error::Conflict)
        );
        // A syntactically valid trainer outside the identity catalog is
        // still rejected wherever the members stand.
        place(&app, a, RegionId::Hoenn, "ROUTE104", 1);
        place(&app, b, RegionId::Hoenn, "ROUTE104", 1);
        assert_eq!(
            app.reserve_battle(
                a,
                group,
                fa,
                trainer_request(923, "HOENN:TRAINER_FABRICATED")
            ),
            Err(Phase2Error::Conflict)
        );
        assert!(
            app.reserve_battle(a, group, fa, trainer_request(924, LOCAL_TRAINER))
                .is_ok()
        );
    }

    #[test]
    fn partner_who_beat_the_trainer_joins_as_helper() {
        let (app, a, b, group, fa, _) = local_fixture();
        seed_finalized_party_defeating(&app, b, 2, Some(LOCAL_TRAINER));
        let view = app
            .reserve_battle(a, group, fa, trainer_request(930, LOCAL_TRAINER))
            .expect("helper partner reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Local);
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
        assert_eq!(role_of(&view, b), BattleMemberRole::Helper);
        let json = serde_json::to_value(&view).unwrap();
        let roles = json["member_roles"].as_array().unwrap();
        assert!(roles.contains(&serde_json::json!("HELPER")));
    }

    #[test]
    fn requester_who_beat_the_trainer_is_rejected() {
        let (app, a, b, group, fa, fb) = local_fixture();
        seed_finalized_party_defeating(&app, a, 2, Some(LOCAL_TRAINER));
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(940, LOCAL_TRAINER)),
            Err(Phase2Error::Conflict)
        );
        // The partner who has not beaten it may still request, with the
        // first member helping.
        let view = app
            .reserve_battle(b, group, fb, trainer_request(941, LOCAL_TRAINER))
            .expect("partner requests");
        assert_eq!(role_of(&view, a), BattleMemberRole::Helper);
        assert_eq!(role_of(&view, b), BattleMemberRole::Participant);
    }

    const CALVIN_1: &str = "HOENN:TRAINER_CALVIN_1";
    const CALVIN_2: &str = "HOENN:TRAINER_CALVIN_2";
    const BRAWLY_1: &str = "HOENN:TRAINER_BRAWLY_1";

    #[test]
    fn rematch_requires_the_requester_to_have_beaten_the_first_battle() {
        let (app, a, _, group, fa, _) = local_fixture();
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(1000, CALVIN_2)),
            Err(Phase2Error::Conflict)
        );
        seed_finalized_party_defeating(&app, a, 2, Some(CALVIN_1));
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1001, CALVIN_2))
            .expect("rematch reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Local);
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
    }

    #[test]
    fn rematch_already_won_at_this_stage_can_be_fought_again() {
        let (app, a, _, group, fa, _) = local_fixture();
        // The old rule rejected a requester who had beaten the trainer; a
        // rematch at its last stage is exactly that.
        seed_finalized_progress(&app, a, 2, &[CALVIN_1, CALVIN_2], 0);
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1010, CALVIN_2))
            .expect("rematch at a beaten stage");
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
    }

    #[test]
    fn rematch_partner_role_follows_the_first_battle() {
        let (app, a, b, group, fa, _) = local_fixture();
        seed_finalized_party_defeating(&app, a, 2, Some(CALVIN_1));
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1020, CALVIN_2))
            .expect("helper partner");
        assert_eq!(role_of(&view, b), BattleMemberRole::Helper);

        let (app, a, b, group, fa, _) = local_fixture();
        seed_finalized_party_defeating(&app, a, 2, Some(CALVIN_1));
        seed_finalized_party_defeating(&app, b, 2, Some(CALVIN_1));
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1021, CALVIN_2))
            .expect("participant partner");
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
        assert_eq!(role_of(&view, b), BattleMemberRole::Participant);
    }

    #[test]
    fn gym_reservation_requires_the_requester_to_lack_the_badge() {
        let (app, a, b, group, fa, _) = local_fixture();
        place(&app, a, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
        place(&app, b, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
        seed_finalized_progress(&app, a, 2, &[], 0b0000_0011);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(1030, BRAWLY_1)),
            Err(Phase2Error::Conflict)
        );
        seed_finalized_progress(&app, a, 3, &[], 0b0000_0001);
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1031, BRAWLY_1))
            .expect("gym reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Local);
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
    }

    #[test]
    fn gym_partner_participates_only_at_the_same_story_point() {
        for (partner_badges, expected) in [
            (0b0000_0001, BattleMemberRole::Participant), // Brawly is next
            (0b0000_0000, BattleMemberRole::Helper),      // behind: no Stone Badge
            (0b0000_0011, BattleMemberRole::Helper),      // already has it
            (0b0000_0101, BattleMemberRole::Helper),      // a later badge, not this one
        ] {
            let (app, a, b, group, fa, _) = local_fixture();
            place(&app, a, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
            place(&app, b, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
            seed_finalized_progress(&app, a, 2, &[], 0b0000_0001);
            seed_finalized_progress(&app, b, 2, &[], partner_badges);
            let view = app
                .reserve_battle(a, group, fa, trainer_request(1040, BRAWLY_1))
                .expect("gym reserve");
            assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
            assert_eq!(
                role_of(&view, b),
                expected,
                "partner badges {partner_badges:#b}"
            );
        }
    }

    #[test]
    fn gym_role_ignores_a_stale_leader_defeat_bit() {
        // The badge, not the trainer bit, decides a gym: a partner whose
        // save recorded the leader but not the badge still participates.
        let (app, a, b, group, fa, _) = local_fixture();
        place(&app, a, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
        place(&app, b, RegionId::Hoenn, "DEWFORD_TOWN_GYM", 1);
        seed_finalized_progress(&app, a, 2, &[], 0b0000_0001);
        seed_finalized_progress(&app, b, 2, &[BRAWLY_1], 0b0000_0001);
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1050, BRAWLY_1))
            .expect("gym reserve");
        assert_eq!(role_of(&view, b), BattleMemberRole::Participant);
    }

    #[test]
    fn local_commit_serves_exact_staged_records_as_peer_party() {
        let (app, a, b, group, fa, fb, battle) = local_accepted();
        // The staged parties differ from the finalized save party; Local
        // mode does not require equality with the anchored save.
        let party_a = [usable(7), party_record(8, 0, false)];
        let party_b = [usable(9), usable(10), party_record(11, 30, true)];
        let first = app
            .commit_battle_snapshot(a, group, battle, fa, local_commit(&party_a, 950))
            .expect("first local commit");
        assert!(first.manifest.is_none());
        // Exact replay is accepted without change.
        assert!(
            app.commit_battle_snapshot(a, group, battle, fa, local_commit(&party_a, 950))
                .is_ok()
        );
        let manifest = app
            .commit_battle_snapshot(b, group, battle, fb, local_commit(&party_b, 951))
            .expect("second local commit")
            .manifest
            .expect("manifest");
        assert_eq!(
            manifest.snapshot_hashes,
            [records_digest(&party_a), records_digest(&party_b)]
        );
        let for_a = app
            .battle_peer_party(a, group, battle, fa)
            .expect("a reads b");
        assert_eq!(for_a.peer_character_id, b.character_id);
        assert_eq!(for_a.snapshot_hash, records_digest(&party_b));
        assert_eq!(
            for_a.party_records,
            party_b.iter().map(|r| hex_string(r)).collect::<Vec<_>>()
        );
        let for_b = app
            .battle_peer_party(b, group, battle, fb)
            .expect("b reads a");
        assert_eq!(for_b.peer_character_id, a.character_id);
        assert_eq!(
            for_b.party_records,
            party_a.iter().map(|r| hex_string(r)).collect::<Vec<_>>()
        );
        // Ready receipts are bound to the staged digests.
        let view = release_start(&app, a, b, group, fa, fb, battle, &manifest);
        assert!(view.start_released);
    }

    #[test]
    fn local_commit_rejects_malformed_unusable_and_oversized_parties() {
        let (app, a, _, group, fa, _, battle) = local_accepted();
        let commit = |request| app.commit_battle_snapshot(a, group, battle, fa, request);

        let mut missing = local_commit(&[usable(1)], 960);
        missing.party_records = None;
        assert_eq!(commit(missing), Err(Phase2Error::InvalidRequest));

        let mut bad_checksum = usable(1);
        bad_checksum[28] ^= 0xff;
        assert_eq!(
            commit(local_commit(&[bad_checksum], 961)),
            Err(Phase2Error::InvalidRequest)
        );

        let mut short = local_commit(&[usable(1)], 962);
        short.party_records.as_mut().unwrap()[0].pop();
        assert_eq!(commit(short), Err(Phase2Error::InvalidRequest));

        let mut upper = local_commit(&[usable(10)], 963);
        let text = upper.party_records.as_mut().unwrap();
        text[0] = text[0].to_uppercase();
        assert_eq!(commit(upper), Err(Phase2Error::InvalidRequest));

        assert_eq!(
            commit(local_commit(&[[0_u8; 100]], 964)),
            Err(Phase2Error::InvalidRequest)
        );
        assert_eq!(
            commit(local_commit(
                &[party_record(1, 0, false), party_record(2, 20, true)],
                965
            )),
            Err(Phase2Error::InvalidRequest)
        );
        assert_eq!(
            commit(local_commit(
                &[usable(1), usable(2), usable(3), usable(4)],
                966
            )),
            Err(Phase2Error::InvalidRequest)
        );
        assert_eq!(
            commit(local_commit(&[party_record(1, 0, false); 7], 967)),
            Err(Phase2Error::InvalidRequest)
        );
        let mut wrong_hash = local_commit(&[usable(1)], 968);
        wrong_hash.snapshot_hash = records_digest(&[usable(2)]);
        assert_eq!(commit(wrong_hash), Err(Phase2Error::InvalidRequest));
        assert!(
            app.store
                .inspect_state(|state| {
                    let record = &state.battle_reservations[&battle];
                    record.consensus.commitments.iter().all(Option::is_none)
                        && record.staged_parties.iter().all(Option::is_none)
                })
                .unwrap()
        );
        // Three usable plus fainted and egg records is the upper bound.
        assert!(
            commit(local_commit(
                &[
                    usable(1),
                    usable(2),
                    usable(3),
                    party_record(4, 0, false),
                    party_record(5, 9, true),
                ],
                969
            ))
            .is_ok()
        );
    }

    #[test]
    fn ledger_battles_reject_staged_party_records() {
        let (app, a, _, group, fa, _, battle) = accepted();
        assert_eq!(
            app.commit_battle_snapshot(a, group, battle, fa, local_commit(&[usable(1)], 970)),
            Err(Phase2Error::InvalidRequest)
        );
    }

    #[test]
    fn local_win_completes_releases_locks_and_issues_no_grants() {
        let (app, a, b, group, fa, fb, battle) = local_accepted();
        app.commit_battle_snapshot(a, group, battle, fa, local_commit(&[usable(1)], 980))
            .expect("a commit");
        let manifest = app
            .commit_battle_snapshot(b, group, battle, fb, local_commit(&[usable(2)], 981))
            .expect("b commit")
            .manifest
            .expect("manifest");
        let hash = play_terminal_turn(&app, a, b, group, fa, fb, battle, &manifest, 982);
        let finish = |actor, fence, number| {
            app.finish_battle(
                actor,
                group,
                battle,
                fence,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(number),
                    result: BattleFinishResult::Won,
                    turn: 1,
                    state_hash: hash.clone(),
                },
            )
        };
        let first = finish(a, fa, 990).expect("first finish");
        assert_eq!(first.status, BattleReservationStatus::Accepted);
        let second = finish(b, fb, 991).expect("second finish");
        assert_eq!(second.status, BattleReservationStatus::Completed);
        assert_eq!(second.reward_mode, BattleRewardMode::Local);
        let (grants, locked) = app
            .store
            .inspect_state(|state| {
                (
                    state.battle_reservations[&battle].commit_grants.clone(),
                    state.active_battle_by_member.contains_key(&a.character_id)
                        || state.active_battle_by_member.contains_key(&b.character_id),
                )
            })
            .unwrap();
        assert!(grants.is_none());
        assert!(!locked);
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, battle, fa),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(app.current_battle(a, group, fa), Err(Phase2Error::NotFound));
        // Exact retry replays the terminal receipt.
        assert_eq!(
            finish(b, fb, 991).expect("replay").status,
            BattleReservationStatus::Completed
        );
        // Both members are free for a new battle.
        assert!(
            app.reserve_battle(a, group, fa, trainer_request(992, LOCAL_TRAINER))
                .is_ok()
        );
    }

    #[test]
    fn legacy_wally_ledger_keeps_its_unchanged_wire_shape() {
        let (app, a, _, group, fa, _) = fixture();
        let view = app
            .reserve_battle(a, group, fa, trainer_request(995, "HOENN:TRAINER_WALLY_1"))
            .expect("wally reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Ledger);
        assert_eq!(view.member_roles, None);
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("reward_mode").is_none());
        assert!(json.get("member_roles").is_none());

        let (_, _, _, _, _, _, reservation, grants) = pending_wally();
        assert_eq!(reservation.reward_mode, BattleRewardMode::Ledger);
        assert_eq!(grants.len(), 2);
    }

    const FLAG_DEFEATED_WALLY_VICTORY_ROAD: usize = 0x7e;
    const VAR_VICTORY_ROAD_1F_STATE: usize = 0x40c3;
    const VAR_ELITE_4_STATE: usize = 0x409c;
    const FLAG_DEFEATED_ELITE_4_SIDNEY: usize = 0x4fb;

    /// The production rule: Wally's Victory Road battle is a local-reward
    /// story battle (A9). Both members stand on Victory Road 1F.
    fn local_wally() -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        GroupId,
        LeaseFence,
        LeaseFence,
    ) {
        let fixture = fixture();
        LEGACY_WALLY_LEDGER.with(|legacy| legacy.set(false));
        fixture
    }

    #[test]
    fn wally_is_a_local_story_battle_with_a_story_point_partner_role() {
        for (partner_flag, partner_var, expected) in [
            (false, 0, BattleMemberRole::Participant), // same story point
            (false, 1, BattleMemberRole::Helper),      // already past it
            (true, 0, BattleMemberRole::Helper),       // flag already set
        ] {
            let (app, a, b, group, fa, _) = local_wally();
            // A lagging requester save (Wally already recorded) is not
            // refused: its own story script decides on the ROM.
            seed_finalized_story(
                &app,
                a,
                2,
                &[],
                0xff,
                &[(FLAG_DEFEATED_WALLY_VICTORY_ROAD, true)],
                &[(VAR_VICTORY_ROAD_1F_STATE, 1)],
            );
            seed_finalized_story(
                &app,
                b,
                2,
                &[],
                0xff,
                &[(FLAG_DEFEATED_WALLY_VICTORY_ROAD, partner_flag)],
                &[(VAR_VICTORY_ROAD_1F_STATE, partner_var)],
            );
            let view = app
                .reserve_battle(a, group, fa, trainer_request(1100, "HOENN:TRAINER_WALLY_1"))
                .expect("wally reserve");
            assert_eq!(view.reward_mode, BattleRewardMode::Local);
            assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
            assert_eq!(role_of(&view, b), expected, "{partner_flag} {partner_var}");
        }
    }

    #[test]
    fn wally_story_battle_keeps_the_exact_victory_road_map() {
        let (app, a, b, group, fa, _) = local_wally();
        place(&app, b, RegionId::Hoenn, "VICTORY_ROAD_B1F", 1);
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer_request(1110, "HOENN:TRAINER_WALLY_1")),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn elite_four_partner_participates_only_in_the_same_room_run() {
        for (partner_var, sidney_beaten, expected) in [
            (1, false, BattleMemberRole::Participant),
            (2, false, BattleMemberRole::Helper), // a room further
            (1, true, BattleMemberRole::Helper),  // already beat Sidney
        ] {
            let (app, a, b, group, fa, _) = local_fixture();
            place(&app, a, RegionId::Hoenn, "EVER_GRANDE_CITY_SIDNEYS_ROOM", 1);
            place(&app, b, RegionId::Hoenn, "EVER_GRANDE_CITY_SIDNEYS_ROOM", 1);
            seed_finalized_story(&app, a, 2, &[], 0xff, &[], &[(VAR_ELITE_4_STATE, 1)]);
            seed_finalized_story(
                &app,
                b,
                2,
                &[],
                0xff,
                &[(FLAG_DEFEATED_ELITE_4_SIDNEY, sidney_beaten)],
                &[(VAR_ELITE_4_STATE, partner_var)],
            );
            let view = app
                .reserve_battle(a, group, fa, trainer_request(1120, "HOENN:TRAINER_SIDNEY"))
                .expect("elite four reserve");
            assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
            assert_eq!(role_of(&view, b), expected, "{partner_var} {sidney_beaten}");
        }
    }

    #[test]
    fn helper_story_battles_never_make_the_partner_a_participant() {
        let (app, a, b, group, fa, _) = local_fixture();
        place(
            &app,
            a,
            RegionId::Hoenn,
            "EVER_GRANDE_CITY_CHAMPIONS_ROOM",
            1,
        );
        place(
            &app,
            b,
            RegionId::Hoenn,
            "EVER_GRANDE_CITY_CHAMPIONS_ROOM",
            1,
        );
        let view = app
            .reserve_battle(a, group, fa, trainer_request(1130, "HOENN:TRAINER_WALLACE"))
            .expect("champion reserve");
        assert_eq!(view.reward_mode, BattleRewardMode::Local);
        assert_eq!(role_of(&view, a), BattleMemberRole::Participant);
        assert_eq!(role_of(&view, b), BattleMemberRole::Helper);
        // An ordinary story entry (an admin whose script only shows text)
        // keeps the trainer-flag rule.
        let (app, a, b, group, fa, _) = local_fixture();
        let view = app
            .reserve_battle(
                a,
                group,
                fa,
                trainer_request(1131, "HOENN:TRAINER_TABITHA_MT_CHIMNEY"),
            )
            .expect("admin reserve");
        assert_eq!(role_of(&view, b), BattleMemberRole::Participant);
    }

    #[test]
    fn records_persisted_before_reward_mode_default_to_ledger() {
        let (app, a, _, group, fa, _) = local_fixture();
        let view = app
            .reserve_battle(a, group, fa, trainer_request(996, LOCAL_TRAINER))
            .unwrap();
        let record = app
            .store
            .inspect_state(|state| state.battle_reservations[&view.battle_id].clone())
            .unwrap();
        let mut json = serde_json::to_value(&record).unwrap();
        json.as_object_mut().unwrap().remove("staged_parties");
        let view_json = json["view"].as_object_mut().unwrap();
        view_json.remove("reward_mode");
        view_json.remove("member_roles");
        let legacy: BattleReservationRecord = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.view.reward_mode, BattleRewardMode::Ledger);
        assert_eq!(legacy.view.member_roles, None);
        assert_eq!(legacy.staged_parties, [None, None]);
    }

    #[test]
    fn wally_grants_are_ledger_entries_served_applied_and_voided() {
        use super::super::ledger::{ExpectedDelta, LedgerOrigin, LedgerStatus, TrainerStoryPolicy};
        let (app, a, b, group, fa, fb, reservation, grants) = pending_wally();
        let entries = app
            .store
            .inspect_state(|state| {
                grants.clone().map(|grant| {
                    (
                        state.ledger_entries[&grant.grant_id].clone(),
                        state
                            .ledger_open_by_character
                            .get(&grant.character_id)
                            .copied(),
                    )
                })
            })
            .expect("ledger");
        for ((entry, open), grant) in entries.iter().zip(&grants) {
            assert_eq!(*open, Some(grant.grant_id));
            assert_eq!(entry.character_id, grant.character_id);
            assert_eq!(
                entry.origin,
                LedgerOrigin::Battle {
                    battle_id: reservation.battle_id
                }
            );
            assert_eq!(entry.base_snapshot_id, grant.source_snapshot_id);
            assert_eq!(entry.status, LedgerStatus::Issued);
            let ExpectedDelta::TrainerWin {
                trainer_id, story, ..
            } = &entry.expected
            else {
                panic!("trainer delta");
            };
            assert_eq!(*trainer_id, grant.trainer_id);
            assert_eq!(*story, TrainerStoryPolicy::WallyVictoryRoad);
        }

        // The ledger route serves the same capability the grant route does.
        let view = super::super::ledger::open_for_character(&app.store, a, a.character_id, fa)
            .expect("open wally entry");
        assert_eq!(view.commit_id, grants[0].grant_id);
        assert_eq!(view.status, LedgerStatus::Delivered);
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, reservation.battle_id, fa),
            Ok(grants[0].clone())
        );

        let source = validated_fixture_save(&fixture_party_sav(), 1);
        let incoming = validated_fixture_save(&wally_story_save(true, true), 2);
        let request_a = finalize_request_for_grant(&app, a, grants[0].grant_id, 1_100);
        assert_eq!(
            app.store.write_transaction(|state| {
                super::super::ledger::apply_on_finalize(
                    state,
                    a,
                    &request_a,
                    &source,
                    &incoming,
                    app.store.now(),
                )
            }),
            Ok(true)
        );
        let applied = app
            .store
            .inspect_state(|state| {
                (
                    state.ledger_entries[&grants[0].grant_id].clone(),
                    state.ledger_open_by_character.contains_key(&a.character_id),
                )
            })
            .expect("applied");
        assert_eq!(applied.0.status, LedgerStatus::Applied);
        assert_eq!(applied.0.applied_snapshot_id, Some(request_a.snapshot_id));
        assert!(!applied.1);
        assert_eq!(
            super::super::ledger::open_for_character(&app.store, a, a.character_id, fa),
            Err(Phase2Error::NotFound)
        );

        // The unapplied member's entry is voided when the reservation ends.
        app.store
            .write_transaction(|state| {
                state
                    .battle_reservations
                    .get_mut(&reservation.battle_id)
                    .expect("reservation")
                    .expires_at = 0;
                Ok::<_, Phase2Error>(())
            })
            .expect("expire reservation");
        assert_eq!(
            super::super::ledger::open_for_character(&app.store, b, b.character_id, fb),
            Err(Phase2Error::NotFound)
        );
        prune_expired(&app.store).expect("prune");
        let voided = app
            .store
            .inspect_state(|state| {
                (
                    state.ledger_entries[&grants[1].grant_id].status,
                    state.ledger_open_by_character.contains_key(&b.character_id),
                )
            })
            .expect("voided");
        assert_eq!(voided, (LedgerStatus::Voided, false));
    }

    #[test]
    fn ledger_mode_reservation_is_refused_while_a_member_has_an_open_entry() {
        let (app, a, b, group, fa, _) = fixture();
        app.store
            .write_transaction(|state| {
                let commit_id = CommitId::new(Uuid::from_u128(0x1ed9)).expect("commit id");
                let snapshot_id = state.characters[&b.character_id]
                    .active_snapshot
                    .expect("snapshot");
                super::super::ledger::issue(
                    state,
                    vec![super::super::ledger::LedgerEntry {
                        commit_id,
                        character_id: b.character_id,
                        origin: super::super::ledger::LedgerOrigin::Trade {
                            offer_id: coop_cloud::TradeOfferId::new(Uuid::from_u128(0x1eda))
                                .expect("offer"),
                        },
                        base_snapshot_id: snapshot_id,
                        base_revision: Revision::new(1),
                        expected: super::super::ledger::ExpectedDelta::Trade {
                            slot: coop_cloud::PartyPosition::new(0).expect("slot"),
                            outgoing: super::super::ledger::PokemonKey {
                                personality: 0,
                                ot_id: 0,
                            },
                            incoming_raw: fixture_party_sav_record(),
                            incoming_key: super::super::ledger::PokemonKey {
                                personality: 0,
                                ot_id: 0,
                            },
                        },
                        status: super::super::ledger::LedgerStatus::Issued,
                        issued_at: 0,
                        applied_snapshot_id: None,
                    }],
                )
            })
            .expect("open entry");
        assert_eq!(
            app.reserve_battle(
                a,
                group,
                fa,
                trainer_request(1_200, "HOENN:TRAINER_WALLY_1")
            ),
            Err(Phase2Error::Conflict)
        );
    }
    fn friendly_rules(team_size: u8) -> FriendlyBattleRules {
        FriendlyBattleRules {
            format: coop_protocol::FriendlyBattleFormat::Singles,
            level_mode: coop_protocol::FriendlyLevelMode::Fifty,
            team_size,
        }
    }

    fn friendly_request(number: u128, rules: FriendlyBattleRules) -> BattleReservationRequest {
        BattleReservationRequest {
            api_version: ApiVersion::V1,
            kind: BattleReservationKind::Friendly,
            trainer_id: None,
            friendly_rules: Some(rules),
            idempotency_key: key(number),
        }
    }

    fn friendly_accepted(
        rules: FriendlyBattleRules,
    ) -> (
        Phase2App,
        AuthenticatedActor,
        AuthenticatedActor,
        GroupId,
        LeaseFence,
        LeaseFence,
        Uuid,
    ) {
        let (app, a, b, group, fa, fb) = fixture();
        let reservation = app
            .reserve_battle(a, group, fa, friendly_request(1_300, rules))
            .expect("friendly reserve");
        app.accept_battle(
            b,
            group,
            reservation.battle_id,
            fb,
            BattleReservationActionRequest {
                api_version: ApiVersion::V1,
                idempotency_key: key(1_301),
            },
        )
        .expect("accept");
        (app, a, b, group, fa, fb, reservation.battle_id)
    }

    #[test]
    fn friendly_challenge_reserves_with_no_rewards_roles_or_trainer() {
        let (app, a, b, group, fa, fb) = fixture();
        let rules = friendly_rules(3);
        let view = app
            .reserve_battle(a, group, fa, friendly_request(1_310, rules))
            .expect("reserve");
        assert_eq!(view.kind, BattleReservationKind::Friendly);
        assert_eq!(view.reward_mode, BattleRewardMode::None);
        assert_eq!(view.member_roles, None);
        assert_eq!(view.trainer_id, None);
        assert_eq!(view.friendly_rules, Some(rules));
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["reward_mode"], "NONE");
        assert_eq!(
            json["friendly_rules"],
            serde_json::json!({"format": "singles", "level_mode": "fifty", "team_size": 3})
        );
        assert!(json.get("member_roles").is_none());
        // The partner sees the same challenge before answering.
        let seen = app.current_battle(b, group, fb).expect("partner view");
        assert_eq!(seen.friendly_rules, Some(rules));
        // An exact retry replays; the same key with other rules conflicts.
        assert_eq!(
            app.reserve_battle(a, group, fa, friendly_request(1_310, rules)),
            Ok(view.clone())
        );
        assert_eq!(
            app.reserve_battle(a, group, fa, friendly_request(1_310, friendly_rules(4))),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn friendly_rules_are_validated_and_never_ride_a_trainer_reservation() {
        let (app, a, _, group, fa, _) = fixture();
        let mut trainer = trainer_request(1_320, "HOENN:TRAINER_WALLY_1");
        trainer.friendly_rules = Some(friendly_rules(2));
        assert_eq!(
            app.reserve_battle(a, group, fa, trainer),
            Err(Phase2Error::InvalidRequest)
        );
        for rules in [
            friendly_rules(0),
            friendly_rules(7),
            FriendlyBattleRules {
                format: coop_protocol::FriendlyBattleFormat::Doubles,
                level_mode: coop_protocol::FriendlyLevelMode::AsIs,
                team_size: 1,
            },
        ] {
            assert_eq!(
                app.reserve_battle(a, group, fa, friendly_request(1_321, rules)),
                Err(Phase2Error::InvalidRequest)
            );
        }
    }

    #[test]
    fn friendly_commit_stages_exactly_the_team_size_of_battle_ready_records() {
        let (app, a, b, group, fa, fb, battle) = friendly_accepted(friendly_rules(2));
        let commit = |actor, fence, request| {
            app.commit_battle_snapshot(actor, group, battle, fence, request)
        };
        let mut missing = local_commit(&[usable(1), usable(2)], 1_330);
        missing.party_records = None;
        assert_eq!(commit(a, fa, missing), Err(Phase2Error::InvalidRequest));
        // Wrong size, a fainted pick and an egg are refused.
        for (records, number) in [
            (vec![usable(1)], 1_331),
            (vec![usable(1), usable(2), usable(3)], 1_332),
            (vec![usable(1), party_record(2, 0, false)], 1_333),
            (vec![usable(1), party_record(2, 20, true)], 1_334),
        ] {
            assert_eq!(
                commit(a, fa, local_commit(&records, number)),
                Err(Phase2Error::InvalidRequest)
            );
        }
        let team_a = [usable(7), usable(8)];
        let team_b = [usable(9), usable(10)];
        assert!(commit(a, fa, local_commit(&team_a, 1_335)).is_ok());
        let manifest = commit(b, fb, local_commit(&team_b, 1_336))
            .expect("second commit")
            .manifest
            .expect("manifest");
        assert_eq!(
            manifest.snapshot_hashes,
            [records_digest(&team_a), records_digest(&team_b)]
        );
        // Each member reads the other's exact picks as its opponent side.
        let for_a = app
            .battle_peer_party(a, group, battle, fa)
            .expect("a reads b");
        assert_eq!(
            for_a.party_records,
            team_b.iter().map(|r| hex_string(r)).collect::<Vec<_>>()
        );
        let for_b = app
            .battle_peer_party(b, group, battle, fb)
            .expect("b reads a");
        assert_eq!(
            for_b.party_records,
            team_a.iter().map(|r| hex_string(r)).collect::<Vec<_>>()
        );
        assert!(release_start(&app, a, b, group, fa, fb, battle, &manifest).start_released);
    }

    #[test]
    fn friendly_result_completes_without_ledger_entries_grants_or_locks() {
        let (app, a, b, group, fa, fb, battle) = friendly_accepted(friendly_rules(1));
        app.commit_battle_snapshot(a, group, battle, fa, local_commit(&[usable(1)], 1_340))
            .expect("a commit");
        let manifest = app
            .commit_battle_snapshot(b, group, battle, fb, local_commit(&[usable(2)], 1_341))
            .expect("b commit")
            .manifest
            .expect("manifest");
        let hash = play_terminal_turn(&app, a, b, group, fa, fb, battle, &manifest, 1_342);
        let finish = |actor, fence, number, result| {
            app.finish_battle(
                actor,
                group,
                battle,
                fence,
                BattleFinishRequest {
                    api_version: ApiVersion::V1,
                    idempotency_key: key(number),
                    result,
                    turn: 1,
                    state_hash: hash.clone(),
                },
            )
        };
        // Trainer results never finish a friendly battle.
        assert_eq!(
            finish(a, fa, 1_349, BattleFinishResult::Won),
            Err(Phase2Error::InvalidRequest)
        );
        assert_eq!(
            finish(a, fa, 1_350, BattleFinishResult::Member1Won)
                .expect("first finish")
                .status,
            BattleReservationStatus::Accepted
        );
        let done = finish(b, fb, 1_351, BattleFinishResult::Member1Won).expect("second finish");
        assert_eq!(done.status, BattleReservationStatus::Completed);
        assert_eq!(done.reward_mode, BattleRewardMode::None);
        let (grants, open, locked) = app
            .store
            .inspect_state(|state| {
                (
                    state.battle_reservations[&battle].commit_grants.clone(),
                    super::super::ledger::has_open_entry(state, a.character_id)
                        || super::super::ledger::has_open_entry(state, b.character_id),
                    state.active_battle_by_member.contains_key(&a.character_id)
                        || state.active_battle_by_member.contains_key(&b.character_id),
                )
            })
            .unwrap();
        assert!(grants.is_none());
        assert!(!open);
        assert!(!locked);
        assert_eq!(
            retrieve_commit_grant(&app.store, a, group, battle, fa),
            Err(Phase2Error::Conflict)
        );
        // The pair is free for the next challenge.
        assert!(
            app.reserve_battle(b, group, fb, friendly_request(1_352, friendly_rules(6)))
                .is_ok()
        );
    }

    #[test]
    fn rounds_beyond_the_former_thirty_two_turn_cap_are_accepted() {
        let (app, a, b, group, fa, fb, battle) = friendly_accepted(friendly_rules(1));
        app.commit_battle_snapshot(a, group, battle, fa, local_commit(&[usable(1)], 1_360))
            .expect("a commit");
        let manifest = app
            .commit_battle_snapshot(b, group, battle, fb, local_commit(&[usable(2)], 1_361))
            .expect("b commit")
            .manifest
            .expect("manifest");
        release_start(&app, a, b, group, fa, fb, battle, &manifest);
        let mut number = 1_400;
        for turn in 1..=40_u16 {
            for (actor, fence) in [(a, fa), (b, fb)] {
                number += 1;
                app.submit_battle_action(
                    actor,
                    group,
                    battle,
                    fence,
                    BattleActionIntentRequest {
                        api_version: ApiVersion::V1,
                        idempotency_key: key(number),
                        turn,
                        action: "01000000".to_owned(),
                    },
                )
                .expect("action");
            }
            for (actor, fence) in [(a, fa), (b, fb)] {
                number += 1;
                app.acknowledge_battle_hash(
                    actor,
                    group,
                    battle,
                    fence,
                    BattleStateHashRequest {
                        api_version: ApiVersion::V1,
                        idempotency_key: key(number),
                        turn,
                        state_hash: "b".repeat(64),
                    },
                )
                .expect("hash");
            }
        }
        let view = app
            .inspect_battle_consensus(a, group, battle, fa)
            .expect("consensus");
        assert_eq!(view.turns.len(), 40);
    }
    #[test]
    fn both_rom_stage_kinds_fence_battle_reservation_for_either_member() {
        for paired in [false, true] {
            for staged in 0..2 {
                let (app, a, b, group, fa, fb) = fixture();
                let actors = [a, b];
                let fences = [fa, fb];
                app.store
                    .write_transaction(|state| {
                        super::super::storage::test_stage_handoff(
                            state,
                            actors[staged].character_id,
                            paired,
                            app.store.now(),
                        );
                        Ok::<_, Phase2Error>(())
                    })
                    .unwrap();
                let before = app
                    .store
                    .inspect_state(|state| {
                        let mut out = Vec::new();
                        ciborium::into_writer(state, &mut out).unwrap();
                        out
                    })
                    .unwrap();
                for caller in 0..2 {
                    assert!(matches!(
                        app.reserve_battle(
                            actors[caller],
                            group,
                            fences[caller],
                            BattleReservationRequest {
                                api_version: ApiVersion::V1,
                                kind: BattleReservationKind::Friendly,
                                trainer_id: None,
                                friendly_rules: None,
                                idempotency_key: key(9900 + caller as u128),
                            }
                        ),
                        Err(Phase2Error::Conflict)
                    ));
                }
                let after = app
                    .store
                    .inspect_state(|state| {
                        let mut out = Vec::new();
                        ciborium::into_writer(state, &mut out).unwrap();
                        out
                    })
                    .unwrap();
                assert_eq!(before, after);
            }
        }
    }
}

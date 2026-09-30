//! Level 1 multiplayer outcome ledger.
//!
//! One entry records one server-authorized outcome that a member's ROM must
//! apply and then declare in a finalized snapshot (`last_applied_commit`).
//! A character has at most one open (`Issued` or `Delivered`) entry, issuance
//! is idempotent per `(origin, character)`, and open trade entries never
//! expire. Wally's commit grants are ledger entries too; they are voided only
//! when their battle reservation ends without being applied, because the
//! reservation that anchors their evidence is gone.

use coop_cloud::{
    ApiVersion, CharacterId, CommitId, LeaseFence, PartyPosition, Revision,
    SnapshotFinalizeRequest, SnapshotId, TradeOfferId, UnixTimestampMillis,
};
use coop_protocol::TrainerInstanceId;
use coop_save::PokemonSlot;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::storage::{State, Store};
use super::{AuthenticatedActor, Phase2Error};

/// What produced a ledger entry. Issuance is idempotent per origin and
/// character.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
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
    pub fn is_open(self) -> bool {
        matches!(self, Self::Issued | Self::Delivered)
    }
}

/// Identifies one Pokémon across party and PC storage. Species is excluded so
/// an evolution still matches.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PokemonKey {
    pub personality: u32,
    pub ot_id: u32,
}

impl PokemonKey {
    fn of(identity: &coop_save::PokemonIdentity) -> Self {
        Self {
            personality: identity.personality,
            ot_id: identity.ot_id,
        }
    }
}

/// Story rules a trainer win carries besides its trainer bit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrainerStoryPolicy {
    None,
    /// The existing Wally Victory Road validator.
    WallyVictoryRoad,
}

/// The exact change the declaring snapshot must contain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpectedDelta {
    /// Some active party slot must hold exactly `incoming_raw` (lowercase hex
    /// on the wire, the same encoding as battle peer-party records) and
    /// `outgoing` must no longer be anywhere in party or PC boxes. `slot` is
    /// the party slot the trade was offered from; it is a hint for the ROM,
    /// which applies the record wherever the outgoing Pokémon now is.
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct LedgerEntry {
    pub commit_id: CommitId,
    pub character_id: CharacterId,
    pub origin: LedgerOrigin,
    pub base_snapshot_id: SnapshotId,
    pub base_revision: Revision,
    pub expected: ExpectedDelta,
    pub status: LedgerStatus,
    pub issued_at: u64,
    #[serde(default)]
    pub applied_snapshot_id: Option<SnapshotId>,
}

/// `GET /v1/characters/{character_id}/ledger/open` response.
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

mod hex_record {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        raw: &[u8; 100],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::super::battles::hex_string(raw))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 100], D::Error> {
        let text = String::deserialize(deserializer)?;
        super::super::battles::parse_hex_record(&text)
            .ok_or_else(|| serde::de::Error::custom("expected a 100-byte lowercase hex record"))
    }
}

/// The character's open entry, if any.
pub(crate) fn open_entry(state: &State, character_id: CharacterId) -> Option<&LedgerEntry> {
    let commit_id = state.ledger_open_by_character.get(&character_id)?;
    state
        .ledger_entries
        .get(commit_id)
        .filter(|entry| entry.character_id == character_id && entry.status.is_open())
}

pub(crate) fn has_open_entry(state: &State, character_id: CharacterId) -> bool {
    open_entry(state, character_id).is_some()
}

fn issued_for(state: &State, origin: LedgerOrigin, character_id: CharacterId) -> Option<CommitId> {
    state
        .ledger_entries
        .values()
        .find(|entry| entry.origin == origin && entry.character_id == character_id)
        .map(|entry| entry.commit_id)
}

/// Issues all `entries` atomically, returning each resulting commit ID in
/// order. An entry already issued for the same `(origin, character)` is
/// returned unchanged. Every check runs before the first insert, so a
/// refusal leaves the state untouched.
///
/// # Errors
///
/// `Conflict` when a character already has a different open entry, or two
/// new entries target the same character; `Internal` on an ID collision.
pub(crate) fn issue(
    state: &mut State,
    entries: Vec<LedgerEntry>,
) -> Result<Vec<CommitId>, Phase2Error> {
    let mut ids = Vec::with_capacity(entries.len());
    let mut fresh = Vec::new();
    for entry in entries {
        if let Some(existing) = issued_for(state, entry.origin, entry.character_id) {
            ids.push(existing);
            continue;
        }
        if has_open_entry(state, entry.character_id)
            || fresh
                .iter()
                .any(|other: &LedgerEntry| other.character_id == entry.character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        if state.ledger_entries.contains_key(&entry.commit_id)
            || fresh
                .iter()
                .any(|other: &LedgerEntry| other.commit_id == entry.commit_id)
        {
            return Err(Phase2Error::Internal);
        }
        ids.push(entry.commit_id);
        fresh.push(entry);
    }
    for entry in fresh {
        state
            .ledger_open_by_character
            .insert(entry.character_id, entry.commit_id);
        state.ledger_entries.insert(entry.commit_id, entry);
    }
    Ok(ids)
}

/// Whether a commit ID is already used by a ledger entry.
pub(crate) fn commit_id_in_use(state: &State, commit_id: CommitId) -> bool {
    state.ledger_entries.contains_key(&commit_id)
}

/// Checks that a declared commit ID may be applied now. A missing entry is
/// accepted here so pre-ledger Wally grants keep their existing validator.
///
/// # Errors
///
/// `Conflict` for another character's entry, an entry that is no longer open,
/// or an entry that is not the character's open one.
pub(crate) fn check_declarable(
    state: &State,
    character_id: CharacterId,
    commit_id: CommitId,
) -> Result<Option<&LedgerEntry>, Phase2Error> {
    let Some(entry) = state.ledger_entries.get(&commit_id) else {
        return Ok(None);
    };
    if entry.character_id != character_id
        || !entry.status.is_open()
        || state.ledger_open_by_character.get(&character_id) != Some(&commit_id)
    {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(entry))
}

/// Marks an open entry applied by `snapshot_id` and clears the open index.
pub(crate) fn mark_applied(state: &mut State, commit_id: CommitId, snapshot_id: SnapshotId) {
    let Some(entry) = state.ledger_entries.get_mut(&commit_id) else {
        return;
    };
    if !entry.status.is_open() {
        return;
    }
    entry.status = LedgerStatus::Applied;
    entry.applied_snapshot_id = Some(snapshot_id);
    let character_id = entry.character_id;
    if state.ledger_open_by_character.get(&character_id) == Some(&commit_id) {
        state.ledger_open_by_character.remove(&character_id);
    }
}

/// Voids the still-open entries of a battle whose reservation ended.
pub(crate) fn void_battle_entries(state: &mut State, battle_id: Uuid) {
    let origin = LedgerOrigin::Battle { battle_id };
    let voided: Vec<(CharacterId, CommitId)> = state
        .ledger_entries
        .values_mut()
        .filter(|entry| entry.origin == origin && entry.status.is_open())
        .map(|entry| {
            entry.status = LedgerStatus::Voided;
            (entry.character_id, entry.commit_id)
        })
        .collect();
    for (character_id, commit_id) in voided {
        if state.ledger_open_by_character.get(&character_id) == Some(&commit_id) {
            state.ledger_open_by_character.remove(&character_id);
        }
    }
}

/// Issues the two Wally commit grants as ledger entries, returning the grants
/// with their ledger commit IDs.
pub(crate) fn issue_battle_grants(
    state: &mut State,
    mut grants: [super::battles::BattleCommitGrant; 2],
    now: u64,
) -> Result<[super::battles::BattleCommitGrant; 2], Phase2Error> {
    let entries = grants
        .iter()
        .map(|grant| {
            let catalog = coop_protocol::identity_catalog::trainer(&grant.trainer_id)
                .map_err(|_| Phase2Error::Conflict)?;
            let trainer_ordinal = catalog.ordinal.ok_or(Phase2Error::Conflict)?;
            let story = if grant.trainer_id.as_str() == "HOENN:TRAINER_WALLY_1" {
                TrainerStoryPolicy::WallyVictoryRoad
            } else {
                TrainerStoryPolicy::None
            };
            Ok(LedgerEntry {
                commit_id: grant.grant_id,
                character_id: grant.character_id,
                origin: LedgerOrigin::Battle {
                    battle_id: grant.battle_id,
                },
                base_snapshot_id: grant.source_snapshot_id,
                base_revision: grant.source_revision,
                expected: ExpectedDelta::TrainerWin {
                    trainer_id: grant.trainer_id.clone(),
                    trainer_ordinal,
                    vanilla_trainer_id: catalog.legacy_value,
                    story,
                },
                status: LedgerStatus::Issued,
                issued_at: now,
                applied_snapshot_id: None,
            })
        })
        .collect::<Result<Vec<_>, Phase2Error>>()?;
    let ids = issue(state, entries)?;
    for (grant, commit_id) in grants.iter_mut().zip(ids) {
        grant.grant_id = commit_id;
    }
    Ok(grants)
}

/// One member's side of a consented trade, read from its anchored head.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TradeSide {
    pub snapshot_id: SnapshotId,
    pub revision: Revision,
    pub slot: PartyPosition,
    pub raw: [u8; 100],
    pub key: PokemonKey,
}

/// Reads the occupied slot record of one member's anchored save.
///
/// # Errors
///
/// `TradePokemonHoldsMail` when the record holds mail (the ROM refuses such a
/// TradeCommit, so the entry could never be applied); `Conflict` for an
/// empty, out-of-party or unreadable slot.
pub(crate) fn trade_side(
    save: &coop_save::ValidatedSave,
    snapshot_id: SnapshotId,
    revision: Revision,
    slot: PartyPosition,
) -> Result<TradeSide, Phase2Error> {
    if slot.index() >= usize::from(save.party_count().map_err(|_| Phase2Error::Conflict)?) {
        return Err(Phase2Error::Conflict);
    }
    match save
        .party_pokemon(slot.index())
        .map_err(|_| Phase2Error::Conflict)?
    {
        PokemonSlot::Occupied(record) if coop_save::party_record_holds_mail(&record) => {
            Err(Phase2Error::TradePokemonHoldsMail)
        }
        PokemonSlot::Occupied(record) => Ok(TradeSide {
            snapshot_id,
            revision,
            slot,
            raw: record.raw,
            key: PokemonKey::of(&record.identity),
        }),
        PokemonSlot::Empty { .. } => Err(Phase2Error::Conflict),
    }
}

/// Issues one Trade entry per member at the second consent. Each member
/// receives the partner's exact slot record.
pub(crate) fn issue_trade(
    state: &mut State,
    offer_id: TradeOfferId,
    members: [CharacterId; 2],
    sides: [TradeSide; 2],
    commit_ids: [CommitId; 2],
    now: u64,
) -> Result<[CommitId; 2], Phase2Error> {
    let entries = (0..2)
        .map(|index| {
            let own = sides[index];
            let partner = sides[1 - index];
            LedgerEntry {
                commit_id: commit_ids[index],
                character_id: members[index],
                origin: LedgerOrigin::Trade { offer_id },
                base_snapshot_id: own.snapshot_id,
                base_revision: own.revision,
                expected: ExpectedDelta::Trade {
                    slot: own.slot,
                    outgoing: own.key,
                    incoming_raw: partner.raw,
                    incoming_key: partner.key,
                },
                status: LedgerStatus::Issued,
                issued_at: now,
                applied_snapshot_id: None,
            }
        })
        .collect();
    let ids = issue(state, entries)?;
    Ok([ids[0], ids[1]])
}

/// Whether any active party slot holds exactly `raw`. The entry's slot is
/// only where the trade was offered from: the player may reorder the party
/// before the ROM applies the commit, and the ROM then writes the record into
/// whichever slot holds the outgoing Pokémon.
fn party_holds_record(
    save: &coop_save::ValidatedSave,
    raw: &[u8; 100],
) -> Result<bool, Phase2Error> {
    let count = usize::from(save.party_count().map_err(|_| Phase2Error::Conflict)?);
    for index in 0..count {
        if let PokemonSlot::Occupied(record) = save
            .party_pokemon(index)
            .map_err(|_| Phase2Error::Conflict)?
            && record.raw == *raw
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn holds(save: &coop_save::ValidatedSave, key: PokemonKey) -> Result<bool, Phase2Error> {
    Ok(!save
        .locate_pokemon(key.personality, key.ot_id)
        .map_err(|_| Phase2Error::Conflict)?
        .is_empty())
}

/// Validates the snapshot being finalized against the ledger and, for a
/// declared entry, applies it in the caller's finalize transaction.
/// Returns whether a ledger entry (or legacy Wally grant) was applied.
///
/// Every check runs before any mutation.
///
/// # Errors
///
/// `Conflict` for a declared entry whose delta does not match exactly or that
/// is not the character's open entry; `Forbidden` for an undeclared outcome
/// whose evidence is already present, or an unknown commit ID. A trade's
/// evidence is the incoming Pokémon (by personality and OT ID) anywhere in
/// party or boxes; a declared trade needs its exact record in any party slot
/// and the outgoing Pokémon gone from party and boxes.
pub(crate) fn apply_on_finalize(
    state: &mut State,
    actor: AuthenticatedActor,
    request: &SnapshotFinalizeRequest,
    source_save: &coop_save::ValidatedSave,
    incoming_save: &coop_save::ValidatedSave,
    now: u64,
) -> Result<bool, Phase2Error> {
    let declared = request.last_applied_commit;
    let trade = match declared {
        Some(commit_id) => match check_declarable(state, actor.character_id, commit_id)? {
            Some(entry) => match &entry.expected {
                ExpectedDelta::Trade {
                    outgoing,
                    incoming_raw,
                    ..
                } => Some((commit_id, *outgoing, *incoming_raw)),
                // The Wally validator checks and marks the ledger entry.
                ExpectedDelta::TrainerWin { .. } => None,
            },
            None => None,
        },
        None => None,
    };
    let Some((commit_id, outgoing, incoming_raw)) = trade else {
        let applied = super::battles::apply_commit_grant(
            state,
            actor,
            request,
            source_save,
            incoming_save,
            now,
        )?;
        if declared.is_none()
            && let Some(entry) = open_entry(state, actor.character_id)
            && let ExpectedDelta::Trade { incoming_key, .. } = &entry.expected
            && holds(incoming_save, *incoming_key)?
        {
            return Err(Phase2Error::Forbidden);
        }
        return Ok(applied);
    };

    // An undeclared Wally story change cannot ride along on a trade.
    let undeclared = SnapshotFinalizeRequest {
        last_applied_commit: None,
        ..request.clone()
    };
    super::battles::apply_commit_grant(state, actor, &undeclared, source_save, incoming_save, now)?;
    if !holds(source_save, outgoing)? || holds(incoming_save, outgoing)? {
        return Err(Phase2Error::Conflict);
    }
    if !party_holds_record(incoming_save, &incoming_raw)? {
        return Err(Phase2Error::Conflict);
    }
    mark_applied(state, commit_id, request.snapshot_id);
    Ok(true)
}

/// Serves the caller's open entry under its exact lease fence and records
/// delivery. A Wally entry whose battle can no longer be applied is hidden.
pub(crate) fn open_for_character(
    store: &Store,
    actor: AuthenticatedActor,
    character_id: CharacterId,
    fence: LeaseFence,
) -> Result<LedgerEntryView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, character_id)?;
        super::group_travel::lease_matches(state, character_id, fence, now)?;
        let entry = open_entry(state, character_id).ok_or(Phase2Error::NotFound)?;
        if let LedgerOrigin::Battle { battle_id } = entry.origin
            && !super::battles::grant_is_live(state, battle_id, now)
        {
            return Err(Phase2Error::NotFound);
        }
        let commit_id = entry.commit_id;
        let issued_at = Store::unix_timestamp(entry.issued_at).map_err(Phase2Error::from)?;
        let entry = state
            .ledger_entries
            .get_mut(&commit_id)
            .ok_or(Phase2Error::Internal)?;
        entry.status = LedgerStatus::Delivered;
        Ok(LedgerEntryView {
            api_version: ApiVersion::V1,
            commit_id,
            character_id,
            origin: entry.origin,
            base_snapshot_id: entry.base_snapshot_id,
            base_revision: entry.base_revision,
            expected: entry.expected.clone(),
            status: entry.status,
            issued_at,
        })
    })
}

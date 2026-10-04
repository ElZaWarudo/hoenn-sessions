//! One-time, operator-invoked fresh start of the pilot checkpoint.
//!
//! Production checkpoints written by older server builds cannot be decoded by
//! this build (for example, persisted snapshot records predate the required
//! `rom_world_id`). The product decision for those players is a fresh start:
//! accounts, invitations and (by default) login sessions survive, while every
//! character is reset to a new revision-0 campaign. Nothing here runs on
//! server startup. Only a checkpoint this build cannot decode and whose raw
//! CBOR positively matches origin/main's persisted schema is transformed;
//! anything else (corruption, a newer build's checkpoint, a decodable state)
//! is refused. The command is gated by the server's advisory lock, by an
//! exact SHA-256 of the payload the operator inspected, and archives the
//! original bytes in the same transaction that replaces them. It never talks
//! to the object store: legacy snapshot objects are left untouched and their
//! IDs are tombstoned so they can never be adopted by a new prepare.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};

use ciborium::Value;
use coop_cloud::{CharacterId, Revision, SnapshotId, UserId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::persistent::{ADVISORY_LOCK_SQL, MAX_STATE_BYTES, decode_state, encode};
use super::storage::{CharacterRecord, MAX_RETIRED_SNAPSHOTS, State, Store};

/// Reason recorded on every archive row written by this command.
pub const ARCHIVE_REASON: &str = "fresh-start";
/// Additive DDL for the archive table, executed inside the maintenance transaction.
pub(crate) const MIGRATION_SQL: &str = include_str!("../../migrations/0004_checkpoint_archive.sql");
pub(crate) const UNLOCK_SQL: &str = "SELECT pg_advisory_unlock(1129271120, 1)";
pub(crate) const CHECKPOINT_TABLE_SQL: &str =
    "SELECT to_regclass('coop_pilot_checkpoint') IS NOT NULL";
pub(crate) const SELECT_FOR_UPDATE_SQL: &str =
    "SELECT format_version, payload FROM coop_pilot_checkpoint WHERE id = 1 FOR UPDATE";
pub(crate) const ARCHIVE_EXISTS_SQL: &str =
    "SELECT EXISTS (SELECT 1 FROM coop_pilot_checkpoint_archive WHERE payload_sha256 = $1)";
pub(crate) const ARCHIVE_INSERT_SQL: &str = "INSERT INTO coop_pilot_checkpoint_archive (reason, format_version, payload, payload_sha256) VALUES ($1, $2, $3, $4)";
pub(crate) const UPDATE_SQL: &str = "UPDATE coop_pilot_checkpoint SET payload = $1, updated_at = now() WHERE id = 1 AND format_version = 1";

/// Every top-level `State` field persisted by origin/main
/// (`git show 17153772bc:coop/crates/coop-server/src/phase2/storage.rs`), the
/// last pre-multi-world build. A checkpoint is transformed only when its root
/// uses a subset of these keys.
pub(crate) const ORIGIN_MAIN_STATE_KEYS: [&str; 49] = [
    "users_by_name",
    "users_by_id",
    "characters",
    "invitations",
    "invitation_issuers",
    "invitation_expires_at",
    "access",
    "refresh",
    "families",
    "leases",
    "acquire_history",
    "prepared",
    "prepare_ops",
    "snapshots",
    "snapshot_by_revision",
    "finalize_ops",
    "restore_ops",
    "restore_staging",
    "retired_snapshots",
    "tickets",
    "realtime_tickets",
    "realtime_ticket_families",
    "realtime_by_runtime",
    "upload_objects",
    "groups",
    "active_group_by_member",
    "group_end_notices",
    "last_group_by_member",
    "last_seen_at",
    "group_invitations",
    "group_pairing_codes",
    "group_pairing_code_issuances",
    "group_pairing_code_attempts",
    "group_idempotency",
    "group_travel_proposals",
    "live_group_travel_by_group",
    "live_group_travel_by_member",
    "group_travel_proposal_idempotency",
    "trade_offers",
    "trade_offer_idempotency",
    "trade_staging",
    "trade_receipts",
    "group_member_world_zones",
    "group_progress_feeds",
    "battle_reservations",
    "active_battle_by_member",
    "battle_idempotency",
    "ledger_entries",
    "ledger_open_by_character",
];
/// The origin/main `State` fields without `#[serde(default)]`: origin/main
/// itself cannot load a checkpoint that lacks one, so every checkpoint the
/// production server has written carries all of them.
pub(crate) const ORIGIN_MAIN_REQUIRED_KEYS: [&str; 25] = [
    "users_by_name",
    "users_by_id",
    "characters",
    "invitations",
    "access",
    "refresh",
    "families",
    "leases",
    "acquire_history",
    "prepared",
    "prepare_ops",
    "snapshots",
    "snapshot_by_revision",
    "finalize_ops",
    "restore_ops",
    "restore_staging",
    "retired_snapshots",
    "tickets",
    "realtime_tickets",
    "realtime_by_runtime",
    "upload_objects",
    "groups",
    "active_group_by_member",
    "group_invitations",
    "group_idempotency",
];
/// Snapshot-bearing maps in which origin/main records never carry the
/// multi-world `rom_world_id` field.
const ORIGIN_MAIN_SNAPSHOT_MAPS: [&str; 6] = [
    "prepared",
    "prepare_ops",
    "snapshots",
    "finalize_ops",
    "restore_ops",
    "restore_staging",
];
/// Upper bound for the rebuilt `retired_snapshots` set. Half of the server's
/// non-evicting replay cache stays free for tombstones written after the
/// fresh start.
pub const TOMBSTONE_HEADROOM_BOUND: usize = MAX_RETIRED_SNAPSHOTS / 2;

/// Top-level checkpoint keys that survive a fresh start verbatim. Every other
/// key, including keys this build does not know, is dropped.
const KEEP_ALWAYS: [&str; 5] = [
    "users_by_name",
    "users_by_id",
    "invitations",
    "invitation_issuers",
    "invitation_expires_at",
];
/// Login-session maps kept unless `--drop-sessions` is supplied.
const KEEP_SESSIONS: [&str; 3] = ["access", "refresh", "families"];
/// Keys rebuilt by the transform rather than copied.
const REBUILT: [&str; 2] = ["characters", "retired_snapshots"];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FreshStartError {
    #[error(
        "usage: coop-server fresh-start --expect-sha256 <64 hex> [--drop-sessions] [--dry-run] [--allow-tombstone-overflow]"
    )]
    Usage,
    #[error("--expect-sha256 must be exactly 64 hexadecimal characters")]
    InvalidSha256,
    #[error("COOP_DATABASE_URL_FILE is missing, unreadable, or not a postgres URL")]
    DatabaseConfiguration,
    #[error("database error: {0}")]
    Database(String),
    #[error("the co-op server advisory lock is held: stop coop-server before running fresh-start")]
    LockHeld,
    #[error("coop_pilot_checkpoint has no row id=1; there is nothing to fresh-start")]
    NoCheckpoint,
    #[error("unsupported checkpoint format_version {0}; expected 1")]
    UnsupportedFormat(i32),
    #[error("checkpoint payload exceeds the 32 MiB limit")]
    TooLarge,
    #[error(
        "checkpoint sha256 is {actual}, not the expected value; re-inspect the payload before retrying"
    )]
    ShaMismatch { actual: String },
    #[error(
        "the expected payload is already archived but the current checkpoint is not fresh (was the archive restored?); refusing"
    )]
    ArchivedButNotFresh,
    #[error(
        "checkpoint does not decode with this build and is not a recognised origin/main (pre-multi-world) checkpoint: {0}; nothing was written. It may be corrupt or written by a newer build: restore from backup or escalate"
    )]
    NotOriginMainCheckpoint(String),
    #[error(
        "checkpoint decodes with this build but {0} character(s) cannot resume; fresh-start never rewrites a current-format checkpoint: investigate"
    )]
    CurrentFormatWithLegacyCharacters(usize),
    #[error(
        "{required} in-flight and head snapshot IDs exceed the {bound}-entry tombstone headroom; review the dry-run report and rerun with --allow-tombstone-overflow only if accepted"
    )]
    TombstoneOverflow { required: usize, bound: usize },
    #[error("legacy checkpoint is malformed: {0}")]
    Malformed(String),
    #[error("transformed checkpoint failed its self-check: {0}")]
    SelfCheck(String),
}

/// Parsed command-line options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshStartOptions {
    pub expected_sha256: [u8; 32],
    pub drop_sessions: bool,
    pub dry_run: bool,
    /// Lets in-flight and head tombstones exceed [`TOMBSTONE_HEADROOM_BOUND`]
    /// (up to the server's full cache), after operator review.
    pub allow_tombstone_overflow: bool,
}

/// What a checkpoint inspection decided, before any write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    AlreadyFresh(&'static str),
    Transform,
}

/// Entry counts for the collections an operator is expected to compare.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub users: usize,
    pub characters: usize,
    pub snapshots: usize,
    pub prepared_snapshots: usize,
    pub ledger_entries: usize,
    pub groups: usize,
    pub trade_offers: usize,
    pub battle_reservations: usize,
    pub access_tokens: usize,
    pub refresh_tokens: usize,
    pub token_families: usize,
    pub retired_snapshots: usize,
}

/// JSON report printed by the command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FreshStartReport {
    /// `fresh_started`, `dry_run`, or `already_fresh`.
    pub outcome: &'static str,
    pub reason: Option<&'static str>,
    pub drop_sessions: bool,
    pub current_payload_sha256: String,
    pub before: Option<Counts>,
    pub after: Option<Counts>,
    pub characters_at_revision_zero: usize,
    pub characters_without_owner: usize,
    /// Legacy snapshot IDs newly tombstoned (not already retired).
    pub retired_snapshots_added: usize,
    /// In-flight (prepared, prepare_ops, restore_staging) and active-head IDs.
    pub retired_snapshots_in_flight_and_heads: usize,
    /// Size of the rebuilt `retired_snapshots` set.
    pub retired_snapshots_final: usize,
    /// Bound applied to the rebuilt set (512 unless overflow was allowed).
    pub retired_snapshots_bound: usize,
    /// Free replay-cache entries left for the new build (1,024 minus final).
    pub retired_snapshots_headroom: usize,
    /// Legacy IDs (history or old tombstones) that did not fit the bound.
    pub retired_snapshots_overflow: usize,
    pub unparsable_snapshot_ids: usize,
    /// Dropped top-level keys with their entry counts in the original payload.
    pub dropped_keys: BTreeMap<String, usize>,
    pub archived_payload_sha256: Option<String>,
    pub new_payload_sha256: Option<String>,
    pub archived_payload_bytes: usize,
    pub new_payload_bytes: usize,
}

/// Result of the pure transform.
pub(crate) struct Transformed {
    pub payload: Vec<u8>,
    pub report: FreshStartReport,
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Parses a 64-character hexadecimal SHA-256.
///
/// # Errors
/// Returns [`FreshStartError::InvalidSha256`] for any other input.
pub fn parse_sha256(text: &str) -> Result<[u8; 32], FreshStartError> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return Err(FreshStartError::InvalidSha256);
    }
    let mut output = [0_u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or(FreshStartError::InvalidSha256)?;
        let low = hex_digit(pair[1]).ok_or(FreshStartError::InvalidSha256)?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Parses the arguments following `fresh-start`.
///
/// # Errors
/// Returns a usage error for missing, repeated, or unknown options.
pub fn parse_args(arguments: &[String]) -> Result<FreshStartOptions, FreshStartError> {
    let mut expected = None;
    let mut drop_sessions = false;
    let mut dry_run = false;
    let mut allow_tombstone_overflow = false;
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--expect-sha256" => {
                let value = iterator.next().ok_or(FreshStartError::Usage)?;
                if expected.replace(parse_sha256(value)?).is_some() {
                    return Err(FreshStartError::Usage);
                }
            }
            "--drop-sessions" if !drop_sessions => drop_sessions = true,
            "--dry-run" if !dry_run => dry_run = true,
            "--allow-tombstone-overflow" if !allow_tombstone_overflow => {
                allow_tombstone_overflow = true;
            }
            _ => return Err(FreshStartError::Usage),
        }
    }
    Ok(FreshStartOptions {
        expected_sha256: expected.ok_or(FreshStartError::Usage)?,
        drop_sessions,
        dry_run,
        allow_tombstone_overflow,
    })
}

/// A character is legacy when this build's authoritative-world resolution
/// (`sessions::authoritative_world`) cannot use it without a catalog lookup:
/// a revision-0 character must have no heads, and a later revision must be
/// backed by a live snapshot that is also its world head.
pub(crate) fn is_legacy_character(
    state: &State,
    character_id: CharacterId,
    record: &CharacterRecord,
) -> bool {
    if record.revision == Revision::initial() {
        return record.active_snapshot.is_some() || !record.world_heads.is_empty();
    }
    let Some(snapshot_id) = record.active_snapshot else {
        return true;
    };
    let Some(snapshot) = state.snapshots.get(&snapshot_id) else {
        return true;
    };
    snapshot.character_id != character_id
        || snapshot.revision != record.revision
        || record.world_heads.get(&snapshot.rom_world_id) != Some(&snapshot_id)
}

pub(crate) fn legacy_character_count(state: &State) -> usize {
    state
        .characters
        .iter()
        .filter(|(id, record)| is_legacy_character(state, **id, record))
        .count()
}

/// Decides, without writing, whether the inspected checkpoint is transformed.
///
/// `expected_archived` reports whether the archive already holds a payload
/// with the operator's expected SHA-256 (a completed earlier run). Only a
/// checkpoint that this build cannot decode *and* that positively matches the
/// origin/main schema is ever transformed; a checkpoint this build decodes is
/// never rewritten.
///
/// # Errors
/// Fails for an unsupported format, oversized payload, digest mismatch, a
/// checkpoint that matches an archived original but is not fresh, a decodable
/// checkpoint with characters that cannot resume, or a payload that is not a
/// recognised origin/main checkpoint (corrupt, or from a newer build).
pub(crate) fn decide(
    format_version: i32,
    payload: &[u8],
    expected_sha256: &[u8; 32],
    expected_archived: bool,
) -> Result<Decision, FreshStartError> {
    if format_version != 1 {
        return Err(FreshStartError::UnsupportedFormat(format_version));
    }
    if payload.len() > MAX_STATE_BYTES {
        return Err(FreshStartError::TooLarge);
    }
    let legacy_characters = decode_state(payload)
        .ok()
        .map(|state| legacy_character_count(&state));
    if expected_archived {
        return if legacy_characters == Some(0) {
            Ok(Decision::AlreadyFresh(
                "the expected payload is already archived and the checkpoint has no legacy characters",
            ))
        } else {
            Err(FreshStartError::ArchivedButNotFresh)
        };
    }
    let actual = sha256(payload);
    if &actual != expected_sha256 {
        return Err(FreshStartError::ShaMismatch {
            actual: hex(&actual),
        });
    }
    match legacy_characters {
        Some(0) => Ok(Decision::AlreadyFresh(
            "the checkpoint already decodes with this build and has no legacy characters",
        )),
        Some(count) => Err(FreshStartError::CurrentFormatWithLegacyCharacters(count)),
        None => {
            parse_origin_main_root(payload)?;
            Ok(Decision::Transform)
        }
    }
}

fn not_origin_main(message: impl Into<String>) -> FreshStartError {
    FreshStartError::NotOriginMainCheckpoint(message.into())
}

fn contains_text_key(value: &Value, name: &str) -> bool {
    match value {
        Value::Map(entries) => entries.iter().any(|(key, value)| {
            matches!(key, Value::Text(text) if text == name)
                || contains_text_key(key, name)
                || contains_text_key(value, name)
        }),
        Value::Array(items) => items.iter().any(|item| contains_text_key(item, name)),
        Value::Tag(_, inner) => contains_text_key(inner, name),
        _ => false,
    }
}

/// Parses the raw CBOR root and proves it has origin/main's persisted shape:
/// a text-keyed map whose keys are a subset of [`ORIGIN_MAIN_STATE_KEYS`] that
/// includes every [`ORIGIN_MAIN_REQUIRED_KEYS`] entry, characters without
/// `world_heads`, and snapshot/prepare records without `rom_world_id`.
///
/// # Errors
/// Returns [`FreshStartError::NotOriginMainCheckpoint`] otherwise.
pub(crate) fn parse_origin_main_root(
    payload: &[u8],
) -> Result<Vec<(String, Value)>, FreshStartError> {
    let mut reader = payload;
    let value: Value = ciborium::from_reader(&mut reader)
        .map_err(|_| not_origin_main("payload is not a single CBOR value"))?;
    if !reader.is_empty() {
        return Err(not_origin_main("payload has trailing bytes"));
    }
    let root = top_level(value).map_err(|error| not_origin_main(error.to_string()))?;
    let unknown: Vec<&str> = root
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| !ORIGIN_MAIN_STATE_KEYS.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err(not_origin_main(format!(
            "top-level keys unknown to origin/main: {}",
            unknown.join(", ")
        )));
    }
    let missing: Vec<&str> = ORIGIN_MAIN_REQUIRED_KEYS
        .iter()
        .copied()
        .filter(|key| get(&root, key).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(not_origin_main(format!(
            "required origin/main keys are missing: {}",
            missing.join(", ")
        )));
    }
    let Some(Value::Map(characters)) = get(&root, "characters") else {
        return Err(not_origin_main("characters is not a map"));
    };
    if characters
        .iter()
        .any(|(_, record)| field(record, "world_heads").is_some())
    {
        return Err(not_origin_main("a character already has world_heads"));
    }
    for key in ORIGIN_MAIN_SNAPSHOT_MAPS {
        if get(&root, key).is_some_and(|value| contains_text_key(value, "rom_world_id")) {
            return Err(not_origin_main(format!(
                "{key} holds a record with rom_world_id"
            )));
        }
    }
    Ok(root)
}

fn malformed(message: impl Into<String>) -> FreshStartError {
    FreshStartError::Malformed(message.into())
}

fn entries(value: &Value) -> usize {
    match value {
        Value::Map(entries) => entries.len(),
        Value::Array(items) => items.len(),
        Value::Null => 0,
        _ => 1,
    }
}

fn top_level(value: Value) -> Result<Vec<(String, Value)>, FreshStartError> {
    let Value::Map(entries) = value else {
        return Err(malformed("checkpoint root is not a CBOR map"));
    };
    let mut output = Vec::with_capacity(entries.len());
    let mut seen = BTreeSet::new();
    for (key, value) in entries {
        let Value::Text(key) = key else {
            return Err(malformed("checkpoint root has a non-text key"));
        };
        if !seen.insert(key.clone()) {
            return Err(malformed(format!("duplicate checkpoint key {key}")));
        }
        output.push((key, value));
    }
    Ok(output)
}

fn get<'a>(root: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    root.iter()
        .find_map(|(name, value)| (name == key).then_some(value))
}

fn map_entries<'a>(
    value: Option<&'a Value>,
    key: &str,
) -> Result<&'a [(Value, Value)], FreshStartError> {
    match value {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Map(entries)) => Ok(entries),
        Some(_) => Err(malformed(format!("{key} is not a map"))),
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match value {
        Value::Map(entries) => entries.iter().find_map(|(key, value)| {
            matches!(key, Value::Text(text) if text == name).then_some(value)
        }),
        _ => None,
    }
}

#[derive(serde::Deserialize)]
struct LegacyCharacterIdentity {
    owner: UserId,
    last_session_epoch: u32,
}

#[derive(serde::Deserialize)]
struct LegacyUser {
    user_id: UserId,
    password_phc: String,
}

/// Snapshot IDs referenced by the legacy checkpoint, deduplicated in the
/// order they were pushed.
struct SnapshotIdCollector {
    ordered: Vec<SnapshotId>,
    seen: BTreeSet<SnapshotId>,
    unparsable: usize,
}

impl SnapshotIdCollector {
    fn new() -> Self {
        Self {
            ordered: Vec::new(),
            seen: BTreeSet::new(),
            unparsable: 0,
        }
    }

    fn push(&mut self, value: &Value) {
        if matches!(value, Value::Null) {
            return;
        }
        match value.deserialized::<SnapshotId>() {
            Ok(id) => {
                if self.seen.insert(id) {
                    self.ordered.push(id);
                }
            }
            Err(_) => self.unparsable += 1,
        }
    }

    fn keys(&mut self, root: &[(String, Value)], key: &str) {
        if let Some(Value::Map(entries)) = get(root, key) {
            for (id, _) in entries {
                self.push(id);
            }
        }
    }

    fn values(&mut self, root: &[(String, Value)], key: &str) {
        if let Some(Value::Map(entries)) = get(root, key) {
            for (_, id) in entries {
                self.push(id);
            }
        }
    }

    fn value_fields(&mut self, root: &[(String, Value)], key: &str, name: &str) {
        if let Some(Value::Map(entries)) = get(root, key) {
            for (_, record) in entries {
                if let Some(id) = field(record, name) {
                    self.push(id);
                }
            }
        }
    }

    fn items(&mut self, root: &[(String, Value)], key: &str) {
        if let Some(Value::Array(items)) = get(root, key) {
            for id in items {
                self.push(id);
            }
        }
    }
}

struct CollectedSnapshotIds {
    /// In-flight work a client journal may retry, then active heads.
    priority: Vec<SnapshotId>,
    /// Committed history, then tombstones already present in the checkpoint.
    rest: Vec<SnapshotId>,
    unparsable: usize,
}

fn collect_snapshot_ids(root: &[(String, Value)]) -> CollectedSnapshotIds {
    let mut collector = SnapshotIdCollector::new();
    collector.keys(root, "prepared");
    collector.values(root, "prepare_ops");
    collector.value_fields(root, "restore_staging", "snapshot_id");
    collector.value_fields(root, "characters", "active_snapshot");
    let priority_len = collector.ordered.len();
    collector.values(root, "snapshot_by_revision");
    collector.keys(root, "snapshots");
    collector.items(root, "retired_snapshots");
    let rest = collector.ordered.split_off(priority_len);
    CollectedSnapshotIds {
        priority: collector.ordered,
        rest,
        unparsable: collector.unparsable,
    }
}

fn counts(root: &[(String, Value)]) -> Counts {
    let count = |key: &str| get(root, key).map_or(0, entries);
    Counts {
        users: count("users_by_id"),
        characters: count("characters"),
        snapshots: count("snapshots"),
        prepared_snapshots: count("prepared"),
        ledger_entries: count("ledger_entries"),
        groups: count("groups"),
        trade_offers: count("trade_offers"),
        battle_reservations: count("battle_reservations"),
        access_tokens: count("access"),
        refresh_tokens: count("refresh"),
        token_families: count("families"),
        retired_snapshots: count("retired_snapshots"),
    }
}

fn typed_counts(state: &State) -> Counts {
    Counts {
        users: state.users_by_id.len(),
        characters: state.characters.len(),
        snapshots: state.snapshots.len(),
        prepared_snapshots: state.prepared.len(),
        ledger_entries: state.ledger_entries.len(),
        groups: state.groups.len(),
        trade_offers: state.trade_offers.len(),
        battle_reservations: state.battle_reservations.len(),
        access_tokens: state.access.len(),
        refresh_tokens: state.refresh.len(),
        token_families: state.families.len(),
        retired_snapshots: state.retired_snapshots.len(),
    }
}

fn serialized<T: Serialize>(value: &T) -> Result<Value, FreshStartError> {
    Value::serialized(value).map_err(|error| FreshStartError::SelfCheck(error.to_string()))
}

fn rebuild_characters(
    root: &[(String, Value)],
) -> Result<(Value, BTreeMap<CharacterId, u32>), FreshStartError> {
    let mut rebuilt = Vec::new();
    let mut epochs = BTreeMap::new();
    for (key, record) in map_entries(get(root, "characters"), "characters")? {
        let character_id = key
            .deserialized::<CharacterId>()
            .map_err(|_| malformed("character key is not a character ID"))?;
        let identity = record
            .deserialized::<LegacyCharacterIdentity>()
            .map_err(|_| malformed(format!("character {character_id} lacks owner or epoch")))?;
        let state = Store::initial_state(character_id)
            .map_err(|_| FreshStartError::SelfCheck("initial character state".into()))?;
        let fresh = CharacterRecord {
            owner: identity.owner,
            state,
            revision: Revision::initial(),
            world_revision: 0,
            active_snapshot: None,
            world_heads: BTreeMap::new(),
            last_session_epoch: identity.last_session_epoch,
        };
        if epochs
            .insert(character_id, identity.last_session_epoch)
            .is_some()
        {
            return Err(malformed(format!("duplicate character {character_id}")));
        }
        rebuilt.push((key.clone(), serialized(&fresh)?));
    }
    Ok((Value::Map(rebuilt), epochs))
}

fn legacy_password_hashes(
    root: &[(String, Value)],
) -> Result<BTreeMap<String, String>, FreshStartError> {
    let mut hashes = BTreeMap::new();
    for (_, record) in map_entries(get(root, "users_by_id"), "users_by_id")? {
        let user = record
            .deserialized::<LegacyUser>()
            .map_err(|_| malformed("user record lacks user_id or password_phc"))?;
        hashes.insert(user.user_id.to_string(), user.password_phc);
    }
    Ok(hashes)
}

struct RetiredMerge {
    ids: Vec<SnapshotId>,
    added: usize,
    priority: usize,
    bound: usize,
    overflow: usize,
}

/// Rebuilds the tombstone set: in-flight and head IDs first (all of them, or
/// a refusal), then committed history, then the checkpoint's own tombstones,
/// up to [`TOMBSTONE_HEADROOM_BOUND`] so the non-evicting replay cache keeps
/// room for the new build. The remainder is reported as overflow.
fn merge_retired(
    root: &[(String, Value)],
    collected: &CollectedSnapshotIds,
    allow_overflow: bool,
) -> Result<RetiredMerge, FreshStartError> {
    let existing: Vec<SnapshotId> = match get(root, "retired_snapshots") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => value
            .deserialized()
            .map_err(|_| malformed("retired_snapshots is not a snapshot ID set"))?,
    };
    let existing: BTreeSet<SnapshotId> = existing.into_iter().collect();
    let priority = collected.priority.len();
    if priority > TOMBSTONE_HEADROOM_BOUND && !allow_overflow {
        return Err(FreshStartError::TombstoneOverflow {
            required: priority,
            bound: TOMBSTONE_HEADROOM_BOUND,
        });
    }
    let bound = if allow_overflow {
        MAX_RETIRED_SNAPSHOTS
    } else {
        TOMBSTONE_HEADROOM_BOUND
    };
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    let mut overflow = 0;
    for id in collected.priority.iter().chain(&collected.rest) {
        if !seen.insert(*id) {
            continue;
        }
        if ids.len() >= bound {
            overflow += 1;
        } else {
            ids.push(*id);
        }
    }
    let added = ids.iter().filter(|id| !existing.contains(id)).count();
    Ok(RetiredMerge {
        ids,
        added,
        priority,
        bound,
        overflow,
    })
}

fn set_slot(output: &mut [(Value, Value)], key: &str, value: Value) -> Result<(), FreshStartError> {
    let slot = output
        .iter_mut()
        .find(|(name, _)| matches!(name, Value::Text(text) if text == key))
        .ok_or_else(|| FreshStartError::SelfCheck(format!("{key} missing from State")))?;
    slot.1 = value;
    Ok(())
}

/// Starts from this build's empty state so every required key exists, then
/// overlays the allowlisted legacy values. Unknown legacy keys never survive.
pub(crate) fn assemble(
    root: &[(String, Value)],
    characters: Value,
    retired: &[SnapshotId],
    drop_sessions: bool,
) -> Result<(Value, BTreeMap<String, usize>), FreshStartError> {
    let keep =
        |key: &str| KEEP_ALWAYS.contains(&key) || (!drop_sessions && KEEP_SESSIONS.contains(&key));
    let Value::Map(mut output) = serialized(&State::default())? else {
        return Err(FreshStartError::SelfCheck(
            "default state is not a map".into(),
        ));
    };
    let mut dropped_keys = BTreeMap::new();
    for (key, value) in root {
        if REBUILT.contains(&key.as_str()) {
            continue;
        }
        if keep(key) {
            set_slot(&mut output, key, value.clone())?;
        } else {
            dropped_keys.insert(key.clone(), entries(value));
        }
    }
    set_slot(&mut output, "characters", characters)?;
    set_slot(&mut output, "retired_snapshots", serialized(&retired)?)?;
    Ok((Value::Map(output), dropped_keys))
}

/// Transforms a legacy checkpoint into a fresh-start checkpoint for this build.
///
/// # Errors
/// Fails without side effects for malformed input or a failed self-check.
pub(crate) fn transform(
    payload: &[u8],
    drop_sessions: bool,
    allow_tombstone_overflow: bool,
) -> Result<Transformed, FreshStartError> {
    if payload.len() > MAX_STATE_BYTES {
        return Err(FreshStartError::TooLarge);
    }
    if let Ok(state) = decode_state(payload) {
        return Err(FreshStartError::CurrentFormatWithLegacyCharacters(
            legacy_character_count(&state),
        ));
    }
    let root = parse_origin_main_root(payload)?;
    let before = counts(&root);
    let password_hashes = legacy_password_hashes(&root)?;
    let (characters, epochs) = rebuild_characters(&root)?;

    let collected = collect_snapshot_ids(&root);
    let unparsable_snapshot_ids = collected.unparsable;
    let retired = merge_retired(&root, &collected, allow_tombstone_overflow)?;
    let (output, dropped_keys) = assemble(&root, characters, &retired.ids, drop_sessions)?;

    let mut intermediate = Vec::new();
    ciborium::into_writer(&output, &mut intermediate)
        .map_err(|error| FreshStartError::SelfCheck(error.to_string()))?;
    let state = decode_state(&intermediate)
        .map_err(|_| FreshStartError::SelfCheck("transformed payload does not decode".into()))?;
    verify(&state, &before, &password_hashes, &epochs, drop_sessions)?;
    if state.retired_snapshots.len() != retired.ids.len() || retired.ids.len() > retired.bound {
        return Err(FreshStartError::SelfCheck(
            "retired_snapshots exceeds its bound".into(),
        ));
    }
    let encoded = encode(&state)
        .map_err(|_| FreshStartError::SelfCheck("transformed payload exceeds 32 MiB".into()))?;
    // The bytes written are the canonical typed encoding; prove they decode.
    let reread = decode_state(&encoded)
        .map_err(|_| FreshStartError::SelfCheck("encoded payload does not decode".into()))?;
    verify(&reread, &before, &password_hashes, &epochs, drop_sessions)?;

    let characters_without_owner = state
        .characters
        .values()
        .filter(|record| !state.users_by_id.contains_key(&record.owner))
        .count();
    let report = FreshStartReport {
        outcome: "fresh_started",
        reason: None,
        drop_sessions,
        current_payload_sha256: hex(&sha256(payload)),
        before: Some(before),
        after: Some(typed_counts(&state)),
        characters_at_revision_zero: state
            .characters
            .values()
            .filter(|record| record.revision == Revision::initial())
            .count(),
        characters_without_owner,
        retired_snapshots_added: retired.added,
        retired_snapshots_in_flight_and_heads: retired.priority,
        retired_snapshots_final: state.retired_snapshots.len(),
        retired_snapshots_bound: retired.bound,
        retired_snapshots_headroom: MAX_RETIRED_SNAPSHOTS
            .saturating_sub(state.retired_snapshots.len()),
        retired_snapshots_overflow: retired.overflow,
        unparsable_snapshot_ids,
        dropped_keys,
        archived_payload_sha256: Some(hex(&sha256(payload))),
        new_payload_sha256: Some(hex(&sha256(&encoded))),
        archived_payload_bytes: payload.len(),
        new_payload_bytes: encoded.len(),
    };
    Ok(Transformed {
        payload: encoded,
        report,
    })
}

fn verify(
    state: &State,
    before: &Counts,
    password_hashes: &BTreeMap<String, String>,
    epochs: &BTreeMap<CharacterId, u32>,
    drop_sessions: bool,
) -> Result<(), FreshStartError> {
    let fail = |message: &str| Err(FreshStartError::SelfCheck(message.to_owned()));
    if state.users_by_id.len() != before.users || state.characters.len() != before.characters {
        return fail("user or character count changed");
    }
    for (user_id, user) in &state.users_by_id {
        if password_hashes.get(&user_id.to_string()) != Some(&user.password_phc) {
            return fail("a password hash changed");
        }
    }
    for (character_id, record) in &state.characters {
        if record.revision != Revision::initial()
            || record.world_revision != 0
            || record.active_snapshot.is_some()
            || !record.world_heads.is_empty()
            || epochs.get(character_id) != Some(&record.last_session_epoch)
            || is_legacy_character(state, *character_id, record)
        {
            return fail("a character is not a fresh revision-0 character");
        }
    }
    let sessions = (
        state.access.len(),
        state.refresh.len(),
        state.families.len(),
    );
    let expected_sessions = if drop_sessions {
        (0, 0, 0)
    } else {
        (
            before.access_tokens,
            before.refresh_tokens,
            before.token_families,
        )
    };
    if sessions != expected_sessions {
        return fail("login session maps were not preserved as requested");
    }
    if !(state.snapshots.is_empty()
        && state.prepared.is_empty()
        && state.leases.is_empty()
        && state.groups.is_empty()
        && state.ledger_entries.is_empty()
        && state.trade_offers.is_empty()
        && state.battle_reservations.is_empty()
        && state.retiring_snapshots.is_empty()
        && state.upload_objects.is_empty())
    {
        return fail("gameplay state survived the fresh start");
    }
    Ok(())
}

fn database(error: &postgres::Error) -> FreshStartError {
    FreshStartError::Database(error.to_string())
}

fn already_fresh(reason: &'static str, payload: &[u8], drop_sessions: bool) -> FreshStartReport {
    FreshStartReport {
        outcome: "already_fresh",
        reason: Some(reason),
        drop_sessions,
        current_payload_sha256: hex(&sha256(payload)),
        before: None,
        after: None,
        characters_at_revision_zero: 0,
        characters_without_owner: 0,
        retired_snapshots_added: 0,
        retired_snapshots_in_flight_and_heads: 0,
        retired_snapshots_final: 0,
        retired_snapshots_bound: TOMBSTONE_HEADROOM_BOUND,
        retired_snapshots_headroom: 0,
        retired_snapshots_overflow: 0,
        unparsable_snapshot_ids: 0,
        dropped_keys: BTreeMap::new(),
        archived_payload_sha256: None,
        new_payload_sha256: None,
        archived_payload_bytes: 0,
        new_payload_bytes: 0,
    }
}

/// Runs the fresh start against one database. The server must be stopped.
///
/// # Errors
/// Returns an error, with every write rolled back, for any failed gate.
pub fn run(
    database_url: &str,
    options: &FreshStartOptions,
) -> Result<FreshStartReport, FreshStartError> {
    let mut config: postgres::Config = database_url
        .parse()
        .map_err(|_| FreshStartError::DatabaseConfiguration)?;
    config.connect_timeout(Duration::from_secs(5));
    let mut client = config
        .connect(postgres::NoTls)
        .map_err(|error| database(&error))?;
    client
        .batch_execute("SET statement_timeout = '60s'; SET lock_timeout = '3s'; SET idle_in_transaction_session_timeout = '120s'")
        .map_err(|error| database(&error))?;
    let owned: bool = client
        .query_one(ADVISORY_LOCK_SQL, &[])
        .map_err(|error| database(&error))?
        .get(0);
    if !owned {
        return Err(FreshStartError::LockHeld);
    }
    let result = run_locked(&mut client, options);
    let _ = client.query_one(UNLOCK_SQL, &[]);
    result
}

fn run_locked(
    client: &mut postgres::Client,
    options: &FreshStartOptions,
) -> Result<FreshStartReport, FreshStartError> {
    let mut transaction = client.transaction().map_err(|error| database(&error))?;
    let has_checkpoint: bool = transaction
        .query_one(CHECKPOINT_TABLE_SQL, &[])
        .map_err(|error| database(&error))?
        .get(0);
    if !has_checkpoint {
        return Err(FreshStartError::NoCheckpoint);
    }
    transaction
        .batch_execute(MIGRATION_SQL)
        .map_err(|error| database(&error))?;
    let row = transaction
        .query_opt(SELECT_FOR_UPDATE_SQL, &[])
        .map_err(|error| database(&error))?
        .ok_or(FreshStartError::NoCheckpoint)?;
    let format_version: i32 = row.get(0);
    let payload: Vec<u8> = row.get(1);
    let expected_archived: bool = transaction
        .query_one(ARCHIVE_EXISTS_SQL, &[&options.expected_sha256.as_slice()])
        .map_err(|error| database(&error))?
        .get(0);
    match decide(
        format_version,
        &payload,
        &options.expected_sha256,
        expected_archived,
    )? {
        Decision::AlreadyFresh(reason) => {
            transaction.rollback().map_err(|error| database(&error))?;
            return Ok(already_fresh(reason, &payload, options.drop_sessions));
        }
        Decision::Transform => {}
    }
    let Transformed {
        payload: new_payload,
        mut report,
    } = transform(
        &payload,
        options.drop_sessions,
        options.allow_tombstone_overflow,
    )?;
    if options.dry_run {
        transaction.rollback().map_err(|error| database(&error))?;
        report.outcome = "dry_run";
        return Ok(report);
    }
    let original_sha256 = sha256(&payload);
    let archived = transaction
        .execute(
            ARCHIVE_INSERT_SQL,
            &[
                &ARCHIVE_REASON,
                &format_version,
                &payload,
                &original_sha256.as_slice(),
            ],
        )
        .map_err(|error| database(&error))?;
    let updated = transaction
        .execute(UPDATE_SQL, &[&new_payload])
        .map_err(|error| database(&error))?;
    if archived != 1 || updated != 1 {
        return Err(FreshStartError::Database(
            "archive insert or checkpoint update affected an unexpected row count".into(),
        ));
    }
    transaction.commit().map_err(|error| database(&error))?;
    Ok(report)
}

/// Command-line entry point: reads the database URL from
/// `COOP_DATABASE_URL_FILE`, exactly like the production server.
///
/// # Errors
/// Returns usage, configuration, gate, or database errors.
pub fn run_cli(arguments: &[String]) -> Result<FreshStartReport, FreshStartError> {
    let options = parse_args(arguments)?;
    let path =
        std::env::var_os("COOP_DATABASE_URL_FILE").ok_or(FreshStartError::DatabaseConfiguration)?;
    let secret = super::production::read_secret(Path::new(&path), 4096)
        .map_err(|_| FreshStartError::DatabaseConfiguration)?;
    let url = std::str::from_utf8(&secret)
        .map_err(|_| FreshStartError::DatabaseConfiguration)?
        .trim();
    if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
        return Err(FreshStartError::DatabaseConfiguration);
    }
    run(url, &options)
}

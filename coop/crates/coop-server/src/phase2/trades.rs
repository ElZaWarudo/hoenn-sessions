//! Server-owned paired trade staging and publication primitives.
//!
//! Consent offers have HTTP routes. Level 1 applies trades in the ROM via
//! the outcome ledger: the second consent issues one ledger entry per member
//! carrying the partner's exact slot record, and each member declares it in
//! its next finalized snapshot. The staging and publication primitives below
//! are unused by that flow and stay disabled.

#![allow(dead_code)]

use coop_cloud::{
    ApiVersion, ArtifactIdentity, CharacterId, CommitId, GroupId, IdempotencyKey, LeaseContract,
    LeaseFence, Revision, Sha256Digest, SnapshotFence, SnapshotFile, SnapshotId, SnapshotRecord,
    TradeDecision, TradeDecisionRequest, TradeOfferCurrentView, TradeOfferId, TradeOfferRequest,
    TradeOfferStatus, TradeOfferView, TradeOfferedPokemon, TradePokemonKey,
};
use sha2::{Digest, Sha256};

use super::{
    AuthenticatedActor, Phase2Error,
    saves::{
        ensure_trade_snapshot_quota, files_storage_usage, seal_object, validate_character_sav,
    },
    storage::{
        GROUP_IDEMPOTENCY_TTL_MS, GroupStatus, MAX_TRADE_STAGES, State, Store, TRADE_STAGE_TTL_MS,
        TradeCommitReceipt, TradeOfferIdempotencyRecord, TradeOfferRecord, TradeSourceCommitment,
        TradeStageRecord, TradeStageStatus, TradeStagedMember,
    },
};

/// Recheck both members and source heads before any staging or commit.
///
/// This remains intentionally separate from object verification: repository
/// locks must never be held while an object-store adapter is called.
pub(crate) fn validate_trade_anchor(
    state: &State,
    record: &TradeOfferRecord,
    now: u64,
) -> Result<TradeOfferView, Phase2Error> {
    let view = record.view;
    if view.status != TradeOfferStatus::Accepted
        || record.consents != [true, true]
        || record.expires_at <= now
        || view.expires_at.value() != record.expires_at
        || view.members[0] == view.members[1]
        || !view.members.contains(&view.initiator)
    {
        return Err(Phase2Error::Conflict);
    }
    let group = state
        .groups
        .get(&view.group_id)
        .ok_or(Phase2Error::Conflict)?;
    if group.status != GroupStatus::Active || group.group.members() != view.members {
        return Err(Phase2Error::Conflict);
    }
    for (index, character_id) in view.members.into_iter().enumerate() {
        if state.active_group_by_member.get(&character_id) != Some(&view.group_id) {
            return Err(Phase2Error::Conflict);
        }
        let character = state
            .characters
            .get(&character_id)
            .ok_or(Phase2Error::Conflict)?;
        let lease = state
            .leases
            .get(&character_id)
            .ok_or(Phase2Error::Conflict)?;
        let source = state.snapshots.get(&view.snapshots[index]);
        // The revision-one record is the server's lineage anchor for later
        // saves. This does not prove the original enrollment was uncloned.
        let first = state
            .snapshot_by_revision
            .get(&(character_id, Revision::new(1)))
            .and_then(|snapshot_id| {
                state
                    .snapshots
                    .get(snapshot_id)
                    .map(|record| (*snapshot_id, record))
            });
        if character.revision != view.revisions[index]
            || character.active_snapshot != Some(view.snapshots[index])
            || lease.released
            || lease.contract.expires_at.value() <= now
            || lease.contract.fence() != record.fences[index]
            || lease.contract.current_revision != view.revisions[index]
            || state
                .snapshot_by_revision
                .get(&(character_id, view.revisions[index]))
                != Some(&view.snapshots[index])
            || source.is_none_or(|snapshot| {
                snapshot.snapshot_id != view.snapshots[index]
                    || snapshot.character_id != character_id
                    || snapshot.revision != view.revisions[index]
                    || snapshot.validate().is_err()
            })
            || first.is_none_or(|(snapshot_id, snapshot)| {
                snapshot.snapshot_id != snapshot_id
                    || snapshot.character_id != character_id
                    || snapshot.revision != Revision::new(1)
                    || snapshot.parent_revision != Revision::initial()
                    || snapshot.validate().is_err()
            })
        {
            return Err(Phase2Error::Conflict);
        }
    }
    Ok(view)
}

#[derive(Clone)]
struct VerifiedSource {
    commitment: TradeSourceCommitment,
    save: coop_save::ValidatedSave,
    pending: Vec<u8>,
}

fn snapshot_file(
    snapshot: &SnapshotRecord,
    artifact: ArtifactIdentity,
) -> Result<SnapshotFile, Phase2Error> {
    snapshot
        .files
        .iter()
        .find(|file| file.artifact == artifact)
        .cloned()
        .ok_or(Phase2Error::Conflict)
}

fn verify_source(
    store: &Store,
    snapshot: &SnapshotRecord,
    fence: LeaseFence,
    slot: coop_cloud::PartyPosition,
) -> Result<VerifiedSource, Phase2Error> {
    snapshot.validate().map_err(|_| Phase2Error::Conflict)?;
    let sav_file = snapshot_file(snapshot, ArtifactIdentity::CharacterSav)?;
    let pending_file = snapshot_file(snapshot, ArtifactIdentity::PendingCommits)?;
    let sav_key = Store::object_key(
        snapshot.character_id,
        snapshot.snapshot_id,
        sav_file.artifact,
    );
    let pending_key = Store::object_key(
        snapshot.character_id,
        snapshot.snapshot_id,
        pending_file.artifact,
    );
    let sav_bytes = store.objects.get(&sav_key)?.ok_or(Phase2Error::Conflict)?;
    let pending = store
        .objects
        .get(&pending_key)?
        .ok_or(Phase2Error::Conflict)?;
    sav_file
        .verify_bytes(&sav_bytes)
        .map_err(|_| Phase2Error::Conflict)?;
    pending_file
        .verify_bytes(&pending)
        .map_err(|_| Phase2Error::Conflict)?;
    if pending_file.sha256 != snapshot.pending_commits_sha256 {
        return Err(Phase2Error::Conflict);
    }
    let save =
        validate_character_sav(&sav_bytes, snapshot.revision).map_err(|_| Phase2Error::Conflict)?;
    let slot_record = match save
        .party_pokemon(slot.index())
        .map_err(|_| Phase2Error::Conflict)?
    {
        coop_save::PokemonSlot::Occupied(record) => record,
        coop_save::PokemonSlot::Empty { .. } => return Err(Phase2Error::Conflict),
    };
    Ok(VerifiedSource {
        commitment: TradeSourceCommitment {
            character_id: snapshot.character_id,
            snapshot_id: snapshot.snapshot_id,
            revision: snapshot.revision,
            fence,
            slot,
            save_sha256: sav_file.sha256,
            pending_commits_sha256: pending_file.sha256,
            slot_sha256: Sha256Digest::of_bytes(&slot_record.raw),
            last_applied_commit: snapshot.last_applied_commit,
        },
        save,
        pending,
    })
}

fn source_matches_snapshot(snapshot: &SnapshotRecord, source: TradeSourceCommitment) -> bool {
    let Ok(sav) = snapshot_file(snapshot, ArtifactIdentity::CharacterSav) else {
        return false;
    };
    let Ok(pending) = snapshot_file(snapshot, ArtifactIdentity::PendingCommits) else {
        return false;
    };
    snapshot.character_id == source.character_id
        && snapshot.snapshot_id == source.snapshot_id
        && snapshot.revision == source.revision
        && snapshot.last_applied_commit == source.last_applied_commit
        && sav.sha256 == source.save_sha256
        && pending.sha256 == source.pending_commits_sha256
        && snapshot.pending_commits_sha256 == source.pending_commits_sha256
}

fn verify_source_commitment(
    store: &Store,
    snapshot: &SnapshotRecord,
    source: TradeSourceCommitment,
) -> Result<(), Phase2Error> {
    let verified = verify_source(store, snapshot, source.fence, source.slot)?;
    if verified.commitment == source {
        Ok(())
    } else {
        Err(Phase2Error::Conflict)
    }
}

fn staged_keys(stage: &TradeStageRecord) -> impl Iterator<Item = String> + '_ {
    stage.outputs.iter().flat_map(|output| {
        output.files.iter().map(move |file| {
            Store::object_key(output.character_id, output.snapshot_id, file.artifact)
        })
    })
}

fn verify_staged_objects(store: &Store, stage: &TradeStageRecord) -> Result<(), Phase2Error> {
    for output in &stage.outputs {
        for file in &output.files {
            let key = Store::object_key(output.character_id, output.snapshot_id, file.artifact);
            let bytes = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
            file.verify_bytes(&bytes)
                .map_err(|_| Phase2Error::Conflict)?;
        }
    }
    Ok(())
}

fn cleanup_trade_stage(
    store: &Store,
    offer_id: coop_cloud::TradeOfferId,
    idempotency_key: IdempotencyKey,
    stage: &TradeStageRecord,
) -> Result<(), Phase2Error> {
    if stage.status == TradeStageStatus::Published {
        return Ok(());
    }
    for key in staged_keys(stage) {
        seal_object(store, &key)?;
    }
    store.write_transaction(|state| {
        if state.trade_staging.get(&offer_id).is_some_and(|current| {
            current.idempotency_key == idempotency_key
                && current.status != TradeStageStatus::Published
        }) {
            state.trade_staging.remove(&offer_id);
        }
        Ok(())
    })
}

fn reconcile_existing_stage(store: &Store, stage: &TradeStageRecord) -> Result<bool, Phase2Error> {
    if stage.status != TradeStageStatus::Staging {
        return Ok(true);
    }
    if verify_staged_objects(store, stage).is_ok() {
        store.write_transaction(|state| {
            let current = state
                .trade_staging
                .get_mut(&stage.offer_id)
                .ok_or(Phase2Error::Conflict)?;
            if current.idempotency_key != stage.idempotency_key
                || current.status != TradeStageStatus::Staging
            {
                return Err(Phase2Error::Conflict);
            }
            current.status = TradeStageStatus::Ready;
            Ok(())
        })?;
        return Ok(true);
    }
    cleanup_trade_stage(store, stage.offer_id, stage.idempotency_key, stage)?;
    Ok(false)
}

fn put_staged_object(
    store: &Store,
    key: String,
    file: &SnapshotFile,
    bytes: Vec<u8>,
) -> Result<(), Phase2Error> {
    match store.objects.put_if_absent(key.clone(), bytes)? {
        true => Ok(()),
        false => {
            let existing = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
            file.verify_bytes(&existing)
                .map_err(|_| Phase2Error::Conflict)
        }
    }
}

/// Verify both source snapshots and stage both transformed outputs. No
/// snapshot head or lease is changed by this function.
pub(crate) fn stage_trade(
    store: &Store,
    offer_id: coop_cloud::TradeOfferId,
    idempotency_key: IdempotencyKey,
    now: u64,
) -> Result<TradeStageRecord, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    let existing = store.read_transaction(|state| {
        let Some(stage) = state.trade_staging.get(&offer_id) else {
            return Ok::<Option<TradeStageRecord>, Phase2Error>(None);
        };
        if stage.idempotency_key != idempotency_key {
            return Err(Phase2Error::Conflict);
        }
        Ok(Some(stage.clone()))
    })?;
    if let Some(stage) = existing {
        if stage.status == TradeStageStatus::Published || stage.status == TradeStageStatus::Ready {
            return Ok(stage);
        }
        if stage.expires_at <= now {
            cleanup_trade_stage(store, offer_id, idempotency_key, &stage)?;
        } else if reconcile_existing_stage(store, &stage)? {
            return store.read_transaction(|state| {
                state
                    .trade_staging
                    .get(&offer_id)
                    .cloned()
                    .ok_or(Phase2Error::Conflict)
            });
        }
    }

    let (record, snapshots) = store.read_transaction(|state| {
        let record = state
            .trade_offers
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        let view = validate_trade_anchor(state, &record, now)?;
        let snapshots: [Result<SnapshotRecord, Phase2Error>; 2] = std::array::from_fn(|index| {
            state
                .snapshots
                .get(&view.snapshots[index])
                .cloned()
                .ok_or(Phase2Error::Conflict)
        });
        let snapshots = snapshots
            .into_iter()
            .collect::<Result<Vec<_>, Phase2Error>>()?;
        let snapshots: [SnapshotRecord; 2] =
            snapshots.try_into().map_err(|_| Phase2Error::Internal)?;
        Ok::<(TradeOfferRecord, [SnapshotRecord; 2]), Phase2Error>((record, snapshots))
    })?;
    let sources = [
        verify_source(store, &snapshots[0], record.fences[0], record.view.slots[0])?,
        verify_source(store, &snapshots[1], record.fences[1], record.view.slots[1])?,
    ];
    let (left, right) = coop_save::trade_party_pokemon(
        &sources[0].save,
        sources[0].commitment.slot.index(),
        &sources[1].save,
        sources[1].commitment.slot.index(),
    )
    .map_err(|_| Phase2Error::Conflict)?;
    let transformed = [left, right];
    let output_ids = [store.snapshot_id()?, store.snapshot_id()?];
    let output_revisions = [
        sources[0]
            .commitment
            .revision
            .next()
            .map_err(|_| Phase2Error::Conflict)?,
        sources[1]
            .commitment
            .revision
            .next()
            .map_err(|_| Phase2Error::Conflict)?,
    ];
    let outputs: [Result<TradeStagedMember, Phase2Error>; 2] = std::array::from_fn(|index| {
        let sav = SnapshotFile::from_bytes(
            ArtifactIdentity::CharacterSav,
            transformed[index].raw_bytes(),
        )
        .map_err(|_| Phase2Error::Internal)?;
        let pending =
            SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, &sources[index].pending)
                .map_err(|_| Phase2Error::Internal)?;
        Ok::<TradeStagedMember, Phase2Error>(TradeStagedMember {
            character_id: sources[index].commitment.character_id,
            snapshot_id: output_ids[index],
            revision: output_revisions[index],
            fence: sources[index].commitment.fence,
            source: sources[index].commitment,
            files: [sav, pending],
        })
    });
    let outputs: [TradeStagedMember; 2] = outputs
        .into_iter()
        .collect::<Result<Vec<_>, Phase2Error>>()?
        .try_into()
        .map_err(|_| Phase2Error::Internal)?;
    let expires_at = now
        .checked_add(TRADE_STAGE_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let reserved_bytes = [
        files_storage_usage(&outputs[0].files)?,
        files_storage_usage(&outputs[1].files)?,
    ];
    let stage = TradeStageRecord {
        offer_id,
        idempotency_key,
        sources: [sources[0].commitment, sources[1].commitment],
        outputs,
        reserved_bytes,
        status: TradeStageStatus::Staging,
        expires_at,
        receipt: None,
    };
    store.write_transaction(|state| {
        if state.trade_receipts.contains_key(&offer_id) {
            return Err(Phase2Error::Conflict);
        }
        if state
            .trade_staging
            .values()
            .filter(|stage| stage.status != TradeStageStatus::Published)
            .count()
            >= MAX_TRADE_STAGES
        {
            return Err(Phase2Error::Busy);
        }
        for output in &stage.outputs {
            ensure_trade_snapshot_quota(state, output.character_id, &output.files, None)?;
        }
        if let Some(current) = state.trade_staging.get(&offer_id) {
            return if current.idempotency_key == idempotency_key {
                Ok::<(), Phase2Error>(())
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        state.trade_staging.insert(offer_id, stage.clone());
        Ok(())
    })?;

    let write_result = (|| {
        for (index, output) in stage.outputs.iter().enumerate() {
            for file in &output.files {
                let bytes = match file.artifact {
                    ArtifactIdentity::CharacterSav => transformed[index].raw_bytes().to_vec(),
                    ArtifactIdentity::PendingCommits => sources[index].pending.clone(),
                    ArtifactIdentity::ResumeSs1 => return Err(Phase2Error::Conflict),
                };
                put_staged_object(
                    store,
                    Store::object_key(output.character_id, output.snapshot_id, file.artifact),
                    file,
                    bytes,
                )?;
            }
        }
        store.write_transaction(|state| {
            let current = state
                .trade_staging
                .get_mut(&offer_id)
                .ok_or(Phase2Error::Conflict)?;
            if current.idempotency_key != idempotency_key
                || current.status != TradeStageStatus::Staging
            {
                return Err(Phase2Error::Conflict);
            }
            current.status = TradeStageStatus::Ready;
            Ok(())
        })
    })();
    if let Err(error) = write_result {
        let cleanup = cleanup_trade_stage(store, offer_id, idempotency_key, &stage);
        return Err(cleanup.err().unwrap_or(error));
    }
    store.read_transaction(|state| {
        state
            .trade_staging
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::Conflict)
    })
}

fn output_snapshot(output: &TradeStagedMember, now: u64) -> Result<SnapshotRecord, Phase2Error> {
    SnapshotRecord::new(
        output.snapshot_id,
        SnapshotFence::new(
            output.fence.session_id,
            output.character_id,
            output.fence.session_epoch,
        ),
        output.source.revision,
        output.revision,
        output.files.to_vec(),
        output.files[1].sha256,
        output.source.last_applied_commit,
        Store::unix_timestamp(now)?,
    )
    .map_err(|_| Phase2Error::Internal)
}

/// Publish both staged snapshots under one repository transaction. Any
/// failed preflight or transaction leaves no usable half-trade: staged
/// objects are sealed before the stage is removed, while a backend failure
/// leaves the durable stage for the recovery worker to retry.
pub(crate) fn publish_trade(
    store: &Store,
    offer_id: coop_cloud::TradeOfferId,
    idempotency_key: IdempotencyKey,
    now: u64,
) -> Result<TradeCommitReceipt, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    if store.repository.is_fenced() {
        return Err(Phase2Error::Internal);
    }
    let (stage, record) = store.read_transaction(|state| {
        if let Some(receipt) = state.trade_receipts.get(&offer_id) {
            return if receipt.idempotency_key == idempotency_key {
                Ok((None, None))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let stage = state
            .trade_staging
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::Conflict)?;
        if stage.idempotency_key != idempotency_key {
            return Err(Phase2Error::Conflict);
        }
        let record = state
            .trade_offers
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        Ok((Some(stage), Some(record)))
    })?;
    if stage.is_none() {
        return store.read_transaction(|state| {
            state
                .trade_receipts
                .get(&offer_id)
                .copied()
                .ok_or(Phase2Error::Conflict)
        });
    }
    let stage = stage.unwrap();
    let _record = record.unwrap();
    if stage.status != TradeStageStatus::Ready {
        return Err(Phase2Error::Conflict);
    }
    if stage.expires_at <= now {
        let cleanup = cleanup_trade_stage(store, offer_id, idempotency_key, &stage);
        return Err(cleanup.err().unwrap_or(Phase2Error::Expired));
    }
    let source_snapshots = store.read_transaction(|state| {
        let snapshots: [Result<SnapshotRecord, Phase2Error>; 2] = std::array::from_fn(|index| {
            state
                .snapshots
                .get(&stage.sources[index].snapshot_id)
                .cloned()
                .ok_or(Phase2Error::Conflict)
        });
        snapshots
            .into_iter()
            .collect::<Result<Vec<_>, Phase2Error>>()?
            .try_into()
            .map_err(|_| Phase2Error::Internal)
    });
    if let Err(error) = source_snapshots
        .and_then(|snapshots: [SnapshotRecord; 2]| {
            verify_source_commitment(store, &snapshots[0], stage.sources[0])?;
            verify_source_commitment(store, &snapshots[1], stage.sources[1])
        })
        .and_then(|()| verify_staged_objects(store, &stage))
    {
        if store.repository.is_fenced() {
            return Err(error);
        }
        let cleanup = cleanup_trade_stage(store, offer_id, idempotency_key, &stage);
        return Err(cleanup.err().unwrap_or(error));
    }
    let snapshots = [
        output_snapshot(&stage.outputs[0], now)?,
        output_snapshot(&stage.outputs[1], now)?,
    ];
    let receipt = TradeCommitReceipt {
        offer_id,
        idempotency_key,
        snapshots: [snapshots[0].snapshot_id, snapshots[1].snapshot_id],
        revisions: [snapshots[0].revision, snapshots[1].revision],
        committed_at: now,
    };
    let commit_result = store.write_transaction(|state| {
        if let Some(known) = state.trade_receipts.get(&offer_id) {
            return if known.idempotency_key == idempotency_key {
                Ok(*known)
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let current_stage = state
            .trade_staging
            .get(&offer_id)
            .ok_or(Phase2Error::Conflict)?;
        if current_stage.idempotency_key != idempotency_key
            || current_stage.status != TradeStageStatus::Ready
            || current_stage.expires_at <= now
        {
            return Err(Phase2Error::Conflict);
        }
        let current_record = state
            .trade_offers
            .get(&offer_id)
            .ok_or(Phase2Error::NotFound)?
            .clone();
        let view = validate_trade_anchor(state, &current_record, now)?;
        for (index, output) in stage.outputs.iter().enumerate() {
            let current_source = state
                .snapshots
                .get(&stage.sources[index].snapshot_id)
                .ok_or(Phase2Error::Conflict)?;
            if !source_matches_snapshot(current_source, stage.sources[index])
                || view.members[index] != output.character_id
                || output.source != stage.sources[index]
                || output.source.slot != view.slots[index]
                || state.snapshots.contains_key(&output.snapshot_id)
                || state
                    .snapshot_by_revision
                    .contains_key(&(output.character_id, output.revision))
            {
                return Err(Phase2Error::Conflict);
            }
            ensure_trade_snapshot_quota(state, output.character_id, &output.files, Some(offer_id))?;
            let character = state
                .characters
                .get(&output.character_id)
                .ok_or(Phase2Error::Conflict)?;
            if character.revision != output.source.revision
                || character.active_snapshot != Some(output.source.snapshot_id)
            {
                return Err(Phase2Error::Conflict);
            }
        }
        let contracts: [Result<LeaseContract, Phase2Error>; 2] = std::array::from_fn(|index| {
            let lease = state
                .leases
                .get(&stage.sources[index].character_id)
                .ok_or(Phase2Error::Conflict)?;
            LeaseContract::new(
                LeaseFence::new(
                    lease.contract.session_id,
                    stage.sources[index].character_id,
                    stage.outputs[index].revision,
                    lease.contract.session_epoch,
                    lease.contract.client_instance_id,
                ),
                lease.contract.expires_at,
                lease.contract.heartbeat_interval_ms,
            )
            .map_err(|_| Phase2Error::Internal)
        });
        let contracts: [LeaseContract; 2] = contracts
            .into_iter()
            .collect::<Result<Vec<_>, Phase2Error>>()?
            .try_into()
            .map_err(|_| Phase2Error::Internal)?;
        for index in 0..2 {
            state
                .snapshots
                .insert(snapshots[index].snapshot_id, snapshots[index].clone());
            state.snapshot_by_revision.insert(
                (
                    stage.outputs[index].character_id,
                    stage.outputs[index].revision,
                ),
                stage.outputs[index].snapshot_id,
            );
        }
        for index in 0..2 {
            let character_id = stage.outputs[index].character_id;
            if let Some(character) = state.characters.get_mut(&character_id) {
                character.revision = stage.outputs[index].revision;
                character.active_snapshot = Some(stage.outputs[index].snapshot_id);
            }
            if let Some(lease) = state.leases.get_mut(&character_id) {
                lease.contract = contracts[index].clone();
            }
        }
        state.trade_receipts.insert(offer_id, receipt);
        let current_stage = state
            .trade_staging
            .get_mut(&offer_id)
            .ok_or(Phase2Error::Conflict)?;
        current_stage.status = TradeStageStatus::Published;
        current_stage.receipt = Some(receipt);
        Ok(receipt)
    });
    match commit_result {
        Ok(receipt) => Ok(receipt),
        Err(error) => {
            // Persistent adapters fence themselves when a commit outcome is
            // ambiguous. The authoritative state may already contain both
            // heads and the receipt, so leave the owned objects in place for
            // restart reconciliation instead of sealing them here.
            if store.repository.is_fenced() {
                return Err(error);
            }
            let cleanup = cleanup_trade_stage(store, offer_id, idempotency_key, &stage);
            Err(cleanup.err().unwrap_or(error))
        }
    }
}

/// Seal and remove expired or abandoned stages. A published stage is retained
/// solely for replay and is never reclaimed by this function.
pub(crate) fn recover_trade_stages(store: &Store, now: u64) -> Result<usize, Phase2Error> {
    let _gate = store.lock_runtime_transition_gate();
    let stages = store.read_transaction(|state| {
        Ok::<Vec<TradeStageRecord>, Phase2Error>(
            state
                .trade_staging
                .values()
                .filter(|stage| stage.status != TradeStageStatus::Published)
                .cloned()
                .collect(),
        )
    })?;
    let mut recovered = 0;
    for stage in stages {
        if stage.expires_at <= now {
            cleanup_trade_stage(store, stage.offer_id, stage.idempotency_key, &stage)?;
            recovered += 1;
        } else if !reconcile_existing_stage(store, &stage)? {
            recovered += 1;
        }
    }
    Ok(recovered)
}

/// Server-owned two-party trade offer consent.
///
/// These functions record consent only. Creating an offer anchors both exact
/// snapshot heads and revisions while the initiator consents to their own
/// slot; the partner explicitly accepts the exact reciprocal slots or
/// rejects. Staging and publication stay behind their existing gates:
/// `validate_trade_anchor` rechecks every anchor before any head can move.
const OP_TRADE_OFFER: &str = "trade_offer_v1";
const OP_TRADE_DECIDE: &str = "trade_decide_v1";
/// The partner has a short consent window; after acceptance both clients need
/// time to quiesce, stage, and adopt the paired save outputs. The in-game UI
/// fits a poll, a prompt, a party selection and a checkpoint in this window.
const TRADE_OFFER_TTL_MS: u64 = 60_000;
const TRADE_ACCEPTED_TTL_MS: u64 = TRADE_STAGE_TTL_MS;
const TRADE_OFFER_REPLAY_TTL_MS: u64 = GROUP_IDEMPOTENCY_TTL_MS;
const MAX_TRADE_OFFERS: usize = 1_024;
const MAX_TRADE_OFFER_RECEIPTS: usize = 4_096;
const MAX_TRADE_OFFER_RECEIPTS_PER_MEMBER: usize = 64;
const MAX_TRADE_OFFER_ID_CANDIDATES: usize = 8;

fn offer_fingerprint<T: serde::Serialize>(
    operation: &str,
    values: &T,
) -> Result<[u8; 32], Phase2Error> {
    let bytes = serde_json::to_vec(&(operation, values)).map_err(|_| Phase2Error::Internal)?;
    Ok(Sha256::digest(bytes).into())
}

fn offer_group(
    state: &State,
    group_id: GroupId,
    actor: CharacterId,
) -> Result<[CharacterId; 2], Phase2Error> {
    let record = state.groups.get(&group_id).ok_or(Phase2Error::NotFound)?;
    if record.status != GroupStatus::Active || !record.group.contains(actor) {
        return Err(Phase2Error::NotFound);
    }
    let members = record.group.members();
    if members
        .iter()
        .any(|member| state.active_group_by_member.get(member) != Some(&group_id))
    {
        return Err(Phase2Error::Internal);
    }
    Ok(members)
}

/// A member lease is live only while its contract is unreleased and
/// unexpired. The companion fence is always server-read; it is never accepted
/// from the caller.
fn live_member_lease(
    state: &State,
    character_id: CharacterId,
    now: u64,
) -> Result<(), Phase2Error> {
    super::group_travel::validate_member(state, character_id)?;
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Forbidden)?;
    if lease.contract.character_id != character_id
        || lease.released
        || lease.contract.expires_at.value() <= now
    {
        return Err(Phase2Error::Forbidden);
    }
    Ok(())
}

fn trade_offer_is_live(record: &TradeOfferRecord, now: u64) -> bool {
    matches!(
        record.view.status,
        TradeOfferStatus::Pending | TradeOfferStatus::Accepted
    ) && record.expires_at > now
}

/// Both exact snapshot heads and revisions must still match the anchored
/// offer. Consent checks run before staging, so slot occupancy is left to
/// source verification at stage time.
fn offer_anchor_is_current(state: &State, view: &TradeOfferView) -> bool {
    view.members[0] != view.members[1]
        && view.members.contains(&view.initiator)
        && [0, 1].into_iter().all(|index| {
            let character_id = view.members[index];
            state
                .characters
                .get(&character_id)
                .is_some_and(|character| {
                    character.revision == view.revisions[index]
                        && character.active_snapshot == Some(view.snapshots[index])
                })
                && state
                    .snapshot_by_revision
                    .get(&(character_id, view.revisions[index]))
                    == Some(&view.snapshots[index])
                && state
                    .snapshots
                    .get(&view.snapshots[index])
                    .is_some_and(|snapshot| {
                        snapshot.character_id == character_id
                            && snapshot.revision == view.revisions[index]
                            && snapshot.validate().is_ok()
                    })
        })
}

/// Marks lapsed live offers expired, evicts the oldest terminal offers only
/// when the map is over its bound, and drops lapsed idempotency receipts.
fn prune_trade_offer_state(state: &mut State, now: u64) {
    for record in state.trade_offers.values_mut() {
        if matches!(
            record.view.status,
            TradeOfferStatus::Pending | TradeOfferStatus::Accepted
        ) && record.expires_at <= now
        {
            record.view.status = TradeOfferStatus::Expired;
        }
    }
    if state.trade_offers.len() >= MAX_TRADE_OFFERS {
        let mut terminal: Vec<_> = state
            .trade_offers
            .iter()
            .filter(|(_, record)| {
                matches!(
                    record.view.status,
                    TradeOfferStatus::Rejected | TradeOfferStatus::Expired
                )
            })
            .map(|(offer_id, record)| (*offer_id, record.expires_at))
            .collect();
        terminal.sort_by_key(|(_, expires_at)| *expires_at);
        let evict = state.trade_offers.len() - MAX_TRADE_OFFERS + 1;
        for (offer_id, _) in terminal.into_iter().take(evict) {
            state.trade_offers.remove(&offer_id);
        }
    }
    state
        .trade_offer_idempotency
        .retain(|_, record| record.expires_at > now);
}

fn ensure_trade_offer_receipt_slot(state: &State, actor: CharacterId) -> Result<(), Phase2Error> {
    if state.trade_offer_idempotency.len() >= MAX_TRADE_OFFER_RECEIPTS
        || state
            .trade_offer_idempotency
            .keys()
            .filter(|(member, _)| *member == actor)
            .count()
            >= MAX_TRADE_OFFER_RECEIPTS_PER_MEMBER
    {
        Err(Phase2Error::Busy)
    } else {
        Ok(())
    }
}

fn trade_offer_idempotency_lookup(
    state: &State,
    actor: CharacterId,
    key: IdempotencyKey,
    fingerprint: [u8; 32],
) -> Result<Option<TradeOfferView>, Phase2Error> {
    let Some(record) = state.trade_offer_idempotency.get(&(actor, key)) else {
        return Ok(None);
    };
    if record.fingerprint != fingerprint {
        return Err(Phase2Error::Conflict);
    }
    Ok(Some(record.view.clone()))
}

fn insert_trade_offer_receipt(
    state: &mut State,
    actor: CharacterId,
    key: IdempotencyKey,
    fingerprint: [u8; 32],
    view: TradeOfferView,
    now: u64,
) {
    state.trade_offer_idempotency.insert(
        (actor, key),
        TradeOfferIdempotencyRecord {
            fingerprint,
            view,
            expires_at: now.saturating_add(TRADE_OFFER_REPLAY_TTL_MS),
        },
    );
}

/// A member's current head as recorded in the repository. Object reads are
/// the caller's job, outside any transaction.
fn current_head(state: &State, member: CharacterId) -> Result<SnapshotRecord, Phase2Error> {
    let character = state
        .characters
        .get(&member)
        .ok_or(Phase2Error::Forbidden)?;
    let snapshot_id = character.active_snapshot.ok_or(Phase2Error::Conflict)?;
    if state
        .snapshot_by_revision
        .get(&(member, character.revision))
        != Some(&snapshot_id)
    {
        return Err(Phase2Error::Conflict);
    }
    let snapshot = state
        .snapshots
        .get(&snapshot_id)
        .ok_or(Phase2Error::Conflict)?;
    if snapshot.character_id != member || snapshot.revision != character.revision {
        return Err(Phase2Error::Conflict);
    }
    Ok(snapshot.clone())
}

/// Summary the partner's ROM shows for an offered party record: species,
/// level (`struct Pokemon.level`, offset 84), egg flag and raw nickname.
fn offered_summary(raw: &[u8; 100]) -> Result<TradeOfferedPokemon, Phase2Error> {
    match coop_save::decode_party_record(*raw).map_err(|_| Phase2Error::Conflict)? {
        coop_save::PokemonSlot::Occupied(record) => Ok(TradeOfferedPokemon {
            species: record.identity.species,
            level: raw[84],
            is_egg: record.identity.is_egg,
            nickname: record.identity.nickname,
        }),
        coop_save::PokemonSlot::Empty { .. } => Err(Phase2Error::Conflict),
    }
}

/// Reads one member's offered slot from a head: an occupied record inside
/// the saved party that holds no mail and, when `expected` is given, is
/// exactly that Pokémon. A different Pokémon means the head is stale.
fn read_offered_side(
    store: &Store,
    snapshot: &SnapshotRecord,
    fence: LeaseFence,
    slot: coop_cloud::PartyPosition,
    expected: Option<TradePokemonKey>,
) -> Result<super::ledger::TradeSide, Phase2Error> {
    let verified = verify_source(store, snapshot, fence, slot)?;
    let side = super::ledger::trade_side(
        &verified.save,
        snapshot.snapshot_id,
        snapshot.revision,
        slot,
    )?;
    if expected
        .is_some_and(|key| key.personality != side.key.personality || key.ot_id != side.key.ot_id)
    {
        return Err(Phase2Error::Conflict);
    }
    Ok(side)
}

/// Creates one pending offer anchoring both exact snapshot heads. The caller
/// must present its exact active lease fence; the companion fence and head
/// are server-read. One live offer per group keeps consent unambiguous.
///
/// An open offer (no partner slot or revision) also reads the caller's own
/// slot first: it must hold the named Pokémon without mail, and its summary
/// is kept for the partner's prompt. The partner's anchor is replaced by its
/// own head when it accepts.
pub(crate) fn create_offer(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    request: &TradeOfferRequest,
) -> Result<TradeOfferView, Phase2Error> {
    if request.api_version != ApiVersion::V1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.group_id != group_id {
        return Err(Phase2Error::NotFound);
    }
    if request.is_mixed() {
        return Err(Phase2Error::InvalidRequest);
    }
    let open = request.is_open();
    let fingerprint = offer_fingerprint(OP_TRADE_OFFER, &(group_id, request))?;
    let candidates: Vec<TradeOfferId> = (0..MAX_TRADE_OFFER_ID_CANDIDATES)
        .map(|_| TradeOfferId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
        .collect::<Result<_, _>>()?;
    // Read failures are deferred so an idempotent replay keeps its result.
    let offered_side = open.then(|| {
        store
            .read_transaction(|state| current_head(state, actor.character_id))
            .and_then(|head| {
                read_offered_side(
                    store,
                    &head,
                    request.fence,
                    request.own_slot,
                    request.own_pokemon,
                )
            })
    });
    let now = store.now();
    let expires_at = now
        .checked_add(TRADE_OFFER_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
        super::group_travel::lease_matches(state, actor.character_id, request.fence, now)?;
        prune_trade_offer_state(state, now);
        if let Some(replay) = trade_offer_idempotency_lookup(
            state,
            actor.character_id,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        let members = offer_group(state, group_id, actor.character_id)?;
        if state
            .trade_offers
            .values()
            .any(|record| record.view.group_id == group_id && trade_offer_is_live(record, now))
        {
            return Err(Phase2Error::Conflict);
        }
        if state.trade_offers.len() >= MAX_TRADE_OFFERS {
            return Err(Phase2Error::Busy);
        }
        ensure_trade_offer_receipt_slot(state, actor.character_id)?;
        live_member_lease(state, members[0], now)?;
        live_member_lease(state, members[1], now)?;
        let caller_index = members
            .iter()
            .position(|member| *member == actor.character_id)
            .ok_or(Phase2Error::Internal)?;
        let partner_index = 1 - caller_index;
        let caller_revision = state
            .leases
            .get(&actor.character_id)
            .ok_or(Phase2Error::Authentication)?
            .contract
            .current_revision;
        let partner_revision = state
            .characters
            .get(&members[partner_index])
            .ok_or(Phase2Error::Forbidden)?
            .revision;
        // An open offer's partner anchor is informational until the partner
        // accepts against its own head, so a mid-checkpoint partner is fine.
        if !open {
            if request.partner_expected_revision != Some(partner_revision) {
                return Err(Phase2Error::Conflict);
            }
            if state
                .leases
                .get(&members[partner_index])
                .is_none_or(|lease| lease.contract.current_revision != partner_revision)
            {
                return Err(Phase2Error::Conflict);
            }
        }
        let revisions = if caller_index == 0 {
            [caller_revision, partner_revision]
        } else {
            [partner_revision, caller_revision]
        };
        let mut snapshots: [Option<SnapshotId>; 2] = [None, None];
        for (index, member) in members.into_iter().enumerate() {
            let character = state
                .characters
                .get(&member)
                .ok_or(Phase2Error::Forbidden)?;
            let snapshot_id = character.active_snapshot.ok_or(Phase2Error::Conflict)?;
            if character.revision != revisions[index]
                || state.snapshot_by_revision.get(&(member, revisions[index])) != Some(&snapshot_id)
            {
                return Err(Phase2Error::Conflict);
            }
            let snapshot = state
                .snapshots
                .get(&snapshot_id)
                .ok_or(Phase2Error::Conflict)?;
            if snapshot.character_id != member
                || snapshot.revision != revisions[index]
                || snapshot.validate().is_err()
            {
                return Err(Phase2Error::Conflict);
            }
            snapshots[index] = Some(snapshot_id);
        }
        let snapshots = [
            snapshots[0].ok_or(Phase2Error::Internal)?,
            snapshots[1].ok_or(Phase2Error::Internal)?,
        ];
        let offered = match &offered_side {
            None => None,
            Some(Err(error)) => return Err(error.clone()),
            Some(Ok(side)) => {
                if side.snapshot_id != snapshots[caller_index]
                    || side.revision != revisions[caller_index]
                    || side.slot != request.own_slot
                {
                    return Err(Phase2Error::Conflict);
                }
                Some(offered_summary(&side.raw)?)
            }
        };
        let partner_fence = state
            .leases
            .get(&members[partner_index])
            .ok_or(Phase2Error::Forbidden)?
            .contract
            .fence();
        let offer_id = candidates
            .into_iter()
            .find(|candidate| !state.trade_offers.contains_key(candidate))
            .ok_or(Phase2Error::Conflict)?;
        // An open offer's partner slot is a placeholder until acceptance.
        let partner_slot = match request.partner_slot {
            Some(slot) => slot,
            None => coop_cloud::PartyPosition::new(0).ok_or(Phase2Error::Internal)?,
        };
        let slots = if caller_index == 0 {
            [request.own_slot, partner_slot]
        } else {
            [partner_slot, request.own_slot]
        };
        let fences = if caller_index == 0 {
            [request.fence, partner_fence]
        } else {
            [partner_fence, request.fence]
        };
        let consents = if caller_index == 0 {
            [true, false]
        } else {
            [false, true]
        };
        let view = TradeOfferView {
            api_version: ApiVersion::V1,
            offer_id,
            group_id,
            members,
            initiator: actor.character_id,
            slots,
            revisions,
            snapshots,
            status: TradeOfferStatus::Pending,
            expires_at: Store::unix_timestamp(expires_at).map_err(Phase2Error::from)?,
            partner_chooses: open,
        };
        state.trade_offers.insert(
            offer_id,
            TradeOfferRecord {
                view: view.clone(),
                fences,
                consents,
                expires_at,
                offered,
            },
        );
        insert_trade_offer_receipt(
            state,
            actor.character_id,
            request.idempotency_key,
            fingerprint,
            view.clone(),
            now,
        );
        Ok(view)
    })
}

/// Returns one offer visible to either immutable participant without changing
/// its deadline. Terminal views stay readable inside their bounded retention.
pub(crate) fn get_offer(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    offer_id: TradeOfferId,
    fence: LeaseFence,
) -> Result<TradeOfferView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
        super::group_travel::lease_matches(state, actor.character_id, fence, now)?;
        prune_trade_offer_state(state, now);
        let record = state
            .trade_offers
            .get(&offer_id)
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || !record.view.members.contains(&actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        Ok(record.view.clone())
    })
}

/// Returns the group's pending open offer with the offered Pokémon, for the
/// partner's in-game prompt. Either member may read it; the initiator's
/// launcher ignores its own. `NotFound` means no pending open offer.
pub(crate) fn current_offer(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    fence: LeaseFence,
) -> Result<TradeOfferCurrentView, Phase2Error> {
    let now = store.now();
    store.write_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
        super::group_travel::lease_matches(state, actor.character_id, fence, now)?;
        prune_trade_offer_state(state, now);
        offer_group(state, group_id, actor.character_id)?;
        state
            .trade_offers
            .values()
            .filter(|record| {
                record.view.group_id == group_id
                    && record.view.status == TradeOfferStatus::Pending
                    && record.expires_at > now
                    && record.view.members.contains(&actor.character_id)
            })
            .find_map(|record| {
                record.offered.map(|offered| TradeOfferCurrentView {
                    offer: record.view,
                    offered,
                })
            })
            .ok_or(Phase2Error::NotFound)
    })
}

/// Reads both members' slot records for a pending offer. Object reads stay
/// outside the repository transaction; the caller rechecks that the heads
/// are still current before issuing ledger entries.
///
/// A strict offer reads both anchored heads. An open offer reads the
/// initiator's anchored head and the accepting member's current head at the
/// slot and Pokémon it chose.
fn accepted_trade_sides(
    store: &Store,
    offer_id: TradeOfferId,
    accepter: CharacterId,
    request: &TradeDecisionRequest,
) -> Result<[super::ledger::TradeSide; 2], Phase2Error> {
    let (record, snapshots) = store.read_transaction(|state| {
        let record = state
            .trade_offers
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        let mut snapshots = record
            .view
            .snapshots
            .map(|snapshot_id| state.snapshots.get(&snapshot_id).cloned());
        if record.view.partner_chooses {
            let index = record
                .view
                .members
                .iter()
                .position(|member| *member == accepter)
                .ok_or(Phase2Error::NotFound)?;
            snapshots[index] = Some(current_head(state, accepter)?);
        }
        Ok::<_, Phase2Error>((record, snapshots))
    })?;
    let mut sides = Vec::with_capacity(2);
    for (index, snapshot) in snapshots.into_iter().enumerate() {
        let snapshot = snapshot.ok_or(Phase2Error::Conflict)?;
        let member = record.view.members[index];
        if record.view.partner_chooses && member == accepter {
            sides.push(read_offered_side(
                store,
                &snapshot,
                request.fence,
                request.own_slot,
                request.own_pokemon,
            )?);
            continue;
        }
        if snapshot.character_id != member || snapshot.revision != record.view.revisions[index] {
            return Err(Phase2Error::Conflict);
        }
        let verified = verify_source(
            store,
            &snapshot,
            record.fences[index],
            record.view.slots[index],
        )?;
        let side = super::ledger::trade_side(
            &verified.save,
            snapshot.snapshot_id,
            snapshot.revision,
            record.view.slots[index],
        )?;
        if member == accepter
            && request.own_pokemon.is_some_and(|key| {
                key.personality != side.key.personality || key.ot_id != side.key.ot_id
            })
        {
            return Err(Phase2Error::Conflict);
        }
        sides.push(side);
    }
    sides.try_into().map_err(|_| Phase2Error::Internal)
}

/// Records one consent decision. Reject is terminal for either member under
/// the caller's own fence. Accept additionally requires the exact live group,
/// both live leases, unchanged anchored heads, and the exact reciprocal
/// slots; the initiator can never accept their own offer. The accepting
/// decision atomically issues one Trade ledger entry per member and is
/// refused with `Conflict` while either member has an open entry, and with
/// `TradePokemonHoldsMail` when either offered Pokémon holds mail.
pub(crate) fn decide_offer(
    store: &Store,
    actor: AuthenticatedActor,
    group_id: GroupId,
    offer_id: TradeOfferId,
    request: &TradeDecisionRequest,
) -> Result<TradeOfferView, Phase2Error> {
    if request.api_version != ApiVersion::V1 {
        return Err(Phase2Error::InvalidRequest);
    }
    if request.offer_id != offer_id {
        return Err(Phase2Error::NotFound);
    }
    let fingerprint = offer_fingerprint(OP_TRADE_DECIDE, &(group_id, request))?;
    // Read failures are deferred: a replay or an earlier refusal must keep
    // its own result, and an accept without both slot records fails closed.
    let (trade_sides, commit_candidates) = if request.decision == TradeDecision::Accept {
        let candidates: Vec<CommitId> = (0..MAX_TRADE_OFFER_ID_CANDIDATES)
            .map(|_| CommitId::new(store.random_uuid()?).map_err(|_| Phase2Error::Internal))
            .collect::<Result<_, _>>()?;
        (
            Some(accepted_trade_sides(
                store,
                offer_id,
                actor.character_id,
                request,
            )),
            candidates,
        )
    } else {
        (None, Vec::new())
    };
    let now = store.now();
    store.write_transaction(|state| {
        super::group_travel::authenticate_caller(state, actor, actor.character_id)?;
        super::group_travel::lease_matches(state, actor.character_id, request.fence, now)?;
        prune_trade_offer_state(state, now);
        if let Some(replay) = trade_offer_idempotency_lookup(
            state,
            actor.character_id,
            request.idempotency_key,
            fingerprint,
        )? {
            return Ok(replay);
        }
        let record = state
            .trade_offers
            .get(&offer_id)
            .cloned()
            .ok_or(Phase2Error::NotFound)?;
        if record.view.group_id != group_id || !record.view.members.contains(&actor.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let caller_index = record
            .view
            .members
            .iter()
            .position(|member| *member == actor.character_id)
            .ok_or(Phase2Error::Internal)?;
        let is_initiator = actor.character_id == record.view.initiator;
        match request.decision {
            TradeDecision::Reject => match record.view.status {
                TradeOfferStatus::Rejected => Ok(record.view),
                TradeOfferStatus::Pending => {
                    if record.expires_at <= now {
                        mark_offer_expired(state, &record.view.offer_id);
                        return Err(Phase2Error::Expired);
                    }
                    ensure_trade_offer_receipt_slot(state, actor.character_id)?;
                    let mut view = record.view.clone();
                    view.status = TradeOfferStatus::Rejected;
                    let stored = state
                        .trade_offers
                        .get_mut(&offer_id)
                        .ok_or(Phase2Error::Conflict)?;
                    stored.view = view.clone();
                    stored.consents[caller_index] = false;
                    insert_trade_offer_receipt(
                        state,
                        actor.character_id,
                        request.idempotency_key,
                        fingerprint,
                        view.clone(),
                        now,
                    );
                    Ok(view)
                }
                TradeOfferStatus::Accepted => Err(Phase2Error::Conflict),
                TradeOfferStatus::Expired => Err(Phase2Error::Expired),
            },
            TradeDecision::Accept => {
                if is_initiator {
                    return Err(Phase2Error::Forbidden);
                }
                match record.view.status {
                    TradeOfferStatus::Accepted => return Ok(record.view),
                    TradeOfferStatus::Pending => {}
                    TradeOfferStatus::Rejected => return Err(Phase2Error::Conflict),
                    TradeOfferStatus::Expired => return Err(Phase2Error::Expired),
                }
                if record.expires_at <= now {
                    mark_offer_expired(state, &record.view.offer_id);
                    return Err(Phase2Error::Expired);
                }
                let group = state.groups.get(&group_id).ok_or(Phase2Error::Conflict)?;
                if group.status != GroupStatus::Active
                    || group.group.members() != record.view.members
                    || state.active_group_by_member.get(&record.view.members[0]) != Some(&group_id)
                    || state.active_group_by_member.get(&record.view.members[1]) != Some(&group_id)
                {
                    return Err(Phase2Error::Conflict);
                }
                live_member_lease(state, record.view.members[0], now)?;
                live_member_lease(state, record.view.members[1], now)?;
                // Mail is named explicitly: the ROM refuses a record that
                // carries mail, so an issued entry could never be applied.
                let sides = match &trade_sides {
                    Some(Ok(sides)) => *sides,
                    Some(Err(Phase2Error::TradePokemonHoldsMail)) => {
                        return Err(Phase2Error::TradePokemonHoldsMail);
                    }
                    _ => return Err(Phase2Error::Conflict),
                };
                // An open offer anchors the accepting side now: its current
                // head (the one just read), its fence and the slot it chose.
                let mut anchored = record.view;
                let mut fences = record.fences;
                if record.view.partner_chooses {
                    anchored.revisions[caller_index] = sides[caller_index].revision;
                    anchored.snapshots[caller_index] = sides[caller_index].snapshot_id;
                    anchored.slots[caller_index] = request.own_slot;
                    fences[caller_index] = request.fence;
                }
                for (index, member) in anchored.members.into_iter().enumerate() {
                    let lease = state.leases.get(&member).ok_or(Phase2Error::Conflict)?;
                    if lease.contract.fence() != fences[index]
                        || lease.contract.current_revision != anchored.revisions[index]
                    {
                        return Err(Phase2Error::Conflict);
                    }
                }
                if !offer_anchor_is_current(state, &anchored) {
                    return Err(Phase2Error::Conflict);
                }
                if anchored.slots[caller_index] != request.own_slot
                    || anchored.slots[1 - caller_index] != request.partner_slot
                {
                    return Err(Phase2Error::Conflict);
                }
                ensure_trade_offer_receipt_slot(state, actor.character_id)?;
                if (0..2).any(|index| {
                    sides[index].snapshot_id != anchored.snapshots[index]
                        || sides[index].revision != anchored.revisions[index]
                        || sides[index].slot != anchored.slots[index]
                }) {
                    return Err(Phase2Error::Conflict);
                }
                let mut fresh = commit_candidates
                    .iter()
                    .copied()
                    .filter(|candidate| !super::ledger::commit_id_in_use(state, *candidate));
                let first = fresh.next().ok_or(Phase2Error::Internal)?;
                let second = fresh
                    .find(|candidate| *candidate != first)
                    .ok_or(Phase2Error::Internal)?;
                super::ledger::issue_trade(
                    state,
                    offer_id,
                    anchored.members,
                    sides,
                    [first, second],
                    now,
                )?;
                let mut view = anchored;
                view.status = TradeOfferStatus::Accepted;
                let expires_at = now
                    .checked_add(TRADE_ACCEPTED_TTL_MS)
                    .ok_or(Phase2Error::Internal)?;
                view.expires_at = Store::unix_timestamp(expires_at).map_err(Phase2Error::from)?;
                let stored = state
                    .trade_offers
                    .get_mut(&offer_id)
                    .ok_or(Phase2Error::Conflict)?;
                stored.view = view.clone();
                stored.fences = fences;
                stored.consents[caller_index] = true;
                stored.expires_at = expires_at;
                insert_trade_offer_receipt(
                    state,
                    actor.character_id,
                    request.idempotency_key,
                    fingerprint,
                    view.clone(),
                    now,
                );
                Ok(view)
            }
        }
    })
}

fn mark_offer_expired(state: &mut State, offer_id: &TradeOfferId) {
    if let Some(record) = state.trade_offers.get_mut(offer_id)
        && matches!(
            record.view.status,
            TradeOfferStatus::Pending | TradeOfferStatus::Accepted
        )
    {
        record.view.status = TradeOfferStatus::Expired;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_cloud::{
        ApiVersion, CharacterCloudState, CharacterId, ClientInstanceId, Group, GroupId,
        LeaseContract, SessionEpoch, SessionId, SigningPrivateKey, SnapshotId, TradeOfferId,
        UnixTimestampMillis, UserId,
    };
    use coop_protocol::{RegionId, RegionalProgress, WorldZone};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::super::storage::{
        CharacterRecord, GroupRecord, InMemoryObjectStore, InMemoryRepository, LeaseRecord,
        ObjectStore, Phase2Config, Repository, StorageError,
    };

    #[derive(Clone)]
    struct FailingCommitRepository {
        inner: InMemoryRepository,
        fail_after_commit: Arc<AtomicBool>,
        fenced: Arc<AtomicBool>,
    }

    impl FailingCommitRepository {
        fn new() -> Self {
            Self {
                inner: InMemoryRepository::new(),
                fail_after_commit: Arc::new(AtomicBool::new(false)),
                fenced: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Repository for FailingCommitRepository {
        fn read_transaction(
            &self,
            operation: &mut dyn FnMut(&State) -> Result<(), StorageError>,
        ) -> Result<(), StorageError> {
            self.inner.read_transaction(operation)
        }

        fn write_transaction(
            &self,
            operation: &mut dyn FnMut(&mut State) -> Result<(), StorageError>,
        ) -> Result<(), StorageError> {
            let result = self.inner.write_transaction(operation);
            if self.fail_after_commit.load(Ordering::Acquire) && result.is_ok() {
                self.fenced.store(true, Ordering::Release);
                Err(StorageError::Persistence)
            } else {
                result
            }
        }

        fn is_fenced(&self) -> bool {
            self.fenced.load(Ordering::Acquire)
        }
    }

    fn id<T>(number: u128, build: fn(uuid::Uuid) -> Result<T, coop_cloud::IdError>) -> T {
        build(uuid::Uuid::from_u128(number)).expect("non-nil id")
    }

    fn accepted_record() -> TradeOfferRecord {
        let members = [id(1, CharacterId::new), id(2, CharacterId::new)];
        let revisions = [Revision::new(1), Revision::new(1)];
        let fences = std::array::from_fn(|index| {
            LeaseFence::new(
                id(10 + index as u128, SessionId::new),
                members[index],
                revisions[index],
                SessionEpoch::new(1).unwrap(),
                id(20 + index as u128, ClientInstanceId::new),
            )
        });
        TradeOfferRecord {
            view: TradeOfferView {
                api_version: ApiVersion::V1,
                offer_id: id(30, TradeOfferId::new),
                group_id: id(31, GroupId::new),
                members,
                initiator: members[0],
                slots: [
                    coop_cloud::PartyPosition::new(0).unwrap(),
                    coop_cloud::PartyPosition::new(0).unwrap(),
                ],
                revisions,
                snapshots: [id(40, SnapshotId::new), id(41, SnapshotId::new)],
                status: TradeOfferStatus::Accepted,
                expires_at: UnixTimestampMillis::new(100),
                partner_chooses: false,
            },
            fences,
            consents: [true, true],
            expires_at: 100,
            offered: None,
        }
    }

    fn trade_test_sav() -> Vec<u8> {
        let mut bytes = super::super::tests::valid_character_sav(false);
        let mut mon = [0_u8; 100];
        mon[19] = 2; // hasSpecies
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

    fn snapshot(
        character: CharacterId,
        snapshot_id: SnapshotId,
        revision: Revision,
    ) -> SnapshotRecord {
        snapshot_for_sav(character, snapshot_id, revision, &trade_test_sav())
    }

    fn snapshot_for_sav(
        character: CharacterId,
        snapshot_id: SnapshotId,
        revision: Revision,
        sav_bytes: &[u8],
    ) -> SnapshotRecord {
        let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, sav_bytes).unwrap();
        let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
        SnapshotRecord::new(
            snapshot_id,
            SnapshotFence::new(
                id(60, SessionId::new),
                character,
                SessionEpoch::new(1).unwrap(),
            ),
            Revision::new(revision.value() - 1),
            revision,
            vec![sav, pending.clone()],
            pending.sha256,
            None,
            UnixTimestampMillis::new(1),
        )
        .unwrap()
    }

    fn anchored_state(record: &TradeOfferRecord) -> State {
        let mut state = State::default();
        let zone = WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        state.groups.insert(
            record.view.group_id,
            GroupRecord {
                group: Group::new(record.view.members[0], record.view.members[1]).unwrap(),
                zone: zone.clone(),
                status: GroupStatus::Active,
                zone_revision: 0,
            },
        );
        for (index, character_id) in record.view.members.into_iter().enumerate() {
            let first_id = id(50 + index as u128, SnapshotId::new);
            let current = snapshot(
                character_id,
                record.view.snapshots[index],
                record.view.revisions[index],
            );
            let first = snapshot(character_id, first_id, Revision::new(1));
            state
                .active_group_by_member
                .insert(character_id, record.view.group_id);
            state.characters.insert(
                character_id,
                CharacterRecord {
                    owner: id(70 + index as u128, UserId::new),
                    state: CharacterCloudState::new(
                        character_id,
                        zone.clone(),
                        vec![RegionalProgress::new(RegionId::Hoenn, 0, 0, vec![], vec![]).unwrap()],
                    )
                    .unwrap(),
                    revision: record.view.revisions[index],
                    world_revision: 0,
                    active_snapshot: Some(record.view.snapshots[index]),
                    last_session_epoch: 1,
                },
            );
            state.leases.insert(
                character_id,
                LeaseRecord {
                    contract: LeaseContract::new(
                        record.fences[index],
                        UnixTimestampMillis::new(200),
                        1_000,
                    )
                    .unwrap(),
                    grace_until: 200,
                    released: false,
                    reconnect: None,
                    release_keys: vec![],
                },
            );
            state
                .snapshot_by_revision
                .insert((character_id, Revision::new(1)), first_id);
            state.snapshot_by_revision.insert(
                (character_id, record.view.revisions[index]),
                record.view.snapshots[index],
            );
            state.snapshots.insert(first_id, first);
            state
                .snapshots
                .insert(record.view.snapshots[index], current);
        }
        state
    }

    fn test_store_with_repository(
        repository: Arc<dyn Repository>,
        objects: Arc<InMemoryObjectStore>,
    ) -> Store {
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "trade-test-key",
        )
        .unwrap()
        .with_test_adapters(
            Arc::new(super::super::storage::FixedClock::new(1_000)),
            Arc::new(super::super::storage::FixedEntropy::new(
                (0_u8..=255).collect(),
            )),
        )
        .with_adapters(repository, objects);
        Store::new(config).unwrap()
    }

    fn test_store(objects: Arc<InMemoryObjectStore>) -> Store {
        test_store_with_repository(Arc::new(InMemoryRepository::new()), objects)
    }

    fn ready_stage(record: &TradeOfferRecord) -> TradeStageRecord {
        let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
        let sav =
            SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &trade_test_sav()).unwrap();
        let validated = validate_character_sav(&trade_test_sav(), Revision::new(1)).unwrap();
        let slot_sha256 = match validated.party_pokemon(0).unwrap() {
            coop_save::PokemonSlot::Occupied(record) => Sha256Digest::of_bytes(&record.raw),
            coop_save::PokemonSlot::Empty { .. } => panic!("trade fixture slot is occupied"),
        };
        let sources = std::array::from_fn(|index| TradeSourceCommitment {
            character_id: record.view.members[index],
            snapshot_id: record.view.snapshots[index],
            revision: record.view.revisions[index],
            fence: record.fences[index],
            slot: record.view.slots[index],
            save_sha256: sav.sha256,
            pending_commits_sha256: pending.sha256,
            slot_sha256,
            last_applied_commit: None,
        });
        let outputs = std::array::from_fn(|index| TradeStagedMember {
            character_id: record.view.members[index],
            snapshot_id: id(80 + index as u128, SnapshotId::new),
            revision: Revision::new(2),
            fence: record.fences[index],
            source: sources[index],
            files: [sav.clone(), pending.clone()],
        });
        TradeStageRecord {
            offer_id: record.view.offer_id,
            idempotency_key: id(90, coop_cloud::IdempotencyKey::new),
            sources,
            outputs,
            reserved_bytes: [
                files_storage_usage(&[sav.clone(), pending.clone()]).unwrap(),
                files_storage_usage(&[sav, pending]).unwrap(),
            ],
            status: TradeStageStatus::Ready,
            expires_at: 50,
            receipt: None,
        }
    }

    fn install_stage(store: &Store, record: &TradeOfferRecord, stage: &TradeStageRecord) {
        store
            .write_transaction(|state| {
                let mut next = anchored_state(record);
                next.trade_offers
                    .insert(record.view.offer_id, record.clone());
                next.trade_staging.insert(stage.offer_id, stage.clone());
                *state = next;
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        for source in &stage.sources {
            let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, &trade_test_sav())
                .unwrap();
            let pending =
                SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
            for file in [sav, pending] {
                let bytes = if file.artifact == ArtifactIdentity::CharacterSav {
                    trade_test_sav()
                } else {
                    b"[]".to_vec()
                };
                store
                    .objects
                    .put(
                        Store::object_key(source.character_id, source.snapshot_id, file.artifact),
                        bytes,
                    )
                    .unwrap();
            }
        }
        for output in &stage.outputs {
            for file in &output.files {
                let bytes = if file.artifact == ArtifactIdentity::CharacterSav {
                    trade_test_sav()
                } else {
                    b"[]".to_vec()
                };
                store
                    .objects
                    .put(
                        Store::object_key(output.character_id, output.snapshot_id, file.artifact),
                        bytes,
                    )
                    .unwrap();
            }
        }
    }

    #[test]
    fn staged_output_keys_are_pair_owned() {
        let member = id(1, CharacterId::new);
        let offer = id(2, TradeOfferId::new);
        let fence = LeaseFence::new(
            id(3, SessionId::new),
            member,
            Revision::new(2),
            SessionEpoch::new(1).unwrap(),
            id(4, ClientInstanceId::new),
        );
        let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
        let source = TradeSourceCommitment {
            character_id: member,
            snapshot_id: id(5, SnapshotId::new),
            revision: Revision::new(2),
            fence,
            slot: coop_cloud::PartyPosition::new(0).unwrap(),
            save_sha256: Sha256Digest::of_bytes(b"sav"),
            pending_commits_sha256: pending.sha256,
            slot_sha256: Sha256Digest::of_bytes(b"slot"),
            last_applied_commit: None,
        };
        let stage = TradeStageRecord {
            offer_id: offer,
            idempotency_key: id(6, coop_cloud::IdempotencyKey::new),
            sources: [source, source],
            outputs: [
                TradeStagedMember {
                    character_id: member,
                    snapshot_id: id(7, SnapshotId::new),
                    revision: Revision::new(3),
                    fence,
                    source,
                    files: [pending.clone(), pending.clone()],
                },
                TradeStagedMember {
                    character_id: member,
                    snapshot_id: id(8, SnapshotId::new),
                    revision: Revision::new(3),
                    fence,
                    source,
                    files: [pending.clone(), pending],
                },
            ],
            reserved_bytes: [4, 4],
            status: TradeStageStatus::Staging,
            expires_at: 10,
            receipt: None,
        };
        let keys: Vec<_> = staged_keys(&stage).collect();
        assert_eq!(keys.len(), 4);
        assert!(keys.iter().all(|key| key.contains("snapshots/")));
    }

    #[test]
    fn source_metadata_rejects_changed_snapshot_declaration() {
        let member = id(1, CharacterId::new);
        let snapshot_id = id(2, SnapshotId::new);
        let fence = LeaseFence::new(
            id(3, SessionId::new),
            member,
            Revision::new(2),
            SessionEpoch::new(1).unwrap(),
            id(4, ClientInstanceId::new),
        );
        let pending = SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap();
        let sav = SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"sav").unwrap();
        let source = TradeSourceCommitment {
            character_id: member,
            snapshot_id,
            revision: Revision::new(2),
            fence,
            slot: coop_cloud::PartyPosition::new(0).unwrap(),
            save_sha256: sav.sha256,
            pending_commits_sha256: pending.sha256,
            slot_sha256: Sha256Digest::of_bytes(b"slot"),
            last_applied_commit: None,
        };
        let snapshot = SnapshotRecord::new(
            snapshot_id,
            SnapshotFence::new(fence.session_id, member, fence.session_epoch),
            Revision::new(1),
            Revision::new(2),
            vec![sav, pending.clone()],
            pending.sha256,
            None,
            coop_cloud::UnixTimestampMillis::new(1),
        )
        .unwrap();
        assert!(source_matches_snapshot(&snapshot, source));
        let changed = SnapshotRecord::new(
            snapshot_id,
            SnapshotFence::new(fence.session_id, member, fence.session_epoch),
            Revision::new(1),
            Revision::new(2),
            vec![
                SnapshotFile::from_bytes(ArtifactIdentity::CharacterSav, b"changed").unwrap(),
                SnapshotFile::from_bytes(ArtifactIdentity::PendingCommits, b"[]").unwrap(),
            ],
            pending.sha256,
            None,
            coop_cloud::UnixTimestampMillis::new(1),
        )
        .unwrap();
        assert!(!source_matches_snapshot(&changed, source));
    }

    #[test]
    fn consent_and_expiry_are_required_before_any_trade_anchor() {
        let record = accepted_record();
        let mut no_consent = record.clone();
        no_consent.consents = [true, false];
        assert_eq!(
            validate_trade_anchor(&State::default(), &no_consent, 1),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            validate_trade_anchor(&State::default(), &record, 100),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn publish_replays_exact_idempotency_without_advancing_twice() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let store = test_store(objects);
        let record = accepted_record();
        let stage = ready_stage(&record);
        install_stage(&store, &record, &stage);
        let receipt =
            publish_trade(&store, record.view.offer_id, stage.idempotency_key, 1).unwrap();
        assert_eq!(
            publish_trade(&store, record.view.offer_id, stage.idempotency_key, 1).unwrap(),
            receipt
        );
        let state = store
            .read_transaction(|state| Ok::<State, Phase2Error>(state.clone()))
            .unwrap();
        for member in record.view.members {
            assert_eq!(state.characters[&member].revision, Revision::new(2));
        }
        assert_eq!(state.snapshots.len(), 6);
        let other_key = id(91, coop_cloud::IdempotencyKey::new);
        assert_eq!(
            publish_trade(&store, record.view.offer_id, other_key, 1),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn ambiguous_repository_commit_keeps_objects_for_restart_reconciliation() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let repository = FailingCommitRepository::new();
        let store = test_store_with_repository(Arc::new(repository.clone()), objects.clone());
        let record = accepted_record();
        let stage = ready_stage(&record);
        install_stage(&store, &record, &stage);
        repository.fail_after_commit.store(true, Ordering::Release);

        assert_eq!(
            publish_trade(&store, record.view.offer_id, stage.idempotency_key, 1),
            Err(Phase2Error::Internal)
        );
        assert!(repository.is_fenced());
        assert_eq!(objects.object_count().unwrap(), 8);
        let state = store
            .read_transaction(|state| Ok::<State, Phase2Error>(state.clone()))
            .unwrap();
        assert!(state.trade_receipts.contains_key(&record.view.offer_id));
        assert_eq!(
            state.trade_staging[&record.view.offer_id].status,
            TradeStageStatus::Published
        );

        let restarted =
            test_store_with_repository(Arc::new(repository.inner.clone()), objects.clone());
        assert_eq!(
            publish_trade(&restarted, record.view.offer_id, stage.idempotency_key, 1)
                .unwrap()
                .snapshots,
            [stage.outputs[0].snapshot_id, stage.outputs[1].snapshot_id]
        );
        assert_eq!(objects.object_count().unwrap(), 8);
    }

    #[test]
    fn live_trade_reservation_counts_against_character_quota() {
        let record = accepted_record();
        let mut stage = ready_stage(&record);
        stage.reserved_bytes = [super::super::storage::MAX_SNAPSHOT_STORAGE_BYTES; 2];
        let mut state = anchored_state(&record);
        state.trade_staging.insert(stage.offer_id, stage.clone());
        assert_eq!(
            ensure_trade_snapshot_quota(
                &state,
                record.view.members[0],
                &stage.outputs[0].files,
                None,
            ),
            Err(Phase2Error::Busy)
        );
        assert_eq!(
            super::super::saves::ensure_snapshot_quota(
                &state,
                record.view.members[0],
                &stage.outputs[0].files,
                None,
                None,
            ),
            Err(Phase2Error::Busy)
        );
    }

    #[test]
    fn stale_source_fences_abort_without_partial_heads() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let store = test_store(objects.clone());
        let record = accepted_record();
        let stage = ready_stage(&record);
        install_stage(&store, &record, &stage);
        store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&record.view.members[0])
                    .unwrap()
                    .revision = Revision::new(3);
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        assert_eq!(
            publish_trade(&store, record.view.offer_id, stage.idempotency_key, 1),
            Err(Phase2Error::Conflict)
        );
        let state = store
            .read_transaction(|state| Ok::<State, Phase2Error>(state.clone()))
            .unwrap();
        assert_eq!(state.snapshots.len(), 4);
        assert!(!state.trade_staging.contains_key(&record.view.offer_id));
        assert_eq!(objects.object_count().unwrap(), 4);
    }

    #[test]
    fn missing_staged_object_seals_the_pair_before_rejecting() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let store = test_store(objects.clone());
        let record = accepted_record();
        let stage = ready_stage(&record);
        install_stage(&store, &record, &stage);
        let key = Store::object_key(
            stage.outputs[0].character_id,
            stage.outputs[0].snapshot_id,
            stage.outputs[0].files[0].artifact,
        );
        objects.delete_if_present(&key).unwrap();
        assert_eq!(
            publish_trade(&store, record.view.offer_id, stage.idempotency_key, 1),
            Err(Phase2Error::Conflict)
        );
        let state = store
            .read_transaction(|state| Ok::<State, Phase2Error>(state.clone()))
            .unwrap();
        assert_eq!(state.snapshots.len(), 4);
        assert!(!state.trade_staging.contains_key(&record.view.offer_id));
        assert_eq!(objects.object_count().unwrap(), 4);
    }

    #[test]
    fn recovery_seals_expired_staging_without_releasing_source_objects() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let store = test_store(objects.clone());
        let record = accepted_record();
        let mut stage = ready_stage(&record);
        stage.status = TradeStageStatus::Staging;
        install_stage(&store, &record, &stage);
        assert_eq!(recover_trade_stages(&store, 51).unwrap(), 1);
        let state = store
            .read_transaction(|state| Ok::<State, Phase2Error>(state.clone()))
            .unwrap();
        assert!(!state.trade_staging.contains_key(&record.view.offer_id));
        assert_eq!(objects.object_count().unwrap(), 4);
    }

    #[test]
    fn store_startup_runs_trade_recovery_before_use() {
        let objects = Arc::new(InMemoryObjectStore::new());
        let repository = InMemoryRepository::new();
        let store = test_store_with_repository(Arc::new(repository.clone()), objects.clone());
        let record = accepted_record();
        let mut stage = ready_stage(&record);
        stage.status = TradeStageStatus::Staging;
        install_stage(&store, &record, &stage);

        let _restarted = test_store_with_repository(Arc::new(repository), objects.clone());
        assert_eq!(objects.object_count().unwrap(), 4);
        assert!(
            !_restarted
                .read_transaction(|state| Ok::<bool, Phase2Error>(
                    state.trade_staging.contains_key(&record.view.offer_id)
                ))
                .unwrap()
        );
    }

    struct ConsentFixture {
        store: Store,
        clock: Arc<super::super::storage::FixedClock>,
        group_id: GroupId,
        members: [CharacterId; 2],
        actors: [super::super::AuthenticatedActor; 2],
        fences: [LeaseFence; 2],
        snapshots: [SnapshotId; 2],
        revision: Revision,
    }

    fn consent_fixture() -> ConsentFixture {
        consent_fixture_with_saves(consent_member_sav)
    }

    fn consent_fixture_with_saves(member_sav: fn(usize) -> Vec<u8>) -> ConsentFixture {
        use super::super::storage::{FixedClock, FixedEntropy, UserRecord};
        let clock = Arc::new(FixedClock::new(1_000_000));
        let config = Phase2Config::local(
            vec![0x55; 32],
            SigningPrivateKey::from_bytes([7; 32]),
            "trade-consent-key",
        )
        .unwrap()
        .with_test_adapters(
            clock.clone(),
            Arc::new(FixedEntropy::new((0_u8..=255).collect())),
        );
        let store = Store::new(config).unwrap();
        let revision = Revision::new(2);
        let members = [id(1, CharacterId::new), id(2, CharacterId::new)];
        let group_id = id(31, GroupId::new);
        let fences = std::array::from_fn(|index| {
            LeaseFence::new(
                id(10 + index as u128, SessionId::new),
                members[index],
                revision,
                SessionEpoch::new(1).unwrap(),
                id(20 + index as u128, ClientInstanceId::new),
            )
        });
        let snapshots = [id(40, SnapshotId::new), id(41, SnapshotId::new)];
        let zone = WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1).unwrap();
        store
            .write_transaction(|state| {
                let mut next = State::default();
                next.groups.insert(
                    group_id,
                    GroupRecord {
                        group: Group::new(members[0], members[1]).unwrap(),
                        zone: zone.clone(),
                        status: GroupStatus::Active,
                        zone_revision: 0,
                    },
                );
                for (index, member) in members.into_iter().enumerate() {
                    let user_id = id(70 + index as u128, UserId::new);
                    let user = UserRecord {
                        user_id,
                        username: coop_cloud::Username::new(format!("trade-user-{index}")).unwrap(),
                        password_phc: String::new(),
                        character_id: member,
                        disabled: false,
                    };
                    next.users_by_id.insert(user_id, user.clone());
                    next.users_by_name
                        .insert(format!("trade-user-{index}"), user);
                    let first_id = id(50 + index as u128, SnapshotId::new);
                    next.snapshot_by_revision
                        .insert((member, Revision::new(1)), first_id);
                    next.snapshots
                        .insert(first_id, snapshot(member, first_id, Revision::new(1)));
                    next.snapshot_by_revision
                        .insert((member, revision), snapshots[index]);
                    next.snapshots.insert(
                        snapshots[index],
                        snapshot_for_sav(member, snapshots[index], revision, &member_sav(index)),
                    );
                    next.active_group_by_member.insert(member, group_id);
                    next.characters.insert(
                        member,
                        CharacterRecord {
                            owner: user_id,
                            state: CharacterCloudState::new(
                                member,
                                zone.clone(),
                                vec![
                                    RegionalProgress::new(RegionId::Hoenn, 0, 0, vec![], vec![])
                                        .unwrap(),
                                ],
                            )
                            .unwrap(),
                            revision,
                            world_revision: 0,
                            active_snapshot: Some(snapshots[index]),
                            last_session_epoch: 1,
                        },
                    );
                    next.leases.insert(
                        member,
                        LeaseRecord {
                            contract: LeaseContract::new(
                                fences[index],
                                UnixTimestampMillis::new(1_600_000),
                                1_000,
                            )
                            .unwrap(),
                            grace_until: 1_600_000,
                            released: false,
                            reconnect: None,
                            release_keys: vec![],
                        },
                    );
                }
                *state = next;
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        // Accepting reads both anchored heads to issue the trade ledger.
        for (index, member) in members.into_iter().enumerate() {
            for (artifact, bytes) in [
                (ArtifactIdentity::CharacterSav, member_sav(index)),
                (ArtifactIdentity::PendingCommits, b"[]".to_vec()),
            ] {
                store
                    .objects
                    .put(Store::object_key(member, snapshots[index], artifact), bytes)
                    .unwrap();
            }
        }
        let actors = std::array::from_fn(|index| super::super::AuthenticatedActor {
            user_id: id(70 + index as u128, UserId::new),
            character_id: members[index],
        });
        ConsentFixture {
            store,
            clock,
            group_id,
            members,
            actors,
            fences,
            snapshots,
            revision,
        }
    }

    /// Member `index` holds two distinct Pokémon in party slots 0 and 1.
    fn consent_member_party(index: usize) -> [[u8; 100]; 2] {
        let base = 24 * (1 + 2 * index as u32);
        [
            super::super::tests::test_party_record(base, 1 + 2 * index as u16),
            super::super::tests::test_party_record(base + 24, 2 + 2 * index as u16),
        ]
    }

    fn consent_member_sav(index: usize) -> Vec<u8> {
        super::super::tests::party_character_sav(2, &consent_member_party(index))
    }

    fn consent_offer_request(
        fixture: &ConsentFixture,
        caller: usize,
        key: u128,
    ) -> coop_cloud::TradeOfferRequest {
        coop_cloud::TradeOfferRequest {
            api_version: ApiVersion::V1,
            fence: fixture.fences[caller],
            group_id: fixture.group_id,
            own_slot: coop_cloud::PartyPosition::new(0).unwrap(),
            partner_slot: coop_cloud::PartyPosition::new(1),
            partner_expected_revision: Some(fixture.revision),
            own_pokemon: None,
            idempotency_key: id(key, coop_cloud::IdempotencyKey::new),
        }
    }

    fn consent_decision_request(
        fixture: &ConsentFixture,
        caller: usize,
        offer_id: TradeOfferId,
        own_slot: u8,
        partner_slot: u8,
        decision: coop_cloud::TradeDecision,
        key: u128,
    ) -> coop_cloud::TradeDecisionRequest {
        coop_cloud::TradeDecisionRequest {
            api_version: ApiVersion::V1,
            fence: fixture.fences[caller],
            offer_id,
            own_slot: coop_cloud::PartyPosition::new(own_slot).unwrap(),
            partner_slot: coop_cloud::PartyPosition::new(partner_slot).unwrap(),
            decision,
            own_pokemon: None,
            idempotency_key: id(key, coop_cloud::IdempotencyKey::new),
        }
    }

    fn canonical_index(view: &TradeOfferView, member: CharacterId) -> usize {
        view.members
            .iter()
            .position(|candidate| *candidate == member)
            .expect("member is anchored")
    }

    #[test]
    fn partner_accept_completes_reciprocal_consent() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        assert_eq!(pending.status, TradeOfferStatus::Pending);
        assert_eq!(pending.initiator, fixture.members[0]);
        assert_eq!(pending.revisions, [fixture.revision; 2]);
        assert_eq!(pending.snapshots, fixture.snapshots);
        assert_eq!(pending.expires_at.value(), 1_000_000 + TRADE_OFFER_TTL_MS);
        let caller_index = canonical_index(&pending, fixture.members[0]);
        assert_eq!(pending.slots[caller_index].index(), 0);
        assert_eq!(pending.slots[1 - caller_index].index(), 1);
        let record = fixture
            .store
            .read_transaction(|state| {
                Ok::<TradeOfferRecord, Phase2Error>(state.trade_offers[&pending.offer_id].clone())
            })
            .unwrap();
        assert_eq!(record.consents[caller_index], true);
        assert_eq!(record.consents[1 - caller_index], false);

        let partner_index = canonical_index(&pending, fixture.members[1]);
        let accept = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            pending.slots[partner_index].index() as u8,
            pending.slots[1 - partner_index].index() as u8,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        let accepted = decide_offer(
            &fixture.store,
            fixture.actors[1],
            fixture.group_id,
            pending.offer_id,
            &accept,
        )
        .unwrap();
        assert_eq!(accepted.status, TradeOfferStatus::Accepted);
        assert_eq!(accepted.expires_at.value(), 1_300_000);
        fixture.clock.advance(30_001);
        assert_eq!(
            get_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                fixture.fences[0],
            )
            .unwrap()
            .status,
            TradeOfferStatus::Accepted
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &accept,
            )
            .unwrap(),
            accepted
        );
        let fetched = get_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            pending.offer_id,
            fixture.fences[0],
        )
        .unwrap();
        assert_eq!(fetched.status, TradeOfferStatus::Accepted);
        fixture.clock.advance(TRADE_ACCEPTED_TTL_MS - 30_000);
        assert_eq!(
            get_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                fixture.fences[0],
            )
            .unwrap()
            .status,
            TradeOfferStatus::Expired
        );
    }

    #[test]
    fn reject_is_idempotent_and_terminal() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        let self_accept = consent_decision_request(
            &fixture,
            0,
            pending.offer_id,
            0,
            1,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                &self_accept,
            ),
            Err(Phase2Error::Forbidden)
        );
        let reject = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            1,
            0,
            coop_cloud::TradeDecision::Reject,
            102,
        );
        let rejected = decide_offer(
            &fixture.store,
            fixture.actors[1],
            fixture.group_id,
            pending.offer_id,
            &reject,
        )
        .unwrap();
        assert_eq!(rejected.status, TradeOfferStatus::Rejected);
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &reject,
            )
            .unwrap(),
            rejected
        );
        let reject_new_key = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            1,
            0,
            coop_cloud::TradeDecision::Reject,
            103,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &reject_new_key,
            )
            .unwrap()
            .status,
            TradeOfferStatus::Rejected
        );
        let late_accept = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            1,
            0,
            coop_cloud::TradeDecision::Accept,
            104,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &late_accept,
            ),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn stale_heads_fences_and_groups_reject_consent() {
        let fixture = consent_fixture();
        let mut wrong_revision = consent_offer_request(&fixture, 0, 100);
        wrong_revision.partner_expected_revision = Some(Revision::new(3));
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &wrong_revision,
            ),
            Err(Phase2Error::Conflict)
        );
        let mut stale_fence = consent_offer_request(&fixture, 0, 101);
        stale_fence.fence = LeaseFence::new(
            id(99, SessionId::new),
            fixture.members[0],
            fixture.revision,
            SessionEpoch::new(2).unwrap(),
            id(20, ClientInstanceId::new),
        );
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &stale_fence,
            ),
            Err(Phase2Error::Authentication)
        );
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 102),
        )
        .unwrap();
        fixture
            .store
            .write_transaction(|state| {
                state
                    .characters
                    .get_mut(&fixture.members[1])
                    .unwrap()
                    .revision = Revision::new(3);
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        let partner_index = canonical_index(&pending, fixture.members[1]);
        let accept = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            pending.slots[partner_index].index() as u8,
            pending.slots[1 - partner_index].index() as u8,
            coop_cloud::TradeDecision::Accept,
            103,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &accept,
            ),
            Err(Phase2Error::Conflict)
        );

        let regrouped = consent_fixture();
        let live = create_offer(
            &regrouped.store,
            regrouped.actors[0],
            regrouped.group_id,
            &consent_offer_request(&regrouped, 0, 100),
        )
        .unwrap();
        regrouped
            .store
            .write_transaction(|state| {
                state.groups.get_mut(&regrouped.group_id).unwrap().status = GroupStatus::Closed;
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        let regrouped_partner = canonical_index(&live, regrouped.members[1]);
        let closed_accept = consent_decision_request(
            &regrouped,
            1,
            live.offer_id,
            live.slots[regrouped_partner].index() as u8,
            live.slots[1 - regrouped_partner].index() as u8,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        assert_eq!(
            decide_offer(
                &regrouped.store,
                regrouped.actors[1],
                regrouped.group_id,
                live.offer_id,
                &closed_accept,
            ),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn conflicting_duplicate_keys_are_rejected() {
        let fixture = consent_fixture();
        let first = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &consent_offer_request(&fixture, 0, 100),
            )
            .unwrap()
            .offer_id,
            first.offer_id
        );
        let mut conflict = consent_offer_request(&fixture, 0, 100);
        conflict.partner_slot = coop_cloud::PartyPosition::new(2);
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &conflict,
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &consent_offer_request(&fixture, 0, 101),
            ),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn exhausted_receipts_leave_offer_pending_for_both_decisions() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        fixture
            .store
            .write_transaction(|state| {
                for index in 0..MAX_TRADE_OFFER_RECEIPTS_PER_MEMBER {
                    state.trade_offer_idempotency.insert(
                        (
                            fixture.members[1],
                            id(1_000 + index as u128, coop_cloud::IdempotencyKey::new),
                        ),
                        TradeOfferIdempotencyRecord {
                            fingerprint: [0; 32],
                            view: pending.clone(),
                            expires_at: 1_500_000,
                        },
                    );
                }
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        let partner_index = canonical_index(&pending, fixture.members[1]);
        for (decision, key) in [
            (coop_cloud::TradeDecision::Reject, 200),
            (coop_cloud::TradeDecision::Accept, 201),
        ] {
            let request = consent_decision_request(
                &fixture,
                1,
                pending.offer_id,
                pending.slots[partner_index].index() as u8,
                pending.slots[1 - partner_index].index() as u8,
                decision,
                key,
            );
            assert_eq!(
                decide_offer(
                    &fixture.store,
                    fixture.actors[1],
                    fixture.group_id,
                    pending.offer_id,
                    &request,
                ),
                Err(Phase2Error::Busy)
            );
            assert_eq!(
                get_offer(
                    &fixture.store,
                    fixture.actors[0],
                    fixture.group_id,
                    pending.offer_id,
                    fixture.fences[0],
                )
                .unwrap()
                .status,
                TradeOfferStatus::Pending
            );
        }
    }

    #[test]
    fn initiator_lease_rollover_invalidates_acceptance() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        let replacement = LeaseFence::new(
            id(99, SessionId::new),
            fixture.members[0],
            fixture.revision,
            SessionEpoch::new(2).unwrap(),
            id(20, ClientInstanceId::new),
        );
        fixture
            .store
            .write_transaction(|state| {
                state.leases.get_mut(&fixture.members[0]).unwrap().contract =
                    LeaseContract::new(replacement, UnixTimestampMillis::new(1_600_000), 1_000)
                        .unwrap();
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        let partner_index = canonical_index(&pending, fixture.members[1]);
        let request = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            pending.slots[partner_index].index() as u8,
            pending.slots[1 - partner_index].index() as u8,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &request,
            ),
            Err(Phase2Error::Conflict)
        );
    }

    #[test]
    fn expired_offers_reject_decisions_but_stay_readable() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        fixture.clock.advance(TRADE_OFFER_TTL_MS + 1);
        let accept = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            1,
            0,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &accept,
            ),
            Err(Phase2Error::Expired)
        );
        let reject = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            1,
            0,
            coop_cloud::TradeDecision::Reject,
            102,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &reject,
            ),
            Err(Phase2Error::Expired)
        );
        assert_eq!(
            get_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                fixture.fences[0],
            )
            .unwrap()
            .status,
            TradeOfferStatus::Expired
        );
    }

    fn accept_fixture_offer(fixture: &ConsentFixture, create_key: u128) -> TradeOfferView {
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(fixture, 0, create_key),
        )
        .unwrap();
        let partner_index = canonical_index(&pending, fixture.members[1]);
        decide_offer(
            &fixture.store,
            fixture.actors[1],
            fixture.group_id,
            pending.offer_id,
            &consent_decision_request(
                fixture,
                1,
                pending.offer_id,
                pending.slots[partner_index].index() as u8,
                pending.slots[1 - partner_index].index() as u8,
                coop_cloud::TradeDecision::Accept,
                create_key + 1,
            ),
        )
        .unwrap()
    }

    fn ledger_state<T>(fixture: &ConsentFixture, read: impl FnOnce(&State) -> T) -> T {
        fixture.store.inspect_state(read).unwrap()
    }

    fn open_trade_entry(
        fixture: &ConsentFixture,
        member: usize,
    ) -> super::super::ledger::LedgerEntry {
        ledger_state(fixture, |state| {
            super::super::ledger::open_entry(state, fixture.members[member])
                .cloned()
                .expect("open trade entry")
        })
    }

    /// The fixture member (not canonical) index of a character.
    fn fixture_index(fixture: &ConsentFixture, member: CharacterId) -> usize {
        fixture
            .members
            .iter()
            .position(|candidate| *candidate == member)
            .expect("fixture member")
    }

    #[test]
    fn accept_issues_both_trade_ledger_entries_with_exact_partner_records() {
        let fixture = consent_fixture();
        let accepted = accept_fixture_offer(&fixture, 100);
        assert_eq!(accepted.status, TradeOfferStatus::Accepted);
        assert_eq!(
            ledger_state(&fixture, |state| state.ledger_entries.len()),
            2
        );
        for member in 0..2 {
            let index = canonical_index(&accepted, fixture.members[member]);
            let partner = fixture_index(&fixture, accepted.members[1 - index]);
            let entry = open_trade_entry(&fixture, member);
            assert_eq!(entry.character_id, fixture.members[member]);
            assert_eq!(
                entry.origin,
                super::super::ledger::LedgerOrigin::Trade {
                    offer_id: accepted.offer_id
                }
            );
            assert_eq!(entry.status, super::super::ledger::LedgerStatus::Issued);
            assert_eq!(entry.base_snapshot_id, accepted.snapshots[index]);
            assert_eq!(entry.base_revision, accepted.revisions[index]);
            let expected_incoming =
                consent_member_party(partner)[accepted.slots[1 - index].index()];
            let expected_outgoing = consent_member_party(member)[accepted.slots[index].index()];
            let super::super::ledger::ExpectedDelta::Trade {
                slot,
                outgoing,
                incoming_raw,
                incoming_key,
            } = entry.expected
            else {
                panic!("trade delta");
            };
            assert_eq!(slot, accepted.slots[index]);
            assert_eq!(incoming_raw, expected_incoming);
            assert_eq!(
                outgoing.personality,
                u32::from_le_bytes(expected_outgoing[0..4].try_into().unwrap())
            );
            assert_eq!(
                incoming_key.personality,
                u32::from_le_bytes(expected_incoming[0..4].try_into().unwrap())
            );
        }

        // Issuance is idempotent per (origin, character): a replayed decision
        // and a direct reissue both return the existing entries.
        let partner_index = canonical_index(&accepted, fixture.members[1]);
        let replay = decide_offer(
            &fixture.store,
            fixture.actors[1],
            fixture.group_id,
            accepted.offer_id,
            &consent_decision_request(
                &fixture,
                1,
                accepted.offer_id,
                accepted.slots[partner_index].index() as u8,
                accepted.slots[1 - partner_index].index() as u8,
                coop_cloud::TradeDecision::Accept,
                101,
            ),
        )
        .unwrap();
        assert_eq!(replay, accepted);
        let existing = accepted
            .members
            .map(|member| open_trade_entry(&fixture, fixture_index(&fixture, member)).commit_id);
        let partner_index = canonical_index(&accepted, fixture.members[1]);
        let sides = accepted_trade_sides(
            &fixture.store,
            accepted.offer_id,
            fixture.members[1],
            &consent_decision_request(
                &fixture,
                1,
                accepted.offer_id,
                accepted.slots[partner_index].index() as u8,
                accepted.slots[1 - partner_index].index() as u8,
                coop_cloud::TradeDecision::Accept,
                101,
            ),
        )
        .unwrap();
        let reissued = fixture
            .store
            .write_transaction(|state| {
                super::super::ledger::issue_trade(
                    state,
                    accepted.offer_id,
                    accepted.members,
                    sides,
                    [id(900, CommitId::new), id(901, CommitId::new)],
                    1,
                )
            })
            .unwrap();
        assert_eq!(reissued, existing);
        assert_eq!(
            ledger_state(&fixture, |state| state.ledger_entries.len()),
            2
        );
    }

    #[test]
    fn second_trade_accept_is_refused_while_a_ledger_entry_is_open() {
        let fixture = consent_fixture();
        let first = accept_fixture_offer(&fixture, 100);
        fixture.clock.advance(TRADE_ACCEPTED_TTL_MS + 1);
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 200),
        )
        .unwrap();
        assert_ne!(pending.offer_id, first.offer_id);
        let partner_index = canonical_index(&pending, fixture.members[1]);
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &consent_decision_request(
                    &fixture,
                    1,
                    pending.offer_id,
                    pending.slots[partner_index].index() as u8,
                    pending.slots[1 - partner_index].index() as u8,
                    coop_cloud::TradeDecision::Accept,
                    201,
                ),
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            ledger_state(&fixture, |state| (
                state.ledger_entries.len(),
                state.trade_offers[&pending.offer_id].view.status
            )),
            (2, TradeOfferStatus::Pending)
        );
    }

    #[test]
    fn open_ledger_entry_is_fenced_owner_only_and_marks_delivery() {
        let fixture = consent_fixture();
        assert_eq!(
            super::super::ledger::open_for_character(
                &fixture.store,
                fixture.actors[0],
                fixture.members[0],
                fixture.fences[0],
            ),
            Err(Phase2Error::NotFound)
        );
        accept_fixture_offer(&fixture, 100);
        let stale = LeaseFence::new(
            id(99, SessionId::new),
            fixture.members[0],
            fixture.revision,
            SessionEpoch::new(2).unwrap(),
            id(20, ClientInstanceId::new),
        );
        assert_eq!(
            super::super::ledger::open_for_character(
                &fixture.store,
                fixture.actors[0],
                fixture.members[0],
                stale,
            ),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            super::super::ledger::open_for_character(
                &fixture.store,
                fixture.actors[1],
                fixture.members[0],
                fixture.fences[0],
            ),
            Err(Phase2Error::Authentication)
        );
        assert_eq!(
            open_trade_entry(&fixture, 0).status,
            super::super::ledger::LedgerStatus::Issued
        );
        let view = super::super::ledger::open_for_character(
            &fixture.store,
            fixture.actors[0],
            fixture.members[0],
            fixture.fences[0],
        )
        .unwrap();
        let entry = open_trade_entry(&fixture, 0);
        assert_eq!(view.commit_id, entry.commit_id);
        assert_eq!(view.status, super::super::ledger::LedgerStatus::Delivered);
        assert_eq!(entry.status, super::super::ledger::LedgerStatus::Delivered);
        let super::super::ledger::ExpectedDelta::Trade { incoming_raw, .. } = &entry.expected
        else {
            panic!("trade delta");
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["origin"]["kind"], "TRADE");
        assert_eq!(json["expected"]["kind"], "TRADE");
        assert_eq!(
            json["expected"]["incoming_raw"],
            super::super::battles::hex_string(incoming_raw)
        );
        let decoded: super::super::ledger::LedgerEntryView = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, view);
    }

    fn ledger_finalize_request(
        fixture: &ConsentFixture,
        member: usize,
        commit: Option<CommitId>,
        number: u128,
    ) -> coop_cloud::SnapshotFinalizeRequest {
        let snapshot = ledger_state(fixture, |state| {
            state.snapshots[&fixture.snapshots[member]].clone()
        });
        let fence = fixture.fences[member];
        coop_cloud::SnapshotFinalizeRequest::new(
            id(number, SnapshotId::new),
            coop_cloud::SnapshotFinalizeFence::new(
                fence.session_id,
                fixture.members[member],
                fixture.revision,
                fence.session_epoch,
                fence.client_instance_id,
                id(number, coop_cloud::IdempotencyKey::new),
            ),
            snapshot.files.clone(),
            snapshot.pending_commits_sha256,
            commit,
        )
        .unwrap()
    }

    fn apply_ledger(
        fixture: &ConsentFixture,
        member: usize,
        request: &coop_cloud::SnapshotFinalizeRequest,
        incoming: &[[u8; 100]],
    ) -> Result<bool, Phase2Error> {
        let source = validate_character_sav(&consent_member_sav(member), Revision::new(2)).unwrap();
        let incoming = validate_character_sav(
            &super::super::tests::party_character_sav(3, incoming),
            Revision::new(3),
        )
        .unwrap();
        fixture.store.write_transaction(|state| {
            super::super::ledger::apply_on_finalize(
                state,
                fixture.actors[member],
                request,
                &source,
                &incoming,
                1,
            )
        })
    }

    #[test]
    fn finalize_applies_only_the_exact_declared_trade() {
        let fixture = consent_fixture();
        let accepted = accept_fixture_offer(&fixture, 100);
        let member = 0;
        let entry = open_trade_entry(&fixture, member);
        let super::super::ledger::ExpectedDelta::Trade {
            slot, incoming_raw, ..
        } = entry.expected
        else {
            panic!("trade delta");
        };
        let own = consent_member_party(member);
        let mut applied_party = own;
        applied_party[slot.index()] = incoming_raw;
        let declared = ledger_finalize_request(&fixture, member, Some(entry.commit_id), 300);
        let undeclared = ledger_finalize_request(&fixture, member, None, 301);

        // A pre-apply save without the incoming Pokémon is ordinary progress.
        assert_eq!(apply_ledger(&fixture, member, &undeclared, &own), Ok(false));
        // The incoming Pokémon in its slot without a declaration is refused.
        assert_eq!(
            apply_ledger(&fixture, member, &undeclared, &applied_party),
            Err(Phase2Error::Forbidden)
        );
        // Wrong bytes in the slot.
        let mut wrong = applied_party;
        wrong[slot.index()] = super::super::tests::test_party_record(24 * 40, 9);
        assert_eq!(
            apply_ledger(&fixture, member, &declared, &wrong),
            Err(Phase2Error::Conflict)
        );
        // The outgoing Pokémon still stored elsewhere.
        let duplicated = [applied_party[0], applied_party[1], own[slot.index()]];
        assert_eq!(
            apply_ledger(&fixture, member, &declared, &duplicated),
            Err(Phase2Error::Conflict)
        );
        // The partner cannot declare this member's entry.
        assert_eq!(
            apply_ledger(
                &fixture,
                1,
                &ledger_finalize_request(&fixture, 1, Some(entry.commit_id), 302),
                &consent_member_party(1),
            ),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            open_trade_entry(&fixture, member).status,
            super::super::ledger::LedgerStatus::Issued
        );

        assert_eq!(
            apply_ledger(&fixture, member, &declared, &applied_party),
            Ok(true)
        );
        let (applied, open) = ledger_state(&fixture, |state| {
            (
                state.ledger_entries[&entry.commit_id].clone(),
                state
                    .ledger_open_by_character
                    .get(&fixture.members[member])
                    .copied(),
            )
        });
        assert_eq!(applied.status, super::super::ledger::LedgerStatus::Applied);
        assert_eq!(applied.applied_snapshot_id, Some(declared.snapshot_id));
        assert_eq!(open, None);

        // An applied entry is never applied again by another snapshot, and
        // it no longer blocks undeclared saves that contain its evidence.
        let redeclared = ledger_finalize_request(&fixture, member, Some(entry.commit_id), 303);
        assert_eq!(
            apply_ledger(&fixture, member, &redeclared, &applied_party),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            apply_ledger(&fixture, member, &undeclared, &applied_party),
            Ok(false)
        );
        // The partner's entry stays open and independent.
        assert_eq!(
            open_trade_entry(&fixture, 1).origin,
            super::super::ledger::LedgerOrigin::Trade {
                offer_id: accepted.offer_id
            }
        );
    }

    #[test]
    fn finalize_accepts_the_incoming_record_in_any_party_slot() {
        let fixture = consent_fixture();
        accept_fixture_offer(&fixture, 100);
        let member = 0;
        let entry = open_trade_entry(&fixture, member);
        let super::super::ledger::ExpectedDelta::Trade {
            slot, incoming_raw, ..
        } = entry.expected
        else {
            panic!("trade delta");
        };
        let own = consent_member_party(member);
        let kept = own[1 - slot.index()];
        // The player moved the outgoing Pokémon before the ROM applied the
        // commit, so the record landed in the other slot.
        let mut reordered = [kept; 2];
        reordered[1 - slot.index()] = incoming_raw;
        assert_ne!(reordered[slot.index()], incoming_raw);
        let declared = ledger_finalize_request(&fixture, member, Some(entry.commit_id), 400);
        let undeclared = ledger_finalize_request(&fixture, member, None, 401);

        // Evidence in another slot still blocks an undeclared save.
        assert_eq!(
            apply_ledger(&fixture, member, &undeclared, &reordered),
            Err(Phase2Error::Forbidden)
        );
        // The outgoing Pokémon gone but the incoming record nowhere.
        assert_eq!(
            apply_ledger(&fixture, member, &declared, &[kept]),
            Err(Phase2Error::Conflict)
        );
        // The incoming Pokémon present with different bytes (for example
        // after a trade evolution) is not the exact record.
        let mut altered = reordered;
        altered[1 - slot.index()][86] ^= 1;
        assert_eq!(
            apply_ledger(&fixture, member, &declared, &altered),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            open_trade_entry(&fixture, member).status,
            super::super::ledger::LedgerStatus::Issued
        );

        assert_eq!(
            apply_ledger(&fixture, member, &declared, &reordered),
            Ok(true)
        );
        assert_eq!(
            ledger_state(&fixture, |state| state.ledger_entries[&entry.commit_id]
                .status),
            super::super::ledger::LedgerStatus::Applied
        );
    }

    fn fixture_key(member: usize, slot: usize) -> coop_cloud::TradePokemonKey {
        let record = consent_member_party(member)[slot];
        coop_cloud::TradePokemonKey {
            personality: u32::from_le_bytes(record[0..4].try_into().unwrap()),
            ot_id: u32::from_le_bytes(record[4..8].try_into().unwrap()),
        }
    }

    /// The in-game UI's offer: slot `slot`, no partner anchors.
    fn open_offer_request(
        fixture: &ConsentFixture,
        caller: usize,
        slot: u8,
        key: u128,
    ) -> coop_cloud::TradeOfferRequest {
        coop_cloud::TradeOfferRequest {
            api_version: ApiVersion::V1,
            fence: fixture.fences[caller],
            group_id: fixture.group_id,
            own_slot: coop_cloud::PartyPosition::new(slot).unwrap(),
            partner_slot: None,
            partner_expected_revision: None,
            own_pokemon: Some(fixture_key(caller, usize::from(slot))),
            idempotency_key: id(key, coop_cloud::IdempotencyKey::new),
        }
    }

    /// The partner's pre-decision checkpoint: a new head at revision 3 with
    /// the same party, and the lease fence moved to it.
    fn advance_member_head(fixture: &ConsentFixture, member: usize) -> LeaseFence {
        let character_id = fixture.members[member];
        let next = Revision::new(3);
        let snapshot_id = id(60 + member as u128, SnapshotId::new);
        let sav = consent_member_sav(member);
        for (artifact, bytes) in [
            (ArtifactIdentity::CharacterSav, sav.clone()),
            (ArtifactIdentity::PendingCommits, b"[]".to_vec()),
        ] {
            fixture
                .store
                .objects
                .put(
                    Store::object_key(character_id, snapshot_id, artifact),
                    bytes,
                )
                .unwrap();
        }
        let old = fixture.fences[member];
        let fence = LeaseFence::new(
            old.session_id,
            character_id,
            next,
            old.session_epoch,
            old.client_instance_id,
        );
        fixture
            .store
            .write_transaction(|state| {
                state
                    .snapshot_by_revision
                    .insert((character_id, next), snapshot_id);
                state.snapshots.insert(
                    snapshot_id,
                    snapshot_for_sav(character_id, snapshot_id, next, &sav),
                );
                let character = state.characters.get_mut(&character_id).unwrap();
                character.revision = next;
                character.active_snapshot = Some(snapshot_id);
                let lease = state.leases.get_mut(&character_id).unwrap();
                lease.contract =
                    LeaseContract::new(fence, UnixTimestampMillis::new(1_600_000), 1_000).unwrap();
                Ok::<(), Phase2Error>(())
            })
            .unwrap();
        fence
    }

    #[test]
    fn open_offer_lets_the_partner_choose_its_slot_after_its_own_checkpoint() {
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &open_offer_request(&fixture, 0, 1, 100),
        )
        .unwrap();
        assert!(pending.partner_chooses);
        assert_eq!(pending.status, TradeOfferStatus::Pending);
        assert_eq!(pending.expires_at.value(), 1_000_000 + TRADE_OFFER_TTL_MS);

        // Both members see the pending offer and the offered Pokémon.
        for member in 0..2 {
            let current = current_offer(
                &fixture.store,
                fixture.actors[member],
                fixture.group_id,
                fixture.fences[member],
            )
            .unwrap();
            assert_eq!(current.offer, pending);
            assert_eq!(current.offered.species, 2);
            assert_eq!(current.offered.level, 0);
            assert!(!current.offered.is_egg);
            assert_eq!(current.offered.nickname, [0; 10]);
        }

        // The partner checkpoints, then accepts its own slot 0 on that head.
        let fence = advance_member_head(&fixture, 1);
        let initiator_index = canonical_index(&pending, fixture.members[0]);
        let mut accept = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            0,
            pending.slots[initiator_index].index() as u8,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        accept.fence = fence;
        accept.own_pokemon = Some(fixture_key(1, 1));
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &accept,
            ),
            Err(Phase2Error::Conflict),
            "the named Pokémon must be in the chosen slot"
        );
        accept.own_pokemon = Some(fixture_key(1, 0));
        accept.idempotency_key = id(102, coop_cloud::IdempotencyKey::new);
        let accepted = decide_offer(
            &fixture.store,
            fixture.actors[1],
            fixture.group_id,
            pending.offer_id,
            &accept,
        )
        .unwrap();
        assert_eq!(accepted.status, TradeOfferStatus::Accepted);
        let partner_index = 1 - initiator_index;
        assert_eq!(accepted.slots[partner_index].index(), 0);
        assert_eq!(accepted.slots[initiator_index].index(), 1);
        assert_eq!(accepted.revisions[partner_index], Revision::new(3));
        assert_eq!(accepted.snapshots[partner_index], id(61, SnapshotId::new));
        assert_eq!(accepted.snapshots[initiator_index], fixture.snapshots[0]);

        // Each side receives the other's exact chosen record.
        for (member, incoming) in [
            (0, consent_member_party(1)[0]),
            (1, consent_member_party(0)[1]),
        ] {
            let entry = open_trade_entry(&fixture, member);
            let super::super::ledger::ExpectedDelta::Trade {
                incoming_raw,
                outgoing,
                ..
            } = entry.expected
            else {
                panic!("trade delta");
            };
            assert_eq!(incoming_raw, incoming);
            let own = fixture_key(member, if member == 0 { 1 } else { 0 });
            assert_eq!(outgoing.personality, own.personality);
        }
        assert_eq!(
            open_trade_entry(&fixture, 1).base_snapshot_id,
            id(61, SnapshotId::new)
        );
        // Accepted offers are no longer current.
        assert_eq!(
            current_offer(&fixture.store, fixture.actors[1], fixture.group_id, fence,),
            Err(Phase2Error::NotFound)
        );
    }

    #[test]
    fn open_offer_refusals_and_initiator_cancel() {
        // Mixed anchors are malformed.
        let fixture = consent_fixture();
        let mut mixed = open_offer_request(&fixture, 0, 0, 100);
        mixed.partner_slot = coop_cloud::PartyPosition::new(1);
        assert_eq!(
            create_offer(&fixture.store, fixture.actors[0], fixture.group_id, &mixed),
            Err(Phase2Error::InvalidRequest)
        );
        // A head that no longer holds the picked Pokémon in that slot is stale.
        let mut stale = open_offer_request(&fixture, 0, 0, 101);
        stale.own_pokemon = Some(fixture_key(0, 1));
        assert_eq!(
            create_offer(&fixture.store, fixture.actors[0], fixture.group_id, &stale),
            Err(Phase2Error::Conflict)
        );
        // An empty slot beyond the saved party is refused.
        let mut empty = open_offer_request(&fixture, 0, 0, 102);
        empty.own_slot = coop_cloud::PartyPosition::new(4).unwrap();
        empty.own_pokemon = None;
        assert_eq!(
            create_offer(&fixture.store, fixture.actors[0], fixture.group_id, &empty),
            Err(Phase2Error::Conflict)
        );
        assert_eq!(
            current_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                fixture.fences[1],
            ),
            Err(Phase2Error::NotFound)
        );

        // The initiator cancels with a reject; the offer stops being current.
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &open_offer_request(&fixture, 0, 0, 103),
        )
        .unwrap();
        // One live offer per group.
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                &open_offer_request(&fixture, 1, 0, 104),
            ),
            Err(Phase2Error::Conflict)
        );
        let cancel = consent_decision_request(
            &fixture,
            0,
            pending.offer_id,
            0,
            0,
            coop_cloud::TradeDecision::Reject,
            105,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                &cancel,
            )
            .unwrap()
            .status,
            TradeOfferStatus::Rejected
        );
        assert_eq!(
            current_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                fixture.fences[1],
            ),
            Err(Phase2Error::NotFound)
        );

        // An open offer is refused at creation when the picked Pokémon holds
        // mail, before the partner is ever asked.
        let fixture = consent_fixture_with_saves(consent_member_sav_offering_mail);
        assert_eq!(
            create_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                &open_offer_request(&fixture, 0, 0, 100),
            ),
            Err(Phase2Error::TradePokemonHoldsMail)
        );
        // An open offer lapses after its window.
        let fixture = consent_fixture();
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &open_offer_request(&fixture, 0, 0, 100),
        )
        .unwrap();
        fixture.clock.advance(TRADE_OFFER_TTL_MS);
        assert_eq!(
            current_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                fixture.fences[1],
            ),
            Err(Phase2Error::NotFound)
        );
        assert_eq!(
            get_offer(
                &fixture.store,
                fixture.actors[0],
                fixture.group_id,
                pending.offer_id,
                fixture.fences[0],
            )
            .unwrap()
            .status,
            TradeOfferStatus::Expired
        );
    }

    /// Member 0 offers slot 0; that record carries a mail index.
    fn consent_member_sav_offering_mail(index: usize) -> Vec<u8> {
        let mut party = consent_member_party(index);
        if index == 0 {
            party[0][85] = 0;
        }
        super::super::tests::party_character_sav(2, &party)
    }

    /// Member 0 holds mail only on slot 1, which is not offered.
    fn consent_member_sav_unoffered_mail(index: usize) -> Vec<u8> {
        let mut party = consent_member_party(index);
        if index == 0 {
            party[1][85] = 0;
        }
        super::super::tests::party_character_sav(2, &party)
    }

    #[test]
    fn accept_refuses_to_issue_a_trade_for_a_pokemon_holding_mail() {
        let fixture = consent_fixture_with_saves(consent_member_sav_offering_mail);
        let pending = create_offer(
            &fixture.store,
            fixture.actors[0],
            fixture.group_id,
            &consent_offer_request(&fixture, 0, 100),
        )
        .unwrap();
        let partner_index = canonical_index(&pending, fixture.members[1]);
        let request = consent_decision_request(
            &fixture,
            1,
            pending.offer_id,
            pending.slots[partner_index].index() as u8,
            pending.slots[1 - partner_index].index() as u8,
            coop_cloud::TradeDecision::Accept,
            101,
        );
        assert_eq!(
            decide_offer(
                &fixture.store,
                fixture.actors[1],
                fixture.group_id,
                pending.offer_id,
                &request,
            ),
            Err(Phase2Error::TradePokemonHoldsMail)
        );
        assert_eq!(
            ledger_state(&fixture, |state| (
                state.ledger_entries.len(),
                state.trade_offers[&pending.offer_id].view.status
            )),
            (0, TradeOfferStatus::Pending)
        );
        // The refusal has a stable public code.
        assert_eq!(
            Phase2Error::TradePokemonHoldsMail.to_string(),
            "an offered Pokémon holds mail"
        );
        let response =
            axum::response::IntoResponse::into_response(Phase2Error::TradePokemonHoldsMail);
        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);

        // Mail on a Pokémon that is not offered does not block the trade.
        let fixture = consent_fixture_with_saves(consent_member_sav_unoffered_mail);
        assert_eq!(
            accept_fixture_offer(&fixture, 100).status,
            TradeOfferStatus::Accepted
        );
        assert_eq!(
            ledger_state(&fixture, |state| state.ledger_entries.len()),
            2
        );
    }
}

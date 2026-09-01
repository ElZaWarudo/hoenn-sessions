//! Fixed-artifact snapshot lifecycle and signed resume packages.

use coop_cloud::{
    ArtifactIdentity, BridgeAbiVersion, CreatedAt, GameBuildId, ManifestBuildInfo, MgbaVersion,
    ProtocolVersion, ResumePackageManifest, Revision, Sha256Digest, SignedManifestEnvelope,
    SnapshotFence, SnapshotFile, SnapshotFinalizeRequest, SnapshotId, SnapshotListRequest,
    SnapshotListResponse, SnapshotPrepareRequest, SnapshotPrepareResponse, SnapshotRecord,
    SnapshotRestoreRequest, SnapshotRestoreResponse, UploadMethod, UploadTarget,
};
use serde::Deserialize;
use std::collections::HashSet;
use subtle::ConstantTimeEq;

use super::storage::{
    MAX_CHARACTER_SAV, MAX_PENDING_COMMITS, MAX_RESUME_RESPONSE, MAX_RESUME_SS1,
    MAX_SNAPSHOT_STORAGE_BYTES, MAX_SNAPSHOTS_PER_CHARACTER, PreparedSnapshot,
    RESTORE_STAGE_TTL_MS, Store, TicketRecord, commit_id_allowed, is_artifact_size_allowed,
};
use super::{AuthenticatedActor, Phase2Error};

#[derive(Deserialize)]
struct BridgeManifest {
    game_build: BuildInfo,
    net_bridge: NetBridge,
}
#[derive(Deserialize)]
struct BuildInfo {
    id: String,
    rom_sha256: String,
}
#[derive(Deserialize)]
struct NetBridge {
    abi_version: u16,
    game_protocol_version: u16,
}

fn bridge_manifest() -> Result<BridgeManifest, Phase2Error> {
    serde_json::from_str(include_str!("../../../../../dist/bridge_manifest.json"))
        .map_err(|_| Phase2Error::Internal)
}

fn owner(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    character: coop_cloud::CharacterId,
) -> bool {
    state
        .characters
        .get(&character)
        .is_some_and(|record| record.owner == actor.user_id && actor.character_id == character)
}

fn active_lease(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    fence: coop_cloud::LeaseFence,
    now: u64,
) -> Result<&super::storage::LeaseRecord, Phase2Error> {
    if !owner(state, actor, fence.character_id) {
        return Err(Phase2Error::NotFound);
    }
    let lease = state
        .leases
        .get(&fence.character_id)
        .ok_or(Phase2Error::Expired)?;
    if lease.released || lease.contract.expires_at.value() <= now || lease.contract.fence() != fence
    {
        return Err(Phase2Error::Conflict);
    }
    Ok(lease)
}

fn active_lease_identity(
    state: &super::storage::State,
    actor: AuthenticatedActor,
    character_id: coop_cloud::CharacterId,
    session_id: coop_cloud::SessionId,
    session_epoch: coop_cloud::SessionEpoch,
    client_instance_id: coop_cloud::ClientInstanceId,
    now: u64,
) -> Result<super::storage::LeaseRecord, Phase2Error> {
    if !owner(state, actor, character_id) {
        return Err(Phase2Error::NotFound);
    }
    let lease = state
        .leases
        .get(&character_id)
        .ok_or(Phase2Error::Expired)?;
    if lease.released
        || lease.contract.expires_at.value() <= now
        || lease.contract.session_id != session_id
        || lease.contract.session_epoch != session_epoch
        || lease.contract.client_instance_id != client_instance_id
    {
        return Err(Phase2Error::Conflict);
    }
    Ok(lease.clone())
}

fn target_for(
    store: &Store,
    state: &mut super::storage::State,
    actor: AuthenticatedActor,
    request: &SnapshotPrepareRequest,
    file: &SnapshotFile,
    expiry: u64,
) -> Result<UploadTarget, Phase2Error> {
    let token = zeroize::Zeroizing::new(store.random_token()?);
    let fingerprint = Store::token_fingerprint(token.as_str());
    let url = format!(
        "{}/v1/uploads/{}?ticket={}",
        store.config.upload_base_url.trim_end_matches('/'),
        token.as_str(),
        token.as_str()
    );
    let target = UploadTarget::new_put(file.artifact, url, Store::unix_timestamp(expiry)?)
        .map_err(|_| Phase2Error::Internal)?;
    state.tickets.insert(
        fingerprint,
        TicketRecord {
            actor: actor.user_id,
            character_id: request.character_id,
            snapshot_id: request.snapshot_id,
            artifact: file.artifact,
            method: UploadMethod::Put,
            expected: file.clone(),
            expires_at: expiry,
            used: false,
        },
    );
    Ok(target)
}

fn remove_prepared(state: &mut super::storage::State, snapshot_id: SnapshotId) {
    state.prepared.remove(&snapshot_id);
    state
        .prepare_ops
        .retain(|_, prepared_id| *prepared_id != snapshot_id);
    state
        .tickets
        .retain(|_, ticket| ticket.snapshot_id != snapshot_id);
}

fn retire_prepared(state: &mut super::storage::State, snapshot_id: SnapshotId) {
    remove_prepared(state, snapshot_id);
    state.retired_snapshots.insert(snapshot_id);
}

fn validate_prepare_files(files: &[SnapshotFile]) -> Result<(), Phase2Error> {
    for file in files {
        if !is_artifact_size_allowed(file.artifact, file.size_bytes) {
            return Err(Phase2Error::InvalidRequest);
        }
        if (file.artifact == ArtifactIdentity::PendingCommits
            && file.size_bytes > MAX_PENDING_COMMITS)
            || (file.artifact == ArtifactIdentity::CharacterSav
                && file.size_bytes > MAX_CHARACTER_SAV)
            || (file.artifact == ArtifactIdentity::ResumeSs1 && file.size_bytes > MAX_RESUME_SS1)
        {
            return Err(Phase2Error::InvalidRequest);
        }
    }
    Ok(())
}

fn response(prepared: &PreparedSnapshot) -> Result<SnapshotPrepareResponse, Phase2Error> {
    let next = prepared
        .request
        .expected_parent_revision
        .next()
        .map_err(|_| Phase2Error::Conflict)?;
    let output = SnapshotPrepareResponse {
        api_version: coop_cloud::ApiVersion::V1,
        snapshot_id: prepared.request.snapshot_id,
        expected_parent_revision: prepared.request.expected_parent_revision,
        next_revision: next,
        session_epoch: prepared.request.session_epoch,
        idempotency_key: prepared.request.idempotency_key,
        files: prepared.request.files.clone(),
        pending_commits_sha256: prepared.request.pending_commits_sha256,
        upload_targets: prepared.upload_targets.clone(),
    };
    output.validate().map_err(|_| Phase2Error::Internal)?;
    Ok(output)
}

fn prepared_with_targets(
    store: &Store,
    state: &mut super::storage::State,
    actor: AuthenticatedActor,
    request: SnapshotPrepareRequest,
    expiry: u64,
) -> Result<PreparedSnapshot, Phase2Error> {
    validate_prepare_files(&request.files)?;
    let mut upload_targets = Vec::with_capacity(request.files.len());
    for file in &request.files {
        match target_for(store, state, actor, &request, file, expiry) {
            Ok(target) => upload_targets.push(target),
            Err(error) => {
                state
                    .tickets
                    .retain(|_, ticket| ticket.snapshot_id != request.snapshot_id);
                return Err(error);
            }
        }
    }
    Ok(PreparedSnapshot {
        request,
        upload_targets,
        expires_at: expiry,
    })
}

pub(crate) fn prepare(
    store: &Store,
    actor: AuthenticatedActor,
    request: SnapshotPrepareRequest,
) -> Result<SnapshotPrepareResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let now = store.now();
    let expiry = now
        .checked_add(super::storage::UPLOAD_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        // Authenticate and fence the character before touching any caller-named
        // or globally expired records. Cleanup is observable state mutation.
        let fence = coop_cloud::LeaseFence::new(
            request.session_id,
            request.character_id,
            request.expected_parent_revision,
            request.session_epoch,
            request.client_instance_id,
        );
        if !owner(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != request.expected_parent_revision {
            return Err(Phase2Error::Conflict);
        }
        if state.retired_snapshots.contains(&request.snapshot_id) {
            return Err(Phase2Error::Conflict);
        }
        if state
            .prepared
            .get(&request.snapshot_id)
            .is_some_and(|prepared| prepared.expires_at <= now)
        {
            retire_prepared(state, request.snapshot_id);
            return Err(Phase2Error::Expired);
        }
        active_lease(state, actor, fence, now)?;
        let operation = (request.character_id, request.idempotency_key);
        if let Some(existing_id) = state.prepare_ops.get(&operation).copied() {
            if let Some(existing) = state.prepared.get(&existing_id) {
                if existing.expires_at <= now {
                    retire_prepared(state, existing_id);
                } else if existing.request == request {
                    return response(existing);
                } else {
                    return Err(Phase2Error::Conflict);
                }
            }
            state.prepare_ops.remove(&operation);
        }
        let expired_snapshots: Vec<_> = state
            .prepared
            .iter()
            .filter_map(|(snapshot_id, prepared)| {
                (prepared.expires_at <= now).then_some(*snapshot_id)
            })
            .collect();
        for snapshot_id in expired_snapshots {
            retire_prepared(state, snapshot_id);
        }
        let prepared_ids: HashSet<_> = state.prepared.keys().copied().collect();
        state
            .tickets
            .retain(|_, ticket| prepared_ids.contains(&ticket.snapshot_id));
        if let Some(existing) = state.prepared.get(&request.snapshot_id) {
            if existing.request == request {
                return response(existing);
            }
            return Err(Phase2Error::Conflict);
        }
        if state.snapshots.contains_key(&request.snapshot_id) {
            return Err(Phase2Error::Conflict);
        }
        let prepared_for_character = state
            .prepared
            .values()
            .filter(|prepared| prepared.request.character_id == request.character_id)
            .count();
        let tickets_for_character = state
            .tickets
            .values()
            .filter(|ticket| ticket.character_id == request.character_id)
            .count();
        if prepared_for_character >= super::storage::MAX_PREPARED_SNAPSHOTS
            || tickets_for_character.saturating_add(request.files.len())
                > super::storage::MAX_UPLOAD_TICKETS
        {
            return Err(Phase2Error::Busy);
        }
        let prepared = prepared_with_targets(store, state, actor, request, expiry)?;
        state
            .prepared
            .insert(prepared.request.snapshot_id, prepared.clone());
        state
            .prepare_ops
            .insert(operation, prepared.request.snapshot_id);
        response(&prepared)
    })
}

fn capability_tokens(path_token: &str, query_token: &str) -> Result<[u8; 32], Phase2Error> {
    if path_token.is_empty()
        || path_token.len() > 512
        || query_token.is_empty()
        || query_token.len() > 512
        || path_token
            .as_bytes()
            .ct_eq(query_token.as_bytes())
            .unwrap_u8()
            != 1
    {
        return Err(Phase2Error::Authentication);
    }
    Ok(Store::token_fingerprint(path_token))
}

fn ticket_limit(ticket: &TicketRecord) -> u64 {
    match ticket.artifact {
        ArtifactIdentity::CharacterSav => MAX_CHARACTER_SAV,
        ArtifactIdentity::PendingCommits => MAX_PENDING_COMMITS,
        ArtifactIdentity::ResumeSs1 => MAX_RESUME_SS1,
    }
}

pub(crate) fn upload_limit(
    store: &Store,
    path_token: &str,
    query_token: &str,
) -> Result<usize, Phase2Error> {
    let fingerprint = capability_tokens(path_token, query_token)?;
    let now = store.now();
    store.read_transaction(|state| {
        let ticket = state
            .tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?;
        if ticket.used {
            return Err(Phase2Error::Conflict);
        }
        if ticket.expires_at <= now {
            return Err(Phase2Error::Expired);
        }
        if state
            .prepared
            .get(&ticket.snapshot_id)
            .is_none_or(|prepared| prepared.expires_at <= now)
        {
            return Err(Phase2Error::Expired);
        }
        if state
            .characters
            .get(&ticket.character_id)
            .is_none_or(|character| character.owner != ticket.actor)
        {
            return Err(Phase2Error::Authentication);
        }
        if ticket.method != UploadMethod::Put {
            return Err(Phase2Error::Authentication);
        }
        usize::try_from(ticket_limit(ticket)).map_err(|_| Phase2Error::Internal)
    })
}

pub(crate) fn upload(store: &Store, token: &str, body: Vec<u8>) -> Result<(), Phase2Error> {
    upload_with_credential(store, token, token, body)
}

pub(crate) fn upload_with_credential(
    store: &Store,
    path_token: &str,
    query_token: &str,
    body: Vec<u8>,
) -> Result<(), Phase2Error> {
    let fingerprint = capability_tokens(path_token, query_token)?;
    let now = store.now();
    let ticket = store.read_transaction(|state| {
        let ticket = state
            .tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?
            .clone();
        if ticket.used {
            return Err(Phase2Error::Conflict);
        }
        if ticket.expires_at <= now {
            return Err(Phase2Error::Expired);
        }
        if state
            .prepared
            .get(&ticket.snapshot_id)
            .is_none_or(|prepared| prepared.expires_at <= now)
        {
            return Err(Phase2Error::Expired);
        }
        if ticket.method != UploadMethod::Put
            || state
                .characters
                .get(&ticket.character_id)
                .is_none_or(|character| character.owner != ticket.actor)
        {
            return Err(Phase2Error::Authentication);
        }
        Ok(ticket)
    })?;
    if u64::try_from(body.len()).map_or(true, |size| size > ticket_limit(&ticket)) {
        return Err(Phase2Error::PayloadTooLarge);
    }
    ticket
        .expected
        .verify_bytes(&body)
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let key = Store::object_key(ticket.character_id, ticket.snapshot_id, ticket.artifact);
    // Object-store work must not run while holding the repository lock. The
    // conditional create also closes the concurrent upload race.
    if !store.objects.put_if_absent(key.clone(), body)? {
        let existing = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
        ticket
            .expected
            .verify_bytes(&existing)
            .map_err(|_| Phase2Error::Conflict)?;
    }
    let commit_now = store.now();
    store.write_transaction(|state| {
        let current = state
            .tickets
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?;
        if current.used
            || current.expires_at <= commit_now
            || current.snapshot_id != ticket.snapshot_id
            || current.character_id != ticket.character_id
            || current.expected != ticket.expected
            || state
                .prepared
                .get(&ticket.snapshot_id)
                .is_none_or(|prepared| prepared.expires_at <= commit_now)
        {
            return Err(Phase2Error::Conflict);
        }
        if let Some(ticket) = state.tickets.get_mut(&fingerprint) {
            ticket.used = true;
        }
        Ok(())
    })
}

fn finalized_record(
    request: &SnapshotFinalizeRequest,
    now: u64,
) -> Result<SnapshotRecord, Phase2Error> {
    SnapshotRecord::new(
        request.snapshot_id,
        SnapshotFence::new(
            request.session_id,
            request.character_id,
            request.session_epoch,
        ),
        request.expected_parent_revision,
        request.revision,
        request.files.clone(),
        request.pending_commits_sha256,
        request.last_applied_commit,
        Store::unix_timestamp(now)?,
    )
    .map_err(|_| Phase2Error::Internal)
}

fn restore_response(record: SnapshotRecord) -> SnapshotRestoreResponse {
    SnapshotRestoreResponse {
        api_version: coop_cloud::ApiVersion::V1,
        pending_commits_sha256: record.pending_commits_sha256,
        last_applied_commit: record.last_applied_commit,
        snapshot: record,
    }
}

enum RestorePreflight {
    Replay(SnapshotRestoreResponse),
    Source(super::storage::CharacterRecord, SnapshotRecord),
}

enum RestoreReservation {
    Replay(SnapshotRestoreResponse),
    Reserved,
    Recovered(super::storage::RestoreStage),
}

fn restore_preflight(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotRestoreRequest,
    now: u64,
) -> Result<RestorePreflight, Phase2Error> {
    store.read_transaction(|state| {
        let _lease = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        if let Some((known, record)) = state
            .restore_ops
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok(RestorePreflight::Replay(restore_response(record.clone())))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?
            .clone();
        if character.revision != request.expected_revision {
            return Err(Phase2Error::Conflict);
        }
        let source = state
            .snapshots
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?
            .clone();
        if source.character_id != request.character_id {
            return Err(Phase2Error::NotFound);
        }
        Ok(RestorePreflight::Source(character, source))
    })
}

fn verified_source_objects(
    store: &Store,
    character_id: coop_cloud::CharacterId,
    source: &SnapshotRecord,
) -> Result<Vec<(ArtifactIdentity, Vec<u8>)>, Phase2Error> {
    source.validate().map_err(|_| Phase2Error::Conflict)?;
    source
        .files
        .iter()
        .map(|file| {
            let key = Store::object_key(character_id, source.snapshot_id, file.artifact);
            let bytes = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
            file.verify_bytes(&bytes)
                .map_err(|_| Phase2Error::Conflict)?;
            Ok((file.artifact, bytes))
        })
        .collect()
}

fn verify_uploaded_objects(
    store: &Store,
    character_id: coop_cloud::CharacterId,
    snapshot_id: coop_cloud::SnapshotId,
    files: &[SnapshotFile],
) -> Result<(), Phase2Error> {
    for file in files {
        let key = Store::object_key(character_id, snapshot_id, file.artifact);
        let bytes = store.objects.get(&key)?.ok_or(Phase2Error::Conflict)?;
        file.verify_bytes(&bytes)
            .map_err(|_| Phase2Error::Conflict)?;
    }
    Ok(())
}

fn copy_restore_objects(
    store: &Store,
    character_id: coop_cloud::CharacterId,
    snapshot_id: SnapshotId,
    objects: Vec<(ArtifactIdentity, Vec<u8>)>,
) -> Result<(), Phase2Error> {
    for (artifact, bytes) in objects {
        if !store.objects.put_if_absent(
            Store::object_key(character_id, snapshot_id, artifact),
            bytes,
        )? {
            return Err(Phase2Error::Conflict);
        }
    }
    Ok(())
}

fn restore_object_keys(
    character_id: coop_cloud::CharacterId,
    snapshot_id: SnapshotId,
    files: &[SnapshotFile],
) -> Vec<String> {
    files
        .iter()
        .map(|file| Store::object_key(character_id, snapshot_id, file.artifact))
        .collect()
}

fn cleanup_restore_objects(store: &Store, keys: &[String]) -> Result<(), Phase2Error> {
    for key in keys {
        store.objects.delete_if_present(key)?;
    }
    Ok(())
}

fn clear_restore_stage(
    store: &Store,
    request: &SnapshotRestoreRequest,
    snapshot_id: SnapshotId,
) -> Result<(), Phase2Error> {
    store.write_transaction(|state| {
        if state
            .restore_staging
            .get(&request.character_id)
            .is_some_and(|stage| stage.snapshot_id == snapshot_id && stage.request == *request)
        {
            state.restore_staging.remove(&request.character_id);
        }
        Ok(())
    })
}

fn staged_restore_object_keys(
    character_id: coop_cloud::CharacterId,
    snapshot_id: SnapshotId,
) -> Vec<String> {
    [
        ArtifactIdentity::CharacterSav,
        ArtifactIdentity::PendingCommits,
        ArtifactIdentity::ResumeSs1,
    ]
    .into_iter()
    .map(|artifact| Store::object_key(character_id, snapshot_id, artifact))
    .collect()
}

fn snapshot_storage_usage(
    state: &super::storage::State,
    character_id: coop_cloud::CharacterId,
) -> Result<u64, Phase2Error> {
    state
        .snapshots
        .values()
        .filter(|snapshot| snapshot.character_id == character_id)
        .try_fold(0_u64, |total, snapshot| {
            total
                .checked_add(files_storage_usage(&snapshot.files)?)
                .ok_or(Phase2Error::Internal)
        })
}

fn files_storage_usage(files: &[SnapshotFile]) -> Result<u64, Phase2Error> {
    files.iter().try_fold(0_u64, |sum, file| {
        sum.checked_add(file.size_bytes)
            .ok_or(Phase2Error::Internal)
    })
}

fn ensure_snapshot_quota(
    state: &super::storage::State,
    character_id: coop_cloud::CharacterId,
    files: &[SnapshotFile],
) -> Result<(), Phase2Error> {
    // Prepared snapshots are transient declarations and do not consume the
    // durable history or storage quota until finalize/restore commits them.
    let snapshot_count = state
        .snapshots
        .values()
        .filter(|snapshot| snapshot.character_id == character_id)
        .count();
    if snapshot_count >= MAX_SNAPSHOTS_PER_CHARACTER {
        return Err(Phase2Error::Busy);
    }
    let used = snapshot_storage_usage(state, character_id)?;
    let requested = files_storage_usage(files)?;
    if used.checked_add(requested).ok_or(Phase2Error::Internal)? > MAX_SNAPSHOT_STORAGE_BYTES {
        return Err(Phase2Error::Busy);
    }
    Ok(())
}

enum FinalizePreflight {
    Replay(SnapshotRecord),
    Prepared(PreparedSnapshot),
}

fn finalize_preflight(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotFinalizeRequest,
    now: u64,
) -> Result<FinalizePreflight, Phase2Error> {
    store.read_transaction(|state| {
        let lease = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        if let Some((known, record)) = state
            .finalize_ops
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok(FinalizePreflight::Replay(record.clone()))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if lease.contract.current_revision != request.expected_parent_revision {
            return Err(Phase2Error::Conflict);
        }
        let prepared = state
            .prepared
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?
            .clone();
        if prepared.expires_at <= now
            || prepared.request.character_id != request.character_id
            || prepared.request.session_id != request.session_id
            || prepared.request.session_epoch != request.session_epoch
            || prepared.request.client_instance_id != request.client_instance_id
            || prepared.request.expected_parent_revision != request.expected_parent_revision
            || !request.matches_declaration(
                &prepared.request.files,
                prepared.request.pending_commits_sha256,
            )
        {
            return Err(if prepared.expires_at <= now {
                Phase2Error::Expired
            } else {
                Phase2Error::Conflict
            });
        }
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != request.expected_parent_revision {
            return Err(Phase2Error::Conflict);
        }
        Ok(FinalizePreflight::Prepared(prepared))
    })
}

pub(crate) fn finalize(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotFinalizeRequest,
) -> Result<SnapshotRecord, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    if !commit_id_allowed(request.last_applied_commit) {
        return Err(Phase2Error::Forbidden);
    }
    let now = store.now();
    let prepared = match finalize_preflight(store, actor, request, now)? {
        FinalizePreflight::Replay(record) => return Ok(record),
        FinalizePreflight::Prepared(prepared) => prepared,
    };
    verify_uploaded_objects(
        store,
        request.character_id,
        request.snapshot_id,
        &request.files,
    )?;
    let snapshot = finalized_record(request, now)?;

    store.write_transaction(|state| {
        let lease = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            store.now(),
        )?;
        if let Some((known, record)) = state
            .finalize_ops
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok(record.clone())
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if lease.contract.current_revision != request.expected_parent_revision {
            return Err(Phase2Error::Conflict);
        }
        let current_prepared = state
            .prepared
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?;
        if current_prepared.expires_at <= store.now()
            || current_prepared.request != prepared.request
        {
            return Err(Phase2Error::Conflict);
        }
        let character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        if character.revision != request.expected_parent_revision
            || state.snapshots.contains_key(&request.snapshot_id)
            || state
                .snapshot_by_revision
                .contains_key(&(request.character_id, request.revision))
        {
            return Err(Phase2Error::Conflict);
        }
        ensure_snapshot_quota(state, request.character_id, &request.files)?;
        let new_contract = coop_cloud::LeaseContract::new(
            coop_cloud::LeaseFence::new(
                lease.contract.session_id,
                request.character_id,
                request.revision,
                lease.contract.session_epoch,
                lease.contract.client_instance_id,
            ),
            lease.contract.expires_at,
            lease.contract.heartbeat_interval_ms,
        )
        .map_err(|_| Phase2Error::Internal)?;
        state
            .snapshots
            .insert(request.snapshot_id, snapshot.clone());
        state.snapshot_by_revision.insert(
            (request.character_id, request.revision),
            request.snapshot_id,
        );
        remove_prepared(state, request.snapshot_id);
        if let Some(character) = state.characters.get_mut(&request.character_id) {
            character.revision = request.revision;
            character.active_snapshot = Some(request.snapshot_id);
        }
        if let Some(lease) = state.leases.get_mut(&request.character_id) {
            lease.contract = new_contract;
        }
        state.finalize_ops.insert(
            (request.character_id, request.idempotency_key),
            (request.clone(), snapshot.clone()),
        );
        Ok(snapshot)
    })
}

pub(crate) fn list(
    store: &Store,
    actor: AuthenticatedActor,
    request: SnapshotListRequest,
) -> Result<SnapshotListResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::InvalidRequest)?;
    let now = store.now();
    store.read_transaction(|state| {
        if !owner(state, actor, request.character_id) {
            return Err(Phase2Error::NotFound);
        }
        let lease = state
            .leases
            .get(&request.character_id)
            .ok_or(Phase2Error::Expired)?;
        if lease.released
            || lease.contract.expires_at.value() <= now
            || lease.contract.session_id != request.session_id
            || lease.contract.session_epoch != request.session_epoch
            || lease.contract.client_instance_id != request.client_instance_id
        {
            return Err(Phase2Error::Conflict);
        }
        let mut snapshots: Vec<_> = state
            .snapshots
            .values()
            .filter(|snapshot| snapshot.character_id == request.character_id)
            .cloned()
            .collect();
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.revision));
        snapshots.truncate(request.limit as usize);
        let result = SnapshotListResponse {
            api_version: coop_cloud::ApiVersion::V1,
            snapshots,
        };
        result.validate().map_err(|_| Phase2Error::Internal)?;
        Ok(result)
    })
}

fn reserve_restore(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotRestoreRequest,
    source: &SnapshotRecord,
    snapshot_id: SnapshotId,
    revision: Revision,
    now: u64,
) -> Result<RestoreReservation, Phase2Error> {
    let stage_expires_at = now
        .checked_add(RESTORE_STAGE_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    store.write_transaction(|state| {
        let _ = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        if let Some((known, record)) = state
            .restore_ops
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok(RestoreReservation::Replay(restore_response(record.clone())))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        if let Some(stage) = state.restore_staging.get(&request.character_id).cloned() {
            if stage.expires_at <= now {
                return Ok(RestoreReservation::Recovered(stage));
            }
            return Err(Phase2Error::Busy);
        }
        let current_character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        let current_source = state
            .snapshots
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?;
        if current_character.revision != request.expected_revision
            || current_source != source
            || state.snapshots.contains_key(&snapshot_id)
            || state
                .snapshot_by_revision
                .contains_key(&(request.character_id, revision))
        {
            return Err(Phase2Error::Conflict);
        }
        ensure_snapshot_quota(state, request.character_id, &source.files)?;
        state.restore_staging.insert(
            request.character_id,
            super::storage::RestoreStage {
                request: request.clone(),
                snapshot_id,
                expires_at: stage_expires_at,
            },
        );
        Ok(RestoreReservation::Reserved)
    })
}

fn compensate_restore_failure(
    store: &Store,
    request: &SnapshotRestoreRequest,
    snapshot_id: SnapshotId,
    keys: &[String],
    original: Phase2Error,
) -> Phase2Error {
    let cleanup = cleanup_restore_objects(store, keys);
    let clear = clear_restore_stage(store, request, snapshot_id);
    if cleanup.is_err() || clear.is_err() {
        Phase2Error::Internal
    } else {
        original
    }
}

fn commit_restore(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotRestoreRequest,
    source: &SnapshotRecord,
    snapshot: SnapshotRecord,
    snapshot_id: SnapshotId,
    revision: Revision,
) -> Result<SnapshotRestoreResponse, Phase2Error> {
    store.write_transaction(|state| {
        let lease = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            store.now(),
        )?;
        if let Some((known, record)) = state
            .restore_ops
            .get(&(request.character_id, request.idempotency_key))
        {
            return if known == request {
                Ok(restore_response(record.clone()))
            } else {
                Err(Phase2Error::Conflict)
            };
        }
        let stage_matches = state
            .restore_staging
            .get(&request.character_id)
            .is_some_and(|stage| stage.snapshot_id == snapshot_id && stage.request == *request);
        if !stage_matches {
            return Err(Phase2Error::Conflict);
        }
        if state
            .restore_staging
            .get(&request.character_id)
            .is_some_and(|stage| stage.expires_at <= store.now())
        {
            return Err(Phase2Error::Expired);
        }
        let current_character = state
            .characters
            .get(&request.character_id)
            .ok_or(Phase2Error::NotFound)?;
        let current_source = state
            .snapshots
            .get(&request.snapshot_id)
            .ok_or(Phase2Error::NotFound)?;
        if current_character.revision != request.expected_revision || current_source != source {
            return Err(Phase2Error::Conflict);
        }
        let contract = coop_cloud::LeaseContract::new(
            coop_cloud::LeaseFence::new(
                lease.contract.session_id,
                request.character_id,
                revision,
                lease.contract.session_epoch,
                lease.contract.client_instance_id,
            ),
            lease.contract.expires_at,
            lease.contract.heartbeat_interval_ms,
        )
        .map_err(|_| Phase2Error::Internal)?;
        state.snapshots.insert(snapshot_id, snapshot.clone());
        state
            .snapshot_by_revision
            .insert((request.character_id, revision), snapshot_id);
        if let Some(character) = state.characters.get_mut(&request.character_id) {
            character.revision = revision;
            character.active_snapshot = Some(snapshot_id);
        }
        if let Some(lease) = state.leases.get_mut(&request.character_id) {
            lease.contract = contract;
        }
        state.restore_staging.remove(&request.character_id);
        state.restore_ops.insert(
            (request.character_id, request.idempotency_key),
            (request.clone(), snapshot.clone()),
        );
        Ok(restore_response(snapshot))
    })
}

pub(crate) fn restore(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotRestoreRequest,
) -> Result<SnapshotRestoreResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let now = store.now();
    let (character, source) = match restore_preflight(store, actor, request, now)? {
        RestorePreflight::Replay(response) => return Ok(response),
        RestorePreflight::Source(character, source) => (character, source),
    };
    let revision = character
        .revision
        .next()
        .map_err(|_| Phase2Error::Conflict)?;
    let snapshot_id = store.snapshot_id()?;
    let snapshot = SnapshotRecord::new(
        snapshot_id,
        SnapshotFence::new(
            request.session_id,
            request.character_id,
            request.session_epoch,
        ),
        character.revision,
        revision,
        source.files.clone(),
        source.pending_commits_sha256,
        source.last_applied_commit,
        Store::unix_timestamp(now)?,
    )
    .map_err(|_| Phase2Error::Internal)?;
    let object_keys = restore_object_keys(request.character_id, snapshot_id, &source.files);
    let reservation = loop {
        match reserve_restore(store, actor, request, &source, snapshot_id, revision, now)? {
            RestoreReservation::Replay(response) => return Ok(response),
            RestoreReservation::Reserved => break RestoreReservation::Reserved,
            RestoreReservation::Recovered(stage) => {
                let stale_keys =
                    staged_restore_object_keys(request.character_id, stage.snapshot_id);
                cleanup_restore_objects(store, &stale_keys)?;
                // Keep the expired stage durable until all compensation has
                // succeeded. A failed clear is therefore retryable too.
                clear_restore_stage(store, &stage.request, stage.snapshot_id)?;
            }
        }
    };
    debug_assert!(matches!(reservation, RestoreReservation::Reserved));

    // Reserve before object-store work so concurrent losers cannot create
    // unreachable copies under random snapshot IDs.
    let source_objects = match verified_source_objects(store, request.character_id, &source) {
        Ok(objects) => objects,
        Err(error) => {
            return Err(compensate_restore_failure(
                store,
                request,
                snapshot_id,
                &object_keys,
                error,
            ));
        }
    };
    if let Err(error) =
        copy_restore_objects(store, request.character_id, snapshot_id, source_objects)
    {
        return Err(compensate_restore_failure(
            store,
            request,
            snapshot_id,
            &object_keys,
            error,
        ));
    }
    match commit_restore(
        store,
        actor,
        request,
        &source,
        snapshot,
        snapshot_id,
        revision,
    ) {
        Ok(response) => Ok(response),
        Err(error) => Err(compensate_restore_failure(
            store,
            request,
            snapshot_id,
            &object_keys,
            error,
        )),
    }
}

pub(crate) fn restore_at(
    store: &Store,
    actor: AuthenticatedActor,
    request: &SnapshotRestoreRequest,
    source_revision: u64,
) -> Result<SnapshotRestoreResponse, Phase2Error> {
    let now = store.now();
    let snapshot_id = store.read_transaction(|state| {
        let _ = active_lease_identity(
            state,
            actor,
            request.character_id,
            request.session_id,
            request.session_epoch,
            request.client_instance_id,
            now,
        )?;
        state
            .snapshot_by_revision
            .get(&(request.character_id, Revision::new(source_revision)))
            .copied()
            .ok_or(Phase2Error::NotFound)
    })?;
    let mut scoped = request.clone();
    scoped.snapshot_id = snapshot_id;
    restore(store, actor, &scoped)
}

fn created_at(ms: u64) -> Result<CreatedAt, Phase2Error> {
    // RFC3339 UTC without introducing a date/time dependency.
    let seconds = ms / 1_000;
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let days = i64::try_from(days).map_err(|_| Phase2Error::Internal)?;
    let (year, month, day) = civil_from_days(days);
    CreatedAt::new(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    ))
    .map_err(|_| Phase2Error::Internal)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

pub(crate) fn resume_package(
    store: &Store,
    actor: AuthenticatedActor,
    fence: coop_cloud::LeaseFence,
    revision: Option<u64>,
) -> Result<SignedManifestEnvelope, Phase2Error> {
    let now = store.now();
    let selected = store.read_transaction(|state| {
        active_lease(state, actor, fence, now)?;
        let character = state
            .characters
            .get(&fence.character_id)
            .ok_or(Phase2Error::NotFound)?;
        character
            .state
            .validate()
            .map_err(|_| Phase2Error::Internal)?;
        Ok(match revision {
            Some(0) => return Err(Phase2Error::NotFound),
            Some(value) => state
                .snapshot_by_revision
                .get(&(fence.character_id, Revision::new(value)))
                .and_then(|id| state.snapshots.get(id)),
            None => character
                .active_snapshot
                .and_then(|id| state.snapshots.get(&id)),
        }
        .ok_or(Phase2Error::NotFound)?
        .clone())
    })?;
    let manifest = bridge_manifest()?;
    let sav = selected
        .files
        .iter()
        .find(|file| file.artifact == ArtifactIdentity::CharacterSav)
        .ok_or(Phase2Error::Internal)?;
    let state_file = selected
        .files
        .iter()
        .find(|file| file.artifact == ArtifactIdentity::ResumeSs1);
    let build = ManifestBuildInfo {
        game_build_id: GameBuildId::new(manifest.game_build.id)
            .map_err(|_| Phase2Error::Internal)?,
        rom_sha256: Sha256Digest::parse(&manifest.game_build.rom_sha256)
            .map_err(|_| Phase2Error::Internal)?,
        mgba_version: MgbaVersion::new("0.10.5").map_err(|_| Phase2Error::Internal)?,
        bridge_abi: BridgeAbiVersion::new(manifest.net_bridge.abi_version)
            .map_err(|_| Phase2Error::Internal)?,
        protocol_version: ProtocolVersion::new(manifest.net_bridge.game_protocol_version)
            .map_err(|_| Phase2Error::Internal)?,
        pending_commits_sha256: selected.pending_commits_sha256,
        snapshot_id: selected.snapshot_id,
        session_epoch: selected.session_epoch,
        last_commit_id: selected.last_applied_commit,
    };
    let package = ResumePackageManifest::new(
        fence.character_id,
        selected.parent_revision,
        selected.revision,
        build,
        sav.sha256,
        state_file.map(|file| file.sha256),
        state_file.is_some(),
        created_at(selected.created_at.value())?,
    )
    .map_err(|_| Phase2Error::Internal)?;
    SignedManifestEnvelope::sign(
        package,
        &store.config.signing_key,
        store.config.signing_key_id.clone(),
    )
    .map_err(|_| Phase2Error::Internal)
}

pub(crate) fn resume_artifact(
    store: &Store,
    actor: AuthenticatedActor,
    fence: coop_cloud::LeaseFence,
    artifact: &str,
    revision: Option<u64>,
) -> Result<Vec<u8>, Phase2Error> {
    let identity = match artifact {
        "character.sav" => ArtifactIdentity::CharacterSav,
        "pending_commits.json" => ArtifactIdentity::PendingCommits,
        "resume.ss1" => ArtifactIdentity::ResumeSs1,
        _ => return Err(Phase2Error::NotFound),
    };
    let now = store.now();
    let (selected, file) = store.read_transaction(|state| {
        active_lease(state, actor, fence, now)?;
        let character = state
            .characters
            .get(&fence.character_id)
            .ok_or(Phase2Error::NotFound)?;
        let selected = match revision {
            Some(0) => return Err(Phase2Error::NotFound),
            Some(value) => state
                .snapshot_by_revision
                .get(&(fence.character_id, Revision::new(value)))
                .and_then(|id| state.snapshots.get(id)),
            None => character
                .active_snapshot
                .and_then(|id| state.snapshots.get(&id)),
        }
        .ok_or(Phase2Error::NotFound)?;
        let file = selected
            .files
            .iter()
            .find(|file| file.artifact == identity)
            .ok_or(Phase2Error::NotFound)?
            .clone();
        Ok((selected.clone(), file))
    })?;
    let key = Store::object_key(fence.character_id, selected.snapshot_id, identity);
    if file.size_bytes > MAX_RESUME_RESPONSE {
        return Err(Phase2Error::PayloadTooLarge);
    }
    let bytes = store.objects.get(&key)?.ok_or(Phase2Error::NotFound)?;
    if u64::try_from(bytes.len()).map_or(true, |size| size > MAX_RESUME_RESPONSE) {
        return Err(Phase2Error::PayloadTooLarge);
    }
    file.verify_bytes(&bytes)
        .map_err(|_| Phase2Error::Internal)?;
    Ok(bytes)
}

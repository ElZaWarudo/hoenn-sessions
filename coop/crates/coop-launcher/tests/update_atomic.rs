//! Focused proof for the signed release-set verifier and generation store.

#[path = "../src/update.rs"]
mod update;

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use update::{
    ArtifactDescriptor, ArtifactIdentity, ArtifactPayload, ArtifactSet, FIXED_ARTIFACT_IDENTITIES,
    GenerationStore, RELEASE_SCHEMA, ReleaseDescriptor, SignedReleaseEnvelope, TrustedReleaseKey,
    UpdateError, WINDOWS_PLATFORM,
};

fn key_material() -> (SigningKey, TrustedReleaseKey) {
    let signing = SigningKey::from_bytes(&[7_u8; 32]);
    let trusted = TrustedReleaseKey::new("pilot-2026", signing.verifying_key().to_bytes())
        .expect("fixture key is valid");
    (signing, trusted)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock is after unix epoch")
        .as_secs() as i64
}

fn sequence_for(release_id: &str) -> u64 {
    match release_id {
        "release-a" => 1,
        "release-b" => 2,
        "release-c" => 3,
        "release-resume" => 4,
        "release-map" => 5,
        _ => 1,
    }
}

fn fixture_at(release_id: &str, sequence: u64, now: i64) -> (ReleaseDescriptor, ArtifactSet) {
    let mut descriptors = Vec::new();
    let mut payloads = Vec::new();
    for identity in FIXED_ARTIFACT_IDENTITIES {
        let bytes = format!("{release_id}:{}", identity.as_str()).into_bytes();
        descriptors.push(ArtifactDescriptor::from_bytes(identity, &bytes));
        payloads.push(ArtifactPayload::new(identity, bytes));
    }
    (
        ReleaseDescriptor {
            schema: RELEASE_SCHEMA,
            release_id: release_id.to_owned(),
            sequence,
            issued_at: now - 1,
            expires_at: now + 86_400,
            platform: WINDOWS_PLATFORM.to_owned(),
            artifacts: descriptors,
        },
        ArtifactSet::new(payloads),
    )
}

fn fixture(release_id: &str) -> (ReleaseDescriptor, ArtifactSet) {
    fixture_at(release_id, sequence_for(release_id), unix_now())
}

fn signed_fixture(
    release_id: &str,
) -> (
    SigningKey,
    TrustedReleaseKey,
    ReleaseDescriptor,
    ArtifactSet,
    Vec<u8>,
) {
    let (signing, trusted) = key_material();
    let (descriptor, payloads) = fixture(release_id);
    let payload = serde_json::to_vec(&descriptor).expect("descriptor serializes");
    let envelope = SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing)
        .expect("fixture envelope signs");
    (signing, trusted, descriptor, payloads, envelope)
}

fn signed_fixture_at(
    release_id: &str,
    sequence: u64,
    now: i64,
) -> (
    SigningKey,
    TrustedReleaseKey,
    ReleaseDescriptor,
    ArtifactSet,
    Vec<u8>,
) {
    let (signing, trusted) = key_material();
    let (descriptor, payloads) = fixture_at(release_id, sequence, now);
    let payload = serde_json::to_vec(&descriptor).expect("descriptor serializes");
    let envelope = SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing)
        .expect("fixture envelope signs");
    (signing, trusted, descriptor, payloads, envelope)
}

fn verify_fixture(release_id: &str) -> (TrustedReleaseKey, update::VerifiedRelease, ArtifactSet) {
    let (_signing, trusted, _descriptor, payloads, envelope) = signed_fixture(release_id);
    let verified =
        SignedReleaseEnvelope::verify_json(&envelope, &trusted).expect("fixture envelope verifies");
    (trusted, verified, payloads)
}

fn verify_fixture_at(
    release_id: &str,
    sequence: u64,
    now: i64,
) -> (TrustedReleaseKey, update::VerifiedRelease, ArtifactSet) {
    let (_signing, trusted, _descriptor, payloads, envelope) =
        signed_fixture_at(release_id, sequence, now);
    let verified =
        SignedReleaseEnvelope::verify_json(&envelope, &trusted).expect("fixture envelope verifies");
    (trusted, verified, payloads)
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn verifies_exact_signed_payload_and_rejects_tampering_before_metadata() {
    let (_signing, trusted, _descriptor, _payloads, mut envelope) = signed_fixture("release-a");
    let mut wire: serde_json::Value = serde_json::from_slice(&envelope).unwrap();
    wire["payload"] = json!(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        br#"{"schema":1}"#,
    ));
    envelope = serde_json::to_vec(&wire).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::SignatureInvalid)
    ));

    wire["unexpected"] = json!(true);
    let malformed = serde_json::to_vec(&wire).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&malformed, &trusted),
        Err(UpdateError::MalformedEnvelope)
    ));
}

#[test]
fn rejects_unknown_descriptor_fields_and_unsupported_binding() {
    let (signing, trusted) = key_material();
    let (descriptor, _payloads) = fixture("release-a");
    let mut value = serde_json::to_value(&descriptor).unwrap();
    value["unexpected"] = json!(true);
    let payload = serde_json::to_vec(&value).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::MalformedDescriptor)
    ));

    let mut wrong = descriptor;
    wrong.platform = "linux-x86_64".to_owned();
    let payload = serde_json::to_vec(&wrong).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::UnsupportedPlatform(_))
    ));
}

#[test]
fn rejects_duplicate_missing_hash_and_size_descriptor_entries() {
    let (signing, trusted) = key_material();
    let (descriptor, _payloads) = fixture("release-a");

    let mut duplicate = descriptor.clone();
    duplicate.artifacts[1] = duplicate.artifacts[0].clone();
    let payload = serde_json::to_vec(&duplicate).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::DuplicateArtifact(_))
    ));

    let mut missing = descriptor.clone();
    missing.artifacts.pop();
    let payload = serde_json::to_vec(&missing).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::MissingArtifact(_))
    ));

    let mut invalid_hash = descriptor.clone();
    invalid_hash.artifacts[0].sha256 = "00".repeat(32);
    let payload = serde_json::to_vec(&invalid_hash).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    let verified = SignedReleaseEnvelope::verify_json(&envelope, &trusted).unwrap();
    let (_descriptor, payloads) = fixture("release-a");
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    assert!(matches!(
        store.install(&verified, payloads),
        Err(UpdateError::ArtifactDigestMismatch(_))
    ));

    let mut invalid_size = descriptor;
    invalid_size.artifacts[0].size = 0;
    let payload = serde_json::to_vec(&invalid_size).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::InvalidArtifactSize { .. })
    ));
}

#[test]
fn atomically_publishes_reuses_and_retains_prior_complete_generations() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (_trusted, first, first_payloads) = verify_fixture("release-a");
    let installed = store.install(&first, first_payloads.clone()).unwrap();
    assert!(!installed.reused());
    assert!(installed.path().join(update::COMPLETE_MARKER).is_file());

    let reused = store.install(&first, first_payloads).unwrap();
    assert!(reused.reused());
    assert_eq!(reused.path(), installed.path());

    let (_trusted, second, second_payloads) = verify_fixture("release-b");
    let second_installed = store.install(&second, second_payloads).unwrap();
    assert!(!second_installed.reused());
    assert!(installed.path().is_dir());
    assert!(second_installed.path().is_dir());
    assert_ne!(installed.path(), second_installed.path());
}

#[test]
fn failed_update_keeps_prior_generation_and_preserves_interrupted_staging() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (_trusted, first, first_payloads) = verify_fixture("release-a");
    let installed = store.install(&first, first_payloads).unwrap();

    let (_trusted, next, mut next_payloads) = verify_fixture("release-b");
    next_payloads = ArtifactSet::new(next_payloads.entries().iter().cloned().map(|mut payload| {
        if payload.identity == ArtifactIdentity::Rom {
            payload.bytes.push(0xff);
        }
        payload
    }));
    assert!(matches!(
        store.install(&next, next_payloads),
        Err(UpdateError::ArtifactSizeMismatch {
            artifact: ArtifactIdentity::Rom,
            ..
        })
    ));
    assert!(installed.path().is_dir());

    // Simulate a process crash after creating its owner-generated staging
    // directory but before writing completion evidence, then restart the
    // store.  Retry must quarantine only that proven-owned incomplete stage
    // and publish the fresh release without touching release-a.
    let interrupted = store
        .generations_path()
        .join(".staging-00000000000000000000000000000001");
    fs::create_dir(&interrupted).unwrap();
    fs::write(interrupted.join("partial"), b"partial").unwrap();
    let prior_path = installed.path().to_path_buf();
    drop(store);
    let restarted = GenerationStore::new(directory.path()).unwrap();
    let (_trusted, retry, retry_payloads) = verify_fixture("release-c");
    let retried = restarted.install(&retry, retry_payloads).unwrap();
    assert!(!retried.reused());
    assert!(prior_path.is_dir());
    assert!(!interrupted.exists());
    assert!(
        restarted
            .generations_path()
            .read_dir()
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".quarantine-"))
    );
}

#[test]
fn resumes_a_complete_compatible_staging_generation_after_restart() {
    const NOW: i64 = 1_800_000_000;
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (_trusted, release, payloads) = verify_fixture_at("release-resume", 4, NOW);
    let staging = store
        .generations_path()
        .join(".staging-00000000000000000000000000000002");
    fs::create_dir(&staging).unwrap();
    for payload in payloads.entries() {
        let path = staging.join(payload.identity.destination());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &payload.bytes).unwrap();
    }
    fs::write(
        staging.join(update::SIGNED_RELEASE_ENVELOPE),
        release.signed_envelope(),
    )
    .unwrap();
    let marker = format!(
        "schema={}\nplatform={}\nrelease_id={}\nsequence={}\nissued_at={}\nexpires_at={}\npayload_sha256={}\n",
        RELEASE_SCHEMA,
        WINDOWS_PLATFORM,
        release.release_id(),
        release.descriptor().sequence,
        release.descriptor().issued_at,
        release.descriptor().expires_at,
        hex_digest(release.payload()),
    );
    fs::write(staging.join(update::COMPLETE_MARKER), marker).unwrap();
    drop(store);

    let restarted = GenerationStore::new(directory.path()).unwrap();
    let resumed = restarted.install_at(&release, payloads, NOW).unwrap();
    assert!(resumed.reused());
    assert_eq!(resumed.release_id(), "release-resume");
    assert!(!staging.exists());
}

#[test]
fn rejects_replay_rollback_and_expired_signed_releases() {
    const NOW: i64 = 1_800_000_000;
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();

    let (_trusted, accepted, accepted_payloads) = verify_fixture_at("release-floor", 2, NOW);
    store.install_at(&accepted, accepted_payloads, NOW).unwrap();
    let accepted_path = directory
        .path()
        .join(update::GENERATIONS_DIRECTORY)
        .join("release-floor");

    let (_trusted, older, older_payloads) = verify_fixture_at("release-older", 1, NOW);
    assert!(matches!(
        store.install_at(&older, older_payloads, NOW),
        Err(UpdateError::ReleaseRollback {
            sequence: 1,
            floor: 2,
        })
    ));

    let (signing, trusted) = key_material();
    let (mut conflicting_descriptor, conflicting_payloads) = fixture_at("release-conflict", 2, NOW);
    conflicting_descriptor.expires_at = NOW + 86_400;
    let conflicting_payload = serde_json::to_vec(&conflicting_descriptor).unwrap();
    let conflicting_envelope =
        SignedReleaseEnvelope::sign_payload(&conflicting_payload, trusted.key_id(), &signing)
            .unwrap();
    let conflicting = SignedReleaseEnvelope::verify_json(&conflicting_envelope, &trusted).unwrap();
    assert!(matches!(
        store.install_at(&conflicting, conflicting_payloads, NOW),
        Err(UpdateError::SequenceConflict)
    ));

    let (signing, trusted) = key_material();
    let (mut expired_descriptor, expired_payloads) = fixture_at("release-expired", 3, NOW);
    expired_descriptor.expires_at = NOW;
    let expired_payload = serde_json::to_vec(&expired_descriptor).unwrap();
    let expired_envelope =
        SignedReleaseEnvelope::sign_payload(&expired_payload, trusted.key_id(), &signing).unwrap();
    let expired = SignedReleaseEnvelope::verify_json(&expired_envelope, &trusted).unwrap();
    assert!(matches!(
        store.install_at(&expired, expired_payloads, NOW),
        Err(UpdateError::ReleaseExpired(value)) if value == NOW
    ));

    let (signing, trusted) = key_material();
    let (mut future_descriptor, future_payloads) = fixture_at("release-future", 4, NOW);
    future_descriptor.issued_at = NOW + update::MAX_CLOCK_SKEW_SECONDS + 1;
    future_descriptor.expires_at = future_descriptor.issued_at + 86_400;
    let future_payload = serde_json::to_vec(&future_descriptor).unwrap();
    let future_envelope =
        SignedReleaseEnvelope::sign_payload(&future_payload, trusted.key_id(), &signing).unwrap();
    let future = SignedReleaseEnvelope::verify_json(&future_envelope, &trusted).unwrap();
    assert!(matches!(
        store.install_at(&future, future_payloads, NOW),
        Err(UpdateError::ReleaseIssuedInFuture(value))
            if value == NOW + update::MAX_CLOCK_SKEW_SECONDS + 1
    ));

    assert!(accepted_path.is_dir());
}

#[test]
fn opens_the_highest_accepted_generation_after_cold_restart() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (trusted, first, first_payloads) = verify_fixture("release-a");
    store.install(&first, first_payloads).unwrap();
    let (_trusted, second, second_payloads) = verify_fixture("release-b");
    store.install(&second, second_payloads).unwrap();
    drop(store);

    let restarted = GenerationStore::new(directory.path()).unwrap();
    let current = restarted
        .open_accepted_current(&trusted, unix_now())
        .unwrap();
    assert!(current.reused());
    assert_eq!(current.release_id(), "release-b");
    assert!(current.path().is_dir());
}

#[test]
fn cold_open_heals_a_marker_orphaned_by_a_crash_before_head_persist() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (trusted, first, first_payloads) = verify_fixture("release-a");
    store.install(&first, first_payloads).unwrap();
    let (_trusted, second, second_payloads) = verify_fixture("release-b");
    store.install(&second, second_payloads).unwrap();

    // Simulate a crash after the marker rename was synced but before the
    // head record was persisted: the marker (and its complete generation)
    // survived, the head did not.
    let heads: Vec<_> = store
        .generations_path()
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".accepted-head-"))
        })
        .collect();
    assert!(!heads.is_empty(), "install persists head records");
    for head in &heads {
        fs::remove_file(head).unwrap();
    }
    drop(store);

    let restarted = GenerationStore::new(directory.path()).unwrap();
    let current = restarted
        .open_accepted_current(&trusted, unix_now())
        .expect("orphan marker heals instead of bricking the store");
    assert!(current.reused());
    assert_eq!(current.release_id(), "release-b");

    // The heal is durable and the store accepts further installs.
    let healed: Vec<_> = restarted
        .generations_path()
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".accepted-head-"))
        })
        .collect();
    assert!(!healed.is_empty(), "heal persists rebuilt head records");
    let (_trusted, third, third_payloads) = verify_fixture("release-c");
    let installed = restarted.install(&third, third_payloads).unwrap();
    assert!(!installed.reused());
    assert_eq!(installed.release_id(), "release-c");
}

#[test]
fn cold_open_blocks_when_the_newest_accepted_marker_is_deleted() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (trusted, first, first_payloads) = verify_fixture("release-a");
    store.install(&first, first_payloads).unwrap();
    let (_trusted, second, second_payloads) = verify_fixture("release-b");
    store.install(&second, second_payloads).unwrap();

    let newest_marker = store
        .generations_path()
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".accepted-generation-"))
                && fs::read(path)
                    .map(|bytes| {
                        bytes
                            .windows(b"release_id=release-b".len())
                            .any(|window| window == b"release_id=release-b")
                    })
                    .unwrap_or(false)
        })
        .expect("newest release marker is persisted");
    fs::remove_file(newest_marker).unwrap();

    drop(store);
    let restarted = GenerationStore::new(directory.path()).unwrap();
    assert!(matches!(
        restarted.open_accepted_current(&trusted, unix_now()),
        Err(UpdateError::MarkerHistoryRegression(_)) | Err(UpdateError::InvalidCurrentMarker(_))
    ));
}

#[test]
fn cold_open_rejects_tampered_envelope_artifact_and_extra_file() {
    const NOW: i64 = 1_800_000_000;
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (trusted, release, payloads) = verify_fixture_at("release-cold", 1, NOW);
    let installed = store.install_at(&release, payloads, NOW).unwrap();

    let marker_path = store
        .generations_path()
        .read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".accepted-generation-"))
        })
        .expect("accepted marker is persisted");
    let marker = fs::read(&marker_path).unwrap();
    let tampered_marker = String::from_utf8(marker.clone())
        .unwrap()
        .replace("sequence=1", "sequence=9");
    fs::write(&marker_path, tampered_marker).unwrap();
    assert!(matches!(
        store.open_accepted_current(&trusted, NOW),
        Err(UpdateError::InvalidCurrentMarker(_))
    ));
    fs::write(&marker_path, marker).unwrap();

    let envelope_path = installed.path().join(update::SIGNED_RELEASE_ENVELOPE);
    let envelope = fs::read(&envelope_path).unwrap();
    fs::write(&envelope_path, b"{}").unwrap();
    assert!(matches!(
        store.open_accepted_current(&trusted, NOW),
        Err(UpdateError::MalformedEnvelope)
    ));
    fs::write(&envelope_path, envelope).unwrap();

    let rom_path = installed.path().join(ArtifactIdentity::Rom.destination());
    let rom = fs::read(&rom_path).unwrap();
    fs::write(&rom_path, b"tampered").unwrap();
    assert!(matches!(
        store.open_accepted_current(&trusted, NOW),
        Err(UpdateError::InvalidCompleteGeneration(_))
    ));
    fs::write(&rom_path, rom).unwrap();

    let extra_path = installed.path().join("unexpected");
    fs::write(&extra_path, b"unexpected").unwrap();
    assert!(matches!(
        store.open_accepted_current(&trusted, NOW),
        Err(UpdateError::InvalidCompleteGeneration(_))
    ));
}

#[cfg(windows)]
#[test]
fn accepted_current_guard_blocks_desktop_and_sidecar_replacement() {
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let (trusted, release, payloads) = verify_fixture("release-guard");
    store.install(&release, payloads).unwrap();
    let accepted = store.open_accepted_current(&trusted, unix_now()).unwrap();
    let handoff = accepted.handoff();
    let desktop = handoff
        .artifact(ArtifactIdentity::DesktopApp)
        .unwrap()
        .path()
        .to_path_buf();
    let sidecar = handoff
        .artifact(ArtifactIdentity::Sidecar)
        .unwrap()
        .path()
        .to_path_buf();

    assert!(fs::write(&desktop, b"replacement").is_err());
    assert!(fs::write(&sidecar, b"replacement").is_err());

    let desktop_replacement = desktop.with_extension("replacement");
    fs::write(&desktop_replacement, b"replacement").unwrap();
    assert!(fs::rename(&desktop_replacement, &desktop).is_err());
    fs::remove_file(desktop_replacement).unwrap();

    let sidecar_replacement = sidecar.with_extension("replacement");
    fs::write(&sidecar_replacement, b"replacement").unwrap();
    assert!(fs::rename(&sidecar_replacement, &sidecar).is_err());
    fs::remove_file(sidecar_replacement).unwrap();
}

#[test]
fn rejects_traversal_and_duplicate_transport_entries() {
    let (signing, trusted) = key_material();
    let (mut descriptor, _payloads) = fixture("../escape");
    descriptor.release_id = "../escape".to_owned();
    let payload = serde_json::to_vec(&descriptor).unwrap();
    let envelope =
        SignedReleaseEnvelope::sign_payload(&payload, trusted.key_id(), &signing).unwrap();
    assert!(matches!(
        SignedReleaseEnvelope::verify_json(&envelope, &trusted),
        Err(UpdateError::InvalidReleaseId(_))
    ));

    let (_descriptor, mut duplicate) = fixture("release-a");
    let first = duplicate.entries()[0].clone();
    let mut entries = duplicate.entries().to_vec();
    entries.push(first);
    duplicate = ArtifactSet::new(entries);
    let (_trusted, verified, _valid) = verify_fixture("release-a");
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    assert!(matches!(
        store.install(&verified, duplicate),
        Err(UpdateError::TooManyArtifacts(_)) | Err(UpdateError::DuplicateArtifact(_))
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_generation_inputs() {
    let directory = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let link = directory.path().join("link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(matches!(
        GenerationStore::new(&link),
        Err(UpdateError::SymlinkOrReparse(_))
    ));

    let store = GenerationStore::new(directory.path()).unwrap();
    let (_trusted, verified, payloads) = verify_fixture("release-a");
    let installed = store.install(&verified, payloads).unwrap();
    let target = installed.path().join(ArtifactIdentity::Rom.destination());
    let moved = outside.path().join("rom");
    fs::rename(&target, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &target).unwrap();
    assert!(matches!(
        store.validate_generation(&verified),
        Err(UpdateError::InvalidCompleteGeneration(_))
    ));
}

#[test]
fn map_sources_are_supported_without_accepting_unknown_destinations() {
    let (_trusted, verified, payloads) = verify_fixture("release-map");
    let map: BTreeMap<_, _> = payloads
        .entries()
        .iter()
        .map(|entry| (entry.identity, entry.bytes.clone()))
        .collect();
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let installed = store.install(&verified, &map).unwrap();
    assert_eq!(installed.release_id(), "release-map");
    assert!(Path::new(ArtifactIdentity::Rom.destination()).is_relative());
}

use std::fs;

use coop_cloud::{
    CharacterId, ClientInstanceId, LeaseFence, Revision, SessionEpoch, SessionId, Sha256Digest,
};
use coop_launcher::{RecoveryDiscovery, RecoveryError, RecoveryMarker, RecoveryMarkerV2};
use tempfile::tempdir;
use uuid::Uuid;

fn candidate(root: &std::path::Path, name: &str, marker: &[u8], sav: &[u8]) {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    fs::write(path.join("character.sav"), sav).unwrap();
    fs::write(path.join("recovery.marker"), marker).unwrap();
}

fn marker() -> RecoveryMarkerV2 {
    RecoveryMarkerV2::new(
        LeaseFence::new(
            SessionId::new(Uuid::from_u128(1)).unwrap(),
            CharacterId::new(Uuid::from_u128(2)).unwrap(),
            Revision::new(8),
            SessionEpoch::new(4).unwrap(),
            ClientInstanceId::new(Uuid::from_u128(3)).unwrap(),
        ),
        Revision::new(8),
        9,
        Sha256Digest::of_bytes(b"next-save"),
    )
    .unwrap()
}

#[test]
fn no_evidence_is_not_a_reconciliation_error() {
    let root = tempdir().unwrap();
    assert!(
        RecoveryDiscovery::discover(root.path())
            .unwrap()
            .candidate()
            .is_none()
    );
}

#[test]
fn empty_workspaces_do_not_hide_one_orphan_save() {
    let root = tempdir().unwrap();
    for index in 0..3 {
        fs::create_dir(root.path().join(format!("coop-session-empty-{index}"))).unwrap();
    }
    let orphan = root.path().join("coop-session-orphan");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("character.sav"), b"unsynced-save").unwrap();

    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();
    assert_eq!(found.marker(), &RecoveryMarker::OrphanSave);
    assert_eq!(
        found.save_sha256(),
        Sha256Digest::of_bytes(b"unsynced-save")
    );
    found.revalidate().unwrap();
    // Discovery itself never discards an orphan. The reconciler may retire it
    // only after matching its digest to a fresh signed cloud head.
    assert_eq!(
        fs::read(orphan.join("character.sav")).unwrap(),
        b"unsynced-save"
    );
}

#[test]
fn orphan_save_cannot_be_silently_replaced_or_made_ambiguous() {
    let root = tempdir().unwrap();
    let orphan = root.path().join("coop-session-orphan");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("character.sav"), b"unsynced-save").unwrap();
    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();
    #[cfg(windows)]
    {
        assert!(fs::write(orphan.join("character.sav"), b"different-save").is_err());
        found.revalidate().unwrap();
    }
    #[cfg(not(windows))]
    {
        fs::write(orphan.join("character.sav"), b"different-save").unwrap();
        assert_eq!(found.revalidate().unwrap_err(), RecoveryError::Replaced);
    }
    assert!(orphan.join("character.sav").exists());

    let second = root.path().join("coop-session-second");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("character.sav"), b"another-save").unwrap();
    assert_eq!(
        RecoveryDiscovery::discover(root.path()).unwrap_err(),
        RecoveryError::Ambiguous
    );
    assert!(orphan.join("character.sav").exists());
    assert!(second.join("character.sav").exists());
}

#[test]
fn missing_workspace_root_is_no_evidence() {
    let root = tempdir().unwrap();
    let missing = root.path().join("not-created");
    assert_eq!(
        RecoveryDiscovery::discover(&missing).unwrap_err(),
        RecoveryError::NoEvidence
    );
}

#[test]
fn unrelated_entries_do_not_consume_recovery_candidate_budget() {
    let root = tempdir().unwrap();
    for index in 0..16 {
        fs::create_dir(root.path().join(format!("unrelated-{index}"))).unwrap();
    }
    candidate(
        root.path(),
        "coop-session-valid",
        b"coop-recovery-v1\n",
        b"save",
    );
    assert!(
        RecoveryDiscovery::discover(root.path())
            .unwrap()
            .candidate()
            .is_some()
    );
}

#[test]
fn legacy_evidence_is_bounded_and_discoverable_but_never_upgraded() {
    let root = tempdir().unwrap();
    candidate(
        root.path(),
        "coop-session-legacy",
        b"coop-recovery-v1\n",
        b"old",
    );
    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();
    assert!(matches!(found.marker(), RecoveryMarker::LegacyV1));
    assert_eq!(found.save_sha256(), Sha256Digest::of_bytes(b"old"));
}

#[test]
fn v2_evidence_binds_fence_generation_and_exact_digest() {
    let root = tempdir().unwrap();
    let marker = marker();
    let sav = b"next-save";
    candidate(
        root.path(),
        "coop-session-v2",
        &marker.encode().unwrap(),
        sav,
    );
    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();
    assert!(matches!(found.marker(), RecoveryMarker::V2(value) if value == &marker));
    found.revalidate().unwrap();
}

#[test]
fn ambiguity_extra_entries_and_replacement_fail_closed() {
    let root = tempdir().unwrap();
    candidate(
        root.path(),
        "coop-session-a",
        b"coop-recovery-v1\n",
        b"same",
    );
    candidate(
        root.path(),
        "coop-recovery-b",
        b"coop-recovery-v1\n",
        b"same",
    );
    assert_eq!(
        RecoveryDiscovery::discover(root.path()).unwrap_err(),
        RecoveryError::Ambiguous
    );

    let root = tempdir().unwrap();
    candidate(
        root.path(),
        "coop-session-a",
        b"coop-recovery-v1\n",
        b"same",
    );
    fs::write(
        root.path().join("coop-session-a").join("unexpected"),
        b"secret-bearing-extra",
    )
    .unwrap();
    assert_eq!(
        RecoveryDiscovery::discover(root.path()).unwrap_err(),
        RecoveryError::Malformed
    );

    let root = tempdir().unwrap();
    candidate(
        root.path(),
        "coop-session-a",
        b"coop-recovery-v1\n",
        b"same",
    );
    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();
    fs::write(
        root.path().join("coop-session-a").join("recovery.marker"),
        marker().encode().unwrap(),
    )
    .unwrap();
    assert_eq!(found.revalidate().unwrap_err(), RecoveryError::Replaced);
}

#[test]
fn locally_forgeable_consistent_v2_is_unresolved_until_server_authorization() {
    let root = tempdir().unwrap();
    let marker = marker();
    candidate(
        root.path(),
        "coop-session-forged",
        &marker.encode().unwrap(),
        b"next-save",
    );
    let found = RecoveryDiscovery::discover(root.path())
        .unwrap()
        .candidate()
        .unwrap();

    // Discovery proves only that the marker and SAV agree locally.  The
    // reconciler's divergent-v2 gate returns AuthorizationMissing before it
    // can invoke prepare/upload/finalize; the evidence remains untouched.
    assert_eq!(found.save_sha256(), marker.save_sha256);
    assert!(found.path().exists());
    assert!(found.path().join("character.sav").exists());
    assert!(found.path().join("recovery.marker").exists());
}

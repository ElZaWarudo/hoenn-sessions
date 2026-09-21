#[path = "../src/config.rs"]
mod config;
#[path = "../src/launch.rs"]
mod launch;

use tempfile::TempDir;

use config::InstallRoots;
use launch::{LaunchTarget, SingleInstanceGuard, select_target};

#[test]
fn bootstrap_selects_release_installed_by_desktop() {
    use coop_launcher::update::{
        ArtifactDescriptor, ArtifactIdentity, ArtifactPayload, ArtifactSet, GenerationStore,
        ReleaseDescriptor, SignedReleaseEnvelope, TrustedReleaseKey,
    };
    let local = TempDir::new().unwrap();
    let roots = InstallRoots::from_local_app_data(local.path()).unwrap();
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
    let key = TrustedReleaseKey::new("test", signing.verifying_key().to_bytes()).unwrap();
    let now = 1_790_000_000;
    let mut descriptors = Vec::new();
    let mut payloads = Vec::new();
    for &id in ArtifactIdentity::all() {
        let bytes = format!("fixture:{}", id.as_str()).into_bytes();
        descriptors.push(ArtifactDescriptor::from_bytes(id, &bytes));
        payloads.push(ArtifactPayload::new(id, bytes));
    }
    let descriptor = ReleaseDescriptor {
        schema: 1,
        release_id: "desktop-installed".into(),
        sequence: 1,
        issued_at: now - 1,
        expires_at: now + 3600,
        platform: "windows-x86_64".into(),
        artifacts: descriptors,
    };
    let envelope = SignedReleaseEnvelope::sign_payload(
        &serde_json::to_vec(&descriptor).unwrap(),
        "test",
        &signing,
    )
    .unwrap();
    let verified = SignedReleaseEnvelope::verify_json(&envelope, &key).unwrap();
    // The desktop's UserPaths installs signed generations in runtime/releases.
    let store = GenerationStore::new(local.path().join("Hoenn Sessions/runtime/releases")).unwrap();
    store
        .install_at(&verified, ArtifactSet::new(payloads), now)
        .unwrap();
    assert!(
        matches!(
            select_target(&roots, Some(&key), now),
            LaunchTarget::Accepted { .. }
        ),
        "bootstrap must reopen the desktop-installed release, not fallback to onboarding"
    );
}

#[test]
fn roots_are_fixed_below_local_app_data() {
    let local = TempDir::new().expect("temp root");
    let roots = InstallRoots::from_local_app_data(local.path()).expect("absolute root");
    assert_eq!(
        roots.install_root(),
        local.path().join("Programs").join(config::PRODUCT_NAME)
    );
    assert_eq!(
        roots.runtime_root(),
        local.path().join(config::PRODUCT_NAME).join("runtime")
    );
    assert_eq!(
        roots.onboarding_path(),
        local
            .path()
            .join("Programs")
            .join(config::PRODUCT_NAME)
            .join("app")
            .join("hoenn-sessions-onboarding.exe")
    );
}

#[test]
fn missing_trust_or_generation_can_only_select_onboarding() {
    let local = TempDir::new().expect("temp root");
    let roots = InstallRoots::from_local_app_data(local.path()).expect("absolute root");
    let target = select_target(&roots, None, 1_800_000_000);
    assert!(matches!(target, LaunchTarget::Onboarding(path) if path == roots.onboarding_path()));

    assert!(!roots.runtime_root().join("generations").exists());
}

#[test]
fn bootstrap_lock_is_single_instance_and_recovers_after_drop() {
    let local = TempDir::new().expect("temp root");
    let roots = InstallRoots::from_local_app_data(local.path()).expect("absolute root");
    let first = SingleInstanceGuard::acquire(&roots).expect("first instance");
    assert!(matches!(
        SingleInstanceGuard::acquire(&roots),
        Err(launch::LaunchError::AlreadyRunning)
    ));
    drop(first);
    let second = SingleInstanceGuard::acquire(&roots).expect("lock released");
    assert!(roots.lock_path().exists());
    drop(second);
    assert!(roots.lock_path().exists());
}

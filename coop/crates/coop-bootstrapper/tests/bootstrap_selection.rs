#[path = "../src/config.rs"]
mod config;
#[path = "../src/launch.rs"]
mod launch;

use tempfile::TempDir;

use config::InstallRoots;
use launch::{LaunchTarget, SingleInstanceGuard, select_target};

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

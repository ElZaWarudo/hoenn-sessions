#![cfg(windows)]

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use coop_launcher::{
    compat::BuildCompatibility,
    process::{CommandSpec, GuardedMgbaChild, staged_rom_marker_contents, staged_rom_marker_path},
};

fn wait_for_ready_save(child: &mut GuardedMgbaChild, implicit_save: &Path) {
    let save_deadline = Instant::now() + Duration::from_secs(3);
    let mut save_ready = false;
    while Instant::now() < save_deadline {
        if fs::metadata(implicit_save)
            .map(|metadata| metadata.len() == 131_072)
            .unwrap_or(false)
        {
            save_ready = true;
            break;
        }
        assert!(
            child
                .try_wait()
                .expect("gameplay status snapshot")
                .is_none(),
            "valid ROM must keep the official mGBA process alive"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        save_ready,
        "valid ROM must create the exact 128 KiB implicit SRAM file"
    );
    assert!(
        child
            .try_wait()
            .expect("gameplay status after SRAM readiness")
            .is_none(),
        "valid ROM must remain live after SRAM readiness"
    );
}

/// Opt-in conformance seam for the pinned official runtime and a matching
/// locally built ROM. Normal CI does not possess either copyrighted artifact.
#[test]
#[ignore = "requires COOP_REAL_MGBA and COOP_REAL_ROM local artifact paths"]
fn validates_and_opens_the_pinned_official_mgba_with_a_guarded_rom() {
    let mgba = PathBuf::from(
        std::env::var_os("COOP_REAL_MGBA")
            .expect("COOP_REAL_MGBA must name the pinned official mGBA executable"),
    );
    let rom = PathBuf::from(
        std::env::var_os("COOP_REAL_ROM")
            .expect("COOP_REAL_ROM must name a ROM matching the generated manifest"),
    );
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("dist")
        .join("bridge_manifest.json");

    let compatibility = BuildCompatibility::validate(&manifest, &rom, &mgba)
        .expect("the pinned executable, version probe, ROM, and manifest must agree");

    assert_eq!(compatibility.manifest.schema_version, 3);
    let emulator = compatibility
        .manifest
        .emulator
        .expect("schema 3 requires an emulator identity");
    assert_eq!(emulator.version, "0.10.5");
    assert_eq!(emulator.platform, "windows-x64");
    assert_eq!(emulator.variant, "Qt");

    // Do not mutate the caller-owned ROM. Recreate the launcher's private
    // staged namespace and ownership marker, then use the production guarded
    // mGBA spawn path. The marker and ROM are removed only after the contained
    // process has been stopped and reaped.
    let staging = tempfile::tempdir().expect("private staging directory");
    let staged_rom = staging.path().join("real artifact with spaces.gba");
    fs::copy(&rom, &staged_rom).expect("ROM copies to private staging");
    let marker = staged_rom_marker_path(&staged_rom);
    fs::write(
        &marker,
        staged_rom_marker_contents(&staged_rom).expect("staged ROM identity"),
    )
    .expect("ownership marker is writable");
    let mut spec = CommandSpec::mgba_owned_staged(&mgba, &staged_rom, &marker)
        .expect("staged ROM is bound to its ownership marker");
    let canonical_rom = staged_rom
        .canonicalize()
        .expect("canonical staged ROM path");
    assert_eq!(
        PathBuf::from(spec.args.last().expect("owned ROM argv")),
        canonical_rom
    );
    let mut child = spec
        .spawn_guarded_mgba()
        .expect("official mGBA opens through the production Job boundary");
    drop(spec);
    assert!(
        staged_rom.exists() && marker.exists(),
        "the guarded owner must retain artifacts after the originating spec is dropped"
    );
    assert!(
        fs::remove_file(&staged_rom).is_err(),
        "the live ROM integrity guard must deny replacement/deletion"
    );
    let implicit_save = staged_rom.with_extension("sav");
    wait_for_ready_save(&mut child, &implicit_save);
    let runtime = tokio::runtime::Runtime::new().expect("runtime starts");
    runtime.block_on(async {
        child
            .stop()
            .await
            .expect("contained gameplay process stops");
    });
    drop(child);
    assert!(
        !implicit_save.exists(),
        "owned implicit SRAM must be cleaned after reap"
    );
    assert!(
        !staged_rom.exists(),
        "owned staged ROM must be cleaned after reap"
    );
    assert!(
        !marker.exists(),
        "ownership marker must be cleaned after reap"
    );
}

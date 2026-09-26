use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::SigningKey;
use serde_json::Value;
use tempfile::TempDir;

const SEED: &str = "0707070707070707070707070707070707070707070707070707070707070707";
const OTHER_SEED: &str = "0808080808080808080808080808080808080808080808080808080808080808";
const KEY_ID: &str = "pilot-v1";

const ARTIFACTS: [(&str, &str); 11] = [
    ("desktop-app", "app/coop-launcher.exe"),
    ("managed-mgba", "runtime/mgba.exe"),
    ("rom", "runtime/game.gba"),
    ("sidecar", "runtime/coop-sidecar.exe"),
    ("bridge-main", "bridge/main.lua"),
    ("bridge-memory", "bridge/memory.lua"),
    ("bridge-protocol", "bridge/protocol.lua"),
    ("bridge-addresses", "bridge/generated_addresses.lua"),
    ("compatibility-manifest", "bridge_manifest.json"),
    ("trust-bundle", "trust/release-trust.json"),
    ("notices", "THIRD_PARTY_NOTICES.txt"),
];

fn public_key_hex(seed_hex: &str) -> String {
    let mut seed = [0_u8; 32];
    for (index, pair) in seed_hex.as_bytes().chunks_exact(2).enumerate() {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => 0,
        };
        seed[index] = digit(pair[0]) * 16 + digit(pair[1]);
    }
    let key = SigningKey::from_bytes(&seed);
    key.verifying_key()
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_coop-release-tool"))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn fixture(dir: &TempDir) -> Vec<String> {
    ARTIFACTS
        .iter()
        .enumerate()
        .map(|(index, (id, destination))| {
            let path = dir.path().join(destination);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("artifact-{index}\n")).unwrap();
            format!("{id}={}", path.display())
        })
        .collect()
}

#[test]
fn game_channel_signs_only_rom_and_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let rom = dir.path().join("game.gba");
    let manifest = dir.path().join("bridge_manifest.json");
    let envelope = dir.path().join("game-envelope.json");
    fs::write(&rom, b"test-rom").unwrap();
    fs::write(&manifest, b"test-manifest").unwrap();
    let signed = command()
        .args([
            "sign-game",
            "--release-id",
            "game-one",
            "--sequence",
            "43",
            "--issued-at",
            &(unix_now() - 60).to_string(),
            "--expires-at",
            &(unix_now() + 3600).to_string(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
            "--output",
            envelope.to_str().unwrap(),
            "--artifact",
            &format!("rom={}", rom.display()),
            "--artifact",
            &format!("compatibility-manifest={}", manifest.display()),
        ])
        .env("HOENN_RELEASE_PRIVATE_SEED_HEX", SEED)
        .output()
        .unwrap();
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    let bytes = fs::read(&envelope).unwrap();
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    let payload = BASE64.decode(wire["payload"].as_str().unwrap()).unwrap();
    let descriptor: Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(descriptor["platform"], "game");
    assert_eq!(descriptor["artifacts"].as_array().unwrap().len(), 2);
    let verified = command()
        .args([
            "verify-game",
            "--envelope",
            envelope.to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let windows = command()
        .args([
            "verify",
            "--envelope",
            envelope.to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(!windows.status.success());
    let wrong_key = command()
        .args([
            "verify-game",
            "--envelope",
            envelope.to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(OTHER_SEED),
        ])
        .output()
        .unwrap();
    assert!(!wrong_key.status.success());
}

fn sign(dir: &TempDir, extra: &[&str]) -> std::process::Output {
    let mut args = vec![
        "sign".to_owned(),
        "--release-id".to_owned(),
        "aabbccdd".to_owned(),
        "--sequence".to_owned(),
        "42".to_owned(),
        "--issued-at".to_owned(),
        (unix_now() - 60).to_string(),
        "--expires-at".to_owned(),
        (unix_now() + 3600).to_string(),
        "--key-id".to_owned(),
        KEY_ID.to_owned(),
        "--public-key-hex".to_owned(),
    ];
    let public = public_key_hex(SEED);
    args.push(public);
    let output = dir.path().join("release-envelope.json");
    args.extend(["--output".to_owned(), output.to_str().unwrap().to_owned()]);
    let artifacts = fixture(dir);
    for artifact in artifacts {
        args.push("--artifact".to_owned());
        args.push(artifact);
    }
    for argument in extra {
        args.push((*argument).to_owned());
    }
    command()
        .args(args)
        .env("HOENN_RELEASE_PRIVATE_SEED_HEX", SEED)
        .output()
        .unwrap()
}

#[test]
fn canonical_envelope_verifies_and_has_exact_inventory_and_destinations() {
    let dir = tempfile::tempdir().unwrap();
    let result = sign(&dir, &[]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let envelope: Value = serde_json::from_slice(&fs::read(dir.path().join("release-envelope.json")).unwrap()).unwrap();
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["key_id"], KEY_ID);
    assert!(envelope["payload"].as_str().unwrap().len() > 0);
    assert!(envelope["signature"].as_str().unwrap().len() > 0);
    let payload = BASE64.decode(envelope["payload"].as_str().unwrap()).unwrap();
    let descriptor: Value = serde_json::from_slice(&payload).unwrap();
    let artifacts = descriptor["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), ARTIFACTS.len());
    for ((id, destination), artifact) in ARTIFACTS.iter().zip(artifacts) {
        assert_eq!(artifact["id"], *id);
        let expected = Path::new(destination).to_string_lossy();
        assert!(!expected.is_empty(), "fixed destination must be non-empty");
    }
    let verify = command()
        .args([
            "verify",
            "--envelope",
            dir.path().join("release-envelope.json").to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
}

#[test]
fn tamper_is_rejected_and_key_mismatch_fails_before_output() {
    let dir = tempfile::tempdir().unwrap();
    let result = sign(&dir, &[]);
    assert!(result.status.success());
    let path = dir.path().join("release-envelope.json");
    let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let payload = envelope["payload"].as_str().unwrap().to_owned();
    let replacement = if payload.starts_with('A') { 'B' } else { 'A' };
    envelope["payload"] = Value::String(format!("{replacement}{}", &payload[1..]));
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let tampered = command()
        .args([
            "verify",
            "--envelope",
            path.to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(!tampered.status.success());

    let mismatch_dir = tempfile::tempdir().unwrap();
    let mismatch = sign(
        &mismatch_dir,
        &["--public-key-hex", &public_key_hex(OTHER_SEED)],
    );
    assert!(!mismatch.status.success());
    assert!(!String::from_utf8_lossy(&mismatch.stdout).contains(SEED));
    assert!(!String::from_utf8_lossy(&mismatch.stderr).contains(SEED));
    assert!(!mismatch_dir.path().join("release-envelope.json").exists());
}

#[test]
fn validity_window_is_bounded_and_seed_is_not_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let result = sign(&dir, &["--expires-at", "1807776001"]);
    assert!(!result.status.success(), "a lifetime over 90 days must fail");
    let output = String::from_utf8_lossy(&result.stdout);
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(!output.contains(SEED));
    assert!(!error.contains(SEED));
    let files = walk_files(dir.path());
    for file in files {
        assert!(!fs::read(file).unwrap().windows(SEED.len()).any(|window| window == SEED.as_bytes()));
    }
}

fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn duplicate_artifacts_and_missing_inventory_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = fixture(&dir);
    let public = public_key_hex(SEED);
    let output = dir.path().join("out.json");
    let issued = (unix_now() - 60).to_string();
    let expires = (unix_now() + 3600).to_string();
    let args = vec![
        "sign".to_owned(), "--release-id".to_owned(), "aabbccdd".to_owned(),
        "--sequence".to_owned(), "1".to_owned(), "--issued-at".to_owned(), issued,
        "--expires-at".to_owned(), expires, "--key-id".to_owned(), KEY_ID.to_owned(),
        "--public-key-hex".to_owned(), public, "--output".to_owned(),
        output.to_str().unwrap().to_owned(), "--artifact".to_owned(), artifacts[0].clone(),
    ];
    let result = command().args(args).env("HOENN_RELEASE_PRIVATE_SEED_HEX", SEED).output().unwrap();
    assert!(!result.status.success());
}

#[test]
fn artifact_map_is_canonical_and_duplicate_map_is_not_accepted() {
    let mut map = BTreeMap::new();
    for (id, destination) in ARTIFACTS {
        assert!(map.insert(id, destination).is_none());
    }
    assert_eq!(map.len(), 11);
}

#[test]
fn verification_rejects_expired_and_future_dated_envelopes() {
    let expired = tempfile::tempdir().unwrap();
    let expired_result = sign(&expired, &[
        "--issued-at",
        &(unix_now() - 3600).to_string(),
        "--expires-at",
        &(unix_now() - 1).to_string(),
    ]);
    assert!(expired_result.status.success(), "signing an already-expired fixture should be allowed");
    let expired_verify = command()
        .args([
            "verify",
            "--envelope",
            expired.path().join("release-envelope.json").to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(!expired_verify.status.success(), "expired envelope must fail freshness verification");

    let future = tempfile::tempdir().unwrap();
    let future_result = sign(&future, &[
        "--issued-at",
        &(unix_now() + 3600).to_string(),
        "--expires-at",
        &(unix_now() + 7200).to_string(),
    ]);
    assert!(future_result.status.success(), "future fixture should be structurally signable");
    let future_verify = command()
        .args([
            "verify",
            "--envelope",
            future.path().join("release-envelope.json").to_str().unwrap(),
            "--key-id",
            KEY_ID,
            "--public-key-hex",
            &public_key_hex(SEED),
        ])
        .output()
        .unwrap();
    assert!(!future_verify.status.success(), "future-dated envelope must fail freshness verification");
}

#[test]
fn quiet_key_gate_verifies_correspondence_without_emitting_material() {
    let good = command()
        .args(["check-key", "--public-key-hex", &public_key_hex(SEED)])
        .env("HOENN_RELEASE_PRIVATE_SEED_HEX", SEED)
        .output()
        .unwrap();
    assert!(good.status.success(), "matching release key must pass quiet gate");
    assert!(good.stdout.is_empty());
    assert!(good.stderr.is_empty());

    let bad = command()
        .args(["check-key", "--public-key-hex", &public_key_hex(OTHER_SEED)])
        .env("HOENN_RELEASE_PRIVATE_SEED_HEX", SEED)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    let combined = format!("{}{}", String::from_utf8_lossy(&bad.stdout), String::from_utf8_lossy(&bad.stderr));
    assert!(!combined.contains(SEED));
    assert!(!combined.contains(&public_key_hex(SEED)));
}

//! Build and verify the private-pilot Windows runtime envelope.
//!
//! The launcher owns the wire types and fixed artifact mapping.  This binary
//! only turns trusted build outputs into those types, hashes them, and signs
//! the canonical payload.  The private seed is read from a protected runtime
//! environment variable and is never accepted as a command-line argument,
//! printed, or written to disk.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use coop_launcher::{
    ArtifactIdentity, MAX_ENVELOPE_BYTES, ReleaseDescriptor, SignedReleaseEnvelope,
    TrustedReleaseKey,
    update::{
        ArtifactDescriptor, FIXED_ARTIFACT_IDENTITIES, MAX_PAYLOAD_BYTES, RELEASE_SCHEMA,
        WINDOWS_PLATFORM,
    },
};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const MAX_KEY_ID_BYTES: usize = 64;
const DEFAULT_SEED_ENV: &str = "HOENN_RELEASE_PRIVATE_SEED_HEX";
const DEFAULT_KEY_ID_ENV: &str = "HOENN_RELEASE_TRUST_KEY_ID";
const DEFAULT_PUBLIC_KEY_ENV: &str = "HOENN_RELEASE_TRUST_PUBLIC_KEY_HEX";
const ENVELOPE_SCHEMA: u16 = RELEASE_SCHEMA;

#[derive(Debug, thiserror::Error)]
enum ToolError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("invalid release input: {0}")]
    Input(String),
    #[error("release artifact is not a bounded regular file: {0}")]
    Artifact(PathBuf),
    #[error("release artifact is too large: {0}")]
    ArtifactTooLarge(PathBuf),
    #[error("cannot read release input")]
    Read(#[source] io::Error),
    #[error("cannot write release output")]
    Write(#[source] io::Error),
    #[error("canonical release payload failed")]
    CanonicalJcs(String),
    #[error("release envelope is invalid")]
    Envelope,
    #[error("signing key does not match the configured public key")]
    KeyMismatch,
}

#[derive(Debug, Default)]
struct Options {
    command: Option<String>,
    release_id: Option<String>,
    sequence: Option<u64>,
    issued_at: Option<i64>,
    expires_at: Option<i64>,
    key_id: Option<String>,
    public_key_hex: Option<String>,
    seed_env: Option<String>,
    output: Option<PathBuf>,
    envelope: Option<PathBuf>,
    trust_bundle: Option<PathBuf>,
    artifacts: Vec<String>,
    allow_expired: bool,
}

fn usage() -> &'static str {
    "coop-release-tool sign --release-id ID --sequence N --issued-at UNIX --expires-at UNIX \
     --key-id ID --public-key-hex HEX --output PATH --artifact ID=PATH... [--trust-bundle PATH]\n\
     coop-release-tool verify --envelope PATH --key-id ID --public-key-hex HEX\n\
     coop-release-tool public-key [--seed-env ENV]\n\
     coop-release-tool check-key --public-key-hex HEX [--seed-env ENV]\n\
     coop-release-tool sign-game --release-id ID --sequence N --issued-at UNIX --expires-at UNIX \
     --key-id ID --public-key-hex HEX --output PATH --artifact rom=PATH \
     --artifact compatibility-manifest=PATH [--artifact region-catalog=PATH \
     --artifact world-1-rom=PATH --artifact world-1-compatibility=PATH \
     --artifact world-1-player-transfer=PATH ...]\n\
     coop-release-tool verify-game --envelope PATH --key-id ID --public-key-hex HEX"
}

fn parse_options() -> Result<Options, ToolError> {
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| ToolError::Usage(usage().to_owned()))?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        std::process::exit(0);
    }
    if command != "sign"
        && command != "verify"
        && command != "public-key"
        && command != "check-key"
        && command != "sign-game"
        && command != "verify-game"
    {
        return Err(ToolError::Usage(usage().to_owned()));
    }
    let mut options = Options {
        command: Some(command.clone()),
        ..Options::default()
    };
    while let Some(flag) = args.next() {
        let value = |args: &mut std::iter::Skip<std::env::Args>| {
            args.next()
                .ok_or_else(|| ToolError::Usage(format!("missing value for {flag}")))
        };
        match flag.as_str() {
            "--release-id" => options.release_id = Some(value(&mut args)?),
            "--sequence" => {
                options.sequence = Some(value(&mut args)?.parse().map_err(|_| {
                    ToolError::Input("sequence must be an unsigned integer".to_owned())
                })?);
            }
            "--issued-at" => {
                options.issued_at = Some(value(&mut args)?.parse().map_err(|_| {
                    ToolError::Input("issued-at must be a Unix timestamp".to_owned())
                })?);
            }
            "--expires-at" => {
                options.expires_at = Some(value(&mut args)?.parse().map_err(|_| {
                    ToolError::Input("expires-at must be a Unix timestamp".to_owned())
                })?);
            }
            "--key-id" => options.key_id = Some(value(&mut args)?),
            "--public-key-hex" => options.public_key_hex = Some(value(&mut args)?),
            "--seed-env" => options.seed_env = Some(value(&mut args)?),
            "--output" => options.output = Some(PathBuf::from(value(&mut args)?)),
            "--envelope" => options.envelope = Some(PathBuf::from(value(&mut args)?)),
            "--trust-bundle" => options.trust_bundle = Some(PathBuf::from(value(&mut args)?)),
            "--artifact" => options.artifacts.push(value(&mut args)?),
            "--allow-expired" if command == "verify-game" => options.allow_expired = true,
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ => return Err(ToolError::Usage(format!("unknown option: {flag}"))),
        }
    }
    Ok(options)
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, ToolError> {
    value.ok_or_else(|| ToolError::Usage(format!("missing required option {name}")))
}

fn env_or_option(
    option: Option<String>,
    name: &str,
    default_env: &str,
) -> Result<String, ToolError> {
    option
        .or_else(|| env::var(default_env).ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::Usage(format!("missing {name} or {default_env}")))
}

fn decode_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], ToolError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ToolError::Input(format!(
            "{label} must be exactly {N} bytes of hex"
        )));
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(output)
}

const fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn identity(value: &str) -> Result<ArtifactIdentity, ToolError> {
    let parsed: ArtifactIdentity =
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
            .map_err(|_| ToolError::Input(format!("unknown artifact identity: {value}")))?;
    if parsed.wire_name() != value {
        return Err(ToolError::Input(format!(
            "noncanonical artifact identity: {value}"
        )));
    }
    Ok(parsed)
}

fn parse_artifacts(values: &[String]) -> Result<BTreeMap<ArtifactIdentity, PathBuf>, ToolError> {
    let mut artifacts = BTreeMap::new();
    for value in values {
        let (id, path) = value
            .split_once('=')
            .ok_or_else(|| ToolError::Input("--artifact must be ID=PATH".to_owned()))?;
        let id = identity(id)?;
        if path.is_empty() || artifacts.insert(id, PathBuf::from(path)).is_some() {
            return Err(ToolError::Input(format!(
                "duplicate or empty artifact path for {id}"
            )));
        }
    }
    canonical_artifact_order(artifacts.keys().copied())?;
    Ok(artifacts)
}

fn canonical_artifact_order(
    identities: impl IntoIterator<Item = ArtifactIdentity>,
) -> Result<Vec<ArtifactIdentity>, ToolError> {
    let seen = identities.into_iter().collect::<BTreeSet<_>>();
    let worlds = seen
        .iter()
        .filter_map(|identity| match identity {
            ArtifactIdentity::WorldRom(id)
            | ArtifactIdentity::WorldCompatibility(id)
            | ArtifactIdentity::WorldPlayerTransfer(id) => Some(*id),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut canonical = FIXED_ARTIFACT_IDENTITIES.to_vec();
    if !worlds.is_empty() {
        canonical.push(ArtifactIdentity::RegionCatalog);
        for world in worlds {
            canonical.extend(ArtifactIdentity::world_artifacts(world));
        }
    }
    if canonical.iter().copied().collect::<BTreeSet<_>>() != seen {
        return Err(ToolError::Input(
            "release must contain every fixed artifact and complete catalog-bound world groups"
                .to_owned(),
        ));
    }
    Ok(canonical)
}

fn hash_artifact(identity: ArtifactIdentity, path: &Path) -> Result<ArtifactDescriptor, ToolError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ToolError::Artifact(path.to_owned()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ToolError::Artifact(path.to_owned()));
    }
    if metadata.len() == 0 || metadata.len() > coop_launcher::MAX_ARTIFACT_BYTES {
        return Err(ToolError::ArtifactTooLarge(path.to_owned()));
    }
    let mut file = fs::File::open(path).map_err(ToolError::Read)?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(ToolError::Read)?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(count as u64);
        if size > coop_launcher::MAX_ARTIFACT_BYTES {
            return Err(ToolError::ArtifactTooLarge(path.to_owned()));
        }
        digest.update(&buffer[..count]);
    }
    Ok(ArtifactDescriptor {
        id: identity,
        size,
        sha256: hex_digest(digest.finalize().into()),
    })
}

fn hex_digest(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonical_descriptor(descriptor: &ReleaseDescriptor) -> Result<Vec<u8>, ToolError> {
    serde_jcs::to_vec(descriptor).map_err(|error| ToolError::CanonicalJcs(error.to_string()))
}

fn validate_descriptor(descriptor: &ReleaseDescriptor, issued_at: i64) -> Result<(), ToolError> {
    descriptor
        .validate_at(issued_at)
        .map_err(|error| ToolError::Input(error.to_string()))?;
    let ids: Vec<_> = descriptor
        .artifacts
        .iter()
        .map(|artifact| artifact.id)
        .collect();
    if ids != canonical_artifact_order(ids.iter().copied())? {
        return Err(ToolError::Input(
            "artifacts are not in canonical order".to_owned(),
        ));
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(ToolError::Write)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(ToolError::Write)?;
    use std::io::Write;
    file.write_all(bytes).map_err(ToolError::Write)
}

#[derive(Debug, Serialize)]
struct TrustBundle<'a> {
    schema: u16,
    algorithm: &'static str,
    key_id: &'a str,
    public_key_hex: String,
}

fn public_key_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GameArtifact {
    id: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GameDescriptor {
    schema: u16,
    release_id: String,
    sequence: u64,
    issued_at: i64,
    expires_at: i64,
    platform: String,
    artifacts: Vec<GameArtifact>,
}

fn validate_game(
    descriptor: &GameDescriptor,
    now: i64,
    require_fresh: bool,
) -> Result<(), ToolError> {
    let valid_id = !descriptor.release_id.is_empty()
        && descriptor.release_id.len() <= 128
        && descriptor.release_id != "."
        && descriptor.release_id != ".."
        && descriptor
            .release_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    if descriptor.schema != 1
        || descriptor.platform != "game"
        || !valid_id
        || descriptor.sequence == 0
        || descriptor.issued_at > now.saturating_add(300)
        || (require_fresh && descriptor.expires_at <= now)
        || descriptor.expires_at <= descriptor.issued_at
        || descriptor.expires_at.saturating_sub(descriptor.issued_at) > 90 * 24 * 60 * 60
    {
        return Err(ToolError::Input("invalid game descriptor".to_owned()));
    }
    let order = game_artifact_order(descriptor.artifacts.iter().map(|a| a.id.as_str()))?;
    for (artifact, expected) in descriptor.artifacts.iter().zip(&order) {
        if artifact.id != *expected
            || artifact.size == 0
            || artifact.size > game_artifact_maximum(&artifact.id)
            || artifact.sha256.len() != 64
            || !artifact
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ToolError::Input("invalid game artifact".to_owned()));
        }
    }
    if order.len() > 2 {
        let rom = &descriptor.artifacts[0];
        let manifest = &descriptor.artifacts[1];
        let world_rom = &descriptor.artifacts[3];
        let world_manifest = &descriptor.artifacts[4];
        if (rom.size, &rom.sha256) != (world_rom.size, &world_rom.sha256)
            || (manifest.size, &manifest.sha256) != (world_manifest.size, &world_manifest.sha256)
        {
            return Err(ToolError::Input(
                "world 1 differs from base game".to_owned(),
            ));
        }
    }
    Ok(())
}

fn game_artifact_order<'a>(
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<String>, ToolError> {
    let ids = ids.into_iter().collect::<Vec<_>>();
    let mut worlds = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for id in &ids {
        if !seen.insert(*id) {
            return Err(ToolError::Input("duplicate game artifact".to_owned()));
        }
        if *id == "rom" || *id == "compatibility-manifest" || *id == "region-catalog" {
            continue;
        }
        let rest = id
            .strip_prefix("world-")
            .ok_or_else(|| ToolError::Input(format!("unknown game artifact: {id}")))?;
        let (number, kind) = rest
            .split_once('-')
            .ok_or_else(|| ToolError::Input(format!("invalid world artifact: {id}")))?;
        let world: u16 = number
            .parse()
            .map_err(|_| ToolError::Input(format!("invalid world id: {id}")))?;
        if world == 0
            || number != world.to_string()
            || !matches!(kind, "rom" | "compatibility" | "player-transfer")
        {
            return Err(ToolError::Input(format!(
                "noncanonical world artifact: {id}"
            )));
        }
        worlds.insert(world);
    }
    if worlds.len() > 16 {
        return Err(ToolError::Input("too many game worlds".to_owned()));
    }
    let mut expected = vec!["rom".to_owned(), "compatibility-manifest".to_owned()];
    if !worlds.is_empty() {
        if !worlds.contains(&1) {
            return Err(ToolError::Input("world 1 is required".to_owned()));
        }
        expected.push("region-catalog".to_owned());
        for world in worlds {
            for kind in ["rom", "compatibility", "player-transfer"] {
                expected.push(format!("world-{world}-{kind}"));
            }
        }
    }
    if seen != expected.iter().map(String::as_str).collect() {
        return Err(ToolError::Input("incomplete game artifact set".to_owned()));
    }
    Ok(expected)
}

fn game_artifact_paths(values: &[String]) -> Result<BTreeMap<String, PathBuf>, ToolError> {
    let mut paths = BTreeMap::new();
    for value in values {
        let (id, path) = value
            .split_once('=')
            .ok_or_else(|| ToolError::Input("--artifact must be ID=PATH".to_owned()))?;
        if path.is_empty() {
            return Err(ToolError::Input("empty artifact path".to_owned()));
        }
        if paths.insert(id.to_owned(), PathBuf::from(path)).is_some() {
            return Err(ToolError::Input("duplicate game artifact".to_owned()));
        }
    }
    game_artifact_order(paths.keys().map(String::as_str))?;
    Ok(paths)
}

fn game_artifact_maximum(id: &str) -> u64 {
    if id == "rom" || id.ends_with("-rom") {
        64 * 1024 * 1024
    } else if id == "region-catalog" {
        256 * 1024
    } else {
        1024 * 1024
    }
}

fn hash_game_artifact(id: &str, path: &Path) -> Result<GameArtifact, ToolError> {
    let maximum = game_artifact_maximum(id);
    let metadata = fs::symlink_metadata(path).map_err(|_| ToolError::Artifact(path.to_owned()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ToolError::Artifact(path.to_owned()));
    }
    if metadata.len() == 0 || metadata.len() > maximum {
        return Err(ToolError::ArtifactTooLarge(path.to_owned()));
    }
    let mut file = fs::File::open(path).map_err(ToolError::Read)?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(ToolError::Read)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > maximum {
            return Err(ToolError::ArtifactTooLarge(path.to_owned()));
        }
        digest.update(&buffer[..count]);
    }
    Ok(GameArtifact {
        id: id.to_owned(),
        size,
        sha256: hex_digest(digest.finalize().into()),
    })
}

fn game_key(options: &Options) -> Result<SigningKey, ToolError> {
    let public_hex = env_or_option(
        options.public_key_hex.clone(),
        "--public-key-hex",
        DEFAULT_PUBLIC_KEY_ENV,
    )?;
    let expected = decode_hex::<32>(&public_hex, "public key")?;
    let seed_env = options
        .seed_env
        .clone()
        .unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env)
            .map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
    );
    let mut seed = decode_hex::<32>(&seed_text, "private seed")?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    if signing_key.verifying_key().to_bytes() != expected {
        return Err(ToolError::KeyMismatch);
    }
    Ok(signing_key)
}

fn sign_game(options: Options) -> Result<(), ToolError> {
    let key = game_key(&options)?;
    sign_game_with_key(options, &key)
}

fn sign_game_with_key(options: Options, key: &SigningKey) -> Result<(), ToolError> {
    let paths = game_artifact_paths(&options.artifacts)?;
    let order = game_artifact_order(paths.keys().map(String::as_str))?;
    let artifacts = order
        .iter()
        .map(|id| hash_game_artifact(id, &paths[id]))
        .collect::<Result<Vec<_>, _>>()?;
    let descriptor = GameDescriptor {
        schema: 1,
        release_id: required(options.release_id.clone(), "--release-id")?,
        sequence: required(options.sequence, "--sequence")?,
        issued_at: required(options.issued_at, "--issued-at")?,
        expires_at: required(options.expires_at, "--expires-at")?,
        platform: "game".to_owned(),
        artifacts,
    };
    validate_game(&descriptor, descriptor.issued_at, true)?;
    let key_id = env_or_option(options.key_id.clone(), "--key-id", DEFAULT_KEY_ID_ENV)?;
    let payload =
        serde_jcs::to_vec(&descriptor).map_err(|e| ToolError::CanonicalJcs(e.to_string()))?;
    let envelope = SignedReleaseEnvelope::sign_payload(&payload, key_id, key)
        .map_err(|e| ToolError::Input(e.to_string()))?;
    write_new(&required(options.output, "--output")?, &envelope)?;
    println!("signed game envelope for {}", descriptor.release_id);
    Ok(())
}

fn verify_game(options: Options) -> Result<(), ToolError> {
    let path = required(options.envelope, "--envelope")?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| ToolError::Artifact(path.clone()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_ENVELOPE_BYTES as u64
    {
        return Err(ToolError::Envelope);
    }
    let bytes = fs::read(path).map_err(ToolError::Read)?;
    let envelope: SignedReleaseEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| ToolError::Envelope)?;
    if serde_json::to_vec(&envelope).map_err(|_| ToolError::Envelope)? != bytes
        || envelope.schema != 1
        || envelope.key_id != env_or_option(options.key_id, "--key-id", DEFAULT_KEY_ID_ENV)?
    {
        return Err(ToolError::Envelope);
    }
    let public_hex = env_or_option(
        options.public_key_hex,
        "--public-key-hex",
        DEFAULT_PUBLIC_KEY_ENV,
    )?;
    let key = VerifyingKey::from_bytes(&decode_hex::<32>(&public_hex, "public key")?)
        .map_err(|_| ToolError::Envelope)?;
    let payload = BASE64
        .decode(&envelope.payload)
        .map_err(|_| ToolError::Envelope)?;
    let signature = BASE64
        .decode(&envelope.signature)
        .map_err(|_| ToolError::Envelope)?;
    if payload.is_empty()
        || payload.len() > MAX_PAYLOAD_BYTES
        || signature.len() != 64
        || BASE64.encode(&payload) != envelope.payload
        || BASE64.encode(&signature) != envelope.signature
    {
        return Err(ToolError::Envelope);
    }
    key.verify(
        &payload,
        &Signature::from_slice(&signature).map_err(|_| ToolError::Envelope)?,
    )
    .map_err(|_| ToolError::Envelope)?;
    let descriptor: GameDescriptor =
        serde_json::from_slice(&payload).map_err(|_| ToolError::Envelope)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ToolError::Envelope)?
        .as_secs() as i64;
    validate_game(&descriptor, now, !options.allow_expired)?;
    if serde_jcs::to_vec(&descriptor).map_err(|_| ToolError::Envelope)? != payload {
        return Err(ToolError::Envelope);
    }
    println!("verified game envelope for {}", descriptor.release_id);
    Ok(())
}

fn sign(options: Options) -> Result<(), ToolError> {
    let release_id = required(options.release_id, "--release-id")?;
    let sequence = required(options.sequence, "--sequence")?;
    if sequence == 0 {
        return Err(ToolError::Input("sequence must be positive".to_owned()));
    }
    let issued_at = required(options.issued_at, "--issued-at")?;
    let expires_at = required(options.expires_at, "--expires-at")?;
    let key_id = env_or_option(options.key_id, "--key-id", DEFAULT_KEY_ID_ENV)?;
    if key_id.len() > MAX_KEY_ID_BYTES || key_id.is_empty() {
        return Err(ToolError::Input("key id is empty or too long".to_owned()));
    }
    let public_hex = env_or_option(
        options.public_key_hex,
        "--public-key-hex",
        DEFAULT_PUBLIC_KEY_ENV,
    )?;
    let expected_public = decode_hex::<32>(&public_hex, "public key")?;
    let seed_env = options
        .seed_env
        .unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env)
            .map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
    );
    let mut seed = decode_hex::<32>(&seed_text, "private seed")?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    if signing_key.verifying_key().to_bytes() != expected_public {
        return Err(ToolError::KeyMismatch);
    }
    let trust_bundle = options.trust_bundle;
    if let Some(path) = &trust_bundle {
        let bundle = TrustBundle {
            schema: ENVELOPE_SCHEMA,
            algorithm: "ed25519",
            key_id: &key_id,
            public_key_hex: public_key_hex(&expected_public),
        };
        let bytes = serde_jcs::to_vec(&bundle)
            .map_err(|error| ToolError::CanonicalJcs(error.to_string()))?;
        write_new(path, &bytes)?;
    }
    let artifact_paths = parse_artifacts(&options.artifacts)?;
    let artifact_order = canonical_artifact_order(artifact_paths.keys().copied())?;
    let mut descriptors = Vec::with_capacity(artifact_order.len());
    for artifact in artifact_order {
        descriptors.push(hash_artifact(artifact, &artifact_paths[&artifact])?);
    }
    let descriptor = ReleaseDescriptor {
        schema: RELEASE_SCHEMA,
        release_id,
        sequence,
        issued_at,
        expires_at,
        platform: WINDOWS_PLATFORM.to_owned(),
        artifacts: descriptors,
    };
    validate_descriptor(&descriptor, issued_at)?;
    let payload = canonical_descriptor(&descriptor)?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(ToolError::Input(
            "canonical payload exceeds launcher bound".to_owned(),
        ));
    }
    let envelope = SignedReleaseEnvelope::sign_payload(&payload, key_id.clone(), &signing_key)
        .map_err(|error| ToolError::Input(error.to_string()))?;
    let output = required(options.output, "--output")?;
    write_new(&output, &envelope)?;
    println!("signed release envelope for {}", descriptor.release_id);
    Ok(())
}

fn verify(options: Options) -> Result<(), ToolError> {
    let envelope_path = required(options.envelope, "--envelope")?;
    let metadata = fs::symlink_metadata(&envelope_path)
        .map_err(|_| ToolError::Artifact(envelope_path.clone()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_ENVELOPE_BYTES as u64
    {
        return Err(ToolError::Envelope);
    }
    let envelope_bytes = fs::read(&envelope_path).map_err(ToolError::Read)?;
    let key_id = env_or_option(options.key_id, "--key-id", DEFAULT_KEY_ID_ENV)?;
    let public_hex = env_or_option(
        options.public_key_hex,
        "--public-key-hex",
        DEFAULT_PUBLIC_KEY_ENV,
    )?;
    let public_key = decode_hex::<32>(&public_hex, "public key")?;
    let trusted = TrustedReleaseKey::new(key_id, public_key)
        .map_err(|error| ToolError::Input(error.to_string()))?;
    let parsed: SignedReleaseEnvelope =
        serde_json::from_slice(&envelope_bytes).map_err(|_| ToolError::Envelope)?;
    let canonical_envelope = serde_json::to_vec(&parsed).map_err(|_| ToolError::Envelope)?;
    if canonical_envelope != envelope_bytes {
        return Err(ToolError::Envelope);
    }
    let verified = SignedReleaseEnvelope::verify_json(&envelope_bytes, &trusted)
        .map_err(|_| ToolError::Envelope)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ToolError::Input("system clock is before Unix epoch".to_owned()))?
        .as_secs()
        .try_into()
        .map_err(|_| ToolError::Input("current Unix time is out of range".to_owned()))?;
    validate_descriptor(verified.descriptor(), now)?;
    let canonical = canonical_descriptor(verified.descriptor())?;
    if canonical != verified.payload() {
        return Err(ToolError::Envelope);
    }
    println!("verified release envelope for {}", verified.release_id());
    Ok(())
}

fn public_key(options: Options) -> Result<(), ToolError> {
    let seed_env = options
        .seed_env
        .unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env)
            .map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
    );
    let mut seed = decode_hex::<32>(&seed_text, "private seed")?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    println!(
        "{}",
        public_key_hex(&signing_key.verifying_key().to_bytes())
    );
    Ok(())
}

/// Check private-seed/public-key correspondence without ever printing either
/// value. This is intentionally a separate, quiet gate for CI jobs: the
/// protected seed remains scoped to the caller's environment and the command
/// emits no key material on success or failure.
fn check_key(options: Options) -> Result<(), ToolError> {
    let public_hex = env_or_option(
        options.public_key_hex,
        "--public-key-hex",
        DEFAULT_PUBLIC_KEY_ENV,
    )?;
    let expected_public = decode_hex::<32>(&public_hex, "public key")?;
    let seed_env = options
        .seed_env
        .unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env)
            .map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
    );
    let mut seed = decode_hex::<32>(&seed_text, "private seed")?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    if signing_key.verifying_key().to_bytes() != expected_public {
        return Err(ToolError::KeyMismatch);
    }
    Ok(())
}

fn run() -> Result<(), ToolError> {
    let options = parse_options()?;
    match options.command.as_deref() {
        Some("sign") => sign(options),
        Some("verify") => verify(options),
        Some("public-key") => public_key(options),
        Some("check-key") => check_key(options),
        Some("sign-game") => sign_game(options),
        Some("verify-game") => verify_game(options),
        _ => Err(ToolError::Usage(usage().to_owned())),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(id: &str, byte: u8) -> GameArtifact {
        GameArtifact {
            id: id.to_owned(),
            size: 1,
            sha256: hex_digest([byte; 32]),
        }
    }

    fn game_descriptor() -> GameDescriptor {
        GameDescriptor {
            schema: 1,
            release_id: "test-region".to_owned(),
            sequence: 1,
            issued_at: 1_700_000_000,
            expires_at: 1_700_086_400,
            platform: "game".to_owned(),
            artifacts: vec![
                artifact("rom", 1),
                artifact("compatibility-manifest", 2),
                artifact("region-catalog", 3),
                artifact("world-1-rom", 1),
                artifact("world-1-compatibility", 2),
                artifact("world-1-player-transfer", 4),
                artifact("world-2-rom", 5),
                artifact("world-2-compatibility", 6),
                artifact("world-2-player-transfer", 7),
            ],
        }
    }

    #[test]
    fn game_envelope_accepts_legacy_and_canonical_multiworld() {
        let descriptor = game_descriptor();
        validate_game(&descriptor, descriptor.issued_at, true).unwrap();
        let mut legacy = game_descriptor();
        legacy.artifacts.truncate(2);
        validate_game(&legacy, legacy.issued_at, true).unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let dir = tempfile::tempdir().unwrap();
        for (index, descriptor) in [legacy, descriptor].into_iter().enumerate() {
            let payload = serde_jcs::to_vec(&descriptor).unwrap();
            let signed =
                SignedReleaseEnvelope::sign_payload(&payload, "test-key".to_owned(), &key).unwrap();
            let path = dir.path().join(format!("{index}.json"));
            write_new(&path, &signed).unwrap();
            verify_game(Options {
                envelope: Some(path),
                key_id: Some("test-key".to_owned()),
                public_key_hex: Some(hex_digest(key.verifying_key().to_bytes())),
                allow_expired: true,
                ..Options::default()
            })
            .unwrap();
        }
    }

    #[test]
    fn sign_game_hashes_and_verifies_real_multiworld_files() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let dir = tempfile::tempdir().unwrap();
        let mut values = Vec::new();
        for (index, id) in [
            "rom",
            "compatibility-manifest",
            "region-catalog",
            "world-1-player-transfer",
            "world-2-rom",
            "world-2-compatibility",
            "world-2-player-transfer",
        ]
        .into_iter()
        .enumerate()
        {
            let path = dir.path().join(format!("artifact-{index}"));
            fs::write(&path, [index as u8 + 1]).unwrap();
            values.push(format!("{}={}", id, path.display()));
            if id == "rom" {
                values.push(format!("world-1-rom={}", path.display()));
            } else if id == "compatibility-manifest" {
                values.push(format!("world-1-compatibility={}", path.display()));
            }
        }
        values.reverse(); // Signing must emit canonical order regardless of CLI order.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let envelope_path = dir.path().join("signed.json");
        sign_game_with_key(
            Options {
                release_id: Some("local-test".to_owned()),
                sequence: Some(1),
                issued_at: Some(now),
                expires_at: Some(now + 3600),
                key_id: Some("local-key".to_owned()),
                public_key_hex: Some(hex_digest(key.verifying_key().to_bytes())),
                output: Some(envelope_path.clone()),
                artifacts: values,
                ..Options::default()
            },
            &key,
        )
        .unwrap();
        verify_game(Options {
            envelope: Some(envelope_path.clone()),
            key_id: Some("local-key".to_owned()),
            public_key_hex: Some(hex_digest(key.verifying_key().to_bytes())),
            ..Options::default()
        })
        .unwrap();
        let signed: SignedReleaseEnvelope =
            serde_json::from_slice(&fs::read(envelope_path).unwrap()).unwrap();
        let payload = BASE64.decode(signed.payload).unwrap();
        let descriptor: GameDescriptor = serde_json::from_slice(&payload).unwrap();
        let ids = descriptor
            .artifacts
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            game_artifact_order(ids.iter().copied())
                .unwrap()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn game_artifact_paths_reject_malformed_sets() {
        let valid = game_descriptor()
            .artifacts
            .into_iter()
            .map(|artifact| format!("{}=unused", artifact.id))
            .collect::<Vec<_>>();
        let paths = game_artifact_paths(&valid).unwrap();
        let order = game_artifact_order(paths.keys().map(String::as_str)).unwrap();
        assert_eq!(order[3], "world-1-rom");
        assert_eq!(order.last().unwrap(), "world-2-player-transfer");
        for bad in [
            "world-0-rom",
            "world-01-rom",
            "world-65536-rom",
            "world-1-rom-extra",
            "world-+1-rom",
            "world-2-unknown",
            "other",
        ] {
            let mut values = valid.clone();
            values.push(format!("{bad}=unused"));
            assert!(game_artifact_paths(&values).is_err(), "{bad}");
        }
        for removed in [
            "rom",
            "region-catalog",
            "world-1-player-transfer",
            "world-2-rom",
        ] {
            let values = valid
                .iter()
                .filter(|v| !v.starts_with(&format!("{removed}=")))
                .cloned()
                .collect::<Vec<_>>();
            assert!(game_artifact_paths(&values).is_err(), "{removed}");
        }
        let mut duplicate = valid.clone();
        duplicate.push(valid[0].clone());
        assert!(game_artifact_paths(&duplicate).is_err());
        let mut too_many = valid[..6].to_vec();
        for world in 2..=17 {
            for kind in ["rom", "compatibility", "player-transfer"] {
                too_many.push(format!("world-{world}-{kind}=unused"));
            }
        }
        assert!(game_artifact_paths(&too_many).is_err());
    }

    #[test]
    fn game_descriptor_rejects_reordered_mismatch_and_oversize() {
        let base = game_descriptor();
        let now = base.issued_at;
        let mut reordered = game_descriptor();
        reordered.artifacts.swap(3, 4);
        assert!(validate_game(&reordered, now, true).is_err());
        let mut mismatch = game_descriptor();
        mismatch.artifacts[3].sha256 = hex_digest([8; 32]);
        assert!(validate_game(&mismatch, now, true).is_err());
        for (index, limit) in [(0, 64 * 1024 * 1024), (2, 256 * 1024), (8, 1024 * 1024)] {
            let mut oversized = game_descriptor();
            oversized.artifacts[index].size = limit + 1;
            assert!(validate_game(&oversized, now, true).is_err());
        }
    }

    #[test]
    fn accepts_complete_multiworld_artifacts_in_canonical_order() {
        let mut values = FIXED_ARTIFACT_IDENTITIES
            .iter()
            .map(|id| format!("{}=unused", id.wire_name()))
            .collect::<Vec<_>>();
        values.push("region-catalog=unused".to_owned());
        for id in [2, 7] {
            let world = coop_launcher::session::RomWorldId::new(id).unwrap();
            values.extend(
                ArtifactIdentity::world_artifacts(world)
                    .into_iter()
                    .map(|artifact| format!("{}=unused", artifact.wire_name())),
            );
        }
        let parsed = parse_artifacts(&values).unwrap();
        let ordered = canonical_artifact_order(parsed.keys().copied()).unwrap();
        assert_eq!(ordered.len(), FIXED_ARTIFACT_IDENTITIES.len() + 7);
        assert_eq!(
            ordered[FIXED_ARTIFACT_IDENTITIES.len()],
            ArtifactIdentity::RegionCatalog
        );
        assert_eq!(
            ordered.last().copied().unwrap().wire_name(),
            "world-7-player-transfer"
        );
    }

    #[test]
    fn rejects_incomplete_or_noncanonical_world_artifacts() {
        let mut values = FIXED_ARTIFACT_IDENTITIES
            .iter()
            .map(|id| format!("{}=unused", id.wire_name()))
            .collect::<Vec<_>>();
        values.push("region-catalog=unused".to_owned());
        values.push("world-2-rom=unused".to_owned());
        assert!(parse_artifacts(&values).is_err());
        for bad in [
            "world-02-rom",
            "world-0-rom",
            "world-2-rom-extra",
            "manifest",
        ] {
            assert!(identity(bad).is_err(), "{bad}");
        }
    }
}

//! Build and verify the private-pilot Windows runtime envelope.
//!
//! The launcher owns the wire types and fixed artifact mapping.  This binary
//! only turns trusted build outputs into those types, hashes them, and signs
//! the canonical payload.  The private seed is read from a protected runtime
//! environment variable and is never accepted as a command-line argument,
//! printed, or written to disk.

use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use coop_launcher::{
    ArtifactIdentity, ReleaseDescriptor, SignedReleaseEnvelope, TrustedReleaseKey,
    MAX_ENVELOPE_BYTES,
    update::{ArtifactDescriptor, FIXED_ARTIFACT_IDENTITIES, MAX_PAYLOAD_BYTES, RELEASE_SCHEMA,
        WINDOWS_PLATFORM},
};
use ed25519_dalek::SigningKey;
use serde::Serialize;
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
}

fn usage() -> &'static str {
    "coop-release-tool sign --release-id ID --sequence N --issued-at UNIX --expires-at UNIX \
     --key-id ID --public-key-hex HEX --output PATH --artifact ID=PATH... [--trust-bundle PATH]\n\
     coop-release-tool verify --envelope PATH --key-id ID --public-key-hex HEX\n\
     coop-release-tool public-key [--seed-env ENV]\n\
     coop-release-tool check-key --public-key-hex HEX [--seed-env ENV]"
}

fn parse_options() -> Result<Options, ToolError> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| ToolError::Usage(usage().to_owned()))?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        std::process::exit(0);
    }
    if command != "sign" && command != "verify" && command != "public-key" && command != "check-key" {
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

fn env_or_option(option: Option<String>, name: &str, default_env: &str) -> Result<String, ToolError> {
    option
        .or_else(|| env::var(default_env).ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::Usage(format!("missing {name} or {default_env}")))
}

fn decode_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], ToolError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ToolError::Input(format!("{label} must be exactly {N} bytes of hex")));
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
    FIXED_ARTIFACT_IDENTITIES
        .iter()
        .copied()
        .find(|candidate| candidate.as_str() == value)
        .ok_or_else(|| ToolError::Input(format!("unknown artifact identity: {value}")))
}

fn parse_artifacts(values: &[String]) -> Result<BTreeMap<ArtifactIdentity, PathBuf>, ToolError> {
    let mut artifacts = BTreeMap::new();
    for value in values {
        let (id, path) = value
            .split_once('=')
            .ok_or_else(|| ToolError::Input("--artifact must be ID=PATH".to_owned()))?;
        let id = identity(id)?;
        if path.is_empty() || artifacts.insert(id, PathBuf::from(path)).is_some() {
            return Err(ToolError::Input(format!("duplicate or empty artifact path for {id}")));
        }
    }
    if artifacts.len() != FIXED_ARTIFACT_IDENTITIES.len() {
        return Err(ToolError::Input(format!(
            "exactly {} fixed artifacts are required",
            FIXED_ARTIFACT_IDENTITIES.len()
        )));
    }
    Ok(artifacts)
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
    let ids: Vec<_> = descriptor.artifacts.iter().map(|artifact| artifact.id).collect();
    if ids != FIXED_ARTIFACT_IDENTITIES {
        return Err(ToolError::Input("artifacts are not in canonical fixed order".to_owned()));
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
    let seed_env = options.seed_env.unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env).map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
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
    let mut descriptors = Vec::with_capacity(FIXED_ARTIFACT_IDENTITIES.len());
    for artifact in FIXED_ARTIFACT_IDENTITIES {
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
        return Err(ToolError::Input("canonical payload exceeds launcher bound".to_owned()));
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
    let metadata = fs::symlink_metadata(&envelope_path).map_err(|_| ToolError::Artifact(envelope_path.clone()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_ENVELOPE_BYTES as u64 {
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
    let trusted = TrustedReleaseKey::new(key_id, public_key).map_err(|error| ToolError::Input(error.to_string()))?;
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
    let seed_env = options.seed_env.unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env).map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
    );
    let mut seed = decode_hex::<32>(&seed_text, "private seed")?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    println!("{}", public_key_hex(&signing_key.verifying_key().to_bytes()));
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
    let seed_env = options.seed_env.unwrap_or_else(|| DEFAULT_SEED_ENV.to_owned());
    let seed_text = Zeroizing::new(
        env::var(&seed_env).map_err(|_| ToolError::Usage(format!("missing signing seed env {seed_env}")))?,
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
        _ => Err(ToolError::Usage(usage().to_owned())),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

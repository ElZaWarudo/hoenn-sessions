//! Small non-Tauri launcher CLI. Stock mGBA scripting remains a manual step.

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use coop_cloud::{AcquireLeaseRequest, AcquireWorldLeaseResponse, TrustedManifestKey};
use coop_launcher::{
    CommandSpec, ReqwestCloudApi, SupervisedChildren,
    auth::AuthSession,
    compat::{BuildCompatibility, SelectedRomWorld, TrustedRomCatalog},
    epoch::EpochStore,
    keychain::{OsKeychain, RefreshTokenStore},
    process::{
        ProcessError, cleanup_owned_staged_rom, staged_rom_marker_contents, staged_rom_marker_path,
    },
    session::{RomWorldId, SessionConfig, SessionLifecycle},
};
use thiserror::Error;

#[derive(Debug, Error)]
enum CliError {
    #[error(
        "usage: coop-launcher --api-base <url> --username <name> --release-catalog <path> [--world-id <expected id>] --mgba <path> --manifest-key <path> --manifest-key-id <id> [--password-stdin]"
    )]
    Usage,
    #[error("missing or invalid CLI value")]
    Value,
    #[error("the server's active ROM world differs from --world-id")]
    WorldMismatch,
    #[cfg(not(windows))]
    #[error("the secure launcher is supported only on Windows")]
    UnsupportedPlatform,
    #[error("launcher operation failed")]
    Runtime,
}

const MAX_MANIFEST_KEY_BYTES: usize = 64;
const MAX_ROM_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
struct Options {
    api_base: String,
    username: String,
    release_catalog: PathBuf,
    /// Optional equality check; the server's acquire response selects the
    /// world.
    world_id: Option<u16>,
    mgba: PathBuf,
    manifest_key: PathBuf,
    manifest_key_id: String,
    password_stdin: bool,
    workspace: PathBuf,
    epoch: PathBuf,
    bridge: PathBuf,
    sidecar: PathBuf,
}

fn parse_options() -> Result<Options, CliError> {
    let private_root = env::temp_dir().join("pokecrossroads-coop-launcher");
    let mut api_base = None;
    let mut username = None;
    let mut release_catalog = None;
    let mut world_id = None;
    let mut mgba = None;
    let mut manifest_key = None;
    let mut manifest_key_id = None;
    let mut password_stdin = false;
    let mut workspace = private_root.join("sessions");
    let mut epoch = private_root.join("epoch.json");
    let mut bridge = PathBuf::from("bridge");
    let mut sidecar = PathBuf::from("coop-sidecar");
    let mut args = env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--password-stdin" {
            password_stdin = true;
            continue;
        }
        let value = args.next().ok_or(CliError::Usage)?;
        let value = value.into_string().map_err(|_| CliError::Value)?;
        match flag.to_str() {
            Some("--api-base") => api_base = Some(value),
            Some("--username") => username = Some(value),
            Some("--release-catalog") => release_catalog = Some(PathBuf::from(value)),
            Some("--world-id") => {
                world_id = Some(value.parse::<u16>().map_err(|_| CliError::Value)?)
            }
            Some("--mgba") => mgba = Some(PathBuf::from(value)),
            Some("--manifest-key") => manifest_key = Some(PathBuf::from(value)),
            Some("--manifest-key-id") => manifest_key_id = Some(value),
            Some("--workspace") => workspace = PathBuf::from(value),
            Some("--epoch") => epoch = PathBuf::from(value),
            Some("--bridge") => bridge = PathBuf::from(value),
            Some("--sidecar") => sidecar = PathBuf::from(value),
            _ => return Err(CliError::Usage),
        }
    }
    Ok(Options {
        api_base: api_base.ok_or(CliError::Usage)?,
        username: username.ok_or(CliError::Usage)?,
        release_catalog: release_catalog.ok_or(CliError::Usage)?,
        world_id,
        mgba: mgba.ok_or(CliError::Usage)?,
        manifest_key: manifest_key.ok_or(CliError::Usage)?,
        manifest_key_id: manifest_key_id.ok_or(CliError::Usage)?,
        password_stdin,
        workspace,
        epoch,
        bridge,
        sidecar,
    })
}

/// Loads the digest-pinned release catalog. The world is not chosen here:
/// only the server's world-aware acquire response selects it.
fn load_release_catalog(options: &Options) -> Result<TrustedRomCatalog, CliError> {
    // Release packaging must compile this digest into the launcher.
    // A caller-supplied digest would authenticate nothing.
    let trusted_sha256 = option_env!("COOP_RELEASE_CATALOG_SHA256").ok_or(CliError::Runtime)?;
    TrustedRomCatalog::load(&options.release_catalog, trusted_sha256).map_err(|_| CliError::Runtime)
}

fn expected_world(options: &Options) -> Result<Option<RomWorldId>, CliError> {
    options
        .world_id
        .map(|id| RomWorldId::new(id).map_err(|_| CliError::Value))
        .transpose()
}

/// Maps the server's active world to the pinned local artifacts. `--world-id`
/// is only an equality check against the server's choice.
fn select_acquired_world(
    catalog: &TrustedRomCatalog,
    active_world_id: RomWorldId,
    expected: Option<RomWorldId>,
) -> Result<SelectedRomWorld, CliError> {
    check_expected_world(active_world_id, expected)?;
    catalog
        .world(active_world_id)
        .cloned()
        .map_err(|_| CliError::Runtime)
}

fn check_expected_world(
    active_world_id: RomWorldId,
    expected: Option<RomWorldId>,
) -> Result<(), CliError> {
    if expected.is_some_and(|expected| expected != active_world_id) {
        return Err(CliError::WorldMismatch);
    }
    Ok(())
}

/// Stages the selected world's ROM and binds the emulator to it. Any
/// failure leaves no owned staged ROM behind.
fn prepare_selected_world(
    selected: &SelectedRomWorld,
    private_root: &Path,
    mgba: &Path,
) -> Result<(CommandSpec, BuildCompatibility), CliError> {
    let (verified_rom, verified_rom_marker) = stage_verified_rom(&selected.rom_path, private_root)?;
    let prepared = (|| {
        let mgba_spec = CommandSpec::mgba_owned_staged(mgba, &verified_rom, &verified_rom_marker)
            .map_err(|_| CliError::Runtime)?;
        let compatibility =
            BuildCompatibility::validate(&selected.bridge_path, &verified_rom, mgba)
                .map_err(|_| CliError::Runtime)?;
        selected
            .check_compatibility(&compatibility)
            .map_err(|_| CliError::Runtime)?;
        Ok((mgba_spec, compatibility))
    })();
    if prepared.is_err() {
        let _ = cleanup_owned_staged_rom(&verified_rom, &verified_rom_marker);
    }
    prepared
}

/// Releases a world lease whose ROM could not be selected, then fails.
async fn release_unselected_world(
    api: &ReqwestCloudApi,
    auth: &mut AuthSession,
    response: AcquireWorldLeaseResponse,
    keychain: &Arc<dyn RefreshTokenStore>,
    error: CliError,
) -> CliError {
    let _ = SessionLifecycle::release_preacquired_world_lease(api, auth, response, keychain).await;
    error
}

fn repository_root() -> Result<PathBuf, CliError> {
    // This binary is shipped with the checked-in bridge tree.  Deriving the
    // root from the compiled manifest directory avoids trusting a caller's
    // current directory or a server-provided path.
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = crate_root.ancestors().nth(3).ok_or(CliError::Value)?;
    fs::canonicalize(root).map_err(|_| CliError::Value)
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, CliError> {
    reject_symlink_components(path)?;
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir().map_err(|_| CliError::Value)?.join(path)
    };
    let mut candidate = absolute.clone();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&candidate) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = candidate.file_name().ok_or(CliError::Value)?.to_owned();
                missing.push(name);
                candidate.pop();
                if candidate.as_os_str().is_empty() {
                    return Err(CliError::Value);
                }
            }
            Err(_) => return Err(CliError::Value),
        }
    }
    let mut canonical = fs::canonicalize(candidate).map_err(|_| CliError::Value)?;
    for component in missing.iter().rev() {
        canonical.push(component);
    }
    reject_symlink_components(&canonical)?;
    Ok(canonical)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn private_temp_root() -> Result<PathBuf, CliError> {
    let root = env::temp_dir().join("pokecrossroads-coop-launcher");
    reject_symlink_components(&root)?;
    canonicalize_with_missing_tail(&root)
}

fn reject_symlink_components(path: &Path) -> Result<(), CliError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir().map_err(|_| CliError::Value)?.join(path)
    };
    let mut component_path = PathBuf::new();
    let components: Vec<_> = absolute.components().collect();
    for (index, component) in components.iter().enumerate() {
        component_path.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&component_path)
            && (metadata.file_type().is_symlink()
                || (index + 1 < components.len() && !metadata.is_dir()))
        {
            return Err(CliError::Value);
        }
    }
    Ok(())
}

fn validate_workspace_parent(path: &Path, repository: &Path) -> Result<PathBuf, CliError> {
    let canonical = canonicalize_with_missing_tail(path)?;
    let private_root = private_temp_root()?;
    // Keep all launcher material below one owner-controlled temporary root.
    // SessionWorkspace chmods it to 0700 on Unix and tempfile supplies the
    // equivalent private directory boundary on Windows.
    if paths_overlap(&canonical, repository) || !canonical.starts_with(&private_root) {
        return Err(CliError::Value);
    }
    Ok(canonical)
}

fn validate_epoch_path(path: &Path, repository: &Path) -> Result<PathBuf, CliError> {
    reject_symlink_components(path)?;
    let canonical = canonicalize_with_missing_tail(path)?;
    let private_root = private_temp_root()?;
    if paths_overlap(&canonical, repository) || !canonical.starts_with(&private_root) {
        return Err(CliError::Value);
    }
    Ok(canonical)
}

fn validate_bridge_source(path: &Path, repository: &Path) -> Result<PathBuf, CliError> {
    reject_symlink_components(path)?;
    let raw = fs::symlink_metadata(path).map_err(|_| CliError::Value)?;
    if raw.file_type().is_symlink() || !raw.is_dir() {
        return Err(CliError::Value);
    }
    let canonical = fs::canonicalize(path).map_err(|_| CliError::Value)?;
    let bridge_root = repository.join("bridge");
    if canonical != bridge_root {
        return Err(CliError::Value);
    }
    for name in [
        "main.lua",
        "memory.lua",
        "protocol.lua",
        "generated_addresses.lua",
    ] {
        let file = canonical.join(name);
        let metadata = fs::symlink_metadata(&file).map_err(|_| CliError::Value)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(CliError::Value);
        }
    }
    Ok(canonical)
}

fn canonicalize_executable(path: &Path) -> Result<PathBuf, CliError> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| CliError::Value)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CliError::Value);
    }
    fs::canonicalize(path).map_err(|_| CliError::Value)
}

fn parse_public_key(bytes: &[u8]) -> Result<[u8; 32], CliError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CliError::Value)?
        .trim();
    if text.len() != 64 {
        return Err(CliError::Value);
    }
    let mut output = [0_u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(CliError::Value)?;
        let low = hex_nibble(pair[1]).ok_or(CliError::Value)?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn read_manifest_key(path: &Path) -> Result<Vec<u8>, CliError> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| CliError::Value)?;
    if !metadata.is_file() {
        return Err(CliError::Value);
    }
    let file = open_read_nofollow(path).map_err(|_| CliError::Value)?;
    let mut bytes = Vec::new();
    file.take((MAX_MANIFEST_KEY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::Value)?;
    if bytes.len() > MAX_MANIFEST_KEY_BYTES {
        return Err(CliError::Value);
    }
    Ok(bytes)
}

fn open_read_nofollow(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn stage_verified_rom(source: &Path, private_root: &Path) -> Result<(PathBuf, PathBuf), CliError> {
    reject_symlink_components(source)?;
    let metadata = fs::symlink_metadata(source).map_err(|_| CliError::Value)?;
    if !metadata.is_file() || metadata.len() > MAX_ROM_BYTES {
        return Err(CliError::Value);
    }
    fs::create_dir_all(private_root).map_err(|_| CliError::Value)?;
    // Give every staged ROM its own unpredictable directory under the current
    // user's launcher temp root. This isolates ordinary concurrent sessions;
    // atomic containment against another process running as the same user is
    // a separate Windows production-hardening requirement.
    let staging = tempfile::Builder::new()
        .prefix("coop-rom-")
        .tempdir_in(private_root)
        .map_err(|_| CliError::Value)?;
    let staging = staging.keep();
    let staged = staging.join(format!("rom-{}.gba", uuid::Uuid::new_v4().simple()));
    let marker = staged_rom_marker_path(&staged);
    let result = (|| {
        let mut input = open_read_nofollow(source).map_err(|_| CliError::Value)?;
        let mut output_options = fs::OpenOptions::new();
        output_options.create_new(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            output_options
                .share_mode(0x0000_0001)
                .custom_flags(0x0020_0000);
        }
        let mut output = output_options.open(&staged).map_err(|_| CliError::Value)?;
        let mut total = 0_u64;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = input.read(&mut buffer).map_err(|_| CliError::Value)?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count as u64).ok_or(CliError::Value)?;
            if total > MAX_ROM_BYTES {
                return Err(CliError::Value);
            }
            output
                .write_all(&buffer[..count])
                .map_err(|_| CliError::Value)?;
        }
        output.sync_all().map_err(|_| CliError::Value)?;
        let marker_contents = staged_rom_marker_contents(&staged).map_err(|_| CliError::Value)?;
        let mut marker_output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&marker)
            .map_err(|_| CliError::Value)?;
        marker_output
            .write_all(&marker_contents)
            .map_err(|_| CliError::Value)?;
        marker_output.sync_all().map_err(|_| CliError::Value)?;
        Ok((staged.clone(), marker.clone()))
    })();
    if result.is_err() {
        // Only remove artifacts while the marker still proves the exact
        // staged ROM identity. If publication or identity validation failed,
        // retain the paths as recovery evidence rather than deleting a
        // replacement that may now occupy either name.
        let _ = cleanup_owned_staged_rom(&staged, &marker);
    }
    result
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), CliError> {
    if let Some(error) = platform_error() {
        return Err(error);
    }
    let options = parse_options()?;
    let expected = expected_world(&options)?;
    let catalog = load_release_catalog(&options)?;
    let repository = repository_root()?;
    let workspace = validate_workspace_parent(&options.workspace, &repository)?;
    let epoch = validate_epoch_path(&options.epoch, &repository)?;
    let bridge = validate_bridge_source(&options.bridge, &repository)?;
    let mgba = canonicalize_executable(&options.mgba)?;
    let sidecar = canonicalize_executable(&options.sidecar)?;
    let private_root = private_temp_root()?;
    // Capture executable identities before any asynchronous authentication or
    // lease work. The process supervisor revalidates these bindings at the
    // exact spawn boundary while retaining the bound file/ancestor handles.
    // The ROM is staged only after the server has selected its world.
    let sidecar_template =
        CommandSpec::sidecar_template(&sidecar).map_err(|_| CliError::Runtime)?;
    let key = TrustedManifestKey::new(
        options.manifest_key_id,
        parse_public_key(&read_manifest_key(&options.manifest_key)?)?,
    )
    .map_err(|_| CliError::Runtime)?;
    let password = if options.password_stdin {
        let mut input = io::stdin().lock().take(4097);
        let mut password = String::new();
        input
            .read_to_string(&mut password)
            .map_err(|_| CliError::Runtime)?;
        if password.len() > 4096 {
            return Err(CliError::Value);
        }
        password
    } else {
        rpassword::prompt_password("Password: ").map_err(|_| CliError::Runtime)?
    };
    let password = AuthSession::password(password).map_err(|_| CliError::Runtime)?;
    let api = ReqwestCloudApi::new(&options.api_base).map_err(|_| CliError::Runtime)?;
    let keychain: Arc<dyn RefreshTokenStore> = Arc::new(OsKeychain);
    let mut auth = AuthSession::login(&api, keychain.as_ref(), options.username, password)
        .await
        .map_err(|_| CliError::Runtime)?;
    // A catalog-bound server binds the lease to a ROM world only through the
    // world-aware route; resume is rejected for an unbound lease.
    let client_instance_id =
        coop_cloud::ClientInstanceId::new(uuid::Uuid::new_v4()).map_err(|_| CliError::Runtime)?;
    let idempotency_key =
        coop_cloud::IdempotencyKey::new(uuid::Uuid::new_v4()).map_err(|_| CliError::Runtime)?;
    let request = AcquireLeaseRequest::new(auth.character_id, client_instance_id, idempotency_key);
    let response =
        SessionLifecycle::acquire_world_with_keychain(&api, &mut auth, request, &keychain)
            .await
            .map_err(|_| CliError::Runtime)?;
    let selected = match select_acquired_world(&catalog, response.active_world_id, expected) {
        Ok(selected) => selected,
        Err(error) => {
            return Err(
                release_unselected_world(&api, &mut auth, response, &keychain, error).await,
            );
        }
    };
    let (mgba_spec, compatibility) = match prepare_selected_world(&selected, &private_root, &mgba) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(
                release_unselected_world(&api, &mut auth, response, &keychain, error).await,
            );
        }
    };
    let config = SessionConfig {
        client_instance_id,
        rom_world_id: response.active_world_id,
        manifest: compatibility,
        trusted_manifest_key: key,
        epoch_store: EpochStore::new(epoch),
        workspace_parent: workspace,
        bridge_lua_dir: bridge.clone(),
    };
    let mut session =
        SessionLifecycle::from_world_lease_with_keychain(&api, auth, config, keychain, response)
            .await
            .map_err(|_| CliError::Runtime)?;
    if session.renew_lease_before_child_start(&api).await.is_err() {
        let _ = session.release(&api).await;
        return Err(CliError::Runtime);
    }
    eprintln!(
        "Session materialized at {}. The launcher validated the pinned mGBA 0.11 Windows x64 Qt build and its startup-script capability. The managed desktop launcher starts the bridge automatically; this CLI path remains diagnostic. Session files: bridge {}, resume {}, captures {}.",
        session.workspace.path().display(),
        session.workspace.path().join("main.lua").display(),
        session.workspace.path().join("resume.input.ss1").display(),
        session.workspace.path().join("resume.ss1").display()
    );
    let expected_epoch = session.lease.session_epoch.value();
    let children = SupervisedChildren::start_with_bridge(
        sidecar_template
            .with_session_epoch(expected_epoch)
            .map_err(|_| CliError::Runtime)?,
        mgba_spec,
        expected_epoch,
        &session.workspace,
        &bridge,
    )
    .await;
    let mut children = match children {
        Ok(children) => children,
        Err(error) => {
            if should_release_after_start_failure(&error) {
                let _ = session.release(&api).await;
            } else {
                // Cleanup uncertainty must retain the fenced lease, but it
                // must not retain a refresh token or live credential family.
                let _ = session.close_credentials(&api).await;
            }
            return Err(CliError::Runtime);
        }
    };
    let shutdown = async {
        // Ctrl-C only requests a safe drain.  The lifecycle decides whether a
        // ready checkpoint is already available and never fabricates one.
        let _ = tokio::signal::ctrl_c().await;
    };
    let lifecycle_result = session
        .run_until_shutdown_with_realtime(&api, &mut children, shutdown)
        .await;
    let child_result = children.stop().await;
    if !should_release_after_children_stop(&child_result) {
        // A failed stop does not prove that the child processes are gone.
        // Preserve an authorized SAV locally and leave the lease fenced rather
        // than releasing it while a live child could still emit traffic.
        let _ = session.preserve_recovery_after_child_failure();
        let _ = session.close_credentials(&api).await;
        return Err(CliError::Runtime);
    }
    let release_result = session.release(&api).await;
    if lifecycle_result.is_err() || release_result.is_err() {
        Err(CliError::Runtime)
    } else {
        Ok(())
    }
}

fn should_release_after_children_stop(result: &Result<(), ProcessError>) -> bool {
    result.is_ok()
}

fn should_release_after_start_failure(error: &ProcessError) -> bool {
    error.cleanup_confirmed()
}

// The non-Windows implementation returns the typed boundary error before any
// path canonicalization, workspace creation, or mGBA probing. Windows has no
// error at this boundary.
#[allow(clippy::unnecessary_wraps)]
fn platform_error() -> Option<CliError> {
    #[cfg(not(windows))]
    {
        Some(CliError::UnsupportedPlatform)
    }
    #[cfg(windows)]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn world_id_is_only_an_equality_check_against_the_server_world() {
        let main = RomWorldId::new(1).unwrap();
        let cormoria = RomWorldId::new(2).unwrap();
        assert!(check_expected_world(cormoria, None).is_ok());
        assert!(check_expected_world(cormoria, Some(cormoria)).is_ok());
        assert!(matches!(
            check_expected_world(cormoria, Some(main)),
            Err(CliError::WorldMismatch)
        ));
        let mut options = Options {
            api_base: String::new(),
            username: String::new(),
            release_catalog: PathBuf::from("release_catalog.json"),
            world_id: None,
            mgba: PathBuf::new(),
            manifest_key: PathBuf::new(),
            manifest_key_id: String::new(),
            password_stdin: false,
            workspace: PathBuf::new(),
            epoch: PathBuf::new(),
            bridge: PathBuf::new(),
            sidecar: PathBuf::new(),
        };
        assert_eq!(expected_world(&options).unwrap(), None);
        options.world_id = Some(2);
        assert_eq!(expected_world(&options).unwrap(), Some(cormoria));
        options.world_id = Some(0);
        assert!(matches!(expected_world(&options), Err(CliError::Value)));
    }

    #[test]
    fn epoch_path_is_confined_to_private_temp_root() {
        let repository = repository_root().expect("repository root");
        let temporary = tempdir().expect("temporary directory");
        let private_root = private_temp_root().expect("private root");
        // The default private root is created lazily by the session, so use a
        // path below it for the positive case without creating the file.
        let valid = private_root.join("tests").join("epoch.json");
        let accepted = validate_epoch_path(&valid, &repository).expect("private epoch path");
        assert!(accepted.starts_with(private_root));
        assert!(validate_epoch_path(&repository.join("Cargo.lock"), &repository).is_err());
        assert!(validate_epoch_path(temporary.path(), &repository).is_err());
    }

    #[test]
    fn rom_is_staged_before_hash_and_launch() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source.gba");
        let private = directory.path().join("private");
        std::fs::write(&source, b"verified-rom").unwrap();
        let (staged, marker) = stage_verified_rom(&source, &private).unwrap();
        std::fs::write(&source, b"replacement-rom").unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"verified-rom");
        assert_eq!(
            std::fs::read(marker).unwrap(),
            staged_rom_marker_contents(staged).unwrap()
        );
    }

    #[test]
    fn child_reap_failure_blocks_lease_release() {
        assert!(!should_release_after_children_stop(&Err(
            coop_launcher::process::ProcessError::ChildExited,
        )));
        assert!(should_release_after_children_stop(&Ok(())));
    }

    #[test]
    fn startup_cleanup_uncertainty_blocks_lease_release() {
        let uncertain = ProcessError::StartupCleanup {
            startup: Box::new(ProcessError::Descriptor),
            cleanup: Box::new(ProcessError::Termination(std::io::Error::other(
                "test cleanup failure",
            ))),
        };
        assert!(!should_release_after_start_failure(&uncertain));
        assert!(should_release_after_start_failure(
            &ProcessError::Descriptor
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn production_entrypoint_fails_before_path_or_probe_work() {
        assert!(matches!(
            platform_error(),
            Some(CliError::UnsupportedPlatform)
        ));
    }
}

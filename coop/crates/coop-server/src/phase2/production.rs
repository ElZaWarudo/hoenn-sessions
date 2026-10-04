//! Explicit production startup using mounted secrets, never development keys.

use super::{
    Phase2App, Phase2Error,
    storage::{Phase2Config, ProductionConfig, StorageError, StorageMode},
};
use std::{
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use zeroize::Zeroizing;

/// Mounted release catalog path. Shared by `phase2-local` and production.
pub(super) const RELEASE_CATALOG_PATH_ENV: &str = "COOP_PHASE2_RELEASE_CATALOG_PATH";
/// Trusted SHA-256 of the exact catalog bytes, supplied by release
/// configuration and never by the catalog file itself.
pub(super) const RELEASE_CATALOG_SHA256_ENV: &str = "COOP_PHASE2_RELEASE_CATALOG_SHA256";
const MAX_RELEASE_CATALOG_BYTES: u64 = 64 * 1024;

/// Startup refusal reasons for the trusted release catalog. Every runtime
/// acquisition, snapshot, and resume path needs the catalog, so a server
/// without one must not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum ReleaseCatalogError {
    #[error(
        "COOP_PHASE2_RELEASE_CATALOG_PATH is required; refusing to start without a trusted release catalog"
    )]
    MissingPath,
    #[error(
        "COOP_PHASE2_RELEASE_CATALOG_SHA256 is required; refusing to start without a trusted release catalog digest"
    )]
    MissingDigest,
    #[error(
        "COOP_PHASE2_RELEASE_CATALOG_SHA256 is not a 64-character lowercase SHA-256 hex digest"
    )]
    InvalidDigest,
    #[error("release catalog at COOP_PHASE2_RELEASE_CATALOG_PATH is missing or unreadable")]
    Unreadable,
    #[error("release catalog exceeds the 64 KiB limit")]
    TooLarge,
    #[error("release catalog bytes do not match COOP_PHASE2_RELEASE_CATALOG_SHA256")]
    DigestMismatch,
    #[error("release catalog or its pinned arrival saves failed validation")]
    Rejected,
}

/// Exact catalog bytes read once at startup, with the trusted digest and the
/// release directory that holds its pinned arrival saves.
pub(super) struct TrustedReleaseCatalogFile {
    pub(super) bytes: Vec<u8>,
    pub(super) digest: coop_cloud::Sha256Digest,
    pub(super) root: PathBuf,
}

impl TrustedReleaseCatalogFile {
    /// Installs the catalog exactly as `phase2-local` does, caching arrival
    /// saves from the catalog's own directory.
    pub(super) fn install(
        &self,
        config: Phase2Config,
    ) -> Result<Phase2Config, ReleaseCatalogError> {
        config
            .with_release_catalog_and_arrival_saves(&self.bytes, self.digest, &self.root)
            .map_err(|_| ReleaseCatalogError::Rejected)
    }
}

/// Reads and digest-checks the catalog named by explicit configuration values.
pub(super) fn read_release_catalog(
    path: Option<&str>,
    digest: Option<&str>,
) -> Result<TrustedReleaseCatalogFile, ReleaseCatalogError> {
    let path = path
        .filter(|value| !value.trim().is_empty())
        .ok_or(ReleaseCatalogError::MissingPath)?;
    let digest = digest
        .filter(|value| !value.trim().is_empty())
        .ok_or(ReleaseCatalogError::MissingDigest)?;
    let digest = coop_cloud::Sha256Digest::parse(digest.trim())
        .map_err(|_| ReleaseCatalogError::InvalidDigest)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| ReleaseCatalogError::Unreadable)?
        .take(MAX_RELEASE_CATALOG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReleaseCatalogError::Unreadable)?;
    if bytes.len() as u64 > MAX_RELEASE_CATALOG_BYTES {
        return Err(ReleaseCatalogError::TooLarge);
    }
    if coop_cloud::Sha256Digest::of_bytes(&bytes) != digest {
        return Err(ReleaseCatalogError::DigestMismatch);
    }
    let root = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Ok(TrustedReleaseCatalogFile {
        bytes,
        digest,
        root,
    })
}

/// Reads the catalog from `COOP_PHASE2_RELEASE_CATALOG_PATH` and
/// `COOP_PHASE2_RELEASE_CATALOG_SHA256`.
pub(super) fn release_catalog_from_env() -> Result<TrustedReleaseCatalogFile, ReleaseCatalogError> {
    let path = std::env::var(RELEASE_CATALOG_PATH_ENV).ok();
    let digest = std::env::var(RELEASE_CATALOG_SHA256_ENV).ok();
    read_release_catalog(path.as_deref(), digest.as_deref())
}

/// Production fails closed unless a trusted catalog is configured and valid.
fn install_release_catalog(
    config: Phase2Config,
    path: Option<&str>,
    digest: Option<&str>,
) -> Result<Phase2Config, ReleaseCatalogError> {
    read_release_catalog(path, digest)?.install(config)
}

fn release_catalog_startup_error(error: ReleaseCatalogError) -> Phase2Error {
    eprintln!("co-op production startup refused: {error}");
    StorageError::InvalidConfiguration.into()
}

/// Whole HTTP operations include process-local transition locks, so move them
/// off the network runtime too. Websocket tasks stay on the owning runtime.
pub(super) async fn offload_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if request.uri().path() == "/health/ready" {
        return next.run(request).await;
    }
    let deadline = if request.uri().path().starts_with("/v1/auth/") {
        Duration::from_secs(15)
    } else {
        Duration::from_secs(120)
    };
    static CAPACITY: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    let Ok(permit) = CAPACITY
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(32)))
        .clone()
        .try_acquire_owned()
    else {
        return Phase2Error::Busy.into_response();
    };
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        runtime.block_on(async {
            tokio::time::timeout(deadline, next.run(request))
                .await
                .unwrap_or_else(|_| Phase2Error::Busy.into_response())
        })
    })
    .await
    .unwrap_or_else(|_| Phase2Error::Internal.into_response())
}

pub(super) fn read_secret(path: &Path, limit: usize) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    let file = std::fs::File::open(path).map_err(|_| StorageError::InvalidConfiguration)?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::InvalidConfiguration)?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err(StorageError::InvalidConfiguration);
    }
    Ok(bytes)
}

fn required(name: &str) -> Result<String, StorageError> {
    std::env::var(name).map_err(|_| StorageError::InvalidConfiguration)
}

fn secret(name: &str, limit: usize) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    read_secret(Path::new(&required(name)?), limit)
}

fn from_env() -> Result<Phase2App, Phase2Error> {
    use std::fmt::Write;
    let database = secret("COOP_DATABASE_URL_FILE", 4096)?;
    let database = std::str::from_utf8(&database)
        .map_err(|_| StorageError::InvalidConfiguration)?
        .trim();
    let production = ProductionConfig::new(database, required("COOP_FIREBASE_BUCKET")?)?;
    let signing = secret("COOP_SIGNING_KEY_FILE", 32)?;
    let signing: [u8; 32] = signing
        .as_slice()
        .try_into()
        .map_err(|_| StorageError::InvalidConfiguration)?;
    let pepper = secret("COOP_INVITE_PEPPER_FILE", 32)?;
    if pepper.len() != 32 || signing == [0; 32] || pepper.iter().all(|byte| *byte == 0) {
        return Err(StorageError::InvalidConfiguration.into());
    }
    let public_key = ed25519_dalek::SigningKey::from_bytes(&signing)
        .verifying_key()
        .to_bytes();
    let public_key_hex = public_key
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        });
    let mut config = Phase2Config::local(
        pepper.to_vec(),
        coop_cloud::SigningPrivateKey::from_bytes(signing),
        "pilot-v1",
    )?
    .with_upload_base_url(required("COOP_UPLOAD_BASE_URL")?);
    // Install the trusted catalog before any adapter connects: without it every
    // world acquisition, snapshot prepare, and resume package would fail.
    let catalog_path = std::env::var(RELEASE_CATALOG_PATH_ENV).ok();
    let catalog_digest = std::env::var(RELEASE_CATALOG_SHA256_ENV).ok();
    config = install_release_catalog(config, catalog_path.as_deref(), catalog_digest.as_deref())
        .map_err(release_catalog_startup_error)?;
    config.mode = StorageMode::PostgresFirebase;
    config.object_store = Some(Arc::new(
        super::firebase::FirebaseStorage::from_service_account_file(
            &production.firebase_bucket,
            Path::new(&required("COOP_FIREBASE_SERVICE_ACCOUNT_FILE")?),
        )?,
    ));
    config.repository = Some(Arc::new(
        super::persistent::PostgresStateRepository::connect(&production.database_url)?,
    ));
    config.production = Some(production);
    let release_root = required("COOP_RELEASE_ROOT")?;
    if release_root.trim().is_empty() {
        return Err(StorageError::InvalidConfiguration.into());
    }
    let app = Phase2App::new(config)?.with_release_root(release_root)?;
    match std::env::var("COOP_BOOTSTRAP_INVITE_FILE") {
        Ok(path) if !path.is_empty() => {
            let bytes = read_secret(Path::new(&path), 1024)?;
            let code = std::str::from_utf8(&bytes)
                .map_err(|_| StorageError::InvalidConfiguration)?
                .trim();
            let invitation = coop_cloud::InvitationCode::new(code)
                .map_err(|_| StorageError::InvalidConfiguration)?;
            match app.add_invitation(invitation.expose_secret()) {
                Ok(()) | Err(Phase2Error::Conflict) => {} // Never reset consumed invitations on restart.
                Err(error) => return Err(error),
            }
        }
        Ok(_) | Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(StorageError::InvalidConfiguration.into());
        }
    }
    println!("Co-op signing identity: key_id=pilot-v1 public_key_hex={public_key_hex}");
    Ok(app)
}

/// Production startup attempts before giving up. The database and object
/// store often trail the server during deploys, so transient connect
/// failures retry with a fixed delay. Invalid secrets fail on every attempt
/// and still exit closed after the last one.
const MAX_STARTUP_ATTEMPTS: u32 = 5;
const STARTUP_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Returns the delay before the next startup attempt, or [`None`] when the
/// retry budget is exhausted. Extracted so the bound is unit-tested.
fn next_startup_delay(attempt: u32) -> Option<Duration> {
    (attempt < MAX_STARTUP_ATTEMPTS).then_some(STARTUP_RETRY_DELAY)
}

async fn load_production_app() -> Result<Phase2App, Phase2Error> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        let loaded = tokio::task::spawn_blocking(from_env)
            .await
            .map_err(|_| Phase2Error::Internal)?;
        match loaded {
            Ok(app) => return Ok(app),
            Err(error) if next_startup_delay(attempt).is_some() => {
                eprintln!(
                    "co-op production startup attempt {attempt}/{MAX_STARTUP_ATTEMPTS} failed ({error}); retrying"
                );
                tokio::time::sleep(STARTUP_RETRY_DELAY).await;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Runs the authenticated persistent server behind a TLS reverse proxy.
///
/// # Errors
/// Fails closed when required secrets, adapters, or the listener are unavailable.
pub async fn serve_phase2_production(address: SocketAddr) -> Result<(), Phase2Error> {
    let app = load_production_app().await?;
    super::spawn_group_expiry_watchdog(app.clone());
    let watchdog = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if watchdog.store.repository.is_fenced() || watchdog.store.objects.is_fenced() {
                eprintln!("co-op persistent backend fenced; exiting for supervisor restart");
                std::process::exit(1);
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|_| Phase2Error::Internal)?;
    let shutdown_signal = app.shutdown.clone();
    axum::serve(listener, app.router())
        .with_graceful_shutdown(shutdown(shutdown_signal))
        .await
        .map_err(|_| Phase2Error::Internal)
}

pub(super) async fn shutdown(notify: tokio::sync::watch::Sender<bool>) {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            notify.send_replace(true);
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
    notify.send_replace(true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_retry_budget_is_bounded() {
        assert_eq!(next_startup_delay(1), Some(STARTUP_RETRY_DELAY));
        assert_eq!(
            next_startup_delay(MAX_STARTUP_ATTEMPTS - 1),
            Some(STARTUP_RETRY_DELAY)
        );
        assert_eq!(next_startup_delay(MAX_STARTUP_ATTEMPTS), None);
        assert_eq!(next_startup_delay(MAX_STARTUP_ATTEMPTS + 1), None);
    }

    fn production_shaped_config() -> Phase2Config {
        Phase2Config::local(
            vec![0x55; 32],
            coop_cloud::SigningPrivateKey::from_bytes([7; 32]),
            "pilot-v1",
        )
        .expect("config")
    }

    #[test]
    fn production_startup_requires_a_trusted_release_catalog() {
        let root = std::env::temp_dir().join(format!(
            "coop-production-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("catalog dir");
        let build = super::super::saves::current_runtime_build_identity().expect("build");
        let catalog = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "worlds": [{"world_id": 1, "build": build}]
        }))
        .expect("catalog bytes");
        let catalog_path = root.join("release_catalog.json");
        std::fs::write(&catalog_path, &catalog).expect("write catalog");
        let path = catalog_path.to_str().expect("utf-8 temp path").to_owned();
        let digest = coop_cloud::Sha256Digest::of_bytes(&catalog).as_hex();
        let other = coop_cloud::Sha256Digest::of_bytes(b"other").as_hex();
        let missing = root.join("absent.json").to_str().unwrap().to_owned();

        let rejected = |path: Option<&str>, digest: Option<&str>| {
            install_release_catalog(production_shaped_config(), path, digest)
                .err()
                .expect("startup must refuse")
        };
        assert_eq!(rejected(None, None), ReleaseCatalogError::MissingPath);
        assert_eq!(
            rejected(Some(""), Some(&digest)),
            ReleaseCatalogError::MissingPath
        );
        assert_eq!(
            rejected(Some(&path), None),
            ReleaseCatalogError::MissingDigest
        );
        assert_eq!(
            rejected(Some(&path), Some("not-a-digest")),
            ReleaseCatalogError::InvalidDigest
        );
        assert_eq!(
            rejected(Some(&missing), Some(&digest)),
            ReleaseCatalogError::Unreadable
        );
        assert_eq!(
            rejected(Some(&path), Some(&other)),
            ReleaseCatalogError::DigestMismatch
        );
        assert_eq!(
            release_catalog_startup_error(ReleaseCatalogError::MissingPath),
            Phase2Error::Internal
        );

        let installed =
            install_release_catalog(production_shaped_config(), Some(&path), Some(&digest))
                .expect("trusted catalog installs");
        assert!(installed.release_catalog.is_some());

        std::fs::write(&catalog_path, vec![b' '; 64 * 1024 + 1]).expect("oversized");
        assert_eq!(
            rejected(Some(&path), Some(&digest)),
            ReleaseCatalogError::TooLarge
        );
        std::fs::remove_file(&catalog_path).expect("remove catalog");
        std::fs::remove_dir(&root).expect("remove catalog dir");
    }
}

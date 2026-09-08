//! Explicit production startup using mounted secrets, never development keys.

use super::{
    Phase2App, Phase2Error,
    storage::{Phase2Config, ProductionConfig, StorageError, StorageMode},
};
use std::{io::Read, net::SocketAddr, path::Path, sync::Arc};
use zeroize::Zeroizing;

/// Whole HTTP operations include process-local transition locks, so move them
/// off the network runtime too. Websocket tasks stay on the owning runtime.
pub(super) async fn offload_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
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
        runtime.block_on(next.run(request))
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
    let app = Phase2App::new(config)?;
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

/// Runs the authenticated persistent server behind a TLS reverse proxy.
///
/// # Errors
/// Fails closed when required secrets, adapters, or the listener are unavailable.
pub async fn serve_phase2_production(address: SocketAddr) -> Result<(), Phase2Error> {
    let app = tokio::task::spawn_blocking(from_env)
        .await
        .map_err(|_| Phase2Error::Internal)??;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|_| Phase2Error::Internal)?;
    axum::serve(listener, app.router())
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|_| Phase2Error::Internal)
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

use coop_cloud::{
    AcquireLeaseRequest, CharacterId, ClientInstanceId, IdempotencyKey, Password, RefreshToken,
    TrustedManifestKey, UserId,
};
use coop_launcher::arrival_verifier::{
    ArrivalProofInput, AuthenticatedArrivalEvidence, verify_arrival_proof,
};
use coop_launcher::keychain::{KeychainError, RefreshTokenStore};
use coop_launcher::live_requests::{LiveRequest, LiveRequestError, live_request_channel};
use coop_launcher::process::ControlChannel;
use coop_launcher::process::{SessionSupervisor, embedded::EmbeddedSupervisor};
use coop_launcher::rom_travel::{LeaseFenceIdentity, RomTravelJournal, TravelPhase};
use coop_launcher::session::{SessionRunOutcome, SessionWorkspace};
use coop_launcher::travel_coordinator::{
    StageOutcome, commit_acknowledged_handoff, recover_pending_handoff, stage_portal_travel,
    verify_staged_arrival_with,
};
use coop_launcher::{
    AuthError, AuthSession, BuildCompatibility, EpochStore, ReqwestCloudApi, SessionConfig,
    SessionError, SessionLifecycle, TrustedRomCatalog, WorldAcquireIntentStore,
};
use coop_sidecar::LocalSidecar;
use coop_sidecar::control::{ControlCommand, ControlEvent};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JString, JValue},
    sys::{jboolean, jlong, jstring},
};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::Zeroizing;

const SERVER: &str = match option_env!("HOENN_SERVER_URL") {
    Some(url) => url,
    None => "https://169-128-190-115.sslip.io",
};
const KEY: &str = "f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188";

// Stable `code` attached to every `{"type":"error"}` poll event. Java must
// match on `code`, never on the human-readable `message` (messages stay
// localized and may change without notice).
const ERROR_LEASE_CONFLICT: &str = "lease_conflict";
const ERROR_CLOUD_UNREACHABLE: &str = "cloud_unreachable";
const ERROR_INTERNAL: &str = "internal_error";
// The installed runtime predates world-bound leases (no signed region
// catalog). Java treats any unmapped code as a generic failure.
const ERROR_UPDATE_REQUIRED: &str = "update_required";

// Classified session failure: a stable machine-readable `code` plus the
// human-readable detail kept for display.
#[derive(Debug)]
struct RunError {
    code: &'static str,
    message: String,
}

impl RunError {
    fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(ERROR_INTERNAL, message.into())
    }

    // Classifies a cloud session failure at its source.
    fn session(prefix: &str, error: SessionError) -> Self {
        let code = code_for_session_error(&error);
        let detail = match &error {
            SessionError::Epoch(epoch) => format!("{epoch:?}"),
            _ => error.to_string(),
        };
        Self::new(code, format!("{prefix}: {detail}"))
    }

    // Classifies an authentication failure at its source.
    fn auth(prefix: &str, error: AuthError) -> Self {
        let detail = error.to_string();
        Self::new(code_for_auth_error(&error), format!("{prefix}: {detail}"))
    }

    fn with_recovery_context(self, revision: u64, recovery: &std::path::Path) -> Self {
        Self::new(
            self.code,
            format!(
                "{}. Revisión cloud {revision}; recuperación, si procede: {}",
                self.message,
                recovery.to_string_lossy().into_owned()
            ),
        )
    }

    fn event(&self) -> Value {
        error_event(self.code, &self.message)
    }
}

// Maps a session failure to its stable poll code at the point of origin:
// lease ownership conflicts, unreachable cloud transport, and everything
// else (device-local or unexpected) as internal.
fn code_for_session_error(error: &SessionError) -> &'static str {
    match error {
        SessionError::AcquireConflict
        | SessionError::Lease
        | SessionError::FinalizeConflict
        | SessionError::CheckpointNotAuthorized => ERROR_LEASE_CONFLICT,
        SessionError::Cloud => ERROR_CLOUD_UNREACHABLE,
        SessionError::Auth(auth) => code_for_auth_error(auth),
        _ => ERROR_INTERNAL,
    }
}

fn code_for_auth_error(error: &AuthError) -> &'static str {
    match error {
        AuthError::Transport => ERROR_CLOUD_UNREACHABLE,
        _ => ERROR_INTERNAL,
    }
}

fn error_event(code: &str, message: &str) -> Value {
    json!({"type":"error","code":code,"message":message})
}

// Recovers a poisoned mutex instead of panicking: a previous holder may
// have panicked (including across a caught JNI panic), but the guarded
// state itself is still usable.
fn recover_lock<'a, T>(mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

// Queues an `internal_error` poll event when a session handle exists.
// Never panics: safe to call from a panic handler.
fn queue_internal_error(message: &str) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let slot = recover_lock(active());
        if let Some(handle) = slot.as_ref() {
            let _ = handle.sink.try_send(error_event(ERROR_INTERNAL, message));
        }
    }));
}

/// Delegates refresh-token persistence to an AES-GCM key held by Android Keystore.
struct AndroidTokens {
    vm: Arc<JavaVM>,
    class: GlobalRef,
}

impl AndroidTokens {
    fn strings<'local>(
        env: &mut JNIEnv<'local>,
        service: &str,
        username: &str,
    ) -> Result<(JString<'local>, JString<'local>), KeychainError> {
        Ok((
            env.new_string(service)
                .map_err(|_| KeychainError::Operation)?,
            env.new_string(username)
                .map_err(|_| KeychainError::Operation)?,
        ))
    }

    fn store_account(&self, auth: &AuthSession) -> Result<(), KeychainError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| KeychainError::Unavailable)?;
        let username = env
            .new_string(auth.username.as_str())
            .map_err(|_| KeychainError::Operation)?;
        let user_id = env
            .new_string(auth.user_id.to_string())
            .map_err(|_| KeychainError::Operation)?;
        let character_id = env
            .new_string(auth.character_id.to_string())
            .map_err(|_| KeychainError::Operation)?;
        let stored = env
            .call_static_method(
                &self.class,
                "storeAccount",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&username),
                    JValue::Object(&user_id),
                    JValue::Object(&character_id),
                ],
            )
            .and_then(|value| value.z())
            .map_err(|_| KeychainError::Operation)?;
        if stored {
            Ok(())
        } else {
            Err(KeychainError::Operation)
        }
    }
}

impl RefreshTokenStore for AndroidTokens {
    fn load(&self, service: &str, username: &str) -> Result<Option<RefreshToken>, KeychainError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| KeychainError::Unavailable)?;
        let (service, username) = Self::strings(&mut env, service, username)?;
        let value = env
            .call_static_method(
                &self.class,
                "load",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&service), JValue::Object(&username)],
            )
            .and_then(|value| value.l())
            .map_err(|_| KeychainError::Operation)?;
        if value.is_null() {
            return Ok(None);
        }
        let token = env
            .get_string(&JString::from(value))
            .map_err(|_| KeychainError::Operation)?
            .to_string_lossy()
            .into_owned();
        RefreshToken::new(token)
            .map(Some)
            .map_err(KeychainError::Invalid)
    }
    fn store(
        &self,
        service: &str,
        username: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| KeychainError::Unavailable)?;
        let (service, username) = Self::strings(&mut env, service, username)?;
        let token = env
            .new_string(token.expose_secret())
            .map_err(|_| KeychainError::Operation)?;
        let stored = env
            .call_static_method(
                &self.class,
                "store",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&service),
                    JValue::Object(&username),
                    JValue::Object(&token),
                ],
            )
            .and_then(|value| value.z())
            .map_err(|_| KeychainError::Operation)?;
        if stored {
            Ok(())
        } else {
            Err(KeychainError::Operation)
        }
    }
    fn delete(&self, service: &str, username: &str) -> Result<(), KeychainError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| KeychainError::Unavailable)?;
        let (service, username) = Self::strings(&mut env, service, username)?;
        let deleted = env
            .call_static_method(
                &self.class,
                "delete",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&service), JValue::Object(&username)],
            )
            .and_then(|value| value.z())
            .map_err(|_| KeychainError::Operation)?;
        if deleted {
            Ok(())
        } else {
            Err(KeychainError::Operation)
        }
    }
}

struct Handle {
    stop: watch::Sender<u8>, // 0 running, 1 close, 2 reconnect from cloud save
    sink: mpsc::Sender<Value>,
    events: Mutex<mpsc::Receiver<Value>>,
    host: Mutex<HostState>,
    revision: std::sync::atomic::AtomicU64,
    finished: std::sync::atomic::AtomicBool,
    verifier_ack: Mutex<Option<(u64, oneshot::Sender<bool>)>>,
    /// Requests into the running realtime session (pairing-code joins).
    live: Mutex<Option<mpsc::Sender<LiveRequest>>>,
}
struct HostState {
    closed: bool,
    stopped_ack: Option<oneshot::Sender<()>>,
}
static ACTIVE: OnceLock<Mutex<Option<Arc<Handle>>>> = OnceLock::new();
static NEXT_VERIFICATION_ID: AtomicU64 = AtomicU64::new(1);
fn active() -> &'static Mutex<Option<Arc<Handle>>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

fn key() -> TrustedManifestKey {
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&KEY[i * 2..i * 2 + 2], 16).expect("public key literal");
    }
    TrustedManifestKey::new("pilot-v1", bytes).expect("strong public key")
}

fn runtime_directory(root: &std::path::Path) -> Result<PathBuf, RunError> {
    let name = std::fs::read_to_string(root.join("runtime/current"))
        .map_err(|_| RunError::internal("Versión del juego no instalada"))?;
    let name = name.trim_end_matches(['\r', '\n']);
    if name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(RunError::internal("Versión del juego inválida"));
    }
    let directory = root.join("runtime").join(name);
    let canonical = directory
        .canonicalize()
        .map_err(|_| RunError::internal("Versión del juego ausente"))?;
    if !canonical.starts_with(root.join("runtime")) {
        return Err(RunError::internal("Ruta del juego inválida"));
    }
    Ok(canonical)
}

fn pending_world_request(
    store: &WorldAcquireIntentStore,
    character_id: CharacterId,
    client_instance_id: ClientInstanceId,
) -> Result<AcquireLeaseRequest, RunError> {
    let requested = match store.read().map_err(|error| {
        RunError::internal(format!("Intención de adquisición ilegible: {error}"))
    })? {
        Some(existing) => existing.request,
        None => AcquireLeaseRequest::new(
            character_id,
            client_instance_id,
            IdempotencyKey::new(uuid::Uuid::new_v4())
                .map_err(|_| RunError::internal("Clave de adquisición inválida"))?,
        )
        .replacing_same_client(),
    };
    if requested.character_id != character_id
        || requested.client_instance_id != client_instance_id
        || !requested.replace_same_client
    {
        return Err(RunError::internal(
            "Intención de adquisición pertenece a otra sesión",
        ));
    }
    store
        .load_or_create(requested)
        .map_err(|error| RunError::internal(format!("No se pudo guardar adquisición: {error}")))
}

fn persist_client_instance(path: &std::path::Path, value: &str) -> Result<(), RunError> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| RunError::internal("No se pudo crear identidad local"))?;
    file.write_all(value.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| RunError::internal("No se pudo guardar identidad local"))?;
    std::fs::rename(&temporary, path)
        .map_err(|_| RunError::internal("No se pudo instalar identidad local"))?;
    if let Some(parent) = path.parent() {
        // Android filesystems support directory fsync. Keep file sync above
        // authoritative on platforms where opening a directory is rejected.
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

fn client_instance_for_run(
    root: &std::path::Path,
    character_id: CharacterId,
    legacy: bool,
) -> Result<ClientInstanceId, RunError> {
    let instance_file = root.join("client-instance.txt");
    let recovered = if legacy {
        None
    } else {
        WorldAcquireIntentStore::new(root.join("world-acquire"), character_id)
            .map_err(|_| RunError::internal("Intención de adquisición no disponible"))?
            .read()
            .map_err(|_| RunError::internal("Intención de adquisición ilegible"))?
            .map(|record| record.request.client_instance_id)
    };
    if instance_file.exists() {
        let stored = std::fs::read_to_string(&instance_file)
            .ok()
            .and_then(|value| ClientInstanceId::parse(&value).ok());
        if let Some(stored) = stored {
            if recovered.is_some_and(|pending| pending != stored) {
                return Err(RunError::internal(
                    "Identidad local difiere de la adquisición",
                ));
            }
            return Ok(stored);
        }
        if recovered.is_none() {
            return Err(RunError::internal("Identidad local inválida"));
        }
    }
    let instance = match recovered {
        Some(instance) => instance,
        None => ClientInstanceId::new(uuid::Uuid::new_v4())
            .map_err(|_| RunError::internal("Identidad local inválida"))?,
    };
    persist_client_instance(&instance_file, &instance.to_string())?;
    Ok(instance)
}

async fn verify_embedded_arrival(
    staged: &coop_launcher::travel_coordinator::StagedDestination,
    nonce: [u8; 16],
    session: &SessionLifecycle,
    destination: &coop_launcher::SelectedRomWorld,
    compatibility: &BuildCompatibility,
    workspace: &SessionWorkspace,
    handle: &Handle,
    events: &mpsc::Sender<Value>,
) -> Result<AuthenticatedArrivalEvidence, coop_launcher::travel_coordinator::TravelCoordinatorError>
{
    use coop_launcher::travel_coordinator::TravelCoordinatorError;
    workspace
        .write_atomic("pending_commits.json", b"[]")
        .map_err(|_| TravelCoordinatorError::Uncertain)?;
    workspace
        .write_atomic("character.sav", staged.destination_save())
        .map_err(|_| TravelCoordinatorError::Uncertain)?;
    let rom = workspace.path().join("destination.gba");
    std::fs::copy(&destination.rom_path, &rom).map_err(|_| TravelCoordinatorError::Uncertain)?;
    let sidecar = LocalSidecar::bind_arrival_verifier()
        .await
        .map_err(|_| TravelCoordinatorError::Uncertain)?;
    let descriptor = sidecar.session_descriptor();
    let sidecar_task = tokio::spawn(sidecar.serve());
    let result = async {
        let registry = session
            .registry_contract()
            .map_err(|_| TravelCoordinatorError::Uncertain)?;
        let mut control = ControlChannel::connect(&descriptor)
            .await
            .map_err(|_| TravelCoordinatorError::Uncertain)?;
        let id = NEXT_VERIFICATION_ID.fetch_add(1, Ordering::Relaxed);
        let (ack_tx, mut ack_rx) = oneshot::channel();
        *recover_lock(&handle.verifier_ack) = Some((id, ack_tx));
        let sent = tokio::time::timeout(
            Duration::from_secs(5),
            events.send(json!({
                "type":"verify_arrival", "verification_id":id,
                "world_id":staged.destination_world().get(),
                "rom":rom, "rom_sha256":destination.rom_sha256().as_hex(),
                "build_id":compatibility.target.game_build_id.value(),
                "save":workspace.path().join("character.sav"),
                "bridge":descriptor.bridge(),
                "bridge_address":compatibility.manifest.net_bridge.address,
                "generation_address":compatibility.manifest.save.generation_address,
            })),
        )
        .await
        .is_ok_and(|result| result.is_ok());
        if !sent {
            recover_lock(&handle.verifier_ack).take();
            return Err(TravelCoordinatorError::Uncertain);
        }
        let observation = tokio::time::timeout(Duration::from_secs(45), async {
            if !matches!(
                control.receive().await,
                Ok(ControlEvent::ArrivalVerifierReady {})
            ) {
                return Err(TravelCoordinatorError::Uncertain);
            }
            control
                .send(&ControlCommand::ArrivalChallenge { nonce })
                .await
                .map_err(|_| TravelCoordinatorError::Uncertain)?;
            let proof = match control.receive().await {
                Ok(ControlEvent::ArrivalProof(proof)) => proof,
                _ => return Err(TravelCoordinatorError::Uncertain),
            };
            verify_arrival_proof(
                ArrivalProofInput {
                    expected_full_sav_sha256: staged.destination_save_sha256(),
                    staged_sav: staged.destination_save(),
                    registry,
                    destination_world: staged.destination_world(),
                    expected_save_generation: staged.destination_save_generation(),
                    expected_map_group: staged.arrival_location()[0],
                    expected_map_num: staged.arrival_location()[1],
                    persisted_nonce: nonce,
                },
                proof,
            )
            .map_err(TravelCoordinatorError::ArrivalVerification)
        })
        .await
        .unwrap_or(Err(TravelCoordinatorError::Uncertain));
        // Always request a native close after Java has accepted the load.
        let stop_sent = tokio::time::timeout(
            Duration::from_secs(5),
            events.send(json!({"type":"verify_arrival_stop","verification_id":id})),
        )
        .await
        .is_ok_and(|result| result.is_ok());
        let closed = tokio::time::timeout(Duration::from_secs(10), &mut ack_rx)
            .await
            .is_ok_and(|ack| ack.is_ok_and(|confirmed| confirmed));
        recover_lock(&handle.verifier_ack).take();
        if !stop_sent || !closed {
            return Err(TravelCoordinatorError::Uncertain);
        }
        observation
    }
    .await;
    sidecar_task.abort();
    let _ = sidecar_task.await;
    result
}

fn spawn_revision_forwarder(
    mut revisions: watch::Receiver<u64>,
    handle: Arc<Handle>,
    events: mpsc::Sender<Value>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while revisions.changed().await.is_ok() {
            let revision = *revisions.borrow_and_update();
            handle.revision.store(revision, Ordering::Release);
            if events
                .send(json!({"type":"saved","revision":revision}))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

async fn run(
    root: PathBuf,
    user: String,
    password: Zeroizing<String>,
    user_id: String,
    character_id: String,
    catalog_sha256: String,
    resume: bool,
    logout_only: bool,
    vm: Arc<JavaVM>,
    credential_class: GlobalRef,
    handle: Arc<Handle>,
    events: mpsc::Sender<Value>,
    mut stop: watch::Receiver<u8>,
) -> Result<(), RunError> {
    let root = root
        .canonicalize()
        .map_err(|_| RunError::internal("Directorio privado inválido"))?;
    // Keep the guard until the lease is released. An exact acquire replay may
    // refer to an epoch accepted by the process that crashed previously; the
    // guard proves another updated Android process cannot still own it.
    let _world_process_lock = if catalog_sha256.is_empty() || logout_only {
        None
    } else {
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .open(root.join("world-acquire-process.lock"))
            .map_err(|_| RunError::internal("No se pudo abrir bloqueo de región"))?;
        // std::fs::File::try_lock always reports Unsupported on Android.
        coop_launcher::file_lock::try_lock_file(&lock).map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                RunError::internal("Otra sesión local usa la región")
            }
            std::fs::TryLockError::Error(_) => RunError::internal("No se pudo bloquear la región"),
        })?;
        Some(lock)
    };
    let api = ReqwestCloudApi::new(SERVER).map_err(|_| RunError::internal("Endpoint inválido"))?;
    let vault = Arc::new(AndroidTokens {
        vm,
        class: credential_class,
    });
    let keychain: Arc<dyn RefreshTokenStore> = vault.clone();
    let bridge = root.join("bridge");
    std::fs::create_dir_all(&bridge)
        .map_err(|_| RunError::internal("No se pudo crear el directorio bridge"))?;
    let mut auth = if resume {
        let user_id = UserId::parse(&user_id)
            .map_err(|_| RunError::internal("Identidad guardada inválida"))?;
        let character_id = CharacterId::parse(&character_id)
            .map_err(|_| RunError::internal("Personaje guardado inválido"))?;
        AuthSession::refresh_from_keychain(&api, vault.as_ref(), &user, user_id, character_id)
            .await
            .map_err(|error| RunError::auth("No se pudo restaurar la sesión guardada", error))?
    } else {
        AuthSession::login(
            &api,
            vault.as_ref(),
            &user,
            Password::new(password.to_string())
                .map_err(|_| RunError::internal("Contraseña inválida"))?,
        )
        .await
        .map_err(|error| RunError::auth("Login rechazado o red no disponible", error))?
    };
    drop(password);
    vault
        .store_account(&auth)
        .map_err(|_| RunError::internal("No se pudo proteger la sesión en el dispositivo"))?;
    if logout_only {
        auth.logout(&api, vault.as_ref())
            .await
            .map_err(|error| RunError::auth("No se pudo cerrar la sesión remota", error))?;
        let _ = events
            .send(json!({"type":"closed","revision":0,"signed_out":true}))
            .await;
        return Ok(());
    }
    let runtime = runtime_directory(&root)?;
    if catalog_sha256.is_empty() {
        // Without a signed region catalog the lease cannot be bound to a ROM
        // world, and the server rejects resume for an unbound lease. Fail
        // closed instead of acquiring through the legacy route.
        return Err(RunError::new(
            ERROR_UPDATE_REQUIRED,
            "Actualización requerida: falta el catálogo de regiones firmado".to_owned(),
        ));
    }
    let client_instance_id = client_instance_for_run(&root, auth.character_id, false)?;
    let epoch_store = match EpochStore::for_character(&root, auth.character_id) {
        Ok(store) => store,
        Err(error) => {
            return Err(RunError::internal(format!(
                "Historial local de sesión no disponible: {error}"
            )));
        }
    };
    let mut world_intent;
    let portal_catalog;
    let (mut selected_world_id, mut selected_rom_sha, mut selected_build_id);
    let (acquired, mut rom_path) = {
        let catalog =
            TrustedRomCatalog::load(&runtime.join("release_catalog.json"), &catalog_sha256)
                .map_err(|_| RunError::internal("Catálogo de regiones incompatible"))?;
        let intent = WorldAcquireIntentStore::new(root.join("world-acquire"), auth.character_id)
            .map_err(|error| {
                RunError::internal(format!("Historial de adquisición no disponible: {error}"))
            })?;
        let mut request = pending_world_request(&intent, auth.character_id, client_instance_id)?;
        let mut response =
            match SessionLifecycle::acquire_world_replacing_same_client_with_keychain(
                &api, &mut auth, request, &keychain,
            )
            .await
            {
                Err(SessionError::AcquireClosed) => {
                    intent.clear_exact(request).map_err(|_| {
                        RunError::internal("No se pudo renovar la adquisición cerrada")
                    })?;
                    request =
                        pending_world_request(&intent, auth.character_id, client_instance_id)?;
                    SessionLifecycle::acquire_world_replacing_same_client_with_keychain(
                        &api, &mut auth, request, &keychain,
                    )
                    .await
                    .map_err(|error| RunError::session("No se pudo adquirir la región", error))?
                }
                Err(error) => {
                    return Err(RunError::session("No se pudo adquirir la región", error));
                }
                Ok(response) => response,
            };
        if epoch_store
            .read(auth.character_id, response.lease.session_id)
            .is_ok_and(|record| {
                record.is_some_and(|record| {
                    record.greatest_epoch == response.lease.session_epoch.value()
                })
            })
        {
            // The same durable key replayed a lease whose epoch had already
            // been accepted before a process crash. It cannot be accepted a
            // second time. Under the process guard, close that exact lease and
            // persist a fresh request before touching local session state.
            SessionLifecycle::release_preacquired_world_lease(&api, &mut auth, response, &keychain)
                .await
                .map_err(|error| {
                    RunError::session("No se pudo cerrar adquisición anterior", error)
                })?;
            intent
                .clear_exact(request)
                .map_err(|_| RunError::internal("No se pudo cerrar intención anterior"))?;
            request = pending_world_request(&intent, auth.character_id, client_instance_id)?;
            response = SessionLifecycle::acquire_world_replacing_same_client_with_keychain(
                &api, &mut auth, request, &keychain,
            )
            .await
            .map_err(|error| RunError::session("No se pudo reacquirir la región", error))?;
        }
        let selected = catalog.world(response.active_world_id);
        let compatibility = selected.and_then(|world| {
            let compatible = BuildCompatibility::load_android(&world.bridge_path, &world.rom_path)?;
            world.check_compatibility(&compatible)?;
            Ok(compatible)
        });
        let compatibility = match compatibility {
            Ok(compatible) => compatible,
            Err(_) => {
                if SessionLifecycle::release_preacquired_world_lease(
                    &api, &mut auth, response, &keychain,
                )
                .await
                .is_ok()
                {
                    let _ = intent.clear_exact(request);
                }
                return Err(RunError::internal(
                    "ROM de región no disponible o incompatible",
                ));
            }
        };
        let rom_path = compatibility.rom_path.clone();
        selected_world_id = response.active_world_id.get();
        selected_rom_sha = compatibility.target.rom_sha256.as_hex();
        selected_build_id = compatibility.target.game_build_id.value().to_owned();
        let config = SessionConfig {
            client_instance_id: response.lease.client_instance_id,
            rom_world_id: response.active_world_id,
            manifest: compatibility,
            trusted_manifest_key: key(),
            epoch_store,
            workspace_parent: root.join(format!("sessions-{}", auth.character_id)),
            bridge_lua_dir: bridge.clone(),
        };
        world_intent = Some((intent, request));
        portal_catalog = Some(catalog);
        (
            SessionLifecycle::from_world_lease_with_keychain(
                &api,
                auth,
                config,
                vault.clone(),
                response,
            )
            .await,
            rom_path,
        )
    };
    let mut session = match acquired {
        Ok(session) => session,
        Err(error) => {
            let detail = match &error {
                coop_launcher::session::SessionError::Epoch(epoch) => format!("{epoch:?}"),
                _ => error.to_string(),
            };
            return Err(RunError::new(
                code_for_session_error(&error),
                format!("No se pudo adquirir/reanudar: {detail}"),
            ));
        }
    };
    let portal_journal_result: Result<Option<RomTravelJournal>, RunError> = async {
        if let Some(catalog) = portal_catalog.as_ref() {
            let journal = RomTravelJournal::new(
                root.join("travel")
                    .join(session.auth.character_id.to_string()),
                session.auth.character_id,
                catalog.world_ids(),
            )
            .map_err(|_| RunError::internal("Historial de viaje no disponible"))?;
            let record = journal
                .read()
                .map_err(|_| RunError::internal("Historial de viaje inválido"))?;
            match record {
                None => {
                    journal
                        .initialize(session.rom_world_id())
                        .map_err(|_| RunError::internal("No se pudo iniciar historial de viaje"))?;
                }
                Some(record)
                    if record.phase == TravelPhase::ArrivalAcknowledged
                        && record.destination_world == Some(session.rom_world_id())
                        && record.active_world != session.rom_world_id() =>
                {
                    commit_acknowledged_handoff(&api, &session.auth, &journal, None)
                        .await
                        .map_err(|_| {
                            RunError::internal("No se pudo reconciliar viaje confirmado")
                        })?;
                }
                Some(record) if record.active_world != session.rom_world_id() => {
                    return Err(RunError::internal(
                        "Región adquirida difiere del viaje local",
                    ));
                }
                Some(record)
                    if matches!(
                        record.phase,
                        TravelPhase::PrepareIntent
                            | TravelPhase::Prepared
                            | TravelPhase::SourceSaved
                            | TravelPhase::DestinationReady
                            | TravelPhase::Launched
                            | TravelPhase::ArrivalAcknowledged
                    ) =>
                {
                    if !matches!(
                        recover_pending_handoff(&api, &session, &journal).await,
                        Ok(StageOutcome::Aborted)
                    ) {
                        return Err(RunError::internal("Viaje pendiente requiere recuperación"));
                    }
                }
                Some(_) => {}
            }
            Ok(Some(journal))
        } else {
            Ok(None)
        }
    }
    .await;
    let portal_journal = match portal_journal_result {
        Ok(journal) => journal,
        Err(error) => {
            let released = session.release_lease_keep_credentials(&api).await.is_ok();
            if released {
                if let Some((intent, request)) = world_intent.as_ref() {
                    let _ = intent.clear_exact(*request);
                }
            }
            return Err(error);
        }
    };
    let (live, live_requests) = live_request_channel();
    session.serve_live_requests(live_requests);
    *recover_lock(&handle.live) = Some(live);
    let revisions = session.observe_revisions();
    handle.revision.store(
        session.revision.value(),
        std::sync::atomic::Ordering::Release,
    );
    let mut revision_task = spawn_revision_forwarder(revisions, handle.clone(), events.clone());
    let (host_tx, mut host_rx) = mpsc::channel::<oneshot::Sender<()>>(1);
    let host_handle = Arc::clone(&handle);
    let host_events = events.clone();
    let host_task = tokio::spawn(async move {
        while let Some(ack) = host_rx.recv().await {
            {
                let mut host = recover_lock(&host_handle.host);
                if host.closed {
                    let _ = ack.send(());
                    continue;
                }
                host.stopped_ack = Some(ack);
            }
            if host_events.send(json!({"type":"stop"})).await.is_err() {
                break;
            }
        }
    });
    macro_rules! portal_step {
        ($operation:expr) => {
            match $operation {
                Ok(value) => value,
                Err(error) => break Err(error),
            }
        };
    }
    let mut can_release = true;
    let outcome: Result<(), RunError> = loop {
        if matches!(*stop.borrow(), 1 | 3) {
            break Ok(());
        }
        if let Err(error) = session.renew_lease_before_child_start(&api).await {
            break Err(RunError::session(
                "No se pudo renovar la sesión antes de iniciar",
                error,
            ));
        }
        let (mut supervisor, descriptor) =
            match EmbeddedSupervisor::start(session.lease.session_epoch.value(), host_tx.clone())
                .await
            {
                Ok(value) => value,
                Err(_) => break Err(RunError::internal("No se pudo iniciar sidecar")),
            };
        recover_lock(&handle.host).closed = false;
        let load = json!({"type":"load","rom":rom_path,
            "world_id":selected_world_id,
            "rom_sha256":selected_rom_sha,
            "build_id":selected_build_id,
            "save":session.workspace.path().join("character.sav"),"bridge":descriptor.bridge(),
            "epoch":session.lease.session_epoch.value(),"revision":session.revision.value(),
            // Only the verified canonical SAV is portable across desktop/Android.
            "signature_verified":session.revision.value()>0});
        let run = if events.send(load).await.is_ok() {
            // Realtime carries presence, Online, pairing, invitations and group
            // travel. The Java bridge pump accepts every realtime message type
            // (guarded by coop-sidecar's android_bridge_parity test).
            session
                .run_until_shutdown_with_realtime_portal(&api, &mut supervisor, async {
                    while *stop.borrow_and_update() == 0 {
                        if stop.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await
                .map_err(|error| RunError::session("Sesión detenida", error))
        } else {
            Err(RunError::internal("Interfaz cerrada"))
        };
        if supervisor.stop_in_place().await.is_err() {
            can_release = false;
            break Err(RunError::internal(
                "No se confirmó la parada del núcleo; recuperación conservada",
            ));
        }
        let run = match run {
            Ok(outcome) => outcome,
            Err(error) => break Err(error),
        };
        if let SessionRunOutcome::PortalTravel(source) = run {
            let (Some(catalog), Some(journal), Some((intent, old_request))) = (
                portal_catalog.as_ref(),
                portal_journal.as_ref(),
                world_intent.as_ref(),
            ) else {
                break Err(RunError::internal("Viaje de región sin catálogo firmado"));
            };
            let record = portal_step!(
                journal
                    .read()
                    .map_err(|_| RunError::internal("Historial de viaje inválido"))
                    .and_then(|record| record
                        .ok_or_else(|| RunError::internal("Historial de viaje ausente")))
            );
            let prepare_key = portal_step!(
                record
                    .prepare_idempotency_key
                    .filter(|_| record.active_world == session.rom_world_id())
                    .or_else(|| IdempotencyKey::new(uuid::Uuid::new_v4()).ok())
                    .ok_or_else(|| RunError::internal("Clave de viaje inválida"))
            );
            let staged =
                match stage_portal_travel(&api, &session, &source, catalog, journal, prepare_key)
                    .await
                {
                    Ok(StageOutcome::Staged(staged)) => staged,
                    _ => break Err(RunError::internal("No se pudo preparar viaje de región")),
                };
            let destination = portal_step!(
                catalog
                    .world(staged.destination_world())
                    .map_err(|_| RunError::internal("Región de destino no firmada"))
            );
            let compatibility = portal_step!(
                BuildCompatibility::load_android(&destination.bridge_path, &destination.rom_path)
                    .map_err(|_| RunError::internal("ROM de destino incompatible"))
            );
            portal_step!(
                destination
                    .check_compatibility(&compatibility)
                    .map_err(|_| RunError::internal("ROM de destino no coincide con catálogo"))
            );
            let destination_workspace = portal_step!(
                SessionWorkspace::create(
                    &root.join(format!("sessions-{}", session.auth.character_id))
                )
                .map_err(|_| RunError::internal("No se pudo crear verificador privado"))
            );
            let fence = LeaseFenceIdentity::new(
                session.lease.session_id,
                session.lease.session_epoch,
                session.lease.client_instance_id,
            );
            let registry = portal_step!(
                session
                    .registry_contract()
                    .map_err(|_| RunError::internal("Registro de guardado inválido"))
            );
            let verified = verify_staged_arrival_with(journal, &staged, fence, registry, |nonce| {
                verify_embedded_arrival(
                    &staged,
                    nonce,
                    &session,
                    destination,
                    &compatibility,
                    &destination_workspace,
                    &handle,
                    &events,
                )
            })
            .await;
            if verified.is_err() {
                break Err(RunError::internal("Llegada a región no verificada"));
            }
            if *stop.borrow() != 0 {
                break Err(RunError::internal("Viaje cancelado antes del commit"));
            }
            let committed = match commit_acknowledged_handoff(
                &api,
                &session.auth,
                journal,
                Some(&destination_workspace),
            )
            .await
            {
                Ok(record) => record,
                Err(_) => {
                    break Err(RunError::internal(
                        "Commit de viaje pendiente de recuperación",
                    ));
                }
            };
            let destination_world = committed
                .destination_world
                .ok_or_else(|| RunError::internal("Destino de viaje ausente"))?;
            // The server commit released the source lease atomically. Never
            // issue the ordinary source release after this point.
            let mut auth = session
                .into_auth_after_committed_handoff(&committed)
                .map_err(|_| RunError::internal("Viaje confirmado no coincide con sesión"))?;
            intent
                .clear_exact(*old_request)
                .map_err(|_| RunError::internal("No se pudo cerrar adquisición anterior"))?;
            let request = pending_world_request(intent, auth.character_id, client_instance_id)?;
            let response = SessionLifecycle::acquire_world_replacing_same_client_with_keychain(
                &api, &mut auth, request, &keychain,
            )
            .await
            .map_err(|error| RunError::session("No se pudo adquirir destino", error))?;
            if response.active_world_id != destination_world {
                if SessionLifecycle::release_preacquired_world_lease(
                    &api, &mut auth, response, &keychain,
                )
                .await
                .is_ok()
                {
                    let _ = intent.clear_exact(request);
                }
                return Err(RunError::internal(
                    "Servidor devolvió otra región tras viaje",
                ));
            }
            let epoch_store = match EpochStore::for_character(&root, auth.character_id) {
                Ok(store) => store,
                Err(_) => {
                    if SessionLifecycle::release_preacquired_world_lease(
                        &api, &mut auth, response, &keychain,
                    )
                    .await
                    .is_ok()
                    {
                        let _ = intent.clear_exact(request);
                    }
                    return Err(RunError::internal("Historial local no disponible"));
                }
            };
            let config = SessionConfig {
                client_instance_id: response.lease.client_instance_id,
                rom_world_id: destination_world,
                manifest: compatibility.clone(),
                trusted_manifest_key: key(),
                epoch_store,
                workspace_parent: root.join(format!("sessions-{}", auth.character_id)),
                bridge_lua_dir: bridge.clone(),
            };
            session = SessionLifecycle::from_world_lease_with_keychain(
                &api,
                auth,
                config,
                vault.clone(),
                response,
            )
            .await
            // SessionLifecycle releases a failed preacquired lease where
            // safe; preserve the durable request if that is uncertain.
            .map_err(|error| RunError::session("No se pudo cargar destino", error))?;
            // The source live-request receiver belongs to the consumed session.
            // Route pairing requests to the newly acquired destination owner.
            let (live, live_requests) = live_request_channel();
            session.serve_live_requests(live_requests);
            *recover_lock(&handle.live) = Some(live);
            world_intent = Some((intent.clone(), request));
            selected_world_id = destination_world.get();
            selected_rom_sha = compatibility.target.rom_sha256.as_hex();
            selected_build_id = compatibility.target.game_build_id.value().to_owned();
            rom_path = destination.rom_path.clone();
            revision_task.abort();
            let _ = revision_task.await;
            // The destination has a fresh revision stream and runtime lease.
            let revisions = session.observe_revisions();
            handle
                .revision
                .store(session.revision.value(), Ordering::Release);
            revision_task = spawn_revision_forwarder(revisions, handle.clone(), events.clone());
            continue;
        }
        if *stop.borrow() != 2 {
            break Ok(());
        }
        // Consume only this request; a concurrent close always takes priority.
        handle.stop.send_if_modified(|command| {
            if *command == 2 {
                *command = 0;
                true
            } else {
                false
            }
        });
        if *stop.borrow() == 1 {
            break Ok(());
        }
        // The server accepts reconnect only after expiry, inside its grace
        // window. The old core, realtime owner and heartbeat loop are stopped.
        // Do not release the lease: reconnect must retain its session identity.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| RunError::internal("Reloj del dispositivo inválido"))?
            .as_millis() as u64;
        let wait_ms = session
            .lease
            .expires_at
            .value()
            .saturating_sub(now)
            .saturating_add(250);
        let _ = events
            .send(json!({"type":"reconnect_wait","wait_ms":wait_ms}))
            .await;
        tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {},
            () = async {
                while !matches!(*stop.borrow_and_update(), 1 | 3) {
                    if stop.changed().await.is_err() { break; }
                }
            } => break Ok(()),
        }
        if let Err(error) = session.reconnect_embedded(&api, &supervisor).await {
            break Err(RunError::session("No se pudo reconectar", error));
        }
    };
    let revision = session.revision.value();
    let recovery = session.workspace.path().to_path_buf();
    // An unresolved handoff still needs the refresh credential to reconcile
    // or abort its exact stage on the next launch.
    let travel_pending = portal_journal
        .as_ref()
        .is_some_and(|journal| match journal.read() {
            Ok(Some(record)) => matches!(
                record.phase,
                TravelPhase::PrepareIntent
                    | TravelPhase::Prepared
                    | TravelPhase::SourceSaved
                    | TravelPhase::DestinationReady
                    | TravelPhase::Launched
                    | TravelPhase::ArrivalAcknowledged
                    | TravelPhase::AbortPending
            ),
            Ok(None) => false,
            Err(_) => true,
        });
    let signed_out = *stop.borrow() == 3 && !travel_pending;
    let release = if can_release && signed_out {
        session.release(&api).await
    } else if can_release {
        session.release_lease_keep_credentials(&api).await
    } else {
        let _ = session.preserve_recovery_after_child_failure();
        session.close_credentials(&api).await
    };
    *recover_lock(&handle.live) = None;
    revision_task.abort();
    let _ = revision_task.await;
    host_task.abort();
    let _ = host_task.await;
    if let Err(error) = outcome {
        return Err(error.with_recovery_context(revision, &recovery));
        // (recovery context applied above; error code preserved)
        // recovery.display()
        // ));
    }
    release.map_err(|error| RunError::session("Cierre pendiente", error))?;
    if let Some((intent, request)) = world_intent {
        intent.clear_exact(request).map_err(|error| {
            RunError::internal(format!("No se pudo cerrar adquisición: {error}"))
        })?;
    }
    let _ = events
        .send(json!({"type":"closed","revision":revision,"signed_out":signed_out}))
        .await;
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_start(
    mut env: JNIEnv,
    _: JClass,
    root: JString,
    user: JString,
    password: JString,
    user_id: JString,
    character_id: JString,
    catalog_sha256: JString,
    resume: jboolean,
    logout_only: jboolean,
) -> jboolean {
    match std::panic::catch_unwind(AssertUnwindSafe(move || {
        let read = |env: &mut JNIEnv, value: &JString| {
            env.get_string(value)
                .map(|v| v.to_string_lossy().into_owned())
        };
        let Ok(credential_class) = env
            .find_class("io/hoenn/sessions/SecureCredentialStore")
            .and_then(|class| env.new_global_ref(class))
        else {
            return 0;
        };
        let (
            Ok(root),
            Ok(user),
            Ok(password),
            Ok(user_id),
            Ok(character_id),
            Ok(catalog_sha256),
            Ok(vm),
        ) = (
            read(&mut env, &root),
            read(&mut env, &user),
            read(&mut env, &password),
            read(&mut env, &user_id),
            read(&mut env, &character_id),
            read(&mut env, &catalog_sha256),
            env.get_java_vm(),
        )
        else {
            return 0;
        };
        let mut slot = recover_lock(active());
        if slot
            .as_ref()
            .is_some_and(|h| !h.finished.load(std::sync::atomic::Ordering::Acquire))
        {
            return 0;
        }
        let (stop, rx) = watch::channel(0);
        let (tx, events) = mpsc::channel(16);
        let handle = Arc::new(Handle {
            stop,
            sink: tx.clone(),
            events: Mutex::new(events),
            host: Mutex::new(HostState {
                closed: true,
                stopped_ack: None,
            }),
            revision: std::sync::atomic::AtomicU64::new(0),
            finished: std::sync::atomic::AtomicBool::new(false),
            verifier_ack: Mutex::new(None),
            live: Mutex::new(None),
        });
        *slot = Some(handle.clone());
        std::thread::spawn(move || {
            let guarded = std::panic::catch_unwind(AssertUnwindSafe(|| {
                match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(async {
                        if let Err(error) = run(
                            PathBuf::from(root),
                            user,
                            Zeroizing::new(password),
                            user_id,
                            character_id,
                            catalog_sha256,
                            resume != 0,
                            logout_only != 0,
                            Arc::new(vm),
                            credential_class,
                            handle.clone(),
                            tx.clone(),
                            rx,
                        )
                        .await
                        {
                            let _ = tx.send(error.event()).await;
                        }
                    }),
                    Err(_) => {
                        let _ = tx
                            .blocking_send(RunError::internal("No se pudo crear runtime").event());
                    }
                }
            }));
            if guarded.is_err() {
                let _ = handle
                    .sink
                    .try_send(error_event(ERROR_INTERNAL, "Pánico en la sesión nativa"));
            }
            handle
                .finished
                .store(true, std::sync::atomic::Ordering::Release);
        });
        1
    })) {
        Ok(value) => value,
        Err(_) => {
            queue_internal_error("panic en start");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_poll(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    match std::panic::catch_unwind(AssertUnwindSafe(move || {
        let handle = recover_lock(active()).clone();
        let Some(handle) = handle else {
            return std::ptr::null_mut();
        };
        let Ok(value) = recover_lock(&handle.events).try_recv() else {
            return std::ptr::null_mut();
        };
        env.new_string(value.to_string())
            .map_or(std::ptr::null_mut(), |s| s.into_raw())
    })) {
        Ok(value) => value,
        Err(_) => {
            queue_internal_error("panic en poll");
            std::ptr::null_mut()
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_stop(_: JNIEnv, _: JClass) {
    if std::panic::catch_unwind(AssertUnwindSafe(stop_inner)).is_err() {
        queue_internal_error("panic en stop");
    }
}

fn stop_inner() {
    if let Some(handle) = recover_lock(active()).as_ref() {
        let _ = handle.stop.send(1);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_acknowledgeStopped(
    _: JNIEnv,
    _: JClass,
) {
    if std::panic::catch_unwind(AssertUnwindSafe(acknowledge_stopped_inner)).is_err() {
        queue_internal_error("panic en acknowledgeStopped");
    }
}

fn acknowledge_stopped_inner() {
    if let Some(handle) = recover_lock(active()).as_ref() {
        let mut host = recover_lock(&handle.host);
        host.closed = true;
        if let Some(ack) = host.stopped_ack.take() {
            let _ = ack.send(());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_arrivalVerifierClosed(
    _: JNIEnv,
    _: JClass,
    verification_id: jlong,
    success: jboolean,
    _reason: JString,
) {
    if std::panic::catch_unwind(AssertUnwindSafe(|| {
        if verification_id <= 0 {
            return;
        }
        if let Some(handle) = recover_lock(active()).as_ref() {
            let mut pending = recover_lock(&handle.verifier_ack);
            if pending
                .as_ref()
                .is_some_and(|(id, _)| *id == verification_id as u64)
            {
                if let Some((_, ack)) = pending.take() {
                    let _ = ack.send(success != 0);
                }
            }
        }
    }))
    .is_err()
    {
        queue_internal_error("panic en arrivalVerifierClosed");
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_isActive(
    _: JNIEnv,
    _: JClass,
) -> jboolean {
    match std::panic::catch_unwind(AssertUnwindSafe(|| {
        u8::from(
            recover_lock(active())
                .as_ref()
                .is_some_and(|h| !h.finished.load(std::sync::atomic::Ordering::Acquire)),
        )
    })) {
        Ok(value) => value,
        Err(_) => {
            queue_internal_error("panic en isActive");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_reconnect(
    _: JNIEnv,
    _: JClass,
) -> jboolean {
    match std::panic::catch_unwind(AssertUnwindSafe(reconnect_inner)) {
        Ok(value) => value,
        Err(_) => {
            queue_internal_error("panic en reconnect");
            0
        }
    }
}

fn reconnect_inner() -> jboolean {
    if let Some(handle) = recover_lock(active()).as_ref() {
        if !handle.finished.load(std::sync::atomic::Ordering::Acquire)
            && handle.revision.load(std::sync::atomic::Ordering::Acquire) > 0
        {
            return u8::from(handle.stop.send_if_modified(|command| {
                if *command == 0 {
                    *command = 2;
                    true
                } else {
                    false
                }
            }));
        }
    }
    0
}

/// Event the app shows after a join-by-code attempt.
fn pairing_event(result: Result<(), LiveRequestError>) -> Value {
    let outcome = match result {
        Ok(()) => "joined",
        Err(LiveRequestError::Refused) => "refused",
        Err(LiveRequestError::Unavailable) => "unavailable",
        Err(LiveRequestError::NotRunning) => "not_running",
    };
    json!({"type":"pairing_redeemed","result":outcome})
}

/// Redeems a pairing code (or `hoenn-sessions://join/` link) through the
/// running session. Returns false when the text is not a code or no session
/// is running; otherwise the outcome arrives later as a `pairing_redeemed`
/// event from `poll`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_redeemPairingCode(
    mut env: JNIEnv,
    _: JClass,
    text: JString,
) -> jboolean {
    match std::panic::catch_unwind(AssertUnwindSafe(move || {
        let Ok(text) = env
            .get_string(&text)
            .map(|v| v.to_string_lossy().into_owned())
        else {
            return 0;
        };
        u8::from(redeem_inner(&text))
    })) {
        Ok(value) => value,
        Err(_) => {
            queue_internal_error("panic en redeemPairingCode");
            0
        }
    }
}

fn redeem_inner(text: &str) -> bool {
    let Some(code) = coop_cloud::pairing_code_from_join_text(text) else {
        return false;
    };
    let Some(handle) = recover_lock(active()).clone() else {
        return false;
    };
    let Some(live) = recover_lock(&handle.live).clone() else {
        return false;
    };
    let (reply, answer) = oneshot::channel();
    if live
        .try_send(LiveRequest::RedeemPairingCode { code, reply })
        .is_err()
    {
        return false;
    }
    std::thread::spawn(move || {
        let result = answer
            .blocking_recv()
            .unwrap_or(Err(LiveRequestError::NotRunning));
        let _ = handle.sink.blocking_send(pairing_event(result));
    });
    true
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_signOut(_: JNIEnv, _: JClass) {
    if std::panic::catch_unwind(AssertUnwindSafe(sign_out_inner)).is_err() {
        queue_internal_error("panic en signOut");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_launcher::{AuthError, EpochError};

    #[test]
    fn android_world_request_replays_exact_key_and_rejects_other_client() {
        let root =
            std::env::temp_dir().join(format!("android-world-acquire-{}", uuid::Uuid::new_v4()));
        let character = CharacterId::new(uuid::Uuid::new_v4()).unwrap();
        let client = ClientInstanceId::new(uuid::Uuid::new_v4()).unwrap();
        let other_client = ClientInstanceId::new(uuid::Uuid::new_v4()).unwrap();
        let store = WorldAcquireIntentStore::new(&root, character).unwrap();
        let first = pending_world_request(&store, character, client).unwrap();
        assert!(first.replace_same_client);
        assert_eq!(
            pending_world_request(&store, character, client).unwrap(),
            first
        );
        assert!(pending_world_request(&store, character, other_client).is_err());
        assert_eq!(store.read().unwrap().unwrap().request, first);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_client_file_recovers_id_from_durable_world_intent() {
        let root =
            std::env::temp_dir().join(format!("android-client-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let character = CharacterId::new(uuid::Uuid::new_v4()).unwrap();
        let first = client_instance_for_run(&root, character, false).unwrap();
        let store = WorldAcquireIntentStore::new(root.join("world-acquire"), character).unwrap();
        let request = pending_world_request(&store, character, first).unwrap();
        std::fs::remove_file(root.join("client-instance.txt")).unwrap();
        assert_eq!(
            client_instance_for_run(&root, character, false).unwrap(),
            first
        );
        std::fs::write(root.join("client-instance.txt"), "truncated").unwrap();
        assert_eq!(
            client_instance_for_run(&root, character, false).unwrap(),
            first
        );
        assert_eq!(store.read().unwrap().unwrap().request, request);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lease_family_maps_to_lease_conflict() {
        for error in [
            SessionError::AcquireConflict,
            SessionError::Lease,
            SessionError::FinalizeConflict,
            SessionError::CheckpointNotAuthorized,
        ] {
            assert_eq!(code_for_session_error(&error), "lease_conflict");
        }
    }

    #[test]
    fn transport_failures_map_to_cloud_unreachable() {
        assert_eq!(
            code_for_session_error(&SessionError::Cloud),
            "cloud_unreachable"
        );
        assert_eq!(
            code_for_session_error(&SessionError::Auth(AuthError::Transport)),
            "cloud_unreachable"
        );
        assert_eq!(
            code_for_auth_error(&AuthError::Transport),
            "cloud_unreachable"
        );
    }

    #[test]
    fn unexpected_failures_map_to_internal_error() {
        for error in [
            SessionError::Unauthorized,
            SessionError::ArtifactNotFound,
            SessionError::Package,
            SessionError::History,
            SessionError::Realtime,
            SessionError::CheckpointTimeout,
            SessionError::CheckpointCorrelation,
            SessionError::MissingPackage,
            SessionError::InvalidBootstrap,
            SessionError::CorruptActiveSav,
            SessionError::Epoch(EpochError::Corrupt),
            SessionError::Auth(AuthError::InvalidCredentials),
            SessionError::Auth(AuthError::RefreshExpired),
            SessionError::RealtimeStatus {
                kind: "overflow",
                message: "realtime lag",
            },
            SessionError::PresenceRecovery {
                transport_failure: true,
            },
        ] {
            assert_eq!(code_for_session_error(&error), "internal_error");
        }
    }

    #[test]
    fn pairing_event_names_each_outcome() {
        assert_eq!(pairing_event(Ok(()))["result"], "joined");
        assert_eq!(
            pairing_event(Err(LiveRequestError::Refused))["result"],
            "refused"
        );
        assert_eq!(
            pairing_event(Err(LiveRequestError::Unavailable))["type"],
            "pairing_redeemed"
        );
        assert!(!redeem_inner("not a code"));
        // A valid code with no running session is refused locally.
        assert!(!redeem_inner("hoenn-sessions://join/ABC-234"));
    }

    #[test]
    fn error_event_carries_stable_code() {
        let event = RunError::session("prefijo", SessionError::Lease).event();
        assert_eq!(event["type"], "error");
        assert_eq!(event["code"], "lease_conflict");
        assert!(event["message"].as_str().unwrap().starts_with("prefijo"));
    }

    #[test]
    fn poisoned_mutex_recovers_with_into_inner() {
        let mutex = Mutex::new(7u32);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().unwrap();
            panic!("poison a proposito");
        }));
        assert!(result.is_err());
        assert!(mutex.is_poisoned());
        assert_eq!(*recover_lock(&mutex), 7);
    }
}

fn sign_out_inner() {
    if let Some(handle) = recover_lock(active()).as_ref() {
        let _ = handle.stop.send(3);
    }
}

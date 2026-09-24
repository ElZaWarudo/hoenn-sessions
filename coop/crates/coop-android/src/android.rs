use coop_cloud::{
    CharacterId, ClientInstanceId, Password, RefreshToken, TrustedManifestKey, UserId,
};
use coop_launcher::keychain::{KeychainError, RefreshTokenStore};
use coop_launcher::process::{SessionSupervisor, embedded::EmbeddedSupervisor};
use coop_launcher::{
    AuthError, AuthSession, BuildCompatibility, EpochStore, ReqwestCloudApi, SessionConfig,
    SessionError, SessionLifecycle,
};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JString, JValue},
    sys::{jboolean, jstring},
};
use serde_json::{Value, json};
use std::{
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
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

// Classified session failure: a stable machine-readable `code` plus the
// human-readable detail kept for display.
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
}
struct HostState {
    closed: bool,
    stopped_ack: Option<oneshot::Sender<()>>,
}
static ACTIVE: OnceLock<Mutex<Option<Arc<Handle>>>> = OnceLock::new();
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

async fn run(
    root: PathBuf,
    user: String,
    password: Zeroizing<String>,
    user_id: String,
    character_id: String,
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
    let runtime = runtime_directory(&root)?;
    let manifest = BuildCompatibility::load_android(
        &runtime.join("bridge_manifest.json"),
        &runtime.join("pokeemerald.gba"),
    )
    .map_err(|_| RunError::internal("ROM/manifiesto incompatible"))?;
    let api = ReqwestCloudApi::new(SERVER).map_err(|_| RunError::internal("Endpoint inválido"))?;
    let vault = Arc::new(AndroidTokens {
        vm,
        class: credential_class,
    });
    let bridge = root.join("bridge");
    std::fs::create_dir_all(&bridge)
        .map_err(|_| RunError::internal("No se pudo crear el directorio bridge"))?;
    let instance_file = root.join("client-instance.txt");
    let instance = if instance_file.exists() {
        std::fs::read_to_string(&instance_file)
            .map_err(|_| RunError::internal("Identidad local ilegible"))?
    } else {
        let value = uuid::Uuid::new_v4().to_string();
        std::fs::write(&instance_file, &value)
            .map_err(|_| RunError::internal("No se pudo guardar identidad local"))?;
        value
    };
    let client_instance_id = serde_json::from_value::<ClientInstanceId>(json!(instance))
        .map_err(|_| RunError::internal("Identidad local inválida"))?;
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
    let epoch_store = match EpochStore::for_character(&root, auth.character_id) {
        Ok(store) => store,
        Err(error) => {
            return Err(RunError::internal(format!(
                "Historial local de sesión no disponible: {error}"
            )));
        }
    };
    let config = SessionConfig {
        client_instance_id,
        manifest,
        trusted_manifest_key: key(),
        epoch_store,
        workspace_parent: root.join(format!("sessions-{}", auth.character_id)),
        bridge_lua_dir: bridge,
    };
    let acquired =
        SessionLifecycle::acquire_replacing_same_client(&api, auth, config, vault.clone()).await;
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
    let mut revisions = session.observe_revisions();
    handle.revision.store(
        session.revision.value(),
        std::sync::atomic::Ordering::Release,
    );
    let revision_handle = handle.clone();
    let revision_events = events.clone();
    let revision_task = tokio::spawn(async move {
        while revisions.changed().await.is_ok() {
            let revision = *revisions.borrow_and_update();
            revision_handle
                .revision
                .store(revision, std::sync::atomic::Ordering::Release);
            if revision_events
                .send(json!({"type":"saved","revision":revision}))
                .await
                .is_err()
            {
                break;
            }
        }
    });
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
        let load = json!({"type":"load","rom":runtime.join("pokeemerald.gba"),
            "save":session.workspace.path().join("character.sav"),"bridge":descriptor.bridge(),
            "epoch":session.lease.session_epoch.value(),"revision":session.revision.value(),
            // Only the verified canonical SAV is portable across desktop/Android.
            "signature_verified":session.revision.value()>0});
        let run = if events.send(load).await.is_ok() {
            // Embedded mGBA currently reboots the ROM bridge as soon as the
            // first moving presence update enters the realtime lifecycle.
            // Keep Android gameplay and cloud checkpoints alive through the
            // proven fenced lifecycle until embedded realtime can survive
            // ordinary overworld movement.
            session
                .run_until_shutdown(&api, &mut supervisor, async {
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
        if let Err(error) = run {
            break Err(error);
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
    let signed_out = *stop.borrow() == 3;
    let release = if can_release && signed_out {
        session.release(&api).await
    } else if can_release {
        session.release_lease_keep_credentials(&api).await
    } else {
        let _ = session.preserve_recovery_after_child_failure();
        session.close_credentials(&api).await
    };
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
        let (Ok(root), Ok(user), Ok(password), Ok(user_id), Ok(character_id), Ok(vm)) = (
            read(&mut env, &root),
            read(&mut env, &user),
            read(&mut env, &password),
            read(&mut env, &user_id),
            read(&mut env, &character_id),
            env.get_java_vm(),
        ) else {
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

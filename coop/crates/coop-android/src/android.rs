use coop_cloud::{ClientInstanceId, Password, RefreshToken, TrustedManifestKey};
use coop_launcher::keychain::{KeychainError, RefreshTokenStore};
use coop_launcher::process::{SessionSupervisor, embedded::EmbeddedSupervisor};
use coop_launcher::{
    AuthSession, BuildCompatibility, EpochStore, ReqwestCloudApi, SessionConfig, SessionLifecycle,
};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jstring},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::Zeroizing;

const SERVER: &str = "https://169-128-190-115.sslip.io";
const KEY: &str = "f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188";

// Session-only tokens. No file/environment fallback and no credentials survive
// process death. Reopening requires login; server refresh rotation still works.
#[derive(Default)]
struct VolatileTokens(Mutex<Option<RefreshToken>>);
impl RefreshTokenStore for VolatileTokens {
    fn load(&self, _: &str, _: &str) -> Result<Option<RefreshToken>, KeychainError> {
        Ok(self.0.lock().map_err(|_| KeychainError::Operation)?.clone())
    }
    fn store(&self, _: &str, _: &str, token: &RefreshToken) -> Result<(), KeychainError> {
        *self.0.lock().map_err(|_| KeychainError::Operation)? = Some(token.clone());
        Ok(())
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
        *self.0.lock().map_err(|_| KeychainError::Operation)? = None;
        Ok(())
    }
}

struct Handle {
    stop: watch::Sender<u8>, // 0 running, 1 close, 2 reconnect from cloud save
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

async fn run(
    root: PathBuf,
    user: String,
    password: Zeroizing<String>,
    handle: Arc<Handle>,
    events: mpsc::Sender<Value>,
    mut stop: watch::Receiver<u8>,
) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|_| "Directorio privado inválido")?;
    let manifest = BuildCompatibility::load_android(
        &root.join("bridge_manifest.json"),
        &root.join("pokeemerald.gba"),
    )
    .map_err(|_| "ROM/manifiesto incompatible")?;
    let api = ReqwestCloudApi::new(SERVER).map_err(|_| "Endpoint inválido")?;
    let vault = Arc::new(VolatileTokens::default());
    let bridge = root.join("bridge");
    std::fs::create_dir_all(&bridge).map_err(|_| "No se pudo crear el directorio bridge")?;
    let instance_file = root.join("client-instance.txt");
    let instance = if instance_file.exists() {
        std::fs::read_to_string(&instance_file).map_err(|_| "Identidad local ilegible")?
    } else {
        let value = uuid::Uuid::new_v4().to_string();
        std::fs::write(&instance_file, &value).map_err(|_| "No se pudo guardar identidad local")?;
        value
    };
    let config = SessionConfig {
        client_instance_id: serde_json::from_value::<ClientInstanceId>(json!(instance))
            .map_err(|_| "Identidad local inválida")?,
        manifest,
        trusted_manifest_key: key(),
        epoch_store: EpochStore::new(root.join("epoch.json")),
        workspace_parent: root.join("sessions"),
        bridge_lua_dir: bridge,
    };
    let auth = AuthSession::login(
        &api,
        vault.as_ref(),
        user,
        Password::new(password.to_string()).map_err(|_| "Contraseña inválida")?,
    )
    .await
    .map_err(|_| "Login rechazado o red no disponible")?;
    drop(password);
    let acquired = SessionLifecycle::acquire_with_keychain(&api, auth, config, vault.clone()).await;
    let mut session = match acquired {
        Ok(session) => session,
        Err(error) => {
            let detail = match &error {
                coop_launcher::session::SessionError::Epoch(epoch) => format!("{epoch:?}"),
                _ => error.to_string(),
            };
            if let Ok(Some(token)) = vault.load("", "") {
                let _ = coop_launcher::AuthApi::logout(&api, coop_cloud::LogoutRequest::new(token))
                    .await;
            }
            let _ = vault.delete("", "");
            return Err(format!("No se pudo adquirir/reanudar: {detail}"));
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
                let mut host = host_handle.host.lock().expect("host lock");
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
    let outcome: Result<(), String> = loop {
        if *stop.borrow() == 1 {
            break Ok(());
        }
        if session.renew_lease_before_child_start(&api).await.is_err() {
            break Err("No se pudo renovar la sesión antes de iniciar".into());
        }
        let (mut supervisor, descriptor) =
            match EmbeddedSupervisor::start(session.lease.session_epoch.value(), host_tx.clone())
                .await
            {
                Ok(value) => value,
                Err(_) => break Err("No se pudo iniciar sidecar".into()),
            };
        handle.host.lock().expect("host lock").closed = false;
        let load = json!({"type":"load","rom":root.join("pokeemerald.gba"),
            "save":session.workspace.path().join("character.sav"),"bridge":descriptor.bridge(),
            "epoch":session.lease.session_epoch.value(),"revision":session.revision.value(),
            // Only the verified canonical SAV is portable across desktop/Android.
            "signature_verified":session.revision.value()>0});
        let run = if events.send(load).await.is_ok() {
            session
                .run_until_shutdown_with_realtime(&api, &mut supervisor, async {
                    while *stop.borrow_and_update() == 0 {
                        if stop.changed().await.is_err() {
                            break;
                        }
                    }
                })
                .await
                .map_err(|e| format!("Sesión detenida: {e}"))
        } else {
            Err("Interfaz cerrada".into())
        };
        if supervisor.stop_in_place().await.is_err() {
            can_release = false;
            break Err("No se confirmó la parada del núcleo; recuperación conservada".into());
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
            .map_err(|_| "Reloj del dispositivo inválido")?
            .as_millis() as u64;
        let wait_ms = session.lease.expires_at.value().saturating_sub(now).saturating_add(250);
        let _ = events.send(json!({"type":"reconnect_wait","wait_ms":wait_ms})).await;
        tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {},
            () = async {
                while *stop.borrow_and_update() != 1 {
                    if stop.changed().await.is_err() { break; }
                }
            } => break Ok(()),
        }
        if let Err(error) = session.reconnect_embedded(&api, &supervisor).await {
            break Err(format!("No se pudo reconectar: {error}"));
        }
    };
    let revision = session.revision.value();
    let recovery = session.workspace.path().to_path_buf();
    let release = if can_release {
        session.release(&api).await
    } else {
        let _ = session.preserve_recovery_after_child_failure();
        session.close_credentials(&api).await
    };
    revision_task.abort();
    let _ = revision_task.await;
    host_task.abort();
    let _ = host_task.await;
    if let Err(error) = outcome {
        return Err(format!(
            "{error}. Revisión cloud {revision}; recuperación, si procede: {}",
            recovery.display()
        ));
    }
    release.map_err(|e| format!("Cierre pendiente: {e}"))?;
    let _ = events
        .send(json!({"type":"closed","revision":revision}))
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
) -> jboolean {
    let read = |env: &mut JNIEnv, value: &JString| {
        env.get_string(value)
            .map(|v| v.to_string_lossy().into_owned())
    };
    let (Ok(root), Ok(user), Ok(password)) = (
        read(&mut env, &root),
        read(&mut env, &user),
        read(&mut env, &password),
    ) else {
        return 0;
    };
    let mut slot = active().lock().expect("session lock");
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
        match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(async {
                if let Err(message) = run(
                    PathBuf::from(root),
                    user,
                    Zeroizing::new(password),
                    handle.clone(),
                    tx.clone(),
                    rx,
                )
                .await
                {
                    let _ = tx.send(json!({"type":"error","message":message})).await;
                }
            }),
            Err(_) => {
                let _ =
                    tx.blocking_send(json!({"type":"error","message":"No se pudo crear runtime"}));
            }
        }
        handle
            .finished
            .store(true, std::sync::atomic::Ordering::Release);
    });
    1
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_poll(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    let handle = active().lock().expect("session lock").clone();
    let Some(handle) = handle else {
        return std::ptr::null_mut();
    };
    let Ok(value) = handle.events.lock().expect("event lock").try_recv() else {
        return std::ptr::null_mut();
    };
    env.new_string(value.to_string())
        .map_or(std::ptr::null_mut(), |s| s.into_raw())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_stop(_: JNIEnv, _: JClass) {
    if let Some(handle) = active().lock().expect("session lock").as_ref() {
        let _ = handle.stop.send(1);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_acknowledgeStopped(
    _: JNIEnv,
    _: JClass,
) {
    if let Some(handle) = active().lock().expect("session lock").as_ref() {
        let mut host = handle.host.lock().expect("host lock");
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
    u8::from(
        active()
            .lock()
            .expect("session lock")
            .as_ref()
            .is_some_and(|h| !h.finished.load(std::sync::atomic::Ordering::Acquire)),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_reconnect(
    _: JNIEnv,
    _: JClass,
) -> jboolean {
    if let Some(handle) = active().lock().expect("session lock").as_ref() {
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

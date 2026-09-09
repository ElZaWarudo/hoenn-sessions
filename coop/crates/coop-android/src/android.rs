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
    stop: watch::Sender<bool>,
    events: Mutex<mpsc::Receiver<Value>>,
    stopped_ack: Mutex<Option<oneshot::Sender<()>>>,
    finished: std::sync::atomic::AtomicBool,
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
    mut stop: watch::Receiver<bool>,
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
    let auth = AuthSession::login(
        &api,
        vault.as_ref(),
        user,
        Password::new(password.to_string()).map_err(|_| "Contraseña inválida")?,
    )
    .await
    .map_err(|_| "Login rechazado o red no disponible")?;
    drop(password);
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
    let mut session = SessionLifecycle::acquire_with_keychain(&api, auth, config, vault)
        .await
        .map_err(|e| format!("No se pudo adquirir/reanudar: {e}"))?;
    let (host_tx, mut host_rx) = mpsc::channel::<oneshot::Sender<()>>(1);
    let host_handle = Arc::clone(&handle);
    let host_events = events.clone();
    let host_task = tokio::spawn(async move {
        while let Some(ack) = host_rx.recv().await {
            *host_handle.stopped_ack.lock().expect("ack lock") = Some(ack);
            if host_events.send(json!({"type":"stop"})).await.is_err() {
                break;
            }
        }
    });
    let (mut supervisor, descriptor) =
        EmbeddedSupervisor::start(session.lease.session_epoch.value(), host_tx)
            .await
            .map_err(|_| "No se pudo iniciar sidecar")?;
    let save = session.workspace.path().join("character.sav");
    let load = json!({"type":"load","rom":root.join("pokeemerald.gba"),"save":save,
        "manifest":root.join("bridge_manifest.json"),"bridge":descriptor.bridge(),
        "epoch":session.lease.session_epoch.value(),"revision":session.revision.value(),
        // Canonical SAV is portable; stock desktop savestates are deliberately not loaded.
        "signature_verified":session.revision.value()>0});
    if events.send(load).await.is_err() {
        let _ = supervisor.stop_in_place().await;
        return Err("Interfaz cerrada".into());
    }
    let run = session
        .run_until_shutdown_with_realtime(&api, &mut supervisor, async move {
            if !*stop.borrow() {
                let _ = stop.changed().await;
            }
        })
        .await;
    let _ = supervisor.stop_in_place().await;
    let revision = session.revision.value();
    let release = session.release(&api).await;
    drop(supervisor);
    host_task.abort();
    let _ = host_task.await;
    if let Err(error) = run {
        return Err(format!(
            "Sesión detenida, recuperación conservada si procede: {error}"
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
    let (stop, rx) = watch::channel(false);
    let (tx, events) = mpsc::channel(16);
    let handle = Arc::new(Handle {
        stop,
        events: Mutex::new(events),
        stopped_ack: Mutex::new(None),
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
        let _ = handle.stop.send(true);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_hoenn_sessions_NativeSession_acknowledgeStopped(
    _: JNIEnv,
    _: JClass,
) {
    if let Some(handle) = active().lock().expect("session lock").as_ref() {
        if let Some(ack) = handle.stopped_ack.lock().expect("ack lock").take() {
            let _ = ack.send(());
        }
    }
}

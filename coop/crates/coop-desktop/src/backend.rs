//! Single-owner backend actor for authentication, releases, and runtime use.

use std::{
    fs::{self, OpenOptions},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    AcquireLeaseRequest, ApiVersion, ClientInstanceId, GroupRomHandoffAbortRequest,
    GroupRomHandoffJoinRequest, HeartbeatLeaseRequest, IdempotencyKey, InvitationCode,
    OnlineSnapshotRequest, RuntimeBuildIdentity,
};
use coop_launcher::arrival_verifier::{
    ArrivalVerificationInput, AuthenticatedArrivalEvidence, verify_arrival,
};
use coop_launcher::paired_travel::{PairedJoinIntent, PairedPhase, PairedTerminal};
use coop_launcher::{
    AcceptedGeneration, ArtifactIdentity, AuthError, AuthSession, BuildCompatibility, CloudApi,
    CommandSpec, Effect, EpochStore, OsKeychain, RecoveryDiscovery, RecoveryMarker,
    RecoveryOutcome, RecoveryReconciler, RecoveryResult, RefreshTokenStore, ReleaseReadiness,
    SessionConfig, SessionLifecycle, SessionWorkspace, StartFailure, TrustedManifestKey,
    TrustedRomCatalog, UpdateFailure, WorldAcquireIntentStore,
    paired_coordinator::{PairedHandoffOutcome, recover_paired_handoff, run_paired_handoff},
    paired_travel::PairedTravelJournal,
    process::{SessionSupervisor, SupervisedChildren, staged_rom_marker_contents, staged_rom_marker_path},
    rom_travel::{LeaseFenceIdentity, RomTravelJournal, TravelPhase, TravelRecord},
    session::{PortalTravelSource, SessionRunOutcome},
    travel_coordinator::{
        ArrivalVerificationConfig, StageOutcome, commit_acknowledged_handoff,
        recover_pending_handoff, recover_pending_handoff_before_restore, stage_portal_travel,
        verify_staged_arrival,
    },
    update::{GenerationStore, UpdateError},
};
use thiserror::Error;
use tokio::{
    runtime::Runtime,
    sync::{Notify, mpsc as tokio_mpsc, oneshot},
};

use crate::{
    config::{AccountRecord, ConfigError, RuntimeConfig, UserPaths},
    release_client::{DownloadedRelease, ReleaseClient, ReleaseError},
};

pub const BOOTSTRAP_RESTART_CODE: i32 = 10;
const KEYCHAIN_SERVICE: &str = "pokecrossroads-coop-launcher";

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("desktop configuration is unavailable")]
    Config(#[from] ConfigError),
    #[error("cloud endpoint is unavailable")]
    Endpoint,
    #[error("release store is unavailable")]
    Store,
    #[error("backend actor is closed")]
    Closed,
}

#[derive(Clone, Debug)]
pub struct BackendConfig {
    pub paths: UserPaths,
    pub runtime: RuntimeConfig,
}

impl BackendConfig {
    pub fn compiled() -> Result<Self, BackendError> {
        let paths = UserPaths::resolve()?;
        paths.ensure_directories()?;
        Ok(Self {
            paths,
            runtime: RuntimeConfig::compiled()?,
        })
    }

    pub fn for_test(paths: UserPaths, runtime: RuntimeConfig) -> Result<Self, BackendError> {
        paths.ensure_directories()?;
        Ok(Self { paths, runtime })
    }
}

#[derive(Debug)]
pub enum BackendEvent {
    AuthenticationSucceeded,
    AuthenticationFailed(coop_launcher::AuthFailure),
    ReleaseCheckFinished(ReleaseReadiness),
    ReleaseCheckFailed(coop_launcher::ServiceFailure),
    UpdateCompleted,
    UpdateFailed(UpdateFailure),
    RestartRequired,
    StartCompleted,
    StartFailed(StartFailure),
    StopCompleted,
    ShutdownUncertain,
    SignOutCompleted,
    SignOutFailed(coop_launcher::SignOutFailure),
    RecoveryReconciled(RecoveryResult),
}

#[derive(Clone)]
pub struct BackendHandle {
    commands: tokio_mpsc::UnboundedSender<BackendCommand>,
    events: Arc<Mutex<mpsc::Receiver<BackendEvent>>>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl BackendHandle {
    pub fn submit(&self, effect: Effect) -> Result<(), BackendError> {
        self.commands
            .send(BackendCommand::Effect(effect))
            .map_err(|_| BackendError::Closed)
    }

    pub fn poll(&self) -> Vec<BackendEvent> {
        let Ok(receiver) = self.events.lock() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn request_shutdown(&self) -> Result<mpsc::Receiver<()>, BackendError> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.commands
            .send(BackendCommand::Shutdown(ack_tx))
            .map_err(|_| BackendError::Closed)?;
        Ok(ack_rx)
    }

    pub fn join(&self) -> bool {
        let Ok(mut thread) = self.thread.lock() else {
            return false;
        };
        thread.take().is_none_or(|thread| thread.join().is_ok())
    }

    pub fn shutdown(&self) -> bool {
        let Ok(ack) = self.request_shutdown() else {
            return self.join();
        };
        ack.recv().is_ok() && self.join()
    }
}

pub fn spawn_backend(config: BackendConfig) -> Result<BackendHandle, BackendError> {
    let api = coop_launcher::ReqwestCloudApi::new(&config.runtime.api_base)
        .map_err(|_| BackendError::Endpoint)?;
    let release = ReleaseClient::new(&config.runtime.api_base, config.runtime.release_key.clone())
        .map_err(|_| BackendError::Endpoint)?;
    let store =
        GenerationStore::new(config.paths.generations_root()).map_err(|_| BackendError::Store)?;
    let pending_credential_cleanup = config.paths.load_cleanup_username()?;
    let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::channel();
    let actor = BackendActor {
        config,
        api,
        release,
        keychain: Arc::new(OsKeychain),
        store,
        auth: None,
        pending_release: None,
        runtime: None,
        commands: command_tx.clone(),
        shutdown_ack: None,
        pending_credential_cleanup,
    };
    let thread = std::thread::Builder::new()
        .name("coop-desktop-backend".into())
        .spawn(move || {
            let Ok(runtime) = Runtime::new() else { return };
            runtime.block_on(actor.run(command_rx, event_tx));
        })
        .map_err(|_| BackendError::Closed)?;
    Ok(BackendHandle {
        commands: command_tx,
        events: Arc::new(Mutex::new(event_rx)),
        thread: Arc::new(Mutex::new(Some(thread))),
    })
}

enum BackendCommand {
    Effect(Effect),
    RuntimeStarted,
    RuntimeFinished(RuntimeCompletion),
    Shutdown(mpsc::Sender<()>),
}

struct RuntimeHandle {
    stop: Option<oneshot::Sender<()>>,
}

type PendingWorldAcquire = (WorldAcquireIntentStore, AcquireLeaseRequest);
type ShutdownFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct PortalRuntime {
    catalog: TrustedRomCatalog,
    journal: RomTravelJournal,
    paired_journal: PairedTravelJournal,
    stop: Arc<Notify>,
}

struct BackendActor {
    config: BackendConfig,
    api: coop_launcher::ReqwestCloudApi,
    release: ReleaseClient,
    keychain: Arc<dyn RefreshTokenStore>,
    store: GenerationStore,
    auth: Option<AuthSession>,
    pending_release: Option<DownloadedRelease>,
    runtime: Option<RuntimeHandle>,
    commands: tokio_mpsc::UnboundedSender<BackendCommand>,
    shutdown_ack: Option<mpsc::Sender<()>>,
    pending_credential_cleanup: Option<String>,
}

struct RuntimeCompletion {
    auth: Option<AuthSession>,
    outcome: RuntimeOutcome,
}

enum RuntimeOutcome {
    StartupFailed(StartFailure),
    Exited { clean_stop: bool },
    Continue(Box<NextRuntime>),
}

struct NextRuntime {
    api: coop_launcher::ReqwestCloudApi,
    session: SessionLifecycle,
    keychain: Arc<dyn RefreshTokenStore>,
    handoff: coop_launcher::GenerationHandoff,
    trusted_manifest_key: TrustedManifestKey,
    epoch_file: PathBuf,
    workspace_parent: PathBuf,
    sidecar_path: PathBuf,
    rom_path: PathBuf,
    mgba_path: PathBuf,
    bridge_path: PathBuf,
    world_intent: Option<PendingWorldAcquire>,
    shutdown: ShutdownFuture,
    portal: Option<PortalRuntime>,
    announce_start: bool,
}

impl BackendActor {
    async fn run(
        mut self,
        mut commands: tokio_mpsc::UnboundedReceiver<BackendCommand>,
        events: mpsc::Sender<BackendEvent>,
    ) {
        while let Some(command) = commands.recv().await {
            match command {
                BackendCommand::Effect(effect) => {
                    self.execute(effect, &events).await;
                }
                BackendCommand::RuntimeStarted => {
                    if self
                        .runtime
                        .as_ref()
                        .is_some_and(|runtime| runtime.stop.is_some())
                    {
                        let _ = events.send(BackendEvent::StartCompleted);
                    }
                }
                BackendCommand::RuntimeFinished(completion) => {
                    let stop_requested = self
                        .runtime
                        .take()
                        .is_some_and(|runtime| runtime.stop.is_none());
                    if completion.auth.is_some() {
                        self.auth = completion.auth;
                    }
                    let _ =
                        events.send(runtime_completion_event(stop_requested, completion.outcome));
                    if let Some(ack) = self.shutdown_ack.take() {
                        let _ = ack.send(());
                        break;
                    }
                }
                BackendCommand::Shutdown(ack) => {
                    if let Some(runtime) = self.runtime.as_mut() {
                        if let Some(stop) = runtime.stop.take() {
                            let _ = stop.send(());
                        }
                        self.shutdown_ack = Some(ack);
                    } else {
                        let _ = ack.send(());
                        break;
                    }
                }
            }
        }
    }

    async fn execute(&mut self, effect: Effect, events: &mpsc::Sender<BackendEvent>) {
        match effect {
            Effect::PromptAuthentication(_) => {}
            Effect::Authenticate(request) => self.authenticate(request, events).await,
            Effect::ResumeSavedSession => self.resume(events).await,
            Effect::CheckRelease => self.check_release(events).await,
            Effect::ApplyUpdate => self.apply_update(events).await,
            Effect::StartRuntime => self.start_runtime(events).await,
            Effect::StopRuntime => self.stop_runtime(events).await,
            Effect::ReconcileRecovery => self.reconcile_recovery(events).await,
            Effect::SignOut => self.sign_out(events).await,
        }
    }

    async fn authenticate(
        &mut self,
        request: coop_launcher::AuthRequest,
        events: &mpsc::Sender<BackendEvent>,
    ) {
        if let Some(mut deferred) = self.auth.take()
            && !self.rollback_auth(&mut deferred).await
        {
            self.auth = Some(deferred);
            send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
            return;
        }
        if self.cleanup_pending_credential().is_err() {
            send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
            return;
        }
        let username = request.username().to_owned();
        let password = match AuthSession::password(request.password().as_str()) {
            Ok(password) => password,
            Err(_) => {
                let _ = events.send(BackendEvent::AuthenticationFailed(
                    coop_launcher::AuthFailure::Rejected,
                ));
                return;
            }
        };
        let result = match request.flow() {
            coop_launcher::AuthFlow::Registration => {
                let Some(invitation) = request.invitation() else {
                    return send_auth_failure(events, coop_launcher::AuthFailure::Rejected);
                };
                let Ok(invitation) = InvitationCode::new(invitation.as_str().to_owned()) else {
                    return send_auth_failure(events, coop_launcher::AuthFailure::Rejected);
                };
                AuthSession::register(
                    &self.api,
                    self.keychain.as_ref(),
                    username,
                    password,
                    invitation,
                )
                .await
            }
            coop_launcher::AuthFlow::SignIn => {
                AuthSession::login(&self.api, self.keychain.as_ref(), username, password).await
            }
            coop_launcher::AuthFlow::Resume => Err(AuthError::Transport),
        };
        match result {
            Ok(auth) => self.accept_auth(auth, events).await,
            Err(error) => send_auth_failure(events, map_auth_failure(&error)),
        }
    }

    async fn accept_auth(&mut self, mut auth: AuthSession, events: &mpsc::Sender<BackendEvent>) {
        let previous = match self.config.paths.load_account() {
            Ok(previous) => previous,
            Err(_) => {
                if !self.rollback_auth(&mut auth).await {
                    self.auth = Some(auth);
                }
                send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
                return;
            }
        };
        let account = AccountRecord {
            username: auth.username.to_string(),
            user_id: auth.user_id,
            character_id: auth.character_id,
        };
        if self.config.paths.save_account(&account).is_err() {
            if !self.rollback_auth(&mut auth).await {
                self.auth = Some(auth);
            }
            send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
            return;
        }
        if let Some(previous) = previous
            && previous.username != account.username
            && self
                .keychain
                .delete(KEYCHAIN_SERVICE, &previous.username)
                .is_err()
        {
            if !self.rollback_auth(&mut auth).await {
                self.auth = Some(auth);
            }
            let _ = self.config.paths.save_account(&previous);
            send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
            return;
        }
        self.auth = Some(auth);
        let _ = events.send(BackendEvent::AuthenticationSucceeded);
    }

    async fn resume(&mut self, events: &mpsc::Sender<BackendEvent>) {
        if self.cleanup_pending_credential().is_err() {
            return send_auth_failure(events, coop_launcher::AuthFailure::Unavailable);
        }
        let account = match self.config.paths.load_account() {
            Ok(Some(account)) => account,
            _ => return send_auth_failure(events, coop_launcher::AuthFailure::SessionExpired),
        };
        let result = AuthSession::refresh_from_keychain(
            &self.api,
            self.keychain.as_ref(),
            account.username,
            account.user_id,
            account.character_id,
        )
        .await;
        match result {
            Ok(auth) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::AuthenticationSucceeded);
            }
            Err(error) => send_auth_failure(events, map_auth_failure(&error)),
        }
    }

    async fn check_release(&mut self, events: &mpsc::Sender<BackendEvent>) {
        let Some(auth) = self.auth.as_ref() else {
            let _ = events.send(BackendEvent::ReleaseCheckFailed(
                coop_launcher::ServiceFailure::Unauthorized,
            ));
            return;
        };
        let latest = match self.release.fetch_latest(auth, false).await {
            Ok(latest) => latest,
            Err(error) => {
                let _ = events.send(BackendEvent::ReleaseCheckFailed(map_service_failure(
                    &error,
                )));
                return;
            }
        };
        let now = match now_seconds() {
            Ok(now) => now,
            Err(()) => {
                let _ = events.send(BackendEvent::ReleaseCheckFailed(
                    coop_launcher::ServiceFailure::NotReady,
                ));
                return;
            }
        };
        let accepted = self
            .store
            .open_accepted_current(self.release.trusted_key(), now);
        let complete = match accepted {
            Ok(current) if current.release_id() == latest.verified.release_id() => {
                self.store.validate_generation(&latest.verified).is_ok()
            }
            Ok(_) => false,
            Err(UpdateError::NoAcceptedGeneration)
            | Err(UpdateError::InvalidCompleteGeneration(_))
            | Err(UpdateError::MalformedEnvelope)
            | Err(UpdateError::MalformedDescriptor)
            | Err(UpdateError::UnsupportedSchema(_))
            | Err(UpdateError::UnsupportedPlatform(_))
            | Err(UpdateError::KeyIdMismatch)
            | Err(UpdateError::SignatureInvalid)
            | Err(UpdateError::InvalidReleaseId(_))
            | Err(UpdateError::ReleaseExpired(_)) => false,
            Err(_) => {
                let _ = events.send(BackendEvent::ReleaseCheckFailed(
                    coop_launcher::ServiceFailure::NotReady,
                ));
                return;
            }
        };
        if complete {
            self.pending_release = None;
            let _ = events.send(BackendEvent::ReleaseCheckFinished(
                ReleaseReadiness::Complete,
            ));
            return;
        }
        match self.release.fetch_latest(auth, true).await {
            Ok(downloaded) => {
                self.pending_release = Some(downloaded);
                let _ = events.send(BackendEvent::ReleaseCheckFinished(
                    ReleaseReadiness::UpdateRequired,
                ));
            }
            Err(error) => {
                let _ = events.send(BackendEvent::ReleaseCheckFailed(map_service_failure(
                    &error,
                )));
            }
        }
    }

    async fn apply_update(&mut self, events: &mpsc::Sender<BackendEvent>) {
        let Some(auth) = self.auth.as_ref() else {
            let _ = events.send(BackendEvent::UpdateFailed(UpdateFailure::ActivationFailed));
            return;
        };
        let pending = match self.pending_release.take() {
            Some(pending) => pending,
            None => match self.release.fetch_latest(auth, true).await {
                Ok(pending) => pending,
                Err(error) => {
                    let _ = events.send(BackendEvent::UpdateFailed(
                        match map_service_failure(&error) {
                            coop_launcher::ServiceFailure::Unauthorized => {
                                UpdateFailure::ActivationFailed
                            }
                            coop_launcher::ServiceFailure::Unavailable
                            | coop_launcher::ServiceFailure::NotReady => UpdateFailure::Unavailable,
                        },
                    ));
                    return;
                }
            },
        };
        let now = match now_seconds() {
            Ok(now) => now,
            Err(()) => {
                let _ = events.send(BackendEvent::UpdateFailed(UpdateFailure::ActivationFailed));
                return;
            }
        };
        let installed = self
            .store
            .install_at(&pending.verified, &pending.artifacts, now);
        let installed = match installed {
            Err(UpdateError::InvalidCompleteGeneration(_)) => {
                self.store
                    .repair_accepted_at(&pending.verified, pending.artifacts, now)
            }
            result => result,
        };
        match installed {
            Ok(_) => {
                // The desktop executable is itself one of the signed artifacts;
                // the stable bootstrapper must select it after this process exits.
                let _ = events.send(BackendEvent::RestartRequired);
            }
            Err(error) => {
                let _ = events.send(BackendEvent::UpdateFailed(map_update_failure(&error)));
            }
        }
    }

    async fn start_runtime(&mut self, events: &mpsc::Sender<BackendEvent>) {
        if self.runtime.is_some() {
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        }
        let auth = match self.take_or_resume_auth().await {
            Ok(auth) => auth,
            Err(_) => {
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let now = match now_seconds() {
            Ok(now) => now,
            Err(()) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let generation = match self
            .store
            .open_accepted_current(self.release.trusted_key(), now)
        {
            Ok(generation) => generation,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                return;
            }
        };
        if generation
            .artifact(ArtifactIdentity::RegionCatalog)
            .is_some()
        {
            self.start_world_runtime(auth, generation, events).await;
            return;
        }
        let Some(manifest) = generation.artifact(ArtifactIdentity::CompatibilityManifest) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let Some(rom) = generation.artifact(ArtifactIdentity::Rom) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let Some(mgba) = generation.artifact(ArtifactIdentity::ManagedMgba) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let Some(sidecar) = generation.artifact(ArtifactIdentity::Sidecar) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let manifest_path = manifest.path().to_owned();
        let rom_path = rom.path().to_owned();
        let mgba_path = mgba.path().to_owned();
        let sidecar_path = sidecar.path().to_owned();
        let compatibility =
            match BuildCompatibility::validate(&manifest_path, &rom_path, &mgba_path) {
                Ok(value) => value,
                Err(_) => {
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                    return;
                }
            };
        let handoff = generation.handoff();
        let api = self.api.clone();
        let keychain = Arc::clone(&self.keychain);
        let paths = self.config.paths.clone();
        let trusted_manifest_key = self.config.runtime.manifest_key.clone();
        let (stop_tx, stop_rx) = oneshot::channel();
        let commands = self.commands.clone();
        let bridge_path = handoff.path().join("bridge");
        tokio::spawn(async move {
            let result = run_runtime(
                api,
                keychain,
                auth,
                handoff,
                compatibility,
                trusted_manifest_key,
                sidecar_path,
                rom_path,
                mgba_path,
                bridge_path,
                paths,
                stop_rx,
                &commands,
            )
            .await;
            let _ = commands.send(BackendCommand::RuntimeFinished(result));
        });
        self.runtime = Some(RuntimeHandle {
            stop: Some(stop_tx),
        });
    }

    async fn start_world_runtime(
        &mut self,
        mut auth: AuthSession,
        generation: AcceptedGeneration,
        events: &mpsc::Sender<BackendEvent>,
    ) {
        let Some(catalog_artifact) = generation.artifact(ArtifactIdentity::RegionCatalog) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let Some(mgba) = generation.artifact(ArtifactIdentity::ManagedMgba) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let Some(sidecar) = generation.artifact(ArtifactIdentity::Sidecar) else {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        };
        let catalog_digest = hex_digest(catalog_artifact.digest());
        let catalog = match TrustedRomCatalog::load(catalog_artifact.path(), &catalog_digest) {
            Ok(catalog) => catalog,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                return;
            }
        };

        let intent =
            match WorldAcquireIntentStore::new(self.config.paths.state_root(), auth.character_id) {
                Ok(intent) => intent,
                Err(_) => {
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                }
            };
        let request = match intent.read() {
            Ok(Some(existing)) => existing.request,
            Ok(None) => match new_world_acquire_request(auth.character_id) {
                Ok(request) => request,
                Err(()) => {
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                }
            },
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let mut request = match intent.load_or_create(request) {
            Ok(request) => request,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let response = match SessionLifecycle::acquire_world_with_keychain(
            &self.api,
            &mut auth,
            request,
            &self.keychain,
        )
        .await
        {
            Ok(response) => response,
            Err(coop_launcher::SessionError::AcquireClosed) => {
                // The server has proved that the durable key can no longer
                // acquire a lease. Clear only that exact request, then
                // persist one replacement before retrying once.
                if intent.clear_exact(request).is_err() {
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                }
                request = match new_world_acquire_request(auth.character_id) {
                    Ok(request) => request,
                    Err(()) => {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                };
                request = match intent.load_or_create(request) {
                    Ok(request) => request,
                    Err(_) => {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                };
                match SessionLifecycle::acquire_world_with_keychain(
                    &self.api,
                    &mut auth,
                    request,
                    &self.keychain,
                )
                .await
                {
                    Ok(response) => response,
                    Err(_) => {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                }
            }
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let journal = match RomTravelJournal::new(
            self.config.paths.state_root().join("travel"),
            auth.character_id,
            catalog.world_ids(),
        ) {
            Ok(journal) => journal,
            Err(_) => {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let paired_path = self.config.paths.state_root().join("paired-travel");
        let paired_journal = PairedTravelJournal::new(&paired_path, auth.character_id);
        if fs::create_dir_all(&paired_path).is_err() || paired_journal.read().is_err() {
            let released = SessionLifecycle::release_preacquired_world_lease(
                &self.api,
                &mut auth,
                response,
                &self.keychain,
            )
            .await
            .is_ok();
            if released {
                let _ = intent.clear_exact(request);
            }
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
            return;
        }
        let mut journal_record = match journal.read() {
            Ok(record) => record,
            Err(_) => {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        // A paired commit can land between the server transaction and the
        // local journal append. Reconcile that durable state before comparing
        // the acquired world with the solo journal. An unresolved paired
        // attempt is deliberately fail-closed: loading either ROM would risk
        // allowing one member to continue from the old world.
        let mut paired_record = match paired_journal.read() {
            Ok(record) => record,
            Err(_) => {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        if let Some(record) = &paired_record {
            if matches!(
                record.phase,
                PairedPhase::JoinIntent
                    | PairedPhase::AttemptKnown
                    | PairedPhase::Staged
                    | PairedPhase::ArrivalVerified
            ) {
                let recovered = recover_paired_handoff(&self.api, &auth, &paired_journal).await;
                if !matches!(
                    recovered,
                    Ok(PairedHandoffOutcome::Aborted | PairedHandoffOutcome::Committed(_))
                ) {
                    let released = SessionLifecycle::release_preacquired_world_lease(
                        &self.api,
                        &mut auth,
                        response,
                        &self.keychain,
                    )
                    .await
                    .is_ok();
                    if released {
                        let _ = intent.clear_exact(request);
                    }
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                }
                paired_record = match paired_journal.read() {
                    Ok(record) => record,
                    Err(_) => {
                        let released = SessionLifecycle::release_preacquired_world_lease(
                            &self.api,
                            &mut auth,
                            response,
                            &self.keychain,
                        )
                        .await
                        .is_ok();
                        if released {
                            let _ = intent.clear_exact(request);
                        }
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                };
            }
        }
        if let Some(record) = &paired_record {
            if matches!(record.phase, PairedPhase::Committed | PairedPhase::Adopted) {
                let Some(PairedTerminal::Committed(commit)) = record.terminal.as_ref() else {
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                };
                if record.phase == PairedPhase::Committed {
                    if response.active_world_id != commit.own_world_id {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                        return;
                    }
                    if journal_record.is_none()
                        && journal.initialize(record.intent.source_world_id).is_err()
                    {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                    if journal.adopt_paired_commit(record).is_err()
                        || paired_journal
                            .record_adopted(record.intent.request.client_intent_key)
                            .is_err()
                    {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                    journal_record = match journal.read() {
                        Ok(record) => record,
                        Err(_) => {
                            self.auth = Some(auth);
                            let _ =
                                events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                            return;
                        }
                    };
                    let solo_adopted = journal_record.as_ref().is_some_and(|solo| {
                        solo.phase == TravelPhase::Committed
                            && solo.active_world == commit.own_world_id
                            && solo.destination_world == Some(commit.own_world_id)
                            && solo.server_stage_snapshot_id == Some(commit.own_snapshot_id)
                    });
                    if !solo_adopted {
                        // The server has already committed this paired handoff.
                        // Preserve the auth fence and lease for recovery instead
                        // of releasing a destination that may still be in use.
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                } else {
                    // An Adopted receipt is historical. The solo journal may
                    // already describe a later ordinary travel, so compare it
                    // with the currently acquired world instead of the old
                    // paired destination. A solo ArrivalAcknowledged record
                    // is also valid: the normal solo recovery below must be
                    // allowed to replay its remote commit.
                    let solo_matches = journal_record.as_ref().is_some_and(|solo| {
                        solo.active_world == response.active_world_id
                            || (solo.phase == TravelPhase::ArrivalAcknowledged
                                && solo.destination_world == Some(response.active_world_id))
                    });
                    if !solo_matches {
                        self.auth = Some(auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                }
            }
        }
        // A crash can occur after the server commits but before the local
        // journal records Committed. A fresh acquire may therefore already
        // return the destination world. Keep that response so the exact
        // commit key can be replayed below instead of rejecting a valid
        // handoff as a world mismatch.
        let remote_committed = journal_record.as_ref().is_some_and(|record| {
            record.phase == TravelPhase::ArrivalAcknowledged
                && record.active_world != response.active_world_id
                && record.destination_world == Some(response.active_world_id)
        });
        let pending_handoff = journal_record.as_ref().is_some_and(|record| {
            matches!(
                record.phase,
                TravelPhase::PrepareIntent
                    | TravelPhase::Prepared
                    | TravelPhase::SourceSaved
                    | TravelPhase::DestinationReady
                    | TravelPhase::Launched
                    | TravelPhase::ArrivalAcknowledged
            ) && !remote_committed
        });
        match journal_record {
            Some(record)
                if record.active_world != response.active_world_id && !remote_committed =>
            {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                return;
            }
            Some(_) => {}
            None => {
                if journal.initialize(response.active_world_id).is_err() {
                    let released = SessionLifecycle::release_preacquired_world_lease(
                        &self.api,
                        &mut auth,
                        response,
                        &self.keychain,
                    )
                    .await
                    .is_ok();
                    if released {
                        let _ = intent.clear_exact(request);
                    }
                    self.auth = Some(auth);
                    let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                    return;
                }
            }
        }
        let settled = if pending_handoff {
            let result =
                recover_pending_handoff_before_restore(&self.api, &auth, &response, &journal).await;
            matches!(result, Ok(StageOutcome::Aborted))
        } else {
            true
        };
        if !settled {
            let released = SessionLifecycle::release_preacquired_world_lease(
                &self.api,
                &mut auth,
                response,
                &self.keychain,
            )
            .await
            .is_ok();
            if released {
                let _ = intent.clear_exact(request);
            }
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
            return;
        }
        let selected = match catalog.world(response.active_world_id) {
            Ok(selected) => selected,
            Err(_) => {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                return;
            }
        };
        let compatibility = match BuildCompatibility::validate(
            &selected.bridge_path,
            &selected.rom_path,
            mgba.path(),
        ) {
            Ok(compatibility) => compatibility,
            Err(_) => {
                let released = SessionLifecycle::release_preacquired_world_lease(
                    &self.api,
                    &mut auth,
                    response,
                    &self.keychain,
                )
                .await
                .is_ok();
                if released {
                    let _ = intent.clear_exact(request);
                }
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
                return;
            }
        };
        if selected.check_compatibility(&compatibility).is_err() {
            let released = SessionLifecycle::release_preacquired_world_lease(
                &self.api,
                &mut auth,
                response,
                &self.keychain,
            )
            .await
            .is_ok();
            if released {
                let _ = intent.clear_exact(request);
            }
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        }

        let rom_path = selected.rom_path.clone();
        let sidecar_path = sidecar.path().to_owned();
        let mgba_path = mgba.path().to_owned();
        let handoff = generation.handoff();
        let bridge_path = handoff.path().join("bridge");
        let config = SessionConfig {
            client_instance_id: response.lease.client_instance_id,
            rom_world_id: response.active_world_id,
            manifest: compatibility,
            trusted_manifest_key: self.config.runtime.manifest_key.clone(),
            epoch_store: EpochStore::new(self.config.paths.epoch_file()),
            workspace_parent: self.config.paths.workspace_parent().to_owned(),
            bridge_lua_dir: bridge_path.clone(),
        };
        let mut session = match SessionLifecycle::from_world_lease_with_keychain(
            &self.api,
            auth,
            config,
            Arc::clone(&self.keychain),
            response,
        )
        .await
        {
            Ok(session) => session,
            Err(error) => {
                #[cfg(debug_assertions)]
                eprintln!("test diagnostic: world session materialization failed: {error:?}");
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        };
        let cold_source_resume = journal.read().ok().flatten().is_some_and(|record| {
            record.phase == TravelPhase::Aborted
                && record.active_world == session.rom_world_id()
                && record.source_revision == Some(session.revision)
        });
        if cold_source_resume && session.discard_resume_after_aborted_handoff().is_err() {
            let released = session
                .release_lease_keep_credentials(&self.api)
                .await
                .is_ok();
            if released {
                let _ = intent.clear_exact(request);
            }
            self.auth = Some(session.auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
            return;
        }
        match journal.read() {
            Ok(Some(record)) => match record.phase {
                TravelPhase::PrepareIntent
                | TravelPhase::Prepared
                | TravelPhase::SourceSaved
                | TravelPhase::DestinationReady
                | TravelPhase::Launched
                | TravelPhase::ArrivalAcknowledged
                    if !remote_committed =>
                {
                    if !matches!(
                        recover_pending_handoff(&self.api, &session, &journal).await,
                        Ok(StageOutcome::Aborted)
                    ) {
                        self.auth = Some(session.auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                }
                TravelPhase::ArrivalAcknowledged if remote_committed => {
                    if commit_acknowledged_handoff(&self.api, &session.auth, &journal, None)
                        .await
                        .is_err()
                    {
                        self.auth = Some(session.auth);
                        let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                        return;
                    }
                }
                _ => {}
            },
            Ok(None) => {}
            Err(_) => {
                self.auth = Some(session.auth);
                let _ = events.send(BackendEvent::StartFailed(StartFailure::Unavailable));
                return;
            }
        }
        let api = self.api.clone();
        let (stop_tx, stop_rx) = oneshot::channel();
        let stop_signal = Arc::new(Notify::new());
        let stop_relay = Arc::clone(&stop_signal);
        tokio::spawn(async move {
            let _ = stop_rx.await;
            stop_relay.notify_one();
        });
        let portal = PortalRuntime {
            catalog,
            journal,
            paired_journal,
            stop: Arc::clone(&stop_signal),
        };
        let trusted_manifest_key = self.config.runtime.manifest_key.clone();
        let epoch_file = self.config.paths.epoch_file().to_owned();
        let workspace_parent = self.config.paths.workspace_parent().to_owned();
        let keychain = Arc::clone(&self.keychain);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let result = run_runtime_chain(NextRuntime {
                api, session, keychain, handoff, trusted_manifest_key,
                epoch_file, workspace_parent, sidecar_path, rom_path, mgba_path,
                bridge_path, world_intent: Some((intent, request)),
                shutdown: Box::pin(async move { stop_signal.notified().await }),
                portal: Some(portal), announce_start: true,
            }, &commands).await;
            let _ = commands.send(BackendCommand::RuntimeFinished(result));
        });
        self.runtime = Some(RuntimeHandle {
            stop: Some(stop_tx),
        });
    }

    async fn stop_runtime(&mut self, events: &mpsc::Sender<BackendEvent>) {
        let Some(runtime) = self.runtime.as_mut() else {
            let _ = events.send(BackendEvent::ShutdownUncertain);
            return;
        };
        if let Some(stop) = runtime.stop.take() {
            let _ = stop.send(());
        }
    }

    async fn reconcile_recovery(&mut self, events: &mpsc::Sender<BackendEvent>) {
        let discovery = match RecoveryDiscovery::discover(self.config.paths.workspace_parent()) {
            Ok(discovery) => discovery,
            Err(_) => {
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        let Some(candidate) = discovery.candidate() else {
            let _ = events.send(BackendEvent::RecoveryReconciled(RecoveryResult::Reconciled));
            return;
        };
        let client_instance_id = match candidate.marker() {
            RecoveryMarker::V2(marker) => marker.prior_client_instance_id,
            RecoveryMarker::LegacyV1 => match ClientInstanceId::new(uuid::Uuid::new_v4()) {
                Ok(value) => value,
                Err(_) => {
                    let _ = events.send(BackendEvent::RecoveryReconciled(
                        RecoveryResult::StillUncertain,
                    ));
                    return;
                }
            },
            RecoveryMarker::OrphanSave => {
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        let auth = match self.take_or_resume_auth().await {
            Ok(auth) => auth,
            Err(_) => {
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        let config = match self.session_config(client_instance_id) {
            Ok(config) => config,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        let result = match RecoveryReconciler::reconcile(
            &self.api,
            auth,
            config,
            Some(self.keychain.clone()),
        )
        .await
        {
            Ok(session) => {
                self.auth = Some(session.auth);
                match session.outcome {
                    RecoveryOutcome::AuthorizationMissing => RecoveryResult::StillUncertain,
                    RecoveryOutcome::NoEvidence
                    | RecoveryOutcome::RetiredLegacy
                    | RecoveryOutcome::RetiredCommittedV2
                    | RecoveryOutcome::CommittedV2 => RecoveryResult::Reconciled,
                }
            }
            Err(_) => RecoveryResult::StillUncertain,
        };
        let _ = events.send(BackendEvent::RecoveryReconciled(result));
    }

    async fn sign_out(&mut self, events: &mpsc::Sender<BackendEvent>) {
        if self.cleanup_pending_credential().is_err() {
            let _ = events.send(BackendEvent::SignOutFailed(
                coop_launcher::SignOutFailure::LocalDeletion,
            ));
            return;
        }
        let Some(mut auth) = self.auth.take() else {
            let account = match self.config.paths.load_account() {
                Ok(Some(account)) => account,
                Ok(None) => {
                    let _ = events.send(BackendEvent::SignOutCompleted);
                    return;
                }
                Err(_) => {
                    let _ = events.send(BackendEvent::SignOutFailed(
                        coop_launcher::SignOutFailure::LocalDeletion,
                    ));
                    return;
                }
            };
            if delete_local_account(self.keychain.as_ref(), &self.config.paths, &account).is_err() {
                let _ = events.send(BackendEvent::SignOutFailed(
                    coop_launcher::SignOutFailure::LocalDeletion,
                ));
            } else {
                let _ = events.send(BackendEvent::SignOutCompleted);
            }
            return;
        };
        let username = auth.username.to_string();
        if self.config.paths.save_cleanup_username(&username).is_err() {
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::SignOutFailed(
                coop_launcher::SignOutFailure::LocalDeletion,
            ));
            return;
        }
        self.pending_credential_cleanup = Some(username);
        let logout = auth.logout(&self.api, self.keychain.as_ref()).await;
        if !matches!(&logout, Err(AuthError::Keychain(_))) {
            self.pending_credential_cleanup = None;
            let _ = self.config.paths.clear_cleanup_username();
        }
        match logout {
            Ok(()) => {
                let event = if self.config.paths.remove_account().is_ok() {
                    BackendEvent::SignOutCompleted
                } else {
                    BackendEvent::SignOutFailed(coop_launcher::SignOutFailure::LocalDeletion)
                };
                let _ = events.send(event);
            }
            Err(AuthError::Keychain(_)) => {
                let _ = events.send(BackendEvent::SignOutFailed(
                    coop_launcher::SignOutFailure::LocalDeletion,
                ));
            }
            Err(_) => {
                // AuthSession::logout deletes the local credential before the
                // remote revocation attempt. Local sign-out is therefore
                // complete even when the best-effort revoke cannot be sent.
                let event = if self.config.paths.remove_account().is_ok() {
                    BackendEvent::SignOutCompleted
                } else {
                    BackendEvent::SignOutFailed(coop_launcher::SignOutFailure::LocalDeletion)
                };
                let _ = events.send(event);
            }
        }
    }

    async fn take_or_resume_auth(&mut self) -> Result<AuthSession, AuthError> {
        self.cleanup_pending_credential()
            .map_err(|_| AuthError::Transport)?;
        if let Some(auth) = self.auth.take() {
            return Ok(auth);
        }
        let account = self
            .config
            .paths
            .load_account()
            .map_err(|_| AuthError::Transport)?
            .ok_or(AuthError::SessionClosed)?;
        AuthSession::refresh_from_keychain(
            &self.api,
            self.keychain.as_ref(),
            account.username,
            account.user_id,
            account.character_id,
        )
        .await
    }

    async fn rollback_auth(&mut self, auth: &mut AuthSession) -> bool {
        let username = auth.username.to_string();
        if self.config.paths.save_cleanup_username(&username).is_err() {
            return false;
        }
        self.pending_credential_cleanup = Some(username);
        if !matches!(
            auth.logout(&self.api, self.keychain.as_ref()).await,
            Err(AuthError::Keychain(_))
        ) {
            self.pending_credential_cleanup = None;
            let _ = self.config.paths.clear_cleanup_username();
        }
        true
    }

    fn cleanup_pending_credential(&mut self) -> Result<(), ()> {
        let username = match self.pending_credential_cleanup.clone() {
            Some(username) => Some(username),
            None => self.config.paths.load_cleanup_username().map_err(|_| ())?,
        };
        let Some(username) = username else {
            return Ok(());
        };
        self.pending_credential_cleanup = Some(username.clone());
        self.keychain
            .delete(KEYCHAIN_SERVICE, &username)
            .map_err(|_| ())?;
        self.config.paths.clear_cleanup_username().map_err(|_| ())?;
        self.pending_credential_cleanup = None;
        Ok(())
    }

    fn session_config(
        &self,
        client_instance_id: ClientInstanceId,
    ) -> Result<SessionConfig, BackendError> {
        let now = now_seconds().map_err(|_| BackendError::Store)?;
        let generation = self
            .store
            .open_accepted_current(self.release.trusted_key(), now)
            .map_err(|_| BackendError::Store)?;
        let manifest = generation
            .artifact(ArtifactIdentity::CompatibilityManifest)
            .ok_or(BackendError::Store)?;
        let rom = generation
            .artifact(ArtifactIdentity::Rom)
            .ok_or(BackendError::Store)?;
        let mgba = generation
            .artifact(ArtifactIdentity::ManagedMgba)
            .ok_or(BackendError::Store)?;
        let compatibility = BuildCompatibility::validate(manifest.path(), rom.path(), mgba.path())
            .map_err(|_| BackendError::Store)?;
        Ok(SessionConfig {
            client_instance_id,
            // Current desktop generation contains Main only; a Cormoria
            // generation must supply its world ID from the trusted catalog.
            rom_world_id: coop_launcher::session::RomWorldId::new(1)
                .map_err(|_| BackendError::Store)?,
            manifest: compatibility,
            trusted_manifest_key: self.config.runtime.manifest_key.clone(),
            epoch_store: EpochStore::new(self.config.paths.epoch_file()),
            workspace_parent: self.config.paths.workspace_parent().to_owned(),
            bridge_lua_dir: generation.handoff().path().join("bridge"),
        })
    }
}

async fn run_runtime(
    api: coop_launcher::ReqwestCloudApi,
    keychain: Arc<dyn RefreshTokenStore>,
    auth: AuthSession,
    _handoff: coop_launcher::GenerationHandoff,
    compatibility: BuildCompatibility,
    trusted_manifest_key: TrustedManifestKey,
    sidecar_path: PathBuf,
    rom_path: PathBuf,
    mgba_path: PathBuf,
    bridge_path: PathBuf,
    paths: UserPaths,
    stop_rx: oneshot::Receiver<()>,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    let config = SessionConfig {
        client_instance_id: match ClientInstanceId::new(uuid::Uuid::new_v4()) {
            Ok(value) => value,
            Err(_) => {
                return RuntimeCompletion {
                    auth: Some(auth),
                    outcome: RuntimeOutcome::StartupFailed(StartFailure::Unavailable),
                };
            }
        },
        // Current desktop generation contains Main only. Do not infer this
        // stable ID from co-op region or the ROM content-build selector.
        rom_world_id: coop_launcher::session::RomWorldId::new(1)
            .expect("registered Main ROM world ID"),
        manifest: compatibility,
        trusted_manifest_key: trusted_manifest_key.clone(),
        epoch_store: EpochStore::new(paths.epoch_file()),
        workspace_parent: paths.workspace_parent().to_owned(),
        bridge_lua_dir: bridge_path.clone(),
    };
    let session =
        match SessionLifecycle::acquire_with_keychain(&api, auth, config, Arc::clone(&keychain))
            .await
        {
            Ok(session) => session,
            Err(_) => {
                return RuntimeCompletion {
                    auth: None,
                    outcome: RuntimeOutcome::StartupFailed(StartFailure::Unavailable),
                };
            }
        };
    run_runtime_chain(NextRuntime {
        api, session, keychain, handoff: _handoff,
        trusted_manifest_key: trusted_manifest_key.clone(),
        epoch_file: paths.epoch_file().to_owned(),
        workspace_parent: paths.workspace_parent().to_owned(),
        sidecar_path, rom_path, mgba_path, bridge_path,
        world_intent: None,
        shutdown: Box::pin(async move { let _ = stop_rx.await; }),
        portal: None, announce_start: true,
    }, commands).await
}

async fn run_runtime_chain(
    mut next: NextRuntime,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    loop {
        let NextRuntime {
            api, session, keychain, handoff, trusted_manifest_key,
            epoch_file, workspace_parent, sidecar_path, rom_path, mgba_path,
            bridge_path, world_intent, shutdown, portal, announce_start,
        } = next;
        let result = run_runtime_with_session(
            api, session, keychain, handoff, trusted_manifest_key,
            epoch_file, workspace_parent, sidecar_path, rom_path, mgba_path,
            bridge_path, world_intent, shutdown, portal, announce_start, commands,
        ).await;
        match result {
            RuntimeCompletion { outcome: RuntimeOutcome::Continue(leg), .. } => next = *leg,
            finished => return finished,
        }
    }
}

async fn run_runtime_with_session(
    api: coop_launcher::ReqwestCloudApi,
    mut session: SessionLifecycle,
    keychain: Arc<dyn RefreshTokenStore>,
    _handoff: coop_launcher::GenerationHandoff,
    trusted_manifest_key: TrustedManifestKey,
    epoch_file: PathBuf,
    workspace_parent: PathBuf,
    sidecar_path: PathBuf,
    rom_path: PathBuf,
    mgba_path: PathBuf,
    bridge_path: PathBuf,
    world_intent: Option<PendingWorldAcquire>,
    shutdown: ShutdownFuture,
    portal: Option<PortalRuntime>,
    announce_start: bool,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    let staged_rom = session.workspace.path().join("game.gba");
    if fs::copy(&rom_path, &staged_rom).is_err() {
        return retain_auth(session, &api, world_intent).await;
    }
    let marker = staged_rom_marker_path(&staged_rom);
    let marker_bytes = match staged_rom_marker_contents(&staged_rom) {
        Ok(bytes) => bytes,
        Err(_) => return retain_auth(session, &api, world_intent).await,
    };
    let marker_result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(&marker_bytes)
        });
    if marker_result.is_err() {
        return retain_auth(session, &api, world_intent).await;
    }
    let mgba = match CommandSpec::mgba_owned_staged(&mgba_path, &staged_rom, &marker) {
        Ok(spec) => spec,
        Err(_) => return retain_auth(session, &api, world_intent).await,
    };
    let sidecar = match CommandSpec::sidecar_template(&sidecar_path)
        .and_then(|spec| spec.with_session_epoch(session.lease.session_epoch.value()))
    {
        Ok(spec) => spec,
        Err(_) => return retain_auth(session, &api, world_intent).await,
    };
    let mut children = match SupervisedChildren::start_with_bridge(
        sidecar,
        mgba,
        session.lease.session_epoch.value(),
        &session.workspace,
        &bridge_path,
    )
    .await
    {
        Ok(children) => children,
        Err(error) => {
            #[cfg(debug_assertions)]
            eprintln!("test diagnostic: managed children startup failed: {error:?}");
            return retain_auth(session, &api, world_intent).await;
        }
    };
    if announce_start {
        let _ = commands.send(BackendCommand::RuntimeStarted);
    }
    let lifecycle = if portal.is_some() {
        match session
            .run_until_shutdown_with_realtime_portal(&api, &mut children, shutdown)
            .await
        {
            Ok(SessionRunOutcome::Completed) => Ok(None),
            Ok(SessionRunOutcome::PortalTravel(source)) => Ok(Some(source)),
            Err(error) => Err(error),
        }
    } else {
        session
            .run_until_shutdown(&api, &mut children, shutdown)
            .await
            .map(|()| None)
    };
    let Some(source) = (match lifecycle {
        Ok(source) => {
            if source.is_none() {
                let stopped = children.stop().await;
                if stopped.is_err() {
                    let _ = session.preserve_recovery_after_child_failure();
                    let _ = session.close_credentials(&api).await;
                    return RuntimeCompletion {
                        auth: None,
                        outcome: RuntimeOutcome::Exited { clean_stop: false },
                    };
                }
            }
            source
        }
        Err(error) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "test diagnostic: live session failed: {error:?}; control terminal: {:?}",
                children.control().terminal_cause()
            );
            let _ = children.stop().await;
            let _ = session.preserve_recovery_after_child_failure();
            let _ = session.close_credentials(&api).await;
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    }) else {
        let released = session.release_lease_keep_credentials(&api).await.is_ok();
        clear_world_intent_after_confirmed_release(world_intent, released);
        let auth = session.auth;
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited {
                clean_stop: released,
            },
        };
    };

    let Some(portal) = portal else {
        return RuntimeCompletion {
            auth: Some(session.auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    };
    run_portal_transition(
        api,
        session,
        keychain,
        _handoff,
        trusted_manifest_key,
        epoch_file,
        workspace_parent,
        sidecar_path,
        mgba_path,
        bridge_path,
        world_intent,
        portal,
        Some(source),
        false,
        commands,
    )
    .await
}

fn reusable_prepare_key(
    record: &TravelRecord,
    current_world: coop_launcher::session::RomWorldId,
) -> Option<IdempotencyKey> {
    // A committed record describes the previous trip. Reusing its key for a
    // return journey makes the server reject the new commit as a conflicting
    // replay of that earlier journey.
    if record.active_world == current_world
        && !matches!(
            record.phase,
            TravelPhase::Idle | TravelPhase::Committed | TravelPhase::Aborted
        )
    {
        record.prepare_idempotency_key
    } else {
        None
    }
}

async fn run_portal_transition(
    api: coop_launcher::ReqwestCloudApi,
    mut session: SessionLifecycle,
    keychain: Arc<dyn RefreshTokenStore>,
    handoff: coop_launcher::GenerationHandoff,
    trusted_manifest_key: TrustedManifestKey,
    epoch_file: PathBuf,
    workspace_parent: PathBuf,
    sidecar_path: PathBuf,
    mgba_path: PathBuf,
    bridge_path: PathBuf,
    world_intent: Option<PendingWorldAcquire>,
    portal: PortalRuntime,
    source: Option<PortalTravelSource>,
    announce_start: bool,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    let PortalRuntime {
        catalog,
        journal,
        paired_journal,
        stop,
    } = portal;
    if session.heartbeat(&api).await.is_err() {
        return portal_failure(session, &api, world_intent).await;
    }
    let active_group = match authoritative_group(&api, &session).await {
        Ok(group) => group,
        Err(_) => return portal_failure(session, &api, world_intent).await,
    };
    if let Some(group_id) = active_group {
        let Some(source) = source else {
            return portal_failure(session, &api, world_intent).await;
        };
        return run_paired_portal_transition(
            api,
            session,
            keychain,
            handoff,
            trusted_manifest_key,
            epoch_file,
            workspace_parent,
            sidecar_path,
            mgba_path,
            bridge_path,
            world_intent,
            source,
            group_id,
            catalog,
            journal,
            paired_journal,
            stop,
            announce_start,
            commands,
        )
        .await;
    }
    let record = match journal.read() {
        Ok(Some(record)) => record,
        _ => return portal_failure(session, &api, world_intent).await,
    };
    let (committed, destination_world) = if record.phase == TravelPhase::ArrivalAcknowledged {
        let result = commit_acknowledged_handoff(&api, &session.auth, &journal, None).await;
        let Ok(record) = result else {
            return portal_failure(session, &api, world_intent).await;
        };
        let Some(destination_world) = record.destination_world else {
            return portal_failure(session, &api, world_intent).await;
        };
        (record, destination_world)
    } else {
        let Some(source) = source else {
            return portal_failure(session, &api, world_intent).await;
        };
        let prepare_key = reusable_prepare_key(&record, session.rom_world_id())
            .or_else(|| IdempotencyKey::new(uuid::Uuid::new_v4()).ok());
        let Some(prepare_key) = prepare_key else {
            return portal_failure(session, &api, world_intent).await;
        };
        let staged_result = {
            let staging =
                stage_portal_travel(&api, &session, &source, &catalog, &journal, prepare_key);
            tokio::pin!(staging);
            let mut staging_heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                tokio::select! {
                    result = &mut staging => break result,
                    _ = staging_heartbeat.tick() => {
                        let renewal = api.heartbeat(&session.auth, HeartbeatLeaseRequest::new(session.lease.fence()));
                        tokio::pin!(renewal);
                        tokio::select! {
                            result = &mut staging => break result,
                            result = &mut renewal => {
                                let _ = result;
                            }
                        }
                    }
                }
            }
        };
        let staged = match staged_result {
            Ok(StageOutcome::Staged(staged)) => staged,
            Ok(StageOutcome::Aborted) | Err(_) => {
                return portal_failure(session, &api, world_intent).await;
            }
        };
        if session.heartbeat(&api).await.is_err() {
            return portal_failure(session, &api, world_intent).await;
        }
        let selected = match catalog.world(staged.destination_world()) {
            Ok(selected) => selected,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        let destination_compatibility = match BuildCompatibility::validate(
            &selected.bridge_path,
            &selected.rom_path,
            &mgba_path,
        ) {
            Ok(compatibility) => compatibility,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        if selected
            .check_compatibility(&destination_compatibility)
            .is_err()
        {
            return portal_failure(session, &api, world_intent).await;
        }
        let destination_workspace = match SessionWorkspace::create(&workspace_parent) {
            Ok(workspace) => workspace,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        if destination_workspace
            .write_generated_addresses(&destination_compatibility.manifest)
            .is_err()
        {
            return portal_failure(session, &api, world_intent).await;
        }
        if destination_workspace
            .write_atomic("pending_commits.json", b"[]")
            .is_err()
        {
            return portal_failure(session, &api, world_intent).await;
        }
        let destination_rom = destination_workspace.path().join("destination.gba");
        if fs::copy(&selected.rom_path, &destination_rom).is_err() {
            return portal_failure(session, &api, world_intent).await;
        }
        let destination_marker = staged_rom_marker_path(&destination_rom);
        let marker_bytes = match staged_rom_marker_contents(&destination_rom) {
            Ok(bytes) => bytes,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        let marker_result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_marker)
            .and_then(|mut file| {
                use std::io::Write;
                file.write_all(&marker_bytes)
            });
        if marker_result.is_err() {
            return portal_failure(session, &api, world_intent).await;
        }
        let verifier_mgba =
            match CommandSpec::mgba_owned_staged(&mgba_path, &destination_rom, &destination_marker)
            {
                Ok(spec) => spec,
                Err(_) => return portal_failure(session, &api, world_intent).await,
            };
        let verifier_sidecar = match CommandSpec::sidecar_template(&sidecar_path)
            .and_then(|spec| spec.with_arrival_verifier())
        {
            Ok(spec) => spec,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        let registry = match session.registry_contract() {
            Ok(registry) => registry,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        let lease_fence = LeaseFenceIdentity::new(
            session.lease.session_id,
            session.lease.session_epoch,
            session.lease.client_instance_id,
        );
        if session.heartbeat(&api).await.is_err() {
            return portal_failure(session, &api, world_intent).await;
        }
        let verification = verify_staged_arrival(
            &journal,
            &staged,
            lease_fence,
            ArrivalVerificationConfig {
                registry,
                catalog: &catalog,
                workspace: &destination_workspace,
                sidecar: verifier_sidecar,
                mgba: verifier_mgba,
                bridge_source: &bridge_path,
            },
        );
        tokio::pin!(verification);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        let arrival_result = loop {
            tokio::select! {
                result = &mut verification => break result,
                _ = heartbeat.tick() => {
                    let renewal = session.heartbeat(&api);
                    tokio::pin!(renewal);
                    tokio::select! {
                        result = &mut verification => break result,
                        result = &mut renewal => {
                            let _ = result;
                        }
                    }
                }
            }
        };
        if arrival_result.is_err() {
            return portal_failure(session, &api, world_intent).await;
        }
        if session.heartbeat(&api).await.is_err() {
            #[cfg(debug_assertions)]
            if let Some(path) = std::env::var_os("HOENN_LOCAL_RETURN_DIAGNOSTIC") {
                let _ = std::fs::write(path, b"post_arrival_heartbeat_error");
            }
            return portal_failure(session, &api, world_intent).await;
        }
        let record = match commit_acknowledged_handoff(
            &api,
            &session.auth,
            &journal,
            Some(&destination_workspace),
        )
        .await
        {
            Ok(record) => record,
            Err(_) => return portal_failure(session, &api, world_intent).await,
        };
        let Some(destination_world) = record.destination_world else {
            return portal_failure(session, &api, world_intent).await;
        };
        (record, destination_world)
    };
    let mut auth = match session.into_auth_after_committed_handoff(&committed) {
        Ok(auth) => auth,
        Err(_) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let Some((intent, old_request)) = world_intent else {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    };
    if intent.clear_exact(old_request).is_err() {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let new_request = match new_world_acquire_request(auth.character_id)
        .ok()
        .and_then(|request| intent.load_or_create(request).ok())
    {
        Some(request) => request,
        None => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let response = match SessionLifecycle::acquire_world_with_keychain(
        &api,
        &mut auth,
        new_request,
        &keychain,
    )
    .await
    {
        Ok(response) if response.active_world_id == destination_world => response,
        _ => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let selected = match catalog.world(destination_world) {
        Ok(selected) => selected,
        Err(_) => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let compatibility =
        match BuildCompatibility::validate(&selected.bridge_path, &selected.rom_path, &mgba_path) {
            Ok(compatibility) => compatibility,
            Err(_) => {
                return RuntimeCompletion {
                    auth: Some(auth),
                    outcome: RuntimeOutcome::Exited { clean_stop: false },
                };
            }
        };
    if selected.check_compatibility(&compatibility).is_err() {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let destination_rom_path = selected.rom_path.clone();
    let config = SessionConfig {
        client_instance_id: response.lease.client_instance_id,
        rom_world_id: destination_world,
        manifest: compatibility,
        trusted_manifest_key: trusted_manifest_key.clone(),
        epoch_store: EpochStore::new(epoch_file.clone()),
        workspace_parent: workspace_parent.clone(),
        bridge_lua_dir: bridge_path.clone(),
    };
    let destination = match SessionLifecycle::from_world_lease_with_keychain(
        &api,
        auth,
        config,
        Arc::clone(&keychain),
        response,
    )
    .await
    {
        Ok(session) => session,
        Err(_) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let destination_portal = PortalRuntime {
        catalog,
        journal,
        paired_journal,
        stop: Arc::clone(&stop),
    };
    // Return the next leg to the outer loop so the previous future drops
    // before another ROM runtime is polled. Nested futures overflowed the
    // signed client's stack on the live Cormoria -> Main return.
    RuntimeCompletion {
        auth: None,
        outcome: RuntimeOutcome::Continue(Box::new(NextRuntime {
            api, session: destination, keychain, handoff, trusted_manifest_key,
            epoch_file, workspace_parent, sidecar_path,
            rom_path: destination_rom_path, mgba_path, bridge_path,
            world_intent: Some((intent, new_request)),
            shutdown: Box::pin(async move { stop.notified().await }),
            portal: Some(destination_portal), announce_start,
        })),
    }
}

async fn authoritative_group(
    api: &coop_launcher::ReqwestCloudApi,
    session: &SessionLifecycle,
) -> Result<Option<coop_cloud::GroupId>, coop_launcher::SessionError> {
    let token = session
        .auth
        .access_token()
        .cloned()
        .ok_or(coop_launcher::SessionError::Unauthorized)?;
    let response = api
        .online_snapshot(
            token,
            OnlineSnapshotRequest {
                api_version: ApiVersion::V1,
                fence: session.lease.fence(),
                incoming_after: None,
            },
        )
        .await
        .map_err(|_| coop_launcher::SessionError::Cloud)?;
    if response.api_version != ApiVersion::V1 {
        return Err(coop_launcher::SessionError::Realtime);
    }
    let Some(group) = response.group else {
        return Ok(None);
    };
    if !group
        .group
        .members
        .iter()
        .any(|member| member.character_id == session.lease.character_id)
    {
        return Err(coop_launcher::SessionError::Realtime);
    }
    Ok(Some(group.group.group_id))
}

async fn stop_paired_handoff(
    api: &coop_launcher::ReqwestCloudApi,
    auth: &AuthSession,
    journal: &PairedTravelJournal,
) -> Option<PairedHandoffOutcome> {
    let mut record = journal.read().ok()??;
    if record.attempt_key.is_none() {
        match recover_paired_handoff(api, auth, journal).await.ok()? {
            PairedHandoffOutcome::Committed(commit) => {
                return Some(PairedHandoffOutcome::Committed(commit));
            }
            PairedHandoffOutcome::Aborted => return Some(PairedHandoffOutcome::Aborted),
            PairedHandoffOutcome::Pending { .. } | PairedHandoffOutcome::Staged { .. } => {
                record = journal.read().ok()??;
            }
        }
    }
    let attempt_key = record.attempt_key?;
    api.abort_group_rom_handoff(
        auth,
        GroupRomHandoffAbortRequest {
            api_version: record.intent.request.api_version,
            group_id: record.intent.request.group_id,
            fence: record.intent.request.fence,
            idempotency_key: attempt_key,
        },
    )
    .await
    .ok()?;
    recover_paired_handoff(api, auth, journal).await.ok()
}

async fn run_paired_portal_transition(
    api: coop_launcher::ReqwestCloudApi,
    session: SessionLifecycle,
    keychain: Arc<dyn RefreshTokenStore>,
    handoff: coop_launcher::GenerationHandoff,
    trusted_manifest_key: TrustedManifestKey,
    epoch_file: PathBuf,
    workspace_parent: PathBuf,
    sidecar_path: PathBuf,
    mgba_path: PathBuf,
    bridge_path: PathBuf,
    world_intent: Option<PendingWorldAcquire>,
    source: PortalTravelSource,
    group_id: coop_cloud::GroupId,
    catalog: TrustedRomCatalog,
    journal: RomTravelJournal,
    paired_journal: PairedTravelJournal,
    stop: Arc<Notify>,
    announce_start: bool,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    let client_intent_key = match IdempotencyKey::new(uuid::Uuid::new_v4()) {
        Ok(key) => key,
        Err(_) => return portal_failure(session, &api, world_intent).await,
    };
    let intent = PairedJoinIntent {
        request: GroupRomHandoffJoinRequest {
            api_version: ApiVersion::V1,
            group_id,
            fence: session.lease.fence(),
            source_snapshot_id: source.source_snapshot_id,
            portal_id: source.portal_id.clone(),
            client_intent_key,
        },
        source_world_id: session.rom_world_id(),
        source_save_sha256: source.source_save_digest,
        catalog_digest: catalog.digest(),
    };
    let expected_nonce = *uuid::Uuid::new_v4().as_bytes();
    if expected_nonce == [0; 16] {
        return portal_failure(session, &api, world_intent).await;
    }
    if session.registry_contract().is_err() {
        return portal_failure(session, &api, world_intent).await;
    }
    let destination_build = |world: coop_launcher::session::RomWorldId| {
        let selected = catalog
            .world(world)
            .map_err(|_| coop_launcher::SessionError::Lease)?;
        let compatibility =
            BuildCompatibility::validate(&selected.bridge_path, &selected.rom_path, &mgba_path)
                .map_err(|_| coop_launcher::SessionError::Lease)?;
        selected
            .check_compatibility(&compatibility)
            .map_err(|_| coop_launcher::SessionError::Lease)?;
        Ok(RuntimeBuildIdentity::from(&compatibility.target))
    };

    let mut stop_requested = false;
    let committed = loop {
        if stop_requested {
            match stop_paired_handoff(&api, &session.auth, &paired_journal).await {
                Some(PairedHandoffOutcome::Committed(_)) => {}
                Some(PairedHandoffOutcome::Aborted) => {
                    return portal_failure(session, &api, world_intent).await;
                }
                _ => {
                    return RuntimeCompletion {
                        auth: Some(session.auth),
                        outcome: RuntimeOutcome::Exited { clean_stop: false },
                    };
                }
            }
        }
        let verify_catalog = &catalog;
        let verify_workspace_parent = &workspace_parent;
        let verify_sidecar = &sidecar_path;
        let verify_mgba = &mgba_path;
        let verify_bridge = &bridge_path;
        let verify = move |checked: &coop_launcher::travel_coordinator::PairedStagedDestination,
                           nonce| {
            let destination_world = checked.destination().destination_world();
            let destination_save_sha256 = checked.destination().destination_save_sha256();
            let destination_save = checked.destination().destination_save().to_vec();
            let destination_save_generation = checked.destination().destination_save_generation();
            let arrival_location = checked.destination().arrival_location();
            async move {
                verify_paired_arrival(
                    destination_world,
                    destination_save_sha256,
                    destination_save,
                    destination_save_generation,
                    arrival_location,
                    nonce,
                    verify_catalog,
                    verify_workspace_parent,
                    verify_sidecar,
                    verify_mgba,
                    verify_bridge,
                )
                .await
                .map_err(|error| error.to_string())
            }
        };
        let (attempt, heartbeat_failed) = {
            let attempt = run_paired_handoff(
                &api,
                &session.auth,
                &paired_journal,
                intent.clone(),
                &source,
                session.rom_world_id(),
                &catalog,
                &destination_build,
                expected_nonce,
                verify,
            );
            tokio::pin!(attempt);
            let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
            let mut heartbeat_failed = false;
            let stop_wait = stop.notified();
            tokio::pin!(stop_wait);
            let attempt = loop {
                tokio::select! {
                    result = &mut attempt => break result,
                    _ = &mut stop_wait, if !stop_requested => {
                        // Do not cancel an owned ROM verifier: it must reap
                        // its children before the exact attempt is aborted.
                        stop_requested = true;
                    }
                    _ = heartbeat.tick() => {
                        let renewed = api.heartbeat(
                            &session.auth,
                            HeartbeatLeaseRequest::new(session.lease.fence()),
                        ).await;
                        if renewed.is_err() {
                            heartbeat_failed = true;
                        }
                    }
                }
            };
            (attempt, heartbeat_failed)
        };
        match attempt {
            Ok(PairedHandoffOutcome::Committed(_)) => {
                break match paired_journal.read() {
                    Ok(Some(record)) => record,
                    _ => {
                        return RuntimeCompletion {
                            auth: Some(session.auth),
                            outcome: RuntimeOutcome::Exited { clean_stop: false },
                        };
                    }
                };
            }
            Ok(PairedHandoffOutcome::Aborted) => {
                return portal_failure(session, &api, world_intent).await;
            }
            Ok(PairedHandoffOutcome::Pending { .. }) | Ok(PairedHandoffOutcome::Staged { .. }) => {
                if heartbeat_failed {
                    match recover_paired_handoff(&api, &session.auth, &paired_journal).await {
                        Ok(PairedHandoffOutcome::Committed(_)) => continue,
                        Ok(PairedHandoffOutcome::Aborted) => {
                            return portal_failure(session, &api, world_intent).await;
                        }
                        _ => {
                            return RuntimeCompletion {
                                auth: Some(session.auth),
                                outcome: RuntimeOutcome::Exited { clean_stop: false },
                            };
                        }
                    }
                }
                if !stop_requested {
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
                        _ = stop.notified() => stop_requested = true,
                    }
                }
            }
            Err(_) => {
                if stop_requested {
                    match stop_paired_handoff(&api, &session.auth, &paired_journal).await {
                        Some(PairedHandoffOutcome::Committed(_)) => continue,
                        Some(PairedHandoffOutcome::Aborted) => {
                            return portal_failure(session, &api, world_intent).await;
                        }
                        _ => {
                            return RuntimeCompletion {
                                auth: Some(session.auth),
                                outcome: RuntimeOutcome::Exited { clean_stop: false },
                            };
                        }
                    }
                }
                let recovered = recover_paired_handoff(&api, &session.auth, &paired_journal).await;
                match recovered {
                    Ok(PairedHandoffOutcome::Committed(_)) => continue,
                    Ok(PairedHandoffOutcome::Aborted) => {
                        return portal_failure(session, &api, world_intent).await;
                    }
                    Ok(PairedHandoffOutcome::Pending { .. })
                    | Ok(PairedHandoffOutcome::Staged { .. })
                    | Err(_) => {
                        return RuntimeCompletion {
                            auth: Some(session.auth),
                            outcome: RuntimeOutcome::Exited { clean_stop: false },
                        };
                    }
                }
            }
        }
    };
    let destination_world = match committed.terminal.as_ref() {
        Some(PairedTerminal::Committed(commit)) => commit.own_world_id,
        _ => {
            return RuntimeCompletion {
                auth: Some(session.auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    if committed.phase != PairedPhase::Committed && committed.phase != PairedPhase::Adopted {
        return RuntimeCompletion {
            auth: Some(session.auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    if journal.adopt_paired_commit(&committed).is_err()
        || paired_journal
            .record_adopted(committed.intent.request.client_intent_key)
            .is_err()
    {
        return RuntimeCompletion {
            auth: Some(session.auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let mut auth = match session.into_auth_after_committed_paired_handoff(&committed) {
        Ok(auth) => auth,
        Err(_) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let Some((intent_store, old_request)) = world_intent else {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    };
    if intent_store.clear_exact(old_request).is_err() {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let new_request = match new_world_acquire_request(auth.character_id)
        .ok()
        .and_then(|request| intent_store.load_or_create(request).ok())
    {
        Some(request) => request,
        None => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let response = match SessionLifecycle::acquire_world_with_keychain(
        &api,
        &mut auth,
        new_request,
        &keychain,
    )
    .await
    {
        Ok(response) if response.active_world_id == destination_world => response,
        _ => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let selected = match catalog.world(destination_world) {
        Ok(selected) => selected,
        Err(_) => {
            return RuntimeCompletion {
                auth: Some(auth),
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let compatibility =
        match BuildCompatibility::validate(&selected.bridge_path, &selected.rom_path, &mgba_path) {
            Ok(compatibility) => compatibility,
            Err(_) => {
                return RuntimeCompletion {
                    auth: Some(auth),
                    outcome: RuntimeOutcome::Exited { clean_stop: false },
                };
            }
        };
    if selected.check_compatibility(&compatibility).is_err() {
        return RuntimeCompletion {
            auth: Some(auth),
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let destination_rom_path = selected.rom_path.clone();
    let config = SessionConfig {
        client_instance_id: response.lease.client_instance_id,
        rom_world_id: destination_world,
        manifest: compatibility,
        trusted_manifest_key: trusted_manifest_key.clone(),
        epoch_store: EpochStore::new(epoch_file.clone()),
        workspace_parent: workspace_parent.clone(),
        bridge_lua_dir: bridge_path.clone(),
    };
    let destination = match SessionLifecycle::from_world_lease_with_keychain(
        &api,
        auth,
        config,
        Arc::clone(&keychain),
        response,
    )
    .await
    {
        Ok(session) => session,
        Err(_) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::Exited { clean_stop: false },
            };
        }
    };
    let destination_portal = PortalRuntime {
        catalog,
        journal,
        paired_journal,
        stop: Arc::clone(&stop),
    };
    RuntimeCompletion {
        auth: None,
        outcome: RuntimeOutcome::Continue(Box::new(NextRuntime {
            api, session: destination, keychain, handoff, trusted_manifest_key,
            epoch_file, workspace_parent, sidecar_path,
            rom_path: destination_rom_path, mgba_path, bridge_path,
            world_intent: Some((intent_store, new_request)),
            shutdown: Box::pin(async move {
                if stop_requested { stop.notify_one(); }
                stop.notified().await;
            }),
            portal: Some(destination_portal), announce_start,
        })),
    }
}

async fn verify_paired_arrival(
    destination_world: coop_launcher::session::RomWorldId,
    destination_save_sha256: coop_cloud::Sha256Digest,
    destination_save: Vec<u8>,
    destination_save_generation: u32,
    arrival_location: [u8; 3],
    nonce: [u8; 16],
    catalog: &TrustedRomCatalog,
    workspace_parent: &std::path::Path,
    sidecar_path: &std::path::Path,
    mgba_path: &std::path::Path,
    bridge_path: &std::path::Path,
) -> Result<AuthenticatedArrivalEvidence, coop_launcher::arrival_verifier::ArrivalVerificationError>
{
    let selected = catalog
        .world(destination_world)
        .map_err(|_| coop_launcher::arrival_verifier::ArrivalVerificationError::WorldMismatch)?;
    let compatibility =
        BuildCompatibility::validate(&selected.bridge_path, &selected.rom_path, mgba_path)
            .map_err(|_| {
                coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
            })?;
    selected.check_compatibility(&compatibility).map_err(|_| {
        coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
    })?;
    let registry = compatibility
        .manifest
        .save
        .registry_contract()
        .map_err(|_| {
            coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
        })?;
    let workspace = SessionWorkspace::create(workspace_parent)
        .map_err(coop_launcher::arrival_verifier::ArrivalVerificationError::Workspace)?;
    workspace
        .write_generated_addresses(&compatibility.manifest)
        .map_err(coop_launcher::arrival_verifier::ArrivalVerificationError::Workspace)?;
    workspace
        .write_atomic("pending_commits.json", b"[]")
        .map_err(coop_launcher::arrival_verifier::ArrivalVerificationError::Workspace)?;
    let destination_rom = workspace.path().join("destination.gba");
    fs::copy(&selected.rom_path, &destination_rom).map_err(|_| {
        coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
    })?;
    let marker = staged_rom_marker_path(&destination_rom);
    let marker_bytes = staged_rom_marker_contents(&destination_rom).map_err(|_| {
        coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
    })?;
    let marker_result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(&marker_bytes)
        });
    if marker_result.is_err() {
        return Err(coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec);
    }
    let mgba =
        CommandSpec::mgba_owned_staged(mgba_path, &destination_rom, &marker).map_err(|_| {
            coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
        })?;
    let sidecar = CommandSpec::sidecar_template(sidecar_path)
        .and_then(|spec| spec.with_arrival_verifier())
        .map_err(|_| {
            coop_launcher::arrival_verifier::ArrivalVerificationError::InvalidProcessSpec
        })?;
    verify_arrival(ArrivalVerificationInput {
        expected_full_sav_sha256: destination_save_sha256,
        staged_sav: &destination_save,
        registry,
        destination_world,
        expected_save_generation: destination_save_generation,
        expected_map_group: arrival_location[0],
        expected_map_num: arrival_location[1],
        persisted_nonce: nonce,
        workspace: &workspace,
        sidecar,
        mgba,
        bridge_source: bridge_path,
    })
    .await
}

async fn portal_failure(
    mut session: SessionLifecycle,
    api: &coop_launcher::ReqwestCloudApi,
    world_intent: Option<PendingWorldAcquire>,
) -> RuntimeCompletion {
    let released = session.release_lease_keep_credentials(api).await.is_ok();
    clear_world_intent_after_confirmed_release(world_intent, released);
    RuntimeCompletion {
        auth: Some(session.auth),
        outcome: RuntimeOutcome::Exited { clean_stop: false },
    }
}

async fn retain_auth(
    mut session: SessionLifecycle,
    api: &coop_launcher::ReqwestCloudApi,
    world_intent: Option<PendingWorldAcquire>,
) -> RuntimeCompletion {
    let clean_stop = session.release_lease_keep_credentials(api).await.is_ok();
    clear_world_intent_after_confirmed_release(world_intent, clean_stop);
    let auth = session.auth;
    RuntimeCompletion {
        auth: Some(auth),
        outcome: startup_failure_outcome(clean_stop),
    }
}

fn clear_world_intent_after_confirmed_release(
    world_intent: Option<PendingWorldAcquire>,
    release_confirmed: bool,
) {
    // A durable request is cleared only after the server has confirmed the
    // release. If release is ambiguous, leave it for the next cold start to
    // receive the exact closed-key (410) result before minting a new key.
    if release_confirmed {
        if let Some((store, request)) = world_intent {
            let _ = store.clear_exact(request);
        }
    }
}

fn startup_failure_outcome(clean_stop: bool) -> RuntimeOutcome {
    if clean_stop {
        RuntimeOutcome::StartupFailed(StartFailure::Unavailable)
    } else {
        RuntimeOutcome::Exited { clean_stop: false }
    }
}

fn send_auth_failure(events: &mpsc::Sender<BackendEvent>, failure: coop_launcher::AuthFailure) {
    let _ = events.send(BackendEvent::AuthenticationFailed(failure));
}

fn map_auth_failure(error: &AuthError) -> coop_launcher::AuthFailure {
    match error {
        AuthError::InvalidCredentials | AuthError::RegistrationComplete => {
            coop_launcher::AuthFailure::Rejected
        }
        AuthError::RefreshExpired => coop_launcher::AuthFailure::SessionExpired,
        _ => coop_launcher::AuthFailure::Unavailable,
    }
}

fn map_service_failure(error: &ReleaseError) -> coop_launcher::ServiceFailure {
    match error {
        ReleaseError::Unauthorized => coop_launcher::ServiceFailure::Unauthorized,
        ReleaseError::Transport | ReleaseError::InvalidEndpoint => {
            coop_launcher::ServiceFailure::Unavailable
        }
        _ => coop_launcher::ServiceFailure::NotReady,
    }
}

fn map_update_failure(error: &UpdateError) -> UpdateFailure {
    match error {
        UpdateError::SignatureInvalid
        | UpdateError::KeyIdMismatch
        | UpdateError::MalformedEnvelope => UpdateFailure::SignatureInvalid,
        UpdateError::ArtifactDigestMismatch(_) | UpdateError::ArtifactSizeMismatch { .. } => {
            UpdateFailure::ArtifactInvalid
        }
        UpdateError::UnsupportedPlatform(_)
        | UpdateError::UnsupportedSchema(_)
        | UpdateError::ReleaseRollback { .. }
        | UpdateError::SequenceConflict => UpdateFailure::Incompatible,
        _ => UpdateFailure::ActivationFailed,
    }
}

fn now_seconds() -> Result<i64, ()> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or(())
}

fn hex_digest(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn new_world_acquire_request(
    character_id: coop_cloud::CharacterId,
) -> Result<AcquireLeaseRequest, ()> {
    let client_instance_id = ClientInstanceId::new(uuid::Uuid::new_v4()).map_err(|_| ())?;
    let idempotency_key = IdempotencyKey::new(uuid::Uuid::new_v4()).map_err(|_| ())?;
    Ok(AcquireLeaseRequest::new(
        character_id,
        client_instance_id,
        idempotency_key,
    ))
}

fn runtime_completion_event(stop_requested: bool, outcome: RuntimeOutcome) -> BackendEvent {
    match outcome {
        RuntimeOutcome::StartupFailed(_) if stop_requested => BackendEvent::StopCompleted,
        RuntimeOutcome::StartupFailed(failure) => BackendEvent::StartFailed(failure),
        RuntimeOutcome::Exited { clean_stop: true } if stop_requested => {
            BackendEvent::StopCompleted
        }
        RuntimeOutcome::Exited { .. } => BackendEvent::ShutdownUncertain,
        RuntimeOutcome::Continue(_) => BackendEvent::ShutdownUncertain,
    }
}

fn delete_local_account(
    keychain: &(impl RefreshTokenStore + ?Sized),
    paths: &UserPaths,
    account: &AccountRecord,
) -> Result<(), ()> {
    keychain
        .delete(KEYCHAIN_SERVICE, &account.username)
        .map_err(|_| ())?;
    paths.remove_account().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{
        BackendConfig, BackendEvent, RuntimeOutcome, delete_local_account, reusable_prepare_key,
        runtime_completion_event, spawn_backend,
    };
    use crate::config::{AccountRecord, RuntimeConfig, UserPaths};
    use coop_cloud::{CharacterId, IdempotencyKey, RefreshToken, UserId};
    use coop_launcher::{
        KeychainError, RefreshTokenStore, TrustedManifestKey, TrustedReleaseKey,
        rom_travel::{RomTravelJournal, TravelPhase},
        session::RomWorldId,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    struct DeleteOnlyKeychain(AtomicBool);

    #[test]
    fn return_trip_does_not_reuse_the_previous_committed_handoff_key() {
        let root = tempfile::tempdir().unwrap();
        let main = RomWorldId::new(1).unwrap();
        let cormoria = RomWorldId::new(2).unwrap();
        let journal = RomTravelJournal::new(
            root.path(),
            CharacterId::new(uuid::Uuid::from_u128(41)).unwrap(),
            [main, cormoria],
        )
        .unwrap();
        let mut record = journal.initialize(main).unwrap();
        let previous_key = IdempotencyKey::new(uuid::Uuid::from_u128(42)).unwrap();
        record.active_world = cormoria;
        record.phase = TravelPhase::Committed;
        record.prepare_idempotency_key = Some(previous_key);

        assert_eq!(reusable_prepare_key(&record, cormoria), None);
        record.phase = TravelPhase::Aborted;
        assert_eq!(reusable_prepare_key(&record, cormoria), None);
        record.phase = TravelPhase::PrepareIntent;
        assert_eq!(reusable_prepare_key(&record, cormoria), Some(previous_key));
        assert_eq!(reusable_prepare_key(&record, main), None);
    }

    #[test]
    fn startup_cleanup_failure_requires_recovery_even_after_stop() {
        for stop_requested in [false, true] {
            assert!(matches!(
                runtime_completion_event(stop_requested, super::startup_failure_outcome(false)),
                BackendEvent::ShutdownUncertain
            ));
        }
    }

    #[tokio::test]
    async fn start_retry_attempts_saved_auth_when_acquisition_consumed_memory_auth() {
        struct MissingCredential(std::sync::atomic::AtomicUsize);
        impl RefreshTokenStore for MissingCredential {
            fn load(&self, _: &str, _: &str) -> Result<Option<RefreshToken>, KeychainError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
            fn store(&self, _: &str, _: &str, _: &RefreshToken) -> Result<(), KeychainError> {
                panic!("missing credential must not be stored")
            }
            fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
                panic!("retry must not delete credentials")
            }
        }
        let root = tempfile::tempdir().expect("temporary directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("paths");
        let runtime = RuntimeConfig::for_test(
            "http://127.0.0.1:9",
            TrustedReleaseKey::new(
                "release-test",
                ed25519_dalek::SigningKey::from_bytes(&[1; 32])
                    .verifying_key()
                    .to_bytes(),
            )
            .unwrap(),
            TrustedManifestKey::new(
                "manifest-test",
                ed25519_dalek::SigningKey::from_bytes(&[2; 32])
                    .verifying_key()
                    .to_bytes(),
            )
            .unwrap(),
        );
        let config = BackendConfig::for_test(paths, runtime).unwrap();
        config
            .paths
            .save_account(&AccountRecord {
                username: "retry-player".into(),
                user_id: UserId::new(uuid::Uuid::from_u128(41)).unwrap(),
                character_id: CharacterId::new(uuid::Uuid::from_u128(42)).unwrap(),
            })
            .unwrap();
        let keychain =
            std::sync::Arc::new(MissingCredential(std::sync::atomic::AtomicUsize::new(0)));
        let mut actor = super::BackendActor {
            api: coop_launcher::ReqwestCloudApi::new(&config.runtime.api_base).unwrap(),
            release: super::ReleaseClient::new(
                &config.runtime.api_base,
                config.runtime.release_key.clone(),
            )
            .unwrap(),
            store: super::GenerationStore::new(config.paths.generations_root()).unwrap(),
            config,
            keychain: keychain.clone(),
            auth: None,
            pending_release: None,
            runtime: None,
            commands: tokio::sync::mpsc::unbounded_channel().0,
            shutdown_ack: None,
            pending_credential_cleanup: None,
        };
        let (events, receiver) = std::sync::mpsc::channel();
        actor.start_runtime(&events).await;
        assert_eq!(
            keychain.0.load(Ordering::SeqCst),
            1,
            "Retry must attempt credential recovery instead of permanently rejecting absent in-memory auth"
        );
        assert!(matches!(
            receiver.try_recv().unwrap(),
            BackendEvent::StartFailed(_)
        ));
    }

    impl RefreshTokenStore for DeleteOnlyKeychain {
        fn load(
            &self,
            _service: &str,
            _username: &str,
        ) -> Result<Option<RefreshToken>, KeychainError> {
            panic!("offline sign-out must not refresh")
        }

        fn store(
            &self,
            _service: &str,
            _username: &str,
            _token: &RefreshToken,
        ) -> Result<(), KeychainError> {
            panic!("offline sign-out must not store")
        }

        fn delete(&self, _service: &str, _username: &str) -> Result<(), KeychainError> {
            self.0.store(true, Ordering::Release);
            Ok(())
        }
    }

    #[test]
    fn runtime_completion_distinguishes_startup_failure_from_uncertain_shutdown() {
        assert!(matches!(
            runtime_completion_event(
                false,
                RuntimeOutcome::StartupFailed(coop_launcher::StartFailure::Unavailable)
            ),
            BackendEvent::StartFailed(coop_launcher::StartFailure::Unavailable)
        ));
        assert!(matches!(
            runtime_completion_event(
                true,
                RuntimeOutcome::StartupFailed(coop_launcher::StartFailure::Unavailable)
            ),
            BackendEvent::StopCompleted
        ));
        assert!(matches!(
            runtime_completion_event(false, RuntimeOutcome::Exited { clean_stop: true }),
            BackendEvent::ShutdownUncertain
        ));
        assert!(matches!(
            runtime_completion_event(true, RuntimeOutcome::Exited { clean_stop: true }),
            BackendEvent::StopCompleted
        ));
    }

    #[test]
    fn idle_backend_shutdown_is_acknowledged_and_joined() {
        let root = tempfile::tempdir().expect("temporary directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("user paths");
        let release_bytes = ed25519_dalek::SigningKey::from_bytes(&[1; 32])
            .verifying_key()
            .to_bytes();
        let manifest_bytes = ed25519_dalek::SigningKey::from_bytes(&[2; 32])
            .verifying_key()
            .to_bytes();
        let runtime = RuntimeConfig::for_test(
            "http://127.0.0.1:9",
            TrustedReleaseKey::new("release-test", release_bytes).expect("release key"),
            TrustedManifestKey::new("manifest-test", manifest_bytes).expect("manifest key"),
        );
        let backend = spawn_backend(BackendConfig::for_test(paths, runtime).expect("config"))
            .expect("backend");

        assert!(backend.shutdown());
    }

    #[test]
    fn malformed_cleanup_marker_blocks_backend_startup() {
        let root = tempfile::tempdir().expect("temporary directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("user paths");
        paths.ensure_directories().expect("state directory");
        std::fs::write(
            paths.state_root().join("credential-cleanup.txt"),
            b"truncated",
        )
        .expect("malformed cleanup marker");
        let release_bytes = ed25519_dalek::SigningKey::from_bytes(&[1; 32])
            .verifying_key()
            .to_bytes();
        let manifest_bytes = ed25519_dalek::SigningKey::from_bytes(&[2; 32])
            .verifying_key()
            .to_bytes();
        let runtime = RuntimeConfig::for_test(
            "http://127.0.0.1:9",
            TrustedReleaseKey::new("release-test", release_bytes).expect("release key"),
            TrustedManifestKey::new("manifest-test", manifest_bytes).expect("manifest key"),
        );

        assert!(spawn_backend(BackendConfig::for_test(paths, runtime).expect("config")).is_err());
    }

    #[test]
    fn persisted_sign_out_deletes_locally_without_refreshing() {
        let root = tempfile::tempdir().expect("temporary directory");
        let paths = UserPaths::from_local_app_data(root.path()).expect("user paths");
        let account = AccountRecord {
            username: "offline-player".to_owned(),
            user_id: UserId::new(uuid::Uuid::from_u128(41)).expect("user id"),
            character_id: CharacterId::new(uuid::Uuid::from_u128(42)).expect("character id"),
        };
        paths.save_account(&account).expect("saved account");
        let keychain = DeleteOnlyKeychain(AtomicBool::new(false));

        delete_local_account(&keychain, &paths, &account).expect("local sign-out");

        assert!(keychain.0.load(Ordering::Acquire));
        assert_eq!(paths.load_account().expect("load account"), None);
    }
}

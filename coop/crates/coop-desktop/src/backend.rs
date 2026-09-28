//! Single-owner backend actor for authentication, releases, and runtime use.

use std::{
    fs::{self, OpenOptions},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};

use coop_cloud::{
    AcquireLeaseRequest, ApiVersion, ClientInstanceId, GroupTravelProposalStatus, IdempotencyKey,
    InvitationCode, ReleaseLeaseRequest, StoryTravelRecoveryAction, StoryTravelRecoveryOutcome,
    StoryTravelRecoveryView,
};
use coop_launcher::{
    ArtifactIdentity, AuthError, AuthSession, BuildCompatibility, CloudApi, CommandSpec, Effect,
    EpochStore, OsKeychain, RecoveryDiscovery, RecoveryMarker, RecoveryOutcome, RecoveryReconciler,
    RecoveryResult, RefreshTokenStore, ReleaseReadiness, SessionConfig, SessionError,
    SessionLifecycle, StartFailure, TrustedManifestKey, UpdateFailure,
    process::{SupervisedChildren, staged_rom_marker_contents, staged_rom_marker_path},
    update::{GenerationStore, UpdateError},
};
use thiserror::Error;
use tokio::{
    runtime::Runtime,
    sync::{mpsc as tokio_mpsc, oneshot},
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
    PartnerStatus(Result<coop_cloud::PartnerStatusResponse, ()>),
    StoryRecoveryInspected(StoryRecoveryStatus),
    StoryRecoveryAbandoned(StoryRecoveryStatus),
}

/// Deliberately safe desktop copy for a fenced story-travel recovery check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryRecoveryStatus {
    /// Server confirmed an unresolved scene marker in a closed group.
    AbandonAvailable,
    /// A marked scene is still waiting for its group to close.
    GroupStillActive,
    /// No unresolved marker remains; ordinary Play can be retried.
    Clear,
    /// Server refused Abandon because a finalized scene or changed head exists.
    MustReconcile,
    /// The request or lease could not be completed; no local recovery was discarded.
    Unavailable,
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

    pub fn fetch_partner_status(&self) -> Result<(), BackendError> {
        self.commands
            .send(BackendCommand::FetchPartnerStatus)
            .map_err(|_| BackendError::Closed)
    }

    pub fn inspect_story_recovery(&self) -> Result<(), BackendError> {
        self.commands
            .send(BackendCommand::InspectStoryRecovery)
            .map_err(|_| BackendError::Closed)
    }

    pub fn abandon_story_recovery(&self) -> Result<(), BackendError> {
        self.commands
            .send(BackendCommand::AbandonStoryRecovery)
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
    FetchPartnerStatus,
    InspectStoryRecovery,
    AbandonStoryRecovery,
    RuntimeStarted,
    RuntimeFinished(RuntimeCompletion),
    Shutdown(mpsc::Sender<()>),
}

struct RuntimeHandle {
    stop: Option<oneshot::Sender<()>>,
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

#[derive(Debug)]
struct RuntimeCompletion {
    auth: Option<AuthSession>,
    outcome: RuntimeOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeOutcome {
    StartupFailed(StartFailure),
    Exited { clean_stop: bool },
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
                BackendCommand::FetchPartnerStatus => {
                    let status = if let Some(auth) = self.auth.as_ref() {
                        self.api.partner_status(auth).await.map_err(|_| ())
                    } else {
                        Err(())
                    };
                    let _ = events.send(BackendEvent::PartnerStatus(status));
                }
                BackendCommand::InspectStoryRecovery => {
                    let status = self.story_recovery(false).await;
                    let _ = events.send(BackendEvent::StoryRecoveryInspected(status));
                }
                BackendCommand::AbandonStoryRecovery => {
                    let status = self.story_recovery(true).await;
                    let _ = events.send(BackendEvent::StoryRecoveryAbandoned(status));
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

    async fn story_recovery(&mut self, abandon: bool) -> StoryRecoveryStatus {
        // A failed startup consumed the in-memory AuthSession. The refresh
        // credential remains in the keychain, and this operation must leave it
        // there even when the server refuses the requested action.
        if self.runtime.is_some() {
            return StoryRecoveryStatus::Unavailable;
        }
        let mut auth = match self.take_or_resume_auth().await {
            Ok(auth) => auth,
            Err(_) => return StoryRecoveryStatus::Unavailable,
        };
        let result =
            fenced_story_recovery(&self.api, &mut auth, self.keychain.as_ref(), abandon).await;
        self.auth = Some(auth);
        result
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
            manifest: compatibility,
            trusted_manifest_key: self.config.runtime.manifest_key.clone(),
            epoch_store: EpochStore::new(self.config.paths.epoch_file()),
            workspace_parent: self.config.paths.workspace_parent().to_owned(),
            bridge_lua_dir: generation.handoff().path().join("bridge"),
        })
    }
}

fn story_recovery_status(
    recovery: Option<&StoryTravelRecoveryView>,
    character_id: coop_cloud::CharacterId,
) -> StoryRecoveryStatus {
    match recovery {
        None => StoryRecoveryStatus::Clear,
        Some(view)
            if view.api_version == ApiVersion::V1
                && view.marker_fence.character_id == character_id
                && view.scene_nonce != 0 =>
        {
            match view.status {
                GroupTravelProposalStatus::Cancelled => StoryRecoveryStatus::AbandonAvailable,
                GroupTravelProposalStatus::AwaitingSceneReceipts
                | GroupTravelProposalStatus::Suspended => StoryRecoveryStatus::GroupStillActive,
                _ => StoryRecoveryStatus::Unavailable,
            }
        }
        Some(_) => StoryRecoveryStatus::Unavailable,
    }
}

fn abandon_proposal_id(
    recovery: Option<&StoryTravelRecoveryView>,
    character_id: coop_cloud::CharacterId,
) -> Option<coop_cloud::GroupTravelProposalId> {
    recovery
        .filter(|view| {
            story_recovery_status(Some(view), character_id) == StoryRecoveryStatus::AbandonAvailable
        })
        .map(|view| view.proposal_id)
}

async fn refresh_story_auth<A: CloudApi>(
    api: &A,
    auth: &mut AuthSession,
    keychain: &dyn RefreshTokenStore,
) -> Result<(), ()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_millis() as u64;
    if auth.should_refresh_at(now) {
        auth.refresh(api, keychain).await.map_err(|_| ())?;
    }
    Ok(())
}

async fn fenced_story_recovery<A: CloudApi>(
    api: &A,
    auth: &mut AuthSession,
    keychain: &dyn RefreshTokenStore,
    abandon: bool,
) -> StoryRecoveryStatus {
    if refresh_story_auth(api, auth, keychain).await.is_err() {
        return StoryRecoveryStatus::Unavailable;
    }
    let Ok(client) = ClientInstanceId::new(uuid::Uuid::new_v4()) else {
        return StoryRecoveryStatus::Unavailable;
    };
    let Ok(acquire_key) = IdempotencyKey::new(uuid::Uuid::new_v4()) else {
        return StoryRecoveryStatus::Unavailable;
    };
    let acquire = AcquireLeaseRequest::new(auth.character_id, client, acquire_key);
    let mut lease = api.acquire(auth, acquire).await;
    if matches!(lease, Err(SessionError::Unauthorized)) {
        if auth.refresh(api, keychain).await.is_err() {
            return StoryRecoveryStatus::Unavailable;
        }
        lease = api.acquire(auth, acquire).await;
    } else if matches!(lease, Err(SessionError::Cloud)) {
        // An acquire response may have been lost after the server committed it.
        // Replaying the same idempotency key avoids creating another lease.
        lease = api.acquire(auth, acquire).await;
    }
    let Ok(lease) = lease else {
        return StoryRecoveryStatus::Unavailable;
    };
    let fence = lease.fence();
    let result = if lease.validate().is_err()
        || lease.character_id != auth.character_id
        || lease.client_instance_id != client
    {
        StoryRecoveryStatus::Unavailable
    } else {
        let result = story_recovery_under_lease(api, auth, keychain, fence, abandon).await;
        result
    };
    // Release on every path after acquire, including a refused Abandon. Keep
    // the rotating refresh credential for another inspection or Play attempt.
    let Ok(release_key) = IdempotencyKey::new(uuid::Uuid::new_v4()) else {
        return StoryRecoveryStatus::Unavailable;
    };
    let release = ReleaseLeaseRequest::new(fence, release_key);
    if refresh_story_auth(api, auth, keychain).await.is_err() {
        return StoryRecoveryStatus::Unavailable;
    }
    let mut released = api.release(auth, release).await;
    if matches!(released, Err(SessionError::Unauthorized))
        && auth.refresh(api, keychain).await.is_ok()
    {
        released = api.release(auth, release).await;
    }
    if released.is_err() {
        StoryRecoveryStatus::Unavailable
    } else {
        result
    }
}

async fn story_recovery_under_lease<A: CloudApi>(
    api: &A,
    auth: &mut AuthSession,
    keychain: &dyn RefreshTokenStore,
    fence: coop_cloud::LeaseFence,
    abandon: bool,
) -> StoryRecoveryStatus {
    use coop_launcher::GroupTravelError;

    if refresh_story_auth(api, auth, keychain).await.is_err() {
        return StoryRecoveryStatus::Unavailable;
    }
    let Some(token) = auth.access_token().cloned() else {
        return StoryRecoveryStatus::Unavailable;
    };
    let mut recovery = api.story_travel_recovery(token, fence).await;
    if matches!(recovery, Err(GroupTravelError::Unauthorized))
        && auth.refresh(api, keychain).await.is_ok()
    {
        let Some(token) = auth.access_token().cloned() else {
            return StoryRecoveryStatus::Unavailable;
        };
        recovery = api.story_travel_recovery(token, fence).await;
    }
    let Ok(recovery) = recovery else {
        return StoryRecoveryStatus::Unavailable;
    };
    let status = story_recovery_status(recovery.as_ref(), auth.character_id);
    if !abandon || status != StoryRecoveryStatus::AbandonAvailable {
        return status;
    }
    let Some(proposal_id) = abandon_proposal_id(recovery.as_ref(), auth.character_id) else {
        return StoryRecoveryStatus::Unavailable;
    };
    if refresh_story_auth(api, auth, keychain).await.is_err() {
        return StoryRecoveryStatus::Unavailable;
    }
    let Some(token) = auth.access_token().cloned() else {
        return StoryRecoveryStatus::Unavailable;
    };
    let mut action = api
        .story_travel_recovery_action(
            token,
            proposal_id,
            fence,
            StoryTravelRecoveryAction::Abandon,
        )
        .await;
    if matches!(action, Err(GroupTravelError::Unauthorized))
        && auth.refresh(api, keychain).await.is_ok()
    {
        let Some(token) = auth.access_token().cloned() else {
            return StoryRecoveryStatus::Unavailable;
        };
        action = api
            .story_travel_recovery_action(
                token,
                proposal_id,
                fence,
                StoryTravelRecoveryAction::Abandon,
            )
            .await;
    }
    match action {
        Ok(view)
            if view.api_version == ApiVersion::V1
                && view.proposal_id == proposal_id
                && view.outcome == StoryTravelRecoveryOutcome::Abandoned =>
        {
            StoryRecoveryStatus::Clear
        }
        Err(GroupTravelError::Stale) => StoryRecoveryStatus::MustReconcile,
        _ => StoryRecoveryStatus::Unavailable,
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
        manifest: compatibility,
        trusted_manifest_key,
        epoch_store: EpochStore::new(paths.epoch_file()),
        workspace_parent: paths.workspace_parent().to_owned(),
        bridge_lua_dir: bridge_path.clone(),
    };
    let mut session = match SessionLifecycle::acquire_with_keychain(&api, auth, config, keychain)
        .await
    {
        Ok(session) => session,
        Err(SessionError::StoryTravelRecoveryPending) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::StartupFailed(StartFailure::StoryTravelRecoveryPending),
            };
        }
        Err(_) => {
            return RuntimeCompletion {
                auth: None,
                outcome: RuntimeOutcome::StartupFailed(StartFailure::Unavailable),
            };
        }
    };
    let staged_rom = session.workspace.path().join("game.gba");
    if fs::copy(&rom_path, &staged_rom).is_err() {
        return retain_auth(session, &api).await;
    }
    let marker = staged_rom_marker_path(&staged_rom);
    let marker_bytes = match staged_rom_marker_contents(&staged_rom) {
        Ok(bytes) => bytes,
        Err(_) => return retain_auth(session, &api).await,
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
        return retain_auth(session, &api).await;
    }
    let mgba = match CommandSpec::mgba_owned_staged(&mgba_path, &staged_rom, &marker) {
        Ok(spec) => spec,
        Err(_) => return retain_auth(session, &api).await,
    };
    let sidecar = match CommandSpec::sidecar_template(&sidecar_path)
        .and_then(|spec| spec.with_session_epoch(session.lease.session_epoch.value()))
    {
        Ok(spec) => spec,
        Err(_) => return retain_auth(session, &api).await,
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
        Err(_) => return retain_auth(session, &api).await,
    };
    let _ = commands.send(BackendCommand::RuntimeStarted);
    let lifecycle = session
        .run_until_shutdown(&api, &mut children, async move {
            let _ = stop_rx.await;
        })
        .await;
    let stopped = children.stop().await;
    if lifecycle.is_err() || stopped.is_err() {
        let _ = session.preserve_recovery_after_child_failure();
        let _ = session.close_credentials(&api).await;
        return RuntimeCompletion {
            auth: None,
            outcome: RuntimeOutcome::Exited { clean_stop: false },
        };
    }
    let released = session.release_lease_keep_credentials(&api).await.is_ok();
    let auth = session.auth;
    RuntimeCompletion {
        auth: Some(auth),
        outcome: RuntimeOutcome::Exited {
            clean_stop: released,
        },
    }
}

async fn retain_auth(
    mut session: SessionLifecycle,
    api: &coop_launcher::ReqwestCloudApi,
) -> RuntimeCompletion {
    let clean_stop = session.release_lease_keep_credentials(api).await.is_ok();
    let auth = session.auth;
    RuntimeCompletion {
        auth: Some(auth),
        outcome: startup_failure_outcome(clean_stop),
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

fn runtime_completion_event(stop_requested: bool, outcome: RuntimeOutcome) -> BackendEvent {
    match outcome {
        RuntimeOutcome::StartupFailed(_) if stop_requested => BackendEvent::StopCompleted,
        RuntimeOutcome::StartupFailed(failure) => BackendEvent::StartFailed(failure),
        RuntimeOutcome::Exited { clean_stop: true } if stop_requested => {
            BackendEvent::StopCompleted
        }
        RuntimeOutcome::Exited { .. } => BackendEvent::ShutdownUncertain,
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
        BackendConfig, BackendEvent, RuntimeOutcome, StoryRecoveryStatus, abandon_proposal_id,
        delete_local_account, runtime_completion_event, spawn_backend, story_recovery_status,
    };
    use crate::config::{AccountRecord, RuntimeConfig, UserPaths};
    use coop_cloud::{CharacterId, RefreshToken, UserId};
    use coop_launcher::{KeychainError, RefreshTokenStore, TrustedManifestKey, TrustedReleaseKey};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn unfinished_scene_can_be_abandoned_only_after_closed_group_is_confirmed() {
        use coop_cloud::{
            ApiVersion, ClientInstanceId, GroupId, GroupTravelProposalId,
            GroupTravelProposalStatus, LeaseFence, Revision, SessionEpoch, SessionId,
            StoryTravelRecoveryView, UnixTimestampMillis,
        };
        let character = CharacterId::new(uuid::Uuid::from_u128(51)).unwrap();
        let fence = LeaseFence::new(
            SessionId::new(uuid::Uuid::from_u128(52)).unwrap(),
            character,
            Revision::new(1),
            SessionEpoch::new(1).unwrap(),
            ClientInstanceId::new(uuid::Uuid::from_u128(53)).unwrap(),
        );
        let mut marker = StoryTravelRecoveryView {
            api_version: ApiVersion::V1,
            proposal_id: GroupTravelProposalId::new(uuid::Uuid::from_u128(54)).unwrap(),
            group_id: GroupId::new(uuid::Uuid::from_u128(55)).unwrap(),
            status: GroupTravelProposalStatus::AwaitingSceneReceipts,
            marker_fence: fence,
            scene_nonce: 7,
            marked_at: UnixTimestampMillis::new(1),
        };
        for status in [
            GroupTravelProposalStatus::AwaitingSceneReceipts,
            GroupTravelProposalStatus::Suspended,
        ] {
            marker.status = status;
            assert_eq!(
                story_recovery_status(Some(&marker), character),
                StoryRecoveryStatus::GroupStillActive
            );
            assert_eq!(abandon_proposal_id(Some(&marker), character), None);
        }
        marker.status = GroupTravelProposalStatus::Cancelled;
        assert_eq!(
            abandon_proposal_id(Some(&marker), character),
            Some(marker.proposal_id)
        );
        assert_eq!(
            abandon_proposal_id(
                Some(&marker),
                CharacterId::new(uuid::Uuid::from_u128(56)).unwrap()
            ),
            None
        );
    }

    #[tokio::test]
    async fn refused_recovery_check_keeps_authenticated_session_for_retry() {
        use coop_cloud::{
            AccessToken, LoginRequest, LoginResponse, LogoutRequest, LogoutResponse,
            RefreshFamilyId, RefreshRequest, RefreshResponse, UnixTimestampMillis,
        };
        use coop_launcher::{AuthApi, AuthError, AuthSession};

        struct LoginOnly;
        impl AuthApi for LoginOnly {
            fn login(&self, _: LoginRequest) -> coop_launcher::auth::AuthFuture<'_, LoginResponse> {
                Box::pin(async {
                    LoginResponse::new(
                        UserId::new(uuid::Uuid::from_u128(61)).unwrap(),
                        CharacterId::new(uuid::Uuid::from_u128(62)).unwrap(),
                        AccessToken::new("test-access").unwrap(),
                        RefreshToken::new("test-refresh").unwrap(),
                        RefreshFamilyId::new(uuid::Uuid::from_u128(63)).unwrap(),
                        UnixTimestampMillis::new(u64::MAX / 4),
                        UnixTimestampMillis::new(u64::MAX / 4),
                    )
                    .map_err(|_| AuthError::InvalidResponse)
                })
            }
            fn refresh(
                &self,
                _: RefreshRequest,
            ) -> coop_launcher::auth::AuthFuture<'_, RefreshResponse> {
                Box::pin(async { Err(AuthError::Transport) })
            }
            fn logout(
                &self,
                _: LogoutRequest,
            ) -> coop_launcher::auth::AuthFuture<'_, LogoutResponse> {
                Box::pin(async { Err(AuthError::Transport) })
            }
        }
        struct RetainKeychain(AtomicBool);
        impl RefreshTokenStore for RetainKeychain {
            fn load(&self, _: &str, _: &str) -> Result<Option<RefreshToken>, KeychainError> {
                Ok(None)
            }
            fn store(&self, _: &str, _: &str, _: &RefreshToken) -> Result<(), KeychainError> {
                Ok(())
            }
            fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
                self.0.store(true, Ordering::SeqCst);
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let paths = UserPaths::from_local_app_data(root.path()).unwrap();
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
        let keychain = std::sync::Arc::new(RetainKeychain(AtomicBool::new(false)));
        let auth = AuthSession::login(
            &LoginOnly,
            keychain.as_ref(),
            "recovery-player",
            AuthSession::password("test-password").unwrap(),
        )
        .await
        .unwrap();
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
            auth: Some(auth),
            pending_release: None,
            runtime: None,
            commands: tokio::sync::mpsc::unbounded_channel().0,
            shutdown_ack: None,
            pending_credential_cleanup: None,
        };
        assert_eq!(
            actor.story_recovery(true).await,
            StoryRecoveryStatus::Unavailable
        );
        assert!(
            actor
                .auth
                .as_ref()
                .and_then(AuthSession::refresh_token)
                .is_some()
        );
        assert!(!keychain.0.load(Ordering::SeqCst));
    }

    struct DeleteOnlyKeychain(AtomicBool);

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

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
    AcquireLeaseRequest, AcquireWorldLeaseResponse, ApiVersion, ClientInstanceId,
    GroupRomHandoffAbortRequest, GroupRomHandoffJoinRequest, GroupTravelProposalStatus,
    HeartbeatLeaseRequest, IdempotencyKey, InvitationCode, OnlineSnapshotRequest,
    ReleaseLeaseRequest, RuntimeBuildIdentity, StoryTravelRecoveryAction,
    StoryTravelRecoveryOutcome, StoryTravelRecoveryView,
};
use coop_launcher::arrival_verifier::{
    ArrivalVerificationInput, AuthenticatedArrivalEvidence, verify_arrival,
};
use coop_launcher::paired_travel::{PairedJoinIntent, PairedPhase, PairedTerminal};
use coop_launcher::{
    AcceptedGeneration, ArtifactIdentity, AuthError, AuthSession, BuildCompatibility, CloudApi,
    CommandSpec, Effect, EpochStore, OsKeychain, RecoveryDiscovery, RecoveryMarker,
    RecoveryOutcome, RecoveryReconciler, RecoveryResult, RefreshTokenStore, ReleaseReadiness,
    SessionConfig, SessionError, SessionLifecycle, SessionWorkspace, StartFailure,
    TrustedManifestKey, TrustedRomCatalog, UpdateFailure, WorldAcquireIntentStore,
    live_requests::{LiveRequest, LiveRequestError, live_request_channel},
    paired_coordinator::{PairedHandoffOutcome, recover_paired_handoff, run_paired_handoff},
    paired_travel::PairedTravelJournal,
    process::{
        SessionSupervisor, SupervisedChildren, staged_rom_marker_contents, staged_rom_marker_path,
    },
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
    PartnerStatus(Result<coop_cloud::PartnerStatusResponse, ()>),
    PairingRedeemed(Result<(), JoinFailure>),
    StoryRecoveryInspected(StoryRecoveryStatus),
    StoryRecoveryAbandoned(StoryRecoveryStatus),
}

/// Why a desktop join by pairing code did not form a group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinFailure {
    /// The text is not a pairing code.
    InvalidCode,
    /// The game is not running, so no lease can redeem the code.
    NotRunning,
    /// The code is unknown, expired, used, or this character is already grouped.
    Refused,
    /// The service could not be reached; the code may still be valid.
    Unavailable,
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

    /// Redeems a pairing code through the running game's session.
    pub fn redeem_pairing_code(&self, code: String) -> Result<(), BackendError> {
        self.commands
            .send(BackendCommand::RedeemPairingCode(code))
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
    RedeemPairingCode(String),
    InspectStoryRecovery,
    AbandonStoryRecovery,
    RuntimeStarted(tokio_mpsc::Sender<LiveRequest>),
    RuntimeLiveRebound(tokio_mpsc::Sender<LiveRequest>),
    RuntimeFinished(RuntimeCompletion),
    Shutdown(mpsc::Sender<()>),
}

struct RuntimeHandle {
    stop: Option<oneshot::Sender<()>>,
    /// Requests into the running session; its token and lease stay there.
    live: Option<tokio_mpsc::Sender<LiveRequest>>,
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
    fn live_sender(&self) -> Option<tokio_mpsc::Sender<LiveRequest>> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.live.clone())
    }

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
                    if let Some(auth) = self.auth.as_ref() {
                        let status = self.api.partner_status(auth).await.map_err(|_| ());
                        let _ = events.send(BackendEvent::PartnerStatus(status));
                    } else if let Some(live) = self.live_sender() {
                        let (reply, answer) = oneshot::channel();
                        let events = events.clone();
                        tokio::spawn(async move {
                            let status = match live.send(LiveRequest::PartnerStatus(reply)).await {
                                Ok(()) => {
                                    answer.await.map_err(|_| ()).and_then(|r| r.map_err(|_| ()))
                                }
                                Err(_) => Err(()),
                            };
                            let _ = events.send(BackendEvent::PartnerStatus(status));
                        });
                    } else {
                        let _ = events.send(BackendEvent::PartnerStatus(Err(())));
                    }
                }
                BackendCommand::RedeemPairingCode(code) => {
                    let Ok(code) = coop_cloud::PairingCode::new(code.trim().to_ascii_uppercase())
                    else {
                        let _ = events
                            .send(BackendEvent::PairingRedeemed(Err(JoinFailure::InvalidCode)));
                        continue;
                    };
                    let Some(live) = self.live_sender() else {
                        let _ = events
                            .send(BackendEvent::PairingRedeemed(Err(JoinFailure::NotRunning)));
                        continue;
                    };
                    let (reply, answer) = oneshot::channel();
                    let events = events.clone();
                    tokio::spawn(async move {
                        let result = match live
                            .send(LiveRequest::RedeemPairingCode { code, reply })
                            .await
                        {
                            Ok(()) => match answer.await {
                                Ok(Ok(())) => Ok(()),
                                Ok(Err(LiveRequestError::Refused)) => Err(JoinFailure::Refused),
                                Ok(Err(LiveRequestError::Unavailable)) => {
                                    Err(JoinFailure::Unavailable)
                                }
                                Ok(Err(LiveRequestError::NotRunning)) | Err(_) => {
                                    Err(JoinFailure::NotRunning)
                                }
                            },
                            Err(_) => Err(JoinFailure::NotRunning),
                        };
                        let _ = events.send(BackendEvent::PairingRedeemed(result));
                    });
                }
                BackendCommand::InspectStoryRecovery => {
                    let status = self.story_recovery(false).await;
                    let _ = events.send(BackendEvent::StoryRecoveryInspected(status));
                }
                BackendCommand::AbandonStoryRecovery => {
                    let status = self.story_recovery(true).await;
                    let _ = events.send(BackendEvent::StoryRecoveryAbandoned(status));
                }
                BackendCommand::RuntimeStarted(live) => {
                    if let Some(runtime) = self.runtime.as_mut()
                        && runtime.stop.is_some()
                    {
                        runtime.live = Some(live);
                        let _ = events.send(BackendEvent::StartCompleted);
                    }
                }
                BackendCommand::RuntimeLiveRebound(live) => {
                    if let Some(runtime) = self.runtime.as_mut()
                        && runtime.stop.is_some()
                    {
                        runtime.live = Some(live);
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
            .is_none()
        {
            // A generation without a trusted region catalog predates
            // world-bound leases. The server rejects resume for an unbound
            // lease, so fail closed and let the release check deliver an
            // update instead of acquiring through the legacy route.
            self.auth = Some(auth);
            let _ = events.send(BackendEvent::StartFailed(StartFailure::NotReady));
            return;
        }
        self.start_world_runtime(auth, generation, events).await;
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
            let result = run_runtime_chain(
                NextRuntime {
                    api,
                    session,
                    keychain,
                    handoff,
                    trusted_manifest_key,
                    epoch_file,
                    workspace_parent,
                    sidecar_path,
                    rom_path,
                    mgba_path,
                    bridge_path,
                    world_intent: Some((intent, request)),
                    shutdown: Box::pin(async move { stop_signal.notified().await }),
                    portal: Some(portal),
                    announce_start: true,
                },
                &commands,
            )
            .await;
            let _ = commands.send(BackendCommand::RuntimeFinished(result));
        });
        self.runtime = Some(RuntimeHandle {
            stop: Some(stop_tx),
            live: None,
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
        // A generation without a trusted region catalog cannot bind a lease
        // to a ROM world, and the server rejects resume for an unbound lease.
        let selection = match self.recovery_world_selection() {
            Ok(selection) => selection,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        // Recovery owns a durable acquire intent separate from the play
        // intent: it is bound to the marker's prior client instance, while
        // play mints its own instance.
        let intent = match WorldAcquireIntentStore::new(
            self.config.paths.state_root().join("recovery-acquire"),
            auth.character_id,
        ) {
            Ok(intent) => intent,
            Err(_) => {
                self.auth = Some(auth);
                let _ = events.send(BackendEvent::RecoveryReconciled(
                    RecoveryResult::StillUncertain,
                ));
                return;
            }
        };
        let (result, auth) = reconcile_recovery_with_world(
            &self.api,
            &self.keychain,
            auth,
            &intent,
            client_instance_id,
            |response| selection.session_config(response),
        )
        .await;
        self.auth = auth;
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

    /// Loads the accepted generation's trusted region catalog for recovery.
    /// The world itself is chosen later from the server's acquire response.
    fn recovery_world_selection(&self) -> Result<RecoveryWorldSelection, BackendError> {
        let now = now_seconds().map_err(|_| BackendError::Store)?;
        let generation = self
            .store
            .open_accepted_current(self.release.trusted_key(), now)
            .map_err(|_| BackendError::Store)?;
        let catalog_artifact = generation
            .artifact(ArtifactIdentity::RegionCatalog)
            .ok_or(BackendError::Store)?;
        let mgba = generation
            .artifact(ArtifactIdentity::ManagedMgba)
            .ok_or(BackendError::Store)?;
        let catalog = TrustedRomCatalog::load(
            catalog_artifact.path(),
            &hex_digest(catalog_artifact.digest()),
        )
        .map_err(|_| BackendError::Store)?;
        Ok(RecoveryWorldSelection {
            catalog,
            mgba_path: mgba.path().to_owned(),
            manifest_key: self.config.runtime.manifest_key.clone(),
            epoch_file: self.config.paths.epoch_file().to_owned(),
            workspace_parent: self.config.paths.workspace_parent().to_owned(),
            bridge_lua_dir: generation.handoff().path().join("bridge"),
        })
    }
}

/// Local inputs for building a recovery session once the server has named
/// the active world.
struct RecoveryWorldSelection {
    catalog: TrustedRomCatalog,
    mgba_path: PathBuf,
    manifest_key: TrustedManifestKey,
    epoch_file: PathBuf,
    workspace_parent: PathBuf,
    bridge_lua_dir: PathBuf,
}

impl RecoveryWorldSelection {
    /// Mirrors `start_world_runtime`: the server-selected world must be in
    /// the trusted catalog and its pinned ROM must pass compatibility.
    fn session_config(&self, response: &AcquireWorldLeaseResponse) -> Result<SessionConfig, ()> {
        let selected = self
            .catalog
            .world(response.active_world_id)
            .map_err(|_| ())?;
        let compatibility = BuildCompatibility::validate(
            &selected.bridge_path,
            &selected.rom_path,
            &self.mgba_path,
        )
        .map_err(|_| ())?;
        selected
            .check_compatibility(&compatibility)
            .map_err(|_| ())?;
        Ok(SessionConfig {
            client_instance_id: response.lease.client_instance_id,
            rom_world_id: response.active_world_id,
            manifest: compatibility,
            trusted_manifest_key: self.manifest_key.clone(),
            epoch_store: EpochStore::new(self.epoch_file.clone()),
            workspace_parent: self.workspace_parent.clone(),
            bridge_lua_dir: self.bridge_lua_dir.clone(),
        })
    }
}

/// Crash recovery under a world-bound lease.
///
/// The request is persisted in `intent` before it is sent and carries the
/// marker's prior client instance. A closed key (410) is rotated once with a
/// fresh idempotency key for the same instance. When `configure` cannot map
/// the server's `active_world_id` to a compatible local world, the lease is
/// released without materializing a session and the evidence is untouched.
/// The intent is cleared only after the server confirmed the release.
async fn reconcile_recovery_with_world<A: CloudApi>(
    api: &A,
    keychain: &Arc<dyn RefreshTokenStore>,
    mut auth: AuthSession,
    intent: &WorldAcquireIntentStore,
    client_instance_id: ClientInstanceId,
    configure: impl FnOnce(&AcquireWorldLeaseResponse) -> Result<SessionConfig, ()>,
) -> (RecoveryResult, Option<AuthSession>) {
    let Ok(mut request) =
        recovery_acquire_request(api, keychain, &mut auth, intent, client_instance_id).await
    else {
        return (RecoveryResult::StillUncertain, Some(auth));
    };
    let response = match SessionLifecycle::acquire_world_with_keychain(
        api, &mut auth, request, keychain,
    )
    .await
    {
        Ok(response) => response,
        Err(SessionError::AcquireClosed) => {
            // The server proved this exact key can no longer acquire.
            // Rotate only the key; V2 recovery keeps the prior instance.
            if intent.clear_exact(request).is_err() {
                return (RecoveryResult::StillUncertain, Some(auth));
            }
            request = match persist_recovery_request(intent, auth.character_id, client_instance_id)
            {
                Ok(request) => request,
                Err(()) => return (RecoveryResult::StillUncertain, Some(auth)),
            };
            match SessionLifecycle::acquire_world_with_keychain(api, &mut auth, request, keychain)
                .await
            {
                Ok(response) => response,
                Err(_) => return (RecoveryResult::StillUncertain, Some(auth)),
            }
        }
        Err(_) => return (RecoveryResult::StillUncertain, Some(auth)),
    };
    let config = match configure(&response) {
        Ok(config)
            if config.rom_world_id == response.active_world_id
                && config.client_instance_id == client_instance_id =>
        {
            config
        }
        _ => {
            if SessionLifecycle::release_preacquired_world_lease(api, &mut auth, response, keychain)
                .await
                .is_ok()
            {
                let _ = intent.clear_exact(request);
            }
            return (RecoveryResult::StillUncertain, Some(auth));
        }
    };
    match RecoveryReconciler::reconcile_world_lease(
        api,
        auth,
        config,
        Arc::clone(keychain),
        response,
    )
    .await
    {
        Ok(session) => {
            // Success always follows a confirmed lease release.
            let _ = intent.clear_exact(request);
            let result = match session.outcome {
                RecoveryOutcome::AuthorizationMissing => RecoveryResult::StillUncertain,
                RecoveryOutcome::NoEvidence
                | RecoveryOutcome::RetiredLegacy
                | RecoveryOutcome::RetiredCommittedV2
                | RecoveryOutcome::CommittedV2 => RecoveryResult::Reconciled,
            };
            (result, Some(session.auth))
        }
        // The intent stays: a retry replays it and learns whether the lease
        // is still live or closed before minting a replacement.
        Err(_) => (RecoveryResult::StillUncertain, None),
    }
}

/// Returns the durable recovery request for `client_instance_id`. A stale
/// intent left for other evidence is settled with the server first.
async fn recovery_acquire_request<A: CloudApi>(
    api: &A,
    keychain: &Arc<dyn RefreshTokenStore>,
    auth: &mut AuthSession,
    intent: &WorldAcquireIntentStore,
    client_instance_id: ClientInstanceId,
) -> Result<AcquireLeaseRequest, ()> {
    match intent.read() {
        Ok(Some(existing)) if existing.request.client_instance_id == client_instance_id => {
            return Ok(existing.request);
        }
        Ok(Some(existing)) => {
            match SessionLifecycle::acquire_world_with_keychain(
                api,
                auth,
                existing.request,
                keychain,
            )
            .await
            {
                Ok(response) => {
                    SessionLifecycle::release_preacquired_world_lease(
                        api, auth, response, keychain,
                    )
                    .await
                    .map_err(|_| ())?;
                }
                Err(SessionError::AcquireClosed) => {}
                Err(_) => return Err(()),
            }
            intent.clear_exact(existing.request).map_err(|_| ())?;
        }
        Ok(None) => {}
        Err(_) => return Err(()),
    }
    persist_recovery_request(intent, auth.character_id, client_instance_id)
}

fn persist_recovery_request(
    intent: &WorldAcquireIntentStore,
    character_id: coop_cloud::CharacterId,
    client_instance_id: ClientInstanceId,
) -> Result<AcquireLeaseRequest, ()> {
    let idempotency_key = IdempotencyKey::new(uuid::Uuid::new_v4()).map_err(|_| ())?;
    intent
        .load_or_create(AcquireLeaseRequest::new(
            character_id,
            client_instance_id,
            idempotency_key,
        ))
        .map_err(|_| ())
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

async fn run_runtime_chain(
    mut next: NextRuntime,
    commands: &tokio_mpsc::UnboundedSender<BackendCommand>,
) -> RuntimeCompletion {
    loop {
        let NextRuntime {
            api,
            session,
            keychain,
            handoff,
            trusted_manifest_key,
            epoch_file,
            workspace_parent,
            sidecar_path,
            rom_path,
            mgba_path,
            bridge_path,
            world_intent,
            shutdown,
            portal,
            announce_start,
        } = next;
        let result = run_runtime_with_session(
            api,
            session,
            keychain,
            handoff,
            trusted_manifest_key,
            epoch_file,
            workspace_parent,
            sidecar_path,
            rom_path,
            mgba_path,
            bridge_path,
            world_intent,
            shutdown,
            portal,
            announce_start,
            commands,
        )
        .await;
        match result {
            RuntimeCompletion {
                outcome: RuntimeOutcome::Continue(leg),
                ..
            } => next = *leg,
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
    let (live, live_requests) = live_request_channel();
    session.serve_live_requests(live_requests);
    let started = if announce_start {
        BackendCommand::RuntimeStarted(live)
    } else {
        BackendCommand::RuntimeLiveRebound(live)
    };
    let _ = commands.send(started);
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
            .run_until_shutdown_with_realtime(&api, &mut children, shutdown)
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
            api,
            session: destination,
            keychain,
            handoff,
            trusted_manifest_key,
            epoch_file,
            workspace_parent,
            sidecar_path,
            rom_path: destination_rom_path,
            mgba_path,
            bridge_path,
            world_intent: Some((intent, new_request)),
            shutdown: Box::pin(async move { stop.notified().await }),
            portal: Some(destination_portal),
            announce_start,
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
            api,
            session: destination,
            keychain,
            handoff,
            trusted_manifest_key,
            epoch_file,
            workspace_parent,
            sidecar_path,
            rom_path: destination_rom_path,
            mgba_path,
            bridge_path,
            world_intent: Some((intent_store, new_request)),
            shutdown: Box::pin(async move {
                if stop_requested {
                    stop.notify_one();
                }
                stop.notified().await;
            }),
            portal: Some(destination_portal),
            announce_start,
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
        BackendConfig, BackendEvent, RuntimeOutcome, StoryRecoveryStatus, abandon_proposal_id,
        delete_local_account, reusable_prepare_key, runtime_completion_event, spawn_backend,
        story_recovery_status,
    };
    use crate::config::{AccountRecord, RuntimeConfig, UserPaths};
    use coop_cloud::{CharacterId, IdempotencyKey, RefreshToken, UserId};
    use coop_launcher::{
        KeychainError, RefreshTokenStore, TrustedManifestKey, TrustedReleaseKey,
        rom_travel::{RomTravelJournal, TravelPhase},
        session::RomWorldId,
    };
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

    /// A catalog-bound server: legacy acquire is never acceptable, a resume
    /// package is served only for a lease bound through `acquire-world`, and
    /// every snapshot mutation is counted so tests can prove there is none.
    mod catalog_bound_recovery {
        use std::{
            collections::VecDeque,
            sync::{Arc, Mutex},
        };

        use coop_cloud::{
            AccessToken, AcquireLeaseRequest, AcquireWorldLeaseResponse, ArtifactIdentity,
            CharacterId, ClientInstanceId, CompatibilityTarget, GameBuildId, HeartbeatLeaseRequest,
            IdempotencyKey, LeaseContract, LeaseFence, LoginRequest, LoginResponse, LogoutRequest,
            LogoutResponse, MgbaVersion, PrepareSnapshotRequest, ReconnectLeaseRequest,
            RefreshFamilyId, RefreshRequest, RefreshResponse, RefreshToken, ReleaseLeaseRequest,
            Revision, SessionEpoch, SessionId, Sha256Digest, SignedManifestEnvelope,
            SnapshotFinalizeRequest, SnapshotListRequest, SnapshotListResponse,
            SnapshotPrepareResponse, SnapshotRecord, SnapshotRestoreRequest,
            SnapshotRestoreResponse, TrustedManifestKey, UnixTimestampMillis, UploadTarget, UserId,
        };
        use coop_launcher::{
            AuthApi, AuthError, AuthSession, BuildCompatibility, CloudApi, EpochStore,
            KeychainError, RecoveryMarkerV2, RecoveryResult, RefreshTokenStore, SessionConfig,
            SessionError, WorldAcquireIntentStore, auth::AuthFuture, session::CloudFuture,
            session::RomWorldId,
        };

        const CHARACTER: u128 = 0x51;
        const PRIOR_CLIENT: u128 = 0x52;

        fn character() -> CharacterId {
            CharacterId::new(uuid::Uuid::from_u128(CHARACTER)).unwrap()
        }

        fn prior_client() -> ClientInstanceId {
            ClientInstanceId::new(uuid::Uuid::from_u128(PRIOR_CLIENT)).unwrap()
        }

        fn server_world() -> RomWorldId {
            RomWorldId::new(2).unwrap()
        }

        #[derive(Default)]
        struct Calls {
            legacy_acquires: usize,
            world_acquires: Vec<AcquireLeaseRequest>,
            world_bound: bool,
            resume_packages: usize,
            releases: Vec<ReleaseLeaseRequest>,
            mutations: usize,
        }

        #[derive(Default)]
        struct CatalogBoundCloud {
            calls: Mutex<Calls>,
            /// Scripted acquire-world failures, consumed before success.
            world_acquire_failures: Mutex<VecDeque<SessionError>>,
        }

        impl CatalogBoundCloud {
            fn lease(client: ClientInstanceId) -> LeaseContract {
                LeaseContract::new(
                    LeaseFence::new(
                        SessionId::new(uuid::Uuid::from_u128(0x53)).unwrap(),
                        character(),
                        Revision::initial(),
                        SessionEpoch::new(2).unwrap(),
                        client,
                    ),
                    UnixTimestampMillis::new(4_000_000_000_000),
                    1,
                )
                .unwrap()
            }
        }

        impl AuthApi for CatalogBoundCloud {
            fn login(&self, _: LoginRequest) -> AuthFuture<'_, LoginResponse> {
                Box::pin(async {
                    LoginResponse::new(
                        UserId::new(uuid::Uuid::from_u128(0x54)).unwrap(),
                        character(),
                        AccessToken::new("recovery-access").unwrap(),
                        RefreshToken::new("recovery-refresh").unwrap(),
                        RefreshFamilyId::new(uuid::Uuid::from_u128(0x55)).unwrap(),
                        UnixTimestampMillis::new(u64::MAX / 4),
                        UnixTimestampMillis::new(u64::MAX / 4),
                    )
                    .map_err(|_| AuthError::InvalidResponse)
                })
            }
            fn refresh(&self, _: RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
                Box::pin(async { Err(AuthError::Transport) })
            }
            fn logout(&self, _: LogoutRequest) -> AuthFuture<'_, LogoutResponse> {
                Box::pin(async { Ok(LogoutResponse::default()) })
            }
        }

        impl CloudApi for CatalogBoundCloud {
            fn acquire<'a>(
                &'a self,
                _: &'a AuthSession,
                _: AcquireLeaseRequest,
            ) -> CloudFuture<'a, LeaseContract> {
                let mut calls = self.calls.lock().unwrap();
                calls.legacy_acquires += 1;
                calls.world_bound = false;
                Box::pin(async { Err(SessionError::Unauthorized) })
            }
            fn acquire_world<'a>(
                &'a self,
                _: &'a AuthSession,
                request: AcquireLeaseRequest,
            ) -> CloudFuture<'a, AcquireWorldLeaseResponse> {
                let mut calls = self.calls.lock().unwrap();
                calls.world_acquires.push(request);
                if let Some(error) = self.world_acquire_failures.lock().unwrap().pop_front() {
                    return Box::pin(async move { Err(error) });
                }
                calls.world_bound = true;
                let response = AcquireWorldLeaseResponse {
                    lease: Self::lease(request.client_instance_id),
                    active_world_id: server_world(),
                    active_snapshot_id: None,
                };
                Box::pin(async move { Ok(response) })
            }
            fn heartbeat<'a>(
                &'a self,
                _: &'a AuthSession,
                request: HeartbeatLeaseRequest,
            ) -> CloudFuture<'a, LeaseContract> {
                let lease = Self::lease(request.client_instance_id);
                Box::pin(async move { Ok(lease) })
            }
            fn reconnect<'a>(
                &'a self,
                _: &'a AuthSession,
                _: ReconnectLeaseRequest,
            ) -> CloudFuture<'a, LeaseContract> {
                Box::pin(async { Err(SessionError::Cloud) })
            }
            fn release<'a>(
                &'a self,
                _: &'a AuthSession,
                request: ReleaseLeaseRequest,
            ) -> CloudFuture<'a, LogoutResponse> {
                let mut calls = self.calls.lock().unwrap();
                calls.releases.push(request);
                calls.world_bound = false;
                Box::pin(async { Ok(LogoutResponse::default()) })
            }
            fn resume_package<'a>(
                &'a self,
                _: &'a AuthSession,
                _: CharacterId,
                _: Revision,
            ) -> CloudFuture<'a, Option<SignedManifestEnvelope>> {
                let mut calls = self.calls.lock().unwrap();
                calls.resume_packages += 1;
                if calls.world_bound {
                    Box::pin(async { Ok(None) })
                } else {
                    Box::pin(async { Err(SessionError::Unauthorized) })
                }
            }
            fn artifact<'a>(
                &'a self,
                _: &'a AuthSession,
                _: CharacterId,
                _: ArtifactIdentity,
                _: Revision,
            ) -> CloudFuture<'a, Vec<u8>> {
                Box::pin(async { Err(SessionError::ArtifactNotFound) })
            }
            fn list_snapshots<'a>(
                &'a self,
                _: &'a AuthSession,
                _: SnapshotListRequest,
            ) -> CloudFuture<'a, SnapshotListResponse> {
                Box::pin(async { Err(SessionError::Cloud) })
            }
            fn restore<'a>(
                &'a self,
                _: &'a AuthSession,
                _: SnapshotRestoreRequest,
            ) -> CloudFuture<'a, SnapshotRestoreResponse> {
                self.calls.lock().unwrap().mutations += 1;
                Box::pin(async { Err(SessionError::Cloud) })
            }
            fn prepare<'a>(
                &'a self,
                _: &'a AuthSession,
                _: PrepareSnapshotRequest,
            ) -> CloudFuture<'a, SnapshotPrepareResponse> {
                self.calls.lock().unwrap().mutations += 1;
                Box::pin(async { Err(SessionError::Cloud) })
            }
            fn upload<'a>(&'a self, _: &'a UploadTarget, _: Vec<u8>) -> CloudFuture<'a, ()> {
                self.calls.lock().unwrap().mutations += 1;
                Box::pin(async { Err(SessionError::Cloud) })
            }
            fn finalize<'a>(
                &'a self,
                _: &'a AuthSession,
                _: SnapshotFinalizeRequest,
            ) -> CloudFuture<'a, SnapshotRecord> {
                self.calls.lock().unwrap().mutations += 1;
                Box::pin(async { Err(SessionError::Cloud) })
            }
        }

        struct NoKeychain;
        impl RefreshTokenStore for NoKeychain {
            fn load(&self, _: &str, _: &str) -> Result<Option<RefreshToken>, KeychainError> {
                Ok(None)
            }
            fn store(&self, _: &str, _: &str, _: &RefreshToken) -> Result<(), KeychainError> {
                Ok(())
            }
            fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
                Ok(())
            }
        }

        struct Fixture {
            root: tempfile::TempDir,
            cloud: CatalogBoundCloud,
            keychain: Arc<dyn RefreshTokenStore>,
            intent: WorldAcquireIntentStore,
            evidence: std::path::PathBuf,
            save: Vec<u8>,
            marker: Vec<u8>,
        }

        impl Fixture {
            /// Interrupted revision-zero save from the prior client
            /// instance: the marker binds the prior lease and exact SAV.
            fn new() -> Self {
                let root = tempfile::tempdir().unwrap();
                let workspace_parent = root.path().join("sessions");
                let evidence = workspace_parent.join("coop-recovery-crash");
                std::fs::create_dir_all(&evidence).unwrap();
                std::fs::create_dir_all(root.path().join("bridge")).unwrap();
                let save = b"unsynced revision-zero save".to_vec();
                let prior_fence = LeaseFence::new(
                    SessionId::new(uuid::Uuid::from_u128(0x56)).unwrap(),
                    character(),
                    Revision::initial(),
                    SessionEpoch::new(1).unwrap(),
                    prior_client(),
                );
                let marker = RecoveryMarkerV2::new(
                    prior_fence,
                    Revision::initial(),
                    1,
                    Sha256Digest::of_bytes(&save),
                )
                .unwrap()
                .encode()
                .unwrap();
                std::fs::write(evidence.join("character.sav"), &save).unwrap();
                std::fs::write(evidence.join("recovery.marker"), &marker).unwrap();
                let intent =
                    WorldAcquireIntentStore::new(root.path().join("recovery-acquire"), character())
                        .unwrap();
                Self {
                    root,
                    cloud: CatalogBoundCloud::default(),
                    keychain: Arc::new(NoKeychain),
                    intent,
                    evidence,
                    save,
                    marker,
                }
            }

            async fn auth(&self) -> AuthSession {
                AuthSession::login(
                    &self.cloud,
                    self.keychain.as_ref(),
                    "recovery-player",
                    AuthSession::password("test-password").unwrap(),
                )
                .await
                .unwrap()
            }

            /// The configuration a trusted catalog would produce for the
            /// server-selected world.
            fn config(&self, response: &AcquireWorldLeaseResponse) -> SessionConfig {
                SessionConfig {
                    client_instance_id: response.lease.client_instance_id,
                    rom_world_id: response.active_world_id,
                    manifest: compatibility(),
                    trusted_manifest_key: TrustedManifestKey::new(
                        "manifest-test",
                        ed25519_dalek::SigningKey::from_bytes(&[2; 32])
                            .verifying_key()
                            .to_bytes(),
                    )
                    .unwrap(),
                    epoch_store: EpochStore::new(self.root.path().join("epoch.json")),
                    workspace_parent: self.root.path().join("sessions"),
                    bridge_lua_dir: self.root.path().join("bridge"),
                }
            }

            fn assert_evidence_untouched(&self) {
                assert_eq!(
                    std::fs::read(self.evidence.join("character.sav")).unwrap(),
                    self.save
                );
                assert_eq!(
                    std::fs::read(self.evidence.join("recovery.marker")).unwrap(),
                    self.marker
                );
            }

            fn session_dirs(&self) -> usize {
                std::fs::read_dir(self.root.path().join("sessions"))
                    .unwrap()
                    .filter(|entry| {
                        entry
                            .as_ref()
                            .unwrap()
                            .file_name()
                            .to_string_lossy()
                            .starts_with("coop-session-")
                    })
                    .count()
            }
        }

        fn compatibility() -> BuildCompatibility {
            let registry_digest = coop_protocol::IDENTITY_REGISTRY_DIGEST
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let manifest = serde_json::from_value(serde_json::json!({
                "schema_version": 4,
                "emulator": {
                    "name": "mGBA",
                    "version": "0.11.0",
                    "build_id": "0.11-9139-3a5bc2462",
                    "source_commit": "3a5bc24629867576b0fb576a5d5a21d3b3d6b576",
                    "platform": "windows-x64",
                    "variant": "Qt",
                    "archive_sha256": "ea7cc0e8632cd80d28bdb55e37aacc58b2b018f564209f790e8cc3caed8c002b",
                    "executable_sha256": "743157a16a1cb478a2b45e6e20e9a482ea397c3820d7e8e27b1e048e85bd5546"
                },
                "game_build": {
                    "id": "pokeemerald-coop",
                    "numeric_id": 65536,
                    "rom_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                },
                "net_bridge": {
                    "symbol": "gCoopNetBridge",
                    "address": 33_554_432,
                    "size": 9244,
                    "magic": 1_347_111_759,
                    "abi_version": 1,
                    "game_protocol_version": 5,
                    "byte_order": "little",
                    "checksum": {"algorithm": "CRC-32/IEEE", "covered_bytes": [0, 139], "stored_offset": 140},
                    "offsets": {"magic": 0, "abi_version": 4, "game_protocol_version": 6, "game_build_id": 8, "status_flags": 12, "last_sidecar_heartbeat": 16, "game_to_network": 20, "network_to_game": 4632},
                    "queue": {"capacity": 32, "size": 4612, "read_index_offset": 0, "write_index_offset": 2, "entries_offset": 4},
                    "message": {"size": 144, "payload_size": 128, "offsets": {"type": 0, "length": 2, "sequence": 4, "session_epoch": 8, "payload": 12, "checksum": 140}}
                },
                "save": {
                    "block3_address": 33_554_432,
                    "coop_offset": 4,
                    "generation_offset": 28,
                    "generation_address": 33_554_464,
                    "crc_offset": 668,
                    "schema_version": 2,
                    "struct_size": 672,
                    "registry_version": coop_protocol::IDENTITY_REGISTRY_VERSION,
                    "registry_digest": registry_digest
                }
            }))
            .unwrap();
            BuildCompatibility {
                target: CompatibilityTarget::new(
                    GameBuildId::new("pokeemerald-coop").unwrap(),
                    Sha256Digest::of_bytes(b"rom"),
                    MgbaVersion::new("0.11.0").unwrap(),
                    coop_cloud::BridgeAbiVersion::new(1).unwrap(),
                    coop_cloud::ProtocolVersion::new(1).unwrap(),
                    Revision::initial(),
                ),
                manifest,
                rom_path: "rom.gba".into(),
                mgba_path: "mgba".into(),
            }
        }

        fn assert_only_world_bound(cloud: &CatalogBoundCloud) {
            let calls = cloud.calls.lock().unwrap();
            assert_eq!(calls.legacy_acquires, 0);
            assert_eq!(calls.mutations, 0);
        }

        #[tokio::test]
        async fn recovery_binds_the_prior_instance_to_the_server_world_and_settles() {
            let fixture = Fixture::new();
            let auth = fixture.auth().await;
            let selected = Mutex::new(None);
            let (result, auth) = super::super::reconcile_recovery_with_world(
                &fixture.cloud,
                &fixture.keychain,
                auth,
                &fixture.intent,
                prior_client(),
                |response| {
                    *selected.lock().unwrap() = Some(response.active_world_id);
                    Ok(fixture.config(response))
                },
            )
            .await;
            // Revision-zero divergent evidence has no server-signed recovery
            // capability: the terminal outcome keeps it for an operator.
            assert_eq!(result, RecoveryResult::StillUncertain);
            assert!(auth.is_some(), "credentials survive a settled lease");
            assert_eq!(*selected.lock().unwrap(), Some(server_world()));
            assert_only_world_bound(&fixture.cloud);
            let calls = fixture.cloud.calls.lock().unwrap();
            assert_eq!(calls.world_acquires.len(), 1);
            let request = calls.world_acquires[0];
            assert_eq!(request.client_instance_id, prior_client());
            assert!(!request.replace_same_client);
            assert_eq!(calls.resume_packages, 1, "resumed under the world lease");
            assert_eq!(calls.releases.len(), 1, "terminal state releases its lease");
            assert_eq!(calls.releases[0].client_instance_id, prior_client());
            drop(calls);
            assert_eq!(fixture.intent.read().unwrap(), None);
            fixture.assert_evidence_untouched();
        }

        #[tokio::test]
        async fn unselectable_server_world_releases_without_materializing() {
            let fixture = Fixture::new();
            let auth = fixture.auth().await;
            let (result, auth) = super::super::reconcile_recovery_with_world(
                &fixture.cloud,
                &fixture.keychain,
                auth,
                &fixture.intent,
                prior_client(),
                // The trusted catalog has no compatible ROM for this world.
                |_| Err(()),
            )
            .await;
            assert_eq!(result, RecoveryResult::StillUncertain);
            assert!(auth.is_some());
            assert_only_world_bound(&fixture.cloud);
            let calls = fixture.cloud.calls.lock().unwrap();
            assert_eq!(calls.resume_packages, 0);
            assert_eq!(calls.releases.len(), 1);
            drop(calls);
            assert_eq!(fixture.session_dirs(), 0);
            assert_eq!(fixture.intent.read().unwrap(), None);
            fixture.assert_evidence_untouched();
        }

        #[tokio::test]
        async fn configuration_for_another_world_is_rejected_before_resume() {
            let fixture = Fixture::new();
            let auth = fixture.auth().await;
            let (result, _) = super::super::reconcile_recovery_with_world(
                &fixture.cloud,
                &fixture.keychain,
                auth,
                &fixture.intent,
                prior_client(),
                |response| {
                    let mut config = fixture.config(response);
                    config.rom_world_id = RomWorldId::new(1).unwrap();
                    Ok(config)
                },
            )
            .await;
            assert_eq!(result, RecoveryResult::StillUncertain);
            assert_only_world_bound(&fixture.cloud);
            let calls = fixture.cloud.calls.lock().unwrap();
            assert_eq!(calls.resume_packages, 0);
            assert_eq!(calls.releases.len(), 1);
            drop(calls);
            assert_eq!(fixture.session_dirs(), 0);
            fixture.assert_evidence_untouched();
        }

        #[tokio::test]
        async fn closed_key_rotates_only_the_key_and_stale_intent_is_settled_first() {
            let fixture = Fixture::new();
            let stale = AcquireLeaseRequest::new(
                character(),
                ClientInstanceId::new(uuid::Uuid::from_u128(0x57)).unwrap(),
                IdempotencyKey::new(uuid::Uuid::from_u128(0x58)).unwrap(),
            );
            fixture.intent.load_or_create(stale).unwrap();
            // The stale request's lease is still live: it is replayed and
            // released before a request for the prior instance is minted.
            let auth = fixture.auth().await;
            let cloud = &fixture.cloud;
            let (first, _) = super::super::reconcile_recovery_with_world(
                cloud,
                &fixture.keychain,
                auth,
                &fixture.intent,
                prior_client(),
                |_| Err(()),
            )
            .await;
            assert_eq!(first, RecoveryResult::StillUncertain);
            {
                let calls = cloud.calls.lock().unwrap();
                assert_eq!(calls.world_acquires.len(), 2);
                assert_eq!(calls.world_acquires[0], stale);
                assert_eq!(calls.world_acquires[1].client_instance_id, prior_client());
                assert_ne!(
                    calls.world_acquires[1].idempotency_key,
                    stale.idempotency_key
                );
                assert_eq!(calls.releases.len(), 2, "stale lease released first");
            }

            // A persisted request whose key the server reports closed.
            let closed = AcquireLeaseRequest::new(
                character(),
                prior_client(),
                IdempotencyKey::new(uuid::Uuid::from_u128(0x59)).unwrap(),
            );
            fixture.intent.load_or_create(closed).unwrap();
            cloud
                .world_acquire_failures
                .lock()
                .unwrap()
                .push_back(SessionError::AcquireClosed);
            let auth = fixture.auth().await;
            let (second, _) = super::super::reconcile_recovery_with_world(
                cloud,
                &fixture.keychain,
                auth,
                &fixture.intent,
                prior_client(),
                |_| Err(()),
            )
            .await;
            assert_eq!(second, RecoveryResult::StillUncertain);
            let calls = cloud.calls.lock().unwrap();
            assert_eq!(calls.world_acquires.len(), 4);
            assert_eq!(calls.world_acquires[2], closed);
            assert_eq!(calls.world_acquires[3].client_instance_id, prior_client());
            assert_ne!(
                calls.world_acquires[3].idempotency_key,
                closed.idempotency_key
            );
            assert_eq!(calls.legacy_acquires, 0);
            assert_eq!(calls.mutations, 0);
            drop(calls);
            assert_eq!(fixture.intent.read().unwrap(), None);
            fixture.assert_evidence_untouched();
        }
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

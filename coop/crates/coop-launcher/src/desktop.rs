//! UI-independent state machine for the one-click desktop launcher.
//!
//! The renderer owns no lifecycle decisions.  It sends [`Command`] values to
//! [`Controller`] and renders the resulting [`UiModel`].  Backend adapters
//! perform the work represented by [`Effect`] values and send the corresponding
//! completion command back to the controller.

use std::fmt;

use zeroize::Zeroize;

/// The account flow currently being authenticated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthFlow {
    /// Create an account with an invitation and then sign in.
    Registration,
    /// Sign in with an existing account.
    SignIn,
    /// Rehydrate a saved refresh-token session.
    Resume,
}

/// Safe categories for authentication failures.
///
/// This deliberately contains no server response, username, invite, token,
/// path, or implementation detail.  The renderer can only expose the generic
/// blocked copy associated with this category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthFailure {
    /// The service could not be reached or returned an unusable response.
    Unavailable,
    /// Credentials or the invitation were not accepted.
    Rejected,
    /// A saved session could not be resumed.
    SessionExpired,
}

/// Safe categories for release-service/readiness failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceFailure {
    /// The readiness service could not be reached.
    Unavailable,
    /// The authenticated session is no longer accepted.
    Unauthorized,
    /// The service has not produced an online-ready result.
    NotReady,
}

/// Safe categories for update failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateFailure {
    /// The update source could not be reached.
    Unavailable,
    /// The signed release metadata was not accepted.
    SignatureInvalid,
    /// An artifact did not match its authenticated digest or size.
    ArtifactInvalid,
    /// The release is not compatible with this launcher.
    Incompatible,
    /// The complete generation could not be activated.
    ActivationFailed,
}

/// Safe categories for sign-out failures.
///
/// [`crate::auth::AuthSession::logout`] deletes the local refresh credential
/// before awaiting remote revocation, and reports a local deletion error even
/// when the remote request also fails.  The controller keeps both outcomes
/// blocked until the adapter explicitly reports [`Command::SignOutCompleted`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignOutFailure {
    /// Durable local keychain deletion failed; remote revoke was still attempted.
    LocalDeletion,
    /// Local deletion succeeded but remote revocation was not confirmed.
    RemoteRevocation,
}

/// Safe categories for managed-runtime startup failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartFailure {
    /// The runtime could not be started.
    Unavailable,
    /// The runtime did not become ready.
    NotReady,
}

/// Why the controller has entered a blocked state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockReason {
    /// Authentication or saved-session restoration failed.
    Authentication(AuthFailure),
    /// Online release readiness failed.
    Service(ServiceFailure),
    /// A required release update failed.
    Update(UpdateFailure),
    /// The managed runtime could not be started.
    Start(StartFailure),
    /// Sign-out did not complete durably and/or remotely.
    SignOut(SignOutFailure),
}

/// The operation that a Retry command will repeat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryTarget {
    /// Prompt for the same kind of authentication again.
    Authentication(AuthFlow),
    /// Re-check online release readiness.
    ReleaseCheck,
    /// Re-apply the required release update.
    Update,
    /// Re-attempt managed-runtime startup.
    Start,
    /// Repeat sign-out until local deletion and remote revocation are confirmed.
    SignOut,
}

/// Why the launcher cannot yet prove that shutdown completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryReason {
    /// Child or lease shutdown returned insufficient evidence.
    ShutdownUncertain,
    /// Recovery evidence was preserved across a launcher restart.
    PreservedAtStartup,
}

/// Evidence supplied when a launcher instance starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapInput {
    /// No preserved recovery evidence is present.
    Clean,
    /// A prior uncertain shutdown left recovery material that must be reconciled.
    PreservedRecovery,
}

/// The current lifecycle state of the desktop launcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// No authenticated session is active; the renderer offers registration or sign-in.
    FirstRun,
    /// An authentication request is in flight and must not be duplicated.
    Authenticating { flow: AuthFlow },
    /// The authenticated service is checking that a complete release is ready.
    CheckingRelease,
    /// A required compatible release generation is being activated.
    Updating,
    /// All online and release gates are satisfied; Play is enabled here only.
    Ready,
    /// A one-click start request is in flight.
    Starting,
    /// The managed runtime and session are active.
    Running,
    /// A bounded stop/checkpoint request is in flight.
    Stopping,
    /// Local credential deletion and remote revocation are in flight.
    SigningOut,
    /// A recoverable failure has disabled Play and offers Retry.
    Blocked {
        /// Safe failure category suitable for telemetry and tests.
        reason: BlockReason,
        /// Operation that Retry may repeat.
        retry: RetryTarget,
    },
    /// Shutdown evidence is incomplete; readiness cannot resume until reconciled.
    RecoveryRequired { reason: RecoveryReason },
}

/// The result of checking the currently activated release generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseReadiness {
    /// A complete, compatible generation is already active.
    Complete,
    /// A complete, compatible generation must be installed before Play.
    UpdateRequired,
}

/// The result of a recovery reconciliation attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryResult {
    /// Shutdown/recovery evidence is now complete.
    Reconciled,
    /// The launcher still cannot prove a safe post-shutdown state.
    StillUncertain,
}

/// A secret input passed to an authentication effect.
///
/// The controller never stores this in its state and its debug output is always
/// redacted.  Backend adapters may consume it with [`Secret::into_inner`].
#[derive(Clone, Eq, PartialEq)]
pub struct Secret {
    value: String,
}

impl Secret {
    /// Creates a secret input from a caller-owned value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Borrows the secret for the duration of a backend request.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Consumes the wrapper and returns the value to a backend adapter.
    #[must_use]
    pub fn into_inner(mut self) -> String {
        std::mem::take(&mut self.value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

/// Typed authentication data carried only by an [`Effect::Authenticate`].
#[derive(Clone, Eq, PartialEq)]
pub struct AuthRequest {
    flow: AuthFlow,
    username: String,
    password: Secret,
    invitation: Option<Secret>,
}

impl AuthRequest {
    fn registration(username: String, password: Secret, invitation: Secret) -> Self {
        Self {
            flow: AuthFlow::Registration,
            username,
            password,
            invitation: Some(invitation),
        }
    }

    fn sign_in(username: String, password: Secret) -> Self {
        Self {
            flow: AuthFlow::SignIn,
            username,
            password,
            invitation: None,
        }
    }

    /// Returns the authentication flow for this request.
    #[must_use]
    pub const fn flow(&self) -> AuthFlow {
        self.flow
    }

    /// Returns the username without exposing any password or invitation value.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the password to the backend adapter.
    #[must_use]
    pub fn password(&self) -> &Secret {
        &self.password
    }

    /// Returns the registration invitation, when this is a registration request.
    #[must_use]
    pub fn invitation(&self) -> Option<&Secret> {
        self.invitation.as_ref()
    }
}

impl fmt::Debug for AuthRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthRequest")
            .field("flow", &self.flow)
            .field("username", &"[REDACTED]")
            .field("password", &"[REDACTED]")
            .field("invitation", &"[REDACTED]")
            .finish()
    }
}

/// Commands accepted by the renderer/controller boundary.
pub enum Command {
    /// Ask the renderer to show registration fields.
    BeginRegistration,
    /// Ask the renderer to show sign-in fields.
    BeginSignIn,
    /// Submit a new-account request.
    SubmitRegistration {
        /// Account name supplied by the player.
        username: String,
        /// Password supplied by the player.
        password: Secret,
        /// Invite supplied by the player.
        invitation: Secret,
    },
    /// Submit an existing-account request.
    SubmitSignIn {
        /// Account name supplied by the player.
        username: String,
        /// Password supplied by the player.
        password: Secret,
    },
    /// Attempt to rehydrate the saved refresh-token session.
    ResumeSavedSession,
    /// Report that authentication completed successfully.
    AuthenticationSucceeded,
    /// Report a safe authentication failure.
    AuthenticationFailed(AuthFailure),
    /// Report the result of the online release-readiness check.
    ReleaseCheckFinished(ReleaseReadiness),
    /// Report that release readiness could not be checked.
    ReleaseCheckFailed(ServiceFailure),
    /// Report that update activation completed.
    UpdateCompleted,
    /// Report a safe update failure.
    UpdateFailed(UpdateFailure),
    /// Request one managed-runtime start.
    Play,
    /// Report that managed-runtime start completed.
    StartCompleted,
    /// Report a safe managed-runtime start failure.
    StartFailed(StartFailure),
    /// Request one bounded stop/checkpoint operation.
    Stop,
    /// Report that stop/checkpoint completed with sufficient evidence.
    StopCompleted,
    /// Report that shutdown completed without sufficient evidence.
    ShutdownUncertain,
    /// Report that local credential deletion and remote revocation both completed.
    SignOutCompleted,
    /// Report a safe local-deletion or remote-revocation failure.
    SignOutFailed(SignOutFailure),
    /// Report the result of a recovery reconciliation attempt.
    RecoveryReconciled(RecoveryResult),
    /// Repeat the operation associated with the current blocked/recovery state.
    Retry,
    /// Delete local authentication state and revoke the remote session.
    SignOut,
}

impl fmt::Debug for Command {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeginRegistration => formatter.write_str("BeginRegistration"),
            Self::BeginSignIn => formatter.write_str("BeginSignIn"),
            Self::SubmitRegistration { .. } => {
                formatter.write_str("SubmitRegistration { [REDACTED] }")
            }
            Self::SubmitSignIn { .. } => formatter.write_str("SubmitSignIn { [REDACTED] }"),
            Self::ResumeSavedSession => formatter.write_str("ResumeSavedSession"),
            Self::AuthenticationSucceeded => formatter.write_str("AuthenticationSucceeded"),
            Self::AuthenticationFailed(failure) => formatter
                .debug_tuple("AuthenticationFailed")
                .field(failure)
                .finish(),
            Self::ReleaseCheckFinished(result) => formatter
                .debug_tuple("ReleaseCheckFinished")
                .field(result)
                .finish(),
            Self::ReleaseCheckFailed(failure) => formatter
                .debug_tuple("ReleaseCheckFailed")
                .field(failure)
                .finish(),
            Self::UpdateCompleted => formatter.write_str("UpdateCompleted"),
            Self::UpdateFailed(failure) => formatter
                .debug_tuple("UpdateFailed")
                .field(failure)
                .finish(),
            Self::Play => formatter.write_str("Play"),
            Self::StartCompleted => formatter.write_str("StartCompleted"),
            Self::StartFailed(failure) => {
                formatter.debug_tuple("StartFailed").field(failure).finish()
            }
            Self::Stop => formatter.write_str("Stop"),
            Self::StopCompleted => formatter.write_str("StopCompleted"),
            Self::ShutdownUncertain => formatter.write_str("ShutdownUncertain"),
            Self::SignOutCompleted => formatter.write_str("SignOutCompleted"),
            Self::SignOutFailed(failure) => formatter
                .debug_tuple("SignOutFailed")
                .field(failure)
                .finish(),
            Self::RecoveryReconciled(result) => formatter
                .debug_tuple("RecoveryReconciled")
                .field(result)
                .finish(),
            Self::Retry => formatter.write_str("Retry"),
            Self::SignOut => formatter.write_str("SignOut"),
        }
    }
}

/// Work that the thin renderer/backend adapter must perform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Open the corresponding credential form without submitting it.
    PromptAuthentication(AuthFlow),
    /// Submit registration or sign-in to the auth adapter.
    Authenticate(AuthRequest),
    /// Rehydrate the saved refresh-token session.
    ResumeSavedSession,
    /// Check authenticated online release readiness.
    CheckRelease,
    /// Activate the required complete release generation.
    ApplyUpdate,
    /// Start the managed runtime/session.
    StartRuntime,
    /// Stop the managed runtime/session and settle the lease.
    StopRuntime,
    /// Reconcile uncertain shutdown evidence before any readiness check.
    ReconcileRecovery,
    /// Delete local auth state and revoke the remote session.
    SignOut,
}

/// Why a command was rejected without changing controller state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandRejection {
    /// Play is only valid in [`State::Ready`].
    PlayUnavailable,
    /// An authentication request is already in flight.
    AuthenticationInFlight,
    /// The command requires the account-choice state.
    AuthenticationUnavailable,
    /// An authentication completion arrived outside authentication.
    NotAuthenticating,
    /// Release readiness completion arrived outside checking.
    NotCheckingRelease,
    /// Update completion arrived outside updating.
    NotUpdating,
    /// Start completion arrived outside starting.
    NotStarting,
    /// Stop is already in flight.
    StopInFlight,
    /// No stoppable session is active.
    StopUnavailable,
    /// Stop completion arrived outside stopping.
    NotStopping,
    /// Sign-out is already in flight.
    SignOutInFlight,
    /// Sign-out completion arrived outside signing out.
    NotSigningOut,
    /// Shutdown uncertainty was reported without an in-flight session.
    NoShutdownInFlight,
    /// Retry is not available in the current state.
    RetryUnavailable,
    /// A session is active and must be stopped before sign-out.
    SessionInFlight,
    /// Authentication is active and must settle before sign-out.
    AuthenticationInFlightForSignOut,
    /// Recovery must be reconciled before sign-out.
    RecoveryPending,
    /// There is no authenticated session to sign out.
    NotAuthenticated,
}

/// The result of dispatching one command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dispatch {
    /// The command was accepted; an optional effect is ready for the adapter.
    Accepted { effect: Option<Effect> },
    /// The command was rejected and state was left unchanged.
    Rejected(CommandRejection),
}

impl Dispatch {
    fn accepted(effect: Option<Effect>) -> Self {
        Self::Accepted { effect }
    }

    fn accepted_without_effect() -> Self {
        Self::accepted(None)
    }

    fn accepted_with(effect: Effect) -> Self {
        Self::accepted(Some(effect))
    }
}

/// Stable status labels used by a renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// The player must choose registration or sign-in.
    AccountChoice,
    /// Authentication is in flight.
    Authenticating,
    /// Online readiness is being checked.
    CheckingRelease,
    /// A required release is being installed.
    Updating,
    /// Play is available.
    Ready,
    /// Start is in flight.
    Starting,
    /// The session is active.
    Running,
    /// Stop/checkpoint is in flight.
    Stopping,
    /// Local deletion and remote revocation are in flight.
    SigningOut,
    /// An operation failed and Retry is available.
    Blocked,
    /// Shutdown evidence must be reconciled.
    RecoveryRequired,
}

/// Renderer-facing, secret-free snapshot of controller state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiModel {
    /// Current lifecycle state.
    pub state: State,
    /// Stable status category.
    pub status: Status,
    /// Concise player-visible copy; never includes backend input/output.
    pub message: &'static str,
    /// Whether the Play action is enabled.
    pub play_enabled: bool,
    /// Whether the Stop action is enabled.
    pub stop_enabled: bool,
    /// Whether the Retry action is enabled.
    pub retry_enabled: bool,
    /// Whether the Sign out action is enabled.
    pub sign_out_enabled: bool,
}

/// Deterministic controller for the desktop lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Controller {
    state: State,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    /// Creates a controller in the first-run account-choice state.
    #[must_use]
    pub const fn new() -> Self {
        Self::from_bootstrap(BootstrapInput::Clean)
    }

    /// Creates a controller from startup recovery evidence.
    ///
    /// Preserved recovery evidence takes precedence over account choice: the
    /// renderer cannot submit auth or readiness work until reconciliation has
    /// completed.
    #[must_use]
    pub const fn from_bootstrap(input: BootstrapInput) -> Self {
        let state = match input {
            BootstrapInput::Clean => State::FirstRun,
            BootstrapInput::PreservedRecovery => State::RecoveryRequired {
                reason: RecoveryReason::PreservedAtStartup,
            },
        };
        Self { state }
    }

    /// Alias for [`Controller::from_bootstrap`] for renderer startup code.
    #[must_use]
    pub const fn from_startup(input: BootstrapInput) -> Self {
        Self::from_bootstrap(input)
    }

    /// Creates a controller from a boolean recovery-marker probe.
    #[must_use]
    pub const fn from_preserved_recovery(preserved: bool) -> Self {
        Self::from_bootstrap(if preserved {
            BootstrapInput::PreservedRecovery
        } else {
            BootstrapInput::Clean
        })
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// Builds the renderer-facing state snapshot.
    #[must_use]
    pub const fn view(&self) -> UiModel {
        let status = self.state.status();
        let (play_enabled, stop_enabled, retry_enabled, sign_out_enabled) = self.state.actions();
        UiModel {
            state: self.state,
            status,
            message: self.state.message(),
            play_enabled,
            stop_enabled,
            retry_enabled,
            sign_out_enabled,
        }
    }

    /// Applies one typed command and returns at most one backend effect.
    ///
    /// Rejected commands never mutate the state.  Completion commands are
    /// accepted only for the operation that is currently in flight, which
    /// makes duplicate auth/start/stop submissions harmless.
    #[must_use]
    #[allow(clippy::needless_return, clippy::too_many_lines)]
    pub fn dispatch(&mut self, command: Command) -> Dispatch {
        let state = self.state;
        match command {
            Command::BeginRegistration => {
                if state == State::FirstRun {
                    Dispatch::accepted_with(Effect::PromptAuthentication(AuthFlow::Registration))
                } else if matches!(state, State::Authenticating { .. }) {
                    Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
                } else {
                    Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
                }
            }
            Command::BeginSignIn => {
                if state == State::FirstRun {
                    Dispatch::accepted_with(Effect::PromptAuthentication(AuthFlow::SignIn))
                } else if matches!(state, State::Authenticating { .. }) {
                    Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
                } else {
                    Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
                }
            }
            Command::SubmitRegistration {
                username,
                password,
                invitation,
            } => {
                if state != State::FirstRun {
                    return if matches!(state, State::Authenticating { .. }) {
                        Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
                    } else {
                        Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
                    };
                }
                self.state = State::Authenticating {
                    flow: AuthFlow::Registration,
                };
                Dispatch::accepted_with(Effect::Authenticate(AuthRequest::registration(
                    username, password, invitation,
                )))
            }
            Command::SubmitSignIn { username, password } => {
                if state != State::FirstRun {
                    return if matches!(state, State::Authenticating { .. }) {
                        Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
                    } else {
                        Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
                    };
                }
                self.state = State::Authenticating {
                    flow: AuthFlow::SignIn,
                };
                Dispatch::accepted_with(Effect::Authenticate(AuthRequest::sign_in(
                    username, password,
                )))
            }
            Command::ResumeSavedSession => {
                if state == State::FirstRun {
                    self.state = State::Authenticating {
                        flow: AuthFlow::Resume,
                    };
                    Dispatch::accepted_with(Effect::ResumeSavedSession)
                } else if matches!(state, State::Authenticating { .. }) {
                    Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
                } else {
                    Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
                }
            }
            Command::AuthenticationSucceeded => {
                if matches!(state, State::Authenticating { .. }) {
                    self.state = State::CheckingRelease;
                    Dispatch::accepted_with(Effect::CheckRelease)
                } else {
                    Dispatch::Rejected(CommandRejection::NotAuthenticating)
                }
            }
            Command::AuthenticationFailed(failure) => {
                if let State::Authenticating { flow } = state {
                    self.state = State::Blocked {
                        reason: BlockReason::Authentication(failure),
                        retry: RetryTarget::Authentication(flow),
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotAuthenticating)
                }
            }
            Command::ReleaseCheckFinished(readiness) => {
                if state != State::CheckingRelease {
                    return Dispatch::Rejected(CommandRejection::NotCheckingRelease);
                }
                match readiness {
                    ReleaseReadiness::Complete => {
                        self.state = State::Ready;
                        Dispatch::accepted_without_effect()
                    }
                    ReleaseReadiness::UpdateRequired => {
                        self.state = State::Updating;
                        Dispatch::accepted_with(Effect::ApplyUpdate)
                    }
                }
            }
            Command::ReleaseCheckFailed(failure) => {
                if state == State::CheckingRelease {
                    self.state = State::Blocked {
                        reason: BlockReason::Service(failure),
                        retry: RetryTarget::ReleaseCheck,
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotCheckingRelease)
                }
            }
            Command::UpdateCompleted => {
                if state == State::Updating {
                    self.state = State::Ready;
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotUpdating)
                }
            }
            Command::UpdateFailed(failure) => {
                if state == State::Updating {
                    self.state = State::Blocked {
                        reason: BlockReason::Update(failure),
                        retry: RetryTarget::Update,
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotUpdating)
                }
            }
            Command::Play => {
                if state == State::Ready {
                    self.state = State::Starting;
                    Dispatch::accepted_with(Effect::StartRuntime)
                } else {
                    Dispatch::Rejected(CommandRejection::PlayUnavailable)
                }
            }
            Command::StartCompleted => {
                if state == State::Starting {
                    self.state = State::Running;
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotStarting)
                }
            }
            Command::StartFailed(failure) => {
                if state == State::Starting {
                    self.state = State::Blocked {
                        reason: BlockReason::Start(failure),
                        retry: RetryTarget::Start,
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotStarting)
                }
            }
            Command::Stop => match state {
                State::Starting | State::Running => {
                    self.state = State::Stopping;
                    Dispatch::accepted_with(Effect::StopRuntime)
                }
                State::Stopping => Dispatch::Rejected(CommandRejection::StopInFlight),
                _ => Dispatch::Rejected(CommandRejection::StopUnavailable),
            },
            Command::StopCompleted => {
                if state == State::Stopping {
                    self.state = State::Ready;
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotStopping)
                }
            }
            Command::ShutdownUncertain => {
                if matches!(state, State::Starting | State::Running | State::Stopping) {
                    self.state = State::RecoveryRequired {
                        reason: RecoveryReason::ShutdownUncertain,
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NoShutdownInFlight)
                }
            }
            Command::SignOutCompleted => {
                if state == State::SigningOut {
                    self.state = State::FirstRun;
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotSigningOut)
                }
            }
            Command::SignOutFailed(failure) => {
                if state == State::SigningOut {
                    self.state = State::Blocked {
                        reason: BlockReason::SignOut(failure),
                        retry: RetryTarget::SignOut,
                    };
                    Dispatch::accepted_without_effect()
                } else {
                    Dispatch::Rejected(CommandRejection::NotSigningOut)
                }
            }
            Command::RecoveryReconciled(result) => {
                let State::RecoveryRequired { reason } = state else {
                    return Dispatch::Rejected(CommandRejection::RecoveryPending);
                };
                match result {
                    RecoveryResult::Reconciled => match reason {
                        RecoveryReason::ShutdownUncertain => {
                            self.state = State::CheckingRelease;
                            Dispatch::accepted_with(Effect::CheckRelease)
                        }
                        RecoveryReason::PreservedAtStartup => {
                            self.state = State::FirstRun;
                            Dispatch::accepted_without_effect()
                        }
                    },
                    RecoveryResult::StillUncertain => Dispatch::accepted_without_effect(),
                }
            }
            Command::Retry => match state {
                State::Blocked { retry, .. } => match retry {
                    RetryTarget::Authentication(flow) => {
                        self.state = State::FirstRun;
                        Dispatch::accepted_with(Effect::PromptAuthentication(flow))
                    }
                    RetryTarget::ReleaseCheck => {
                        self.state = State::CheckingRelease;
                        Dispatch::accepted_with(Effect::CheckRelease)
                    }
                    RetryTarget::Update => {
                        self.state = State::Updating;
                        Dispatch::accepted_with(Effect::ApplyUpdate)
                    }
                    RetryTarget::Start => {
                        self.state = State::Starting;
                        Dispatch::accepted_with(Effect::StartRuntime)
                    }
                    RetryTarget::SignOut => {
                        self.state = State::SigningOut;
                        Dispatch::accepted_with(Effect::SignOut)
                    }
                },
                State::RecoveryRequired { .. } => {
                    Dispatch::accepted_with(Effect::ReconcileRecovery)
                }
                _ => Dispatch::Rejected(CommandRejection::RetryUnavailable),
            },
            Command::SignOut => match state {
                State::CheckingRelease | State::Updating | State::Ready | State::Blocked { .. } => {
                    self.state = State::SigningOut;
                    Dispatch::accepted_with(Effect::SignOut)
                }
                State::FirstRun => Dispatch::Rejected(CommandRejection::NotAuthenticated),
                State::Authenticating { .. } => {
                    Dispatch::Rejected(CommandRejection::AuthenticationInFlightForSignOut)
                }
                State::Starting | State::Running | State::Stopping => {
                    Dispatch::Rejected(CommandRejection::SessionInFlight)
                }
                State::SigningOut => Dispatch::Rejected(CommandRejection::SignOutInFlight),
                State::RecoveryRequired { .. } => {
                    Dispatch::Rejected(CommandRejection::RecoveryPending)
                }
            },
        }
    }
}

impl State {
    /// Returns the stable status category for this state.
    #[must_use]
    pub const fn status(self) -> Status {
        match self {
            Self::FirstRun => Status::AccountChoice,
            Self::Authenticating { .. } => Status::Authenticating,
            Self::CheckingRelease => Status::CheckingRelease,
            Self::Updating => Status::Updating,
            Self::Ready => Status::Ready,
            Self::Starting => Status::Starting,
            Self::Running => Status::Running,
            Self::Stopping => Status::Stopping,
            Self::SigningOut => Status::SigningOut,
            Self::Blocked { .. } => Status::Blocked,
            Self::RecoveryRequired { .. } => Status::RecoveryRequired,
        }
    }

    /// Returns concise player-visible copy for this state.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::FirstRun => "Create an account or sign in.",
            Self::Authenticating { .. } => "Signing in securely…",
            Self::CheckingRelease => "Checking online release readiness…",
            Self::Updating => "Installing the required update…",
            Self::Ready => "Ready to play.",
            Self::Starting => "Starting your session…",
            Self::Running => "Your session is running.",
            Self::Stopping => "Saving and stopping…",
            Self::SigningOut => "Signing out securely…",
            Self::Blocked { .. } => "This action is blocked. Retry to continue.",
            Self::RecoveryRequired { .. } => "Recovery is required before play can resume.",
        }
    }

    const fn actions(self) -> (bool, bool, bool, bool) {
        (
            matches!(self, Self::Ready),
            matches!(self, Self::Starting | Self::Running),
            matches!(self, Self::Blocked { .. } | Self::RecoveryRequired { .. }),
            matches!(
                self,
                Self::CheckingRelease | Self::Updating | Self::Ready | Self::Blocked { .. }
            ),
        )
    }
}

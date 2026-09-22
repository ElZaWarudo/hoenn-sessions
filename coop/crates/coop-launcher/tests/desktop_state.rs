#[path = "../src/desktop.rs"]
mod desktop;

use desktop::{
    AuthFailure, AuthFlow, BlockReason, BootstrapInput, Command, CommandRejection, Controller,
    Dispatch, Effect, RecoveryReason, RecoveryResult, ReleaseReadiness, RetryTarget, Secret,
    ServiceFailure, SignOutFailure, State, UpdateFailure,
};

fn submit_sign_in(controller: &mut Controller) {
    assert!(matches!(
        controller.dispatch(Command::SubmitSignIn {
            username: "returning-player".to_owned(),
            password: Secret::new("password-value"),
        }),
        Dispatch::Accepted {
            effect: Some(Effect::Authenticate(_))
        }
    ));
}

fn authenticate_and_check(controller: &mut Controller) {
    submit_sign_in(controller);
    assert_eq!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
}

fn make_ready(controller: &mut Controller) {
    authenticate_and_check(controller);
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::Ready);
}

#[test]
fn new_player_registration_update_play_stop_and_sign_out() {
    let mut controller = Controller::new();
    assert_eq!(controller.state(), State::FirstRun);
    assert!(!controller.view().play_enabled);

    assert_eq!(
        controller.dispatch(Command::BeginRegistration),
        Dispatch::Accepted {
            effect: Some(Effect::PromptAuthentication(AuthFlow::Registration)),
        }
    );
    assert!(matches!(
        controller.dispatch(Command::SubmitRegistration {
            username: "new-player".to_owned(),
            password: Secret::new("password-value"),
            invitation: Secret::new("invite-value"),
        }),
        Dispatch::Accepted {
            effect: Some(Effect::Authenticate(_))
        }
    ));
    assert_eq!(
        controller.state(),
        State::Authenticating {
            flow: AuthFlow::Registration,
        }
    );
    assert_eq!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(controller.state(), State::CheckingRelease);
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(
            ReleaseReadiness::UpdateRequired
        )),
        Dispatch::Accepted {
            effect: Some(Effect::ApplyUpdate),
        }
    );
    assert_eq!(controller.state(), State::Updating);
    assert_eq!(
        controller.dispatch(Command::UpdateCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::Ready);
    assert!(controller.view().play_enabled);

    assert_eq!(
        controller.dispatch(Command::Play),
        Dispatch::Accepted {
            effect: Some(Effect::StartRuntime),
        }
    );
    assert_eq!(controller.state(), State::Starting);
    assert_eq!(
        controller.dispatch(Command::StartCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::Running);
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::Stop),
        Dispatch::Accepted {
            effect: Some(Effect::StopRuntime),
        }
    );
    assert_eq!(controller.state(), State::Stopping);
    assert_eq!(
        controller.dispatch(Command::StopCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::Ready);
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Accepted {
            effect: Some(Effect::SignOut),
        }
    );
    assert_eq!(controller.state(), State::SigningOut);
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::SignOutCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::FirstRun);
    assert!(!controller.view().play_enabled);
}

#[test]
fn returning_player_resume_uses_the_same_online_readiness_gate() {
    let mut controller = Controller::new();
    assert_eq!(
        controller.dispatch(Command::ResumeSavedSession),
        Dispatch::Accepted {
            effect: Some(Effect::ResumeSavedSession),
        }
    );
    assert_eq!(
        controller.state(),
        State::Authenticating {
            flow: AuthFlow::Resume,
        }
    );
    assert_eq!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(controller.state(), State::CheckingRelease);
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::Ready);
}

#[test]
fn authentication_service_and_update_failures_are_blocked_and_retryable() {
    let mut auth = Controller::new();
    submit_sign_in(&mut auth);
    assert_eq!(
        auth.dispatch(Command::AuthenticationFailed(AuthFailure::Rejected)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        auth.state(),
        State::Blocked {
            reason: BlockReason::Authentication(AuthFailure::Rejected),
            retry: RetryTarget::Authentication(AuthFlow::SignIn),
        }
    );
    assert_eq!(
        auth.dispatch(Command::Play),
        Dispatch::Rejected(CommandRejection::PlayUnavailable)
    );
    assert_eq!(
        auth.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::PromptAuthentication(AuthFlow::SignIn)),
        }
    );
    assert_eq!(auth.state(), State::FirstRun);

    let mut service = Controller::new();
    authenticate_and_check(&mut service);
    assert_eq!(
        service.dispatch(Command::ReleaseCheckFailed(ServiceFailure::Unavailable)),
        Dispatch::Accepted { effect: None }
    );
    assert!(matches!(service.state(), State::Blocked { .. }));
    assert_eq!(
        service.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(service.state(), State::CheckingRelease);

    let mut update = Controller::new();
    authenticate_and_check(&mut update);
    assert_eq!(
        update.dispatch(Command::ReleaseCheckFinished(
            ReleaseReadiness::UpdateRequired
        )),
        Dispatch::Accepted {
            effect: Some(Effect::ApplyUpdate),
        }
    );
    assert_eq!(
        update.dispatch(Command::UpdateFailed(UpdateFailure::SignatureInvalid)),
        Dispatch::Accepted { effect: None }
    );
    assert!(matches!(update.state(), State::Blocked { .. }));
    assert_eq!(
        update.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::ApplyUpdate),
        }
    );
    assert_eq!(update.state(), State::Updating);
}

fn forbidden_play_controllers() -> Vec<Controller> {
    let mut authentication = Controller::new();
    submit_sign_in(&mut authentication);

    let mut checking = Controller::new();
    authenticate_and_check(&mut checking);

    let mut updating = Controller::new();
    authenticate_and_check(&mut updating);
    assert!(matches!(
        updating.dispatch(Command::ReleaseCheckFinished(
            ReleaseReadiness::UpdateRequired
        )),
        Dispatch::Accepted { .. }
    ));

    let mut starting = Controller::new();
    make_ready(&mut starting);
    assert!(matches!(
        starting.dispatch(Command::Play),
        Dispatch::Accepted { .. }
    ));

    let mut running = starting.clone();
    assert!(matches!(
        running.dispatch(Command::StartCompleted),
        Dispatch::Accepted { .. }
    ));

    let mut stopping = running.clone();
    assert!(matches!(
        stopping.dispatch(Command::Stop),
        Dispatch::Accepted { .. }
    ));

    let mut signing_out = Controller::new();
    make_ready(&mut signing_out);
    assert!(matches!(
        signing_out.dispatch(Command::SignOut),
        Dispatch::Accepted { .. }
    ));

    let mut blocked = Controller::new();
    authenticate_and_check(&mut blocked);
    assert!(matches!(
        blocked.dispatch(Command::ReleaseCheckFailed(ServiceFailure::NotReady)),
        Dispatch::Accepted { .. }
    ));

    let mut recovery = running.clone();
    assert!(matches!(
        recovery.dispatch(Command::Stop),
        Dispatch::Accepted { .. }
    ));
    assert!(matches!(
        recovery.dispatch(Command::ShutdownUncertain),
        Dispatch::Accepted { .. }
    ));

    vec![
        Controller::new(),
        authentication,
        checking,
        updating,
        starting,
        running,
        stopping,
        signing_out,
        blocked,
        recovery,
    ]
}

#[test]
fn play_is_forbidden_in_every_non_ready_state() {
    for mut controller in forbidden_play_controllers() {
        let state_before = controller.state();
        assert_eq!(
            controller.dispatch(Command::Play),
            Dispatch::Rejected(CommandRejection::PlayUnavailable),
            "Play must be rejected in {state_before:?}"
        );
        assert_eq!(controller.state(), state_before);
        assert!(!controller.view().play_enabled);
    }
}

#[test]
fn duplicate_auth_start_and_stop_actions_are_rejected() {
    let mut controller = Controller::new();
    submit_sign_in(&mut controller);
    assert_eq!(
        controller.dispatch(Command::SubmitSignIn {
            username: "second-attempt".to_owned(),
            password: Secret::new("second-password"),
        }),
        Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
    );
    assert_eq!(
        controller.dispatch(Command::BeginRegistration),
        Dispatch::Rejected(CommandRejection::AuthenticationInFlight)
    );

    assert_eq!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        controller.dispatch(Command::Play),
        Dispatch::Accepted {
            effect: Some(Effect::StartRuntime),
        }
    );
    assert_eq!(
        controller.dispatch(Command::Play),
        Dispatch::Rejected(CommandRejection::PlayUnavailable)
    );
    assert_eq!(
        controller.dispatch(Command::StartCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        controller.dispatch(Command::Stop),
        Dispatch::Accepted {
            effect: Some(Effect::StopRuntime),
        }
    );
    assert_eq!(
        controller.dispatch(Command::Stop),
        Dispatch::Rejected(CommandRejection::StopInFlight)
    );
    assert_eq!(controller.state(), State::Stopping);
}

#[test]
fn uncertain_shutdown_requires_reconciliation_before_readiness() {
    let mut controller = Controller::new();
    make_ready(&mut controller);
    assert!(matches!(
        controller.dispatch(Command::Play),
        Dispatch::Accepted { .. }
    ));
    assert!(matches!(
        controller.dispatch(Command::StartCompleted),
        Dispatch::Accepted { .. }
    ));
    assert!(matches!(
        controller.dispatch(Command::Stop),
        Dispatch::Accepted { .. }
    ));
    assert_eq!(
        controller.dispatch(Command::ShutdownUncertain),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        controller.state(),
        State::RecoveryRequired {
            reason: RecoveryReason::ShutdownUncertain,
        }
    );
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::ReconcileRecovery),
        }
    );
    assert_eq!(
        controller.dispatch(Command::RecoveryReconciled(RecoveryResult::StillUncertain)),
        Dispatch::Accepted { effect: None }
    );
    assert!(matches!(controller.state(), State::RecoveryRequired { .. }));
    assert_eq!(
        controller.dispatch(Command::RecoveryReconciled(RecoveryResult::Reconciled)),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(controller.state(), State::CheckingRelease);
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    );
    assert!(controller.view().play_enabled);
}

#[test]
fn status_and_debug_output_never_contains_authentication_values() {
    let username = "private-user";
    let password = "private-password";
    let invitation = "private-invite";
    let command = Command::SubmitRegistration {
        username: username.to_owned(),
        password: Secret::new(password),
        invitation: Secret::new(invitation),
    };
    let debug = format!("{command:?}");
    assert!(!debug.contains(username));
    assert!(!debug.contains(password));
    assert!(!debug.contains(invitation));

    let mut controller = Controller::new();
    assert!(matches!(
        controller.dispatch(command),
        Dispatch::Accepted {
            effect: Some(Effect::Authenticate(_))
        }
    ));
    let effect_debug = format!(
        "{:?}",
        controller.dispatch(Command::AuthenticationFailed(AuthFailure::Unavailable,))
    );
    assert!(!effect_debug.contains(username));
    assert!(!effect_debug.contains(password));
    assert!(!effect_debug.contains(invitation));
    assert!(!controller.view().message.contains(username));
    assert!(!controller.view().message.contains(password));
    assert!(!controller.view().message.contains(invitation));
}

#[test]
fn sign_out_is_explicit_and_cannot_race_active_session_or_recovery() {
    let mut controller = Controller::new();
    make_ready(&mut controller);
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Accepted {
            effect: Some(Effect::SignOut),
        }
    );
    assert_eq!(controller.state(), State::SigningOut);
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Rejected(CommandRejection::SignOutInFlight)
    );
    assert_eq!(
        controller.dispatch(Command::SignOutFailed(SignOutFailure::RemoteRevocation)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        controller.state(),
        State::Blocked {
            reason: BlockReason::SignOut(SignOutFailure::RemoteRevocation),
            retry: RetryTarget::SignOut,
        }
    );
    assert_eq!(
        controller.dispatch(Command::Play),
        Dispatch::Rejected(CommandRejection::PlayUnavailable)
    );
    assert_eq!(
        controller.dispatch(Command::ResumeSavedSession),
        Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
    );
    assert_eq!(
        controller.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::SignOut),
        }
    );
    assert_eq!(controller.state(), State::SigningOut);
    assert_eq!(
        controller.dispatch(Command::SignOutFailed(SignOutFailure::LocalDeletion)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(
        controller.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::SignOut),
        }
    );
    assert_eq!(controller.state(), State::SigningOut);
    assert_eq!(
        controller.dispatch(Command::SignOutCompleted),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::FirstRun);
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Rejected(CommandRejection::NotAuthenticated)
    );

    make_ready(&mut controller);
    assert!(matches!(
        controller.dispatch(Command::Play),
        Dispatch::Accepted { .. }
    ));
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Rejected(CommandRejection::SessionInFlight)
    );
    assert!(matches!(
        controller.dispatch(Command::StartCompleted),
        Dispatch::Accepted { .. }
    ));
    assert!(matches!(
        controller.dispatch(Command::Stop),
        Dispatch::Accepted { .. }
    ));
    assert!(matches!(
        controller.dispatch(Command::ShutdownUncertain),
        Dispatch::Accepted { .. }
    ));
    assert_eq!(
        controller.dispatch(Command::SignOut),
        Dispatch::Rejected(CommandRejection::RecoveryPending)
    );
}

#[test]
fn preserved_recovery_on_restart_blocks_auth_and_play_until_reconciled() {
    let mut controller = Controller::from_bootstrap(BootstrapInput::PreservedRecovery);
    assert_eq!(
        controller.state(),
        State::RecoveryRequired {
            reason: RecoveryReason::PreservedAtStartup,
        }
    );
    assert!(!controller.view().play_enabled);
    assert_eq!(
        controller.dispatch(Command::ResumeSavedSession),
        Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
    );
    assert_eq!(
        controller.dispatch(Command::SubmitSignIn {
            username: "returning-player".to_owned(),
            password: Secret::new("password-value"),
        }),
        Dispatch::Rejected(CommandRejection::AuthenticationUnavailable)
    );
    assert_eq!(
        controller.dispatch(Command::Retry),
        Dispatch::Accepted {
            effect: Some(Effect::ReconcileRecovery),
        }
    );
    assert_eq!(
        controller.dispatch(Command::RecoveryReconciled(RecoveryResult::StillUncertain)),
        Dispatch::Accepted { effect: None }
    );
    assert!(matches!(
        controller.state(),
        State::RecoveryRequired {
            reason: RecoveryReason::PreservedAtStartup
        }
    ));
    assert_eq!(
        controller.dispatch(Command::RecoveryReconciled(RecoveryResult::Reconciled)),
        Dispatch::Accepted { effect: None }
    );
    assert_eq!(controller.state(), State::FirstRun);
    assert_eq!(
        controller.dispatch(Command::Play),
        Dispatch::Rejected(CommandRejection::PlayUnavailable)
    );

    submit_sign_in(&mut controller);
    assert_eq!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted {
            effect: Some(Effect::CheckRelease),
        }
    );
    assert_eq!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    );
    assert!(controller.view().play_enabled);
}

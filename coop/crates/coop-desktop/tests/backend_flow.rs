#[cfg(windows)]
#[path = "../src/config.rs"]
mod config;
#[cfg(windows)]
#[path = "../src/release_client.rs"]
mod release_client;
#[cfg(windows)]
#[path = "../src/backend.rs"]
mod backend;

use coop_launcher::{Command, Controller, Dispatch, Effect, ReleaseReadiness, State};

#[test]
fn controller_allows_play_only_after_authenticated_release_gate() {
    let mut controller = Controller::new();
    assert_eq!(controller.state(), State::FirstRun);
    assert!(matches!(
        controller.dispatch(Command::BeginSignIn),
        Dispatch::Accepted { effect: Some(Effect::PromptAuthentication(_)) }
    ));
    assert!(matches!(
        controller.dispatch(Command::SubmitSignIn {
            username: "player".into(),
            password: coop_launcher::Secret::new("password"),
        }),
        Dispatch::Accepted { effect: Some(Effect::Authenticate(_)) }
    ));
    assert!(matches!(
        controller.dispatch(Command::AuthenticationSucceeded),
        Dispatch::Accepted { effect: Some(Effect::CheckRelease) }
    ));
    assert!(matches!(
        controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete)),
        Dispatch::Accepted { effect: None }
    ));
    assert_eq!(controller.state(), State::Ready);
    assert!(matches!(
        controller.dispatch(Command::Play),
        Dispatch::Accepted { effect: Some(Effect::StartRuntime) }
    ));
}

#[test]
fn stop_returns_to_ready_without_signing_out() {
    let mut controller = Controller::new();
    let _ = controller.dispatch(Command::SubmitSignIn {
        username: "player".into(),
        password: coop_launcher::Secret::new("password"),
    });
    let _ = controller.dispatch(Command::AuthenticationSucceeded);
    let _ = controller.dispatch(Command::ReleaseCheckFinished(ReleaseReadiness::Complete));
    let _ = controller.dispatch(Command::Play);
    let _ = controller.dispatch(Command::StartCompleted);
    assert!(matches!(controller.dispatch(Command::Stop), Dispatch::Accepted { .. }));
    let _ = controller.dispatch(Command::StopCompleted);
    assert_eq!(controller.state(), State::Ready);
}

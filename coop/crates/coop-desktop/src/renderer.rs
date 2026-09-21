//! Thin eframe renderer. All lifecycle authority remains in Controller.

#![cfg(windows)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use coop_launcher::{
    AuthFailure, AuthFlow, BlockReason, BootstrapInput, Command, Controller, Dispatch, Effect,
    RecoveryResult, ReleaseReadiness, ServiceFailure, SignOutFailure, StartFailure, State, UiModel,
    UpdateFailure,
};
use eframe::egui::{self, Align, Color32, Layout, RichText, TextEdit};

use crate::backend::{BackendEvent, BackendHandle};

pub struct DesktopApp {
    controller: Controller,
    backend: BackendHandle,
    flow: Option<AuthFlow>,
    username: String,
    password: String,
    invitation: String,
    restart_requested: Arc<AtomicBool>,
    focus_form: bool,
    shutdown_ack: Option<mpsc::Receiver<()>>,
    allow_close: bool,
}

impl DesktopApp {
    pub fn new(
        backend: BackendHandle,
        bootstrap: BootstrapInput,
        restart_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            controller: Controller::from_bootstrap(bootstrap),
            backend,
            flow: None,
            username: String::new(),
            password: String::new(),
            invitation: String::new(),
            restart_requested,
            focus_form: false,
            shutdown_ack: None,
            allow_close: false,
        }
    }

    pub fn model(&self) -> UiModel {
        self.controller.view()
    }

    pub fn resume_saved_session(&mut self) {
        self.dispatch(Command::ResumeSavedSession);
    }

    fn dispatch(&mut self, command: Command) {
        if let Dispatch::Accepted {
            effect: Some(effect),
        } = self.controller.dispatch(command)
        {
            let _ = self.backend.submit(effect);
        }
    }

    fn apply_events(&mut self, context: &egui::Context) {
        for event in self.backend.poll() {
            let command = match event {
                BackendEvent::AuthenticationSucceeded => Command::AuthenticationSucceeded,
                BackendEvent::AuthenticationFailed(failure) => {
                    Command::AuthenticationFailed(failure)
                }
                BackendEvent::ReleaseCheckFinished(readiness) => {
                    Command::ReleaseCheckFinished(readiness)
                }
                BackendEvent::ReleaseCheckFailed(failure) => Command::ReleaseCheckFailed(failure),
                BackendEvent::UpdateCompleted => Command::UpdateCompleted,
                BackendEvent::UpdateFailed(failure) => Command::UpdateFailed(failure),
                BackendEvent::RestartRequired => {
                    self.restart_requested.store(true, Ordering::Release);
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                    continue;
                }
                BackendEvent::StartCompleted => Command::StartCompleted,
                BackendEvent::StartFailed(failure) => Command::StartFailed(failure),
                BackendEvent::StopCompleted => Command::StopCompleted,
                BackendEvent::ShutdownUncertain => Command::ShutdownUncertain,
                BackendEvent::SignOutCompleted => Command::SignOutCompleted,
                BackendEvent::SignOutFailed(failure) => Command::SignOutFailed(failure),
                BackendEvent::RecoveryReconciled(result) => Command::RecoveryReconciled(result),
            };
            self.dispatch(command);
        }
    }

    fn draw_auth(&mut self, ui: &mut egui::Ui) {
        let Some(flow) = self.flow else {
            ui.horizontal(|ui| {
                if ui.add(primary_button("Create account")).clicked() {
                    self.flow = Some(AuthFlow::Registration);
                    self.focus_form = true;
                    self.dispatch(Command::BeginRegistration);
                }
                if ui.add(primary_button("Sign in")).clicked() {
                    self.flow = Some(AuthFlow::SignIn);
                    self.focus_form = true;
                    self.dispatch(Command::BeginSignIn);
                }
                if ui.button("Resume saved session").clicked() {
                    self.dispatch(Command::ResumeSavedSession);
                }
            });
            return;
        };
        let heading = if flow == AuthFlow::Registration {
            "Create your account"
        } else {
            "Sign in"
        };
        ui.heading(heading);
        ui.add_space(10.0);
        let username = ui.add_sized(
            [360.0, 44.0],
            TextEdit::singleline(&mut self.username).hint_text("Username"),
        );
        if self.focus_form {
            username.request_focus();
            self.focus_form = false;
        }
        ui.add_sized(
            [360.0, 44.0],
            TextEdit::singleline(&mut self.password)
                .password(true)
                .hint_text("Password"),
        );
        if flow == AuthFlow::Registration {
            ui.add_sized(
                [360.0, 44.0],
                TextEdit::singleline(&mut self.invitation).hint_text("Invitation code"),
            );
        }
        ui.add_space(10.0);
        if ui
            .add(primary_button(if flow == AuthFlow::Registration {
                "Create account"
            } else {
                "Sign in"
            }))
            .clicked()
        {
            let password = coop_launcher::Secret::new(std::mem::take(&mut self.password));
            if flow == AuthFlow::Registration {
                let invitation = coop_launcher::Secret::new(std::mem::take(&mut self.invitation));
                self.dispatch(Command::SubmitRegistration {
                    username: self.username.clone(),
                    password,
                    invitation,
                });
            } else {
                self.dispatch(Command::SubmitSignIn {
                    username: self.username.clone(),
                    password,
                });
            }
        }
        if ui.button("Back").clicked() {
            self.flow = None;
            self.password.clear();
            self.invitation.clear();
        }
    }

    fn draw_actions(&mut self, ui: &mut egui::Ui, model: UiModel) {
        ui.horizontal(|ui| {
            let play_button = primary_button("Play");
            let play = ui.add_enabled(model.play_enabled, play_button);
            if play.clicked() {
                self.dispatch(Command::Play);
            }
            let stop_button = primary_button("Stop");
            let stop = ui.add_enabled(model.stop_enabled, stop_button);
            if stop.clicked() {
                self.dispatch(Command::Stop);
            }
            if model.retry_enabled && ui.button("Retry").clicked() {
                self.dispatch(Command::Retry);
            }
            if model.sign_out_enabled && ui.button("Sign out").clicked() {
                self.dispatch(Command::SignOut);
            }
        });
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if context.input(|input| input.viewport().close_requested()) && !self.allow_close {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.shutdown_ack.is_none() {
                match self.backend.request_shutdown() {
                    Ok(ack) => self.shutdown_ack = Some(ack),
                    Err(_) => {
                        self.allow_close = true;
                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if let Some(ack) = self.shutdown_ack.as_ref() {
            match ack.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = self.backend.join();
                    self.shutdown_ack = None;
                    self.allow_close = true;
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.apply_events(context);
        let model = self.controller.view();
        egui::CentralPanel::default().show(context, |ui| {
            ui.set_width(520.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.heading(RichText::new("Hoenn Sessions").size(28.0));
                ui.add_space(8.0);
                let message = if self.shutdown_ack.is_some() {
                    "Closing safely…"
                } else {
                    model.message
                };
                ui.label(RichText::new(message).color(Color32::from_rgb(40, 40, 40)));
                ui.add_space(22.0);
                if matches!(model.state, State::FirstRun) {
                    self.draw_auth(ui);
                }
                self.draw_actions(ui, model);
            });
        });
        context.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Normal window close is intercepted in update and remains responsive
        // until the actor acknowledges cleanup. This is a final fallback for
        // exits that bypass the viewport close-request handshake.
        if !self.allow_close {
            let _ = self.backend.shutdown();
        }
    }
}

fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(label).min_size(egui::vec2(112.0, 44.0))
}

#[allow(dead_code)]
fn _failure_copy(
    reason: BlockReason,
    _auth: AuthFailure,
    _service: ServiceFailure,
    _update: UpdateFailure,
    _start: StartFailure,
    _sign_out: SignOutFailure,
    _recovery: RecoveryResult,
    _release: ReleaseReadiness,
    _effect: Effect,
) -> &'static str {
    match reason {
        BlockReason::Authentication(_) => "Authentication needs attention. Retry to continue.",
        BlockReason::Service(_) => "The online service is unavailable. Retry to continue.",
        BlockReason::Update(_) => "The required update could not be installed. Retry to continue.",
        BlockReason::Start(_) => "Your session could not start. Retry to continue.",
        BlockReason::SignOut(_) => "Sign out did not finish. Retry to continue.",
    }
}

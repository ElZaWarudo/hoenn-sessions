//! Thin eframe renderer. All lifecycle authority remains in Controller.

#![cfg(windows)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coop_launcher::{
    AuthFailure, AuthFlow, BlockReason, BootstrapInput, Command, Controller, Dispatch, Effect,
    RecoveryResult, ReleaseReadiness, ServiceFailure, SignOutFailure, StartFailure, State, UiModel,
    UpdateFailure,
};
use eframe::egui::{self, Align, Color32, Layout, RichText, TextEdit};

use crate::backend::{BackendEvent, BackendHandle, JoinFailure, StoryRecoveryStatus};

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
    authenticated: bool,
    runtime_active: bool,
    partner_status: Option<coop_cloud::PartnerStatusResponse>,
    partner_error: bool,
    last_partner_refresh: Option<Instant>,
    story_recovery_status: Option<StoryRecoveryStatus>,
    story_recovery_busy: bool,
    confirm_story_abandon: bool,
    join_code: String,
    join_busy: bool,
    join_message: Option<&'static str>,
    /// File the bootstrapper writes when a `hoenn-sessions://join/` link opens.
    join_inbox: Option<std::path::PathBuf>,
    last_inbox_check: Option<Instant>,
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
            authenticated: false,
            runtime_active: false,
            partner_status: None,
            partner_error: false,
            last_partner_refresh: None,
            story_recovery_status: None,
            story_recovery_busy: false,
            confirm_story_abandon: false,
            join_code: String::new(),
            join_busy: false,
            join_message: None,
            join_inbox: None,
            last_inbox_check: None,
        }
    }

    /// Watches the file a join link hands over through the bootstrapper.
    pub fn set_join_inbox(&mut self, inbox: std::path::PathBuf) {
        self.join_inbox = Some(inbox);
    }

    /// Pre-fills the join box; the player still presses Join to redeem.
    pub fn prefill_join(&mut self, text: &str) -> bool {
        match coop_cloud::pairing_code_from_join_text(text) {
            Some(code) => {
                self.join_code = code.as_str().to_owned();
                self.join_message = Some("Join link received. Press Join to team up.");
                true
            }
            None => false,
        }
    }

    fn check_join_inbox(&mut self) {
        if self
            .last_inbox_check
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.last_inbox_check = Some(Instant::now());
        let Some(inbox) = self.join_inbox.clone() else {
            return;
        };
        if let Some(text) = take_join_inbox(&inbox) {
            let _ = self.prefill_join(&text);
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
                BackendEvent::AuthenticationSucceeded => {
                    self.authenticated = true;
                    self.last_partner_refresh = None;
                    Command::AuthenticationSucceeded
                }
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
                BackendEvent::StartCompleted => {
                    self.runtime_active = true;
                    Command::StartCompleted
                }
                BackendEvent::StartFailed(failure) => {
                    self.runtime_active = false;
                    self.story_recovery_status = None;
                    self.confirm_story_abandon = false;
                    Command::StartFailed(failure)
                }
                BackendEvent::StopCompleted => {
                    self.runtime_active = false;
                    self.last_partner_refresh = None;
                    Command::StopCompleted
                }
                BackendEvent::ShutdownUncertain => Command::ShutdownUncertain,
                BackendEvent::SignOutCompleted => {
                    self.authenticated = false;
                    self.runtime_active = false;
                    self.partner_status = None;
                    Command::SignOutCompleted
                }
                BackendEvent::SignOutFailed(failure) => Command::SignOutFailed(failure),
                BackendEvent::RecoveryReconciled(result) => Command::RecoveryReconciled(result),
                BackendEvent::PartnerStatus(result) => {
                    match result {
                        Ok(status) => {
                            self.partner_status = Some(status);
                            self.partner_error = false;
                        }
                        Err(()) => self.partner_error = true,
                    }
                    continue;
                }
                BackendEvent::PairingRedeemed(result) => {
                    self.join_busy = false;
                    self.join_message = Some(join_result_copy(result));
                    if result.is_ok() {
                        self.join_code.clear();
                        self.last_partner_refresh = None;
                    }
                    continue;
                }
                BackendEvent::StoryRecoveryInspected(status) => {
                    self.story_recovery_status = Some(status);
                    self.story_recovery_busy = false;
                    self.confirm_story_abandon = false;
                    continue;
                }
                BackendEvent::StoryRecoveryAbandoned(status) => {
                    self.story_recovery_status = Some(status);
                    self.story_recovery_busy = false;
                    self.confirm_story_abandon = false;
                    continue;
                }
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
                self.runtime_active = true;
                self.dispatch(Command::Play);
            }
            let stop_button = primary_button("Stop");
            let stop = ui.add_enabled(model.stop_enabled, stop_button);
            if stop.clicked() {
                self.dispatch(Command::Stop);
            }
            if model.retry_enabled && ui.button("Retry").clicked() {
                self.story_recovery_status = None;
                self.confirm_story_abandon = false;
                self.dispatch(Command::Retry);
            }
            if model.sign_out_enabled && ui.button("Sign out").clicked() {
                self.story_recovery_status = None;
                self.confirm_story_abandon = false;
                self.dispatch(Command::SignOut);
            }
        });
        if matches!(
            model.state,
            State::Blocked {
                reason: BlockReason::Start(StartFailure::StoryTravelRecoveryPending),
                ..
            }
        ) {
            self.draw_story_recovery(ui);
        }
    }

    fn draw_story_recovery(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        if ui
            .add_enabled(
                !self.story_recovery_busy,
                egui::Button::new("Check recovery options"),
            )
            .clicked()
            && self.backend.inspect_story_recovery().is_ok()
        {
            self.story_recovery_busy = true;
            self.story_recovery_status = None;
            self.confirm_story_abandon = false;
        }
        if self.story_recovery_busy {
            ui.label("Checking the scene record…");
            return;
        }
        match self.story_recovery_status {
            Some(StoryRecoveryStatus::AbandonAvailable) => {
                ui.label("The group has closed. You can abandon the unfinished boat scene if it never completed on this device.");
                if !self.confirm_story_abandon {
                    if ui.button("Abandon unfinished boat scene…").clicked() {
                        self.confirm_story_abandon = true;
                    }
                } else {
                    ui.label("Confirm only if the boat scene did not finish. The server will refuse if it finds a completed save.");
                    ui.horizontal(|ui| {
                        if ui.button("Confirm abandon").clicked()
                            && self.backend.abandon_story_recovery().is_ok()
                        {
                            self.story_recovery_busy = true;
                            self.story_recovery_status = None;
                            self.confirm_story_abandon = false;
                        }
                        if ui.button("Keep scene record").clicked() {
                            self.confirm_story_abandon = false;
                        }
                    });
                }
            }
            Some(StoryRecoveryStatus::GroupStillActive) => {
                ui.label("Your partner's group is still active. Wait for the scene to finish or the group to close, then check again.");
            }
            Some(StoryRecoveryStatus::Clear) => {
                ui.label("The scene record is clear. Select Retry to start your session.");
            }
            Some(StoryRecoveryStatus::MustReconcile) => {
                ui.label("A completed or changed save was found. The scene must be reconciled; it cannot be abandoned.");
            }
            Some(StoryRecoveryStatus::Unavailable) => {
                ui.label("Recovery could not be checked. Your sign-in and scene record were kept. Check again when the service is available.");
            }
            None => {}
        }
    }

    fn draw_partner(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.separator();
        ui.heading("Partner");
        match self
            .partner_status
            .as_ref()
            .and_then(|status| status.partner.as_ref())
        {
            Some(partner) => {
                ui.label(format!(
                    "{} · {}",
                    partner.username.as_str(),
                    if partner.online { "Online" } else { "Offline" }
                ));
                if let Some(live_zone) = &partner.live_world_zone {
                    ui.label(format!(
                        "Current: {} (live) · {} badges",
                        live_zone.map.replace('_', " "),
                        partner.badge_count
                    ));
                    ui.label(format!(
                        "Last saved: {}",
                        partner.world_zone.map.replace('_', " ")
                    ));
                } else {
                    ui.label(format!(
                        "Last saved: {} · {} badges",
                        partner.world_zone.map.replace('_', " "),
                        partner.badge_count
                    ));
                }
                ui.label(if partner.group_active {
                    "Group active"
                } else {
                    "Group ended"
                });
                if !partner.online {
                    ui.label(last_seen_label(partner.last_seen_at));
                }
            }
            None if self.partner_status.is_some() => {
                ui.label("No partner yet");
            }
            None => {
                ui.label("Loading partner…");
            }
        }
        if self.partner_error {
            ui.label("Partner status is unavailable. Retrying…");
        }
        self.draw_join(ui);
    }

    fn draw_join(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label("Join a partner by code");
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.join_code)
                    .hint_text("ABC-234")
                    .char_limit(7)
                    .desired_width(96.0),
            );
            let ready = self.runtime_active && !self.join_busy && !self.join_code.is_empty();
            if ui.add_enabled(ready, egui::Button::new("Join")).clicked() {
                if self
                    .backend
                    .redeem_pairing_code(self.join_code.clone())
                    .is_ok()
                {
                    self.join_busy = true;
                    self.join_message = Some("Joining…");
                } else {
                    self.join_message = Some(join_result_copy(Err(JoinFailure::Unavailable)));
                }
            }
        });
        if !self.runtime_active {
            ui.label("Press Play first; you both stay where you are when the group forms.");
        }
        if let Some(message) = self.join_message {
            ui.label(message);
        }
    }
}

fn join_result_copy(result: Result<(), JoinFailure>) -> &'static str {
    match result {
        Ok(()) => "Grouped with your partner. You both stay on your own maps.",
        Err(JoinFailure::InvalidCode) => "That is not a pairing code. Codes look like ABC-234.",
        Err(JoinFailure::NotRunning) => "Press Play first, then join with the code.",
        Err(JoinFailure::Refused) => {
            "That code is expired, already used, or you are already in a group."
        }
        Err(JoinFailure::Unavailable) => "The service is unavailable. Try the code again shortly.",
    }
}

/// Reads and removes a small join-link handoff file written by the bootstrapper.
fn take_join_inbox(inbox: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(inbox).ok()?;
    let _ = std::fs::remove_file(inbox);
    if bytes.len() > 128 {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn last_seen_label(timestamp: Option<coop_cloud::UnixTimestampMillis>) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64);
    last_seen_label_at(timestamp, now_ms)
}

fn last_seen_label_at(
    timestamp: Option<coop_cloud::UnixTimestampMillis>,
    now_ms: Option<u64>,
) -> String {
    let Some(timestamp) = timestamp else {
        return "Last seen unknown".to_owned();
    };
    let Some(now_ms) = now_ms else {
        return "Last seen unknown".to_owned();
    };
    let elapsed = now_ms.saturating_sub(timestamp.value()) / 1_000;
    if elapsed < 60 {
        "Last seen less than a minute ago".to_owned()
    } else if elapsed < 3_600 {
        let minutes = elapsed / 60;
        format!(
            "Last seen {minutes} minute{} ago",
            if minutes == 1 { "" } else { "s" }
        )
    } else if elapsed < 86_400 {
        let hours = elapsed / 3_600;
        format!(
            "Last seen {hours} hour{} ago",
            if hours == 1 { "" } else { "s" }
        )
    } else {
        let days = elapsed / 86_400;
        format!(
            "Last seen {days} day{} ago",
            if days == 1 { "" } else { "s" }
        )
    }
}

#[cfg(test)]
mod partner_tests {
    use super::{last_seen_label_at, take_join_inbox};

    #[test]
    fn join_inbox_is_consumed_once_and_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let inbox = directory.path().join("join-link.txt");
        assert_eq!(take_join_inbox(&inbox), None);
        std::fs::write(&inbox, "hoenn-sessions://join/ABC-234").unwrap();
        assert_eq!(
            take_join_inbox(&inbox).as_deref(),
            Some("hoenn-sessions://join/ABC-234")
        );
        assert!(!inbox.exists());
        std::fs::write(&inbox, vec![b'A'; 129]).unwrap();
        assert_eq!(take_join_inbox(&inbox), None);
        assert!(!inbox.exists());
    }
    use coop_cloud::UnixTimestampMillis;

    #[test]
    fn last_seen_uses_only_observed_time_and_handles_clock_skew() {
        assert_eq!(last_seen_label_at(None, Some(300_000)), "Last seen unknown");
        assert_eq!(
            last_seen_label_at(Some(UnixTimestampMillis::new(100_000)), None),
            "Last seen unknown"
        );
        assert_eq!(
            last_seen_label_at(Some(UnixTimestampMillis::new(100_000)), Some(160_000)),
            "Last seen 1 minute ago"
        );
        assert_eq!(
            last_seen_label_at(Some(UnixTimestampMillis::new(200_000)), Some(100_000)),
            "Last seen less than a minute ago"
        );
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
        self.check_join_inbox();
        if self.authenticated
            && self
                .last_partner_refresh
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(30))
        {
            if self.backend.fetch_partner_status().is_ok() {
                self.last_partner_refresh = Some(Instant::now());
            }
        }
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
                if self.authenticated {
                    self.draw_partner(ui);
                }
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

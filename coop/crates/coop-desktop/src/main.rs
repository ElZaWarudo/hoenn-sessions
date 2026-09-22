#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod backend;
#[cfg(windows)]
mod config;
#[cfg(windows)]
mod release_client;
#[cfg(windows)]
mod renderer;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    run_windows()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(1)
}

#[cfg(windows)]
fn run_windows() -> std::process::ExitCode {
    use std::sync::{Arc, atomic::AtomicBool};

    use coop_launcher::{BootstrapInput, RecoveryDiscovery};
    use backend::{BackendConfig, spawn_backend, BOOTSTRAP_RESTART_CODE};
    use renderer::DesktopApp;

    let config = match BackendConfig::compiled() {
        Ok(config) => config,
        Err(_) => return std::process::ExitCode::from(1),
    };
    let bootstrap = match RecoveryDiscovery::discover(config.paths.workspace_parent()) {
        Ok(discovery) => {
            if discovery.candidate().is_none() {
                BootstrapInput::Clean
            } else {
                BootstrapInput::PreservedRecovery
            }
        }
        Err(_) => BootstrapInput::PreservedRecovery,
    };
    let has_saved_account = config.paths.load_account().ok().flatten().is_some();
    let backend = match spawn_backend(config) {
        Ok(backend) => backend,
        Err(_) => return std::process::ExitCode::from(1),
    };
    let restart_requested = Arc::new(AtomicBool::new(false));
    let mut app = DesktopApp::new(backend.clone(), bootstrap, Arc::clone(&restart_requested));
    if has_saved_account && matches!(bootstrap, BootstrapInput::Clean) {
        app.resume_saved_session();
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([600.0, 420.0])
            .with_min_inner_size([600.0, 420.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Hoenn Sessions",
        options,
        Box::new(move |_creation_context| Ok(Box::new(app))),
    );
    if restart_requested.load(std::sync::atomic::Ordering::Acquire) {
        std::process::ExitCode::from(BOOTSTRAP_RESTART_CODE as u8)
    } else if result.is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(1)
    }
}

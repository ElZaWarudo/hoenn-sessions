#![cfg(windows)]

use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};

use coop_launcher::{
    compat::{MAX_MGBA_OUTPUT_BYTES, MGBA_PROBE_TIMEOUT},
    windows_mgba_supervisor::{MgbaSupervisor, ProbeOutput, probe},
};
use tempfile::TempDir;

fn command_path() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join("cmd.exe")
}

struct FixtureDirectory {
    // Declare the nested directory first so TempDir drops it before the
    // owning outer directory, allowing both levels to be removed.
    nested: TempDir,
    outer: TempDir,
}

impl FixtureDirectory {
    fn path(&self) -> &Path {
        self.nested.path()
    }

    fn outer_path(&self) -> &Path {
        self.outer.path()
    }
}

fn fixture_directory() -> FixtureDirectory {
    let outer = tempfile::Builder::new()
        .prefix("coop containment path spaces ")
        .tempdir()
        .expect("fixture parent is writable");
    let nested = tempfile::Builder::new()
        .prefix("nested fixture ")
        .tempdir_in(outer.path())
        .expect("fixture directory is writable");
    FixtureDirectory { nested, outer }
}

struct DescendantFixture {
    marker: PathBuf,
    started: PathBuf,
    args: Vec<String>,
    _child_directory: TempDir,
}

fn delayed_descendant(directory: &FixtureDirectory, hold_root: bool) -> DescendantFixture {
    descendant_fixture(directory, hold_root, None)
}

fn gated_descendant(
    directory: &FixtureDirectory,
    hold_root: bool,
    release: &Path,
) -> DescendantFixture {
    descendant_fixture(directory, hold_root, Some(release))
}

fn descendant_fixture(
    directory: &FixtureDirectory,
    hold_root: bool,
    release: Option<&Path>,
) -> DescendantFixture {
    let marker = directory.path().join("descendant marker");
    let started = directory.path().join("descendant started");
    let marker_text = marker.to_str().expect("temporary path is valid");
    let started_text = started.to_str().expect("temporary path is valid");
    // Keep the batch file's own pathname free of spaces so only its quoted
    // marker operands exercise path-with-spaces parsing. The marker remains
    // in the deliberately spaced private fixture directory.
    let child_script = tempfile::Builder::new()
        .prefix("coop-child-")
        .tempdir()
        .expect("child script directory is writable");
    let cmd = command_path();
    let cmd = cmd.to_str().expect("cmd path is valid");
    let ping = PathBuf::from(cmd).with_file_name("ping.exe");
    let ping = ping.to_str().expect("ping path is valid");
    let completion_wait = release.map_or_else(
        || format!("\"{ping}\" -n 2 127.0.0.1 >nul"),
        |release| {
            let release = release.to_str().expect("release path is valid");
            format!(
                ":wait_for_release\r\nif exist \"{release}\" goto release_observed\r\n\"{ping}\" -n 2 127.0.0.1 >nul\r\ngoto wait_for_release\r\n:release_observed"
            )
        },
    );
    let child_path = child_script.path().join("descendant.cmd");
    fs::write(
        &child_path,
        format!(
            "@echo off\r\necho started>\"{started_text}\"\r\n{completion_wait}\r\necho descendant>\"{marker_text}\"\r\nexit /b 0\r\n"
        ),
    )
    .expect("delayed descendant script is writable");
    let child_path = child_path.to_str().expect("child path is valid");
    let hold = if hold_root {
        format!("\"{ping}\" -n 30 127.0.0.1 >nul")
    } else {
        // The root exits immediately after the START handshake. The child
        // itself writes the final marker after roughly one second.
        String::new()
    };
    let root_path = child_script.path().join("root.cmd");
    fs::write(
        &root_path,
        format!(
            "@echo off\r\nstart \"\" /B \"{cmd}\" /D /C call \"{child_path}\"\r\n:wait_for_descendant\r\nif exist \"{started_text}\" goto descendant_started\r\n\"{ping}\" -n 1 127.0.0.1 >nul\r\ngoto wait_for_descendant\r\n:descendant_started\r\n{hold}\r\nexit /b 0\r\n"
        ),
    )
    .expect("root script is writable");
    let root_path = root_path.to_str().expect("root path is valid");
    DescendantFixture {
        marker,
        started,
        args: vec!["/D".into(), "/C".into(), root_path.into()],
        _child_directory: child_script,
    }
}

fn wait_for_marker(path: &Path) {
    let deadline = Instant::now() + MGBA_PROBE_TIMEOUT;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "descendant-start handshake did not create {}",
        path.display()
    );
}

#[test]
fn fixture_directory_owns_and_cleans_its_spaced_outer_directory() {
    let outer = {
        let directory = fixture_directory();
        let outer = directory.outer_path().to_path_buf();
        assert!(outer.exists(), "outer fixture directory was created");
        outer
    };
    assert!(
        !outer.exists(),
        "owned outer fixture directory must be removed after nested cleanup"
    );
}

#[tokio::test]
async fn contained_process_waits_and_reaps() {
    let supervisor = MgbaSupervisor::spawn(&command_path(), &["/C".into(), "exit 0".into()])
        .expect("contained command starts");
    let status = supervisor.wait().await.expect("contained command reaps");
    assert!(status.success());
    assert!(supervisor.try_wait().expect("status snapshot").is_some());
}

#[test]
fn probe_captures_bounded_output_inside_job() {
    let output = probe(
        &command_path(),
        &["/C".into(), "echo mGBA 0.10.5".into()],
        Duration::from_secs(2),
    )
    .expect("contained probe completes");
    let ProbeOutput { status, stdout, .. } = output;
    assert!(status.success());
    assert!(String::from_utf8_lossy(&stdout).contains("mGBA 0.10.5"));
}

#[test]
fn probe_rejects_oversized_output_without_reader_lifetime() {
    let directory = tempfile::tempdir().expect("probe fixture directory is writable");
    let payload = directory.path().join("oversized-output.bin");
    fs::write(&payload, vec![b'x'; MAX_MGBA_OUTPUT_BYTES + 1])
        .expect("oversized probe payload is writable");
    let error = probe(
        &command_path(),
        &[
            "/D".into(),
            "/C".into(),
            "type".into(),
            payload.to_str().expect("payload path is valid").into(),
        ],
        MGBA_PROBE_TIMEOUT,
    )
    .expect_err("oversized output must be rejected");
    assert!(
        matches!(
            &error,
            coop_launcher::windows_mgba_supervisor::ProbeFailure::OutputTooLarge
        ),
        "unexpected probe failure: {error:?}"
    );
}

#[test]
fn probe_timeout_kills_started_delayed_descendant_before_marker() {
    let directory = fixture_directory();
    let release = directory.path().join("descendant release");
    let fixture = gated_descendant(&directory, true, &release);
    let started = fixture.started.clone();
    let args = fixture.args.clone();
    let join = thread::spawn(move || probe(&command_path(), &args, MGBA_PROBE_TIMEOUT));
    wait_for_marker(&started);
    let error = join
        .join()
        .expect("probe thread reaps")
        .expect_err("held root exceeds probe deadline");
    assert!(
        matches!(
            &error,
            coop_launcher::windows_mgba_supervisor::ProbeFailure::Timeout
        ),
        "unexpected probe failure: {error:?}"
    );
    fs::write(&release, b"release").expect("descendant release gate is writable");
    thread::sleep(Duration::from_secs(2));
    assert!(
        !fixture.marker.exists(),
        "descendant survived timed-out probe Job cleanup"
    );
}

#[tokio::test]
async fn root_exit_kills_started_delayed_descendant_before_marker() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, false);
    let supervisor =
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts");
    wait_for_marker(&fixture.started);
    let status = supervisor.wait().await.expect("contained root reaps");
    assert!(status.success(), "root must exit normally: {status:?}");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!fixture.marker.exists());
}

#[tokio::test]
async fn forced_stop_kills_started_delayed_descendant_before_marker() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    let mut supervisor =
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts");
    wait_for_marker(&fixture.started);
    supervisor.stop().await.expect("contained stop reaps");
    thread::sleep(Duration::from_secs(2));
    assert!(!fixture.marker.exists());
    supervisor
        .stop()
        .await
        .expect("repeated stop is idempotent");
}

#[tokio::test]
async fn cancelled_shutdown_replays_owner_evidence_without_a_second_termination() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    let mut supervisor =
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts");
    wait_for_marker(&fixture.started);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut shutdown = Box::pin(supervisor.shutdown(false, deadline));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(
        matches!(shutdown.as_mut().poll(&mut context), Poll::Pending),
        "shutdown must expose a cancellation point while the owner works"
    );
    drop(shutdown);

    let evidence = supervisor
        .shutdown(false, Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(
        evidence.root,
        coop_launcher::windows_mgba_supervisor::RootReapEvidence::Reaped
    );
    assert_eq!(
        supervisor
            .shutdown(false, Instant::now() + Duration::from_secs(5))
            .await,
        evidence,
        "repeated shutdown must replay the same terminal evidence"
    );
    thread::sleep(Duration::from_secs(3));
    assert!(!fixture.marker.exists());
}

#[tokio::test]
async fn cancelled_enqueued_stop_preserves_cached_result_for_retry() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    let mut supervisor =
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts");
    wait_for_marker(&fixture.started);
    let mut stop = Box::pin(supervisor.stop());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(
        matches!(stop.as_mut().poll(&mut context), Poll::Pending),
        "first stop must be cancelled after enqueue"
    );
    drop(stop);
    supervisor
        .stop()
        .await
        .expect("retry must use the first cached stop result");
    supervisor
        .stop()
        .await
        .expect("cached stop result remains idempotent");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!fixture.marker.exists());
}

#[test]
fn drop_kills_started_delayed_descendant_before_marker() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    {
        let _supervisor =
            MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts");
        wait_for_marker(&fixture.started);
    }
    thread::sleep(Duration::from_secs(2));
    assert!(!fixture.marker.exists());
}

#[tokio::test]
async fn cancellation_drop_kills_started_delayed_descendant_before_marker() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    let supervisor = Arc::new(
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts"),
    );
    wait_for_marker(&fixture.started);
    let waiter = Arc::clone(&supervisor);
    let task = tokio::spawn(async move { waiter.wait().await });
    tokio::task::yield_now().await;
    task.abort();
    let _ = task.await;
    let mut supervisor = Arc::try_unwrap(supervisor).expect("cancelled wait released");
    supervisor
        .stop()
        .await
        .expect("stop remains usable after wait cancellation");
    supervisor
        .stop()
        .await
        .expect("repeated stop remains idempotent after cancellation");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!fixture.marker.exists());
}

#[test]
fn cancelled_waiters_do_not_fill_the_queue_or_break_stop() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, true);
    let supervisor = Arc::new(
        MgbaSupervisor::spawn(&command_path(), &fixture.args).expect("contained root starts"),
    );
    wait_for_marker(&fixture.started);
    let mut tasks = Vec::new();
    for _ in 0..(8 + 4) {
        let wait = Arc::clone(&supervisor);
        tasks.push(thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("runtime starts");
            runtime.block_on(async {
                tokio::time::timeout(Duration::from_millis(20), wait.wait()).await
            })
        }));
    }
    for task in tasks {
        let _ = task.join().expect("cancelled wait thread joins");
    }
    let mut supervisor = Arc::try_unwrap(supervisor).expect("all waiters released");
    let runtime = tokio::runtime::Runtime::new().expect("runtime starts");
    runtime.block_on(async {
        supervisor.stop().await.expect("stop after cancelled waits");
        supervisor
            .stop()
            .await
            .expect("repeated stop is idempotent");
    });
    thread::sleep(Duration::from_secs(2));
    assert!(!fixture.marker.exists());
}

#[test]
fn delayed_descendant_fixture_writes_marker_without_containment() {
    let directory = fixture_directory();
    let fixture = delayed_descendant(&directory, false);
    let mut root = std::process::Command::new(command_path())
        .args(&fixture.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fixture root starts");
    wait_for_marker(&fixture.started);
    root.wait().expect("fixture root reaps");
    thread::sleep(Duration::from_secs(2));
    assert!(fixture.marker.exists());
}

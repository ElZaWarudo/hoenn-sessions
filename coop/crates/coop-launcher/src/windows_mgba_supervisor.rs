//! Windows-only mGBA process containment.
//!
//! `windows-spawn` owns the `CreateProcessW` transaction and attaches the
//! caller-provided Job through `PROC_THREAD_ATTRIBUTE_JOB_LIST`.  This module
//! keeps that crate's blocking `Child` and `Job` values on one dedicated
//! thread.  Tokio sees only a bounded command channel and one-shot replies;
//! dropping the boundary synchronously asks that thread to terminate the Job
//! and joins it before ownership is released.

use std::{
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, oneshot};
use windows_spawn::{Child, Command, DropPolicy, Job, SpawnOptions, Stdio};

const COMMAND_CAPACITY: usize = 8;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const TERMINATION_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_OUTPUT_BYTES: usize = 64 * 1024;
// This NT namespace path is resolved by the OS, not by a mutable process
// environment variable. GLOBALROOT bypasses DOS-device and WoW64 path
// substitution while SystemRoot is the kernel's trusted system-directory
// link. The launcher is the signed Windows x64 build, so the real System32
// taskkill helper is required here.
const TRUSTED_SYSTEM32_DIRECTORY: &str = r"\\?\GLOBALROOT\SystemRoot\System32";

/// Evidence for the one best-effort Qt-friendly close request.  A helper
/// success is deliberately not an exit claim: the retained process handle is
/// the only root-exit oracle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftCloseEvidence {
    NotAttempted,
    Requested,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootReapEvidence {
    Reaped,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobTerminationEvidence {
    Initiated,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryEvidence {
    Clean,
    Required,
}

/// Typed evidence returned by the contained supervisor's bounded shutdown.
/// `JobTerminationEvidence::Initiated` records only the native request; this API has
/// no descendant-count notification and therefore never claims Job emptiness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MgbaShutdownEvidence {
    pub soft_close: SoftCloseEvidence,
    pub root: RootReapEvidence,
    pub job: JobTerminationEvidence,
    pub recovery: RecoveryEvidence,
}

#[derive(Debug)]
pub struct MgbaSupervisor {
    commands: mpsc::Sender<SupervisorCommand>,
    state: Arc<Mutex<Option<ExitStatus>>>,
    stop_state: Arc<Mutex<Option<io::Result<()>>>>,
    shutdown_state: Arc<Mutex<Option<MgbaShutdownEvidence>>>,
    shutdown: Arc<AtomicBool>,
    stop_result: Option<io::Result<()>>,
    join: Option<thread::JoinHandle<()>>,
}

struct SupervisorShared {
    state: Arc<Mutex<Option<ExitStatus>>>,
    stop_state: Arc<Mutex<Option<io::Result<()>>>>,
    shutdown_state: Arc<Mutex<Option<MgbaShutdownEvidence>>>,
    shutdown: Arc<AtomicBool>,
}

enum SupervisorCommand {
    Wait {
        reply: oneshot::Sender<io::Result<ExitStatus>>,
    },
    Stop {
        reply: oneshot::Sender<io::Result<()>>,
    },
    Shutdown,
    ShutdownPlan {
        soft_close: bool,
        deadline: Instant,
        reply: oneshot::Sender<MgbaShutdownEvidence>,
    },
}

#[derive(Debug)]
pub struct ProbeOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum ProbeFailure {
    Spawn(io::Error),
    Output(io::Error),
    OutputTooLarge,
    Timeout,
    Cleanup(io::Error),
}

impl MgbaSupervisor {
    /// Creates one contained, long-lived mGBA process.
    ///
    /// The process is not returned until its Job has been created, configured
    /// kill-on-close, and attached to the creation transaction.  The caller
    /// therefore cannot observe an uncontained process handle.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor thread or contained process
    /// cannot be created.
    pub fn spawn(path: &Path, args: &[String]) -> io::Result<Self> {
        let path = path.to_owned();
        let args = args.to_vec();
        let (commands, mut command_receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (ready_sender, ready_receiver) = std_mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(None));
        let stop_state = Arc::new(Mutex::new(None));
        let shutdown_state = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));
        let shared = SupervisorShared {
            state: Arc::clone(&state),
            stop_state: Arc::clone(&stop_state),
            shutdown_state: Arc::clone(&shutdown_state),
            shutdown: Arc::clone(&shutdown),
        };
        let join = thread::Builder::new()
            .name("coop-mgba-supervisor".to_owned())
            .spawn(move || {
                supervisor_thread(&path, &args, &mut command_receiver, &ready_sender, &shared);
            })
            .map_err(io::Error::other)?;

        match ready_receiver.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                commands,
                state,
                stop_state,
                shutdown_state,
                shutdown,
                stop_result: None,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = commands.try_send(SupervisorCommand::Shutdown);
                let _ = join.join();
                Err(io::Error::new(io::ErrorKind::TimedOut, error.to_string()))
            }
        }
    }

    /// Waits for the contained root and asks Windows to terminate its Job
    /// descendants before returning.
    ///
    /// `windows-spawn` exposes no descendant-count wait primitive. The root
    /// handle is reaped with `Child::try_wait`, while `Job::terminate` invokes
    /// Windows' `TerminateJobObject` operation for every process in that Job.
    /// `windows-spawn` exposes no descendant-count or descendant-wait
    /// primitive, so the retained-descendant marker tests provide bounded
    /// non-survival evidence but cannot prove that a Job has zero active
    /// descendants at the instant this method returns.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor boundary closes unexpectedly
    /// or the root process cannot be reaped.
    pub async fn wait(&self) -> io::Result<ExitStatus> {
        // A naturally exited root publishes its status before the owner
        // thread releases Child/Job and returns. Read that terminal value
        // first so a wait racing the final command-drain cannot lose its
        // reply when the owner thread exits.
        if let Some(status) = self.cached_exit_status()? {
            return Ok(status);
        }
        let (reply, receiver) = oneshot::channel();
        if self
            .commands
            .send(SupervisorCommand::Wait { reply })
            .await
            .is_err()
        {
            return self.cached_exit_status()?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "mGBA supervisor closed")
            });
        }
        match receiver.await {
            Ok(result) => result,
            Err(_) => self.cached_exit_status()?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "mGBA supervisor dropped")
            }),
        }
    }

    /// Requests Job termination, polls the root handle to a deadline, and
    /// joins the supervisor thread.
    ///
    /// The native `TerminateJobObject` request is synchronous and has no safe
    /// cancellation API in `windows-spawn`; the deadline bounds only the
    /// subsequent root-handle polling. The owner thread is joined before this
    /// method reports its cached result.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor boundary closes unexpectedly
    /// or cleanup reports uncertainty about termination initiation or root
    /// reaping. A successful root observation is not descendant-empty proof.
    pub async fn stop(&mut self) -> io::Result<()> {
        if let Some(result) = self.cached_stop_result()? {
            // Natural root exit records the idempotent stop outcome before
            // the owner thread's final return. Join before exposing that
            // cached result so Child/Job handles have been released.
            self.join_thread();
            self.stop_result = Some(result.clone_result());
            return result;
        }
        if let Some(result) = self.stop_result.as_ref().map(CloneResult::clone_result) {
            self.join_thread();
            return result;
        }
        if self.join.is_none() {
            let result = Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mGBA supervisor closed before stop",
            ));
            self.stop_result = Some(result.clone_result());
            return result;
        }
        let (reply, receiver) = oneshot::channel();
        let result = match self.commands.send(SupervisorCommand::Stop { reply }).await {
            Ok(()) => {
                // Keep a cancellation point after the command is enqueued and
                // before awaiting its reply. If the caller is cancelled here,
                // the supervisor thread still records the terminal outcome
                // for the next stop attempt.
                tokio::task::yield_now().await;
                match receiver.await {
                    Ok(result) => result,
                    Err(_) => Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "mGBA supervisor dropped",
                    )),
                }
            }
            Err(_) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mGBA supervisor closed",
            )),
        };
        // The Stop command may already have been consumed when this future is
        // cancelled, leaving its one-shot reply undeliverable. Always inspect
        // the thread-owned terminal result before interpreting that dropped
        // reply as BrokenPipe; a retry must observe the first stop outcome.
        let result = if let Some(shared) = self.cached_stop_result()? {
            shared
        } else {
            result
        };
        self.join_thread();
        self.stop_result = Some(result.clone_result());
        result
    }

    /// Runs the complete bounded mGBA close path.  The helper and the Job
    /// operation execute on the owner thread so the retained Child handle
    /// cannot be released between PID capture and the exact System32 helper
    /// invocation.  A cancellation after enqueue is safe: the owner records
    /// the same terminal evidence and the next call replays it.
    pub async fn shutdown(
        &mut self,
        attempt_soft_close: bool,
        deadline: Instant,
    ) -> MgbaShutdownEvidence {
        if let Some(evidence) = self.cached_shutdown_evidence() {
            return if self.join_thread_until(deadline).await {
                evidence
            } else {
                evidence_requires_recovery(evidence)
            };
        }
        if self.join.is_none() {
            return MgbaShutdownEvidence {
                soft_close: SoftCloseEvidence::NotAttempted,
                root: if self.cached_exit_status().ok().flatten().is_some() {
                    RootReapEvidence::Reaped
                } else {
                    RootReapEvidence::Unknown
                },
                job: JobTerminationEvidence::Unknown,
                recovery: RecoveryEvidence::Required,
            };
        }
        let (reply, receiver) = oneshot::channel();
        let command = SupervisorCommand::ShutdownPlan {
            soft_close: attempt_soft_close,
            deadline,
            reply,
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let result = if matches!(
            tokio::time::timeout(remaining, self.commands.send(command)).await,
            Ok(Ok(()))
        ) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, receiver).await {
                Ok(Ok(evidence)) => evidence,
                _ => MgbaShutdownEvidence {
                    soft_close: SoftCloseEvidence::NotAttempted,
                    root: RootReapEvidence::Unknown,
                    job: JobTerminationEvidence::Unknown,
                    recovery: RecoveryEvidence::Required,
                },
            }
        } else {
            MgbaShutdownEvidence {
                soft_close: SoftCloseEvidence::NotAttempted,
                root: RootReapEvidence::Unknown,
                job: JobTerminationEvidence::Unknown,
                recovery: RecoveryEvidence::Required,
            }
        };
        // If the result raced cancellation/EOF, prefer owner-published
        // evidence. The owner must finish before clean evidence is returned:
        // that join is what closes Child and Job handles before the launcher
        // can release its workspace or lease. If the owner is still blocked
        // at the absolute deadline, retain recovery rather than joining past
        // the bound.
        let result = self.cached_shutdown_evidence().unwrap_or(result);
        if self.join_thread_until(deadline).await {
            result
        } else {
            evidence_requires_recovery(result)
        }
    }

    /// Sends a synchronous Job-termination request and joins the owner
    /// thread. This is used only from cancellation/drop paths where awaiting
    /// is impossible. `TerminateJobObject` is synchronous and uncancellable in
    /// the safe `windows-spawn` API; joining does not prove descendants are
    /// absent.
    pub fn shutdown_sync(&mut self) -> MgbaShutdownEvidence {
        if let Some(evidence) = self.cached_shutdown_evidence() {
            self.join_thread();
            return evidence;
        }
        self.shutdown.store(true, Ordering::Release);
        let _ = self.commands.try_send(SupervisorCommand::Shutdown);
        self.join_thread();
        self.cached_shutdown_evidence()
            .unwrap_or(MgbaShutdownEvidence {
                soft_close: SoftCloseEvidence::NotAttempted,
                root: RootReapEvidence::Unknown,
                job: JobTerminationEvidence::Unknown,
                recovery: RecoveryEvidence::Required,
            })
    }

    /// A nonblocking status snapshot retained for test and event diagnostics.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the supervisor state lock is poisoned.
    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        self.state
            .lock()
            .map(|state| *state)
            .map_err(|_| io::Error::other("mGBA supervisor state poisoned"))
    }

    fn join_thread(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    async fn join_thread_until(&mut self, deadline: Instant) -> bool {
        loop {
            if self.join.is_none() {
                return true;
            }
            if self
                .join
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished)
            {
                self.join_thread();
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let next_poll = (now + POLL_INTERVAL).min(deadline);
            tokio::time::sleep_until(tokio::time::Instant::from_std(next_poll)).await;
        }
    }

    fn cached_stop_result(&self) -> io::Result<Option<io::Result<()>>> {
        self.stop_state
            .lock()
            .map(|state| state.as_ref().map(CloneResult::clone_result))
            .map_err(|_| io::Error::other("mGBA supervisor stop state poisoned"))
    }

    fn cached_exit_status(&self) -> io::Result<Option<ExitStatus>> {
        self.state
            .lock()
            .map(|state| state.as_ref().copied())
            .map_err(|_| io::Error::other("mGBA supervisor state poisoned"))
    }

    fn cached_shutdown_evidence(&self) -> Option<MgbaShutdownEvidence> {
        self.shutdown_state.lock().ok().and_then(|state| *state)
    }
}

impl Drop for MgbaSupervisor {
    fn drop(&mut self) {
        // The shutdown request is best-effort, but the join is mandatory: a
        // supervisor may not outlive the launcher that owns its process and
        // Job handles. A synchronous native operation can exceed the nominal
        // polling deadline; safe Rust has no cancellation primitive for it.
        let _ = self.shutdown_sync();
    }
}

fn supervisor_thread(
    path: &Path,
    args: &[String],
    commands: &mut mpsc::Receiver<SupervisorCommand>,
    ready: &std_mpsc::SyncSender<io::Result<()>>,
    shared: &SupervisorShared,
) {
    let (job, mut child) = match spawn_owned(path, args) {
        Ok(value) => value,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    supervisor_loop(commands, &mut child, &job, shared);
}

fn spawn_owned(path: &Path, args: &[String]) -> io::Result<(Job, Child)> {
    let job = Job::create().and_then(|job| {
        job.set_kill_on_close(true)?;
        Ok(job)
    })?;
    let mut command = Command::new(path);
    configure_command(&mut command, path, args);
    let child = command.spawn_with(
        SpawnOptions::new()
            .job(&job)
            .drop_policy(DropPolicy::KillTree),
    )?;
    Ok((job, child))
}

fn supervisor_loop(
    commands: &mut mpsc::Receiver<SupervisorCommand>,
    child: &mut Child,
    job: &Job,
    shared: &SupervisorShared,
) {
    let mut pending_waits: Vec<oneshot::Sender<io::Result<ExitStatus>>> =
        Vec::with_capacity(COMMAND_CAPACITY);
    let mut finished = None;

    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            let evidence = shutdown_owned_process(
                child,
                job,
                finished.as_ref(),
                false,
                Instant::now() + TERMINATION_TIMEOUT,
            );
            let result = if matches!(evidence.recovery, RecoveryEvidence::Required) {
                Err(io::Error::other("mGBA shutdown uncertain"))
            } else {
                Ok(())
            };
            remember_stop_result(&shared.stop_state, &result);
            remember_shutdown_evidence(&shared.shutdown_state, evidence);
            return;
        }
        poll_owned_root(child, job, &mut finished, &mut pending_waits, shared);

        if process_supervisor_commands(
            commands,
            &mut finished,
            child,
            job,
            &mut pending_waits,
            &shared.stop_state,
            &shared.shutdown_state,
        ) {
            return;
        }
        if finished.is_some() {
            // The root and its Job have reached their terminal state. Any
            // waiters queued in this iteration were answered above; release
            // the blocking Child/Job handles now so stop() can join a truly
            // finished owner thread rather than merely observing cached state.
            return;
        }
        if commands.is_closed() && finished.is_none() {
            let termination = terminate_job(child, job);
            let result: io::Result<()> = termination
                .as_ref()
                .map(|_| ())
                .map_err(|error| io::Error::new(error.kind(), error.to_string()));
            remember_stop_result(&shared.stop_state, &result);
            remember_shutdown_evidence(
                &shared.shutdown_state,
                MgbaShutdownEvidence {
                    soft_close: SoftCloseEvidence::NotAttempted,
                    root: if termination.is_ok() {
                        RootReapEvidence::Reaped
                    } else {
                        RootReapEvidence::Unknown
                    },
                    job: if termination.is_ok() {
                        JobTerminationEvidence::Initiated
                    } else {
                        JobTerminationEvidence::Unknown
                    },
                    recovery: if termination.is_ok() {
                        RecoveryEvidence::Clean
                    } else {
                        RecoveryEvidence::Required
                    },
                },
            );
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn poll_owned_root(
    child: &mut Child,
    job: &Job,
    finished: &mut Option<io::Result<ExitStatus>>,
    pending_waits: &mut Vec<oneshot::Sender<io::Result<ExitStatus>>>,
    shared: &SupervisorShared,
) {
    if finished.is_some() {
        return;
    }
    match child.try_wait() {
        Ok(Some(status)) => {
            let result = terminate_job(child, job).map(|_| status);
            let stop_result = exit_result_to_unit(&result);
            remember_stop_result(&shared.stop_state, &stop_result);
            remember_shutdown_evidence(
                &shared.shutdown_state,
                MgbaShutdownEvidence {
                    soft_close: SoftCloseEvidence::NotAttempted,
                    root: if result.is_ok() {
                        RootReapEvidence::Reaped
                    } else {
                        RootReapEvidence::Unknown
                    },
                    job: if result.is_ok() {
                        JobTerminationEvidence::Initiated
                    } else {
                        JobTerminationEvidence::Unknown
                    },
                    recovery: if result.is_ok() {
                        RecoveryEvidence::Clean
                    } else {
                        RecoveryEvidence::Required
                    },
                },
            );
            if let Ok(status) = result.as_ref()
                && let Ok(mut current) = shared.state.lock()
            {
                *current = Some(*status);
            }
            for reply in pending_waits.drain(..) {
                let _ = reply.send(result.clone_result());
            }
            *finished = Some(result);
        }
        Ok(None) => {}
        Err(error) => {
            let result = Err(error);
            for reply in pending_waits.drain(..) {
                let _ = reply.send(result.clone_result());
            }
            *finished = Some(result);
        }
    }
}

fn process_supervisor_commands(
    commands: &mut mpsc::Receiver<SupervisorCommand>,
    finished: &mut Option<io::Result<ExitStatus>>,
    child: &mut Child,
    job: &Job,
    pending_waits: &mut Vec<oneshot::Sender<io::Result<ExitStatus>>>,
    stop_state: &Arc<Mutex<Option<io::Result<()>>>>,
    shutdown_state: &Arc<Mutex<Option<MgbaShutdownEvidence>>>,
) -> bool {
    pending_waits.retain(|reply| !reply.is_closed());
    while let Ok(command) = commands.try_recv() {
        match command {
            SupervisorCommand::Wait { reply } => {
                if reply.is_closed() {
                    continue;
                }
                if let Some(result) = finished {
                    let _ = reply.send(result.clone_result());
                } else if pending_waits.len() < COMMAND_CAPACITY {
                    pending_waits.push(reply);
                } else {
                    let _ = reply.send(Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "mGBA wait queue is full",
                    )));
                }
            }
            SupervisorCommand::Stop { reply } => {
                let result = if let Some(result) = finished {
                    result.clone_result().map(|_| ())
                } else {
                    terminate_job(child, job).map(|_| ())
                };
                remember_stop_result(stop_state, &result);
                remember_shutdown_evidence(
                    shutdown_state,
                    MgbaShutdownEvidence {
                        soft_close: SoftCloseEvidence::NotAttempted,
                        root: if result.is_ok() {
                            RootReapEvidence::Reaped
                        } else {
                            RootReapEvidence::Unknown
                        },
                        job: if result.is_ok() {
                            JobTerminationEvidence::Initiated
                        } else {
                            JobTerminationEvidence::Unknown
                        },
                        recovery: if result.is_ok() {
                            RecoveryEvidence::Clean
                        } else {
                            RecoveryEvidence::Required
                        },
                    },
                );
                let _ = reply.send(result.clone_result());
                for reply in pending_waits.drain(..) {
                    let _ = reply.send(Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "mGBA stopped before wait completed",
                    )));
                }
                return true;
            }
            SupervisorCommand::Shutdown => {
                let evidence = shutdown_owned_process(
                    child,
                    job,
                    finished.as_ref(),
                    false,
                    Instant::now() + TERMINATION_TIMEOUT,
                );
                let result = if matches!(evidence.recovery, RecoveryEvidence::Required) {
                    Err(io::Error::other("mGBA shutdown uncertain"))
                } else {
                    Ok(())
                };
                remember_stop_result(stop_state, &result);
                remember_shutdown_evidence(shutdown_state, evidence);
                return true;
            }
            SupervisorCommand::ShutdownPlan {
                soft_close,
                deadline,
                reply,
            } => {
                let evidence =
                    shutdown_owned_process(child, job, finished.as_ref(), soft_close, deadline);
                let result = if matches!(evidence.recovery, RecoveryEvidence::Required) {
                    Err(io::Error::other("mGBA shutdown uncertain"))
                } else {
                    Ok(())
                };
                remember_stop_result(stop_state, &result);
                remember_shutdown_evidence(shutdown_state, evidence);
                let _ = reply.send(evidence);
                return true;
            }
        }
    }
    false
}

fn remember_stop_result(stop_state: &Arc<Mutex<Option<io::Result<()>>>>, result: &io::Result<()>) {
    if let Ok(mut state) = stop_state.lock() {
        *state = Some(result.clone_result());
    }
}

fn remember_shutdown_evidence(
    state: &Arc<Mutex<Option<MgbaShutdownEvidence>>>,
    evidence: MgbaShutdownEvidence,
) {
    if let Ok(mut current) = state.lock() {
        *current = Some(evidence);
    }
}

fn evidence_requires_recovery(evidence: MgbaShutdownEvidence) -> MgbaShutdownEvidence {
    MgbaShutdownEvidence {
        recovery: RecoveryEvidence::Required,
        ..evidence
    }
}

fn shutdown_owned_process(
    child: &mut Child,
    job: &Job,
    finished: Option<&io::Result<ExitStatus>>,
    attempt_soft_close: bool,
    deadline: Instant,
) -> MgbaShutdownEvidence {
    let mut soft_close = SoftCloseEvidence::NotAttempted;
    let mut root_reaped = finished.is_some_and(Result::is_ok);
    if !root_reaped && attempt_soft_close && Instant::now() < deadline {
        soft_close = match request_soft_close(child, deadline) {
            Ok(true) => SoftCloseEvidence::Requested,
            Ok(false) | Err(_) => SoftCloseEvidence::Failed,
        };
    }

    // Give a successful Qt-friendly request a short, bounded opportunity to
    // let the root close naturally.  This is still one absolute deadline.
    if !root_reaped {
        let natural_deadline = if attempt_soft_close {
            (Instant::now() + Duration::from_secs(2)).min(deadline)
        } else {
            Instant::now()
        };
        root_reaped = poll_root(child, natural_deadline);
    }

    // A natural root exit is not descendant evidence.  Always issue the Job
    // request while the Job handle remains owned, then independently reap the
    // root handle.  No API in windows-spawn can prove the Job is empty.
    let mut job_termination_initiated = false;
    if Instant::now() < deadline && job.terminate(1).is_ok() {
        job_termination_initiated = true;
    }
    if !root_reaped {
        root_reaped = poll_root(child, deadline);
    }
    MgbaShutdownEvidence {
        soft_close,
        root: if root_reaped {
            RootReapEvidence::Reaped
        } else {
            RootReapEvidence::Unknown
        },
        job: if job_termination_initiated {
            JobTerminationEvidence::Initiated
        } else {
            JobTerminationEvidence::Unknown
        },
        recovery: if root_reaped && job_termination_initiated {
            RecoveryEvidence::Clean
        } else {
            RecoveryEvidence::Required
        },
    }
}

fn poll_root(child: &mut Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() >= deadline => return false,
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(_) => return false,
        }
    }
}

#[cfg(windows)]
fn request_soft_close(child: &mut Child, deadline: Instant) -> io::Result<bool> {
    // Do not issue a PID command for a root whose retained handle already
    // proves exit. This closes the ordinary PID-reuse path; the borrow of the
    // retained handle then remains live for the complete helper transaction.
    if child.try_wait()?.is_some() {
        return Ok(false);
    }
    let (helper, args) = taskkill_command(child.id())?;
    // The retained Child handle is intentionally borrowed for the complete
    // PID operation. Windows keeps a process identifier valid until all
    // handles to that process object are closed, so this owner-held handle
    // closes the PID-reuse window while taskkill resolves /PID. /PID is the
    // only helper mode: no shell, PATH lookup, image, tree, or force flags
    // are ever accepted here.
    let mut helper_child = std::process::Command::new(&helper)
        .env_clear()
        .current_dir(helper.parent().expect("System32 helper has a parent"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    loop {
        match helper_child.try_wait()? {
            Some(status) => return Ok(status.success()),
            None if Instant::now() >= deadline => {
                let _ = helper_child.kill();
                let _ = helper_child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "taskkill helper exceeded shutdown deadline",
                ));
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    }
}

#[cfg(windows)]
fn taskkill_command(pid: u32) -> io::Result<(PathBuf, [String; 2])> {
    let system32 = PathBuf::from(TRUSTED_SYSTEM32_DIRECTORY);
    let helper = system32.join("taskkill.exe");
    if !helper.is_absolute()
        || !helper
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("System32"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "taskkill helper is not the absolute System32 binary",
        ));
    }
    Ok((helper, ["/PID".to_owned(), pid.to_string()]))
}

#[cfg(not(windows))]
fn request_soft_close(_child: &mut Child, _deadline: Instant) -> io::Result<bool> {
    Err(io::Error::other("taskkill helper is Windows-only"))
}

fn exit_result_to_unit(result: &io::Result<ExitStatus>) -> io::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
    }
}

fn configure_command(command: &mut Command, path: &Path, args: &[String]) {
    command.env_clear();
    // The executable is always absolute and its containing directory is the
    // only working-directory authority needed by mGBA.  No ambient PATH or
    // CWD is inherited across this security boundary.
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        command.current_dir(parent);
        // A command-shell fixture (and Windows CRT startup in general) needs
        // the system root, but does not need any ambient user environment.
        // Derive it only for an executable directly under System32 so a
        // caller-controlled PATH/CWD can never influence this value.
        if parent
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("System32"))
            && let Some(system_root) = parent.parent()
        {
            command.env("SystemRoot", system_root);
            command.env("WINDIR", system_root);
            command.env("ComSpec", path);
        }
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
}

fn terminate_job(child: &mut Child, job: &Job) -> io::Result<ExitStatus> {
    // TerminateJobObject applies to every process currently in the Job; the
    // polling loop only reaps the root because windows-spawn has no safe
    // descendant wait/count API. KillTree's private Job is also closed by
    // Child::drop after this root-handle polling phase. Neither a successful
    // request nor an observed root exit proves the Job is descendant-empty.
    terminate_job_until(child, job, Instant::now() + TERMINATION_TIMEOUT)
}

fn terminate_job_until(child: &mut Child, job: &Job, deadline: Instant) -> io::Result<ExitStatus> {
    // TerminateJobObject itself has no safe cancellation API. Once it returns,
    // however, only bounded try_wait polling remains; never call Child::wait.
    job.terminate(1)?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "mGBA root-handle reap deadline elapsed after the Job-termination request",
                ));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => break Err(error),
        }
    }
}

trait CloneResult<T> {
    fn clone_result(&self) -> io::Result<T>;
}

impl CloneResult<()> for io::Result<()> {
    fn clone_result(&self) -> io::Result<()> {
        match self {
            Ok(()) => Ok(()),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }
}

impl CloneResult<ExitStatus> for io::Result<ExitStatus> {
    fn clone_result(&self) -> io::Result<ExitStatus> {
        match self {
            Ok(status) => Ok(*status),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }
}

/// Runs the short `--version` command in its own contained supervisor thread.
/// The same Job boundary as gameplay is used, including for output overflow,
/// timeout, and a root that leaves a file-owning descendant behind.
///
/// # Errors
///
/// Returns a [`ProbeFailure`] when process creation, bounded output capture,
/// timeout handling, or contained cleanup fails.
///
/// The post-spawn poll and root-handle reap use one absolute cleanup
/// deadline. Safe Rust cannot interrupt a synchronous `CreateProcessW` or
/// `TerminateJobObject` call exposed by `windows-spawn`; a stall in either
/// call can therefore delay the mandatory owner-thread join beyond that
/// nominal deadline. The thread is joined rather than detached so a stalled
/// operation cannot outlive the containment owner. A returned cleanup
/// error represents uncertainty about termination initiation or root
/// reaping, not proof that descendants remain or have exited.
pub fn probe(path: &Path, args: &[String], timeout: Duration) -> Result<ProbeOutput, ProbeFailure> {
    let path = path.to_owned();
    let args = args.to_vec();
    let (result_sender, result_receiver) = std_mpsc::sync_channel(1);
    let (abort_sender, abort_receiver) = std_mpsc::sync_channel(1);
    let join = thread::Builder::new()
        .name("coop-mgba-probe-supervisor".to_owned())
        .spawn(move || {
            probe_thread(&path, &args, timeout, &abort_receiver, &result_sender);
        })
        .map_err(ProbeFailure::Spawn)?;
    if let Ok(result) =
        result_receiver.recv_timeout(timeout + TERMINATION_TIMEOUT + Duration::from_secs(1))
    {
        let _ = join.join();
        result
    } else {
        let _ = abort_sender.send(());
        let _ = join.join();
        Err(ProbeFailure::Cleanup(io::Error::new(
            io::ErrorKind::TimedOut,
            "mGBA probe cleanup did not report before its final deadline",
        )))
    }
}

fn probe_thread(
    path: &Path,
    args: &[String],
    timeout: Duration,
    abort: &std_mpsc::Receiver<()>,
    result_sender: &std_mpsc::SyncSender<Result<ProbeOutput, ProbeFailure>>,
) {
    let (mut child, job, stdout, stderr) = match spawn_probe_child(path, args) {
        Ok(value) => value,
        Err(error) => {
            let _ = result_sender.send(Err(error));
            return;
        }
    };
    let operation_deadline = Instant::now() + timeout;
    let cleanup_deadline = operation_deadline + TERMINATION_TIMEOUT;
    let (status, failure) = poll_probe(&mut child, operation_deadline, abort, &stdout, &stderr);

    // The root is either still live or has exited.  Terminating the Job in
    // both cases handle retained output-file handles without any PID-based
    // helper.
    let termination = terminate_job_until(&mut child, &job, cleanup_deadline);
    // Drop the Child and Job only after the synchronous TerminateJobObject
    // request and root-handle poll. The OS operation requests termination for
    // every process in the supplied Job; because windows-spawn has no
    // descendant count/wait API, this is not a proof that no descendant
    // remains active. Any termination error therefore reports initiation or
    // root-reap uncertainty.
    drop(child);
    drop(job);
    let _ = result_sender.send(finish_probe(status, termination, &stdout, &stderr, failure));
}

fn spawn_probe_child(
    path: &Path,
    args: &[String],
) -> Result<(Child, Job, tempfile::NamedTempFile, tempfile::NamedTempFile), ProbeFailure> {
    let stdout = tempfile::NamedTempFile::new().map_err(ProbeFailure::Spawn)?;
    let stderr = tempfile::NamedTempFile::new().map_err(ProbeFailure::Spawn)?;
    let job = Job::create()
        .and_then(|job| {
            job.set_kill_on_close(true)?;
            Ok(job)
        })
        .map_err(ProbeFailure::Spawn)?;
    let mut command = Command::new(path);
    command.env_clear();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        command.current_dir(parent);
        if parent
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("System32"))
            && let Some(system_root) = parent.parent()
        {
            command.env("SystemRoot", system_root);
            command.env("WINDIR", system_root);
            command.env("ComSpec", path);
        }
    }
    command.args(args).stdin(Stdio::null());
    command.stdout(Stdio::from(stdout.reopen().map_err(ProbeFailure::Spawn)?));
    command.stderr(Stdio::from(stderr.reopen().map_err(ProbeFailure::Spawn)?));
    let child = command
        .spawn_with(
            SpawnOptions::new()
                .job(&job)
                .drop_policy(DropPolicy::KillTree),
        )
        .map_err(ProbeFailure::Spawn)?;
    Ok((child, job, stdout, stderr))
}

fn poll_probe(
    child: &mut Child,
    deadline: Instant,
    abort: &std_mpsc::Receiver<()>,
    stdout: &tempfile::NamedTempFile,
    stderr: &tempfile::NamedTempFile,
) -> (Option<ExitStatus>, Option<ProbeFailure>) {
    let mut status = None;
    let mut failure = None;
    loop {
        if abort.try_recv().is_ok() {
            failure = Some(ProbeFailure::Timeout);
            break;
        }
        if temp_file_oversized(stdout) || temp_file_oversized(stderr) {
            failure = Some(ProbeFailure::OutputTooLarge);
            break;
        }
        match child.try_wait() {
            Ok(Some(root_status)) => {
                status = Some(root_status);
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                failure = Some(ProbeFailure::Timeout);
                break;
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                failure = Some(ProbeFailure::Cleanup(error));
                break;
            }
        }
    }
    (status, failure)
}

fn finish_probe(
    status: Option<ExitStatus>,
    termination: io::Result<ExitStatus>,
    stdout_file: &tempfile::NamedTempFile,
    stderr_file: &tempfile::NamedTempFile,
    failure: Option<ProbeFailure>,
) -> Result<ProbeOutput, ProbeFailure> {
    // Cleanup uncertainty is the strongest result: returning timeout or bad
    // output while a Job may still own descendants would be unsafe.
    let cleanup_status = termination.map_err(ProbeFailure::Cleanup)?;
    let Some(status) = status else {
        return Err(failure.unwrap_or_else(|| {
            ProbeFailure::Cleanup(io::Error::other("mGBA probe status unavailable"))
        }));
    };
    let stdout = read_probe_file(stdout_file)?;
    let stderr = read_probe_file(stderr_file)?;
    if let Some(failure) = failure {
        return Err(failure);
    }
    let _ = cleanup_status;
    Ok(ProbeOutput {
        status,
        stdout,
        stderr,
    })
}

fn temp_file_oversized(file: &tempfile::NamedTempFile) -> bool {
    file.as_file()
        .metadata()
        .map(|metadata| metadata.len() > PROBE_OUTPUT_BYTES as u64)
        .unwrap_or(false)
}

fn read_probe_file(file: &tempfile::NamedTempFile) -> Result<Vec<u8>, ProbeFailure> {
    let metadata = file.as_file().metadata().map_err(ProbeFailure::Output)?;
    if metadata.len() > PROBE_OUTPUT_BYTES as u64 {
        return Err(ProbeFailure::OutputTooLarge);
    }
    let mut reader = file.reopen().map_err(ProbeFailure::Output)?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(ProbeFailure::Output)?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|error| ProbeFailure::Output(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(PROBE_OUTPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(ProbeFailure::Output)?;
    if bytes.len() > PROBE_OUTPUT_BYTES {
        return Err(ProbeFailure::OutputTooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        JobTerminationEvidence, MgbaSupervisor, RecoveryEvidence, RootReapEvidence,
        SoftCloseEvidence, taskkill_command,
    };
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    fn command_path() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32")
            .join("cmd.exe")
    }

    #[test]
    fn taskkill_argv_is_exact_absolute_system32_pid_only() {
        let (path, args) = taskkill_command(4242).expect("trusted System32 path is available");
        assert!(path.is_absolute());
        assert_eq!(
            path.parent().and_then(|parent| parent.to_str()),
            Some(r"\\?\GLOBALROOT\SystemRoot\System32")
        );
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("taskkill.exe")
        );
        assert_eq!(args, ["/PID", "4242"]);
    }

    #[test]
    fn taskkill_path_ignores_a_hijacked_systemroot_environment() {
        const PROBE_ENVIRONMENT: &str = "COOP_TASKKILL_SYSTEMROOT_PROBE";
        if std::env::var_os(PROBE_ENVIRONMENT).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let (path, _) = taskkill_command(4242).expect("trusted system path is available");
            assert_eq!(
                path.parent().and_then(|parent| parent.to_str()),
                Some(r"\\?\GLOBALROOT\SystemRoot\System32")
            );
            return;
        }

        let status = std::process::Command::new(
            std::env::current_exe().expect("test executable path is available"),
        )
        .args([
            "--exact",
            "windows_mgba_supervisor::tests::taskkill_path_ignores_a_hijacked_systemroot_environment",
            "--nocapture",
        ])
        .env("SystemRoot", r"C:\attacker-controlled\SystemRoot")
        .env(PROBE_ENVIRONMENT, "1")
        .status()
        .expect("environment probe starts");
        assert!(status.success(), "environment probe failed: {status}");
    }

    #[tokio::test]
    async fn expired_shutdown_returns_without_joining_a_stalled_owner() {
        let mut supervisor = MgbaSupervisor::spawn(
            &command_path(),
            &["/D".into(), "/C".into(), "ping -n 30 127.0.0.1 >nul".into()],
        )
        .expect("contained helper fixture starts");
        let started = Instant::now();
        let evidence = supervisor
            .shutdown(
                false,
                started
                    .checked_sub(Duration::from_millis(1))
                    .expect("test instant has elapsed time"),
            )
            .await;
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(evidence.recovery, RecoveryEvidence::Required);
        assert_eq!(evidence.root, RootReapEvidence::Unknown);
    }

    #[tokio::test]
    async fn shutdown_records_helper_and_job_evidence_without_descendant_claim() {
        let mut supervisor = MgbaSupervisor::spawn(
            &command_path(),
            &["/D".into(), "/C".into(), "ping -n 30 127.0.0.1 >nul".into()],
        )
        .expect("contained helper fixture starts");
        let evidence = supervisor
            .shutdown(true, Instant::now() + Duration::from_secs(5))
            .await;
        assert!(matches!(
            evidence.soft_close,
            SoftCloseEvidence::Requested | SoftCloseEvidence::Failed
        ));
        assert_eq!(evidence.root, RootReapEvidence::Reaped);
        assert_eq!(evidence.job, JobTerminationEvidence::Initiated);
        assert_eq!(evidence.recovery, RecoveryEvidence::Clean);
    }
}

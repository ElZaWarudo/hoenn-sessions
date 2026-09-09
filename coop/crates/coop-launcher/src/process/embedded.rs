//! In-process Android supervision. Protocol state remains in SessionLifecycle.
use super::{
    ControlChannel, ProcessError, RawSupervisorEvent, SessionSettlement, SessionSupervisor,
    SupervisorEvent,
};
use coop_sidecar::control::{CommandStatus, ControlCommand, ControlEvent, ShutdownRequest};
use coop_sidecar::{LocalSidecar, SessionDescriptor};
use std::{io, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

pub struct EmbeddedSupervisor {
    control: ControlChannel,
    sidecar: Option<JoinHandle<Result<(), coop_sidecar::SidecarError>>>,
    host_stop: mpsc::Sender<oneshot::Sender<()>>,
    stopped: bool,
}

impl EmbeddedSupervisor {
    pub async fn start(
        epoch: u32,
        host_stop: mpsc::Sender<oneshot::Sender<()>>,
    ) -> Result<(Self, SessionDescriptor), ProcessError> {
        let sidecar = LocalSidecar::bind_with_epoch(epoch)
            .await
            .map_err(|_| ProcessError::Descriptor)?;
        let descriptor = sidecar.session_descriptor();
        let task = tokio::spawn(sidecar.serve());
        let control = match ControlChannel::connect(&descriptor).await {
            Ok(control) => control,
            Err(error) => {
                task.abort();
                let _ = task.await;
                return Err(error);
            }
        };
        Ok((
            Self {
                control,
                sidecar: Some(task),
                host_stop,
                stopped: false,
            },
            descriptor,
        ))
    }

    async fn stop_tasks(&mut self) -> Result<(), ProcessError> {
        if self.stopped {
            return Ok(());
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let (ack, receive) = oneshot::channel();
        let paused = tokio::time::timeout_at(deadline, async {
            self.host_stop.send(ack).await.map_err(|_| ())?;
            receive.await.map_err(|_| ())
        })
        .await;
        let pump = self.control.shutdown_until(deadline).await;
        if let Some(task) = self.sidecar.take() {
            task.abort();
            let _ = task.await;
        }
        if !matches!(paused, Ok(Ok(()))) || pump.is_err() {
            return Err(ProcessError::Termination(io::Error::other(
                "embedded core did not acknowledge stop",
            )));
        }
        self.stopped = true;
        Ok(())
    }

    #[must_use]
    pub const fn stopped(&self) -> bool {
        self.stopped
    }
}

impl SessionSupervisor for EmbeddedSupervisor {
    fn control(&mut self) -> &mut ControlChannel {
        &mut self.control
    }
    async fn observe_raw(&mut self) -> Result<RawSupervisorEvent, ProcessError> {
        let Some(task) = self.sidecar.as_mut() else {
            return Err(ProcessError::ChildExited);
        };
        tokio::select! {
            biased;
            _ = task => { self.sidecar = None; Err(ProcessError::ChildExited) },
            event = self.control.receive() => event.map(RawSupervisorEvent::Control),
        }
    }
    async fn settle_raw(
        &mut self,
        event: RawSupervisorEvent,
    ) -> Result<SupervisorEvent, ProcessError> {
        match event {
            RawSupervisorEvent::Control(event) => Ok(SupervisorEvent::Control(event)),
            _ => {
                self.stop_tasks().await?;
                Err(ProcessError::ChildExited)
            }
        }
    }
    async fn next_event(&mut self) -> Result<SupervisorEvent, ProcessError> {
        let event = self.observe_raw().await?;
        self.settle_raw(event).await
    }
    async fn stop_in_place(&mut self) -> Result<(), ProcessError> {
        self.stop_tasks().await
    }
    async fn shutdown(&mut self, epoch: u32, drained: bool) -> SessionSettlement {
        if self.stopped {
            return SessionSettlement {
                recovery_required: false,
            };
        }
        if drained {
            let id = super::new_command_id();
            let _ = tokio::time::timeout(Duration::from_millis(500), async {
                self.control
                    .send(&ControlCommand::ShutdownRequest(ShutdownRequest {
                        command_id: id,
                        session_epoch: epoch,
                    }))
                    .await?;
                loop {
                    match self.control.receive().await? {
                        ControlEvent::CommandResult {
                            command_id,
                            status: CommandStatus::Applied | CommandStatus::Replayed,
                            reason: None,
                        } if command_id == id => break Ok::<(), ProcessError>(()),
                        ControlEvent::CommandResult { .. } => return Err(ProcessError::Descriptor),
                        _ => {}
                    }
                }
            })
            .await;
        }
        // Task abortion is joined and native execution is stopped by an explicit
        // frame-thread acknowledgement, not by assumed Windows process evidence.
        SessionSettlement {
            recovery_required: self.stop_tasks().await.is_err(),
        }
    }
}

impl Drop for EmbeddedSupervisor {
    fn drop(&mut self) {
        if let Some(task) = &self.sidecar {
            task.abort();
        }
        if !self.stopped {
            let (ack, _) = oneshot::channel();
            let _ = self.host_stop.try_send(ack);
        }
    }
}

//! Single-process pilot persistence. The database session owns an advisory
//! lock for its lifetime; losing it fences the cache until process restart.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use super::storage::{PostgresRepository, Repository, State, StorageError};

const MAX_STATE_BYTES: usize = 32 * 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(35);
type Job<T> = Box<dyn FnOnce(&mut T) + Send>;

/// All socket work and client runtimes live on an owned OS thread. Waiting
/// callers yield Tokio's worker via `block_in_place`. Current-thread async
/// runtimes fail explicitly instead of silently blocking their event loop.
pub(super) fn blocking<T>(
    operation: impl FnOnce() -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(operation)
        }
        Ok(_) => Err(StorageError::InvalidConfiguration),
        Err(_) => operation(),
    }
}

pub(super) struct IoWorker<T> {
    sender: mpsc::SyncSender<Job<T>>,
    failed: AtomicBool,
    admission: Mutex<()>,
    timeout: Duration,
}

impl<T: Send + 'static> IoWorker<T> {
    pub(super) fn start(
        factory: impl FnOnce() -> Result<T, StorageError> + Send + 'static,
    ) -> Result<Self, StorageError> {
        Self::start_with_timeout(factory, IO_TIMEOUT)
    }

    pub(super) fn start_with_timeout(
        factory: impl FnOnce() -> Result<T, StorageError> + Send + 'static,
        timeout: Duration,
    ) -> Result<Self, StorageError> {
        blocking(|| {
            let (sender, receiver) = mpsc::sync_channel::<Job<T>>(1);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("coop-storage".into())
                .spawn(move || {
                    let mut client = match factory() {
                        Ok(client) => client,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    while let Ok(job) = receiver.recv() {
                        job(&mut client);
                    }
                })
                .map_err(|_| StorageError::Transaction)?;
            ready_rx
                .recv_timeout(IO_TIMEOUT)
                .map_err(|_| StorageError::Transaction)??;
            Ok(Self {
                sender,
                failed: AtomicBool::new(false),
                admission: Mutex::new(()),
                timeout,
            })
        })
    }

    pub(super) fn run<R: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut T) -> Result<R, StorageError> + Send + 'static,
    ) -> Result<R, StorageError> {
        blocking(|| {
            // Queue time belongs to admission, not the active operation's
            // socket deadline. HTTP admission bounds the number of waiters.
            let _admission = self.admission.lock().map_err(|_| StorageError::Lock)?;
            if self.failed.load(Ordering::Acquire) {
                return Err(StorageError::Transaction);
            }
            let (sender, receiver) = mpsc::sync_channel(1);
            self.sender
                .try_send(Box::new(move |client| {
                    let _ = sender.send(operation(client));
                }))
                .map_err(|_| StorageError::Transaction)?;
            if let Ok(result) = receiver.recv_timeout(self.timeout) {
                result
            } else {
                self.failed.store(true, Ordering::Release);
                Err(StorageError::Transaction)
            }
        })
    }
}

/// Versioned CBOR metadata checkpoint, suited to a small, single-server pilot.
/// Save bytes live in the object store, never in this row. No callback retries.
pub(super) struct PostgresStateRepository {
    io: IoWorker<postgres::Client>,
    state: Mutex<State>,
    fenced: AtomicBool,
}

fn encode(state: &State) -> Result<Vec<u8>, StorageError> {
    let mut bytes = Vec::new();
    ciborium::into_writer(state, &mut bytes).map_err(|_| StorageError::Transaction)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(StorageError::Transaction);
    }
    Ok(bytes)
}

impl PostgresStateRepository {
    /// Opens the database and exclusively owns the pilot runtime.
    ///
    /// # Errors
    /// Fails on an unavailable database, another owner, or unsupported state.
    pub(super) fn connect(database_url: &str) -> Result<Self, StorageError> {
        let database_url = database_url.to_owned();
        let io = IoWorker::start(move || {
            let mut config: postgres::Config = database_url
                .parse()
                .map_err(|_| StorageError::InvalidConfiguration)?;
            config.connect_timeout(Duration::from_secs(5));
            let mut client = config
                .connect(postgres::NoTls)
                .map_err(|_| StorageError::Transaction)?;
            client.batch_execute("SET statement_timeout = '5s'; SET lock_timeout = '3s'; SET idle_in_transaction_session_timeout = '10s'")
                .map_err(|_| StorageError::Transaction)?;
            let owned: bool = client
                .query_one("SELECT pg_try_advisory_lock(1129271120, 1)", &[])
                .map_err(|_| StorageError::Transaction)?
                .get(0);
            if !owned {
                return Err(StorageError::Transaction);
            }
            client
                .batch_execute(include_str!("../../migrations/0003_pilot_checkpoint.sql"))
                .map_err(|_| StorageError::Transaction)?;
            let initial = encode(&State::default())?;
            client.execute("INSERT INTO coop_pilot_checkpoint (id, format_version, payload) VALUES (1, 1, $1) ON CONFLICT (id) DO NOTHING", &[&initial])
                .map_err(|_| StorageError::Transaction)?;
            Ok(client)
        })?;
        let bytes: Vec<u8> = io.run(|client| {
            let row = client
                .query_one(
                    "SELECT format_version, payload FROM coop_pilot_checkpoint WHERE id = 1",
                    &[],
                )
                .map_err(|_| StorageError::Transaction)?;
            if row.get::<_, i32>(0) != 1 {
                return Err(StorageError::Transaction);
            }
            Ok(row.get(1))
        })?;
        if bytes.len() > MAX_STATE_BYTES {
            return Err(StorageError::Transaction);
        }
        let mut reader = bytes.as_slice();
        let state = ciborium::from_reader(&mut reader).map_err(|_| StorageError::Transaction)?;
        if !reader.is_empty() {
            return Err(StorageError::Transaction);
        }
        Ok(Self {
            io,
            state: Mutex::new(state),
            fenced: AtomicBool::new(false),
        })
    }

    fn healthy(&self) -> Result<(), StorageError> {
        if self.fenced.load(Ordering::Acquire) {
            return Err(StorageError::Persistence);
        }
        self.io
            .run(|client| {
                client
                    .simple_query("SELECT 1")
                    .map(|_| ())
                    .map_err(|_| StorageError::Transaction)
            })
            .inspect_err(|_| {
                self.fenced.store(true, Ordering::Release);
            })
            .map_err(|_| StorageError::Persistence)
    }
}

impl Repository for PostgresStateRepository {
    fn read_transaction(
        &self,
        operation: &mut dyn FnMut(&State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        blocking(|| {
            let state = self.state.lock().map_err(|_| StorageError::Lock)?;
            self.healthy()?;
            operation(&state)
        })
    }

    fn write_transaction(
        &self,
        operation: &mut dyn FnMut(&mut State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        blocking(|| {
            let mut state = self.state.lock().map_err(|_| StorageError::Lock)?;
            self.healthy()?;
            // The service deliberately mutates replay revocations before
            // returning an authentication error. Commit those mutations too.
            let outcome = operation(&mut state);
            let persisted = encode(&state).and_then(|bytes| self.io.run(move |client| {
                let mut transaction = client.transaction().map_err(|_| StorageError::Transaction)?;
                transaction.query_one("SELECT id FROM coop_pilot_checkpoint WHERE id = 1 FOR UPDATE", &[]).map_err(|_| StorageError::Transaction)?;
                let changed = transaction.execute("UPDATE coop_pilot_checkpoint SET payload = $1, updated_at = now() WHERE id = 1 AND format_version = 1", &[&bytes]).map_err(|_| StorageError::Transaction)?;
                if changed != 1 { return Err(StorageError::Transaction); }
                transaction.commit().map_err(|_| StorageError::Transaction)
            }));
            if persisted.is_err() {
                self.fenced.store(true, Ordering::Release);
            }
            persisted.map_err(|_| StorageError::Persistence)?;
            outcome
        })
    }
}

impl PostgresRepository for PostgresStateRepository {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_jobs_get_their_full_deadline_after_admission() {
        let worker = std::sync::Arc::new(
            IoWorker::start_with_timeout(|| Ok(()), Duration::from_millis(500)).expect("worker"),
        );
        let (started, ready) = mpsc::sync_channel(1);
        let first = worker.clone();
        let thread = std::thread::spawn(move || {
            first.run(move |()| {
                started.send(()).expect("started");
                std::thread::sleep(Duration::from_millis(300));
                Ok(1)
            })
        });
        ready.recv().expect("first job running");
        assert_eq!(
            worker.run(|()| {
                std::thread::sleep(Duration::from_millis(300));
                Ok(2)
            }),
            Ok(2)
        );
        assert_eq!(thread.join().expect("first caller"), Ok(1));
    }
}

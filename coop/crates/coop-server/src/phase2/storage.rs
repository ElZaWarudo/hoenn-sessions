//! Storage and infrastructure boundaries for authenticated Phase 2.

use argon2::{
    Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use coop_cloud::{
    CharacterCloudState, CharacterId, ClientInstanceId, CommitId, IdempotencyKey, LeaseContract,
    RefreshFamilyId, Revision, SessionId, SigningPrivateKey, SnapshotFile, SnapshotFinalizeRequest,
    SnapshotId, SnapshotPrepareRequest, SnapshotRecord, SnapshotRestoreRequest,
    UnixTimestampMillis, UploadTarget, UserId,
};
use coop_protocol::{RegionId, RegionalProgress, WorldZone};
use getrandom::fill as random_fill;
use hmac::Mac;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const ACCESS_TTL_MS: u64 = 15 * 60 * 1_000;
pub const REFRESH_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
pub const HEARTBEAT_INTERVAL_MS: u32 = 10_000;
pub const LEASE_TTL_MS: u64 = 30_000;
pub const RECONNECT_GRACE_MS: u64 = 90_000;
pub const UPLOAD_TTL_MS: u64 = 5 * 60 * 1_000;
pub const MAX_CHARACTER_SAV: u64 = 1_048_576;
pub const MAX_PENDING_COMMITS: u64 = 1_048_576;
pub const MAX_RESUME_SS1: u64 = 32 * 1_048_576;
pub const MAX_RESUME_RESPONSE: u64 = 40 * 1_048_576;
pub const MAX_PREPARED_SNAPSHOTS: usize = 128;
pub const MAX_UPLOAD_TICKETS: usize = 384;
pub const MAX_SNAPSHOTS_PER_CHARACTER: usize = 100;
pub const MAX_SNAPSHOT_STORAGE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_RELEASE_KEYS: usize = 256;
pub const ACQUIRE_IDEMPOTENCY_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
pub const MAX_ACQUIRE_HISTORY: usize = 256;
pub const RESTORE_STAGE_TTL_MS: u64 = 5 * 60 * 1_000;
pub const MAX_ACCESS_RECORDS_PER_CHARACTER: usize = 1_024;
pub const MAX_REFRESH_RECORDS_PER_CHARACTER: usize = 1_024;
pub const MAX_FAMILY_RECORDS_PER_CHARACTER: usize = 128;
pub const MAX_ACCESS_RECORDS_GLOBAL: usize = 16_384;
pub const MAX_REFRESH_RECORDS_GLOBAL: usize = 16_384;
pub const MAX_FAMILY_RECORDS_GLOBAL: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageMode {
    Phase2Local,
    PostgresFirebase,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StorageError {
    #[error("entropy source unavailable")]
    Entropy,
    #[error("clock value is outside the supported range")]
    Clock,
    #[error("production adapters are unavailable")]
    ProductionUnavailable,
    #[error("state lock unavailable")]
    Lock,
    #[error("invalid local adapter configuration")]
    InvalidConfiguration,
    #[error("password engine unavailable")]
    Password,
    #[error("repository transaction failed")]
    Transaction,
}

/// Password hashing and verification boundary. Implementations must use a
/// memory-hard password hash and should be configured with a test-only policy
/// when deterministic tests need a lower-cost operation.
pub trait PasswordEngine: Send + Sync {
    /// Hashes a password into a PHC string.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured password engine cannot hash.
    fn hash(&self, password: &str) -> Result<String, StorageError>;
    /// Verifies a password against a PHC string without exposing parse errors.
    fn verify(&self, password: &str, phc: &str) -> bool;
    /// Returns a PHC dummy used to equalize unknown-user login work.
    fn dummy_phc(&self) -> &str;
}

/// Argon2id password engine used by local mode and injectable in tests.
pub struct ArgonPasswordEngine {
    params: Params,
    dummy_phc: String,
}

impl ArgonPasswordEngine {
    /// Creates an Argon2id engine with an explicit resource policy.
    ///
    /// # Errors
    ///
    /// Returns an error when Argon2 rejects the requested parameters.
    pub fn new(memory_kib: u32, iterations: u32, lanes: u32) -> Result<Self, StorageError> {
        let params =
            Params::new(memory_kib, iterations, lanes, None).map_err(|_| StorageError::Password)?;
        let engine = Self {
            params,
            dummy_phc: String::new(),
        };
        let dummy_phc = engine.hash_with_salt("dummy password")?;
        Ok(Self {
            dummy_phc,
            ..engine
        })
    }

    /// Creates the production policy (19,456 KiB, two iterations, one lane).
    ///
    /// # Panics
    ///
    /// Panics only if the compile-time production Argon2 parameters are
    /// rejected by the Argon2 library.
    #[must_use]
    pub fn production() -> Self {
        let params = Params::new(19_456, 2, 1, None).expect("production Argon2 policy");
        Self {
            params,
            dummy_phc: "$argon2id$v=19$m=19456,t=2,p=1$sUqdATt6cZlDUSAFETIqxg$6wHrJVz2ilL92GTlXsq86hF4iMX551k2AFNf004LDvY".to_owned(),
        }
    }

    fn argon(&self) -> Argon2<'static> {
        Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            self.params.clone(),
        )
    }

    fn hash_with_salt(&self, password: &str) -> Result<String, StorageError> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| StorageError::Password)
    }
}

impl PasswordEngine for ArgonPasswordEngine {
    fn hash(&self, password: &str) -> Result<String, StorageError> {
        self.hash_with_salt(password)
    }

    fn verify(&self, password: &str, phc: &str) -> bool {
        let Ok(hash) = PasswordHash::new(phc) else {
            return false;
        };
        self.argon()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    }

    fn dummy_phc(&self) -> &str {
        &self.dummy_phc
    }
}

pub trait Clock: Send + Sync {
    /// Returns server-owned Unix milliseconds.
    fn now_ms(&self) -> u64;
}
pub trait Entropy: Send + Sync {
    /// Fills a buffer with cryptographically suitable bytes.
    ///
    /// # Errors
    ///
    /// Returns an infrastructure error when entropy cannot be obtained.
    fn fill(&self, bytes: &mut [u8]) -> Result<(), StorageError>;
}

/// Atomic state repository boundary used by the Phase 2 service.
///
/// The in-memory implementation is the only available adapter in this
/// milestone. A persistent adapter must provide the same whole-state
/// transaction boundary before it can be enabled.
pub trait Repository: Send + Sync {
    /// Runs a short read-only repository operation atomically. Adapters with
    /// a real transaction engine override this method; the local adapter
    /// provides the same boundary with its process-local lock.
    ///
    /// # Errors
    ///
    /// Returns an adapter error when the transaction cannot be opened or the
    /// operation fails.
    fn read_transaction(
        &self,
        operation: &mut dyn FnMut(&State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError>;

    /// Runs a short state transition atomically. Object-store work is not
    /// permitted in the callback.
    ///
    /// # Errors
    ///
    /// Returns an adapter error when the transaction cannot be opened or the
    /// operation fails.
    fn write_transaction(
        &self,
        operation: &mut dyn FnMut(&mut State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError>;
}

/// Immutable-artifact object-store boundary used by snapshot operations.
pub trait ObjectStore: Send + Sync {
    /// Reads an object by its server-derived key.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Lock`] when the adapter cannot access its state.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    /// Writes an object under a server-derived key.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Lock`] when the adapter cannot access its state.
    fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError>;
    /// Atomically creates an object if the key is absent. Implementations
    /// backed by a remote store must map this to a conditional create. The
    /// default fails closed because a contains-then-put sequence is not an
    /// atomic create under concurrency.
    ///
    /// # Errors
    ///
    /// Returns an adapter error when the existence check or write fails.
    fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
        let _ = (key, bytes);
        Err(StorageError::Transaction)
    }
    /// Deletes an object during failed staged publication. Implementations
    /// backed by a remote store must make this operation idempotent.
    ///
    /// # Errors
    ///
    /// Returns an adapter error when the object cannot be removed.
    fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
        let _ = key;
        Err(StorageError::Transaction)
    }
    /// Checks whether an object exists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Lock`] when the adapter cannot access its state.
    fn contains(&self, key: &str) -> Result<bool, StorageError>;
}

/// Process-local repository adapter for deterministic tests and local mode.
#[derive(Clone, Default)]
pub struct InMemoryRepository {
    state: Arc<RwLock<State>>,
}

impl InMemoryRepository {
    /// Creates an empty in-memory repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Repository for InMemoryRepository {
    fn read_transaction(
        &self,
        operation: &mut dyn FnMut(&State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        let state = self.state.read().map_err(|_| StorageError::Lock)?;
        operation(&state)
    }

    fn write_transaction(
        &self,
        operation: &mut dyn FnMut(&mut State) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        let mut state = self.state.write().map_err(|_| StorageError::Lock)?;
        operation(&mut state)
    }
}

/// Process-local object-store adapter for fixed snapshot artifacts.
#[derive(Clone, Default)]
pub struct InMemoryObjectStore {
    objects: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl InMemoryObjectStore {
    /// Creates an empty in-memory object store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn object_count(&self) -> Result<usize, StorageError> {
        Ok(self.objects.read().map_err(|_| StorageError::Lock)?.len())
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.objects
            .read()
            .map_err(|_| StorageError::Lock)
            .map(|objects| objects.get(key).cloned())
    }

    fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError> {
        self.objects
            .write()
            .map_err(|_| StorageError::Lock)?
            .insert(key, bytes);
        Ok(())
    }

    fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
        let mut objects = self.objects.write().map_err(|_| StorageError::Lock)?;
        if objects.contains_key(&key) {
            return Ok(false);
        }
        objects.insert(key, bytes);
        Ok(true)
    }

    fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self
            .objects
            .write()
            .map_err(|_| StorageError::Lock)?
            .remove(key)
            .is_some())
    }

    fn contains(&self, key: &str) -> Result<bool, StorageError> {
        self.objects
            .read()
            .map_err(|_| StorageError::Lock)
            .map(|objects| objects.contains_key(key))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;
static LAST_SYSTEM_CLOCK_MS: AtomicU64 = AtomicU64::new(0);
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        let observed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(u64::MAX, |d| d.as_millis().try_into().unwrap_or(u64::MAX));
        let mut previous = LAST_SYSTEM_CLOCK_MS.load(Ordering::Acquire);
        loop {
            if observed <= previous {
                return previous;
            }
            match LAST_SYSTEM_CLOCK_MS.compare_exchange_weak(
                previous,
                observed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return observed,
                Err(current) => previous = current,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct OsEntropy;
impl Entropy for OsEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), StorageError> {
        random_fill(bytes).map_err(|_| StorageError::Entropy)
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct FixedClock(std::sync::atomic::AtomicU64);
#[cfg(test)]
impl FixedClock {
    /// Creates a clock at a fixed Unix millisecond value.
    #[must_use]
    pub fn new(ms: u64) -> Self {
        Self(std::sync::atomic::AtomicU64::new(ms))
    }
    /// Replaces the current time.
    pub fn set(&self, ms: u64) {
        self.0.store(ms, std::sync::atomic::Ordering::SeqCst);
    }
    /// Advances the current time.
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }
}
#[cfg(test)]
impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct FixedEntropy {
    bytes: Vec<u8>,
    cursor: std::sync::Mutex<usize>,
}
#[cfg(test)]
impl FixedEntropy {
    /// Creates a repeating deterministic byte source for tests.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            cursor: std::sync::Mutex::new(0),
        }
    }
}
#[cfg(test)]
impl Entropy for FixedEntropy {
    fn fill(&self, output: &mut [u8]) -> Result<(), StorageError> {
        let mut cursor = self.cursor.lock().map_err(|_| StorageError::Lock)?;
        if self.bytes.is_empty() {
            return Err(StorageError::Entropy);
        }
        for byte in output {
            *byte = self.bytes[*cursor % self.bytes.len()];
            *cursor = cursor.saturating_add(1);
        }
        Ok(())
    }
}

/// Explicit server configuration for the Phase 2 adapters.
#[derive(Clone)]
pub struct Phase2Config {
    pub(crate) mode: StorageMode,
    pub(crate) invite_pepper: Zeroizing<Vec<u8>>,
    pub(crate) signing_key: SigningPrivateKey,
    pub(crate) signing_key_id: String,
    pub(crate) upload_base_url: String,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) entropy: Arc<dyn Entropy>,
    pub(crate) password_engine: Arc<dyn PasswordEngine>,
    pub(crate) repository: Option<Arc<dyn Repository>>,
    pub(crate) object_store: Option<Arc<dyn ObjectStore>>,
    pub(crate) production: Option<ProductionConfig>,
}

/// Validated connection identifiers required by the production adapters.
/// The adapters are intentionally not enabled until their implementations are
/// available, but configuration must still be validated before startup.
#[derive(Clone, Eq, PartialEq)]
pub struct ProductionConfig {
    pub database_url: String,
    pub firebase_bucket: String,
}

impl fmt::Debug for ProductionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProductionConfig([REDACTED])")
    }
}

impl ProductionConfig {
    /// Creates validated production connection identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidConfiguration`] for empty or oversized
    /// connection identifiers.
    pub fn new(
        database_url: impl Into<String>,
        firebase_bucket: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let database_url = database_url.into();
        let firebase_bucket = firebase_bucket.into();
        let config = Self {
            database_url,
            firebase_bucket,
        };
        config.validate()
    }

    fn validate(self) -> Result<Self, StorageError> {
        let database =
            url::Url::parse(&self.database_url).map_err(|_| StorageError::InvalidConfiguration)?;
        let valid_database_scheme = matches!(database.scheme(), "postgres" | "postgresql");
        let valid_bucket = !self.firebase_bucket.is_empty()
            && self.firebase_bucket.len() <= 512
            && self
                .firebase_bucket
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
        if !valid_database_scheme
            || database.host_str().is_none_or(str::is_empty)
            || self.database_url.len() > 4096
            || !valid_bucket
        {
            return Err(StorageError::InvalidConfiguration);
        }
        Ok(self)
    }
}

/// Explicit marker for a PostgreSQL-backed repository adapter.
pub trait PostgresRepository: Repository {}
/// Explicit marker for a Firebase-backed immutable object-store adapter.
pub trait FirebaseObjectStore: ObjectStore {}
impl Phase2Config {
    /// Creates a loopback-only local configuration with supplied secrets.
    ///
    /// # Errors
    ///
    /// Returns an error when the pepper is too short.
    pub fn local(
        pepper: impl Into<Vec<u8>>,
        signing_key: SigningPrivateKey,
        key_id: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let pepper = pepper.into();
        let signing_key_id = key_id.into();
        if pepper.len() < 16 {
            return Err(StorageError::Entropy);
        }
        if !valid_key_id(&signing_key_id) {
            return Err(StorageError::InvalidConfiguration);
        }
        Ok(Self {
            mode: StorageMode::Phase2Local,
            invite_pepper: Zeroizing::new(pepper),
            signing_key,
            signing_key_id,
            upload_base_url: "http://127.0.0.1".to_owned(),
            clock: Arc::new(SystemClock),
            entropy: Arc::new(OsEntropy),
            password_engine: Arc::new(ArgonPasswordEngine::production()),
            repository: None,
            object_store: None,
            production: None,
        })
    }
    /// Selects the not-yet-implemented PostgreSQL/Firebase adapters.
    #[must_use]
    pub fn postgres_firebase() -> Self {
        Self {
            mode: StorageMode::PostgresFirebase,
            invite_pepper: Zeroizing::new(Vec::new()),
            signing_key: SigningPrivateKey::from_bytes([0; 32]),
            signing_key_id: String::new(),
            upload_base_url: String::new(),
            clock: Arc::new(SystemClock),
            entropy: Arc::new(OsEntropy),
            password_engine: Arc::new(ArgonPasswordEngine::production()),
            repository: None,
            object_store: None,
            production: None,
        }
    }
    /// Selects production mode with validated adapter connection settings.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidConfiguration`] when the supplied
    /// production settings are not valid.
    pub fn postgres_firebase_with_config(
        production: ProductionConfig,
    ) -> Result<Self, StorageError> {
        let production = production.validate()?;
        let mut config = Self::postgres_firebase();
        config.production = Some(production);
        Ok(config)
    }
    /// Replaces infrastructure with explicitly injected test adapters.
    #[cfg(test)]
    #[must_use]
    pub fn with_test_adapters(mut self, clock: Arc<dyn Clock>, entropy: Arc<dyn Entropy>) -> Self {
        self.clock = clock;
        self.entropy = entropy;
        self
    }
    /// Replaces password hashing with an explicitly injected test engine.
    #[cfg(test)]
    #[must_use]
    pub fn with_password_engine(mut self, engine: Arc<dyn PasswordEngine>) -> Self {
        self.password_engine = engine;
        self
    }
    /// Replaces the local state and artifact adapters with explicit
    /// capability-oriented implementations.
    #[cfg(test)]
    #[must_use]
    pub fn with_adapters(
        mut self,
        repository: Arc<dyn Repository>,
        object_store: Arc<dyn ObjectStore>,
    ) -> Self {
        self.repository = Some(repository);
        self.object_store = Some(object_store);
        self
    }
    /// Sets the local upload URL prefix.
    #[must_use]
    pub fn with_upload_base_url(mut self, url: impl Into<String>) -> Self {
        self.upload_base_url = url.into();
        self
    }
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[derive(Clone)]
pub(crate) struct UserRecord {
    pub user_id: UserId,
    pub password_phc: String,
    pub character_id: CharacterId,
    pub disabled: bool,
}
#[derive(Clone)]
pub(crate) struct CharacterRecord {
    pub owner: UserId,
    pub state: CharacterCloudState,
    pub revision: Revision,
    pub active_snapshot: Option<SnapshotId>,
    pub last_session_epoch: u32,
}
#[derive(Clone)]
pub(crate) struct AccessRecord {
    pub user_id: UserId,
    pub character_id: CharacterId,
    pub family_id: RefreshFamilyId,
    pub expires_at: u64,
    pub revoked: bool,
}
#[derive(Clone)]
pub(crate) struct RefreshRecord {
    pub user_id: UserId,
    pub character_id: CharacterId,
    pub family_id: RefreshFamilyId,
    pub generation: u32,
    pub expires_at: u64,
    pub consumed: bool,
}
#[derive(Clone)]
pub(crate) struct FamilyRecord {
    pub user_id: UserId,
    pub character_id: CharacterId,
    pub expires_at: u64,
    pub revoked: bool,
}
#[derive(Clone)]
pub(crate) struct LeaseRecord {
    pub contract: LeaseContract,
    pub grace_until: u64,
    pub released: bool,
    pub reconnect: Option<(IdempotencyKey, coop_cloud::LeaseFence, LeaseContract)>,
    pub release_keys: Vec<(IdempotencyKey, coop_cloud::LeaseFence)>,
}
#[derive(Clone)]
pub(crate) struct PreparedSnapshot {
    pub request: SnapshotPrepareRequest,
    pub upload_targets: Vec<UploadTarget>,
    pub expires_at: u64,
}
#[derive(Clone)]
pub(crate) struct RestoreStage {
    pub request: SnapshotRestoreRequest,
    pub snapshot_id: SnapshotId,
    pub expires_at: u64,
}
#[derive(Clone)]
pub(crate) struct TicketRecord {
    pub actor: UserId,
    pub character_id: CharacterId,
    pub snapshot_id: SnapshotId,
    pub artifact: coop_cloud::ArtifactIdentity,
    pub method: coop_cloud::UploadMethod,
    pub expected: SnapshotFile,
    pub expires_at: u64,
    pub used: bool,
}

#[derive(Clone)]
pub(crate) struct AcquireRecord {
    pub character_id: CharacterId,
    pub client_instance_id: ClientInstanceId,
    pub contract: LeaseContract,
    pub expires_at: u64,
}

#[derive(Default)]
pub struct State {
    pub(crate) users_by_name: HashMap<String, UserRecord>,
    pub(crate) users_by_id: HashMap<UserId, UserRecord>,
    pub(crate) characters: HashMap<CharacterId, CharacterRecord>,
    pub(crate) invitations: HashMap<[u8; 32], bool>,
    pub(crate) access: HashMap<[u8; 32], AccessRecord>,
    pub(crate) refresh: HashMap<[u8; 32], RefreshRecord>,
    pub(crate) families: HashMap<RefreshFamilyId, FamilyRecord>,
    pub(crate) leases: HashMap<CharacterId, LeaseRecord>,
    pub(crate) acquire_history: HashMap<IdempotencyKey, AcquireRecord>,
    pub(crate) prepared: HashMap<SnapshotId, PreparedSnapshot>,
    pub(crate) prepare_ops: HashMap<(CharacterId, IdempotencyKey), SnapshotId>,
    pub(crate) snapshots: HashMap<SnapshotId, SnapshotRecord>,
    pub(crate) snapshot_by_revision: HashMap<(CharacterId, Revision), SnapshotId>,
    pub(crate) finalize_ops:
        HashMap<(CharacterId, IdempotencyKey), (SnapshotFinalizeRequest, SnapshotRecord)>,
    pub(crate) restore_ops:
        HashMap<(CharacterId, IdempotencyKey), (SnapshotRestoreRequest, SnapshotRecord)>,
    pub(crate) restore_staging: HashMap<CharacterId, RestoreStage>,
    pub(crate) retired_snapshots: HashSet<SnapshotId>,
    pub(crate) tickets: HashMap<[u8; 32], TicketRecord>,
}
#[derive(Clone)]
pub(crate) struct Store {
    pub(crate) repository: Arc<dyn Repository>,
    pub config: Arc<Phase2Config>,
    pub(crate) clock_floor: Arc<AtomicU64>,
    pub(crate) objects: Arc<dyn ObjectStore>,
    account_inflight: Arc<std::sync::Mutex<HashSet<String>>>,
}
impl Store {
    pub fn new(config: Phase2Config) -> Result<Self, StorageError> {
        if config.mode != StorageMode::Phase2Local {
            return Err(StorageError::ProductionUnavailable);
        }
        let upload_url = url::Url::parse(&config.upload_base_url)
            .map_err(|_| StorageError::InvalidConfiguration)?;
        let loopback = match upload_url.host() {
            Some(url::Host::Ipv4(address)) => address == std::net::Ipv4Addr::LOCALHOST,
            Some(url::Host::Ipv6(address)) => address == std::net::Ipv6Addr::LOCALHOST,
            None | Some(url::Host::Domain(_)) => false,
        };
        if upload_url.scheme() != "http"
            || !loopback
            || !upload_url.username().is_empty()
            || upload_url.password().is_some()
            || upload_url.query().is_some()
            || upload_url.fragment().is_some()
            || (!upload_url.path().is_empty() && upload_url.path() != "/")
        {
            return Err(StorageError::InvalidConfiguration);
        }
        let repository: Arc<dyn Repository> = config
            .repository
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryRepository::new()));
        let object_store: Arc<dyn ObjectStore> = config
            .object_store
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryObjectStore::new()));
        Ok(Self {
            repository,
            config: Arc::new(config),
            clock_floor: Arc::new(AtomicU64::new(0)),
            objects: object_store,
            account_inflight: Arc::new(std::sync::Mutex::new(HashSet::new())),
        })
    }
    pub(crate) fn now(&self) -> u64 {
        let observed = self.config.clock.now_ms();
        let mut previous = self.clock_floor.load(Ordering::Acquire);
        loop {
            if observed <= previous {
                return previous;
            }
            match self.clock_floor.compare_exchange_weak(
                previous,
                observed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return observed,
                Err(current) => previous = current,
            }
        }
    }

    /// Executes a short read-only repository operation under the adapter's
    /// transaction boundary. Callers must copy all data needed before doing
    /// object-store or other blocking work.
    pub(crate) fn read_transaction<T, E, F>(&self, operation: F) -> Result<T, E>
    where
        E: From<StorageError>,
        F: FnOnce(&State) -> Result<T, E>,
    {
        let mut operation = Some(operation);
        let mut result = None;
        let mut failure = None;
        let mut callback = |state: &State| {
            let Some(operation) = operation.take() else {
                return Err(StorageError::Transaction);
            };
            match operation(state) {
                Ok(value) => {
                    result = Some(value);
                    Ok(())
                }
                Err(error) => {
                    failure = Some(error);
                    Err(StorageError::Transaction)
                }
            }
        };
        self.repository
            .read_transaction(&mut callback)
            .map_err(|error| failure.take().unwrap_or_else(|| E::from(error)))?;
        result.ok_or_else(|| E::from(StorageError::Transaction))
    }

    /// Executes a short atomic repository transition. Object-store calls must
    /// happen outside this callback; this is the semantic write boundary used
    /// by snapshot/session transitions.
    pub(crate) fn write_transaction<T, E, F>(&self, operation: F) -> Result<T, E>
    where
        E: From<StorageError>,
        F: FnOnce(&mut State) -> Result<T, E>,
    {
        let mut operation = Some(operation);
        let mut result = None;
        let mut failure = None;
        let mut callback = |state: &mut State| {
            let Some(operation) = operation.take() else {
                return Err(StorageError::Transaction);
            };
            match operation(state) {
                Ok(value) => {
                    result = Some(value);
                    Ok(())
                }
                Err(error) => {
                    failure = Some(error);
                    Err(StorageError::Transaction)
                }
            }
        };
        self.repository
            .write_transaction(&mut callback)
            .map_err(|error| failure.take().unwrap_or_else(|| E::from(error)))?;
        result.ok_or_else(|| E::from(StorageError::Transaction))
    }

    #[cfg(test)]
    pub(crate) fn inspect_state<T, F>(&self, operation: F) -> Result<T, StorageError>
    where
        F: FnOnce(&State) -> T,
    {
        self.read_transaction(|state| Ok(operation(state)))
    }

    pub(crate) fn try_account_admission(
        &self,
        username: &str,
    ) -> Result<Option<AccountAdmission>, StorageError> {
        let mut active = self
            .account_inflight
            .lock()
            .map_err(|_| StorageError::Lock)?;
        if !active.insert(username.to_owned()) {
            return Ok(None);
        }
        Ok(Some(AccountAdmission {
            active: self.account_inflight.clone(),
            username: username.to_owned(),
        }))
    }
    pub(crate) fn invitation_fingerprint(&self, code: &str) -> [u8; 32] {
        let mut mac = hmac::Hmac::<Sha256>::new_from_slice(&self.config.invite_pepper)
            .expect("HMAC accepts every key");
        mac.update(code.as_bytes());
        mac.finalize().into_bytes().into()
    }
    pub(crate) fn token_fingerprint(token: &str) -> [u8; 32] {
        Sha256::digest(token.as_bytes()).into()
    }
    pub(crate) fn random_token(&self) -> Result<String, StorageError> {
        let mut bytes = Zeroizing::new([0_u8; 32]);
        self.config.entropy.fill(&mut bytes[..])?;
        Ok(URL_SAFE_NO_PAD.encode(bytes.as_slice()))
    }
    pub(crate) fn random_uuid(&self) -> Result<Uuid, StorageError> {
        let mut bytes = [0_u8; 16];
        self.config.entropy.fill(&mut bytes)?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Ok(Uuid::from_bytes(bytes))
    }
    pub(crate) fn user_id(&self) -> Result<UserId, StorageError> {
        UserId::new(self.random_uuid()?).map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn character_id(&self) -> Result<CharacterId, StorageError> {
        CharacterId::new(self.random_uuid()?).map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn family_id(&self) -> Result<RefreshFamilyId, StorageError> {
        RefreshFamilyId::new(self.random_uuid()?).map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn session_id(&self) -> Result<SessionId, StorageError> {
        SessionId::new(self.random_uuid()?).map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn snapshot_id(&self) -> Result<SnapshotId, StorageError> {
        SnapshotId::new(self.random_uuid()?).map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn initial_state(
        character_id: CharacterId,
    ) -> Result<CharacterCloudState, StorageError> {
        let progress = RegionalProgress::new(RegionId::Hoenn, 0, 0, vec![], vec![])
            .map_err(|_| StorageError::Entropy)?;
        CharacterCloudState::new(
            character_id,
            WorldZone::new(RegionId::Hoenn, "LITTLEROOT_TOWN", 1)
                .map_err(|_| StorageError::Entropy)?,
            vec![progress],
        )
        .map_err(|_| StorageError::Entropy)
    }
    pub(crate) fn object_key(
        character: CharacterId,
        snapshot: SnapshotId,
        artifact: coop_cloud::ArtifactIdentity,
    ) -> String {
        format!(
            "characters/{character}/snapshots/{snapshot}/{}",
            artifact.as_str()
        )
    }
    pub(crate) fn unix_timestamp(value: u64) -> Result<UnixTimestampMillis, StorageError> {
        if value == 0 {
            return Err(StorageError::Clock);
        }
        Ok(UnixTimestampMillis::new(value))
    }
}

pub(crate) struct AccountAdmission {
    active: Arc<std::sync::Mutex<HashSet<String>>>,
    username: String,
}

impl Drop for AccountAdmission {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.username);
        }
    }
}
pub(crate) fn is_artifact_size_allowed(artifact: coop_cloud::ArtifactIdentity, size: u64) -> bool {
    match artifact {
        coop_cloud::ArtifactIdentity::CharacterSav => size > 0 && size <= MAX_CHARACTER_SAV,
        coop_cloud::ArtifactIdentity::PendingCommits => size <= MAX_PENDING_COMMITS,
        coop_cloud::ArtifactIdentity::ResumeSs1 => size > 0 && size <= MAX_RESUME_SS1,
    }
}
pub(crate) fn commit_id_allowed(commit: Option<CommitId>) -> bool {
    commit.is_none()
}

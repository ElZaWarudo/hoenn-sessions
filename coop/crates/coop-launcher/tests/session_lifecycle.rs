use std::sync::{Arc, Mutex};

use coop_cloud::{
    AccessToken, CharacterId, LoginResponse, RefreshFamilyId, RefreshResponse, RefreshToken,
    SessionEpoch, SessionId, UnixTimestampMillis, UserId,
};
use coop_launcher::{
    AuthApi, AuthError, CommandSpec, EpochError, EpochStore, OsKeychain, RefreshTokenStore,
    SessionWorkspace,
    auth::AuthFuture,
    keychain::{KEYCHAIN_SERVICE, KeychainError},
};
use tempfile::tempdir;
use uuid::Uuid;

fn character() -> CharacterId {
    CharacterId::new(Uuid::from_u128(1)).unwrap()
}
fn session() -> SessionId {
    SessionId::new(Uuid::from_u128(2)).unwrap()
}

#[test]
fn endpoint_arguments_have_no_shell_or_script_injection_surface() {
    let sidecar = CommandSpec::sidecar("sidecar", 9).unwrap();
    assert_eq!(sidecar.args, ["--session-epoch", "9"]);
    let mgba = CommandSpec::mgba("mgba", r"C:\roms\safe.gba").unwrap();
    assert_eq!(mgba.args, [r"C:\roms\safe.gba"]);
    assert!(CommandSpec::sidecar("sidecar", 0).is_err());
    assert!(CommandSpec::mgba("mgba", "bad\0path").is_err());
    assert_eq!(format!("{OsKeychain:?}"), "OsKeychain");
}

#[test]
fn epoch_store_is_atomic_monotonic_and_fail_closed() {
    let directory = tempdir().unwrap();
    let store = EpochStore::new(directory.path().join("state/epoch.json"));
    let first = store
        .accept(character(), session(), SessionEpoch::new(1).unwrap())
        .unwrap();
    assert_eq!(first.greatest_epoch, 1);
    assert!(matches!(
        store.accept(character(), session(), SessionEpoch::new(1).unwrap()),
        Err(EpochError::Stale)
    ));
    assert!(matches!(
        store.accept(
            character(),
            SessionId::new(Uuid::from_u128(3)).unwrap(),
            SessionEpoch::new(1).unwrap()
        ),
        Err(EpochError::Stale)
    ));
    assert_eq!(
        store
            .accept(
                character(),
                SessionId::new(Uuid::from_u128(3)).unwrap(),
                SessionEpoch::new(2).unwrap()
            )
            .unwrap()
            .greatest_epoch,
        2
    );
    let bytes = std::fs::read(store.path()).unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_ok());
}

#[test]
fn epoch_store_rejects_corruption_without_resetting_or_rewriting_state() {
    let directory = tempdir().unwrap();
    let store = EpochStore::new(directory.path().join("state/epoch.json"));
    store
        .accept(character(), session(), SessionEpoch::new(4).unwrap())
        .unwrap();
    std::fs::write(store.path(), b"not-json").unwrap();
    let before = std::fs::read(store.path()).unwrap();
    assert!(matches!(
        store.read(character(), session()),
        Err(EpochError::Corrupt)
    ));
    assert!(matches!(
        store.accept(character(), session(), SessionEpoch::new(5).unwrap()),
        Err(EpochError::Corrupt)
    ));
    assert_eq!(std::fs::read(store.path()).unwrap(), before);
}

#[test]
fn epoch_store_rejects_exhaustion_without_rewriting_accepted_state() {
    let directory = tempdir().unwrap();
    let store = EpochStore::new(directory.path().join("state/epoch.json"));
    store
        .accept(character(), session(), SessionEpoch::new(4).unwrap())
        .unwrap();
    let before = std::fs::read(store.path()).unwrap();
    assert!(matches!(
        store.accept(character(), session(), SessionEpoch::new(u32::MAX).unwrap()),
        Err(EpochError::Exhausted)
    ));
    assert_eq!(std::fs::read(store.path()).unwrap(), before);
    assert_eq!(
        store
            .read(character(), session())
            .unwrap()
            .unwrap()
            .greatest_epoch,
        4
    );
}

#[test]
fn epoch_store_recovers_stale_lock_and_bounds_repeated_persistence() {
    let directory = tempdir().unwrap();
    let store = EpochStore::new(directory.path().join("state/epoch.json"));
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    let mut lock_path = store.path().as_os_str().to_os_string();
    lock_path.push(".lock");
    std::fs::write(&lock_path, b"created_millis=0\n").unwrap();
    for epoch in 1..=70 {
        store
            .accept(character(), session(), SessionEpoch::new(epoch).unwrap())
            .unwrap();
    }
    assert_eq!(
        store
            .read(character(), session())
            .unwrap()
            .unwrap()
            .greatest_epoch,
        70
    );
    let versioned = std::fs::read_dir(store.path().parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".epoch-"))
        .count();
    assert!(versioned <= 64);
}

#[test]
fn reconnect_accepts_exact_persisted_replay_and_server_rotation() {
    let directory = tempdir().unwrap();
    let store = EpochStore::new(directory.path().join("epoch.json"));
    let character = character();
    let session = session();
    store
        .accept(character, session, SessionEpoch::new(4).unwrap())
        .unwrap();

    let replay = store
        .accept_reconnect(
            character,
            session,
            SessionEpoch::new(4).unwrap(),
            SessionEpoch::new(4).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(replay.greatest_epoch, 4);

    let rotated = store
        .accept_reconnect(
            character,
            session,
            SessionEpoch::new(4).unwrap(),
            SessionEpoch::new(5).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(rotated.greatest_epoch, 5);
    assert!(matches!(
        store.accept_reconnect(
            character,
            session,
            SessionEpoch::new(4).unwrap(),
            SessionEpoch::new(6).unwrap(),
            true,
        ),
        Err(EpochError::Stale)
    ));
}

#[cfg(unix)]
#[test]
fn epoch_kernel_lock_serializes_competing_accepts_without_reclaiming_path() {
    let directory = tempdir().unwrap();
    let store = Arc::new(EpochStore::new(directory.path().join("epoch.json")));
    let character = character();
    let session = session();
    store
        .accept(character, session, SessionEpoch::new(1).unwrap())
        .unwrap();
    let lock_path = store.path().with_file_name("epoch.json.lock");
    assert!(lock_path.exists());
    let mut workers = Vec::new();
    for _ in 0..8 {
        let store = Arc::clone(&store);
        workers.push(std::thread::spawn(move || {
            store.accept(character, session, SessionEpoch::new(2).unwrap())
        }));
    }
    let successes = workers
        .into_iter()
        .filter_map(|worker| worker.join().unwrap().ok())
        .count();
    assert_eq!(successes, 1);
    assert!(lock_path.exists());
}

#[test]
fn workspace_writes_only_fixed_names_and_pending_bootstrap_is_exact() {
    let directory = tempdir().unwrap();
    let workspace = SessionWorkspace::create(directory.path()).unwrap();
    workspace
        .write_atomic("pending_commits.json", b"[]")
        .unwrap();
    assert_eq!(
        std::fs::read(workspace.path().join("pending_commits.json")).unwrap(),
        b"[]"
    );
    #[cfg(windows)]
    assert!(
        workspace
            .write_atomic("pending_commits.json", b"[1]")
            .is_err()
    );
    #[cfg(windows)]
    assert_eq!(
        std::fs::read(workspace.path().join("pending_commits.json")).unwrap(),
        b"[]"
    );
    #[cfg(not(windows))]
    {
        workspace
            .write_atomic("pending_commits.json", b"[1]")
            .unwrap();
        assert_eq!(
            std::fs::read(workspace.path().join("pending_commits.json")).unwrap(),
            b"[1]"
        );
    }
    assert!(workspace.write_atomic("../../escape", b"bad").is_err());
    std::fs::create_dir(workspace.path().join("character.sav")).unwrap();
    assert!(
        workspace
            .write_atomic("character.sav", b"must-not-follow")
            .is_err()
    );
    std::fs::remove_dir(workspace.path().join("character.sav")).unwrap();
    workspace
        .write_session_lua("127.0.0.1", 1234, "0123456789abcdef0123456789abcdef")
        .unwrap();
    let lua = std::fs::read_to_string(workspace.path().join("session.lua")).unwrap();
    assert!(lua.contains("127.0.0.1"));
    assert!(!lua.contains("control"));
}

#[test]
fn authorized_recovery_preserves_only_sav_and_nonsecret_marker() {
    let directory = tempdir().unwrap();
    let mut workspace = SessionWorkspace::create(directory.path()).unwrap();
    workspace
        .write_atomic("character.sav", b"stable-save")
        .unwrap();
    for (name, value) in [
        ("pending_commits.json", b"[]".as_slice()),
        ("resume.ss1", b"state"),
        (
            "session.lua",
            b"return { secret = '0123456789abcdef0123456789abcdef' }",
        ),
        ("main.lua", b"bridge"),
        ("memory.lua", b"bridge"),
        ("protocol.lua", b"bridge"),
        ("generated_addresses.lua", b"addresses"),
    ] {
        workspace.write_atomic(name, value).unwrap();
    }
    let recovery_path = workspace.preserve_recovery().unwrap();
    assert_eq!(
        std::fs::read(recovery_path.join("character.sav")).unwrap(),
        b"stable-save"
    );
    assert_eq!(
        std::fs::read(recovery_path.join("recovery.marker")).unwrap(),
        b"coop-recovery-v1\n"
    );
    for name in [
        "pending_commits.json",
        "resume.ss1",
        "session.lua",
        "main.lua",
        "memory.lua",
        "protocol.lua",
        "generated_addresses.lua",
    ] {
        assert!(
            !recovery_path.join(name).exists(),
            "secret material remained: {name}"
        );
    }
    assert!(recovery_path.exists());
}

#[test]
fn production_keychain_fails_closed_and_test_keychain_contract_is_secret_scoped() {
    #[cfg(not(windows))]
    assert!(matches!(
        OsKeychain.load(KEYCHAIN_SERVICE, "ash"),
        Err(KeychainError::Unavailable) | Ok(None)
    ));
    #[cfg(windows)]
    assert_eq!(format!("{OsKeychain:?}"), "OsKeychain");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeKeychain {
        seen: Arc::clone(&seen),
    };
    let token = RefreshToken::new("refresh-token").unwrap();
    fake.store(KEYCHAIN_SERVICE, "ash", &token).unwrap();
    assert_eq!(fake.load(KEYCHAIN_SERVICE, "ash").unwrap(), None);
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        &[
            "pokecrossroads-coop-launcher:ash",
            "pokecrossroads-coop-launcher:ash"
        ]
    );
}

#[tokio::test]
async fn access_expiry_proactively_rotates_and_keychain_failure_keeps_old_pair() {
    let keychain = MemoryKeychain::default();
    let api = FakeAuthApi::with_refresh_responses(vec![
        refresh_response("final-access", "final-refresh"),
        refresh_response("new-access", "new-refresh"),
    ]);
    let login = LoginResponse::new(
        user(),
        character(),
        AccessToken::new("old-access").unwrap(),
        RefreshToken::new("old-refresh").unwrap(),
        family(),
        UnixTimestampMillis::new(200_000),
        UnixTimestampMillis::new(300_000),
    )
    .unwrap();
    let login_api = LoginPassthrough(login);
    let auth = coop_launcher::AuthSession::login(
        &login_api,
        &keychain,
        "ash",
        coop_launcher::AuthSession::password("password").unwrap(),
    )
    .await
    .unwrap();
    assert!(auth.should_refresh_at(180_001));

    let mut auth = auth;
    auth.refresh_at(&api, &keychain, 180_001).await.unwrap();
    assert_eq!(
        auth.access_token()
            .expect("refreshed session remains active")
            .expose_secret(),
        "new-access"
    );
    assert_eq!(
        keychain
            .load(KEYCHAIN_SERVICE, "ash")
            .unwrap()
            .unwrap()
            .expose_secret(),
        "new-refresh"
    );

    let failing = MemoryKeychain {
        token: Mutex::new(Some(RefreshToken::new("new-refresh").unwrap())),
        fail_store: true,
    };
    let mut auth = auth;
    let old_access = auth
        .access_token()
        .expect("session remains active before failed refresh")
        .expose_secret()
        .to_owned();
    let error = auth.refresh_at(&api, &failing, 180_001).await.unwrap_err();
    assert!(matches!(
        error,
        AuthError::Keychain(KeychainError::Operation)
    ));
    assert_eq!(
        auth.access_token()
            .expect("failed refresh preserves active access token")
            .expose_secret(),
        old_access
    );
    assert_eq!(
        auth.refresh_token()
            .expect("failed refresh preserves active refresh token")
            .expose_secret(),
        "new-refresh"
    );
    assert_eq!(
        api.refresh_calls.lock().unwrap().as_slice(),
        [
            RefreshToken::new("old-refresh").unwrap(),
            RefreshToken::new("new-refresh").unwrap()
        ]
    );
    assert_eq!(
        failing
            .load(KEYCHAIN_SERVICE, "ash")
            .unwrap()
            .unwrap()
            .expose_secret(),
        "new-refresh"
    );
    assert!(!format!("{auth:?}").contains("new-access"));
}

#[tokio::test]
async fn expired_refresh_is_rejected_before_network_call() {
    let keychain = MemoryKeychain::default();
    let api = FakeAuthApi::with_refresh_response(refresh_response("new-access", "new-refresh"));
    let login = LoginResponse::new(
        user(),
        character(),
        AccessToken::new("old-access").unwrap(),
        RefreshToken::new("old-refresh").unwrap(),
        family(),
        UnixTimestampMillis::new(200_000),
        UnixTimestampMillis::new(300_000),
    )
    .unwrap();
    let login_api = LoginPassthrough(login);
    let mut auth = coop_launcher::AuthSession::login(
        &login_api,
        &keychain,
        "ash",
        coop_launcher::AuthSession::password("password").unwrap(),
    )
    .await
    .unwrap();
    let result = auth.refresh_at(&api, &keychain, 300_000);
    assert!(matches!(result.await, Err(AuthError::RefreshExpired)));
    assert_eq!(api.refresh_calls.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn logout_revokes_session_and_deletes_rotating_refresh_token() {
    let login = LoginResponse::new(
        user(),
        character(),
        AccessToken::new("old-access").unwrap(),
        RefreshToken::new("old-refresh").unwrap(),
        family(),
        UnixTimestampMillis::new(200_000),
        UnixTimestampMillis::new(300_000),
    )
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let api = LogoutSpyApi::new(login, Arc::clone(&events));
    let keychain = LogoutSpyKeychain::new(Arc::clone(&events));
    let mut auth = coop_launcher::AuthSession::login(
        &api,
        &keychain,
        "ash",
        coop_launcher::AuthSession::password("password").unwrap(),
    )
    .await
    .unwrap();
    events.lock().unwrap().clear();
    auth.logout(&api, &keychain).await.unwrap();
    assert_eq!(
        api.revoked.lock().unwrap().as_slice(),
        [RefreshToken::new("old-refresh").unwrap()]
    );
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["delete".to_owned(), "revoke".to_owned()]
    );
    assert!(keychain.token.lock().unwrap().is_none());
    assert!(auth.access_token().is_none());
    assert!(auth.refresh_token().is_none());
}

#[tokio::test]
async fn logout_deletes_local_refresh_token_when_remote_revoke_fails() {
    let keychain = MemoryKeychain::default();
    let login = LoginResponse::new(
        user(),
        character(),
        AccessToken::new("old-access").unwrap(),
        RefreshToken::new("old-refresh").unwrap(),
        family(),
        UnixTimestampMillis::new(200_000),
        UnixTimestampMillis::new(300_000),
    )
    .unwrap();
    let login_api = LoginPassthrough(login);
    let mut auth = coop_launcher::AuthSession::login(
        &login_api,
        &keychain,
        "ash",
        coop_launcher::AuthSession::password("password").unwrap(),
    )
    .await
    .unwrap();
    let error = auth.logout(&LogoutFailureApi, &keychain).await.unwrap_err();
    assert!(matches!(error, AuthError::Transport));
    assert!(keychain.load(KEYCHAIN_SERVICE, "ash").unwrap().is_none());
    assert!(auth.access_token().is_none());
    assert!(auth.refresh_token().is_none());
}

fn user() -> UserId {
    UserId::new(Uuid::from_u128(10)).unwrap()
}

fn family() -> RefreshFamilyId {
    RefreshFamilyId::new(Uuid::from_u128(11)).unwrap()
}

fn refresh_response(access: &str, refresh: &str) -> RefreshResponse {
    RefreshResponse::new(
        AccessToken::new(access).unwrap(),
        RefreshToken::new(refresh).unwrap(),
        family(),
        UnixTimestampMillis::new(400_000),
        UnixTimestampMillis::new(500_000),
    )
    .unwrap()
}

#[derive(Default)]
struct MemoryKeychain {
    token: Mutex<Option<RefreshToken>>,
    fail_store: bool,
}

impl RefreshTokenStore for MemoryKeychain {
    fn load(&self, _service: &str, _user: &str) -> Result<Option<RefreshToken>, KeychainError> {
        Ok(self.token.lock().unwrap().clone())
    }

    fn store(
        &self,
        _service: &str,
        _user: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError> {
        if self.fail_store {
            return Err(KeychainError::Operation);
        }
        *self.token.lock().unwrap() = Some(token.clone());
        Ok(())
    }

    fn delete(&self, _service: &str, _user: &str) -> Result<(), KeychainError> {
        *self.token.lock().unwrap() = None;
        Ok(())
    }
}

struct FakeAuthApi {
    refresh_responses: Mutex<Vec<RefreshResponse>>,
    refresh_calls: Mutex<Vec<RefreshToken>>,
}

impl FakeAuthApi {
    fn with_refresh_response(response: RefreshResponse) -> Self {
        Self::with_refresh_responses(vec![response.clone(), response])
    }

    fn with_refresh_responses(responses: Vec<RefreshResponse>) -> Self {
        Self {
            refresh_responses: Mutex::new(responses),
            refresh_calls: Mutex::new(Vec::new()),
        }
    }
}

impl AuthApi for FakeAuthApi {
    fn login(&self, _request: coop_cloud::LoginRequest) -> AuthFuture<'_, LoginResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }

    fn refresh(&self, request: coop_cloud::RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        self.refresh_calls
            .lock()
            .unwrap()
            .push(request.refresh_token);
        let response = self.refresh_responses.lock().unwrap().pop();
        Box::pin(async move { response.ok_or(AuthError::Transport) })
    }

    fn logout(
        &self,
        _request: coop_cloud::LogoutRequest,
    ) -> AuthFuture<'_, coop_cloud::LogoutResponse> {
        Box::pin(async { Ok(coop_cloud::LogoutResponse::default()) })
    }
}

struct LoginPassthrough(LoginResponse);
impl AuthApi for LoginPassthrough {
    fn login(&self, _request: coop_cloud::LoginRequest) -> AuthFuture<'_, LoginResponse> {
        let response = self.0.clone();
        Box::pin(async move { Ok(response) })
    }
    fn refresh(&self, _request: coop_cloud::RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }
    fn logout(
        &self,
        _request: coop_cloud::LogoutRequest,
    ) -> AuthFuture<'_, coop_cloud::LogoutResponse> {
        Box::pin(async { Ok(coop_cloud::LogoutResponse::default()) })
    }
}

struct LogoutSpyApi {
    login: LoginResponse,
    revoked: Arc<Mutex<Vec<RefreshToken>>>,
    events: Arc<Mutex<Vec<String>>>,
}

impl LogoutSpyApi {
    fn new(login: LoginResponse, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            login,
            revoked: Arc::new(Mutex::new(Vec::new())),
            events,
        }
    }
}

impl AuthApi for LogoutSpyApi {
    fn login(&self, _request: coop_cloud::LoginRequest) -> AuthFuture<'_, LoginResponse> {
        let response = self.login.clone();
        Box::pin(async move { Ok(response) })
    }

    fn refresh(&self, _request: coop_cloud::RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }

    fn logout(
        &self,
        request: coop_cloud::LogoutRequest,
    ) -> AuthFuture<'_, coop_cloud::LogoutResponse> {
        self.revoked.lock().unwrap().push(request.refresh_token);
        self.events.lock().unwrap().push("revoke".to_owned());
        Box::pin(async { Ok(coop_cloud::LogoutResponse::default()) })
    }
}

struct LogoutSpyKeychain {
    token: Mutex<Option<RefreshToken>>,
    events: Arc<Mutex<Vec<String>>>,
}

impl LogoutSpyKeychain {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            token: Mutex::new(None),
            events,
        }
    }
}

impl RefreshTokenStore for LogoutSpyKeychain {
    fn load(&self, _service: &str, _user: &str) -> Result<Option<RefreshToken>, KeychainError> {
        Ok(self.token.lock().unwrap().clone())
    }

    fn store(
        &self,
        _service: &str,
        _user: &str,
        token: &RefreshToken,
    ) -> Result<(), KeychainError> {
        *self.token.lock().unwrap() = Some(token.clone());
        Ok(())
    }

    fn delete(&self, _service: &str, _user: &str) -> Result<(), KeychainError> {
        self.events.lock().unwrap().push("delete".to_owned());
        *self.token.lock().unwrap() = None;
        Ok(())
    }
}

struct LogoutFailureApi;
impl AuthApi for LogoutFailureApi {
    fn login(&self, _request: coop_cloud::LoginRequest) -> AuthFuture<'_, LoginResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }
    fn refresh(&self, _request: coop_cloud::RefreshRequest) -> AuthFuture<'_, RefreshResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }
    fn logout(
        &self,
        _request: coop_cloud::LogoutRequest,
    ) -> AuthFuture<'_, coop_cloud::LogoutResponse> {
        Box::pin(async { Err(AuthError::Transport) })
    }
}

struct FakeKeychain {
    seen: Arc<Mutex<Vec<String>>>,
}
impl RefreshTokenStore for FakeKeychain {
    fn load(&self, service: &str, user: &str) -> Result<Option<RefreshToken>, KeychainError> {
        self.seen.lock().unwrap().push(format!("{service}:{user}"));
        Ok(None)
    }
    fn store(&self, service: &str, user: &str, _token: &RefreshToken) -> Result<(), KeychainError> {
        self.seen.lock().unwrap().push(format!("{service}:{user}"));
        Ok(())
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), KeychainError> {
        self.seen.lock().unwrap().push(format!("{service}:{user}"));
        Ok(())
    }
}

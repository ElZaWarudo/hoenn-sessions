//! Focused black-box coverage for the authenticated private-pilot distributor.

use std::{
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use coop_cloud::{InvitationCode, LoginRequest, Password, RegisterRequest, SigningPrivateKey};
use coop_server::{Phase2App, Phase2Config, phase2::releases};
use http_body_util::BodyExt;
use tower::ServiceExt;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const INVITE: &str = "release-api-test-invite";
const USERNAME: &str = "ReleaseApiTestUser";
const PASSWORD: &str = "release-api-test-password";

struct Fixture {
    app: Phase2App,
    root: PathBuf,
    access_token: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture() -> TestResult<Fixture> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("coop-release-api-{stamp}"));
    fs::create_dir_all(&root)?;
    write_release(&root, "release-one", b"signed-envelope-one")?;
    fs::write(root.join(releases::CURRENT_RELEASE_FILE), b"release-one\n")?;
    let android = root.join(releases::ANDROID_DIRECTORY).join("apk-one");
    fs::create_dir_all(&android)?;
    fs::write(root.join(releases::ANDROID_CURRENT_FILE), b"apk-one\n")?;
    fs::write(
        android.join(releases::ANDROID_METADATA_FILE),
        b"{\"release_id\":\"apk-one\",\"version_code\":4}",
    )?;
    fs::write(android.join(releases::ANDROID_APK_FILE), b"private-apk")?;
    let game = root.join(releases::GAME_DIRECTORY).join("game-one");
    fs::create_dir_all(&game)?;
    fs::write(root.join(releases::GAME_CURRENT_FILE), b"game-one\n")?;
    fs::write(
        game.join(releases::RELEASE_ENVELOPE_FILE),
        b"signed-game-envelope",
    )?;
    fs::write(game.join("game.gba"), b"private-rom")?;
    fs::write(game.join("bridge_manifest.json"), b"private-manifest")?;
    let installer = root.join(releases::INSTALLER_DIRECTORY).join("msi-one");
    fs::create_dir_all(&installer)?;
    fs::write(root.join(releases::INSTALLER_CURRENT_FILE), b"msi-one\n")?;
    fs::write(
        installer.join(releases::INSTALLER_METADATA_FILE),
        b"{\"release_id\":\"msi-one\"}",
    )?;
    fs::write(installer.join(releases::INSTALLER_MSI_FILE), b"private-msi")?;

    let config = Phase2Config::local(
        vec![0x55; 32],
        SigningPrivateKey::from_bytes([7; 32]),
        "release-api-test-key",
    )?;
    let app = Phase2App::new(config)?.with_release_root(root.clone())?;
    app.add_invitation(INVITE)?;
    let password = Password::new(PASSWORD)?;
    app.register(RegisterRequest::new(
        USERNAME,
        password.clone(),
        InvitationCode::new(INVITE)?,
    )?)?;
    let login = app.login(LoginRequest::new(USERNAME, password)?)?;
    Ok(Fixture {
        app,
        root,
        access_token: login.access_token.expose_secret().to_owned(),
    })
}

fn write_release(root: &Path, release_id: &str, envelope: &[u8]) -> TestResult<()> {
    let release = root.join(releases::RELEASES_DIRECTORY).join(release_id);
    fs::create_dir_all(&release)?;
    fs::write(release.join(releases::RELEASE_ENVELOPE_FILE), envelope)?;
    for (artifact_id, destination) in releases::FIXED_ARTIFACTS {
        let path = release.join(destination);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("artifact:{artifact_id}").as_bytes())?;
    }
    Ok(())
}

async fn request(
    router: Router,
    uri: &str,
    token: Option<&str>,
    range: Option<&str>,
) -> TestResult<(StatusCode, HeaderMap, Vec<u8>)> {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(range) = range {
        builder = builder.header(header::RANGE, range);
    }
    let response = router.oneshot(builder.body(Body::empty())?).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await?.to_bytes().to_vec();
    Ok((status, headers, body))
}

fn assert_private_no_store(headers: &HeaderMap) {
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("private, no-store")
    );
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "covers the complete invitation and registration HTTP flow"
)]
async fn portal_invitation_requires_login_and_registers_once() -> TestResult<()> {
    let fixture = fixture()?;
    let page = fixture
        .app
        .router()
        .oneshot(Request::builder().uri("/").body(Body::empty())?)
        .await?;
    assert_eq!(page.status(), StatusCode::OK);
    assert!(
        page.headers()[header::CONTENT_TYPE]
            .to_str()?
            .starts_with("text/html")
    );
    let uri = "/v1/auth/invitations";
    let unauthorized = fixture
        .app
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let response = fixture
        .app
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", fixture.access_token),
                )
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = response.into_body().collect().await?.to_bytes();
    let code = serde_json::from_slice::<serde_json::Value>(&bytes)?["invitation_code"]
        .as_str()
        .ok_or("missing invitation code")?
        .to_owned();
    let invite = |token: &str| {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
    };
    for _ in 0..4 {
        assert_eq!(
            fixture
                .app
                .router()
                .oneshot(invite(&fixture.access_token)?)
                .await?
                .status(),
            StatusCode::CREATED
        );
    }
    assert_eq!(
        fixture
            .app
            .router()
            .oneshot(invite(&fixture.access_token)?)
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    let registration = RegisterRequest::new(
        "SecondPlayer",
        Password::new("another-password")?,
        InvitationCode::new(code)?,
    )?;
    let body = serde_json::to_vec(&registration)?;
    let register = || {
        Request::builder()
            .method("POST")
            .uri("/v1/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.clone()))
    };
    assert_eq!(
        fixture.app.router().oneshot(register()?).await?.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        fixture.app.router().oneshot(register()?).await?.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fixture
            .app
            .router()
            .oneshot(invite(&fixture.access_token)?)
            .await?
            .status(),
        StatusCode::CREATED
    );
    let second = fixture.app.login(LoginRequest::new(
        "SecondPlayer",
        Password::new("another-password")?,
    )?)?;
    assert_eq!(
        fixture
            .app
            .router()
            .oneshot(invite(second.access_token.expose_secret())?)
            .await?
            .status(),
        StatusCode::CREATED
    );
    Ok(())
}

#[tokio::test]
async fn windows_installer_is_private_and_streamed_from_fixed_path() -> TestResult<()> {
    let fixture = fixture()?;
    for uri in [
        "/v1/releases/windows-x86_64/installer/latest",
        "/v1/releases/windows-x86_64/installer/msi-one/msi",
    ] {
        let (status, headers, _) = request(fixture.app.router(), uri, None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_private_no_store(&headers);
        let (status, _, _) = request(
            fixture.app.router(),
            uri,
            Some(&fixture.access_token),
            Some("bytes=0-1"),
        )
        .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _, metadata) = request(
        fixture.app.router(),
        "/v1/releases/windows-x86_64/installer/latest",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metadata, b"{\"release_id\":\"msi-one\"}");
    let (status, headers, bytes) = request(
        fixture.app.router(),
        "/v1/releases/windows-x86_64/installer/msi-one/msi",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"private-msi");
    assert_eq!(headers[header::CONTENT_TYPE], "application/x-msi");
    Ok(())
}

#[tokio::test]
async fn android_apk_and_metadata_are_authenticated_and_path_closed() -> TestResult<()> {
    let fixture = fixture()?;
    let latest = "/v1/releases/android/latest";
    let apk = "/v1/releases/android/apk-one/apk";
    for uri in [latest, apk] {
        let (status, headers, _) = request(fixture.app.router(), uri, None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_private_no_store(&headers);
        let (status, headers, _) = request(
            fixture.app.router(),
            uri,
            Some(&fixture.access_token),
            Some("bytes=0-1"),
        )
        .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_private_no_store(&headers);
    }
    let (status, _, body) = request(
        fixture.app.router(),
        latest,
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"{\"release_id\":\"apk-one\",\"version_code\":4}");
    let (status, _, body) =
        request(fixture.app.router(), apk, Some(&fixture.access_token), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"private-apk");
    let (status, headers, body) = request(
        fixture.app.router(),
        apk,
        Some(&fixture.access_token),
        Some("bytes=8-"),
    )
    .await?;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers[header::CONTENT_RANGE], "bytes 8-10/11");
    assert_eq!(body, b"apk");
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/android/%2e%2e/apk",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn game_channel_is_private_and_supports_exact_resume_ranges() -> TestResult<()> {
    let fixture = fixture()?;
    let latest = "/v1/releases/game/latest";
    let rom = "/v1/releases/game/game-one/artifacts/rom";
    for uri in [latest, rom] {
        let (status, _, _) = request(fixture.app.router(), uri, None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, body) = request(
        fixture.app.router(),
        latest,
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"signed-game-envelope");
    let (status, headers, body) = request(
        fixture.app.router(),
        rom,
        Some(&fixture.access_token),
        Some("bytes=8-"),
    )
    .await?;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers[header::CONTENT_RANGE], "bytes 8-10/11");
    assert_eq!(body, b"rom");
    let (status, _, _) = request(
        fixture.app.router(),
        rom,
        Some(&fixture.access_token),
        Some("bytes=11-"),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/game/game-one/artifacts/sidecar",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn latest_authenticates_before_filesystem_and_preserves_exact_bytes() -> TestResult<()> {
    let fixture = fixture()?;
    let uri = "/v1/releases/windows-x86_64/latest";
    let (status, headers, _body) = request(fixture.app.router(), uri, None, None).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_private_no_store(&headers);

    let (status, headers, body) =
        request(fixture.app.router(), uri, Some(&fixture.access_token), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"signed-envelope-one");
    assert_eq!(headers[header::CONTENT_LENGTH], body.len().to_string());
    assert_private_no_store(&headers);
    assert!(headers.get(header::ACCEPT_RANGES).is_none());

    let (status, headers, _) = request(
        fixture.app.router(),
        uri,
        Some(&fixture.access_token),
        Some("bytes=0-1"),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_private_no_store(&headers);
    Ok(())
}

#[tokio::test]
async fn every_fixed_mapping_streams_exact_bytes_and_unknown_ids_fail() -> TestResult<()> {
    let fixture = fixture()?;
    for (artifact_id, _) in releases::FIXED_ARTIFACTS {
        let uri = format!("/v1/releases/release-one/artifacts/{artifact_id}");
        let (status, headers, body) = request(
            fixture.app.router(),
            &uri,
            Some(&fixture.access_token),
            None,
        )
        .await?;
        let expected = format!("artifact:{artifact_id}");
        assert_eq!(status, StatusCode::OK, "{artifact_id}");
        assert_eq!(body, expected.as_bytes(), "{artifact_id}");
        assert_eq!(headers[header::CONTENT_LENGTH], body.len().to_string());
        assert_private_no_store(&headers);
        assert!(headers.get(header::ACCEPT_RANGES).is_none());
    }

    for artifact_id in ["bootstrapper", "manifest", "not-a-fixed-id"] {
        let uri = format!("/v1/releases/release-one/artifacts/{artifact_id}");
        let (status, headers, _) = request(
            fixture.app.router(),
            &uri,
            Some(&fixture.access_token),
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::NOT_FOUND, "{artifact_id}");
        assert_private_no_store(&headers);
    }
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/release-one/artifacts/%2e%2e",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn signed_world_artifact_ids_use_closed_numeric_paths() -> TestResult<()> {
    let fixture = fixture()?;
    let release = fixture
        .root
        .join(releases::RELEASES_DIRECTORY)
        .join("release-one");
    let artifacts = [
        ("region-catalog", "release_catalog.json"),
        ("world-1-rom", "worlds/1/game.gba"),
        ("world-1-compatibility", "worlds/1/bridge_manifest.json"),
        ("world-1-player-transfer", "worlds/1/player_transfer.json"),
        ("world-2-rom", "worlds/2/game.gba"),
        ("world-2-compatibility", "worlds/2/bridge_manifest.json"),
        ("world-2-player-transfer", "worlds/2/player_transfer.json"),
        ("world-7-rom", "worlds/7/game.gba"),
    ];
    for (identity, relative) in artifacts {
        let path = release.join(relative);
        fs::create_dir_all(path.parent().expect("artifact parent"))?;
        let bytes = format!("artifact:{identity}");
        fs::write(path, bytes.as_bytes())?;
        let uri = format!("/v1/releases/release-one/artifacts/{identity}");
        let (status, _, body) = request(
            fixture.app.router(),
            &uri,
            Some(&fixture.access_token),
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{identity}");
        assert_eq!(body, bytes.as_bytes(), "{identity}");
        let (status, _, _) = request(fixture.app.router(), &uri, None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{identity}");
    }
    for identity in [
        "world-0-rom",
        "world-01-rom",
        "world-65536-rom",
        "world-2-unknown",
        "world-2-rom-extra",
        "world-2-..",
        "world-2-rom%2f..",
    ] {
        let uri = format!("/v1/releases/release-one/artifacts/{identity}");
        let (status, _, _) = request(
            fixture.app.router(),
            &uri,
            Some(&fixture.access_token),
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::NOT_FOUND, "{identity}");
    }
    Ok(())
}

#[tokio::test]
async fn current_switch_and_path_bounds_fail_closed() -> TestResult<()> {
    let fixture = fixture()?;
    write_release(&fixture.root, "release-two", b"signed-envelope-two")?;
    let current_tmp = fixture.root.join("current.tmp");
    fs::write(&current_tmp, b"release-two\n")?;
    if fs::rename(
        &current_tmp,
        fixture.root.join(releases::CURRENT_RELEASE_FILE),
    )
    .is_err()
    {
        fs::remove_file(fixture.root.join(releases::CURRENT_RELEASE_FILE))?;
        fs::rename(
            &current_tmp,
            fixture.root.join(releases::CURRENT_RELEASE_FILE),
        )?;
    }
    let (status, _, body) = request(
        fixture.app.router(),
        "/v1/releases/windows-x86_64/latest",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"signed-envelope-two");

    fs::write(
        fixture.root.join(releases::CURRENT_RELEASE_FILE),
        b"release-one\n",
    )?;

    let (status, _, _) = request(
        fixture.app.router(),
        &format!(
            "/v1/releases/{}/artifacts/rom",
            "r".repeat(releases::MAX_RELEASE_ID_BYTES + 1)
        ),
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let directory = fixture
        .root
        .join(releases::RELEASES_DIRECTORY)
        .join("release-one")
        .join("bridge_manifest.json");
    fs::remove_file(&directory)?;
    fs::create_dir(&directory)?;
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/release-one/artifacts/compatibility-manifest",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let envelope = fixture
        .root
        .join(releases::RELEASES_DIRECTORY)
        .join("release-one")
        .join(releases::RELEASE_ENVELOPE_FILE);
    let oversized = File::create(&envelope)?;
    oversized.set_len(releases::MAX_ENVELOPE_BYTES + 1)?;
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/windows-x86_64/latest",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    let rom = fixture
        .root
        .join(releases::RELEASES_DIRECTORY)
        .join("release-one")
        .join("runtime/game.gba");
    let oversized = File::create(&rom)?;
    oversized.set_len(releases::MAX_ARTIFACT_BYTES + 1)?;
    let (status, _, _) = request(
        fixture.app.router(),
        "/v1/releases/release-one/artifacts/rom",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_artifacts_are_rejected() -> TestResult<()> {
    use std::os::unix::fs::symlink;

    let fixture = fixture()?;
    let outside = fixture.root.join("outside.bin");
    fs::write(&outside, b"outside")?;
    let rom = fixture
        .root
        .join(releases::RELEASES_DIRECTORY)
        .join("release-one")
        .join("runtime/game.gba");
    fs::remove_file(&rom)?;
    symlink(&outside, &rom)?;
    let (status, headers, _) = request(
        fixture.app.router(),
        "/v1/releases/release-one/artifacts/rom",
        Some(&fixture.access_token),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_private_no_store(&headers);
    Ok(())
}

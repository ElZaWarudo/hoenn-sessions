//! Authenticated, path-closed distribution of the private-pilot runtime.
//!
//! This module is intentionally a dumb distributor. The launcher verifies the
//! signed envelope, release freshness, rollback policy, and every artifact
//! digest. The server only authenticates callers, selects the current release,
//! and serves bytes from a fixed owner-controlled tree.

use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

use axum::{
    body::{Body, Bytes},
    extract::{Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::stream;
use tokio::io::AsyncReadExt;

use super::{Phase2App, Phase2Error};

/// The directory below `COOP_RELEASE_ROOT` containing immutable releases.
pub const RELEASES_DIRECTORY: &str = "releases";
/// The atomically replaced file naming the release served by `latest`.
pub const CURRENT_RELEASE_FILE: &str = "current";
/// The exact signed envelope filename served by `latest`.
pub const RELEASE_ENVELOPE_FILE: &str = "release-envelope.json";
pub const ANDROID_DIRECTORY: &str = "android";
pub const ANDROID_CURRENT_FILE: &str = "android/current";
pub const ANDROID_METADATA_FILE: &str = "metadata.json";
pub const ANDROID_APK_FILE: &str = "app-release.apk";
/// The maximum exact envelope size accepted by the distributor.
pub const MAX_ENVELOPE_BYTES: u64 = 1024 * 1024;
/// The maximum size of one streamed artifact.
pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
/// The maximum release identifier size accepted by the route.
pub const MAX_RELEASE_ID_BYTES: usize = 128;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;
const CURRENT_MARKER_MAX_BYTES: u64 = MAX_RELEASE_ID_BYTES as u64 + 1;

/// The closed server-side artifact mapping. Paths never come from request
/// metadata or the signed envelope.
pub const FIXED_ARTIFACTS: &[(&str, &str)] = &[
    ("desktop-app", "app/coop-launcher.exe"),
    ("managed-mgba", "runtime/mgba.exe"),
    ("rom", "runtime/game.gba"),
    ("sidecar", "runtime/coop-sidecar.exe"),
    ("bridge-main", "bridge/main.lua"),
    ("bridge-memory", "bridge/memory.lua"),
    ("bridge-protocol", "bridge/protocol.lua"),
    ("bridge-addresses", "bridge/generated_addresses.lua"),
    ("compatibility-manifest", "bridge_manifest.json"),
    ("trust-bundle", "trust/release-trust.json"),
    ("notices", "THIRD_PARTY_NOTICES.txt"),
];

/// Serves the exact signed envelope for the atomically selected release.
pub(crate) async fn latest(State(app): State<Phase2App>, headers: HeaderMap) -> Response {
    private_response(match latest_inner(&app, &headers) {
        Ok(response) => response,
        Err(error) => error.into_response(),
    })
}

/// Serves the currently published Android APK metadata, independent of game releases.
pub(crate) async fn android_latest(State(app): State<Phase2App>, headers: HeaderMap) -> Response {
    private_response(match android_latest_inner(&app, &headers) {
        Ok(response) => response,
        Err(error) => error.into_response(),
    })
}

/// Streams a privately published Android APK after bearer authentication.
pub(crate) async fn android_apk(
    State(app): State<Phase2App>,
    headers: HeaderMap,
    AxumPath(release_id): AxumPath<String>,
) -> Response {
    private_response(match android_apk_inner(&app, &headers, &release_id) {
        Ok(response) => response,
        Err(error) => error.into_response(),
    })
}

/// Streams one fixed artifact from a named release generation.
pub(crate) async fn artifact(
    State(app): State<Phase2App>,
    headers: HeaderMap,
    AxumPath((release_id, artifact_id)): AxumPath<(String, String)>,
) -> Response {
    private_response(
        match artifact_inner(&app, &headers, &release_id, &artifact_id) {
            Ok(response) => response,
            Err(error) => error.into_response(),
        },
    )
}

fn latest_inner(app: &Phase2App, headers: &HeaderMap) -> Result<Response, Phase2Error> {
    // Authentication must precede every release-root or marker filesystem
    // operation so unauthenticated callers receive no filesystem oracle.
    super::actor(headers, app)?;
    reject_range(headers)?;
    let root = configured_root(app)?;
    let release_id = current_release_id(root)?;
    let relative = PathBuf::from(RELEASES_DIRECTORY)
        .join(&release_id)
        .join(RELEASE_ENVELOPE_FILE);
    let path = resolve_under(root, &relative)?;
    let bytes = read_bounded(&path, MAX_ENVELOPE_BYTES)?;
    let length = bytes.len().to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, length)
        .body(Body::from(bytes))
        .map_err(|_| Phase2Error::Internal)
}

fn android_latest_inner(app: &Phase2App, headers: &HeaderMap) -> Result<Response, Phase2Error> {
    super::actor(headers, app)?;
    reject_range(headers)?;
    let root = configured_root(app)?;
    let release_id = marker_release_id(root, Path::new(ANDROID_CURRENT_FILE))?;
    let relative = PathBuf::from(ANDROID_DIRECTORY)
        .join(&release_id)
        .join(ANDROID_METADATA_FILE);
    let bytes = read_bounded(&resolve_under(root, &relative)?, 8192)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .body(Body::from(bytes))
        .map_err(|_| Phase2Error::Internal)
}

fn android_apk_inner(
    app: &Phase2App,
    headers: &HeaderMap,
    release_id: &str,
) -> Result<Response, Phase2Error> {
    super::actor(headers, app)?;
    reject_range(headers)?;
    validate_release_id(release_id)?;
    let root = configured_root(app)?;
    let relative = PathBuf::from(ANDROID_DIRECTORY)
        .join(release_id)
        .join(ANDROID_APK_FILE);
    let (file, length) = open_bounded(&resolve_under(root, &relative)?, MAX_ARTIFACT_BYTES)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.android.package-archive",
        )
        .header(header::CONTENT_LENGTH, length.to_string())
        .body(Body::from_stream(artifact_stream(
            tokio::fs::File::from_std(file),
            length,
        )))
        .map_err(|_| Phase2Error::Internal)
}

fn artifact_inner(
    app: &Phase2App,
    headers: &HeaderMap,
    release_id: &str,
    artifact_id: &str,
) -> Result<Response, Phase2Error> {
    // As above, the bearer check is deliberately the first operation.
    super::actor(headers, app)?;
    reject_range(headers)?;
    validate_release_id(release_id)?;
    let destination = fixed_destination(artifact_id).ok_or(Phase2Error::NotFound)?;
    let root = configured_root(app)?;
    let relative = PathBuf::from(RELEASES_DIRECTORY)
        .join(release_id)
        .join(destination);
    let path = resolve_under(root, &relative)?;
    let (file, length) = open_bounded(&path, MAX_ARTIFACT_BYTES)?;
    let stream = artifact_stream(tokio::fs::File::from_std(file), length);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, length.to_string())
        .body(Body::from_stream(stream))
        .map_err(|_| Phase2Error::Internal)
}

fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn configured_root(app: &Phase2App) -> Result<&Path, Phase2Error> {
    app.release_root
        .as_deref()
        .map(PathBuf::as_path)
        .ok_or(Phase2Error::Internal)
}

fn reject_range(headers: &HeaderMap) -> Result<(), Phase2Error> {
    if headers.contains_key(header::RANGE) {
        Err(Phase2Error::InvalidRequest)
    } else {
        Ok(())
    }
}

fn current_release_id(root: &Path) -> Result<String, Phase2Error> {
    marker_release_id(root, Path::new(CURRENT_RELEASE_FILE))
}

fn marker_release_id(root: &Path, marker: &Path) -> Result<String, Phase2Error> {
    let marker = resolve_under(root, marker)?;
    let bytes = read_bounded(&marker, CURRENT_MARKER_MAX_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Phase2Error::NotFound)?;
    let release_id = text.trim_end_matches(['\r', '\n']);
    validate_release_id(release_id)?;
    Ok(release_id.to_owned())
}

fn validate_release_id(value: &str) -> Result<(), Phase2Error> {
    if value.is_empty()
        || value.len() > MAX_RELEASE_ID_BYTES
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(Phase2Error::NotFound);
    }
    Ok(())
}

fn fixed_destination(artifact_id: &str) -> Option<&'static str> {
    FIXED_ARTIFACTS
        .iter()
        .find_map(|(id, destination)| (*id == artifact_id).then_some(*destination))
}

fn resolve_under(root: &Path, relative: &Path) -> Result<PathBuf, Phase2Error> {
    if relative.is_absolute() {
        return Err(Phase2Error::NotFound);
    }
    let root = checked_directory(root)?;
    let candidate = root.join(relative);
    reject_unsafe_components(&candidate)?;
    let canonical = fs::canonicalize(&candidate).map_err(map_io_error)?;
    if !canonical.starts_with(&root) || canonical == root {
        return Err(Phase2Error::NotFound);
    }
    Ok(canonical)
}

fn checked_directory(path: &Path) -> Result<PathBuf, Phase2Error> {
    let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
    if unsafe_metadata(&metadata) || !metadata.is_dir() {
        return Err(Phase2Error::NotFound);
    }
    reject_unsafe_components(path)?;
    let canonical = fs::canonicalize(path).map_err(map_io_error)?;
    let canonical_metadata = fs::symlink_metadata(&canonical).map_err(map_io_error)?;
    if unsafe_metadata(&canonical_metadata) || !canonical_metadata.is_dir() {
        return Err(Phase2Error::NotFound);
    }
    Ok(canonical)
}

fn reject_unsafe_components(path: &Path) -> Result<(), Phase2Error> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(map_io_error)?;
        if unsafe_metadata(&metadata) {
            return Err(Phase2Error::NotFound);
        }
    }
    Ok(())
}

fn open_bounded(path: &Path, maximum: u64) -> Result<(File, u64), Phase2Error> {
    let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
    if unsafe_metadata(&metadata) || !metadata.is_file() {
        return Err(Phase2Error::NotFound);
    }
    let length = metadata.len();
    if length > maximum {
        return Err(Phase2Error::PayloadTooLarge);
    }
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(map_io_error)?;
    let opened_metadata = file.metadata().map_err(map_io_error)?;
    if unsafe_metadata(&opened_metadata) || !opened_metadata.is_file() {
        return Err(Phase2Error::NotFound);
    }
    let opened_length = opened_metadata.len();
    if opened_length > maximum {
        return Err(Phase2Error::PayloadTooLarge);
    }
    if opened_length != length {
        return Err(Phase2Error::Conflict);
    }
    Ok((file, opened_length))
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, Phase2Error> {
    let (file, expected_length) = open_bounded(path, maximum)?;
    let capacity = usize::try_from(expected_length).map_err(|_| Phase2Error::PayloadTooLarge)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(map_io_error)?;
    if bytes.len() as u64 != expected_length {
        return Err(Phase2Error::Conflict);
    }
    Ok(bytes)
}

fn artifact_stream(
    file: tokio::fs::File,
    remaining: u64,
) -> impl futures_util::Stream<Item = Result<Bytes, io::Error>> + Send + 'static {
    stream::unfold((file, remaining), |(mut file, mut remaining)| async move {
        if remaining == 0 {
            return None;
        }
        let amount = remaining.min(STREAM_CHUNK_BYTES as u64) as usize;
        let mut chunk = vec![0_u8; amount];
        match file.read(&mut chunk).await {
            Ok(0) => Some((
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "release artifact changed while streaming",
                )),
                (file, 0),
            )),
            Ok(read) => {
                remaining -= read as u64;
                chunk.truncate(read);
                Some((Ok(Bytes::from(chunk)), (file, remaining)))
            }
            Err(error) => Some((Err(error), (file, 0))),
        }
    })
}

fn map_io_error(error: io::Error) -> Phase2Error {
    if error.kind() == io::ErrorKind::NotFound {
        Phase2Error::NotFound
    } else {
        Phase2Error::Internal
    }
}

fn unsafe_metadata(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink() || is_reparse_point(metadata)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
const fn is_reparse_point(_: &Metadata) -> bool {
    false
}

//! Strict authenticated release transport for the desktop actor.

use std::{time::{SystemTime, UNIX_EPOCH}};

use coop_launcher::{
    ArtifactIdentity, ArtifactPayload, ArtifactSet, AuthSession, SignedReleaseEnvelope,
    TrustedReleaseKey, VerifiedRelease, MAX_ARTIFACT_BYTES, MAX_ENVELOPE_BYTES,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;

const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReleaseError {
    #[error("release endpoint is not permitted")]
    InvalidEndpoint,
    #[error("release service is unavailable")]
    Transport,
    #[error("release service rejected the request")]
    Unauthorized,
    #[error("release service returned status {0}")]
    Status(u16),
    #[error("release response was invalid or too large")]
    Response,
    #[error("signed release was not accepted")]
    Envelope,
    #[error("release artifact did not match its signed identity")]
    Artifact,
    #[error("release clock is unavailable")]
    Clock,
}

#[derive(Clone, Debug)]
pub struct DownloadedRelease {
    pub verified: VerifiedRelease,
    pub artifacts: ArtifactSet,
}

#[derive(Clone)]
pub struct ReleaseClient {
    client: reqwest::Client,
    base: Url,
    trusted_key: TrustedReleaseKey,
}

impl std::fmt::Debug for ReleaseClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReleaseClient")
            .field("base", &self.base)
            .field("trusted_key", &self.trusted_key.key_id())
            .finish_non_exhaustive()
    }
}

impl ReleaseClient {
    pub fn new(base: &str, trusted_key: TrustedReleaseKey) -> Result<Self, ReleaseError> {
        let base = Url::parse(base).map_err(|_| ReleaseError::InvalidEndpoint)?;
        let host = base.host_str().ok_or(ReleaseError::InvalidEndpoint)?;
        let loopback = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
        if base.cannot_be_a_base()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || !base.path().trim_matches('/').is_empty()
            || (!loopback && base.scheme() != "https")
            || (loopback && !matches!(base.scheme(), "http" | "https"))
            || base.port().is_some_and(|port| {
                matches!((base.scheme(), port), ("https", 443) | ("http", 80))
            })
        {
            return Err(ReleaseError::InvalidEndpoint);
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|_| ReleaseError::Transport)?;
        Ok(Self { client, base, trusted_key })
    }

    pub fn trusted_key(&self) -> &TrustedReleaseKey { &self.trusted_key }

    pub async fn fetch_latest(
        &self,
        auth: &AuthSession,
        with_artifacts: bool,
    ) -> Result<DownloadedRelease, ReleaseError> {
        let token = auth.access_token().ok_or(ReleaseError::Unauthorized)?;
        let envelope_url = self
            .base
            .join("v1/releases/windows-x86_64/latest")
            .map_err(|_| ReleaseError::InvalidEndpoint)?;
        let response = self
            .client
            .get(envelope_url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| ReleaseError::Transport)?;
        let bytes = read_bounded(response, MAX_ENVELOPE_BYTES).await?;
        let verified = SignedReleaseEnvelope::verify_json(&bytes, &self.trusted_key)
            .map_err(|_| ReleaseError::Envelope)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ReleaseError::Clock)?
            .as_secs()
            .try_into()
            .map_err(|_| ReleaseError::Clock)?;
        verified
            .descriptor()
            .validate_at(now)
            .map_err(|_| ReleaseError::Envelope)?;
        let artifacts = if with_artifacts {
            let mut entries = Vec::with_capacity(ArtifactIdentity::all().len());
            for identity in ArtifactIdentity::all().iter().copied() {
                let path = format!(
                    "v1/releases/{}/artifacts/{}",
                    verified.release_id(),
                    identity.as_str()
                );
                let url = self.base.join(&path).map_err(|_| ReleaseError::InvalidEndpoint)?;
                let response = self
                    .client
                    .get(url)
                    .bearer_auth(token.expose_secret())
                    .send()
                    .await
                    .map_err(|_| ReleaseError::Transport)?;
                let bytes = read_bounded(response, MAX_ARTIFACT_BYTES as usize).await?;
                let descriptor = verified
                    .descriptor()
                    .artifact(identity)
                    .ok_or(ReleaseError::Artifact)?;
                let mut digest = String::with_capacity(64);
                for byte in Sha256::digest(&bytes) {
                    digest.push_str(&format!("{byte:02x}"));
                }
                if bytes.len() as u64 != descriptor.size || digest != descriptor.sha256 {
                    return Err(ReleaseError::Artifact);
                }
                entries.push(ArtifactPayload::new(identity, bytes));
            }
            ArtifactSet::new(entries)
        } else {
            ArtifactSet::new(std::iter::empty())
        };
        Ok(DownloadedRelease { verified, artifacts })
    }
}

async fn read_bounded(response: reqwest::Response, maximum: usize) -> Result<Vec<u8>, ReleaseError> {
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ReleaseError::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(ReleaseError::Status(response.status().as_u16()));
    }
    if response.content_length().is_some_and(|size| size > maximum as u64) {
        return Err(ReleaseError::Response);
    }
    let mut bytes = Vec::with_capacity(
        response.content_length().unwrap_or(0).min(maximum as u64) as usize,
    );
    let mut response = response;
    while let Some(chunk) = response.chunk().await.map_err(|_| ReleaseError::Transport)? {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(ReleaseError::Response);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

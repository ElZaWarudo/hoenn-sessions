//! Private Firebase bucket access through the Google Cloud Storage JSON API.
//! Credential endpoints are fixed; downloaded object bytes and error bodies
//! never enter logs. Conditional creation preserves publication ownership.

use super::{
    persistent::IoWorker,
    storage::{FirebaseObjectStore, MAX_RESUME_RESPONSE, ObjectStore, StorageError},
};
use reqwest::{
    Method, StatusCode,
    blocking::{Client, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const OAUTH_URL: &str = "https://oauth2.googleapis.com/token";
const STORAGE_URL: &str = "https://storage.googleapis.com";
const METADATA_LIMIT: usize = 64 * 1024;
const RETIRED_CONTENT_TYPE: &str = "application/x-hoenn-retired";

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}
#[derive(Deserialize)]
struct OAuthResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}
#[derive(Deserialize)]
struct Metadata {
    generation: String,
    #[serde(rename = "contentType", default)]
    content_type: String,
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "type")]
    kind: String,
    client_email: String,
    private_key: String,
    private_key_id: String,
    token_uri: String,
}

struct StorageClient {
    http: Client,
    bucket: String,
    endpoint: String,
    email: String,
    key: jsonwebtoken::EncodingKey,
    key_id: String,
    token: Zeroizing<String>,
    token_until: u64,
}

/// A bounded worker with a private, server-only service account credential.
pub(super) struct FirebaseStorage {
    io: IoWorker<StorageClient>,
}

fn now_seconds() -> Result<u64, StorageError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| StorageError::Clock)
}

fn body(response: Response, limit: usize) -> Result<Vec<u8>, StorageError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(StorageError::Transaction);
    }
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::Transaction)?;
    if bytes.len() > limit {
        return Err(StorageError::Transaction);
    }
    Ok(bytes)
}

impl FirebaseStorage {
    /// Loads a Google service-account JSON credential from a mounted file.
    ///
    /// # Errors
    /// Rejects invalid credentials or a credential containing an untrusted
    /// OAuth endpoint. Does not access the network until the first operation.
    pub(super) fn from_service_account_file(
        bucket: &str,
        path: &Path,
    ) -> Result<Self, StorageError> {
        let bytes = super::production::read_secret(path, METADATA_LIMIT)?;
        let mut credentials: Credentials =
            serde_json::from_slice(&bytes).map_err(|_| StorageError::InvalidConfiguration)?;
        let private_key = Zeroizing::new(std::mem::take(&mut credentials.private_key));
        if credentials.kind != "service_account"
            || credentials.token_uri != OAUTH_URL
            || credentials.client_email.is_empty()
            || credentials.client_email.len() > 512
            || credentials.private_key_id.is_empty()
            || credentials.private_key_id.len() > 256
            || bucket.is_empty()
            || bucket.len() > 222
            || !bucket
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            return Err(StorageError::InvalidConfiguration);
        }
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|_| StorageError::InvalidConfiguration)?;
        let bucket = bucket.to_owned();
        // A cold delete may perform OAuth, metadata lookup and conditional
        // deletion: three bounded 25-second requests. Leave scheduling room.
        let io = IoWorker::start_with_timeout(
            move || {
                let http = Client::builder()
                    .connect_timeout(Duration::from_secs(5))
                    .timeout(Duration::from_secs(25))
                    .redirect(reqwest::redirect::Policy::none())
                    .no_proxy()
                    .build()
                    .map_err(|_| StorageError::Transaction)?;
                Ok(StorageClient {
                    http,
                    bucket,
                    endpoint: STORAGE_URL.to_owned(),
                    email: credentials.client_email,
                    key,
                    key_id: credentials.private_key_id,
                    token: Zeroizing::new(String::new()),
                    token_until: 0,
                })
            },
            Duration::from_secs(85),
        )?;
        Ok(Self { io })
    }
}

impl StorageClient {
    fn authorize(&mut self) -> Result<(), StorageError> {
        let now = now_seconds()?;
        if now < self.token_until {
            return Ok(());
        }
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(self.key_id.clone());
        let assertion = Zeroizing::new(
            jsonwebtoken::encode(
                &header,
                &Claims {
                    iss: &self.email,
                    scope: "https://www.googleapis.com/auth/devstorage.read_write",
                    aud: OAUTH_URL,
                    iat: now,
                    exp: now + 3600,
                },
                &self.key,
            )
            .map_err(|_| StorageError::Transaction)?,
        );
        let response = self
            .http
            .post(OAUTH_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .map_err(|_| StorageError::Transaction)?;
        if !response.status().is_success() {
            return Err(StorageError::Transaction);
        }
        let bytes = Zeroizing::new(body(response, METADATA_LIMIT)?);
        let mut token: OAuthResponse =
            serde_json::from_slice(&bytes).map_err(|_| StorageError::Transaction)?;
        if !token.token_type.eq_ignore_ascii_case("bearer")
            || token.expires_in <= 60
            || token.expires_in > 3600
            || token.access_token.is_empty()
        {
            return Err(StorageError::Transaction);
        }
        self.token = Zeroizing::new(std::mem::take(&mut token.access_token));
        self.token_until = now + token.expires_in - 60;
        Ok(())
    }

    fn url(&self, key: &str, upload: bool) -> Result<url::Url, StorageError> {
        if key.is_empty() || key.len() > 1024 || key.bytes().any(|byte| byte < 32) {
            return Err(StorageError::InvalidConfiguration);
        }
        let mut url =
            url::Url::parse(&self.endpoint).map_err(|_| StorageError::InvalidConfiguration)?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| StorageError::InvalidConfiguration)?;
            if upload {
                path.push("upload");
            }
            path.extend(["storage", "v1", "b", &self.bucket, "o"]);
            if !upload {
                path.push(key);
            }
        }
        Ok(url)
    }

    fn metadata(&mut self, key: &str) -> Result<Option<Metadata>, StorageError> {
        self.authorize()?;
        let response = self
            .http
            .get(self.url(key, false)?)
            .query(&[("fields", "generation,contentType")])
            .bearer_auth(self.token.as_str())
            .send()
            .map_err(|_| StorageError::Transaction)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(StorageError::Transaction);
        }
        let metadata: Metadata = serde_json::from_slice(&body(response, METADATA_LIMIT)?)
            .map_err(|_| StorageError::Transaction)?;
        if metadata.generation.is_empty()
            || metadata.generation.len() > 32
            || !metadata.generation.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(StorageError::Transaction);
        }
        Ok(Some(metadata))
    }
}

impl ObjectStore for FirebaseStorage {
    fn retire_if_absent(&self, key: &str) -> Result<bool, StorageError> {
        let key = key.to_owned();
        self.io.run(move |client| {
            client.authorize()?;
            let response = client
                .http
                .post(client.url(&key, true)?)
                .query(&[
                    ("uploadType", "media"),
                    ("name", &key),
                    ("ifGenerationMatch", "0"),
                ])
                .header(reqwest::header::CONTENT_TYPE, RETIRED_CONTENT_TYPE)
                .bearer_auth(client.token.as_str())
                .body(Vec::new())
                .send()
                .map_err(|_| StorageError::Transaction)?;
            if response.status() == StatusCode::PRECONDITION_FAILED {
                return Ok(client
                    .metadata(&key)?
                    .is_some_and(|value| value.content_type == RETIRED_CONTENT_TYPE));
            }
            if !response.status().is_success() {
                return Err(StorageError::Transaction);
            }
            Ok(true)
        })
    }
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let key = key.to_owned();
        self.io.run(move |client| {
            client.authorize()?;
            let response = client
                .http
                .get(client.url(&key, false)?)
                .query(&[("alt", "media")])
                .bearer_auth(client.token.as_str())
                .send()
                .map_err(|_| StorageError::Transaction)?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(StorageError::Transaction);
            }
            if response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .is_some_and(|value| value == RETIRED_CONTENT_TYPE)
            {
                return Ok(None);
            }
            body(
                response,
                usize::try_from(MAX_RESUME_RESPONSE).map_err(|_| StorageError::Transaction)?,
            )
            .map(Some)
        })
    }

    fn put(&self, key: String, bytes: Vec<u8>) -> Result<(), StorageError> {
        if self.put_if_absent(key, bytes)? {
            Ok(())
        } else {
            Err(StorageError::Transaction)
        }
    }

    fn put_if_absent(&self, key: String, bytes: Vec<u8>) -> Result<bool, StorageError> {
        if bytes.len() as u64 > MAX_RESUME_RESPONSE {
            return Err(StorageError::Transaction);
        }
        self.io.run(move |client| {
            client.authorize()?;
            let response = client
                .http
                .post(client.url(&key, true)?)
                .query(&[
                    ("uploadType", "media"),
                    ("name", &key),
                    ("ifGenerationMatch", "0"),
                ])
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .bearer_auth(client.token.as_str())
                .body(bytes)
                .send()
                .map_err(|_| StorageError::Transaction)?;
            if response.status() == StatusCode::PRECONDITION_FAILED {
                return Ok(false);
            }
            if !response.status().is_success() {
                return Err(StorageError::Transaction);
            }
            Ok(true)
        })
    }

    fn delete_if_present(&self, key: &str) -> Result<bool, StorageError> {
        let key = key.to_owned();
        self.io.run(move |client| {
            let Some(metadata) = client.metadata(&key)? else {
                return Ok(false);
            };
            if metadata.content_type == RETIRED_CONTENT_TYPE {
                return Ok(false);
            }
            let response = client
                .http
                .request(Method::DELETE, client.url(&key, false)?)
                .query(&[("ifGenerationMatch", metadata.generation)])
                .bearer_auth(client.token.as_str())
                .send()
                .map_err(|_| StorageError::Transaction)?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(false);
            }
            if !response.status().is_success() {
                return Err(StorageError::Transaction);
            }
            Ok(true)
        })
    }

    fn contains(&self, key: &str) -> Result<bool, StorageError> {
        let key = key.to_owned();
        self.io.run(move |client| {
            client
                .metadata(&key)
                .map(|value| value.is_some_and(|value| value.content_type != RETIRED_CONTENT_TYPE))
        })
    }
}

impl FirebaseObjectStore for FirebaseStorage {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::TcpListener,
        sync::{Arc, Mutex},
    };

    fn mock(
        replies: Vec<(u16, &'static str)>,
    ) -> (
        FirebaseStorage,
        Arc<Mutex<Vec<String>>>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("mock address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let thread = std::thread::spawn(move || {
            for (status, response_body) in replies {
                let (mut stream, _) = listener.accept().expect("mock connection");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("read deadline");
                let mut bytes = Vec::new();
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).expect("request header");
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break;
                    }
                    assert!(bytes.len() < 16384);
                }
                let headers = String::from_utf8(bytes).expect("headers");
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut request_body = vec![0; length];
                stream.read_exact(&mut request_body).expect("request body");
                recorded.lock().expect("record").push(headers);
                write!(stream, "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}", response_body.len()).expect("response");
            }
        });
        let io = IoWorker::start(move || {
            Ok(StorageClient {
                http: Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(5))
                    .build()
                    .expect("HTTP"),
                bucket: "test-bucket".into(),
                endpoint,
                email: "unused".into(),
                key: jsonwebtoken::EncodingKey::from_secret(b"unused"),
                key_id: "unused".into(),
                token: Zeroizing::new("mock-token".into()),
                token_until: u64::MAX,
            })
        })
        .expect("worker");
        (FirebaseStorage { io }, requests, thread)
    }

    #[test]
    fn conditional_creates_and_generation_fenced_delete_match_gcs_contract() {
        let (store, requests, thread) = mock(vec![
            (200, "{}"),
            (412, "{}"),
            (200, "save"),
            (200, "{\"generation\":\"7\"}"),
            (412, "{}"),
            (404, "{}"),
        ]);
        assert!(
            store
                .put_if_absent("characters/a/save.bin".into(), b"save".to_vec())
                .expect("created")
        );
        assert!(
            !store
                .put_if_absent("characters/a/save.bin".into(), b"different".to_vec())
                .expect("immutable conflict")
        );
        assert_eq!(
            store.get("characters/a/save.bin").expect("get"),
            Some(b"save".to_vec())
        );
        assert_eq!(
            store.delete_if_present("characters/a/save.bin"),
            Err(StorageError::Transaction)
        );
        assert!(
            !store
                .delete_if_present("characters/a/save.bin")
                .expect("already absent")
        );
        thread.join().expect("mock completed");
        let requests = requests.lock().expect("requests");
        assert!(requests[0].starts_with("POST /upload/storage/v1/b/test-bucket/o?"));
        assert!(requests[0].contains("ifGenerationMatch=0"));
        assert!(requests[1].contains("ifGenerationMatch=0"));
        assert!(requests[2].contains("characters%2Fa%2Fsave.bin?alt=media"));
        assert!(requests[4].starts_with("DELETE "));
        assert!(requests[4].contains("ifGenerationMatch=7"));
        assert!(requests.iter().all(|request| {
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer mock-token")
        }));
    }

    #[test]
    fn retirement_fences_cannot_be_overwritten_or_deleted() {
        let marker = "{\"generation\":\"8\",\"contentType\":\"application/x-hoenn-retired\"}";
        let (store, requests, thread) = mock(vec![
            (200, "{}"),
            (412, "{}"),
            (200, marker),
            (200, marker),
            (412, "{}"),
            (200, marker),
        ]);
        assert!(store.retire_if_absent("abandoned").expect("seal"));
        assert!(
            !store
                .put_if_absent("abandoned".into(), vec![1])
                .expect("late writer loses")
        );
        assert!(!store.contains("abandoned").expect("not a live artifact"));
        assert!(
            !store
                .delete_if_present("abandoned")
                .expect("marker protected")
        );
        assert!(
            store
                .retire_if_absent("abandoned")
                .expect("idempotent retirement")
        );
        thread.join().expect("mock complete");
        let requests = requests.lock().expect("requests");
        assert!(requests[0].contains("ifGenerationMatch=0"));
        assert!(requests[0].contains(RETIRED_CONTENT_TYPE));
        assert!(
            requests
                .iter()
                .all(|request| !request.starts_with("DELETE "))
        );
    }

    #[test]
    fn authorization_failures_and_redirects_do_not_become_missing_objects() {
        let (store, _, thread) = mock(vec![
            (403, "private diagnostic"),
            (302, "redirect"),
            (404, "{}"),
        ]);
        assert_eq!(store.get("object"), Err(StorageError::Transaction));
        assert_eq!(store.contains("object"), Err(StorageError::Transaction));
        assert_eq!(store.get("object"), Ok(None));
        thread.join().expect("mock completed");
    }
}

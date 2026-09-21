#[cfg(windows)]
#[path = "../src/config.rs"]
mod config;
#[cfg(windows)]
#[path = "../src/release_client.rs"]
mod release_client;
#[cfg(windows)]
#[path = "../src/backend.rs"]
mod backend;

use coop_launcher::TrustedReleaseKey;
use release_client::{ReleaseClient, ReleaseError};

#[test]
fn release_client_rejects_non_https_authority() {
    let key = TrustedReleaseKey::new("test", [1_u8; 32]);
    if let Ok(key) = key {
        assert_eq!(
            ReleaseClient::new("file:///release", key).unwrap_err(),
            ReleaseError::InvalidEndpoint
        );
    }
}

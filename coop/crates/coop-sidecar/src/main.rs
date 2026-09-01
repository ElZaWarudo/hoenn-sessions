use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Write},
};

use coop_sidecar::LocalSidecar;
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
enum CliError {
    #[error("usage: coop-sidecar --session-epoch <nonzero-u32>")]
    Usage,
    #[error("--session-epoch must be a nonzero unsigned 32-bit integer")]
    InvalidSessionEpoch,
}

fn parse_session_epoch(arguments: impl IntoIterator<Item = OsString>) -> Result<u32, CliError> {
    let mut arguments = arguments.into_iter();
    let Some(flag) = arguments.next() else {
        return Err(CliError::Usage);
    };
    let Some(value) = arguments.next() else {
        return Err(CliError::Usage);
    };
    if flag != OsStr::new("--session-epoch") || arguments.next().is_some() {
        return Err(CliError::Usage);
    }

    value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|epoch| *epoch != 0)
        .ok_or(CliError::InvalidSessionEpoch)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_epoch = parse_session_epoch(env::args_os().skip(1))?;
    let server = LocalSidecar::bind_with_epoch(session_epoch).await?;
    let descriptor = serde_json::to_string(&server.session_descriptor())?;

    // This is the sole intentional disclosure of the per-process secret.
    println!("{descriptor}");
    io::stdout().flush()?;

    server.serve().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn session_epoch_argument_accepts_a_nonzero_u32() {
        assert_eq!(
            parse_session_epoch(arguments(&["--session-epoch", "4294967295"])),
            Ok(u32::MAX)
        );
    }

    #[test]
    fn session_epoch_argument_rejects_missing_zero_malformed_and_extra_values() {
        assert_eq!(parse_session_epoch(arguments(&[])), Err(CliError::Usage));
        assert_eq!(
            parse_session_epoch(arguments(&["--session-epoch", "0"])),
            Err(CliError::InvalidSessionEpoch)
        );
        assert_eq!(
            parse_session_epoch(arguments(&["--session-epoch", "not-a-number"])),
            Err(CliError::InvalidSessionEpoch)
        );
        assert_eq!(
            parse_session_epoch(arguments(&["--session-epoch", "1", "extra"])),
            Err(CliError::Usage)
        );
        assert_eq!(
            parse_session_epoch(arguments(&["--other", "1"])),
            Err(CliError::Usage)
        );
    }
}

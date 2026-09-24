use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Write},
};

use coop_sidecar::LocalSidecar;
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
enum CliError {
    #[error("usage: coop-sidecar --session-epoch <nonzero-u32> | --arrival-verifier")]
    Usage,
    #[error("--session-epoch must be a nonzero unsigned 32-bit integer")]
    InvalidSessionEpoch,
}

#[derive(Debug, Eq, PartialEq)]
enum CliMode {
    Gameplay(u32),
    ArrivalVerifier,
}

fn parse_mode(arguments: impl IntoIterator<Item = OsString>) -> Result<CliMode, CliError> {
    let mut arguments = arguments.into_iter();
    let Some(flag) = arguments.next() else {
        return Err(CliError::Usage);
    };
    if flag == OsStr::new("--arrival-verifier") {
        return if arguments.next().is_none() {
            Ok(CliMode::ArrivalVerifier)
        } else {
            Err(CliError::Usage)
        };
    }
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
        .map(CliMode::Gameplay)
        .ok_or(CliError::InvalidSessionEpoch)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = match parse_mode(env::args_os().skip(1))? {
        CliMode::Gameplay(session_epoch) => LocalSidecar::bind_with_epoch(session_epoch).await?,
        CliMode::ArrivalVerifier => LocalSidecar::bind_arrival_verifier().await?,
    };
    let descriptor = server.session_descriptor().to_bounded_json_line()?;

    // This is the sole intentional disclosure of the per-process secret.
    io::stdout().write_all(&descriptor)?;
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
            parse_mode(arguments(&["--session-epoch", "4294967295"])),
            Ok(CliMode::Gameplay(u32::MAX))
        );
    }

    #[test]
    fn session_epoch_argument_rejects_missing_zero_malformed_and_extra_values() {
        assert_eq!(parse_mode(arguments(&[])), Err(CliError::Usage));
        assert_eq!(
            parse_mode(arguments(&["--session-epoch", "0"])),
            Err(CliError::InvalidSessionEpoch)
        );
        assert_eq!(
            parse_mode(arguments(&["--session-epoch", "not-a-number"])),
            Err(CliError::InvalidSessionEpoch)
        );
        assert_eq!(
            parse_mode(arguments(&["--session-epoch", "1", "extra"])),
            Err(CliError::Usage)
        );
        assert_eq!(
            parse_mode(arguments(&["--other", "1"])),
            Err(CliError::Usage)
        );
        assert_eq!(
            parse_mode(arguments(&["--arrival-verifier"])),
            Ok(CliMode::ArrivalVerifier)
        );
        assert_eq!(
            parse_mode(arguments(&["--arrival-verifier", "0"])),
            Err(CliError::Usage)
        );
    }
}

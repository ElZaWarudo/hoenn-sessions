//! Release-time verifier for a ROM-written first-arrival Flash1M image.

use sha2::{Digest, Sha256};
use std::path::Path;

fn run() -> Result<(), &'static str> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("expected save path")?;
    let expected_digest = args.next().ok_or("expected save digest")?;
    let expected_digest = expected_digest.to_str().ok_or("invalid save digest")?;
    if expected_digest.len() != 64
        || !expected_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("invalid save digest");
    }
    let parse_byte = |value: Option<std::ffi::OsString>| -> Result<u8, &'static str> {
        value
            .ok_or("expected map coordinate")?
            .to_str()
            .and_then(|text| text.parse::<u8>().ok())
            .ok_or("invalid map coordinate")
    };
    let expected = [
        parse_byte(args.next())?,
        parse_byte(args.next())?,
        parse_byte(args.next())?,
    ];
    if args.next().is_some() {
        return Err("unexpected argument");
    }
    let bytes = std::fs::read(Path::new(&path)).map_err(|_| "arrival save unreadable")?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected_digest {
        return Err("arrival save digest changed during validation");
    }
    let registry = coop_save::RegistryContract::new(
        coop_protocol::IDENTITY_REGISTRY_VERSION,
        coop_protocol::IDENTITY_REGISTRY_DIGEST,
    );
    let save = coop_save::parse_v2(&bytes, registry).map_err(|_| "invalid V2 arrival save")?;
    if !save.coop().online_eligible() {
        return Err("arrival save is not online eligible");
    }
    let local = save
        .logical_sector_payload(1)
        .ok_or("arrival save has no map state")?;
    if local.get(4..7) != Some(expected.as_slice()) {
        return Err("arrival save map does not match catalog");
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("arrival save: {reason}");
            std::process::ExitCode::FAILURE
        }
    }
}

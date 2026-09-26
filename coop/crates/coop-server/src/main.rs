use std::{net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mode, address, release_fixture_root) = runtime_config()?;
    match mode {
        ServerMode::Phase1 => {
            let listener = tokio::net::TcpListener::bind(address).await?;
            axum::serve(listener, coop_server::app()).await?;
        }
        ServerMode::Phase2Local => {
            coop_server::phase2::serve_phase2_local_with_release_fixture_root(
                address,
                release_fixture_root,
            )
            .await?;
        }
        ServerMode::PostgresFirebase => {
            coop_server::phase2::production::serve_phase2_production(address).await?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerMode {
    Phase1,
    Phase2Local,
    PostgresFirebase,
}

fn server_mode(value: &str) -> Result<ServerMode, Box<dyn std::error::Error>> {
    match value.to_ascii_lowercase().as_str() {
        "phase1" => Ok(ServerMode::Phase1),
        "phase2-local" => Ok(ServerMode::Phase2Local),
        "postgres-firebase" | "production" => Ok(ServerMode::PostgresFirebase),
        _ => Err(format!("unknown coop-server mode: {value}").into()),
    }
}

fn runtime_config() -> Result<(ServerMode, SocketAddr, Option<PathBuf>), Box<dyn std::error::Error>>
{
    let mut arguments = std::env::args().skip(1);
    let environment_mode = match std::env::var("COOP_SERVER_MODE") {
        Ok(value) => Some(server_mode(&value)?),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("COOP_SERVER_MODE is not valid UTF-8".into());
        }
    };
    let mut mode = environment_mode.unwrap_or(ServerMode::Phase1);
    let environment_bind = match std::env::var("COOP_SERVER_BIND_ADDR") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("COOP_SERVER_BIND_ADDR is not valid UTF-8".into());
        }
    };
    let mut configured = environment_bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1:3000".to_owned());
    let mut bind_seen = environment_bind.is_some();
    let mut release_fixture_root =
        std::env::var_os("COOP_PHASE2_RELEASE_FIXTURE_ROOT").map(PathBuf::from);
    let mut fixture_seen = release_fixture_root.is_some();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--phase1" => mode = requested_mode(environment_mode, ServerMode::Phase1)?,
            "--phase2-local" => mode = requested_mode(environment_mode, ServerMode::Phase2Local)?,
            "--mode" => {
                let value = arguments
                    .next()
                    .ok_or("--mode requires phase1, phase2-local, or postgres-firebase")?;
                mode = requested_mode(environment_mode, server_mode(&value)?)?;
            }
            "--bind" => {
                if bind_seen {
                    return Err("only one bind address may be supplied".into());
                }
                configured = arguments
                    .next()
                    .ok_or("--bind requires an address such as 127.0.0.1:3000")?;
                bind_seen = true;
            }
            "--release-fixture-root" => {
                if fixture_seen {
                    return Err("only one release fixture root may be supplied".into());
                }
                release_fixture_root = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--release-fixture-root requires a path")?,
                ));
                fixture_seen = true;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown option: {value}").into());
            }
            value => {
                if bind_seen {
                    return Err("only one bind address may be supplied".into());
                }
                value.clone_into(&mut configured);
                bind_seen = true;
            }
        }
    }
    let address = if mode == ServerMode::PostgresFirebase {
        configured.parse()?
    } else {
        bind_address_from(&configured)?
    };
    validate_release_fixture(mode, address, release_fixture_root.as_deref())?;
    Ok((mode, address, release_fixture_root))
}

fn validate_release_fixture(
    mode: ServerMode,
    address: SocketAddr,
    root: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(root) = root {
        if mode != ServerMode::Phase2Local || !address.ip().is_loopback() {
            return Err("release fixture root requires phase2-local on a loopback bind".into());
        }
        if root.as_os_str().is_empty() {
            return Err("release fixture root cannot be empty".into());
        }
    }
    Ok(())
}

fn requested_mode(
    environment_mode: Option<ServerMode>,
    requested: ServerMode,
) -> Result<ServerMode, Box<dyn std::error::Error>> {
    if let Some(environment) = environment_mode
        && environment != requested
    {
        return Err("command-line server mode conflicts with COOP_SERVER_MODE".into());
    }
    Ok(requested)
}

fn bind_address_from(configured: &str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let address = configured.parse::<SocketAddr>()?;
    if !address.ip().is_loopback() {
        return Err(
            "coop-server is an unauthenticated dev adapter and only accepts loopback binds".into(),
        );
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthenticated_server_requires_loopback_bind() {
        assert!(bind_address_from("127.0.0.1:3000").is_ok());
        assert!(bind_address_from("[::1]:3000").is_ok());
        assert!(bind_address_from("0.0.0.0:3000").is_err());
        assert!(bind_address_from("192.0.2.1:3000").is_err());
    }

    #[test]
    fn runtime_modes_are_explicit_and_fail_closed() {
        assert_eq!(server_mode("phase1").expect("phase1"), ServerMode::Phase1);
        assert_eq!(
            server_mode("phase2-local").expect("phase2 local"),
            ServerMode::Phase2Local
        );
        assert_eq!(
            server_mode("postgres-firebase").expect("persistent"),
            ServerMode::PostgresFirebase
        );
        assert!(server_mode("unknown").is_err());
        assert_eq!(
            requested_mode(Some(ServerMode::Phase1), ServerMode::Phase1).expect("same mode"),
            ServerMode::Phase1
        );
        assert!(requested_mode(Some(ServerMode::Phase1), ServerMode::Phase2Local).is_err());
        assert!(requested_mode(Some(ServerMode::Phase2Local), ServerMode::Phase1).is_err());
    }

    #[test]
    fn release_fixture_is_local_loopback_only() {
        let root = std::path::Path::new("fixture");
        assert!(
            validate_release_fixture(
                ServerMode::Phase2Local,
                "127.0.0.1:3000".parse().unwrap(),
                Some(root)
            )
            .is_ok()
        );
        assert!(
            validate_release_fixture(
                ServerMode::Phase2Local,
                "[::1]:3000".parse().unwrap(),
                Some(root)
            )
            .is_ok()
        );
        assert!(
            validate_release_fixture(
                ServerMode::Phase2Local,
                "0.0.0.0:3000".parse().unwrap(),
                Some(root)
            )
            .is_err()
        );
        assert!(
            validate_release_fixture(
                ServerMode::Phase1,
                "127.0.0.1:3000".parse().unwrap(),
                Some(root)
            )
            .is_err()
        );
        assert!(
            validate_release_fixture(
                ServerMode::PostgresFirebase,
                "127.0.0.1:3000".parse().unwrap(),
                Some(root)
            )
            .is_err()
        );
        assert!(
            validate_release_fixture(
                ServerMode::Phase2Local,
                "127.0.0.1:3000".parse().unwrap(),
                Some(std::path::Path::new(""))
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn local_fixture_server_rejects_non_loopback_before_loading_secrets() {
        let address = "0.0.0.0:0".parse().expect("valid address");
        let result = coop_server::phase2::serve_phase2_local_with_release_fixture_root(
            address,
            Some(PathBuf::from("fixture")),
        )
        .await;
        assert!(result.is_err());
    }
}

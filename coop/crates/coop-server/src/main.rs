use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = bind_address()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, coop_server::app()).await?;
    Ok(())
}

fn bind_address() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let configured = match arguments.next().as_deref() {
        Some("--bind") => arguments
            .next()
            .ok_or("--bind requires an address such as 127.0.0.1:3000")?,
        Some(argument) if argument.starts_with("--") => {
            return Err(format!("unknown option: {argument}").into());
        }
        Some(argument) => argument.to_owned(),
        None => {
            std::env::var("COOP_SERVER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
        }
    };
    bind_address_from(&configured)
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
}

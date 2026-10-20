//! `genesy-360-api healthcheck`: probe own /healthz over raw HTTP/1.1. The distroless image has
//! no shell, curl or wget, so the compose healthcheck runs the binary itself.

use std::time::Duration;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

/// Docker's healthcheck timeout is the outer bound; stay well under it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// True when `127.0.0.1:{port}/healthz` answers 200 within the timeout.
pub async fn probe(port: u16) -> bool {
    timeout(PROBE_TIMEOUT, status_line(port))
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|line| line.starts_with("HTTP/1.1 200"))
}

async fn status_line(port: u16) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut head = [0u8; 64];
    let n = stream.read(&mut head).await?;
    Ok(String::from_utf8_lossy(&head[..n]).into_owned())
}

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use crate::paths;

/// Fire-and-forget notification to the daemon to process pending deletions.
/// Silently ignores errors (daemon may not be running).
pub fn notify_daemon() {
    let socket_path = paths::socket_path(None);
    let _ = send_post(&socket_path, "/tasks/process-deletions");
}

/// Fire-and-forget notification to the daemon to replenish the pool.
/// Silently ignores errors (daemon may not be running).
pub fn notify_pool_replenish() {
    let socket_path = paths::socket_path(None);
    let _ = send_post(&socket_path, "/pool/replenish");
}

fn send_post(socket_path: &std::path::Path, path: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut buf = [0u8; 128];
    let _ = stream.read(&mut buf)?;
    Ok(())
}

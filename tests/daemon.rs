mod common;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tempfile::TempDir;

use common::{DaemonFixture, wait_for_path, wait_for_path_removed};

fn work_bin() -> &'static str {
    env!("CARGO_BIN_EXE_work")
}

fn http_request(sock: &std::path::Path, request: &str) -> String {
    let mut stream = UnixStream::connect(sock).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

// --- Lifecycle tests ---

#[test]
fn daemon_start_creates_runtime_files() {
    let d = DaemonFixture::start();

    assert!(d.socket_path().exists());
    assert!(d.pid_path().exists());

    let pid_str = std::fs::read_to_string(d.pid_path()).unwrap();
    let pid: i32 = pid_str.trim().parse().unwrap();
    assert_eq!(pid, d.pid());
}

#[test]
fn daemon_clean_shutdown_on_sigterm() {
    let tmp = TempDir::new().unwrap();

    let mut child = std::process::Command::new(work_bin())
        .env("WORK_HOME", tmp.path())
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_RUNTIME_DIR")
        .args(["daemon", "start"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let sock = tmp.path().join("runtime/work.sock");
    let pid_file = tmp.path().join("runtime/work.pid");
    assert!(wait_for_path(&sock, Duration::from_secs(5)));

    signal::kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "daemon exited with non-zero status: {status}"
    );

    assert!(
        wait_for_path_removed(&sock, Duration::from_secs(5)),
        "socket file not cleaned up"
    );
    assert!(!pid_file.exists(), "PID file not cleaned up");
}

#[test]
fn daemon_start_refuses_if_already_running() {
    let d = DaemonFixture::start();

    let output = d
        .cmd()
        .args(["daemon", "start"])
        .output()
        .expect("failed to run second daemon");

    assert!(!output.status.success(), "second daemon should have failed");
}

#[test]
fn daemon_start_force_overrides_existing_files() {
    let tmp = TempDir::new().unwrap();

    // Create stale runtime files.
    let runtime = tmp.path().join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::write(runtime.join("work.pid"), "99999").unwrap();
    std::fs::write(runtime.join("work.sock"), "").unwrap();

    let mut child = std::process::Command::new(work_bin())
        .env("WORK_HOME", tmp.path())
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_RUNTIME_DIR")
        .args(["daemon", "start", "--force"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn daemon with --force");

    let pid_file = tmp.path().join("runtime/work.pid");
    let expected_pid = child.id().to_string();
    let start = Instant::now();
    let mut matched = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(contents) = std::fs::read_to_string(&pid_file)
            && contents.trim() == expected_pid
        {
            matched = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(matched, "PID file never updated to new daemon PID");

    signal::kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();
    child.wait().unwrap();
}

// --- API tests ---

#[test]
fn api_health() {
    let d = DaemonFixture::start();
    let resp = http_request(
        &d.socket_path(),
        "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("200"), "expected 200, got: {resp}");
    assert!(
        resp.contains(r#"{"status":"ok"}"#),
        "expected health JSON, got: {resp}"
    );
}

#[test]
fn api_projects_crud() {
    let d = DaemonFixture::start();
    let sock = d.socket_path();

    // List should be empty.
    let resp = http_request(
        &sock,
        "GET /projects HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("200"));
    assert!(resp.contains("[]"));

    // Create a project.
    let body = r#"{"name":"myproj","path":"/tmp/myproj"}"#;
    let req = format!(
        "POST /projects HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let resp = http_request(&sock, &req);
    assert!(resp.contains("201"), "expected 201, got: {resp}");

    // List should have one project.
    let resp = http_request(
        &sock,
        "GET /projects HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("200"));
    assert!(resp.contains("myproj"));

    // Delete the project.
    let resp = http_request(
        &sock,
        "DELETE /projects/myproj HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("204"), "expected 204, got: {resp}");

    // List should be empty again.
    let resp = http_request(
        &sock,
        "GET /projects HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("[]"));
}

#[test]
fn api_delete_nonexistent_returns_404() {
    let d = DaemonFixture::start();
    let resp = http_request(
        &d.socket_path(),
        "DELETE /projects/nope HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("404"), "expected 404, got: {resp}");
}

#[test]
fn api_create_duplicate_returns_409() {
    let d = DaemonFixture::start();
    let sock = d.socket_path();

    let body = r#"{"name":"dup","path":"/tmp/a"}"#;
    let req = format!(
        "POST /projects HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let resp = http_request(&sock, &req);
    assert!(resp.contains("201"));

    let body2 = r#"{"name":"dup","path":"/tmp/b"}"#;
    let req2 = format!(
        "POST /projects HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body2.len(),
        body2
    );
    let resp = http_request(&sock, &req2);
    assert!(resp.contains("409"), "expected 409, got: {resp}");
}

#[test]
fn api_reset_database() {
    let d = DaemonFixture::start();
    let sock = d.socket_path();

    // Create a project.
    let body = r#"{"name":"gone","path":"/tmp/gone"}"#;
    let req = format!(
        "POST /projects HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    http_request(&sock, &req);

    // Reset.
    let resp = http_request(
        &sock,
        "POST /reset-database HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("204"), "expected 204, got: {resp}");

    // List should be empty.
    let resp = http_request(
        &sock,
        "GET /projects HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(resp.contains("[]"));
}

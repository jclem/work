#![allow(dead_code)]

use std::path::Path;
use std::time::{Duration, Instant};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tempfile::TempDir;

fn work_bin() -> &'static str {
    env!("CARGO_BIN_EXE_work")
}

pub fn wait_for_path(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

pub fn wait_for_path_removed(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A running daemon process backed by a temporary directory.
/// Sends SIGTERM and waits for exit on drop.
pub struct DaemonFixture {
    child: std::process::Child,
    pub work_dir: TempDir,
}

impl DaemonFixture {
    pub fn start() -> Self {
        let tmp = TempDir::new().unwrap();

        let child = std::process::Command::new(work_bin())
            .env("WORK_HOME", tmp.path())
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_RUNTIME_DIR")
            .args(["daemon", "start"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn daemon");

        let fixture = Self {
            child,
            work_dir: tmp,
        };

        let sock = fixture.socket_path();
        assert!(
            wait_for_path(&sock, Duration::from_secs(5)),
            "daemon socket not created"
        );

        fixture
    }

    pub fn socket_path(&self) -> std::path::PathBuf {
        self.work_dir.path().join("runtime/work.sock")
    }

    pub fn pid_path(&self) -> std::path::PathBuf {
        self.work_dir.path().join("runtime/work.pid")
    }

    pub fn pid(&self) -> i32 {
        self.child.id() as i32
    }

    /// Build a CLI command that talks to this daemon's work home.
    pub fn cmd(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(work_bin());
        cmd.env("WORK_HOME", self.work_dir.path());
        cmd.env_remove("XDG_DATA_HOME");
        cmd.env_remove("XDG_RUNTIME_DIR");
        cmd
    }

    /// Build an assert_cmd::Command that talks to this daemon's work home.
    pub fn assert_cmd(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(work_bin());
        cmd.env("WORK_HOME", self.work_dir.path());
        cmd.env_remove("XDG_DATA_HOME");
        cmd.env_remove("XDG_RUNTIME_DIR");
        cmd
    }
}

impl Drop for DaemonFixture {
    fn drop(&mut self) {
        let _ = signal::kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        let _ = self.child.wait();
    }
}

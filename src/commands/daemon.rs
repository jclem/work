use std::fs;
use std::path::PathBuf;
use std::process;

use crate::cli::DaemonCommand;
use crate::error::{self, CliError};
use crate::logger::Logger;
use crate::paths;
use crate::workd::Workd;

pub async fn execute(command: DaemonCommand, logger: Logger) -> Result<(), CliError> {
    match command {
        DaemonCommand::Start(args) => {
            if let Ok(pid) = read_pid()
                && is_process_alive(pid)
            {
                if args.force {
                    kill_pid(pid)?;
                    error::print_success(&format!("stopped daemon (pid {pid})"));
                } else {
                    return Err(CliError::with_hint(
                        format!("daemon is already running (pid {pid})"),
                        "use `work daemon start --force` to replace it",
                    ));
                }
            }

            if args.detach {
                daemonize(args.socket)
            } else {
                Workd::start(logger, args.socket).await
            }
        }
        DaemonCommand::SocketPath(args) => {
            println!("{}", paths::socket_path(args.socket).display());
            Ok(())
        }
        DaemonCommand::Pid => {
            println!("{}", read_pid()?);
            Ok(())
        }
        DaemonCommand::Stop => {
            let pid = read_pid()?;
            kill_pid(pid)?;
            error::print_success(&format!("stopped daemon (pid {pid})"));
            Ok(())
        }
        DaemonCommand::Install => super::daemon_install::install(),
        DaemonCommand::Uninstall => super::daemon_install::uninstall(),
        DaemonCommand::Restart(args) => {
            if let Ok(pid) = read_pid() {
                kill_pid(pid)?;
                error::print_success(&format!("stopped daemon (pid {pid})"));
            }
            daemonize(args.socket)
        }
    }
}

fn daemonize(socket: Option<PathBuf>) -> Result<(), CliError> {
    let exe = std::env::current_exe()
        .map_err(|e| CliError::with_source("failed to resolve executable path", e))?;

    let log_path = paths::daemon_log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CliError::with_source(format!("failed to create {}", parent.display()), e)
        })?;
    }

    let log_file = fs::File::create(&log_path)
        .map_err(|e| CliError::with_source("failed to create daemon log file", e))?;

    let mut cmd = process::Command::new(exe);
    cmd.args(["daemon", "start"]);

    if let Some(ref s) = socket {
        cmd.arg("--socket").arg(s);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::from(log_file));

    let child = cmd
        .spawn()
        .map_err(|e| CliError::with_source("failed to spawn daemon process", e))?;

    error::print_success(&format!(
        "daemon started (pid {}, log {})",
        child.id(),
        log_path.display()
    ));

    Ok(())
}

fn read_pid() -> Result<u32, CliError> {
    let pid_path = paths::pid_file_path();
    let content = fs::read_to_string(&pid_path).map_err(|_| {
        CliError::with_hint(
            "daemon is not running",
            "start the daemon with `work daemon start`",
        )
    })?;

    content
        .trim()
        .parse::<u32>()
        .map_err(|_| CliError::new("invalid PID file contents"))
}

fn is_process_alive(pid: u32) -> bool {
    // kill -0 checks if the process exists without sending a signal.
    process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn kill_pid(pid: u32) -> Result<(), CliError> {
    let status = process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map_err(|source| CliError::with_source("failed to execute kill", source))?;

    if !status.success() {
        // Process likely doesn't exist; clean up stale PID file.
        let _ = fs::remove_file(paths::pid_file_path());
    }

    Ok(())
}

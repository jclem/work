#[cfg(target_os = "macos")]
mod platform {
    use std::fs;
    use std::process;

    use crate::error::{self, CliError};
    use crate::paths;

    const LABEL: &str = "com.jclem.work";

    fn plist_path() -> Result<std::path::PathBuf, CliError> {
        let home = std::env::var("HOME")
            .map_err(|_| CliError::new("HOME environment variable is not set"))?;
        Ok(std::path::PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn uid() -> Result<u32, CliError> {
        // SAFETY: getuid is always safe to call.
        let uid = unsafe { libc::getuid() };
        Ok(uid)
    }

    pub fn install() -> Result<(), CliError> {
        let exe = std::env::current_exe()
            .map_err(|e| CliError::with_source("failed to resolve executable path", e))?;

        let log_path = paths::daemon_log_path();
        let plist_path = plist_path()?;

        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CliError::with_source(format!("failed to create {}", parent.display()), e)
            })?;
        }

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>daemon</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
            exe = exe.display(),
            log = log_path.display(),
        );

        fs::write(&plist_path, plist).map_err(|e| {
            CliError::with_source(format!("failed to write {}", plist_path.display()), e)
        })?;

        let uid = uid()?;
        let status = process::Command::new("launchctl")
            .args([
                "bootstrap",
                &format!("gui/{uid}"),
                &plist_path.to_string_lossy(),
            ])
            .status()
            .map_err(|e| CliError::with_source("failed to run launchctl bootstrap", e))?;

        if !status.success() {
            return Err(CliError::new("launchctl bootstrap failed"));
        }

        error::print_success(&format!("installed launch agent: {}", plist_path.display()));
        Ok(())
    }

    pub fn uninstall() -> Result<(), CliError> {
        let plist_path = plist_path()?;

        if !plist_path.exists() {
            return Err(CliError::new("launch agent is not installed"));
        }

        let uid = uid()?;
        let status = process::Command::new("launchctl")
            .args([
                "bootout",
                &format!("gui/{uid}"),
                &plist_path.to_string_lossy(),
            ])
            .status()
            .map_err(|e| CliError::with_source("failed to run launchctl bootout", e))?;

        if !status.success() {
            return Err(CliError::new("launchctl bootout failed"));
        }

        fs::remove_file(&plist_path).map_err(|e| {
            CliError::with_source(format!("failed to remove {}", plist_path.display()), e)
        })?;

        error::print_success("uninstalled launch agent");
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::error::CliError;

    pub fn install() -> Result<(), CliError> {
        Err(CliError::new(
            "daemon install is not supported on this platform",
        ))
    }

    pub fn uninstall() -> Result<(), CliError> {
        Err(CliError::new(
            "daemon uninstall is not supported on this platform",
        ))
    }
}

pub fn install() -> Result<(), crate::error::CliError> {
    platform::install()
}

pub fn uninstall() -> Result<(), crate::error::CliError> {
    platform::uninstall()
}

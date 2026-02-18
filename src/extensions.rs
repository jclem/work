use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::config::Config;
use crate::error::CliError;

/// Resolve the binary path for a named extension backend.
pub fn resolve_extension(config: &Config, backend: &str) -> Result<PathBuf, CliError> {
    let extensions = config.extensions.as_ref().ok_or_else(|| {
        CliError::with_hint(
            format!("unknown backend '{backend}'"),
            "configure extensions in ~/.config/work/config.toml under [extensions.<name>]",
        )
    })?;

    let ext = extensions.get(backend).ok_or_else(|| {
        CliError::with_hint(
            format!("unknown backend '{backend}'"),
            "configure extensions in ~/.config/work/config.toml under [extensions.<name>]",
        )
    })?;

    let path = PathBuf::from(&ext.binary);

    if !path.exists() {
        return Err(CliError::with_hint(
            format!("extension binary not found: {}", path.display()),
            format!("check the binary path in [extensions.{backend}]"),
        ));
    }

    Ok(path)
}

#[derive(Debug, Deserialize)]
pub struct CreateResponse {
    pub path: String,
}

/// Invoke an extension's `create` command.
pub fn invoke_create(
    binary: &Path,
    name: &str,
    project_name: &str,
    project_path: &str,
) -> Result<CreateResponse, CliError> {
    let output = Command::new(binary)
        .args([
            "create",
            name,
            "--project-name",
            project_name,
            "--project-path",
            project_path,
        ])
        .output()
        .map_err(|source| {
            CliError::with_source(
                format!("failed to run extension {}", binary.display()),
                source,
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::new(format!(
            "extension create failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|source| {
        CliError::with_source("failed to parse extension create response", source)
    })
}

/// Invoke an extension's `destroy` command.
pub fn invoke_destroy(binary: &Path, name: &str) -> Result<(), CliError> {
    let output = Command::new(binary)
        .args(["destroy", name])
        .output()
        .map_err(|source| {
            CliError::with_source(
                format!("failed to run extension {}", binary.display()),
                source,
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::new(format!(
            "extension destroy failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Invoke an extension's `rename` command.
#[allow(dead_code)]
pub fn invoke_rename(binary: &Path, old: &str, new: &str) -> Result<(), CliError> {
    let output = Command::new(binary)
        .args(["rename", old, new])
        .output()
        .map_err(|source| {
            CliError::with_source(
                format!("failed to run extension {}", binary.display()),
                source,
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::new(format!(
            "extension rename failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Invoke an extension's `warm` command. Returns the number of environments created.
pub fn invoke_warm(
    binary: &Path,
    project_name: &str,
    project_path: &str,
    count: u32,
) -> Result<u32, CliError> {
    let output = Command::new(binary)
        .args([
            "warm",
            "--project-name",
            project_name,
            "--project-path",
            project_path,
            "--count",
            &count.to_string(),
        ])
        .output()
        .map_err(|source| {
            CliError::with_source(
                format!("failed to run extension {}", binary.display()),
                source,
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::new(format!(
            "extension warm failed: {}",
            stderr.trim()
        )));
    }

    #[derive(Deserialize)]
    struct WarmResponse {
        created: u32,
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: WarmResponse = serde_json::from_str(stdout.trim()).map_err(|source| {
        CliError::with_source("failed to parse extension warm response", source)
    })?;

    Ok(resp.created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_extension_returns_error_when_no_extensions() {
        let config = Config::default();
        let result = resolve_extension(&config, "boxy");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown backend"));
    }

    #[test]
    fn resolve_extension_returns_error_for_unknown_backend() {
        use std::collections::HashMap;
        let config = Config {
            extensions: Some(HashMap::new()),
            ..Config::default()
        };
        let result = resolve_extension(&config, "boxy");
        assert!(result.is_err());
    }

    #[test]
    fn parse_create_response() {
        let json = r#"{"path":"boxy:my-task"}"#;
        let resp: CreateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.path, "boxy:my-task");
    }
}

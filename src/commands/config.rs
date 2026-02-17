use crate::cli::ConfigCommand;
use crate::error::CliError;
use crate::paths;

pub fn execute(command: ConfigCommand) -> Result<(), CliError> {
    match command {
        ConfigCommand::Edit => edit(),
    }
}

fn edit() -> Result<(), CliError> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .map_err(|_| {
            CliError::with_hint(
                "no editor configured",
                "set $EDITOR or $VISUAL in your shell environment",
            )
        })?;

    let config_dir = paths::config_dir();
    std::fs::create_dir_all(&config_dir).map_err(|source| {
        CliError::with_source(
            format!(
                "failed to create config directory: {}",
                config_dir.display()
            ),
            source,
        )
    })?;

    let config_path = paths::config_path();
    if !config_path.exists() {
        std::fs::File::create(&config_path).map_err(|source| {
            CliError::with_source(
                format!("failed to create config file: {}", config_path.display()),
                source,
            )
        })?;
    }

    let status = std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|source| {
            CliError::with_source(format!("failed to launch editor: {editor}"), source)
        })?;

    if !status.success() {
        return Err(CliError::new("editor exited with non-zero status"));
    }

    Ok(())
}

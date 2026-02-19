use std::path::PathBuf;

use crate::cli::{InstallSkillArgs, Provider};
use crate::error::{self, CliError};

const SKILL_CONTENT: &str = include_str!("../../templates/skills/work/SKILLS.md");

/// Return the list of providers to install for.
fn target_providers(provider: Option<Provider>) -> Vec<Provider> {
    match provider {
        Some(p) => vec![p],
        None => vec![Provider::Claude, Provider::Codex],
    }
}

/// Return the provider-specific directory name (e.g. ".claude", ".codex").
fn provider_dir_name(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => ".claude",
        Provider::Codex => ".codex",
    }
}

/// Return the display name for a provider.
fn provider_display_name(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

/// Resolve the base directory for skill installation.
fn skill_base_dir(provider: &Provider, global: bool) -> Result<PathBuf, CliError> {
    if global {
        let home = std::env::var("HOME")
            .map_err(|_| CliError::new("HOME environment variable is not set"))?;
        Ok(PathBuf::from(home).join(provider_dir_name(provider)))
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| CliError::with_source("failed to read current directory", e))?;
        Ok(cwd.join(provider_dir_name(provider)))
    }
}

pub fn execute(args: InstallSkillArgs) -> Result<(), CliError> {
    let providers = target_providers(args.provider);
    let scope = if args.global { "global" } else { "local" };

    for provider in &providers {
        let base = skill_base_dir(provider, args.global)?;
        let skill_dir = base.join("skills").join("work");
        let skill_path = skill_dir.join("SKILL.md");

        std::fs::create_dir_all(&skill_dir).map_err(|e| {
            CliError::with_source(
                format!("failed to create directory: {}", skill_dir.display()),
                e,
            )
        })?;

        std::fs::write(&skill_path, SKILL_CONTENT).map_err(|e| {
            CliError::with_source(
                format!("failed to write skill file: {}", skill_path.display()),
                e,
            )
        })?;

        error::print_success(&format!(
            "Installed work skill for {} ({scope}): {}",
            provider_display_name(provider),
            skill_path.display(),
        ));
    }

    Ok(())
}

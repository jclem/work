use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::CliError;
use crate::paths;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub projects: Option<HashMap<String, ProjectConfig>>,
    pub daemon: Option<DaemonConfig>,
    pub orchestrator: Option<OrchestratorConfig>,
    #[serde(rename = "default-branch")]
    pub default_branch: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub hooks: Option<HooksConfig>,
    pub orchestrator: Option<OrchestratorConfig>,
    #[serde(rename = "pool-size")]
    pub pool_size: Option<u32>,
    #[serde(rename = "default-branch")]
    pub default_branch: Option<String>,
}

fn default_max_load() -> f64 {
    0.7
}

fn default_min_memory() -> f64 {
    10.0
}

fn default_poll_interval() -> u64 {
    300
}

fn default_pool_pull_interval() -> u64 {
    3600
}

#[derive(Debug, Deserialize)]
pub struct DaemonConfig {
    #[serde(rename = "pool-max-load", default = "default_max_load")]
    pub pool_max_load: f64,
    #[serde(rename = "pool-min-memory-pct", default = "default_min_memory")]
    pub pool_min_memory_pct: f64,
    #[serde(rename = "pool-poll-interval", default = "default_poll_interval")]
    pub pool_poll_interval: u64,
    /// When true, pool worktrees are periodically pulled to stay up-to-date.
    #[serde(rename = "pool-pull-enabled", default)]
    pub pool_pull_enabled: bool,
    /// Seconds between pool pull cycles (default 3600 = 1 hour).
    #[serde(rename = "pool-pull-interval", default = "default_pool_pull_interval")]
    pub pool_pull_interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    #[serde(rename = "new-after")]
    pub new_after: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrchestratorConfig {
    #[serde(rename = "agent-command")]
    pub agent_command: Option<Vec<String>>,
    #[serde(rename = "system-prompt")]
    pub system_prompt: Option<String>,
    #[serde(rename = "max-agents-in-flight")]
    pub max_agents_in_flight: Option<u32>,
    #[serde(rename = "max-sessions-per-issue")]
    pub max_sessions_per_issue: Option<u32>,
}

pub const DEFAULT_AGENT_COMMAND: &[&str] = &[
    "claude",
    "-p",
    "--dangerously-skip-permissions",
    "--disallowedTools",
    "EnterPlanMode",
    "--system-prompt",
    "{system_prompt}",
    "{issue}",
];

pub fn load() -> Result<Config, CliError> {
    let path = paths::config_path();

    if !path.exists() {
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| {
        CliError::with_source(
            format!("failed to read config file: {}", path.display()),
            source,
        )
    })?;

    let config: Config = toml::from_str(&content).map_err(|source| {
        CliError::with_source(
            format!("failed to parse config file: {}", path.display()),
            source,
        )
    })?;

    Ok(config)
}

pub fn load_project_config(project_path: &str) -> Result<ProjectConfig, CliError> {
    let path = Path::new(project_path).join(".work/config.toml");

    if !path.exists() {
        return Ok(ProjectConfig::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| {
        CliError::with_source(
            format!("failed to read project config: {}", path.display()),
            source,
        )
    })?;

    let config: ProjectConfig = toml::from_str(&content).map_err(|source| {
        CliError::with_source(
            format!("failed to parse project config: {}", path.display()),
            source,
        )
    })?;

    Ok(config)
}

/// Returns the effective pool size for a project. Checks project-level config
/// first, then falls back to the global config. Returns 0 (no pre-warming) if
/// neither specifies a pool size.
pub fn effective_pool_size(global_config: &Config, project_name: &str, project_path: &str) -> u32 {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(size) = project_cfg.pool_size
    {
        return size;
    }

    // Fall back to global config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(size) = project_cfg.pool_size
    {
        return size;
    }

    0
}

/// Returns the effective default branch for a project. Checks project-level
/// config first, then falls back to the global config. Defaults to "main".
pub fn effective_default_branch(
    global_config: &Config,
    project_name: &str,
    project_path: &str,
) -> String {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(branch) = project_cfg.default_branch
    {
        return branch;
    }

    // Fall back to global per-project config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(branch) = &project_cfg.default_branch
    {
        return branch.clone();
    }

    // Fall back to global default-branch.
    if let Some(branch) = &global_config.default_branch {
        return branch.clone();
    }

    "main".to_string()
}

/// Returns the effective agent command for a project. Checks project-level
/// .work/config.toml first, then global per-project config, then global
/// orchestrator config. Returns a vec of [binary, args...] with placeholders
/// `{issue}`, `{system_prompt}`, and `{report_path}` to be replaced at runtime.
pub fn effective_agent_command(
    global_config: &Config,
    project_name: &str,
    project_path: &str,
) -> Vec<String> {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(orch) = &project_cfg.orchestrator
        && let Some(cmd) = &orch.agent_command
    {
        return cmd.clone();
    }

    // Fall back to global per-project config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(orch) = &project_cfg.orchestrator
        && let Some(cmd) = &orch.agent_command
    {
        return cmd.clone();
    }

    // Fall back to global orchestrator config.
    if let Some(orch) = &global_config.orchestrator
        && let Some(cmd) = &orch.agent_command
    {
        return cmd.clone();
    }

    DEFAULT_AGENT_COMMAND
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Returns the effective max agents in flight. Checks global orchestrator
/// config. Defaults to 4.
pub fn effective_max_agents(global_config: &Config) -> u32 {
    global_config
        .orchestrator
        .as_ref()
        .and_then(|o| o.max_agents_in_flight)
        .unwrap_or(4)
}

/// Returns the effective max sessions per issue. Checks global orchestrator
/// config. Defaults to 5.
pub fn effective_max_sessions_per_issue(global_config: &Config) -> u32 {
    global_config
        .orchestrator
        .as_ref()
        .and_then(|o| o.max_sessions_per_issue)
        .unwrap_or(5)
}

pub fn hook_script<'a>(config: &'a Config, project_name: &str, hook_name: &str) -> Option<&'a str> {
    let projects = config.projects.as_ref()?;
    let project = projects.get(project_name)?;
    project_hook_script(project, hook_name)
}

pub fn project_hook_script<'a>(project: &'a ProjectConfig, hook_name: &str) -> Option<&'a str> {
    let hooks = project.hooks.as_ref()?;

    match hook_name {
        "new-after" => hooks.new_after.as_deref(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_default_when_file_missing() {
        let config = Config::default();
        assert!(config.projects.is_none());
    }

    #[test]
    fn hook_script_returns_none_for_missing_project() {
        let config = Config::default();
        assert!(hook_script(&config, "no-such-project", "new-after").is_none());
    }

    #[test]
    fn hook_script_returns_script_content() {
        let config = Config {
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    hooks: Some(HooksConfig {
                        new_after: Some("echo hello".to_string()),
                    }),
                    ..ProjectConfig::default()
                },
            )])),
            ..Config::default()
        };
        assert_eq!(
            hook_script(&config, "my-project", "new-after"),
            Some("echo hello")
        );
    }

    #[test]
    fn hook_script_returns_none_for_unknown_hook() {
        let config = Config {
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    hooks: Some(HooksConfig {
                        new_after: Some("echo hello".to_string()),
                    }),
                    ..ProjectConfig::default()
                },
            )])),
            ..Config::default()
        };
        assert!(hook_script(&config, "my-project", "unknown-hook").is_none());
    }

    #[test]
    fn deserialize_config_from_toml() {
        let toml_str = r#"
[projects.my-project.hooks]
new-after = """
#!/bin/bash
echo "hello"
"""
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let script = hook_script(&config, "my-project", "new-after").unwrap();
        assert!(script.contains("echo \"hello\""));
    }

    #[test]
    fn project_hook_script_returns_script() {
        let project = ProjectConfig {
            hooks: Some(HooksConfig {
                new_after: Some("echo project".to_string()),
            }),
            ..ProjectConfig::default()
        };
        assert_eq!(
            project_hook_script(&project, "new-after"),
            Some("echo project")
        );
    }

    #[test]
    fn project_hook_script_returns_none_without_hooks() {
        let project = ProjectConfig::default();
        assert!(project_hook_script(&project, "new-after").is_none());
    }

    #[test]
    fn deserialize_project_config_from_toml() {
        let toml_str = r#"
[hooks]
new-after = """
#!/bin/bash
echo "local"
"""
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let script = project_hook_script(&config, "new-after").unwrap();
        assert!(script.contains("echo \"local\""));
    }
}

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::CliError;
use crate::paths;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Named orchestrator definitions, keyed by name.
    pub orchestrators: Option<HashMap<String, OrchestratorDefinition>>,
    pub projects: Option<HashMap<String, ProjectConfig>>,
    pub daemon: Option<DaemonConfig>,
    pub orchestrator: Option<OrchestratorConfig>,
    pub tui: Option<TuiConfig>,
    #[serde(rename = "default-branch")]
    pub default_branch: Option<String>,
    /// Script body to generate task/worktree names. Written to a temp file and
    /// executed directly, so it can use any interpreter via a shebang line
    /// (e.g. `#!/usr/bin/env fish`). Its trimmed stdout is used as the task
    /// name. If the script fails, the built-in adjective-noun generator is used
    /// as a fallback. The environment variables `WORK_PROJECT` and `WORK_ISSUE`
    /// (if available) are set before the script runs.
    #[serde(rename = "task-name-command")]
    pub task_name_command: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub hooks: Option<HooksConfig>,
    pub orchestrator: Option<ProjectOrchestrator>,
    #[serde(rename = "pool-size")]
    pub pool_size: Option<u32>,
    #[serde(rename = "default-branch")]
    pub default_branch: Option<String>,
    /// Project-level override for task name generation. See
    /// [`Config::task_name_command`] for details.
    #[serde(rename = "task-name-command")]
    pub task_name_command: Option<String>,
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

fn default_pool_pull_enabled() -> bool {
    true
}

fn default_pool_pull_interval() -> u64 {
    3600
}

fn default_pr_cleanup_enabled() -> bool {
    true
}

fn default_pr_cleanup_interval() -> u64 {
    300
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
    /// Defaults to true so users get an up-to-date branch when starting work.
    #[serde(rename = "pool-pull-enabled", default = "default_pool_pull_enabled")]
    pub pool_pull_enabled: bool,
    /// Seconds between pool pull cycles (default 3600 = 1 hour).
    #[serde(rename = "pool-pull-interval", default = "default_pool_pull_interval")]
    pub pool_pull_interval: u64,
    /// When true, sessions with merged/closed PRs are automatically cleaned up.
    /// Defaults to true.
    #[serde(rename = "pr-cleanup-enabled", default = "default_pr_cleanup_enabled")]
    pub pr_cleanup_enabled: bool,
    /// Seconds between PR cleanup sweeps (default 300 = 5 minutes).
    #[serde(
        rename = "pr-cleanup-interval",
        default = "default_pr_cleanup_interval"
    )]
    pub pr_cleanup_interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    #[serde(rename = "new-after")]
    pub new_after: Option<String>,
}

/// A named orchestrator definition. Lives under `[orchestrators.<name>]` in the
/// global config. Contains only the agent-level settings (command and prompt).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrchestratorDefinition {
    #[serde(rename = "agent-command")]
    pub agent_command: Option<Vec<String>>,
    #[serde(rename = "system-prompt")]
    pub system_prompt: Option<String>,
}

/// Per-project orchestrator reference. Can be a name string referencing a named
/// orchestrator (`orchestrator = "claude"`) or an inline table with orchestrator
/// settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ProjectOrchestrator {
    Inline(OrchestratorConfig),
    Name(String),
}

/// Global orchestrator settings under `[orchestrator]`. The `default` field
/// selects a named orchestrator from `[orchestrators]` as the baseline. Inline
/// `agent-command` and `system-prompt` take precedence over the named
/// orchestrator's values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrchestratorConfig {
    /// Name of the default orchestrator from `[orchestrators]`.
    pub default: Option<String>,
    #[serde(rename = "agent-command")]
    pub agent_command: Option<Vec<String>>,
    #[serde(rename = "system-prompt")]
    pub system_prompt: Option<String>,
    #[serde(rename = "max-agents-in-flight")]
    pub max_agents_in_flight: Option<u32>,
    #[serde(rename = "max-sessions-per-issue")]
    pub max_sessions_per_issue: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct TuiConfig {
    /// Auto-refresh interval in seconds (default 5).
    #[serde(rename = "refresh-interval")]
    pub refresh_interval: Option<u64>,
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

/// Look up a named orchestrator definition from the global `[orchestrators]` table.
fn resolve_named_orchestrator<'a>(
    config: &'a Config,
    name: &str,
) -> Option<&'a OrchestratorDefinition> {
    config.orchestrators.as_ref()?.get(name)
}

/// Resolve agent-command from a `ProjectOrchestrator` value, looking up named
/// orchestrators in the global config when needed.
fn resolve_project_orchestrator_agent_command(
    global_config: &Config,
    orch: &ProjectOrchestrator,
) -> Option<Vec<String>> {
    match orch {
        ProjectOrchestrator::Name(name) => {
            resolve_named_orchestrator(global_config, name).and_then(|d| d.agent_command.clone())
        }
        ProjectOrchestrator::Inline(cfg) => {
            // Inline agent-command takes precedence; fall back to default name.
            if cfg.agent_command.is_some() {
                return cfg.agent_command.clone();
            }
            cfg.default
                .as_deref()
                .and_then(|name| resolve_named_orchestrator(global_config, name))
                .and_then(|d| d.agent_command.clone())
        }
    }
}

/// Resolve system-prompt from a `ProjectOrchestrator` value.
fn resolve_project_orchestrator_system_prompt(
    global_config: &Config,
    orch: &ProjectOrchestrator,
) -> Option<String> {
    match orch {
        ProjectOrchestrator::Name(name) => {
            resolve_named_orchestrator(global_config, name).and_then(|d| d.system_prompt.clone())
        }
        ProjectOrchestrator::Inline(cfg) => {
            if cfg.system_prompt.is_some() {
                return cfg.system_prompt.clone();
            }
            cfg.default
                .as_deref()
                .and_then(|name| resolve_named_orchestrator(global_config, name))
                .and_then(|d| d.system_prompt.clone())
        }
    }
}

/// Returns the effective agent command for a project. Checks project-level
/// .work/config.toml first, then global per-project config, then global
/// orchestrator config (inline, then named default). Returns a vec of
/// [binary, args...] with placeholders `{issue}`, `{system_prompt}`, and
/// `{report_path}` to be replaced at runtime.
pub fn effective_agent_command(
    global_config: &Config,
    project_name: &str,
    project_path: &str,
) -> Vec<String> {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(ref orch) = project_cfg.orchestrator
        && let Some(cmd) = resolve_project_orchestrator_agent_command(global_config, orch)
    {
        return cmd;
    }

    // Fall back to global per-project config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(ref orch) = project_cfg.orchestrator
        && let Some(cmd) = resolve_project_orchestrator_agent_command(global_config, orch)
    {
        return cmd;
    }

    // Fall back to global orchestrator config: inline first, then default name.
    if let Some(ref orch) = global_config.orchestrator {
        if let Some(ref cmd) = orch.agent_command {
            return cmd.clone();
        }
        if let Some(ref default_name) = orch.default
            && let Some(def) = resolve_named_orchestrator(global_config, default_name)
            && let Some(ref cmd) = def.agent_command
        {
            return cmd.clone();
        }
    }

    DEFAULT_AGENT_COMMAND
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Returns the effective system prompt for a project. Checks project-level
/// .work/config.toml first, then global per-project config, then global
/// orchestrator config (inline, then named default). Returns `None` when no
/// custom prompt is configured (callers should fall back to the built-in
/// default).
pub fn effective_system_prompt(
    global_config: &Config,
    project_name: &str,
    project_path: &str,
) -> Option<String> {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(ref orch) = project_cfg.orchestrator
        && let Some(prompt) = resolve_project_orchestrator_system_prompt(global_config, orch)
    {
        return Some(prompt);
    }

    // Fall back to global per-project config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(ref orch) = project_cfg.orchestrator
        && let Some(prompt) = resolve_project_orchestrator_system_prompt(global_config, orch)
    {
        return Some(prompt);
    }

    // Fall back to global orchestrator config: inline first, then default name.
    if let Some(ref orch) = global_config.orchestrator {
        if orch.system_prompt.is_some() {
            return orch.system_prompt.clone();
        }
        if let Some(ref default_name) = orch.default
            && let Some(def) = resolve_named_orchestrator(global_config, default_name)
            && def.system_prompt.is_some()
        {
            return def.system_prompt.clone();
        }
    }

    None
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

/// Returns the effective task-name-command for a project. Checks project-level
/// `.work/config.toml` first, then global per-project config, then the global
/// `task-name-command`. Returns `None` when no custom command is configured.
pub fn effective_task_name_command(
    global_config: &Config,
    project_name: &str,
    project_path: &str,
) -> Option<String> {
    // Project-level .work/config.toml takes priority.
    if let Ok(project_cfg) = load_project_config(project_path)
        && let Some(cmd) = project_cfg.task_name_command
    {
        return Some(cmd);
    }

    // Fall back to global per-project config.
    if let Some(projects) = &global_config.projects
        && let Some(project_cfg) = projects.get(project_name)
        && let Some(cmd) = &project_cfg.task_name_command
    {
        return Some(cmd.clone());
    }

    // Fall back to global task-name-command.
    global_config.task_name_command.clone()
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

    #[test]
    fn pool_pull_enabled_defaults_to_true() {
        let toml_str = "[daemon]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert!(daemon.pool_pull_enabled);
    }

    #[test]
    fn pool_pull_enabled_can_be_disabled() {
        let toml_str = "[daemon]\npool-pull-enabled = false\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert!(!daemon.pool_pull_enabled);
    }

    #[test]
    fn pool_pull_interval_defaults_to_3600() {
        let toml_str = "[daemon]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert_eq!(daemon.pool_pull_interval, 3600);
    }

    #[test]
    fn pr_cleanup_enabled_defaults_to_true() {
        let toml_str = "[daemon]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert!(daemon.pr_cleanup_enabled);
    }

    #[test]
    fn pr_cleanup_enabled_can_be_disabled() {
        let toml_str = "[daemon]\npr-cleanup-enabled = false\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert!(!daemon.pr_cleanup_enabled);
    }

    #[test]
    fn pr_cleanup_interval_defaults_to_300() {
        let toml_str = "[daemon]\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert_eq!(daemon.pr_cleanup_interval, 300);
    }

    #[test]
    fn pr_cleanup_interval_can_be_customized() {
        let toml_str = "[daemon]\npr-cleanup-interval = 600\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let daemon = config.daemon.unwrap();
        assert_eq!(daemon.pr_cleanup_interval, 600);
    }

    #[test]
    fn tui_refresh_interval_parses() {
        let toml_str = "[tui]\nrefresh-interval = 10\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let tui = config.tui.unwrap();
        assert_eq!(tui.refresh_interval, Some(10));
    }

    #[test]
    fn tui_section_omitted_defaults_to_none() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.tui.is_none());
    }

    #[test]
    fn task_name_command_parses_global() {
        let toml_str = "task-name-command = \"llm-name-gen\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.task_name_command.as_deref(), Some("llm-name-gen"));
    }

    #[test]
    fn task_name_command_defaults_to_none() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.task_name_command.is_none());
    }

    #[test]
    fn task_name_command_parses_project_level() {
        let toml_str = "task-name-command = \"my-namer --style haiku\"\n";
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.task_name_command.as_deref(),
            Some("my-namer --style haiku")
        );
    }

    #[test]
    fn task_name_command_per_project_in_global() {
        let toml_str = r#"
[projects.my-project]
task-name-command = "echo my-custom-name"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let project = config.projects.as_ref().unwrap().get("my-project").unwrap();
        assert_eq!(
            project.task_name_command.as_deref(),
            Some("echo my-custom-name")
        );
    }

    // -- Named orchestrator tests --

    #[test]
    fn deserialize_named_orchestrators() {
        let toml_str = r#"
[orchestrators.claude]
agent-command = ["claude", "-p", "{issue}"]
system-prompt = "You are Claude."

[orchestrators.codex]
agent-command = ["codex", "exec", "{issue}"]
system-prompt = "You are Codex."
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let orchestrators = config.orchestrators.unwrap();
        assert_eq!(orchestrators.len(), 2);

        let claude = &orchestrators["claude"];
        assert_eq!(
            claude.agent_command.as_deref().unwrap(),
            &["claude", "-p", "{issue}"]
        );
        assert_eq!(claude.system_prompt.as_deref().unwrap(), "You are Claude.");

        let codex = &orchestrators["codex"];
        assert_eq!(
            codex.agent_command.as_deref().unwrap(),
            &["codex", "exec", "{issue}"]
        );
    }

    #[test]
    fn deserialize_orchestrator_default_name() {
        let toml_str = r#"
[orchestrator]
default = "claude"
max-agents-in-flight = 8
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let orch = config.orchestrator.unwrap();
        assert_eq!(orch.default.as_deref(), Some("claude"));
        assert_eq!(orch.max_agents_in_flight, Some(8));
        assert!(orch.agent_command.is_none());
    }

    #[test]
    fn deserialize_project_orchestrator_as_name() {
        let toml_str = r#"
[projects.my-project]
orchestrator = "codex"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let projects = config.projects.unwrap();
        let project = &projects["my-project"];
        match project.orchestrator.as_ref().unwrap() {
            ProjectOrchestrator::Name(name) => assert_eq!(name, "codex"),
            other => panic!("expected Name variant, got {:?}", other),
        }
    }

    #[test]
    fn deserialize_project_orchestrator_as_inline_table() {
        let toml_str = r#"
[projects.my-project.orchestrator]
agent-command = ["custom", "{issue}"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let projects = config.projects.unwrap();
        let project = &projects["my-project"];
        match project.orchestrator.as_ref().unwrap() {
            ProjectOrchestrator::Inline(cfg) => {
                assert_eq!(
                    cfg.agent_command.as_deref().unwrap(),
                    &["custom", "{issue}"]
                );
            }
            other => panic!("expected Inline variant, got {:?}", other),
        }
    }

    #[test]
    fn resolve_agent_command_from_named_orchestrator_via_project() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "codex".to_string(),
                OrchestratorDefinition {
                    agent_command: Some(vec!["codex".to_string(), "exec".to_string()]),
                    system_prompt: None,
                },
            )])),
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    orchestrator: Some(ProjectOrchestrator::Name("codex".to_string())),
                    ..ProjectConfig::default()
                },
            )])),
            ..Config::default()
        };
        // Use a non-existent project path so project-level config is skipped.
        let cmd = effective_agent_command(&config, "my-project", "/nonexistent");
        assert_eq!(cmd, vec!["codex", "exec"]);
    }

    #[test]
    fn resolve_agent_command_from_named_default() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "claude".to_string(),
                OrchestratorDefinition {
                    agent_command: Some(vec!["claude".to_string(), "-p".to_string()]),
                    system_prompt: None,
                },
            )])),
            orchestrator: Some(OrchestratorConfig {
                default: Some("claude".to_string()),
                agent_command: None,
                system_prompt: None,
                max_agents_in_flight: None,
                max_sessions_per_issue: None,
            }),
            ..Config::default()
        };
        let cmd = effective_agent_command(&config, "other-project", "/nonexistent");
        assert_eq!(cmd, vec!["claude", "-p"]);
    }

    #[test]
    fn inline_agent_command_overrides_named_default() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "claude".to_string(),
                OrchestratorDefinition {
                    agent_command: Some(vec!["claude".to_string()]),
                    system_prompt: None,
                },
            )])),
            orchestrator: Some(OrchestratorConfig {
                default: Some("claude".to_string()),
                agent_command: Some(vec!["inline-cmd".to_string()]),
                system_prompt: None,
                max_agents_in_flight: None,
                max_sessions_per_issue: None,
            }),
            ..Config::default()
        };
        let cmd = effective_agent_command(&config, "other-project", "/nonexistent");
        assert_eq!(cmd, vec!["inline-cmd"]);
    }

    #[test]
    fn resolve_system_prompt_from_named_orchestrator_via_project() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "codex".to_string(),
                OrchestratorDefinition {
                    agent_command: None,
                    system_prompt: Some("Codex prompt".to_string()),
                },
            )])),
            projects: Some(HashMap::from([(
                "my-project".to_string(),
                ProjectConfig {
                    orchestrator: Some(ProjectOrchestrator::Name("codex".to_string())),
                    ..ProjectConfig::default()
                },
            )])),
            ..Config::default()
        };
        let prompt = effective_system_prompt(&config, "my-project", "/nonexistent");
        assert_eq!(prompt.as_deref(), Some("Codex prompt"));
    }

    #[test]
    fn resolve_system_prompt_from_named_default() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "claude".to_string(),
                OrchestratorDefinition {
                    agent_command: None,
                    system_prompt: Some("Claude prompt".to_string()),
                },
            )])),
            orchestrator: Some(OrchestratorConfig {
                default: Some("claude".to_string()),
                agent_command: None,
                system_prompt: None,
                max_agents_in_flight: None,
                max_sessions_per_issue: None,
            }),
            ..Config::default()
        };
        let prompt = effective_system_prompt(&config, "other", "/nonexistent");
        assert_eq!(prompt.as_deref(), Some("Claude prompt"));
    }

    #[test]
    fn inline_system_prompt_overrides_named_default() {
        let config = Config {
            orchestrators: Some(HashMap::from([(
                "claude".to_string(),
                OrchestratorDefinition {
                    agent_command: None,
                    system_prompt: Some("Named prompt".to_string()),
                },
            )])),
            orchestrator: Some(OrchestratorConfig {
                default: Some("claude".to_string()),
                agent_command: None,
                system_prompt: Some("Inline prompt".to_string()),
                max_agents_in_flight: None,
                max_sessions_per_issue: None,
            }),
            ..Config::default()
        };
        let prompt = effective_system_prompt(&config, "other", "/nonexistent");
        assert_eq!(prompt.as_deref(), Some("Inline prompt"));
    }

    #[test]
    fn system_prompt_returns_none_when_no_config() {
        let config = Config::default();
        let prompt = effective_system_prompt(&config, "project", "/nonexistent");
        assert!(prompt.is_none());
    }

    #[test]
    fn deserialize_full_named_orchestrator_config() {
        let toml_str = r#"
[orchestrators.claude]
agent-command = ["claude", "-p", "--system-prompt", "{system_prompt}", "{issue}"]
system-prompt = "You are Claude."

[orchestrators.codex]
agent-command = ["codex", "exec", "{issue}"]

[orchestrator]
default = "claude"
max-agents-in-flight = 6
max-sessions-per-issue = 5

[projects.frontend]
orchestrator = "codex"

[projects.backend.orchestrator]
agent-command = ["custom", "{issue}"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        // Named orchestrators parsed correctly.
        let orchestrators = config.orchestrators.as_ref().unwrap();
        assert_eq!(orchestrators.len(), 2);

        // Global default is "claude".
        let orch = config.orchestrator.as_ref().unwrap();
        assert_eq!(orch.default.as_deref(), Some("claude"));
        assert_eq!(orch.max_agents_in_flight, Some(6));

        // Frontend project references by name.
        let projects = config.projects.as_ref().unwrap();
        match projects["frontend"].orchestrator.as_ref().unwrap() {
            ProjectOrchestrator::Name(name) => assert_eq!(name, "codex"),
            other => panic!("expected Name, got {:?}", other),
        }

        // Backend project uses inline config.
        match projects["backend"].orchestrator.as_ref().unwrap() {
            ProjectOrchestrator::Inline(cfg) => {
                assert_eq!(
                    cfg.agent_command.as_deref().unwrap(),
                    &["custom", "{issue}"]
                );
            }
            other => panic!("expected Inline, got {:?}", other),
        }

        // Resolve agent command for frontend → codex's command.
        let cmd = effective_agent_command(&config, "frontend", "/nonexistent");
        assert_eq!(cmd, vec!["codex", "exec", "{issue}"]);

        // Resolve agent command for backend → inline command.
        let cmd = effective_agent_command(&config, "backend", "/nonexistent");
        assert_eq!(cmd, vec!["custom", "{issue}"]);

        // Resolve agent command for unknown project → default (claude).
        let cmd = effective_agent_command(&config, "unknown", "/nonexistent");
        assert_eq!(
            cmd,
            vec![
                "claude",
                "-p",
                "--system-prompt",
                "{system_prompt}",
                "{issue}"
            ]
        );
    }

    #[test]
    fn project_orchestrator_name_in_project_level_config() {
        // Ensure ProjectConfig can be deserialized with orchestrator = "name"
        // (as would appear in .work/config.toml).
        let toml_str = r#"orchestrator = "codex""#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        match config.orchestrator.as_ref().unwrap() {
            ProjectOrchestrator::Name(name) => assert_eq!(name, "codex"),
            other => panic!("expected Name variant, got {:?}", other),
        }
    }
}

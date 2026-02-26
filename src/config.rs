use std::collections::HashMap;

use crate::paths;

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub daemon: Option<DaemonConfig>,
    #[serde(alias = "default-environment-provider")]
    pub environment_provider: Option<String>,
    #[serde(alias = "default-task-provider")]
    pub task_provider: Option<String>,
    pub projects: Option<HashMap<String, ProjectConfig>>,
    pub tasks: Option<TasksConfig>,
    pub environments: Option<EnvironmentsConfig>,
}

#[derive(Default, serde::Deserialize)]
pub struct DaemonConfig {
    #[serde(default)]
    pub debug: bool,
}

#[derive(serde::Deserialize)]
pub struct TasksConfig {
    pub providers: HashMap<String, TaskProviderConfig>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum TaskProviderConfig {
    #[serde(rename = "command")]
    Command { command: String, args: Vec<String> },
}

#[derive(serde::Deserialize)]
pub struct EnvironmentsConfig {
    pub providers: HashMap<String, EnvironmentProviderConfig>,
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectConfig {
    #[serde(alias = "default-environment-provider")]
    pub environment_provider: Option<String>,
    #[serde(alias = "default-task-provider")]
    pub task_provider: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum EnvironmentProviderConfig {
    #[serde(rename = "script")]
    Script { path: String },
}

impl Config {
    pub fn default_task_provider_for_project(&self, project_name: &str) -> Option<String> {
        self.projects
            .as_ref()
            .and_then(|p| p.get(project_name))
            .and_then(|p| p.task_provider.clone())
            .or_else(|| self.task_provider.clone())
    }

    pub fn default_environment_provider_for_project(&self, project_name: &str) -> Option<String> {
        self.projects
            .as_ref()
            .and_then(|p| p.get(project_name))
            .and_then(|p| p.environment_provider.clone())
            .or_else(|| self.environment_provider.clone())
    }

    pub fn get_task_provider(&self, name: &str) -> anyhow::Result<&TaskProviderConfig> {
        self.tasks
            .as_ref()
            .and_then(|t| t.providers.get(name))
            .ok_or_else(|| anyhow::anyhow!("task provider not found: {name}"))
    }

    pub fn get_environment_provider(
        &self,
        name: &str,
    ) -> anyhow::Result<&EnvironmentProviderConfig> {
        self.environments
            .as_ref()
            .and_then(|e| e.providers.get(name))
            .ok_or_else(|| anyhow::anyhow!("environment provider not found in config: {name}"))
    }
}

pub fn load() -> anyhow::Result<Config> {
    let path = paths::config_dir()?.join("config.toml");

    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, EnvironmentProviderConfig};

    #[test]
    fn project_specific_defaults_override_global_defaults() {
        let config: Config = toml::from_str(
            r#"
task-provider = "global-task"
environment-provider = "global-env"

[projects.backend]
task-provider = "backend-task"
environment-provider = "backend-env"
"#,
        )
        .unwrap();

        assert_eq!(
            config
                .default_task_provider_for_project("backend")
                .as_deref(),
            Some("backend-task")
        );
        assert_eq!(
            config
                .default_environment_provider_for_project("backend")
                .as_deref(),
            Some("backend-env")
        );
    }

    #[test]
    fn project_defaults_fall_back_to_global_defaults() {
        let config: Config = toml::from_str(
            r#"
task-provider = "global-task"
environment-provider = "global-env"

[projects.frontend]
task-provider = "frontend-task"
"#,
        )
        .unwrap();

        assert_eq!(
            config
                .default_task_provider_for_project("unknown")
                .as_deref(),
            Some("global-task")
        );
        assert_eq!(
            config
                .default_environment_provider_for_project("frontend")
                .as_deref(),
            Some("global-env")
        );
    }

    #[test]
    fn old_default_keys_still_deserialize() {
        let config: Config = toml::from_str(
            r#"
default-task-provider = "global-task"
default-environment-provider = "global-env"

[projects.frontend]
default-task-provider = "frontend-task"
default-environment-provider = "frontend-env"
"#,
        )
        .unwrap();

        assert_eq!(config.task_provider.as_deref(), Some("global-task"));
        assert_eq!(config.environment_provider.as_deref(), Some("global-env"));
        assert_eq!(
            config
                .default_task_provider_for_project("frontend")
                .as_deref(),
            Some("frontend-task")
        );
        assert_eq!(
            config
                .default_environment_provider_for_project("frontend")
                .as_deref(),
            Some("frontend-env")
        );
    }

    #[test]
    fn environment_script_provider_uses_path_key() {
        let config: Config = toml::from_str(
            r#"
[environments.providers.sandbox]
type = "script"
path = "/tmp/sandbox-provider.sh"
"#,
        )
        .unwrap();

        let provider = config.get_environment_provider("sandbox").unwrap();
        assert!(matches!(
            provider,
            EnvironmentProviderConfig::Script { path }
            if path == "/tmp/sandbox-provider.sh"
        ));
    }
}

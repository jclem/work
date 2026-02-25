use std::collections::HashMap;

use crate::paths;

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub daemon: Option<DaemonConfig>,
    pub default_environment_provider: Option<String>,
    pub default_task_provider: Option<String>,
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

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum EnvironmentProviderConfig {
    #[serde(rename = "script")]
    Script { command: String },
}

impl Config {
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

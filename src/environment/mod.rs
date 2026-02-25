mod git_worktree;
mod script;

use std::path::PathBuf;

use crate::db::Project;

pub struct RunSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub stdin_data: Option<Vec<u8>>,
}

pub trait EnvironmentProvider {
    fn prepare(&self, project: &Project, env_id: &str) -> anyhow::Result<serde_json::Value>;
    fn update(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value>;
    fn claim(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value>;
    fn remove(&self, metadata: &serde_json::Value) -> anyhow::Result<()>;
    fn run(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec>;
}

pub fn list_providers() -> Vec<String> {
    let mut providers = vec!["git-worktree".to_string()];

    if let Ok(config) = crate::config::load()
        && let Some(envs) = &config.environments
    {
        providers.extend(envs.providers.keys().cloned());
    }

    providers
}

pub fn get_provider(name: &str) -> anyhow::Result<Box<dyn EnvironmentProvider>> {
    match name {
        "git-worktree" => Ok(Box::new(git_worktree::GitWorktreeProvider)),
        _ => {
            let config = crate::config::load()?;
            let env_config = config.get_environment_provider(name)?;
            match env_config {
                crate::config::EnvironmentProviderConfig::Script { command } => {
                    Ok(Box::new(script::ScriptProvider {
                        command: command.clone(),
                    }))
                }
            }
        }
    }
}

mod git_worktree;
mod script;

use std::path::{Path, PathBuf};

use crate::db::Project;

pub struct RunSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub stdin_data: Option<Vec<u8>>,
    pub env: Vec<(String, String)>,
}

pub struct ProviderExecCommand {
    pub name: String,
    pub help: Option<String>,
}

pub trait EnvironmentProvider {
    fn prepare(
        &self,
        project: &Project,
        env_id: &str,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value>;
    fn update(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value>;
    fn claim(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value>;
    fn remove(&self, metadata: &serde_json::Value, log_path: Option<&Path>) -> anyhow::Result<()>;
    fn run(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec>;
    fn exec_commands(
        &self,
        _metadata: &serde_json::Value,
    ) -> anyhow::Result<Vec<ProviderExecCommand>> {
        Ok(Vec::new())
    }
    fn exec(
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
                crate::config::EnvironmentProviderConfig::Script { path } => {
                    Ok(Box::new(script::ScriptProvider {
                        path: path.clone(),
                    }))
                }
            }
        }
    }
}

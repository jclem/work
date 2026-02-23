mod git_worktree;

use crate::db::Project;

pub trait EnvironmentProvider {
    fn prepare(&self, project: &Project, env_id: &str) -> anyhow::Result<serde_json::Value>;
    fn update(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value>;
    fn claim(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value>;
    fn remove(&self, metadata: &serde_json::Value) -> anyhow::Result<()>;
}

pub fn list_providers() -> &'static [&'static str] {
    &["git-worktree"]
}

pub fn get_provider(name: &str) -> anyhow::Result<Box<dyn EnvironmentProvider>> {
    match name {
        "git-worktree" => Ok(Box::new(git_worktree::GitWorktreeProvider)),
        _ => anyhow::bail!("unknown environment provider: {name}"),
    }
}

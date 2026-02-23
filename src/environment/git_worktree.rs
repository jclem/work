use std::process::Command;

use serde_json::json;

use crate::db::Project;

use super::EnvironmentProvider;

pub struct GitWorktreeProvider;

impl EnvironmentProvider for GitWorktreeProvider {
    fn prepare(&self, project: &Project, env_id: &str) -> anyhow::Result<serde_json::Value> {
        let worktree_path = crate::paths::data_dir()?.join("worktrees").join(env_id);
        let branch = format!("work-env-{env_id}");

        std::fs::create_dir_all(worktree_path.parent().unwrap())?;

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                &worktree_path.to_string_lossy(),
            ])
            .current_dir(&project.path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {stderr}");
        }

        Ok(json!({
            "project_path": project.path,
            "worktree_path": worktree_path.to_string_lossy(),
            "branch": branch,
        }))
    }

    fn update(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let worktree_path = metadata["worktree_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing worktree_path in metadata"))?;

        let output = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git fetch failed: {stderr}");
        }

        let output = Command::new("git")
            .args(["merge", "origin/HEAD"])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git merge failed: {stderr}");
        }

        Ok(metadata.clone())
    }

    fn claim(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Ok(metadata.clone())
    }

    fn remove(&self, metadata: &serde_json::Value) -> anyhow::Result<()> {
        let worktree_path = metadata["worktree_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing worktree_path in metadata"))?;
        let project_path = metadata["project_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing project_path in metadata"))?;
        let branch = metadata["branch"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing branch in metadata"))?;

        let output = Command::new("git")
            .args(["worktree", "remove", "--force", worktree_path])
            .current_dir(project_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("is not a working tree") {
                anyhow::bail!("git worktree remove failed: {stderr}");
            }
        }

        let output = Command::new("git")
            .args(["branch", "-D", branch])
            .current_dir(project_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("not found") {
                anyhow::bail!("git branch -D failed: {stderr}");
            }
        }

        Ok(())
    }
}

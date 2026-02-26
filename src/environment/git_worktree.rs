use std::path::PathBuf;
use std::process::Command;

use serde_json::json;

use crate::db::Project;

use super::{EnvironmentProvider, ProviderExecCommand, RunSpec};

pub struct GitWorktreeProvider;

impl EnvironmentProvider for GitWorktreeProvider {
    fn prepare(
        &self,
        project: &Project,
        env_id: &str,
        _log_path: Option<&std::path::Path>,
    ) -> anyhow::Result<serde_json::Value> {
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

    fn update(
        &self,
        metadata: &serde_json::Value,
        _log_path: Option<&std::path::Path>,
    ) -> anyhow::Result<serde_json::Value> {
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

    fn claim(
        &self,
        metadata: &serde_json::Value,
        _log_path: Option<&std::path::Path>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(metadata.clone())
    }

    fn run(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec> {
        let worktree_path = metadata["worktree_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing worktree_path in metadata"))?;

        Ok(RunSpec {
            program: command.to_string(),
            args: args.to_vec(),
            cwd: Some(PathBuf::from(worktree_path)),
            stdin_data: None,
            env: Vec::new(),
        })
    }

    fn exec(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec> {
        let worktree_path = metadata["worktree_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing worktree_path in metadata"))?;

        if command == "cd" {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
            return Ok(RunSpec {
                program: shell,
                args: Vec::new(),
                cwd: Some(PathBuf::from(worktree_path)),
                stdin_data: None,
                env: Vec::new(),
            });
        }

        Ok(RunSpec {
            program: command.to_string(),
            args: args.to_vec(),
            cwd: Some(PathBuf::from(worktree_path)),
            stdin_data: None,
            env: Vec::new(),
        })
    }

    fn exec_commands(
        &self,
        _metadata: &serde_json::Value,
    ) -> anyhow::Result<Vec<ProviderExecCommand>> {
        Ok(vec![ProviderExecCommand {
            name: "cd".to_string(),
            help: Some("Open a shell in the worktree directory".to_string()),
        }])
    }

    fn remove(
        &self,
        metadata: &serde_json::Value,
        _log_path: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_commands_include_cd() {
        let provider = GitWorktreeProvider;
        let commands = provider.exec_commands(&json!({})).unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "cd");
    }

    #[test]
    fn exec_cd_runs_shell_in_worktree() {
        let provider = GitWorktreeProvider;
        let metadata = json!({ "worktree_path": "/tmp/worktree" });

        let run_spec = provider.exec(&metadata, "cd", &[]).unwrap();

        assert_eq!(
            run_spec.program,
            std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
        );
        assert_eq!(run_spec.args, Vec::<String>::new());
        assert_eq!(run_spec.cwd, Some(PathBuf::from("/tmp/worktree")));
    }

    #[test]
    fn exec_non_cd_passthrough() {
        let provider = GitWorktreeProvider;
        let metadata = json!({ "worktree_path": "/tmp/worktree" });
        let args = vec!["-la".to_string()];

        let run_spec = provider.exec(&metadata, "ls", &args).unwrap();

        assert_eq!(run_spec.program, "ls");
        assert_eq!(run_spec.args, args);
        assert_eq!(run_spec.cwd, Some(PathBuf::from("/tmp/worktree")));
    }
}

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

use crate::db::Project;

use super::{EnvironmentProvider, ProviderExecCommand, RunSpec};

const BASE_BRANCH: &str = "main";

pub struct ApfsWorktreeProvider;

impl ApfsWorktreeProvider {
    fn metadata_string<'a>(
        metadata: &'a serde_json::Value,
        field: &str,
    ) -> anyhow::Result<&'a str> {
        metadata[field]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing {field} in metadata"))
    }

    fn run_command(command: &mut Command, error_prefix: &str) -> anyhow::Result<()> {
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{error_prefix}: {stderr}");
        }

        Ok(())
    }
}

impl EnvironmentProvider for ApfsWorktreeProvider {
    fn prepare(
        &self,
        project: &Project,
        env_id: &str,
        _log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        let project_path = PathBuf::from(&project.path);
        let worktrees_dir = crate::paths::data_dir()?.join("worktrees");
        let worktree_path = worktrees_dir.join(env_id);
        let branch = format!("work-env-{env_id}");

        std::fs::create_dir_all(&worktrees_dir)?;

        Self::run_command(
            Command::new("git")
                .args(["fetch", "origin", BASE_BRANCH])
                .current_dir(&project_path),
            "git fetch failed",
        )?;

        let branch_output = Command::new("git")
            .args(["branch", &branch, &format!("origin/{BASE_BRANCH}")])
            .current_dir(&project_path)
            .output()?;
        if !branch_output.status.success() {
            let stderr = String::from_utf8_lossy(&branch_output.stderr);
            if !stderr.contains("already exists") {
                anyhow::bail!("git branch failed: {stderr}");
            }
        }

        Self::run_command(
            Command::new("git")
                .args([
                    "worktree",
                    "add",
                    "--no-checkout",
                    &worktree_path.to_string_lossy(),
                    &branch,
                ])
                .current_dir(&project_path),
            "git worktree add failed",
        )?;

        for entry in std::fs::read_dir(&project_path)? {
            let entry = entry?;
            let file_name = entry.file_name();
            if file_name == std::ffi::OsStr::new(".git")
                || file_name == std::ffi::OsStr::new(".worktrees")
            {
                continue;
            }

            Self::run_command(
                Command::new("cp")
                    .arg("-cR")
                    .arg(entry.path())
                    .arg(&worktree_path),
                "apfs clone copy failed",
            )?;
        }

        Self::run_command(
            Command::new("git")
                .args(["reset", "--hard", &branch])
                .current_dir(&worktree_path),
            "git reset --hard failed",
        )?;

        Ok(json!({
            "project_path": project.path,
            "worktree_path": worktree_path.to_string_lossy(),
            "branch": branch,
            "base_branch": BASE_BRANCH,
        }))
    }

    fn update(
        &self,
        metadata: &serde_json::Value,
        _log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        let project_path = Self::metadata_string(metadata, "project_path")?;
        let worktree_path = Self::metadata_string(metadata, "worktree_path")?;
        let base_branch = metadata["base_branch"].as_str().unwrap_or(BASE_BRANCH);

        Self::run_command(
            Command::new("git")
                .args(["fetch", "origin", base_branch])
                .current_dir(project_path),
            "git fetch failed",
        )?;

        Self::run_command(
            Command::new("git")
                .args(["reset", "--hard", &format!("origin/{base_branch}")])
                .current_dir(worktree_path),
            "git reset --hard origin failed",
        )?;

        Ok(metadata.clone())
    }

    fn claim(
        &self,
        metadata: &serde_json::Value,
        _log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(metadata.clone())
    }

    fn run(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec> {
        let worktree_path = Self::metadata_string(metadata, "worktree_path")?;

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
        let worktree_path = Self::metadata_string(metadata, "worktree_path")?;

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

    fn remove(&self, metadata: &serde_json::Value, _log_path: Option<&Path>) -> anyhow::Result<()> {
        let project_path = Self::metadata_string(metadata, "project_path")?;
        let worktree_path = Self::metadata_string(metadata, "worktree_path")?;
        let branch = Self::metadata_string(metadata, "branch")?;

        let worktree_output = Command::new("git")
            .args(["worktree", "remove", "--force", worktree_path])
            .current_dir(project_path)
            .output()?;
        if !worktree_output.status.success() {
            let stderr = String::from_utf8_lossy(&worktree_output.stderr);
            if !stderr.contains("is not a working tree") {
                anyhow::bail!("git worktree remove failed: {stderr}");
            }
        }

        let branch_output = Command::new("git")
            .args(["branch", "-D", branch])
            .current_dir(project_path)
            .output()?;
        if !branch_output.status.success() {
            let stderr = String::from_utf8_lossy(&branch_output.stderr);
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
        let provider = ApfsWorktreeProvider;
        let commands = provider.exec_commands(&json!({})).unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "cd");
    }

    #[test]
    fn exec_cd_runs_shell_in_worktree() {
        let provider = ApfsWorktreeProvider;
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
        let provider = ApfsWorktreeProvider;
        let metadata = json!({ "worktree_path": "/tmp/worktree" });
        let args = vec!["-la".to_string()];

        let run_spec = provider.exec(&metadata, "ls", &args).unwrap();

        assert_eq!(run_spec.program, "ls");
        assert_eq!(run_spec.args, args);
        assert_eq!(run_spec.cwd, Some(PathBuf::from("/tmp/worktree")));
    }
}

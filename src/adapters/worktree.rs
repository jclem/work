use std::path::Path;
use std::process::Command;

use crate::error::CliError;

use super::TaskAdapter;

pub struct GitWorktreeAdapter;

impl GitWorktreeAdapter {
    /// Pull the default branch into a pool worktree so it stays up-to-date.
    ///
    /// Runs `git -C <worktree> pull --ff-only origin <default_branch>` which
    /// fast-forwards the local branch to match the remote. If the pull cannot
    /// fast-forward (e.g. diverged history) the error is propagated so the
    /// caller can log it and move on.
    pub fn pull(&self, worktree_path: &Path, default_branch: &str) -> Result<(), CliError> {
        let output = Command::new("git")
            .args(["-C"])
            .arg(worktree_path)
            .args(["pull", "--ff-only", "origin", default_branch])
            .output()
            .map_err(|source| CliError::with_source("failed to run git pull", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(format!(
                "git pull --ff-only failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Claim a pre-warmed pool worktree by renaming the branch and moving the worktree.
    pub fn claim_pooled(
        &self,
        project_path: &str,
        temp_name: &str,
        task_name: &str,
        old_path: &Path,
        new_path: &Path,
    ) -> Result<(), CliError> {
        // 1. Rename the branch: git -C <project> branch -m <temp_name> <task_name>
        let output = Command::new("git")
            .args(["-C", project_path, "branch", "-m", temp_name, task_name])
            .output()
            .map_err(|source| CliError::with_source("failed to run git", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(format!(
                "git branch rename failed: {}",
                stderr.trim()
            )));
        }

        // 2. Move the worktree: git -C <project> worktree move <old_path> <new_path>
        let output = Command::new("git")
            .args(["-C", project_path, "worktree", "move"])
            .arg(old_path)
            .arg(new_path)
            .output()
            .map_err(|source| CliError::with_source("failed to run git", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(format!(
                "git worktree move failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }
}

impl TaskAdapter for GitWorktreeAdapter {
    fn create(
        &self,
        project_path: &str,
        task_name: &str,
        worktree_path: &Path,
        start_point: &str,
    ) -> Result<(), CliError> {
        let output = Command::new("git")
            .args(["-C", project_path, "worktree", "add", "-b", task_name])
            .arg(worktree_path)
            .arg(start_point)
            .output()
            .map_err(|source| CliError::with_source("failed to run git", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(format!(
                "git worktree add failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    fn create_from_branch(
        &self,
        project_path: &str,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<(), CliError> {
        let output = Command::new("git")
            .args(["-C", project_path, "worktree", "add"])
            .arg(worktree_path)
            .arg(branch)
            .output()
            .map_err(|source| CliError::with_source("failed to run git", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(format!(
                "git worktree add failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    fn remove(
        &self,
        project_path: &str,
        task_name: &str,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), CliError> {
        // Remove the worktree (idempotent — skip if already gone).
        if worktree_path.exists() {
            let mut args = vec!["-C", project_path, "worktree", "remove"];
            if force {
                args.push("--force");
            }

            let output = Command::new("git")
                .args(&args)
                .arg(worktree_path)
                .output()
                .map_err(|source| CliError::with_source("failed to run git", source))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(CliError::new(format!(
                    "git worktree remove failed: {}",
                    stderr.trim()
                )));
            }
        }

        // Delete the branch (idempotent — ignore "not found" errors).
        let delete_flag = if force { "-D" } else { "-d" };
        let output = Command::new("git")
            .args(["-C", project_path, "branch", delete_flag, task_name])
            .output()
            .map_err(|source| CliError::with_source("failed to run git", source))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if !stderr.contains("not found") {
                return Err(CliError::new(format!(
                    "git branch delete failed: {}",
                    stderr
                )));
            }
        }

        Ok(())
    }
}

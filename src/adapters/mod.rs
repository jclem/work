pub mod worktree;

use std::path::Path;

use crate::error::CliError;

pub trait TaskAdapter {
    fn create(
        &self,
        project_path: &str,
        task_name: &str,
        worktree_path: &Path,
    ) -> Result<(), CliError>;

    fn remove(
        &self,
        project_path: &str,
        task_name: &str,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), CliError>;
}

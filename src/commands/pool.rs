use crate::cli::PoolCommand;
use crate::client;
use crate::error::{self, CliError};

pub fn execute(command: PoolCommand) -> Result<(), CliError> {
    match command {
        PoolCommand::Clear => clear(),
    }
}

fn clear() -> Result<(), CliError> {
    let resp = client::clear_pool()?;
    error::print_success(&format!(
        "Removed {} pool worktree(s).",
        resp.pool_worktrees
    ));
    Ok(())
}

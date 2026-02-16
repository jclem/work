use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "work")]
#[command(version)]
#[command(about = "Work CLI")]
#[command(long_about = None)]
#[command(propagate_version = true)]
#[command(subcommand_required = true)]
pub struct Cli {
    /// Show detailed error source chains.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the CLI version
    Version,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },

    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    Start(DaemonStartArgs),
    SocketPath(DaemonSocketPathArgs),
}

#[derive(Debug, Args)]
pub struct DaemonStartArgs {
    /// Override the unix socket path used by the daemon.
    #[arg(long, value_name = "PATH")]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DaemonSocketPathArgs {
    /// Override the unix socket path to print.
    #[arg(long, value_name = "PATH")]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum ProjectsCommand {
    /// Create a project in the local project registry.
    Create(ProjectsCreateArgs),

    /// List projects in the local project registry.
    #[command(alias = "ls")]
    List(ProjectsListArgs),

    /// Delete a project by name.
    Delete(ProjectsDeleteArgs),
}

#[derive(Debug, Args)]
pub struct ProjectsCreateArgs {
    /// Project path. Defaults to the current working directory.
    #[arg(value_name = "PROJECT_PATH")]
    pub project_path: Option<PathBuf>,

    /// Project name. Defaults to the project path basename.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectsListArgs {
    /// Output as JSON.
    #[arg(long, conflicts_with = "plain")]
    pub json: bool,

    /// Output as tab-separated values with no headers.
    #[arg(long, conflicts_with = "json")]
    pub plain: bool,
}

#[derive(Debug, Args)]
pub struct ProjectsDeleteArgs {
    /// Project name.
    #[arg(value_name = "PROJECT_NAME")]
    pub project_name: String,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn cli_requires_a_subcommand() {
        let result = Cli::try_parse_from(["work"]);
        assert!(result.is_err());
    }

    #[test]
    fn projects_list_alias_ls_parses() {
        let cli = Cli::try_parse_from(["work", "projects", "ls"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Projects {
                command: ProjectsCommand::List(_)
            }
        ));
    }

    #[test]
    fn projects_list_rejects_conflicting_output_flags() {
        let result = Cli::try_parse_from(["work", "projects", "list", "--json", "--plain"]);
        assert!(result.is_err());
    }
}

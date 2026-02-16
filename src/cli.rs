use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "work")]
#[command(version)]
#[command(about = "Work CLI")]
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

    /// Alias for `work projects list`.
    Ls(ProjectsListArgs),
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

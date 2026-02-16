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

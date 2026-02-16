mod cli;
mod error;
mod logger;
mod paths;
mod workd;

use clap::Parser;

use crate::cli::{Cli, Command, DaemonCommand};
use crate::error::CliError;
use crate::logger::get_logger;
use crate::workd::Workd;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    let verbose = cli.verbose;

    if let Err(error) = run(cli).await {
        error::print_error(&error, verbose);
        std::process::exit(error::exit_code(&error));
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let logger = get_logger();

    match cli.command {
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        Command::Daemon { command } => match command {
            DaemonCommand::Start(args) => Workd::start(logger, args.socket).await?,
            DaemonCommand::SocketPath(args) => {
                println!("{}", paths::socket_path(args.socket).display());
            }
        },
    }

    Ok(())
}

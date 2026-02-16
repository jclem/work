mod cli;
mod db;
mod error;
mod logger;
mod paths;
mod projects;
mod workd;

use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;

use crate::cli::{Cli, Command, DaemonCommand, ProjectsCommand};
use crate::error::CliError;
use crate::logger::get_logger;
use crate::projects as projects_command;
use crate::workd::Workd;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    CompleteEnv::with_factory(Cli::command).complete();

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
        Command::Completions { shell } => {
            use clap_complete::env::{self, EnvCompleter};

            let shell: &dyn EnvCompleter = match shell {
                clap_complete::Shell::Bash => &env::Bash,
                clap_complete::Shell::Elvish => &env::Elvish,
                clap_complete::Shell::Fish => &env::Fish,
                clap_complete::Shell::PowerShell => &env::Powershell,
                clap_complete::Shell::Zsh => &env::Zsh,
                _ => unreachable!("all shell variants handled"),
            };

            let completer = {
                let arg0 = std::env::args_os().next().unwrap_or_else(|| "work".into());
                let path = std::path::PathBuf::from(arg0);
                if path.components().count() > 1 {
                    std::env::current_dir()
                        .map(|dir| dir.join(&path))
                        .unwrap_or(path)
                } else {
                    path
                }
            };
            let completer = completer.to_string_lossy();

            let mut stdout = std::io::stdout();
            shell
                .write_registration("COMPLETE", "work", "work", &completer, &mut stdout)
                .map_err(|source| {
                    CliError::with_source("failed to write shell completions", source)
                })?;
        }
        Command::Daemon { command } => match command {
            DaemonCommand::Start(args) => Workd::start(logger, args.socket).await?,
            DaemonCommand::SocketPath(args) => {
                println!("{}", paths::socket_path(args.socket).display());
            }
        },
        Command::Projects { command } => {
            projects_command::execute(command)?;
        }
        Command::Ls(args) => {
            projects_command::execute(ProjectsCommand::List(args))?;
        }
    }

    Ok(())
}

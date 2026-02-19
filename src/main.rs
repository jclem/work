mod adapters;
mod cli;
mod client;
mod commands;
mod completions;
mod config;
mod db;
mod error;
mod logger;
mod paths;
mod state;
mod tui;
mod workd;

use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;

use crate::cli::{Cli, Command};
use crate::commands::{
    config as config_command, daemon as daemon_command, doctor as doctor_command,
    init as init_command, pool as pool_command, projects as projects_command,
    sessions as sessions_command, tasks as tasks_command, tree as tree_command,
};
use crate::error::CliError;
use crate::logger::get_logger;

#[tokio::main]
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
        Command::License => {
            print!("{}", include_str!("../LICENSE.md"));
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
        Command::Init { shell } => init_command::run(shell),
        Command::Daemon { command } => daemon_command::execute(command, logger.clone()).await?,
        Command::Projects { command } => {
            projects_command::execute(command)?;
        }
        Command::Start(args) => sessions_command::start(args)?,
        Command::List(args) => sessions_command::list(args)?,
        Command::Show(args) => sessions_command::show(args)?,
        Command::Stop(args) => sessions_command::stop(args)?,
        Command::Delete(args) => sessions_command::delete(args)?,
        Command::Open(args) => sessions_command::open(args)?,
        Command::Pr(args) => sessions_command::pr(args)?,
        Command::Logs(args) => sessions_command::logs(args)?,
        Command::Cd(args) => tasks_command::cd(args)?,
        Command::Task { command } => tasks_command::execute(command)?,
        Command::Tree(_) => tree_command::run()?,
        Command::Nuke(args) => tasks_command::nuke(args)?,
        Command::Pool { command } => pool_command::execute(command)?,
        Command::Config { command } => config_command::execute(command)?,
        Command::Doctor => doctor_command::run()?,
        Command::Tui(args) => tui::run(args.interval)?,
    }

    Ok(())
}

use crate::cli::DaemonCommand;
use crate::error::CliError;
use crate::logger::Logger;
use crate::paths;
use crate::workd::Workd;

pub async fn execute(command: DaemonCommand, logger: Logger) -> Result<(), CliError> {
    match command {
        DaemonCommand::Start(args) => Workd::start(logger, args.socket).await,
        DaemonCommand::SocketPath(args) => {
            println!("{}", paths::socket_path(args.socket).display());
            Ok(())
        }
    }
}

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::completions;

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

    /// Print the MIT license
    License,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },

    /// Print shell initialization script (wrapper function).
    Init {
        /// Shell to generate the init script for
        shell: InitShell,
    },

    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },

    /// Start new sessions for an issue.
    #[command(alias = "new", alias = "create")]
    Start(SessionStartArgs),

    /// List sessions.
    #[command(alias = "ls")]
    List(SessionListArgs),

    /// Show session details and report.
    Show(SessionShowArgs),

    /// Rank sessions by heuristic score.
    Rank(SessionRankArgs),

    /// Accept a session, abandon siblings.
    Pick(SessionPickArgs),

    /// Reject a session.
    Reject(SessionRejectArgs),

    /// Stop a running session.
    Stop(SessionStopArgs),

    /// Delete a session and its worktree.
    #[command(alias = "rm")]
    Delete(SessionDeleteArgs),

    /// Open the session's worktree.
    Open(SessionOpenArgs),

    /// Tail live session output.
    Logs(SessionLogsArgs),

    /// Manage tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },

    /// Show a tree of all projects, tasks, and sessions.
    Tree(TreeArgs),

    /// Remove all tasks and projects.
    Nuke(NukeArgs),

    /// Manage the worktree pool.
    #[command(hide = true)]
    Pool {
        #[command(subcommand)]
        command: PoolCommand,
    },

    /// Manage configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Check the health of the work system.
    Doctor,

    /// Launch the interactive TUI dashboard.
    #[command(alias = "ui")]
    Tui(TuiArgs),
}

#[derive(Debug, Subcommand)]
pub enum PoolCommand {
    /// Remove all pool worktrees.
    Clear,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Open the config file in $EDITOR.
    Edit,
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Create a new task in the current project.
    #[command(alias = "create")]
    New(TaskNewArgs),
    /// List tasks.
    #[command(alias = "ls")]
    List(TaskListArgs),
    /// Change directory to a task's worktree, or the project root.
    Cd(TaskCdArgs),
    /// Delete a task.
    #[command(alias = "rm")]
    Delete(TaskDeleteArgs),
}

#[derive(Debug, Args)]
pub struct SessionStartArgs {
    /// Issue description (freeform text). Opens $EDITOR when omitted, or reads from stdin if piped.
    #[arg(value_name = "ISSUE")]
    pub issue: Option<String>,

    /// Number of parallel agent sessions to start.
    #[arg(long, default_value_t = 1)]
    pub agents: u32,

    /// Project to run sessions in.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct SessionListArgs {
    /// Filter by issue text.
    #[arg(long)]
    pub issue: Option<String>,

    /// Project to filter sessions for.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,

    /// Output as JSON.
    #[arg(long, conflicts_with = "plain")]
    pub json: bool,

    /// Output as tab-separated values with no headers.
    #[arg(long, conflicts_with = "json")]
    pub plain: bool,
}

#[derive(Debug, Args)]
pub struct SessionShowArgs {
    /// Session ID.
    #[arg(value_name = "ID")]
    pub id: i64,
}

#[derive(Debug, Args)]
pub struct SessionRankArgs {
    /// Issue to rank sessions for.
    #[arg(long)]
    pub issue: String,

    /// Project to filter sessions for.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct SessionPickArgs {
    /// Session ID to accept.
    #[arg(value_name = "ID")]
    pub id: i64,
}

#[derive(Debug, Args)]
pub struct SessionRejectArgs {
    /// Session ID to reject.
    #[arg(value_name = "ID")]
    pub id: i64,

    /// Reason for rejection.
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct SessionStopArgs {
    /// Session ID to stop.
    #[arg(value_name = "ID")]
    pub id: i64,
}

#[derive(Debug, Args)]
pub struct SessionDeleteArgs {
    /// Session ID to delete.
    #[arg(value_name = "ID")]
    pub id: i64,
}

#[derive(Debug, Args)]
pub struct SessionOpenArgs {
    /// Session ID.
    #[arg(value_name = "ID")]
    pub id: i64,
}

#[derive(Debug, Args)]
pub struct SessionLogsArgs {
    /// Session ID.
    #[arg(value_name = "ID")]
    pub id: i64,

    /// Follow the log output (like `tail -f`).
    #[arg(short, long)]
    pub follow: bool,
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon.
    Start(DaemonStartArgs),
    /// Print the daemon socket path.
    SocketPath(DaemonSocketPathArgs),
    /// Print the daemon PID.
    Pid,
    /// Stop the daemon.
    Stop,
    /// Restart the daemon.
    Restart(DaemonRestartArgs),
    /// Install the daemon as a Launch Agent (macOS).
    Install,
    /// Uninstall the daemon Launch Agent (macOS).
    Uninstall,
}

#[derive(Debug, Args)]
pub struct DaemonStartArgs {
    /// Override the unix socket path used by the daemon.
    #[arg(long, value_name = "PATH")]
    pub socket: Option<PathBuf>,

    /// Run in the foreground (default).
    #[arg(long, conflicts_with = "detach")]
    pub attach: bool,

    /// Daemonize and run in the background.
    #[arg(long, conflicts_with = "attach")]
    pub detach: bool,

    /// Replace an already-running daemon.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct DaemonSocketPathArgs {
    /// Override the unix socket path to print.
    #[arg(long, value_name = "PATH")]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DaemonRestartArgs {
    /// Override the unix socket path used by the daemon.
    #[arg(long, value_name = "PATH")]
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum ProjectsCommand {
    /// Create a project in the local project registry.
    #[command(alias = "new")]
    Create(ProjectsCreateArgs),

    /// List projects in the local project registry.
    #[command(alias = "ls")]
    List(ProjectsListArgs),

    /// Delete a project by name.
    #[command(alias = "rm")]
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

#[derive(Debug, Args)]
pub struct TaskNewArgs {
    /// Task name. Generated if omitted.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Use an existing branch instead of creating a new one.
    #[arg(short, long, value_name = "BRANCH", add = completions::branch_name_completer())]
    pub branch: Option<String>,

    /// Project to create the task in.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,

    /// Don't cd into the new worktree.
    #[arg(long)]
    pub no_cd: bool,
}

#[derive(Debug, Args)]
pub struct TaskListArgs {
    /// Output as JSON.
    #[arg(long, conflicts_with = "plain")]
    pub json: bool,

    /// Output as tab-separated values with no headers.
    #[arg(long, conflicts_with = "json")]
    pub plain: bool,

    /// List tasks across all projects.
    #[arg(long)]
    pub all: bool,

    /// Project to list tasks for.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskDeleteArgs {
    /// Task name.
    #[arg(value_name = "NAME", add = completions::task_name_completer())]
    pub name: String,

    /// Project the task belongs to.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,

    /// Force removal even if the worktree has changes.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct TaskCdArgs {
    /// Task name. If omitted, change to the project root.
    #[arg(value_name = "NAME", add = completions::task_name_completer())]
    pub name: Option<String>,

    /// Project the task belongs to.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct TuiArgs {
    /// Auto-refresh interval in seconds.
    #[arg(long, default_value_t = 5)]
    pub interval: u64,
}

#[derive(Debug, Args)]
pub struct TreeArgs {}

#[derive(Debug, Args)]
pub struct NukeArgs {
    /// Skip confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum InitShell {
    Fish,
    Bash,
    Zsh,
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

    #[test]
    fn task_new_parses_without_name() {
        let cli = Cli::try_parse_from(["work", "task", "new"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                command: TaskCommand::New(TaskNewArgs { name: None, .. })
            }
        ));
    }

    #[test]
    fn task_new_parses_with_name() {
        let cli = Cli::try_parse_from(["work", "task", "new", "my-task"]).unwrap();
        if let Command::Task {
            command: TaskCommand::New(args),
        } = cli.command
        {
            assert_eq!(args.name.as_deref(), Some("my-task"));
        } else {
            panic!("expected Command::Task New");
        }
    }

    #[test]
    fn task_new_parses_with_branch_short() {
        let cli = Cli::try_parse_from(["work", "task", "new", "-b", "feature-branch"]).unwrap();
        if let Command::Task {
            command: TaskCommand::New(args),
        } = cli.command
        {
            assert_eq!(args.branch.as_deref(), Some("feature-branch"));
            assert!(args.name.is_none());
        } else {
            panic!("expected Command::Task New");
        }
    }

    #[test]
    fn task_new_parses_with_branch_long() {
        let cli =
            Cli::try_parse_from(["work", "task", "new", "--branch", "feature-branch"]).unwrap();
        if let Command::Task {
            command: TaskCommand::New(args),
        } = cli.command
        {
            assert_eq!(args.branch.as_deref(), Some("feature-branch"));
        } else {
            panic!("expected Command::Task New");
        }
    }

    #[test]
    fn task_new_parses_with_name_and_branch() {
        let cli = Cli::try_parse_from(["work", "task", "new", "my-task", "-b", "feature-branch"])
            .unwrap();
        if let Command::Task {
            command: TaskCommand::New(args),
        } = cli.command
        {
            assert_eq!(args.name.as_deref(), Some("my-task"));
            assert_eq!(args.branch.as_deref(), Some("feature-branch"));
        } else {
            panic!("expected Command::Task New");
        }
    }

    #[test]
    fn task_list_alias_ls_parses() {
        let cli = Cli::try_parse_from(["work", "task", "ls"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                command: TaskCommand::List(_)
            }
        ));
    }

    #[test]
    fn task_list_rejects_conflicting_output_flags() {
        let result = Cli::try_parse_from(["work", "task", "list", "--json", "--plain"]);
        assert!(result.is_err());
    }

    #[test]
    fn task_cd_parses_with_name() {
        let cli = Cli::try_parse_from(["work", "task", "cd", "my-task"]).unwrap();
        if let Command::Task {
            command: TaskCommand::Cd(args),
        } = cli.command
        {
            assert_eq!(args.name.as_deref(), Some("my-task"));
        } else {
            panic!("expected Command::Task Cd");
        }
    }

    #[test]
    fn task_cd_parses_without_name() {
        let cli = Cli::try_parse_from(["work", "task", "cd"]).unwrap();
        if let Command::Task {
            command: TaskCommand::Cd(args),
        } = cli.command
        {
            assert!(args.name.is_none());
        } else {
            panic!("expected Command::Task Cd");
        }
    }

    #[test]
    fn task_delete_alias_rm_parses() {
        let cli = Cli::try_parse_from(["work", "task", "rm", "my-task"]).unwrap();
        if let Command::Task {
            command: TaskCommand::Delete(args),
        } = cli.command
        {
            assert_eq!(args.name, "my-task");
        } else {
            panic!("expected Command::Task Delete");
        }
    }

    #[test]
    fn task_delete_requires_name() {
        let result = Cli::try_parse_from(["work", "task", "delete"]);
        assert!(result.is_err());
    }

    #[test]
    fn tree_parses() {
        let cli = Cli::try_parse_from(["work", "tree"]).unwrap();
        assert!(matches!(cli.command, Command::Tree(_)));
    }

    #[test]
    fn doctor_parses() {
        let cli = Cli::try_parse_from(["work", "doctor"]).unwrap();
        assert!(matches!(cli.command, Command::Doctor));
    }

    #[test]
    fn daemon_start_defaults_to_attach() {
        let cli = Cli::try_parse_from(["work", "daemon", "start"]).unwrap();
        if let Command::Daemon {
            command: DaemonCommand::Start(args),
        } = cli.command
        {
            assert!(!args.attach);
            assert!(!args.detach);
        } else {
            panic!("expected Command::Daemon Start");
        }
    }

    #[test]
    fn daemon_start_accepts_detach() {
        let cli = Cli::try_parse_from(["work", "daemon", "start", "--detach"]).unwrap();
        if let Command::Daemon {
            command: DaemonCommand::Start(args),
        } = cli.command
        {
            assert!(args.detach);
            assert!(!args.attach);
        } else {
            panic!("expected Command::Daemon Start");
        }
    }

    #[test]
    fn daemon_start_accepts_attach() {
        let cli = Cli::try_parse_from(["work", "daemon", "start", "--attach"]).unwrap();
        if let Command::Daemon {
            command: DaemonCommand::Start(args),
        } = cli.command
        {
            assert!(args.attach);
            assert!(!args.detach);
        } else {
            panic!("expected Command::Daemon Start");
        }
    }

    #[test]
    fn daemon_start_rejects_attach_and_detach() {
        let result = Cli::try_parse_from(["work", "daemon", "start", "--attach", "--detach"]);
        assert!(result.is_err());
    }

    #[test]
    fn daemon_restart_parses() {
        let cli = Cli::try_parse_from(["work", "daemon", "restart"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon {
                command: DaemonCommand::Restart(_),
            }
        ));
    }

    #[test]
    fn start_parses_positional_issue() {
        let cli = Cli::try_parse_from(["work", "start", "fix the login bug"]).unwrap();
        if let Command::Start(args) = cli.command {
            assert_eq!(args.issue.as_deref(), Some("fix the login bug"));
        } else {
            panic!("expected Command::Start");
        }
    }

    #[test]
    fn start_alias_new_parses() {
        let cli = Cli::try_parse_from(["work", "new", "fix the login bug"]).unwrap();
        if let Command::Start(args) = cli.command {
            assert_eq!(args.issue.as_deref(), Some("fix the login bug"));
        } else {
            panic!("expected Command::Start (via alias new)");
        }
    }

    #[test]
    fn start_parses_without_issue() {
        let cli = Cli::try_parse_from(["work", "start"]).unwrap();
        if let Command::Start(args) = cli.command {
            assert!(args.issue.is_none());
        } else {
            panic!("expected Command::Start");
        }
    }

    #[test]
    fn start_parses_with_agents_flag() {
        let cli = Cli::try_parse_from(["work", "start", "my issue", "--agents", "3"]).unwrap();
        if let Command::Start(args) = cli.command {
            assert_eq!(args.issue.as_deref(), Some("my issue"));
            assert_eq!(args.agents, 3);
        } else {
            panic!("expected Command::Start");
        }
    }

    #[test]
    fn list_alias_ls_parses() {
        let cli = Cli::try_parse_from(["work", "ls"]).unwrap();
        assert!(matches!(cli.command, Command::List(_)));
    }

    #[test]
    fn list_rejects_conflicting_output_flags() {
        let result = Cli::try_parse_from(["work", "list", "--json", "--plain"]);
        assert!(result.is_err());
    }

    #[test]
    fn delete_alias_rm_parses() {
        let cli = Cli::try_parse_from(["work", "rm", "42"]).unwrap();
        if let Command::Delete(args) = cli.command {
            assert_eq!(args.id, 42);
        } else {
            panic!("expected Command::Delete");
        }
    }

    #[test]
    fn logs_parses_with_follow() {
        let cli = Cli::try_parse_from(["work", "logs", "42", "--follow"]).unwrap();
        if let Command::Logs(args) = cli.command {
            assert_eq!(args.id, 42);
            assert!(args.follow);
        } else {
            panic!("expected Command::Logs");
        }
    }

    #[test]
    fn logs_parses_without_follow() {
        let cli = Cli::try_parse_from(["work", "logs", "7"]).unwrap();
        if let Command::Logs(args) = cli.command {
            assert_eq!(args.id, 7);
            assert!(!args.follow);
        } else {
            panic!("expected Command::Logs");
        }
    }

    #[test]
    fn tui_parses_default_interval() {
        let cli = Cli::try_parse_from(["work", "tui"]).unwrap();
        if let Command::Tui(args) = cli.command {
            assert_eq!(args.interval, 5);
        } else {
            panic!("expected Command::Tui");
        }
    }

    #[test]
    fn tui_parses_custom_interval() {
        let cli = Cli::try_parse_from(["work", "tui", "--interval", "10"]).unwrap();
        if let Command::Tui(args) = cli.command {
            assert_eq!(args.interval, 10);
        } else {
            panic!("expected Command::Tui");
        }
    }

    #[test]
    fn tui_alias_ui_parses() {
        let cli = Cli::try_parse_from(["work", "ui"]).unwrap();
        assert!(matches!(cli.command, Command::Tui(_)));
    }
}

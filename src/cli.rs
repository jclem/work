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

    /// Stop a running session.
    Stop(SessionStopArgs),

    /// Delete a session and its worktree.
    #[command(alias = "rm")]
    Delete(SessionDeleteArgs),

    /// Open the session's worktree.
    Open(SessionOpenArgs),

    /// Open the session's pull request in a browser.
    Pr(SessionPrArgs),

    /// Tail live session output.
    Logs(SessionLogsArgs),

    /// Change directory to a task's worktree, or the project root.
    Cd(TaskCdArgs),

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

    /// Install the work AI agent skill into a project or global config.
    InstallSkill(InstallSkillArgs),

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

    /// Task/branch name for the new session (requires --agents 1).
    #[arg(short, long, value_name = "NAME")]
    pub name: Option<String>,

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
pub struct SessionPrArgs {
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
    /// Show daemon logs.
    Logs(DaemonLogsArgs),
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

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DaemonLogsArgs {
    /// Follow the log output (like `tail -f`).
    #[arg(short, long)]
    pub follow: bool,

    #[command(subcommand)]
    pub command: Option<DaemonLogsCommand>,
}

#[derive(Debug, Subcommand)]
pub enum DaemonLogsCommand {
    /// Print the daemon log path.
    Path,
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
    /// Auto-refresh interval in seconds (default: 5, or set in config).
    #[arg(long)]
    pub interval: Option<u64>,
}

#[derive(Debug, Args)]
pub struct TreeArgs {}

#[derive(Debug, Args)]
pub struct NukeArgs {
    /// Skip confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct InstallSkillArgs {
    /// Install only for a specific provider instead of all providers.
    #[arg(short, long, value_name = "PROVIDER")]
    pub provider: Option<Provider>,

    /// Install into the global config directory instead of the current directory.
    #[arg(short, long)]
    pub global: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Provider {
    Claude,
    Codex,
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
    fn cd_parses_with_name() {
        let cli = Cli::try_parse_from(["work", "cd", "my-task"]).unwrap();
        if let Command::Cd(args) = cli.command {
            assert_eq!(args.name.as_deref(), Some("my-task"));
        } else {
            panic!("expected Command::Cd");
        }
    }

    #[test]
    fn cd_parses_without_name() {
        let cli = Cli::try_parse_from(["work", "cd"]).unwrap();
        if let Command::Cd(args) = cli.command {
            assert!(args.name.is_none());
        } else {
            panic!("expected Command::Cd");
        }
    }

    #[test]
    fn cd_parses_with_project() {
        let cli = Cli::try_parse_from(["work", "cd", "my-task", "--project", "myproj"]).unwrap();
        if let Command::Cd(args) = cli.command {
            assert_eq!(args.name.as_deref(), Some("my-task"));
            assert_eq!(args.project.as_deref(), Some("myproj"));
        } else {
            panic!("expected Command::Cd");
        }
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
    fn daemon_logs_parses_without_follow() {
        let cli = Cli::try_parse_from(["work", "daemon", "logs"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon {
                command: DaemonCommand::Logs(DaemonLogsArgs {
                    follow: false,
                    command: None,
                }),
            }
        ));
    }

    #[test]
    fn daemon_logs_parses_with_follow() {
        let cli = Cli::try_parse_from(["work", "daemon", "logs", "--follow"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon {
                command: DaemonCommand::Logs(DaemonLogsArgs {
                    follow: true,
                    command: None,
                }),
            }
        ));
    }

    #[test]
    fn daemon_logs_path_parses() {
        let cli = Cli::try_parse_from(["work", "daemon", "logs", "path"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon {
                command: DaemonCommand::Logs(DaemonLogsArgs {
                    follow: false,
                    command: Some(DaemonLogsCommand::Path),
                }),
            }
        ));
    }

    #[test]
    fn daemon_logs_accepts_short_follow() {
        let cli = Cli::try_parse_from(["work", "daemon", "logs", "-f"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon {
                command: DaemonCommand::Logs(DaemonLogsArgs {
                    follow: true,
                    command: None,
                }),
            }
        ));
    }

    #[test]
    fn daemon_logs_rejects_follow_with_path() {
        let result = Cli::try_parse_from(["work", "daemon", "logs", "path", "--follow"]);
        assert!(result.is_err());
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
    fn start_parses_with_name_short() {
        let cli = Cli::try_parse_from(["work", "start", "-n", "hotfix-login"]).unwrap();
        if let Command::Start(args) = cli.command {
            assert_eq!(args.name.as_deref(), Some("hotfix-login"));
            assert!(args.issue.is_none());
        } else {
            panic!("expected Command::Start");
        }
    }

    #[test]
    fn start_parses_with_name_long() {
        let cli =
            Cli::try_parse_from(["work", "new", "--name", "hotfix-login", "fix the login bug"])
                .unwrap();
        if let Command::Start(args) = cli.command {
            assert_eq!(args.name.as_deref(), Some("hotfix-login"));
            assert_eq!(args.issue.as_deref(), Some("fix the login bug"));
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
    fn pr_parses() {
        let cli = Cli::try_parse_from(["work", "pr", "42"]).unwrap();
        if let Command::Pr(args) = cli.command {
            assert_eq!(args.id, 42);
        } else {
            panic!("expected Command::Pr");
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
    fn tui_parses_without_interval() {
        let cli = Cli::try_parse_from(["work", "tui"]).unwrap();
        if let Command::Tui(args) = cli.command {
            assert!(args.interval.is_none());
        } else {
            panic!("expected Command::Tui");
        }
    }

    #[test]
    fn tui_parses_custom_interval() {
        let cli = Cli::try_parse_from(["work", "tui", "--interval", "10"]).unwrap();
        if let Command::Tui(args) = cli.command {
            assert_eq!(args.interval, Some(10));
        } else {
            panic!("expected Command::Tui");
        }
    }

    #[test]
    fn tui_alias_ui_parses() {
        let cli = Cli::try_parse_from(["work", "ui"]).unwrap();
        assert!(matches!(cli.command, Command::Tui(_)));
    }

    #[test]
    fn install_skill_parses_no_args() {
        let cli = Cli::try_parse_from(["work", "install-skill"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(args.provider.is_none());
            assert!(!args.global);
        } else {
            panic!("expected Command::InstallSkill");
        }
    }

    #[test]
    fn install_skill_parses_provider_claude() {
        let cli = Cli::try_parse_from(["work", "install-skill", "-p", "claude"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(matches!(args.provider, Some(Provider::Claude)));
        } else {
            panic!("expected Command::InstallSkill");
        }
    }

    #[test]
    fn install_skill_parses_provider_codex() {
        let cli = Cli::try_parse_from(["work", "install-skill", "--provider", "codex"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(matches!(args.provider, Some(Provider::Codex)));
        } else {
            panic!("expected Command::InstallSkill");
        }
    }

    #[test]
    fn install_skill_parses_global_flag() {
        let cli = Cli::try_parse_from(["work", "install-skill", "--global"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(args.global);
        } else {
            panic!("expected Command::InstallSkill");
        }
    }

    #[test]
    fn install_skill_parses_global_short_flag() {
        let cli = Cli::try_parse_from(["work", "install-skill", "-g"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(args.global);
        } else {
            panic!("expected Command::InstallSkill");
        }
    }

    #[test]
    fn install_skill_parses_combined_flags() {
        let cli = Cli::try_parse_from(["work", "install-skill", "-g", "-p", "claude"]).unwrap();
        if let Command::InstallSkill(args) = cli.command {
            assert!(args.global);
            assert!(matches!(args.provider, Some(Provider::Claude)));
        } else {
            panic!("expected Command::InstallSkill");
        }
    }
}

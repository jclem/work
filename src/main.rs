use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};

mod client;
mod config;
mod daemon;
mod db;
mod environment;
mod id;
mod paths;
mod tui;

#[derive(Parser)]
#[command(name = "work", about = "A CLI for managing work", version)]
struct Cli {
    #[arg(long, global = true, env = "WORK_DEBUG")]
    debug: bool,

    #[arg(long, global = true, env = "WORK_HOME")]
    work_home: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Reset the database (destroys all data)
    #[command(hide = true)]
    ResetDatabase,

    /// Manage projects
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },

    /// Manage tasks
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },

    /// View task logs
    Logs {
        /// Task ID
        #[arg(add = ArgValueCompleter::new(complete_task_ids))]
        id: String,

        /// Follow log output in realtime
        #[arg(short, long)]
        follow: bool,
    },

    /// Manage environments
    #[command(alias = "env")]
    Environment {
        #[command(subcommand)]
        command: EnvironmentCommand,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Manage the daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Print version information
    Version,

    /// Open the terminal UI
    Tui,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::aot::Shell,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Human,
    Plain,
    Json,
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Create a new project
    New {
        /// Project name (defaults to current directory basename)
        name: Option<String>,

        /// Project path (defaults to current working directory)
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },

    /// Remove a project
    #[command(alias = "rm")]
    Remove {
        /// Project name
        name: String,
    },

    /// List all projects
    #[command(alias = "ls")]
    List {
        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
}

#[derive(Subcommand)]
enum EnvironmentCommand {
    /// Prepare and claim a new environment
    Create {
        /// Project name (defaults to project matching current directory)
        project: Option<String>,

        /// Provider (uses config default if not specified)
        #[arg(long, add = ArgValueCompleter::new(complete_env_providers))]
        provider: Option<String>,

        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Prepare a new environment and add it to the pool
    Prepare {
        /// Project name (defaults to project matching current directory)
        project: Option<String>,

        /// Provider (uses config default if not specified)
        #[arg(long, add = ArgValueCompleter::new(complete_env_providers))]
        provider: Option<String>,

        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Update a pooled environment
    Update {
        /// Environment ID
        id: String,

        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Claim an environment from the pool
    Claim {
        /// Claim a specific environment by ID
        id: Option<String>,

        /// Claim next available for this provider (required if no id)
        #[arg(long, add = ArgValueCompleter::new(complete_env_providers))]
        provider: Option<String>,

        /// Project name (required if no id)
        #[arg(long)]
        project: Option<String>,

        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Remove an environment
    #[command(alias = "rm")]
    Remove {
        /// Environment ID
        #[arg(add = ArgValueCompleter::new(complete_env_ids))]
        id: String,
    },

    /// List environments
    #[command(alias = "ls")]
    List {
        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Manage environment providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
}

#[derive(Subcommand)]
enum ProviderCommand {
    /// List available providers
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Create a new task
    New {
        /// Task description
        description: String,

        /// Project name (defaults to project matching current directory)
        #[arg(long)]
        project: Option<String>,

        /// Task provider (uses config default if not specified)
        #[arg(long, add = ArgValueCompleter::new(complete_task_providers))]
        provider: Option<String>,

        /// Environment provider (uses config default if not specified)
        #[arg(long, add = ArgValueCompleter::new(complete_env_providers))]
        env_provider: Option<String>,

        /// Follow task logs after creation
        #[arg(short, long)]
        attach: bool,

        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },

    /// Remove a task and its environment
    #[command(alias = "rm")]
    Remove {
        /// Task ID
        #[arg(add = ArgValueCompleter::new(complete_task_ids))]
        id: String,
    },

    /// List tasks
    #[command(alias = "ls")]
    List {
        /// Output format
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Open the config file in $EDITOR
    Edit,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the daemon
    Start {
        /// Remove existing runtime files before starting
        #[arg(long)]
        force: bool,
    },

    /// Install the daemon as a launchd LaunchAgent
    Install,

    /// Uninstall the daemon LaunchAgent
    Uninstall,
}

#[tokio::main]
async fn main() {
    clap_complete::env::CompleteEnv::with_factory(Cli::command).complete();

    if let Err(e) = run().await {
        eprintln!("\x1b[1;31merror:\x1b[0m {e}");

        // Print the chain of causes, if any.
        let mut source = e.source();
        while let Some(cause) = source {
            eprintln!("  \x1b[1;31mcaused by:\x1b[0m {cause}");
            source = std::error::Error::source(cause);
        }

        std::process::exit(1);
    }
}

fn complete_env_providers(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_str().unwrap_or_default();
    paths::init(None);
    environment::list_providers()
        .into_iter()
        .filter(|p| p.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}

fn complete_task_providers(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_str().unwrap_or_default();
    paths::init(None);
    config::load()
        .ok()
        .and_then(|c| c.tasks.map(|t| t.providers.into_keys().collect::<Vec<_>>()))
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}

fn complete_env_ids(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_str().unwrap_or_default().to_owned();

    let result = std::thread::spawn(move || -> anyhow::Result<Vec<CompletionCandidate>> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            paths::init(None);
            let client = client::DaemonClient::new()?;
            let envs = client.list_environments().await?;
            let projects = client.list_projects().await?;

            let cwd = std::env::current_dir()?.canonicalize()?;
            let current_project = projects.iter().find(|p| cwd.starts_with(&p.path));

            let candidates = envs
                .iter()
                .filter(|e| {
                    current_project
                        .as_ref()
                        .is_none_or(|p| e.project_id == p.id)
                })
                .filter(|e| e.id.to_string().starts_with(&current))
                .map(|e| {
                    let help = if let Some(proj) = projects.iter().find(|p| p.id == e.project_id) {
                        format!("{} ({})", proj.name, e.status)
                    } else {
                        e.status.clone()
                    };
                    CompletionCandidate::new(e.id.to_string()).help(Some(help.into()))
                })
                .collect();

            Ok(candidates)
        })
    })
    .join();

    result.ok().and_then(|r| r.ok()).unwrap_or_default()
}

fn complete_task_ids(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_str().unwrap_or_default().to_owned();

    let result = std::thread::spawn(move || -> anyhow::Result<Vec<CompletionCandidate>> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            paths::init(None);
            let client = client::DaemonClient::new()?;
            let tasks = client.list_tasks().await?;

            let candidates = tasks
                .iter()
                .filter(|t| t.id.starts_with(&current))
                .map(|t| {
                    let help = format!("{} ({})", t.description, t.status);
                    CompletionCandidate::new(t.id.to_string()).help(Some(help.into()))
                })
                .collect();

            Ok(candidates)
        })
    })
    .join();

    result.ok().and_then(|r| r.ok()).unwrap_or_default()
}

fn print_env(env: &db::Environment, format: &OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Human => {
            let path = env.metadata["worktree_path"].as_str().unwrap_or("-");
            let branch = env.metadata["branch"].as_str().unwrap_or("-");
            println!(
                "\x1b[1;32m{}\x1b[0m \x1b[2m(id: {})\x1b[0m",
                env.status, env.id
            );
            println!("  \x1b[1mprovider:\x1b[0m  {}", env.provider);
            println!("  \x1b[1mproject:\x1b[0m   {}", env.project_id);
            println!("  \x1b[1mbranch:\x1b[0m    {}", branch);
            println!("  \x1b[1mpath:\x1b[0m      {}", path);
        }
        OutputFormat::Plain => {
            let path = env.metadata["worktree_path"].as_str().unwrap_or("");
            println!("{}\t{}\t{}\t{}", env.id, env.provider, env.status, path);
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(env)?);
        }
    }
    Ok(())
}

fn print_task(task: &db::Task, format: &OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Human => {
            println!(
                "\x1b[1;32m{}\x1b[0m \x1b[2m(id: {})\x1b[0m",
                task.status, task.id
            );
            println!("  \x1b[1mprovider:\x1b[0m      {}", task.provider);
            println!("  \x1b[1mproject:\x1b[0m       {}", task.project_id);
            println!(
                "  \x1b[1menvironment:\x1b[0m   {}",
                task.environment_id.as_deref().unwrap_or("-")
            );
            println!("  \x1b[1mdescription:\x1b[0m   {}", task.description);
        }
        OutputFormat::Plain => {
            println!(
                "{}\t{}\t{}\t{}",
                task.id, task.provider, task.status, task.description
            );
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(task)?);
        }
    }
    Ok(())
}

fn resolve_project(projects: &[db::Project], name: Option<String>) -> anyhow::Result<&db::Project> {
    if let Some(name) = name {
        return projects
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow::anyhow!("project not found: {name}"));
    }

    let cwd = std::env::current_dir()?;
    let cwd = cwd.canonicalize()?;

    projects
        .iter()
        .find(|p| cwd.starts_with(&p.path))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "could not determine project from current directory; specify a project name"
            )
        })
}

async fn follow_task_logs(client: &client::DaemonClient, task_id: &str) -> anyhow::Result<()> {
    use std::io::Write;

    client
        .tail_task_logs(task_id, |chunk| {
            let _ = std::io::stdout().write_all(chunk);
        })
        .await
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let is_daemon = matches!(cli.command, Some(Command::Daemon { .. }));

    paths::init(cli.work_home);
    paths::ensure_dirs()?;

    let config = config::load()?;

    let config_debug = config.daemon.as_ref().is_some_and(|d| d.debug);

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(if cli.debug || config_debug {
            tracing::Level::DEBUG
        } else if is_daemon {
            tracing::Level::INFO
        } else {
            tracing::Level::WARN
        })
        .init();

    match cli.command {
        Some(Command::Daemon { command }) => match command {
            DaemonCommand::Start { force } => daemon::start(force).await?,
            DaemonCommand::Install => daemon::install()?,
            DaemonCommand::Uninstall => daemon::uninstall()?,
        },
        Some(Command::Config { command }) => match command {
            ConfigCommand::Edit => {
                let editor =
                    std::env::var("EDITOR").map_err(|_| anyhow::anyhow!("$EDITOR is not set"))?;
                let path = paths::config_dir()?.join("config.toml");
                std::fs::create_dir_all(path.parent().unwrap())?;
                let status = std::process::Command::new(&editor).arg(&path).status()?;
                if !status.success() {
                    anyhow::bail!("{editor} exited with {status}");
                }
            }
        },
        Some(Command::Version) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        }
        Some(Command::Completions { shell }) => {
            let status = std::process::Command::new(std::env::current_exe()?)
                .env("COMPLETE", shell.to_string())
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        Some(cmd) => {
            let client = client::DaemonClient::new()?;
            match cmd {
                Command::ResetDatabase => client.reset_database().await?,
                Command::Project { command } => match command {
                    ProjectCommand::List { format } => {
                        let projects = client.list_projects().await?;
                        match format {
                            OutputFormat::Human => {
                                if projects.is_empty() {
                                    return Ok(());
                                }
                                let max_name = projects.iter().map(|p| p.name.len()).max().unwrap();
                                let name_width = max_name.max(4);
                                println!("{:<name_width$}  PATH", "NAME");
                                for p in &projects {
                                    println!("{:<name_width$}  {}", p.name, p.path);
                                }
                            }
                            OutputFormat::Plain => {
                                for p in &projects {
                                    println!("{}\t{}", p.name, p.path);
                                }
                            }
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string(&projects)?);
                            }
                        }
                    }
                    ProjectCommand::Remove { name } => {
                        client.delete_project(&name).await?;
                    }
                    ProjectCommand::New { name, path } => {
                        let path = match path {
                            Some(p) => p,
                            None => std::env::current_dir()?,
                        };

                        let path = path.canonicalize()?;

                        let name = match name {
                            Some(n) => n,
                            None => path
                                .file_name()
                                .ok_or_else(|| {
                                    anyhow::anyhow!("could not determine directory name")
                                })?
                                .to_string_lossy()
                                .into_owned(),
                        };

                        client
                            .create_project(&name, &path.to_string_lossy())
                            .await?;
                    }
                },
                Command::Environment { command } => match command {
                    EnvironmentCommand::Create {
                        project,
                        provider,
                        format,
                    } => {
                        let projects = client.list_projects().await?;
                        let proj = resolve_project(&projects, project)?;
                        let provider = provider
                            .or(config.default_environment_provider_for_project(&proj.name))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "--provider is required (or set environment-provider in config)"
                                )
                            })?;
                        let env = client.prepare_environment(&proj.id, &provider).await?;
                        let env = client.claim_environment(&env.id).await?;
                        print_env(&env, &format)?;
                    }
                    EnvironmentCommand::Prepare {
                        project,
                        provider,
                        format,
                    } => {
                        let projects = client.list_projects().await?;
                        let proj = resolve_project(&projects, project)?;
                        let provider = provider
                            .or(config.default_environment_provider_for_project(&proj.name))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "--provider is required (or set environment-provider in config)"
                                )
                            })?;
                        let env = client.prepare_environment(&proj.id, &provider).await?;
                        print_env(&env, &format)?;
                    }
                    EnvironmentCommand::Update { id, format } => {
                        let env = client.update_environment(&id).await?;
                        print_env(&env, &format)?;
                    }
                    EnvironmentCommand::Claim {
                        id,
                        provider,
                        project,
                        format,
                    } => {
                        let env = if let Some(id) = id {
                            client.claim_environment(&id).await?
                        } else {
                            let project_name = project.ok_or_else(|| {
                                anyhow::anyhow!("--project is required when no id is given")
                            })?;
                            let projects = client.list_projects().await?;
                            let proj = projects
                                .iter()
                                .find(|p| p.name == project_name)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("project not found: {project_name}")
                                })?;
                            let provider = provider
                                .or(config.default_environment_provider_for_project(&proj.name))
                                .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "--provider is required when no id is given (or set environment-provider in config)"
                                    )
                                })?;
                            client.claim_next_environment(&provider, &proj.id).await?
                        };
                        print_env(&env, &format)?;
                    }
                    EnvironmentCommand::Remove { id } => {
                        client.remove_environment(&id).await?;
                    }
                    EnvironmentCommand::List { format } => {
                        let envs = client.list_environments().await?;
                        match format {
                            OutputFormat::Human => {
                                if envs.is_empty() {
                                    return Ok(());
                                }
                                println!(
                                    "{:<22}  {:<12}  {:<14}  {:<22}  PATH",
                                    "ID", "PROVIDER", "STATUS", "PROJ"
                                );
                                for e in &envs {
                                    let path = e.metadata["worktree_path"].as_str().unwrap_or("-");
                                    println!(
                                        "{:<22}  {:<12}  {:<14}  {:<22}  {}",
                                        e.id, e.provider, e.status, e.project_id, path
                                    );
                                }
                            }
                            OutputFormat::Plain => {
                                for e in &envs {
                                    println!(
                                        "{}\t{}\t{}\t{}",
                                        e.id, e.provider, e.status, e.project_id
                                    );
                                }
                            }
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string(&envs)?);
                            }
                        }
                    }
                    EnvironmentCommand::Provider { command } => match command {
                        ProviderCommand::List => {
                            for name in &environment::list_providers() {
                                println!("{name}");
                            }
                        }
                    },
                },
                Command::Task { command } => match command {
                    TaskCommand::New {
                        description,
                        project,
                        provider,
                        env_provider,
                        attach,
                        format,
                    } => {
                        let projects = client.list_projects().await?;
                        let proj = resolve_project(&projects, project)?;
                        let task_provider_name = provider
                            .or(config.default_task_provider_for_project(&proj.name))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "--provider is required (or set task-provider in config)"
                                )
                            })?;
                        let env_provider = env_provider
                            .or(config.default_environment_provider_for_project(&proj.name))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "--env-provider is required (or set environment-provider in config)"
                                )
                            })?;

                        config.get_task_provider(&task_provider_name)?;

                        let task = client
                            .create_task(&proj.id, &task_provider_name, &env_provider, &description)
                            .await?;

                        print_task(&task, &format)?;

                        if attach {
                            follow_task_logs(&client, &task.id).await?;
                        }
                    }
                    TaskCommand::Remove { id } => {
                        client.remove_task(&id).await?;
                    }
                    TaskCommand::List { format } => {
                        let tasks = client.list_tasks().await?;
                        match format {
                            OutputFormat::Human => {
                                if tasks.is_empty() {
                                    return Ok(());
                                }
                                println!(
                                    "{:<22}  {:<12}  {:<10}  DESCRIPTION",
                                    "ID", "PROVIDER", "STATUS"
                                );
                                for t in &tasks {
                                    println!(
                                        "{:<22}  {:<12}  {:<10}  {}",
                                        t.id, t.provider, t.status, t.description
                                    );
                                }
                            }
                            OutputFormat::Plain => {
                                for t in &tasks {
                                    println!(
                                        "{}\t{}\t{}\t{}",
                                        t.id, t.provider, t.status, t.description
                                    );
                                }
                            }
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string(&tasks)?);
                            }
                        }
                    }
                },
                Command::Logs { id, follow } => {
                    if follow {
                        follow_task_logs(&client, &id).await?;
                    } else {
                        let log_path = paths::task_log_path(&id)?;
                        if !log_path.exists() {
                            anyhow::bail!("no logs found for task {id}");
                        }
                        let contents = std::fs::read_to_string(&log_path)?;
                        print!("{contents}");
                    }
                }
                Command::Tui => tui::run(client).await?,
                Command::Config { .. }
                | Command::Daemon { .. }
                | Command::Completions { .. }
                | Command::Version => {
                    unreachable!()
                }
            }
        }
        None => {}
    }

    Ok(())
}

use std::io::IsTerminal;

use crate::cli::{self, TaskCdArgs, TaskCommand, TaskDeleteArgs, TaskListArgs, TaskNewArgs};
use crate::client;
use crate::error::{self, CliError};

pub fn execute(command: TaskCommand) -> Result<(), CliError> {
    match command {
        TaskCommand::New(args) => create(args),
        TaskCommand::List(args) => list(args),
        TaskCommand::Cd(args) => cd(args),
        TaskCommand::Delete(args) => delete(args),
    }
}

pub fn create(args: TaskNewArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?;
    let cwd = cwd
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy();

    let resp = client::create_task(
        args.name.as_deref(),
        args.branch.as_deref(),
        args.project.as_deref(),
        &cwd_str,
    )?;

    error::print_success(&format!("Task created: {}", resp.name));
    println!("{}", resp.path);

    if let Some(script) = resp.hook_script {
        let tmp_dir = std::env::temp_dir().join("work-hooks");
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| CliError::with_source("failed to create temp hook directory", e))?;

        let tmp_file = tmp_dir.join(format!("new-after-{}", std::process::id()));
        std::fs::write(&tmp_file, &script)
            .map_err(|e| CliError::with_source("failed to write temp hook script", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_file, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| CliError::with_source("failed to chmod temp hook script", e))?;
        }

        let tmp_display = tmp_file.display();
        if args.no_cd {
            shell_eval(&format!("\"{tmp_display}\""));
        } else {
            shell_eval(&format!("cd \"{}\"\n\"{tmp_display}\"", resp.path));
        }
    } else if !args.no_cd {
        shell_eval(&format!("cd \"{}\"", resp.path));
    }

    Ok(())
}

pub fn cd(args: TaskCdArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy().to_string();

    match args.name {
        Some(name) => {
            let tasks = client::list_tasks(args.project.as_deref(), Some(&cwd_str), false)?;

            let task = tasks.iter().find(|t| t.name == name).ok_or_else(|| {
                CliError::with_hint(
                    "task not found",
                    "Run `work task list` to see available tasks.",
                )
            })?;

            shell_eval(&format!("cd \"{}\"", task.path));
        }
        None => {
            let project = client::detect_project(args.project.as_deref(), &cwd_str)?;
            shell_eval(&format!("cd \"{}\"", project.path));
        }
    }

    Ok(())
}

pub fn list(args: TaskListArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy().to_string();

    let tasks = client::list_tasks(args.project.as_deref(), Some(&cwd_str), args.all)?;

    if args.json {
        let json = serde_json::to_string_pretty(&tasks)
            .map_err(|e| CliError::with_source("failed to serialize tasks", e))?;
        println!("{json}");
        return Ok(());
    }

    if tasks.is_empty() {
        if !args.plain {
            eprintln!("No tasks found.");
        }
        return Ok(());
    }

    if args.plain {
        for task in &tasks {
            if let Some(ref project) = task.project_name {
                println!("{}\t{}\t{}", project, task.name, task.path);
            } else {
                println!("{}\t{}", task.name, task.path);
            }
        }
        return Ok(());
    }

    if args.all {
        let project_width = tasks.iter().fold("PROJECT".len(), |max, t| {
            max.max(t.project_name.as_ref().map_or(0, |n| n.len()))
        });
        let name_width = tasks
            .iter()
            .fold("NAME".len(), |max, t| max.max(t.name.len()));
        println!(
            "{:<project_width$}  {:<name_width$}  PATH",
            "PROJECT", "NAME"
        );
        for task in &tasks {
            println!(
                "{:<project_width$}  {:<name_width$}  {}",
                task.project_name.as_deref().unwrap_or(""),
                task.name,
                task.path
            );
        }
    } else {
        let name_width = tasks
            .iter()
            .fold("NAME".len(), |max, t| max.max(t.name.len()));
        println!("{:<name_width$}  PATH", "NAME");
        for task in &tasks {
            println!("{:<name_width$}  {}", task.name, task.path);
        }
    }

    Ok(())
}

pub fn delete(args: TaskDeleteArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy();

    client::delete_task(&args.name, args.project.as_deref(), &cwd_str, args.force)?;
    error::print_success("Task deleted.");
    Ok(())
}

pub fn nuke(args: cli::NukeArgs) -> Result<(), CliError> {
    if !args.yes {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return Err(CliError::with_hint(
                "cannot confirm in a non-interactive environment",
                "Use --yes to skip the confirmation prompt.",
            ));
        }

        let confirmed = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt("Remove all tasks, pool worktrees, and projects?")
            .default(false)
            .interact()
            .map_err(|e| {
                let dialoguer::Error::IO(source) = e;
                CliError::with_source("failed to read confirmation", source)
            })?;

        if !confirmed {
            return Ok(());
        }
    }

    let resp = client::nuke()?;
    error::print_success(&format!(
        "Removed {} task(s), {} pool worktree(s), and {} project(s).",
        resp.tasks, resp.pool_worktrees, resp.projects
    ));
    Ok(())
}

fn shell_eval(cmd: &str) {
    if let Ok(path) = std::env::var("WORK_SHELL_EVAL") {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{cmd}")
            });
    }
}

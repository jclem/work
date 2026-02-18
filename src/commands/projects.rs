use std::path::PathBuf;

use crate::cli::{ProjectsCommand, ProjectsCreateArgs, ProjectsDeleteArgs, ProjectsListArgs};
use crate::client;
use crate::error::{self, CliError};

pub fn execute(command: ProjectsCommand) -> Result<(), CliError> {
    match command {
        ProjectsCommand::Create(args) => create_project(args),
        ProjectsCommand::List(args) => list_projects(args),
        ProjectsCommand::Delete(args) => delete_project(args),
    }
}

fn create_project(args: ProjectsCreateArgs) -> Result<(), CliError> {
    let project_path = resolve_project_path(args.project_path)?;
    client::create_project(&project_path, args.name.as_deref())?;
    error::print_success("Project created.");
    Ok(())
}

fn list_projects(args: ProjectsListArgs) -> Result<(), CliError> {
    let projects = client::list_projects()?;

    if args.json {
        let json = serde_json::to_string_pretty(&projects)
            .map_err(|e| CliError::with_source("failed to serialize projects", e))?;
        println!("{json}");
        return Ok(());
    }

    if projects.is_empty() {
        if !args.plain {
            eprintln!("No projects found.");
        }
        return Ok(());
    }

    if args.plain {
        for project in &projects {
            println!("{}\t{}", project.name, project.path);
        }
        return Ok(());
    }

    let name_width = projects
        .iter()
        .fold("NAME".len(), |max, p| max.max(p.name.len()));
    println!("{:<name_width$}  PATH", "NAME");
    for project in &projects {
        println!("{:<name_width$}  {}", project.name, project.path);
    }

    Ok(())
}

fn delete_project(args: ProjectsDeleteArgs) -> Result<(), CliError> {
    client::delete_project(&args.project_name)?;
    error::print_success("Project deleted.");
    Ok(())
}

fn resolve_project_path(project_path: Option<PathBuf>) -> Result<String, CliError> {
    let path = match project_path {
        Some(path) => path,
        None => std::env::current_dir()
            .map_err(|e| CliError::with_source("failed to read current directory", e))?,
    };

    let canonical = path
        .canonicalize()
        .map_err(|e| CliError::with_source(format!("failed to resolve {}", path.display()), e))?;

    if !canonical.is_dir() {
        return Err(CliError::with_hint(
            format!("{} is not a directory", canonical.display()),
            "pass a directory path to `work projects create`",
        ));
    }

    Ok(canonical.to_string_lossy().into_owned())
}

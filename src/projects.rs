use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::Serialize;

use crate::cli::{ProjectsCommand, ProjectsCreateArgs, ProjectsDeleteArgs, ProjectsListArgs};
use crate::db;
use crate::error::{self, CliError};

#[derive(Debug, Serialize)]
struct Project {
    name: String,
    path: String,
    #[serde(rename = "createdAt")]
    created_at: i64,
    #[serde(rename = "updatedAt")]
    updated_at: i64,
}

pub fn execute(command: ProjectsCommand) -> Result<(), CliError> {
    match command {
        ProjectsCommand::Create(args) => create_project(args),
        ProjectsCommand::List(args) => list_projects(args),
        ProjectsCommand::Delete(args) => delete_project(args),
    }
}

fn create_project(args: ProjectsCreateArgs) -> Result<(), CliError> {
    let project_path = resolve_project_path(args.project_path)?;
    let name = resolve_project_name(&project_path, args.name)?;

    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let now = unix_timestamp_seconds()?;
    connection
        .execute(
            "INSERT INTO projects (name, path, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4)",
            params![name, project_path, now, now],
        )
        .map_err(map_project_insert_error)?;

    error::print_success("Project created.");
    Ok(())
}

fn list_projects(args: ProjectsListArgs) -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let mut statement = connection
        .prepare("SELECT name, path, createdAt, updatedAt FROM projects ORDER BY name")
        .map_err(|source| CliError::with_source("failed to prepare project query", source))?;

    let rows = statement
        .query_map([], |row| {
            Ok(Project {
                name: row.get(0)?,
                path: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|source| CliError::with_source("failed to query projects", source))?;

    let projects: Vec<Project> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CliError::with_source("failed to load projects", source))?;

    if args.json {
        let json = serde_json::to_string_pretty(&projects)
            .map_err(|source| CliError::with_source("failed to serialize projects", source))?;
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
        .fold("NAME".len(), |max, project| max.max(project.name.len()));
    println!("{:<name_width$}  PATH", "NAME");
    for project in &projects {
        println!("{:<name_width$}  {}", project.name, project.path);
    }

    Ok(())
}

fn delete_project(args: ProjectsDeleteArgs) -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let deleted_rows = connection
        .execute(
            "DELETE FROM projects WHERE name = ?1",
            params![args.project_name],
        )
        .map_err(|source| CliError::with_source("failed to delete project", source))?;

    if deleted_rows == 0 {
        return Err(CliError::with_hint(
            "project not found",
            "run `work projects list` to see existing project names",
        ));
    }

    error::print_success("Project deleted.");
    Ok(())
}

fn resolve_project_path(project_path: Option<PathBuf>) -> Result<String, CliError> {
    let path = match project_path {
        Some(path) => path,
        None => std::env::current_dir()
            .map_err(|source| CliError::with_source("failed to read current directory", source))?,
    };

    let canonical_path = path.canonicalize().map_err(|source| {
        CliError::with_source(format!("failed to resolve {}", path.display()), source)
    })?;

    if !canonical_path.is_dir() {
        return Err(CliError::with_hint(
            format!("{} is not a directory", canonical_path.display()),
            "pass a directory path to `work projects create`",
        ));
    }

    Ok(canonical_path.to_string_lossy().into_owned())
}

fn resolve_project_name(
    project_path: &str,
    explicit_name: Option<String>,
) -> Result<String, CliError> {
    if let Some(name) = explicit_name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(CliError::with_hint(
                "project name cannot be empty",
                "pass a non-empty value to --name",
            ));
        }
        return Ok(trimmed.to_string());
    }

    let path = PathBuf::from(project_path);
    let basename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            CliError::with_hint(
                format!("cannot infer a project name from {}", path.display()),
                "pass --name to set a project name explicitly",
            )
        })?;

    Ok(basename.to_string())
}

fn map_project_insert_error(source: rusqlite::Error) -> CliError {
    match source {
        rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("projects.name") => {
            CliError::with_hint(
                "a project with this name already exists",
                "choose another name or run `work projects delete <project-name>` first",
            )
        }
        rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("projects.path") => {
            CliError::with_hint(
                "a project for this path already exists",
                "run `work projects list` to inspect existing projects",
            )
        }
        other => CliError::with_source("failed to create project", other),
    }
}

fn unix_timestamp_seconds() -> Result<i64, CliError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|source| CliError::with_source("system clock is before unix epoch", source))?;

    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_project_name_uses_explicit_name() {
        let name = resolve_project_name("/tmp/demo", Some("custom".to_string())).unwrap();
        assert_eq!(name, "custom");
    }

    #[test]
    fn resolve_project_name_uses_path_basename() {
        let name = resolve_project_name("/tmp/demo", None).unwrap();
        assert_eq!(name, "demo");
    }
}

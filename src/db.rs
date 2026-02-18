use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::error::CliError;
use crate::paths;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  path TEXT NOT NULL UNIQUE,
  createdAt INTEGER NOT NULL,
  updatedAt INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY,
  projectId INTEGER NOT NULL,
  name TEXT NOT NULL,
  path TEXT NOT NULL,
  createdAt INTEGER NOT NULL,
  updatedAt INTEGER NOT NULL,
  FOREIGN KEY (projectId) REFERENCES projects(id) ON DELETE CASCADE,
  UNIQUE (projectId, name),
  UNIQUE (projectId, path)
);

CREATE TABLE IF NOT EXISTS pool (
  id INTEGER PRIMARY KEY,
  projectId INTEGER NOT NULL,
  tempName TEXT NOT NULL,
  path TEXT NOT NULL,
  createdAt INTEGER NOT NULL,
  FOREIGN KEY (projectId) REFERENCES projects(id) ON DELETE CASCADE,
  UNIQUE (projectId, tempName)
);

CREATE TABLE IF NOT EXISTS jobs (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  createdAt INTEGER NOT NULL
);
"#;

pub fn open_database() -> Result<Connection, CliError> {
    let database_path = paths::database_path();
    ensure_parent_dir(&database_path)?;

    let connection = Connection::open(&database_path).map_err(|source| {
        CliError::with_source(
            format!("failed to open {}", database_path.display()),
            source,
        )
    })?;

    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|source| CliError::with_source("failed to enable sqlite foreign keys", source))?;

    Ok(connection)
}

pub fn prepare_schema(connection: &Connection) -> Result<(), CliError> {
    connection
        .execute_batch(SCHEMA_SQL)
        .map_err(|source| CliError::with_source("failed to prepare database schema", source))?;

    // Idempotent migrations via ALTER TABLE; ignore "duplicate column" errors.
    for stmt in &[
        "ALTER TABLE tasks ADD COLUMN status TEXT NOT NULL DEFAULT 'active'",
        "ALTER TABLE tasks ADD COLUMN deleteForce INTEGER NOT NULL DEFAULT 0",
    ] {
        match connection.execute_batch(stmt) {
            Ok(()) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => {
                return Err(CliError::with_source(
                    "failed to migrate database schema",
                    e,
                ));
            }
        }
    }

    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            CliError::with_source(format!("failed to create {}", parent.display()), source)
        })?;
    }

    Ok(())
}

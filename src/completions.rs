use std::ffi::OsStr;

use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use rusqlite::params;

use crate::db;

pub fn task_name_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(complete_task_names)
}

fn complete_task_names(_current: &OsStr) -> Vec<CompletionCandidate> {
    task_name_candidates().unwrap_or_default()
}

fn task_name_candidates() -> Option<Vec<CompletionCandidate>> {
    let connection = db::open_database().ok()?;
    db::prepare_schema(&connection).ok()?;

    let project_id = detect_project_id(&connection)?;

    let mut stmt = connection
        .prepare("SELECT name FROM tasks WHERE projectId = ?1 ORDER BY name")
        .ok()?;

    let rows = stmt
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .ok()?;

    Some(
        rows.filter_map(|r| r.ok())
            .map(CompletionCandidate::new)
            .collect(),
    )
}

fn detect_project_id(connection: &rusqlite::Connection) -> Option<i64> {
    let cwd = std::env::current_dir().ok()?;
    let cwd = cwd.canonicalize().ok()?;
    let cwd_str = cwd.to_string_lossy();

    let mut stmt = connection.prepare("SELECT id, path FROM projects").ok()?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;

    let mut best_id = None;
    let mut best_len = 0;

    for row in rows {
        let (id, path) = row.ok()?;
        if cwd_str.starts_with(&path)
            && (cwd_str.len() == path.len() || cwd_str.as_bytes()[path.len()] == b'/')
            && path.len() > best_len
        {
            best_len = path.len();
            best_id = Some(id);
        }
    }

    best_id
}

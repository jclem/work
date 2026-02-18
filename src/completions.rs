use std::ffi::OsStr;

use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use rusqlite::params;

use crate::db;

pub fn task_name_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(complete_task_names)
}

pub fn branch_name_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(complete_branch_names)
}

fn complete_task_names(_current: &OsStr) -> Vec<CompletionCandidate> {
    task_name_candidates().unwrap_or_default()
}

fn complete_branch_names(_current: &OsStr) -> Vec<CompletionCandidate> {
    branch_name_candidates().unwrap_or_default()
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

fn branch_name_candidates() -> Option<Vec<CompletionCandidate>> {
    let connection = db::open_database().ok()?;
    db::prepare_schema(&connection).ok()?;

    let project_path = detect_project_path(&connection)?;

    let output = std::process::Command::new("git")
        .args(["-C", &project_path, "branch", "--format=%(refname:short)"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(|line| CompletionCandidate::new(line.to_string()))
            .collect(),
    )
}

fn detect_project_path(connection: &rusqlite::Connection) -> Option<String> {
    let (_, path) = detect_project(connection)?;
    Some(path)
}

fn detect_project(connection: &rusqlite::Connection) -> Option<(i64, String)> {
    let cwd = std::env::current_dir().ok()?;
    let cwd = cwd.canonicalize().ok()?;
    let cwd_str = cwd.to_string_lossy();

    let mut stmt = connection
        .prepare("SELECT id, name, path FROM projects")
        .ok()?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .ok()?;

    let projects: Vec<(i64, String, String)> = rows.filter_map(|r| r.ok()).collect();

    let mut best: Option<(i64, String)> = None;
    let mut best_len = 0;

    for (id, _, path) in &projects {
        if cwd_str.starts_with(path)
            && (cwd_str.len() == path.len() || cwd_str.as_bytes()[path.len()] == b'/')
            && path.len() > best_len
        {
            best_len = path.len();
            best = Some((*id, path.clone()));
        }
    }

    if best.is_none() {
        for (id, name, path) in &projects {
            let wt_base = crate::paths::project_worktrees_dir(name);
            let wt_base_str = wt_base.to_string_lossy();
            if cwd_str.starts_with(wt_base_str.as_ref())
                && (cwd_str.len() == wt_base_str.len()
                    || cwd_str.as_bytes()[wt_base_str.len()] == b'/')
            {
                best = Some((*id, path.clone()));
                break;
            }
        }
    }

    best
}

fn detect_project_id(connection: &rusqlite::Connection) -> Option<i64> {
    let (id, _) = detect_project(connection)?;
    Some(id)
}

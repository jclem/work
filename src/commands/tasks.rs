use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;
use rusqlite::params;
use serde::Serialize;

use crate::adapters::TaskAdapter;
use crate::adapters::worktree::GitWorktreeAdapter;
use crate::cli::{DeleteArgs, ListArgs, NewArgs};
use crate::config;
use crate::db;
use crate::error::{self, CliError};
use crate::paths;

const ADJECTIVES: &[&str] = &[
    "amber", "bold", "calm", "dark", "eager", "fair", "glad", "happy", "idle", "jolly", "keen",
    "lush", "mild", "neat", "open", "pale", "quick", "rare", "safe", "tall", "vast", "warm",
    "young", "zen", "agile", "brave", "crisp", "deep", "even", "fresh", "green", "huge", "icy",
    "just", "kind", "lean", "mossy", "noble", "odd", "plain", "quiet", "rapid", "sharp", "tidy",
    "ultra", "vivid", "wild", "extra", "zesty", "dry",
];

const NOUNS: &[&str] = &[
    "ant", "bear", "cat", "deer", "elk", "fox", "goat", "hare", "ibis", "jay", "kite", "lark",
    "mole", "newt", "owl", "puma", "quail", "ram", "seal", "toad", "urchin", "vole", "wolf", "yak",
    "zebra", "ape", "bass", "crab", "dove", "eel", "frog", "gull", "hawk", "iguana", "jackal",
    "koala", "lion", "moose", "narwhal", "otter", "parrot", "robin", "snake", "tiger", "vulture",
    "whale", "wren", "ox", "finch", "crane",
];

#[derive(Debug, Serialize)]
struct Task {
    name: String,
    path: String,
    #[serde(rename = "projectName", skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: i64,
    #[serde(rename = "updatedAt")]
    updated_at: i64,
}

pub fn create(args: NewArgs) -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let (project_id, project_name, project_path) =
        detect_project(&connection, args.project.as_deref())?;
    let task_name = args.name.unwrap_or_else(generate_task_name);
    let worktree_path = paths::worktree_path(&project_name, &task_name);

    let adapter = GitWorktreeAdapter;
    let global_cfg = config::load()?;
    let default_branch = config::effective_default_branch(&global_cfg, &project_name, &project_path);

    // Attempt to claim a pre-warmed worktree from the pool.
    let claimed = try_claim_pool(&connection, &adapter, project_id, &project_path, &task_name, &worktree_path);

    if !claimed {
        adapter.create(&project_path, &task_name, &worktree_path, &default_branch)?;
    }

    let now = unix_timestamp_seconds()?;
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    connection
        .execute(
            "INSERT INTO tasks (projectId, name, path, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, task_name, worktree_path_str, now, now],
        )
        .map_err(|source| CliError::with_source("failed to create task", source))?;

    error::print_success(&format!("Task created: {task_name}"));
    println!("{}", worktree_path.display());

    // Resolve hook script: project .work/config.toml takes priority over global config.
    let worktree_display = worktree_path.display();
    let project_cfg = config::load_project_config(&project_path)?;
    let hook_script = config::project_hook_script(&project_cfg, "new-after");

    let hook_script = match hook_script {
        Some(s) => Some(s),
        None => config::hook_script(&global_cfg, &project_name, "new-after"),
    };

    if let Some(script) = hook_script {
        let tmp_dir = std::env::temp_dir().join("work-hooks");
        std::fs::create_dir_all(&tmp_dir).map_err(|source| {
            CliError::with_source("failed to create temp hook directory", source)
        })?;

        let tmp_file = tmp_dir.join(format!("new-after-{}", std::process::id()));
        std::fs::write(&tmp_file, script).map_err(|source| {
            CliError::with_source("failed to write temp hook script", source)
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_file, std::fs::Permissions::from_mode(0o755))
                .map_err(|source| {
                    CliError::with_source("failed to chmod temp hook script", source)
                })?;
        }

        let tmp_display = tmp_file.display();
        if args.no_cd {
            shell_eval(&format!("\"{tmp_display}\""));
        } else {
            shell_eval(&format!(
                "cd \"{worktree_display}\"\n\"{tmp_display}\""
            ));
        }
    } else if !args.no_cd {
        shell_eval(&format!("cd \"{worktree_display}\""));
    }

    Ok(())
}

pub fn list(args: ListArgs) -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let tasks = if args.all {
        let mut stmt = connection
            .prepare(
                "SELECT t.name, t.path, t.createdAt, t.updatedAt, p.name \
                 FROM tasks t JOIN projects p ON t.projectId = p.id \
                 WHERE t.status = 'active' \
                 ORDER BY p.name, t.name",
            )
            .map_err(|source| CliError::with_source("failed to prepare task query", source))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Task {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    project_name: Some(row.get(4)?),
                })
            })
            .map_err(|source| CliError::with_source("failed to query tasks", source))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|source| CliError::with_source("failed to load tasks", source))?
    } else {
        let (project_id, _, _) = detect_project(&connection, args.project.as_deref())?;

        let mut stmt = connection
            .prepare(
                "SELECT name, path, createdAt, updatedAt \
                 FROM tasks WHERE projectId = ?1 AND status = 'active' ORDER BY name",
            )
            .map_err(|source| CliError::with_source("failed to prepare task query", source))?;

        let rows = stmt
            .query_map(params![project_id], |row| {
                Ok(Task {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    project_name: None,
                })
            })
            .map_err(|source| CliError::with_source("failed to query tasks", source))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|source| CliError::with_source("failed to load tasks", source))?
    };

    if args.json {
        let json = serde_json::to_string_pretty(&tasks)
            .map_err(|source| CliError::with_source("failed to serialize tasks", source))?;
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

pub fn delete(args: DeleteArgs) -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let (project_id, _, _) = detect_project(&connection, args.project.as_deref())?;

    // Verify the task exists and is active (not already queued for deletion).
    connection
        .query_row(
            "SELECT path FROM tasks WHERE projectId = ?1 AND name = ?2 AND status = 'active'",
            params![project_id, args.name],
            |row| row.get::<_, String>(0),
        )
        .map_err(|source| match source {
            rusqlite::Error::QueryReturnedNoRows => {
                CliError::with_hint("task not found", "run `work list` to see existing tasks")
            }
            other => CliError::with_source("failed to look up task", other),
        })?;

    connection
        .execute(
            "UPDATE tasks SET status = 'deleting', deleteForce = ?1 WHERE projectId = ?2 AND name = ?3",
            params![args.force as i32, project_id, args.name],
        )
        .map_err(|source| CliError::with_source("failed to mark task for deletion", source))?;

    error::print_success("Task queued for deletion.");

    crate::client::notify_daemon();

    Ok(())
}

pub fn nuke() -> Result<(), CliError> {
    let connection = db::open_database()?;
    db::prepare_schema(&connection)?;

    let adapter = GitWorktreeAdapter;

    // Remove pool worktrees first.
    let mut pool_stmt = connection
        .prepare(
            "SELECT po.tempName, po.path, p.path \
             FROM pool po JOIN projects p ON po.projectId = p.id",
        )
        .map_err(|source| CliError::with_source("failed to query pool entries", source))?;

    let pool_entries: Vec<(String, String, String)> = pool_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|source| CliError::with_source("failed to query pool entries", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CliError::with_source("failed to load pool entries", source))?;

    for (temp_name, pool_path, project_path) in &pool_entries {
        let _ = adapter.remove(project_path, temp_name, Path::new(pool_path), true);
    }

    connection
        .execute("DELETE FROM pool", [])
        .map_err(|source| CliError::with_source("failed to delete pool entries", source))?;

    // Remove task worktrees.
    let mut stmt = connection
        .prepare(
            "SELECT t.name, t.path, p.path \
             FROM tasks t JOIN projects p ON t.projectId = p.id",
        )
        .map_err(|source| CliError::with_source("failed to query tasks", source))?;

    let tasks: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|source| CliError::with_source("failed to query tasks", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CliError::with_source("failed to load tasks", source))?;

    for (task_name, task_path, project_path) in &tasks {
        adapter.remove(project_path, task_name, Path::new(task_path), true)?;
    }

    connection
        .execute("DELETE FROM tasks", [])
        .map_err(|source| CliError::with_source("failed to delete tasks", source))?;

    let deleted_projects = connection
        .execute("DELETE FROM projects", [])
        .map_err(|source| CliError::with_source("failed to delete projects", source))?;

    error::print_success(&format!(
        "Removed {} task(s), {} pool worktree(s), and {} project(s).",
        tasks.len(),
        pool_entries.len(),
        deleted_projects
    ));
    Ok(())
}

fn try_claim_pool(
    connection: &rusqlite::Connection,
    adapter: &GitWorktreeAdapter,
    project_id: i64,
    project_path: &str,
    task_name: &str,
    worktree_path: &std::path::PathBuf,
) -> bool {
    // Atomically select-and-delete the oldest pool entry for this project.
    let result: Result<(i64, String, String), _> = connection.query_row(
        "DELETE FROM pool WHERE id = (
            SELECT id FROM pool WHERE projectId = ?1 ORDER BY createdAt ASC LIMIT 1
        ) RETURNING id, tempName, path",
        params![project_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match result {
        Ok((_pool_id, temp_name, old_path_str)) => {
            let old_path = std::path::Path::new(&old_path_str);

            match adapter.claim_pooled(project_path, &temp_name, task_name, old_path, worktree_path) {
                Ok(()) => {
                    // Trigger pool replenishment in background.
                    crate::client::notify_pool_replenish();
                    true
                }
                Err(e) => {
                    eprintln!("pool claim failed ({e}), falling back to normal creation");
                    false
                }
            }
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => false,
        Err(_) => false,
    }
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

fn detect_project(
    connection: &rusqlite::Connection,
    explicit_project: Option<&str>,
) -> Result<(i64, String, String), CliError> {
    if let Some(name) = explicit_project {
        let row = connection
            .query_row(
                "SELECT id, name, path FROM projects WHERE name = ?1",
                params![name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|source| match source {
                rusqlite::Error::QueryReturnedNoRows => CliError::with_hint(
                    format!("project '{name}' not found"),
                    "run `work projects list` to see existing projects",
                ),
                other => CliError::with_source("failed to look up project", other),
            })?;
        return Ok(row);
    }

    let cwd = std::env::current_dir()
        .map_err(|source| CliError::with_source("failed to read current directory", source))?;
    let cwd = cwd.canonicalize().map_err(|source| {
        CliError::with_source("failed to canonicalize current directory", source)
    })?;
    let cwd_str = cwd.to_string_lossy();

    let mut stmt = connection
        .prepare("SELECT id, name, path FROM projects")
        .map_err(|source| CliError::with_source("failed to query projects", source))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|source| CliError::with_source("failed to query projects", source))?;

    let mut best: Option<(i64, String, String)> = None;
    let mut best_len = 0;

    for row in rows {
        let (id, name, path) =
            row.map_err(|source| CliError::with_source("failed to load project", source))?;

        if cwd_str.starts_with(&path)
            && (cwd_str.len() == path.len() || cwd_str.as_bytes()[path.len()] == b'/')
            && path.len() > best_len
        {
            best_len = path.len();
            best = Some((id, name, path));
        }
    }

    best.ok_or_else(|| {
        CliError::with_hint(
            "could not detect project from current directory",
            "run `work projects create` to register a project, or pass --project",
        )
    })
}

fn generate_task_name() -> String {
    let date = today_date_string();
    let mut rng = rand::thread_rng();
    let adj = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    format!("{date}-{adj}-{noun}")
}

fn today_date_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs();
    let days = (secs / 86400) as i64;

    // Civil date from days since epoch (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
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
    fn generate_task_name_has_date_prefix() {
        let name = generate_task_name();
        let expected_prefix = today_date_string();
        assert!(
            name.starts_with(&expected_prefix),
            "expected '{name}' to start with '{expected_prefix}'"
        );
    }

    #[test]
    fn generate_task_name_has_three_parts_after_date() {
        let name = generate_task_name();
        // Format: YYYY-MM-DD-adjective-noun (5 dash-separated parts)
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 5, "expected 5 parts in '{name}'");
    }

    #[test]
    fn today_date_string_format() {
        let date = today_date_string();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }
}

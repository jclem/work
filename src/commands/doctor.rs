use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process;
use std::time::Duration;

use rusqlite::Connection;

use crate::db;
use crate::error::CliError;
use crate::paths;

enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

struct CheckResult {
    label: String,
    status: CheckStatus,
    message: Option<String>,
    hint: Option<String>,
}

impl CheckResult {
    fn pass(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: CheckStatus::Pass,
            message: None,
            hint: None,
        }
    }

    fn warn_with_hint(
        label: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status: CheckStatus::Warn,
            message: Some(message.into()),
            hint: Some(hint.into()),
        }
    }

    fn fail(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: CheckStatus::Fail,
            message: Some(message.into()),
            hint: None,
        }
    }

    fn fail_with_hint(
        label: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status: CheckStatus::Fail,
            message: Some(message.into()),
            hint: Some(hint.into()),
        }
    }
}

pub fn run() -> Result<(), CliError> {
    let mut results = Vec::new();

    let conn = check_database(&mut results);
    let daemon_alive = check_daemon(&mut results);

    if let Some(conn) = &conn {
        check_projects(conn, &mut results);
        check_tasks(conn, &mut results);
        check_pool(conn, &mut results);
        check_sessions(conn, &mut results, daemon_alive);
        check_orphans(conn, &mut results);
    }

    print_results(&results);

    let has_failures = results
        .iter()
        .any(|r| matches!(r.status, CheckStatus::Fail));

    if has_failures {
        Err(CliError::new("doctor found problems"))
    } else {
        Ok(())
    }
}

fn check_database(results: &mut Vec<CheckResult>) -> Option<Connection> {
    let conn = match db::open_database() {
        Ok(c) => c,
        Err(e) => {
            results.push(CheckResult::fail("database", format!("cannot open: {e}")));
            return None;
        }
    };

    if let Err(e) = db::prepare_schema(&conn) {
        results.push(CheckResult::fail("database", format!("schema error: {e}")));
        return None;
    }

    results.push(CheckResult::pass("database"));
    Some(conn)
}

fn check_projects(conn: &Connection, results: &mut Vec<CheckResult>) {
    let mut stmt = match conn.prepare("SELECT name, path FROM projects ORDER BY name") {
        Ok(s) => s,
        Err(e) => {
            results.push(CheckResult::fail("projects", format!("query failed: {e}")));
            return;
        }
    };

    let rows: Vec<(String, String)> = match stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(r) => r,
        Err(e) => {
            results.push(CheckResult::fail("projects", format!("query failed: {e}")));
            return;
        }
    };

    if rows.is_empty() {
        results.push(CheckResult::pass("projects: none registered"));
        return;
    }

    let mut failures = Vec::new();

    for (name, path) in &rows {
        let p = Path::new(path);
        if !p.exists() {
            failures.push(CheckResult::fail_with_hint(
                format!("project '{name}'"),
                format!("path does not exist: {path}"),
                "run `work projects delete` to remove stale projects",
            ));
        } else if !is_git_repo(p) {
            failures.push(CheckResult::fail(
                format!("project '{name}'"),
                format!("not a git repo: {path}"),
            ));
        }
    }

    if failures.is_empty() {
        results.push(CheckResult::pass(format!(
            "projects: {} registered",
            rows.len()
        )));
    } else {
        results.append(&mut failures);
    }
}

fn check_tasks(conn: &Connection, results: &mut Vec<CheckResult>) {
    let mut stmt = match conn.prepare(
        "SELECT t.name, t.path, p.name, p.path \
         FROM tasks t JOIN projects p ON t.projectId = p.id \
         WHERE t.status = 'active' \
         ORDER BY p.name, t.name",
    ) {
        Ok(s) => s,
        Err(e) => {
            results.push(CheckResult::fail("tasks", format!("query failed: {e}")));
            return;
        }
    };

    let rows: Vec<(String, String, String, String)> = match stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(r) => r,
        Err(e) => {
            results.push(CheckResult::fail("tasks", format!("query failed: {e}")));
            return;
        }
    };

    if rows.is_empty() {
        results.push(CheckResult::pass("tasks: none active"));
        return;
    }

    let mut failures = Vec::new();

    for (task_name, task_path, project_name, project_path) in &rows {
        let p = Path::new(task_path);
        if !p.exists() {
            failures.push(CheckResult::fail(
                format!("task '{project_name}/{task_name}'"),
                format!("worktree path missing: {task_path}"),
            ));
        }

        if !branch_exists(Path::new(project_path), task_name) {
            failures.push(CheckResult::fail(
                format!("task '{project_name}/{task_name}'"),
                format!("git branch '{task_name}' not found in project repo"),
            ));
        }
    }

    if failures.is_empty() {
        results.push(CheckResult::pass(format!("tasks: {} active", rows.len())));
    } else {
        results.append(&mut failures);
    }
}

fn check_pool(conn: &Connection, results: &mut Vec<CheckResult>) {
    let mut stmt = match conn.prepare(
        "SELECT po.tempName, po.path, p.name, p.path \
         FROM pool po JOIN projects p ON po.projectId = p.id \
         ORDER BY p.name, po.tempName",
    ) {
        Ok(s) => s,
        Err(e) => {
            results.push(CheckResult::fail("pool", format!("query failed: {e}")));
            return;
        }
    };

    let rows: Vec<(String, String, String, String)> = match stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(r) => r,
        Err(e) => {
            results.push(CheckResult::fail("pool", format!("query failed: {e}")));
            return;
        }
    };

    if rows.is_empty() {
        results.push(CheckResult::pass("pool: empty"));
        return;
    }

    let mut failures = Vec::new();

    for (temp_name, pool_path, project_name, project_path) in &rows {
        let p = Path::new(pool_path);
        if !p.exists() {
            failures.push(CheckResult::fail(
                format!("pool '{project_name}/{temp_name}'"),
                format!("worktree path missing: {pool_path}"),
            ));
        }

        if !branch_exists(Path::new(project_path), temp_name) {
            failures.push(CheckResult::fail(
                format!("pool '{project_name}/{temp_name}'"),
                format!("git branch '{temp_name}' not found in project repo"),
            ));
        }
    }

    if failures.is_empty() {
        results.push(CheckResult::pass(format!("pool: {} entries", rows.len())));
    } else {
        results.append(&mut failures);
    }
}

fn check_sessions(conn: &Connection, results: &mut Vec<CheckResult>, daemon_alive: bool) {
    // Count sessions by status.
    let mut stmt = match conn
        .prepare("SELECT status, COUNT(*) FROM sessions GROUP BY status ORDER BY status")
    {
        Ok(s) => s,
        Err(e) => {
            results.push(CheckResult::fail("sessions", format!("query failed: {e}")));
            return;
        }
    };

    let counts: Vec<(String, i64)> = match stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(r) => r,
        Err(e) => {
            results.push(CheckResult::fail("sessions", format!("query failed: {e}")));
            return;
        }
    };

    if counts.is_empty() {
        results.push(CheckResult::pass("sessions: none"));
        return;
    }

    let total: i64 = counts.iter().map(|(_, c)| c).sum();
    let running = counts
        .iter()
        .find(|(s, _)| s == "running")
        .map_or(0, |(_, c)| *c);
    let planned = counts
        .iter()
        .find(|(s, _)| s == "planned")
        .map_or(0, |(_, c)| *c);

    // Only warn about orphaned running sessions when the daemon is down.
    // When the daemon is alive, it manages running sessions — they're expected.
    if !daemon_alive {
        if running > 0 {
            results.push(CheckResult::warn_with_hint(
                "sessions",
                format!("{running} session(s) stuck in 'running' status"),
                "restart the daemon to recover them, or use `work session stop <ID>`",
            ));
        }

        // Check for orphaned agent processes.
        if let Ok(mut pid_stmt) = conn
            .prepare("SELECT id, pid FROM sessions WHERE status = 'running' AND pid IS NOT NULL")
        {
            let orphaned_procs: Vec<(i64, i64)> = pid_stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                .unwrap_or_default();

            for (id, pid) in &orphaned_procs {
                let alive = unsafe { libc::kill(*pid as i32, 0) } == 0;
                if alive {
                    results.push(CheckResult::warn_with_hint(
                        format!("session {id}"),
                        format!("orphaned agent process (pid {pid}) still running"),
                        format!("stop with `work session stop {id}`"),
                    ));
                }
            }
        }
    }

    // Check for sessions with missing worktrees.
    let mut wt_stmt = match conn.prepare(
        "SELECT s.id, s.branchName, t.path \
         FROM sessions s \
         LEFT JOIN tasks t ON s.taskId = t.id \
         WHERE s.status IN ('planned', 'running', 'reported')",
    ) {
        Ok(s) => s,
        Err(_) => {
            let summary = format!("{total} total ({planned} planned, {running} running)");
            results.push(CheckResult::pass(format!("sessions: {summary}")));
            return;
        }
    };

    let active_sessions: Vec<(i64, String, Option<String>)> = wt_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .unwrap_or_default();

    let mut missing_worktrees = Vec::new();
    for (id, branch, task_path) in &active_sessions {
        match task_path {
            None => {
                missing_worktrees.push(CheckResult::fail_with_hint(
                    format!("session {id} ({branch})"),
                    "task record missing (deleted while session active)",
                    format!("delete with `work session delete {id}`"),
                ));
            }
            Some(path) if !Path::new(path).exists() => {
                missing_worktrees.push(CheckResult::fail_with_hint(
                    format!("session {id} ({branch})"),
                    format!("worktree missing: {path}"),
                    format!("delete with `work session delete {id}`"),
                ));
            }
            _ => {}
        }
    }

    if missing_worktrees.is_empty() {
        let summary = format!("{total} total ({planned} planned, {running} running)");
        results.push(CheckResult::pass(format!("sessions: {summary}")));
    } else {
        results.append(&mut missing_worktrees);
    }
}

fn check_daemon(results: &mut Vec<CheckResult>) -> bool {
    let pid_path = paths::pid_file_path();

    let pid_content = match fs::read_to_string(&pid_path) {
        Ok(s) => s,
        Err(_) => {
            results.push(CheckResult::warn_with_hint(
                "daemon",
                "not running (no PID file)",
                "start the daemon with `work daemon start`",
            ));
            return false;
        }
    };

    let pid: u32 = match pid_content.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            results.push(CheckResult::fail("daemon", "invalid PID file contents"));
            return false;
        }
    };

    if !is_process_alive(pid) {
        results.push(CheckResult::fail_with_hint(
            "daemon",
            format!("PID {pid} is not running (stale PID file)"),
            "start the daemon with `work daemon start`",
        ));
        return false;
    }

    let socket_path = paths::socket_path(None);
    match probe_healthz(&socket_path) {
        Ok(()) => {
            results.push(CheckResult::pass(format!("daemon: running (pid {pid})")));
            true
        }
        Err(e) => {
            results.push(CheckResult::fail(
                "daemon",
                format!("pid {pid} alive but healthz failed: {e}"),
            ));
            false
        }
    }
}

fn check_orphans(conn: &Connection, results: &mut Vec<CheckResult>) {
    let projects_dir = paths::projects_dir();

    if !projects_dir.exists() {
        return;
    }

    // Collect all known worktree paths from tasks and pool.
    let mut known_paths = HashSet::new();

    if let Ok(mut stmt) = conn.prepare("SELECT path FROM tasks WHERE status = 'active'")
        && let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0))
    {
        for row in rows.flatten() {
            known_paths.insert(row);
        }
    }

    if let Ok(mut stmt) = conn.prepare("SELECT path FROM pool")
        && let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0))
    {
        for row in rows.flatten() {
            known_paths.insert(row);
        }
    }

    // Also include "deleting" tasks so we don't report them as orphans.
    if let Ok(mut stmt) = conn.prepare("SELECT path FROM tasks WHERE status = 'deleting'")
        && let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0))
    {
        for row in rows.flatten() {
            known_paths.insert(row);
        }
    }

    // Scan projects/<project>/worktrees/<entry> on disk.
    let mut orphans = Vec::new();

    let project_dirs = match fs::read_dir(&projects_dir) {
        Ok(d) => d,
        Err(_) => return,
    };

    for project_entry in project_dirs.flatten() {
        let worktrees_dir = project_entry.path().join("worktrees");
        if !worktrees_dir.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&worktrees_dir) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }

            let path_str = entry_path.to_string_lossy().to_string();
            if !known_paths.contains(&path_str) {
                orphans.push(path_str);
            }
        }
    }

    if orphans.is_empty() {
        return;
    }

    for orphan in &orphans {
        results.push(CheckResult::warn_with_hint(
            "orphan",
            format!("untracked worktree: {orphan}"),
            "manually remove it or re-create the task",
        ));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_git_repo(path: &Path) -> bool {
    process::Command::new("git")
        .args(["-C", &path.to_string_lossy(), "rev-parse", "--git-dir"])
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn branch_exists(project_path: &Path, branch_name: &str) -> bool {
    let output = process::Command::new("git")
        .args([
            "-C",
            &project_path.to_string_lossy(),
            "branch",
            "--list",
            branch_name,
        ])
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::null())
        .output();

    match output {
        Ok(o) => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        Err(_) => false,
    }
}

fn is_process_alive(pid: u32) -> bool {
    process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn probe_healthz(socket_path: &Path) -> Result<(), String> {
    let mut stream =
        UnixStream::connect(socket_path).map_err(|e| format!("connect failed: {e}"))?;

    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("set timeout: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("set timeout: {e}"))?;

    let request = "GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("read failed: {e}"))?;

    let response = String::from_utf8_lossy(&buf);
    if response.contains("200") {
        Ok(())
    } else {
        Err(format!(
            "unexpected response: {}",
            response.lines().next().unwrap_or("")
        ))
    }
}

fn print_results(results: &[CheckResult]) {
    let mut stderr = anstream::stderr();

    let green =
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)));
    let yellow =
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)));
    let red = anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)));
    let cyan = anstyle::Style::new()
        .bold()
        .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan)));
    let dimmed = anstyle::Style::new().dimmed();

    for result in results {
        match result.status {
            CheckStatus::Pass => {
                let _ = writeln!(stderr, "{green}\u{2714}{green:#} {}", result.label);
            }
            CheckStatus::Warn => {
                let msg = result.message.as_deref().unwrap_or("");
                let _ = writeln!(stderr, "{yellow}!{yellow:#} {}: {msg}", result.label);
                if let Some(hint) = &result.hint {
                    let _ = writeln!(stderr, "  {cyan}hint:{cyan:#} {dimmed}{hint}{dimmed:#}");
                }
            }
            CheckStatus::Fail => {
                let msg = result.message.as_deref().unwrap_or("");
                let _ = writeln!(stderr, "{red}\u{2717}{red:#} {}: {msg}", result.label);
                if let Some(hint) = &result.hint {
                    let _ = writeln!(stderr, "  {cyan}hint:{cyan:#} {dimmed}{hint}{dimmed:#}");
                }
            }
        }
    }

    let passes = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Pass))
        .count();
    let warns = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Warn))
        .count();
    let fails = results
        .iter()
        .filter(|r| matches!(r.status, CheckStatus::Fail))
        .count();

    let _ = writeln!(stderr);

    if fails > 0 {
        let _ = writeln!(
            stderr,
            "{passes} passed, {warns} warning(s), {red}{fails} failed{red:#}"
        );
    } else if warns > 0 {
        let _ = writeln!(
            stderr,
            "{passes} passed, {yellow}{warns} warning(s){yellow:#}, 0 failed"
        );
    } else {
        let _ = writeln!(
            stderr,
            "{green}{passes} passed{green:#}, 0 warning(s), 0 failed"
        );
    }
}

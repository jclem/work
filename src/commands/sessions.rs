use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use crate::cli::{
    SessionDeleteArgs, SessionListArgs, SessionLogsArgs, SessionOpenArgs, SessionPickArgs,
    SessionRankArgs, SessionRejectArgs, SessionShowArgs, SessionStartArgs, SessionStopArgs,
};
use crate::client;
use crate::error::{self, CliError};
use crate::workd::SessionInfo;

pub fn start(args: SessionStartArgs) -> Result<(), CliError> {
    let issue = match args.issue {
        Some(text) => text,
        None => read_stdin_issue()?,
    };

    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy();

    let resp = client::start_sessions(&issue, args.agents, args.project.as_deref(), &cwd_str)?;

    error::print_success(&format!(
        "Started {} session(s) for issue",
        resp.sessions.len()
    ));

    for session in &resp.sessions {
        eprintln!(
            "  session {} (attempt {}) -> {}",
            session.id, session.attempt_no, session.branch_name
        );
    }

    Ok(())
}

pub fn list(args: SessionListArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy().to_string();

    let sessions = client::list_sessions(
        args.issue.as_deref(),
        args.project.as_deref(),
        Some(&cwd_str),
    )?;

    if args.json {
        let json = serde_json::to_string_pretty(&sessions)
            .map_err(|e| CliError::with_source("failed to serialize sessions", e))?;
        println!("{json}");
        return Ok(());
    }

    if sessions.is_empty() {
        if !args.plain {
            eprintln!("No sessions found.");
        }
        return Ok(());
    }

    if args.plain {
        for s in &sessions {
            let mergeable = match s.mergeable {
                Some(true) => "yes",
                Some(false) => "no",
                None => "-",
            };
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                s.id, s.issue_ref, s.attempt_no, s.status, s.branch_name, mergeable
            );
        }
        return Ok(());
    }

    let id_width = sessions
        .iter()
        .fold("ID".len(), |max, s| max.max(s.id.to_string().len()));
    let issue_width = sessions
        .iter()
        .fold("ISSUE".len(), |max, s| {
            max.max(truncate_len(&s.issue_ref, 40))
        })
        .min(40);
    let status_width = sessions
        .iter()
        .fold("STATUS".len(), |max, s| max.max(s.status.len()));
    let branch_width = sessions
        .iter()
        .fold("BRANCH".len(), |max, s| max.max(s.branch_name.len()));

    println!(
        "{:<id_width$}  {:<issue_width$}  ATT  {:<status_width$}  {:<branch_width$}  MERGE",
        "ID", "ISSUE", "STATUS", "BRANCH"
    );

    for s in &sessions {
        let mergeable = match s.mergeable {
            Some(true) => "yes",
            Some(false) => "no",
            None => "-",
        };
        let issue = truncate(&s.issue_ref, 40);
        println!(
            "{:<id_width$}  {:<issue_width$}  {:<3}  {:<status_width$}  {:<branch_width$}  {}",
            s.id, issue, s.attempt_no, s.status, s.branch_name, mergeable
        );
    }

    Ok(())
}

pub fn show(args: SessionShowArgs) -> Result<(), CliError> {
    let resp = client::show_session(args.id)?;
    let s = &resp.session;

    eprintln!("Session {}", s.id);
    eprintln!("  Issue:     {}", s.issue_ref);
    eprintln!("  Attempt:   {}", s.attempt_no);
    eprintln!("  Status:    {}", s.status);
    eprintln!("  Branch:    {}", s.branch_name);
    eprintln!("  Base SHA:  {}", s.base_sha);
    if let Some(ref head) = s.head_sha {
        eprintln!("  Head SHA:  {head}");
    }
    if let Some(mergeable) = s.mergeable {
        eprintln!("  Mergeable: {}", if mergeable { "yes" } else { "no" });
    }
    if let Some(exit_code) = s.exit_code {
        eprintln!("  Exit code: {exit_code}");
    }
    if let Some(ref path) = s.task_path {
        eprintln!("  Worktree:  {path}");
    }

    if let Some(ref report) = resp.report
        && !report.is_empty()
    {
        eprintln!();
        println!("{report}");
    }

    Ok(())
}

pub fn rank(args: SessionRankArgs) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::with_source("failed to read current directory", e))?
        .canonicalize()
        .map_err(|e| CliError::with_source("failed to canonicalize current directory", e))?;
    let cwd_str = cwd.to_string_lossy().to_string();

    let sessions =
        client::list_sessions(Some(&args.issue), args.project.as_deref(), Some(&cwd_str))?;

    if sessions.is_empty() {
        eprintln!("No sessions found for this issue.");
        return Ok(());
    }

    let mut scored: Vec<(i64, &SessionInfo)> = sessions
        .iter()
        .map(|s| {
            let mut score: i64 = 0;
            if s.has_report {
                score += 100;
            }
            if s.mergeable == Some(true) {
                score += 50;
            }
            if let Some(lines) = s.lines_changed {
                score -= lines as i64 / 20;
            }
            if let Some(files) = s.files_changed
                && files > 20
            {
                score -= 10;
            }
            (score, s)
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let id_width = scored
        .iter()
        .fold("ID".len(), |max, (_, s)| max.max(s.id.to_string().len()));
    let status_width = scored
        .iter()
        .fold("STATUS".len(), |max, (_, s)| max.max(s.status.len()));

    println!(
        "{:>5}  {:<id_width$}  {:<status_width$}  MERGE  DIFF     SUMMARY",
        "SCORE", "ID", "STATUS"
    );

    for (score, s) in &scored {
        let mergeable = match s.mergeable {
            Some(true) => "yes  ",
            Some(false) => "no   ",
            None => "-    ",
        };
        let diff = match (s.lines_changed, s.files_changed) {
            (Some(l), Some(f)) => format!("+/-{l} {f}f"),
            _ => "-".to_string(),
        };
        let summary = s.summary_excerpt.as_deref().unwrap_or("-");
        println!(
            "{score:>5}  {:<id_width$}  {:<status_width$}  {mergeable}  {diff:<7}  {summary}",
            s.id, s.status
        );
    }

    Ok(())
}

pub fn pick(args: SessionPickArgs) -> Result<(), CliError> {
    client::pick_session(args.id)?;
    error::print_success(&format!("Session {} picked.", args.id));
    Ok(())
}

pub fn reject(args: SessionRejectArgs) -> Result<(), CliError> {
    client::reject_session(args.id, args.reason.as_deref())?;
    error::print_success(&format!("Session {} rejected.", args.id));
    Ok(())
}

pub fn stop(args: SessionStopArgs) -> Result<(), CliError> {
    client::stop_session(args.id)?;
    error::print_success(&format!("Session {} stopped.", args.id));
    Ok(())
}

pub fn delete(args: SessionDeleteArgs) -> Result<(), CliError> {
    client::delete_session(args.id)?;
    error::print_success(&format!("Session {} deleted.", args.id));
    Ok(())
}

pub fn logs(args: SessionLogsArgs) -> Result<(), CliError> {
    let resp = client::show_session(args.id)?;
    let task_path = resp
        .session
        .task_path
        .ok_or_else(|| CliError::new("session has no associated worktree"))?;

    let log_path = Path::new(&task_path).join(".work/session-output.log");

    if !log_path.exists() {
        return Err(CliError::with_hint(
            "no output log found for this session",
            "the session may not have started yet",
        ));
    }

    if args.follow {
        // Use `tail -f` for live following — this hands off control to the
        // child process and naturally streams until the user interrupts or
        // the file stops growing.
        let status = std::process::Command::new("tail")
            .args(["-f", &log_path.to_string_lossy()])
            .status()
            .map_err(|e| CliError::with_source("failed to run tail", e))?;

        if !status.success() {
            return Err(CliError::new("tail exited with a non-zero status"));
        }
    } else {
        let file = std::fs::File::open(&log_path)
            .map_err(|e| CliError::with_source("failed to open output log", e))?;
        let reader = io::BufReader::new(file);
        let stdout = io::stdout();
        let mut out = stdout.lock();
        io::copy(&mut io::BufReader::new(reader), &mut out)
            .map_err(|e| CliError::with_source("failed to read output log", e))?;
        let _ = out.flush();
    }

    Ok(())
}

pub fn open(args: SessionOpenArgs) -> Result<(), CliError> {
    let resp = client::show_session(args.id)?;
    let path = resp
        .session
        .task_path
        .ok_or_else(|| CliError::new("session has no associated worktree"))?;

    shell_eval(&format!("cd \"{path}\""));
    Ok(())
}

/// Read the issue description from stdin.
///
/// Returns an error if stdin is a terminal (nothing piped/redirected) or if
/// the resulting text is empty.
fn read_stdin_issue() -> Result<String, CliError> {
    let stdin = io::stdin();
    if stdin.lock().is_terminal() {
        return Err(CliError::with_hint(
            "no issue provided",
            "pass the issue as an argument or pipe it via stdin",
        ));
    }

    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::with_source("failed to read issue from stdin", e))?;

    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        return Err(CliError::with_hint(
            "empty issue from stdin",
            "pass the issue as an argument or pipe it via stdin",
        ));
    }

    Ok(trimmed)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

fn truncate_len(s: &str, max: usize) -> usize {
    s.len().min(max)
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

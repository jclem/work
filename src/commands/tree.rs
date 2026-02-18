use std::io::Write;

use crate::client;
use crate::error::CliError;
use crate::workd::{ProjectInfo, SessionInfo, TaskInfo};

/// A project with its associated tasks and sessions.
struct ProjectNode {
    project: ProjectInfo,
    tasks: Vec<TaskNode>,
}

struct TaskNode {
    task: TaskInfo,
    sessions: Vec<SessionInfo>,
}

pub fn run() -> Result<(), CliError> {
    let projects = client::list_projects()?;

    if projects.is_empty() {
        eprintln!("No projects found.");
        return Ok(());
    }

    // Build the tree: projects -> tasks -> sessions.
    let mut nodes: Vec<ProjectNode> = Vec::new();

    for project in projects {
        let tasks = client::list_tasks(Some(&project.name), None, false)?;
        let mut task_nodes: Vec<TaskNode> = Vec::new();

        for task in tasks {
            let sessions: Vec<SessionInfo> =
                client::list_sessions(None, Some(&project.name), None)?
                    .into_iter()
                    .filter(|s| {
                        s.branch_name.contains(&task.name)
                            || s.project_name.as_deref() == Some(&project.name)
                                && task_owns_session(&task, s)
                    })
                    .collect();

            task_nodes.push(TaskNode { task, sessions });
        }

        nodes.push(ProjectNode {
            project,
            tasks: task_nodes,
        });
    }

    if nodes.is_empty() {
        eprintln!("No matching projects found.");
        return Ok(());
    }

    let mut out = anstream::stdout();
    let bold = anstyle::Style::new().bold();
    let dimmed = anstyle::Style::new().dimmed();
    let cyan = anstyle::Style::new()
        .bold()
        .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan)));
    let green =
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)));
    let yellow =
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)));

    let total_projects = nodes.len();

    for (pi, node) in nodes.iter().enumerate() {
        let is_last_project = pi == total_projects - 1;
        let project_prefix = if is_last_project {
            "\u{2514}\u{2500}\u{2500} "
        } else {
            "\u{251c}\u{2500}\u{2500} "
        };
        let project_continuation = if is_last_project {
            "    "
        } else {
            "\u{2502}   "
        };

        // Project line: bold name + dimmed path
        let _ = write!(out, "{project_prefix}{bold}{}{bold:#}", node.project.name);
        let _ = writeln!(out, "  {dimmed}{}{dimmed:#}", node.project.path);

        let total_tasks = node.tasks.len();
        if total_tasks == 0 {
            let _ = writeln!(out, "{project_continuation}{dimmed}(no tasks){dimmed:#}");
        }

        for (ti, task_node) in node.tasks.iter().enumerate() {
            let is_last_task = ti == total_tasks - 1;
            let task_prefix = if is_last_task {
                format!("{project_continuation}\u{2514}\u{2500}\u{2500} ")
            } else {
                format!("{project_continuation}\u{251c}\u{2500}\u{2500} ")
            };
            let task_continuation = if is_last_task {
                format!("{project_continuation}    ")
            } else {
                format!("{project_continuation}\u{2502}   ")
            };

            let _ = write!(out, "{task_prefix}{cyan}{}{cyan:#}", task_node.task.name);
            let _ = writeln!(out, "  {dimmed}{}{dimmed:#}", task_node.task.path);

            let total_sessions = task_node.sessions.len();
            for (si, session) in task_node.sessions.iter().enumerate() {
                let is_last_session = si == total_sessions - 1;
                let session_prefix = if is_last_session {
                    format!("{task_continuation}\u{2514}\u{2500}\u{2500} ")
                } else {
                    format!("{task_continuation}\u{251c}\u{2500}\u{2500} ")
                };

                let status_style = match session.status.as_str() {
                    "running" => green,
                    "stopped" | "rejected" => yellow,
                    _ => dimmed,
                };

                let mergeable_str = match session.mergeable {
                    Some(true) => " \u{2714}",
                    Some(false) => " \u{2718}",
                    None => "",
                };

                let _ = writeln!(
                    out,
                    "{session_prefix}session {id} {dimmed}(attempt {att}){dimmed:#} {status_style}{status}{status_style:#}{mergeable_str}",
                    id = session.id,
                    att = session.attempt_no,
                    status = session.status,
                );
            }
        }
    }

    Ok(())
}

/// Heuristic: a session belongs to a task if the session's branch name matches
/// the task name (the common naming convention).
fn task_owns_session(task: &TaskInfo, session: &SessionInfo) -> bool {
    session.branch_name.contains(&task.name)
}

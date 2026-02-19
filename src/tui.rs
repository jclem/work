use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
};

use crate::client;
use crate::error::CliError;
use crate::workd::{ProjectInfo, SessionInfo, ShowSessionResponse, TaskInfo};

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Sessions,
    Projects,
    Tasks,
    Daemon,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Sessions, Tab::Projects, Tab::Tasks, Tab::Daemon];

    fn title(self) -> &'static str {
        match self {
            Tab::Projects => "Projects",
            Tab::Tasks => "Tasks",
            Tab::Sessions => "Sessions",
            Tab::Daemon => "Daemon",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Input mode for text prompts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct InputPrompt {
    label: String,
    value: String,
    on_confirm: InputAction,
}

#[derive(Debug, Clone)]
enum InputAction {
    CreateProject,
    CreateTask,
    StartSession,
    RejectSession,
}

// ---------------------------------------------------------------------------
// Confirmation dialog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Confirm {
    message: String,
    on_confirm: ConfirmAction,
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    DeleteProject(String),
    DeleteTask(String),
    Nuke,
    ClearPool,
    StopSession(i64),
    DeleteSession(i64),
    PickSession(i64),
}

// ---------------------------------------------------------------------------
// Detail overlay
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SessionDetail {
    response: ShowSessionResponse,
    scroll: u16,
}

// ---------------------------------------------------------------------------
// Session log viewer overlay
// ---------------------------------------------------------------------------

struct SessionLogs {
    session_id: i64,
    log_path: PathBuf,
    lines: Vec<String>,
    scroll: u16,
    follow: bool,
    last_read: Instant,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct App {
    tab: Tab,
    should_quit: bool,
    last_refresh: Instant,

    // Status bar message
    status_message: Option<(String, Instant, StatusKind)>,

    // Data
    projects: Vec<ProjectInfo>,
    tasks: Vec<TaskInfo>,
    sessions: Vec<SessionInfo>,
    daemon_status: DaemonStatus,

    // List states
    project_list_state: ListState,
    task_list_state: ListState,
    session_list_state: ListState,

    // Overlays
    input: Option<InputPrompt>,
    confirm: Option<Confirm>,
    session_detail: Option<SessionDetail>,
    session_logs: Option<SessionLogs>,
    help_visible: bool,
}

#[derive(Debug, Clone)]
enum StatusKind {
    Success,
    Error,
}

#[derive(Debug, Clone)]
struct DaemonStatus {
    running: bool,
    pid: Option<String>,
    message: String,
}

impl Default for DaemonStatus {
    fn default() -> Self {
        Self {
            running: false,
            pid: None,
            message: "Unknown".to_string(),
        }
    }
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            tab: Tab::Sessions,
            should_quit: false,
            last_refresh: Instant::now() - Duration::from_secs(999),
            status_message: None,
            projects: Vec::new(),
            tasks: Vec::new(),
            sessions: Vec::new(),
            daemon_status: DaemonStatus::default(),
            project_list_state: ListState::default(),
            task_list_state: ListState::default(),
            session_list_state: ListState::default(),
            input: None,
            confirm: None,
            session_detail: None,
            session_logs: None,
            help_visible: false,
        };
        app.refresh_all();
        app
    }

    fn set_status(&mut self, message: String, kind: StatusKind) {
        self.status_message = Some((message, Instant::now(), kind));
    }

    fn refresh_all(&mut self) {
        self.refresh_projects();
        self.refresh_tasks();
        self.refresh_sessions();
        self.refresh_daemon();
        self.last_refresh = Instant::now();
    }

    fn refresh_projects(&mut self) {
        match client::list_projects() {
            Ok(projects) => self.projects = projects,
            Err(_) => self.projects.clear(),
        }
        self.clamp_selection_projects();
    }

    fn refresh_tasks(&mut self) {
        match client::list_tasks(None, None, true) {
            Ok(tasks) => self.tasks = tasks,
            Err(_) => self.tasks.clear(),
        }
        self.clamp_selection_tasks();
    }

    fn refresh_sessions(&mut self) {
        match client::list_sessions(None, None, None) {
            Ok(mut sessions) => {
                sessions.sort_by(|a, b| {
                    let pa = a.project_name.as_deref().unwrap_or("");
                    let pb = b.project_name.as_deref().unwrap_or("");
                    pa.cmp(pb).then(a.attempt_no.cmp(&b.attempt_no))
                });
                self.sessions = sessions;
            }
            Err(_) => self.sessions.clear(),
        }
        self.clamp_selection_sessions();
    }

    fn refresh_daemon(&mut self) {
        // Check daemon by reading PID file and probing health
        let pid_path = crate::paths::pid_file_path();
        match std::fs::read_to_string(&pid_path) {
            Ok(content) => {
                let pid_str = content.trim().to_string();
                if let Ok(pid) = pid_str.parse::<u32>() {
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    if alive {
                        self.daemon_status = DaemonStatus {
                            running: true,
                            pid: Some(pid_str),
                            message: format!("Running (pid {})", pid),
                        };
                    } else {
                        self.daemon_status = DaemonStatus {
                            running: false,
                            pid: Some(pid_str),
                            message: format!("Stale PID file (pid {})", pid),
                        };
                    }
                } else {
                    self.daemon_status = DaemonStatus {
                        running: false,
                        pid: None,
                        message: "Invalid PID file".to_string(),
                    };
                }
            }
            Err(_) => {
                self.daemon_status = DaemonStatus {
                    running: false,
                    pid: None,
                    message: "Not running (no PID file)".to_string(),
                };
            }
        }
    }

    fn clamp_selection_projects(&mut self) {
        if self.projects.is_empty() {
            self.project_list_state.select(None);
        } else if self.project_list_state.selected().is_none() {
            self.project_list_state.select(Some(0));
        } else if let Some(i) = self.project_list_state.selected()
            && i >= self.projects.len()
        {
            self.project_list_state
                .select(Some(self.projects.len() - 1));
        }
    }

    fn clamp_selection_tasks(&mut self) {
        if self.tasks.is_empty() {
            self.task_list_state.select(None);
        } else if self.task_list_state.selected().is_none() {
            self.task_list_state.select(Some(0));
        } else if let Some(i) = self.task_list_state.selected()
            && i >= self.tasks.len()
        {
            self.task_list_state.select(Some(self.tasks.len() - 1));
        }
    }

    fn clamp_selection_sessions(&mut self) {
        if self.sessions.is_empty() {
            self.session_list_state.select(None);
        } else if self.session_list_state.selected().is_none() {
            self.session_list_state.select(Some(0));
        } else if let Some(i) = self.session_list_state.selected()
            && i >= self.sessions.len()
        {
            self.session_list_state
                .select(Some(self.sessions.len() - 1));
        }
    }

    fn selected_project(&self) -> Option<&ProjectInfo> {
        self.project_list_state
            .selected()
            .and_then(|i| self.projects.get(i))
    }

    fn selected_task(&self) -> Option<&TaskInfo> {
        self.task_list_state
            .selected()
            .and_then(|i| self.tasks.get(i))
    }

    fn selected_session(&self) -> Option<&SessionInfo> {
        self.session_list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    fn next_tab(&mut self) {
        let idx = self.tab.index();
        let next = (idx + 1) % Tab::ALL.len();
        self.tab = Tab::ALL[next];
    }

    fn prev_tab(&mut self) {
        let idx = self.tab.index();
        let prev = if idx == 0 {
            Tab::ALL.len() - 1
        } else {
            idx - 1
        };
        self.tab = Tab::ALL[prev];
    }

    fn move_up(&mut self) {
        match self.tab {
            Tab::Projects => move_list_up(&mut self.project_list_state, self.projects.len()),
            Tab::Tasks => move_list_up(&mut self.task_list_state, self.tasks.len()),
            Tab::Sessions => move_list_up(&mut self.session_list_state, self.sessions.len()),
            Tab::Daemon => {}
        }
    }

    fn move_down(&mut self) {
        match self.tab {
            Tab::Projects => move_list_down(&mut self.project_list_state, self.projects.len()),
            Tab::Tasks => move_list_down(&mut self.task_list_state, self.tasks.len()),
            Tab::Sessions => move_list_down(&mut self.session_list_state, self.sessions.len()),
            Tab::Daemon => {}
        }
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    fn handle_action(&mut self, code: KeyCode) {
        match self.tab {
            Tab::Projects => self.handle_projects_action(code),
            Tab::Tasks => self.handle_tasks_action(code),
            Tab::Sessions => self.handle_sessions_action(code),
            Tab::Daemon => self.handle_daemon_action(code),
        }
    }

    fn handle_projects_action(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('n') | KeyCode::Char('c') => {
                self.input = Some(InputPrompt {
                    label: "Project path (leave empty for cwd):".to_string(),
                    value: String::new(),
                    on_confirm: InputAction::CreateProject,
                });
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(project) = self.selected_project() {
                    let name = project.name.clone();
                    self.confirm = Some(Confirm {
                        message: format!("Delete project '{name}'?"),
                        on_confirm: ConfirmAction::DeleteProject(name),
                    });
                }
            }
            _ => {}
        }
    }

    fn handle_tasks_action(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('n') | KeyCode::Char('c') => {
                self.input = Some(InputPrompt {
                    label: "Task name (leave empty to auto-generate):".to_string(),
                    value: String::new(),
                    on_confirm: InputAction::CreateTask,
                });
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(task) = self.selected_task() {
                    let name = task.name.clone();
                    self.confirm = Some(Confirm {
                        message: format!("Delete task '{name}'?"),
                        on_confirm: ConfirmAction::DeleteTask(name),
                    });
                }
            }
            KeyCode::Char('N') => {
                self.confirm = Some(Confirm {
                    message: "NUKE all tasks, pool worktrees, and projects?".to_string(),
                    on_confirm: ConfirmAction::Nuke,
                });
            }
            KeyCode::Char('P') => {
                self.confirm = Some(Confirm {
                    message: "Clear all pool worktrees?".to_string(),
                    on_confirm: ConfirmAction::ClearPool,
                });
            }
            _ => {}
        }
    }

    fn handle_sessions_action(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('s') => {
                self.input = Some(InputPrompt {
                    label: "Issue description:".to_string(),
                    value: String::new(),
                    on_confirm: InputAction::StartSession,
                });
            }
            KeyCode::Enter => {
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    match client::show_session(id) {
                        Ok(resp) => {
                            self.session_detail = Some(SessionDetail {
                                response: resp,
                                scroll: 0,
                            });
                        }
                        Err(e) => {
                            self.set_status(format!("Error: {e}"), StatusKind::Error);
                        }
                    }
                }
            }
            KeyCode::Char('p') => {
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    self.confirm = Some(Confirm {
                        message: format!("Pick session {id} (abandon siblings)?"),
                        on_confirm: ConfirmAction::PickSession(id),
                    });
                }
            }
            KeyCode::Char('x') => {
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    self.confirm = Some(Confirm {
                        message: format!("Stop session {id}?"),
                        on_confirm: ConfirmAction::StopSession(id),
                    });
                }
            }
            KeyCode::Char('r') => {
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    self.input = Some(InputPrompt {
                        label: format!("Reason for rejecting session {id} (optional):"),
                        value: String::new(),
                        on_confirm: InputAction::RejectSession,
                    });
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    self.confirm = Some(Confirm {
                        message: format!("Delete session {id} and its worktree?"),
                        on_confirm: ConfirmAction::DeleteSession(id),
                    });
                }
            }
            _ => {}
        }
    }

    fn open_session_logs(&mut self) {
        let Some(session) = self.selected_session() else {
            return;
        };

        let id = session.id;
        let task_path = match &session.task_path {
            Some(p) => p.clone(),
            None => {
                self.set_status(format!("Session {id} has no worktree"), StatusKind::Error);
                return;
            }
        };

        let log_path = std::path::Path::new(&task_path).join(".work/session-output.log");

        if !log_path.exists() {
            self.set_status(
                format!("No log file for session {id} (may not have started yet)"),
                StatusKind::Error,
            );
            return;
        }

        let lines = read_log_lines(&log_path);
        let line_count = lines.len() as u16;

        self.session_logs = Some(SessionLogs {
            session_id: id,
            log_path,
            follow: session.status == "running" || session.status == "planned",
            scroll: line_count.saturating_sub(1),
            lines,
            last_read: Instant::now(),
        });
    }

    fn handle_daemon_action(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('s') => {
                // Start daemon (detached)
                match std::process::Command::new("work")
                    .args(["daemon", "start", "--detach"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(_) => {
                        self.set_status("Daemon start requested".to_string(), StatusKind::Success);
                        // Give it a moment to start
                        std::thread::sleep(Duration::from_millis(500));
                        self.refresh_daemon();
                    }
                    Err(e) => {
                        self.set_status(format!("Failed to start daemon: {e}"), StatusKind::Error);
                    }
                }
            }
            KeyCode::Char('x') => {
                // Stop daemon
                match client::stop_daemon() {
                    Ok(()) => {
                        self.set_status("Daemon stopped".to_string(), StatusKind::Success);
                        std::thread::sleep(Duration::from_millis(500));
                        self.refresh_daemon();
                    }
                    Err(e) => {
                        self.set_status(format!("Failed to stop daemon: {e}"), StatusKind::Error);
                    }
                }
            }
            KeyCode::Char('R') => {
                // Restart daemon
                match std::process::Command::new("work")
                    .args(["daemon", "restart"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(_) => {
                        self.set_status(
                            "Daemon restart requested".to_string(),
                            StatusKind::Success,
                        );
                        std::thread::sleep(Duration::from_millis(1000));
                        self.refresh_daemon();
                    }
                    Err(e) => {
                        self.set_status(
                            format!("Failed to restart daemon: {e}"),
                            StatusKind::Error,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Confirm / input handlers
    // -----------------------------------------------------------------------

    fn execute_confirm(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };

        match confirm.on_confirm {
            ConfirmAction::DeleteProject(name) => match client::delete_project(&name) {
                Ok(()) => {
                    self.set_status(format!("Project '{name}' deleted"), StatusKind::Success);
                    self.refresh_projects();
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
            ConfirmAction::DeleteTask(name) => {
                let cwd = std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                match client::delete_task(&name, None, &cwd, false) {
                    Ok(()) => {
                        self.set_status(format!("Task '{name}' deleted"), StatusKind::Success);
                        self.refresh_tasks();
                    }
                    Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
                }
            }
            ConfirmAction::Nuke => match client::nuke() {
                Ok(resp) => {
                    self.set_status(
                        format!(
                            "Nuked {} task(s), {} pool, {} project(s)",
                            resp.tasks, resp.pool_worktrees, resp.projects
                        ),
                        StatusKind::Success,
                    );
                    self.refresh_all();
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
            ConfirmAction::ClearPool => match client::clear_pool() {
                Ok(resp) => {
                    self.set_status(
                        format!("Cleared {} pool worktree(s)", resp.pool_worktrees),
                        StatusKind::Success,
                    );
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
            ConfirmAction::StopSession(id) => match client::stop_session(id) {
                Ok(()) => {
                    self.set_status(format!("Session {id} stopped"), StatusKind::Success);
                    self.refresh_sessions();
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
            ConfirmAction::DeleteSession(id) => match client::delete_session(id) {
                Ok(()) => {
                    self.set_status(format!("Session {id} deleted"), StatusKind::Success);
                    self.refresh_sessions();
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
            ConfirmAction::PickSession(id) => match client::pick_session(id) {
                Ok(()) => {
                    self.set_status(format!("Session {id} picked"), StatusKind::Success);
                    self.refresh_sessions();
                }
                Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
            },
        }
    }

    fn execute_input(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };

        let value = input.value.trim().to_string();

        match input.on_confirm {
            InputAction::CreateProject => {
                let path = if value.is_empty() {
                    std::env::current_dir()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                } else {
                    value
                };
                match client::create_project(&path, None) {
                    Ok(resp) => {
                        self.set_status(
                            format!("Project '{}' created", resp.name),
                            StatusKind::Success,
                        );
                        self.refresh_projects();
                    }
                    Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
                }
            }
            InputAction::CreateTask => {
                let cwd = std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let name = if value.is_empty() {
                    None
                } else {
                    Some(value.as_str())
                };
                match client::create_task(name, None, None, &cwd) {
                    Ok(resp) => {
                        self.set_status(
                            format!("Task '{}' created at {}", resp.name, resp.path),
                            StatusKind::Success,
                        );
                        self.refresh_tasks();
                    }
                    Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
                }
            }
            InputAction::StartSession => {
                if value.is_empty() {
                    self.set_status("Issue cannot be empty".to_string(), StatusKind::Error);
                    return;
                }
                let cwd = std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                match client::start_sessions(&value, 1, None, &cwd) {
                    Ok(resp) => {
                        self.set_status(
                            format!("Started {} session(s)", resp.sessions.len()),
                            StatusKind::Success,
                        );
                        self.refresh_sessions();
                    }
                    Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
                }
            }
            InputAction::RejectSession => {
                // The ID was stored from the selected session when the input was opened
                if let Some(session) = self.selected_session() {
                    let id = session.id;
                    let reason = if value.is_empty() {
                        None
                    } else {
                        Some(value.as_str())
                    };
                    match client::reject_session(id, reason) {
                        Ok(()) => {
                            self.set_status(format!("Session {id} rejected"), StatusKind::Success);
                            self.refresh_sessions();
                        }
                        Err(e) => self.set_status(format!("Error: {e}"), StatusKind::Error),
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// List helpers
// ---------------------------------------------------------------------------

fn move_list_up(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    state.select(Some(if i == 0 { len - 1 } else { i - 1 }));
}

fn move_list_down(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    state.select(Some(if i >= len - 1 { 0 } else { i + 1 }));
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn run() -> Result<(), CliError> {
    enable_raw_mode().map_err(|e| CliError::with_source("failed to enable raw mode", e))?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| CliError::with_source("failed to enter alternate screen", e))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)
        .map_err(|e| CliError::with_source("failed to create terminal", e))?;

    let mut app = App::new();
    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn run_loop(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    loop {
        terminal
            .draw(|f| ui(f, app))
            .map_err(|e| CliError::with_source("failed to draw frame", e))?;

        if app.should_quit {
            return Ok(());
        }

        // Auto-refresh every 5 seconds
        if app.last_refresh.elapsed() > Duration::from_secs(5) {
            app.refresh_all();
        }

        // Refresh session logs every second when the overlay is open
        if let Some(ref mut logs) = app.session_logs
            && logs.last_read.elapsed() > Duration::from_secs(1)
        {
            let new_lines = read_log_lines(&logs.log_path);
            let grew = new_lines.len() > logs.lines.len();
            logs.lines = new_lines;
            logs.last_read = Instant::now();
            if logs.follow && grew {
                logs.scroll = (logs.lines.len() as u16).saturating_sub(1);
            }
        }

        // Clear stale status messages after 5 seconds
        if let Some((_, ts, _)) = &app.status_message
            && ts.elapsed() > Duration::from_secs(5)
        {
            app.status_message = None;
        }

        if event::poll(Duration::from_millis(250))
            .map_err(|e| CliError::with_source("failed to poll events", e))?
            && let Event::Key(key) =
                event::read().map_err(|e| CliError::with_source("failed to read event", e))?
        {
            handle_key(app, key);
        }
    }
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

fn handle_key(app: &mut App, key: KeyEvent) {
    // Help overlay
    if app.help_visible {
        app.help_visible = false;
        return;
    }

    // Session logs overlay
    if let Some(ref mut logs) = app.session_logs {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.session_logs = None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                logs.follow = false;
                logs.scroll = logs.scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                logs.follow = false;
                logs.scroll = logs.scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                logs.follow = false;
                logs.scroll = logs.scroll.saturating_add(20);
            }
            KeyCode::PageUp => {
                logs.follow = false;
                logs.scroll = logs.scroll.saturating_sub(20);
            }
            KeyCode::Char('f') => {
                logs.follow = !logs.follow;
                if logs.follow {
                    logs.scroll = (logs.lines.len() as u16).saturating_sub(1);
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                logs.scroll = (logs.lines.len() as u16).saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => {
                logs.follow = false;
                logs.scroll = 0;
            }
            _ => {}
        }
        return;
    }

    // Session detail overlay
    if let Some(ref mut detail) = app.session_detail {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.session_detail = None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                detail.scroll = detail.scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                detail.scroll = detail.scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                detail.scroll = detail.scroll.saturating_add(20);
            }
            KeyCode::PageUp => {
                detail.scroll = detail.scroll.saturating_sub(20);
            }
            _ => {}
        }
        return;
    }

    // Confirm dialog
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                app.execute_confirm();
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                app.confirm = None;
            }
            _ => {}
        }
        return;
    }

    // Input prompt
    if let Some(ref mut input) = app.input {
        match key.code {
            KeyCode::Enter => {
                app.execute_input();
            }
            KeyCode::Esc => {
                app.input = None;
            }
            KeyCode::Backspace => {
                input.value.pop();
            }
            KeyCode::Char(c) => {
                input.value.push(c);
            }
            _ => {}
        }
        return;
    }

    // Ctrl+L: open session logs (Sessions tab only)
    if key.code == KeyCode::Char('l')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.tab == Tab::Sessions
    {
        app.open_session_logs();
        return;
    }

    // Global keys
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Tab => app.next_tab(),
        KeyCode::BackTab => app.prev_tab(),
        KeyCode::Char('1') => app.tab = Tab::Sessions,
        KeyCode::Char('2') => app.tab = Tab::Projects,
        KeyCode::Char('3') => app.tab = Tab::Tasks,
        KeyCode::Char('4') => app.tab = Tab::Daemon,
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Char('?') => app.help_visible = true,
        KeyCode::F(5) => app.refresh_all(),
        _ => app.handle_action(key.code),
    }
}

// ---------------------------------------------------------------------------
// UI rendering
// ---------------------------------------------------------------------------

fn ui(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Main layout: tabs bar + content + status bar
    let chunks = Layout::vertical([
        Constraint::Length(3), // Tab bar
        Constraint::Min(5),    // Content
        Constraint::Length(1), // Status bar
    ])
    .split(area);

    render_tabs(f, app, chunks[0]);
    render_content(f, app, chunks[1]);
    render_status_bar(f, app, chunks[2]);

    // Overlays
    if let Some(ref confirm) = app.confirm {
        render_confirm(f, confirm, area);
    }

    if let Some(ref input) = app.input {
        render_input(f, input, area);
    }

    if let Some(ref detail) = app.session_detail {
        render_session_detail(f, detail, area);
    }

    if let Some(ref logs) = app.session_logs {
        render_session_logs(f, logs, area);
    }

    if app.help_visible {
        render_help(f, app, area);
    }
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" work "),
        )
        .select(app.tab.index())
        .highlight_style(Style::default().fg(Color::Cyan).bold())
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(tabs, area);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    match app.tab {
        Tab::Projects => render_projects(f, app, area),
        Tab::Tasks => render_tasks(f, app, area),
        Tab::Sessions => render_sessions(f, app, area),
        Tab::Daemon => render_daemon(f, app, area),
    }
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let (msg, style) = if let Some((ref message, _, ref kind)) = app.status_message {
        let color = match kind {
            StatusKind::Success => Color::Green,
            StatusKind::Error => Color::Red,
        };
        (message.as_str(), Style::default().fg(color))
    } else {
        (
            "? help │ q quit │ Tab/S-Tab switch │ ↑↓/jk navigate │ F5 refresh",
            Style::default().fg(Color::DarkGray),
        )
    };

    let bar = Paragraph::new(msg).style(style);
    f.render_widget(bar, area);
}

// ---------------------------------------------------------------------------
// Projects tab
// ---------------------------------------------------------------------------

fn render_projects(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let line = Line::from(vec![
                Span::styled(&p.name, Style::default().fg(Color::Cyan).bold()),
                Span::raw("  "),
                Span::styled(&p.path, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Projects ")
                .title_bottom(Line::from(" n new │ d delete ").right_aligned()),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, area, &mut app.project_list_state);
}

// ---------------------------------------------------------------------------
// Tasks tab
// ---------------------------------------------------------------------------

fn render_tasks(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|t| {
            let project = t.project_name.as_deref().unwrap_or("?").to_string();
            let line = Line::from(vec![
                Span::styled(project, Style::default().fg(Color::Yellow)),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(&t.name, Style::default().fg(Color::Cyan).bold()),
                Span::raw("  "),
                Span::styled(&t.path, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Tasks ")
                .title_bottom(
                    Line::from(" n new │ d delete │ N nuke │ P clear pool ").right_aligned(),
                ),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, area, &mut app.task_list_state);
}

// ---------------------------------------------------------------------------
// Sessions tab
// ---------------------------------------------------------------------------

fn render_sessions(f: &mut Frame, app: &mut App, area: Rect) {
    // Split area: list on left, details/preview on right
    let chunks =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).split(area);

    // Session list
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .map(|s| {
            let status_color = match s.status.as_str() {
                "running" => Color::Yellow,
                "reported" => Color::Green,
                "picked" => Color::Cyan,
                "rejected" => Color::Red,
                "stopped" => Color::DarkGray,
                "planned" => Color::Blue,
                "failed" => Color::Red,
                _ => Color::White,
            };

            let mergeable = match s.mergeable {
                Some(true) => Span::styled(" ✓", Style::default().fg(Color::Green)),
                Some(false) => Span::styled(" ✗", Style::default().fg(Color::Red)),
                None => Span::styled(" -", Style::default().fg(Color::DarkGray)),
            };

            let issue = truncate_str(&s.issue_ref, 30);

            let mut spans = vec![
                Span::styled(format!("{:>3}", s.id), Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(
                    format!("{:<8}", s.status),
                    Style::default().fg(status_color),
                ),
                mergeable,
                Span::raw("  "),
                Span::styled(issue, Style::default().fg(Color::White)),
            ];

            if let Some(ref name) = s.project_name {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    name.as_str(),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Sessions ")
                .title_bottom(
                    Line::from(
                        " s start │ ↵ details │ ^l logs │ p pick │ x stop │ r reject │ d delete ",
                    )
                    .right_aligned(),
                ),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, chunks[0], &mut app.session_list_state);

    // Session preview pane
    let preview = if let Some(session) = app.selected_session() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(session.id.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Project:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    session.project_name.as_deref().unwrap_or("—"),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Issue:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(&session.issue_ref, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Attempt:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    session.attempt_no.to_string(),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&session.status, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Branch:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&session.branch_name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Base SHA: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate_str(&session.base_sha, 12),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];

        if let Some(ref head) = session.head_sha {
            lines.push(Line::from(vec![
                Span::styled("Head SHA: ", Style::default().fg(Color::DarkGray)),
                Span::styled(truncate_str(head, 12), Style::default().fg(Color::White)),
            ]));
        }

        if let Some(mergeable) = session.mergeable {
            lines.push(Line::from(vec![
                Span::styled("Merge:    ", Style::default().fg(Color::DarkGray)),
                if mergeable {
                    Span::styled("yes", Style::default().fg(Color::Green))
                } else {
                    Span::styled("no", Style::default().fg(Color::Red))
                },
            ]));
        }

        if let Some(exit_code) = session.exit_code {
            lines.push(Line::from(vec![
                Span::styled("Exit:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    exit_code.to_string(),
                    if exit_code == 0 {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
            ]));
        }

        if let Some(ref path) = session.task_path {
            lines.push(Line::from(vec![
                Span::styled("Path:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(path.as_str(), Style::default().fg(Color::DarkGray)),
            ]));
        }

        if let (Some(lines_changed), Some(files_changed)) =
            (session.lines_changed, session.files_changed)
        {
            lines.push(Line::from(vec![
                Span::styled("Diff:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("+/- {} lines, {} files", lines_changed, files_changed),
                    Style::default().fg(Color::White),
                ),
            ]));
        }

        if let Some(ref excerpt) = session.summary_excerpt {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Summary:",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::UNDERLINED),
            )]));
            lines.push(Line::from(vec![Span::styled(
                excerpt.as_str(),
                Style::default().fg(Color::White),
            )]));
        }

        Text::from(lines)
    } else {
        Text::from(vec![Line::from(Span::styled(
            "No session selected",
            Style::default().fg(Color::DarkGray),
        ))])
    };

    let preview_widget = Paragraph::new(preview)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Details "),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(preview_widget, chunks[1]);
}

// ---------------------------------------------------------------------------
// Daemon tab
// ---------------------------------------------------------------------------

fn render_daemon(f: &mut Frame, app: &App, area: Rect) {
    let status_color = if app.daemon_status.running {
        Color::Green
    } else {
        Color::Red
    };

    let indicator = if app.daemon_status.running {
        "●"
    } else {
        "○"
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{indicator} {}", app.daemon_status.message),
                Style::default().fg(status_color),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(ref pid) = app.daemon_status.pid {
        lines.push(Line::from(vec![
            Span::styled("  PID:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(pid.as_str(), Style::default().fg(Color::White)),
        ]));
    }

    let socket_path = crate::paths::socket_path(None);
    lines.push(Line::from(vec![
        Span::styled("  Socket:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            socket_path.to_string_lossy().to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let db_path = crate::paths::database_path();
    lines.push(Line::from(vec![
        Span::styled("  DB:      ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            db_path.to_string_lossy().to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Keyboard shortcuts:",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::UNDERLINED),
    )]));
    lines.push(Line::from(vec![
        Span::styled("    s", Style::default().fg(Color::Cyan)),
        Span::styled(
            " start daemon (detached)",
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    x", Style::default().fg(Color::Cyan)),
        Span::styled(" stop daemon", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    R", Style::default().fg(Color::Cyan)),
        Span::styled(" restart daemon", Style::default().fg(Color::White)),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Daemon ")
            .title_bottom(Line::from(" s start │ x stop │ R restart ").right_aligned()),
    );

    f.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Overlay dialogs
// ---------------------------------------------------------------------------

fn render_confirm(f: &mut Frame, confirm: &Confirm, area: Rect) {
    let popup = centered_rect(50, 7, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            &confirm.message,
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" y ", Style::default().fg(Color::Green).bold()),
            Span::raw("confirm  "),
            Span::styled(" n ", Style::default().fg(Color::Red).bold()),
            Span::raw("cancel"),
        ]),
    ];

    let paragraph = Paragraph::new(text).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm "),
    );

    f.render_widget(paragraph, popup);
}

fn render_input(f: &mut Frame, input: &InputPrompt, area: Rect) {
    let popup = centered_rect(60, 7, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(&input.label, Style::default().fg(Color::Cyan))),
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(Color::Cyan)),
            Span::raw(&input.value),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(Span::styled(
            " Enter confirm │ Esc cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(text).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Input "),
    );

    f.render_widget(paragraph, popup);
}

fn render_session_detail(f: &mut Frame, detail: &SessionDetail, area: Rect) {
    let popup = centered_rect(85, 80, area);
    f.render_widget(Clear, popup);

    let s = &detail.response.session;

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Session ", Style::default().fg(Color::DarkGray)),
            Span::styled(s.id.to_string(), Style::default().fg(Color::Cyan).bold()),
        ]),
        Line::from(vec![
            Span::styled("  Project:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                s.project_name.as_deref().unwrap_or("—"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Issue:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(&s.issue_ref, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Attempt:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(s.attempt_no.to_string(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Status:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(&s.status, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Branch:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(&s.branch_name, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Base SHA:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(&s.base_sha, Style::default().fg(Color::White)),
        ]),
    ];

    if let Some(ref head) = s.head_sha {
        lines.push(Line::from(vec![
            Span::styled("  Head SHA:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(head.as_str(), Style::default().fg(Color::White)),
        ]));
    }

    if let Some(mergeable) = s.mergeable {
        lines.push(Line::from(vec![
            Span::styled("  Mergeable: ", Style::default().fg(Color::DarkGray)),
            if mergeable {
                Span::styled("yes", Style::default().fg(Color::Green))
            } else {
                Span::styled("no", Style::default().fg(Color::Red))
            },
        ]));
    }

    if let Some(exit_code) = s.exit_code {
        lines.push(Line::from(vec![
            Span::styled("  Exit code: ", Style::default().fg(Color::DarkGray)),
            Span::styled(exit_code.to_string(), Style::default().fg(Color::White)),
        ]));
    }

    if let Some(ref path) = s.task_path {
        lines.push(Line::from(vec![
            Span::styled("  Worktree:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(path.as_str(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    if let Some(ref report) = detail.response.report
        && !report.is_empty()
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "── Report ──────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        for line in report.lines() {
            lines.push(Line::from(Span::raw(line)));
        }
    }

    let content_height = lines.len() as u16;
    let inner_height = popup.height.saturating_sub(2); // borders

    let paragraph = Paragraph::new(lines)
        .scroll((detail.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Session Detail ")
                .title_bottom(Line::from(" ↑↓ scroll │ q/Esc close ").right_aligned()),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, popup);

    // Scrollbar
    if content_height > inner_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let mut scrollbar_state =
            ScrollbarState::new(content_height.saturating_sub(inner_height) as usize)
                .position(detail.scroll as usize);
        let scrollbar_area = Rect {
            x: popup.x + popup.width - 1,
            y: popup.y + 1,
            width: 1,
            height: popup.height.saturating_sub(2),
        };
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

fn render_session_logs(f: &mut Frame, logs: &SessionLogs, area: Rect) {
    let popup = centered_rect(90, 90, area);
    f.render_widget(Clear, popup);

    let inner_height = popup.height.saturating_sub(2); // account for borders

    let lines: Vec<Line> = logs
        .lines
        .iter()
        .map(|l| Line::from(Span::raw(l.as_str())))
        .collect();

    let content_height = lines.len() as u16;

    // Clamp scroll so we don't scroll past the end
    let max_scroll = content_height.saturating_sub(inner_height);
    let scroll = logs.scroll.min(max_scroll);

    let follow_indicator = if logs.follow {
        Span::styled(
            " FOLLOW ",
            Style::default().fg(Color::Black).bg(Color::Green),
        )
    } else {
        Span::styled(
            " PAUSED ",
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        )
    };

    let title_line = Line::from(vec![
        Span::raw(" Session "),
        Span::styled(
            logs.session_id.to_string(),
            Style::default().fg(Color::Cyan).bold(),
        ),
        Span::raw(" Logs "),
        follow_indicator,
        Span::raw(" "),
    ]);

    let paragraph = Paragraph::new(lines).scroll((scroll, 0)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title_line)
            .title_bottom(
                Line::from(" ↑↓ scroll │ f follow │ g top │ G end │ q/Esc close ").right_aligned(),
            ),
    );

    f.render_widget(paragraph, popup);

    // Scrollbar
    if content_height > inner_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let mut scrollbar_state =
            ScrollbarState::new(content_height.saturating_sub(inner_height) as usize)
                .position(scroll as usize);
        let scrollbar_area = Rect {
            x: popup.x + popup.width - 1,
            y: popup.y + 1,
            width: 1,
            height: popup.height.saturating_sub(2),
        };
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(70, 75, area);
    f.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            "Global",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        help_line("q / Esc", "Quit"),
        help_line("Tab", "Next tab"),
        help_line("Shift+Tab", "Previous tab"),
        help_line("1-4", "Jump to tab"),
        help_line("↑ / k", "Move up"),
        help_line("↓ / j", "Move down"),
        help_line("F5", "Refresh all data"),
        help_line("?", "Toggle this help"),
        Line::from(""),
    ];

    match app.tab {
        Tab::Projects => {
            lines.push(Line::from(Span::styled(
                "Projects",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(help_line("n / c", "Create project (register cwd or path)"));
            lines.push(help_line("d / Del", "Delete selected project"));
        }
        Tab::Tasks => {
            lines.push(Line::from(Span::styled(
                "Tasks",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(help_line("n / c", "Create new task"));
            lines.push(help_line("d / Del", "Delete selected task"));
            lines.push(help_line("N", "Nuke all tasks, pool, and projects"));
            lines.push(help_line("P", "Clear pool worktrees"));
        }
        Tab::Sessions => {
            lines.push(Line::from(Span::styled(
                "Sessions",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(help_line("s", "Start new session (enter issue)"));
            lines.push(help_line("Enter", "View session details & report"));
            lines.push(help_line("Ctrl+l", "View session output logs"));
            lines.push(help_line("p", "Pick session (accept, abandon siblings)"));
            lines.push(help_line("x", "Stop running session"));
            lines.push(help_line("r", "Reject session (with optional reason)"));
            lines.push(help_line("d / Del", "Delete session and worktree"));
        }
        Tab::Daemon => {
            lines.push(Line::from(Span::styled(
                "Daemon",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(help_line("s", "Start daemon (detached)"));
            lines.push(help_line("x", "Stop daemon"));
            lines.push(help_line("R", "Restart daemon"));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Help ")
            .title_bottom(Line::from(" Press any key to close ").right_aligned()),
    );

    f.render_widget(paragraph, popup);
}

fn help_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {key:<18}"), Style::default().fg(Color::Yellow)),
        Span::styled(desc, Style::default().fg(Color::White)),
    ])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    // Clamp percent_y by absolute pixels — min height 5
    let height = (area.height * percent_y / 100).max(5).min(area.height);
    let width = (area.width * percent_x / 100).max(20).min(area.width);

    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let x = area.x + (area.width.saturating_sub(width)) / 2;

    Rect::new(x, y, width, height)
}

fn read_log_lines(path: &std::path::Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => content.lines().map(String::from).collect(),
        Err(_) => Vec::new(),
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

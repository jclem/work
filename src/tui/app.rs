use std::collections::HashSet;

use crate::client::DaemonClient;
use crate::db::{Environment, Project, Task};
use crate::paths;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tasks,
    Projects,
    Environments,
    Daemon,
    Logs,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Tasks,
        Tab::Projects,
        Tab::Environments,
        Tab::Daemon,
        Tab::Logs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Tasks => "Tasks",
            Tab::Projects => "Projects",
            Tab::Environments => "Environments",
            Tab::Daemon => "Daemon",
            Tab::Logs => "Logs",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Tasks => 0,
            Tab::Projects => 1,
            Tab::Environments => 2,
            Tab::Daemon => 3,
            Tab::Logs => 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskViewMode {
    Flat,
    Tree,
}

pub enum TreeRow {
    Project(usize),
    Task(usize),
    TaskEnvironment(usize),
}

pub enum DetailView {
    TaskLog { task_id: String },
    EnvironmentLog { env_id: String },
}

pub enum Confirm {
    DeleteTask {
        task_id: String,
        skip_provider: bool,
    },
    DeleteProject {
        project_name: String,
    },
    DeleteEnvironment {
        env_id: String,
        skip_provider: bool,
    },
}

pub struct CreateTaskPrompt {
    pub selected_project: usize,
}

pub struct App {
    pub should_quit: bool,
    pub tab: Tab,
    pub detail: Option<DetailView>,
    pub confirm: Option<Confirm>,
    pub create_task_prompt: Option<CreateTaskPrompt>,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub environments: Vec<Environment>,
    pub selected: usize,
    pub log_content: String,
    pub log_scroll: usize,
    pub error: Option<String>,
    pub daemon_connected: bool,
    pub tui_log_content: String,
    pub tui_log_scroll: usize,
    pub task_view_mode: TaskViewMode,
    pub tree_rows: Vec<TreeRow>,
    pub collapsed_projects: HashSet<usize>,
    pub collapsed_tasks: HashSet<usize>,
}

impl App {
    pub fn new() -> Self {
        let task_view_mode = load_task_view_mode();
        Self {
            should_quit: false,
            tab: Tab::Tasks,
            detail: None,
            confirm: None,
            create_task_prompt: None,
            tasks: Vec::new(),
            projects: Vec::new(),
            environments: Vec::new(),
            selected: 0,
            log_content: String::new(),
            log_scroll: 0,
            error: None,
            daemon_connected: false,
            tui_log_content: String::new(),
            tui_log_scroll: 0,
            task_view_mode,
            tree_rows: Vec::new(),
            collapsed_projects: HashSet::new(),
            collapsed_tasks: HashSet::new(),
        }
    }

    pub async fn poll(&mut self, client: &DaemonClient) {
        match client.list_tasks().await {
            Ok(tasks) => {
                self.tasks = tasks;
                self.error = None;
                self.daemon_connected = true;
            }
            Err(e) => {
                self.error = Some(format!("daemon: {e}"));
                self.daemon_connected = false;
            }
        }

        if let Ok(projects) = client.list_projects().await {
            self.projects = projects;
        }

        if let Ok(environments) = client.list_environments().await {
            self.environments = environments;
        }

        self.rebuild_tree();
        self.clamp_selected();
        self.refresh_tui_logs();
        self.refresh_detail_logs();
    }

    pub fn rebuild_tree(&mut self) {
        self.tree_rows.clear();

        // Group tasks by project, preserving project order.
        for (pi, project) in self.projects.iter().enumerate() {
            let project_tasks: Vec<usize> = self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.project_id == project.id)
                .map(|(i, _)| i)
                .collect();

            if project_tasks.is_empty() {
                continue;
            }

            self.tree_rows.push(TreeRow::Project(pi));

            if !self.collapsed_projects.contains(&pi) {
                for ti in project_tasks {
                    self.tree_rows.push(TreeRow::Task(ti));
                    if !self.collapsed_tasks.contains(&ti) {
                        self.tree_rows.push(TreeRow::TaskEnvironment(ti));
                    }
                }
            }
        }

        // Tasks with no matching project.
        let orphan_tasks: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| !self.projects.iter().any(|p| p.id == t.project_id))
            .map(|(i, _)| i)
            .collect();

        if !orphan_tasks.is_empty() {
            for ti in orphan_tasks {
                self.tree_rows.push(TreeRow::Task(ti));
                if !self.collapsed_tasks.contains(&ti) {
                    self.tree_rows.push(TreeRow::TaskEnvironment(ti));
                }
            }
        }
    }

    pub fn collapse_section(&mut self) {
        if self.task_view_mode != TaskViewMode::Tree {
            return;
        }
        match self.tree_rows.get(self.selected) {
            Some(TreeRow::Project(pi)) => {
                let pi = *pi;
                self.collapsed_projects.insert(pi);
                self.rebuild_tree();
                self.move_selected_to_project(pi);
            }
            Some(TreeRow::Task(ti)) => {
                let ti = *ti;
                // Task has children — collapse it.
                self.collapsed_tasks.insert(ti);
                self.rebuild_tree();
                self.clamp_selected();
            }
            Some(TreeRow::TaskEnvironment(ti)) => {
                let ti = *ti;
                self.collapsed_tasks.insert(ti);
                self.rebuild_tree();
                // Move selection to the parent task row.
                for (i, row) in self.tree_rows.iter().enumerate() {
                    if matches!(row, TreeRow::Task(t) if *t == ti) {
                        self.selected = i;
                        break;
                    }
                }
                self.clamp_selected();
            }
            None => {}
        }
    }

    pub fn expand_section(&mut self) {
        if self.task_view_mode != TaskViewMode::Tree {
            return;
        }
        match self.tree_rows.get(self.selected) {
            Some(TreeRow::Project(pi)) => {
                let pi = *pi;
                if self.collapsed_projects.remove(&pi) {
                    self.rebuild_tree();
                    self.clamp_selected();
                }
            }
            Some(TreeRow::Task(ti)) => {
                let ti = *ti;
                if self.collapsed_tasks.remove(&ti) {
                    self.rebuild_tree();
                    self.clamp_selected();
                }
            }
            _ => {}
        }
    }

    pub fn collapse_all(&mut self) {
        if self.task_view_mode != TaskViewMode::Tree {
            return;
        }
        for pi in 0..self.projects.len() {
            self.collapsed_projects.insert(pi);
        }
        for ti in 0..self.tasks.len() {
            self.collapsed_tasks.insert(ti);
        }
        self.rebuild_tree();
        self.clamp_selected();
    }

    pub fn expand_all(&mut self) {
        if self.task_view_mode != TaskViewMode::Tree {
            return;
        }
        self.collapsed_projects.clear();
        self.collapsed_tasks.clear();
        self.rebuild_tree();
        self.clamp_selected();
    }

    fn move_selected_to_project(&mut self, pi: usize) {
        for (i, row) in self.tree_rows.iter().enumerate() {
            if matches!(row, TreeRow::Project(p) if *p == pi) {
                self.selected = i;
                break;
            }
        }
        self.clamp_selected();
    }

    pub fn is_project_collapsed(&self, project_index: usize) -> bool {
        self.collapsed_projects.contains(&project_index)
    }

    pub fn is_task_collapsed(&self, task_index: usize) -> bool {
        self.collapsed_tasks.contains(&task_index)
    }

    fn clamp_selected(&mut self) {
        let len = self.list_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Tasks => match self.task_view_mode {
                TaskViewMode::Flat => self.tasks.len(),
                TaskViewMode::Tree => self.tree_rows.len(),
            },
            Tab::Projects => self.projects.len(),
            Tab::Environments => self.environments.len(),
            Tab::Daemon => 0,
            Tab::Logs => 0,
        }
    }

    pub fn toggle_task_view_mode(&mut self) {
        self.task_view_mode = match self.task_view_mode {
            TaskViewMode::Flat => TaskViewMode::Tree,
            TaskViewMode::Tree => TaskViewMode::Flat,
        };
        self.selected = 0;
        save_task_view_mode(self.task_view_mode);
    }

    pub fn next_tab(&mut self) {
        let idx = (self.tab.index() + 1) % Tab::ALL.len();
        self.tab = Tab::ALL[idx];
        self.selected = 0;
    }

    pub fn prev_tab(&mut self) {
        let idx = (self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len();
        self.tab = Tab::ALL[idx];
        self.selected = 0;
    }

    pub fn select_tab(&mut self, idx: usize) {
        if let Some(&tab) = Tab::ALL.get(idx) {
            self.tab = tab;
            self.selected = 0;
        }
    }

    pub fn select_next(&mut self) {
        let len = self.list_len();
        if len > 0 && self.selected < len - 1 {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Returns the task index for the currently selected row, if it points to a task.
    fn selected_task_index(&self) -> Option<usize> {
        match self.task_view_mode {
            TaskViewMode::Flat => {
                if self.selected < self.tasks.len() {
                    Some(self.selected)
                } else {
                    None
                }
            }
            TaskViewMode::Tree => match self.tree_rows.get(self.selected) {
                Some(TreeRow::Task(ti)) => Some(*ti),
                _ => None,
            },
        }
    }

    fn default_project_id_for_new_task(&self) -> Option<&str> {
        if self.tab != Tab::Tasks {
            return None;
        }

        match self.task_view_mode {
            TaskViewMode::Flat => self
                .tasks
                .get(self.selected)
                .map(|task| task.project_id.as_str()),
            TaskViewMode::Tree => match self.tree_rows.get(self.selected) {
                Some(TreeRow::Project(pi)) => {
                    self.projects.get(*pi).map(|project| project.id.as_str())
                }
                Some(TreeRow::Task(ti) | TreeRow::TaskEnvironment(ti)) => {
                    self.tasks.get(*ti).map(|task| task.project_id.as_str())
                }
                None => None,
            },
        }
    }

    pub fn begin_create_task_prompt(&mut self) {
        if self.tab != Tab::Tasks {
            return;
        }
        if self.projects.is_empty() {
            self.error = Some("no projects available".to_string());
            return;
        }

        let selected_project = self
            .default_project_id_for_new_task()
            .and_then(|project_id| self.projects.iter().position(|p| p.id == project_id))
            .unwrap_or(0);

        self.create_task_prompt = Some(CreateTaskPrompt { selected_project });
        self.error = None;
    }

    pub fn cancel_create_task_prompt(&mut self) {
        self.create_task_prompt = None;
    }

    pub fn create_task_prompt_select_next(&mut self) {
        let Some(prompt) = self.create_task_prompt.as_mut() else {
            return;
        };
        if prompt.selected_project + 1 < self.projects.len() {
            prompt.selected_project += 1;
        }
    }

    pub fn create_task_prompt_select_prev(&mut self) {
        let Some(prompt) = self.create_task_prompt.as_mut() else {
            return;
        };
        if prompt.selected_project > 0 {
            prompt.selected_project -= 1;
        }
    }

    pub fn create_task_prompt_selected_project(&self) -> Option<&Project> {
        self.create_task_prompt
            .as_ref()
            .and_then(|prompt| self.projects.get(prompt.selected_project))
    }

    pub fn enter_detail(&mut self) {
        match self.tab {
            Tab::Tasks => {
                if let Some(ti) = self.selected_task_index() {
                    let task_id = self.tasks[ti].id.clone();
                    self.log_content = read_task_log(&task_id);
                    self.log_scroll = self.log_content.lines().count().saturating_sub(1);
                    self.detail = Some(DetailView::TaskLog { task_id });
                }
            }
            Tab::Environments => {
                if let Some(env) = self.environments.get(self.selected) {
                    let env_id = env.id.clone();
                    self.log_content = read_environment_log(&env_id);
                    self.log_scroll = self.log_content.lines().count().saturating_sub(1);
                    self.detail = Some(DetailView::EnvironmentLog { env_id });
                }
            }
            _ => {}
        }
    }

    pub fn exit_detail(&mut self) {
        self.detail = None;
        self.log_content.clear();
        self.log_scroll = 0;
    }

    pub fn scroll_log_down(&mut self, amount: usize) {
        let line_count = self.log_content.lines().count();
        self.log_scroll = self
            .log_scroll
            .saturating_add(amount)
            .min(line_count.saturating_sub(1));
    }

    pub fn scroll_log_up(&mut self, amount: usize) {
        self.log_scroll = self.log_scroll.saturating_sub(amount);
    }

    pub fn scroll_log_top(&mut self) {
        self.log_scroll = 0;
    }

    pub fn scroll_log_bottom(&mut self) {
        let line_count = self.log_content.lines().count();
        self.log_scroll = line_count.saturating_sub(1);
    }

    pub fn refresh_detail_logs(&mut self) {
        let old_line_count = self.log_content.lines().count();
        let was_at_bottom = self.log_scroll >= old_line_count.saturating_sub(1);

        let new_content = match self.detail.as_ref() {
            Some(DetailView::TaskLog { task_id }) => read_task_log(task_id),
            Some(DetailView::EnvironmentLog { env_id }) => read_environment_log(env_id),
            None => return,
        };
        self.log_content = new_content;
        let new_line_count = self.log_content.lines().count();
        if was_at_bottom {
            self.log_scroll = new_line_count.saturating_sub(1);
        } else {
            self.log_scroll = self.log_scroll.min(new_line_count.saturating_sub(1));
        }
    }

    pub fn refresh_tui_logs(&mut self) {
        let old_line_count = self.tui_log_content.lines().count();
        let was_at_bottom = self.tui_log_scroll >= old_line_count.saturating_sub(1);

        self.tui_log_content = read_tui_log();
        let new_line_count = self.tui_log_content.lines().count();
        if was_at_bottom {
            self.tui_log_scroll = new_line_count.saturating_sub(1);
        } else {
            self.tui_log_scroll = self.tui_log_scroll.min(new_line_count.saturating_sub(1));
        }
    }

    pub fn scroll_tui_log_down(&mut self, amount: usize) {
        let line_count = self.tui_log_content.lines().count();
        self.tui_log_scroll = self
            .tui_log_scroll
            .saturating_add(amount)
            .min(line_count.saturating_sub(1));
    }

    pub fn scroll_tui_log_up(&mut self, amount: usize) {
        self.tui_log_scroll = self.tui_log_scroll.saturating_sub(amount);
    }

    pub fn scroll_tui_log_top(&mut self) {
        self.tui_log_scroll = 0;
    }

    pub fn scroll_tui_log_bottom(&mut self) {
        let line_count = self.tui_log_content.lines().count();
        self.tui_log_scroll = line_count.saturating_sub(1);
    }

    pub fn prompt_delete(&mut self) {
        self.prompt_delete_with_options(false);
    }

    pub fn prompt_force_delete(&mut self) {
        self.prompt_delete_with_options(true);
    }

    fn prompt_delete_with_options(&mut self, skip_provider: bool) {
        match self.tab {
            Tab::Tasks => {
                if let Some(ti) = self.selected_task_index() {
                    self.confirm = Some(Confirm::DeleteTask {
                        task_id: self.tasks[ti].id.clone(),
                        skip_provider,
                    });
                }
            }
            Tab::Projects => {
                if let Some(project) = self.projects.get(self.selected) {
                    self.confirm = Some(Confirm::DeleteProject {
                        project_name: project.name.clone(),
                    });
                }
            }
            Tab::Environments => {
                if let Some(env) = self.environments.get(self.selected) {
                    self.confirm = Some(Confirm::DeleteEnvironment {
                        env_id: env.id.clone(),
                        skip_provider,
                    });
                }
            }
            _ => {}
        }
    }

    pub async fn confirm_delete(&mut self, client: &DaemonClient) {
        match &self.confirm {
            Some(Confirm::DeleteTask {
                task_id,
                skip_provider,
            }) => {
                let task_id = task_id.clone();
                match client.remove_task(&task_id, *skip_provider).await {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(format!("delete failed: {e}")),
                }
            }
            Some(Confirm::DeleteProject { project_name }) => {
                let name = project_name.clone();
                match client.delete_project(&name).await {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(format!("delete failed: {e}")),
                }
            }
            Some(Confirm::DeleteEnvironment {
                env_id,
                skip_provider,
            }) => {
                let env_id = env_id.clone();
                match client.remove_environment(&env_id, *skip_provider).await {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(format!("delete failed: {e}")),
                }
            }
            None => return,
        }
        self.confirm = None;
        self.poll(client).await;
    }

    pub fn set_disconnected(&mut self) {
        self.daemon_connected = false;
        self.error = Some("daemon disconnected".to_string());
    }

    pub fn cancel_confirm(&mut self) {
        self.confirm = None;
    }

    pub fn project_name(&self, project_id: &str) -> &str {
        self.projects
            .iter()
            .find(|p| p.id == project_id)
            .map(|p| p.name.as_str())
            .unwrap_or("-")
    }

    pub fn find_environment(&self, env_id: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.id == env_id)
    }
}

fn state_file_path() -> Option<std::path::PathBuf> {
    paths::state_dir().ok().map(|d| d.join("tui.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TuiState {
    task_view_mode: TaskViewMode,
}

fn load_task_view_mode() -> TaskViewMode {
    state_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<TuiState>(&s).ok())
        .map(|s| s.task_view_mode)
        .unwrap_or(TaskViewMode::Flat)
}

fn save_task_view_mode(mode: TaskViewMode) {
    if let Some(path) = state_file_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let state = TuiState {
            task_view_mode: mode,
        };
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn read_task_log(task_id: &str) -> String {
    paths::task_log_path(task_id)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

fn read_environment_log(env_id: &str) -> String {
    paths::environment_log_path(env_id)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

fn read_tui_log() -> String {
    paths::tui_log_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use rand::Rng;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::net::UnixListener;
use tokio::sync::{Notify, Semaphore, watch};

use sysinfo::System;

use crate::adapters::TaskAdapter;
use crate::adapters::worktree::GitWorktreeAdapter;
use crate::config;
use crate::db;
use crate::error::CliError;
use crate::logger::Logger;
use crate::paths;

#[derive(Clone)]
struct AppState {
    deletion_notify: Arc<Notify>,
    pool_notify: Arc<Notify>,
    logger: Logger,
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateProjectResponse {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteProjectRequest {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct DetectProjectRequest {
    pub project: Option<String>,
    pub cwd: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub name: Option<String>,
    pub branch: Option<String>,
    pub project: Option<String>,
    pub cwd: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateTaskResponse {
    pub name: String,
    pub path: String,
    #[serde(rename = "hookScript", skip_serializing_if = "Option::is_none")]
    pub hook_script: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ListTasksRequest {
    pub project: Option<String>,
    pub cwd: Option<String>,
    pub all: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct TaskInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "projectName", skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteTaskRequest {
    pub name: String,
    pub project: Option<String>,
    pub cwd: String,
    pub force: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct NukeResponse {
    pub tasks: usize,
    #[serde(rename = "poolWorktrees")]
    pub pool_worktrees: usize,
    pub projects: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ClearPoolResponse {
    #[serde(rename = "poolWorktrees")]
    pub pool_worktrees: usize,
}

// ---------------------------------------------------------------------------
// API error type
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

struct ApiError {
    status: StatusCode,
    message: String,
    hint: Option<String>,
}

impl ApiError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            hint: None,
        }
    }
}

impl From<CliError> for ApiError {
    fn from(err: CliError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
            hint: err.hint().map(|s| s.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = ErrorBody {
            error: self.message,
            hint: self.hint,
        };
        (self.status, Json(body)).into_response()
    }
}

async fn run_blocking<F, T>(f: F) -> Result<T, ApiError>
where
    F: FnOnce() -> Result<T, CliError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("task panicked: {e}")))?
        .map_err(ApiError::from)
}

pub struct Workd {
    sql: Connection,
    logger: Logger,
    socket_path: PathBuf,
}

impl Workd {
    pub async fn start(
        logger: Logger,
        socket_path_override: Option<PathBuf>,
    ) -> Result<(), CliError> {
        let workd = Self::new(logger, socket_path_override)?;
        workd.start_inner().await
    }

    fn new(logger: Logger, socket_path_override: Option<PathBuf>) -> Result<Self, CliError> {
        let logger = logger.child("workd");
        let database_path = paths::database_path();
        let socket_path = paths::socket_path(socket_path_override);

        ensure_parent_dir(&database_path)?;
        ensure_parent_dir(&socket_path)?;

        let sql = db::open_database()?;

        Ok(Self {
            sql,
            logger,
            socket_path,
        })
    }

    async fn start_inner(&self) -> Result<(), CliError> {
        self.logger.info("starting daemon");
        self.prepare_database()?;
        self.start_http_listener().await
    }

    fn prepare_database(&self) -> Result<(), CliError> {
        self.log_timed("prepareDatabase", || db::prepare_schema(&self.sql))
    }

    async fn start_http_listener(&self) -> Result<(), CliError> {
        remove_stale_socket(&self.socket_path)?;

        let listener = UnixListener::bind(&self.socket_path).map_err(|source| {
            CliError::with_source(
                format!("failed to bind {}", self.socket_path.display()),
                source,
            )
        })?;

        let pid_path = paths::pid_file_path();
        ensure_parent_dir(&pid_path)?;
        fs::write(&pid_path, std::process::id().to_string()).map_err(|source| {
            CliError::with_source(format!("failed to write {}", pid_path.display()), source)
        })?;

        let socket_guard = SocketCleanup {
            path: self.socket_path.clone(),
            pid_path,
        };

        self.logger
            .info(format!("http listening on {}", self.socket_path.display()));

        let deletion_notify = Arc::new(Notify::new());
        let pool_notify = Arc::new(Notify::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Spawn background deletion worker.
        let deletion_handle = tokio::spawn(deletion_worker(
            deletion_notify.clone(),
            shutdown_rx.clone(),
            self.logger.clone(),
        ));

        // Spawn background pool worker.
        let pool_handle = tokio::spawn(pool_worker(
            pool_notify.clone(),
            shutdown_rx,
            self.logger.clone(),
        ));

        let state = AppState {
            deletion_notify,
            pool_notify,
            logger: self.logger.clone(),
        };

        let app = Router::new()
            .route("/", get(root))
            .route("/healthz", get(healthz))
            .route("/tasks/process-deletions", post(process_deletions))
            .route("/pool/replenish", post(pool_replenish))
            .route("/pool/clear", post(handle_clear_pool))
            .route("/projects/create", post(handle_create_project))
            .route("/projects/list", post(handle_list_projects))
            .route("/projects/delete", post(handle_delete_project))
            .route("/projects/detect", post(handle_detect_project))
            .route("/tasks/create", post(handle_create_task))
            .route("/tasks/list", post(handle_list_tasks))
            .route("/tasks/delete", post(handle_delete_task))
            .route("/tasks/nuke", post(handle_nuke))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                request_logger,
            ))
            .with_state(state);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(self.logger.clone()))
            .await
            .map_err(|source| CliError::with_source("http listener exited unexpectedly", source))?;

        self.logger
            .info("waiting for in-flight work to finish (signal again to force quit)");
        let _ = shutdown_tx.send(true);

        tokio::select! {
            _ = async {
                let _ = tokio::join!(deletion_handle, pool_handle);
            } => {}
            _ = force_shutdown_signal(self.logger.clone()) => {
                self.logger.info("forced shutdown");
            }
        }

        self.logger.info("shutdown complete");

        drop(socket_guard);

        Ok(())
    }

    fn log_timed<T, F>(&self, operation: &str, f: F) -> Result<T, CliError>
    where
        F: FnOnce() -> Result<T, CliError>,
    {
        let start = Instant::now();

        match f() {
            Ok(value) => {
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                self.logger
                    .info(format!("{operation} \u{2713} ({elapsed_ms:.1}ms)"));
                Ok(value)
            }
            Err(error) => {
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                self.logger
                    .error(format!("{operation} \u{2717} ({elapsed_ms:.1}ms)"));
                Err(error)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Background deletion worker
// ---------------------------------------------------------------------------

struct DeletionTask {
    id: i64,
    name: String,
    path: String,
    force: bool,
    project_path: String,
}

async fn deletion_worker(notify: Arc<Notify>, mut shutdown: watch::Receiver<bool>, logger: Logger) {
    let logger = logger.child("deletionWorker");

    // Drain any stale "deleting" rows left from a previous run.
    notify.notify_one();

    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = shutdown.changed() => {
                logger.info("shutdown received, finishing in-flight deletions");
                break;
            }
        }

        process_pending_deletions(&logger).await;
    }
}

async fn process_pending_deletions(logger: &Logger) {
    let tasks = {
        let query_logger = logger.clone();
        match tokio::task::spawn_blocking(move || query_deleting_tasks(&query_logger)).await {
            Ok(Ok(tasks)) => tasks,
            Ok(Err(e)) => {
                logger.error(format!("failed to query deleting tasks: {e}"));
                return;
            }
            Err(e) => {
                logger.error(format!("query task panicked: {e}"));
                return;
            }
        }
    };

    if tasks.is_empty() {
        return;
    }

    logger.info(format!("processing {} deletion(s)", tasks.len()));

    let semaphore = Arc::new(Semaphore::new(4));
    let mut handles = Vec::new();

    for task in tasks {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let logger = logger.clone();

        handles.push(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            process_deletion(&logger, task);
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
}

fn query_deleting_tasks(logger: &Logger) -> Result<Vec<DeletionTask>, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.path, t.deleteForce, p.path \
             FROM tasks t JOIN projects p ON t.projectId = p.id \
             WHERE t.status = 'deleting'",
        )
        .map_err(|e| CliError::with_source("failed to prepare deletion query", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(DeletionTask {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                force: row.get(3)?,
                project_path: row.get(4)?,
            })
        })
        .map_err(|e| CliError::with_source("failed to query deleting tasks", e))?;

    let tasks = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CliError::with_source("failed to load deleting tasks", e))?;

    if !tasks.is_empty() {
        logger.info(format!("found {} task(s) pending deletion", tasks.len()));
    }

    Ok(tasks)
}

fn process_deletion(logger: &Logger, task: DeletionTask) {
    let adapter = GitWorktreeAdapter;

    if let Err(e) = adapter.remove(
        &task.project_path,
        &task.name,
        Path::new(&task.path),
        task.force,
    ) {
        logger.error(format!("failed to remove worktree for {}: {e}", task.name));
        return;
    }

    match db::open_database() {
        Ok(conn) => match conn.execute("DELETE FROM tasks WHERE id = ?1", params![task.id]) {
            Ok(_) => logger.info(format!("deleted task {}", task.name)),
            Err(e) => logger.error(format!("failed to remove {} from database: {e}", task.name)),
        },
        Err(e) => {
            logger.error(format!(
                "failed to open database for {} cleanup: {e}",
                task.name
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Background pool worker
// ---------------------------------------------------------------------------

struct PoolProject {
    id: i64,
    name: String,
    path: String,
}

async fn pool_worker(notify: Arc<Notify>, mut shutdown: watch::Receiver<bool>, logger: Logger) {
    let logger = logger.child("poolWorker");

    // Fill any deficit on startup.
    notify.notify_one();

    loop {
        let poll_secs = config::load()
            .ok()
            .and_then(|c| c.daemon)
            .map_or(300, |d| d.pool_poll_interval);

        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(poll_secs)) => {
                logger.info("periodic poll");
            }
            _ = shutdown.changed() => {
                logger.info("shutdown received");
                break;
            }
        }

        if let Err(e) = process_pool_jobs(&logger).await {
            logger.error(format!("pool job processing failed: {e}"));
        }

        if let Err(e) = replenish_pools(&logger, &mut shutdown).await {
            logger.error(format!("pool replenishment failed: {e}"));
        }
    }
}

async fn process_pool_jobs(logger: &Logger) -> Result<(), CliError> {
    let job_ids = {
        let logger = logger.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db::open_database()?;
            db::prepare_schema(&conn)?;

            let mut stmt = conn
                .prepare("SELECT id FROM jobs WHERE kind = 'clear_pool' ORDER BY createdAt ASC")
                .map_err(|e| CliError::with_source("failed to query pool jobs", e))?;

            let ids: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .map_err(|e| CliError::with_source("failed to query pool jobs", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CliError::with_source("failed to load pool jobs", e))?;

            if !ids.is_empty() {
                logger.info(format!("found {} clear_pool job(s)", ids.len()));
            }

            Ok(ids)
        })
        .await
        .map_err(|e| CliError::new(format!("query pool jobs task panicked: {e}")))?
    }?;

    for job_id in job_ids {
        let logger = logger.clone();
        tokio::task::spawn_blocking(move || {
            process_clear_pool_job(&logger, job_id);
        })
        .await
        .map_err(|e| CliError::new(format!("clear_pool job task panicked: {e}")))?;
    }

    Ok(())
}

fn process_clear_pool_job(logger: &Logger, job_id: i64) {
    let conn = match db::open_database() {
        Ok(c) => c,
        Err(e) => {
            logger.error(format!("failed to open database for clear_pool job: {e}"));
            return;
        }
    };

    if let Err(e) = db::prepare_schema(&conn) {
        logger.error(format!("failed to prepare schema for clear_pool job: {e}"));
        return;
    }

    let adapter = GitWorktreeAdapter;

    let entries: Vec<(i64, String, String, String)> = match conn.prepare(
        "SELECT po.id, po.tempName, po.path, p.path \
         FROM pool po JOIN projects p ON po.projectId = p.id",
    ) {
        Ok(mut stmt) => match stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                logger.error(format!("failed to query pool entries: {e}"));
                return;
            }
        },
        Err(e) => {
            logger.error(format!("failed to prepare pool query: {e}"));
            return;
        }
    };

    for (pool_id, temp_name, pool_path, project_path) in &entries {
        let _ = adapter.remove(project_path, temp_name, Path::new(pool_path), true);
        if let Err(e) = conn.execute("DELETE FROM pool WHERE id = ?1", params![pool_id]) {
            logger.error(format!(
                "failed to delete pool entry {temp_name} from database: {e}"
            ));
        } else {
            logger.info(format!("cleared pool entry {temp_name}"));
        }
    }

    match conn.execute("DELETE FROM jobs WHERE id = ?1", params![job_id]) {
        Ok(_) => logger.info(format!("completed clear_pool job {job_id}")),
        Err(e) => logger.error(format!("failed to delete job {job_id}: {e}")),
    }
}

async fn replenish_pools(
    logger: &Logger,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<(), CliError> {
    let global_config = config::load()?;

    let daemon_cfg = global_config.daemon.as_ref();
    let max_load_frac = daemon_cfg.map_or(0.7, |d| d.pool_max_load);
    let min_memory_pct = daemon_cfg.map_or(10.0, |d| d.pool_min_memory_pct);
    let projects = {
        let query_logger = logger.clone();
        match tokio::task::spawn_blocking(move || query_all_projects(&query_logger)).await {
            Ok(Ok(projects)) => projects,
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(CliError::new(format!("query projects task panicked: {e}")));
            }
        }
    };

    for project in &projects {
        let pool_size = config::effective_pool_size(&global_config, &project.name, &project.path);

        if pool_size == 0 {
            continue;
        }

        let default_branch =
            config::effective_default_branch(&global_config, &project.name, &project.path);

        let current_count = {
            let project_id = project.id;
            match tokio::task::spawn_blocking(move || count_pool_entries(project_id)).await {
                Ok(Ok(count)) => count,
                Ok(Err(e)) => {
                    logger.error(format!(
                        "failed to count pool entries for {}: {e}",
                        project.name
                    ));
                    continue;
                }
                Err(e) => {
                    logger.error(format!(
                        "count pool task panicked for {}: {e}",
                        project.name
                    ));
                    continue;
                }
            }
        };

        let deficit = pool_size.saturating_sub(current_count);

        if deficit == 0 {
            continue;
        }

        logger.info(format!(
            "project {}: pool {current_count}/{pool_size}, creating {deficit} worktree(s)",
            project.name
        ));

        for _ in 0..deficit {
            // Check for shutdown before each creation.
            if *shutdown.borrow() {
                logger.info("shutdown during pool replenishment, stopping");
                return Ok(());
            }

            // Resource gating: skip if system is under pressure.
            let (load_ok, mem_ok) = check_system_resources(max_load_frac, min_memory_pct);
            if !load_ok || !mem_ok {
                if !load_ok {
                    logger.info("skipping: system load too high");
                }
                if !mem_ok {
                    logger.info("skipping: available memory too low");
                }
                return Ok(());
            }

            let temp_name = generate_pool_temp_name();
            let worktree_path = paths::worktree_path(&project.name, &temp_name);
            let project_path = project.path.clone();
            let project_id = project.id;
            let project_name = project.name.clone();
            let temp_name_clone = temp_name.clone();
            let worktree_path_clone = worktree_path.clone();
            let logger_clone = logger.clone();
            let default_branch_clone = default_branch.clone();

            let result = tokio::task::spawn_blocking(move || {
                let adapter = GitWorktreeAdapter;
                adapter.create(&project_path, &temp_name_clone, &worktree_path_clone, &default_branch_clone)?;

                let conn = db::open_database()?;
                db::prepare_schema(&conn)?;

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| CliError::with_source("system clock error", e))?
                    .as_secs() as i64;

                let wt_str = worktree_path_clone.to_string_lossy().to_string();
                match conn.execute(
                    "INSERT INTO pool (projectId, tempName, path, createdAt) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![project_id, temp_name_clone, wt_str, now],
                ) {
                    Ok(_) => {
                        logger_clone.info(format!(
                            "created pool worktree {temp_name_clone} for {project_name}"
                        ));
                        Ok(())
                    }
                    Err(e) => {
                        // DB insert failed — clean up the worktree we just created.
                        logger_clone.error(format!(
                            "failed to insert pool entry {temp_name_clone}: {e}, cleaning up worktree"
                        ));
                        let _ = adapter.remove(
                            &project_name,
                            &temp_name_clone,
                            &worktree_path_clone,
                            true,
                        );
                        Err(CliError::with_source("failed to insert pool entry", e))
                    }
                }
            })
            .await;

            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    logger.error(format!("pool creation failed for {}: {e}", project.name));
                }
                Err(e) => {
                    logger.error(format!(
                        "pool creation task panicked for {}: {e}",
                        project.name
                    ));
                }
            }
        }
    }

    Ok(())
}

fn query_all_projects(logger: &Logger) -> Result<Vec<PoolProject>, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let mut stmt = conn
        .prepare("SELECT id, name, path FROM projects")
        .map_err(|e| CliError::with_source("failed to prepare project query", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PoolProject {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
            })
        })
        .map_err(|e| CliError::with_source("failed to query projects", e))?;

    let projects = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CliError::with_source("failed to load projects", e))?;

    logger.info(format!("found {} project(s) to check", projects.len()));
    Ok(projects)
}

fn count_pool_entries(project_id: i64) -> Result<u32, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM pool WHERE projectId = ?1",
            rusqlite::params![project_id],
            |row| row.get(0),
        )
        .map_err(|e| CliError::with_source("failed to count pool entries", e))?;

    Ok(count)
}

fn check_system_resources(max_load_frac: f64, min_memory_pct: f64) -> (bool, bool) {
    let load_avg = System::load_average();
    let num_cpus = {
        let mut sys = System::new();
        sys.refresh_cpu_list(sysinfo::CpuRefreshKind::default());
        let count = sys.cpus().len();
        if count > 0 { count as f64 } else { 1.0 }
    };
    let load_threshold = max_load_frac * num_cpus;
    let load_ok = load_avg.one <= load_threshold;

    let mut sys = System::new();
    sys.refresh_memory();
    let total_mem = sys.total_memory();
    let available_mem = sys.available_memory();
    let mem_ok = if total_mem > 0 {
        let available_pct = (available_mem as f64 / total_mem as f64) * 100.0;
        available_pct >= min_memory_pct
    } else {
        true // Can't determine memory, don't gate.
    };

    (load_ok, mem_ok)
}

fn generate_pool_temp_name() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let hex: u32 = rng.r#gen();
    format!("__pool-{hex:08x}")
}

// ---------------------------------------------------------------------------
// Request logging middleware
// ---------------------------------------------------------------------------

async fn request_logger(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let status = response.status().as_u16();
    state
        .logger
        .info(format!("{method} {path} {status} ({elapsed_ms:.1}ms)"));
    response
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn root() -> impl IntoResponse {
    "workd\n"
}

async fn healthz() -> impl IntoResponse {
    "ok\n"
}

async fn process_deletions(State(state): State<AppState>) -> impl IntoResponse {
    state.deletion_notify.notify_one();
    "ok\n"
}

async fn pool_replenish(State(state): State<AppState>) -> impl IntoResponse {
    state.pool_notify.notify_one();
    "ok\n"
}

// ---------------------------------------------------------------------------
// API handlers
// ---------------------------------------------------------------------------

async fn handle_create_project(
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, ApiError> {
    Ok(Json(run_blocking(move || create_project_inner(req)).await?))
}

async fn handle_list_projects() -> Result<Json<Vec<ProjectInfo>>, ApiError> {
    Ok(Json(run_blocking(list_projects_inner).await?))
}

async fn handle_delete_project(
    Json(req): Json<DeleteProjectRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    run_blocking(move || delete_project_inner(req)).await?;
    Ok(Json(serde_json::json!({})))
}

async fn handle_detect_project(
    Json(req): Json<DetectProjectRequest>,
) -> Result<Json<ProjectInfo>, ApiError> {
    Ok(Json(
        run_blocking(move || detect_project_handler(req)).await?,
    ))
}

async fn handle_create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, ApiError> {
    let pool_notify = state.pool_notify.clone();
    Ok(Json(
        run_blocking(move || create_task_inner(req, pool_notify)).await?,
    ))
}

async fn handle_list_tasks(
    Json(req): Json<ListTasksRequest>,
) -> Result<Json<Vec<TaskInfo>>, ApiError> {
    Ok(Json(run_blocking(move || list_tasks_inner(req)).await?))
}

async fn handle_delete_task(
    Json(req): Json<DeleteTaskRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    run_blocking(move || delete_task_inner(req)).await?;
    Ok(Json(serde_json::json!({})))
}

async fn handle_nuke() -> Result<Json<NukeResponse>, ApiError> {
    Ok(Json(run_blocking(nuke_inner).await?))
}

async fn handle_clear_pool(
    State(state): State<AppState>,
) -> Result<Json<ClearPoolResponse>, ApiError> {
    let resp = run_blocking(clear_pool_inner).await?;
    state.pool_notify.notify_one();
    Ok(Json(resp))
}

// ---------------------------------------------------------------------------
// API handler implementations
// ---------------------------------------------------------------------------

fn create_project_inner(req: CreateProjectRequest) -> Result<CreateProjectResponse, CliError> {
    let name = resolve_project_name(&req.path, req.name)?;
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let now = unix_timestamp_seconds()?;
    conn.execute(
        "INSERT INTO projects (name, path, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4)",
        params![name, req.path, now, now],
    )
    .map_err(map_project_insert_error)?;

    Ok(CreateProjectResponse { name })
}

fn list_projects_inner() -> Result<Vec<ProjectInfo>, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let mut stmt = conn
        .prepare("SELECT name, path, createdAt, updatedAt FROM projects ORDER BY name")
        .map_err(|e| CliError::with_source("failed to prepare project query", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ProjectInfo {
                name: row.get(0)?,
                path: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|e| CliError::with_source("failed to query projects", e))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| CliError::with_source("failed to load projects", e))
}

fn delete_project_inner(req: DeleteProjectRequest) -> Result<(), CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let project_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM projects WHERE name = ?1",
            params![req.name],
            |row| row.get(0),
        )
        .ok();

    if let Some(project_id) = project_id {
        let mut pool_stmt = conn
            .prepare(
                "SELECT po.tempName, po.path, p.path \
                 FROM pool po JOIN projects p ON po.projectId = p.id \
                 WHERE po.projectId = ?1",
            )
            .map_err(|e| CliError::with_source("failed to query pool entries", e))?;

        let pool_entries: Vec<(String, String, String)> = pool_stmt
            .query_map(params![project_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| CliError::with_source("failed to query pool entries", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CliError::with_source("failed to load pool entries", e))?;

        let adapter = GitWorktreeAdapter;
        for (temp_name, pool_path, project_path) in &pool_entries {
            let _ = adapter.remove(project_path, temp_name, Path::new(pool_path), true);
        }

        conn.execute("DELETE FROM pool WHERE projectId = ?1", params![project_id])
            .map_err(|e| CliError::with_source("failed to delete pool entries", e))?;
    }

    let deleted = conn
        .execute("DELETE FROM projects WHERE name = ?1", params![req.name])
        .map_err(|e| CliError::with_source("failed to delete project", e))?;

    if deleted == 0 {
        return Err(CliError::with_hint(
            "project not found",
            "run `work projects list` to see existing project names",
        ));
    }

    Ok(())
}

fn detect_project_handler(req: DetectProjectRequest) -> Result<ProjectInfo, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let (id, _, _) = detect_project(&conn, req.project.as_deref(), &req.cwd)?;

    conn.query_row(
        "SELECT name, path, createdAt, updatedAt FROM projects WHERE id = ?1",
        params![id],
        |row| {
            Ok(ProjectInfo {
                name: row.get(0)?,
                path: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .map_err(|e| CliError::with_source("failed to look up project", e))
}

fn create_task_inner(
    req: CreateTaskRequest,
    pool_notify: Arc<Notify>,
) -> Result<CreateTaskResponse, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let (project_id, project_name, project_path) =
        detect_project(&conn, req.project.as_deref(), &req.cwd)?;
    let task_name = req
        .name
        .or_else(|| req.branch.clone())
        .unwrap_or_else(generate_task_name);
    let worktree_path = paths::worktree_path(&project_name, &task_name);

    let adapter = GitWorktreeAdapter;
    let global_cfg = config::load()?;

    if let Some(ref branch) = req.branch {
        adapter.create_from_branch(&project_path, branch, &worktree_path)?;
    } else {
        let default_branch =
            config::effective_default_branch(&global_cfg, &project_name, &project_path);

        let claimed = try_claim_pool(
            &conn,
            &adapter,
            project_id,
            &project_path,
            &task_name,
            &worktree_path,
            &pool_notify,
        );

        if !claimed {
            adapter.create(&project_path, &task_name, &worktree_path, &default_branch)?;
        }
    }

    let now = unix_timestamp_seconds()?;
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    conn.execute(
        "INSERT INTO tasks (projectId, name, path, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![project_id, task_name, worktree_path_str, now, now],
    )
    .map_err(|e| CliError::with_source("failed to create task", e))?;

    let project_cfg = config::load_project_config(&project_path)?;
    let hook_script = config::project_hook_script(&project_cfg, "new-after")
        .map(|s| s.to_string())
        .or_else(|| {
            config::hook_script(&global_cfg, &project_name, "new-after").map(|s| s.to_string())
        });

    Ok(CreateTaskResponse {
        name: task_name,
        path: worktree_path_str,
        hook_script,
    })
}

fn list_tasks_inner(req: ListTasksRequest) -> Result<Vec<TaskInfo>, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let all = req.all.unwrap_or(false);

    if all {
        let mut stmt = conn
            .prepare(
                "SELECT t.name, t.path, t.createdAt, t.updatedAt, p.name \
                 FROM tasks t JOIN projects p ON t.projectId = p.id \
                 WHERE t.status = 'active' \
                 ORDER BY p.name, t.name",
            )
            .map_err(|e| CliError::with_source("failed to prepare task query", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(TaskInfo {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    project_name: Some(row.get(4)?),
                })
            })
            .map_err(|e| CliError::with_source("failed to query tasks", e))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| CliError::with_source("failed to load tasks", e))
    } else {
        let cwd = req.cwd.as_deref().unwrap_or("");
        let (project_id, _, _) = detect_project(&conn, req.project.as_deref(), cwd)?;

        let mut stmt = conn
            .prepare(
                "SELECT name, path, createdAt, updatedAt \
                 FROM tasks WHERE projectId = ?1 AND status = 'active' ORDER BY name",
            )
            .map_err(|e| CliError::with_source("failed to prepare task query", e))?;

        let rows = stmt
            .query_map(params![project_id], |row| {
                Ok(TaskInfo {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    project_name: None,
                })
            })
            .map_err(|e| CliError::with_source("failed to query tasks", e))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| CliError::with_source("failed to load tasks", e))
    }
}

fn delete_task_inner(req: DeleteTaskRequest) -> Result<(), CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let (project_id, _, project_path) = detect_project(&conn, req.project.as_deref(), &req.cwd)?;
    let force = req.force.unwrap_or(false);

    let (task_id, task_path): (i64, String) = conn
        .query_row(
            "SELECT id, path FROM tasks WHERE projectId = ?1 AND name = ?2 AND status = 'active'",
            params![project_id, req.name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|source| match source {
            rusqlite::Error::QueryReturnedNoRows => {
                CliError::with_hint("task not found", "run `work list` to see existing tasks")
            }
            other => CliError::with_source("failed to look up task", other),
        })?;

    let adapter = GitWorktreeAdapter;
    adapter.remove(&project_path, &req.name, Path::new(&task_path), force)?;

    conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])
        .map_err(|e| CliError::with_source("failed to delete task from database", e))?;

    Ok(())
}

fn nuke_inner() -> Result<NukeResponse, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let adapter = GitWorktreeAdapter;

    // Remove pool worktrees first.
    let mut pool_stmt = conn
        .prepare(
            "SELECT po.tempName, po.path, p.path \
             FROM pool po JOIN projects p ON po.projectId = p.id",
        )
        .map_err(|e| CliError::with_source("failed to query pool entries", e))?;

    let pool_entries: Vec<(String, String, String)> = pool_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| CliError::with_source("failed to query pool entries", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CliError::with_source("failed to load pool entries", e))?;

    for (temp_name, pool_path, project_path) in &pool_entries {
        let _ = adapter.remove(project_path, temp_name, Path::new(pool_path), true);
    }

    conn.execute("DELETE FROM pool", [])
        .map_err(|e| CliError::with_source("failed to delete pool entries", e))?;

    // Remove task worktrees.
    let mut stmt = conn
        .prepare(
            "SELECT t.name, t.path, p.path \
             FROM tasks t JOIN projects p ON t.projectId = p.id",
        )
        .map_err(|e| CliError::with_source("failed to query tasks", e))?;

    let tasks: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| CliError::with_source("failed to query tasks", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CliError::with_source("failed to load tasks", e))?;

    for (task_name, task_path, project_path) in &tasks {
        adapter.remove(project_path, task_name, Path::new(task_path), true)?;
    }

    conn.execute("DELETE FROM tasks", [])
        .map_err(|e| CliError::with_source("failed to delete tasks", e))?;

    let deleted_projects = conn
        .execute("DELETE FROM projects", [])
        .map_err(|e| CliError::with_source("failed to delete projects", e))?;

    Ok(NukeResponse {
        tasks: tasks.len(),
        pool_worktrees: pool_entries.len(),
        projects: deleted_projects,
    })
}

fn clear_pool_inner() -> Result<ClearPoolResponse, CliError> {
    let conn = db::open_database()?;
    db::prepare_schema(&conn)?;

    let count: usize = conn
        .query_row("SELECT COUNT(*) FROM pool", [], |row| row.get(0))
        .map_err(|e| CliError::with_source("failed to count pool entries", e))?;

    let now = unix_timestamp_seconds()?;
    conn.execute(
        "INSERT INTO jobs (kind, createdAt) VALUES ('clear_pool', ?1)",
        params![now],
    )
    .map_err(|e| CliError::with_source("failed to create clear_pool job", e))?;

    Ok(ClearPoolResponse {
        pool_worktrees: count,
    })
}

// ---------------------------------------------------------------------------
// Shared helpers (used by API handlers)
// ---------------------------------------------------------------------------

fn detect_project(
    connection: &rusqlite::Connection,
    explicit_project: Option<&str>,
    cwd: &str,
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

    let projects: Vec<(i64, String, String)> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CliError::with_source("failed to load project", source))?;

    // First: try matching cwd against project repo paths.
    let mut best: Option<(i64, String, String)> = None;
    let mut best_len = 0;

    for (id, name, path) in &projects {
        if cwd.starts_with(path)
            && (cwd.len() == path.len() || cwd.as_bytes()[path.len()] == b'/')
            && path.len() > best_len
        {
            best_len = path.len();
            best = Some((*id, name.clone(), path.clone()));
        }
    }

    // Second: try matching cwd against managed worktree paths.
    if best.is_none() {
        for (id, name, path) in &projects {
            let wt_base = paths::project_worktrees_dir(name);
            let wt_base_str = wt_base.to_string_lossy();
            if cwd.starts_with(wt_base_str.as_ref())
                && (cwd.len() == wt_base_str.len() || cwd.as_bytes()[wt_base_str.len()] == b'/')
            {
                best = Some((*id, name.clone(), path.clone()));
                break;
            }
        }
    }

    best.ok_or_else(|| {
        CliError::with_hint(
            "could not detect project from current directory",
            "run `work projects create` to register a project, or pass --project",
        )
    })
}

fn try_claim_pool(
    connection: &rusqlite::Connection,
    adapter: &GitWorktreeAdapter,
    project_id: i64,
    project_path: &str,
    task_name: &str,
    worktree_path: &Path,
    pool_notify: &Arc<Notify>,
) -> bool {
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

            match adapter.claim_pooled(project_path, &temp_name, task_name, old_path, worktree_path)
            {
                Ok(()) => {
                    pool_notify.notify_one();
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_parent_dir(path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            CliError::with_source(format!("failed to create {}", parent.display()), source)
        })?;
    }

    Ok(())
}

fn remove_stale_socket(socket_path: &Path) -> Result<(), CliError> {
    if !socket_path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(socket_path).map_err(|source| {
        CliError::with_source(format!("failed to stat {}", socket_path.display()), source)
    })?;

    if metadata.file_type().is_socket() {
        fs::remove_file(socket_path).map_err(|source| {
            CliError::with_source(
                format!("failed to remove {}", socket_path.display()),
                source,
            )
        })?;
        return Ok(());
    }

    Err(CliError::with_hint(
        format!("{} exists and is not a unix socket", socket_path.display()),
        "remove the existing file or choose another path with --socket",
    ))
}

async fn force_shutdown_signal(logger: Logger) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => logger.info("received second SIGINT"),
            _ = sigterm.recv() => logger.info("received second SIGTERM"),
        }
    }

    #[cfg(not(unix))]
    {
        if ctrl_c.await.is_ok() {
            logger.info("received second shutdown signal");
        }
    }
}

async fn shutdown_signal(logger: Logger) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => logger.info("received SIGINT"),
            _ = sigterm.recv() => logger.info("received SIGTERM"),
        }
    }

    #[cfg(not(unix))]
    {
        if ctrl_c.await.is_ok() {
            logger.info("received shutdown signal");
        }
    }
}

struct SocketCleanup {
    path: PathBuf,
    pid_path: PathBuf,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(&self.pid_path);
    }
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

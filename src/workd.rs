use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use rusqlite::{Connection, params};
use tokio::net::UnixListener;
use tokio::sync::{watch, Notify, Semaphore};

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
        };

        let app = Router::new()
            .route("/", get(root))
            .route("/healthz", get(healthz))
            .route("/tasks/process-deletions", post(process_deletions))
            .route("/pool/replenish", post(pool_replenish))
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
            Err(e) => logger.error(format!(
                "failed to remove {} from database: {e}",
                task.name
            )),
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

        if let Err(e) = replenish_pools(&logger, &mut shutdown).await {
            logger.error(format!("pool replenishment failed: {e}"));
        }
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
        let pool_size =
            config::effective_pool_size(&global_config, &project.name, &project.path);

        if pool_size == 0 {
            continue;
        }

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
                    logger.error(format!("count pool task panicked for {}: {e}", project.name));
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

            let result = tokio::task::spawn_blocking(move || {
                let adapter = GitWorktreeAdapter;
                adapter.create(&project_path, &temp_name_clone, &worktree_path_clone)?;

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
                    logger.error(format!("pool creation task panicked for {}: {e}", project.name));
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
        let mut sigterm = signal(SignalKind::terminate())
            .expect("failed to register SIGTERM handler");

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
        let mut sigterm = signal(SignalKind::terminate())
            .expect("failed to register SIGTERM handler");

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

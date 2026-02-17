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

use crate::adapters::TaskAdapter;
use crate::adapters::worktree::GitWorktreeAdapter;
use crate::db;
use crate::error::CliError;
use crate::logger::Logger;
use crate::paths;

#[derive(Clone)]
struct AppState {
    deletion_notify: Arc<Notify>,
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
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Spawn background deletion worker.
        let worker_handle = tokio::spawn(deletion_worker(
            deletion_notify.clone(),
            shutdown_rx,
            self.logger.clone(),
        ));

        let state = AppState { deletion_notify };

        let app = Router::new()
            .route("/", get(root))
            .route("/healthz", get(healthz))
            .route("/tasks/process-deletions", post(process_deletions))
            .with_state(state);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(self.logger.clone()))
            .await
            .map_err(|source| CliError::with_source("http listener exited unexpectedly", source))?;

        self.logger
            .info("waiting for in-flight deletions to finish (signal again to force quit)");
        let _ = shutdown_tx.send(true);

        tokio::select! {
            _ = worker_handle => {}
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

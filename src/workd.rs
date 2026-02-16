use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use axum::routing::get;
use axum::{Router, response::IntoResponse};
use rusqlite::Connection;
use tokio::net::UnixListener;

use crate::db;
use crate::error::CliError;
use crate::logger::Logger;
use crate::paths;

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

        let socket_guard = SocketCleanup {
            path: self.socket_path.clone(),
        };

        self.logger
            .info(format!("http listening on {}", self.socket_path.display()));

        let app = Router::new()
            .route("/", get(root))
            .route("/healthz", get(healthz));

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(self.logger.clone()))
            .await
            .map_err(|source| CliError::with_source("http listener exited unexpectedly", source))?;

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
                    .info(format!("{operation} ✓ ({elapsed_ms:.1}ms)"));
                Ok(value)
            }
            Err(error) => {
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                self.logger
                    .error(format!("{operation} ✗ ({elapsed_ms:.1}ms)"));
                Err(error)
            }
        }
    }
}

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

async fn root() -> impl IntoResponse {
    "workd\n"
}

async fn healthz() -> impl IntoResponse {
    "ok\n"
}

async fn shutdown_signal(logger: Logger) {
    if tokio::signal::ctrl_c().await.is_ok() {
        logger.info("received shutdown signal");
    } else {
        logger.error("failed to listen for shutdown signal");
    }
}

struct SocketCleanup {
    path: PathBuf,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

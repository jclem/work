mod jobs;
mod routes;

use std::fs;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::routing::{delete, get, post};
use tokio::net::UnixListener;
use tokio::sync::watch;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

fn pid_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("work.pid")
}

fn socket_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("work.sock")
}

fn cleanup(runtime_dir: &Path) {
    let pid = pid_path(runtime_dir);
    let sock = socket_path(runtime_dir);
    if pid.exists() {
        let _ = fs::remove_file(&pid);
        tracing::debug!(path = %pid.display(), "removed PID file");
    }
    if sock.exists() {
        let _ = fs::remove_file(&sock);
        tracing::debug!(path = %sock.display(), "removed socket file");
    }
}

pub async fn start(force: bool) -> anyhow::Result<()> {
    let runtime_dir = crate::paths::runtime_dir()?;
    fs::create_dir_all(&runtime_dir)?;

    let pid = pid_path(&runtime_dir);
    let sock = socket_path(&runtime_dir);

    if pid.exists() || sock.exists() {
        if force {
            tracing::debug!("--force: removing existing runtime files");
            cleanup(&runtime_dir);
        } else {
            anyhow::bail!(
                "daemon already running (found runtime files in {}); use --force to override",
                runtime_dir.display()
            );
        }
    }

    crate::db::initialize()?;
    tracing::debug!("database initialized");

    fs::write(&pid, std::process::id().to_string())?;
    tracing::debug!(path = %pid.display(), pid = std::process::id(), "wrote PID file");

    let listener = UnixListener::bind(&sock)?;
    tracing::info!(socket = %sock.display(), "listening");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let job_handle = tokio::spawn(jobs::run(shutdown_rx));

    let app = Router::new()
        .route("/health", get(routes::health))
        .route(
            "/projects",
            get(routes::list_projects).post(routes::create_project),
        )
        .route("/projects/{name}", delete(routes::delete_project))
        .route(
            "/environments",
            get(routes::list_environments).post(routes::prepare_environment),
        )
        .route(
            "/environments/{id}/update",
            post(routes::update_environment),
        )
        .route("/environments/{id}/claim", post(routes::claim_environment))
        .route("/environments/claim", post(routes::claim_next_environment))
        .route("/environments/{id}", delete(routes::remove_environment))
        .route("/tasks", get(routes::list_tasks).post(routes::create_task))
        .route(
            "/tasks/{id}",
            get(routes::get_task).delete(routes::remove_task),
        )
        .route("/tasks/{id}/logs", get(routes::tail_task_logs))
        .route("/reset-database", post(routes::reset_database))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::TRACE))
                .on_response(DefaultOnResponse::new().level(tracing::Level::TRACE)),
        );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    let _ = shutdown_tx.send(true);
    let _ = job_handle.await;

    cleanup(&runtime_dir);
    tracing::info!("daemon shut down");

    Ok(())
}

async fn shutdown_signal() {
    let mut sigint =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = sigint.recv() => { tracing::debug!("received SIGINT"); }
        _ = sigterm.recv() => { tracing::debug!("received SIGTERM"); }
    }
}

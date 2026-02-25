pub mod events;
mod jobs;
mod routes;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use axum::Router;
use axum::routing::{delete, get, post};
use tokio::net::UnixListener;
use tokio::sync::watch;
use tower_http::trace::TraceLayer;

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
    tracing::info!(
        config = %crate::paths::config_dir()?.display(),
        data = %crate::paths::data_dir()?.display(),
        state = %crate::paths::state_dir()?.display(),
        runtime = %crate::paths::runtime_dir()?.display(),
        "starting daemon"
    );

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
        .route("/events", get(routes::events))
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
        .route("/environments/{id}/logs", get(routes::tail_environment_logs))
        .route("/tasks", get(routes::list_tasks).post(routes::create_task))
        .route(
            "/tasks/{id}",
            get(routes::get_task).delete(routes::remove_task),
        )
        .route("/tasks/{id}/logs", get(routes::tail_task_logs))
        .route("/reset-database", post(routes::reset_database))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<_>| {
                    tracing::info_span!("request", method = %req.method(), path = %req.uri().path())
                })
                .on_response(
                    |res: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     _span: &tracing::Span| {
                        tracing::info!(status = %res.status().as_u16(), latency_ms = latency.as_millis(), "response");
                    },
                ),
        );

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_signal().await;
            tracing::info!("closing event streams");
            events::shutdown();
        })
        .await?;

    // Spawn a task that forces exit on a second signal.
    let rd = runtime_dir.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::warn!("received second signal, forcing shutdown");
        cleanup(&rd);
        std::process::exit(1);
    });

    tracing::info!("stopping job processor");
    let _ = shutdown_tx.send(true);
    let _ = job_handle.await;

    cleanup(&runtime_dir);
    tracing::info!("daemon shut down");

    Ok(())
}

const LABEL: &str = "com.jclem.work";

fn plist_path() -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn get_uid() -> anyhow::Result<String> {
    let output = Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        anyhow::bail!("failed to get uid");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn install() -> anyhow::Result<()> {
    let binary_path = std::env::current_exe()?;
    let state_dir = crate::paths::state_dir()?;
    fs::create_dir_all(&state_dir)?;

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>daemon</string>
        <string>start</string>
        <string>--force</string>
    </array>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{out_log}</string>
    <key>StandardErrorPath</key>
    <string>{err_log}</string>
</dict>
</plist>
"#,
        label = LABEL,
        binary = binary_path.display(),
        out_log = state_dir.join("daemon.out.log").display(),
        err_log = state_dir.join("daemon.err.log").display(),
    );

    let plist_path = plist_path()?;
    fs::create_dir_all(plist_path.parent().unwrap())?;
    fs::write(&plist_path, &plist)?;

    let uid = get_uid()?;
    let status = Command::new("launchctl")
        .args([
            "bootstrap",
            &format!("gui/{uid}"),
            &plist_path.to_string_lossy(),
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("launchctl bootstrap failed with {status}");
    }

    println!("daemon installed and started ({})", plist_path.display());
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let plist_path = plist_path()?;
    let uid = get_uid()?;

    let status = Command::new("launchctl")
        .args([
            "bootout",
            &format!("gui/{uid}"),
            &plist_path.to_string_lossy(),
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("launchctl bootout failed with {status}");
    }

    fs::remove_file(&plist_path)?;
    println!("daemon uninstalled ({})", plist_path.display());
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

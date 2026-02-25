use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub async fn events() -> impl IntoResponse {
    let mut rx = super::events::subscribe();
    let (tx, mpsc_rx) = mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    if result.is_err() {
                        break;
                    }
                    let chunk = axum::body::Bytes::from("data: update\n\n");
                    if tx.send(Ok(chunk)).await.is_err() {
                        break;
                    }
                }
                _ = super::events::shutdown_notified() => {
                    break;
                }
            }
        }
    });

    let stream = ReceiverStream::new(mpsc_rx);
    let body = Body::from_stream(stream);

    (
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-cache"),
        ],
        body,
    )
}

pub async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

pub async fn list_projects() -> impl IntoResponse {
    match crate::db::list_projects() {
        Ok(projects) => (StatusCode::OK, Json(json!(projects))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list projects");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
}

pub async fn create_project(Json(body): Json<CreateProjectRequest>) -> impl IntoResponse {
    match crate::db::create_project(&body.name, &std::path::PathBuf::from(&body.path)) {
        Ok(()) => {
            tracing::debug!(name = %body.name, path = %body.path, "project created");
            super::events::notify();
            (
                StatusCode::CREATED,
                Json(json!({"name": body.name, "path": body.path})),
            )
                .into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("UNIQUE constraint") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn delete_project(Path(name): Path<String>) -> impl IntoResponse {
    match crate::db::delete_project(&name) {
        Ok(()) => {
            tracing::debug!(name = %name, "project removed");
            super::events::notify();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("project not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn reset_database() -> impl IntoResponse {
    match crate::db::reset() {
        Ok(()) => {
            tracing::debug!("database reset");
            super::events::notify();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to reset database");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct PrepareEnvironmentRequest {
    pub project_id: String,
    pub provider: String,
    #[serde(default)]
    pub claim_after_prepare: bool,
}

pub async fn prepare_environment(Json(body): Json<PrepareEnvironmentRequest>) -> impl IntoResponse {
    let result = crate::db::stage_prepare_environment(
        &body.project_id,
        &body.provider,
        body.claim_after_prepare,
    );

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, project_id = %env.project_id, "environment preparing");
            super::events::notify();
            (StatusCode::ACCEPTED, Json(json!(env))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to prepare environment");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn list_environments() -> impl IntoResponse {
    match crate::db::list_environments() {
        Ok(envs) => (StatusCode::OK, Json(json!(envs))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list environments");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn get_environment(Path(id): Path<String>) -> impl IntoResponse {
    match crate::db::get_environment(&id) {
        Ok(env) => (StatusCode::OK, Json(json!(env))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn update_environment(Path(id): Path<String>) -> impl IntoResponse {
    let result = crate::db::stage_update_environment(&id);

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, "environment update queued");
            super::events::notify();
            (StatusCode::ACCEPTED, Json(json!(env))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to update environment");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn claim_environment(Path(id): Path<String>) -> impl IntoResponse {
    let result = crate::db::stage_claim_environment(&id);

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, "environment claim queued");
            super::events::notify();
            (StatusCode::ACCEPTED, Json(json!(env))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to claim environment");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct ClaimNextEnvironmentRequest {
    pub provider: String,
    pub project_id: String,
}

#[derive(Default, serde::Deserialize)]
pub struct RemoveQuery {
    #[serde(default)]
    pub skip_provider: bool,
}

pub async fn claim_next_environment(
    Json(body): Json<ClaimNextEnvironmentRequest>,
) -> impl IntoResponse {
    let result = crate::db::stage_claim_next_environment(&body.provider, &body.project_id);

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, project_id = %env.project_id, "environment claim queued (next)");
            super::events::notify();
            (StatusCode::ACCEPTED, Json(json!(env))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to claim next environment");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn remove_environment(
    Path(id): Path<String>,
    Query(query): Query<RemoveQuery>,
) -> impl IntoResponse {
    let result = if query.skip_provider {
        crate::db::force_delete_environment(&id)
    } else {
        crate::db::stage_remove_environment(&id)
    };

    match result {
        Ok(()) => {
            tracing::debug!(id = %id, skip_provider = query.skip_provider, "environment removed request accepted");
            super::events::notify();
            if query.skip_provider {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::ACCEPTED.into_response()
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else if msg.contains("attached to task") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CreateTaskRequest {
    pub project_id: String,
    pub provider: String,
    pub env_provider: String,
    pub description: String,
}

pub async fn create_task(Json(body): Json<CreateTaskRequest>) -> impl IntoResponse {
    let result = crate::db::stage_task_create(
        &body.project_id,
        &body.provider,
        &body.env_provider,
        &body.description,
    );

    match result {
        Ok(task) => {
            tracing::debug!(id = %task.id, provider = %task.provider, "task created");
            super::events::notify();
            (StatusCode::ACCEPTED, Json(json!(task))).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("UNIQUE constraint") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn list_tasks() -> impl IntoResponse {
    match crate::db::list_tasks() {
        Ok(tasks) => (StatusCode::OK, Json(json!(tasks))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list tasks");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn get_task(Path(id): Path<String>) -> impl IntoResponse {
    match crate::db::get_task(&id) {
        Ok(task) => (StatusCode::OK, Json(json!(task))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn remove_task(
    Path(id): Path<String>,
    Query(query): Query<RemoveQuery>,
) -> impl IntoResponse {
    let result = if query.skip_provider {
        crate::db::force_delete_task(&id)
    } else {
        crate::db::stage_remove_task(&id)
    };

    match result {
        Ok(()) => {
            tracing::debug!(id = %id, skip_provider = query.skip_provider, "task removed request accepted");
            super::events::notify();
            if query.skip_provider {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::ACCEPTED.into_response()
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({"error": msg}))).into_response()
        }
    }
}

pub async fn tail_task_logs(Path(id): Path<String>) -> impl IntoResponse {
    let task = match crate::db::get_task(&id) {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return (status, Json(json!({"error": msg}))).into_response();
        }
    };

    let log_path = match crate::paths::task_log_path(&id) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    // If the task is already terminal, return the full log file.
    if task.status == "complete" || task.status == "failed" {
        let contents = std::fs::read(&log_path).unwrap_or_default();
        return (StatusCode::OK, contents).into_response();
    }

    // Stream logs via a channel.
    let (tx, rx) = mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);
    tokio::spawn(tail_log_to_channel(id, log_path, tx));
    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    (StatusCode::OK, body).into_response()
}

pub async fn tail_environment_logs(Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = crate::db::get_environment(&id) {
        let msg = e.to_string();
        let status = if msg.contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (status, Json(json!({"error": msg}))).into_response();
    }

    let log_path = match crate::paths::environment_log_path(&id) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let (tx, rx) = mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);
    tokio::spawn(tail_environment_log_to_channel(id, log_path, tx));
    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    (StatusCode::OK, body).into_response()
}

async fn tail_log_to_channel(
    task_id: String,
    log_path: std::path::PathBuf,
    tx: mpsc::Sender<Result<axum::body::Bytes, std::io::Error>>,
) {
    use std::io::{Read, Seek};

    let mut pos = 0u64;
    let mut tick: u64 = 0;

    loop {
        // Read any new bytes from the log file.
        if let Ok(metadata) = std::fs::metadata(&log_path)
            && metadata.len() > pos
            && let Ok(mut f) = std::fs::File::open(&log_path)
            && f.seek(std::io::SeekFrom::Start(pos)).is_ok()
        {
            let mut buf = vec![0u8; (metadata.len() - pos) as usize];
            if f.read_exact(&mut buf).is_ok() {
                pos = metadata.len();
                if tx.send(Ok(axum::body::Bytes::from(buf))).await.is_err() {
                    return; // client disconnected
                }
            }
        }

        // Check task status every ~1s (every 10 ticks).
        if tick.is_multiple_of(10)
            && let Ok(task) = crate::db::get_task(&task_id)
            && (task.status == "complete" || task.status == "failed")
        {
            // Drain remaining bytes.
            if let Ok(metadata) = std::fs::metadata(&log_path)
                && metadata.len() > pos
                && let Ok(mut f) = std::fs::File::open(&log_path)
                && f.seek(std::io::SeekFrom::Start(pos)).is_ok()
            {
                let mut buf = vec![0u8; (metadata.len() - pos) as usize];
                if f.read_exact(&mut buf).is_ok() {
                    let _ = tx.send(Ok(axum::body::Bytes::from(buf))).await;
                }
            }
            return; // dropping tx closes the stream
        }

        tick = tick.wrapping_add(1);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn tail_environment_log_to_channel(
    env_id: String,
    log_path: std::path::PathBuf,
    tx: mpsc::Sender<Result<axum::body::Bytes, std::io::Error>>,
) {
    use std::io::{Read, Seek};

    let mut pos = 0u64;
    let mut tick: u64 = 0;

    loop {
        if let Ok(metadata) = std::fs::metadata(&log_path)
            && metadata.len() > pos
            && let Ok(mut f) = std::fs::File::open(&log_path)
            && f.seek(std::io::SeekFrom::Start(pos)).is_ok()
        {
            let mut buf = vec![0u8; (metadata.len() - pos) as usize];
            if f.read_exact(&mut buf).is_ok() {
                pos = metadata.len();
                if tx.send(Ok(axum::body::Bytes::from(buf))).await.is_err() {
                    return;
                }
            }
        }

        if tick.is_multiple_of(10) {
            let env_gone = crate::db::get_environment(&env_id).is_err();
            if env_gone {
                if let Ok(metadata) = std::fs::metadata(&log_path)
                    && metadata.len() > pos
                    && let Ok(mut f) = std::fs::File::open(&log_path)
                    && f.seek(std::io::SeekFrom::Start(pos)).is_ok()
                {
                    let mut buf = vec![0u8; (metadata.len() - pos) as usize];
                    if f.read_exact(&mut buf).is_ok() {
                        let _ = tx.send(Ok(axum::body::Bytes::from(buf))).await;
                    }
                }
                return;
            }
        }

        tick = tick.wrapping_add(1);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

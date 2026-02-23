use axum::Json;
use axum::body::Body;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

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
}

pub async fn prepare_environment(Json(body): Json<PrepareEnvironmentRequest>) -> impl IntoResponse {
    let result = (|| {
        let project = crate::db::get_project(&body.project_id)?;
        let provider = crate::environment::get_provider(&body.provider)?;
        let env_id = crate::id::new_id();
        let metadata = provider.prepare(&project, &env_id)?;
        crate::db::create_environment(&env_id, &body.project_id, &body.provider, &metadata)
    })();

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, project_id = %env.project_id, "environment prepared");
            (StatusCode::CREATED, Json(json!(env))).into_response()
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

pub async fn update_environment(Path(id): Path<String>) -> impl IntoResponse {
    let result = (|| {
        let env = crate::db::get_environment(&id)?;
        if env.status != "pool" {
            anyhow::bail!("environment {id} is not in the pool");
        }
        let provider = crate::environment::get_provider(&env.provider)?;
        let new_metadata = provider.update(&env.metadata)?;
        crate::db::update_environment_metadata(&id, &new_metadata)?;
        crate::db::get_environment(&id)
    })();

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, "environment updated");
            (StatusCode::OK, Json(json!(env))).into_response()
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
    let result = (|| {
        let env = crate::db::get_environment(&id)?;
        let provider = crate::environment::get_provider(&env.provider)?;
        let new_metadata = provider.claim(&env.metadata)?;
        crate::db::claim_environment(&id)?;
        crate::db::update_environment_metadata(&id, &new_metadata)?;
        crate::db::get_environment(&id)
    })();

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, "environment claimed");
            (StatusCode::OK, Json(json!(env))).into_response()
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

pub async fn claim_next_environment(
    Json(body): Json<ClaimNextEnvironmentRequest>,
) -> impl IntoResponse {
    let result = (|| {
        let env = crate::db::claim_next_environment(&body.provider, &body.project_id)?;
        let provider = crate::environment::get_provider(&env.provider)?;
        let new_metadata = provider.claim(&env.metadata)?;
        crate::db::update_environment_metadata(&env.id, &new_metadata)?;
        crate::db::get_environment(&env.id)
    })();

    match result {
        Ok(env) => {
            tracing::debug!(id = %env.id, provider = %env.provider, project_id = %env.project_id, "environment claimed (next)");
            (StatusCode::OK, Json(json!(env))).into_response()
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

pub async fn remove_environment(Path(id): Path<String>) -> impl IntoResponse {
    let result = (|| {
        let env = crate::db::get_environment(&id)?;
        let provider = crate::environment::get_provider(&env.provider)?;
        provider.remove(&env.metadata)?;
        crate::db::delete_environment(&id)
    })();

    match result {
        Ok(()) => {
            tracing::debug!(id = %id, "environment removed");
            StatusCode::NO_CONTENT.into_response()
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

#[derive(serde::Deserialize)]
pub struct CreateTaskRequest {
    pub project_id: String,
    pub provider: String,
    pub env_provider: String,
    pub description: String,
}

pub async fn create_task(Json(body): Json<CreateTaskRequest>) -> impl IntoResponse {
    let result = (|| {
        let task = crate::db::create_task(&body.project_id, &body.provider, &body.description)?;
        crate::db::create_job(
            "run_task",
            &serde_json::json!({
                "task_id": task.id,
                "env_provider": body.env_provider,
            }),
        )?;
        Ok::<_, anyhow::Error>(task)
    })();

    match result {
        Ok(task) => {
            tracing::debug!(id = %task.id, provider = %task.provider, "task created");
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

pub async fn remove_task(Path(id): Path<String>) -> impl IntoResponse {
    let result = (|| {
        let task = crate::db::get_task(&id)?;
        if let Some(env_id) = &task.environment_id {
            let env = crate::db::get_environment(env_id)?;
            let provider = crate::environment::get_provider(&env.provider)?;
            provider.remove(&env.metadata)?;
        }
        crate::db::delete_task_and_environment(&id)?;
        if let Ok(log_path) = crate::paths::task_log_path(&id) {
            let _ = std::fs::remove_file(log_path);
        }
        Ok::<(), anyhow::Error>(())
    })();

    match result {
        Ok(()) => {
            tracing::debug!(id = %id, "task removed");
            StatusCode::NO_CONTENT.into_response()
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

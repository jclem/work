use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::paths;
use crate::workd::{
    CreateProjectRequest, CreateProjectResponse, CreateTaskRequest, CreateTaskResponse,
    DeleteProjectRequest, DeleteTaskRequest, ListTasksRequest, NukeResponse, ProjectInfo, TaskInfo,
};

fn daemon_error() -> CliError {
    CliError::with_hint(
        "daemon is not running",
        "start the daemon with `work daemon start`",
    )
}

fn post_json<Req, Resp>(path: &str, body: &Req) -> Result<Resp, CliError>
where
    Req: Serialize,
    Resp: for<'de> Deserialize<'de>,
{
    let socket_path = paths::socket_path(None);

    let mut stream = UnixStream::connect(&socket_path).map_err(|_| daemon_error())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| CliError::with_source("failed to set write timeout", e))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(300)))
        .map_err(|e| CliError::with_source("failed to set read timeout", e))?;

    let body_json = serde_json::to_vec(body)
        .map_err(|e| CliError::with_source("failed to serialize request", e))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_json.len()
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|_| daemon_error())?;
    stream.write_all(&body_json).map_err(|_| daemon_error())?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .map_err(|e| CliError::with_source("failed to read daemon response", e))?;

    let response = String::from_utf8_lossy(&response_bytes);

    let header_end = response
        .find("\r\n\r\n")
        .ok_or_else(|| CliError::new("malformed response from daemon"))?;

    let headers = &response[..header_end];
    let body = &response[header_end + 4..];

    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| CliError::new("empty response from daemon"))?;

    let status_code: u16 = status_line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CliError::new("malformed status line from daemon"))?;

    if status_code >= 400 {
        #[derive(Deserialize)]
        struct ErrorResponse {
            error: String,
            hint: Option<String>,
        }

        if let Ok(err) = serde_json::from_str::<ErrorResponse>(body) {
            if let Some(hint) = err.hint {
                return Err(CliError::with_hint(err.error, hint));
            } else {
                return Err(CliError::new(err.error));
            }
        }

        return Err(CliError::new(format!("daemon returned HTTP {status_code}")));
    }

    serde_json::from_str(body)
        .map_err(|e| CliError::with_source("failed to parse daemon response", e))
}

// ---------------------------------------------------------------------------
// Project operations
// ---------------------------------------------------------------------------

pub fn create_project(path: &str, name: Option<&str>) -> Result<CreateProjectResponse, CliError> {
    post_json(
        "/projects/create",
        &CreateProjectRequest {
            path: path.to_string(),
            name: name.map(|s| s.to_string()),
        },
    )
}

pub fn list_projects() -> Result<Vec<ProjectInfo>, CliError> {
    post_json("/projects/list", &serde_json::json!({}))
}

pub fn delete_project(name: &str) -> Result<(), CliError> {
    let _: serde_json::Value = post_json(
        "/projects/delete",
        &DeleteProjectRequest {
            name: name.to_string(),
        },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Task operations
// ---------------------------------------------------------------------------

pub fn create_task(
    name: Option<&str>,
    project: Option<&str>,
    cwd: &str,
) -> Result<CreateTaskResponse, CliError> {
    post_json(
        "/tasks/create",
        &CreateTaskRequest {
            name: name.map(|s| s.to_string()),
            project: project.map(|s| s.to_string()),
            cwd: cwd.to_string(),
        },
    )
}

pub fn list_tasks(
    project: Option<&str>,
    cwd: Option<&str>,
    all: bool,
) -> Result<Vec<TaskInfo>, CliError> {
    post_json(
        "/tasks/list",
        &ListTasksRequest {
            project: project.map(|s| s.to_string()),
            cwd: cwd.map(|s| s.to_string()),
            all: Some(all),
        },
    )
}

pub fn delete_task(
    name: &str,
    project: Option<&str>,
    cwd: &str,
    force: bool,
) -> Result<(), CliError> {
    let _: serde_json::Value = post_json(
        "/tasks/delete",
        &DeleteTaskRequest {
            name: name.to_string(),
            project: project.map(|s| s.to_string()),
            cwd: cwd.to_string(),
            force: Some(force),
        },
    )?;
    Ok(())
}

pub fn nuke() -> Result<NukeResponse, CliError> {
    post_json("/tasks/nuke", &serde_json::json!({}))
}

// ---------------------------------------------------------------------------
// Fire-and-forget notifications (internal daemon triggers)
// ---------------------------------------------------------------------------

/// Fire-and-forget notification to the daemon to replenish the pool.
/// Silently ignores errors (daemon may not be running).
#[allow(dead_code)]
pub fn notify_pool_replenish() {
    let socket_path = paths::socket_path(None);
    let _ = send_post(&socket_path, "/pool/replenish");
}

fn send_post(socket_path: &std::path::Path, path: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut buf = [0u8; 128];
    let _ = stream.read(&mut buf)?;
    Ok(())
}

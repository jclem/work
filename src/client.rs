use std::path::PathBuf;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

use crate::db::{Environment, Project, Task};

pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    pub fn new() -> anyhow::Result<Self> {
        let runtime_dir = crate::paths::runtime_dir()?;
        Ok(Self {
            socket_path: runtime_dir.join("work.sock"),
        })
    }

    async fn request(
        &self,
        method: hyper::Method,
        uri: &str,
        body: Option<&str>,
    ) -> anyhow::Result<(hyper::StatusCode, String)> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            anyhow::anyhow!(
                "could not connect to daemon at {}: {e}\nIs the daemon running? Start it with: work daemon start",
                self.socket_path.display(),
            )
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::spawn(conn);

        let req_body = match body {
            Some(b) => Full::new(Bytes::from(b.to_owned())),
            None => Full::new(Bytes::new()),
        };

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost");

        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }

        let req = builder.body(req_body)?;
        let res = sender.send_request(req).await?;
        let status = res.status();
        let res_bytes = res.into_body().collect().await?.to_bytes();
        let text = String::from_utf8(res_bytes.to_vec())?;

        Ok((status, text))
    }

    pub async fn list_projects(&self) -> anyhow::Result<Vec<Project>> {
        let (status, body) = self.request(hyper::Method::GET, "/projects", None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn create_project(&self, name: &str, path: &str) -> anyhow::Result<()> {
        let payload = serde_json::json!({"name": name, "path": path}).to_string();
        let (status, body) = self
            .request(hyper::Method::POST, "/projects", Some(&payload))
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(())
    }

    pub async fn delete_project(&self, name: &str) -> anyhow::Result<()> {
        let uri = format!("/projects/{name}");
        let (status, body) = self.request(hyper::Method::DELETE, &uri, None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(())
    }

    pub async fn reset_database(&self) -> anyhow::Result<()> {
        let (status, body) = self
            .request(hyper::Method::POST, "/reset-database", None)
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(())
    }

    pub async fn prepare_environment(
        &self,
        project_id: &str,
        provider: &str,
    ) -> anyhow::Result<Environment> {
        let payload =
            serde_json::json!({"project_id": project_id, "provider": provider}).to_string();
        let (status, body) = self
            .request(hyper::Method::POST, "/environments", Some(&payload))
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn list_environments(&self) -> anyhow::Result<Vec<Environment>> {
        let (status, body) = self
            .request(hyper::Method::GET, "/environments", None)
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn update_environment(&self, id: &str) -> anyhow::Result<Environment> {
        let uri = format!("/environments/{id}/update");
        let (status, body) = self.request(hyper::Method::POST, &uri, None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn claim_environment(&self, id: &str) -> anyhow::Result<Environment> {
        let uri = format!("/environments/{id}/claim");
        let (status, body) = self.request(hyper::Method::POST, &uri, None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn claim_next_environment(
        &self,
        provider: &str,
        project_id: &str,
    ) -> anyhow::Result<Environment> {
        let payload =
            serde_json::json!({"provider": provider, "project_id": project_id}).to_string();
        let (status, body) = self
            .request(hyper::Method::POST, "/environments/claim", Some(&payload))
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn remove_environment(&self, id: &str) -> anyhow::Result<()> {
        let uri = format!("/environments/{id}");
        let (status, body) = self.request(hyper::Method::DELETE, &uri, None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(())
    }

    pub async fn create_task(
        &self,
        project_id: &str,
        provider: &str,
        env_provider: &str,
        description: &str,
    ) -> anyhow::Result<Task> {
        let payload = serde_json::json!({
            "project_id": project_id,
            "provider": provider,
            "env_provider": env_provider,
            "description": description,
        })
        .to_string();
        let (status, body) = self
            .request(hyper::Method::POST, "/tasks", Some(&payload))
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn list_tasks(&self) -> anyhow::Result<Vec<Task>> {
        let (status, body) = self.request(hyper::Method::GET, "/tasks", None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn remove_task(&self, id: &str) -> anyhow::Result<()> {
        let uri = format!("/tasks/{id}");
        let (status, body) = self.request(hyper::Method::DELETE, &uri, None).await?;
        if !status.is_success() {
            anyhow::bail!("{}", extract_error(&body));
        }
        Ok(())
    }

    pub async fn tail_task_logs(
        &self,
        task_id: &str,
        mut on_chunk: impl FnMut(&[u8]),
    ) -> anyhow::Result<()> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            anyhow::anyhow!(
                "could not connect to daemon at {}: {e}\nIs the daemon running? Start it with: work daemon start",
                self.socket_path.display(),
            )
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::spawn(conn);

        let req = hyper::Request::builder()
            .method(hyper::Method::GET)
            .uri(format!("/tasks/{task_id}/logs"))
            .header("host", "localhost")
            .body(Full::new(Bytes::new()))?;

        let res = sender.send_request(req).await?;
        let status = res.status();

        if !status.is_success() {
            let body_bytes = res.into_body().collect().await?.to_bytes();
            let text = String::from_utf8(body_bytes.to_vec())?;
            anyhow::bail!("{}", extract_error(&text));
        }

        let mut body = res.into_body();
        while let Some(frame) = body.frame().await {
            let frame = frame?;
            if let Some(data) = frame.data_ref() {
                on_chunk(data);
            }
        }

        Ok(())
    }
}

fn extract_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.as_str().map(String::from))
        .unwrap_or_else(|| body.to_string())
}

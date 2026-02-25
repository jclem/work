use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::json;

use crate::db::Project;

use super::{EnvironmentProvider, RunSpec};

pub struct ScriptProvider {
    pub command: String,
}

impl ScriptProvider {
    fn call(
        &self,
        action: &str,
        input: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        let input_bytes = serde_json::to_vec(input)?;

        let mut command = Command::new(&self.command);
        command
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if let Some(path) = log_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let stderr_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            command.stderr(Stdio::from(stderr_file));
        } else {
            command.stderr(Stdio::inherit());
        }

        let mut child = command.spawn()?;

        use std::io::Write;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?
            .write_all(&input_bytes)?;

        let output = child.wait_with_output()?;

        if !output.status.success() {
            anyhow::bail!(
                "{} {} failed with status {}",
                self.command,
                action,
                output.status
            );
        }

        let stdout = String::from_utf8(output.stdout)?;
        Ok(serde_json::from_str(stdout.trim())?)
    }
}

impl EnvironmentProvider for ScriptProvider {
    fn prepare(
        &self,
        project: &Project,
        env_id: &str,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call(
            "prepare",
            &json!({
                "project_name": project.name,
                "project_path": project.path,
                "env_id": env_id,
            }),
            log_path,
        )
    }

    fn update(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call("update", metadata, log_path)
    }

    fn claim(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call("claim", metadata, log_path)
    }

    fn remove(&self, metadata: &serde_json::Value, log_path: Option<&Path>) -> anyhow::Result<()> {
        let input = json!({ "metadata": metadata });
        let input_bytes = serde_json::to_vec(&input)?;

        let mut command = Command::new(&self.command);
        command.arg("remove").stdin(Stdio::piped());
        if let Some(path) = log_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let stdout_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let stderr_file = stdout_file.try_clone()?;
            command.stdout(Stdio::from(stdout_file));
            command.stderr(Stdio::from(stderr_file));
        } else {
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        }

        let mut child = command.spawn()?;

        use std::io::Write;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?
            .write_all(&input_bytes)?;

        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("{} remove failed with status {}", self.command, status);
        }

        Ok(())
    }

    fn run(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec> {
        let input = json!({
            "metadata": metadata,
            "command": command,
            "args": args,
        });

        Ok(RunSpec {
            program: self.command.clone(),
            args: vec!["run".to_string()],
            cwd: None,
            stdin_data: Some(serde_json::to_vec(&input)?),
        })
    }
}

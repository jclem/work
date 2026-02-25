use std::process::{Command, Stdio};

use serde_json::json;

use crate::db::Project;

use super::{EnvironmentProvider, RunSpec};

pub struct ScriptProvider {
    pub command: String,
}

impl ScriptProvider {
    fn call(&self, action: &str, input: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let input_bytes = serde_json::to_vec(input)?;

        let mut child = Command::new(&self.command)
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

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
    fn prepare(&self, project: &Project, env_id: &str) -> anyhow::Result<serde_json::Value> {
        self.call(
            "prepare",
            &json!({
                "project_name": project.name,
                "project_path": project.path,
                "env_id": env_id,
            }),
        )
    }

    fn update(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.call("update", metadata)
    }

    fn claim(&self, metadata: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.call("claim", metadata)
    }

    fn remove(&self, metadata: &serde_json::Value) -> anyhow::Result<()> {
        let input = json!({ "metadata": metadata });
        let input_bytes = serde_json::to_vec(&input)?;

        let mut child = Command::new(&self.command)
            .arg("remove")
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

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

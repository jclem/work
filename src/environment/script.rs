use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::json;

use crate::db::Project;

use super::{EnvironmentProvider, ProviderExecCommand, RunSpec};

pub struct ScriptProvider {
    pub path: String,
}

impl ScriptProvider {
    fn call(
        &self,
        action: &str,
        input: &serde_json::Value,
        log_path: Option<&Path>,
        quiet_stderr: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let input_bytes = serde_json::to_vec(input)?;

        let mut command = Command::new(&self.path);
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
        } else if quiet_stderr {
            command.stderr(Stdio::null());
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
                self.path,
                action,
                output.status
            );
        }

        let stdout = String::from_utf8(output.stdout)?;
        Ok(serde_json::from_str(stdout.trim())?)
    }

    fn parse_exec_commands(value: serde_json::Value) -> anyhow::Result<Vec<ProviderExecCommand>> {
        if let Some(array) = value.as_array() {
            let mut commands = Vec::with_capacity(array.len());
            for item in array {
                if let Some(name) = item.as_str() {
                    commands.push(ProviderExecCommand {
                        name: name.to_string(),
                        help: None,
                    });
                    continue;
                }

                let obj = item.as_object().ok_or_else(|| {
                    anyhow::anyhow!("commands entries must be strings or objects")
                })?;
                let name = obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("commands object entries must include name"))?;
                let help = obj
                    .get("help")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("description").and_then(|v| v.as_str()))
                    .map(str::to_string);
                commands.push(ProviderExecCommand {
                    name: name.to_string(),
                    help,
                });
            }
            return Ok(commands);
        }

        if let Some(obj) = value.as_object() {
            let mut commands = Vec::with_capacity(obj.len());
            for (name, help_value) in obj {
                commands.push(ProviderExecCommand {
                    name: name.to_string(),
                    help: help_value.as_str().map(str::to_string),
                });
            }
            return Ok(commands);
        }

        anyhow::bail!("commands output must be an array or object");
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
            false,
        )
    }

    fn update(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call("update", metadata, log_path, false)
    }

    fn claim(
        &self,
        metadata: &serde_json::Value,
        log_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call("claim", metadata, log_path, false)
    }

    fn remove(&self, metadata: &serde_json::Value, log_path: Option<&Path>) -> anyhow::Result<()> {
        let input = json!({ "metadata": metadata });
        let input_bytes = serde_json::to_vec(&input)?;

        let mut command = Command::new(&self.path);
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
            anyhow::bail!("{} remove failed with status {}", self.path, status);
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
            program: self.path.clone(),
            args: vec!["run".to_string()],
            cwd: None,
            stdin_data: Some(serde_json::to_vec(&input)?),
            env: Vec::new(),
        })
    }

    fn exec_commands(
        &self,
        metadata: &serde_json::Value,
    ) -> anyhow::Result<Vec<ProviderExecCommand>> {
        let output = self.call("commands", &json!({ "metadata": metadata }), None, true)?;
        Self::parse_exec_commands(output)
    }

    fn exec(
        &self,
        metadata: &serde_json::Value,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<RunSpec> {
        let mut run_args = Vec::with_capacity(args.len() + 2);
        run_args.push("exec".to_string());
        run_args.push(command.to_string());
        run_args.extend(args.iter().cloned());

        Ok(RunSpec {
            program: self.path.clone(),
            args: run_args,
            cwd: None,
            stdin_data: None,
            env: vec![(
                "WORK_ENV_METADATA".to_string(),
                serde_json::to_string(metadata)?,
            )],
        })
    }
}

use tokio::sync::watch;

use crate::db;

pub async fn run(mut shutdown: watch::Receiver<bool>) {
    tracing::info!("job processor started");

    loop {
        match db::claim_pending_jobs() {
            Ok(jobs) => {
                for job in jobs {
                    tokio::spawn(async move {
                        process_job(job).await;
                    });
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to claim pending jobs");
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            _ = shutdown.changed() => {
                tracing::info!("job processor shutting down");
                break;
            }
        }
    }
}

async fn process_job(job: db::Job) {
    tracing::info!(id = %job.id, job_type = %job.job_type, "processing job");

    let result = match job.job_type.as_str() {
        "prepare_environment" => prepare_environment(&job).await,
        "remove_environment" => remove_environment(&job).await,
        "run_task" => run_task(&job).await,
        other => {
            tracing::error!(job_type = %other, "unknown job type");
            Err(anyhow::anyhow!("unknown job type: {other}"))
        }
    };

    let status = if result.is_ok() { "complete" } else { "failed" };

    if let Err(e) = &result {
        tracing::error!(id = %job.id, error = %e, "job failed");
        if job.job_type == "run_task" {
            if let Some(task_id) = job.payload["task_id"].as_str() {
                if let Err(update_err) = db::update_task_status(task_id, "failed") {
                    tracing::error!(
                        id = %job.id,
                        task_id = %task_id,
                        error = %update_err,
                        "failed to mark task as failed after run_task job failure"
                    );
                } else {
                    super::events::notify();
                }
            } else {
                tracing::error!(
                    id = %job.id,
                    "run_task job failed but payload did not include task_id"
                );
            }
        }
    }

    if let Err(e) = db::update_job_status(&job.id, status) {
        tracing::error!(id = %job.id, error = %e, "failed to update job status");
    }
}

async fn prepare_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();
    let task_id = job.payload["task_id"].as_str().map(|s| s.to_string());

    let env = db::get_environment(&env_id)?;
    let project = db::get_project(&env.project_id)?;
    let provider_name = env.provider.clone();

    tracing::info!(env_id = %env_id, provider = %provider_name, "preparing environment");

    let eid = env_id.clone();
    let metadata_result = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.prepare(&project, &eid)
    })
    .await?;

    let metadata = match metadata_result {
        Ok(metadata) => metadata,
        Err(e) => {
            if let Err(update_err) = db::fail_preparing_environment(&env_id) {
                tracing::error!(
                    env_id = %env_id,
                    error = %update_err,
                    "failed to mark preparing environment as failed"
                );
            }
            if let Some(task_id) = task_id.as_deref() {
                if let Err(update_err) = db::update_task_status(task_id, "failed") {
                    tracing::error!(
                        task_id = %task_id,
                        error = %update_err,
                        "failed to mark task as failed after environment prepare failure"
                    );
                }
            }
            super::events::notify();
            return Err(e);
        }
    };

    db::finish_preparing_environment(&env_id, &metadata)?;
    if let Some(task_id) = task_id.as_deref() {
        if let Err(e) = db::create_job(
            "run_task",
            &serde_json::json!({
                "task_id": task_id,
                "env_id": env_id,
            }),
        ) {
            let _ = db::update_environment_status(&env_id, "failed");
            let _ = db::update_task_status(task_id, "failed");
            super::events::notify();
            return Err(e);
        }
    }
    super::events::notify();

    tracing::info!(env_id = %env_id, "environment prepared");

    Ok(())
}

async fn remove_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();

    let env = db::get_environment(&env_id)?;
    let provider_name = env.provider.clone();
    let metadata = env.metadata.clone();

    tracing::info!(env_id = %env_id, provider = %provider_name, "removing environment");

    tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.remove(&metadata)
    })
    .await??;

    db::delete_environment(&env_id)?;
    super::events::notify();

    tracing::info!(env_id = %env_id, "environment removed");

    Ok(())
}

async fn run_task(job: &db::Job) -> anyhow::Result<()> {
    let config = crate::config::load()?;

    let task_id = job.payload["task_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing task_id"))?;
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?;

    let task = db::get_task(task_id)?;
    let env = db::get_environment(env_id)?;
    db::claim_environment(&env.id)?;
    let provider_name = env.provider.clone();
    let meta = env.metadata.clone();
    let new_metadata_result = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.claim(&meta)
    })
    .await?;
    let env = match new_metadata_result {
        Ok(metadata) => {
            db::update_environment_metadata(&env.id, &metadata)?;
            db::get_environment(&env.id)?
        }
        Err(e) => {
            db::update_environment_status(&env.id, "failed")?;
            super::events::notify();
            return Err(e);
        }
    };

    db::start_task(task_id)?;
    super::events::notify();

    let task_provider_config = config.get_task_provider(&task.provider)?;
    let crate::config::TaskProviderConfig::Command { command: cmd, args } = task_provider_config;

    let resolved_args: Vec<String> = args
        .iter()
        .map(|a| a.replace("{task_description}", &task.description))
        .collect();

    let run_spec = {
        let provider_name = env.provider.clone();
        let meta = env.metadata.clone();
        let cmd = cmd.to_string();
        let args = resolved_args.clone();
        tokio::task::spawn_blocking(move || {
            let provider = crate::environment::get_provider(&provider_name)?;
            provider.run(&meta, &cmd, &args)
        })
        .await??
    };

    let log_path = crate::paths::task_log_path(task_id)?;
    std::fs::create_dir_all(log_path.parent().unwrap())?;
    let log_file = std::fs::File::create(&log_path)?;
    let stderr_file = log_file.try_clone()?;

    tracing::info!(task_id = %task_id, command = %run_spec.program, log = %log_path.display(), "running task command");

    let mut command = tokio::process::Command::new(&run_spec.program);
    command.args(&run_spec.args);

    if let Some(cwd) = &run_spec.cwd {
        command.current_dir(cwd);
    }

    if run_spec.stdin_data.is_some() {
        command.stdin(std::process::Stdio::piped());
    } else {
        command.stdin(std::process::Stdio::null());
    }

    command.stdout(std::process::Stdio::from(log_file));
    command.stderr(std::process::Stdio::from(stderr_file));

    let mut child = command.spawn()?;

    if let Some(data) = run_spec.stdin_data {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&data).await?;
        }
    }

    let status = child.wait().await?;

    let task_status = if status.success() {
        "complete"
    } else {
        "failed"
    };

    db::update_task_status(task_id, task_status)?;
    super::events::notify();

    tracing::info!(task_id = %task_id, status = %task_status, "task finished");

    Ok(())
}

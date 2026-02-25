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

    let env = db::get_environment(&env_id)?;
    let project = db::get_project(&env.project_id)?;
    let provider_name = env.provider.clone();

    tracing::info!(env_id = %env_id, provider = %provider_name, "preparing environment");

    let eid = env_id.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.prepare(&project, &eid)
    })
    .await??;

    db::finish_preparing_environment(&env_id, &metadata)?;
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
    let env_provider = job.payload["env_provider"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_provider"))?;

    let task = db::get_task(task_id)?;
    let project = db::get_project(&task.project_id)?;

    let env = match db::claim_next_environment(env_provider, &project.id) {
        Ok(env) => {
            let provider_name = env.provider.clone();
            let meta = env.metadata.clone();
            let new_metadata = tokio::task::spawn_blocking(move || {
                let provider = crate::environment::get_provider(&provider_name)?;
                provider.claim(&meta)
            })
            .await??;
            db::update_environment_metadata(&env.id, &new_metadata)?;
            db::get_environment(&env.id)?
        }
        Err(_) => {
            let env_provider = env_provider.to_string();
            let env_id = crate::id::new_id();
            let project_clone = project.clone();
            let eid = env_id.clone();
            let ep = env_provider.clone();
            let metadata = tokio::task::spawn_blocking(move || {
                let provider = crate::environment::get_provider(&ep)?;
                provider.prepare(&project_clone, &eid)
            })
            .await??;
            let env = db::create_environment(&env_id, &project.id, &env_provider, &metadata)?;
            db::claim_environment(&env.id)?;
            let provider_name = env.provider.clone();
            let meta = env.metadata.clone();
            let new_metadata = tokio::task::spawn_blocking(move || {
                let provider = crate::environment::get_provider(&provider_name)?;
                provider.claim(&meta)
            })
            .await??;
            db::update_environment_metadata(&env.id, &new_metadata)?;
            db::get_environment(&env.id)?
        }
    };

    db::start_task(task_id, &env.id)?;
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

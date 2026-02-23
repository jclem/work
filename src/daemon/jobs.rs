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
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
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
            let provider = crate::environment::get_provider(&env.provider)?;
            let new_metadata = provider.claim(&env.metadata)?;
            db::update_environment_metadata(&env.id, &new_metadata)?;
            db::get_environment(&env.id)?
        }
        Err(_) => {
            let provider = crate::environment::get_provider(env_provider)?;
            let env_id = crate::id::new_id();
            let metadata = provider.prepare(&project, &env_id)?;
            let env = db::create_environment(&env_id, &project.id, env_provider, &metadata)?;
            db::claim_environment(&env.id)?;
            let new_metadata = provider.claim(&env.metadata)?;
            db::update_environment_metadata(&env.id, &new_metadata)?;
            db::get_environment(&env.id)?
        }
    };

    db::start_task(task_id, &env.id)?;

    let task_provider_config = config.get_task_provider(&task.provider)?;
    let crate::config::TaskProviderConfig::Command { command: cmd, args } = task_provider_config;

    let worktree_path = env.metadata["worktree_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("environment has no worktree_path"))?;

    let resolved_args: Vec<String> = args
        .iter()
        .map(|a| a.replace("{task_description}", &task.description))
        .collect();

    let log_path = crate::paths::task_log_path(task_id)?;
    std::fs::create_dir_all(log_path.parent().unwrap())?;
    let log_file = std::fs::File::create(&log_path)?;
    let stderr_file = log_file.try_clone()?;

    tracing::info!(task_id = %task_id, command = %cmd, cwd = %worktree_path, log = %log_path.display(), "running task command");

    let status = tokio::process::Command::new(cmd)
        .args(&resolved_args)
        .current_dir(worktree_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr_file))
        .status()
        .await?;

    let task_status = if status.success() {
        "complete"
    } else {
        "failed"
    };

    db::update_task_status(task_id, task_status)?;

    tracing::info!(task_id = %task_id, status = %task_status, "task finished");

    Ok(())
}

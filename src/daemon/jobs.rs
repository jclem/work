use std::io::Write;
use std::sync::Arc;

use tokio::sync::{Semaphore, oneshot, watch};

use crate::db;

const POLL_INTERVAL_MS: u64 = 100;
const CLAIM_BATCH_LIMIT: usize = 8;
const MAX_CONCURRENT_JOBS: usize = 8;
const JOB_LEASE_SECONDS: i64 = 30;
const JOB_LEASE_RENEW_INTERVAL_SECONDS: u64 = 10;
const RETRY_LIMIT: i64 = 2;

fn env_id_for_lifecycle_job(job: &db::Job) -> Option<&str> {
    match job.job_type.as_str() {
        "prepare_environment"
        | "update_environment"
        | "claim_environment"
        | "remove_environment"
        | "remove_task" => job.payload["env_id"].as_str(),
        _ => None,
    }
}

fn append_environment_lifecycle_log(env_id: &str, line: &str) {
    let log_path = match crate::paths::environment_log_path(env_id) {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(env_id = %env_id, error = %e, "failed to build environment log path");
            return;
        }
    };

    if let Some(parent) = log_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(env_id = %env_id, error = %e, "failed to create environment log directory");
        return;
    }

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => file,
        Err(e) => {
            tracing::warn!(env_id = %env_id, error = %e, path = %log_path.display(), "failed to open environment lifecycle log");
            return;
        }
    };

    let ts = chrono::Utc::now().to_rfc3339();
    if let Err(e) = writeln!(file, "{ts} {line}") {
        tracing::warn!(env_id = %env_id, error = %e, path = %log_path.display(), "failed to append environment lifecycle log");
    }
}

fn environment_log_path(env_id: &str) -> Option<std::path::PathBuf> {
    crate::paths::environment_log_path(env_id).ok()
}

pub async fn run(mut shutdown: watch::Receiver<bool>) {
    tracing::info!("job processor started");
    let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS));

    loop {
        let available = permits.available_permits();
        if available > 0 {
            let claim_limit = available.min(CLAIM_BATCH_LIMIT);
            match db::claim_pending_jobs(claim_limit, JOB_LEASE_SECONDS) {
                Ok(jobs) => {
                    for job in jobs {
                        let permits = permits.clone();
                        tokio::spawn(async move {
                            let _permit = match permits.acquire_owned().await {
                                Ok(permit) => permit,
                                Err(_) => return,
                            };
                            process_job(job).await;
                        });
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to claim pending jobs");
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)) => {}
            _ = shutdown.changed() => {
                tracing::info!("job processor shutting down");
                break;
            }
        }
    }
}

fn retry_delay_seconds(attempt: i64) -> i64 {
    let exp = (attempt.max(1) as u32).min(5);
    (2_i64.pow(exp)).min(60)
}

fn spawn_job_lease_heartbeat(job_id: String) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(JOB_LEASE_RENEW_INTERVAL_SECONDS);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    match db::refresh_job_lease(&job_id, JOB_LEASE_SECONDS) {
                        Ok(true) => {}
                        Ok(false) => return,
                        Err(e) => {
                            tracing::warn!(id = %job_id, error = %e, "failed to refresh job lease");
                        }
                    }
                }
                _ = &mut stop_rx => {
                    return;
                }
            }
        }
    });
    (stop_tx, handle)
}

async fn process_job(job: db::Job) {
    let lifecycle_env_id = env_id_for_lifecycle_job(&job).map(str::to_string);
    let attempt_number = job.attempt + 1;

    tracing::info!(
        id = %job.id,
        job_type = %job.job_type,
        attempt = job.attempt,
        "processing job"
    );
    if let Some(env_id) = lifecycle_env_id.as_deref() {
        append_environment_lifecycle_log(
            env_id,
            &format!(
                "job={} attempt={} phase=start",
                job.job_type, attempt_number
            ),
        );
    }

    let (lease_stop_tx, lease_handle) = spawn_job_lease_heartbeat(job.id.clone());

    let result = match job.job_type.as_str() {
        "prepare_environment" => prepare_environment(&job).await,
        "update_environment" => update_environment(&job).await,
        "claim_environment" => claim_environment(&job).await,
        "remove_environment" => remove_environment(&job).await,
        "remove_task" => remove_task(&job).await,
        "run_task" => run_task(&job).await,
        other => Err(anyhow::anyhow!("unknown job type: {other}")),
    };
    let _ = lease_stop_tx.send(());
    let _ = lease_handle.await;

    match result {
        Ok(()) => {
            if let Err(e) = db::mark_job_complete(&job.id) {
                tracing::error!(id = %job.id, error = %e, "failed to mark job complete");
            }
            if let Some(env_id) = lifecycle_env_id.as_deref() {
                append_environment_lifecycle_log(
                    env_id,
                    &format!(
                        "job={} attempt={} phase=complete",
                        job.job_type, attempt_number
                    ),
                );
            }
        }
        Err(e) => {
            tracing::error!(id = %job.id, error = %e, "job failed");
            let error_message = e.to_string();

            let can_retry = job.attempt < RETRY_LIMIT;
            if can_retry {
                let delay = retry_delay_seconds(job.attempt);
                if let Some(env_id) = lifecycle_env_id.as_deref() {
                    append_environment_lifecycle_log(
                        env_id,
                        &format!(
                            "job={} attempt={} phase=retrying delay_seconds={} error={}",
                            job.job_type, attempt_number, delay, error_message
                        ),
                    );
                }

                if let Err(requeue_err) = db::requeue_job(&job.id, &error_message, delay) {
                    tracing::error!(
                        id = %job.id,
                        error = %requeue_err,
                        "failed to requeue failed job"
                    );
                    if let Some(env_id) = lifecycle_env_id.as_deref() {
                        append_environment_lifecycle_log(
                            env_id,
                            &format!(
                                "job={} attempt={} phase=retry_requeue_failed error={}",
                                job.job_type, attempt_number, requeue_err
                            ),
                        );
                    }
                    let _ = db::mark_job_failed(&job.id, &error_message);
                    apply_terminal_failure_side_effects(&job);
                }
                return;
            }

            if let Some(env_id) = lifecycle_env_id.as_deref() {
                append_environment_lifecycle_log(
                    env_id,
                    &format!(
                        "job={} attempt={} phase=failed error={}",
                        job.job_type, attempt_number, error_message
                    ),
                );
            }

            if let Err(mark_err) = db::mark_job_failed(&job.id, &error_message) {
                tracing::error!(id = %job.id, error = %mark_err, "failed to mark job failed");
            }
            apply_terminal_failure_side_effects(&job);
        }
    }
}

fn apply_terminal_failure_side_effects(job: &db::Job) {
    match job.job_type.as_str() {
        "prepare_environment" => {
            if let Some(env_id) = job.payload["env_id"].as_str() {
                let _ = db::update_environment_status(env_id, "failed");
            }
            if let Some(task_id) = job.payload["task_id"].as_str() {
                let _ = db::update_task_status(task_id, "failed");
            }
            super::events::notify();
        }
        "run_task" => {
            if let Some(task_id) = job.payload["task_id"].as_str() {
                let _ = db::update_task_status(task_id, "failed");
            }
            if let Some(env_id) = job.payload["env_id"].as_str() {
                let _ = db::update_environment_status(env_id, "failed");
            }
            super::events::notify();
        }
        "claim_environment" => {
            if let Some(env_id) = job.payload["env_id"].as_str() {
                let _ = db::update_environment_status(env_id, "failed");
            }
            if let Some(task_id) = job.payload["task_id"].as_str() {
                let _ = db::update_task_status(task_id, "failed");
            }
            super::events::notify();
        }
        "update_environment" | "remove_environment" => {
            if let Some(env_id) = job.payload["env_id"].as_str() {
                let _ = db::update_environment_status(env_id, "failed");
            }
            super::events::notify();
        }
        "remove_task" => {
            if let Some(env_id) = job.payload["env_id"].as_str() {
                let _ = db::update_environment_status(env_id, "failed");
            }
            super::events::notify();
        }
        _ => {}
    }
}

async fn prepare_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();
    let task_id = job.payload["task_id"].as_str().map(|s| s.to_string());
    let claim_after_prepare = job.payload["claim_after_prepare"]
        .as_bool()
        .unwrap_or(false);

    let env = db::get_environment(&env_id)?;
    if env.status == "pool" || env.status == "in_use" {
        if let Some(task_id) = task_id.as_deref() {
            let dedupe = format!("run_task:task:{task_id}");
            db::create_job_with_dedupe(
                "run_task",
                &serde_json::json!({
                    "task_id": task_id,
                    "env_id": env_id,
                }),
                Some(&dedupe),
            )?;
            super::events::notify();
        }
        return Ok(());
    }

    if env.status != "preparing" {
        anyhow::bail!(
            "environment {env_id} has unexpected status {} while preparing",
            env.status
        );
    }

    let project = db::get_project(&env.project_id)?;
    let provider_name = env.provider.clone();
    let log_path = environment_log_path(&env_id);

    tracing::info!(env_id = %env_id, provider = %provider_name, "preparing environment");

    let eid = env_id.clone();
    let prepared_metadata = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.prepare(&project, &eid, log_path.as_deref())
    })
    .await??;

    let should_claim = claim_after_prepare || task_id.is_some();

    let final_metadata = if should_claim {
        let provider_name = env.provider.clone();
        let meta = prepared_metadata.clone();
        let log_path = environment_log_path(&env_id);
        tokio::task::spawn_blocking(move || {
            let provider = crate::environment::get_provider(&provider_name)?;
            provider.claim(&meta, log_path.as_deref())
        })
        .await??
    } else {
        prepared_metadata
    };

    let final_status = if should_claim { "in_use" } else { "pool" };
    db::complete_preparing_environment(&env_id, final_status, &final_metadata)?;

    if let Some(task_id) = task_id.as_deref() {
        let dedupe = format!("run_task:task:{task_id}");
        db::create_job_with_dedupe(
            "run_task",
            &serde_json::json!({
                "task_id": task_id,
                "env_id": env_id,
            }),
            Some(&dedupe),
        )?;
    }

    super::events::notify();
    tracing::info!(env_id = %env_id, status = %final_status, "environment prepared");

    Ok(())
}

async fn update_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();
    let env = match db::get_environment(&env_id) {
        Ok(env) => env,
        Err(_) => return Ok(()),
    };
    if env.status != "pool" {
        return Ok(());
    }

    let provider_name = env.provider.clone();
    let metadata = env.metadata.clone();
    let log_path = environment_log_path(&env_id);
    let new_metadata = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.update(&metadata, log_path.as_deref())
    })
    .await??;

    db::update_environment_metadata(&env_id, &new_metadata)?;
    super::events::notify();
    Ok(())
}

async fn claim_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();
    let task_id = job.payload["task_id"].as_str().map(|id| id.to_string());

    let env = match db::get_environment(&env_id) {
        Ok(env) => env,
        Err(_) => return Ok(()),
    };
    if env.status != "in_use" {
        if task_id.is_some() {
            anyhow::bail!("environment {env_id} is not in use");
        }
        return Ok(());
    }

    let provider_name = env.provider.clone();
    let metadata = env.metadata.clone();
    let log_path = environment_log_path(&env_id);
    let new_metadata = tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.claim(&metadata, log_path.as_deref())
    })
    .await??;

    db::update_environment_metadata(&env_id, &new_metadata)?;

    if let Some(task_id) = task_id.as_deref() {
        let task = db::get_task(task_id)?;
        if task.status == "pending" {
            let dedupe = format!("run_task:task:{task_id}");
            db::create_job_with_dedupe(
                "run_task",
                &serde_json::json!({
                    "task_id": task_id,
                    "env_id": env_id,
                }),
                Some(&dedupe),
            )?;
        }
    }

    super::events::notify();
    Ok(())
}

async fn remove_environment(job: &db::Job) -> anyhow::Result<()> {
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();

    let env = match db::get_environment(&env_id) {
        Ok(env) => env,
        Err(_) => return Ok(()),
    };
    let provider_name = env.provider.clone();
    let metadata = env.metadata.clone();
    let log_path = environment_log_path(&env_id);

    tracing::info!(env_id = %env_id, provider = %provider_name, "removing environment");

    tokio::task::spawn_blocking(move || {
        let provider = crate::environment::get_provider(&provider_name)?;
        provider.remove(&metadata, log_path.as_deref())
    })
    .await??;

    db::delete_environment(&env_id)?;
    super::events::notify();

    tracing::info!(env_id = %env_id, "environment removed");

    Ok(())
}

async fn remove_task(job: &db::Job) -> anyhow::Result<()> {
    let task_id = job.payload["task_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing task_id"))?
        .to_string();
    let env_id = job.payload["env_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job payload missing env_id"))?
        .to_string();

    if let Ok(env) = db::get_environment(&env_id) {
        let provider_name = env.provider.clone();
        let metadata = env.metadata.clone();
        let log_path = environment_log_path(&env_id);
        tokio::task::spawn_blocking(move || {
            let provider = crate::environment::get_provider(&provider_name)?;
            provider.remove(&metadata, log_path.as_deref())
        })
        .await??;
    }

    db::delete_task_and_environment(&task_id, &env_id)?;
    if let Ok(log_path) = crate::paths::task_log_path(&task_id) {
        let _ = std::fs::remove_file(log_path);
    }
    super::events::notify();
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
    if task.status == "complete" || task.status == "failed" {
        return Ok(());
    }
    if task.status == "started" {
        anyhow::bail!("task {task_id} is already started");
    }

    let env = db::get_environment(env_id)?;
    if env.status != "in_use" {
        anyhow::bail!("environment {env_id} is not in use");
    }

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

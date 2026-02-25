mod migrations;

use rusqlite::{Connection, OptionalExtension, Transaction};

fn db_path() -> Result<std::path::PathBuf, anyhow::Error> {
    Ok(crate::paths::data_dir()?.join("database.sqlite3"))
}

fn connect() -> Result<Connection, anyhow::Error> {
    let mut conn = Connection::open(db_path()?)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrations::run(&mut conn)?;
    Ok(conn)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn insert_job_tx(
    tx: &Transaction<'_>,
    job_type: &str,
    payload: &serde_json::Value,
    dedupe_key: Option<&str>,
) -> Result<String, anyhow::Error> {
    let payload_str = serde_json::to_string(payload)?;
    let now = now_rfc3339();

    if let Some(dedupe_key) = dedupe_key {
        if let Some(existing_id) = tx
            .query_row(
                "SELECT id FROM jobs WHERE dedupe_key = ?1 AND status IN ('pending', 'running') LIMIT 1",
                rusqlite::params![dedupe_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing_id);
        }

        // Terminal jobs should not block replaying the same logical operation.
        tx.execute(
            "UPDATE jobs SET dedupe_key = NULL WHERE dedupe_key = ?1 AND status IN ('complete', 'failed')",
            rusqlite::params![dedupe_key],
        )?;
    }

    let id = crate::id::new_id();
    let insert_result = tx.execute(
        "INSERT INTO jobs (id, type, payload, status, created_at, updated_at, dedupe_key, attempt, not_before, lease_expires_at, last_error) VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, 0, NULL, NULL, NULL)",
        rusqlite::params![&id, job_type, payload_str, &now, &now, dedupe_key],
    );

    if let Err(err) = insert_result {
        if dedupe_key.is_some()
            && err
                .to_string()
                .contains("UNIQUE constraint failed: jobs.dedupe_key")
            && let Some(existing_id) = tx
                .query_row(
                    "SELECT id FROM jobs WHERE dedupe_key = ?1 AND status IN ('pending', 'running') LIMIT 1",
                    rusqlite::params![dedupe_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
        {
            return Ok(existing_id);
        }

        return Err(err.into());
    }

    Ok(id)
}

pub fn initialize() -> Result<(), anyhow::Error> {
    connect()?;
    Ok(())
}

pub fn reset() -> Result<(), anyhow::Error> {
    let path = db_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
        tracing::debug!("removed database file");
    }
    initialize()?;
    Ok(())
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn list_projects() -> Result<Vec<Project>, anyhow::Error> {
    let conn = connect()?;
    let mut stmt =
        conn.prepare("SELECT id, name, path, created_at, updated_at FROM projects ORDER BY name")?;
    let projects = stmt
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(projects)
}

pub fn delete_project(name: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let rows = conn.execute(
        "DELETE FROM projects WHERE name = ?1",
        rusqlite::params![name],
    )?;
    if rows == 0 {
        anyhow::bail!("project not found: {name}");
    }
    Ok(())
}

pub fn create_project(name: &str, path: &std::path::Path) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let id = crate::id::new_id();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO projects (id, name, path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, name, path.to_string_lossy(), &now, &now],
    )?;
    Ok(())
}

pub fn get_project(id: &str) -> Result<Project, anyhow::Error> {
    let conn = connect()?;
    let project = conn.query_row(
        "SELECT id, name, path, created_at, updated_at FROM projects WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )?;
    Ok(project)
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Environment {
    pub id: String,
    pub project_id: String,
    pub provider: String,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_environment(row: &rusqlite::Row) -> rusqlite::Result<Environment> {
    let metadata_str: String = row.get(4)?;
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata_str).unwrap_or(serde_json::json!({}));
    Ok(Environment {
        id: row.get(0)?,
        project_id: row.get(1)?,
        provider: row.get(2)?,
        status: row.get(3)?,
        metadata,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn complete_preparing_environment(
    id: &str,
    status: &str,
    metadata: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    if status != "pool" && status != "in_use" {
        anyhow::bail!("invalid completion status for preparing environment: {status}");
    }

    let conn = connect()?;
    let now = now_rfc3339();
    let metadata_str = serde_json::to_string(metadata)?;
    let rows = conn.execute(
        "UPDATE environments SET status = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4 AND status = 'preparing'",
        rusqlite::params![status, metadata_str, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment {id} is not in preparing status");
    }
    Ok(())
}

pub fn get_environment(id: &str) -> Result<Environment, anyhow::Error> {
    let conn = connect()?;
    let env = conn.query_row(
        "SELECT id, project_id, provider, status, metadata, created_at, updated_at FROM environments WHERE id = ?1",
        rusqlite::params![id],
        row_to_environment,
    )?;
    Ok(env)
}

pub fn list_environments() -> Result<Vec<Environment>, anyhow::Error> {
    let conn = connect()?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, provider, status, metadata, created_at, updated_at FROM environments ORDER BY id",
    )?;
    let envs = stmt
        .query_map([], row_to_environment)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(envs)
}

pub fn update_environment_metadata(
    id: &str,
    metadata: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let metadata_str = serde_json::to_string(metadata)?;
    let rows = conn.execute(
        "UPDATE environments SET metadata = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![metadata_str, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment not found: {id}");
    }
    Ok(())
}

pub fn update_environment_status(id: &str, status: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let rows = conn.execute(
        "UPDATE environments SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment not found: {id}");
    }
    Ok(())
}

fn claim_environment_tx(tx: &Transaction<'_>, id: &str) -> Result<(), anyhow::Error> {
    let now = now_rfc3339();
    let rows = tx.execute(
        "UPDATE environments SET status = 'in_use', updated_at = ?1 WHERE id = ?2 AND status = 'pool'",
        rusqlite::params![&now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment {id} is not in the pool (may not exist or already claimed)");
    }
    Ok(())
}

pub fn delete_environment(id: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let rows = conn.execute(
        "DELETE FROM environments WHERE id = ?1",
        rusqlite::params![id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment not found: {id}");
    }
    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Task {
    pub id: String,
    pub environment_id: String,
    pub project_id: String,
    pub provider: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        environment_id: row.get(1)?,
        project_id: row.get(2)?,
        provider: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn stage_prepare_environment(
    project_id: &str,
    provider: &str,
    claim_after_prepare: bool,
) -> Result<Environment, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let project_exists: Option<String> = tx
        .query_row(
            "SELECT id FROM projects WHERE id = ?1",
            rusqlite::params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    if project_exists.is_none() {
        anyhow::bail!("project not found: {project_id}");
    }

    let env_id = crate::id::new_id();
    let now = now_rfc3339();
    tx.execute(
        "INSERT INTO environments (id, project_id, provider, status, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, 'preparing', '{}', ?4, ?5)",
        rusqlite::params![&env_id, project_id, provider, &now, &now],
    )?;

    let payload = serde_json::json!({
        "env_id": env_id,
        "claim_after_prepare": claim_after_prepare,
    });
    let dedupe = format!("prepare_environment:env:{env_id}");
    let _ = insert_job_tx(&tx, "prepare_environment", &payload, Some(&dedupe))?;

    tx.commit()?;
    get_environment(&env_id)
}

pub fn stage_task_create(
    project_id: &str,
    task_provider: &str,
    env_provider: &str,
    description: &str,
) -> Result<Task, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let project_exists: Option<String> = tx
        .query_row(
            "SELECT id FROM projects WHERE id = ?1",
            rusqlite::params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    if project_exists.is_none() {
        anyhow::bail!("project not found: {project_id}");
    }

    let task_id = crate::id::new_id();
    let now = now_rfc3339();
    let mut created_new_environment = false;

    let env_id = {
        let candidate_env_id: Option<String> = tx
            .query_row(
                "SELECT id FROM environments WHERE provider = ?1 AND project_id = ?2 AND status = 'pool' ORDER BY created_at ASC LIMIT 1",
                rusqlite::params![env_provider, project_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(candidate_env_id) = candidate_env_id {
            let claimed = tx.execute(
                "UPDATE environments SET status = 'in_use', updated_at = ?1 WHERE id = ?2 AND status = 'pool'",
                rusqlite::params![&now, &candidate_env_id],
            )?;
            if claimed == 1 {
                candidate_env_id
            } else {
                created_new_environment = true;
                let new_env_id = crate::id::new_id();
                tx.execute(
                    "INSERT INTO environments (id, project_id, provider, status, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, 'preparing', '{}', ?4, ?5)",
                    rusqlite::params![&new_env_id, project_id, env_provider, &now, &now],
                )?;
                new_env_id
            }
        } else {
            created_new_environment = true;
            let new_env_id = crate::id::new_id();
            tx.execute(
                "INSERT INTO environments (id, project_id, provider, status, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, 'preparing', '{}', ?4, ?5)",
                rusqlite::params![&new_env_id, project_id, env_provider, &now, &now],
            )?;
            new_env_id
        }
    };

    tx.execute(
        "INSERT INTO tasks (id, environment_id, project_id, provider, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7)",
        rusqlite::params![&task_id, &env_id, project_id, task_provider, description, &now, &now],
    )?;

    if created_new_environment {
        let payload = serde_json::json!({
            "task_id": task_id,
            "env_id": env_id,
        });
        let dedupe = format!("prepare_environment:env:{env_id}");
        let _ = insert_job_tx(&tx, "prepare_environment", &payload, Some(&dedupe))?;
    } else {
        let payload = serde_json::json!({
            "task_id": task_id,
            "env_id": env_id,
        });
        let dedupe = format!("claim_environment:task:{task_id}");
        let _ = insert_job_tx(&tx, "claim_environment", &payload, Some(&dedupe))?;
    }

    tx.commit()?;
    get_task(&task_id)
}

pub fn stage_update_environment(id: &str) -> Result<Environment, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let env: Option<Environment> = {
        let mut stmt = tx.prepare(
            "SELECT id, project_id, provider, status, metadata, created_at, updated_at FROM environments WHERE id = ?1",
        )?;
        stmt.query_row(rusqlite::params![id], row_to_environment)
            .optional()?
    };

    let env = env.ok_or_else(|| anyhow::anyhow!("environment not found: {id}"))?;
    if env.status != "pool" {
        anyhow::bail!("environment {id} is not in the pool");
    }

    let payload = serde_json::json!({ "env_id": id });
    let dedupe = format!("update_environment:env:{id}");
    let _ = insert_job_tx(&tx, "update_environment", &payload, Some(&dedupe))?;

    tx.commit()?;
    get_environment(id)
}

pub fn stage_claim_environment(id: &str) -> Result<Environment, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    claim_environment_tx(&tx, id)?;

    let payload = serde_json::json!({ "env_id": id });
    let dedupe = format!("claim_environment:env:{id}");
    let _ = insert_job_tx(&tx, "claim_environment", &payload, Some(&dedupe))?;

    tx.commit()?;
    get_environment(id)
}

pub fn stage_claim_next_environment(
    provider: &str,
    project_id: &str,
) -> Result<Environment, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let id: String = tx
        .query_row(
            "SELECT id FROM environments WHERE provider = ?1 AND project_id = ?2 AND status = 'pool' ORDER BY created_at ASC LIMIT 1",
            rusqlite::params![provider, project_id],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("no available environment for provider={provider} project_id={project_id}"))?;

    claim_environment_tx(&tx, &id)?;
    let payload = serde_json::json!({ "env_id": id });
    let dedupe = format!("claim_environment:env:{id}");
    let _ = insert_job_tx(&tx, "claim_environment", &payload, Some(&dedupe))?;

    tx.commit()?;
    get_environment(&id)
}

pub fn stage_remove_environment(id: &str) -> Result<(), anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let task_for_environment: Option<String> = tx
        .query_row(
            "SELECT id FROM tasks WHERE environment_id = ?1 LIMIT 1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(task_id) = task_for_environment {
        anyhow::bail!("environment {id} is attached to task {task_id}; remove the task instead");
    }

    let status: Option<String> = tx
        .query_row(
            "SELECT status FROM environments WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .optional()?;

    let status = status.ok_or_else(|| anyhow::anyhow!("environment not found: {id}"))?;
    if status == "removing" {
        anyhow::bail!("environment {id} is already being removed");
    }

    let now = now_rfc3339();
    tx.execute(
        "UPDATE environments SET status = 'removing', updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, id],
    )?;

    let payload = serde_json::json!({ "env_id": id });
    let dedupe = format!("remove_environment:env:{id}");
    let _ = insert_job_tx(&tx, "remove_environment", &payload, Some(&dedupe))?;

    tx.commit()?;
    Ok(())
}

pub fn force_delete_environment(id: &str) -> Result<(), anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let task_for_environment: Option<String> = tx
        .query_row(
            "SELECT id FROM tasks WHERE environment_id = ?1 LIMIT 1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(task_id) = task_for_environment {
        anyhow::bail!("environment {id} is attached to task {task_id}; remove the task instead");
    }

    let rows = tx.execute(
        "DELETE FROM environments WHERE id = ?1",
        rusqlite::params![id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment not found: {id}");
    }

    tx.commit()?;
    Ok(())
}

pub fn stage_remove_task(task_id: &str) -> Result<(), anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let env_id: String = tx
        .query_row(
            "SELECT environment_id FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("task not found: {task_id}"))?;

    let env_status: Option<String> = tx
        .query_row(
            "SELECT status FROM environments WHERE id = ?1",
            rusqlite::params![&env_id],
            |row| row.get(0),
        )
        .optional()?;
    let env_status =
        env_status.ok_or_else(|| anyhow::anyhow!("environment not found: {env_id}"))?;

    if env_status != "removing" {
        let now = now_rfc3339();
        tx.execute(
            "UPDATE environments SET status = 'removing', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![&now, &env_id],
        )?;
    }

    let payload = serde_json::json!({
        "task_id": task_id,
        "env_id": env_id,
    });
    let dedupe = format!("remove_task:task:{task_id}");
    let _ = insert_job_tx(&tx, "remove_task", &payload, Some(&dedupe))?;

    tx.commit()?;
    Ok(())
}

pub fn force_delete_task(task_id: &str) -> Result<(), anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;

    let env_id: String = tx
        .query_row(
            "SELECT environment_id FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("task not found: {task_id}"))?;

    tx.execute(
        "DELETE FROM tasks WHERE id = ?1",
        rusqlite::params![task_id],
    )?;
    let env_rows = tx.execute(
        "DELETE FROM environments WHERE id = ?1",
        rusqlite::params![&env_id],
    )?;
    if env_rows == 0 {
        anyhow::bail!("environment not found: {env_id}");
    }

    tx.commit()?;
    Ok(())
}

pub fn start_task(id: &str) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let rows = conn.execute(
        "UPDATE tasks SET status = 'started', updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("task not found: {id}");
    }
    drop(conn);
    get_task(id)
}

pub fn get_task(id: &str) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let task = conn.query_row(
        "SELECT id, environment_id, project_id, provider, description, status, created_at, updated_at FROM tasks WHERE id = ?1",
        rusqlite::params![id],
        row_to_task,
    )?;
    Ok(task)
}

pub fn list_tasks() -> Result<Vec<Task>, anyhow::Error> {
    let conn = connect()?;
    let mut stmt = conn.prepare(
        "SELECT id, environment_id, project_id, provider, description, status, created_at, updated_at FROM tasks ORDER BY created_at DESC",
    )?;
    let tasks = stmt
        .query_map([], row_to_task)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

pub fn delete_task_and_environment(task_id: &str, env_id: &str) -> Result<(), anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM tasks WHERE id = ?1",
        rusqlite::params![task_id],
    )?;
    tx.execute(
        "DELETE FROM environments WHERE id = ?1",
        rusqlite::params![env_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn update_task_status(id: &str, status: &str) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let rows = conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("task not found: {id}");
    }
    drop(conn);
    get_task(id)
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Job {
    pub id: String,
    #[serde(rename = "type")]
    pub job_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempt: i64,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
    let payload_str: String = row.get(2)?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));
    Ok(Job {
        id: row.get(0)?,
        job_type: row.get(1)?,
        payload,
        status: row.get(3)?,
        attempt: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn create_job_with_dedupe(
    job_type: &str,
    payload: &serde_json::Value,
    dedupe_key: Option<&str>,
) -> Result<Job, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;
    let id = insert_job_tx(&tx, job_type, payload, dedupe_key)?;
    tx.commit()?;
    get_job(&id)
}

pub fn get_job(id: &str) -> Result<Job, anyhow::Error> {
    let conn = connect()?;
    let job = conn.query_row(
        "SELECT id, type, payload, status, attempt, created_at, updated_at FROM jobs WHERE id = ?1",
        rusqlite::params![id],
        row_to_job,
    )?;
    Ok(job)
}

pub fn claim_pending_jobs(limit: usize, lease_seconds: i64) -> Result<Vec<Job>, anyhow::Error> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let mut conn = connect()?;
    let tx = conn.transaction()?;
    let now = now_rfc3339();
    let lease_expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(lease_seconds)).to_rfc3339();

    let mut jobs = {
        let mut stmt = tx.prepare(
            "SELECT id, type, payload, status, attempt, created_at, updated_at
             FROM jobs
             WHERE (
                 (status = 'pending' AND (not_before IS NULL OR not_before <= ?1))
                 OR
                 (status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?1)
             )
             ORDER BY created_at ASC
             LIMIT ?2",
        )?;
        stmt.query_map(rusqlite::params![&now, limit as i64], row_to_job)?
            .collect::<Result<Vec<_>, _>>()?
    };

    for job in &mut jobs {
        tx.execute(
            "UPDATE jobs SET status = 'running', attempt = attempt + 1, not_before = NULL, lease_expires_at = ?1, last_error = NULL, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![&lease_expires_at, &now, &job.id],
        )?;
        job.status = "running".to_string();
        job.updated_at = now.clone();
        job.attempt += 1;
    }

    tx.commit()?;
    Ok(jobs)
}

pub fn mark_job_complete(id: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let rows = conn.execute(
        "UPDATE jobs SET status = 'complete', dedupe_key = NULL, not_before = NULL, lease_expires_at = NULL, last_error = NULL, updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("job not found: {id}");
    }
    Ok(())
}

pub fn mark_job_failed(id: &str, error: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = now_rfc3339();
    let rows = conn.execute(
        "UPDATE jobs SET status = 'failed', dedupe_key = NULL, not_before = NULL, lease_expires_at = NULL, last_error = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![error, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("job not found: {id}");
    }
    Ok(())
}

pub fn requeue_job(id: &str, error: &str, delay_seconds: i64) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now();
    let not_before = (now + chrono::Duration::seconds(delay_seconds)).to_rfc3339();
    let rows = conn.execute(
        "UPDATE jobs SET status = 'pending', not_before = ?1, lease_expires_at = NULL, last_error = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![&not_before, error, &now.to_rfc3339(), id],
    )?;
    if rows == 0 {
        anyhow::bail!("job not found: {id}");
    }
    Ok(())
}

pub fn refresh_job_lease(id: &str, lease_seconds: i64) -> Result<bool, anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now();
    let lease_expires_at = (now + chrono::Duration::seconds(lease_seconds)).to_rfc3339();
    let rows = conn.execute(
        "UPDATE jobs SET lease_expires_at = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'running'",
        rusqlite::params![&lease_expires_at, &now.to_rfc3339(), id],
    )?;
    Ok(rows > 0)
}

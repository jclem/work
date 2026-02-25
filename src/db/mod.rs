mod migrations;

use rusqlite::Connection;

fn db_path() -> Result<std::path::PathBuf, anyhow::Error> {
    Ok(crate::paths::data_dir()?.join("database.sqlite3"))
}

fn connect() -> Result<Connection, anyhow::Error> {
    let mut conn = Connection::open(db_path()?)?;
    migrations::run(&mut conn)?;
    Ok(conn)
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

pub fn create_preparing_environment(
    id: &str,
    project_id: &str,
    provider: &str,
) -> Result<Environment, anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO environments (id, project_id, provider, status, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, 'preparing', '{}', ?4, ?5)",
        rusqlite::params![id, project_id, provider, &now, &now],
    )?;
    get_environment(id)
}

pub fn finish_preparing_environment(
    id: &str,
    metadata: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
    let metadata_str = serde_json::to_string(metadata)?;
    let rows = conn.execute(
        "UPDATE environments SET status = 'pool', metadata = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'preparing'",
        rusqlite::params![metadata_str, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment {id} is not in preparing status");
    }
    Ok(())
}

pub fn fail_preparing_environment(id: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE environments SET status = 'failed', updated_at = ?1 WHERE id = ?2 AND status = 'preparing'",
        rusqlite::params![&now, id],
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
    let now = chrono::Utc::now().to_rfc3339();
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
    let now = chrono::Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE environments SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment not found: {id}");
    }
    Ok(())
}

pub fn claim_environment(id: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE environments SET status = 'in_use', updated_at = ?1 WHERE id = ?2 AND status = 'pool'",
        rusqlite::params![&now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("environment {id} is not in the pool (may not exist or already claimed)");
    }
    Ok(())
}

pub fn claim_next_environment(
    provider: &str,
    project_id: &str,
) -> Result<Environment, anyhow::Error> {
    let conn = connect()?;
    let id: String = conn
        .query_row(
            "SELECT id FROM environments WHERE provider = ?1 AND project_id = ?2 AND status = 'pool' ORDER BY created_at ASC LIMIT 1",
            rusqlite::params![provider, project_id],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("no available environment for provider={provider} project_id={project_id}"))?;
    drop(conn);
    claim_environment(&id)?;
    get_environment(&id)
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

pub fn create_task(
    environment_id: &str,
    project_id: &str,
    provider: &str,
    description: &str,
) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let id = crate::id::new_id();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (id, environment_id, project_id, provider, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7)",
        rusqlite::params![id, environment_id, project_id, provider, description, &now, &now],
    )?;
    get_task(&id)
}

pub fn start_task(id: &str) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
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

pub fn delete_task(task_id: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let rows = conn.execute(
        "DELETE FROM tasks WHERE id = ?1",
        rusqlite::params![task_id],
    )?;
    if rows == 0 {
        anyhow::bail!("task not found: {task_id}");
    }
    Ok(())
}

pub fn update_task_status(id: &str, status: &str) -> Result<Task, anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
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
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn create_job(job_type: &str, payload: &serde_json::Value) -> Result<Job, anyhow::Error> {
    let conn = connect()?;
    let id = crate::id::new_id();
    let now = chrono::Utc::now().to_rfc3339();
    let payload_str = serde_json::to_string(payload)?;
    conn.execute(
        "INSERT INTO jobs (id, type, payload, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
        rusqlite::params![id, job_type, payload_str, &now, &now],
    )?;
    drop(conn);
    get_job(&id)
}

pub fn get_job(id: &str) -> Result<Job, anyhow::Error> {
    let conn = connect()?;
    let job = conn.query_row(
        "SELECT id, type, payload, status, created_at, updated_at FROM jobs WHERE id = ?1",
        rusqlite::params![id],
        row_to_job,
    )?;
    Ok(job)
}

pub fn claim_pending_jobs() -> Result<Vec<Job>, anyhow::Error> {
    let mut conn = connect()?;
    let tx = conn.transaction()?;
    let jobs = {
        let mut stmt = tx.prepare(
            "SELECT id, type, payload, status, created_at, updated_at FROM jobs WHERE status = 'pending'",
        )?;
        stmt.query_map([], row_to_job)?
            .collect::<Result<Vec<_>, _>>()?
    };

    let now = chrono::Utc::now().to_rfc3339();
    for job in &jobs {
        tx.execute(
            "UPDATE jobs SET status = 'running', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![&now, &job.id],
        )?;
    }
    tx.commit()?;
    Ok(jobs)
}

pub fn update_job_status(id: &str, status: &str) -> Result<(), anyhow::Error> {
    let conn = connect()?;
    let now = chrono::Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE jobs SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, &now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("job not found: {id}");
    }
    Ok(())
}

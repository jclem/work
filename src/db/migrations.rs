use rusqlite::Connection;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_init",
        sql: include_str!("../../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "0002_environment_failed_status",
        sql: include_str!("../../migrations/0002_environment_failed_status.sql"),
    },
    Migration {
        version: 3,
        name: "0003_task_environment_not_null",
        sql: include_str!("../../migrations/0003_task_environment_not_null.sql"),
    },
    Migration {
        version: 4,
        name: "0004_jobs_queue_metadata",
        sql: include_str!("../../migrations/0004_jobs_queue_metadata.sql"),
    },
];

pub fn run(conn: &mut Connection) -> Result<(), anyhow::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
    )?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for migration in MIGRATIONS {
        if migration.version <= current_version {
            continue;
        }

        tracing::debug!(name = migration.name, "applying migration");

        let now = chrono::Utc::now().to_rfc3339();

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![migration.version, migration.name, now],
        )?;
        tx.commit()?;
    }

    Ok(())
}

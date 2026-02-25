INSERT INTO environments (id, project_id, provider, status, metadata, created_at, updated_at)
SELECT
    'env-migrated-' || t.id,
    t.project_id,
    'migration-placeholder',
    'failed',
    '{"reason":"backfilled missing task environment"}',
    t.created_at,
    t.updated_at
FROM tasks t
WHERE t.environment_id IS NULL;

CREATE TABLE tasks_new (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL UNIQUE REFERENCES environments(id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    provider TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'started', 'complete', 'failed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO tasks_new (id, environment_id, project_id, provider, description, status, created_at, updated_at)
SELECT
    id,
    COALESCE(environment_id, 'env-migrated-' || id),
    project_id,
    provider,
    description,
    status,
    created_at,
    updated_at
FROM tasks;

DROP TABLE tasks;

ALTER TABLE tasks_new RENAME TO tasks;

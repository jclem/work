CREATE TABLE environments_new (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    provider TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('preparing', 'pool', 'in_use', 'removing', 'failed')),
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO environments_new (id, project_id, provider, status, metadata, created_at, updated_at)
SELECT id, project_id, provider, status, metadata, created_at, updated_at
FROM environments;

DROP TABLE environments;

ALTER TABLE environments_new RENAME TO environments;

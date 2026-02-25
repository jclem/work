ALTER TABLE jobs ADD COLUMN dedupe_key TEXT;
ALTER TABLE jobs ADD COLUMN attempt INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN not_before TEXT;
ALTER TABLE jobs ADD COLUMN lease_expires_at TEXT;
ALTER TABLE jobs ADD COLUMN last_error TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS jobs_dedupe_key_unique
ON jobs(dedupe_key)
WHERE dedupe_key IS NOT NULL;

-- Jobs may be left running after an unclean shutdown. Requeue them.
UPDATE jobs
SET status = 'pending',
    lease_expires_at = NULL
WHERE status = 'running';

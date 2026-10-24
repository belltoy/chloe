-- Background jobs (B1-01). `(kind, dedup_key)` is unique so the same work is queued once;
-- `lease_id` fences stale workers; times are unix microseconds.
CREATE TABLE jobs (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL,
    dedup_key        TEXT NOT NULL,
    payload          TEXT NOT NULL,
    state            TEXT NOT NULL CHECK (state IN ('pending','leased','succeeded','dead_letter')),
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL,
    last_error       TEXT,
    next_run_at      INTEGER NOT NULL,
    lease_id         TEXT,
    leased_by        TEXT,
    lease_expires_at INTEGER,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE (kind, dedup_key)
);
CREATE INDEX jobs_due ON jobs(state, next_run_at);

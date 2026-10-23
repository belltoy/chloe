-- Captured agent events (B0-04). A session is one agent's session; `events.idempotency_key`
-- is unique so importing the same capture twice stores nothing new.
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,
    agent         TEXT NOT NULL,
    native_id     TEXT NOT NULL,
    project_id    TEXT REFERENCES projects(id), -- NULL: directory is not a registered project
    cwd           TEXT,
    state         TEXT NOT NULL CHECK (state IN ('open','ended')),
    started_at    INTEGER NOT NULL, -- unix microseconds of the earliest event
    last_event_at INTEGER NOT NULL,
    UNIQUE (agent, native_id)
);

CREATE TABLE events (
    seq              INTEGER PRIMARY KEY, -- arrival order
    idempotency_key  TEXT NOT NULL UNIQUE,
    session_id       TEXT NOT NULL REFERENCES sessions(id),
    kind             TEXT NOT NULL CHECK (kind IN ('session_start','user_prompt','tool_result_summary','context_compacted','session_end')),
    native_event_id  TEXT,
    tool_name        TEXT,
    text             TEXT,
    redactions       TEXT NOT NULL, -- JSON array of reason codes
    schema_version   INTEGER NOT NULL,
    occurred_at      INTEGER NOT NULL,
    imported_at      INTEGER NOT NULL
);
CREATE INDEX events_session ON events(session_id, seq);

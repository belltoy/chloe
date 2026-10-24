-- A new event kind: the agent's final reply of a turn (`assistant_message`). SQLite cannot
-- change a CHECK constraint, so the table is rebuilt; nothing references `events`.
DROP INDEX events_session;
ALTER TABLE events RENAME TO events_old;
CREATE TABLE events (
    seq              INTEGER PRIMARY KEY, -- arrival order
    idempotency_key  TEXT NOT NULL UNIQUE,
    session_id       TEXT NOT NULL REFERENCES sessions(id),
    kind             TEXT NOT NULL CHECK (kind IN ('session_start','user_prompt','tool_result_summary','assistant_message','context_compacted','session_end')),
    native_event_id  TEXT,
    tool_name        TEXT,
    text             TEXT,
    redactions       TEXT NOT NULL, -- JSON array of reason codes
    schema_version   INTEGER NOT NULL,
    occurred_at      INTEGER NOT NULL,
    imported_at      INTEGER NOT NULL
);
INSERT INTO events SELECT seq, idempotency_key, session_id, kind, native_event_id, tool_name, text, redactions, schema_version, occurred_at, imported_at FROM events_old;
DROP TABLE events_old;
CREATE INDEX events_session ON events(session_id, seq);

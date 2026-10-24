-- Extraction output waiting for review (B1-04). `state` already allows the review states of
-- B2 so that milestone needs no table rebuild. Re-running an extraction cannot repeat a
-- candidate with the same kind and title for the same session.
CREATE TABLE candidates (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    project_id TEXT REFERENCES projects(id), -- NULL: the session's directory is not a project
    kind       TEXT NOT NULL,
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    evidence   TEXT NOT NULL, -- JSON of Evidence
    state      TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','accepted','rejected','merged','conflict')),
    created_at INTEGER NOT NULL,
    UNIQUE (session_id, kind, title)
);
CREATE INDEX candidates_state ON candidates(state, created_at);

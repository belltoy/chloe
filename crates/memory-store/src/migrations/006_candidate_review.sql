-- Review of candidates (B2-01): the decision, the memory it points at, and provenance.
ALTER TABLE candidates ADD COLUMN memory_id  TEXT REFERENCES memories(id);
ALTER TABLE candidates ADD COLUMN reason     TEXT;
ALTER TABLE candidates ADD COLUMN decided_at INTEGER;
ALTER TABLE candidates ADD COLUMN decided_by TEXT;

-- Which candidates a memory was created from or merged with. A candidate is a source of at
-- most one memory.
CREATE TABLE memory_sources (
    id           TEXT PRIMARY KEY,
    memory_id    TEXT NOT NULL REFERENCES memories(id),
    candidate_id TEXT NOT NULL UNIQUE REFERENCES candidates(id),
    session_id   TEXT NOT NULL REFERENCES sessions(id),
    relation     TEXT NOT NULL CHECK (relation IN ('created','merged')),
    evidence     TEXT NOT NULL, -- JSON of Evidence
    created_at   INTEGER NOT NULL
);
CREATE INDEX memory_sources_memory ON memory_sources(memory_id, created_at);

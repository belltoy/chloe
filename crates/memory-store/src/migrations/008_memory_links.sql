-- Relations between memories (B2-06). `from_id` is the newer memory, `to_id` the one it
-- replaced; a memory can be replaced once.
CREATE TABLE memory_links (
    id         TEXT PRIMARY KEY,
    from_id    TEXT NOT NULL REFERENCES memories(id),
    to_id      TEXT NOT NULL REFERENCES memories(id),
    relation   TEXT NOT NULL CHECK (relation IN ('supersedes')),
    created_at INTEGER NOT NULL,
    CHECK (from_id <> to_id),
    UNIQUE (to_id, relation)
);
CREATE INDEX memory_links_from ON memory_links(from_id);

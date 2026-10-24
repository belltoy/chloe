-- Embedding index (B3-01). One pinned model for the whole index; `generation` changes when the
-- model is switched. Vectors are little-endian f32; `revision` marks the memory revision embedded.
CREATE TABLE embedding_index (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    model_id   TEXT NOT NULL,
    dimension  INTEGER,
    generation INTEGER NOT NULL
);
CREATE TABLE embeddings (
    memory_id  TEXT PRIMARY KEY REFERENCES memories(id),
    revision   INTEGER NOT NULL,
    vector     BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

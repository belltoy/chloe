CREATE TABLE projects (
    id             TEXT PRIMARY KEY,
    canonical_path TEXT NOT NULL UNIQUE,
    created_at     INTEGER NOT NULL
);

CREATE TABLE project_aliases (
    project_id TEXT NOT NULL REFERENCES projects(id),
    path       TEXT NOT NULL UNIQUE,
    remote_url TEXT,
    kind       TEXT NOT NULL
);

-- project_id NULL = global scope.
CREATE TABLE memories (
    id         TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id),
    kind       TEXT NOT NULL CHECK (kind IN ('fact','decision','procedure','constraint','gotcha')),
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('active','disputed','superseded','archived')),
    source     TEXT NOT NULL CHECK (source IN ('manual','agent','extracted')),
    created_at INTEGER NOT NULL, -- unix microseconds
    updated_at INTEGER NOT NULL,
    revision   INTEGER NOT NULL CHECK (revision >= 1)
);
CREATE INDEX memories_scope ON memories(project_id, status, updated_at);

CREATE TABLE memory_revisions (
    memory_id  TEXT NOT NULL REFERENCES memories(id),
    revision   INTEGER NOT NULL,
    snapshot   TEXT NOT NULL, -- JSON of the Memory after this revision
    changed_at INTEGER NOT NULL,
    cause      TEXT NOT NULL,
    PRIMARY KEY (memory_id, revision)
);

-- shortcut: default unicode61 tokenizer, unsegmented CJK text matches whole runs only; revisit in A3.
CREATE VIRTUAL TABLE memory_fts USING fts5(title, content, content='memories', content_rowid='rowid');

CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memory_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
    INSERT INTO memory_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;

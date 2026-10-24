-- Keyword search for Chinese/Japanese/Korean text (B3-07). `unicode61` cannot segment CJK, so
-- a second full-text index uses the `trigram` tokenizer (any 3 consecutive characters match).
-- `memory_fts` is untouched: queries without CJK text behave as before. Existing memories are
-- indexed from `memories` by the rebuild below.
CREATE VIRTUAL TABLE memory_fts_cjk USING fts5(
    title,
    content,
    content='memories',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER memories_cjk_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memory_fts_cjk(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;
CREATE TRIGGER memories_cjk_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memory_fts_cjk(memory_fts_cjk, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
END;
CREATE TRIGGER memories_cjk_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memory_fts_cjk(memory_fts_cjk, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
    INSERT INTO memory_fts_cjk(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;

INSERT INTO memory_fts_cjk(memory_fts_cjk) VALUES ('rebuild');

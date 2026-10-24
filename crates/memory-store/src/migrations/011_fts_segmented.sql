-- Chinese keyword search (B3-07), second attempt: the trigram index of migration 010 is
-- replaced by one over text that the application has cut into words (jieba). The index keeps
-- its own copy of the segmented text; rows share `rowid` with `memories`, and the application
-- writes them (a trigger cannot segment). `fts_meta` records which segmentation built the
-- index so a change triggers a rebuild on open.
DROP TRIGGER IF EXISTS memories_cjk_ai;
DROP TRIGGER IF EXISTS memories_cjk_ad;
DROP TRIGGER IF EXISTS memories_cjk_au;
DROP TABLE IF EXISTS memory_fts_cjk;

CREATE VIRTUAL TABLE memory_fts_seg USING fts5(title, content);
CREATE TABLE fts_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

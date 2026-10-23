-- The revision table doubles as the append-only audit trail (operation = cause, plus who did it).
ALTER TABLE memory_revisions ADD COLUMN actor TEXT NOT NULL DEFAULT 'unknown';

CREATE TRIGGER memory_revisions_no_update BEFORE UPDATE ON memory_revisions BEGIN
    SELECT RAISE(ABORT, 'memory_revisions is append-only');
END;
CREATE TRIGGER memory_revisions_no_delete BEFORE DELETE ON memory_revisions BEGIN
    SELECT RAISE(ABORT, 'memory_revisions is append-only');
END;

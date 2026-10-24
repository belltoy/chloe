-- Extraction watermark: the highest event `seq` of a session that an extraction job has
-- covered. Candidates and the watermark are written in one transaction, so a crash cannot
-- leave one without the other, and a session resumed after extraction is extracted again from
-- where it stopped.
ALTER TABLE sessions ADD COLUMN extracted_seq INTEGER NOT NULL DEFAULT 0;

-- Sessions that an earlier version already extracted keep that: everything they have today
-- counts as covered.
UPDATE sessions SET extracted_seq = COALESCE((SELECT max(seq) FROM events WHERE session_id = sessions.id), 0)
WHERE EXISTS (
    SELECT 1 FROM jobs
    WHERE kind = 'extract_session' AND state = 'succeeded'
      AND dedup_key = sessions.agent || '|' || sessions.native_id
);

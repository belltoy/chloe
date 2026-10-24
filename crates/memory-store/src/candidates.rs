//! `CandidateStore` for SQLite: extraction output waiting for review.

use memory_core::extract::{Candidate, CandidateStore, Evidence, StoredCandidate};
use memory_core::{Error, Result};
use rusqlite::{OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{SqliteStore, begin, conv, from_micros, micros, now, storage, tag, untag};

const MAX_ROWS: usize = 1000;

impl CandidateStore for SqliteStore {
    fn add_candidates(
        &self,
        native_agent: &str,
        native_session_id: &str,
        candidates: &[Candidate],
    ) -> Result<usize> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let session: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT id, project_id FROM sessions WHERE agent=?1 AND native_id=?2",
                params![native_agent, native_session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        let Some((session_id, project_id)) = session else {
            return Err(Error::NotFound(format!("session {native_session_id}")));
        };
        let at = micros(now());
        let mut added = 0;
        for c in candidates {
            added += tx
                .execute(
                    "INSERT OR IGNORE INTO candidates (id, session_id, project_id, kind, title, content, evidence, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        session_id,
                        project_id,
                        tag(&c.kind),
                        c.title,
                        c.content,
                        serde_json::to_string(&c.evidence).map_err(storage)?,
                        at
                    ],
                )
                .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(added)
    }

    fn pending_candidates(&self, limit: usize) -> Result<Vec<StoredCandidate>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, kind, title, content, evidence, created_at
                 FROM candidates WHERE state='pending' ORDER BY created_at DESC, rowid DESC LIMIT ?1",
            )
            .map_err(storage)?;
        let rows = stmt
            .query_map([limit.clamp(1, MAX_ROWS) as i64], to_candidate)
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }
}

fn to_candidate(r: &Row) -> rusqlite::Result<StoredCandidate> {
    let evidence: Evidence = conv(5, serde_json::from_str(&r.get::<_, String>(5)?))?;
    Ok(StoredCandidate {
        id: r.get(0)?,
        project_id: r
            .get::<_, Option<String>>(1)?
            .map(|s| conv(1, s.parse()))
            .transpose()?,
        candidate: Candidate {
            kind: untag(r, 2)?,
            title: r.get(3)?,
            content: r.get(4)?,
            evidence,
        },
        created_at: conv(6, from_micros(r.get(6)?))?,
    })
}

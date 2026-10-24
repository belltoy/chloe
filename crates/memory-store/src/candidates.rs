//! `CandidateStore` for SQLite: extraction output, its review state and provenance.

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, MemorySource, Proposal, Resolution,
    SourceRelation, StoredCandidate,
};
use memory_core::{Error, Memory, MemoryId, ProjectId, Result, Source, Status};
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::{
    SqliteStore, begin, conv, from_micros, insert_memory, micros, now, storage, tag, untag,
    valid_content, valid_title,
};

const MAX_ROWS: usize = 1000;
const MAX_REASON_CHARS: usize = 1000;
const COLS: &str = "id, project_id, kind, title, content, evidence, state, memory_id, reason, created_at, decided_at, proposal";

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
                    "INSERT OR IGNORE INTO candidates
                    (id, session_id, project_id, kind, title, content, evidence, created_at)
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

    fn candidates(
        &self,
        state: Option<CandidateState>,
        limit: usize,
    ) -> Result<Vec<StoredCandidate>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLS} FROM candidates WHERE (?1 IS NULL OR state=?1)
                 ORDER BY created_at DESC, rowid DESC LIMIT ?2"
            ))
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                params![state.map(|s| tag(&s)), limit.clamp(1, MAX_ROWS) as i64],
                to_candidate,
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    fn candidate(&self, id: &str) -> Result<StoredCandidate> {
        fetch(&self.conn(), id)
    }

    fn set_proposal(&self, id: &str, proposal: &Proposal) -> Result<()> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let c = fetch(&tx, id)?;
        if !c.state.is_open() {
            return Err(Error::InvalidArgument(format!(
                "candidate {id} is already {}",
                c.state.as_str()
            )));
        }
        tx.execute(
            "UPDATE candidates SET proposal=?1 WHERE id=?2",
            params![serde_json::to_string(proposal).map_err(storage)?, id],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }

    fn resolve_candidate(&self, id: &str, resolution: Resolution) -> Result<StoredCandidate> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let c = fetch(&tx, id)?;
        if !c.state.is_open() {
            return Err(Error::InvalidArgument(format!(
                "candidate {id} is already {}",
                c.state.as_str()
            )));
        }
        let (state, memory, reason) = match resolution {
            Resolution::Accept { global } => {
                let m = new_memory(&c, global)?;
                insert_memory(&tx, &m, "extracted", &self.actor)?;
                add_source(&tx, &c, m.id, SourceRelation::Created)?;
                (CandidateState::Accepted, Some(m.id), None)
            }
            Resolution::Reject { reason } => (CandidateState::Rejected, None, Some(reason)),
            Resolution::Merge { into, reason } => {
                visible_memory(&tx, &c, into)?;
                add_source(&tx, &c, into, SourceRelation::Merged)?;
                (CandidateState::Merged, Some(into), Some(reason))
            }
            Resolution::Conflict { with, reason } => {
                if c.state != CandidateState::Pending {
                    return Err(Error::InvalidArgument(format!(
                        "candidate {id} is already {}",
                        c.state.as_str()
                    )));
                }
                visible_memory(&tx, &c, with)?;
                (CandidateState::Conflict, Some(with), Some(reason))
            }
        };
        let reason = reason.map(|r| r.chars().take(MAX_REASON_CHARS).collect::<String>());
        tx.execute(
            "UPDATE candidates
             SET
                 state=?1,
                 memory_id=?2,
                 reason=?3,
                 decided_at=?4,
                 decided_by=?5
             WHERE id=?6",
            params![
                tag(&state),
                memory.map(|m| m.to_string()),
                reason,
                micros(now()),
                self.actor,
                id
            ],
        )
        .map_err(storage)?;
        let out = fetch(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(out)
    }

    fn memory_sources(&self, memory: MemoryId) -> Result<Vec<MemorySource>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT
                     candidate_id,
                     relation,
                     evidence,
                     created_at
                 FROM memory_sources
                 WHERE memory_id=?1
                 ORDER BY
                     created_at,
                     rowid",
            )
            .map_err(storage)?;
        let rows = stmt
            .query_map([memory.to_string()], |r| {
                Ok(MemorySource {
                    candidate_id: r.get(0)?,
                    relation: untag(r, 1)?,
                    evidence: conv(2, serde_json::from_str(&r.get::<_, String>(2)?))?,
                    recorded_at: conv(3, from_micros(r.get(3)?))?,
                })
            })
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }
}

fn fetch(conn: &Connection, id: &str) -> Result<StoredCandidate> {
    conn.query_row(
        &format!("SELECT {COLS} FROM candidates WHERE id=?1"),
        [id],
        to_candidate,
    )
    .optional()
    .map_err(storage)?
    .ok_or_else(|| Error::NotFound(format!("candidate {id}")))
}

fn new_memory(c: &StoredCandidate, global: bool) -> Result<Memory> {
    let project_id: Option<ProjectId> = if global {
        None
    } else {
        Some(c.project_id.ok_or_else(|| {
            Error::InvalidArgument(
                "the candidate's session has no registered project; accept it as global".into(),
            )
        })?)
    };
    let ts = now();
    Ok(Memory {
        id: MemoryId::new(),
        project_id,
        kind: c.candidate.kind,
        title: valid_title(c.candidate.title.clone())?,
        content: valid_content(c.candidate.content.clone())?,
        status: Status::Active,
        source: Source::Extracted,
        created_at: ts,
        updated_at: ts,
        revision: 1,
    })
}

/// The memory must exist and be in the candidate's project or global; anything else is
/// `NotFound`, like every cross-project read.
fn visible_memory(conn: &Connection, c: &StoredCandidate, id: MemoryId) -> Result<()> {
    let project: Option<Option<String>> = conn
        .query_row(
            "SELECT project_id FROM memories WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?;
    let ok = match project {
        None => false,
        Some(None) => true,
        Some(Some(p)) => c.project_id.is_some_and(|cp| cp.to_string() == p),
    };
    if ok {
        Ok(())
    } else {
        Err(Error::NotFound(format!("memory {id}")))
    }
}

fn add_source(
    conn: &Connection,
    c: &StoredCandidate,
    memory: MemoryId,
    relation: SourceRelation,
) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_sources
        (id, memory_id, candidate_id, session_id, relation, evidence, created_at)
         SELECT
             ?1,
             ?2,
             c.id,
             c.session_id,
             ?3,
             c.evidence,
             ?4
         FROM candidates c
         WHERE
             c.id=?5",
        params![
            Uuid::new_v4().to_string(),
            memory.to_string(),
            tag(&relation),
            micros(now()),
            c.id
        ],
    )
    .map_err(storage)?;
    Ok(())
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
        state: untag(r, 6)?,
        memory_id: r
            .get::<_, Option<String>>(7)?
            .map(|s| conv(7, s.parse()))
            .transpose()?,
        reason: r.get(8)?,
        proposal: r
            .get::<_, Option<String>>(11)?
            .map(|j| conv(11, serde_json::from_str(&j)))
            .transpose()?,
        created_at: conv(9, from_micros(r.get(9)?))?,
        decided_at: r
            .get::<_, Option<i64>>(10)?
            .map(|us| conv(10, from_micros(us)))
            .transpose()?,
    })
}

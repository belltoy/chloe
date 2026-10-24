//! `EventStore` for SQLite: sessions and idempotent event rows.

use std::path::Path;

use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, IngestReport, Rejection, Result,
    SessionInfo, SessionState,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::project::find_project;
use crate::{SqliteStore, begin, conv, from_micros, micros, now, storage, tag, untag};

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_NAME_BYTES: usize = 256;
const MAX_ROWS: usize = 1000;

/// A reason code when the event must not be stored.
fn invalid(e: &AgentEvent) -> Option<&'static str> {
    if e.schema_version != EVENT_SCHEMA_VERSION {
        return Some("unsupported_schema_version");
    }
    let name_ok = |s: &str| !s.is_empty() && s.len() <= MAX_NAME_BYTES;
    if !name_ok(&e.native_agent) || !name_ok(&e.native_session_id) {
        return Some("invalid_field");
    }
    let too_long = |s: &Option<String>, max| s.as_ref().is_some_and(|s| s.len() > max);
    if too_long(&e.text, MAX_TEXT_BYTES)
        || too_long(&e.tool_name, MAX_NAME_BYTES)
        || too_long(&e.native_event_id, MAX_NAME_BYTES)
    {
        return Some("too_long");
    }
    None
}

/// Existing session id, or a new row. A session whose project was unknown when it started
/// gets one as soon as an event shows its directory has been registered since.
fn session_id(conn: &Connection, e: &AgentEvent) -> Result<String> {
    let project = || {
        e.cwd
            .as_deref()
            .and_then(|c| find_project(conn, Path::new(c)))
    };
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, project_id FROM sessions WHERE agent=?1 AND native_id=?2",
            params![e.native_agent, e.native_session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    if let Some((id, current)) = existing {
        if current.is_none() {
            if let Some(p) = project() {
                conn.execute(
                    "UPDATE sessions SET project_id=?1 WHERE id=?2",
                    params![p, id],
                )
                .map_err(storage)?;
            }
        }
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    let at = micros(e.occurred_at);
    conn.execute(
        "INSERT INTO sessions (id, agent, native_id, project_id, cwd, state, started_at, last_event_at)
         VALUES (?1,?2,?3,?4,?5,'open',?6,?6)",
        params![id, e.native_agent, e.native_session_id, project(), e.cwd, at],
    )
    .map_err(storage)?;
    Ok(id)
}

impl EventStore for SqliteStore {
    fn ingest(&self, events: &[AgentEvent]) -> Result<IngestReport> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let mut report = IngestReport::default();
        let imported_at = micros(now());
        for (index, e) in events.iter().enumerate() {
            if let Some(reason) = invalid(e) {
                report.rejected.push(Rejection {
                    index,
                    reason: reason.into(),
                });
                continue;
            }
            let sid = session_id(&tx, e)?;
            let at = micros(e.occurred_at);
            let added = tx
                .execute(
                    "INSERT OR IGNORE INTO events (idempotency_key, session_id, kind, native_event_id, tool_name, text, redactions, schema_version, occurred_at, imported_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        e.idempotency_key(),
                        sid,
                        tag(&e.kind),
                        e.native_event_id,
                        e.tool_name,
                        e.text,
                        serde_json::to_string(&e.redactions).map_err(storage)?,
                        e.schema_version,
                        at,
                        imported_at
                    ],
                )
                .map_err(storage)?;
            if added == 0 {
                report.duplicates += 1;
                continue;
            }
            report.inserted += 1;
            // The newest event decides the state, so a late-arriving old event cannot reopen
            // an ended session (or end an open one).
            let state = if e.kind == EventKind::SessionEnd {
                "ended"
            } else {
                "open"
            };
            tx.execute(
                "UPDATE sessions SET
                   state = CASE WHEN ?1 >= last_event_at THEN ?2 ELSE state END,
                   last_event_at = max(last_event_at, ?1),
                   started_at = min(started_at, ?1),
                   cwd = coalesce(cwd, ?3)
                 WHERE id = ?4",
                params![at, state, e.cwd, sid],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(report)
    }

    fn sessions(&self, limit: usize) -> Result<Vec<SessionInfo>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.agent, s.native_id, s.project_id, s.cwd, s.state, s.started_at, s.last_event_at,
                        (SELECT count(*) FROM events e WHERE e.session_id = s.id)
                 FROM sessions s ORDER BY s.last_event_at DESC, s.rowid DESC LIMIT ?1",
            )
            .map_err(storage)?;
        let rows = stmt
            .query_map([limit.clamp(1, MAX_ROWS) as i64], to_session)
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    fn session_events(
        &self,
        agent: &str,
        native_session_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentEvent>> {
        self.events_of(agent, native_session_id, limit, "ASC")
    }

    fn session_tail(
        &self,
        agent: &str,
        native_session_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentEvent>> {
        let mut events = self.events_of(agent, native_session_id, limit, "DESC")?;
        events.reverse();
        Ok(events)
    }
}

impl SqliteStore {
    /// `order` is one of two constants below, never user input.
    fn events_of(
        &self,
        agent: &str,
        native_session_id: &str,
        limit: usize,
        order: &str,
    ) -> Result<Vec<AgentEvent>> {
        debug_assert!(matches!(order, "ASC" | "DESC"));
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT s.agent, s.native_id, e.native_event_id, e.kind, e.occurred_at, s.cwd, e.tool_name, e.text, e.redactions, e.schema_version
                 FROM events e JOIN sessions s ON s.id = e.session_id
                 WHERE s.agent = ?1 AND s.native_id = ?2 ORDER BY e.seq {order} LIMIT ?3"
            ))
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                params![agent, native_session_id, limit.clamp(1, MAX_ROWS) as i64],
                to_event,
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }
}

fn to_session(r: &Row) -> rusqlite::Result<SessionInfo> {
    Ok(SessionInfo {
        id: r.get(0)?,
        native_agent: r.get(1)?,
        native_session_id: r.get(2)?,
        project_id: r
            .get::<_, Option<String>>(3)?
            .map(|s| conv(3, s.parse()))
            .transpose()?,
        cwd: r.get(4)?,
        state: match r.get::<_, String>(5)?.as_str() {
            "ended" => SessionState::Ended,
            _ => SessionState::Open,
        },
        started_at: conv(6, from_micros(r.get(6)?))?,
        last_event_at: conv(7, from_micros(r.get(7)?))?,
        events: r.get::<_, i64>(8)? as usize,
    })
}

fn to_event(r: &Row) -> rusqlite::Result<AgentEvent> {
    Ok(AgentEvent {
        schema_version: r.get(9)?,
        native_agent: r.get(0)?,
        native_session_id: r.get(1)?,
        native_event_id: r.get(2)?,
        kind: untag(r, 3)?,
        occurred_at: conv(4, from_micros(r.get(4)?))?,
        cwd: r.get(5)?,
        tool_name: r.get(6)?,
        text: r.get(7)?,
        redactions: conv(8, serde_json::from_str(&r.get::<_, String>(8)?))?,
    })
}

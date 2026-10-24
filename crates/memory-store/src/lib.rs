//! SQLite implementation of `MemoryService`: migrations, transactions, FTS5, history, scope.

mod candidates;
mod embeddings;
mod events;
mod jobs;
mod project;
mod segment;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use memory_core::service::{
    Hit, MemoryChanges, MemoryService, NewMemory, Revision, SearchQuery, TermMatches,
};
use memory_core::{Error, Memory, MemoryId, ProjectId, Result, Status};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use time::OffsetDateTime;

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_audit.sql"),
    include_str!("migrations/003_events.sql"),
    include_str!("migrations/004_jobs.sql"),
    include_str!("migrations/005_candidates.sql"),
    include_str!("migrations/006_candidate_review.sql"),
    include_str!("migrations/007_candidate_proposal.sql"),
    include_str!("migrations/008_memory_links.sql"),
    include_str!("migrations/009_embeddings.sql"),
    include_str!("migrations/010_fts_cjk.sql"),
    include_str!("migrations/011_fts_segmented.sql"),
    include_str!("migrations/012_extraction_watermark.sql"),
    include_str!("migrations/013_assistant_message.sql"),
];
/// How long `open` keeps retrying a locked initialisation.
const OPEN_RETRY: Duration = Duration::from_secs(5);
const MAX_TITLE_CHARS: usize = 200;
const MAX_CONTENT_BYTES: usize = 64 * 1024;
const MAX_LIMIT: usize = 100;
const COLS: &str =
    "id, project_id, kind, title, content, status, source, created_at, updated_at, revision";

/// Creates missing parent directories (0700) and the database file (0600) so memories are
/// readable only by the current user; SQLite gives the WAL/SHM files the database's mode.
fn create_private(path: &Path) -> Result<()> {
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true);
    let mut file = std::fs::OpenOptions::new();
    file.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        dirs.mode(0o700);
        file.mode(0o600);
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        dirs.create(dir).map_err(storage)?;
    }
    file.open(path).map_err(storage)?;
    Ok(())
}

/// The index a query is searched in and the terms that are OR-ed. The table name is a constant.
fn fts_plan(query: &str) -> (&'static str, Vec<String>) {
    match segment::terms(query) {
        Some(terms) => ("memory_fts_seg", terms),
        None => (
            "memory_fts",
            query.split_whitespace().map(String::from).collect(),
        ),
    }
}

/// Every term quoted, so user input can never be parsed as FTS5 syntax.
fn or_query(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// For diagnostics: the terms a search ORs together, and the ones that match one memory.
#[derive(Debug, Clone, Serialize)]
pub struct FtsCoverage {
    pub terms: Vec<String>,
    pub matched: Vec<String>,
}

fn storage(e: impl std::fmt::Display) -> Error {
    Error::Storage(e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct Doctor {
    pub schema_version: u32,
    pub sqlite_ok: bool,
    pub fts_ok: bool,
    pub foreign_keys_ok: bool,
}

impl Doctor {
    pub fn healthy(&self) -> bool {
        self.sqlite_ok && self.fts_ok && self.foreign_keys_ok
    }
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
    /// Recorded in the audit trail for every write (e.g. `cli`, `mcp`).
    actor: String,
}

impl SqliteStore {
    /// Opens (creating if needed) the database and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        create_private(path)?;
        let mut conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(storage)?;
        init(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            actor: "unknown".into(),
        })
    }

    /// How long a write waits for another process's lock before failing with `Error::Storage`.
    pub fn with_busy_timeout(self, timeout: Duration) -> Result<Self> {
        self.conn().busy_timeout(timeout).map_err(storage)?;
        Ok(self)
    }

    pub fn with_actor(mut self, actor: &str) -> Self {
        self.actor = actor.into();
        self
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn doctor(&self) -> Result<Doctor> {
        let conn = self.conn();
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(storage)?;
        let fk_violation = conn
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .map_err(storage)?;
        Ok(Doctor {
            schema_version: schema_version(&conn)?,
            sqlite_ok: integrity == "ok",
            // The command errors out when the index disagrees with `memories`.
            fts_ok: ["memory_fts", "memory_fts_seg"].iter().all(|t| {
                conn.execute(
                    &format!("INSERT INTO {t}({t}, rank) VALUES ('integrity-check', 1)"),
                    [],
                )
                .is_ok()
            }) && segment::in_step(&conn),
            foreign_keys_ok: fk_violation.is_none(),
        })
    }

    /// Which terms of `query` (as `search` would use them) the memory matches. For diagnostics;
    /// one FTS probe per term.
    pub fn fts_coverage(&self, query: &str, id: MemoryId) -> Result<FtsCoverage> {
        let (table, terms) = fts_plan(query);
        let conn = self.conn();
        let mut matched = vec![];
        for t in &terms {
            let hit = conn
                .query_row(
                    &format!(
                        "SELECT 1 FROM {table} WHERE {table} MATCH ?1 \
                         AND rowid = (SELECT rowid FROM memories WHERE id=?2)"
                    ),
                    params![or_query(std::slice::from_ref(t)), id.to_string()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(storage)?;
            if hit.is_some() {
                matched.push(t.clone());
            }
        }
        Ok(FtsCoverage { terms, matched })
    }

    /// Rebuilds the FTS index from `memories`.
    pub fn reindex(&self) -> Result<()> {
        let mut conn = self.conn();
        conn.execute("INSERT INTO memory_fts(memory_fts) VALUES ('rebuild')", [])
            .map_err(storage)?;
        let tx = begin(&mut conn)?;
        segment::rebuild(&tx)?;
        tx.commit().map_err(storage)
    }

    /// Loads the memory visible to `project`, applies `f`, and bumps the revision via CAS,
    /// all in one write transaction (the history row is part of it).
    fn mutate(
        &self,
        project: Option<ProjectId>,
        id: MemoryId,
        expected: u32,
        cause: &str,
        f: impl FnOnce(&mut Memory) -> Result<()>,
    ) -> Result<Memory> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let mut m = fetch(&tx, project, id)?;
        if m.revision != expected {
            return Err(Error::Conflict {
                expected,
                current: m.revision,
            });
        }
        f(&mut m)?;
        m.revision += 1;
        m.updated_at = now();
        tx.execute(
            "UPDATE memories SET kind=?2, title=?3, content=?4, status=?5, updated_at=?6, revision=?7 WHERE id=?1",
            params![id.to_string(), tag(&m.kind), m.title, m.content, tag(&m.status), micros(m.updated_at), m.revision],
        )
        .map_err(storage)?;
        let rowid = tx
            .query_row(
                "SELECT rowid FROM memories WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(storage)?;
        segment::write(&tx, rowid, &m.title, &m.content)?;
        write_revision(&tx, &m, cause, &self.actor)?;
        tx.commit().map_err(storage)?;
        Ok(m)
    }
}

impl MemoryService for SqliteStore {
    fn save(&self, new: NewMemory) -> Result<Memory> {
        let title = valid_title(new.title)?;
        let content = valid_content(new.content)?;
        let ts = now();
        let m = Memory {
            id: MemoryId::new(),
            project_id: new.project_id,
            kind: new.kind,
            title,
            content,
            status: Status::Active,
            source: new.source,
            created_at: ts,
            updated_at: ts,
            revision: 1,
        };
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        insert_memory(&tx, &m, "create", &self.actor)?;
        tx.commit().map_err(storage)?;
        Ok(m)
    }

    fn get(&self, project: Option<ProjectId>, id: MemoryId) -> Result<Memory> {
        fetch(&self.conn(), project, id)
    }

    fn update(
        &self,
        project: Option<ProjectId>,
        id: MemoryId,
        expected_revision: u32,
        changes: MemoryChanges,
    ) -> Result<Memory> {
        if changes.kind.is_none() && changes.title.is_none() && changes.content.is_none() {
            return Err(Error::InvalidArgument("no changes given".into()));
        }
        let title = changes.title.map(valid_title).transpose()?;
        let content = changes.content.map(valid_content).transpose()?;
        self.mutate(project, id, expected_revision, "update", |m| {
            if let Some(k) = changes.kind {
                m.kind = k;
            }
            if let Some(t) = title {
                m.title = t;
            }
            if let Some(c) = content {
                m.content = c;
            }
            Ok(())
        })
    }

    fn archive(
        &self,
        project: Option<ProjectId>,
        id: MemoryId,
        expected_revision: u32,
    ) -> Result<Memory> {
        self.mutate(project, id, expected_revision, "archive", |m| {
            m.status = Status::Archived;
            Ok(())
        })
    }

    fn list(
        &self,
        project: Option<ProjectId>,
        include_inactive: bool,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLS} FROM memories WHERE project_id IS ?1 AND (?2 OR status='active') \
                 ORDER BY updated_at DESC, rowid DESC LIMIT ?3"
            ))
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                params![
                    project.map(|p| p.to_string()),
                    include_inactive,
                    clamp(limit)
                ],
                to_memory,
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    fn matched_terms(&self, query: &str, ids: &[MemoryId]) -> Result<TermMatches> {
        let (table, terms) = fts_plan(query);
        let mut out = TermMatches {
            terms: terms.clone(),
            ..Default::default()
        };
        if ids.is_empty() {
            return Ok(out);
        }
        let marks = vec!["?"; ids.len()].join(",");
        let conn = self.conn();
        // Table name is a constant; ids and the term are bound parameters.
        let mut stmt = conn
            .prepare(&format!(
                "SELECT m.id FROM {table} JOIN memories m ON m.rowid = {table}.rowid \
                 WHERE {table} MATCH ? AND m.id IN ({marks})"
            ))
            .map_err(storage)?;
        for t in &terms {
            let mut args = vec![or_query(std::slice::from_ref(t))];
            args.extend(ids.iter().map(|i| i.to_string()));
            let found: Vec<String> = stmt
                .query_map(rusqlite::params_from_iter(args), |r| r.get(0))
                .map_err(storage)?
                .collect::<rusqlite::Result<_>>()
                .map_err(storage)?;
            for id in found {
                let id = id.parse().map_err(storage)?;
                out.matched.entry(id).or_default().push(t.clone());
            }
        }
        Ok(out)
    }

    fn search(&self, project: Option<ProjectId>, q: SearchQuery) -> Result<Vec<Hit>> {
        // Quote every token so user input can never be parsed as FTS5 syntax. Tokens are OR-ed:
        // agents send long natural-language queries, and AND returns nothing if one word is absent.
        // Queries with CJK text go to the word-segmented index; the table name is a constant.
        let (table, terms) = fts_plan(&q.query);
        let fts_query = or_query(&terms);
        if fts_query.is_empty() {
            return Err(Error::InvalidArgument("empty query".into()));
        }
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT m.id, m.project_id, m.kind, m.title, m.content, m.status, m.source, \
                        m.created_at, m.updated_at, m.revision, bm25({table}, 5.0, 1.0) \
                 FROM {table} JOIN memories m ON m.rowid = {table}.rowid \
                 WHERE {table} MATCH ?1 \
                   AND (m.project_id IS ?2 OR (?3 AND m.project_id IS NULL)) \
                   AND (?4 OR m.status = 'active') \
                 ORDER BY bm25({table}, 5.0, 1.0), m.updated_at DESC LIMIT ?5"
            ))
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                params![
                    fts_query,
                    project.map(|p| p.to_string()),
                    q.include_global,
                    q.include_inactive,
                    clamp(q.limit)
                ],
                |r| {
                    Ok(Hit {
                        memory: to_memory(r)?,
                        score: -r.get::<_, f64>(10)?,
                    })
                },
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    fn history(&self, project: Option<ProjectId>, id: MemoryId) -> Result<Vec<Revision>> {
        let conn = self.conn();
        fetch(&conn, project, id)?;
        let mut stmt = conn
            .prepare("SELECT revision, cause, snapshot, actor FROM memory_revisions WHERE memory_id=?1 ORDER BY revision")
            .map_err(storage)?;
        let rows = stmt
            .query_map([id.to_string()], |r| {
                Ok(Revision {
                    revision: r.get(0)?,
                    cause: r.get(1)?,
                    actor: r.get(3)?,
                    snapshot: conv(2, serde_json::from_str(&r.get::<_, String>(2)?))?,
                })
            })
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }
}

/// Write transaction; IMMEDIATE takes the write lock up front so busy_timeout applies
/// instead of a mid-transaction SQLITE_BUSY on lock upgrade.
fn begin(conn: &mut Connection) -> Result<Transaction<'_>> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(storage)
}

/// Forward-only; a database from a newer binary is refused (downgrade unsupported).
/// Switching to WAL and migrating a brand-new database needs locks that SQLite's busy timeout
/// does not always wait for, so several processes opening it at once (two agents starting
/// `chloe mcp serve` together) can see "database is locked". Both steps are idempotent, so
/// retry them for as long as the busy timeout would have.
fn init(conn: &mut Connection) -> Result<()> {
    let started = std::time::Instant::now();
    loop {
        let attempt = conn
            .execute_batch(
                "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
            )
            .map_err(storage)
            .and_then(|()| migrate(conn));
        match attempt {
            // shortcut: matched on the message because errors are flattened to text here; keep
            // it narrow and revisit if SQLite ever words this differently.
            Err(Error::Storage(m)) if m.contains("locked") && started.elapsed() < OPEN_RETRY => {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = begin(conn)?;
    let current = schema_version(&tx)? as usize;
    if current > MIGRATIONS.len() {
        return Err(Error::Storage(format!(
            "database schema v{current} is newer than this binary (v{})",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        tx.execute_batch(sql).map_err(storage)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)
            .map_err(storage)?;
    }
    segment::ensure(&tx)?;
    tx.commit().map_err(storage)
}

/// Visible to `project` if scopes match, or if the memory is global (readable and editable
/// from any project). Anything else is `NotFound`.
fn fetch(conn: &Connection, project: Option<ProjectId>, id: MemoryId) -> Result<Memory> {
    conn.query_row(
        &format!(
            "SELECT {COLS} FROM memories WHERE id=?1 AND (project_id IS ?2 OR project_id IS NULL)"
        ),
        params![id.to_string(), project.map(|p| p.to_string())],
        to_memory,
    )
    .optional()
    .map_err(storage)?
    .ok_or_else(|| Error::NotFound(format!("memory {id}")))
}

/// Inserts a new memory and its first revision row inside the caller's transaction.
pub(crate) fn insert_memory(tx: &Transaction, m: &Memory, cause: &str, actor: &str) -> Result<()> {
    if let Some(p) = m.project_id {
        let found = tx
            .query_row(
                "SELECT 1 FROM projects WHERE id=?1",
                [p.to_string()],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage)?;
        if found.is_none() {
            return Err(Error::NotFound(format!("project {p}")));
        }
    }
    tx.execute(
        &format!("INSERT INTO memories ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
        params![
            m.id.to_string(),
            m.project_id.map(|p| p.to_string()),
            tag(&m.kind),
            m.title,
            m.content,
            tag(&m.status),
            tag(&m.source),
            micros(m.created_at),
            micros(m.updated_at),
            m.revision
        ],
    )
    .map_err(storage)?;
    segment::write(tx, tx.last_insert_rowid(), &m.title, &m.content)?;
    write_revision(tx, m, cause, actor)?;
    Ok(())
}

pub(crate) fn write_revision(tx: &Transaction, m: &Memory, cause: &str, actor: &str) -> Result<()> {
    let snapshot = serde_json::to_string(m).map_err(|e| Error::Internal(e.to_string()))?;
    tx.execute(
        "INSERT INTO memory_revisions (memory_id, revision, snapshot, changed_at, cause, actor) VALUES (?1,?2,?3,?4,?5,?6)",
        params![m.id.to_string(), m.revision, snapshot, micros(m.updated_at), cause, actor],
    )
    .map_err(storage)?;
    Ok(())
}

pub(crate) fn valid_title(t: String) -> Result<String> {
    let t = t.trim().to_owned();
    if t.is_empty() || t.chars().count() > MAX_TITLE_CHARS {
        return Err(Error::InvalidArgument(format!(
            "title must be 1-{MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(t)
}

pub(crate) fn valid_content(c: String) -> Result<String> {
    if c.trim().is_empty() || c.len() > MAX_CONTENT_BYTES {
        return Err(Error::InvalidArgument(format!(
            "content must be non-empty and at most {MAX_CONTENT_BYTES} bytes"
        )));
    }
    Ok(c)
}

fn clamp(limit: usize) -> i64 {
    limit.clamp(1, MAX_LIMIT) as i64
}

/// Truncated to microseconds so a value round-trips through the INTEGER column unchanged.
fn now() -> OffsetDateTime {
    from_micros(micros(OffsetDateTime::now_utc())).expect("current time in range")
}

fn micros(t: OffsetDateTime) -> i64 {
    (t.unix_timestamp_nanos() / 1000) as i64
}

fn from_micros(us: i64) -> std::result::Result<OffsetDateTime, time::error::ComponentRange> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(us) * 1000)
}

fn tag<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("unit enum serializes to a string")
}

fn conv<T, E: std::error::Error + Send + Sync + 'static>(
    col: usize,
    r: std::result::Result<T, E>,
) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(col, Type::Text, Box::new(e)))
}

fn untag<T: DeserializeOwned>(r: &Row, col: usize) -> rusqlite::Result<T> {
    conv(
        col,
        serde_json::from_value(serde_json::Value::String(r.get(col)?)),
    )
}

fn to_memory(r: &Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: conv(0, r.get::<_, String>(0)?.parse())?,
        project_id: r
            .get::<_, Option<String>>(1)?
            .map(|s| conv(1, s.parse()))
            .transpose()?,
        kind: untag(r, 2)?,
        title: r.get(3)?,
        content: r.get(4)?,
        status: untag(r, 5)?,
        source: untag(r, 6)?,
        created_at: conv(7, from_micros(r.get(7)?))?,
        updated_at: conv(8, from_micros(r.get(8)?))?,
        revision: r.get(9)?,
    })
}

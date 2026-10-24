//! `JobQueue` for SQLite. Every state change runs in an IMMEDIATE transaction, so two workers
//! (separate processes included) can never lease the same job.

use memory_core::job::{
    DEFAULT_MAX_ATTEMPTS, Enqueued, Job, JobCounts, JobQueue, JobState, MAX_ERROR_CHARS, backoff,
};
use memory_core::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{SqliteStore, begin, conv, from_micros, micros, storage, tag, untag};

const COLS: &str = "id, kind, dedup_key, payload, state, attempts, max_attempts, last_error, next_run_at, lease_id, leased_by, lease_expires_at, created_at, updated_at";
const MAX_JOBS: usize = 1000;

fn to_job(r: &Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        kind: r.get(1)?,
        dedup_key: r.get(2)?,
        payload: conv(3, serde_json::from_str(&r.get::<_, String>(3)?))?,
        state: untag(r, 4)?,
        attempts: r.get(5)?,
        max_attempts: r.get(6)?,
        last_error: r.get(7)?,
        next_run_at: conv(8, from_micros(r.get(8)?))?,
        lease_id: r.get(9)?,
        leased_by: r.get(10)?,
        lease_expires_at: r
            .get::<_, Option<i64>>(11)?
            .map(|us| conv(11, from_micros(us)))
            .transpose()?,
        created_at: conv(12, from_micros(r.get(12)?))?,
        updated_at: conv(13, from_micros(r.get(13)?))?,
    })
}

fn fetch(conn: &Connection, id: &str) -> Result<Job> {
    conn.query_row(
        &format!("SELECT {COLS} FROM jobs WHERE id=?1"),
        [id],
        to_job,
    )
    .optional()
    .map_err(storage)?
    .ok_or_else(|| Error::NotFound(format!("job {id}")))
}

/// Jobs whose lease ran out go back to the queue, or to the dead letter once their attempts are
/// used up, so a job that keeps killing its worker cannot loop forever.
fn reclaim_expired(conn: &Connection, now: OffsetDateTime) -> Result<()> {
    let at = micros(now);
    let mut stmt = conn
        .prepare(
            "
            SELECT
                id,
                attempts,
                max_attempts
            FROM jobs
            WHERE
                state='leased' AND lease_expires_at <= ?1
        ",
        )
        .map_err(storage)?;
    let expired: Vec<(String, u32, u32)> = stmt
        .query_map([at], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(storage)?
        .collect::<rusqlite::Result<_>>()
        .map_err(storage)?;
    for (id, attempts, max) in expired {
        let (state, next) = if attempts >= max {
            (JobState::DeadLetter, at)
        } else {
            (JobState::Pending, micros(now + backoff(attempts)))
        };
        conn.execute(
            "UPDATE jobs
             SET
                 state=?1,
                 next_run_at=?2,
                 last_error='lease expired (the worker may have crashed)',
                 lease_id=NULL,
                 leased_by=NULL,
                 lease_expires_at=NULL,
                 updated_at=?3
            WHERE
                id=?4",
            params![tag(&state), next, at, id],
        )
        .map_err(storage)?;
    }
    Ok(())
}

/// Shared by `complete` and `fail`: the job must still be leased under this exact lease.
fn check_lease(conn: &Connection, id: &str, lease_id: &str) -> Result<Job> {
    let job = fetch(conn, id)?;
    if job.state == JobState::Leased && job.lease_id.as_deref() == Some(lease_id) {
        Ok(job)
    } else {
        Err(Error::StaleLease(format!(
            "job {id} is {} and no longer under this lease",
            job.state.as_str()
        )))
    }
}

impl JobQueue for SqliteStore {
    fn enqueue(
        &self,
        kind: &str,
        dedup_key: &str,
        payload: serde_json::Value,
        now: OffsetDateTime,
    ) -> Result<Enqueued> {
        if kind.trim().is_empty() || dedup_key.trim().is_empty() {
            return Err(Error::InvalidArgument(
                "job kind and key must not be empty".into(),
            ));
        }
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let id = Uuid::new_v4().to_string();
        let at = micros(now);
        let added = tx
            .execute(
                "INSERT OR IGNORE INTO jobs (
                    id,
                    kind,
                    dedup_key,
                    payload,
                    state,
                    max_attempts,
                    next_run_at,
                    created_at,
                    updated_at
                )
                VALUES (?1,?2,?3,?4,'pending',?5,?6,?6,?6)",
                params![
                    id,
                    kind,
                    dedup_key,
                    payload.to_string(),
                    DEFAULT_MAX_ATTEMPTS,
                    at
                ],
            )
            .map_err(storage)?;
        let job = tx
            .query_row(
                &format!("SELECT {COLS} FROM jobs WHERE kind=?1 AND dedup_key=?2"),
                params![kind, dedup_key],
                to_job,
            )
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(Enqueued {
            job,
            created: added == 1,
        })
    }

    fn lease(&self, worker: &str, lease_for: Duration, now: OffsetDateTime) -> Result<Option<Job>> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        reclaim_expired(&tx, now)?;
        let at = micros(now);
        let due: Option<String> = tx
            .query_row(
                "SELECT id
                FROM jobs
                WHERE
                    state='pending' AND next_run_at <= ?1
                ORDER BY next_run_at, created_at, rowid
                LIMIT 1",
                [at],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let Some(id) = due else {
            tx.commit().map_err(storage)?;
            return Ok(None);
        };
        tx.execute(
            "UPDATE jobs
             SET
                 state='leased',
                 attempts=attempts+1,
                 lease_id=?1,
                 leased_by=?2,
                 lease_expires_at=?3,
                 updated_at=?4
             WHERE id=?5",
            params![
                Uuid::new_v4().to_string(),
                worker,
                micros(now + lease_for),
                at,
                id
            ],
        )
        .map_err(storage)?;
        let job = fetch(&tx, &id)?;
        tx.commit().map_err(storage)?;
        Ok(Some(job))
    }

    fn complete(&self, id: &str, lease_id: &str, now: OffsetDateTime) -> Result<Job> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        check_lease(&tx, id, lease_id)?;
        tx.execute(
            "UPDATE jobs
             SET
                 state='succeeded',
                 last_error=NULL,
                 lease_id=NULL,
                 leased_by=NULL,
                 lease_expires_at=NULL,
                 updated_at=?1
             WHERE id=?2",
            params![micros(now), id],
        )
        .map_err(storage)?;
        let job = fetch(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }

    fn fail(&self, id: &str, lease_id: &str, error: &str, now: OffsetDateTime) -> Result<Job> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let job = check_lease(&tx, id, lease_id)?;
        let (state, next) = if job.attempts >= job.max_attempts {
            (JobState::DeadLetter, micros(now))
        } else {
            (JobState::Pending, micros(now + backoff(job.attempts)))
        };
        let error: String = error.chars().take(MAX_ERROR_CHARS).collect();
        tx.execute(
            "UPDATE jobs
             SET
                 state=?1,
                 next_run_at=?2,
                 last_error=?3,
                 lease_id=NULL,
                 leased_by=NULL,
                 lease_expires_at=NULL,
                 updated_at=?4
             WHERE id=?5",
            params![tag(&state), next, error, micros(now), id],
        )
        .map_err(storage)?;
        let job = fetch(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }

    fn jobs(&self, state: Option<JobState>, limit: usize) -> Result<Vec<Job>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLS} FROM jobs WHERE (?1 IS NULL OR state=?1) ORDER BY updated_at DESC, rowid DESC LIMIT ?2"
            ))
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                params![state.map(|s| tag(&s)), limit.clamp(1, MAX_JOBS) as i64],
                to_job,
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage)
    }

    fn job(&self, id: &str) -> Result<Job> {
        fetch(&self.conn(), id)
    }

    fn counts(&self) -> Result<JobCounts> {
        let conn = self.conn();
        let n = |state: &str| -> Result<usize> {
            conn.query_row("SELECT count(*) FROM jobs WHERE state=?1", [state], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(storage)
        };
        Ok(JobCounts {
            pending: n("pending")?,
            leased: n("leased")?,
            succeeded: n("succeeded")?,
            dead_letter: n("dead_letter")?,
        })
    }

    fn retry(&self, id: &str, now: OffsetDateTime) -> Result<Job> {
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let job = fetch(&tx, id)?;
        match job.state {
            JobState::DeadLetter => tx.execute(
                "UPDATE jobs
                 SET
                     state='pending',
                     attempts=0,
                     next_run_at=?1,
                     updated_at=?1
                 WHERE id=?2",
                params![micros(now), id],
            ),
            JobState::Pending => tx.execute(
                "UPDATE jobs
                 SET
                     next_run_at=?1,
                     updated_at=?1
                 WHERE id=?2",
                params![micros(now), id],
            ),
            s => {
                return Err(Error::InvalidArgument(format!(
                    "job {id} is {} and cannot be retried",
                    s.as_str()
                )));
            }
        }
        .map_err(storage)?;
        let job = fetch(&tx, id)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }
}

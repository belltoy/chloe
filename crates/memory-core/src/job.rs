//! Background job queue types (B1-01). The queue is generic: B1 puts session extraction on it,
//! later work can add other kinds. Time is always passed in, so behaviour is testable without
//! sleeping.

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::Result;

pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;
const BACKOFF_BASE_SECS: i64 = 30;
const BACKOFF_CAP_SECS: i64 = 3600;
/// Stored errors are cut to this many characters; they must never carry memory bodies.
pub const MAX_ERROR_CHARS: usize = 2000;

/// `pending` with `attempts > 0` is what the design calls "retry": waiting out its backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Leased,
    Succeeded,
    DeadLetter,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Leased => "leased",
            Self::Succeeded => "succeeded",
            Self::DeadLetter => "dead_letter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    /// Unique per `kind`: enqueueing the same key twice yields one job.
    pub dedup_key: String,
    pub payload: serde_json::Value,
    pub state: JobState,
    /// Attempts started so far (counted when leased, so a crashed attempt counts too).
    pub attempts: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub next_run_at: OffsetDateTime,
    /// The fencing token of the current lease; `complete` and `fail` must present it.
    pub lease_id: Option<String>,
    pub leased_by: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub lease_expires_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enqueued {
    pub job: Job,
    /// `false` when a job with the same `(kind, dedup_key)` already existed (in any state).
    pub created: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCounts {
    pub pending: usize,
    pub leased: usize,
    pub succeeded: usize,
    pub dead_letter: usize,
}

/// Wait before attempt number `attempts + 1` after `attempts` failed ones: 30 s, 60 s, 120 s, ...
/// capped at one hour. No jitter: there is one machine and one worker pool.
pub fn backoff(attempts: u32) -> Duration {
    let secs = BACKOFF_BASE_SECS.saturating_mul(1i64 << attempts.saturating_sub(1).min(20));
    Duration::seconds(secs.min(BACKOFF_CAP_SECS))
}

/// Durable job queue (implemented in `memory-store`). Every method that decides something by
/// time takes `now`.
pub trait JobQueue {
    /// Idempotent per `(kind, dedup_key)`; an existing job is returned unchanged.
    fn enqueue(
        &self,
        kind: &str,
        dedup_key: &str,
        payload: serde_json::Value,
        now: OffsetDateTime,
    ) -> Result<Enqueued>;
    /// Reclaims expired leases (back to `pending` with backoff, or to `dead_letter` once the
    /// attempts are used up), then hands the oldest due job to `worker` for `lease_for`.
    fn lease(&self, worker: &str, lease_for: Duration, now: OffsetDateTime) -> Result<Option<Job>>;
    /// Like `lease`, but only jobs of `kind`: workers of one kind never take another's work.
    fn lease_kind(
        &self,
        kind: &str,
        worker: &str,
        lease_for: Duration,
        now: OffsetDateTime,
    ) -> Result<Option<Job>>;
    /// `Error::Conflict` if the lease is no longer this one (expired and reclaimed, or already
    /// finished); a late, valid result is still accepted while nobody has reclaimed the job.
    fn complete(&self, id: &str, lease_id: &str, now: OffsetDateTime) -> Result<Job>;
    /// Schedules a retry after `backoff(attempts)`, or dead-letters when attempts are used up.
    fn fail(&self, id: &str, lease_id: &str, error: &str, now: OffsetDateTime) -> Result<Job>;
    fn jobs(&self, state: Option<JobState>, limit: usize) -> Result<Vec<Job>>;
    fn job(&self, id: &str) -> Result<Job>;
    fn counts(&self) -> Result<JobCounts>;
    /// Dead letter: back to `pending` with a fresh attempt budget. Pending: run it now instead
    /// of waiting out the backoff. Leased and succeeded jobs are refused.
    fn retry(&self, id: &str, now: OffsetDateTime) -> Result<Job>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_is_capped() {
        let secs: Vec<_> = (1..=9).map(|a| backoff(a).whole_seconds()).collect();
        assert_eq!(secs, [30, 60, 120, 240, 480, 960, 1920, 3600, 3600]);
        assert_eq!(backoff(0).whole_seconds(), 30);
        assert_eq!(backoff(u32::MAX).whole_seconds(), 3600);
    }
}

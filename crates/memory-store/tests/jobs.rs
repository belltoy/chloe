use std::path::PathBuf;

use memory_core::Error;
use memory_core::job::{DEFAULT_MAX_ATTEMPTS, JobQueue, JobState, backoff};
use memory_store::SqliteStore;
use serde_json::json;
use time::{Duration, OffsetDateTime};

fn db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    (dir, path)
}

fn at(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000 + secs).unwrap()
}

const LEASE: Duration = Duration::seconds(60);

#[test]
fn enqueue_dedups_by_kind_and_key() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    let a = s.enqueue("extract", "s1", json!({"n": 1}), at(0)).unwrap();
    let b = s.enqueue("extract", "s1", json!({"n": 2}), at(1)).unwrap();
    let c = s.enqueue("other", "s1", json!({}), at(1)).unwrap();
    assert!(a.created && !b.created && c.created);
    assert_eq!(a.job.id, b.job.id);
    assert_eq!(b.job.payload, json!({"n": 1}));
    assert!(s.enqueue("", "k", json!({}), at(0)).is_err());
}

#[test]
fn leases_oldest_first_and_only_once() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    let first = s.enqueue("k", "a", json!({}), at(0)).unwrap().job;
    s.enqueue("k", "b", json!({}), at(1)).unwrap();
    let j = s.lease("w1", LEASE, at(5)).unwrap().unwrap();
    assert_eq!((j.id.as_str(), j.attempts), (first.id.as_str(), 1));
    assert_eq!(j.state, JobState::Leased);
    let j2 = s.lease("w2", LEASE, at(5)).unwrap().unwrap();
    assert_ne!(j.id, j2.id);
    assert!(s.lease("w3", LEASE, at(5)).unwrap().is_none());
    let done = s
        .complete(&j.id, j.lease_id.as_ref().unwrap(), at(6))
        .unwrap();
    assert_eq!(done.state, JobState::Succeeded);
    assert_eq!(s.counts().unwrap().succeeded, 1);
}

#[test]
fn failure_backs_off_then_dead_letters() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    s.enqueue("k", "a", json!({}), at(0)).unwrap();
    let mut now = at(0);
    for attempt in 1..=DEFAULT_MAX_ATTEMPTS {
        let j = s.lease("w", LEASE, now).unwrap().unwrap();
        assert_eq!(j.attempts, attempt);
        let long = "x".repeat(5000);
        let f = s
            .fail(&j.id, j.lease_id.as_ref().unwrap(), &long, now)
            .unwrap();
        assert_eq!(f.last_error.as_ref().unwrap().len(), 2000);
        if attempt == DEFAULT_MAX_ATTEMPTS {
            assert_eq!(f.state, JobState::DeadLetter);
            break;
        }
        assert_eq!(f.state, JobState::Pending);
        assert_eq!(f.next_run_at, now + backoff(attempt));
        // Not due one second early.
        assert!(
            s.lease("w", LEASE, f.next_run_at - Duration::seconds(1))
                .unwrap()
                .is_none()
        );
        now = f.next_run_at;
    }
    assert!(
        s.lease("w", LEASE, now + Duration::days(1))
            .unwrap()
            .is_none()
    );
    assert_eq!(s.counts().unwrap().dead_letter, 1);
}

#[test]
fn stale_lease_is_fenced() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    s.enqueue("k", "a", json!({}), at(0)).unwrap();
    let old = s.lease("w1", LEASE, at(0)).unwrap().unwrap();
    // The worker dies; after expiry another one takes over (after the backoff).
    // Reclaiming happens on the first lease call after expiry; the backoff runs from then.
    assert!(s.lease("w2", LEASE, at(60)).unwrap().is_none());
    let later = at(60) + backoff(1);
    let new = s.lease("w2", LEASE, later).unwrap().unwrap();
    assert_eq!(new.attempts, 2);
    let stale = s.complete(&old.id, old.lease_id.as_ref().unwrap(), later);
    assert!(matches!(stale, Err(Error::StaleLease(_))));
    assert!(matches!(
        s.fail(&old.id, old.lease_id.as_ref().unwrap(), "e", later),
        Err(Error::StaleLease(_))
    ));
    assert!(matches!(
        s.complete("nope", "x", later),
        Err(Error::NotFound(_))
    ));
    s.complete(&new.id, new.lease_id.as_ref().unwrap(), later)
        .unwrap();
}

#[test]
fn crashing_job_ends_in_dead_letter() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    s.enqueue("k", "a", json!({}), at(0)).unwrap();
    let mut now = at(0);
    for _ in 0..DEFAULT_MAX_ATTEMPTS {
        s.lease("w", LEASE, now).unwrap().unwrap();
        // The worker never reports back: reclaim after expiry, then wait out the backoff.
        now += Duration::hours(1);
        assert!(s.lease("w", LEASE, now).unwrap().is_none());
        now += Duration::hours(2);
    }
    assert!(s.lease("w", LEASE, now).unwrap().is_none());
    let j = &s.jobs(Some(JobState::DeadLetter), 10).unwrap()[0];
    assert_eq!(j.attempts, DEFAULT_MAX_ATTEMPTS);
    assert!(j.last_error.as_ref().unwrap().contains("lease expired"));
}

#[test]
fn retry_revives_dead_letter_only() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    let id = s.enqueue("k", "a", json!({}), at(0)).unwrap().job.id;
    s.lease("w", LEASE, at(0)).unwrap();
    assert!(matches!(
        s.retry(&id, at(1)),
        Err(Error::InvalidArgument(_))
    ));
    let mut now = at(0);
    // Burn the attempts through failures.
    let mut j = s.job(&id).unwrap();
    loop {
        s.fail(&id, j.lease_id.as_ref().unwrap(), "boom", now)
            .unwrap();
        j = s.job(&id).unwrap();
        if j.state == JobState::DeadLetter {
            break;
        }
        now = j.next_run_at;
        j = s.lease("w", LEASE, now).unwrap().unwrap();
    }
    let r = s.retry(&id, now).unwrap();
    assert_eq!((r.state, r.attempts), (JobState::Pending, 0));
    assert!(s.lease("w", LEASE, now).unwrap().is_some());
}

#[test]
fn jobs_survive_reopen() {
    let (_d, p) = db();
    let id = {
        let s = SqliteStore::open(&p).unwrap();
        let id = s.enqueue("k", "a", json!({"x": 1}), at(0)).unwrap().job.id;
        s.lease("w", LEASE, at(0)).unwrap();
        id
    };
    let s = SqliteStore::open(&p).unwrap();
    assert_eq!(s.job(&id).unwrap().payload, json!({"x": 1}));
    // The lease outlived the "crashed" process and is reclaimed once it expires.
    assert!(s.lease("w", LEASE, at(10)).unwrap().is_none());
    assert!(s.lease("w", LEASE, at(60)).unwrap().is_none()); // reclaimed, backing off
    assert!(s.lease("w", LEASE, at(60) + backoff(1)).unwrap().is_some());
}

#[test]
fn concurrent_workers_never_share_a_job() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    for i in 0..40 {
        s.enqueue("k", &i.to_string(), json!({}), at(0)).unwrap();
    }
    let handles: Vec<_> = (0..4)
        .map(|w| {
            let p = p.clone();
            std::thread::spawn(move || {
                let s = SqliteStore::open(&p).unwrap();
                let mut got = vec![];
                while let Some(j) = s.lease(&format!("w{w}"), LEASE, at(1)).unwrap() {
                    got.push(j.id);
                }
                got
            })
        })
        .collect();
    let mut all: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(all.len(), 40);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 40);
}

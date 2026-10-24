//! B2-05: repeating or interrupting consolidation, and several workers at once, must not
//! multiply candidates, memories or sources. Concurrency here is real: each thread opens its
//! own connection to the same file, like separate processes.

use std::path::{Path, PathBuf};

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, ExtractionInput, MemoryExtractor,
    Relation, RelationJudge, Resolution, Verdict,
};
use memory_core::job::JobQueue;
use memory_core::service::{MemoryService, NewMemory};
use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, Kind, Memory, Result, Source,
};
use memory_engine::{Window, apply_policy, plan, propose_pending, run_next};
use memory_store::SqliteStore;
use time::{Duration, OffsetDateTime};

/// One fixed candidate per session, so a repeated extraction gives the same candidate.
struct Fixed;

impl MemoryExtractor for Fixed {
    fn name(&self) -> String {
        "fixed".into()
    }
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>> {
        Ok(vec![Candidate {
            kind: Kind::Fact,
            title: format!("fact about {}", input.native_session_id),
            content: "c".into(),
            evidence: Evidence {
                native_agent: input.native_agent.clone(),
                native_session_id: input.native_session_id.clone(),
                events: vec![input.events[0].idempotency_key()],
                file: None,
                commit: None,
            },
        }])
    }
}

/// Calls everything a duplicate of the first memory shown.
struct AlwaysDuplicate;

impl RelationJudge for AlwaysDuplicate {
    fn judge(&self, _: &Candidate, similar: &[Memory]) -> Result<Verdict> {
        Ok(Verdict {
            relation: Relation::Duplicate,
            memory: Some(similar[0].id),
            reason: "same".into(),
        })
    }
}

fn db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    (dir, path)
}

fn session(s: &SqliteStore, id: &str) {
    s.ingest(&[
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: id.into(),
            native_event_id: Some("p".into()),
            kind: EventKind::UserPrompt,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: None,
            tool_name: None,
            text: Some("we use WAL".into()),
            redactions: vec![],
        },
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: id.into(),
            native_event_id: Some("end".into()),
            kind: EventKind::SessionEnd,
            occurred_at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(1),
            cwd: None,
            tool_name: None,
            text: None,
            redactions: vec![],
        },
    ])
    .unwrap();
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// plan, run every due job, propose, apply policy.
fn consolidate(s: &SqliteStore, judge: Option<&dyn RelationJudge>) {
    plan(s, now(), Duration::minutes(30), Window::default(), None).unwrap();
    while run_next(s, &Fixed, "w", now()).unwrap().is_some() {}
    propose_pending(s, judge).unwrap();
    apply_policy(s).unwrap();
}

fn snapshot(s: &SqliteStore) -> Vec<(String, String, String)> {
    let mut v: Vec<_> = s
        .candidates(None, 1000)
        .unwrap()
        .into_iter()
        .map(|c| (c.id, c.candidate.title, c.state.as_str().to_string()))
        .collect();
    v.sort();
    v
}

#[test]
fn repeating_consolidate_changes_nothing() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    for i in 0..3 {
        session(&s, &format!("s{i}"));
    }
    consolidate(&s, None);
    let first = snapshot(&s);
    let jobs = s.counts().unwrap();
    assert_eq!((first.len(), jobs.succeeded), (3, 3));
    for _ in 0..3 {
        consolidate(&s, None);
        assert_eq!(snapshot(&s), first);
        assert_eq!(s.counts().unwrap(), jobs);
    }
}

#[test]
fn repeating_consolidate_after_decisions_keeps_them() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    session(&s, "a");
    session(&s, "b");
    consolidate(&s, None);
    let all = s.candidates(None, 10).unwrap();
    s.resolve_candidate(
        &all[0].id,
        Resolution::Reject {
            reason: "no".into(),
        },
    )
    .unwrap();
    s.resolve_candidate(&all[1].id, Resolution::Accept { global: true })
        .unwrap();
    let before = snapshot(&s);
    for _ in 0..3 {
        consolidate(&s, Some(&AlwaysDuplicate));
    }
    assert_eq!(snapshot(&s), before);
    assert_eq!(s.list(None, true, 10).unwrap().len(), 1);
}

#[test]
fn a_crash_after_storing_candidates_but_before_completing_adds_no_duplicates() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    session(&s, "a");
    plan(&s, now(), Duration::minutes(30), Window::default(), None).unwrap();

    // The worker leases, stores its candidates, then dies before `complete`.
    let t0 = now();
    let job = s.lease("dead", Duration::minutes(10), t0).unwrap().unwrap();
    let input = ExtractionInput {
        native_agent: "claude_code".into(),
        native_session_id: "a".into(),
        events: s.session_events("a", 10).unwrap(),
    };
    s.add_candidates("claude_code", "a", &Fixed.extract(&input).unwrap())
        .unwrap();
    drop(job);
    drop(s);

    // A later run reclaims the lease (after the backoff) and extracts again.
    let s = SqliteStore::open(&p).unwrap();
    assert!(
        run_next(&s, &Fixed, "w", t0 + Duration::minutes(11))
            .unwrap()
            .is_none()
    );
    let done = run_next(&s, &Fixed, "w", t0 + Duration::hours(1))
        .unwrap()
        .unwrap();
    assert_eq!(
        done.result,
        Ok(0),
        "the repeated extraction adds nothing new"
    );
    assert_eq!(s.candidates(None, 10).unwrap().len(), 1);
    assert_eq!(s.counts().unwrap().succeeded, 1);
}

#[test]
fn two_workers_never_extract_the_same_session_twice() {
    let (_d, p) = db();
    let s = SqliteStore::open(&p).unwrap();
    for i in 0..24 {
        session(&s, &format!("s{i}"));
    }
    plan(&s, now(), Duration::minutes(30), Window::default(), None).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|w| {
            let p = p.clone();
            std::thread::spawn(move || {
                let s = SqliteStore::open(&p).unwrap();
                let mut sessions = vec![];
                while let Some(d) = run_next(&s, &Fixed, &format!("w{w}"), now()).unwrap() {
                    assert!(d.result.is_ok());
                    sessions.push(d.session);
                }
                sessions
            })
        })
        .collect();
    let mut ran: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(ran.len(), 24);
    ran.sort();
    ran.dedup();
    assert_eq!(ran.len(), 24, "a session ran twice");
    assert_eq!(s.candidates(None, 100).unwrap().len(), 24);
    assert_eq!(s.counts().unwrap().succeeded, 24);
}

fn raced_candidates(p: &Path, n: usize) -> Vec<String> {
    let s = SqliteStore::open(p).unwrap();
    for i in 0..n {
        session(&s, &format!("r{i}"));
    }
    plan(&s, now(), Duration::minutes(30), Window::default(), None).unwrap();
    while run_next(&s, &Fixed, "w", now()).unwrap().is_some() {}
    s.candidates(None, 100)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect()
}

#[test]
fn racing_reviewers_have_exactly_one_winner_per_candidate() {
    let (_d, p) = db();
    let ids = raced_candidates(&p, 12);
    let s = SqliteStore::open(&p).unwrap();
    let target = s
        .save(NewMemory {
            project_id: None,
            kind: Kind::Fact,
            title: "existing".into(),
            content: "body".into(),
            source: Source::Manual,
        })
        .unwrap();

    // Three reviewers per candidate: one accepts, one merges, one rejects.
    let handles: Vec<_> = (0..3)
        .map(|who| {
            let (p, ids) = (p.clone(), ids.clone());
            std::thread::spawn(move || {
                let s = SqliteStore::open(&p).unwrap();
                ids.iter()
                    .map(|id| {
                        let r = match who {
                            0 => Resolution::Accept { global: true },
                            1 => Resolution::Merge {
                                into: target.id,
                                reason: "dup".into(),
                            },
                            _ => Resolution::Reject {
                                reason: "no".into(),
                            },
                        };
                        s.resolve_candidate(id, r).is_ok()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let wins: Vec<Vec<bool>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for i in 0..ids.len() {
        let n = wins.iter().filter(|w| w[i]).count();
        assert_eq!(n, 1, "candidate {i} had {n} winners");
    }
    let all = s.candidates(None, 100).unwrap();
    let count = |st| all.iter().filter(|c| c.state == st).count();
    let (accepted, merged) = (
        count(CandidateState::Accepted),
        count(CandidateState::Merged),
    );
    assert_eq!(count(CandidateState::Pending), 0);
    // One new memory per accepted candidate, one source per accepted or merged one.
    assert_eq!(s.list(None, true, 100).unwrap().len(), 1 + accepted);
    let sources: usize = s
        .list(None, true, 100)
        .unwrap()
        .iter()
        .map(|m| s.memory_sources(m.id).unwrap().len())
        .sum();
    assert_eq!(sources, accepted + merged);
    assert_eq!(s.memory_sources(target.id).unwrap().len(), merged);
}

#[test]
fn concurrent_policy_runs_merge_each_candidate_once() {
    let (_d, p) = db();
    let ids = raced_candidates(&p, 10);
    let s = SqliteStore::open(&p).unwrap();
    let target = s
        .save(NewMemory {
            project_id: None,
            kind: Kind::Fact,
            title: "fact about".into(),
            content: "body".into(),
            source: Source::Manual,
        })
        .unwrap();
    propose_pending(&s, Some(&AlwaysDuplicate)).unwrap();
    assert!(
        s.candidates(None, 100)
            .unwrap()
            .iter()
            .all(|c| c.proposal.is_some())
    );

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = p.clone();
            std::thread::spawn(move || {
                apply_policy(&SqliteStore::open(&p).unwrap())
                    .unwrap()
                    .merged
            })
        })
        .collect();
    let merged: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(merged, ids.len());
    assert_eq!(s.memory_sources(target.id).unwrap().len(), ids.len());
    assert_eq!(s.list(None, true, 100).unwrap().len(), 1);
}

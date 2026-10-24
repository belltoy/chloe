use std::cell::{Cell, RefCell};

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, ExtractionInput, MemoryExtractor,
};
use memory_core::job::{DEFAULT_MAX_ATTEMPTS, JobQueue, JobState, backoff};
use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, Error, EventKind, EventStore, Kind, Result};
use memory_engine::{Window, plan, run_next, select};
use memory_store::SqliteStore;
use time::{Duration, OffsetDateTime};

fn t(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000 + secs).unwrap()
}

fn ev(session: &str, kind: EventKind, id: &str, text: Option<&str>, secs: i64) -> AgentEvent {
    AgentEvent {
        schema_version: EVENT_SCHEMA_VERSION,
        native_agent: "claude_code".into(),
        native_session_id: session.into(),
        native_event_id: Some(id.into()),
        kind,
        occurred_at: t(secs),
        cwd: None,
        tool_name: None,
        text: text.map(Into::into),
        redactions: vec![],
    }
}

/// Fake provider: replies from a script, records what it was shown.
#[derive(Default)]
struct Fake {
    down: Cell<bool>,
    seen: RefCell<Vec<usize>>,
}

impl MemoryExtractor for Fake {
    fn name(&self) -> String {
        "fake".into()
    }
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>> {
        self.seen.borrow_mut().push(input.events.len());
        if self.down.get() {
            return Err(Error::Unavailable("connection refused".into()));
        }
        Ok(vec![Candidate {
            kind: Kind::Decision,
            title: format!("decision of {}", input.native_session_id),
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

fn store() -> (tempfile::TempDir, SqliteStore) {
    let dir = tempfile::tempdir().unwrap();
    let s = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    (dir, s)
}

fn capture(s: &SqliteStore, session: &str, ended: bool, last: i64) {
    let mut v = vec![
        ev(
            session,
            EventKind::UserPrompt,
            "p",
            Some("use WAL"),
            last - 1,
        ),
        ev(session, EventKind::ToolResultSummary, "r", Some("ok"), last),
    ];
    if ended {
        v.push(ev(session, EventKind::SessionEnd, "end", None, last));
    }
    s.ingest(&v).unwrap();
}

const IDLE: Duration = Duration::minutes(30);

#[test]
fn plans_ended_and_idle_sessions_once() {
    let (_d, s) = store();
    capture(&s, "ended", true, 10);
    capture(&s, "idle", false, 10);
    capture(&s, "busy", false, 3000);
    let now = t(3000 + 5);
    let p = plan(&s, now, IDLE, Window::default(), None).unwrap();
    assert_eq!((p.enqueued, p.already_queued), (2, 0));
    let p = plan(&s, now, IDLE, Window::default(), None).unwrap();
    assert_eq!((p.enqueued, p.already_queued), (0, 2));
    // Manual: the busy session on request, and only it.
    let p = plan(&s, now, IDLE, Window::default(), Some("busy")).unwrap();
    assert_eq!((p.enqueued, p.already_queued), (1, 0));
    assert!(matches!(
        plan(&s, now, IDLE, Window::default(), Some("nope")),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn run_stores_candidates_and_completes_the_job() {
    let (_d, s) = store();
    capture(&s, "a", true, 10);
    plan(&s, t(20), IDLE, Window::default(), None).unwrap();
    let x = Fake::default();
    let done = run_next(&s, &x, "w", t(20)).unwrap().unwrap();
    assert_eq!((done.session.as_str(), done.result), ("a", Ok(1)));
    assert!(run_next(&s, &x, "w", t(20)).unwrap().is_none());
    assert_eq!(s.counts().unwrap().succeeded, 1);
    let c = s.candidates(Some(CandidateState::Pending), 10).unwrap();
    assert_eq!(c[0].candidate.title, "decision of a");
    // A repeated extraction of the same session adds nothing.
    assert_eq!(
        s.add_candidates("claude_code", "a", &[c[0].candidate.clone()])
            .unwrap(),
        0
    );
}

#[test]
fn provider_down_keeps_events_and_retries_later() {
    let (_d, s) = store();
    capture(&s, "a", true, 10);
    plan(&s, t(20), IDLE, Window::default(), None).unwrap();
    let x = Fake::default();
    x.down.set(true);
    let done = run_next(&s, &x, "w", t(20)).unwrap().unwrap();
    let f = done.result.unwrap_err();
    assert!(f.error.contains("connection refused") && !f.dead_letter);
    assert!(
        s.candidates(Some(CandidateState::Pending), 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.session_events("claude_code", "a", 10).unwrap().len(), 3);
    // Still backing off.
    assert!(run_next(&s, &x, "w", t(21)).unwrap().is_none());
    // Provider back after the backoff.
    x.down.set(false);
    let done = run_next(&s, &x, "w", t(20) + backoff(1)).unwrap().unwrap();
    assert_eq!(done.result, Ok(1));
}

#[test]
fn provider_down_for_good_ends_in_dead_letter() {
    let (_d, s) = store();
    capture(&s, "a", true, 10);
    plan(&s, t(20), IDLE, Window::default(), None).unwrap();
    let x = Fake::default();
    x.down.set(true);
    let mut now = t(20);
    let mut last = None;
    for _ in 0..DEFAULT_MAX_ATTEMPTS {
        last = Some(run_next(&s, &x, "w", now).unwrap().unwrap());
        now += Duration::days(1);
    }
    assert!(last.unwrap().result.unwrap_err().dead_letter);
    assert_eq!(s.jobs(Some(JobState::DeadLetter), 5).unwrap().len(), 1);
}

#[test]
fn session_without_text_does_not_call_the_model() {
    let (_d, s) = store();
    s.ingest(&[ev("e", EventKind::SessionEnd, "end", None, 10)])
        .unwrap();
    plan(&s, t(20), IDLE, Window::default(), None).unwrap();
    let x = Fake::default();
    assert_eq!(run_next(&s, &x, "w", t(20)).unwrap().unwrap().result, Ok(0));
    assert!(x.seen.borrow().is_empty());
}

#[test]
fn queued_work_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    {
        let s = SqliteStore::open(&path).unwrap();
        capture(&s, "a", true, 10);
        plan(&s, t(20), IDLE, Window::default(), None).unwrap();
    }
    let s = SqliteStore::open(&path).unwrap();
    let done = run_next(&s, &Fake::default(), "w", t(20)).unwrap().unwrap();
    assert_eq!(done.result, Ok(1));
}

#[test]
fn window_keeps_the_newest_events_within_both_limits() {
    let events: Vec<_> = (0..10)
        .map(|i| {
            ev(
                "s",
                EventKind::UserPrompt,
                &i.to_string(),
                Some(&"x".repeat(100)),
                i,
            )
        })
        .collect();
    let by_count = select(
        events.clone(),
        Window {
            max_events: 3,
            max_chars: 100_000,
        },
    );
    let ids: Vec<_> = by_count
        .iter()
        .map(|e| e.native_event_id.clone().unwrap())
        .collect();
    assert_eq!(ids, ["7", "8", "9"]);
    // 140 per event: 300 chars fit two.
    assert_eq!(
        select(
            events.clone(),
            Window {
                max_events: 100,
                max_chars: 300
            }
        )
        .len(),
        2
    );
    // A tiny budget still keeps the last event.
    let one = select(
        events,
        Window {
            max_events: 100,
            max_chars: 1,
        },
    );
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].native_event_id.as_deref(), Some("9"));
}

#[test]
fn the_window_in_the_job_limits_what_the_model_sees() {
    let (_d, s) = store();
    let many: Vec<_> = (0..50)
        .map(|i| {
            ev(
                "big",
                EventKind::UserPrompt,
                &i.to_string(),
                Some("work"),
                i,
            )
        })
        .collect();
    s.ingest(&many).unwrap();
    let w = Window {
        max_events: 5,
        max_chars: 20_000,
    };
    plan(&s, t(100), IDLE, w, Some("big")).unwrap();
    let x = Fake::default();
    run_next(&s, &x, "w", t(100)).unwrap();
    assert_eq!(*x.seen.borrow(), [5]);
}

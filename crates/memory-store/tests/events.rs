use std::path::PathBuf;

use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, SessionState};
use memory_store::SqliteStore;
use time::OffsetDateTime;

fn db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    (dir, path)
}

fn at(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000 + secs).unwrap()
}

fn ev(session: &str, kind: EventKind, id: Option<&str>, secs: i64) -> AgentEvent {
    AgentEvent {
        schema_version: EVENT_SCHEMA_VERSION,
        native_agent: "claude_code".into(),
        native_session_id: session.into(),
        native_event_id: id.map(Into::into),
        kind,
        occurred_at: at(secs),
        cwd: None,
        tool_name: None,
        text: None,
        redactions: vec![],
    }
}

fn session(s: &SqliteStore, native: &str) -> memory_core::SessionInfo {
    s.sessions(100)
        .unwrap()
        .into_iter()
        .find(|x| x.native_session_id == native)
        .unwrap()
}

#[test]
fn importing_the_same_capture_twice_stores_nothing_new() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let batch = vec![
        ev("s1", EventKind::SessionStart, None, 0),
        ev("s1", EventKind::UserPrompt, Some("p1"), 1),
        ev("s1", EventKind::ToolResultSummary, Some("t1"), 2),
        ev("s1", EventKind::SessionEnd, None, 3),
    ];
    let first = s.ingest(&batch).unwrap();
    assert_eq!(
        (first.inserted, first.duplicates, first.rejected.len()),
        (4, 0, 0)
    );
    let again = s.ingest(&batch).unwrap();
    assert_eq!((again.inserted, again.duplicates), (0, 4));
    assert_eq!(s.session_events("s1", 100).unwrap().len(), 4);
    assert_eq!(s.sessions(100).unwrap().len(), 1);
    assert_eq!(session(&s, "s1").events, 4);
}

#[test]
fn duplicates_inside_one_batch_and_across_sessions() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let e = ev("s1", EventKind::ToolResultSummary, Some("t1"), 0);
    let r = s.ingest(&[e.clone(), e.clone()]).unwrap();
    assert_eq!((r.inserted, r.duplicates), (1, 1));
    // Same native event id in another session, or another agent, is a different event.
    let mut other_agent = e.clone();
    other_agent.native_agent = "codex".into();
    let r = s
        .ingest(&[
            ev("s2", EventKind::ToolResultSummary, Some("t1"), 0),
            other_agent,
        ])
        .unwrap();
    assert_eq!(r.inserted, 2);
}

#[test]
fn bad_events_are_counted_and_do_not_sink_the_batch() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let mut future = ev("s1", EventKind::UserPrompt, Some("p0"), 0);
    future.schema_version = EVENT_SCHEMA_VERSION + 1;
    let mut nameless = ev("", EventKind::SessionStart, None, 0);
    nameless.native_session_id.clear();
    let mut huge = ev("s1", EventKind::UserPrompt, Some("p1"), 1);
    huge.text = Some("x".repeat(100_000));
    let good = ev("s1", EventKind::SessionStart, None, 2);
    let r = s.ingest(&[future, nameless, huge, good]).unwrap();
    assert_eq!(r.inserted, 1);
    let reasons: Vec<_> = r
        .rejected
        .iter()
        .map(|x| (x.index, x.reason.as_str()))
        .collect();
    assert_eq!(
        reasons,
        [
            (0, "unsupported_schema_version"),
            (1, "invalid_field"),
            (2, "too_long")
        ]
    );
    assert_eq!(s.session_events("s1", 100).unwrap().len(), 1);
}

#[test]
fn a_session_without_session_end_stays_open() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    s.ingest(&[
        ev("s1", EventKind::SessionStart, None, 0),
        ev("s1", EventKind::UserPrompt, Some("p1"), 1),
    ])
    .unwrap();
    let info = session(&s, "s1");
    assert_eq!(info.state, SessionState::Open);
    assert_eq!(info.last_event_at, at(1));

    s.ingest(&[ev("s1", EventKind::SessionEnd, None, 5)])
        .unwrap();
    assert_eq!(session(&s, "s1").state, SessionState::Ended);

    // A resumed session reopens; a late-arriving older event changes nothing.
    s.ingest(&[ev("s1", EventKind::UserPrompt, Some("p2"), 10)])
        .unwrap();
    assert_eq!(session(&s, "s1").state, SessionState::Open);
    s.ingest(&[ev("s1", EventKind::SessionEnd, None, 20)])
        .unwrap();
    s.ingest(&[ev("s1", EventKind::ToolResultSummary, Some("late"), 15)])
        .unwrap();
    let info = session(&s, "s1");
    assert_eq!(
        (info.state, info.last_event_at),
        (SessionState::Ended, at(20))
    );
}

#[test]
fn events_keep_their_fields_and_survive_a_restart() {
    let (_d, path) = db();
    let mut e = ev("s1", EventKind::ToolResultSummary, Some("t1"), 0);
    e.tool_name = Some("Bash".into());
    e.text = Some("echo API_KEY=[REDACTED]".into());
    e.redactions = vec!["secret".into()];
    e.cwd = Some("/nowhere/at/all".into());
    SqliteStore::open(&path)
        .unwrap()
        .ingest(&[e.clone()])
        .unwrap();
    let back = SqliteStore::open(&path)
        .unwrap()
        .session_events("s1", 10)
        .unwrap();
    assert_eq!(back, vec![e]);
}

#[test]
fn project_is_linked_only_when_the_directory_is_registered() {
    let (d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let proj = d.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let mut start = ev("s1", EventKind::SessionStart, None, 0);
    start.cwd = Some(proj.to_string_lossy().into());
    s.ingest(&[start.clone()]).unwrap();
    assert_eq!(
        session(&s, "s1").project_id,
        None,
        "ingest must not create projects"
    );
    assert!(s.sessions(10).unwrap()[0].cwd.is_some());

    // Once the project exists, the next event of the session attaches it.
    let id = s.resolve_project(&proj).unwrap();
    let mut next = ev("s1", EventKind::UserPrompt, Some("p1"), 1);
    next.cwd = start.cwd.clone();
    s.ingest(&[next]).unwrap();
    assert_eq!(session(&s, "s1").project_id, Some(id));

    // A directory that no longer exists is just "no project".
    let mut gone = ev("s2", EventKind::SessionStart, None, 0);
    gone.cwd = Some("/definitely/not/here".into());
    s.ingest(&[gone]).unwrap();
    assert_eq!(session(&s, "s2").project_id, None);
}

use std::path::Path;

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, Resolution, SourceRelation,
};
use memory_core::service::{MemoryService, NewMemory, SearchQuery};
use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, Error, EventKind, EventStore, Kind, ProjectId, Source,
};
use memory_store::SqliteStore;
use time::OffsetDateTime;

struct World {
    _dir: tempfile::TempDir,
    store: SqliteStore,
    project: ProjectId,
}

fn session(store: &SqliteStore, id: &str, cwd: Option<&Path>) {
    store
        .ingest(&[AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: id.into(),
            native_event_id: Some("e".into()),
            kind: EventKind::UserPrompt,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: cwd.map(|c| c.to_string_lossy().into()),
            tool_name: None,
            text: Some("t".into()),
            redactions: vec![],
        }])
        .unwrap();
}

fn cand(session: &str, title: &str) -> Candidate {
    Candidate {
        kind: Kind::Decision,
        title: title.into(),
        content: format!("content of {title}"),
        evidence: Evidence {
            native_agent: "claude_code".into(),
            native_session_id: session.into(),
            events: vec!["k1".into()],
            file: Some("src/a.rs".into()),
            commit: None,
        },
    }
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let project = store.resolve_project(&proj).unwrap();
    session(&store, "s", Some(&proj));
    World {
        _dir: dir,
        store,
        project,
    }
}

fn add(w: &World, title: &str) -> String {
    w.store
        .add_candidates("claude_code", "s", &[cand("s", title)])
        .unwrap();
    w.store
        .candidates(None, 100)
        .unwrap()
        .into_iter()
        .find(|c| c.candidate.title == title)
        .unwrap()
        .id
}

fn manual(w: &World, project: Option<ProjectId>) -> memory_core::Memory {
    w.store
        .save(NewMemory {
            project_id: project,
            kind: Kind::Fact,
            title: "existing".into(),
            content: "existing content".into(),
            source: Source::Manual,
        })
        .unwrap()
}

#[test]
fn accept_creates_an_extracted_memory_with_provenance() {
    let w = world();
    let id = add(&w, "Use WAL");
    let c = w
        .store
        .resolve_candidate(&id, Resolution::Accept { global: false })
        .unwrap();
    assert_eq!(c.state, CandidateState::Accepted);
    assert!(c.decided_at.is_some());
    let m = w.store.get(Some(w.project), c.memory_id.unwrap()).unwrap();
    assert_eq!(
        (m.source, m.title.as_str(), m.project_id),
        (Source::Extracted, "Use WAL", Some(w.project))
    );
    // Searchable like any memory, and the audit trail says where it came from.
    let hits = w
        .store
        .search(
            Some(w.project),
            SearchQuery {
                query: "WAL".into(),
                limit: 5,
                include_global: false,
                include_inactive: false,
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        w.store.history(Some(w.project), m.id).unwrap()[0].cause,
        "extracted"
    );
    let src = w.store.memory_sources(m.id).unwrap();
    assert_eq!(
        (src.len(), src[0].relation, src[0].candidate_id.as_str()),
        (1, SourceRelation::Created, id.as_str())
    );
    assert_eq!(src[0].evidence.file.as_deref(), Some("src/a.rs"));
}

#[test]
fn a_decision_is_final() {
    let w = world();
    let id = add(&w, "a");
    w.store
        .resolve_candidate(
            &id,
            Resolution::Reject {
                reason: "noise".into(),
            },
        )
        .unwrap();
    let c = w.store.candidate(&id).unwrap();
    assert_eq!(
        (c.state, c.reason.as_deref(), c.memory_id),
        (CandidateState::Rejected, Some("noise"), None)
    );
    for r in [
        Resolution::Accept { global: false },
        Resolution::Reject { reason: "x".into() },
    ] {
        assert!(matches!(
            w.store.resolve_candidate(&id, r),
            Err(Error::InvalidArgument(_))
        ));
    }
    assert!(matches!(
        w.store
            .resolve_candidate("nope", Resolution::Accept { global: false }),
        Err(Error::NotFound(_))
    ));
    assert_eq!(
        w.store
            .candidates(Some(CandidateState::Pending), 10)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn merge_keeps_the_memory_and_adds_a_source() {
    let w = world();
    let m = manual(&w, Some(w.project));
    let id = add(&w, "same point");
    let c = w
        .store
        .resolve_candidate(
            &id,
            Resolution::Merge {
                into: m.id,
                reason: "duplicate".into(),
            },
        )
        .unwrap();
    assert_eq!((c.state, c.memory_id), (CandidateState::Merged, Some(m.id)));
    let after = w.store.get(Some(w.project), m.id).unwrap();
    assert_eq!(
        (after.revision, after.content.as_str()),
        (1, "existing content")
    );
    let src = w.store.memory_sources(m.id).unwrap();
    assert_eq!((src.len(), src[0].relation), (1, SourceRelation::Merged));
}

#[test]
fn conflict_stays_open_for_a_human() {
    let w = world();
    let m = manual(&w, Some(w.project));
    let id = add(&w, "contradicts");
    let c = w
        .store
        .resolve_candidate(
            &id,
            Resolution::Conflict {
                with: m.id,
                reason: "says the opposite".into(),
            },
        )
        .unwrap();
    assert_eq!(
        (c.state, c.memory_id),
        (CandidateState::Conflict, Some(m.id))
    );
    assert!(c.state.is_open());
    assert_eq!(w.store.memory_sources(m.id).unwrap().len(), 0);
    let again = Resolution::Conflict {
        with: m.id,
        reason: "x".into(),
    };
    assert!(matches!(
        w.store.resolve_candidate(&id, again),
        Err(Error::InvalidArgument(_))
    ));
    // The human decides.
    let c = w
        .store
        .resolve_candidate(&id, Resolution::Accept { global: false })
        .unwrap();
    assert_eq!(c.state, CandidateState::Accepted);
    assert_ne!(c.memory_id, Some(m.id));
}

#[test]
fn other_projects_memories_are_not_found_and_nothing_changes() {
    let w = world();
    let other = w._dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let other_project = w.store.resolve_project(&other).unwrap();
    let foreign = manual(&w, Some(other_project));
    let id = add(&w, "x");
    for r in [
        Resolution::Merge {
            into: foreign.id,
            reason: "r".into(),
        },
        Resolution::Conflict {
            with: foreign.id,
            reason: "r".into(),
        },
    ] {
        assert!(matches!(
            w.store.resolve_candidate(&id, r),
            Err(Error::NotFound(_))
        ));
    }
    assert_eq!(
        w.store.candidate(&id).unwrap().state,
        CandidateState::Pending
    );
    assert!(w.store.memory_sources(foreign.id).unwrap().is_empty());
    // A global memory is fine.
    let global = manual(&w, None);
    w.store
        .resolve_candidate(
            &id,
            Resolution::Merge {
                into: global.id,
                reason: "r".into(),
            },
        )
        .unwrap();
}

#[test]
fn candidate_without_a_project_is_accepted_only_as_global() {
    let w = world();
    session(&w.store, "loose", None);
    w.store
        .add_candidates("claude_code", "loose", &[cand("loose", "loose one")])
        .unwrap();
    let id = w
        .store
        .candidates(None, 10)
        .unwrap()
        .into_iter()
        .find(|c| c.candidate.title == "loose one")
        .unwrap()
        .id;
    assert!(matches!(
        w.store
            .resolve_candidate(&id, Resolution::Accept { global: false }),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(
        w.store.candidate(&id).unwrap().state,
        CandidateState::Pending
    );
    let c = w
        .store
        .resolve_candidate(&id, Resolution::Accept { global: true })
        .unwrap();
    let m = w.store.get(None, c.memory_id.unwrap()).unwrap();
    assert_eq!(m.project_id, None);
}

#[test]
fn racing_accepts_create_exactly_one_memory() {
    let w = world();
    let id = add(&w, "race");
    let path = w._dir.path().join("t.db");
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (path, id) = (path.clone(), id.clone());
            std::thread::spawn(move || {
                SqliteStore::open(&path)
                    .unwrap()
                    .resolve_candidate(&id, Resolution::Accept { global: false })
                    .is_ok()
            })
        })
        .collect();
    let won = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(won, 1);
    assert_eq!(w.store.list(Some(w.project), false, 10).unwrap().len(), 1);
}

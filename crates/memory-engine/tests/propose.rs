use std::cell::{Cell, RefCell};

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, ProposalMethod, ProposedAction, Relation,
    RelationJudge, Verdict,
};
use memory_core::service::{MemoryService, NewMemory};
use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, Error, EventKind, EventStore, Kind, Memory, MemoryId,
    ProjectId, Result, Source,
};
use memory_engine::{ProposeReport, propose_pending};
use memory_store::SqliteStore;
use time::OffsetDateTime;

struct World {
    dir: tempfile::TempDir,
    store: SqliteStore,
    project: ProjectId,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let project = store.resolve_project(&proj).unwrap();
    store
        .ingest(&[AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            native_event_id: Some("e".into()),
            kind: EventKind::UserPrompt,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: Some(proj.to_string_lossy().into()),
            tool_name: None,
            text: Some("t".into()),
            redactions: vec![],
        }])
        .unwrap();
    World {
        dir,
        store,
        project,
    }
}

fn memory(w: &World, project: Option<ProjectId>, kind: Kind, title: &str, content: &str) -> Memory {
    w.store
        .save(NewMemory {
            project_id: project,
            kind,
            title: title.into(),
            content: content.into(),
            source: Source::Manual,
        })
        .unwrap()
}

fn candidate(w: &World, kind: Kind, title: &str, content: &str) -> String {
    let c = Candidate {
        kind,
        title: title.into(),
        content: content.into(),
        evidence: Evidence {
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            events: vec!["k".into()],
            file: None,
            commit: None,
        },
    };
    w.store.add_candidates("claude_code", "s", &[c]).unwrap();
    w.store
        .candidates(None, 100)
        .unwrap()
        .into_iter()
        .find(|s| s.candidate.title == title)
        .unwrap()
        .id
}

/// Answers with a fixed verdict (naming the first shown memory unless told otherwise).
struct Judge {
    relation: Relation,
    memory: Option<MemoryId>,
    use_first: bool,
    fail: Cell<bool>,
    calls: RefCell<usize>,
}

impl Judge {
    fn new(relation: Relation) -> Self {
        Self {
            relation,
            memory: None,
            use_first: true,
            fail: Cell::new(false),
            calls: RefCell::new(0),
        }
    }
}

impl RelationJudge for Judge {
    fn judge(&self, _: &Candidate, similar: &[Memory]) -> Result<Verdict> {
        *self.calls.borrow_mut() += 1;
        if self.fail.get() {
            return Err(Error::Unavailable("down".into()));
        }
        let memory = if self.relation == Relation::Unrelated {
            None
        } else if self.use_first {
            Some(similar[0].id)
        } else {
            self.memory
        };
        Ok(Verdict {
            relation: self.relation,
            memory,
            reason: "because".into(),
        })
    }
}

fn proposal(w: &World, id: &str) -> memory_core::extract::Proposal {
    w.store
        .candidate(id)
        .unwrap()
        .proposal
        .expect("no proposal")
}

#[test]
fn nothing_similar_proposes_create_without_asking_the_model() {
    let w = world();
    memory(
        &w,
        Some(w.project),
        Kind::Fact,
        "Deployment runs on Fridays",
        "weekly",
    );
    let id = candidate(&w, Kind::Decision, "Use WAL mode for SQLite", "concurrency");
    let j = Judge::new(Relation::Duplicate);
    let r = propose_pending(&w.store, Some(&j)).unwrap();
    assert_eq!(
        r,
        ProposeReport {
            proposed: 1,
            deferred: 0
        }
    );
    let p = proposal(&w, &id);
    assert_eq!(
        (p.action, p.target, p.method),
        (ProposedAction::Create, None, ProposalMethod::Rules)
    );
    assert!(p.similar.is_empty());
    assert_eq!(*j.calls.borrow(), 0);
    // The candidate stays pending; nothing was applied.
    assert_eq!(
        w.store.candidate(&id).unwrap().state,
        CandidateState::Pending
    );
}

#[test]
fn same_kind_and_title_is_a_rule_based_merge_proposal() {
    let w = world();
    let m = memory(
        &w,
        Some(w.project),
        Kind::Decision,
        "Retry init for five seconds",
        "details",
    );
    let id = candidate(
        &w,
        Kind::Decision,
        "Retry init() for five seconds",
        "other words",
    );
    let j = Judge::new(Relation::Contradicts);
    propose_pending(&w.store, Some(&j)).unwrap();
    let p = proposal(&w, &id);
    assert_eq!(
        (p.action, p.target, p.method),
        (ProposedAction::Merge, Some(m.id), ProposalMethod::Rules)
    );
    assert!(
        p.reason.contains("Retry init for five seconds"),
        "{}",
        p.reason
    );
    assert_eq!(p.similar[0].memory_id, m.id);
    assert_eq!(*j.calls.borrow(), 0);
}

#[test]
fn same_title_with_another_kind_is_not_merged_by_rule() {
    let w = world();
    memory(
        &w,
        Some(w.project),
        Kind::Fact,
        "Retry init for five seconds",
        "details",
    );
    let id = candidate(&w, Kind::Decision, "Retry init for five seconds", "details");
    propose_pending(&w.store, None).unwrap();
    assert_eq!(proposal(&w, &id).action, ProposedAction::Create);
}

#[test]
fn without_a_model_similar_memories_are_listed_not_decided() {
    let w = world();
    memory(
        &w,
        Some(w.project),
        Kind::Fact,
        "SQLite journal uses WAL",
        "write ahead log",
    );
    let id = candidate(
        &w,
        Kind::Decision,
        "Switch the journal to WAL",
        "SQLite write ahead log",
    );
    propose_pending(&w.store, None).unwrap();
    let p = proposal(&w, &id);
    assert_eq!(
        (p.action, p.method),
        (ProposedAction::Create, ProposalMethod::Rules)
    );
    assert_eq!(p.similar.len(), 1);
    assert!(p.reason.contains("no model"), "{}", p.reason);
}

#[test]
fn the_model_decides_between_duplicate_conflict_and_unrelated() {
    for (relation, action) in [
        (Relation::Duplicate, ProposedAction::Merge),
        (Relation::Contradicts, ProposedAction::Conflict),
        (Relation::Unrelated, ProposedAction::Create),
    ] {
        let w = world();
        let m = memory(
            &w,
            Some(w.project),
            Kind::Fact,
            "SQLite journal uses WAL",
            "write ahead log",
        );
        let id = candidate(
            &w,
            Kind::Decision,
            "Switch the journal to WAL",
            "SQLite write ahead log",
        );
        propose_pending(&w.store, Some(&Judge::new(relation))).unwrap();
        let p = proposal(&w, &id);
        assert_eq!(
            (p.action, p.method, p.reason.as_str()),
            (action, ProposalMethod::Model, "because")
        );
        assert_eq!(p.target, (action != ProposedAction::Create).then_some(m.id));
    }
}

#[test]
fn a_memory_the_model_was_not_shown_is_ignored() {
    let w = world();
    memory(
        &w,
        Some(w.project),
        Kind::Fact,
        "SQLite journal uses WAL",
        "write ahead log",
    );
    let id = candidate(
        &w,
        Kind::Decision,
        "Switch the journal to WAL",
        "SQLite write ahead log",
    );
    let mut j = Judge::new(Relation::Duplicate);
    j.use_first = false;
    j.memory = Some(MemoryId::new());
    propose_pending(&w.store, Some(&j)).unwrap();
    let p = proposal(&w, &id);
    assert_eq!((p.action, p.target), (ProposedAction::Create, None));
}

#[test]
fn a_failing_model_defers_and_the_next_run_proposes() {
    let w = world();
    memory(
        &w,
        Some(w.project),
        Kind::Fact,
        "SQLite journal uses WAL",
        "write ahead log",
    );
    let id = candidate(
        &w,
        Kind::Decision,
        "Switch the journal to WAL",
        "SQLite write ahead log",
    );
    let j = Judge::new(Relation::Duplicate);
    j.fail.set(true);
    assert_eq!(
        propose_pending(&w.store, Some(&j)).unwrap(),
        ProposeReport {
            proposed: 0,
            deferred: 1
        }
    );
    assert!(w.store.candidate(&id).unwrap().proposal.is_none());
    j.fail.set(false);
    assert_eq!(
        propose_pending(&w.store, Some(&j)).unwrap(),
        ProposeReport {
            proposed: 1,
            deferred: 0
        }
    );
    // Already proposed: not asked again.
    let calls = *j.calls.borrow();
    assert_eq!(
        propose_pending(&w.store, Some(&j)).unwrap(),
        ProposeReport::default()
    );
    assert_eq!(*j.calls.borrow(), calls);
}

#[test]
fn only_active_memories_of_the_project_or_global_count() {
    let w = world();
    let other = w.dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let other_project = w.store.resolve_project(&other).unwrap();
    memory(
        &w,
        Some(other_project),
        Kind::Decision,
        "Switch the journal to WAL",
        "foreign",
    );
    let archived = memory(
        &w,
        Some(w.project),
        Kind::Decision,
        "Switch the journal to WAL",
        "old",
    );
    w.store.archive(Some(w.project), archived.id, 1).unwrap();
    let id = candidate(&w, Kind::Decision, "Switch the journal to WAL", "again");
    propose_pending(&w.store, None).unwrap();
    assert!(proposal(&w, &id).similar.is_empty());

    // A global memory is compared.
    let global = memory(
        &w,
        None,
        Kind::Decision,
        "Switch the journal to WAL",
        "global",
    );
    let id2 = candidate(&w, Kind::Decision, "Switch the journal to WAL now", "again");
    propose_pending(&w.store, None).unwrap();
    let p = proposal(&w, &id2);
    assert_eq!(
        (p.action, p.target),
        (ProposedAction::Merge, Some(global.id))
    );
}

#[test]
fn a_decided_candidate_takes_no_proposal() {
    let w = world();
    let id = candidate(&w, Kind::Fact, "x y z", "c");
    w.store
        .resolve_candidate(
            &id,
            memory_core::extract::Resolution::Reject { reason: "r".into() },
        )
        .unwrap();
    assert_eq!(
        propose_pending(&w.store, None).unwrap(),
        ProposeReport::default()
    );
    let p = memory_core::extract::Proposal {
        action: ProposedAction::Create,
        target: None,
        method: ProposalMethod::Rules,
        reason: "r".into(),
        similar: vec![],
    };
    assert!(matches!(
        w.store.set_proposal(&id, &p),
        Err(Error::InvalidArgument(_))
    ));
}

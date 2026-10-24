use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, Proposal, ProposalMethod, ProposedAction,
    SourceRelation,
};
use memory_core::service::{MemoryService, NewMemory};
use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, Kind, Memory, MemoryId, ProjectId,
    Source,
};
use memory_engine::{PolicyReport, apply_policy};
use memory_store::SqliteStore;
use time::OffsetDateTime;

struct World {
    _dir: tempfile::TempDir,
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
        _dir: dir,
        store,
        project,
    }
}

fn memory(w: &World, kind: Kind) -> Memory {
    w.store
        .save(NewMemory {
            project_id: Some(w.project),
            kind,
            title: "existing".into(),
            content: "existing content".into(),
            source: Source::Manual,
        })
        .unwrap()
}

/// A pending candidate of `kind` carrying the given proposal.
fn candidate(w: &World, kind: Kind, action: ProposedAction, target: Option<MemoryId>) -> String {
    let title = format!(
        "cand {kind:?} {action:?} {}",
        w.store.candidates(None, 1000).unwrap().len()
    );
    let c = Candidate {
        kind,
        title: title.clone(),
        content: "c".into(),
        evidence: Evidence {
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            events: vec!["k".into()],
            file: None,
            commit: None,
        },
    };
    w.store.add_candidates("claude_code", "s", &[c]).unwrap();
    let id = w
        .store
        .candidates(None, 1000)
        .unwrap()
        .into_iter()
        .find(|s| s.candidate.title == title)
        .unwrap()
        .id;
    let p = Proposal {
        action,
        target,
        method: ProposalMethod::Model,
        reason: "same point".into(),
        similar: vec![],
        needs_review: None,
    };
    w.store.set_proposal(&id, &p).unwrap();
    id
}

fn state(w: &World, id: &str) -> CandidateState {
    w.store.candidate(id).unwrap().state
}

#[test]
fn low_risk_duplicate_is_merged_and_the_memory_is_untouched() {
    for kind in [Kind::Fact, Kind::Procedure, Kind::Gotcha] {
        let w = world();
        let m = memory(&w, kind);
        let id = candidate(&w, kind, ProposedAction::Merge, Some(m.id));
        let r = apply_policy(&w.store).unwrap();
        assert_eq!(
            r,
            PolicyReport {
                merged: 1,
                flagged: 0,
                needs_review: 0
            }
        );
        let c = w.store.candidate(&id).unwrap();
        assert_eq!((c.state, c.memory_id), (CandidateState::Merged, Some(m.id)));
        assert!(c.reason.unwrap().starts_with("auto-merged"));
        assert_eq!(w.store.get(Some(w.project), m.id).unwrap().revision, 1);
        assert_eq!(
            w.store.memory_sources(m.id).unwrap()[0].relation,
            SourceRelation::Merged
        );
    }
}

#[test]
fn constraints_and_decisions_are_never_merged_automatically() {
    // (candidate kind, memory kind)
    for (ck, mk) in [
        (Kind::Constraint, Kind::Constraint),
        (Kind::Decision, Kind::Decision),
        (Kind::Fact, Kind::Constraint),
        (Kind::Gotcha, Kind::Decision),
        (Kind::Constraint, Kind::Fact),
    ] {
        let w = world();
        let m = memory(&w, mk);
        let id = candidate(&w, ck, ProposedAction::Merge, Some(m.id));
        let r = apply_policy(&w.store).unwrap();
        assert_eq!(
            r,
            PolicyReport {
                merged: 0,
                flagged: 0,
                needs_review: 1
            },
            "{ck:?}/{mk:?}"
        );
        let c = w.store.candidate(&id).unwrap();
        assert_eq!(c.state, CandidateState::Pending);
        assert!(c.proposal.unwrap().needs_review.is_some());
        assert!(w.store.memory_sources(m.id).unwrap().is_empty());
        // Running again changes nothing.
        assert_eq!(apply_policy(&w.store).unwrap().merged, 0);
    }
}

#[test]
fn other_blockers_wait_for_a_person() {
    // Different kinds.
    let w = world();
    let m = memory(&w, Kind::Fact);
    let id = candidate(&w, Kind::Gotcha, ProposedAction::Merge, Some(m.id));
    apply_policy(&w.store).unwrap();
    assert_eq!(state(&w, &id), CandidateState::Pending);
    // Archived target.
    let w = world();
    let m = memory(&w, Kind::Fact);
    let id = candidate(&w, Kind::Fact, ProposedAction::Merge, Some(m.id));
    w.store.archive(Some(w.project), m.id, 1).unwrap();
    apply_policy(&w.store).unwrap();
    assert_eq!(state(&w, &id), CandidateState::Pending);
}

#[test]
fn a_conflict_is_flagged_but_nothing_else_changes() {
    for mk in [Kind::Decision, Kind::Fact] {
        let w = world();
        let m = memory(&w, mk);
        let id = candidate(&w, Kind::Fact, ProposedAction::Conflict, Some(m.id));
        let r = apply_policy(&w.store).unwrap();
        assert_eq!(
            r,
            PolicyReport {
                merged: 0,
                flagged: 1,
                needs_review: 0
            }
        );
        let c = w.store.candidate(&id).unwrap();
        assert_eq!(
            (c.state, c.memory_id),
            (CandidateState::Conflict, Some(m.id))
        );
        assert!(c.state.is_open());
        assert_eq!(w.store.get(Some(w.project), m.id).unwrap().revision, 1);
        assert!(w.store.memory_sources(m.id).unwrap().is_empty());
        // A flagged candidate is not processed again.
        assert_eq!(apply_policy(&w.store).unwrap(), PolicyReport::default());
    }
}

#[test]
fn creating_a_memory_is_never_automatic() {
    let w = world();
    let id = candidate(&w, Kind::Fact, ProposedAction::Create, None);
    assert_eq!(apply_policy(&w.store).unwrap(), PolicyReport::default());
    assert_eq!(state(&w, &id), CandidateState::Pending);
    assert!(w.store.list(Some(w.project), true, 10).unwrap().is_empty());
}

#[test]
fn a_candidate_without_a_proposal_is_left_alone() {
    let w = world();
    let c = Candidate {
        kind: Kind::Fact,
        title: "bare".into(),
        content: "c".into(),
        evidence: Evidence {
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            events: vec![],
            file: None,
            commit: None,
        },
    };
    w.store.add_candidates("claude_code", "s", &[c]).unwrap();
    assert_eq!(apply_policy(&w.store).unwrap(), PolicyReport::default());
}

#[test]
fn superseding_is_never_applied_automatically() {
    for (mk, guarded) in [
        (Kind::Fact, false),
        (Kind::Decision, true),
        (Kind::Constraint, true),
    ] {
        let w = world();
        let m = memory(&w, mk);
        let id = candidate(&w, Kind::Fact, ProposedAction::Supersede, Some(m.id));
        let r = apply_policy(&w.store).unwrap();
        assert_eq!(
            r,
            PolicyReport {
                merged: 0,
                flagged: 0,
                needs_review: 1
            },
            "{mk:?}"
        );
        let c = w.store.candidate(&id).unwrap();
        assert_eq!(c.state, CandidateState::Pending);
        let why = c.proposal.unwrap().needs_review.unwrap();
        assert_eq!(why.contains("constraint or decision"), guarded, "{why}");
        let still = w.store.get(Some(w.project), m.id).unwrap();
        assert_eq!(
            (still.status, still.revision),
            (memory_core::Status::Active, 1)
        );
    }
}

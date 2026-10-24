//! Hand-designed conflict samples (B severe-conflict overwrites, B-09). Each sample is an
//! existing Active memory and a new candidate that contradicts, replaces or restates it, run
//! through the real proposal step and the real safety policy with a model verdict that is
//! *given* (sometimes a wrong one: an unreliable model must not be able to overwrite anything).
//! The invariant for every sample: the existing memory is untouched (status, revision, text)
//! and no second Active memory appears. `expect` is where the candidate ends up.
//! The `#[ignore]`d test asks a live local model what it would say about the same pairs:
//! `cargo test -p memory-engine --test conflicts -- --ignored --nocapture`.

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, Evidence, Relation, RelationJudge, Verdict,
};
use memory_core::service::{MemoryService, NewMemory};
use memory_core::{
    AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, Kind, Memory, Result, Source, Status,
};
use memory_engine::{apply_policy, propose_pending};
use memory_store::SqliteStore;
use time::OffsetDateTime;

/// What the (stubbed) model says; always about the first memory shown.
struct Says(Relation);

impl RelationJudge for Says {
    fn judge(&self, _: &Candidate, similar: &[Memory]) -> Result<Verdict> {
        Ok(Verdict {
            relation: self.0,
            memory: (self.0 != Relation::Unrelated).then(|| similar[0].id),
            reason: "stub".into(),
        })
    }
}

struct Sample {
    name: &'static str,
    existing: (Kind, &'static str, &'static str),
    candidate: (Kind, &'static str, &'static str),
    /// The verdict the stub returns.
    says: Relation,
    /// The relation a careful reader would pick (used by the live comparison only).
    ideal: Relation,
    /// Where the candidate may end up. Anything else fails.
    expect: &'static [CandidateState],
}

use CandidateState::{Conflict, Merged, Pending};
use Relation::{Contradicts, Duplicate, Supersedes, Unrelated};

#[rustfmt::skip]
const SAMPLES: &[Sample] = &[
    Sample { name: "01 constraint vs a contradicting constraint, judged contradicts",
        existing: (Kind::Constraint, "No network listeners by default", "The service must not open a network listener unless the user enables it."),
        candidate: (Kind::Constraint, "Expose an HTTP listener on 0.0.0.0:8080", "The service listens on 0.0.0.0:8080 for network clients by default."),
        says: Contradicts, ideal: Contradicts, expect: &[Conflict] },
    Sample { name: "02 same pair, model wrongly says duplicate",
        existing: (Kind::Constraint, "No network listeners by default", "The service must not open a network listener unless the user enables it."),
        candidate: (Kind::Constraint, "Expose an HTTP listener on 0.0.0.0:8080", "The service listens on 0.0.0.0:8080 for network clients by default."),
        // The candidate carries a port number the memory lacks, so the duplicate verdict becomes a conflict.
        says: Duplicate, ideal: Contradicts, expect: &[Conflict] },
    Sample { name: "03 decision vs a decision that replaces it, judged supersedes",
        existing: (Kind::Decision, "Use SQLite, no daemon", "We keep SQLite as the only store and run no daemon."),
        candidate: (Kind::Decision, "Move the store to Postgres", "We now use Postgres instead of SQLite as the store, running a daemon."),
        says: Supersedes, ideal: Supersedes, expect: &[Pending] },
    Sample { name: "04 decision vs a fact that restates it differently, wrongly judged duplicate",
        existing: (Kind::Decision, "Use SQLite, no daemon", "We keep SQLite as the only store and run no daemon."),
        candidate: (Kind::Fact, "Postgres is the production store", "The production store is Postgres behind a daemon."),
        says: Duplicate, ideal: Contradicts, expect: &[Pending] },
    Sample { name: "05 fact with another day, wrongly judged duplicate",
        existing: (Kind::Fact, "Staging DB is read-only on Fridays", "The staging database is read-only on Fridays 02:00-04:00 UTC because of backups."),
        candidate: (Kind::Fact, "Staging DB is read-only on Saturdays", "The staging database is read-only on Saturdays 02:00-04:00 UTC because of backups."),
        says: Duplicate, ideal: Supersedes, expect: &[Conflict] },
    Sample { name: "06 fact with another version, judged supersedes",
        existing: (Kind::Fact, "CI image runs Python 3.9", "The CI image runs Python 3.9, so no match statements."),
        candidate: (Kind::Fact, "CI image runs Python 3.12", "The CI image was upgraded and runs Python 3.12, so match statements work."),
        says: Supersedes, ideal: Supersedes, expect: &[Pending] },
    Sample { name: "07 constraint vs a gotcha that licenses what it forbids, judged contradicts",
        existing: (Kind::Constraint, "Never log memory bodies", "Logs must never contain memory bodies or secrets."),
        candidate: (Kind::Gotcha, "Logging memory bodies is fine for debugging", "Logging memory bodies helped debugging and is fine to leave on."),
        says: Contradicts, ideal: Contradicts, expect: &[Conflict] },
    Sample { name: "08 procedure vs its opposite, judged contradicts",
        existing: (Kind::Procedure, "Sign before publishing", "Run make sign first, then make publish."),
        candidate: (Kind::Procedure, "Publish without signing", "Run make publish alone; signing is not needed."),
        says: Contradicts, ideal: Contradicts, expect: &[Conflict] },
    Sample { name: "09 Chinese constraint vs its opposite, judged contradicts",
        existing: (Kind::Constraint, "对外调用必须显式设置超时", "所有对外调用必须显式设置超时,不允许依赖默认值。"),
        candidate: (Kind::Fact, "对外调用可以使用默认超时", "对外调用可以直接使用默认超时,无需显式设置。"),
        says: Contradicts, ideal: Contradicts, expect: &[Conflict] },
    Sample { name: "10 constraint vs an unrelated-looking candidate, wrongly judged supersedes",
        existing: (Kind::Constraint, "Defaults are local-only", "No remote LLM unless explicitly enabled; no network listeners."),
        candidate: (Kind::Decision, "Send session text to a hosted model", "Session text is sent to a hosted LLM service for extraction."),
        says: Supersedes, ideal: Contradicts, expect: &[Pending] },
    Sample { name: "11 gotcha with the opposite claim, wrongly judged duplicate (known gap)",
        existing: (Kind::Gotcha, "SQLite NOT NULL column needs a default", "ALTER TABLE ADD COLUMN with NOT NULL fails on SQLite unless a DEFAULT is given."),
        candidate: (Kind::Gotcha, "SQLite NOT NULL column needs no default", "ALTER TABLE ADD COLUMN with NOT NULL works on SQLite without any DEFAULT."),
        // Polarity is not checked for the same kind of low-risk note, so this is merged as a
        // source. The existing memory is still untouched, but the contradiction is not surfaced.
        says: Duplicate, ideal: Contradicts, expect: &[Merged] },
    Sample { name: "12 decision vs a model verdict of unrelated",
        existing: (Kind::Decision, "Use SQLite, no daemon", "We keep SQLite as the only store and run no daemon."),
        candidate: (Kind::Decision, "Run a background daemon", "We run a background daemon that owns the SQLite store."),
        says: Unrelated, ideal: Contradicts, expect: &[Pending] },
];

fn world() -> (tempfile::TempDir, SqliteStore, memory_core::ProjectId) {
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
    (dir, store, project)
}

fn save(s: &SqliteStore, p: memory_core::ProjectId, m: (Kind, &str, &str)) -> Memory {
    s.save(NewMemory {
        project_id: Some(p),
        kind: m.0,
        title: m.1.into(),
        content: m.2.into(),
        source: Source::Manual,
    })
    .unwrap()
}

fn add_candidate(s: &SqliteStore, c: (Kind, &str, &str)) -> String {
    s.add_candidates(
        "claude_code",
        "s",
        &[Candidate {
            kind: c.0,
            title: c.1.into(),
            content: c.2.into(),
            evidence: Evidence {
                native_agent: "claude_code".into(),
                native_session_id: "s".into(),
                events: vec!["k".into()],
                file: None,
                commit: None,
            },
        }],
    )
    .unwrap();
    s.candidates(None, 10).unwrap().remove(0).id
}

fn untouched(before: &Memory, after: &Memory) -> bool {
    (
        before.status,
        before.revision,
        &before.title,
        &before.content,
        before.updated_at,
    ) == (
        after.status,
        after.revision,
        &after.title,
        &after.content,
        after.updated_at,
    )
}

#[test]
fn no_sample_overwrites_or_duplicates_an_active_memory() {
    assert!(SAMPLES.len() >= 10);
    let mut wrong: Vec<String> = vec![];
    for s in SAMPLES {
        let (_d, store, project) = world();
        let before = save(&store, project, s.existing);
        let id = add_candidate(&store, s.candidate);
        propose_pending(&store, Some(&Says(s.says))).unwrap();
        apply_policy(&store).unwrap();

        let after = store.get(Some(project), before.id).unwrap();
        assert!(
            untouched(&before, &after),
            "{}: the existing memory changed",
            s.name
        );
        assert_eq!(after.status, Status::Active, "{}", s.name);
        let active = store.list(Some(project), false, 100).unwrap();
        assert_eq!(
            active.len(),
            1,
            "{}: a second Active memory appeared",
            s.name
        );
        let state = store.candidate(&id).unwrap().state;
        if !s.expect.contains(&state) {
            wrong.push(format!("{}: candidate ended {state:?}", s.name));
        }
        // Whatever happened, a person can still see and decide it.
        if state == Pending || state == Conflict {
            assert!(state.is_open(), "{}", s.name);
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_contradicting_candidate_in_another_project_cannot_reach_this_projects_memory() {
    let (d, store, project) = world();
    let before = save(
        &store,
        project,
        (
            Kind::Constraint,
            "No network listeners by default",
            "The service must not open a network listener.",
        ),
    );
    let other_dir = d.path().join("other");
    std::fs::create_dir(&other_dir).unwrap();
    store.resolve_project(&other_dir).unwrap();
    store
        .ingest(&[AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: "o".into(),
            native_event_id: Some("e".into()),
            kind: EventKind::UserPrompt,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: Some(other_dir.to_string_lossy().into()),
            tool_name: None,
            text: Some("t".into()),
            redactions: vec![],
        }])
        .unwrap();
    store
        .add_candidates(
            "claude_code",
            "o",
            &[Candidate {
                kind: Kind::Constraint,
                title: "Expose a network listener".into(),
                content: "The service must open a network listener for clients.".into(),
                evidence: Evidence {
                    native_agent: "claude_code".into(),
                    native_session_id: "o".into(),
                    events: vec!["k".into()],
                    file: None,
                    commit: None,
                },
            }],
        )
        .unwrap();
    propose_pending(&store, Some(&Says(Contradicts))).unwrap();
    apply_policy(&store).unwrap();
    let after = store.get(Some(project), before.id).unwrap();
    assert!(untouched(&before, &after));
    let c = store.candidates(None, 10).unwrap().remove(0);
    // No similar memory is visible from the other project, so there is nothing to conflict with.
    assert_eq!(c.state, Pending);
    assert!(c.proposal.unwrap().target.is_none());
}

#[test]
#[ignore = "needs a running Ollama with the model pulled"]
fn what_a_live_model_says_about_the_same_pairs() {
    let mut cfg = memory_llm::OllamaConfig::default();
    cfg.base_url = std::env::var("CHLOE_OLLAMA_URL").unwrap_or(cfg.base_url);
    cfg.model = std::env::var("CHLOE_LLM_MODEL").unwrap_or(cfg.model);
    let model_name = cfg.model.clone();
    let model = memory_llm::Ollama::new(cfg).unwrap();
    let mut right = 0;
    let mut out = format!(
        "# Live judge on the conflict samples ({model_name})\n\n| sample | ideal | model said | action after policy |\n|---|---|---|---|\n"
    );
    for s in SAMPLES {
        let (_d, store, project) = world();
        let before = save(&store, project, s.existing);
        let id = add_candidate(&store, s.candidate);
        propose_pending(&store, Some(&model)).unwrap();
        apply_policy(&store).unwrap();
        let c = store.candidate(&id).unwrap();
        let said = c
            .proposal
            .as_ref()
            .map_or("none".to_string(), |p| format!("{:?}", p.action));
        let ideal_action = match s.ideal {
            Duplicate => "Merge",
            Contradicts => "Conflict",
            Supersedes => "Supersede",
            Unrelated => "Create",
        };
        right += (said == ideal_action) as usize;
        let after = store.get(Some(project), before.id).unwrap();
        assert!(
            untouched(&before, &after),
            "{}: the existing memory changed",
            s.name
        );
        out.push_str(&format!(
            "| {} | {} | {} | {:?} |\n",
            s.name, ideal_action, said, c.state
        ));
    }
    out.push_str(&format!("\nThe model chose the ideal relation in {right} of {} samples; the existing memory was untouched in all.\n", SAMPLES.len()));
    println!("{out}");
    if let Ok(p) = std::env::var("CHLOE_CONFLICT_REPORT") {
        std::fs::write(p, out).unwrap();
    }
}

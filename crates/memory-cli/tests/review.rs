use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use memory_core::extract::{Candidate, CandidateStore, Evidence};
use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, Kind};
use memory_store::SqliteStore;
use serde_json::Value;
use time::OffsetDateTime;

struct Env {
    _tmp: tempfile::TempDir,
    db: PathBuf,
    proj: PathBuf,
}

fn chloe(e: &Env, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chloe"))
        .env_remove("CHLOE_DB")
        .arg("--db-path")
        .arg(&e.db)
        .arg("--dir")
        .arg(&e.proj)
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}

fn ok(e: &Env, args: &[&str]) -> Value {
    let out = chloe(e, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// A project with one captured session and the given candidate titles.
fn env(titles: &[&str], with_project: bool) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let (db, proj) = (tmp.path().join("t.db"), tmp.path().join("p"));
    std::fs::create_dir(&proj).unwrap();
    let store = SqliteStore::open(&db).unwrap();
    if with_project {
        store.resolve_project(&proj).unwrap();
    }
    let cwd: Option<&Path> = with_project.then_some(proj.as_path());
    store
        .ingest(&[AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            native_event_id: Some("e".into()),
            kind: EventKind::UserPrompt,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: cwd.map(|c| c.to_string_lossy().into()),
            tool_name: None,
            text: Some("t".into()),
            redactions: vec![],
        }])
        .unwrap();
    let cands: Vec<_> = titles
        .iter()
        .map(|t| Candidate {
            kind: Kind::Decision,
            title: (*t).into(),
            content: format!("content of {t}"),
            evidence: Evidence {
                native_agent: "claude_code".into(),
                native_session_id: "s".into(),
                events: vec!["key1".into()],
                file: Some("src/a.rs".into()),
                commit: None,
            },
        })
        .collect();
    store.add_candidates("claude_code", "s", &cands).unwrap();
    Env {
        _tmp: tmp,
        db,
        proj,
    }
}

fn id_of(e: &Env, title: &str) -> String {
    ok(e, &["candidates", "list", "--state", "all"])
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["title"] == title)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn accept_creates_a_memory_with_traceable_sources() {
    let e = env(&["use WAL"], true);
    let id = id_of(&e, "use WAL");
    let shown = ok(&e, &["candidates", "show", &id]);
    assert_eq!(
        (shown["state"].as_str(), shown["evidence"]["file"].as_str()),
        (Some("pending"), Some("src/a.rs"))
    );

    let done = ok(&e, &["candidates", "accept", &id]);
    assert_eq!(done["state"], "accepted");
    let mem = done["memory_id"].as_str().unwrap();

    let m = ok(&e, &["memory", "show", mem]);
    assert_eq!(
        (m["source"].as_str(), m["title"].as_str()),
        (Some("extracted"), Some("use WAL"))
    );
    let hist = ok(&e, &["memory", "history", mem]);
    assert_eq!(hist[0]["cause"], "extracted");
    let src = ok(&e, &["memory", "sources", mem]);
    assert_eq!(
        (src[0]["relation"].as_str(), src[0]["candidate_id"].as_str()),
        (Some("created"), Some(id.as_str()))
    );

    // No longer open, and a second decision is refused with an error code.
    assert_eq!(ok(&e, &["candidates", "list"]).as_array().unwrap().len(), 0);
    let again = chloe(&e, &["candidates", "reject", &id]);
    assert!(!again.status.success());
    let err: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(err["error"]["code"], "invalid_argument");
}

#[test]
fn reject_and_merge() {
    let e = env(&["noise", "same point"], true);
    let rejected = ok(
        &e,
        &[
            "candidates",
            "reject",
            &id_of(&e, "noise"),
            "--reason",
            "not useful",
        ],
    );
    assert_eq!(
        (rejected["state"].as_str(), rejected["reason"].as_str()),
        (Some("rejected"), Some("not useful"))
    );

    let existing = ok(
        &e,
        &["memory", "add", "--title", "existing", "--content", "body"],
    );
    let mid = existing["id"].as_str().unwrap();
    let merged = ok(
        &e,
        &[
            "candidates",
            "merge",
            &id_of(&e, "same point"),
            "--into",
            mid,
        ],
    );
    assert_eq!(
        (merged["state"].as_str(), merged["memory_id"].as_str()),
        (Some("merged"), Some(mid))
    );
    // The memory itself is unchanged; the trace is in its sources.
    assert_eq!(ok(&e, &["memory", "show", mid])["revision"], 1);
    assert_eq!(ok(&e, &["memory", "sources", mid])[0]["relation"], "merged");

    let all = ok(&e, &["candidates", "list", "--state", "all"]);
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(
        ok(&e, &["candidates", "list", "--state", "rejected"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !chloe(&e, &["candidates", "list", "--state", "bogus"])
            .status
            .success()
    );
}

#[test]
fn candidate_without_a_project_needs_global() {
    let e = env(&["loose"], false);
    let id = id_of(&e, "loose");
    let out = chloe(&e, &["candidates", "accept", &id]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("global"));
    let done = ok(&e, &["candidates", "accept", &id, "--global"]);
    assert_eq!(done["state"], "accepted");
}

#[test]
fn unknown_ids_are_not_found() {
    let e = env(&[], true);
    let out = chloe(&e, &["candidates", "show", "nope"]);
    let err: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(err["error"]["code"], "not_found");
}

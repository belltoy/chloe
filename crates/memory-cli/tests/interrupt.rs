//! B2-05 with real processes: kill -9 in the middle of a consolidation, and a repeated run.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use memory_core::Result;
use memory_core::extract::{Candidate, CandidateStore, Evidence, ExtractionInput, MemoryExtractor};
use memory_core::job::JobQueue;
use memory_engine::run_next;
use memory_store::SqliteStore;
use serde_json::{Value, json};
use time::OffsetDateTime;

fn cmd(db: &Path, url: &str, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chloe"));
    c.env_remove("CHLOE_DB")
        .env("CHLOE_OLLAMA_URL", url)
        .arg("--db-path")
        .arg(db)
        .arg("--json")
        .args(args);
    c
}

fn run_json(db: &Path, url: &str, args: &[&str]) -> Value {
    let out = cmd(db, url, args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn capture(db: &Path) {
    let dir = memory_ingest::spool::dir_for(db).join("pending");
    std::fs::create_dir_all(&dir).unwrap();
    let e = |i: u32, kind: &str, text: Option<&str>| {
        json!({"schema_version":1,"native_agent":"claude_code","native_session_id":"s1",
               "native_event_id":format!("e{i}"),"kind":kind,
               "occurred_at":format!("2026-01-01T00:00:0{i}Z"),"text":text})
    };
    let events = [
        e(1, "user_prompt", Some("Decision: keep SQLite")),
        e(2, "session_end", None),
    ];
    for (i, ev) in events.iter().enumerate() {
        std::fs::write(dir.join(format!("{i}.json")), ev.to_string()).unwrap();
    }
}

/// Accepts connections and never answers, like a model that is still thinking.
fn hanging_server() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut held = vec![];
        for s in l.incoming().flatten() {
            let mut s = s;
            let _ = s.read(&mut [0u8; 4096]);
            held.push(s);
        }
    });
    url
}

fn answering_server(body: String) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut data = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if data.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

struct Stub;

impl MemoryExtractor for Stub {
    fn name(&self) -> String {
        "stub".into()
    }
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>> {
        Ok(vec![Candidate {
            kind: memory_core::Kind::Decision,
            title: "Keep SQLite".into(),
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

#[test]
fn kill_9_during_extraction_loses_nothing_and_recovers() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    capture(&db);
    let url = hanging_server();

    let mut child = cmd(&db, &url, &["consolidate"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Wait until the job is leased, i.e. the process is inside the model call.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(s) = SqliteStore::open(&db)
            && s.counts().map(|c| c.leased).unwrap_or(0) == 1
        {
            break;
        }
        assert!(Instant::now() < deadline, "the job was never leased");
        std::thread::sleep(Duration::from_millis(50));
    }
    child.kill().unwrap();
    child.wait().unwrap();

    // The database is intact: events are there, nothing half-written, the job is still leased.
    let doctor = run_json(&db, &url, &["doctor"]);
    assert_eq!(doctor["healthy"], true, "{doctor}");
    let s = SqliteStore::open(&db).unwrap();
    assert_eq!(
        memory_core::EventStore::session_events(&s, "claude_code", "s1", 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(s.counts().unwrap().leased, 1);
    assert!(s.candidates(None, 10).unwrap().is_empty());

    // A run right away does not take the job over while its lease is valid.
    let r = run_json(&db, &url, &["consolidate", "--no-auto-apply"]);
    assert_eq!(r["sessions_extracted"], 0);

    // Once the lease has run out the job is reclaimed (after its backoff) and finishes once.
    let t0 = OffsetDateTime::now_utc();
    assert!(
        run_next(&s, &Stub, "w", t0 + time::Duration::minutes(11))
            .unwrap()
            .is_none()
    );
    let done = run_next(&s, &Stub, "w", t0 + time::Duration::hours(2))
        .unwrap()
        .unwrap();
    assert_eq!(done.result, Ok(1));
    assert_eq!(s.candidates(None, 10).unwrap().len(), 1);
    assert_eq!(s.counts().unwrap().succeeded, 1);
}

#[test]
fn repeated_consolidate_runs_extract_once() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    capture(&db);
    let reply = json!({"response": json!({"candidates": [{
        "kind": "decision", "title": "Keep SQLite", "content": "Decided.", "evidence": ["E1"]}]}).to_string()});
    let url = answering_server(reply.to_string());

    let first = run_json(&db, &url, &["consolidate"]);
    assert_eq!(
        (
            first["sessions_extracted"].as_u64(),
            first["new_candidates"].as_u64()
        ),
        (Some(1), Some(1))
    );
    let listed = run_json(&db, &url, &["candidates", "list", "--state", "all"]);
    for _ in 0..3 {
        let again = run_json(&db, &url, &["consolidate"]);
        assert_eq!(
            (
                again["imported_events"].as_u64(),
                again["sessions_queued"].as_u64(),
                again["sessions_extracted"].as_u64(),
                again["new_candidates"].as_u64()
            ),
            (Some(0), Some(0), Some(0), Some(0))
        );
        assert_eq!(
            run_json(&db, &url, &["candidates", "list", "--state", "all"]),
            listed
        );
    }
}

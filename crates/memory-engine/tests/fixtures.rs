//! Session fixtures (B1-05). Each `fixtures/*.json` is a captured session plus the model reply
//! to replay and what the pipeline must keep. The replay goes through the real prompt, the real
//! validation and the real job/candidate stores, so only the model is faked. The `#[ignore]`d
//! test runs the same sessions against a live local model:
//! `cargo test -p memory-engine --test fixtures -- --ignored --nocapture`.

use std::cell::RefCell;
use std::path::Path;

use memory_core::extract::{
    Candidate, CandidateState, CandidateStore, ExtractionInput, MemoryExtractor,
};
use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, EventStore, Result};
use memory_engine::{Window, plan, run_next};
use memory_llm::prompt;
use memory_llm::validate::from_reply;
use memory_store::SqliteStore;
use serde_json::Value;
use time::{Duration, OffsetDateTime};

struct Fixture {
    name: String,
    json: Value,
}

fn load() -> Vec<Fixture> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| Fixture {
            name: p.file_stem().unwrap().to_string_lossy().into(),
            json: serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap(),
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn events(f: &Fixture) -> Vec<AgentEvent> {
    let mut rows: Vec<Value> = f.json["head"].as_array().cloned().unwrap_or_default();
    if let Some(fill) = f.json.get("filler") {
        for i in 0..fill["count"].as_u64().unwrap() {
            rows.push(serde_json::json!({
                "id": format!("f{i}"), "kind": "tool_result_summary", "tool": "Bash", "text": fill["text"],
            }));
        }
    }
    rows.extend(f.json["events"].as_array().unwrap().iter().cloned());
    let base = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    rows.iter()
        .enumerate()
        .map(|(i, r)| AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: format!("fx-{}", f.name),
            native_event_id: Some(r["id"].as_str().unwrap().into()),
            kind: serde_json::from_value(r["kind"].clone()).unwrap(),
            occurred_at: base + Duration::seconds(i as i64),
            cwd: None,
            tool_name: r["tool"].as_str().map(Into::into),
            text: r["text"].as_str().map(Into::into),
            redactions: vec![],
        })
        .collect()
}

/// Replays the fixture's reply. `@id` in `evidence` names an event by its native id and is
/// turned into the `E<n>` ref the prompt gave that event, if the event is in the window.
struct Replay {
    reply: Value,
    seen: RefCell<usize>,
    rejected: RefCell<Vec<&'static str>>,
}

impl MemoryExtractor for Replay {
    fn name(&self) -> String {
        "replay".into()
    }
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>> {
        *self.seen.borrow_mut() = input.events.len();
        let mut reply = self.reply.clone();
        for c in reply["candidates"].as_array_mut().unwrap() {
            for e in c["evidence"].as_array_mut().unwrap() {
                let Some(id) = e.as_str().and_then(|s| s.strip_prefix('@')) else {
                    continue;
                };
                if let Some(i) = input
                    .events
                    .iter()
                    .position(|ev| ev.native_event_id.as_deref() == Some(id))
                {
                    *e = Value::String(format!("E{}", i + 1));
                }
            }
        }
        let out = from_reply(input, &prompt::build(input), reply)?;
        *self.rejected.borrow_mut() = out.rejected;
        Ok(out.accepted)
    }
}

#[test]
fn fixtures_replay_to_the_expected_candidates() {
    let fixtures = load();
    assert!(fixtures.len() >= 5);
    for f in fixtures {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
        let session = format!("fx-{}", f.name);
        store.ingest(&events(&f)).unwrap();
        let now = OffsetDateTime::now_utc();
        let p = plan(&store, now, Duration::minutes(30), Window::default(), None).unwrap();
        assert_eq!(p.enqueued, 1, "{}", f.name);

        let x = Replay {
            reply: f.json["reply"].clone(),
            seen: RefCell::new(0),
            rejected: RefCell::new(vec![]),
        };
        let done = run_next(&store, &x, "w", now).unwrap().unwrap();
        assert_eq!(done.session, session, "{}", f.name);
        assert!(done.result.is_ok(), "{}: {:?}", f.name, done.result);

        let want = &f.json["expect"];
        let mut stored = store
            .candidates(Some(CandidateState::Pending), 100)
            .unwrap();
        stored.reverse(); // oldest first = reply order
        let want_accepted = want["accepted"].as_array().unwrap();
        assert_eq!(stored.len(), want_accepted.len(), "{}: accepted", f.name);
        for (got, w) in stored.iter().zip(want_accepted) {
            let c = &got.candidate;
            let ctx = format!("{}: {}", f.name, c.title);
            assert_eq!(serde_json::to_value(c.kind).unwrap(), w["kind"], "{ctx}");
            assert!(
                c.title.contains(w["title_contains"].as_str().unwrap()),
                "{ctx}"
            );
            assert_eq!(
                c.evidence.events.len() as u64,
                w["events"].as_u64().unwrap(),
                "{ctx}"
            );
            assert_eq!(
                c.evidence.file.as_deref(),
                w["file"].as_str(),
                "{ctx}: file"
            );
            assert_eq!(
                c.evidence.commit.as_deref(),
                w["commit"].as_str(),
                "{ctx}: commit"
            );
        }
        let want_rejected: Vec<&str> = want["rejected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(*x.rejected.borrow(), want_rejected, "{}: rejected", f.name);
        // The character budget can cut below the event limit, so this is an upper bound.
        if let Some(max) = want["seen_at_most"].as_u64() {
            let seen = *x.seen.borrow() as u64;
            assert!(
                seen <= max && seen < events(&f).len() as u64,
                "{}: window {seen}",
                f.name
            );
        }
    }
}

/// Against a real local model (`CHLOE_OLLAMA_URL`, `CHLOE_LLM_MODEL` as for the CLI). The
/// checks are deliberately loose: a count range and kinds that must appear.
#[test]
#[ignore = "needs a running Ollama with the model pulled"]
fn fixtures_against_a_live_model() {
    let mut cfg = memory_llm::OllamaConfig::default();
    cfg.base_url = std::env::var("CHLOE_OLLAMA_URL").unwrap_or(cfg.base_url);
    cfg.model = std::env::var("CHLOE_LLM_MODEL").unwrap_or(cfg.model);
    let model = memory_llm::Ollama::new(cfg).unwrap();
    let mut failures = vec![];
    for f in load() {
        let all = events(&f);
        let window = Window::default();
        let input = ExtractionInput {
            native_agent: "claude_code".into(),
            native_session_id: format!("fx-{}", f.name),
            events: memory_engine::select(all, window),
        };
        let got = model.extract(&input).unwrap();
        println!("== {} ({} candidates)", f.name, got.len());
        for c in &got {
            println!("   {:?}: {}", c.kind, c.title);
        }
        let live = &f.json["live"];
        let (min, max) = (
            live["at_least"].as_u64().unwrap(),
            live["at_most"].as_u64().unwrap(),
        );
        if !(min..=max).contains(&(got.len() as u64)) {
            failures.push(format!(
                "{}: {} candidates, wanted {min}..={max}",
                f.name,
                got.len()
            ));
        }
        for k in live["kinds_include"].as_array().into_iter().flatten() {
            if !got
                .iter()
                .any(|c| serde_json::to_value(c.kind).unwrap() == *k)
            {
                failures.push(format!("{}: no {k} candidate", f.name));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

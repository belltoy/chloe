//! Hand-made extraction evaluation set (B-16): `docs/evidence/milestone-b/extraction/sessions.json`.
//! The plain tests check the set and the scorer; the `#[ignore]`d test runs every session
//! through a live local model and prints a markdown report (set `CHLOE_EXTRACTION_REPORT` to
//! also write it to a file):
//! `cargo test -p memory-engine --test extraction_eval -- --ignored --nocapture`.

use std::fmt::Write as _;
use std::path::Path;

use memory_core::extract::{Candidate, ExtractionInput, MemoryExtractor};
use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION};
use memory_engine::{Window, select};
use serde_json::Value;
use time::{Duration, OffsetDateTime};

fn load() -> Vec<Value> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/milestone-b/extraction/sessions.json");
    let v: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    v["sessions"].as_array().unwrap().clone()
}

fn events(s: &Value) -> Vec<AgentEvent> {
    let id = s["id"].as_str().unwrap();
    let base = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    s["events"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, r)| AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: format!("ex-{id}"),
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

/// Every group of `terms` has a term in `text` (case-insensitive).
fn has_terms(text: &str, terms: &Value) -> bool {
    let t = text.to_lowercase();
    terms.as_array().unwrap().iter().all(|g| {
        g.as_array()
            .unwrap()
            .iter()
            .any(|w| t.contains(&w.as_str().unwrap().to_lowercase()))
    })
}

fn text(c: &Candidate) -> String {
    format!("{} {}", c.title, c.content)
}

fn kind(c: &Candidate) -> String {
    serde_json::to_value(c.kind)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}

fn met(c: &Candidate, x: &Value) -> bool {
    x["kind_any"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k.as_str() == Some(&kind(c)))
        && has_terms(&text(c), &x["terms"])
}

#[test]
fn the_set_is_well_formed() {
    let all = load();
    assert!(all.len() >= 20);
    let mut ids: Vec<_> = all.iter().map(|s| s["id"].as_str().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "duplicate session ids");
    let groups = |g: &str| all.iter().filter(|s| s["group"] == g).count();
    assert!(groups("useful") >= 10 && groups("nothing") + groups("trap") >= 10);
    for s in &all {
        let id = s["id"].as_str().unwrap();
        assert!(!events(s).is_empty(), "{id}");
        let useful = s["group"] == "useful";
        let expect = s["expect"].as_array().unwrap();
        assert!(!useful || !expect.is_empty(), "{id}");
        if s["group"] == "nothing" {
            assert!(expect.is_empty(), "{id}");
        }
        for x in expect {
            assert!(!x["kind_any"].as_array().unwrap().is_empty(), "{id}");
            assert!(!x["terms"].as_array().unwrap().is_empty(), "{id}");
        }
    }
}

#[test]
fn the_scorer_matches_kind_and_every_term_group() {
    let c = |k, t: &str| Candidate {
        kind: k,
        title: t.into(),
        content: String::new(),
        evidence: memory_core::extract::Evidence {
            native_agent: "a".into(),
            native_session_id: "s".into(),
            events: vec![],
            file: None,
            commit: None,
        },
    };
    let x = serde_json::json!({"kind_any": ["gotcha"], "terms": [["floor"], ["page", "last"]]});
    assert!(met(
        &c(memory_core::Kind::Gotcha, "Floor division drops a page"),
        &x
    ));
    assert!(!met(
        &c(memory_core::Kind::Fact, "Floor division drops a page"),
        &x
    ));
    assert!(!met(&c(memory_core::Kind::Gotcha, "Floor division"), &x));
}

#[test]
#[ignore = "needs a running Ollama with the model pulled"]
fn extraction_against_a_live_model() {
    let mut cfg = memory_llm::OllamaConfig::default();
    cfg.base_url = std::env::var("CHLOE_OLLAMA_URL").unwrap_or(cfg.base_url);
    cfg.model = std::env::var("CHLOE_LLM_MODEL").unwrap_or(cfg.model);
    let model_name = cfg.model.clone();
    let model = memory_llm::Ollama::new(cfg).unwrap();

    let (mut expected, mut found) = (0, 0);
    let (mut total, mut false_ext, mut trapped, mut extras) = (0, 0, 0, 0);
    let mut empty_sessions = (0, 0); // (sessions without knowledge, of which quiet)
    let mut per = String::new();
    for s in load() {
        let id = s["id"].as_str().unwrap();
        let all_events = events(&s);
        let got = model
            .extract(&ExtractionInput {
                native_agent: "claude_code".into(),
                native_session_id: format!("ex-{id}"),
                events: select(events(&s), Window::default()),
            })
            .unwrap();
        total += got.len();
        let expect = s["expect"].as_array().unwrap();
        let traps = s["traps"].as_array().unwrap();
        let _ = writeln!(
            per,
            "### {id} ({}) — {}",
            s["group"].as_str().unwrap(),
            s["title"].as_str().unwrap()
        );
        for x in expect {
            expected += 1;
            let hit = got.iter().any(|c| met(c, x));
            found += hit as usize;
            let _ = writeln!(
                per,
                "- {} expected: {}",
                if hit { "FOUND" } else { "MISSED" },
                x["note"].as_str().unwrap()
            );
        }
        for c in &got {
            let tag = if traps.iter().any(|t| has_terms(&text(c), &t["terms"])) {
                trapped += 1;
                false_ext += 1;
                "TRAP"
            } else if expect.iter().any(|x| met(c, x)) {
                "matches"
            } else if expect.is_empty() {
                false_ext += 1;
                "UNEXPECTED"
            } else {
                extras += 1;
                "extra (needs human check)"
            };
            let _ = writeln!(
                per,
                "  - [{tag}] {}: {} — {}",
                kind(c),
                c.title,
                c.content.replace('\n', " ")
            );
            let cited: Vec<String> = c
                .evidence
                .events
                .iter()
                .map(|k| {
                    all_events
                        .iter()
                        .find(|e| &e.idempotency_key() == k)
                        .and_then(|e| e.native_event_id.clone())
                        .unwrap_or_else(|| "?".into())
                })
                .collect();
            let _ = writeln!(per, "    evidence: {}", cited.join(", "));
        }
        if expect.is_empty() {
            empty_sessions.0 += 1;
            empty_sessions.1 += got.is_empty() as usize;
        }
        per.push('\n');
    }
    let report = format!(
        "# B-16 extraction evaluation\n\nModel: `{model_name}`, temperature from the default config, window = default.\n\n\
         | measure | value |\n|---|---|\n\
         | expected items found | {found} / {expected} |\n\
         | candidates produced | {total} |\n\
         | false extractions (trap or any candidate in a session with no knowledge) | {false_ext} (of which trap matches {trapped}) |\n\
         | sessions with no knowledge that produced nothing | {} / {} |\n\
         | extra candidates not covered by a label (to be judged by a person) | {extras} |\n\n{per}",
        empty_sessions.1, empty_sessions.0
    );
    println!("{report}");
    if let Ok(p) = std::env::var("CHLOE_EXTRACTION_REPORT") {
        std::fs::write(p, report).unwrap();
    }
}

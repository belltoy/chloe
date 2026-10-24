use memory_mcp::serve;
use memory_store::SqliteStore;
use serde_json::{Value, json};

fn session(store: &SqliteStore, project: memory_core::ProjectId, msgs: &[Value]) -> Vec<Value> {
    let input: String = msgs.iter().map(|m| format!("{m}\n")).collect();
    let mut out = Vec::new();
    serve(store, project, input.as_bytes(), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn call(id: u32, name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
}

fn payload(resp: &Value) -> Value {
    serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn protocol_and_tool_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let proj_dir = dir.path().join("p");
    std::fs::create_dir(&proj_dir).unwrap();
    let pid = store.resolve_project(&proj_dir).unwrap();

    let r = session(
        &store,
        pid,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
            call(
                4,
                "memory_save",
                json!({"kind": "gotcha", "title": "T", "content": "use nextest"}),
            ),
        ],
    );
    assert_eq!(r.len(), 4, "notification must not be answered");
    assert_eq!(r[0]["result"]["protocolVersion"], "2024-11-05");
    assert!(
        r[0]["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("memory_search")
    );
    assert_eq!(r[1]["result"]["tools"].as_array().unwrap().len(), 6);
    assert_eq!(r[2]["error"]["code"], -32601);
    let saved = payload(&r[3]);
    let id = saved["id"].as_str().unwrap();

    let r = session(
        &store,
        pid,
        &[
            call(1, "memory_search", json!({"query": "nextest"})),
            call(
                2,
                "memory_update",
                json!({"id": id, "expected_revision": 1, "title": "T2"}),
            ),
            call(
                3,
                "memory_update",
                json!({"id": id, "expected_revision": 1, "title": "T3"}),
            ),
            call(4, "memory_get", json!({"id": id, "bogus": 1})),
            call(
                5,
                "memory_archive",
                json!({"id": id, "expected_revision": 2}),
            ),
            call(6, "memory_search", json!({"query": "nextest"})),
            call(7, "no_such_tool", json!({})),
        ],
    );
    assert_eq!(payload(&r[0])["hits"].as_array().unwrap().len(), 1);
    assert_eq!(payload(&r[1])["revision"], 2);
    assert_eq!(r[2]["result"]["isError"], true);
    assert_eq!(payload(&r[2])["code"], "conflict");
    assert_eq!(payload(&r[3])["code"], "invalid_argument");
    assert_eq!(payload(&r[4])["status"], "archived");
    assert!(payload(&r[5])["hits"].as_array().unwrap().is_empty());
    assert_eq!(r[6]["error"]["code"], -32602);

    // Another project cannot see the memory.
    let other_dir = dir.path().join("q");
    std::fs::create_dir(&other_dir).unwrap();
    let other = store.resolve_project(&other_dir).unwrap();
    let r = session(&store, other, &[call(1, "memory_get", json!({"id": id}))]);
    assert_eq!(payload(&r[0])["code"], "not_found");
}

#[test]
fn global_scope_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let mk = |n: &str| {
        let p = dir.path().join(n);
        std::fs::create_dir(&p).unwrap();
        store.resolve_project(&p).unwrap()
    };
    let (a, b) = (mk("a"), mk("b"));

    let r = session(
        &store,
        a,
        &[
            call(
                1,
                "memory_save",
                json!({"kind": "fact", "title": "G", "content": "shared lint rule", "scope": "global"}),
            ),
            call(
                2,
                "memory_save",
                json!({"kind": "fact", "title": "P", "content": "private lint rule"}),
            ),
            call(
                3,
                "memory_save",
                json!({"kind": "fact", "title": "X", "content": "x", "scope": "galaxy"}),
            ),
        ],
    );
    let gid = payload(&r[0])["id"].as_str().unwrap().to_string();
    assert!(payload(&r[0])["project_id"].is_null());
    assert_eq!(payload(&r[2])["code"], "invalid_argument");

    let r = session(
        &store,
        b,
        &[
            call(1, "memory_search", json!({"query": "lint"})),
            call(
                2,
                "memory_search",
                json!({"query": "lint", "include_global": true}),
            ),
            call(
                3,
                "memory_update",
                json!({"id": gid, "expected_revision": 1, "title": "G2"}),
            ),
        ],
    );
    assert!(payload(&r[0])["hits"].as_array().unwrap().is_empty());
    let hits = payload(&r[1])["hits"].clone();
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["memory"]["title"], "G");
    assert_eq!(payload(&r[2])["revision"], 2);
}

#[test]
fn search_results_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let proj = dir.path().join("p");
    std::fs::create_dir(&proj).unwrap();
    let pid = store.resolve_project(&proj).unwrap();
    let long = format!("needle {}", "x".repeat(5000));

    let r = session(
        &store,
        pid,
        &[
            call(
                1,
                "memory_save",
                json!({"kind": "fact", "title": "Long", "content": long}),
            ),
            call(
                2,
                "memory_save",
                json!({"kind": "fact", "title": "Short", "content": "needle"}),
            ),
            call(
                3,
                "memory_search",
                json!({"query": "needle", "limit": 1000}),
            ),
        ],
    );
    let id = payload(&r[0])["id"].as_str().unwrap().to_string();
    let hits = payload(&r[2])["hits"].clone();
    let long_hit = hits
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["memory"]["title"] == "Long")
        .unwrap();
    assert_eq!(
        long_hit["memory"]["content"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        1000
    );
    assert_eq!(long_hit["content_truncated"], true);
    let short_hit = hits
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["memory"]["title"] == "Short")
        .unwrap();
    assert_eq!(short_hit["content_truncated"], false);

    // memory_get still returns the full body.
    let r = session(&store, pid, &[call(1, "memory_get", json!({"id": id}))]);
    assert_eq!(
        payload(&r[0])["content"].as_str().unwrap().len(),
        long.len()
    );
}

#[test]
fn search_explains_hits_and_respects_the_total_budget() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let proj = dir.path().join("p");
    std::fs::create_dir(&proj).unwrap();
    let pid = store.resolve_project(&proj).unwrap();
    let body = format!("needle {}", "x".repeat(800));
    let mut msgs: Vec<Value> = (0..3)
        .map(|i| {
            call(
                i,
                "memory_save",
                json!({"kind": "fact", "title": format!("T{i}"), "content": body}),
            )
        })
        .collect();
    msgs.push(call(
        9,
        "memory_search",
        json!({"query": "needle", "max_chars": 1000}),
    ));
    let r = session(&store, pid, &msgs);
    let hits = payload(&r[3])["hits"].clone();
    let hits = hits.as_array().unwrap();
    assert_eq!(hits.len(), 3, "hits are kept, only bodies are cut");
    let total: usize = hits
        .iter()
        .map(|h| h["memory"]["content"].as_str().unwrap().chars().count())
        .sum();
    assert!(total <= 1000, "{total}");
    assert_eq!(hits[2]["content_truncated"], true);
    assert_eq!(hits[0]["matched_by"], json!(["fts"]));
    assert_eq!(hits[0]["fts_rank"], 1);
    assert!(hits[0]["vector_rank"].is_null());
    assert_eq!(hits[0]["memory"]["status"], "active");
}

struct Down;
impl memory_core::embed::Embedder for Down {
    fn model_id(&self) -> String {
        "down".into()
    }
    fn embed(&self, _: &[String]) -> memory_core::Result<Vec<Vec<f32>>> {
        Err(memory_core::Error::Unavailable("connection refused".into()))
    }
}

#[test]
fn search_reports_its_mode_and_why_it_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let proj = dir.path().join("p");
    std::fs::create_dir(&proj).unwrap();
    let pid = store.resolve_project(&proj).unwrap();
    let msgs = [
        call(
            1,
            "memory_save",
            json!({"kind": "fact", "title": "T", "content": "needle"}),
        ),
        call(2, "memory_search", json!({"query": "needle"})),
    ];
    let run = |embedder: Option<&dyn memory_core::embed::Embedder>| {
        let input: String = msgs.iter().map(|m| format!("{m}\n")).collect();
        let mut out = Vec::new();
        memory_mcp::serve_with(
            &store,
            if embedder.is_some() {
                memory_engine::SearchMode::Hybrid
            } else {
                memory_engine::SearchMode::Fts
            },
            embedder,
            pid,
            input.as_bytes(),
            &mut out,
        )
        .unwrap();
        let last = String::from_utf8(out)
            .unwrap()
            .lines()
            .last()
            .unwrap()
            .to_string();
        payload(&serde_json::from_str(&last).unwrap())
    };
    // Full-text only by configuration: not a degradation.
    let p = run(None);
    assert_eq!(
        (p["mode"].as_str(), p["degraded"].is_null()),
        (Some("fts"), true)
    );
    // Provider outage: hits still come back, with the reason.
    let p = run(Some(&Down));
    assert_eq!(
        (&p["mode"], &p["requested_mode"]),
        (&json!("fts"), &json!("hybrid"))
    );
    assert!(
        p["degraded"]
            .as_str()
            .unwrap()
            .contains("connection refused")
    );
    assert_eq!(p["candidates"], json!({"fts": 2, "vector": 0}));
    // (each run saves the memory once more)
    assert_eq!(p["hits"].as_array().unwrap().len(), 2);
}

#[test]
fn context_pack_is_grouped_bounded_and_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let mk = |n: &str| {
        let p = dir.path().join(n);
        std::fs::create_dir(&p).unwrap();
        store.resolve_project(&p).unwrap()
    };
    let (a, b) = (mk("a"), mk("b"));
    let save = |i, kind: &str, title: &str| {
        call(
            i,
            "memory_save",
            json!({"kind": kind, "title": title, "content": format!("database {}", "x ".repeat(300))}),
        )
    };
    // Project b saves one; project a never sees it.
    let r = session(&store, b, &[save(1, "fact", "Theirs")]);
    let theirs = payload(&r[0])["id"].as_str().unwrap().to_string();
    let msgs = [
        save(1, "gotcha", "Mine gotcha"),
        save(2, "constraint", "Mine rule"),
        call(
            3,
            "memory_context",
            json!({"task": "database work", "budget_chars": 400}),
        ),
        call(4, "memory_context", json!({"task": "database work"})),
        call(
            5,
            "memory_context",
            json!({"task": "database work", "budget_chars": 5}),
        ),
        call(6, "memory_get", json!({"id": theirs})),
    ];
    let r = session(&store, a, &msgs);
    let small = payload(&r[2]);
    assert!(small["used"].as_u64().unwrap() <= 400, "{small}");
    assert_eq!(
        (small["mode"].as_str(), small["requested_mode"].as_str()),
        (Some("fts"), Some("fts"))
    );
    let full = payload(&r[3]);
    let kinds: Vec<_> = full["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["constraint", "gotcha"]);
    assert_eq!(full["omitted"], 0);
    let item = &full["groups"][0]["items"][0];
    assert!(
        item["id"].is_string() && item["source"] == "agent" && item["summary_truncated"] == true
    );
    assert!(!full.to_string().contains("Theirs"));
    // Bad budget is an error result; another project's id cannot be expanded.
    assert_eq!(payload(&r[4])["code"], "invalid_argument");
    assert_eq!(payload(&r[5])["code"], "not_found");
}

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
    assert_eq!(r[1]["result"]["tools"].as_array().unwrap().len(), 5);
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
    assert_eq!(payload(&r[0]).as_array().unwrap().len(), 1);
    assert_eq!(payload(&r[1])["revision"], 2);
    assert_eq!(r[2]["result"]["isError"], true);
    assert_eq!(payload(&r[2])["code"], "conflict");
    assert_eq!(payload(&r[3])["code"], "invalid_argument");
    assert_eq!(payload(&r[4])["status"], "archived");
    assert!(payload(&r[5]).as_array().unwrap().is_empty());
    assert_eq!(r[6]["error"]["code"], -32602);

    // Another project cannot see the memory.
    let other_dir = dir.path().join("q");
    std::fs::create_dir(&other_dir).unwrap();
    let other = store.resolve_project(&other_dir).unwrap();
    let r = session(&store, other, &[call(1, "memory_get", json!({"id": id}))]);
    assert_eq!(payload(&r[0])["code"], "not_found");
}

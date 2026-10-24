use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

use serde_json::{Value, json};

/// Reads one HTTP request and returns its JSON body.
fn read_body(s: &mut TcpStream) -> Value {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).unwrap_or(0);
        if n == 0 {
            return Value::Null;
        }
        data.extend_from_slice(&buf[..n]);
        let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
        let len = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if data.len() >= end + 4 + len {
            return serde_json::from_slice(&data[end + 4..end + 4 + len]).unwrap_or(Value::Null);
        }
    }
}

/// A stand-in for Ollama's `/api/embed`: one fixed 2-d vector per input text.
fn embed_server() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for s in l.incoming() {
            let mut s = s.unwrap();
            let n = read_body(&mut s)["input"].as_array().map_or(0, Vec::len);
            let body = json!({"embeddings": vec![[1.0, 0.5]; n]}).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

#[test]
fn backfill_stops_at_the_limit_and_says_how_many_are_left() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let url = embed_server();
    let run = |args: &[&str]| -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
            .env_remove("CHLOE_DB")
            .arg("--db-path")
            .arg(dir.path().join("t.db"))
            .arg("--dir")
            .arg(&proj)
            .arg("--json")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
    };
    run(&["init"]);
    for i in 0..3 {
        run(&[
            "memory",
            "add",
            "--title",
            &format!("t{i}"),
            "--content",
            "body",
        ]);
    }
    let backfill = |limit: &str| run(&["embeddings", "backfill", "--limit", limit, "--url", &url]);
    let first = backfill("2");
    assert_eq!(
        (first["embedded"].as_i64(), first["remaining"].as_i64()),
        (Some(2), Some(1))
    );
    let second = backfill("2");
    assert_eq!(
        (second["embedded"].as_i64(), second["remaining"].as_i64()),
        (Some(1), Some(0))
    );
    assert_eq!(run(&["embeddings", "status"])["missing"], 0);
}

#[test]
fn explain_works_in_every_mode_and_filters_archived_memories() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let url = embed_server();
    let run = |args: &[&str]| -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
            .env_remove("CHLOE_DB")
            .arg("--db-path")
            .arg(dir.path().join("t.db"))
            .arg("--dir")
            .arg(&proj)
            .arg("--json")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
    };
    run(&["init"]);
    let mut ids = vec![];
    for i in 0..2 {
        let m = run(&[
            "memory",
            "add",
            "--title",
            &format!("pool {i}"),
            "--content",
            "pool sizing",
        ]);
        ids.push(m["id"].as_str().unwrap().to_string());
    }
    run(&["embeddings", "backfill", "--url", &url]);
    run(&["memory", "archive", &ids[1], "--revision", "1"]);
    for mode in ["fts", "vector", "hybrid"] {
        let r = run(&[
            "memory",
            "search",
            "pool",
            "--mode",
            mode,
            "--explain",
            "--url",
            &url,
        ]);
        let e = &r["explain"];
        assert_eq!(r["mode"], mode, "{r}");
        assert_eq!(
            (e["status_filter"].as_str(), e["scope"].is_string()),
            (Some("active"), true),
            "{mode}: {r}"
        );
        assert!(
            e["fusion"].is_string()
                && e["fts_returned"].is_number()
                && e["vector_returned"].is_number()
        );
        // The archived memory is filtered out of every path.
        let hits = r["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{mode}: {r}");
        assert_eq!(hits[0]["memory"]["id"], ids[0].as_str());
    }
}

#[test]
fn mcp_search_does_not_hang_on_a_stalled_embedding_server() {
    use std::io::Write as _;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    let db = dir.path().join("t.db");
    let chloe = |args: &[&str]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_chloe"));
        c.env_remove("CHLOE_DB")
            .arg("--db-path")
            .arg(&db)
            .arg("--dir")
            .arg(&proj)
            .args(args);
        c
    };
    assert!(chloe(&["init"]).output().unwrap().status.success());
    assert!(
        chloe(&[
            "memory",
            "add",
            "--title",
            "pool",
            "--content",
            "pool sizing"
        ])
        .output()
        .unwrap()
        .status
        .success()
    );
    // Accepts the connection and never answers.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let _held = std::thread::spawn(move || {
        let conns: Vec<_> = l.incoming().take(2).collect();
        std::thread::sleep(Duration::from_secs(30));
        drop(conns);
    });
    let mut child = chloe(&["mcp", "serve", "--mode", "hybrid", "--url", &url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let search = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"memory_search","arguments":{"query":"pool"}}});
    writeln!(child.stdin.take().unwrap(), "{search}").unwrap();
    let start = Instant::now();
    let out = child.wait_with_output().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(15),
        "{:?}",
        start.elapsed()
    );
    let reply: Value = serde_json::from_slice(&out.stdout).unwrap();
    let body: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        (body["mode"].as_str(), body["requested_mode"].as_str()),
        (Some("fts"), Some("hybrid"))
    );
    assert!(body["degraded"].is_string());
    assert_eq!(body["hits"].as_array().unwrap().len(), 1);
}

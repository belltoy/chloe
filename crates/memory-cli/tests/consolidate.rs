use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

fn chloe(db: &Path, url: &str, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
        .env_remove("CHLOE_DB")
        .env("CHLOE_OLLAMA_URL", url)
        .arg("--db-path")
        .arg(db)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Read one whole HTTP request (headers and body) so closing the socket never resets a
/// client that is still sending.
fn read_request(s: &mut std::net::TcpStream) {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).unwrap_or(0);
        if n == 0 {
            return;
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
            return;
        }
    }
}

/// One canned reply for one request.
fn server(body: String) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        read_request(&mut s);
        let _ = write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    url
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
        e(1, "user_prompt", Some("Decision: keep SQLite, no daemon")),
        e(2, "session_end", None),
    ];
    for (i, ev) in events.iter().enumerate() {
        std::fs::write(dir.join(format!("{i}.json")), ev.to_string()).unwrap();
    }
}

#[test]
fn consolidate_extracts_candidates_and_survives_a_provider_outage() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    capture(&db);

    // Nothing listens on port 9: events are kept and the failure is reported.
    let r = chloe(&db, "http://127.0.0.1:9", &["consolidate"]);
    assert_eq!(
        (
            r["imported_events"].as_u64(),
            r["sessions_extracted"].as_u64()
        ),
        (Some(2), Some(0))
    );
    assert!(
        r["failed"][0]["error"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
    assert_eq!(
        chloe(&db, "http://127.0.0.1:9", &["candidates", "list"]),
        json!([])
    );

    // Backing off: a second run neither duplicates the job nor calls the model.
    let r = chloe(&db, "http://127.0.0.1:9", &["consolidate"]);
    assert_eq!(
        (
            r["sessions_queued"].as_u64(),
            r["failed"].as_array().unwrap().len()
        ),
        (Some(0), 0)
    );

    // Retry now with a working model.
    let id = chloe(&db, "http://127.0.0.1:9", &["jobs", "list"])[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    chloe(&db, "http://127.0.0.1:9", &["jobs", "retry", &id]);
    let reply = json!({"response": json!({"candidates": [{
        "kind": "decision", "title": "Keep SQLite, no daemon", "content": "Decided by the user.",
        "evidence": ["E1"]}]}).to_string()});
    let url = server(reply.to_string());
    let r = chloe(&db, &url, &["consolidate"]);
    assert_eq!(
        (
            r["sessions_extracted"].as_u64(),
            r["new_candidates"].as_u64()
        ),
        (Some(1), Some(1))
    );
    let c = chloe(&db, &url, &["candidates", "list"]);
    assert_eq!(c[0]["title"], "Keep SQLite, no daemon");
    assert_eq!(c[0]["evidence"]["native_session_id"], "s1");
}

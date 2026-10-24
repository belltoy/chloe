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

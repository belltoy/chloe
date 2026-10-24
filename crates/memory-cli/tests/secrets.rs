//! B-SEC-01: a fake credential must not reach disk, the model, or any output.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;

use serde_json::{Value, json};

const FAKE: &str = "FAKE_TEST_SECRET_9f8e7d6c5b4a39281706";

fn chloe(db: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chloe"));
    c.env_remove("CHLOE_DB")
        .env_remove("CHLOE_CAPTURE")
        .arg("--db-path")
        .arg(db);
    c
}

/// Reads one HTTP request and returns its body.
fn read_body(s: &mut std::net::TcpStream) -> String {
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
                return String::from_utf8_lossy(&data[end + 4..]).into_owned();
            }
        }
    }
    String::new()
}

fn hook(db: &Path, payload: Value) -> std::process::Output {
    let mut child = chloe(db)
        .args(["hook", "claude"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn all_files_under(dir: &Path, out: &mut Vec<Vec<u8>>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            all_files_under(&p, out);
        } else {
            out.push(std::fs::read(&p).unwrap());
        }
    }
}

#[test]
fn a_fake_credential_never_reaches_disk_the_model_or_any_output() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let mut outputs: Vec<(String, Vec<u8>)> = vec![];

    // Adapter: the token appears in a prompt, a command and an environment assignment.
    let payloads = [
        json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt_id":"p","cwd":"/p",
               "prompt": format!("call the API with sk-{FAKE} please")}),
        json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Bash","tool_use_id":"t1",
               "tool_input":{"command": format!("API_TOKEN={FAKE} curl https://x.test -H 'Authorization: Bearer {FAKE}'")},
               "tool_response": format!("echo {FAKE}")}),
        json!({"session_id":"s","hook_event_name":"SessionEnd"}),
    ];
    for p in payloads {
        let out = hook(&db, p);
        assert!(out.status.success());
        outputs.push(("hook stdout".into(), out.stdout));
        outputs.push(("hook stderr".into(), out.stderr));
    }

    // Persistence: spool, then database.
    let import = chloe(&db)
        .args(["--json", "events", "import"])
        .output()
        .unwrap();
    outputs.push(("import".into(), [import.stdout, import.stderr].concat()));
    let listed = chloe(&db)
        .args(["--json", "events", "sessions"])
        .output()
        .unwrap();
    let sessions: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let sid = sessions[0]["native_session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let events = chloe(&db)
        .args(["--json", "events", "list", &sid])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&events.stdout).contains("[REDACTED]"));
    outputs.push(("events list".into(), events.stdout));

    // Extraction input: what the model is sent.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let mut s = s.unwrap();
            tx.send(read_body(&mut s)).ok();
            let body = json!({"response": json!({"candidates": []}).to_string()}).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let run = chloe(&db)
        .env("CHLOE_OLLAMA_URL", &url)
        .env("CHLOE_LOG", "trace")
        .args(["--json", "consolidate"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    outputs.push(("consolidate".into(), [run.stdout, run.stderr].concat()));
    let sent = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the model was never called");
    assert!(
        sent.contains("[REDACTED]") && sent.contains("curl"),
        "the model saw no event: {sent}"
    );
    outputs.push(("model request".into(), sent.into_bytes()));

    // Everything on disk next to the database (database, WAL, spool).
    let mut files = vec![];
    all_files_under(tmp.path(), &mut files);
    outputs.extend(files.into_iter().map(|f| ("file on disk".to_string(), f)));

    for (what, bytes) in outputs {
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(FAKE) && !text.contains(&FAKE[..24]),
            "{what} holds the credential"
        );
    }
}

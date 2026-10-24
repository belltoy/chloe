use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use memory_core::extract::{ExtractionInput, MemoryExtractor};
use memory_core::{Error, Kind};
use memory_llm::{Ollama, OllamaConfig};

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

/// Serves `replies` (status line, body) one connection each, then stops.
fn server(replies: Vec<(&'static str, String)>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for (status, body) in replies {
            let (mut s, _) = l.accept().unwrap();
            read_request(&mut s);
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

fn client(url: String) -> Ollama {
    Ollama::new(OllamaConfig {
        base_url: url,
        timeout: Duration::from_secs(5),
        ..Default::default()
    })
    .unwrap()
}

fn event(text: &str) -> memory_core::AgentEvent {
    memory_core::AgentEvent {
        schema_version: memory_core::EVENT_SCHEMA_VERSION,
        native_agent: "claude_code".into(),
        native_session_id: "s".into(),
        native_event_id: Some("e1".into()),
        kind: memory_core::EventKind::UserPrompt,
        occurred_at: time::OffsetDateTime::UNIX_EPOCH,
        cwd: None,
        tool_name: None,
        text: Some(text.into()),
        redactions: vec![],
    }
}

fn input() -> ExtractionInput {
    ExtractionInput {
        native_agent: "claude_code".into(),
        native_session_id: "s".into(),
        events: vec![event("Use WAL because of concurrency")],
    }
}

fn reply(inner: &str) -> (&'static str, String) {
    ("200 OK", serde_json::json!({"response": inner}).to_string())
}

#[test]
fn extracts_candidates_from_model_output() {
    let url = server(vec![reply(
        r#"{"candidates":[{"kind":"decision","title":"Use WAL","content":"Because of concurrency","evidence":["E1"]}]}"#,
    )]);
    let c = client(url).extract(&input()).unwrap();
    assert_eq!((c.len(), c[0].kind), (1, Kind::Decision));
    assert_eq!(c[0].evidence.events.len(), 1);
    assert_eq!(c[0].evidence.native_session_id, "s");
}

#[test]
fn lists_models() {
    let body = r#"{"models":[{"name":"qwen3:8b"},{"name":"gemma3:4b"}]}"#;
    let url = server(vec![("200 OK", body.into())]);
    assert_eq!(client(url).models().unwrap(), ["qwen3:8b", "gemma3:4b"]);
}

#[test]
fn bad_output_is_rejected_not_trusted() {
    for bad in [
        "not json",
        r#"{"candidates":[{"kind":"secret","title":"t","content":"c","evidence":["E1"]}]}"#,
    ] {
        let url = server(vec![reply(bad)]);
        let e = client(url).extract(&input()).unwrap_err();
        assert!(matches!(e, Error::InvalidArgument(_)), "{e}");
    }
}

#[test]
fn missing_model_and_dead_server_are_unavailable() {
    let url = server(vec![(
        "404 Not Found",
        r#"{"error":"model not found"}"#.into(),
    )]);
    assert!(matches!(
        client(url).extract(&input()),
        Err(Error::Unavailable(_))
    ));
    // Nothing listens on a just-closed port.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let e = client(format!("http://127.0.0.1:{port}")).extract(&input());
    assert!(matches!(e, Err(Error::Unavailable(_))));
}

#[test]
fn remote_needs_opt_in() {
    let remote = |allow_remote| {
        Ollama::new(OllamaConfig {
            base_url: "http://192.168.1.9:11434".into(),
            allow_remote,
            ..Default::default()
        })
    };
    assert!(remote(false).is_err());
    assert!(remote(true).is_ok());
    let https = Ollama::new(OllamaConfig {
        base_url: "https://127.0.0.1".into(),
        ..Default::default()
    });
    assert!(https.is_err());
}

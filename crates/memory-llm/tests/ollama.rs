use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use memory_core::extract::{ExtractionInput, MemoryExtractor};
use memory_core::{Error, Kind};
use memory_llm::{Ollama, OllamaConfig};

/// Serves `replies` (status line, body) one connection each, then stops.
fn server(replies: Vec<(&'static str, String)>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for (status, body) in replies {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
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

fn input() -> ExtractionInput {
    ExtractionInput {
        native_agent: "claude_code".into(),
        native_session_id: "s".into(),
        events: vec![],
    }
}

fn reply(inner: &str) -> (&'static str, String) {
    ("200 OK", serde_json::json!({"response": inner}).to_string())
}

#[test]
fn extracts_candidates_from_model_output() {
    let url = server(vec![reply(
        r#"{"candidates":[{"kind":"decision","title":"Use WAL","content":"Because of concurrency"}]}"#,
    )]);
    let c = client(url).extract(&input()).unwrap();
    assert_eq!((c.len(), c[0].kind), (1, Kind::Decision));
    assert!(c[0].evidence.is_empty());
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
        r#"{"candidates":[{"kind":"secret","title":"t","content":"c"}]}"#,
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

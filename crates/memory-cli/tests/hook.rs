use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

fn chloe() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chloe"));
    c.env_remove("CHLOE_DB").env_remove("CHLOE_CAPTURE");
    c
}

fn run_hook(db: &Path, stdin: &[u8], capture: Option<&str>) -> Output {
    run_agent_hook("claude", db, stdin, capture)
}

fn run_agent_hook(agent: &str, db: &Path, stdin: &[u8], capture: Option<&str>) -> Output {
    let mut c = chloe();
    c.arg("--db-path").arg(db).args(["hook", agent]);
    if let Some(v) = capture {
        c.env("CHLOE_CAPTURE", v);
    }
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn spool_lines(db: &Path) -> Vec<Value> {
    let p = memory_ingest::spool::path_for(db);
    std::fs::read_to_string(p)
        .map(|s| {
            s.lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn hook_spools_a_redacted_event_and_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let payload = json!({"session_id":"s1","prompt_id":"p1","hook_event_name":"UserPromptSubmit",
        "prompt":"deploy with API_KEY=hunter2 please"});
    let out = run_hook(&db, payload.to_string().as_bytes(), None);
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "stdout is fed back to the model by Claude Code"
    );
    let lines = spool_lines(&db);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["text"], "deploy with API_KEY=[REDACTED] please");
    assert_eq!(lines[0]["kind"], "user_prompt");
    assert!(
        !std::fs::read_to_string(memory_ingest::spool::path_for(&db))
            .unwrap()
            .contains("hunter2")
    );
    assert!(
        !db.exists(),
        "the hook must not open or create the database"
    );
}

#[test]
fn hook_never_fails_the_agent() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    for bad in [
        &b""[..],
        b"not json",
        b"{}",
        br#"{"session_id":"s"}"#,
        br#"{"session_id":"s","hook_event_name":"Stop"}"#,
    ] {
        let out = run_hook(&db, bad, None);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(bad));
        assert!(out.stdout.is_empty());
    }
    assert!(spool_lines(&db).is_empty());
}

#[test]
fn kill_switch_disables_capture() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let payload = json!({"session_id":"s1","hook_event_name":"SessionStart"}).to_string();
    assert!(
        run_hook(&db, payload.as_bytes(), Some("off"))
            .status
            .success()
    );
    assert!(spool_lines(&db).is_empty());
    assert!(
        run_hook(&db, payload.as_bytes(), Some("on"))
            .status
            .success()
    );
    assert_eq!(spool_lines(&db).len(), 1);
}

#[test]
fn install_and_uninstall_keep_other_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let original =
        json!({"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"other"}]}]}});
    std::fs::write(&settings, serde_json::to_string_pretty(&original).unwrap()).unwrap();
    let run = |verb: &str| {
        let out = chloe()
            .args(["hook", verb, "claude", "--settings"])
            .arg(&settings)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(run("install").contains("installed"));
    assert!(run("install").contains("nothing to do"));
    let s: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(s["hooks"]["Stop"][0], original["hooks"]["Stop"][0]);
    assert_eq!(
        s["hooks"]["SessionEnd"][0]["hooks"][0]["command"],
        "chloe hook claude"
    );
    assert!(run("uninstall").contains("removed"));
    let s: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(s, original);
}

#[test]
fn codex_hook_spools_and_installs_into_hooks_json() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let payload = json!({"session_id":"c1","turn_id":"t1","hook_event_name":"PostToolUse","tool_name":"Bash",
        "tool_input":{"command":"echo hi"},"tool_response":"hi\n","tool_use_id":"exec-1"});
    let out = run_agent_hook("codex", &db, payload.to_string().as_bytes(), None);
    assert!(out.status.success() && out.stdout.is_empty());
    let lines = spool_lines(&db);
    assert_eq!(
        (
            lines[0]["native_agent"].as_str(),
            lines[0]["native_event_id"].as_str()
        ),
        (Some("codex"), Some("exec-1"))
    );

    let hooks_json = dir.path().join("hooks.json");
    let original = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"workmux set-window-status done"}]}]}});
    std::fs::write(
        &hooks_json,
        serde_json::to_string_pretty(&original).unwrap(),
    )
    .unwrap();
    let run = |verb: &str| {
        let out = chloe()
            .args(["hook", verb, "codex", "--settings"])
            .arg(&hooks_json)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(
        run("install").contains("/hooks"),
        "must tell the user to trust the hooks"
    );
    let s: Value = serde_json::from_str(&std::fs::read_to_string(&hooks_json).unwrap()).unwrap();
    assert_eq!(
        s["hooks"]["SessionEnd"][0]["hooks"][0]["command"],
        "chloe hook codex"
    );
    assert!(run("uninstall").contains("removed"));
    let s: Value = serde_json::from_str(&std::fs::read_to_string(&hooks_json).unwrap()).unwrap();
    assert_eq!(s, original);
}

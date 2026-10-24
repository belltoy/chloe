//! Hook adapters for Claude Code and Codex: parse a hook's stdin JSON into an `AgentEvent`, and
//! add or remove our entries in the agent's hooks file. Both agents use the same hook shape;
//! `Spec` holds what differs. Payload fields were captured from Claude Code 2.1.309 and Codex
//! 0.164.0 (see docs/agent-events.md).

use std::path::Path;

use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, EventKind};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::redact::{clean, mentions_sensitive_path};

/// What differs between agents.
pub struct Spec {
    pub agent: &'static str,
    /// The exact command we put in the hooks file; also how we recognise our own entries.
    pub command: &'static str,
    /// Native events we capture. Anything else is ignored.
    pub events: &'static [&'static str],
    /// Payload field that identifies a user prompt (`prompt_id` / `turn_id`).
    pub prompt_id_field: &'static str,
    /// Seconds; the hook only appends a line, so this is a ceiling, not an expectation.
    pub timeout_secs: u64,
}

pub const CLAUDE: Spec = Spec {
    agent: "claude_code",
    command: "chloe hook claude",
    events: &[
        "SessionStart",
        "UserPromptSubmit",
        "PostToolUse",
        "Stop",
        "PostCompact",
        "SessionEnd",
    ],
    prompt_id_field: "prompt_id",
    timeout_secs: 5,
};

/// `PostCompact` is left out: its payload was never captured from Codex 0.164.0.
pub const CODEX: Spec = Spec {
    agent: "codex",
    command: "chloe hook codex",
    events: &[
        "SessionStart",
        "UserPromptSubmit",
        "PostToolUse",
        "Stop",
        "SessionEnd",
    ],
    prompt_id_field: "turn_id",
    // Codex clamps `SessionEnd` hooks to 3 s and warns on every run if asked for more.
    timeout_secs: 3,
};

pub const PROMPT_CHARS: usize = 4000;
pub const SUMMARY_CHARS: usize = 300;
/// Room for the path plus what an edit replaced and what it wrote.
pub const EDIT_CHARS: usize = 1200;
/// The agent's final reply of a turn: enough for a root-cause sentence or two.
pub const ASSISTANT_CHARS: usize = 1000;
const NAME_CHARS: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hook payload is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("hook payload lacks `{0}`")]
    Missing(&'static str),
    #[error("{0}")]
    Settings(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn str_field<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `Ok(None)` for events we do not capture; `Err` when a core field (`session_id`,
/// `hook_event_name`) is missing, so the caller can count it.
pub fn parse_hook(
    spec: &Spec,
    input: &[u8],
    now: OffsetDateTime,
) -> Result<Option<AgentEvent>, Error> {
    let v: Value = serde_json::from_slice(input)?;
    let session = str_field(&v, "session_id").ok_or(Error::Missing("session_id"))?;
    let name = str_field(&v, "hook_event_name").ok_or(Error::Missing("hook_event_name"))?;
    let mut ev = AgentEvent {
        schema_version: EVENT_SCHEMA_VERSION,
        native_agent: spec.agent.into(),
        native_session_id: session.into(),
        native_event_id: None,
        kind: EventKind::SessionStart,
        occurred_at: now,
        cwd: str_field(&v, "cwd").map(String::from),
        tool_name: None,
        text: None,
        redactions: vec![],
    };
    if !spec.events.contains(&name) {
        return Ok(None);
    }
    match name {
        "SessionStart" => {}
        "SessionEnd" => ev.kind = EventKind::SessionEnd,
        "PostCompact" => ev.kind = EventKind::ContextCompacted,
        "UserPromptSubmit" => {
            ev.kind = EventKind::UserPrompt;
            ev.native_event_id = str_field(&v, spec.prompt_id_field).map(String::from);
            if let Some(p) = str_field(&v, "prompt") {
                set_text(&mut ev, clean(p, PROMPT_CHARS));
            }
        }
        "Stop" => {
            ev.kind = EventKind::AssistantMessage;
            ev.native_event_id = str_field(&v, spec.prompt_id_field).map(String::from);
            if let Some(m) = str_field(&v, "last_assistant_message") {
                set_text(&mut ev, clean(m, ASSISTANT_CHARS));
            }
        }
        "PostToolUse" => {
            ev.kind = EventKind::ToolResultSummary;
            ev.native_event_id = str_field(&v, "tool_use_id").map(String::from);
            let tool = str_field(&v, "tool_name");
            ev.tool_name = tool.map(|t| t.chars().take(NAME_CHARS).collect());
            // `tool_response` is deliberately never read: it can hold file contents or secrets.
            if let Some(raw) = tool.and_then(|t| tool_summary(t, v.get("tool_input"))) {
                if mentions_sensitive_path(&raw) {
                    ev.redactions.push("sensitive_path".into());
                } else {
                    let limit = if matches!(tool, Some("Edit" | "MultiEdit" | "apply_patch")) {
                        EDIT_CHARS
                    } else {
                        SUMMARY_CHARS
                    };
                    set_text(&mut ev, clean(&raw, limit));
                }
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(ev))
}

fn set_text(ev: &mut AgentEvent, c: crate::redact::Cleaned) {
    ev.text = Some(c.text);
    ev.redactions
        .extend(c.reasons.into_iter().map(String::from));
}

/// What the tool was asked to do, from its input only: the command, path, pattern or query.
/// An edit also carries the text it replaced and the text it wrote (`-` / `+` lines), because
/// that is the only record of what a fix was. File contents the tool returns are never read.
fn tool_summary(tool: &str, input: Option<&Value>) -> Option<String> {
    if matches!(tool, "Edit" | "MultiEdit") {
        return edit_summary(input?);
    }
    let key = match tool {
        // Codex's patch tool: the whole patch (`-`/`+` lines), the counterpart of an Edit.
        "Bash" | "apply_patch" => "command",
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "file_path",
        "Glob" | "Grep" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => return None,
    };
    str_field(input?, key).map(String::from)
}

fn edit_summary(input: &Value) -> Option<String> {
    let mut out = str_field(input, "file_path")?.to_string();
    let single = [input.clone()];
    let edits = input
        .get("edits")
        .and_then(Value::as_array)
        .map_or(&single[..], Vec::as_slice);
    for e in edits {
        for (mark, key) in [("-", "old_string"), ("+", "new_string")] {
            if let Some(s) = e.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                out.push_str(&format!(
                    "\n{mark} {}",
                    s.replace('\n', &format!("\n{mark} "))
                ));
            }
        }
    }
    Some(out)
}

fn our_handler(spec: &Spec, h: &Value) -> bool {
    h.get("command").and_then(Value::as_str) == Some(spec.command)
}

/// Add our handler to each captured event unless it is already there. Returns whether
/// anything changed. Other tools' hooks are never touched.
pub fn install(spec: &Spec, settings: &mut Value) -> Result<bool, Error> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| Error::Settings("settings.json is not a JSON object".into()))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| Error::Settings("`hooks` is not an object".into()))?;
    let mut changed = false;
    for &ev in spec.events {
        let groups = hooks
            .entry(ev)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| Error::Settings(format!("`hooks.{ev}` is not an array")))?;
        let present = groups.iter().any(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hs| hs.iter().any(|h| our_handler(spec, h)))
        });
        if !present {
            groups.push(json!({"hooks": [{"type": "command", "command": spec.command, "timeout": spec.timeout_secs}]}));
            changed = true;
        }
    }
    Ok(changed)
}

/// Remove only our handlers; drop groups, events and a `hooks` object left empty by that.
pub fn uninstall(spec: &Spec, settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        for g in groups.iter_mut() {
            if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = hs.len();
                hs.retain(|h| !our_handler(spec, h));
                changed |= hs.len() != before;
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|hs| !hs.is_empty())
        });
    }
    let emptied = |v: &Value| v.as_array().is_some_and(Vec::is_empty);
    hooks.retain(|_, v| !(changed && emptied(v)));
    if changed
        && hooks.is_empty()
        && let Some(root) = settings.as_object_mut()
    {
        root.remove("hooks");
    }
    changed
}

/// Read, modify and atomically rewrite a settings file. A missing file is created for
/// `install` and left absent for `uninstall`; an unparsable file is never touched.
pub fn edit_settings_file(spec: &Spec, path: &Path, install_it: bool) -> Result<bool, Error> {
    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut doc = match &existing {
        Some(s) if !s.trim().is_empty() => serde_json::from_str(s).map_err(|e| {
            Error::Settings(format!(
                "{} is not valid JSON ({e}); not modified",
                path.display()
            ))
        })?,
        _ if install_it => json!({}),
        _ => return Ok(false),
    };
    let changed = if install_it {
        install(spec, &mut doc)?
    } else {
        uninstall(spec, &mut doc)
    };
    if changed {
        let tmp = path.with_extension("json.chloe-tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&doc)? + "\n")?;
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, meta.permissions())?;
        }
        std::fs::rename(&tmp, path)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn parse(v: Value) -> Option<AgentEvent> {
        parse_hook(&CLAUDE, v.to_string().as_bytes(), NOW).unwrap()
    }

    // Shapes below are copied from payloads captured on Claude Code 2.1.309.
    #[test]
    fn maps_captured_events() {
        let start = parse(json!({"session_id":"s1","transcript_path":"/t","cwd":"/p","hook_event_name":"SessionStart","source":"startup"})).unwrap();
        assert_eq!(
            (start.kind, start.cwd.as_deref(), start.native_event_id),
            (EventKind::SessionStart, Some("/p"), None)
        );

        let prompt = parse(json!({"session_id":"s1","prompt_id":"p1","hook_event_name":"UserPromptSubmit","prompt":"fix the bug"})).unwrap();
        assert_eq!(
            (
                prompt.kind,
                prompt.native_event_id.as_deref(),
                prompt.text.as_deref()
            ),
            (EventKind::UserPrompt, Some("p1"), Some("fix the bug"))
        );

        let tool = parse(json!({"session_id":"s1","hook_event_name":"PostToolUse","tool_name":"Bash",
            "tool_input":{"command":"echo hi"},"tool_response":{"stdout":"SECRET-OUTPUT"},"tool_use_id":"toolu_1"})).unwrap();
        assert_eq!(
            (
                tool.kind,
                tool.native_event_id.as_deref(),
                tool.text.as_deref()
            ),
            (
                EventKind::ToolResultSummary,
                Some("toolu_1"),
                Some("echo hi")
            )
        );
        assert!(
            !serde_json::to_string(&tool)
                .unwrap()
                .contains("SECRET-OUTPUT")
        );

        assert_eq!(
            parse(json!({"session_id":"s1","hook_event_name":"SessionEnd","reason":"other"}))
                .unwrap()
                .kind,
            EventKind::SessionEnd
        );
        assert_eq!(
            parse(json!({"session_id":"s1","hook_event_name":"PostCompact","trigger":"auto"}))
                .unwrap()
                .kind,
            EventKind::ContextCompacted
        );
    }

    #[test]
    fn ignores_other_events_and_rejects_bad_core_fields() {
        assert!(parse(json!({"session_id":"s","hook_event_name":"PreToolUse"})).is_none());
        assert!(parse(json!({"session_id":"s","hook_event_name":"FutureEvent","new":1})).is_none());
        for bad in [
            json!({"hook_event_name":"SessionStart"}),
            json!({"session_id":"","hook_event_name":"SessionStart"}),
            json!({"session_id":"s"}),
        ] {
            assert!(parse_hook(&CLAUDE, bad.to_string().as_bytes(), NOW).is_err());
        }
        assert!(parse_hook(&CLAUDE, b"not json", NOW).is_err());
    }

    #[test]
    fn redacts_limits_and_drops_sensitive_paths() {
        let key = "sk-abcdefghijklmnopqrstuvwxyz0123456789";
        let p = parse(json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":format!("use {key} and {}", "x".repeat(9000))})).unwrap();
        let text = p.text.unwrap();
        assert!(!text.contains("sk-abc") && text.chars().count() <= PROMPT_CHARS + 20);
        assert_eq!(p.redactions, vec!["secret", "truncated"]);

        let read = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Read","tool_input":{"file_path":"/home/u/.env"},"tool_use_id":"t"})).unwrap();
        assert_eq!(
            (read.text, read.redactions),
            (None, vec!["sensitive_path".to_string()])
        );
        let cat = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"cat ~/.ssh/id_rsa"},"tool_use_id":"t"})).unwrap();
        assert_eq!(cat.text, None);
        // An MCP or unknown tool gets no summary at all.
        let mcp = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"mcp__x__y","tool_input":{"a":"b"},"tool_use_id":"t"})).unwrap();
        assert_eq!(mcp.text, None);
    }

    // Shapes copied from payloads captured on Codex 0.164.0: `turn_id` instead of `prompt_id`,
    // and `tool_response` is a plain string.
    #[test]
    fn maps_captured_codex_events() {
        let parse = |v: Value| parse_hook(&CODEX, v.to_string().as_bytes(), NOW).unwrap();
        let start = parse(json!({"session_id":"c1","transcript_path":"/t","cwd":"/p","hook_event_name":"SessionStart","model":"m","permission_mode":"default","source":"startup"})).unwrap();
        assert_eq!(
            (start.native_agent.as_str(), start.kind),
            ("codex", EventKind::SessionStart)
        );

        let prompt = parse(json!({"session_id":"c1","turn_id":"t1","hook_event_name":"UserPromptSubmit","prompt":"fix it"})).unwrap();
        assert_eq!(
            (prompt.native_event_id.as_deref(), prompt.text.as_deref()),
            (Some("t1"), Some("fix it"))
        );

        let tool = parse(json!({"session_id":"c1","turn_id":"t1","hook_event_name":"PostToolUse","tool_name":"Bash",
            "tool_input":{"command":"echo hi"},"tool_response":"SECRET-OUTPUT\n","tool_use_id":"exec-1"})).unwrap();
        assert_eq!(
            (tool.native_event_id.as_deref(), tool.text.as_deref()),
            (Some("exec-1"), Some("echo hi"))
        );
        assert!(
            !serde_json::to_string(&tool)
                .unwrap()
                .contains("SECRET-OUTPUT")
        );

        assert_eq!(
            parse(json!({"session_id":"c1","hook_event_name":"SessionEnd","reason":"other"}))
                .unwrap()
                .kind,
            EventKind::SessionEnd
        );
        // Not captured for Codex: turn end, and compaction (payload never seen).
        let stop = parse(json!({"session_id":"c1","turn_id":"t1","hook_event_name":"Stop","last_assistant_message":"done"})).unwrap();
        assert_eq!(
            (
                stop.kind,
                stop.native_event_id.as_deref(),
                stop.text.as_deref()
            ),
            (EventKind::AssistantMessage, Some("t1"), Some("done"))
        );
        assert!(parse(json!({"session_id":"c1","hook_event_name":"PostCompact"})).is_none());
    }

    #[test]
    fn codex_install_uses_its_own_command_and_events() {
        let mut s = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"workmux set-window-status done"}]}]}});
        assert!(install(&CODEX, &mut s).unwrap());
        assert!(s["hooks"].get("PostCompact").is_none());
        assert_eq!(
            s["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "chloe hook codex"
        );
        assert_eq!(s["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 3);
        // Our Claude entries and Codex entries are told apart.
        assert!(install(&CLAUDE, &mut s).unwrap());
        assert!(uninstall(&CODEX, &mut s));
        assert_eq!(
            s["hooks"]["SessionEnd"][0]["hooks"][0]["command"],
            "chloe hook claude"
        );
        assert!(uninstall(&CLAUDE, &mut s));
        assert_eq!(
            s,
            json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"workmux set-window-status done"}]}]}})
        );
    }

    fn foreign() -> Value {
        json!({"model":"x","hooks":{
            "SessionStart":[{"hooks":[{"type":"command","command":"workmux register-agent"}],"matcher":"startup"}],
            "Stop":[{"hooks":[{"type":"command","command":"other stop"}]}]}})
    }

    #[test]
    fn install_appends_idempotently_and_uninstall_restores() {
        let original = foreign();
        let mut s = original.clone();
        assert!(install(&CLAUDE, &mut s).unwrap());
        assert!(
            !install(&CLAUDE, &mut s).unwrap(),
            "second install must be a no-op"
        );
        for &ev in CLAUDE.events {
            let n = s["hooks"][ev]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| {
                    g["hooks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|h| our_handler(&CLAUDE, h))
                })
                .count();
            assert_eq!(n, 1, "{ev}");
        }
        assert_eq!(
            s["hooks"]["SessionStart"][0],
            original["hooks"]["SessionStart"][0]
        );
        assert!(uninstall(&CLAUDE, &mut s));
        assert_eq!(s, original);
        assert!(!uninstall(&CLAUDE, &mut s));
    }

    #[test]
    fn uninstall_on_fresh_file_removes_the_hooks_key_and_keeps_key_order() {
        let mut s = json!({"zeta":1,"alpha":2});
        install(&CLAUDE, &mut s).unwrap();
        assert!(uninstall(&CLAUDE, &mut s));
        assert_eq!(s, json!({"zeta":1,"alpha":2}));
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"zeta":1,"alpha":2}"#
        );
    }

    #[test]
    fn file_edit_refuses_invalid_json_and_handles_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert!(
            !edit_settings_file(&CLAUDE, &p, false).unwrap(),
            "uninstall of a missing file is a no-op"
        );
        assert!(!p.exists());
        assert!(edit_settings_file(&CLAUDE, &p, true).unwrap());
        assert!(p.exists() && !dir.path().join("settings.json.chloe-tmp").exists());
        std::fs::write(&p, "{ not json").unwrap();
        assert!(edit_settings_file(&CLAUDE, &p, true).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
    }

    #[test]
    fn an_edit_keeps_what_it_replaced_and_wrote_but_redacted_and_bounded() {
        let e = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Edit",
            "tool_input":{"file_path":"/p/a.py","old_string":"return n // k","new_string":"return -(-n // k)  # key sk-abcdefghijklmnopqrstuvwxyz0123456789"},
            "tool_response":{"filePath":"/p/a.py","structuredPatch":"SECRET-OUTPUT"},"tool_use_id":"t"})).unwrap();
        let text = e.text.unwrap();
        assert!(
            text.starts_with("/p/a.py\n- return n // k\n+ return -(-n // k)"),
            "{text}"
        );
        assert!(
            !text.contains("sk-abc") && !text.contains("SECRET-OUTPUT"),
            "{text}"
        );
        let multi = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"MultiEdit",
            "tool_input":{"file_path":"/p/a.py","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]},"tool_use_id":"t"})).unwrap();
        assert_eq!(multi.text.unwrap(), "/p/a.py\n- a\n+ b\n- c\n+ d");
        let long = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Edit",
            "tool_input":{"file_path":"/p/a.py","old_string":"x".repeat(5000),"new_string":"y"},"tool_use_id":"t"})).unwrap();
        assert!(long.text.unwrap().chars().count() <= EDIT_CHARS + 20);
        // Editing a credential file keeps no text at all.
        let env = parse(json!({"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Edit",
            "tool_input":{"file_path":"/p/.env","old_string":"A=1","new_string":"A=2"},"tool_use_id":"t"})).unwrap();
        assert!(env.text.is_none());
    }

    #[test]
    fn the_final_reply_is_kept_redacted_and_cut() {
        let long = format!(
            "Root cause: floor division. key sk-abcdefghijklmnopqrstuvwxyz0123456789 {}",
            "x".repeat(3000)
        );
        let e = parse(
            json!({"session_id":"s","hook_event_name":"Stop","prompt_id":"p1",
            "last_assistant_message":long,"stop_hook_active":false}),
        )
        .unwrap();
        assert_eq!(
            (e.kind, e.native_event_id.as_deref()),
            (EventKind::AssistantMessage, Some("p1"))
        );
        let text = e.text.unwrap();
        assert!(text.starts_with("Root cause: floor division."));
        assert!(
            !text.contains("sk-abc") && text.chars().count() <= ASSISTANT_CHARS + 20,
            "{}",
            text.len()
        );
        // A Stop without a message is still an event, with no text.
        assert!(
            parse(json!({"session_id":"s","hook_event_name":"Stop"}))
                .unwrap()
                .text
                .is_none()
        );
    }

    fn parse_codex(v: Value) -> Option<AgentEvent> {
        parse_hook(&CODEX, v.to_string().as_bytes(), NOW).unwrap()
    }

    #[test]
    fn a_codex_patch_is_kept_like_an_edit() {
        // Shape copied from a payload captured on codex-cli 0.164.0.
        let patch = "*** Begin Patch\n*** Update File: /p/pager.py\n@@\n-    return n // k\n+    return (n + k - 1) // k\n*** End Patch";
        let e = parse_codex(json!({"session_id":"c1","turn_id":"t1","hook_event_name":"PostToolUse","tool_name":"apply_patch",
            "tool_input":{"command":patch},"tool_response":"Exit code: 0\nOutput:\nSuccess. Updated: M /p/pager.py","tool_use_id":"u1"})).unwrap();
        assert_eq!(e.text.as_deref(), Some(patch));
        assert_eq!(e.tool_name.as_deref(), Some("apply_patch"));
        let env = parse_codex(
            json!({"session_id":"c1","hook_event_name":"PostToolUse","tool_name":"apply_patch",
            "tool_input":{"command":"*** Update File: /p/.env\n-A=1\n+A=2"},"tool_use_id":"u2"}),
        )
        .unwrap();
        assert!(env.text.is_none());
    }
}

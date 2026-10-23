//! Claude Code hook adapter: parse a hook's stdin JSON into an `AgentEvent`, and add or remove
//! our entries in a Claude Code `settings.json`. Payload fields were captured from
//! Claude Code 2.1.309 (see docs/agent-events.md).

use std::path::Path;

use memory_core::{AgentEvent, EVENT_SCHEMA_VERSION, EventKind};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::redact::{clean, mentions_sensitive_path};

pub const AGENT: &str = "claude_code";
/// The exact command we put in `settings.json`; it is also how we recognise our own entries.
pub const HOOK_COMMAND: &str = "chloe hook claude";
/// Native events we capture. Anything else is ignored.
pub const EVENTS: [&str; 5] = [
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
    "PostCompact",
    "SessionEnd",
];
/// Seconds; the hook only appends a line, so this is a ceiling, not an expectation.
const HOOK_TIMEOUT_SECS: u64 = 5;

pub const PROMPT_CHARS: usize = 4000;
pub const SUMMARY_CHARS: usize = 300;
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
pub fn parse_hook(input: &[u8], now: OffsetDateTime) -> Result<Option<AgentEvent>, Error> {
    let v: Value = serde_json::from_slice(input)?;
    let session = str_field(&v, "session_id").ok_or(Error::Missing("session_id"))?;
    let name = str_field(&v, "hook_event_name").ok_or(Error::Missing("hook_event_name"))?;
    let mut ev = AgentEvent {
        schema_version: EVENT_SCHEMA_VERSION,
        native_agent: AGENT.into(),
        native_session_id: session.into(),
        native_event_id: None,
        kind: EventKind::SessionStart,
        occurred_at: now,
        cwd: str_field(&v, "cwd").map(String::from),
        tool_name: None,
        text: None,
        redactions: vec![],
    };
    match name {
        "SessionStart" => {}
        "SessionEnd" => ev.kind = EventKind::SessionEnd,
        "PostCompact" => ev.kind = EventKind::ContextCompacted,
        "UserPromptSubmit" => {
            ev.kind = EventKind::UserPrompt;
            ev.native_event_id = str_field(&v, "prompt_id").map(String::from);
            if let Some(p) = str_field(&v, "prompt") {
                set_text(&mut ev, clean(p, PROMPT_CHARS));
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
                    set_text(&mut ev, clean(&raw, SUMMARY_CHARS));
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
fn tool_summary(tool: &str, input: Option<&Value>) -> Option<String> {
    let key = match tool {
        "Bash" => "command",
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "file_path",
        "Glob" | "Grep" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => return None,
    };
    str_field(input?, key).map(String::from)
}

fn our_handler(h: &Value) -> bool {
    h.get("command").and_then(Value::as_str) == Some(HOOK_COMMAND)
}

/// Add our handler to each captured event unless it is already there. Returns whether
/// anything changed. Other tools' hooks are never touched.
pub fn install(settings: &mut Value) -> Result<bool, Error> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| Error::Settings("settings.json is not a JSON object".into()))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| Error::Settings("`hooks` is not an object".into()))?;
    let mut changed = false;
    for ev in EVENTS {
        let groups = hooks
            .entry(ev)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| Error::Settings(format!("`hooks.{ev}` is not an array")))?;
        let present = groups.iter().any(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hs| hs.iter().any(our_handler))
        });
        if !present {
            groups.push(json!({"hooks": [{"type": "command", "command": HOOK_COMMAND, "timeout": HOOK_TIMEOUT_SECS}]}));
            changed = true;
        }
    }
    Ok(changed)
}

/// Remove only our handlers; drop groups, events and a `hooks` object left empty by that.
pub fn uninstall(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        for g in groups.iter_mut() {
            if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = hs.len();
                hs.retain(|h| !our_handler(h));
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
    if changed && hooks.is_empty() {
        if let Some(root) = settings.as_object_mut() {
            root.remove("hooks");
        }
    }
    changed
}

/// Read, modify and atomically rewrite a settings file. A missing file is created for
/// `install` and left absent for `uninstall`; an unparsable file is never touched.
pub fn edit_settings_file(path: &Path, install_it: bool) -> Result<bool, Error> {
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
        install(&mut doc)?
    } else {
        uninstall(&mut doc)
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
        parse_hook(v.to_string().as_bytes(), NOW).unwrap()
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
        assert!(parse(json!({"session_id":"s","hook_event_name":"Stop"})).is_none());
        assert!(parse(json!({"session_id":"s","hook_event_name":"FutureEvent","new":1})).is_none());
        for bad in [
            json!({"hook_event_name":"SessionStart"}),
            json!({"session_id":"","hook_event_name":"SessionStart"}),
            json!({"session_id":"s"}),
        ] {
            assert!(parse_hook(bad.to_string().as_bytes(), NOW).is_err());
        }
        assert!(parse_hook(b"not json", NOW).is_err());
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

    fn foreign() -> Value {
        json!({"model":"x","hooks":{
            "SessionStart":[{"hooks":[{"type":"command","command":"workmux register-agent"}],"matcher":"startup"}],
            "Stop":[{"hooks":[{"type":"command","command":"other stop"}]}]}})
    }

    #[test]
    fn install_appends_idempotently_and_uninstall_restores() {
        let original = foreign();
        let mut s = original.clone();
        assert!(install(&mut s).unwrap());
        assert!(!install(&mut s).unwrap(), "second install must be a no-op");
        for ev in EVENTS {
            let n = s["hooks"][ev]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| g["hooks"].as_array().unwrap().iter().any(our_handler))
                .count();
            assert_eq!(n, 1, "{ev}");
        }
        assert_eq!(
            s["hooks"]["SessionStart"][0],
            original["hooks"]["SessionStart"][0]
        );
        assert!(uninstall(&mut s));
        assert_eq!(s, original);
        assert!(!uninstall(&mut s));
    }

    #[test]
    fn uninstall_on_fresh_file_removes_the_hooks_key_and_keeps_key_order() {
        let mut s = json!({"zeta":1,"alpha":2});
        install(&mut s).unwrap();
        assert!(uninstall(&mut s));
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
            !edit_settings_file(&p, false).unwrap(),
            "uninstall of a missing file is a no-op"
        );
        assert!(!p.exists());
        assert!(edit_settings_file(&p, true).unwrap());
        assert!(p.exists() && !dir.path().join("settings.json.chloe-tmp").exists());
        std::fs::write(&p, "{ not json").unwrap();
        assert!(edit_settings_file(&p, true).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
    }
}

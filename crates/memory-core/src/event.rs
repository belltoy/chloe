//! Normalized agent events (B0-01). Agent-specific formats are mapped to this by adapters;
//! nothing here knows about a particular agent.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Bumped when a field is removed or changes meaning; adding optional fields does not bump it.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    SessionStart,
    UserPrompt,
    ToolResultSummary,
    ContextCompacted,
    SessionEnd,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::UserPrompt => "user_prompt",
            Self::ToolResultSummary => "tool_result_summary",
            Self::ContextCompacted => "context_compacted",
            Self::SessionEnd => "session_end",
        }
    }
}

/// One captured event. Everything except the identity fields is optional because agents
/// expose different data. `text` and `tool_name` are already redacted and length-limited by
/// the adapter; the core never sees raw tool output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    pub schema_version: u32,
    /// Adapter name such as `claude_code` or `codex`; an open string, not an enum, so a new
    /// agent needs no core change.
    pub native_agent: String,
    pub native_session_id: String,
    /// The agent's own id for this event when it has one (e.g. a tool call id or prompt id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_event_id: Option<String>,
    pub kind: EventKind,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Reason codes for what the adapter removed or cut (`secret`, `truncated`,
    /// `sensitive_path`); never the removed content itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redactions: Vec<String>,
}

impl AgentEvent {
    /// Stable key for dropping duplicate captures: `(agent, session, kind, native_event_id)`.
    /// Without a native id, repeats of the same kind in one session (a resumed `SessionStart`,
    /// say) collapse into one; adapters should supply an id wherever the agent has one.
    pub fn idempotency_key(&self) -> String {
        // Length-prefixed so no field value can shift into its neighbour.
        let id = self.native_event_id.as_deref().unwrap_or("");
        let parts = [
            self.native_agent.as_str(),
            self.native_session_id.as_str(),
            self.kind.as_str(),
            id,
        ];
        parts
            .iter()
            .map(|p| format!("{}:{p}", p.len()))
            .collect::<Vec<_>>()
            .join("|")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(session: &str, id: Option<&str>) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: session.into(),
            native_event_id: id.map(Into::into),
            kind: EventKind::ToolResultSummary,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            cwd: None,
            tool_name: Some("Bash".into()),
            text: None,
            redactions: vec![],
        }
    }

    #[test]
    fn key_is_stable_and_separates_fields() {
        assert_eq!(
            ev("s", Some("t1")).idempotency_key(),
            ev("s", Some("t1")).idempotency_key()
        );
        assert_ne!(
            ev("s", Some("t1")).idempotency_key(),
            ev("s", Some("t2")).idempotency_key()
        );
        // "a|1:b" + "" must not collide with "a" + "b".
        assert_ne!(
            ev("a", Some("b")).idempotency_key(),
            ev("a|1:b", None).idempotency_key()
        );
    }

    #[test]
    fn tolerates_unknown_fields_and_roundtrips() {
        let json = r#"{"schema_version":1,"native_agent":"codex","native_session_id":"s",
            "kind":"session_end","occurred_at":"1970-01-01T00:00:00Z","future_field":42}"#;
        let e: AgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(e.kind, EventKind::SessionEnd);
        let back: AgentEvent = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}

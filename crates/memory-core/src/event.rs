//! Normalized agent events (B0-01). Agent-specific formats are mapped to this by adapters;
//! nothing here knows about a particular agent.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{ProjectId, Result};

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
    /// An event without a native id is identified by `occurred_at` instead, which the adapter
    /// fixed when it captured it: re-importing the same capture gives the same key, while two
    /// real `SessionEnd`s of a resumed session stay distinct.
    pub fn idempotency_key(&self) -> String {
        // Length-prefixed so no field value can shift into its neighbour.
        let id = match &self.native_event_id {
            Some(id) => id.clone(),
            None => format!("@{}", self.occurred_at.unix_timestamp_nanos()),
        };
        let parts = [
            self.native_agent.as_str(),
            self.native_session_id.as_str(),
            self.kind.as_str(),
            id.as_str(),
        ];
        parts
            .iter()
            .map(|p| format!("{}:{p}", p.len()))
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Why an event was not stored. `index` is its position in the submitted batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    pub index: usize,
    /// `unsupported_schema_version`, `invalid_field` or `too_long`.
    pub reason: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestReport {
    pub inserted: usize,
    /// Already stored (same idempotency key); nothing changed.
    pub duplicates: usize,
    pub rejected: Vec<Rejection>,
}

/// `Ended` only when the latest event of the session is a `SessionEnd`; a session whose end
/// was never delivered stays `Open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Open,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub native_agent: String,
    pub native_session_id: String,
    /// `None` when the session's directory is not a registered project.
    pub project_id: Option<ProjectId>,
    pub cwd: Option<String>,
    pub state: SessionState,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_event_at: OffsetDateTime,
    pub events: usize,
}

/// Durable, idempotent storage of captured events (implemented in `memory-store`).
pub trait EventStore {
    /// Stores each valid event once; rejected events are counted, never fatal to the batch.
    fn ingest(&self, events: &[AgentEvent]) -> Result<IngestReport>;
    /// Most recently active sessions first.
    fn sessions(&self, limit: usize) -> Result<Vec<SessionInfo>>;
    /// Events of one session in arrival order.
    fn session_events(&self, native_session_id: &str, limit: usize) -> Result<Vec<AgentEvent>>;
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
        // Without a native id, the capture time tells two real events apart.
        let mut later = ev("s", None);
        later.occurred_at += time::Duration::seconds(1);
        assert_ne!(ev("s", None).idempotency_key(), later.idempotency_key());
        assert_eq!(
            ev("s", None).idempotency_key(),
            ev("s", None).idempotency_key()
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

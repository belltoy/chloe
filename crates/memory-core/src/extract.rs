//! Memory extraction contract (B1-02). Providers (a local model, an API) implement
//! [`MemoryExtractor`]; the core knows nothing about HTTP or any particular model. What a
//! provider returns is a *candidate* — untrusted model output, never a stored memory.

use serde::{Deserialize, Serialize};

use crate::{AgentEvent, Kind, Result};

/// A session's events, already redacted by the adapter, in the order they happened.
#[derive(Debug, Clone)]
pub struct ExtractionInput {
    pub native_agent: String,
    pub native_session_id: String,
    pub events: Vec<AgentEvent>,
}

/// Where a candidate came from. `session` and `events` are filled in by us from the input,
/// never taken from the model; `file` and `commit` are kept only when the cited events
/// actually mention them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub native_agent: String,
    pub native_session_id: String,
    /// Idempotency keys of the cited events (see `AgentEvent::idempotency_key`).
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub kind: Kind,
    pub title: String,
    pub content: String,
    pub evidence: Evidence,
}

pub trait MemoryExtractor {
    /// Short provider name for logs and status, e.g. `ollama:qwen3:8b`.
    fn name(&self) -> String;

    /// `Error::Unavailable` when the provider cannot be reached, so the caller can keep the
    /// events and retry later instead of treating it as a bad session.
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>>;
}

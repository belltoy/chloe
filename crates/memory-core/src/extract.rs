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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub kind: Kind,
    pub title: String,
    pub content: String,
    /// Idempotency keys of the events the candidate is based on.
    #[serde(default)]
    pub evidence: Vec<String>,
}

pub trait MemoryExtractor {
    /// Short provider name for logs and status, e.g. `ollama:qwen3:8b`.
    fn name(&self) -> String;

    /// `Error::Unavailable` when the provider cannot be reached, so the caller can keep the
    /// events and retry later instead of treating it as a bad session.
    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>>;
}

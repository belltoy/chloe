//! Memory extraction contract (B1-02). Providers (a local model, an API) implement
//! [`MemoryExtractor`]; the core knows nothing about HTTP or any particular model. What a
//! provider returns is a *candidate* — untrusted model output, never a stored memory.

use serde::{Deserialize, Serialize};

use crate::{AgentEvent, Kind, Memory, MemoryId, ProjectId, Result};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateState {
    Pending,
    Accepted,
    Rejected,
    /// Folded into an existing memory.
    Merged,
    /// Looks like it contradicts an existing memory; waits for a human.
    Conflict,
}

impl CandidateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Merged => "merged",
            Self::Conflict => "conflict",
        }
    }

    /// Still waiting for a decision.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::Conflict)
    }
}

/// What a reviewer (or, later, a policy) decides about an open candidate. `reason` is kept
/// with the decision so it can be explained afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Create a memory (source `Extracted`) from the candidate. A candidate whose session has
    /// no registered project becomes a memory only with `global`.
    Accept {
        global: bool,
    },
    Reject {
        reason: String,
    },
    /// Keep the existing memory and record the candidate as one more source of it.
    Merge {
        into: MemoryId,
        reason: String,
    },
    /// Flag a contradiction with an existing memory; the candidate stays reviewable.
    Conflict {
        with: MemoryId,
        reason: String,
    },
    /// Create a memory from the candidate and mark `target` as superseded by it, linking the
    /// two. The target must be active and in the same scope as the new memory. A constraint
    /// or decision is superseded only with `by_human`, which the automatic policy never sets.
    Supersede {
        target: MemoryId,
        reason: String,
        global: bool,
        by_human: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedAction {
    /// Nothing similar enough: a new memory.
    Create,
    /// Says what `target` already says.
    Merge,
    /// Contradicts `target`.
    Conflict,
    /// Replaces `target`: a newer version of the same point. Only a person applies it.
    Supersede,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalMethod {
    Rules,
    Model,
}

/// An existing memory found by full-text search, kept so the proposal can be explained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarMemory {
    pub memory_id: MemoryId,
    pub title: String,
    pub score: f64,
}

/// What consolidation suggests doing with a candidate. It is advice, not a decision: nothing
/// is applied until a policy (B2-03) or a person calls `resolve_candidate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub action: ProposedAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<MemoryId>,
    pub method: ProposalMethod,
    /// Plain-language explanation.
    pub reason: String,
    pub similar: Vec<SimilarMemory>,
    /// Set by the safety policy when it will not apply this proposal on its own: why a person
    /// has to decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_review: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Duplicate,
    Contradicts,
    /// The new note says it replaces the existing one (a changed decision, a corrected fact).
    Supersedes,
    Unrelated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub relation: Relation,
    /// The memory the relation is about; `None` only for `Unrelated`.
    pub memory: Option<MemoryId>,
    pub reason: String,
}

/// Compares a candidate with the similar memories full-text search found. Implemented by a
/// model provider; `Error::Unavailable` when it cannot be reached.
pub trait RelationJudge {
    fn judge(&self, candidate: &Candidate, similar: &[Memory]) -> Result<Verdict>;
}

/// A candidate as stored, with its review state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredCandidate {
    pub id: String,
    pub project_id: Option<ProjectId>,
    #[serde(flatten)]
    pub candidate: Candidate,
    pub state: CandidateState,
    /// The memory it created, was merged into, or conflicts with.
    pub memory_id: Option<MemoryId>,
    pub reason: Option<String>,
    /// Latest suggestion for an open candidate.
    pub proposal: Option<Proposal>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub decided_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRelation {
    /// The memory was created from this candidate.
    Created,
    /// The candidate was merged into the memory.
    Merged,
}

/// Where a memory came from: the candidate and the session evidence behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemorySource {
    pub candidate_id: String,
    pub relation: SourceRelation,
    pub evidence: Evidence,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkRelation {
    Supersedes,
}

/// `from` is the newer memory and `to` the one it replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryLink {
    pub from: MemoryId,
    pub to: MemoryId,
    pub relation: LinkRelation,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

pub trait CandidateStore {
    /// Stores the candidates of one captured session as `pending`; one with the same kind and
    /// title as an earlier one is skipped. Returns how many were new. `NotFound` when the
    /// session was never captured.
    fn add_candidates(
        &self,
        native_agent: &str,
        native_session_id: &str,
        candidates: &[Candidate],
    ) -> Result<usize>;
    /// Newest first; `None` lists every state.
    fn candidates(
        &self,
        state: Option<CandidateState>,
        limit: usize,
    ) -> Result<Vec<StoredCandidate>>;
    fn candidate(&self, id: &str) -> Result<StoredCandidate>;
    /// Stores (replaces) the proposal of an open candidate; any other state is
    /// `InvalidArgument`.
    fn set_proposal(&self, id: &str, proposal: &Proposal) -> Result<()>;
    /// Applies a decision to a `pending` or `conflict` candidate in one transaction; any
    /// other state is `InvalidArgument`. Merge and conflict targets must be visible from the
    /// candidate's project (its own or global), otherwise `NotFound`.
    fn resolve_candidate(&self, id: &str, resolution: Resolution) -> Result<StoredCandidate>;
    /// Links in which the memory is either end, oldest first.
    fn memory_links(&self, memory: MemoryId) -> Result<Vec<MemoryLink>>;
    /// The candidates a memory was created from or merged with, oldest first.
    fn memory_sources(&self, memory: MemoryId) -> Result<Vec<MemorySource>>;
}

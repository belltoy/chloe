//! Model output is a proposal, not a fact (B1-03): every candidate is checked here and
//! anything that fails is dropped with a reason code (never the text), so a bad reply can
//! at worst produce fewer candidates.

use memory_core::extract::{Candidate, Evidence, ExtractionInput};
use memory_core::{Error, EventKind, Kind, Result};
use memory_ingest::redact;
use serde::Deserialize;
use serde_json::Value;

use crate::prompt::Prompt;

pub const MAX_TITLE_CHARS: usize = 100;
pub const MAX_CONTENT_CHARS: usize = 1500;
pub const MAX_CANDIDATES: usize = 8;
const MAX_PATH_CHARS: usize = 300;

/// What the model is asked to return; deliberately separate from [`Candidate`].
#[derive(Debug, Deserialize)]
pub struct Raw {
    pub kind: Kind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub accepted: Vec<Candidate>,
    /// Reason codes of dropped candidates: `bad_title`, `bad_content`, `no_evidence`,
    /// `secret`, `sensitive_path`, `unstated_rule`, `too_many`.
    pub rejected: Vec<&'static str>,
}

fn valid_commit(c: &str) -> bool {
    (7..=40).contains(&c.len()) && c.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// A model reply (already parsed JSON) to checked candidates. Provider-independent, so tests
/// and other providers go through the same checks.
pub fn from_reply(input: &ExtractionInput, prompt: &Prompt, reply: Value) -> Result<Outcome> {
    #[derive(Deserialize)]
    struct Reply {
        candidates: Vec<Raw>,
    }
    let reply: Reply = serde_json::from_value(reply)
        .map_err(|_| Error::InvalidArgument("model output does not match the schema".into()))?;
    Ok(validate(input, prompt, reply.candidates))
}

pub fn validate(input: &ExtractionInput, prompt: &Prompt, raw: Vec<Raw>) -> Outcome {
    let mut out = Outcome::default();
    for r in raw {
        if out.accepted.len() >= MAX_CANDIDATES {
            out.rejected.push("too_many");
            continue;
        }
        match check(input, prompt, r) {
            Ok(c) => out.accepted.push(c),
            Err(reason) => out.rejected.push(reason),
        }
    }
    out
}

fn check(
    input: &ExtractionInput,
    prompt: &Prompt,
    r: Raw,
) -> std::result::Result<Candidate, &'static str> {
    let title = r.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err("bad_title");
    }
    let content = r.content.trim().to_string();
    if content.is_empty() || content.chars().count() > MAX_CONTENT_CHARS {
        return Err("bad_content");
    }
    // Refs the model made up (not in the prompt) are ignored; a candidate left with none
    // has no evidence and is dropped.
    let mut cited: Vec<usize> = r
        .evidence
        .iter()
        .filter_map(|e| prompt.refs.iter().position(|(r, _)| r == e.trim()))
        .collect();
    cited.sort_unstable();
    cited.dedup();
    if cited.is_empty() {
        return Err("no_evidence");
    }
    // A constraint or a decision is something a person stated or chose; one backed only by the
    // agent's own messages and tool output is the agent's idea (or a workaround) taken for a rule.
    if matches!(r.kind, Kind::Constraint | Kind::Decision)
        && !cited.iter().any(|&i| {
            input
                .events
                .get(i)
                .is_some_and(|e| e.kind == EventKind::UserPrompt)
        })
    {
        return Err("unstated_rule");
    }
    // Reject rather than redact: a model that reproduces a secret is not to be trusted
    // with the rest of the text either.
    let leaks = |s: &str| redact::redact(s).1 > 0;
    if leaks(&title) || leaks(&content) {
        return Err("secret");
    }
    if redact::mentions_sensitive_path(&title) || redact::mentions_sensitive_path(&content) {
        return Err("sensitive_path");
    }
    // File and commit are kept only if a cited event literally shows them.
    let shown = |needle: &str| cited.iter().any(|&i| prompt.texts[i].contains(needle));
    let file = r
        .file
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty() && f.chars().count() <= MAX_PATH_CHARS)
        .filter(|f| !redact::mentions_sensitive_path(f) && shown(f));
    let commit = r
        .commit
        .map(|c| c.trim().to_string())
        .filter(|c| valid_commit(c) && shown(c));
    Ok(Candidate {
        kind: r.kind,
        title,
        content,
        evidence: Evidence {
            native_agent: input.native_agent.clone(),
            native_session_id: input.native_session_id.clone(),
            events: cited.iter().map(|&i| prompt.refs[i].1.clone()).collect(),
            file,
            commit,
        },
    })
}

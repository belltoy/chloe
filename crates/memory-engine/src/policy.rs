//! Safety policy (B2-03): what may be applied without a person. Deliberately small and
//! fixed, not configurable.
//!
//! - Only a `merge` of a low-risk duplicate is applied: a merge leaves the existing memory
//!   untouched and records the candidate as another source, and it must be the same kind,
//!   neither side a constraint or a decision, and the target still active.
//! - A `conflict` is applied as a flag only: the candidate moves to `conflict` and waits for a
//!   person; no memory changes.
//! - Creating a memory is never automatic, and neither is superseding one (the store itself
//!   refuses to supersede a constraint or decision unless a person says so), nor anything
//!   else touching a constraint or a decision.

use memory_core::extract::{CandidateStore, Proposal, ProposedAction, Resolution, StoredCandidate};
use memory_core::service::MemoryService;
use memory_core::{Error, Kind, Memory, Result, Status};

const BATCH: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    AutoMerge,
    FlagConflict,
    /// A person decides; the text says why.
    Human(&'static str),
    /// Nothing to do: a plain `create` waits for review like any candidate.
    Leave,
}

fn guarded(kind: Kind) -> bool {
    matches!(kind, Kind::Constraint | Kind::Decision)
}

pub fn evaluate(c: &StoredCandidate, p: &Proposal, target: Option<&Memory>) -> Outcome {
    match p.action {
        ProposedAction::Create => Outcome::Leave,
        ProposedAction::Conflict => {
            if target.is_some() {
                Outcome::FlagConflict
            } else {
                Outcome::Human("the conflicting memory is gone")
            }
        }
        // Replacing a memory hides it from search everywhere, so it is always a person's call.
        ProposedAction::Supersede => match target {
            Some(t) if guarded(t.kind) => Outcome::Human(
                "the existing memory is a constraint or decision; only a person can supersede it",
            ),
            _ => Outcome::Human("replacing a memory is always decided by a person"),
        },
        ProposedAction::Merge => {
            let Some(t) = target else {
                return Outcome::Human("the merge target is gone");
            };
            if guarded(c.candidate.kind) {
                Outcome::Human("a constraint or decision is never merged automatically")
            } else if guarded(t.kind) {
                Outcome::Human("the existing memory is a constraint or decision")
            } else if c.candidate.kind != t.kind {
                Outcome::Human("the candidate and the memory are different kinds")
            } else if t.status != Status::Active {
                Outcome::Human("the existing memory is no longer active")
            } else {
                Outcome::AutoMerge
            }
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PolicyReport {
    pub merged: usize,
    pub flagged: usize,
    /// Proposals a person has to decide.
    pub needs_review: usize,
}

/// Applies what the policy allows to pending candidates that have a proposal.
pub fn apply_policy<S: MemoryService + CandidateStore>(store: &S) -> Result<PolicyReport> {
    let mut report = PolicyReport::default();
    for c in store.candidates_to_apply(BATCH)? {
        let Some(p) = &c.proposal else { continue };
        let target = match p.target {
            Some(id) => match store.get(c.project_id, id) {
                Ok(m) => Some(m),
                Err(Error::NotFound(_)) => None,
                Err(e) => return Err(e),
            },
            None => None,
        };
        let resolution = match evaluate(&c, p, target.as_ref()) {
            Outcome::Leave => continue,
            Outcome::Human(why) => {
                report.needs_review += 1;
                if p.needs_review.as_deref() != Some(why) {
                    let mut p = p.clone();
                    p.needs_review = Some(why.to_string());
                    store.set_proposal(&c.id, &p)?;
                }
                continue;
            }
            Outcome::AutoMerge => Resolution::Merge {
                into: p.target.expect("merge has a target"),
                reason: format!("auto-merged as a low-risk duplicate: {}", p.reason),
            },
            Outcome::FlagConflict => Resolution::Conflict {
                with: p.target.expect("conflict has a target"),
                reason: format!("flagged for review: {}", p.reason),
            },
        };
        let merge = matches!(resolution, Resolution::Merge { .. });
        match store.resolve_candidate(&c.id, resolution) {
            Ok(_) if merge => report.merged += 1,
            Ok(_) => report.flagged += 1,
            // Decided by someone else in the meantime.
            Err(Error::InvalidArgument(_) | Error::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}

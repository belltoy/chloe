//! Proposals for pending candidates (B2-02): find similar memories by full-text search, then
//! suggest create, merge or conflict with a reason. Nothing is applied here.

use std::collections::HashSet;

use memory_core::Result;
use memory_core::extract::{
    CandidateState, CandidateStore, Proposal, ProposalMethod, ProposedAction, Relation,
    RelationJudge, SimilarMemory, StoredCandidate,
};
use memory_core::service::{Hit, MemoryService, SearchQuery};

/// Memories compared with one candidate.
const SIMILAR: usize = 5;
/// Candidates handled per call.
const BATCH: usize = 200;
/// Words of the content added to the title when searching.
const CONTENT_WORDS: usize = 15;
/// Title word overlap (Jaccard) at which two same-kind notes count as the same point.
const SAME_TITLE: f64 = 0.8;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProposeReport {
    pub proposed: usize,
    /// Left without a proposal because the judge failed; the next run tries again.
    pub deferred: usize,
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn overlap(a: &str, b: &str) -> f64 {
    let (a, b): (HashSet<_>, HashSet<_>) = (
        words(a).into_iter().collect(),
        words(b).into_iter().collect(),
    );
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / union as f64
}

fn query(c: &StoredCandidate) -> String {
    let mut seen = HashSet::new();
    words(&c.candidate.title)
        .into_iter()
        .chain(words(&c.candidate.content).into_iter().take(CONTENT_WORDS))
        .filter(|w| seen.insert(w.clone()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn proposal(
    action: ProposedAction,
    target: Option<memory_core::MemoryId>,
    method: ProposalMethod,
    reason: String,
    similar: Vec<SimilarMemory>,
) -> Proposal {
    Proposal {
        action,
        target,
        method,
        reason,
        similar,
        needs_review: None,
    }
}

/// `Ok(None)` when the judge failed and the candidate should be retried later.
fn propose_one<S: MemoryService>(
    store: &S,
    judge: Option<&dyn RelationJudge>,
    c: &StoredCandidate,
) -> Result<Option<Proposal>> {
    let q = query(c);
    let hits: Vec<Hit> = if q.is_empty() {
        vec![]
    } else {
        store.search(
            c.project_id,
            SearchQuery {
                query: q,
                limit: SIMILAR,
                include_global: true,
                include_inactive: false,
            },
        )?
    };
    let similar: Vec<SimilarMemory> = hits
        .iter()
        .map(|h| SimilarMemory {
            memory_id: h.memory.id,
            title: h.memory.title.clone(),
            score: h.score,
        })
        .collect();
    if hits.is_empty() {
        return Ok(Some(proposal(
            ProposedAction::Create,
            None,
            ProposalMethod::Rules,
            "no similar memory found by full-text search".into(),
            similar,
        )));
    }
    // Rule: the same kind with (nearly) the same title is the same point.
    let t = &c.candidate.title;
    if let Some(h) = hits
        .iter()
        .find(|h| h.memory.kind == c.candidate.kind && overlap(t, &h.memory.title) >= SAME_TITLE)
    {
        let pct = (overlap(t, &h.memory.title) * 100.0).round();
        return Ok(Some(proposal(
            ProposedAction::Merge,
            Some(h.memory.id),
            ProposalMethod::Rules,
            format!(
                "same kind and {pct}% of the title words match memory \"{}\"",
                h.memory.title
            ),
            similar,
        )));
    }
    let Some(judge) = judge else {
        return Ok(Some(proposal(
            ProposedAction::Create,
            None,
            ProposalMethod::Rules,
            format!(
                "{} similar memories found but no model was available to compare them; check them before accepting",
                similar.len()
            ),
            similar,
        )));
    };
    let memories: Vec<_> = hits.iter().map(|h| h.memory.clone()).collect();
    let v = match judge.judge(&c.candidate, &memories) {
        Ok(v) => v,
        Err(e) => {
            // The error text carries no memory or session content.
            tracing::warn!(error = %e, "relation judge failed; candidate left for the next run");
            return Ok(None);
        }
    };
    // The target must be one of the memories we showed; otherwise the verdict is ignored.
    let target = v.memory.filter(|m| hits.iter().any(|h| h.memory.id == *m));
    Ok(Some(match (v.relation, target) {
        (Relation::Duplicate, Some(t)) => proposal(
            ProposedAction::Merge,
            Some(t),
            ProposalMethod::Model,
            v.reason,
            similar,
        ),
        (Relation::Contradicts, Some(t)) => proposal(
            ProposedAction::Conflict,
            Some(t),
            ProposalMethod::Model,
            v.reason,
            similar,
        ),
        (Relation::Unrelated, _) => proposal(
            ProposedAction::Create,
            None,
            ProposalMethod::Model,
            v.reason,
            similar,
        ),
        _ => proposal(
            ProposedAction::Create,
            None,
            ProposalMethod::Model,
            "the model named a memory that was not among the similar ones; treated as unrelated"
                .into(),
            similar,
        ),
    }))
}

/// Proposes for every pending candidate that has none yet.
pub fn propose_pending<S: MemoryService + CandidateStore>(
    store: &S,
    judge: Option<&dyn RelationJudge>,
) -> Result<ProposeReport> {
    let mut report = ProposeReport::default();
    let mut todo = store.candidates(Some(CandidateState::Pending), BATCH)?;
    todo.retain(|c| c.proposal.is_none());
    todo.reverse(); // oldest first
    for c in todo {
        match propose_one(store, judge, &c)? {
            Some(p) => {
                store.set_proposal(&c.id, &p)?;
                report.proposed += 1;
            }
            None => report.deferred += 1,
        }
    }
    Ok(report)
}

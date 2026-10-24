//! Proposals for pending candidates (B2-02): find similar memories by full-text search, then
//! suggest create, merge or conflict with a reason. Nothing is applied here.

use std::collections::HashSet;

use memory_core::Memory;
use memory_core::Result;
use memory_core::extract::{
    CandidateStore, Proposal, ProposalMethod, ProposedAction, Relation, RelationJudge,
    SimilarMemory, StoredCandidate,
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
/// Body word overlap needed on top of that before rules alone call it a duplicate.
const SAME_BODY: f64 = 0.8;

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

/// Words for comparing notes: Latin words as they are, CJK text as character pairs, so that
/// unsegmented Chinese is not one giant word.
fn tokens(s: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for w in words(s) {
        let chars: Vec<char> = w.chars().collect();
        // Everything from U+2E80 up is CJK, kana, hangul or full-width forms.
        if chars.iter().any(|c| (*c as u32) >= 0x2E80) {
            if chars.len() == 1 {
                out.insert(w);
            }
            out.extend(chars.windows(2).map(|p| p.iter().collect::<String>()));
        } else {
            out.insert(w);
        }
    }
    out
}

fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (tokens(a), tokens(b));
    let union = a.union(&b).count();
    if union == 0 {
        0.0
    } else {
        a.intersection(&b).count() as f64 / union as f64
    }
}

const DATE_WORDS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "january",
    "february",
    "march",
    "april",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Numbers and negations decide what a sentence claims, and barely move a similarity score:
/// "pool size is 8" and "pool size is 16", "is allowed" and "is not allowed".
fn claims(s: &str) -> (Vec<String>, bool) {
    const NEGATIONS: &[&str] = &[
        "not",
        "no",
        "never",
        "without",
        "cannot",
        "dont",
        "doesnt",
        "isnt",
        "arent",
        "wont",
        "disable",
        "disabled",
        "forbidden",
        "不",
        "没",
        "无",
        "别",
        "禁止",
        "不能",
        "不要",
        "不得",
        "非",
    ];
    let mut numbers: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .filter(|w| w.chars().any(|c| c.is_ascii_digit()))
        .map(str::to_lowercase)
        .collect();
    let lower = s.to_lowercase().replace('\'', "");
    // Days and months are values too: "read-only on Fridays" and "on Saturdays" differ by
    // nothing a score or a digit check would see.
    numbers.extend(
        words(&lower)
            .into_iter()
            .map(|w| w.trim_end_matches('s').to_string())
            .filter(|w| DATE_WORDS.contains(&w.as_str())),
    );
    numbers.extend(
        [
            "周一", "周二", "周三", "周四", "周五", "周六", "周日", "星期",
        ]
        .iter()
        .filter(|d| lower.contains(*d))
        .map(|d| d.to_string()),
    );
    numbers.sort();
    numbers.dedup();
    let negated = NEGATIONS.iter().any(|n| {
        if n.is_ascii() {
            words(&lower).iter().any(|w| w == n)
        } else {
            lower.contains(n)
        }
    });
    (numbers, negated)
}

/// The same point: same kind is checked by the caller; here the title and the body must both be
/// nearly the same words, with the same numbers and the same polarity.
fn same_point(c: &StoredCandidate, m: &Memory) -> bool {
    let (ct, mt) = (&c.candidate.title, &m.title);
    let (cb, mb) = (&c.candidate.content, &m.content);
    overlap(ct, mt) >= SAME_TITLE
        && similarity(cb, mb) >= SAME_BODY
        && claims(&format!("{ct} {cb}")) == claims(&format!("{mt} {mb}"))
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
    // Rule: the same kind with nearly the same title and body, and the same numbers and
    // polarity, is the same point. A similar title alone is not enough: the body may differ
    // in the one detail that matters.
    if let Some(h) = hits
        .iter()
        .find(|h| h.memory.kind == c.candidate.kind && same_point(c, &h.memory))
    {
        return Ok(Some(proposal(
            ProposedAction::Merge,
            Some(h.memory.id),
            ProposalMethod::Rules,
            format!(
                "same kind, nearly the same title and body, same numbers and polarity as memory \"{}\"",
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
    // A model that calls two notes the same while their numbers or days differ has missed the
    // one detail that matters (a corrected value is not a duplicate): a person looks. Polarity
    // is not compared here: a paraphrase ("alone does not sign" vs "must sign") flips it
    // without changing the point, and the judge itself is asked about opposites.
    let claims_differ = |t: &memory_core::MemoryId| {
        hits.iter().find(|h| h.memory.id == *t).is_some_and(|h| {
            claims(&format!("{} {}", c.candidate.title, c.candidate.content)).0
                != claims(&format!("{} {}", h.memory.title, h.memory.content)).0
        })
    };
    Ok(Some(match (v.relation, target) {
        (Relation::Duplicate, Some(t)) if claims_differ(&t) => proposal(
            ProposedAction::Conflict,
            Some(t),
            ProposalMethod::Model,
            format!(
                "the model saw a duplicate but numbers or days differ: {}",
                v.reason
            ),
            similar,
        ),
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
        (Relation::Supersedes, Some(t)) => proposal(
            ProposedAction::Supersede,
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

/// Proposes for every pending candidate that has none yet. Candidates the judge could not
/// handle are skipped for the rest of this run so they cannot hold up the newer ones.
pub fn propose_pending<S: MemoryService + CandidateStore>(
    store: &S,
    judge: Option<&dyn RelationJudge>,
) -> Result<ProposeReport> {
    let mut report = ProposeReport::default();
    let mut deferred = HashSet::new();
    loop {
        let todo: Vec<_> = store
            .candidates_to_propose(BATCH + deferred.len())?
            .into_iter()
            .filter(|c| !deferred.contains(&c.id))
            .collect();
        if todo.is_empty() {
            return Ok(report);
        }
        for c in todo {
            match propose_one(store, judge, &c)? {
                Some(p) => {
                    store.set_proposal(&c.id, &p)?;
                    report.proposed += 1;
                }
                None => {
                    deferred.insert(c.id);
                    report.deferred += 1;
                }
            }
        }
    }
}

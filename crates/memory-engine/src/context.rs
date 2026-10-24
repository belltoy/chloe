//! Context Pack (B3-06): the memories relevant to a task, as short source-linked summaries
//! grouped by kind and bounded in size. It reuses `search`, so scope, status and mode behave as
//! in search; the agent expands one memory with `memory_get`. Nothing is injected automatically.

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::service::{MemoryService, SearchQuery};
use memory_core::{Error, Kind, MemoryId, ProjectId, Result, Source};
use serde::Serialize;

use crate::{SearchMode, search};

/// Candidates considered before the budget cuts the list.
const CANDIDATES: usize = 30;
/// A summary starts at this many characters and shrinks to fit the budget.
const SUMMARY_CHARS: usize = 300;
/// Below this a summary says too little to be worth its place.
const MIN_SUMMARY_CHARS: usize = 60;
/// What an item costs besides title and summary: id, kind, source, punctuation.
const ITEM_OVERHEAD: usize = 60;
pub const DEFAULT_BUDGET: usize = 4000;
pub const MIN_BUDGET: usize = 200;
pub const MAX_BUDGET: usize = 20_000;
/// Output order: rules first, then what was decided, what bites, how to, and plain facts.
const KIND_ORDER: [Kind; 5] = [
    Kind::Constraint,
    Kind::Decision,
    Kind::Gotcha,
    Kind::Procedure,
    Kind::Fact,
];

#[derive(Debug, Clone, Serialize)]
pub struct PackItem {
    pub id: MemoryId,
    pub title: String,
    pub summary: String,
    /// True when the body is longer than `summary`: `memory_get` returns it whole.
    pub summary_truncated: bool,
    pub source: Source,
    pub revision: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackGroup {
    pub kind: Kind,
    pub items: Vec<PackItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    /// The paths that produced the pack, and the ones asked for (differs after a fallback).
    pub mode: SearchMode,
    pub requested_mode: SearchMode,
    /// Why vectors were skipped, when they were.
    pub degraded: Option<String>,
    pub budget: usize,
    /// Characters the items take (titles, summaries and a fixed overhead each); never above `budget`.
    pub used: usize,
    pub groups: Vec<PackGroup>,
    /// Relevant memories left out because the budget ran out.
    pub omitted: usize,
}

/// First `max` characters of the whitespace-collapsed text, and whether anything was cut.
fn summarize(text: &str, max: usize) -> (String, bool) {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return (flat, false);
    }
    let mut cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    (cut, true)
}

pub fn context_pack<S: MemoryService + EmbeddingStore>(
    store: &S,
    mode: SearchMode,
    embedder: Option<&dyn Embedder>,
    project: Option<ProjectId>,
    task: &str,
    include_global: bool,
    budget: usize,
) -> Result<ContextPack> {
    if !(MIN_BUDGET..=MAX_BUDGET).contains(&budget) {
        return Err(Error::InvalidArgument(format!(
            "budget must be between {MIN_BUDGET} and {MAX_BUDGET} characters"
        )));
    }
    let found = search(
        store,
        mode,
        embedder,
        project,
        SearchQuery {
            query: task.to_string(),
            limit: CANDIDATES,
            include_global,
            include_inactive: false,
        },
    )?;
    let (mut used, mut omitted) = (0, 0);
    let mut groups: Vec<PackGroup> = KIND_ORDER
        .iter()
        .map(|&kind| PackGroup {
            kind,
            items: vec![],
        })
        .collect();
    for h in found.hits {
        let m = h.hit.memory;
        let title: String = m.title.chars().take(120).collect();
        let fixed = ITEM_OVERHEAD + title.chars().count();
        let room = budget.saturating_sub(used + fixed);
        if room < MIN_SUMMARY_CHARS.min(m.content.chars().count()) {
            omitted += 1;
            continue;
        }
        let (summary, summary_truncated) = summarize(&m.content, SUMMARY_CHARS.min(room));
        used += fixed + summary.chars().count();
        if let Some(g) = groups.iter_mut().find(|g| g.kind == m.kind) {
            g.items.push(PackItem {
                id: m.id,
                title,
                summary,
                summary_truncated,
                source: m.source,
                revision: m.revision,
            });
        }
    }
    groups.retain(|g| !g.items.is_empty());
    Ok(ContextPack {
        mode: found.mode,
        requested_mode: mode,
        degraded: found.degraded,
        budget,
        used,
        groups,
        omitted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_collapse_whitespace_and_cut_on_characters() {
        assert_eq!(summarize("a  b\n c", 10), ("a b c".into(), false));
        assert_eq!(summarize("数据库并发读写", 4), ("数据库…".into(), true));
    }
}

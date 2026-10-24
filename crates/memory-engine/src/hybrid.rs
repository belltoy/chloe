//! Hybrid retrieval (B3-02): FTS top-N and vector top-N fused by reciprocal rank. Scope and
//! status are hard filters inside both paths, so fusion never sees a memory the caller may
//! not read. Only FTS failing fails the search; a vector path that is unavailable is skipped
//! and reported in `degraded`.

use std::collections::HashMap;

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::service::{Hit, MemoryService, SearchQuery};
use memory_core::{MemoryId, ProjectId, Result};
use serde::Serialize;

/// Candidates taken from each path before fusion.
const TOP_N: usize = 50;
/// English function words. FTS ORs the query tokens, so in a natural-language question they
/// match half the memories and bury the real hits; the vector path covers meaning anyway.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "at", "for", "with", "by", "from", "as",
    "about", "into", "is", "are", "was", "were", "be", "been", "do", "does", "did", "can", "could",
    "should", "would", "how", "what", "why", "when", "where", "which", "who", "we", "you", "i",
    "it", "this", "that", "my", "our", "your", "there", "if", "so", "not", "no",
];

/// The query without stop words; unchanged when nothing else would be left.
pub fn fts_text(query: &str) -> String {
    let kept: Vec<&str> = query
        .split_whitespace()
        .filter(|t| {
            let t = t
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            !STOP_WORDS.contains(&t.as_str())
        })
        .collect();
    if kept.is_empty() {
        query.to_string()
    } else {
        kept.join(" ")
    }
}

/// Standard RRF damping constant.
const RRF_K: f64 = 60.0;
/// Keyword matches are noisy on natural-language questions (and absent for unsegmented CJK),
/// so a vector rank counts for more than the same FTS rank.
const VECTOR_WEIGHT: f64 = 2.0;

#[derive(Debug, Clone, Serialize)]
pub struct HybridHit {
    /// `score` is the fused RRF score.
    #[serde(flatten)]
    pub hit: Hit,
    /// The paths that returned it: `fts`, `vector` or both.
    pub matched_by: Vec<&'static str>,
    /// 1-based rank in each path, `None` when that path did not return the memory.
    pub fts_rank: Option<usize>,
    pub vector_rank: Option<usize>,
}

/// Which retrieval paths a search uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Fts,
    /// Vector only. Falls back to full-text, reporting why, when the vector path fails.
    Vector,
    Hybrid,
}

impl SearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fts => "fts",
            Self::Vector => "vector",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HybridResult {
    /// The paths that produced `hits`: what was asked for, or `fts` after a fallback.
    pub mode: SearchMode,
    pub hits: Vec<HybridHit>,
    /// How many candidates each path returned before fusion (0 when skipped).
    pub fts_returned: usize,
    pub vector_returned: usize,
    /// Why the vector path was skipped; the hits are then FTS only.
    pub degraded: Option<String>,
}

/// Runs a search in the requested mode. Full-text is also the fallback when the vector path is
/// unavailable (`degraded` says why).
pub fn search<S: MemoryService + EmbeddingStore>(
    store: &S,
    mode: SearchMode,
    embedder: Option<&dyn Embedder>,
    project: Option<ProjectId>,
    query: SearchQuery,
) -> Result<HybridResult> {
    match mode {
        SearchMode::Fts => Ok(HybridResult {
            degraded: None,
            ..hybrid_search(store, None, project, query)?
        }),
        SearchMode::Hybrid => hybrid_search(store, embedder, project, query),
        SearchMode::Vector => {
            let Some(e) = embedder else {
                return hybrid_search(store, None, project, query);
            };
            match vector_hits(store, e, project, &query) {
                Ok(v) => Ok(HybridResult {
                    mode,
                    fts_returned: 0,
                    vector_returned: v.len(),
                    degraded: None,
                    hits: v
                        .into_iter()
                        .enumerate()
                        .map(|(i, hit)| HybridHit {
                            hit,
                            matched_by: vec!["vector"],
                            fts_rank: None,
                            vector_rank: Some(i + 1),
                        })
                        .collect(),
                }),
                Err(err) => {
                    let mut r = hybrid_search(store, None, project, query)?;
                    r.degraded = Some(err.to_string());
                    Ok(r)
                }
            }
        }
    }
}

pub fn hybrid_search<S: MemoryService + EmbeddingStore>(
    store: &S,
    embedder: Option<&dyn Embedder>,
    project: Option<ProjectId>,
    query: SearchQuery,
) -> Result<HybridResult> {
    let limit = query.limit;
    let wide = SearchQuery {
        limit: TOP_N.max(limit),
        ..query.clone()
    };
    let fts = store.search(
        project,
        SearchQuery {
            query: fts_text(&query.query),
            ..wide.clone()
        },
    )?;
    let (vector, degraded) = match embedder {
        None => (vec![], Some("no embedding provider".to_string())),
        Some(e) => match vector_hits(store, e, project, &wide) {
            Ok(v) => (v, None),
            Err(err) => (vec![], Some(err.to_string())),
        },
    };

    let (fts_returned, vector_returned) = (fts.len(), vector.len());
    let mut fused: HashMap<MemoryId, HybridHit> = HashMap::new();
    for (rank, h) in fts.into_iter().enumerate() {
        let rank = rank + 1;
        fused.insert(
            h.memory.id,
            HybridHit {
                hit: Hit {
                    score: 1.0 / (RRF_K + rank as f64),
                    ..h
                },
                matched_by: vec![],
                fts_rank: Some(rank),
                vector_rank: None,
            },
        );
    }
    for (rank, h) in vector.into_iter().enumerate() {
        let rank = rank + 1;
        let add = VECTOR_WEIGHT / (RRF_K + rank as f64);
        fused
            .entry(h.memory.id)
            .and_modify(|f| {
                f.hit.score += add;
                f.vector_rank = Some(rank);
            })
            .or_insert(HybridHit {
                hit: Hit { score: add, ..h },
                matched_by: vec![],
                fts_rank: None,
                vector_rank: Some(rank),
            });
    }
    let mut hits: Vec<_> = fused.into_values().collect();
    for h in &mut hits {
        h.matched_by = [("fts", h.fts_rank), ("vector", h.vector_rank)]
            .into_iter()
            .filter_map(|(n, r)| r.map(|_| n))
            .collect();
    }
    hits.sort_by(|a, b| {
        b.hit
            .score
            .total_cmp(&a.hit.score)
            .then(b.hit.memory.updated_at.cmp(&a.hit.memory.updated_at))
            .then(a.hit.memory.id.0.cmp(&b.hit.memory.id.0))
    });
    hits.truncate(limit);
    Ok(HybridResult {
        mode: if degraded.is_some() {
            SearchMode::Fts
        } else {
            SearchMode::Hybrid
        },
        hits,
        fts_returned,
        vector_returned,
        degraded,
    })
}

fn vector_hits<S: EmbeddingStore>(
    store: &S,
    embedder: &dyn Embedder,
    project: Option<ProjectId>,
    q: &SearchQuery,
) -> Result<Vec<Hit>> {
    let mut v = embedder.embed(std::slice::from_ref(&q.query))?;
    store.vector_search(project, &embedder.model_id(), &v.remove(0), q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_words_are_dropped_unless_nothing_else_is_left() {
        assert_eq!(
            fts_text("How do we release a new version?"),
            "release new version?"
        );
        assert_eq!(fts_text("What is THE rule"), "rule");
        assert_eq!(fts_text("how do we"), "how do we");
        assert_eq!(fts_text("busy_timeout 怎么设置"), "busy_timeout 怎么设置");
    }
}

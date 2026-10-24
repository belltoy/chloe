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
/// Standard RRF damping constant.
const RRF_K: f64 = 60.0;

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

#[derive(Debug, Clone)]
pub struct HybridResult {
    pub hits: Vec<HybridHit>,
    /// How many candidates each path returned before fusion (0 when skipped).
    pub fts_returned: usize,
    pub vector_returned: usize,
    /// Why the vector path was skipped; the hits are then FTS only.
    pub degraded: Option<String>,
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
    let fts = store.search(project, wide.clone())?;
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
        let add = 1.0 / (RRF_K + rank as f64);
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

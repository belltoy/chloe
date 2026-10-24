//! Retrieval evaluation (B3-04): load a fixed, hand-labelled dataset into a store, embed it,
//! then score FTS, Vector and Hybrid on the same queries. Nothing here reads a user database;
//! the caller passes a store it created for the run.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::job::JobQueue;
use memory_core::service::{MemoryService, NewMemory, SearchQuery};
use memory_core::{Error, Kind, MemoryId, ProjectId, Result, Source};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{hybrid_search, plan_embeddings, run_next_embedding};

/// Hits scored per query; Recall is taken at `RECALL_AT`.
const DEPTH: usize = 10;
const RECALL_AT: usize = 5;
pub const MODES: [&str; 3] = ["fts", "vector", "hybrid"];

#[derive(Debug, Clone, Deserialize)]
pub struct EvalMemory {
    pub key: String,
    pub kind: Kind,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalQuery {
    pub id: String,
    /// Optional kind of query (`ident`, `para`, `hardneg`, ...), shown in diagnostics.
    #[serde(default)]
    pub tag: String,
    /// `en`, `zh`, `mixed` or `cross` (query and memory in different languages).
    pub lang: String,
    pub text: String,
    /// Keys of the memories a person judged relevant.
    pub relevant: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dataset {
    pub memories: Vec<EvalMemory>,
    pub queries: Vec<EvalQuery>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    /// A language, or `all`.
    pub lang: String,
    pub mode: &'static str,
    pub queries: usize,
    pub recall_at_5: f64,
    pub mrr: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub model_id: String,
    pub dimension: Option<usize>,
    pub memories: usize,
    pub queries: usize,
    /// Dataset memory key to the id it was saved under (for diagnostics).
    #[serde(skip)]
    pub ids: HashMap<String, MemoryId>,
    /// Timed runs per query and mode behind the latency columns.
    pub runs: usize,
    pub rows: Vec<Row>,
}

/// (lang, mode) -> per query (recall, reciprocal rank), and every timed run in ms.
type Samples = BTreeMap<(String, &'static str), (Vec<(f64, f64)>, Vec<f64>)>;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    // Nearest rank.
    let i = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i]
}

/// Scores one ranked list: (recall@5, reciprocal rank within the depth).
fn score(ranked: &[MemoryId], relevant: &[MemoryId]) -> (f64, f64) {
    let found = ranked
        .iter()
        .take(RECALL_AT)
        .filter(|id| relevant.contains(id))
        .count();
    let rr = ranked
        .iter()
        .position(|id| relevant.contains(id))
        .map_or(0.0, |i| 1.0 / (i + 1) as f64);
    (found as f64 / relevant.len() as f64, rr)
}

pub fn run_eval<S: MemoryService + EmbeddingStore + JobQueue>(
    store: &S,
    embedder: &dyn Embedder,
    project: ProjectId,
    ds: &Dataset,
    runs: usize,
) -> Result<Report> {
    let mut ids: HashMap<&str, MemoryId> = HashMap::new();
    for m in &ds.memories {
        let saved = store.save(NewMemory {
            project_id: Some(project),
            kind: m.kind,
            title: m.title.clone(),
            content: m.content.clone(),
            source: Source::Manual,
        })?;
        if ids.insert(&m.key, saved.id).is_some() {
            return Err(Error::InvalidArgument(format!("duplicate key {}", m.key)));
        }
    }
    let now = OffsetDateTime::now_utc();
    plan_embeddings(store, embedder, now)?;
    while let Some(d) = run_next_embedding(store, embedder, "eval", now)? {
        if let Err(e) = d.result {
            return Err(Error::Unavailable(format!("embedding failed: {e}")));
        }
    }
    let model_id = embedder.model_id();
    // The first call can load the model; keep that out of the latency samples.
    embedder.embed(&["warm-up".to_string()])?;

    let mut samples: Samples = BTreeMap::new();
    for q in &ds.queries {
        let relevant = q
            .relevant
            .iter()
            .map(|k| {
                ids.get(k.as_str())
                    .copied()
                    .ok_or_else(|| Error::InvalidArgument(format!("{}: unknown key {k}", q.id)))
            })
            .collect::<Result<Vec<_>>>()?;
        if relevant.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "{}: nothing relevant",
                q.id
            )));
        }
        let sq = SearchQuery {
            query: q.text.clone(),
            limit: DEPTH,
            include_global: false,
            include_inactive: false,
        };
        for mode in MODES {
            let mut times = vec![];
            let mut ranked: Vec<MemoryId> = vec![];
            for _ in 0..runs.max(1) {
                let start = Instant::now();
                ranked = match mode {
                    "fts" => store
                        .search(Some(project), sq.clone())?
                        .into_iter()
                        .map(|h| h.memory.id)
                        .collect(),
                    "vector" => {
                        let v = embedder.embed(std::slice::from_ref(&q.text))?.remove(0);
                        store
                            .vector_search(Some(project), &model_id, &v, &sq)?
                            .into_iter()
                            .map(|h| h.memory.id)
                            .collect()
                    }
                    _ => {
                        let r = hybrid_search(store, Some(embedder), Some(project), sq.clone())?;
                        if let Some(why) = r.degraded {
                            return Err(Error::Unavailable(format!("hybrid degraded: {why}")));
                        }
                        r.hits.into_iter().map(|h| h.hit.memory.id).collect()
                    }
                };
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            let (recall, rr) = score(&ranked, &relevant);
            for lang in [q.lang.clone(), "all".into()] {
                let s = samples.entry((lang, mode)).or_default();
                s.0.push((recall, rr));
                s.1.extend(&times);
            }
        }
    }

    let langs = ["en", "zh", "mixed", "cross", "all"];
    let rows = langs
        .iter()
        .flat_map(|l| MODES.iter().map(move |m| (l.to_string(), *m)))
        .filter_map(|key| {
            let (quality, times) = samples.get(&key)?;
            let n = quality.len() as f64;
            let mut ms = times.clone();
            ms.sort_by(f64::total_cmp);
            Some(Row {
                lang: key.0,
                mode: key.1,
                queries: quality.len(),
                recall_at_5: quality.iter().map(|x| x.0).sum::<f64>() / n,
                mrr: quality.iter().map(|x| x.1).sum::<f64>() / n,
                p50_ms: percentile(&ms, 0.50),
                p95_ms: percentile(&ms, 0.95),
            })
        })
        .collect();
    Ok(Report {
        model_id,
        dimension: store.embedding_index()?.and_then(|i| i.dimension),
        memories: ds.memories.len(),
        queries: ds.queries.len(),
        runs: runs.max(1),
        ids: ids.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics() {
        let (a, b, c) = (MemoryId::new(), MemoryId::new(), MemoryId::new());
        assert_eq!(score(&[a, b, c], &[b, c]), (1.0, 0.5));
        assert_eq!(score(&[a], &[b]), (0.0, 0.0));
        // Relevant beyond rank 5 still counts toward MRR but not Recall@5.
        let far = vec![a, a, a, a, a, b];
        assert_eq!(score(&far, &[b]), (0.0, 1.0 / 6.0));
        let v = [1.0, 2.0, 3.0, 4.0];
        assert_eq!((percentile(&v, 0.5), percentile(&v, 0.95)), (2.0, 4.0));
    }
}

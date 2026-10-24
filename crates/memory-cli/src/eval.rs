//! `chloe eval`: score FTS, Vector and Hybrid retrieval on a labelled dataset. The run uses its
//! own temporary database, never the configured one.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::service::{MemoryService, SearchQuery};
use memory_core::{MemoryId, ProjectId};
use memory_engine::{Dataset, Report, fts_text, hybrid_search, run_eval};
use memory_llm::Ollama;
use memory_store::SqliteStore;

use crate::{llm::Provider, print_json};

#[derive(Args)]
pub struct Eval {
    /// Dataset JSON: `memories` and `queries` with human-labelled `relevant` keys
    #[arg(long, default_value = "docs/evidence/milestone-b/eval/dataset.json")]
    dataset: PathBuf,
    /// Timed runs per query and mode (latency percentiles are taken over all of them)
    #[arg(long, default_value_t = 1)]
    runs: usize,
    /// After the table, explain every query whose first hybrid hit is not relevant: the FTS and
    /// vector ranks and scores, which query terms each candidate matched, and what won
    #[arg(long)]
    diagnose: bool,
    /// Free text recorded in the report, e.g. the machine
    #[arg(long, default_value = "")]
    note: String,
    #[command(flatten)]
    provider: Provider,
}

pub fn run(args: Eval, json: bool) -> anyhow::Result<()> {
    let ds: Dataset = serde_json::from_slice(
        &std::fs::read(&args.dataset).with_context(|| args.dataset.display().to_string())?,
    )
    .context("invalid dataset")?;
    let embedder = args.provider.open()?;
    let dir = std::env::temp_dir().join(format!("chloe-eval-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let result = (|| -> anyhow::Result<(Report, Option<String>)> {
        let store = SqliteStore::open(&dir.join("eval.db"))?;
        let project = store.resolve_project(&dir)?;
        let report = run_eval(&store, &embedder, project, &ds, args.runs)?;
        let diagnosis = args
            .diagnose
            .then(|| diagnose(&store, &embedder, project, &ds, &report.ids))
            .transpose()?;
        Ok((report, diagnosis))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let (report, diagnosis) = result?;
    if json {
        return print_json(&serde_json::json!({
            "note": args.note,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map_or(0, |n| n.get()),
            "report": report,
        }));
    }
    println!("# Retrieval evaluation\n");
    println!(
        "- model: {} ({} dimensions)\n- data: {} memories, {} queries, {} timed run(s) each\n- host: {}/{}, {} cpus{}\n",
        report.model_id,
        report.dimension.map_or("?".into(), |d| d.to_string()),
        report.memories,
        report.queries,
        report.runs,
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        if args.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", args.note)
        },
    );
    println!(
        "| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |\n|---|---|---|---|---|---|---|"
    );
    for r in &report.rows {
        println!(
            "| {} | {} | {} | {:.2} | {:.2} | {:.1} | {:.1} |",
            r.lang, r.mode, r.queries, r.recall_at_5, r.mrr, r.p50_ms, r.p95_ms
        );
    }
    if let Some(d) = diagnosis {
        println!("\n{d}");
    }
    Ok(())
}

const DEPTH: usize = 50;

fn rank_of(ids: &[MemoryId], id: MemoryId) -> Option<usize> {
    ids.iter().position(|x| *x == id).map(|i| i + 1)
}

fn show(r: Option<usize>) -> String {
    r.map_or("-".into(), |r| r.to_string())
}

/// One block per query whose first hybrid hit is not relevant, then counts of how the winners
/// got there. Uses the same candidate generation as `hybrid_search` (top 50 per path, same
/// filters and FTS text), so the ranks shown are the ones that were fused.
fn diagnose(
    store: &SqliteStore,
    embedder: &Ollama,
    project: ProjectId,
    ds: &Dataset,
    ids: &HashMap<String, MemoryId>,
) -> anyhow::Result<String> {
    let key_of: HashMap<MemoryId, &str> = ids.iter().map(|(k, v)| (*v, k.as_str())).collect();
    let mut out = String::from("# Diagnosis: queries where the first hybrid hit is not relevant\n");
    let (mut failing, mut in_both, mut fts_only, mut vec_only) = (0, 0, 0, 0);
    let (mut expected_not_in_fts, mut expected_vec_first) = (0, 0);
    for q in &ds.queries {
        let relevant: Vec<MemoryId> = q.relevant.iter().map(|k| ids[k]).collect();
        let sq = SearchQuery {
            query: q.text.clone(),
            limit: DEPTH,
            include_global: false,
            include_inactive: false,
        };
        let fused = hybrid_search(store, Some(embedder), Some(project), sq.clone())?;
        if fused
            .hits
            .first()
            .is_some_and(|h| relevant.contains(&h.hit.memory.id))
        {
            continue;
        }
        failing += 1;
        let fts = store.search(
            Some(project),
            SearchQuery {
                query: fts_text(&q.text),
                ..sq.clone()
            },
        )?;
        let fts_ids: Vec<MemoryId> = fts.iter().map(|h| h.memory.id).collect();
        let qv = embedder.embed(std::slice::from_ref(&q.text))?.remove(0);
        let vec_hits = store.vector_search(Some(project), &embedder.model_id(), &qv, &sq)?;
        let vec_ids: Vec<MemoryId> = vec_hits.iter().map(|h| h.memory.id).collect();
        let cosine = |id: MemoryId| vec_hits.iter().find(|h| h.memory.id == id).map(|h| h.score);
        let fused_ids: Vec<MemoryId> = fused.hits.iter().map(|h| h.hit.memory.id).collect();

        out += &format!(
            "\n## {} [{}{}] {}\n\n",
            q.id,
            q.lang,
            if q.tag.is_empty() {
                String::new()
            } else {
                format!("/{}", q.tag)
            },
            q.text
        );
        let cov0 = store.fts_coverage(&fts_text(&q.text), relevant[0])?;
        out += &format!(
            "FTS terms ({}): {}\n\n",
            cov0.terms.len(),
            cov0.terms.join(" ")
        );
        out += "| expected | title | fts# | vec# | cos | hybrid# | matched terms |\n|---|---|---|---|---|---|---|\n";
        for (k, id) in q.relevant.iter().zip(&relevant) {
            let m = store.get(Some(project), *id)?;
            let cov = store.fts_coverage(&fts_text(&q.text), *id)?;
            out += &format!(
                "| {k} | {} | {} | {} | {} | {} | {}/{} {} |\n",
                m.title,
                show(rank_of(&fts_ids, *id)),
                show(rank_of(&vec_ids, *id)),
                cosine(*id).map_or("-".into(), |c| format!("{c:.2}")),
                show(rank_of(&fused_ids, *id)),
                cov.matched.len(),
                cov.terms.len(),
                cov.matched.join(" "),
            );
        }
        out += "\n| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |\n|---|---|---|---|---|---|---|---|---|\n";
        for (i, h) in fused.hits.iter().take(5).enumerate() {
            let id = h.hit.memory.id;
            let cov = store.fts_coverage(&fts_text(&q.text), id)?;
            out += &format!(
                "| {} | {} | {} | {} | {} | {} | {:.4} | {}/{} {} | {} |\n",
                i + 1,
                key_of.get(&id).copied().unwrap_or("?"),
                h.hit.memory.title,
                show(h.fts_rank),
                show(h.vector_rank),
                cosine(id).map_or("-".into(), |c| format!("{c:.2}")),
                h.hit.score,
                cov.matched.len(),
                cov.terms.len(),
                cov.matched.join(" "),
                if relevant.contains(&id) { "yes" } else { "no" },
            );
        }
        if let Some(w) = fused.hits.first() {
            match (w.fts_rank, w.vector_rank) {
                (Some(_), Some(_)) => in_both += 1,
                (Some(_), None) => fts_only += 1,
                _ => vec_only += 1,
            }
        }
        if relevant.iter().all(|id| rank_of(&fts_ids, *id).is_none()) {
            expected_not_in_fts += 1;
        }
        if vec_ids.first().is_some_and(|id| relevant.contains(id)) {
            expected_vec_first += 1;
        }
    }
    out += &format!(
        "\n## Summary\n\n- failing queries: {failing} of {}\n- winner found by both paths: {in_both}; by FTS only: {fts_only}; by vector only: {vec_only}\n- no relevant memory in the FTS candidates: {expected_not_in_fts}\n- a relevant memory is the vector's first hit: {expected_vec_first}\n",
        ds.queries.len()
    );
    Ok(out)
}

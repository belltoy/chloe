//! `chloe eval`: score FTS, Vector and Hybrid retrieval on a labelled dataset. The run uses its
//! own temporary database, never the configured one.

use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use memory_engine::{Dataset, Report, run_eval};
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
    let result = (|| -> anyhow::Result<Report> {
        let store = SqliteStore::open(&dir.join("eval.db"))?;
        let project = store.resolve_project(&dir)?;
        Ok(run_eval(&store, &embedder, project, &ds, args.runs)?)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let report = result?;
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
    Ok(())
}

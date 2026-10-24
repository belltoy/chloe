//! `chloe embeddings`: the vector index's model and the backfill of missing vectors. Switching
//! `--embed-model` drops the old vectors and rebuilds them with the new model.

use clap::Subcommand;
use memory_core::embed::EmbeddingStore;
use memory_core::job::JobQueue;
use memory_engine::{plan_embeddings, run_next_embedding};
use memory_store::SqliteStore;
use serde_json::json;
use time::OffsetDateTime;

use crate::{llm::Provider, print_json};

#[derive(Subcommand)]
pub enum EmbeddingsCommand {
    /// The pinned model and dimension, and how many memories still lack a vector
    Status,
    /// Embed every memory without a vector (rebuilds everything after a model switch)
    Backfill {
        /// Embed at most this many memories in one run; run again for the rest
        #[arg(long, default_value_t = 500)]
        limit: usize,
        #[command(flatten)]
        provider: Provider,
    },
}

pub fn run(store: &SqliteStore, command: EmbeddingsCommand, json: bool) -> anyhow::Result<()> {
    match command {
        EmbeddingsCommand::Status => {
            let index = store.embedding_index()?;
            let missing = store.missing_embeddings(usize::MAX >> 1)?.len();
            if json {
                return print_json(&json!({"index": index, "missing": missing}));
            }
            match index {
                Some(i) => println!(
                    "model = {}\ndimension = {}\nmissing = {missing}",
                    i.model_id,
                    i.dimension.map_or("unknown".into(), |d| d.to_string())
                ),
                None => println!("no embedding model chosen yet\nmissing = {missing}"),
            }
        }
        EmbeddingsCommand::Backfill { limit, provider } => {
            let embedder = provider.open()?;
            let plan = plan_embeddings(store, &embedder, OffsetDateTime::now_utc())?;
            let worker = format!("cli-{}", std::process::id());
            let (mut done, mut failed) = (0, vec![]);
            while done + failed.len() < limit
                && let Some(d) =
                    run_next_embedding(store, &embedder, &worker, OffsetDateTime::now_utc())?
            {
                match d.result {
                    Ok(()) => done += 1,
                    Err(e) => failed.push((d.memory, e)),
                }
            }
            let remaining = store.missing_embeddings(usize::MAX >> 1)?.len();
            if json {
                return print_json(&json!({
                    "remaining": remaining,
                    "queued": plan.enqueued,
                    "already_queued": plan.already_queued,
                    "rebuilt": plan.rebuilt,
                    "embedded": done,
                    "failed": failed,
                    "jobs": store.counts()?,
                }));
            }
            if plan.rebuilt {
                println!("model changed: old vectors dropped, rebuilding");
            }
            println!(
                "queued {}, embedded {done}, {remaining} still without a vector",
                plan.enqueued
            );
            if remaining > 0 && done + failed.len() >= limit {
                println!("limit {limit} reached: run `chloe embeddings backfill` again");
            }
            for (m, e) in &failed {
                println!("  {m}: {e} (will retry, or `chloe jobs list`)");
            }
        }
    }
    Ok(())
}

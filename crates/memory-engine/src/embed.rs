//! Embedding backfill (B3-01): `plan_embeddings` queues one job per memory revision that has no
//! vector under the current model, `run_next_embedding` runs one. Switching the model clears
//! the vectors (see `EmbeddingStore::use_embedding_model`) and the new generation makes the
//! next plan queue everything again.

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::job::JobQueue;
use memory_core::{Error, MemoryId, Result};
use serde::Deserialize;
use serde_json::json;
use time::{Duration, OffsetDateTime};

pub const EMBED_JOB_KIND: &str = "embed_memory";
const LEASE: Duration = Duration::minutes(5);
/// Memories queued per `plan_embeddings` call.
const BATCH: usize = 500;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EmbedPlan {
    pub enqueued: usize,
    /// Missing vectors whose job already exists (waiting, failed or running).
    pub already_queued: usize,
    /// The model changed and the old vectors were dropped.
    pub rebuilt: bool,
}

pub fn plan_embeddings<S: EmbeddingStore + JobQueue>(
    store: &S,
    embedder: &dyn Embedder,
    now: OffsetDateTime,
) -> Result<EmbedPlan> {
    let model = embedder.model_id();
    let before = store.embedding_index()?;
    let index = store.use_embedding_model(&model)?;
    let mut plan = EmbedPlan {
        rebuilt: before.is_some_and(|b| b.model_id != model),
        ..Default::default()
    };
    for p in store.missing_embeddings(BATCH)? {
        let key = format!("{}|{}|{}|{}", index.generation, model, p.id, p.revision);
        let payload = json!({"memory": p.id, "revision": p.revision, "model": model});
        if store.enqueue(EMBED_JOB_KIND, &key, payload, now)?.created {
            plan.enqueued += 1;
        } else {
            plan.already_queued += 1;
        }
    }
    Ok(plan)
}

#[derive(Debug, PartialEq, Eq)]
pub struct EmbedDone {
    pub memory: String,
    /// `Err` is the failed attempt's message; the job retries on its backoff.
    pub result: std::result::Result<(), String>,
}

#[derive(Deserialize)]
struct Payload {
    memory: MemoryId,
    model: String,
}

/// Leases and runs one embedding job; `Ok(None)` when nothing is due. A provider that is down
/// is a failed attempt, not an error of this call.
pub fn run_next_embedding<S: EmbeddingStore + JobQueue>(
    store: &S,
    embedder: &dyn Embedder,
    worker: &str,
    now: OffsetDateTime,
) -> Result<Option<EmbedDone>> {
    let Some(job) = store.lease_kind(EMBED_JOB_KIND, worker, LEASE, now)? else {
        return Ok(None);
    };
    let lease = job.lease_id.clone().unwrap_or_default();
    let name = job.dedup_key.clone();
    let attempt = |p: &Payload| -> Result<()> {
        if p.model != embedder.model_id() {
            return Err(Error::InvalidArgument(
                "job was queued for another model".into(),
            ));
        }
        let (revision, text) = store.embedding_text(p.memory)?;
        let mut v = embedder.embed(&[text])?;
        // A revision moved on since queueing is skipped; the next plan queues the new one.
        store.put_embedding(p.memory, revision, &p.model, &v.remove(0))?;
        Ok(())
    };
    let result = serde_json::from_value::<Payload>(job.payload.clone())
        .map_err(|_| Error::InvalidArgument("unreadable job payload".into()))
        .and_then(|p| attempt(&p).map(|()| p.memory.to_string()));
    Ok(Some(match result {
        Ok(memory) => {
            store.complete(&job.id, &lease, now)?;
            EmbedDone {
                memory,
                result: Ok(()),
            }
        }
        Err(e) => {
            store.fail(&job.id, &lease, &e.to_string(), now)?;
            EmbedDone {
                memory: name,
                result: Err(e.to_string()),
            }
        }
    }))
}

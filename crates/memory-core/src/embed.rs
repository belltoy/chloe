//! Embedding index types (B3-01). The index is pinned to one `(model_id, dimension)`; vectors
//! from different models or sizes are never mixed. Switching models clears the vectors and
//! starts a new `generation`, so backfill jobs of the old model are not mistaken for done.

use serde::{Deserialize, Serialize};

use crate::{MemoryId, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingIndex {
    pub model_id: String,
    /// `None` until the first vector fixes it.
    pub dimension: Option<usize>,
    pub generation: u32,
}

/// A memory whose current revision has no vector yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEmbedding {
    pub id: MemoryId,
    pub revision: u32,
}

/// Turns text into vectors (implemented in `memory-llm`).
pub trait Embedder {
    /// Stable identity of the model, recorded with the index.
    fn model_id(&self) -> String;
    /// One vector per input, in order; all of the same length.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Vector storage (implemented in `memory-store`).
pub trait EmbeddingStore {
    fn embedding_index(&self) -> Result<Option<EmbeddingIndex>>;
    /// Pins the index to `model_id`. A different pinned model is replaced: every vector is
    /// dropped and the generation is bumped. Returns the index now in force.
    fn use_embedding_model(&self, model_id: &str) -> Result<EmbeddingIndex>;
    /// Memories without a vector for their current revision, oldest first.
    fn missing_embeddings(&self, limit: usize) -> Result<Vec<PendingEmbedding>>;
    /// `"{title}\n\n{content}"` of the memory as it is now, with its revision.
    fn embedding_text(&self, id: MemoryId) -> Result<(u32, String)>;
    /// Stores the vector of `revision`. Refused when `model_id` is not the pinned model or the
    /// length differs from the pinned dimension; a revision that is no longer current is
    /// ignored (`Ok(false)`).
    fn put_embedding(
        &self,
        id: MemoryId,
        revision: u32,
        model_id: &str,
        vector: &[f32],
    ) -> Result<bool>;
    fn get_embedding(&self, id: MemoryId) -> Result<Option<Vec<f32>>>;
}

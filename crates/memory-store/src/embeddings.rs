//! `EmbeddingStore` for SQLite.

use memory_core::embed::{EmbeddingIndex, EmbeddingStore, PendingEmbedding};
use memory_core::{Error, MemoryId, Result};
use rusqlite::{Connection, OptionalExtension, params};

use memory_core::ProjectId;
use memory_core::service::{Hit, SearchQuery};

use crate::{COLS, SqliteStore, begin, clamp, micros, now, storage, to_memory};

fn index(conn: &Connection) -> Result<Option<EmbeddingIndex>> {
    conn.query_row(
        "SELECT model_id, dimension, generation FROM embedding_index",
        [],
        |r| {
            Ok(EmbeddingIndex {
                model_id: r.get(0)?,
                dimension: r.get::<_, Option<i64>>(1)?.map(|d| d as usize),
                generation: r.get(2)?,
            })
        },
    )
    .optional()
    .map_err(storage)
}

fn encode(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn decode(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl EmbeddingStore for SqliteStore {
    fn embedding_index(&self) -> Result<Option<EmbeddingIndex>> {
        index(&self.conn())
    }

    fn use_embedding_model(&self, model_id: &str) -> Result<EmbeddingIndex> {
        if model_id.trim().is_empty() {
            return Err(Error::InvalidArgument("model id must not be empty".into()));
        }
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let current = index(&tx)?;
        if let Some(i) = current.as_ref().filter(|i| i.model_id == model_id) {
            return Ok(i.clone());
        }
        let generation = current.map_or(1, |i| i.generation + 1);
        tx.execute("DELETE FROM embeddings", []).map_err(storage)?;
        tx.execute(
            "INSERT INTO embedding_index (id, model_id, dimension, generation)
             VALUES (1, ?1, NULL, ?2)
             ON CONFLICT(id) DO UPDATE SET
                 model_id=excluded.model_id,
                 dimension=NULL,
                 generation=excluded.generation",
            params![model_id, generation],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(EmbeddingIndex {
            model_id: model_id.into(),
            dimension: None,
            generation,
        })
    }

    fn missing_embeddings(&self, limit: usize) -> Result<Vec<PendingEmbedding>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.revision
                 FROM memories m LEFT JOIN embeddings e ON e.memory_id = m.id
                 WHERE e.revision IS NULL OR e.revision < m.revision
                 ORDER BY m.created_at, m.rowid
                 LIMIT ?1",
            )
            .map_err(storage)?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                let id: String = r.get(0)?;
                Ok((id, r.get::<_, u32>(1)?))
            })
            .map_err(storage)?;
        rows.map(|r| {
            let (id, revision) = r.map_err(storage)?;
            Ok(PendingEmbedding {
                id: id.parse().map_err(storage)?,
                revision,
            })
        })
        .collect()
    }

    fn embedding_text(&self, id: MemoryId) -> Result<(u32, String)> {
        self.conn()
            .query_row(
                "SELECT revision, title || char(10) || char(10) || content FROM memories WHERE id=?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| Error::NotFound(format!("memory {id}")))
    }

    fn put_embedding(
        &self,
        id: MemoryId,
        revision: u32,
        model_id: &str,
        vector: &[f32],
    ) -> Result<bool> {
        if vector.is_empty() || vector.iter().any(|f| !f.is_finite()) {
            return Err(Error::InvalidArgument(
                "embedding must be non-empty and finite".into(),
            ));
        }
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let Some(idx) = index(&tx)?.filter(|i| i.model_id == model_id) else {
            return Err(Error::InvalidArgument(format!(
                "the index is not pinned to model {model_id}"
            )));
        };
        match idx.dimension {
            Some(d) if d != vector.len() => {
                return Err(Error::InvalidArgument(format!(
                    "model {model_id} returned {} dimensions, the index has {d}",
                    vector.len()
                )));
            }
            Some(_) => {}
            None => {
                tx.execute(
                    "UPDATE embedding_index SET dimension=?1",
                    [vector.len() as i64],
                )
                .map_err(storage)?;
            }
        }
        let current: Option<u32> = tx
            .query_row(
                "SELECT revision FROM memories WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if current != Some(revision) {
            tx.commit().map_err(storage)?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO embeddings (memory_id, revision, vector, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(memory_id) DO UPDATE SET
                 revision=excluded.revision,
                 vector=excluded.vector,
                 updated_at=excluded.updated_at",
            params![id.to_string(), revision, encode(vector), micros(now())],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(true)
    }

    fn vector_search(
        &self,
        project: Option<ProjectId>,
        model_id: &str,
        query: &[f32],
        q: &SearchQuery,
    ) -> Result<Vec<Hit>> {
        let conn = self.conn();
        let Some(idx) = index(&conn)?.filter(|i| i.model_id == model_id) else {
            return Err(Error::InvalidArgument(format!(
                "the index is not pinned to model {model_id}"
            )));
        };
        if idx.dimension != Some(query.len()) {
            return Err(Error::InvalidArgument(
                "query vector does not match the index dimension".into(),
            ));
        }
        let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let qn = norm(query);
        if qn == 0.0 {
            return Err(Error::InvalidArgument("query vector is all zeros".into()));
        }
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {}, e.vector
                 FROM embeddings e JOIN memories m ON m.id = e.memory_id
                 WHERE e.revision = m.revision
                   AND (m.project_id IS ?1 OR (?2 AND m.project_id IS NULL))
                   AND (?3 OR m.status = 'active')",
                COLS.split(", ")
                    .map(|c| format!("m.{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .map_err(storage)?;
        // shortcut: scans every vector in scope in memory, upgrade to an ANN index past ~100k memories.
        let mut hits = stmt
            .query_map(
                params![
                    project.map(|p| p.to_string()),
                    q.include_global,
                    q.include_inactive
                ],
                |r| Ok((to_memory(r)?, r.get::<_, Vec<u8>>(10)?)),
            )
            .map_err(storage)?
            .map(|r| {
                let (memory, blob) = r.map_err(storage)?;
                let v = decode(&blob);
                let n = norm(&v);
                let dot: f32 = v.iter().zip(query).map(|(a, b)| a * b).sum();
                let score = if n == 0.0 {
                    0.0
                } else {
                    f64::from(dot / (n * qn))
                };
                Ok(Hit { memory, score })
            })
            .collect::<Result<Vec<_>>>()?;
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(b.memory.updated_at.cmp(&a.memory.updated_at))
        });
        hits.truncate(clamp(q.limit) as usize);
        Ok(hits)
    }

    fn get_embedding(&self, id: MemoryId) -> Result<Option<Vec<f32>>> {
        self.conn()
            .query_row(
                "SELECT vector FROM embeddings WHERE memory_id=?1",
                [id.to_string()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map(|b| b.map(|b| decode(&b)))
            .map_err(storage)
    }
}

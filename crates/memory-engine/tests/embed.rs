use std::cell::Cell;

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::job::{JobQueue, JobState};
use memory_core::service::{MemoryChanges, MemoryService, NewMemory};
use memory_core::{Error, Kind, Memory, Result, Source};
use memory_engine::{EMBED_JOB_KIND, plan_embeddings, run_next, run_next_embedding};
use memory_store::SqliteStore;
use time::OffsetDateTime;

struct Fake {
    model: &'static str,
    dim: usize,
    down: Cell<bool>,
}

fn fake(model: &'static str, dim: usize) -> Fake {
    Fake {
        model,
        dim,
        down: Cell::new(false),
    }
}

impl Embedder for Fake {
    fn model_id(&self) -> String {
        self.model.into()
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if self.down.get() {
            return Err(Error::Unavailable("down".into()));
        }
        Ok(texts
            .iter()
            .map(|t| vec![t.len() as f32; self.dim])
            .collect())
    }
}

fn store() -> (tempfile::TempDir, SqliteStore) {
    let dir = tempfile::tempdir().unwrap();
    let s = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    (dir, s)
}

fn save(s: &SqliteStore, title: &str) -> Memory {
    s.save(NewMemory {
        project_id: None,
        kind: Kind::Fact,
        title: title.into(),
        content: "内容 body".into(),
        source: Source::Manual,
    })
    .unwrap()
}

/// Plans and runs every job; returns (planned, embedded, failed).
fn backfill(s: &SqliteStore, e: &Fake) -> (usize, usize, usize) {
    let now = OffsetDateTime::now_utc();
    let planned = plan_embeddings(s, e, now).unwrap().enqueued;
    let (mut ok, mut bad) = (0, 0);
    while let Some(d) = run_next_embedding(s, e, "w", now).unwrap() {
        if d.result.is_ok() { ok += 1 } else { bad += 1 }
    }
    (planned, ok, bad)
}

#[test]
fn backfills_once_and_pins_model_and_dimension() {
    let (_d, s) = store();
    let (a, b) = (save(&s, "a"), save(&s, "b"));
    let e = fake("m1", 3);
    assert_eq!(backfill(&s, &e), (2, 2, 0));
    let idx = s.embedding_index().unwrap().unwrap();
    assert_eq!((idx.model_id.as_str(), idx.dimension), ("m1", Some(3)));
    assert_eq!(s.get_embedding(a.id).unwrap().unwrap().len(), 3);
    assert!(s.get_embedding(b.id).unwrap().is_some());
    // Nothing is missing, so a second run queues nothing.
    assert_eq!(backfill(&s, &e), (0, 0, 0));
}

#[test]
fn edited_memory_is_embedded_again() {
    let (_d, s) = store();
    let m = save(&s, "a");
    let e = fake("m1", 2);
    backfill(&s, &e);
    let before = s.get_embedding(m.id).unwrap().unwrap();
    s.update(
        None,
        m.id,
        1,
        MemoryChanges {
            title: Some("a much longer title".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(backfill(&s, &e), (1, 1, 0));
    assert_ne!(s.get_embedding(m.id).unwrap().unwrap(), before);
}

#[test]
fn model_switch_rebuilds_everything_even_back_to_the_first_model() {
    let (_d, s) = store();
    let m = save(&s, "a");
    backfill(&s, &fake("m1", 2));
    let plan = plan_embeddings(&s, &fake("m2", 4), OffsetDateTime::now_utc()).unwrap();
    assert!(plan.rebuilt);
    assert!(
        s.get_embedding(m.id).unwrap().is_none(),
        "old vectors are dropped"
    );
    while run_next_embedding(&s, &fake("m2", 4), "w", OffsetDateTime::now_utc())
        .unwrap()
        .is_some()
    {}
    assert_eq!(s.get_embedding(m.id).unwrap().unwrap().len(), 4);
    // m1's finished job must not make a switch back look done.
    assert_eq!(backfill(&s, &fake("m1", 2)), (1, 1, 0));
    assert_eq!(s.get_embedding(m.id).unwrap().unwrap().len(), 2);
}

#[test]
fn dimension_and_model_mismatches_are_refused() {
    let (_d, s) = store();
    let m = save(&s, "a");
    backfill(&s, &fake("m1", 2));
    let wrong_dim = s.put_embedding(m.id, 1, "m1", &[1.0, 2.0, 3.0]);
    assert!(matches!(wrong_dim, Err(Error::InvalidArgument(_))));
    let wrong_model = s.put_embedding(m.id, 1, "m2", &[1.0, 2.0]);
    assert!(matches!(wrong_model, Err(Error::InvalidArgument(_))));
    assert!(s.put_embedding(m.id, 1, "m1", &[f32::NAN, 1.0]).is_err());
    // The stored vector survived the refusals.
    assert_eq!(s.get_embedding(m.id).unwrap().unwrap().len(), 2);
}

#[test]
fn provider_outage_retries_and_extraction_leaves_embed_jobs_alone() {
    let (_d, s) = store();
    let m = save(&s, "a");
    let e = fake("m1", 2);
    e.down.set(true);
    assert_eq!(backfill(&s, &e), (1, 0, 1));
    let jobs = s.jobs(None, 10).unwrap();
    assert_eq!(
        (jobs[0].kind.as_str(), jobs[0].state),
        (EMBED_JOB_KIND, JobState::Pending)
    );
    assert!(s.get_embedding(m.id).unwrap().is_none());

    struct Never;
    impl memory_core::extract::MemoryExtractor for Never {
        fn name(&self) -> String {
            "never".into()
        }
        fn extract(
            &self,
            _: &memory_core::extract::ExtractionInput,
        ) -> Result<Vec<memory_core::extract::Candidate>> {
            unreachable!("extraction must not receive an embed job")
        }
    }
    // Far enough ahead that the embed job's backoff has passed.
    let later = OffsetDateTime::now_utc() + time::Duration::hours(2);
    assert!(run_next(&s, &Never, "x", later).unwrap().is_none());

    e.down.set(false);
    let d = run_next_embedding(&s, &e, "w", later).unwrap().unwrap();
    assert!(d.result.is_ok());
    assert!(s.get_embedding(m.id).unwrap().is_some());
}

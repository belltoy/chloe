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

// ---- B3-02 hybrid retrieval ----

use memory_core::service::SearchQuery;
use memory_core::{ProjectId, Status};
use memory_engine::hybrid_search;

/// Embeds by topic keyword so "semantic" matches need no shared words with the query.
struct Topics;
impl Embedder for Topics {
    fn model_id(&self) -> String {
        "topics".into()
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let has = |t: &str, ws: &[&str]| ws.iter().any(|w| t.contains(w)) as u8 as f32;
        Ok(texts
            .iter()
            .map(|t| {
                vec![
                    has(t, &["database", "数据库", "SQLite"]),
                    has(t, &["network", "网络"]),
                    0.01,
                ]
            })
            .collect())
    }
}

fn q(text: &str) -> SearchQuery {
    SearchQuery {
        query: text.into(),
        limit: 10,
        include_global: false,
        include_inactive: false,
    }
}

fn save_in(s: &SqliteStore, p: Option<ProjectId>, title: &str, content: &str) -> Memory {
    s.save(NewMemory {
        project_id: p,
        kind: Kind::Fact,
        title: title.into(),
        content: content.into(),
        source: Source::Manual,
    })
    .unwrap()
}

fn embed_all(s: &SqliteStore) {
    let now = OffsetDateTime::now_utc();
    plan_embeddings(s, &Topics, now).unwrap();
    while run_next_embedding(s, &Topics, "w", now).unwrap().is_some() {}
}

fn project(dir: &tempfile::TempDir, name: &str, s: &SqliteStore) -> ProjectId {
    let p = dir.path().join(name);
    std::fs::create_dir(&p).unwrap();
    s.resolve_project(&p).unwrap()
}

#[test]
fn vector_path_finds_what_keywords_miss_and_both_paths_fuse() {
    let (d, s) = store();
    let p = project(&d, "p", &s);
    let wal = save_in(&s, Some(p), "WAL", "SQLite 并发读写");
    let net = save_in(&s, Some(p), "Listener", "never open a network port");
    embed_all(&s);
    // No shared token with either memory: FTS is empty, the vector path answers.
    let r = hybrid_search(&s, Some(&Topics), Some(p), q("database")).unwrap();
    assert!(r.degraded.is_none());
    assert_eq!(r.hits[0].hit.memory.id, wal.id);
    assert_eq!((r.hits[0].fts_rank, r.hits[0].vector_rank), (None, Some(1)));
    // "network" matches `net` by keyword and by vector: it leads and carries both ranks.
    let r = hybrid_search(&s, Some(&Topics), Some(p), q("network")).unwrap();
    assert_eq!(r.hits[0].hit.memory.id, net.id);
    assert_eq!(
        (r.hits[0].fts_rank, r.hits[0].vector_rank),
        (Some(1), Some(1))
    );
}

#[test]
fn scope_and_status_are_hard_filters_on_the_vector_path() {
    let (d, s) = store();
    let (a, b) = (project(&d, "a", &s), project(&d, "b", &s));
    let mine = save_in(&s, Some(a), "mine", "database");
    save_in(&s, Some(b), "theirs", "database");
    let global = save_in(&s, None, "global", "database");
    let old = save_in(&s, Some(a), "old", "database");
    s.archive(Some(a), old.id, 1).unwrap();
    embed_all(&s);
    let ids = |query: SearchQuery| -> Vec<_> {
        hybrid_search(&s, Some(&Topics), Some(a), query)
            .unwrap()
            .hits
            .iter()
            .map(|h| h.hit.memory.id)
            .collect()
    };
    assert_eq!(ids(q("database")), [mine.id]);
    let mut with_global = q("database");
    with_global.include_global = true;
    let got = ids(with_global);
    assert!(got.contains(&mine.id) && got.contains(&global.id) && got.len() == 2);
    let mut all = q("database");
    all.include_inactive = true;
    assert!(ids(all).contains(&old.id));
    assert_eq!(s.get(Some(a), old.id).unwrap().status, Status::Archived);
}

#[test]
fn degrades_to_fts_when_the_vector_path_is_unavailable() {
    let (d, s) = store();
    let p = project(&d, "p", &s);
    let m = save_in(&s, Some(p), "WAL", "SQLite journal");
    // Not embedded yet: the index is not pinned, so the vector path is skipped.
    let r = hybrid_search(&s, Some(&Topics), Some(p), q("journal")).unwrap();
    assert!(r.degraded.is_some());
    assert_eq!(r.hits[0].hit.memory.id, m.id);
    // Provider down, and no provider at all.
    embed_all(&s);
    let down = fake("topics", 3);
    down.down.set(true);
    for e in [Some(&down as &dyn Embedder), None] {
        let r = hybrid_search(&s, e, Some(p), q("journal")).unwrap();
        assert!(r.degraded.is_some());
        assert_eq!(r.hits[0].hit.memory.id, m.id);
    }
    // A stale vector (memory edited since) is not served.
    s.update(
        Some(p),
        m.id,
        1,
        MemoryChanges {
            content: Some("changed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let r = hybrid_search(&s, Some(&Topics), Some(p), q("database")).unwrap();
    assert!(r.hits.is_empty());
}

#[test]
fn fts_failure_is_not_degraded_away() {
    let (_d, s) = store();
    assert!(matches!(
        hybrid_search(&s, Some(&Topics), None, q("   ")),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn eval_scores_every_mode_per_language_and_rejects_bad_labels() {
    use memory_engine::{Dataset, run_eval};
    let ds = |relevant: &str| -> Dataset {
        serde_json::from_value(serde_json::json!({
            "memories": [
                {"key": "a", "kind": "fact", "title": "WAL", "content": "SQLite journal"},
                {"key": "b", "kind": "fact", "title": "Port", "content": "never open a network port"},
            ],
            "queries": [
                {"id": "q1", "lang": "en", "text": "journal", "relevant": [relevant]},
                {"id": "q2", "lang": "cross", "text": "database", "relevant": ["a"]},
            ],
        }))
        .unwrap()
    };
    let (d, s) = store();
    let p = project(&d, "p", &s);
    let report = run_eval(&s, &Topics, p, &ds("a"), 1).unwrap();
    // 3 modes x (en, cross, all)
    assert_eq!(report.rows.len(), 9);
    let row = |lang: &str, mode: &str| {
        report
            .rows
            .iter()
            .find(|r| r.lang == lang && r.mode == mode)
            .unwrap()
    };
    assert_eq!(
        (row("en", "fts").recall_at_5, row("en", "fts").mrr),
        (1.0, 1.0)
    );
    // "database" shares no word with the memory: only the vector path finds it.
    assert_eq!(row("cross", "fts").recall_at_5, 0.0);
    assert_eq!(row("cross", "vector").mrr, 1.0);
    assert_eq!(row("all", "hybrid").queries, 2);

    let (d, s) = store();
    let p = project(&d, "p", &s);
    assert!(matches!(
        run_eval(&s, &Topics, p, &ds("missing"), 1),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn each_mode_uses_only_its_paths_and_vector_falls_back_to_fts() {
    use memory_engine::{SearchMode, search};
    let (d, s) = store();
    let p = project(&d, "p", &s);
    let net = save_in(&s, Some(p), "Listener", "never open a network port");
    embed_all(&s);
    let run = |m, e: Option<&dyn Embedder>| search(&s, m, e, Some(p), q("network")).unwrap();
    let r = run(SearchMode::Fts, None);
    assert_eq!((r.mode, r.degraded.is_none()), (SearchMode::Fts, true));
    assert_eq!(r.hits[0].matched_by, ["fts"]);
    let r = run(SearchMode::Vector, Some(&Topics));
    assert_eq!((r.mode, r.fts_returned), (SearchMode::Vector, 0));
    assert_eq!(r.hits[0].hit.memory.id, net.id);
    assert_eq!(r.hits[0].matched_by, ["vector"]);
    assert_eq!(
        run(SearchMode::Hybrid, Some(&Topics)).hits[0].matched_by,
        ["fts", "vector"]
    );
    // Vector asked for, provider down: full-text answers and says why.
    let down = fake("topics", 3);
    down.down.set(true);
    let r = run(SearchMode::Vector, Some(&down));
    assert_eq!(r.mode, SearchMode::Fts);
    assert!(r.degraded.is_some());
    assert_eq!(r.hits[0].hit.memory.id, net.id);
}

#[test]
fn a_one_word_keyword_match_does_not_join_fusion_unless_it_is_an_identifier() {
    let (d, s) = store();
    let p = project(&d, "p", &s);
    let common = save_in(&s, Some(p), "note", "policy only");
    let two = save_in(&s, Some(p), "net", "network policy details");
    let code = save_in(&s, Some(p), "code", "E1042 means retry");
    embed_all(&s);
    let r = hybrid_search(&s, Some(&Topics), Some(p), q("network policy E1042")).unwrap();
    let rank = |id| {
        r.hits
            .iter()
            .find(|h| h.hit.memory.id == id)
            .map(|h| h.fts_rank)
    };
    // One shared common word: not an FTS candidate any more (the vector path may still list it).
    assert_ne!(rank(common.id), Some(Some(1)));
    assert!(rank(common.id).is_none_or(|r| r.is_none()));
    assert!(rank(two.id).unwrap().is_some());
    assert!(rank(code.id).unwrap().is_some());
    // Without a vector path nothing is gated.
    let r = hybrid_search(&s, None, Some(p), q("network policy E1042")).unwrap();
    assert!(r.hits.iter().any(|h| h.hit.memory.id == common.id));
}

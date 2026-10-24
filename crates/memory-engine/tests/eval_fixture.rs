//! The committed evaluation dataset (B3-04) as a CI fixture: it must stay well formed, and the
//! deterministic FTS baseline must not drift unnoticed. The vector side needs a real model and
//! is measured by `chloe eval`, not here.

use std::collections::HashSet;

use memory_core::embed::Embedder;
use memory_core::{ProjectId, Result};
use memory_engine::{Dataset, run_eval};
use memory_store::SqliteStore;

fn dataset() -> Dataset {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/evidence/milestone-b/eval/dataset.json"
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Deterministic stand-in: a bag of hashed characters. Good enough to exercise the run.
struct Chars;
impl Embedder for Chars {
    fn model_id(&self) -> String {
        "chars".into()
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.01_f32; 64];
                t.chars().for_each(|c| v[c as usize % 64] += 1.0);
                v
            })
            .collect())
    }
}

#[test]
fn dataset_is_well_formed() {
    let ds = dataset();
    assert!(
        (50..=100).contains(&ds.memories.len()),
        "{}",
        ds.memories.len()
    );
    assert!(
        (20..=30).contains(&ds.queries.len()),
        "{}",
        ds.queries.len()
    );
    let keys: HashSet<_> = ds.memories.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys.len(), ds.memories.len(), "duplicate memory key");
    let ids: HashSet<_> = ds.queries.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids.len(), ds.queries.len(), "duplicate query id");
    for q in &ds.queries {
        assert!(
            ["en", "zh", "mixed", "cross"].contains(&q.lang.as_str()),
            "{}",
            q.id
        );
        assert!(!q.relevant.is_empty(), "{}", q.id);
        assert!(
            q.relevant.iter().all(|k| keys.contains(k.as_str())),
            "{}",
            q.id
        );
    }
    for lang in ["en", "zh", "mixed", "cross"] {
        assert!(
            ds.queries.iter().filter(|q| q.lang == lang).count() >= 5,
            "{lang}"
        );
    }
}

#[test]
fn fts_baseline_has_not_drifted() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let p: ProjectId = store.resolve_project(dir.path()).unwrap();
    let report = run_eval(&store, &Chars, p, &dataset(), 1).unwrap();
    let fts = |lang: &str| {
        report
            .rows
            .iter()
            .find(|r| r.lang == lang && r.mode == "fts")
            .unwrap()
            .recall_at_5
    };
    // Known limitation (B3-07): unicode61 does not segment Chinese, so Chinese-only queries
    // find nothing. When B3-07 lands this assertion should be replaced by a real floor.
    assert_eq!(fts("zh"), 0.0);
    assert!(fts("en") >= 0.6, "{}", fts("en"));
    assert!(fts("all") >= 0.3, "{}", fts("all"));
    assert_eq!(report.rows.len(), 15);
}

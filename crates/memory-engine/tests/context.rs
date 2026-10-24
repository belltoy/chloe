use memory_core::service::{MemoryService, NewMemory};
use memory_core::{Error, Kind, ProjectId, Source};
use memory_engine::{SearchMode, context_pack};
use memory_store::SqliteStore;

fn world() -> (tempfile::TempDir, SqliteStore, ProjectId, ProjectId) {
    let dir = tempfile::tempdir().unwrap();
    let s = SqliteStore::open(&dir.path().join("t.db")).unwrap();
    let mk = |n: &str| {
        let p = dir.path().join(n);
        std::fs::create_dir(&p).unwrap();
        s.resolve_project(&p).unwrap()
    };
    let (a, b) = (mk("a"), mk("b"));
    (dir, s, a, b)
}

fn save(s: &SqliteStore, p: Option<ProjectId>, kind: Kind, title: &str, body: &str) {
    s.save(NewMemory {
        project_id: p,
        kind,
        title: title.into(),
        content: body.into(),
        source: Source::Manual,
    })
    .unwrap();
}

fn pack(s: &SqliteStore, p: ProjectId, task: &str, budget: usize) -> memory_engine::ContextPack {
    context_pack(s, SearchMode::Fts, None, Some(p), task, false, budget).unwrap()
}

#[test]
fn groups_by_kind_in_a_fixed_order_with_ids_and_sources() {
    let (_d, s, a, _) = world();
    save(
        &s,
        Some(a),
        Kind::Fact,
        "Database file",
        "the database lives in one file",
    );
    save(
        &s,
        Some(a),
        Kind::Gotcha,
        "Database locked",
        "writers block each other",
    );
    save(
        &s,
        Some(a),
        Kind::Constraint,
        "Database writes",
        "never write from two threads",
    );
    let p = pack(&s, a, "database", 4000);
    let kinds: Vec<_> = p.groups.iter().map(|g| g.kind).collect();
    assert_eq!(kinds, [Kind::Constraint, Kind::Gotcha, Kind::Fact]);
    let item = &p.groups[0].items[0];
    assert_eq!(
        (item.source, item.revision, item.summary_truncated),
        (Source::Manual, 1, false)
    );
    // The id expands to the full memory.
    assert_eq!(s.get(Some(a), item.id).unwrap().title, "Database writes");
    assert_eq!((p.mode, p.omitted), (SearchMode::Fts, 0));
}

#[test]
fn the_budget_is_never_exceeded_and_the_rest_is_counted() {
    let (_d, s, a, _) = world();
    let body = format!("database {}", "word ".repeat(200));
    for i in 0..10 {
        save(
            &s,
            Some(a),
            Kind::Fact,
            &format!("Database note {i}"),
            &body,
        );
    }
    let small = pack(&s, a, "database", 400);
    assert!(small.used <= 400, "{}", small.used);
    let shown: usize = small.groups.iter().map(|g| g.items.len()).sum();
    assert!(
        shown >= 1 && small.omitted == 10 - shown,
        "{shown} shown, {} omitted",
        small.omitted
    );
    assert!(small.groups[0].items[0].summary_truncated);
    let large = pack(&s, a, "database", 20_000);
    assert!(large.used <= 20_000 && large.omitted == 0);
    assert!(large.used > small.used);
}

#[test]
fn another_projects_memories_are_not_in_the_pack_nor_expandable() {
    let (_d, s, a, b) = world();
    save(&s, Some(a), Kind::Fact, "Mine", "database mine");
    save(&s, Some(b), Kind::Fact, "Theirs", "database theirs");
    let p = pack(&s, a, "database", 4000);
    let titles: Vec<_> = p
        .groups
        .iter()
        .flat_map(|g| g.items.iter().map(|i| i.title.clone()))
        .collect();
    assert_eq!(titles, ["Mine"]);
    let theirs = pack(&s, b, "database", 4000).groups[0].items[0].id;
    assert!(matches!(s.get(Some(a), theirs), Err(Error::NotFound(_))));
}

#[test]
fn archived_memories_are_left_out_and_a_bad_budget_is_rejected() {
    let (_d, s, a, _) = world();
    save(&s, Some(a), Kind::Fact, "Old", "database old");
    let id = pack(&s, a, "database", 4000).groups[0].items[0].id;
    s.archive(Some(a), id, 1).unwrap();
    assert!(pack(&s, a, "database", 4000).groups.is_empty());
    for bad in [0, 199, 20_001] {
        let r = context_pack(&s, SearchMode::Fts, None, Some(a), "database", false, bad);
        assert!(matches!(r, Err(Error::InvalidArgument(_))), "{bad}");
    }
}

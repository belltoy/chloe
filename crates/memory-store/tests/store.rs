use std::path::PathBuf;
use std::sync::{Arc, Barrier};

use memory_core::service::{MemoryChanges, MemoryService, NewMemory, SearchQuery};
use memory_core::{Error, Kind, ProjectId, Source, Status};
use memory_store::SqliteStore;

fn db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    (dir, path)
}

fn new(project: Option<ProjectId>, title: &str, content: &str) -> NewMemory {
    NewMemory {
        project_id: project,
        kind: Kind::Fact,
        title: title.into(),
        content: content.into(),
        source: Source::Manual,
    }
}

fn q(query: &str) -> SearchQuery {
    SearchQuery {
        query: query.into(),
        limit: 10,
        include_global: false,
        include_inactive: false,
    }
}

fn project(store: &SqliteStore, dir: &tempfile::TempDir, name: &str) -> ProjectId {
    let p = dir.path().join(name);
    std::fs::create_dir_all(&p).unwrap();
    store.resolve_project(&p).unwrap()
}

#[test]
fn persists_across_restart() {
    let (dir, path) = db();
    let (pid, id) = {
        let s = SqliteStore::open(&path).unwrap();
        let pid = project(&s, &dir, "a");
        (
            pid,
            s.save(new(Some(pid), "Build", "use cargo nextest"))
                .unwrap()
                .id,
        )
    };
    let s = SqliteStore::open(&path).unwrap();
    assert_eq!(s.get(Some(pid), id).unwrap().title, "Build");
    assert_eq!(project(&s, &dir, "a"), pid);
    assert_eq!(s.search(Some(pid), q("nextest")).unwrap().len(), 1);
}

#[test]
fn update_keeps_history_and_syncs_fts() {
    let (dir, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let pid = project(&s, &dir, "a");
    let m = s.save(new(Some(pid), "Lint", "run clippy")).unwrap();
    let m2 = s
        .update(
            Some(pid),
            m.id,
            1,
            MemoryChanges {
                content: Some("run rustfmt".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(m2.revision, 2);
    assert!(s.search(Some(pid), q("clippy")).unwrap().is_empty());
    assert_eq!(s.search(Some(pid), q("rustfmt")).unwrap().len(), 1);

    let a = s.archive(Some(pid), m.id, 2).unwrap();
    assert_eq!((a.status, a.revision), (Status::Archived, 3));
    assert!(s.search(Some(pid), q("rustfmt")).unwrap().is_empty());
    assert!(s.list(Some(pid), false, 10).unwrap().is_empty());
    assert_eq!(s.list(Some(pid), true, 10).unwrap().len(), 1);

    let h = s.history(Some(pid), m.id).unwrap();
    let causes: Vec<_> = h.iter().map(|r| (r.revision, r.cause.as_str())).collect();
    assert_eq!(causes, [(1, "create"), (2, "update"), (3, "archive")]);
    assert_eq!(h[0].snapshot.content, "run clippy");
}

#[test]
fn stale_revision_conflicts() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let m = s.save(new(None, "t", "c")).unwrap();
    let ch = || MemoryChanges {
        title: Some("x".into()),
        ..Default::default()
    };
    s.update(None, m.id, 1, ch()).unwrap();
    assert!(matches!(
        s.update(None, m.id, 1, ch()),
        Err(Error::Conflict {
            expected: 1,
            current: 2
        })
    ));
    assert!(matches!(
        s.archive(None, m.id, 1),
        Err(Error::Conflict { .. })
    ));
}

#[test]
fn failed_update_rolls_back_everything() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let m = s.save(new(None, "old title", "body")).unwrap();
    // Occupy revision 2 so the history insert (last step of update) fails.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO memory_revisions (memory_id, revision, snapshot, changed_at, cause) VALUES (?1, 2, '{}', 0, 'x')",
            [m.id.to_string()],
        )
        .unwrap();
    let ch = MemoryChanges {
        title: Some("new title".into()),
        ..Default::default()
    };
    assert!(matches!(
        s.update(None, m.id, 1, ch),
        Err(Error::Storage(_))
    ));
    let after = s.get(None, m.id).unwrap();
    assert_eq!((after.revision, after.title.as_str()), (1, "old title"));
    assert_eq!(s.search(None, q("old")).unwrap().len(), 1);
    assert!(s.search(None, q("new")).unwrap().is_empty());
}

#[test]
fn concurrent_cas_has_one_winner() {
    let (_d, path) = db();
    let m = SqliteStore::open(&path)
        .unwrap()
        .save(new(None, "t", "c"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                let s = SqliteStore::open(&path).unwrap();
                barrier.wait();
                s.update(
                    None,
                    m.id,
                    1,
                    MemoryChanges {
                        title: Some(format!("w{i}")),
                        ..Default::default()
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter(|r| r.is_err())
            .all(|r| matches!(r, Err(Error::Conflict { .. })))
    );
    let s = SqliteStore::open(&path).unwrap();
    assert_eq!(s.get(None, m.id).unwrap().revision, 2);
    assert_eq!(s.history(None, m.id).unwrap().len(), 2);
}

#[test]
fn projects_are_isolated() {
    let (dir, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let (a, b) = (project(&s, &dir, "a"), project(&s, &dir, "b"));
    assert_ne!(a, b);
    let ma = s.save(new(Some(a), "secret", "alpha token")).unwrap();
    let g = s.save(new(None, "shared", "alpha global")).unwrap();

    assert!(matches!(s.get(Some(b), ma.id), Err(Error::NotFound(_))));
    assert!(matches!(
        s.update(
            Some(b),
            ma.id,
            1,
            MemoryChanges {
                title: Some("x".into()),
                ..Default::default()
            }
        ),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        s.archive(Some(b), ma.id, 1),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(s.history(Some(b), ma.id), Err(Error::NotFound(_))));
    assert!(s.search(Some(b), q("alpha")).unwrap().is_empty());
    assert!(s.list(Some(b), true, 10).unwrap().is_empty());

    // Global memories are opt-in for search, but readable by id.
    assert_eq!(s.get(Some(b), g.id).unwrap().id, g.id);
    let with_global = SearchQuery {
        include_global: true,
        ..q("alpha")
    };
    let hits = s.search(Some(a), with_global).unwrap();
    assert_eq!(hits.len(), 2);
    assert!(matches!(
        s.save(new(Some(ProjectId::new()), "t", "c")),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn search_input_is_never_fts_syntax() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    s.save(new(None, "Quote", "say \"hello\" world")).unwrap();
    for bad in ["\"", "hello AND", "col:x", "NEAR(", "*", "a OR"] {
        s.search(None, q(bad)).unwrap();
    }
    assert_eq!(s.search(None, q("\"hello\"")).unwrap().len(), 1);
    assert!(matches!(
        s.search(None, q("   ")),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn search_matches_any_token_and_ranks_more_matches_first() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    s.save(new(
        None,
        "Use sqlx pool size 8",
        "Pool size 8 chosen after load tests",
    ))
    .unwrap();
    s.save(new(None, "Retry policy", "size of the retry budget"))
        .unwrap();
    let hits = s.search(None, q("pool size connection pool")).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.title, "Use sqlx pool size 8");
}

#[test]
fn validation() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    assert!(matches!(
        s.save(new(None, " ", "c")),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        s.save(new(None, "t", &"x".repeat(70_000))),
        Err(Error::InvalidArgument(_))
    ));
    let m = s.save(new(None, "t", "c")).unwrap();
    assert!(matches!(
        s.update(None, m.id, 1, MemoryChanges::default()),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn doctor_detects_and_reindex_repairs() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    s.save(new(None, "findme", "c")).unwrap();
    assert!(s.doctor().unwrap().healthy());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO memory_fts(memory_fts) VALUES ('delete-all')",
            [],
        )
        .unwrap();
    assert!(!s.doctor().unwrap().fts_ok);
    assert!(s.search(None, q("findme")).unwrap().is_empty());
    s.reindex().unwrap();
    assert!(s.doctor().unwrap().healthy());
    assert_eq!(s.search(None, q("findme")).unwrap().len(), 1);
}

#[test]
fn newer_schema_is_refused() {
    let (_d, path) = db();
    drop(SqliteStore::open(&path).unwrap());
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    assert!(matches!(SqliteStore::open(&path), Err(Error::Storage(_))));
}

#[test]
fn scope_resolver_git_worktree_and_plain_dir() {
    let (dir, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let main = dir.path().join("repo");
    std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
    std::fs::create_dir_all(main.join("src/deep")).unwrap();
    let wt = dir.path().join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let gitdir = main.join(".git/worktrees/wt").canonicalize().unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();

    let id = s.resolve_project(&main).unwrap();
    assert_eq!(s.resolve_project(&main.join("src/deep")).unwrap(), id);
    assert_eq!(s.resolve_project(&wt).unwrap(), id);
    let plain = project(&s, &dir, "plain");
    assert_ne!(plain, id);

    // A moved checkout is a new project until explicitly linked.
    let moved = dir.path().join("moved");
    std::fs::create_dir_all(&moved).unwrap();
    s.add_alias(id, &moved, "moved").unwrap();
    assert_eq!(s.resolve_project(&moved).unwrap(), id);
    assert!(matches!(
        s.add_alias(plain, &moved, "moved"),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        s.resolve_project(&dir.path().join("nope")),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn audit_trail_records_actor_and_is_append_only() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path).unwrap().with_actor("cli");
    let m = s.save(new(None, "t", "body")).unwrap();
    s.archive(None, m.id, 1).unwrap();
    let h = s.history(None, m.id).unwrap();
    let trail: Vec<_> = h
        .iter()
        .map(|r| (r.revision, r.cause.as_str(), r.actor.as_str()))
        .collect();
    assert_eq!(trail, [(1, "create", "cli"), (2, "archive", "cli")]);

    let raw = rusqlite::Connection::open(&path).unwrap();
    assert!(
        raw.execute("UPDATE memory_revisions SET cause='x'", [])
            .is_err()
    );
    assert!(raw.execute("DELETE FROM memory_revisions", []).is_err());
}

#[test]
fn global_memory_is_editable_from_any_project_but_private_is_not() {
    let (dir, path) = db();
    let s = SqliteStore::open(&path).unwrap();
    let (a, b) = (project(&s, &dir, "a"), project(&s, &dir, "b"));
    let g = s.save(new(None, "G", "global")).unwrap();
    let p = s.save(new(Some(a), "P", "private")).unwrap();
    let ch = || MemoryChanges {
        title: Some("T".into()),
        ..Default::default()
    };
    assert_eq!(s.update(Some(b), g.id, 1, ch()).unwrap().revision, 2);
    assert!(matches!(
        s.update(Some(b), p.id, 1, ch()),
        Err(Error::NotFound(_))
    ));
}

#[cfg(unix)]
#[test]
fn database_and_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data/sub/t.db");
    let s = SqliteStore::open(&path).unwrap();
    s.save(new(None, "t", "c")).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    let wal = path.with_extension("db-wal");
    if wal.exists() {
        assert_eq!(mode(&wal), 0o600);
    }
}

#[test]
fn busy_database_fails_with_storage_error_then_recovers() {
    let (_d, path) = db();
    let s = SqliteStore::open(&path)
        .unwrap()
        .with_busy_timeout(std::time::Duration::from_millis(100))
        .unwrap();
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap(); // holds the write lock
    let err = s.save(new(None, "t", "c")).unwrap_err();
    assert!(
        matches!(&err, Error::Storage(m) if m.contains("locked")),
        "{err}"
    );
    other.execute_batch("ROLLBACK").unwrap();
    s.save(new(None, "t", "c")).unwrap();
}

#[test]
fn chinese_keywords_match_through_word_segmentation() {
    let (dir, path) = db();
    let store = SqliteStore::open(&path).unwrap();
    let p = project(&store, &dir, "p");
    let wal = store
        .save(new(Some(p), "并发写入", "数据库使用 WAL 模式支持并发读写"))
        .unwrap();
    store
        .save(new(Some(p), "Other", "nothing about that"))
        .unwrap();
    // A whole sentence: question words are dropped, the content words match.
    let hits = store.search(Some(p), q("怎样支持并发读写？")).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, wal.id);
    // Title matches, mixed script, and a two-character word (which a trigram index cannot do).
    assert_eq!(store.search(Some(p), q("并发写入")).unwrap().len(), 1);
    assert_eq!(store.search(Some(p), q("WAL模式")).unwrap().len(), 1);
    assert_eq!(store.search(Some(p), q("并发")).unwrap().len(), 1);
    // Quotes and FTS syntax in the query stay data.
    assert!(store.search(Some(p), q("\"并发读写\" OR * NEAR(")).is_ok());
    // Nothing but question words: falls back to the regular index instead of failing.
    assert!(store.search(Some(p), q("什么吗")).is_ok());

    // Edits and archives keep the index in step, and a rebuild leaves it consistent.
    let m = store
        .update(
            Some(p),
            wal.id,
            1,
            MemoryChanges {
                content: Some("改成了别的内容".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(store.search(Some(p), q("读写模式")).unwrap().is_empty());
    assert_eq!(store.search(Some(p), q("改成了别的")).unwrap().len(), 1);
    store.reindex().unwrap();
    assert!(store.doctor().unwrap().fts_ok);
    store.archive(Some(p), m.id, m.revision).unwrap();
    assert!(store.search(Some(p), q("改成了别的")).unwrap().is_empty());
}

#[test]
fn the_segmented_index_is_built_on_upgrade_and_rebuilt_when_segmentation_changes() {
    let (dir, path) = db();
    let p;
    {
        let store = SqliteStore::open(&path).unwrap();
        p = project(&store, &dir, "p");
        store
            .save(new(Some(p), "旧记忆", "升级之前保存的中文内容"))
            .unwrap();
    }
    let raw = || rusqlite::Connection::open(&path).unwrap();
    // A database from the trigram version (schema 10): no segmented index yet.
    raw()
        .execute_batch(
            "DROP TABLE memory_fts_seg; DROP TABLE fts_meta;
             ALTER TABLE sessions DROP COLUMN extracted_seq;
             CREATE VIRTUAL TABLE memory_fts_cjk USING fts5(title, content, tokenize='trigram');
             PRAGMA user_version = 10;",
        )
        .unwrap();
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(store.search(Some(p), q("升级之前保存")).unwrap().len(), 1);
    assert!(store.doctor().unwrap().fts_ok);
    drop(store);
    // An index built by another segmentation is rebuilt on open.
    raw()
        .execute_batch(
            "DELETE FROM memory_fts_seg; UPDATE fts_meta SET value = 'older-dictionary';",
        )
        .unwrap();
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(store.search(Some(p), q("升级之前保存")).unwrap().len(), 1);
}

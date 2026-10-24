use std::process::Command;

use memory_core::job::JobQueue;
use memory_store::SqliteStore;
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};

fn chloe(db: &std::path::Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
        .env_remove("CHLOE_DB")
        .arg("--db-path")
        .arg(db)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn list_show_and_retry_a_dead_letter() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let id = {
        let s = SqliteStore::open(&db).unwrap();
        let mut now = OffsetDateTime::now_utc() - Duration::days(1);
        let id = s
            .enqueue("extract", "s1", json!({"s": 1}), now)
            .unwrap()
            .job
            .id;
        loop {
            let Some(j) = s.lease("w", Duration::seconds(5), now).unwrap() else {
                now += Duration::hours(2);
                continue;
            };
            let f = s
                .fail(&j.id, j.lease_id.as_ref().unwrap(), "boom", now)
                .unwrap();
            if f.state == memory_core::job::JobState::DeadLetter {
                break id;
            }
        }
    };
    let dead = chloe(&db, &["jobs", "list", "--state", "dead_letter"]);
    assert_eq!(dead.as_array().unwrap().len(), 1);
    assert_eq!(chloe(&db, &["jobs", "show", &id])["last_error"], "boom");
    let r = chloe(&db, &["jobs", "retry", &id]);
    assert_eq!(
        (r["state"].as_str(), r["attempts"].as_u64()),
        (Some("pending"), Some(0))
    );
    assert!(
        chloe(&db, &["jobs", "list", "--state", "dead_letter"])
            .as_array()
            .unwrap()
            .is_empty()
    );
}

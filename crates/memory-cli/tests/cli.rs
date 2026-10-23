use std::process::Command;

use serde_json::Value;

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    fn run(&self, args: &[&str]) -> (bool, Value) {
        let proj = self.dir.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
            .env_remove("CHLOE_DB")
            .arg("--db-path")
            .arg(self.dir.path().join("t.db"))
            .arg("--dir")
            .arg(&proj)
            .arg("--json")
            .args(args)
            .output()
            .unwrap();
        let v = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (out.status.success(), v)
    }
}

#[test]
fn memory_lifecycle_json() {
    let e = Env {
        dir: tempfile::tempdir().unwrap(),
    };
    assert!(e.run(&["init"]).0);
    let (ok, m) = e.run(&[
        "memory",
        "add",
        "--title",
        "Build",
        "--content",
        "use nextest",
        "--kind",
        "procedure",
    ]);
    assert!(ok);
    let id = m["id"].as_str().unwrap().to_string();

    let (_, hits) = e.run(&["memory", "search", "nextest"]);
    assert_eq!(hits.as_array().unwrap().len(), 1);

    let (ok, m) = e.run(&[
        "memory",
        "update",
        &id,
        "--revision",
        "1",
        "--title",
        "Build2",
    ]);
    assert!(ok);
    assert_eq!(m["revision"], 2);

    let (ok, err) = e.run(&["memory", "update", &id, "--revision", "1", "--title", "x"]);
    assert!(!ok);
    assert_eq!(err["error"]["code"], "conflict");

    assert!(e.run(&["memory", "archive", &id, "--revision", "2"]).0);
    let (_, hits) = e.run(&["memory", "search", "nextest"]);
    assert!(hits.as_array().unwrap().is_empty());

    let (_, h) = e.run(&["memory", "history", &id]);
    let trail: Vec<_> = h
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["cause"].as_str().unwrap(), r["actor"].as_str().unwrap()))
        .collect();
    assert_eq!(
        trail,
        [("create", "cli"), ("update", "cli"), ("archive", "cli")]
    );

    let (ok, d) = e.run(&["doctor"]);
    assert!(ok, "{d}");
    let names: Vec<_> = d["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"mcp_roundtrip") && names.contains(&"fts_consistent"));
}

#[test]
fn doctor_reports_unusable_database() {
    let e = Env {
        dir: tempfile::tempdir().unwrap(),
    };
    // A file where the database's parent directory should be.
    let blocker = e.dir.path().join("blocker");
    std::fs::write(&blocker, "").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_chloe"))
        .args(["--json", "--db-path"])
        .arg(blocker.join("t.db"))
        .arg("doctor")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let failed: Vec<_> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] == false)
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(failed, ["database_open"]);
}

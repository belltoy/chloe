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
    assert!(e.run(&["doctor"]).0);
}

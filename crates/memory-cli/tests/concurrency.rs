use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn chloe(db: &std::path::Path, dir: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chloe"));
    c.env_remove("CHLOE_DB")
        .arg("--db-path")
        .arg(db)
        .arg("--dir")
        .arg(dir)
        .arg("--json");
    c
}

#[test]
fn parallel_processes_all_write_without_loss() {
    let tmp = tempfile::tempdir().unwrap();
    let (db, proj) = (tmp.path().join("t.db"), tmp.path().join("p"));
    std::fs::create_dir(&proj).unwrap();
    assert!(chloe(&db, &proj).arg("init").status().unwrap().success());

    let children: Vec<_> = (0..12)
        .map(|i| {
            chloe(&db, &proj)
                .args([
                    "memory",
                    "add",
                    "--title",
                    &format!("note {i}"),
                    "--content",
                    "parallelword",
                ])
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success(), "a parallel writer failed");
    }

    let out = chloe(&db, &proj)
        .args(["memory", "search", "parallelword", "--limit", "50"])
        .output()
        .unwrap();
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 12);
    assert!(
        chloe(&db, &proj)
            .arg("doctor")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn killed_server_leaves_committed_data_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let (db, proj) = (tmp.path().join("t.db"), tmp.path().join("p"));
    std::fs::create_dir(&proj).unwrap();

    let mut server = chloe(&db, &proj)
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_save","arguments":{"kind":"fact","title":"Survivor","content":"crashword"}}}"#;
    writeln!(server.stdin.as_mut().unwrap(), "{call}").unwrap();
    let mut reply = String::new();
    BufReader::new(server.stdout.as_mut().unwrap())
        .read_line(&mut reply)
        .unwrap();
    assert!(reply.contains("\"isError\":false"), "{reply}");
    server.kill().unwrap(); // SIGKILL: no clean shutdown
    server.wait().unwrap();

    let out = chloe(&db, &proj)
        .args(["memory", "search", "crashword"])
        .output()
        .unwrap();
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert!(
        chloe(&db, &proj)
            .arg("doctor")
            .output()
            .unwrap()
            .status
            .success()
    );
}

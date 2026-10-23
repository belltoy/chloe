//! `chloe doctor`: database health plus a real `mcp serve` round trip on a throwaway database.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, bail};
use memory_store::SqliteStore;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

pub fn run(db_path: &Path, reindex: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let mut add = |name, r: anyhow::Result<String>| {
        out.push(match r {
            Ok(detail) => Check {
                name,
                ok: true,
                detail,
            },
            Err(e) => Check {
                name,
                ok: false,
                detail: format!("{e:#}"),
            },
        })
    };
    let flag = |ok: bool| if ok { Ok("ok".into()) } else { bail!("failed") };

    match SqliteStore::open(db_path) {
        Err(e) => add("database_open", Err(e.into())),
        Ok(store) => {
            add("database_open", Ok(db_path.display().to_string()));
            if reindex {
                add(
                    "reindex",
                    store
                        .reindex()
                        .map(|_| "rebuilt".into())
                        .map_err(Into::into),
                );
            }
            match store.doctor() {
                Ok(d) => {
                    add("schema_version", Ok(d.schema_version.to_string()));
                    add("sqlite_integrity", flag(d.sqlite_ok));
                    add("fts_consistent", flag(d.fts_ok));
                    add("foreign_keys", flag(d.foreign_keys_ok));
                }
                Err(e) => add("database_checks", Err(e.into())),
            }
        }
    }
    add("mcp_roundtrip", mcp_roundtrip());
    out
}

fn mcp_roundtrip() -> anyhow::Result<String> {
    let dir = std::env::temp_dir().join(format!("chloe-doctor-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let result = spawn_and_check(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn spawn_and_check(dir: &Path) -> anyhow::Result<String> {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--db-path")
        .arg(dir.join("doctor.db"))
        .arg("--dir")
        .arg(dir)
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start `mcp serve`")?;
    let mut client = Client::new(&mut child);
    let result = client.exercise();
    let _ = child.kill();
    let _ = child.wait();
    result
}

struct Client {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
}

impl Client {
    fn new(child: &mut Child) -> Self {
        Self {
            stdin: child.stdin.take().expect("piped"),
            stdout: BufReader::new(child.stdout.take().expect("piped")),
            next_id: 0,
        }
    }

    // shortcut: no read timeout, a server that hangs hangs doctor; add one if that happens.
    fn rpc(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.next_id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        )?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let v: Value = serde_json::from_str(&line).context("no valid reply from `mcp serve`")?;
        if let Some(e) = v.get("error") {
            bail!("{method}: {e}");
        }
        Ok(v["result"].clone())
    }

    fn call(&mut self, tool: &str, args: Value) -> anyhow::Result<Value> {
        let r = self.rpc("tools/call", json!({"name": tool, "arguments": args}))?;
        let payload: Value =
            serde_json::from_str(r["content"][0]["text"].as_str().unwrap_or("null"))?;
        if r["isError"] == true {
            bail!("{tool}: {payload}");
        }
        Ok(payload)
    }

    fn exercise(&mut self) -> anyhow::Result<String> {
        let init = self.rpc("initialize", json!({"protocolVersion": "2025-06-18"}))?;
        if init["instructions"].as_str().is_none_or(str::is_empty) {
            bail!("initialize returned no instructions");
        }
        let tools = self.rpc("tools/list", json!({}))?;
        let n = tools["tools"].as_array().map_or(0, Vec::len);
        let saved = self.call(
            "memory_save",
            json!({"kind": "fact", "title": "doctor probe", "content": "doctor probe"}),
        )?;
        let id = saved["id"].clone();
        if self
            .call("memory_search", json!({"query": "probe"}))?
            .as_array()
            .map_or(0, Vec::len)
            != 1
        {
            bail!("memory_search did not find the probe memory");
        }
        self.call("memory_get", json!({"id": id}))?;
        self.call("memory_archive", json!({"id": id, "expected_revision": 1}))?;
        Ok(format!("{n} tools; save/search/get/archive ok"))
    }
}

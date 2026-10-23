//! Minimal MCP server over stdio (newline-delimited JSON-RPC 2.0), exposing the memory tools.
//! All business logic stays in `MemoryService`; this crate only maps JSON to service calls.
//! Scope is fixed at startup: every call runs as the given project.

use std::io::{self, BufRead, Write};

use memory_core::service::{MemoryChanges, MemoryService, NewMemory, SearchQuery};
use memory_core::{Error, Kind, MemoryId, ProjectId, Result, Source};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const LATEST: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    kind: Kind,
    title: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    query: String,
    limit: Option<usize>,
    #[serde(default)]
    include_global: bool,
    #[serde(default)]
    include_inactive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    id: MemoryId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: MemoryId,
    expected_revision: u32,
    kind: Option<Kind>,
    title: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    id: MemoryId,
    expected_revision: u32,
}

fn schema(description: &str, props: Value, required: &[&str]) -> Value {
    json!({
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false
        }
    })
}

fn tools() -> Value {
    let kind = json!({
        "type": "string",
        "enum": [
            "fact",
            "decision",
            "procedure",
            "constraint",
            "gotcha"
        ]
    });

    let id = json!({
        "type": "string",
        "description": "Memory UUID"
    });

    let rev = json!({
        "type": "integer",
        "minimum": 1,
        "description": "Revision you last saw; a mismatch returns a conflict error"
    });

    let mut list = vec![
        (
            "memory_save",
            schema(
                "Save a long-term memory for the current project.",
                json!({"kind": kind, "title": {"type": "string"}, "content": {"type": "string"}}),
                &["kind", "title", "content"],
            ),
        ),
        (
            "memory_search",
            schema(
                "Full-text search memories of the current project (active only by default).",
                json!({
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "default": 10},
                    "include_global": {"type": "boolean", "default": false},
                    "include_inactive": {"type": "boolean", "default": false}
                }),
                &["query"],
            ),
        ),
        (
            "memory_get",
            schema("Fetch one memory by id.", json!({"id": id}), &["id"]),
        ),
        (
            "memory_update",
            schema(
                "Update a memory; omitted fields are unchanged.",
                json!({
                    "id": id,
                    "expected_revision": rev,
                    "kind": kind,
                    "title": {"type": "string"},
                    "content": {"type": "string"}
                }),
                &["id", "expected_revision"],
            ),
        ),
        (
            "memory_archive",
            schema(
                "Archive a memory so it no longer appears in default searches.",
                json!({"id": id, "expected_revision": rev}),
                &["id", "expected_revision"],
            ),
        ),
    ];
    let tools: Vec<Value> = list
        .drain(..)
        .map(|(name, mut t)| {
            t["name"] = name.into();
            t
        })
        .collect();
    json!({ "tools": tools })
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T> {
    serde_json::from_value(args).map_err(|e| Error::InvalidArgument(e.to_string()))
}

fn to_json(v: &impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| Error::Internal(e.to_string()))
}

/// `None` means the tool name is unknown (a protocol error, not a tool error).
fn call_tool<S: MemoryService>(
    svc: &S,
    project: ProjectId,
    name: &str,
    args: Value,
) -> Option<Result<Value>> {
    let p = Some(project);
    Some(match name {
        "memory_save" => parse::<Save>(args).and_then(|a| {
            to_json(&svc.save(NewMemory {
                project_id: p,
                kind: a.kind,
                title: a.title,
                content: a.content,
                source: Source::Agent,
            })?)
        }),
        "memory_search" => parse::<Search>(args).and_then(|a| {
            to_json(&svc.search(
                p,
                SearchQuery {
                    query: a.query,
                    limit: a.limit.unwrap_or(10),
                    include_global: a.include_global,
                    include_inactive: a.include_inactive,
                },
            )?)
        }),
        "memory_get" => parse::<Get>(args).and_then(|a| to_json(&svc.get(p, a.id)?)),
        "memory_update" => parse::<Update>(args).and_then(|a| {
            to_json(&svc.update(
                p,
                a.id,
                a.expected_revision,
                MemoryChanges {
                    kind: a.kind,
                    title: a.title,
                    content: a.content,
                },
            )?)
        }),
        "memory_archive" => parse::<Archive>(args)
            .and_then(|a| to_json(&svc.archive(p, a.id, a.expected_revision)?)),
        _ => return None,
    })
}

fn tool_result(r: Result<Value>) -> Value {
    let (is_error, v) = match r {
        Ok(v) => (false, v),
        Err(e) => (true, json!({"code": e.code(), "message": e.to_string()})),
    };
    json!({
        "isError": is_error,
        "content": [{"type": "text", "text": v.to_string()}]
    })
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

/// Returns the response, or `None` for notifications.
fn handle<S: MemoryService>(svc: &S, project: ProjectId, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone(); // no id => notification, never answered
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match msg.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = SUPPORTED.iter().find(|v| **v == asked).unwrap_or(&LATEST);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "chloe", "version": env!("CARGO_PKG_VERSION")}
            })
        }
        Some("ping") => json!({}),
        Some("tools/list") => tools(),
        Some("tools/call") => {
            let name = params["name"].as_str().unwrap_or_default();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(svc, project, name, args) {
                Some(r) => tool_result(r),
                None => return Some(rpc_error(id, -32602, &format!("unknown tool: {name}"))),
            }
        }
        _ => return Some(rpc_error(id, -32601, "method not found")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Serves requests until `input` closes. Only protocol messages are written to `out`.
pub fn serve<S: MemoryService>(
    svc: &S,
    project: ProjectId,
    input: impl BufRead,
    mut out: impl Write,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(svc, project, &msg),
            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

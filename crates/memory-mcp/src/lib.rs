//! Minimal MCP server over stdio (newline-delimited JSON-RPC 2.0), exposing the memory tools.
//! All business logic stays in `MemoryService`; this crate only maps JSON to service calls.
//! Scope is fixed at startup: every call runs as the given project.

use std::io::{self, BufRead, Write};

use memory_core::embed::{Embedder, EmbeddingStore};
use memory_core::service::{MemoryChanges, MemoryService, NewMemory, SearchQuery};
use memory_core::{Error, Kind, MemoryId, ProjectId, Result, Source};
use memory_engine::SearchMode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const INSTRUCTIONS: &str = "chloe is long-term project memory shared by coding agents. \
Before reading code or answering any question about this project, call memory_search first, \
with a few keywords from the task (words missing from a memory do not exclude it). \
When a durable decision, constraint, gotcha or procedure is made or learned, save it with memory_save \
right away, without waiting to be asked (kind: decision, constraint, gotcha, procedure or fact); \
never save transient task state or secrets. Use scope=global only for knowledge that applies to every project. \
To change a memory, memory_get it, then memory_update or memory_archive with its current expected_revision; \
on a conflict error, re-read and retry. Search returns active memories of the current project only \
(set include_global to add global ones).";
/// Search results carry at most this many characters of each body; `memory_get` returns it all.
const SNIPPET_CHARS: usize = 1000;
/// Total body characters one search returns, so a broad query cannot flood the agent's context.
const DEFAULT_MAX_CHARS: usize = 8000;
const MAX_CHARS: usize = 50_000;
const MAX_LIMIT: usize = 50;
const LATEST: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum Scope {
    #[default]
    Project,
    Global,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    #[serde(default)]
    scope: Scope,
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
    max_chars: Option<usize>,
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
                json!({
                    "kind": kind, "title": {"type": "string"}, "content": {"type": "string"},
                    "scope": {"type": "string", "enum": ["project", "global"], "default": "project",
                        "description": "Use global only for knowledge that applies to every project"}
                }),
                &["kind", "title", "content"],
            ),
        ),
        (
            "memory_search",
            schema(
                "Search memories of the current project (active only by default). The result says how the search ran (mode: the paths used, fts, vector or hybrid; requested_mode: what the server is set to; degraded: why it fell back to fts, else null) and how many candidates each path returned. Each hit says which retrieval paths found it (matched_by: fts, vector) and its rank in each. Bodies are cut to fit max_chars in total; memory_get returns one in full.",
                json!({
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": 10},
                    "include_global": {"type": "boolean", "default": false},
                    "include_inactive": {"type": "boolean", "default": false},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": MAX_CHARS, "default": DEFAULT_MAX_CHARS,
                        "description": "Total characters of memory bodies across all hits"}
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
fn call_tool<S: MemoryService + EmbeddingStore>(
    svc: &S,
    mode: SearchMode,
    embedder: Option<&dyn Embedder>,
    project: ProjectId,
    name: &str,
    args: Value,
) -> Option<Result<Value>> {
    let p = Some(project);
    Some(match name {
        "memory_save" => parse::<Save>(args).and_then(|a| {
            to_json(&svc.save(NewMemory {
                project_id: match a.scope {
                    Scope::Project => p,
                    Scope::Global => None,
                },
                kind: a.kind,
                title: a.title,
                content: a.content,
                source: Source::Agent,
            })?)
        }),
        "memory_search" => parse::<Search>(args).and_then(|a| {
            let found = memory_engine::search(
                svc,
                mode,
                embedder,
                p,
                SearchQuery {
                    query: a.query,
                    limit: a.limit.unwrap_or(10).min(MAX_LIMIT),
                    include_global: a.include_global,
                    include_inactive: a.include_inactive,
                },
            )?;
            let mut budget = a.max_chars.unwrap_or(DEFAULT_MAX_CHARS).min(MAX_CHARS);
            let (fts_returned, vector_returned) = (found.fts_returned, found.vector_returned);
            let hits = found
                .hits
                .into_iter()
                .map(|mut h| {
                    let keep = SNIPPET_CHARS.min(budget);
                    let cut = h.hit.memory.content.chars().count() > keep;
                    if cut {
                        h.hit.memory.content = h.hit.memory.content.chars().take(keep).collect();
                    }
                    budget -= h.hit.memory.content.chars().count();
                    let mut v = to_json(&h)?;
                    v["content_truncated"] = cut.into();
                    Ok(v)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "mode": found.mode,
                "requested_mode": mode,
                "degraded": found.degraded,
                "candidates": {"fts": fts_returned, "vector": vector_returned},
                "hits": hits,
            }))
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
fn handle<S: MemoryService + EmbeddingStore>(
    svc: &S,
    mode: SearchMode,
    embedder: Option<&dyn Embedder>,
    project: ProjectId,
    msg: &Value,
) -> Option<Value> {
    let id = msg.get("id")?.clone(); // no id => notification, never answered
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match msg.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = SUPPORTED.iter().find(|v| **v == asked).unwrap_or(&LATEST);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "instructions": INSTRUCTIONS,
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
            match call_tool(svc, mode, embedder, project, name, args) {
                Some(r) => tool_result(r),
                None => return Some(rpc_error(id, -32602, &format!("unknown tool: {name}"))),
            }
        }
        _ => return Some(rpc_error(id, -32601, "method not found")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Full-text search only.
pub fn serve<S: MemoryService + EmbeddingStore>(
    svc: &S,
    project: ProjectId,
    input: impl BufRead,
    out: impl Write,
) -> io::Result<()> {
    serve_with(svc, SearchMode::Fts, None, project, input, out)
}

/// Serves requests until `input` closes. Only protocol messages are written to `out`. With an
/// `mode`, `memory_search` uses full-text, vectors or both (the latter two need an `embedder`),
/// and falls back to full-text when the vector path fails.
pub fn serve_with<S: MemoryService + EmbeddingStore>(
    svc: &S,
    mode: SearchMode,
    embedder: Option<&dyn Embedder>,
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
            Ok(msg) => handle(svc, mode, embedder, project, &msg),
            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

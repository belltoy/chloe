# chloe

Local-first long-term memory for coding agents (Claude Code, Codex, ...). Agents save, search, update and archive project knowledge through MCP; data lives in a local SQLite file.

> **Status: Milestone A complete; Milestone B in progress.** Manual memory over MCP and CLI works. Built from Milestone B: hook capture, extraction into reviewable candidates with a local model, an embedding index, hybrid retrieval and an evaluation harness. Not done: the real end-to-end acceptance run (Claude capture, extract, review, Codex retrieval) and the context pack (B3-06). See `docs/implementation-plan.md`.

## Quickstart (about 5 minutes)

```sh
cargo install --path crates/memory-cli   # installs the `chloe` binary
cd your-project
chloe init                               # registers this project in the local database
chloe doctor                             # database + a real MCP round trip

claude mcp add chloe -- chloe mcp serve  # Claude Code
codex mcp add chloe -- chloe mcp serve   # Codex (add default_tools_approval_mode = "approve" for `codex exec`)
```

**Required:** paste the snippet from [`docs/agents/memory-instructions.md`](docs/agents/memory-instructions.md) into your `CLAUDE.md` / `AGENTS.md`. Without it, Claude Code and Codex did not search or save on their own in our control run, even though the server sends the same text as MCP `instructions`. Both agents share one database, so what one saves the other can search. Details, verified versions and a runnable two-way demo: [`docs/mcp-clients.md`](docs/mcp-clients.md).

Optional, all local (needs [Ollama](https://ollama.com) on this machine; nothing is sent elsewhere unless you pass `--allow-remote`):

```sh
chloe hook install claude                # capture sessions (also: codex); events are redacted and queued
chloe consolidate                        # extract memory candidates from captured sessions
chloe candidates list                    # review them; nothing becomes a memory without you
chloe embeddings backfill                # embed memories (default model qwen3-embedding:0.6b)
chloe mcp serve --mode hybrid            # full-text + vector search (fts is the default; vector also exists)
chloe eval                               # score FTS / Vector / Hybrid on a labelled set
```

Manage memories yourself with the CLI (`--json` on everything): `chloe memory add|search|list|show|update|archive|history`.

## How it works

```text
Claude Code ── MCP stdio ──┐
Codex       ── MCP stdio ──┼── memory-mcp ──┐
CLI ───────────────────────┘                ├── MemoryService ── memory-store (SQLite + FTS5)
                                            └── memory-core (types, errors, config)
```

- One database file shared by every `chloe` process; each MCP server is scoped to the project of its working directory (a Git worktree resolves to its main checkout).
- Updates use optimistic concurrency (`expected_revision`); every change appends to an audit trail (`chloe memory history`).
- Search defaults to SQLite FTS5 over title and body (Chinese is cut into words with jieba), active memories of the current project only. `--mode vector|hybrid` adds embeddings; when the model server is down the search falls back to full-text and says why (`degraded`). Global memories are explicit (`scope: global`, `include_global`).
- Architecture and evaluation: [`docs/architecture.md`](docs/architecture.md), `docs/evidence/milestone-b/eval/`.

## Privacy

- Everything is local: no network listeners and no remote services. The optional model features talk only to a local Ollama unless you opt in to a remote one. stdout carries only command output and MCP messages; logs go to stderr and never contain memory bodies.
- The database directory is created `0700` and the file `0600` (Unix).
- Memories are plain text in SQLite: do not save secrets. Memory text retrieved by an agent is untrusted data, not instructions.

## Design trade-offs and known limitations

- SQLite shared by several processes (WAL + 5 s busy timeout) instead of a daemon: simple and robust, but a write blocked longer than the timeout returns a storage error.
- Chinese keyword search uses jieba's default dictionary (about 3 MB in the binary, ~70 ms to load on first use). Japanese and Korean are not segmented.
- Hybrid trails Vector on cross-language queries (see `docs/evidence/milestone-b/eval/fusion-gate.md`); the held-out set is AI-written and has not been reviewed by a person.
- Hooks write to `$CHLOE_DB` or the default database; `--db-path` given at `hook install` is not recorded.
- Extraction keeps only the newest events that fit its window; a very long burst of new events is partly skipped.
- A moved or renamed project directory becomes a new project; there is no alias-linking command yet.
- Search snippets are cut to 1000 characters (`memory_get` returns the full body); search returns at most 50 hits.
- Global memories can be edited from any project.
- Verified with Claude Code 2.1.309 and codex-cli 0.164.0 on macOS only; Linux is covered by CI, not by real agents.

## Build and run

```sh
cargo build --workspace
cargo run -p memory-cli -- --help
cargo run -p memory-cli -- config            # print resolved configuration
cargo run -p memory-cli -- --json config
```

The binary is named `chloe`.

## Configuration

| Setting | Flag | Env | Default |
|---|---|---|---|
| Database path | `--db-path` | `CHLOE_DB` | `memory.db` in `$XDG_DATA_HOME/chloe`, falling back to `~/.local/share/chloe` (macOS and Linux); the platform data dir on Windows |
| Log level | `--log-level` | `CHLOE_LOG` | `info` |

Logs go to stderr; stdout is reserved for command output and MCP stdio. Tests always pass an explicit temp path and never touch your real database.

## Layout

```text
crates/
  memory-core/   domain types, errors, config, service trait (no SQL, no agent code)
  memory-store/  SQLite store, scope resolver, FTS5 search
  memory-mcp/    MCP stdio server
  memory-cli/    `chloe` binary
```

Milestone B adds more crates.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks on Linux and macOS.

## Docs

Design, plan and acceptance criteria (English; Chinese originals are the `-cn.md` files): `docs/engineering-design.md`, `docs/implementation-plan.md`, `docs/acceptance-criteria.md`.

## License

[Apache-2.0](LICENSE)

Copyright 2026 belltoy. Licensed under the Apache License, Version 2.0.

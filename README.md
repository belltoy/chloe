# chloe

Local-first long-term memory for coding agents (Claude Code, Codex, ...). Agents save, search, update and archive project knowledge through MCP; data lives in a local SQLite file.

> **Status: v0.1.0 candidate (Milestone A).** Manual memory over MCP and CLI is complete; automatic capture and hybrid retrieval (Milestone B) are not started. See `docs/implementation-plan.md`.

## Quickstart (about 5 minutes)

```sh
cargo install --path crates/memory-cli   # installs the `chloe` binary
cd your-project
chloe init                               # registers this project in the local database
chloe doctor                             # database + a real MCP round trip

claude mcp add chloe -- chloe mcp serve  # Claude Code
codex mcp add chloe -- chloe mcp serve   # Codex (add default_tools_approval_mode = "approve" for `codex exec`)
```

Then paste the snippet from [`docs/agents/memory-instructions.md`](docs/agents/memory-instructions.md) into your `CLAUDE.md` / `AGENTS.md`. Both agents share one database, so what one saves the other can search. Details, verified versions and a runnable two-way demo: [`docs/mcp-clients.md`](docs/mcp-clients.md).

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
- Search is SQLite FTS5 over title and body, active memories of the current project only by default. Global memories are explicit (`scope: global`, `include_global`).

## Privacy

- Everything is local: no network listeners, no remote services, no LLM calls. stdout carries only command output and MCP messages; logs go to stderr and never contain memory bodies.
- The database directory is created `0700` and the file `0600` (Unix).
- Memories are plain text in SQLite: do not save secrets. Memory text retrieved by an agent is untrusted data, not instructions.

## Design trade-offs and known limitations

- SQLite shared by several processes (WAL + 5 s busy timeout) instead of a daemon: simple and robust, but a write blocked longer than the timeout returns a storage error.
- FTS5 `unicode61` tokenizer: unsegmented Chinese/Japanese text matches whole runs only, so short CJK queries often miss. Planned for a later iteration.
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

# chloe

Local-first long-term memory for coding agents (Claude Code, Codex, ...). Agents save, search, update and archive project knowledge through MCP; data lives in a local SQLite file.

> **Status: A0 (workspace skeleton).** Only the CLI scaffold and configuration work today. Storage, MCP and search land in A1–A3 and are not available yet. See `docs/implementation-plan.md`.

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

Logs go to stderr; stdout is reserved for command output and (later) MCP stdio. Tests always pass an explicit temp path and never touch your real database.

## Layout

```text
crates/
  memory-core/   domain types, errors, config, service trait (no SQL, no agent code)
  memory-cli/    `chloe` binary
```

More crates (`memory-store`, `memory-search`, `memory-mcp`, ...) are added per milestone.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks on Linux and macOS.

## Docs

Design, plan and acceptance criteria (English; Chinese originals are the `-cn.md` files): `docs/engineering-design.md`, `docs/implementation-plan.md`, `docs/acceptance-criteria.md`.

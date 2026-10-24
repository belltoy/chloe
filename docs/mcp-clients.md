# Connecting agents to chloe (MCP)

`chloe mcp serve` is a stdio MCP server. Memories are scoped to the project of its working directory (or `--dir`), so agents must launch it from the project root — both clients below do this by default. Point every agent at the same database (`--db-path` / `CHLOE_DB`, default: platform data dir) to share memories.

Tools: `memory_save`, `memory_search`, `memory_get`, `memory_update`, `memory_archive`.

Recommended: add `--mode hybrid` to every `chloe mcp serve` below (also in the TOML `args`). It needs a local Ollama embedding model and `chloe embeddings backfill`; if Ollama is down the search falls back to full-text and reports `degraded`. The default `--mode fts` has no stemming ("page" misses "pages") and no cross-language matching.

## Claude Code

```sh
claude mcp add chloe -- chloe mcp serve --mode hybrid  # local scope, this project
claude mcp add -s project chloe -- chloe mcp serve --mode hybrid  # writes .mcp.json (shareable)
```

Tool calls prompt for approval; allow them with the permission rule `mcp__chloe` (or `--allowedTools "mcp__chloe__memory_save,mcp__chloe__memory_search"`).

## Codex

```sh
codex mcp add chloe -- chloe mcp serve --mode hybrid
```

or in `~/.codex/config.toml`:

```toml
[mcp_servers.chloe]
command = "chloe"
args = ["mcp", "serve", "--mode", "hybrid"]
default_tools_approval_mode = "approve"   # without this, `codex exec` (approval policy never) rejects every call
```

## Verified

Verified 2026-10-23 on macOS with Claude Code 2.1.309 and codex-cli 0.164.0, both against one temp database and project directory:

1. Claude Code (`claude -p --mcp-config <file> --strict-mcp-config`) called `memory_save` then `memory_search` (1 hit).
2. Codex (`codex exec -c mcp_servers.chloe.*`, same keys as the TOML above) called `memory_search`, found that memory, and `memory_update` took it from revision 1 to 2; `chloe memory show` confirmed the change.

Not verified: the persistent `claude mcp add` / `codex mcp add` commands themselves (the runs above used the equivalent per-invocation config, to avoid touching the real client configs), and the interactive UIs. No screenshots or recordings were captured.

## Cross-agent demo

`examples/cross-agent/run.sh` copies `examples/cross-agent/repo` to a temp dir with a temp database, runs both directions (Claude Code saves a memory with a random marker and Codex retrieves it; then Codex saves one and Claude Code retrieves it), and prints PASS/FAIL. Run with `CHLOE=target/debug/chloe examples/cross-agent/run.sh`. Passed on 2026-10-23 with the versions above.

Agent instructions to paste into `CLAUDE.md` / `AGENTS.md` (required: in a control run without them, Claude Code and Codex neither searched nor saved, although the server sends the same text as MCP `instructions`): [agents/memory-instructions.md](./agents/memory-instructions.md). `chloe doctor` checks the database and runs a real `mcp serve` round trip on a throwaway database.

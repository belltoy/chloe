# Connecting agents to chloe (MCP)

`chloe mcp serve` is a stdio MCP server. Memories are scoped to the project of its working directory (or `--dir`), so agents must launch it from the project root — both clients below do this by default. Point every agent at the same database (`--db-path` / `CHLOE_DB`, default: platform data dir) to share memories.

Tools: `memory_save`, `memory_search`, `memory_get`, `memory_update`, `memory_archive`.

## Claude Code

```sh
claude mcp add chloe -- chloe mcp serve                 # local scope, this project
claude mcp add -s project chloe -- chloe mcp serve      # writes .mcp.json (shareable)
```

Tool calls prompt for approval; allow them with the permission rule `mcp__chloe` (or `--allowedTools "mcp__chloe__memory_save,mcp__chloe__memory_search"`).

## Codex

```sh
codex mcp add chloe -- chloe mcp serve
```

or in `~/.codex/config.toml`:

```toml
[mcp_servers.chloe]
command = "chloe"
args = ["mcp", "serve"]
default_tools_approval_mode = "approve"   # without this, `codex exec` (approval policy never) rejects every call
```

## Verified

Verified 2026-10-23 on macOS with Claude Code 2.1.309 and codex-cli 0.164.0, both against one temp database and project directory:

1. Claude Code (`claude -p --mcp-config <file> --strict-mcp-config`) called `memory_save` then `memory_search` (1 hit).
2. Codex (`codex exec -c mcp_servers.chloe.*`, same keys as the TOML above) called `memory_search`, found that memory, and `memory_update` took it from revision 1 to 2; `chloe memory show` confirmed the change.

Not verified: the persistent `claude mcp add` / `codex mcp add` commands themselves (the runs above used the equivalent per-invocation config, to avoid touching the real client configs), and the interactive UIs. No screenshots or recordings were captured.

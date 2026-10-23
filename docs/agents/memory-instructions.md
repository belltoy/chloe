# Agent instructions for chloe

The same text is sent by `chloe mcp serve` as the MCP `instructions`. Some clients show server instructions poorly, so paste it into your project's `CLAUDE.md` (Claude Code) or `AGENTS.md` (Codex):

```markdown
## Project memory (chloe)

- Before starting work, call `memory_search` with keywords for the task.
- Save durable knowledge with `memory_save` (kind: decision, constraint, gotcha, procedure or fact). Never save transient task state or secrets.
- Use `scope: global` only for knowledge that applies to every project.
- To change a memory: `memory_get` it, then `memory_update` / `memory_archive` with its current `expected_revision`. On a conflict error, re-read and retry.
- Search returns active memories of the current project only; set `include_global` to add global ones.
```

Connection configuration for Claude Code and Codex: see [../mcp-clients.md](../mcp-clients.md).

# Acceptance Run — Milestone A

Date: 2026-10-23 · Version: 0.1.0 candidate, based on `3b97b30` plus the A3 changes in the commit that added this file
OS / CPU: macOS (Darwin 27.0.0, arm64) · Claude Code 2.1.309 · codex-cli 0.164.0 · SQLite CLI 3.54.0 · rustc 1.98.1

Evidence: [`acceptance-a.log`](acceptance-a.log) (`scripts/acceptance-a.sh`), [`cross-agent.log`](cross-agent.log) (`examples/cross-agent/run.sh`).

| Check | Priority | Status | Evidence | Note |
|-------|----------|--------|----------|------|
| A-01 | P0 | PASS (local) | acceptance-a.log | fmt, clippy, tests pass on macOS. Linux/CI was **not** run for this version |
| A-02 – A-07 | P0 | PASS | acceptance-a.log | |
| A-08 | P0 | PASS | cross-agent.log | Both directions with real Claude Code and Codex |
| A-09, A-10 | P0 | PASS | acceptance-a.log | |
| A-11 | P0 | PASS | acceptance-a.log | `SQLITE_BUSY`, rollback, 12 parallel writer processes, SIGKILL of a server after a committed write. No test kills a process in the middle of a transaction |
| A-12 | P0 | PASS | acceptance-a.log | db `0600`, dir `0700`, no sockets on an `mcp serve` process, no memory body in logs at `trace` level (checked on CLI `add`) |
| A-13 | P1 | PASS | acceptance-a.log | |
| A-14 | P1 | PASS | acceptance-a.log | |
| A-15 | P1 | NOT RUN | | README quickstart not executed from a clean environment (it installs into the user's cargo bin) |
| A-16 | P0 | PASS | acceptance-a.log | macOS `sandbox-exec` with network denied |
| A-17, A-18 | P0 | PASS | acceptance-a.log | |
| A-19 | P1 | PASS | acceptance-a.log | Read-only database and unusable path. The criterion originally also listed a "non-startable MCP binary"; `doctor` launches its own executable, so that case cannot occur and was removed from the criterion |
| A-20 | P1 | PARTIAL | acceptance-a.log | `instructions` content verified. That a real agent follows the `CLAUDE.md` snippet was **not** tested |

Blockers: none known. Open before tagging `v0.1.0`: run CI on Linux (push), run A-15, optionally A-20 with real agents.

Known limitations: see README ("Design trade-offs and known limitations"), notably unsegmented CJK text in FTS.

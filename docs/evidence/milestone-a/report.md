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
| A-15 | P1 | PASS (manual) | | 2026-10-23: followed the README Quickstart by hand; Claude Code saved a memory, `chloe memory list` showed it, Codex found it by search. No log captured, and the machine was the author's own, not a verified-clean environment |
| A-16 | P0 | PASS | acceptance-a.log | macOS `sandbox-exec` with network denied |
| A-17, A-18 | P0 | PASS | acceptance-a.log | |
| A-19 | P1 | PASS | acceptance-a.log | Read-only database and unusable path. The criterion originally also listed a "non-startable MCP binary"; `doctor` launches its own executable, so that case cannot occur and was removed from the criterion |
| A-20 | P1 | PASS (manual) | acceptance-a.log | `instructions` content verified. 2026-10-23, one run per agent in a Vue project, snippet in `CLAUDE.md`/`AGENTS.md`: Codex 0.164.0 called `memory_search` before touching code and answered from the memory; given a decision in plain words it searched, then called `memory_save` (a correct Decision, listed by `chloe memory list`), then went on to edit the repo. Claude Code 2.1.309 searched and answered from the memory, but the order against its file search and a save were not captured. Single runs, no snippet-less control, screenshots not stored. A first Codex run exposed a search bug (AND over query words), fixed in `10bf017`; the snippet was reworded to say search first and save without being asked |

Blockers: none known. Open before tagging `v0.1.0`: run CI on Linux (push), optionally re-run A-15 in a clean environment with a captured log, optionally repeat A-20 (several runs per agent, a run without the snippet, Claude Code's save).

Known limitations: see README ("Design trade-offs and known limitations"), notably unsegmented CJK text in FTS.

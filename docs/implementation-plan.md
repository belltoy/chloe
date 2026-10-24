# Chloe — Local-first Coding Agent Memory — Implementation Plan

> Document ID: AM-PLAN-001 · Version: v0.1 · Date: 2026-10-23 · Depends on: AM-DESIGN-001
>
> Goal: Milestone A becomes a daily-usable local MCP memory; Milestone B demonstrates the automatic memory loop and evaluation results.

## 1. Scope and Overall Schedule

Assumes a single developer fluent in Rust working part-time. Roughly 6–8 iterations are suggested (about 4–8 weeks is a planning estimate, not a guarantee): finish a usable A first, then develop B. Progress is judged by acceptance criteria, not calendar dates.

| Phase | Goal | Must deliver |
|---|---|---|
| A0 | Project skeleton | Workspace, CI, error and config conventions |
| A1 | Persistent domain model | SQLite schema, migrations, transactions, versions and scope |
| A2 | Agent interface | MCP stdio, CLI, Claude Code/Codex configuration |
| A3 | Usability and stability | FTS5, isolation/concurrency/persistence E2E, docs |
| B0 | Capture | Claude hook + Codex adapter, events and idempotency |
| B1 | Extraction | Background queue, LLM provider, candidate knowledge |
| B2 | Consolidation | Dedup, review, provenance and revisions |
| B3 | Search & Evaluation | Embedding/Hybrid, fixed evaluation set and report |

## 2. Task Dependencies

```text
A0 → A1 → A2 → A3 → A acceptance
                     ↓
                    B0 → B1 → B2 → B3 → B acceptance
                             \___________/
                              evaluation fixtures can be built in parallel
```

Continuous items across all phases: automated tests, documentation, a minimal demonstrable example, version-compatibility notes, security review.

**Relation to the milestone-adjustments proposal** ([milestone-adjustments-cn](./milestone-adjustments-cn.md)): its A1–A8 / B1–B9 are feature IDs, not schedule phases. They map onto the A0–A3 / B0–B3 phases as follows; do not mix the two numberings.

| Proposal item | Where it lands |
|---|---|
| A1–A4 (MCP/SQLite/FTS5, CRUD + scope, agent integration, CLI + reliability) | A1-01 – A1-05, A2-01 – A2-04, A3-02 |
| A5 typed memory and status | A0-02, A1-03 (validated kind/status enums) |
| A6 append-only audit events | A1-06 |
| A7 agent guidance and configuration templates | A2-03, A2-06 |
| A8 `chloe doctor` | A1-04 (database), A2-07 (config, MCP, read/write/search) |
| Explicit global scope (A2) | A2-05 |
| B1–B5 (capture, extraction, dedup, hybrid retrieval, benchmark) | B0, B1, B2, B3 |
| B6 evidence / provenance | B1-03, B2-01 |
| B7 conflict + supersession | B2-01, B2-03, B2-06 |
| B8 explain / diagnostic | B3-03 |
| B9 Context Pack | B3-06 |

## 3. Milestone A: Implementation Tasks

### A0 Project Initialization

- **A0-01** Create a Rust Cargo workspace with unified fmt/clippy/test, dependency versions and lints.
- **A0-02** In `memory-core`, build IDs, the Memory type, error classification, traits and serializable interfaces; leave extension points for B but do not abstract all features prematurely.
- **A0-03** Configure the data directory (XDG/platform-standard), log level and optional `--db-path`; ensure test databases are never mixed with the user's database.
- **A0-04** GitHub Actions: Linux/macOS build, `cargo fmt --check`, Clippy, unit tests; dependency license checking is a bonus.

**Exit condition**: a clean checkout builds/tests, and `chloe --help` and configuration output run.

### A1 SQLite and Memory Business Logic

- **A1-01** Migration design: projects, aliases, memories, memory_revisions and necessary indexes; SQLite WAL, busy_timeout, foreign-key constraints.
- **A1-02** Scope resolver: map Git root / worktree / plain directory to a stable ID; record path-move and remote-alias policy in the docs.
- **A1-03** Core service implements create/get/update/archive/list; updates use revision CAS; history retained.
- **A1-04** FTS5 external-content + triggers, delete/update sync; `reindex`/`doctor` can detect and repair the index.
- **A1-05** Integration tests: persistence across restart, transaction rollback, concurrent CAS, cross-project filtering.
- **A1-06** Append-only audit trail: every create/update/archive records the operation (`cause`), the actor (`cli` / `mcp`) and the revision in `memory_revisions`; triggers reject UPDATE/DELETE on it; `chloe memory history <id>` lists it in order. No full event sourcing.

**Exit condition**: CRUD and FTS queries work through the service API and all key DB tests pass.

### A2 MCP and CLI

- **A2-01** MCP stdio tools: save/search/get/update/archive; declare JSON Schemas and clear error classification.
- **A2-02** CLI: init/project show/memory add|search|show|update|archive/doctor/mcp serve, supporting `--json`.
- **A2-03** Write and actually verify MCP connection configuration for Claude Code / Codex; record the versions used and screenshots/recordings (do not rely on guesses in the README).
- **A2-04** Provide an example repo and a cross-agent script that runs both directions: agent A saves and agent B retrieves, then B saves and A retrieves, demonstrating a shared database.
- **A2-05** Explicit global scope: `memory_save` accepts `scope: project|global` (default `project`) and `chloe memory add` has `--global`; global memories appear in search only with `include_global`; cross-project reads stay `NotFound`.
- **A2-06** Agent guidance: the MCP `initialize` response carries `instructions` (when to search, when to save, how to handle conflicts); ship recommended `CLAUDE.md` / `AGENTS.md` snippets and configuration templates under `docs/agents/`. No skill-management system.
- **A2-07** Extend `chloe doctor`: database writable and migrated, FTS consistent, configuration valid, `mcp serve` starts and completes initialize/tools-list, and a save → search → get → archive round trip succeeds against a throwaway database (never the user's data). Real agent calls remain a separate acceptance.

**Exit condition**: both Claude Code and Codex successfully call the MCP interface on the same DB, and each can read what the other wrote.

### A3 Stability, Docs and Release

- **A3-01** Check scope, status, parameters, concurrency and sensitive-information logging; limit tool result content.
- **A3-02** Multi-instance concurrency integration tests; confirm `SQLITE_BUSY` handling, transaction boundaries and error feedback.
- **A3-03** README quickstart (goal: doable in 5 minutes), architecture sketch, technical trade-offs, privacy notes and known limitations.
- **A3-04** Basic CI release job (at least build macOS/Linux artifacts); tag an example version `v0.1.0`.
- **A3-05** Run the A acceptance of AM-ACCEPT-001, save terminal output and logs, fix blockers.

**A Exit Gate**: all mandatory A acceptance items pass; "code implemented" must not substitute for MCP verification with real clients.

## 4. Milestone B: Implementation Tasks

### B0 Automatic Agent Capture

- **B0-01** Define `AgentEvent`, SessionID, native event mapping and event schema version; build a per-agent support matrix.
- **B0-02** Claude Code hook adapter: install/uninstall, event parsing, limits/redaction, fast exit.
- **B0-03** Codex adapter: choose a supported path (notification, hook or transcript reader) for the actual version; stay read-only and tolerate field changes.
- **B0-04** Events table and idempotency key; fixture tests for input failure, duplicates, unknown versions and missing SessionEnd.
- **B0-05** Persistent hook-event queue and crash recovery; choose daemon or local spool, with explicit reliability boundaries.

**Exit condition**: capturing the same session repeatedly yields no duplicate events; the agent's main flow is not blocked by a brief LLM or store outage.

### B1 Extraction Pipeline

- **B1-01** Job queue: lease, attempts, exponential backoff, dead-letter, CLI inspect/retry.
- **B1-02** `MemoryExtractor` provider trait; at least one demonstrable local model configuration (e.g. Ollama); external APIs are opt-in only.
- **B1-03** Prompt & schema: restrict candidate types, title length and evidence references (session ID, event IDs, and a file path or commit when available), forbid leaking secrets; do not treat model output as authoritative fact.
- **B1-04** SessionEnd / idle / manual consolidate trigger extraction with a controllable input window; when the model is unavailable, keep events and a clear status.
- **B1-05** Fixtures: a valid technical decision, ordinary useless operations, failure-and-fix, missing evidence, an over-long session.

**Exit condition**: a representative session produces reviewable candidates; failures are retryable, and confirmed enqueued jobs survive a process restart.

### B2 Dedup, Merge and Review

- **B2-01** Candidate states: pending/accepted/rejected/merged/conflict; linked to memory provenance.
- **B2-02** Use FTS first to find duplicate candidates; a model or rules decide create/merge/conflict; record explainable reasons.
- **B2-03** Simple safety policy: ordinary low-risk duplicates may auto-merge; user-specified constraints and conflicts with existing decisions need human approval.
- **B2-04** CLI review list/show/accept/reject/merge, with traceable version history.
- **B2-05** Idempotency tests: repeated consolidate, abnormal interruption, two workers leasing the same candidate.
- **B2-06** Supersession: a `supersedes` link between memories and the `superseded` status; model output may only propose it, and an explicit user Constraint/Decision is never superseded without human approval.

**Exit condition**: duplicate candidates do not multiply without bound, and contradictory candidates never silently overwrite existing confirmed memories.

### B3 Hybrid Retrieval and Evaluation

- **B3-01** Embedding provider/index: persist local model ID and dimension, backfill jobs for missing vectors, rebuild on model switch; the candidate model must cover both English and Chinese.
- **B3-02** Vector top-N and FTS top-N, RRF fusion ranking, hard filtering by status/scope, degradable on anything but FTS failure.
- **B3-03** Explainable results (matched_by, rank, source, status), with token/character limits on what is returned to the agent; a diagnostic mode (`--explain`) shows the FTS/Vector paths, scope, status filter and main ranking factors.
- **B3-04** Fixed evaluation dataset (suggested 50–100 memories, 20–30 queries), human-labeled relevant memory IDs, run Recall@5, MRR, p50/p95 for FTS/Vector/Hybrid, reported per language (English, Chinese, mixed, cross-language queries); record model/hardware/configuration.
- **B3-05** Add fixture tests to CI, run benchmarks in a controlled environment; publish architecture and demo video with known limitations.
- **B3-06** Context Pack: a size-bounded result grouped by kind (decisions / constraints / gotchas / procedures) with memory IDs and sources and a way to expand one memory on demand (`memory_get`); truncation is by a configurable token/character budget.
- **B3-07** Chinese/CJK keyword search, deliberately deferred from A (A keeps the `unicode61` FTS tokenizer; unsegmented CJK text only matches whole runs). Choose and measure an approach: `trigram` tokenizer with `LIKE` for short tokens, or a `LIKE` fallback when FTS finds nothing. Needs a migration that rebuilds `memory_fts`; existing memories are re-indexed from `memories`, so nothing saved in A is lost.
  - **As built:** the trigram attempt (migration 010) was measured and replaced by word segmentation with [jieba-rs](https://crates.io/crates/jieba-rs) (migration 011: `memory_fts_seg` and `fts_meta`; `crates/memory-store/src/segment.rs`). The index side stores `cut_for_search` words; the query side uses `cut` minus a small stop-word list, so negations are kept. The segmented text is written in the same transaction as the memory, and the index is rebuilt when `SEGMENT_VERSION` changes (also by `chloe doctor --reindex`; `doctor` compares its content with the memories). Cost: about 3 MB in the binary and ~70 ms to load the dictionary on first CJK use. Result: Chinese full-text Recall@5 0.00 → 1.00 on the development set; Japanese and Korean are not segmented. See `docs/evidence/milestone-b/eval/diagnosis-b3-07.md`.

**B Exit Gate**: the full chain of automatic capture → candidate extraction → review/merge → cross-agent retrieval passes, with a reproducible performance/quality report; Hybrid is not required to beat every baseline, but underperforming a baseline must be explained and have a fallback.

## 5. Per-iteration Output

| Iteration | Git commit/PR-level result | Demo |
|---|---|---|
| 1 | Workspace + Core + CI | CLI help |
| 2 | DB + FTS + Scope + revision | CLI CRUD + data survives restart |
| 3 | MCP + two-agent configuration | Claude writes / Codex reads |
| 4 | A stability and docs | Full A acceptance recording |
| 5 | Claude/Codex capture adapters | Event log + idempotency test |
| 6 | Queue + LLM extraction | Session produces candidates |
| 7 | Consolidation + review | Duplicate merge, conflict pending review |
| 8 | Hybrid + benchmarks | Retrieval report + B demo |

If time is limited, iteration 5 prioritizes Claude Code; for Codex, at minimum ensure B can use the already implemented MCP retrieval, add automatic capture later, and never claim that the two agents have equivalent hook capability.

## 6. Engineering Cadence and Quality Bar

Every task needs: a corresponding issue/commit, at least one test, a README or module doc update, and a runnable command. Before merging run `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test --workspace`; cross-platform smoke testing relies on macOS/Linux CI, and real-agent tests require the corresponding clients installed and their versions recorded.

Keep a demo recording and logs for every milestone so interviewers can reproduce with minimal setup. Without a provider API key, all of A and B's capture and candidate review must be runnable (B's automatic extraction needs a configurable provider).

## 7. Risk Register

| Risk | Impact | Mitigation |
|---|---|---|
| Agent hook/transcript format upgrades | B automatic capture breaks | Adapter contract tests, version support matrix, degrade to manual MCP |
| LLM generates wrong facts | Knowledge base pollution | Candidate + evidence + review; confirmed constraints never auto-overwritten |
| Concurrent SQLite write contention | MCP failures / lost events | WAL, timeout/retry, short transactions; single-writer daemon if needed |
| Native session privacy exposure | Code/secret leakage | Allowlist capture, redaction, no network reporting by default, logs never print bodies |
| Local model latency and cost | Degraded experience | Async queue, rate limiting, FTS fallback, switchable provider |
| Scope misidentification | Cross-project leakage | Stable project ID, strict filtering, isolation tests |
| Eval data bias | Quality cannot be demonstrated | Fixed dataset, human labels, public environment and negative cases |

## 8. Definition of Done / Releases

- **v0.1.0 (A)**: a new user can initialize a local database, connect Claude Code/Codex, and save and retrieve project memories; CRUD, FTS, history and isolation all pass.
- **v0.2.0 (B)**: real sessions automatically produce candidates that can be reviewed into the store, plus hybrid search and benchmarks, with knowledge usable across agents.

If B cannot reproduce the full automatic flow, release it as alpha rather than listing unverified features in the README.

## 9. Related Documents

- [AM-DESIGN-001](./engineering-design.md): system boundaries, architecture and key technical decisions.
- [AM-ACCEPT-001](./acceptance-criteria.md): executable acceptance items and pass thresholds.
- [competitor-research](./competitor-research.md) and [milestone-adjustments-cn](./milestone-adjustments-cn.md): rationale for the A5–A8 and B6–B9 additions.

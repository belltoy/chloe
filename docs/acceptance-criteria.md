# Chloe — Local-first Coding Agent Memory — Milestone Acceptance Criteria and Test Plan

> Document ID: AM-ACCEPT-001 · Version: v0.1 · Date: 2026-10-23 · Related: AM-DESIGN-001, AM-PLAN-001
>
> The CLI commands in this document are **examples of the target interface**, not an already-built executable. If the interface changes during implementation, update this document accordingly. Test tooling, agent versions, operating system, model and dataset versions must be recorded.

## 1. Acceptance Principles and Test Environment

**Each acceptance item** records: ID, environment, preconditions, steps, expected result, actual result, evidence (command output / test log / recording), verdict (PASS/FAIL/BLOCKED) and defect number. Marking an item as passed merely by eyeballing code is forbidden.

**Target environment**: macOS arm64 as the primary demo platform, Linux x86_64 CI for portability verification; temporary SQLite test databases, each test initialized independently; real Claude Code and Codex clients (versions pinned in the report). Real-agent tests that cannot run in CI need a manual on-machine acceptance recording. Security tests use fake secrets, never real credentials.

**Overall gate**: all P0 items must pass; P1 items must pass or have an explicitly approved degradation plan and record; P2 are enhancements. v0.1.0 cannot be declared complete if A fails; v0.2.0 cannot be declared stably complete if B fails.

## 2. Milestone A Acceptance Matrix

| ID | Priority | Item | Repeatable steps | Pass criteria |
|---|---|---|---|---|
| A-01 | P0 | Project builds | Run fmt/clippy/test on a fresh checkout | All three pass, CI green on macOS/Linux |
| A-02 | P0 | Local initialization | Run `init` twice on an empty directory | First creates, second is idempotent; existing data is not wiped |
| A-03 | P0 | CRUD | Add, get, update, archive one memory via CLI | Attributes correct, revision strictly increasing, invisible to default search after archive |
| A-04 | P0 | Restart persistence | Write, exit the process, restart CLI/MCP and search | Content and revision fully identical |
| A-05 | P0 | FTS index correctness | Add, update a keyword, archive, searching each time | Hit after add; old term no longer hits after update; archived not returned by default |
| A-06 | P0 | Project isolation | Initialize repo-A/repo-B, write an A-only entry in A | Default query in B returns 0; read by ID in B is rejected/invisible |
| A-07 | P0 | Worktree project identity | Initialize and read two worktrees of one repository | Identified as the same repository ID by default, without overwriting other repositories |
| A-08 | P0 | Two-agent sharing, both directions | Claude Code writes via MCP and a brand-new Codex session searches it; then Codex writes and a brand-new Claude Code session searches it | Each agent retrieves the same id/content written by the other; no database file was copied |
| A-09 | P0 | MCP conformance | List tools, call with valid/invalid JSON parameters | Tool schemas recognizable; invalid parameters return structured errors without crashing |
| A-10 | P0 | Concurrent version conflict | Concurrently update the same id at the same revision | Exactly one request succeeds; the other returns a conflict; no half-written data |
| A-11 | P0 | SQLite fault recovery | Simulate `SQLITE_BUSY` and unexpected process termination, then restart | Correct retry/error; DB reopens and records are undamaged |
| A-12 | P0 | Data privacy baseline | Inspect configuration, listening ports, logs, permissions | No public listening by default; database accessible only to the user; logs do not leak memory bodies |
| A-13 | P1 | FTS rebuild | Deliberately create a test index anomaly, run `doctor/reindex` | Anomaly detected and repaired, result consistent with records |
| A-14 | P1 | CLI JSON mode | Use `--json` for CRUD/search | Output machine-parseable, error codes stable |
| A-15 | P1 | Quickstart | Follow the README in a clean environment for the minimal use case | No guessing of undocumented configuration; two-agent operation completes |
| A-16 | P0 | Offline operation | With the network disabled and no LLM/embedding configured, run MCP CRUD and FTS search | All operations succeed; nothing attempts an outbound connection |
| A-17 | P0 | Audit trail | Create, update and archive one memory; read `memory history` | Rows appear in order with operation, actor, revision and snapshot; UPDATE/DELETE on the audit table is rejected |
| A-18 | P0 | Explicit global scope | Save one global and one project memory; search from another project with and without `include_global` | Global memory appears only with `include_global`; the project memory never appears; invalid scope returns a clear error |
| A-19 | P1 | `doctor` diagnostics | Run `doctor` on a healthy setup, then on a read-only database and on an unusable database path | Healthy passes; each fault is named in the output and exits non-zero; the user's real database is untouched |
| A-20 | P1 | Agent guidance | Call `initialize` and inspect the response; follow `docs/agents/` snippets in a real agent | `instructions` is present and accurate; the real agent searches before work and saves a durable decision when prompted by the snippet |

**Minimal manual A acceptance script (illustrative, not implying implemented):**

```bash
chloe init /tmp/chloe-demo
chloe project show /tmp/chloe-demo
chloe memory add --project /tmp/chloe-demo --kind decision \
  --title "gRPC channel pooling" \
  --content "An endpoint may use multiple connections."
chloe memory search --project /tmp/chloe-demo "channel pooling" --json
chloe doctor
cargo test --workspace
```

Real-agent tests require configuring MCP in their project directories and having Claude Code and Codex actually call the tools; keep the session transcript or a demo recording. If an agent's permission policy blocks a call, record it and clearly distinguish permission errors from product defects.

### Negative-case Requirements for A

- A nonexistent memory ID, empty title, over-long content, invalid kind or invalid project ID returns a clear error and never panics.
- Archived/cross-project memories cannot bypass filtering by guessing IDs.
- CLI and MCP see consistent state within the same project.
- Searches containing punctuation, code symbols, or with no FTS match cause no SQL error or process exit.

### A Exit Criteria

A-01 through A-12 and A-16 through A-18 all PASS, A-13 through A-15 and A-19 through A-20 have no unexplained FAIL; the README shows a reproducible cross-agent scenario; tag `v0.1.0`.

## 3. Milestone B Acceptance Matrix

| ID | Priority | Item | Steps | Pass criteria |
|---|---|---|---|---|
| B-01 | P0 | Claude automatic capture | Configure the hook and run a session with a user task and key tool actions | Valid events with agent/session/project appear in the event table; no manual `memory_save` needed |
| B-02 | P0 | Codex adapter | Install a declared-supported Codex version and run a session | At least one real capture path works; unsupported events are listed explicitly, completeness not faked |
| B-03 | P0 | Event idempotency | Import the same hook/transcript fixture twice in a row | Unique event count does not increase; the same session does not trigger a successful job twice |
| B-04 | P0 | Redaction | Inject a fake API key, an over-long payload, sensitive paths | Persisted events / logs / LLM input never contain the full fake key; length bounded |
| B-05 | P0 | Hook degradation | Shut down the daemon / make the LLM time out, then run the agent | Agent's core task continues; hook exits within the configured short timeout and warns/counts |
| B-06 | P0 | Extraction pipeline | Extract from a fixed session fixture with a configured local provider | Parseable candidates with kind/title/content/evidence; Active memories not directly overwritten |
| B-07 | P0 | Fault retry | Inject model timeout; kill the queue worker and restart | Confirmed enqueued jobs are retained; retries bounded with no duplicate effects |
| B-08 | P0 | Dedup | Extract the same technical fact in two sessions | No two equivalent Active memories created; provenance links viewable |
| B-09 | P0 | Conflict pending review | A new candidate contradicts a user-specified Constraint | Existing Active not silently overwritten; Conflict Pending can be reviewed via CLI |
| B-10 | P0 | Full chain | Claude Code auto-produces a candidate, reviewed to Active; a new Codex session queries | Codex finds the knowledge via MCP with correct source/session; no manual `memory_save` |
| B-11 | P0 | Scope guarantee | Two projects with similar candidates/vectors | No search branch leaks the other project's content, including Vector/RRF |
| B-12 | P0 | Embedding degradation | No local embedding service or model unavailable | FTS still works; diagnosable degradation reason; MCP does not hang |
| B-13 | P0 | Reproducible retrieval evaluation | Run all three modes with a fixed model/dataset/queries | Machine-readable report with Recall@5, MRR, p50/p95 and environment info, broken down by language (English, Chinese, mixed, cross-language) |
| B-14 | P1 | Schema compatibility | Event fixtures of different versions, unknown fields, missing end event | Supportable formats parse; unsupported ones are explicitly rejected and counted |
| B-15 | P1 | Vector index consistency | Update/archive a memory then retrieve; change model_id/dimension | No wrong hits from stale vectors; different model dimensions never mixed |
| B-16 | P1 | Extraction quality | Evaluation set mixing irrelevant and useful events | Shows candidate acceptance rate and false-extraction cases; serious factual errors never automatically become Active |
| B-17 | P1 | Diagnostic visibility | `doctor/stats/jobs` to view backlog, failures and provider | User can diagnose why no memory was generated automatically |
| B-18 | P2 | Manual session compensation | Simulate missing SessionEnd, run compensation/idle handling | Session can enter extraction, and dedup holds |
| B-19 | P0 | Evidence / provenance | Extract a candidate from a fixture session and accept it | The memory shows agent, session and event IDs (and commit/path when present); a memory without evidence cannot become Active automatically |
| B-20 | P0 | Supersession | Propose superseding an old Decision, then one that is a user Constraint | The Decision is superseded with a link and revisions on both; the Constraint change waits for human approval and is not applied silently |
| B-21 | P1 | Explain | Run a query with `--explain` in FTS, Vector and Hybrid modes | Output lists matched paths, ranks, scope, status filter and ranking factors; degraded paths are named; archived memories are filtered |
| B-22 | P1 | Context Pack | Request a pack with a small and a large budget | Output stays within the budget, is grouped by kind, lists memory IDs and sources, and expansion by ID returns the full memory; a cross-project ID is not expandable |

### Key Executable Scenarios for B

**Scenario B-E2E-01: automatic learning + cross-agent recall**

1. In a clean test repository, create a new project and connect the real Claude Code hook and the Memory MCP.
2. Ask it to fix a minimal bug with a clear cause; make sure there is an observable task, change and result.
3. End the session or trigger `consolidate` manually; confirm events are stored, the job completes and the candidate has a source.
4. Review the candidate to Active via CLI; record the memory ID.
5. Fully quit Claude Code and start a new Codex session; search for the problem via MCP.
6. Show the same ID, an accurate summary and the source; confirm there was no manual `memory_save`.

**Scenario B-FAULT-01: failure and recovery**

1. Create an extraction job from a fixed event fixture.
2. Simulate process termination after the worker obtains the lease.
3. Restart the worker and wait for the lease to expire so the job is reclaimed.
4. Expect exactly one valid candidate/memory in the end; job state is inspectable, and retries do not loop forever.

**Scenario B-SEC-01: sensitive information**

1. Generate a fake token (specifically labelled, e.g. `FAKE_TEST_SECRET_...`).
2. Put the token in a tool-output fixture.
3. Check in order: adapter → persistence → extraction input → logs.
4. The full token must not appear at any stage; a redaction placeholder may remain.

## 4. Quality and Performance Benchmarks (targets, not existing results)

| Metric | Initial threshold | Notes |
|---|---|---|
| A local FTS retrieval p95 | ≤ 200 ms | 1,000 short memories, macOS target dev machine, after warm-up, 200 queries; record hardware/data size |
| A CRUD request success rate | 100% | No fault injection, 500 sequential CRUD operations |
| A concurrent version conflicts | 100% correctly detected | 100 rounds of two writers at the same revision |
| B capture idempotency | 0 duplicate events | Replay at least 1,000 fixture events twice |
| B hook response | p95 ≤ 500 ms | Local short-event receipt only, not waiting for the LLM; based on 100 samples |
| B automatic severe-conflict overwrites | 0 | At least 10 hand-designed conflict samples |
| B agent cross-project leakage | 0 | 20 cross-project negative cases, including vector retrieval |
| B eval size | ≥ 50 memories, ≥ 20 labelled questions | Covering code symbols, synonyms, outdated information, unrelated projects, and English, Chinese, mixed and cross-language queries |
| B retrieval benchmark | Publish Recall@5/MRR/p50/p95 | **Hybrid is not required to win**; if worse, analyze why |

These thresholds are suggested first-version acceptance targets and must be calibrated on the actual dev machine and model environment. Model inference time is excluded from `hook response`; retrieval latency reports must state whether remote embedding calls are included, and should report warm-cache / local-only / including-provider latency separately.

## 5. Acceptance Report Template

```markdown
# Acceptance Run
Date:
Version / Commit:
OS / CPU / RAM:
Claude Code version:
Codex version:
SQLite version:
LLM / Embedding model:
Dataset checksum:

| Check | Priority | Status | Evidence | Issue |
|-------|----------|--------|----------|-------|
| A-01  | P0       | PASS   | ...      |       |

Blockers:
Known limitations:
Sign-off:
```

Evidence should be collected under `docs/evidence/milestone-a/` and `docs/evidence/milestone-b/`, without user private data, full raw agent transcripts or real credentials.

## 6. Release Gates / Unacceptable Substitute Evidence

### Milestone A — Release Gate

- All A P0 items pass; both real agent clients complete MCP interoperation.
- Each real agent reads memories written by the other, and MCP CRUD/FTS works offline.
- Data remains queryable after process restart; FTS sync, CAS and cross-project isolation have automated tests.
- README configuration steps are reproducible; no remote network service is enabled by default.

### Milestone B — Release Gate

- All B P0 items pass; no mandatory external cloud model dependency, with at least a configurable local provider.
- Evidence exists for the real chain from automatic capture to candidate to cross-agent retrieval.
- No high-authority conflict may overwrite an existing decision without review.
- FTS, Vector and Hybrid can each be benchmarked individually, and the report includes reproducible configuration and failure cases.

**Not accepted**: screenshots of integrations with only a mock MCP; the model merely saying "I remembered"; counting only the number of new memories; absolute latency numbers with no recorded environment; memory-accuracy claims with no source or human evaluation.

## 7. Requirement — Task — Acceptance Traceability

| Capability | Tasks | Acceptance IDs |
|---|---|---|
| Project knowledge and scope | A1-01 – A1-03 | A-02/A-03/A-06/A-07 |
| FTS, versions and reliable storage | A1-03 – A1-05 | A-04/A-05/A-10/A-11/A-13 |
| MCP + CLI for two agents | A2-01 – A2-04 | A-08/A-09/A-14/A-15 |
| Default privacy | A3-01 | A-12 |
| Audit trail | A1-06 | A-17 |
| Explicit global scope | A2-05 | A-18 |
| Agent guidance and `doctor` | A2-06 – A2-07 | A-19/A-20 |
| Offline operation | A1 – A2 | A-16 |
| Provenance and supersession | B1-03, B2-01, B2-06 | B-19/B-20 |
| Explain and Context Pack | B3-03, B3-06 | B-21/B-22 |
| Automatic hook/transcript | B0-01 – B0-05 | B-01 – B-05/B-14 |
| LLM extraction and recovery | B1-01 – B1-05 | B-06/B-07/B-18 |
| Consolidation + review | B2-01 – B2-05 | B-08/B-09/B-16 |
| Embedding/Hybrid/Bench | B3-01 – B3-05, B3-07 | B-10 – B-13/B-15/B-17 |

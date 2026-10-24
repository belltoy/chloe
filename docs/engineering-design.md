# Chloe — Local-first Coding Agent Memory — Engineering Design

> Document ID: AM-DESIGN-001 · Version: v0.1 · Date: 2026-10-23 · Status: for implementation review
>
> Scope: Milestone A (a usable MCP memory service) + Milestone B (automatic capture, extraction, dedup, hybrid retrieval and evaluation).

## 1. Goals and Boundaries

### 1.1 Goals

Build a Rust-core, single-machine, local-by-default long-term memory system that lets coding agents such as Claude Code and Codex share project knowledge across sessions and across agents. The system must provide searchable, revisable memories with project scope and provenance, rather than simply storing whole conversations. Delivery is judged by reproducible real development workflows and performance/quality evaluation.

**Milestone A:** Agents explicitly save, search, read, update and deprecate project memories through MCP; two agents on the same repo see the same effective knowledge; SQLite persistence, FTS5, CLI, tests and local configuration are in place.

**Milestone B:** Adapters obtain usable session events or transcripts automatically and ingest them reliably; candidate knowledge is extracted via a configurable local/remote LLM provider; results are merged and deduplicated while keeping provenance and versions; optional local embeddings and hybrid retrieval; an evaluation set demonstrates benefit and cost.

### 1.2 Non-goals

No cloud accounts, remote multi-user support, multi-device sync, P2P, complex web/desktop GUI, distributed storage, full session replay, complex knowledge graph, automatic code rewriting, or general-purpose coding-agent execution engine.
Milestone B does **not** guarantee that all agents expose equivalent native events; features must degrade progressively according to actual adapter capability. Fully offline LLM inference is not required, but "data stored locally" must be clearly distinguished from "using a cloud model".

### 1.3 Principles

1. **Agent-agnostic core**: agent formats and hook differences are encapsulated in adapters.
2. **Local-first & explicit egress**: listen locally and store locally by default; calling cloud models must be explicitly enabled.
3. **Non-blocking capture**: memory failures must not impair the agent's basic operation; define which events may be lost and which persistent state must not be.
4. **Evidence over assertion**: LLM output is first a candidate; what is finally written has provenance, is reviewable and versioned.
5. **Deterministic baseline**: with no model configured, A is fully usable; in B, rule-based capture, candidate review and FTS keep working.
6. **Small deployable unit**: single machine, single instance first; no multi-service orchestration just for demos.

## 2. User Scenarios and Key Use Cases

- **UC-A1**: Claude Code connects to MCP, saves constraints and design decisions of `repo-X`, and immediately searches them by keyword.
- **UC-A2**: After Claude Code is closed, Codex reads the existing memories via MCP in the same repository.
- **UC-A3**: An ordinary query in another repository does not accidentally return `repo-X` private memories; authorized global memories can be queried explicitly.
- **UC-A4**: Edit a memory, view versions, deprecate an old conclusion; search returns only Active status by default.
- **UC-A5**: A new user runs `chloe doctor` and learns whether the database, configuration and MCP server work; the agent, guided by the server's instructions, searches before starting work and saves durable decisions.
- **UC-B1**: Key activity happens in a new Claude Code session; the adapter captures it and it enters the extraction queue after the session ends.
- **UC-B2**: Duplicate reports of the same session produce no duplicate events/memories; model-call failures are recoverable.
- **UC-B3**: Duplicate technical conclusions merge into one; candidates that conflict with a confirmed decision enter a review queue.
- **UC-B4**: Codex can retrieve knowledge produced by Claude Code, with source and task context.
- **UC-B5**: A benchmark compares FTS, Vector and Hybrid on Recall@K, latency and context tokens.
- **UC-B6**: A new task asks for a Context Pack and gets a compact, source-linked summary by kind, expanding single memories only when needed.

## 3. Overall Architecture

```text
Claude Code  ── MCP (stdio) ──┐
Codex        ── MCP (stdio) ──┤   [memory-mcp]
                              ├── [memory-core / service]
CLI ──────────────────────────┤           │
                              │           ├── [memory-store: SQLite + FTS5]
Hooks/Transcript Readers ─────┘           ├── [memory-search]
      │                                   └── [memory-engine: B]
      └── [memory-ingest: B] ─── job queue ─────┘
                                               │
                                      Optional LLM / Embeddings
```

**Deployment convention:** In the MVP each MCP stdio process may use the same SQLite file (WAL + busy_timeout + transactions and retries) and the CLI may access it directly. In B, if hooks need fast asynchronous reporting, prefer a single-machine background worker/daemon or a local persistent spool, so that each hook does not trigger an expensive extraction. Do not assume a "single writer actor" is naturally compatible with "multiple independent stdio processes": the latter still depends on SQLite's cross-process write lock. Whether to introduce a daemon is decided by actual concurrency tests; for B, a local daemon that accepts events centrally and coordinates jobs is recommended.

### 3.1 Rust Workspace

```text
crates/
  memory-core/       # IDs, domain model, scope, state machine, traits
  memory-store/      # SQLite migrations, transactions, FTS sync, version history
  memory-search/     # A: FTS; B: vectors, fusion/rerank
  memory-mcp/        # MCP tools/server; stdio lifecycle
  memory-cli/        # init / project / memory / doctor / eval
  memory-ingest/     # B: agent adapters, event normalization, redaction, spool
  memory-engine/     # B: extraction, dedup, merge, review, job execution
  memory-llm/        # B: replaceable extraction/embedding providers
  memory-eval/       # B: evaluation tooling and fixed fixtures
```

Initially a Cargo workspace with shared dependency versions is enough; no dynamic plugin loading. Core depends on neither a specific agent nor SQL.

## 4. Domain Model and Identity

### 4.1 Project Identity / Scope

- The repository ID is a locally persistent UUID; a Git `remote` is only a mutable alias and cannot be the sole globally unique key.
- **Resolver policy (A1)**: the identity key is the canonical path of the main checkout. A Git worktree resolves to its main repository and is recorded as a `worktree` alias; a plain directory is its own project. A moved or renamed path becomes a NEW project until linked with `add_alias(project, path, "moved")`, which refuses paths already owned by another project. The remote URL is never used as identity and is not auto-populated in A.
- Normalized local paths, the Git top-level and worktrees map to the same repository ID; standalone non-Git directories can have a project ID initialized explicitly.
- `scope` has at least `global` / `project`; the MVP does no cross-user sharing, derives the scope from the current working path by default and validates it strictly.
- Branch and worktree may serve as provenance metadata and future B filters; A does not enforce branch isolation.
- The MCP client passes project_id explicitly, or the server identifies it from trusted configuration/working path; do not trust a repo ID invented by the model. Multiple MCP instances must resolve to the same persistent ID.

### 4.2 Memory

```text
Memory {
  id: UUID, project_id: UUID | null, kind: Fact|Decision|Procedure|Constraint|Gotcha,
  title: String, content: String,
  status: Active|Disputed|Superseded|Archived,
  source: Manual|Agent|Extracted,
  created_at, updated_at, revision: integer
}
```

- The first version builds no complex confidence scoring; provenance and status matter more than false-precision scores.
- Types and states are closed enums validated in the service and by database `CHECK` constraints; invalid values return `InvalidArgument` and never touch stored data.
- Global scope is explicit: a memory is global only when saved with `scope: global`; a project-scoped caller sees global memories in search only with `include_global`.
- Every create/update/archive appends a row to `memory_revisions` (operation, actor, revision, snapshot); the table is append-only (triggers reject UPDATE/DELETE). This is an audit trail, not event sourcing.
- B memories carry evidence: agent, session, event IDs and, when available, file path or commit. A supersession is a `supersedes` link plus status `superseded`, proposed by the extractor and applied only per the review policy.
- B adds the unconfirmed `CandidateMemory`, `SourceRef` (agent/session/event/commit/path, etc.) and a merge operation log.
- `revision` provides optimistic concurrency control; updates require expected_revision, and a failure returns a conflict hint.
- Search returns only Active by default; history is available through an explicit include_inactive; each result returns id, scope, snippet, status, source and score.

### 4.3 Schema (conceptual)

| Table | Key fields | Purpose |
|---|---|---|
| `projects` | id, canonical_path, created_at | Stable project identity |
| `project_aliases` | project_id, path, remote_url, kind | worktree / moved-path mapping |
| `memories` | id, project_id, kind, title, content, status, revision, timestamps | Current memories |
| `memory_revisions` | memory_id, revision, snapshot, changed_at, cause, actor | Append-only audit trail |
| `memory_fts` | title, content | FTS5 external-content index |
| `sessions` (B) | id, agent, native_id, project_id, state | Session association |
| `events` (B) | idempotency_key, session_id, event_type, safe_payload, timestamp | Normalized capture |
| `jobs` (B) | id, kind, state, attempts, lease_until, last_error | Durable background jobs |
| `candidates` (B) | id, session_id, content, proposal_state, evidence | Knowledge awaiting confirmation |
| `memory_sources` (B) | memory_id, source_kind, source_ref, details | Provenance |
| `memory_links` (B) | from_id, to_id, relation (`supersedes` / `conflicts_with`), created_at | Supersession and conflict relations |
| `embedding_index` (B) | model_id, dimension, generation | The one model the index is pinned to |
| `embeddings` (B) | memory_id, revision, vector, updated_at | Semantic retrieval |

Enable SQLite `foreign_keys=ON`, WAL and `busy_timeout`; use migration scripts and transactions. FTS5 external-content must have INSERT/UPDATE/DELETE triggers, plus `rebuild` and consistency-check commands. Full-text search must never interpolate unescaped strings into hand-built SQL.

## 5. Milestone A: MCP + Manual Memory

### 5.1 MCP Interface Contract

The following tools are suggested (parameters and error objects are schema-validated):

- `memory_save({project, kind, title, content, scope?}) -> {id, revision, status}`: explicit creation; `scope` is `project` (default) or `global`.
- `memory_search({project, query, limit?, include_global?}) -> {hits[]}`: project isolation and Active by default; FTS5 + explainable ranking.
- `memory_get({id, project}) -> {memory, revisions_summary}`: fetch details by ID; a cross-project read is rejected / invisible.
- `memory_update({id, expected_revision, changes}) -> {revision}`: concurrency-safe update.
- `memory_archive({id, expected_revision})`: soft delete / deprecate, keeping history.

A `memory_list` may be added for debugging, but semantic retrieval must not depend on a list implementation. The MCP server and client configuration adapt to the supported SDK/agent versions, and the documentation gives real configuration examples for Claude Code and Codex (do not invent configuration keys; verify against real versions during implementation).

The server's `initialize` response includes `instructions` telling the agent when to search (before starting work on a project), when to save (durable decisions, constraints, gotchas, procedures — not transient task state), and to re-read and retry on a revision conflict. Clients differ in how visibly they surface server instructions, so the repository also ships `CLAUDE.md` / `AGENTS.md` snippets and configuration templates under `docs/agents/`.

### 5.2 CLI

```text
chloe init [path]
chloe project show [path]
chloe memory add --project ...
chloe memory search --project ... "keywords"
chloe memory show <id>
chloe memory update <id> --revision N ...
chloe memory archive <id> --revision N
chloe memory history <id>
chloe doctor
chloe mcp serve
```

All MCP business logic goes through the same service layer to avoid CLI/MCP behavior drift. JSON output is supported for automation.

### 5.3 Retrieval Baseline

- FTS5 (optionally the unicode61 tokenizer) matches title/body; ranking is explainable, combining title boost, exact phrase and recent update.
- Default limit of 5–10 results, with a source/readable snippet; set a maximum response character/token budget.
- Provide predictable feedback for short queries, code symbols and no-match cases. Do not automatically inject arbitrary project knowledge into every prompt; in A the agent retrieves actively.

### 5.4 Lifecycle

A's `save` is an atomic transaction: write the current record + initial revision + FTS trigger. `update` does a compare-and-swap on revision, inserts a history version and updates the index in one transaction; `archive` is a status change. A conflict or I/O failure must never return success.

### 5.5 Agent Guidance and `doctor`

`chloe doctor` verifies, in order: the database opens, is writable and migrated; FTS is consistent (`--reindex` repairs it); the configuration resolves; `mcp serve` starts and answers initialize and tools/list; and a save → search → get → archive round trip works. The round trip runs against a throwaway database, never the user's data. It exits non-zero when any check fails and supports `--json`. Whether a real agent actually calls the tools is verified separately by the acceptance runs, not by `doctor`.

## 6. Milestone B: Automatic Memory Loop

### 6.1 Capturing & Adapters

B first picks Claude Code as the primary adapter, implementing real working hooks; for Codex, choose hook notifications or a read-only local transcript reader depending on the installed version's capabilities. **Do not assume all agents have equivalent ToolStart / ToolEnd / SessionEnd events.**

Unified event types are `SessionStart`, `UserPrompt`, `ToolResultSummary`, `ContextCompacted` and `SessionEnd` (fields optional), retaining `native_agent`, `native_session_id` and `schema_version`. Adapters handle native-format version compatibility and feature detection; they are tolerant of unknown fields and reject (and count) invalid core fields. Capture has timeouts, length limits, configurable sampling/filtering, secret and sensitive-path redaction, and a kill switch. Uploading full source code, environment variables or tool output to model services is forbidden by default.

Reliability strategy: hooks return fast; write small redacted events to a local persistent queue/SQLite first, then run the LLM asynchronously. Idempotency uses `(agent, native_session_id, native_event_id)` or a stably reconstructible composite key; duplicate capture adds no events. Only **at-least-once processing after durable acceptance + idempotent effects** is promised, not end-to-end exactly-once.

### 6.2 Extraction

Triggers: SessionEnd (if available), manual `consolidate`, idle timeout (for agents without SessionEnd). Each session defines a maximum input length and a configurable segment-selection strategy. A `MemoryExtractor` trait outputs structured candidates: type/title/content/evidence/notes. Model output must be schema-validated and size-limited. Without an LLM, events are still retained, and the job is `pending_provider` or extraction is skipped, with status reported.

As built (B1-02): `memory_core::extract::MemoryExtractor` returns `Candidate`s (kind, title, content, evidence) and reports an unreachable provider as `Error::Unavailable`, so the caller keeps the events and retries. The `memory-llm` crate implements it for Ollama (`/api/generate` with a JSON-schema `format`, temperature 0, default model `qwen3:8b`, server `http://127.0.0.1:11434`). A non-loopback URL is refused unless `--allow-remote` is given, and only plain `http://` is supported. Model output that is not valid JSON or does not match the schema is rejected, never stored. `chloe llm check` and `chloe llm extract <session>` (prints candidates, saves nothing) demonstrate it; `doctor` reports the provider as informational. Config: `CHLOE_OLLAMA_URL`, `CHLOE_LLM_MODEL`.

As built (B1-03): the prompt (`memory-llm/src/prompt.rs`) lists the session's events as `E1`, `E2`, … inside a `<session_events>` block, escapes `<` in event text so an event cannot close the block, and tells the model the events are untrusted data. The model cites events by those short refs, so it cannot invent an event key. Every reply goes through `validate`: title 1–100 chars, content 1–1500 chars, at most 8 candidates, at least one real event ref (otherwise `no_evidence`), and a candidate containing a secret or a credential path is rejected whole, not redacted. `file` and `commit` are kept only if a cited event literally shows them. Session, agent and event keys in `Evidence` come from our input, never from the model. Dropped candidates are logged as reason codes only. The model's text is still a proposal: it can paraphrase or add detail the events do not give, which is why candidates go through review (B2) before they become memories.

As built (B1-04): `chloe consolidate` imports the capture queue, enqueues one `extract_session` job per due session (deduplicated by agent and session id), then runs the jobs in the same process; there is still no daemon, so extraction happens when that command runs. A session is due when it has ended (a `SessionEnd` arrived), or has had no event for `--idle-minutes` (default 30, for agents or runs without a `SessionEnd`), or is named with `--session`. The model sees only the newest `--max-events` events (default 200) that fit `--max-chars` (default 20000); the window is stored in the job so a retry sees the same input. Candidates are stored in the `candidates` table as `pending` (unique per session, kind and title) and listed with `chloe candidates list`; the table's `state` column already allows the B2 review states. When the model server is down the job fails with its error text and retries on the job backoff (events are untouched); after the attempts are used up it waits in the dead letter for `chloe jobs retry`. Known limits: a session is extracted once, so events added after its job ran are not re-extracted; a crash between storing candidates and completing the job can, because the model is not deterministic, store a re-run's differently-titled candidates twice, which B2 dedup is meant to absorb.

### 6.3 Consolidation

- Use same-project FTS/vector search to find existing memories that may be duplicates.
- Classify candidates as `create / merge / supersede-proposal / flag-conflict / reject`.
- Only clearly duplicate, low-risk candidates may be auto-merged; conflicts with explicit user constraints/decisions must enter the review queue.
- Original memories and merge sources always keep a queryable revision record; prevent a model-generated contradictory conclusion from directly overwriting user-confirmed information.
- Supersession: the extractor may only propose `supersede`; applying it sets the old memory to `superseded`, links it with `supersedes`, and appends a revision to both. Superseding an explicit user Constraint/Decision requires human approval.

As built (B2-01): a candidate is `pending`, `accepted`, `rejected`, `merged` or `conflict`; `pending` and `conflict` are open, the other three are final. `CandidateStore::resolve_candidate` applies one decision in a single transaction: *accept* creates a memory with source `Extracted` and a `memory_revisions` row with cause `extracted`; *reject* keeps the reason; *merge* leaves the target memory untouched and records the candidate as another source of it; *conflict* flags a contradiction with a memory and leaves the candidate open for a human. The decision, its reason, the memory it points at, when and by which actor are stored on the candidate. Provenance is table `memory_sources` (memory, candidate, session, relation `created`/`merged`, the candidate's evidence as JSON); a candidate is the source of at most one memory, and `memory_sources(memory)` lists them. A merge or conflict target must be in the candidate's project or global, otherwise `NotFound`; a candidate whose session has no registered project is accepted only as global. Racing decisions on one candidate: exactly one wins. Deciding automatically (B2-02/03) and the review CLI (B2-04) build on this and are not part of it.

As built (B2-02): after extraction, `chloe consolidate` gives every pending candidate without one a *proposal* (`candidates.proposal`, JSON): an action (`create`, `merge`, `conflict`), the target memory, the method (`rules` or `model`), a plain-language reason and the similar memories it looked at. A proposal is advice only; the candidate stays `pending` and nothing is applied (that is B2-03's policy and B2-04's CLI). Steps: (1) full-text search over the candidate's title and the first words of its content, in the candidate's project plus global, `Active` memories only, top 5; (2) none found: `create` by rules; (3) same kind and at least 80% title-word overlap (Jaccard): `merge` into that memory by rules; (4) otherwise the local model compares the candidate with the found memories and answers `duplicate`, `contradicts` or `unrelated`, naming one of the notes it was shown (`merge`, `conflict`, `create`); a memory it was not shown is ignored and treated as unrelated, M-refs in its reason are replaced by memory titles, and a reason that looks like a secret is withheld. With no model reachable, a candidate that needs one gets no proposal and is retried on the next run; if the model is simply not configured the reason says similar memories exist but were not compared. Known limits: similarity is lexical (FTS), so a paraphrase with different words is not found, and CJK text matches only whole unsegmented runs until B3-07; one live run on `qwen3:8b` classified a duplicate, a contradiction and an unrelated candidate correctly, which is an example, not a measurement.

As built (B2-03): after proposing, `chloe consolidate` applies a small fixed safety policy (`memory-engine/src/policy.rs`; `--no-auto-apply` turns it off and only suggests). A `merge` proposal is applied automatically only when the candidate and the target memory are the same kind, neither is a `constraint` or `decision`, and the target is still `active`; the existing memory is not modified, the candidate becomes `merged` with the reason `auto-merged as a low-risk duplicate: …`, and `memory_sources` records it. A `conflict` proposal is applied as a flag only: the candidate moves to `conflict` and stays open, no memory changes. A `create` is never applied. A merge that the policy refuses (a constraint or decision on either side, different kinds, an inactive or vanished target) stays `pending` and its proposal gets `needs_review` with the reason, shown by `chloe candidates list`. The rules do not depend on the proposal's method, so a model-made duplicate verdict gets no more trust than a rule-made one; the limit is that a wrongly judged "duplicate" of a low-risk kind is merged and its extra detail is not kept (the candidate row keeps the full text and evidence, but it is final).

As built (B2-04): the review queue is `chloe candidates`. `list` shows the open candidates (pending, and conflicts awaiting a person) with the suggestion and any `needs a person` reason; `--state pending|accepted|rejected|merged|conflict|all` shows others. `show <id>` prints the text, the evidence (agent, session, event keys, file, commit), the suggestion and, once decided, the decision with its reason and the memory it points to. `accept <id> [--global]` creates the memory (a candidate whose session has no registered project needs `--global`), `reject <id> [--reason]`, `merge <id> --into <memory> [--reason]`; all use the same single-transaction `resolve_candidate` as the automatic policy, so a conflict-flagged candidate can be accepted, rejected or merged by a person and an already-decided one is refused (`invalid_argument`). Traceability: the candidate row keeps the decision, reason, time and actor (`cli`); `chloe memory history` shows the `extracted` revision of an accepted memory; `chloe memory sources <id>` lists the candidates it was created from or absorbed, with their evidence. A merge does not edit the memory, so there is no revision to show; to fold new wording in, `chloe memory update` first (which writes a revision), then merge. Candidate ids are full UUIDs (no prefix matching), and the queue lists every project's candidates, because it is the owner's CLI and not an agent-facing tool.

As built (B2-06): supersession is a fifth outcome next to create, merge and conflict. The model judge may answer `supersedes` (the new note clearly says it replaces or changes an existing one, as opposed to merely contradicting it); the proposal becomes `supersede` with the target. It is only a proposal: the safety policy never applies it, and it records `needs_review` ("replacing a memory is always decided by a person", or, when the target is a constraint or decision, "only a person can supersede it"). A person applies it with `chloe candidates supersede <id> --target <memory> [--reason] [--global]`. In one transaction the store creates the new memory from the candidate (source `Extracted`, revision cause `extracted`), sets the old memory to `superseded` with a new revision (cause `superseded`), adds a row to `memory_links` (`from` = newer, `to` = replaced, relation `supersedes`, a memory can be replaced once) and records the candidate as the new memory's source; the candidate becomes `accepted`. Rules enforced by the store, not by the caller: the target must be `active` and in the same scope as the new memory (a project memory is not replaced by a global one or the other way round; another project's memory is `NotFound`), and a `constraint` or `decision` is superseded only when the resolution carries `by_human`, which the automatic policy never sets. A superseded memory disappears from default search and stays readable by id and with `include_inactive`; `chloe memory links <id>` shows both directions, `chloe memory history <id>` the `superseded` revision. There is no undo yet. In one live run the model proposed a supersession of a different, also plausible, memory than the one intended, which is why it stays a proposal.
- CLI review is acceptable initially; no complex GUI.

### 6.4 Embedding + Hybrid Retrieval

- Providers are isolated behind an interface; local Ollama / compatible embedding APIs are the preferred demo option; optional external APIs must be explicitly enabled by the user.
- The embedding model must cover English and Chinese (memories and queries are often mixed, and code docs are mostly English). Keyword search for CJK text is a separate FTS-tokenizer question deferred to B (B3-07); Milestone A documents it as a known limitation.
- Strictly validate `model_id + dimension`; changing models requires rebuilding the vector index, and different dimensions are never mixed.
- Take FTS and Vector top-N each, fuse by RRF, then hard-filter by project scope and status, and rerank moderately; results report retrieval source and latency.
- As built (B3-01): `memory_core::embed::Embedder` / `EmbeddingStore`; `memory-llm` implements `Embedder` for Ollama (`/api/embed`, default model `qwen3-embedding:0.6b`, 1024 dimensions, config `CHLOE_EMBED_MODEL` / `--embed-model`). `embedding_index` pins one model; the first vector fixes the dimension and a vector of another model or size is refused. `chloe embeddings backfill` queues one `embed_memory` job per memory revision without a vector (an edited memory is embedded again) and runs them; jobs are leased per kind so extraction workers never take them. Choosing another model drops all vectors and bumps `generation`, which is part of the job key, so the rebuild is queued even for a model used before. `chloe embeddings status` shows the pinned model, dimension and missing count. Memories are not embedded on save yet; run `backfill`.
- When embeddings are temporarily unavailable, fall back to FTS; MCP must not become wholly unavailable due to model-service failure.
- Query-vector caching and ingest-time vector updates may come later; first ensure consistency and baseline evaluation.

### 6.5 Background Jobs

A job goes `pending → leased → succeeded / retry / dead_letter`. Each job stores `attempts`, error, next retry time and lease deadline; expired leases are reclaimed after a crash. Extraction is deduplicated by session ID; repeating any step must be idempotent. Exponential backoff has an upper bound, and users can inspect the failure queue and retry via the CLI.

As built (B1-01): table `jobs`, unique on `(kind, dedup_key)`, so enqueueing the same work twice returns the existing job. `retry` is not a stored state: it is a `pending` job with `attempts > 0` waiting for its backoff (30 s, doubling, capped at 1 h; 5 attempts by default). A lease carries a `lease_id` that `complete`/`fail` must present; a worker whose lease expired and was reclaimed gets `stale_lease` and its late result is dropped. A job whose workers keep crashing dead-letters after its attempts are used up. There is no worker yet; `chloe jobs list|show|retry` inspects the queue and `doctor` reports its counts.

### 6.6 Explain and Context Pack

- **Explain**: a diagnostic mode returns, per hit, the retrieval paths that matched (FTS / Vector), their ranks, the scope, status and the main ranking factors, plus whether a path was skipped or degraded. Hard filters (scope, status) are applied before ranking and are reported.
- **Context Pack**: given a task description and a token/character budget, returns memories grouped by kind, each as a short summary with its memory ID and source, in a bounded size; the agent expands a single memory with `memory_get`. It reuses the same retrieval and filters as search, so isolation guarantees are identical, and it never injects automatically.

## 7. Security, Privacy and Reliability

- Default configuration: local SQLite file (readable/writable only by the current user), MCP stdio, no external listening; if HTTP is enabled, default to `127.0.0.1` + token/same-machine safeguards, and document the security boundary.
- Before writing events, apply structured-field allowlisting, secret detection (tokens, keys, etc.) and log truncation; keep reason codes and redaction statistics.
- Do not promote historical text retrieved via MCP to system instructions; it is low-trust data. Limit prompt injection and malicious memory modification; important automatic state changes require human review.
- Use an independent data directory and DB migrations; support SQLite's official backup mechanism or a backup after safe shutdown, and do not rely solely on copying a WAL data file that is being written.
- Agent disconnects, MCP crashes, full disk, database busy and LLM timeouts must all return deterministic errors and record diagnostics; the hook side must not block for long.
- Do not promise true "fully offline" unless a local provider is selected and all external requests are disabled.

## 8. Observability and Operations

Structured logging with `tracing`, with session/project/request/job correlation IDs, never printing sensitive bodies; `doctor` checks the database, migrations, FTS, agent adapter configuration, provider status, queue and data-directory permissions.

Suggested metrics: MCP operation latency/error rate, sqlite_busy retry count, capture_accept/drop, queue backlog, extraction success rate, candidate accept/merge/reject ratios, retrieval p50/p95, FTS/vector hit counts, LLM tokens and call counts (if available). The local version can be observed through logs and a CLI `stats`; no Prometheus service is needed.

## 9. Test Strategy and Evaluation

- **Unit**: state transitions, scope resolver, token/length limits, merge decisions, ranking.
- **DB Integration**: FTS trigger sync, migration upgrade/downgrade policy (state explicitly if downgrade is unsupported), transaction rollback, concurrent update conflicts, power-loss recovery simulation.
- **MCP Integration**: stdio tool list, schema validation, full CRUD, two instances sharing data on the same repo.
- **Adapter Contract**: a minimal JSON fixture per agent; unknown fields, format changes, duplicate events, missing SessionEnd.
- **Fault Injection**: model timeout, database busy, process interruption, disk error, duplicate queue leasing.
- **Retrieval Evaluation**: fixed dataset and queries, comparing FTS/Vector/Hybrid on Recall@5, MRR, p50/p95 and context tokens; guarantee no cross-project leakage.
- **End-to-end Demo**: Claude Code records an engineering constraint / finishes a problem; a new Codex session retrieves the relevant memory and its source. First written explicitly in A; B demonstrates automatic capture and extraction.

## 10. Key Trade-offs and Risks

| Decision | Choice | Given up / risk |
|---|---|---|
| Storage authority | SQLite, versions retained | Bidirectional Markdown editing sync not supported for now |
| Retrieval | FTS5 in A, hybrid in B | Vector index adds model cost and complexity |
| Integration | Standard MCP interface + progressive adapters | Event capability cannot be uniform across all agents |
| Write semantics | Transactions + idempotency/retry | No cross-process global exactly-once promise |
| Automatic knowledge changes | Review first, keep provenance | Less automation, but avoids pollution |
| Configuration | Local by default, only explicit providers egress | Cloud-model convenience vs privacy trade-off |

## 11. Deliverables and Mapping

- **AM-PLAN-001**: tasks and dependencies for A/B.
- **AM-ACCEPT-001**: repeatable acceptance criteria and evidence requirements for each item.
- Rationale for the A5–A8 / B6–B9 additions: [competitor-research](./competitor-research.md) and [milestone-adjustments-cn](./milestone-adjustments-cn.md).
- Public repository: `README.md`, `ARCHITECTURE.md`, `SECURITY.md`, `docs/agents/`, `examples/`, `benchmarks/`, CI.

## 12. Items to Verify (not blocking Milestone A)

1. Stable API of the chosen MCP SDK and the configuration mechanism of current Claude Code / Codex versions.
2. Hook events Codex actually exposes and its transcript file format; build a capability matrix.
3. Retrieval quality and resource usage of local embedding models on short code-related sentences.
4. Whether a resident daemon is needed: A can rely on cross-process SQLite; evaluate under B's capture pressure and long-running tasks.
5. Whether semantic memory needs a separate Episode: the MVP does no complex lifecycle modeling and first observes real samples.

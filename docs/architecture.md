# Architecture (Milestone B)

chloe is a local-first memory service for coding agents. Everything runs on one machine; the only network traffic is to a local Ollama server, and only when extraction or embeddings are used.

## Data flow

```
agent hooks ──► spool ──► events ──► extract job ──► candidates ──► review / policy ──► memories
 (Claude, Codex)  (files)  (SQLite)   (Ollama LLM)    (pending)     (accept, merge,       │
                                                                      supersede)           ├─► FTS5 index
                                                                                           └─► embed job ──► vectors
agent ──► MCP (stdio) / CLI ──► MemoryService ──► hybrid_search: FTS top-50 + vector top-50 ──► RRF ──► hits
```

| Crate | Role |
|---|---|
| `memory-core` | IDs, domain types, errors, config and the traits (`MemoryService`, `EventStore`, `JobQueue`, `CandidateStore`, `EmbeddingStore`, `Embedder`, `MemoryExtractor`). No SQL, no MCP. |
| `memory-store` | SQLite implementation of all store traits: migrations, FTS5, revisions, job queue, embeddings. |
| `memory-ingest` | Hook payload parsing, secret redaction and the on-disk spool. |
| `memory-llm` | Ollama: JSON-schema extraction and `/api/embed` embeddings. |
| `memory-engine` | Consolidation planning, candidate proposals and policy, embedding backfill, hybrid search, evaluation. Knows only the traits. |
| `memory-mcp` | MCP over stdio; maps tools to the service and the engine. |
| `memory-cli` | The `chloe` binary. |

## Retrieval

- Both paths take the top 50 within the caller's project (plus global memories if asked) and the `active` status. Scope and status are hard filters inside each path.
- FTS: English stop words removed, tokens quoted and OR-ed (`bm25`, title weighted 5×).
- Vector: cosine similarity over the current revision's vector; the index is pinned to one model and dimension (default `qwen3-embedding:0.6b`, 1024).
- Fusion: reciprocal rank (k = 60), a vector rank weighted twice an FTS rank. Each hit reports `matched_by` and its rank in each path.
- If the vector path is unavailable (no provider, server down, no vectors, other model) the result is FTS only and says why. Only an FTS failure fails the search.

## Try it

```sh
chloe memory add --title "Use WAL" --content "SQLite WAL for concurrent readers" --kind decision
chloe embeddings backfill                       # needs `ollama pull qwen3-embedding:0.6b`
chloe memory search --hybrid --explain "how do we allow concurrent access"
chloe mcp serve --hybrid                        # what the agent's MCP config runs
chloe eval --runs 20                            # quality and latency report
```

Capture and review: `chloe hook install claude`, then `chloe consolidate` and `chloe candidates list`.

## Evidence

`docs/evidence/milestone-b/`: `eval/report.md` (retrieval quality and latency, model comparison), `b1-fixtures.md` (extraction fixtures), `b2-idempotency.md` (consolidation). The dataset is `eval/dataset.json`. CI runs the dataset integrity and FTS baseline tests; vector quality needs a model and is measured by hand with `chloe eval`.

## Known limitations

- **Chinese keyword search**: FTS uses `unicode61`, which does not segment CJK, so Chinese-only queries find nothing by keyword (Recall@5 0.00 in the report). Chinese is served by the vector path only until B3-07.
- **Hybrid is below Vector** on the current evaluation set (0.93 / 0.86 vs 1.00 / 0.94 Recall@5 / MRR). The set has no exact-identifier queries where keywords should win, so FTS cannot show its value; the fusion weight is a judgement, not a tuned optimum.
- **Evaluation set is small** (100 memories, 30 queries), labelled by the assistant and not reviewed by a person; a one-query difference is within noise.
- **Vectors are not created on save**: run `chloe embeddings backfill`. Until then new memories are found by FTS only.
- **Linear vector scan**: fine for thousands of memories, not for hundreds of thousands.
- **Query embedding adds about 25 ms** to each hybrid search; MCP uses a 5 s timeout and falls back to FTS.
- **Ollama over plain `http://` only**; remote servers need `--allow-remote` and send memory text off the machine.
- **Codex capture** is weaker than Claude Code's; the two agents do not have equivalent hook capability.
- **Demo video**: not recorded yet; the commands above are the demo script.

# Retrieval evaluation

- model: ollama:qwen3-embedding:0.6b (1024 dimensions)
- data: 50 memories, 25 queries
- host: macos/aarch64, 10 cpus (Apple M1 Max, Ollama local)

| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| en | fts | 6 | 1.00 | 0.87 | 0.4 | 0.7 |
| en | vector | 6 | 1.00 | 1.00 | 25.7 | 33.7 |
| en | hybrid | 6 | 1.00 | 1.00 | 23.9 | 29.3 |
| zh | fts | 6 | 0.00 | 0.00 | 0.2 | 0.2 |
| zh | vector | 6 | 1.00 | 1.00 | 20.0 | 22.9 |
| zh | hybrid | 6 | 1.00 | 1.00 | 20.7 | 31.8 |
| mixed | fts | 6 | 0.58 | 0.67 | 0.2 | 0.7 |
| mixed | vector | 6 | 1.00 | 1.00 | 26.5 | 40.4 |
| mixed | hybrid | 6 | 1.00 | 1.00 | 24.2 | 31.7 |
| cross | fts | 7 | 0.14 | 0.05 | 0.4 | 0.4 |
| cross | vector | 7 | 1.00 | 1.00 | 23.0 | 28.7 |
| cross | hybrid | 7 | 0.43 | 0.36 | 26.0 | 38.1 |
| all | fts | 25 | 0.42 | 0.38 | 0.3 | 0.7 |
| all | vector | 25 | 1.00 | 1.00 | 23.0 | 33.7 |
| all | hybrid | 25 | 0.84 | 0.82 | 24.9 | 31.8 |

## Reading the numbers

- Dataset: `dataset.json`, 50 memories (20 English, 18 Chinese, 12 Chinese with English terms) and 25 queries (6 en, 6 zh, 6 mixed, 7 cross-language), labelled by hand; drafted by the assistant, **not yet reviewed by a person**. Run with `chloe eval`; it uses a temporary database.
- Latency of Vector and Hybrid includes embedding the query with the local model (~20 ms); FTS is a SQLite query only. Single run, warm model, no repetition, so p95 over 6–7 queries is close to the maximum.
- **Vector is perfect on this set** (Recall@5 and MRR 1.00 everywhere). With 50 memories on distinct topics this is a ceiling, not evidence of a large margin; a bigger, harder set is needed before drawing conclusions.
- **FTS fails on Chinese** (0.00): known limitation, `unicode61` does not segment CJK (B3-07).
- **Hybrid is below Vector on cross-language queries** (Recall@5 0.43 vs 1.00) and overall (0.84 vs 1.00). Cause, checked on `how should credentials be handled in the repository` and `how do we release a new version`: FTS ORs every token with no stop-word removal, so words like `the`, `how`, `we`, `new` return 12–15 weak candidates; RRF gives them the same weight as the vector path and pushes the true vector #1 down. The vector path alone is the better ranker here.
- Fallback today: `chloe memory search` without `--hybrid` (FTS) and MCP without `--hybrid`; there is no vector-only mode exposed yet. Candidate fixes, not applied (to avoid tuning on the evaluation set): weight the vector path above FTS in RRF, drop stop words from the FTS query, or fuse FTS only when it returns few candidates.

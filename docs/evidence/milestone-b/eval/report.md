# Retrieval evaluation

- model: ollama:qwen3-embedding:0.6b (1024 dimensions)
- data: 100 memories, 30 queries
- host: macos/aarch64, 10 cpus (Apple M1 Max, Ollama local)

| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| en | fts | 7 | 0.71 | 0.66 | 0.6 | 1.0 |
| en | vector | 7 | 1.00 | 0.89 | 28.7 | 36.8 |
| en | hybrid | 7 | 0.86 | 0.81 | 27.1 | 33.1 |
| zh | fts | 7 | 0.00 | 0.00 | 0.2 | 0.3 |
| zh | vector | 7 | 1.00 | 1.00 | 24.5 | 31.4 |
| zh | hybrid | 7 | 1.00 | 1.00 | 24.2 | 33.5 |
| mixed | fts | 7 | 0.57 | 0.62 | 0.3 | 0.4 |
| mixed | vector | 7 | 1.00 | 1.00 | 25.4 | 29.0 |
| mixed | hybrid | 7 | 1.00 | 1.00 | 25.7 | 28.2 |
| cross | fts | 9 | 0.11 | 0.03 | 0.5 | 0.8 |
| cross | vector | 9 | 1.00 | 0.89 | 24.7 | 29.7 |
| cross | hybrid | 9 | 0.89 | 0.68 | 24.2 | 27.6 |
| all | fts | 30 | 0.33 | 0.31 | 0.3 | 0.8 |
| all | vector | 30 | 1.00 | 0.94 | 24.7 | 31.4 |
| all | hybrid | 30 | 0.93 | 0.86 | 24.6 | 33.1 |

## Embedding model comparison (same dataset, same run conditions)

Overall (30 queries), Recall@5 / MRR:

| model | vector | hybrid | cross-language vector | cross-language hybrid |
|---|---|---|---|---|
| `qwen3-embedding:0.6b` (default) | 1.00 / 0.94 | 0.93 / 0.86 | 1.00 / 0.89 | 0.89 / 0.68 |
| `bge-m3` | 0.98 / 0.92 | 0.95 / 0.82 | 1.00 / 0.79 | 0.89 / 0.60 |

Both are 1024-dimensional and answer in about 25–30 ms per query on this machine. `bge-m3` misses one mixed-language query in the top 5 and ranks cross-language answers lower; `qwen3-embedding:0.6b` is better on overall vector and cross-language ranking, while `bge-m3` is slightly better on English MRR (0.93 vs 0.89) and overall hybrid Recall@5 (0.95 vs 0.93); `qwen3-embedding:0.6b` stays the default. One `bge-m3` p95 of 133 ms (en, vector) looks like a one-off stall, not a trend; latency comparisons need repeated runs (B3-05). Differences of one query are within noise at this size.

## Reading the numbers

- Dataset: `dataset.json`, now 100 memories (45 English, 33 Chinese, 22 Chinese with English terms; about half are deliberate near-topic distractors) and 30 queries (7 en, 7 zh, 7 mixed, 9 cross-language), labelled by hand; drafted by the assistant, **not yet reviewed by a person**. Queries q01–q25 are unchanged from the first run; q26–q30 are new and paraphrased on purpose. Run with `chloe eval`; it uses a temporary database.
- Vector is no longer perfect (MRR 0.94), which is what the larger set was for.
- **FTS fails on Chinese** (0.00): known limitation, `unicode61` does not segment CJK (B3-07).
- **Hybrid is still below Vector** (0.93 / 0.86 vs 1.00 / 0.94). A sweep of the vector weight in the fusion on this set (1, 2, 3, 5, vector only) improves monotonically up to vector only (overall MRR 0.84, 0.86, 0.86, 0.87, 0.94). So on this dataset FTS adds nothing over Vector. That says little about real use: the queries are natural-language questions, and none depends on an exact identifier or error string, which is where keyword search is expected to win. `VECTOR_WEIGHT = 2` is kept as a modest default; raising it to chase this table would be tuning to a dataset that cannot show FTS's value. Next step is queries with exact identifiers (error codes, flag names), not another weight.
- Latency of Vector and Hybrid includes embedding the query (~20 ms); FTS is a SQLite query only. Single run, warm model.
- Underperforming a baseline is explained above; the fallback is `chloe memory search` / MCP without `--hybrid` (FTS only). There is no vector-only switch yet.

## History: first hybrid fix

First run (50 memories, 25 queries): Hybrid 0.84 / 0.82 overall, below Vector, because FTS ORs every token so function words returned 12–15 weak candidates. English stop words are now removed from the FTS query (0.96 / 0.92 on that set) and a vector rank counts twice an FTS rank (0.96 / 0.94).

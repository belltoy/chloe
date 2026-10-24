# Retrieval evaluation

- model: ollama:qwen3-embedding:0.6b (1024 dimensions)
- data: 50 memories, 25 queries
- host: macos/aarch64, 10 cpus (Apple M1 Max, Ollama local)

| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| en | fts | 6 | 1.00 | 0.87 | 0.5 | 0.6 |
| en | vector | 6 | 1.00 | 1.00 | 30.9 | 65.8 |
| en | hybrid | 6 | 1.00 | 1.00 | 32.3 | 38.7 |
| zh | fts | 6 | 0.00 | 0.00 | 0.2 | 0.2 |
| zh | vector | 6 | 1.00 | 1.00 | 22.3 | 33.4 |
| zh | hybrid | 6 | 1.00 | 1.00 | 21.7 | 40.7 |
| mixed | fts | 6 | 0.58 | 0.67 | 0.2 | 0.3 |
| mixed | vector | 6 | 1.00 | 1.00 | 22.8 | 28.7 |
| mixed | hybrid | 6 | 1.00 | 1.00 | 23.3 | 27.3 |
| cross | fts | 7 | 0.14 | 0.05 | 0.4 | 0.8 |
| cross | vector | 7 | 1.00 | 1.00 | 25.0 | 29.2 |
| cross | hybrid | 7 | 0.86 | 0.78 | 25.8 | 34.6 |
| all | fts | 25 | 0.42 | 0.38 | 0.3 | 0.6 |
| all | vector | 25 | 1.00 | 1.00 | 23.5 | 42.1 |
| all | hybrid | 25 | 0.96 | 0.94 | 25.6 | 38.7 |

## Reading the numbers

- Dataset: `dataset.json`, 50 memories (20 English, 18 Chinese, 12 Chinese with English terms) and 25 queries (6 en, 6 zh, 6 mixed, 7 cross-language), labelled by hand; drafted by the assistant, **not yet reviewed by a person**. Run with `chloe eval`; it uses a temporary database.
- Latency of Vector and Hybrid includes embedding the query with the local model (~20 ms); FTS is a SQLite query only. Single run, warm model, no repetition, so p95 over 6–7 queries is close to the maximum.
- **Vector is perfect on this set.** With 50 memories on distinct topics this is a ceiling, not evidence of a large margin; a bigger, harder set is needed before drawing conclusions.
- **FTS fails on Chinese** (0.00): known limitation, `unicode61` does not segment CJK (B3-07).

## Hybrid fix: FTS noise (first run vs now)

First run: Hybrid Recall@5 / MRR was 0.84 / 0.82 overall and 0.43 / 0.36 on cross-language queries, below Vector. Cause: FTS ORs every query token, so function words (`the`, `how`, `we`) returned 12–15 weak candidates, and RRF gave them the same weight as the vector path.

Two changes in `hybrid_search`, both from that diagnosis rather than searched for on the dataset:
1. English stop words are removed from the FTS query (kept as is if only stop words remain). Cross Recall@5 / MRR: 0.43 / 0.36 → 0.86 / 0.71; overall 0.84 / 0.82 → 0.96 / 0.92.
2. A vector rank counts twice an FTS rank in the fusion (`VECTOR_WEIGHT = 2`). Only a small further gain here (cross MRR 0.71 → 0.78, overall MRR 0.92 → 0.94). The factor 2 is a judgement, not a tuned optimum.

Still below Vector (0.96 vs 1.00 overall): two cross-language queries (`how do we release a new version`, `how long does the session cache live before login expires`) match unrelated memories on real content words (`new`, `version`, `session`). That is ordinary keyword noise, not something stop words fix.

Both changes apply wherever `hybrid_search` runs, including `chloe mcp serve` without `--hybrid` (FTS only, now without stop words). The plain `chloe memory search` is unchanged. Re-check on a larger set (B3-05) before treating 2 as final.

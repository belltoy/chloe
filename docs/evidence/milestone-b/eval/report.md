# Retrieval evaluation

- model: ollama:qwen3-embedding:0.6b (1024 dimensions)
- data: 100 memories, 30 queries, 20 timed run(s) each
- host: macos/aarch64, 10 cpus (Apple M1 Max, Ollama local, nothing else running)

| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| en | fts | 7 | 0.71 | 0.66 | 0.5 | 1.6 |
| en | vector | 7 | 1.00 | 0.89 | 25.7 | 34.3 |
| en | hybrid | 7 | 0.86 | 0.81 | 25.8 | 34.4 |
| zh | fts | 7 | 0.71 | 0.64 | 0.3 | 0.5 |
| zh | vector | 7 | 1.00 | 1.00 | 24.6 | 29.1 |
| zh | hybrid | 7 | 1.00 | 1.00 | 25.2 | 31.9 |
| mixed | fts | 7 | 0.71 | 0.75 | 0.4 | 0.6 |
| mixed | vector | 7 | 1.00 | 1.00 | 25.6 | 31.5 |
| mixed | hybrid | 7 | 0.93 | 1.00 | 25.3 | 30.0 |
| cross | fts | 9 | 0.11 | 0.03 | 0.5 | 0.7 |
| cross | vector | 9 | 1.00 | 0.89 | 25.6 | 31.9 |
| cross | hybrid | 9 | 0.89 | 0.61 | 26.3 | 30.5 |
| all | fts | 30 | 0.53 | 0.49 | 0.4 | 0.8 |
| all | vector | 30 | 1.00 | 0.94 | 25.4 | 32.7 |
| all | hybrid | 30 | 0.92 | 0.84 | 25.6 | 32.3 |


## Embedding model comparison (same dataset, same run conditions)

Measured before B3-07: the FTS and Hybrid numbers in this section and in "Repeated run" use the old Chinese-blind FTS. The table at the top and the B3-07 section are current; the model ranking is about the vector path and does not depend on that.

Overall (30 queries), Recall@5 / MRR:

| model | vector | hybrid | cross-language vector | cross-language hybrid |
|---|---|---|---|---|
| `qwen3-embedding:0.6b` (default) | 1.00 / 0.94 | 0.93 / 0.86 | 1.00 / 0.89 | 0.89 / 0.68 |
| `bge-m3` | 0.98 / 0.92 | 0.95 / 0.82 | 1.00 / 0.79 | 0.89 / 0.60 |

Both are 1024-dimensional and answer in about 25–30 ms per query on this machine. `bge-m3` misses one mixed-language query in the top 5 and ranks cross-language answers lower; `qwen3-embedding:0.6b` is better on overall vector and cross-language ranking, while `bge-m3` is slightly better on English MRR (0.93 vs 0.89) and overall hybrid Recall@5 (0.95 vs 0.93); `qwen3-embedding:0.6b` stays the default. The 133 ms p95 `bge-m3` showed in the single run was a one-off stall: with a warm-up call and 20 timed runs per query (below) it is gone. Differences of one query are within noise at this size.

## Reading the numbers

- Dataset: `dataset.json`, now 100 memories (45 English, 33 Chinese, 22 Chinese with English terms; about half are deliberate near-topic distractors) and 30 queries (7 en, 7 zh, 7 mixed, 9 cross-language), labelled by hand; drafted by the assistant, **not yet reviewed by a person**. Queries q01–q25 are unchanged from the first run; q26–q30 are new and paraphrased on purpose. Run with `chloe eval`; it uses a temporary database.
- Vector is no longer perfect (MRR 0.94), which is what the larger set was for.
- **Chinese keyword search** was 0.00 before B3-07; see the section below.
- **Hybrid is still below Vector** (0.93 / 0.86 vs 1.00 / 0.94). A sweep of the vector weight in the fusion on this set (1, 2, 3, 5, vector only) improves monotonically up to vector only (overall MRR 0.84, 0.86, 0.86, 0.87, 0.94). So on this dataset FTS adds nothing over Vector. That says little about real use: the queries are natural-language questions, and none depends on an exact identifier or error string, which is where keyword search is expected to win. `VECTOR_WEIGHT = 2` is kept as a modest default; raising it to chase this table would be tuning to a dataset that cannot show FTS's value. Next step is queries with exact identifiers (error codes, flag names), not another weight.
- Latency of Vector and Hybrid includes embedding the query (~20 ms); FTS is a SQLite query only. Single run, warm model.
- Underperforming a baseline is explained above; the fallback is `chloe memory search` / MCP without `--hybrid` (FTS only). There is no vector-only switch yet.

## History: first hybrid fix

First run (50 memories, 25 queries): Hybrid 0.84 / 0.82 overall, below Vector, because FTS ORs every token so function words returned 12–15 weak candidates. English stop words are now removed from the FTS query (0.96 / 0.92 on that set) and a vector rank counts twice an FTS rank (0.96 / 0.94).

## Repeated run (B3-05)

`chloe eval --runs 20` (one untimed warm-up embedding, then 20 timed runs per query and mode; 600 samples per row), same machine (Apple M1 Max, 10 cores, local Ollama, nothing else running). Overall:

| model | mode | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|
| `qwen3-embedding:0.6b` | fts | 0.33 | 0.31 | 0.2 | 0.6 |
| `qwen3-embedding:0.6b` | vector | 1.00 | 0.94 | 24.8 | 31.1 |
| `qwen3-embedding:0.6b` | hybrid | 0.93 | 0.86 | 25.2 | 32.5 |
| `bge-m3` | fts | 0.33 | 0.31 | 0.2 | 0.7 |
| `bge-m3` | vector | 0.98 | 0.92 | 27.0 | 34.1 |
| `bge-m3` | hybrid | 0.95 | 0.82 | 27.3 | 34.2 |

Quality does not change between runs (it is computed once per query). Vector and Hybrid latency is almost entirely the query embedding (about 25 ms); the vector scan over 100 memories and the fusion are well under a millisecond. This is a single machine and a single session, not a controlled lab: no pinned CPU frequency, no cold-cache runs, and the Ollama build was whatever was installed.

## B3-07: Chinese keyword search

Chosen: a second FTS5 index, `memory_fts_cjk`, with the `trigram` tokenizer, used only for queries that contain CJK characters (migration 010 builds it from `memories`, triggers keep it current, `doctor --reindex` rebuilds both). The query side cuts each run of CJK characters into every overlapping 3-character piece and ORs them, plus any 3+ character run of letters or digits, so a whole Chinese sentence matches memories that share any 3 consecutive characters, ranked by `bm25`. `memory_fts` and English-only queries are untouched.

Why not the alternatives named in the plan: replacing `unicode61` by `trigram` everywhere would change English matching (substring matches, and tokens under 3 characters such as `ci` or `db` stop matching). A `LIKE` fallback when FTS finds nothing needs the query to be a substring of a memory, which a natural-language sentence almost never is; this was reasoned, not measured. Only the trigram index was implemented and measured.

Same dataset and model, first column from the previous run (`unicode61`), Recall@5 / MRR:

| group | FTS before | FTS now | Hybrid before | Hybrid now |
|---|---|---|---|---|
| zh | 0.00 / 0.00 | 0.71 / 0.64 | 1.00 / 1.00 | 1.00 / 1.00 |
| mixed | 0.57 / 0.62 | 0.71 / 0.75 | 1.00 / 1.00 | 0.93 / 1.00 |
| cross | 0.11 / 0.03 | 0.11 / 0.03 | 0.89 / 0.68 | 0.89 / 0.61 |
| all | 0.33 / 0.31 | 0.53 / 0.49 | 0.93 / 0.86 | 0.92 / 0.84 |

- Chinese-only FTS goes from nothing to 0.71. Hybrid does not gain: Vector already answered every Chinese query, and the extra keyword candidates cost one mixed query a second relevant memory in the top 5 (0.93) and a little cross-language MRR. That is one query, within noise, but it is not an improvement.
- Limits: a CJK word of one or two characters is not searchable by keyword (such a query falls back to the regular index); a query with CJK text drops its Latin tokens shorter than 3 characters; Japanese and Korean are covered by the same trigram rule but not evaluated; the trigram index is larger than the regular one.

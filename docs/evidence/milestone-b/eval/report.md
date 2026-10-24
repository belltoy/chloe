# Retrieval evaluation

- model: ollama:qwen3-embedding:0.6b (1024 dimensions)
- data: 100 memories, 30 queries, 20 timed run(s) each
- host: macos/aarch64, 10 cpus (Apple M1 Max, Ollama local, nothing else running)

| lang | mode | n | Recall@5 | MRR | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| en | fts | 7 | 0.71 | 0.66 | 0.5 | 2.3 |
| en | vector | 7 | 1.00 | 0.89 | 18.7 | 29.2 |
| en | hybrid | 7 | 0.86 | 0.81 | 18.7 | 26.6 |
| zh | fts | 7 | 1.00 | 1.00 | 0.3 | 0.9 |
| zh | vector | 7 | 1.00 | 1.00 | 17.1 | 24.0 |
| zh | hybrid | 7 | 1.00 | 1.00 | 18.0 | 25.3 |
| mixed | fts | 7 | 0.71 | 0.86 | 0.1 | 1.4 |
| mixed | vector | 7 | 1.00 | 1.00 | 18.7 | 24.9 |
| mixed | hybrid | 7 | 1.00 | 1.00 | 19.9 | 25.8 |
| cross | fts | 9 | 0.11 | 0.02 | 0.3 | 1.4 |
| cross | vector | 9 | 1.00 | 0.89 | 18.2 | 25.9 |
| cross | hybrid | 9 | 0.78 | 0.48 | 19.6 | 30.4 |
| all | fts | 30 | 0.60 | 0.59 | 0.3 | 1.4 |
| all | vector | 30 | 1.00 | 0.94 | 18.0 | 27.1 |
| all | hybrid | 30 | 0.90 | 0.80 | 19.0 | 28.2 |


## Embedding model comparison (same dataset, same run conditions)

Measured before B3-07: the FTS and Hybrid numbers in this section and in "Repeated run" use the old Chinese-blind FTS (latency was also measured before the later runs, which show ~18 ms per query). The table at the top and the B3-07 section are current; the model ranking is about the vector path and does not depend on that.

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

Chosen: application-level word segmentation with `jieba-rs` (default dictionary). A second FTS5 index, `memory_fts_seg` (`unicode61` over text already cut into words), is used only for queries that contain CJK characters; `memory_fts` and English-only queries are untouched. The application writes the index in the same transaction as the memory (a trigger cannot segment); `fts_meta` records the segmentation version and the index is rebuilt on open when it changes, which also builds it for memories saved before the upgrade (migration 011). `doctor` checks that the index has one row per memory and the current version, and `doctor --reindex` rebuilds it.

- Index side: `cut_for_search` (fine-grained words plus compounds, so `busy_timeout` is indexed as `busy`, `timeout` and `busy_timeout`). Query side: `cut`, then punctuation and stop words removed, OR-ed and ranked by `bm25`.
- Stop words (particles, question words, pronouns and a few generic words such as 问题, 使用, 通过) apply to the query only, so editing the list never needs a reindex. Negations (不, 没, 不能) are kept on purpose. The list was extended once after looking at which cross-language queries failed, so it is mildly fitted to this set.
- A first attempt used a `trigram` tokenizer (migration 010, never released, dropped by migration 011). It could not find one- and two-character words and gave less than jieba.

Recall@5 / MRR on the same dataset and model:

| group | FTS, unicode61 (before) | FTS, trigram | FTS, jieba | Hybrid before | Hybrid, trigram | Hybrid, jieba |
|---|---|---|---|---|---|---|
| zh | 0.00 / 0.00 | 0.71 / 0.64 | 1.00 / 1.00 | 1.00 / 1.00 | 1.00 / 1.00 | 1.00 / 1.00 |
| mixed | 0.57 / 0.62 | 0.71 / 0.75 | 0.71 / 0.86 | 1.00 / 1.00 | 0.93 / 1.00 | 1.00 / 1.00 |
| cross | 0.11 / 0.03 | 0.11 / 0.03 | 0.11 / 0.02 | 0.89 / 0.68 | 0.89 / 0.61 | 0.78 / 0.48 |
| all | 0.33 / 0.31 | 0.53 / 0.49 | 0.60 / 0.59 | 0.93 / 0.86 | 0.92 / 0.84 | 0.90 / 0.80 |

- **Chinese keyword search now works**: zh 0.00 to 1.00, overall FTS 0.33 to 0.60.
- **Hybrid got slightly worse overall (0.93 / 0.86 to 0.90 / 0.80), entirely in cross-language queries.** A Chinese query against an English memory has no true keyword match, but Chinese distractors now match on shared words (for example 网络, 服务, 数据库 appear in many memories), rank high in FTS, and the fusion lets them overtake the vector's correct first answer. Before B3-07 these queries had no FTS hits at all, so the weakness of the fusion was hidden. Vector alone is unaffected (1.00 / 0.94). Fusion changes (a minimum number of matched terms, or a weaker FTS weight when it disagrees strongly with the vector path) are the next thing to try; they were not tried here.
- Cost: the release binary grows from 6.4 MB to 9.4 MB; the dictionary loads on first use of a CJK query or write, about 70 ms; English-only commands and hook capture do not load it.
- Limits: Japanese and Korean go through the same segmenter but are not evaluated; segmentation of technical terms is whatever the default dictionary does; the repeated 20-run benchmark above was taken with the segmented index.

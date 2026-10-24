# Fusion diagnosis after B3-07 (baseline, not a result)

Produced by `chloe eval --diagnose` on `dataset.json` (100 memories, 30 queries), `qwen3-embedding:0.6b`, hybrid as of commit `5d44096` (jieba FTS, English stop words removed, vector rank weighted 2x). No algorithm was changed to produce it. Columns: `fts#`/`vec#` are the ranks in each path's top 50 (`-` = not returned), `cos` the cosine similarity, `RRF` the fused score, `matched terms` the FTS query terms that memory contains.

What it shows (8 of 30 queries have a non-relevant first hit):

- **In all 8, the winner was in both candidate lists.** None won through FTS alone or vector alone. The wrong memory sits in the vector top 2 to 30 and in the FTS top 1 to 4, and the two rank bonuses add up.
- **In all 8, no relevant memory is in the FTS candidates.** These are cross-language or paraphrased queries; FTS has nothing right to offer, so every FTS candidate is noise.
- **Every winner matched exactly one query term** (of 3 to 7: `rejected`, `监听`, `关键词`, `session`, `version`, `same`, `记忆`, `session`). A single-term match is weak evidence, which is what a coverage gate would test.
- **The right memory's cosine is higher than the winner's in 7 of 8, but the margin is small in several** (q03 0.65 vs 0.51, q19 0.70 vs 0.49, q23 0.75 vs 0.51, q25 0.60 vs 0.52, q26 0.48 vs 0.43, q20 0.54 vs 0.53, q27 0.57 vs 0.56) and reversed once (q29 0.61 vs 0.68). Rank-only fusion ignores the gap; a score-aware rule could use it where it is large (q19, q23) but not where it is 0.01.
- A relevant memory is the vector's first hit in 5 of the 8; in the other 3 the vector path itself is wrong or ambiguous (q20, q26, q29; vector rank 2, 4 and 2), so a fusion rule can only help if it lifts the right memory over a wrong vector first hit.
- Two terms the query side keeps are weak: `会出` (a bad cut of 会出问题) and generic Latin words such as `before`, `long`, `live`. This is a candidate-quality issue on the FTS side, separate from the fusion arithmetic.

This supports the hypothesis that noisy but not-quite-wrong FTS candidates overtake the right vector answer by accumulating two rank bonuses, and it does not support a single-path failure. It does not say which remedy works; that needs the held-out set.

## Per-query detail: queries where the first hybrid hit is not relevant

## q03 [en] why does a stale update get rejected

FTS terms (4): stale update get rejected

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| e05 | Optimistic concurrency for updates | - | 1 | 0.65 | 2 | 0/4  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | e38 | Candidate states | 1 | 6 | 0.51 | 0.0467 | 1/4 rejected | no |
| 2 | e05 | Optimistic concurrency for updates | - | 1 | 0.65 | 0.0328 | 0/4  | yes |
| 3 | e31 | Status values | - | 2 | 0.54 | 0.0323 | 0/4  | no |
| 4 | z16 | 依赖升级 | - | 3 | 0.53 | 0.0317 | 0/4  | no |
| 5 | e07 | Cross-project reads leak existence | - | 4 | 0.52 | 0.0312 | 0/4  | no |

## q19 [cross] 不能让服务监听网络端口吗

FTS terms (5): 不能 服务 监听 网络 端口

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| e02 | No network listeners by default | - | 1 | 0.70 | 5 | 0/5  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | x14 | Ollama 默认地址 | 2 | 2 | 0.49 | 0.0484 | 1/5 监听 | no |
| 2 | x08 | JSON schema 校验 | 3 | 15 | 0.37 | 0.0425 | 1/5 不能 | no |
| 3 | z12 | 测试数据隔离 | 4 | 19 | 0.36 | 0.0409 | 1/5 不能 | no |
| 4 | x09 | Kubernetes 探针 | 1 | 30 | 0.34 | 0.0386 | 1/5 服务 | no |
| 5 | e02 | No network listeners by default | - | 1 | 0.70 | 0.0328 | 0/5  | yes |

## q20 [cross] 搜索关键词里有特殊符号会出问题吗

FTS terms (4): 搜索 关键词 特殊符号 会出

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| e03 | FTS5 query syntax injection | - | 2 | 0.54 | 3 | 0/4  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | z02 | 中文关键词只能整段匹配 | 1 | 3 | 0.53 | 0.0481 | 1/4 关键词 | no |
| 2 | x16 | Unicode 规范化 | - | 1 | 0.55 | 0.0328 | 0/4  | no |
| 3 | e03 | FTS5 query syntax injection | - | 2 | 0.54 | 0.0323 | 0/4  | yes |
| 4 | e27 | Large pasted prompts | - | 4 | 0.45 | 0.0312 | 0/4  | no |
| 5 | z29 | 合并去重 | - | 5 | 0.45 | 0.0308 | 0/4  | no |

## q23 [cross] how long does the session cache live before login expires

FTS terms (7): long session cache live before login expires

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| x05 | Redis 会话缓存 | - | 1 | 0.75 | 7 | 0/7  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | e14 | Resumed sessions are not extracted again | 4 | 2 | 0.51 | 0.0479 | 1/7 session | no |
| 2 | x19 | SQLite 页缓存 | 1 | 4 | 0.50 | 0.0476 | 1/7 cache | no |
| 3 | e11 | Redact secrets before storing events | 2 | 14 | 0.43 | 0.0432 | 1/7 before | no |
| 4 | e27 | Large pasted prompts | 6 | 12 | 0.44 | 0.0429 | 1/7 before | no |
| 5 | e37 | Windows line endings | 5 | 19 | 0.41 | 0.0407 | 1/7 before | no |

## q25 [cross] how do we release a new version

FTS terms (3): release new version

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| z04 | 发布流程 | - | 1 | 0.60 | 4 | 0/3  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | e23 | Schema version numbering | 1 | 3 | 0.52 | 0.0481 | 1/3 version | no |
| 2 | e18 | Supersession is proposed, not automatic | 3 | 18 | 0.39 | 0.0415 | 1/3 new | no |
| 3 | e19 | Add a new schema migration | 2 | 21 | 0.39 | 0.0408 | 1/3 new | no |
| 4 | z04 | 发布流程 | - | 1 | 0.60 | 0.0328 | 0/3  | yes |
| 5 | z16 | 依赖升级 | - | 2 | 0.56 | 0.0323 | 0/3  | no |

## q26 [en] what happens if two writers collide on the same record

FTS terms (6): happens two writers collide same record

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| e05 | Optimistic concurrency for updates | - | 4 | 0.48 | 6 | 0/6  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | e44 | Idempotent import | 1 | 14 | 0.43 | 0.0434 | 1/6 same | no |
| 2 | e01 | Use SQLite WAL mode | 2 | 18 | 0.41 | 0.0418 | 1/6 same | no |
| 3 | z29 | 合并去重 | - | 1 | 0.54 | 0.0328 | 0/6  | no |
| 4 | e27 | Large pasted prompts | - | 2 | 0.49 | 0.0323 | 0/6  | no |
| 5 | e18 | Supersession is proposed, not automatic | - | 3 | 0.49 | 0.0317 | 0/6  | no |

## q27 [cross] 怎样防止别人通过编号猜出别的项目里有没有某条记忆

FTS terms (6): 防止 别人 编号 猜 项目 记忆

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| e07 | Cross-project reads leak existence | - | 1 | 0.57 | 6 | 0/6  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | z01 | 评测集固定不变 | 2 | 2 | 0.56 | 0.0484 | 1/6 记忆 | no |
| 2 | z27 | 审计记录 | 4 | 3 | 0.55 | 0.0474 | 1/6 记忆 | no |
| 3 | x04 | Git worktree 项目识别 | 1 | 6 | 0.49 | 0.0467 | 2/6 项目 记忆 | no |
| 4 | z31 | 状态取值 | 3 | 8 | 0.48 | 0.0453 | 1/6 记忆 | no |
| 5 | z18 | 性能基线 | 5 | 15 | 0.44 | 0.0421 | 1/6 记忆 | no |

## q29 [cross] which parts of a session do we keep

FTS terms (3): parts session keep

| expected | title | fts# | vec# | cos | hybrid# | matched terms |
|---|---|---|---|---|---|---|
| z17 | 会话采集范围 | - | 2 | 0.61 | 4 | 0/3  |

| hybrid# | key | title | fts# | vec# | cos | RRF | matched terms | relevant |
|---|---|---|---|---|---|---|---|---|
| 1 | e14 | Resumed sessions are not extracted again | 3 | 1 | 0.68 | 0.0487 | 1/3 session | no |
| 2 | e27 | Large pasted prompts | 2 | 5 | 0.54 | 0.0469 | 1/3 keep | no |
| 3 | e30 | Embedding request size | 1 | 48 | 0.36 | 0.0349 | 1/3 keep | no |
| 4 | z17 | 会话采集范围 | - | 2 | 0.61 | 0.0323 | 0/3  | yes |
| 5 | x05 | Redis 会话缓存 | - | 3 | 0.58 | 0.0317 | 0/3  | no |

## Summary

- failing queries: 8 of 30
- winner found by both paths: 8; by FTS only: 0; by vector only: 0
- no relevant memory in the FTS candidates: 8
- a relevant memory is the vector's first hit: 5


# Hybrid fusion: single-term FTS gate

Problem (see `diagnosis-b3-07.md`): in 8 of 30 development queries the wrong memory won because it matched exactly one query term in FTS, and an FTS rank outweighed the vector path's better judgement.

Candidate (decided before looking at the held-out set): with several query terms, an FTS candidate takes part in fusion only if it matches at least two of them, or one that looks like an identifier (contains a digit, `_` or `:`; so `E1042`, `certificate_expired`, `gen_server:call` keep their bonus). Only applied when the vector path is available. A plainer variant without the identifier exemption (`cov2`) was run too.

Protocol: held-out labels frozen first (`holdout-review.md`, revised once after an AI audit, no human review); each variant run once on the held-out set; nothing tuned on it. Merge condition: beat the current Hybrid on the held-out set. Single run, qwen3-embedding:0.6b; with 14 and 15 queries per group, a one-query difference is noise.

Recall@5 / MRR, all queries:

| set | Vector | Hybrid before | cov2 | cov2 + identifier exemption (shipped) |
|---|---|---|---|---|
| development (30) | 1.00 / 0.94 | 0.90 / 0.80 | 1.00 / 0.93 | 1.00 / 0.93 |
| held-out (50) | 0.98 / 0.98 | 0.96 / 0.87 | 1.00 / 0.94 | 1.00 / 0.95 |

Held-out by group, shipped variant (before): en 1.00 / 1.00 (1.00 / 1.00), zh 1.00 / 1.00 (1.00 / 1.00), mixed 1.00 / 1.00 (1.00 / 1.00), cross 1.00 / 0.84 (0.87 / 0.57). Vector alone: cross 1.00 / 1.00, en 0.93 / 0.93.

What remains: Hybrid still trails Vector on cross-language MRR (0.84 vs 1.00 held-out, 0.83 vs 0.89 development). The held-out set was looked at once per variant, so it is not used to tune this further. The shipped variant was chosen over `cov2` for the single-term identifier evidence it keeps, not for the one-query difference in the score.

## After the held-out label review

The labels were reviewed (`review-holdout.md`): h17 gained e11 and x17, h27 gained z12. Same code and model, both label versions run now (`chloe eval --dataset holdout.json`, one run):

| held-out, all queries | old labels, Recall@5 / MRR | reviewed labels |
|---|---|---|
| FTS | 0.77 / 0.72 | 0.76 / 0.72 |
| Vector | 0.98 / 0.98 | 0.97 / 0.98 |
| Hybrid | 1.00 / 0.95 | 0.99 / 0.95 |

Only the zh group moved (Recall@5 1.00 to 0.97 for Vector and Hybrid; the added memories for h17 are not all in the top 5). MRR and the cross-language numbers are unchanged, so the conclusion above stands: Hybrid still trails Vector on cross-language MRR (0.84 vs 1.00). The earlier table in this file was measured on the old labels and an earlier build.

# B-16 phase 1 targets

Phase 1 is the first improvement round on the hand-made set (`sessions.json`, 20 sessions, `qwen3:8b`). The numbers are goals for this set, not guarantees for production. They are not changed because of a single run.

## Hard lines (zero on the acceptance set)

These errors can pollute long-term memory, so the acceptance set must show none. Zero here is a requirement of this acceptance, not a claim that production never errs. The original B-16 rule, "serious factual errors never automatically become Active", is verified separately.

| Measure | Definition | Now | Target | Status |
|---|---|---|---|---|
| Hallucination | Candidate content has no support in the session, or a number, negation or scope is altered. Judged by a person over all candidates, including TRAP and UNEXPECTED ones | 0 among the 17 reviewed extras; the other 21 candidates are not reviewed | 0 | **Review pending** (17 of 38 covered) |
| Constraint/Decision false extraction | A constraint or decision that records an environment workaround, an assistant preference, or an abandoned approach | 2 (t01 cache workaround, t05 `anyhow` preference) | 0 | **Not met** |

## Soft goals

| Measure | Definition | Now | Phase 1 goal | Status |
|---|---|---|---|---|
| Recall | Expected items found / expected items | 11/12 on the frozen original labels | ≥ 80% | Met |
| Content-valid rate (including duplicates) | (candidates matching a label + valid extras + duplicate extras) / all candidates. Duplicates are correct but still cost review time, and a valid candidate is not necessarily ready to accept | 18/38 = 47.4% (provisional: the 12 label matches were not re-reviewed) | ≥ 60% | **Not met** |

## Reported without a goal

| Measure | Now | Note |
|---|---|---|
| Strict valid rate (valuable only) | 15/38 = 39.5%, provisional | The 12 label matches may contain duplicates; no uniform re-review yet. Set a gate later |
| Silence on no-knowledge sessions | 4/8 | Reference only this phase. Fix the false rules and candidate noise first, then set a gate on a larger no-knowledge sample |
| Recall after the u07 label | 12/13 | Recomputed after a label was added from this run's output (post-hoc). Not independent validation; the frozen set stays 11/12 |

## Resolution of one metric

One item moves the number by: a candidate ≈ 2.6 points (1/38), a recall item ≈ 8.3 points (1/12), a no-knowledge session ≈ 12.5 points (1/8).

## Status

Two measures are clearly not met (Constraint/Decision false extraction, content-valid rate), the hallucination line is pending a full review, recall is met.

## To turn goals into gates

A second reviewer judging the same candidates, about 40 sessions, at least one more model, and a uniform review of the 12 label matches.

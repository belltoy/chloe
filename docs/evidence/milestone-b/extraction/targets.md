# B-16 phase 1 targets

Phase 1 is the first improvement round on the hand-made set (`sessions.json`, 20 sessions, `qwen3:8b`). The numbers are goals for this set, not guarantees for production. They are not changed because of a single run.

## Hard lines (zero on the acceptance set)

These errors can pollute long-term memory, so the acceptance set must show none. Zero here is a requirement of this acceptance, not a claim that production never errs. The original B-16 rule, "serious factual errors never automatically become Active", is verified separately (done, below).

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

About 40 sessions, at least one more model, and a uniform review of the 12 label matches and the other candidates not yet reviewed. The verdicts on the 17 extras in `review.md` were already checked by two reviewers (reported by the user; the second reviewer's notes are not in the repo). The session labels in `sessions.json` (`expect`, `traps`) are not covered by that.

## "Serious errors never become Active on their own" (checked)

- Only `resolve_candidate` with `Accept` or `Supersede` creates a memory from a candidate. Its only callers are `chloe candidates accept` and `chloe candidates supersede` (a person at the CLI); MCP has no candidate tools.
- The automatic step, `apply_policy`, can only merge into an existing memory (adds a source, no new Active memory) or flag a conflict; creating or superseding is never automatic (`memory-engine/tests/policy.rs`: `creating_a_memory_is_never_automatic`, `superseding_is_never_applied_automatically`, constraints and decisions never auto-merged).
- New end-to-end assertion in `memory-cli/tests/consolidate.rs`: after a full `consolidate` that produces a decision candidate, the project and global memory lists are empty and the candidate is `pending`.
- Limit: this shows nothing is promoted without a person. It does not make a wrong candidate less wrong when a person accepts it; that is the hard line above.

## Phase 1, round 1 (prompt rules + "constraint/decision must cite a user prompt")

Changes: `memory-llm/src/prompt.rs` (constraints/decisions need the user; environment workarounds and reverted attempts are not knowledge; do not record what the agent did or what happened this time) and `memory-llm/src/validate.rs` (reject a constraint or decision whose evidence has no `user_prompt` event, reason `unstated_rule`; test in `memory-llm/tests/validate.rs`).

Before changing anything I added 4 fresh sessions (t06, t07, u11, u12; 24 in total) so the fix is not judged only on the sessions that motivated it. Same model, same set, before and after (`report-24-before.md`, `report-24-after.md`):

| Measure | Before | After |
|---|---|---|
| Expected items found | 14 / 15 | 14 / 15 (same miss: u04, filed as decision) |
| Candidates | 47 | 35 |
| False (trap or in a no-knowledge session) | 14 (trap matches 7) | 8 (trap matches 5) |
| No-knowledge sessions left empty | 4 / 10 | 7 / 10 |
| Unlabelled extras (not reviewed by a person) | 18 | 13 |

- Constraint false extraction: t01 (cache workaround) and t05 (assistant `anyhow` preference) are gone. Hard line on constraints: no constraint false extraction left in this set.
- Hard line on decisions is still **not met**: two decisions come from a user prompt that only asked for an edit (n02 "rename tmp to buffer", t07 "add /health without versioning"). The structural rule cannot catch these because the user prompt really is cited.
- The 2 remaining trap matches in t02 are the known false alarm of the label (a faithful record of a user-stated reason), so real false decisions are n02 and t07.
- t06 still produces a gotcha and a procedure about the sandbox cache. They are not constraints or decisions, so they count under content noise, not under the hard line.
- Not computed: content-valid rate and hallucination review for the new run. The 13 extras and the new candidates need the same person review as `review.md`.
- Caveat: t01, t05 motivated the change, so their improvement is not independent evidence. t06 and t07 were written before the change and show the rule helps on constraints and some decisions, but t06/t07 outputs were seen at baseline before the fix was written.

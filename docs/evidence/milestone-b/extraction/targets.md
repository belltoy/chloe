# B-16 phase 1 targets

Phase 1 is the first improvement round on the hand-made set (`sessions.json`, 20 sessions, `qwen3:8b`). The numbers are goals for this set, not guarantees for production. They are not changed because of a single run.

## Hard lines (zero on the acceptance set)

These errors can pollute long-term memory, so the acceptance set must show none. Zero here is a requirement of this acceptance, not a claim that production never errs. The original B-16 rule, "serious factual errors never automatically become Active", is verified separately (done, below).

| Measure | Definition | Now | Target | Status |
|---|---|---|---|---|
| Hallucination | Candidate content has no support in the session, or a number, negation or scope is altered. Judged by a person over all candidates, including TRAP and UNEXPECTED ones | first baseline (original prompt, 38 candidates, all reviewed): 8 partly unsupported (about 21%), 0 fully unfaithful; current prompt (28 sessions, 37 candidates, two reviewers, `review-current.md`): 13 of 37 partly or wholly unsupported (35.1%) after the three strictness rulings | 0 | **Not met** on both (21.1% and 35.1%) |
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

Reported per prompt version. First baseline: hallucination line not met (8/38 partly unsupported, see "Hallucination review of the first baseline, completed" below), Constraint/Decision false extraction not met, content-valid rate not met, recall met. Current prompt: Constraint false extraction 0, Decision false extraction 1 (accepted residue), recall 16/16 on the set the prompt was tuned on; hallucination and content-valid rate not yet reviewed.

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

## Phase 1, round 2 (decisions that are only a request)

Cause: a user prompt that merely asks for an edit ("rename tmp to buffer", "bump the version") was written up as "the user decided ...". The user-prompt guard cannot catch it because a user prompt really is cited.

Before changing the prompt I added 4 fresh sessions (n06 version bump, n07 add a test, n08 colour change, u13 a stated choice with a reason; 28 in total). At baseline (`report-28-before.md`) five decisions came from plain requests: n02, n06, n07, n08, t07.

Prompt changes (`memory-llm/src/prompt.rs`), three attempts, all with the same model and set. The examples in the prompt are different from the sessions in the set on purpose:

| Attempt | Found | Candidates | False | No-knowledge sessions empty | Decisions from a plain request |
|---|---|---|---|---|---|
| baseline (round 1 prompt) | 15 / 16 | 44 | 15 (trap 5) | 7 / 13 | 5 (n02, n06, n07, n08, t07) |
| 1: "a request is a task, not a decision" | 15 / 16 | 42 | 15 (trap 5) | 7 / 13 | 4: the sentence alone did not work |
| 2: plus concrete request examples, "such a session gets an empty list" | 14 / 16 | 32 | 8 (trap 1) | 8 / 13 | 1 (n07) but lost u01 (bug cause, nothing extracted) and the rustls label (content found, label too strict) |
| **3 (kept): "... not knowledge unless it exposed a cause, a failure or a surprise"** | **16 / 16** | **37** | **8 (trap 2)** | **9 / 13** | **1 (n07)** |

(`report-28-after.md` is attempt 3. Attempt 2 got worse on u01 because the wording told the model that a fix session is empty; attempt 3 repairs that.)

Status of the hard lines on this 28-session set after round 2:
- Constraint false extraction: 0.
- Decision false extraction: **1** (n07: "the user decided to add a unit test ..."). Not met. The two remaining trap matches are t06 (a gotcha and a procedure about the sandbox cache: noise, not a constraint or decision).
- Hallucination review: not done for the new outputs.

Caveats: one model, one run per attempt, temperature 0. One added sentence moved found items from 14 to 16 and a session from empty to non-empty, so the numbers are sensitive to wording and 16/16 is not a stable property. The u04 hit appeared without a change aimed at it. The examples in the prompt were written to differ from the set, but the set was used to choose between attempts, so these figures are no longer a held-out measure; new sessions are needed to check them. Attempt 3 was chosen after seeing attempt 2's loss on u01.

Next options for the last decision false extraction: a second verification pass over each decision ("is this a task or a choice with a reason?"), or accept the residue and rely on review (decisions are never auto-merged and every candidate waits for a person).

## Decision (user, 2026-10-24)

The residue of decision false extractions that are only a request (n07) is accepted for phase 1; review of each candidate is the safeguard. The Decision hard line therefore stays "not met" on this set rather than being re-defined.

## Hallucination review of the first baseline, completed

`review.md` covered 17 extras (0 hallucinated); `review-rest.md` covers the other 21 candidates (checked by the user together with a second AI): 13 faithful, **8 partial**, 0 unfaithful. The partial ones are: an invented motive or benefit (u08 feature choice, n02 rename "for readability", t05 `?` and `anyhow` motives), a wrong attribution (t01 "the user fixed" what the assistant did; u10 "the team decided" what the assistant changed), a rule widened from one fix (u10 "use UTC for all date calculations"), and a one-off command turned into a general procedure (n02, t01 cache constraint).

So the hallucination hard line is **not met** on the first baseline: 8 of 38 candidates (about 21%) state something the events do not support, and none of them is a pure fabrication. The 17 earlier ones count as 0 by the earlier review. Dominant failure: the model explains *why* when the events only show *what*, and attributes agent actions to the user or "the team". The prompt already says "do not add reasons, causes or details they do not give"; the 8B model does not follow it reliably.

This was measured on the first-baseline outputs (original prompt). The round 1 and 2 prompts changed what is produced; the same review has not been done for them.

## Hallucination review of the current prompt (37 candidates)

Two reviewers, claim by claim, citation status recorded separately (`review-current.md`): 19 faithful, 15 partly faithful, 2 unfaithful, 1 undecided (#9). Rate 17/37 = 45.9% (48.6% if #9 is ruled partial). Not comparable to the 21.1% of the first baseline: this review also counts "the assistant reported success" written as "verified". The three strictness choices that move the number (assistant report vs verified result; "known issue / standard approach" wording; one fix widened to a rule) are listed in the review and need a decision from the user; with the most lenient choice on the first, about 30–32%. Wrong attribution fell to 0. Still present: invented motives and benefits, widened scope, a sandbox workaround as a lasting limitation. New: assistant reports promoted to verified results, a wrong command detail, a diff read as two queries, an invented consequence. Failures are mostly content, not citation: only #33 and #37 are faithful with weak citations. Six candidates that the keyword grader counted as hits are partly unfaithful.

Next step agreed in discussion: a minimal prompt change (drop "must state the why"; assistant reports stay reports; one fix is not a rule), validated on new frozen sessions written before looking at results; only if a clear residue remains, add the attribution check and a second verification pass.

### Rulings on the three strictness choices (user, after seeing the counts)

1. An explicit assistant report may support a candidate that restates it; only "independently verified", a wider result or a stronger conclusion is a deviation (#2, #5, #10, #24 withdrawn; #8 and #17 stay partial). 2. Wording such as "known issue" is judged by what it adds: #9 not counted, #27 and #29 counted. 3. A concrete fix written as a general guarantee is a widened scope: #21 counted.

Final: 24 faithful, 11 partly faithful, 2 unfaithful, 0 undecided; **13/37 = 35.1%**. The rulings were made after the counts were known, so they are post-hoc; they bring the standard closer to the first baseline's (21.1%), which accepted assistant reports as support. The earlier 45.9% / 48.6% figures in this file are superseded by this one.

## Frozen validation of the prompt change (17 new sessions)

`sessions-frozen.json` (9 useful, 8 nothing) was written and committed (`b8fb370`) before the prompt change (`d1e2e0bf…` → `58b5d66d…`, commit `8775331`); the prompt was changed once and not tuned on these results. Same model (qwen3:8b, temp 0), validator and window. Review: `review-frozen.md` (39 distinct candidates, two reviewers; one disagreement, #33, ruled partial: the cause is the candidate's inference, no event states it).

| measure | before | after |
|---|---|---|
| expected items found | 9/9 | 9/9 |
| candidates | 20 | 20 |
| faithful / partial / unfaithful | 11 / 9 / 0 | 14 / 6 / 0 |
| hallucination candidate rate | 9/20 = 45% | 6/20 = 30% |
| false extractions (no-knowledge sessions) | 5 | 3 |
| silent no-knowledge sessions | 5/8 | 5/8 |

Deviations (before/after): invented motive/benefit 4/0, wrong attribution 1/0, widened scope 4/3, generalisation 2/1, inferred unstated fact 1/3, wrong command/number 2/1; citation errors (faithful but incomplete citations) 1/3. The three changes aimed at motive, attribution and scope worked on those tags; what remains is scope (titles dropping a stated limit), inferred causes, and a retry count read wrong.

Verdict: the direction is right (45% → 30%, with a second reviewer 40% → 25%) but the **hallucination hard line (0) is still not met**, and 20 candidates from one model and one run do not show a stable gain. Per the agreed plan the prompt is not tuned further on this set; next options are the deterministic attribution check and a verification pass, preferably with a different model.

Rerun of the frozen set with the `misattributed` check (`frozen-after-attrib.md`): the 20 candidates are identical to `frozen-after.md`, so the check dropped nothing and changes none of the numbers above. With the current prompt the model no longer writes "the user did X" for agent acts on these sessions; the check only guards the old failure (it would have caught three older outputs). Remaining failures are scope, inferred causes and a miscounted retry, which it cannot see.

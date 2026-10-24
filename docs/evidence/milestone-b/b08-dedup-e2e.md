# B-08 dedup, live model, two sessions

`examples/dedup-e2e/run.sh`, log in `b08-dedup-e2e-run.log`. Two hand-written sessions go through the real capture spool, `chloe consolidate` (qwen3:8b for extraction and for the duplicate judgement) and the review commands. No agent is run, so hook capture is not covered here (that is B-E2E-01). The reviewer in the script follows the model's proposal (merge or create) and leaves conflicts.

Session 1 states three points; session 2 restates them in other words:

| Case | Session 2 | Wanted | Result |
|---|---|---|---|
| A same fact (`make sign` before `make publish`) | other wording, kind filed differently (decision vs gotcha) | no second Active memory, provenance kept | **PASS**, but only after a person merged it: the model proposed merge, and the policy left it pending because the kinds differ. If the person had accepted it as new, there would have been two Active memories (first version of the script did exactly that and failed) |
| B similar topic, other detail (SQLite NOT NULL needs DEFAULT vs DROP COLUMN needs 3.35) | different point | stays a separate memory | **PASS** (proposal: create) |
| C same subject, contradicting detail (read-only Fridays vs Saturdays) | "not Fridays, Saturdays" | flagged as a conflict, or a person decides | **FAIL**: the model judged it a duplicate (its own reason says "same knowledge ... but about Saturday instead of Friday"), the fact-kind policy auto-merged it, and the memory still says Fridays. The correction is only a merged source link; nothing in any memory text or open candidate shows it |

Provenance is viewable (`memory sources`): merged candidates appear with relation `Merged` and their session.

## What this means for B-08

- The criterion "no two equivalent Active memories" holds in this run for the equal-fact case, with the caveat that it relied on the person following the proposal.
- A real defect: a candidate that changes a value (day, number, version) of an existing fact can be auto-merged as a duplicate, silently losing the correction. This breaks the intent of the dedup and conflict rules (B-09). One scenario, one run, one model; not a rate.
- Candidate fixes (not done): tell the judge that the same subject with a different value is a conflict, not a duplicate; and/or never auto-merge when the candidate contains numbers or identifiers the target lacks; and/or do not auto-merge facts at all.

## Stability check and fix (follow-up)

The script now has four cases (D: a version change, Python 3.9 to 3.12) and three wordings of the staging correction (`VARIANT=1|2|3`). Logs are in `b08-runs/`.

**Before the fix, 4 runs (variant 1 twice, 2, 3): case C failed in 4/4.** The model called the Saturday note a duplicate every time, whatever the wording. In two runs the policy left it for a person and the scripted reviewer followed the merge proposal; in two it was auto-merged. Case D (Python version) passed 4/4: the model called it a change, not a duplicate. So the error is stable for a day-of-week change and not for a version change, on this model.

**Fix (both parts):**
1. `memory-llm/src/judge.rs`: a duplicate needs every value (day, number, version, name, limit) to be the same; the same subject with another value is never a duplicate, and a corrected value is a supersede.
2. `memory-engine/src/propose.rs`: when the model says duplicate but the numbers or the day/month names differ between the candidate and the target, the proposal becomes a conflict (`the model saw a duplicate but numbers or days differ: ...`). Test: `a_duplicate_verdict_with_another_day_or_number_becomes_a_conflict`. Day and month names now count as values in `claims`, so the rule path no longer calls "read-only on Fridays" and "on Saturdays" the same point either.

**After the fix, 5 runs (variant 1 three times, 2, 3): A, B, C, D passed 5/5.** In variant 1 the judge now proposes supersede (part 1 at work); in variants 2 and 3 it proposes conflict. Nothing is auto-merged away.

Things found on the way:
- My first version of part 2 also compared polarity ("not"). It flagged the genuine sign-before-publish duplicate as a conflict in 4/4 runs because the second note says "does not sign" and the first says "must sign". Polarity was removed from this guard; opposites are left to the judge prompt.
- The check for C/D in the script was fixed to look at the new value only; the earlier version missed it when the candidate title did not contain the subject word.

Limits: hand-written sessions, one model, 5 + 4 runs, three wordings. Case A still relies on a person (different kinds are never auto-merged), so the automatic duplicate merge of equal facts was not exercised by this script; `policy.rs` tests cover it with a stub. Contradicting constraints and decisions (kind guarded) were not tried. A new value that the model phrases without a digit or a day name (a changed tool name, a different region) is not caught by part 2.

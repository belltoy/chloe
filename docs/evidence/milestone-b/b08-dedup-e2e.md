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

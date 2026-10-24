# B-08 dedup with real Claude Code sessions

2026-10-24, Claude Code 2.1.310 (real, with chloe's capture hooks), chloe 0.1.0 release build, extraction and duplicate judge `qwen3:8b` (local Ollama). Three runs, each from an empty database, two real `claude -p` sessions per run on the same fixture repo (`fixture/`, reset with git between sessions). Session 2's wording of the corrections differs per run (`prompts/s2-v1..3.txt`). A person-reviewer script (`review.sh`) follows the model's proposal for pending candidates (merge or create) and leaves conflicts and supersedes open. Logs per run are in `runs/runN/logs/`; `run.sh` and `review.sh` reproduce a run (`./run.sh <n> <variant>`, then `./review.sh <n>`).

Prompts were written before the first run and not changed afterwards. The user facts (staging day, Python version, region, the generated/ rule, `make clean`) are stated in the prompt as notes, because no file or tool output carries them.

## Result per case

`n/e` = not extracted into a candidate, so the case could not be tested in that run. It is counted as neither pass nor fail.

| Case | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| A same fact (sign before publish) | n/e in session 2 (one Active memory) | pass: model proposed merge, a person merged it (the policy left it pending) | **pass, merged automatically** (procedure, `merged` by the policy) |
| B similar topic, other detail | pass (SQLite version kept separate) | pass | pass |
| C value change, day | n/e in session 1 | n/e in session 1 (session 2 added "Staging read-only window" as a new fact) | **pass**: the model said duplicate, the numbers/days guard turned it into a conflict; both memories stay, correction open for a person |
| D value change, version | n/e in session 1 | n/e in session 2: "CI uses Python 3.9" stays Active and stale | n/e in session 2: same |
| E value change, no digits (region) | supersede proposed, left open; the target was a different memory that happened to mention eu-west | n/e in session 2 | pass: conflict, left open |
| F equal fact, other kind (`make clean`) | n/e in session 1 | merge proposed (kinds differ, so pending); a person merged it | n/e in session 1 |
| G contradicts a Constraint (generated/) | pass: conflict | pass: conflict | pass: conflict |

Never happened in 3 runs: two equivalent Active memories, a changed value merged away, a Constraint superseded without a person.

## What this shows

- No dedup failure in these runs. The earlier hand-written run's failure (day merged away) did not recur; in run 3 the guard from the earlier fix fired on a real session.
- Run 3 is the first time the automatic merge of an equal fact (case A) was seen in a real session.
- Coverage is thin. C was testable in 1 of 3 runs, D in 0 of 3, E in 1 (a clean one), F in 1 and only with a person's merge. The limit is extraction recall: user-stated notes are often not turned into candidates in one of the two sessions. Dedup cannot correct a fact that was never extracted: in runs 2 and 3 "CI uses Python 3.9" and, in run 2, "eu-west" stay Active after the user said they changed.
- Case E ("a value change without digits") was not shown to fail. It reached a conflict once, by the model's own judgement, not by the guard. The guard only sees numbers and day/month names; this is unchanged.
- Run 3 left a junk Active memory ("Migration was successful") because the reviewer accepted every pending candidate; that is extraction quality (B-16), not dedup.

## Limits

Three runs, one model (qwen3:8b), one agent (Claude Code), one fixture, facts stated as notes in the prompt. Session 1 candidates were all accepted by script, so what was extracted decided what could be tested. No second model was tried. Not enough runs to give a rate for any case.

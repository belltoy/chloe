# B-08 real sessions, round 2 (prompts changed)

Same setup as `report.md` (Claude Code 2.1.310, chloe 0.1.0, `qwen3:8b`, empty database per run, two real `claude -p` sessions per run), runs 4 to 6. After round 1 the extraction did not turn the notes in the prompt into candidates often enough to test C, D and E, so the prompts were changed (this is a departure from the "prompts not changed" rule of round 1; round 1's results stand as they were):

- The staging day, the Python version and the region now sit in files of the fixture (`fixture2/ops/staging.md`, `.ci/Dockerfile`, `deploy.cfg`). The agent has to read them to do its tasks (write `scripts/import_window.py`, `check_py.py`, `deploy_target.py`). Before session 2, `overlay2-s2/` replaces them with the new values (Saturday, Python 3.12, `us-east`).
- `make clean` then `make publish` is run in both sessions, so the `.signed` gotcha comes from tool output.
- The rule "never edit generated/" is a user sentence in session 1; session 2 lifts it in the task, and asks for an edit of `generated/api.py`.
- Prompts: `prompts/s1-v4..6.txt`, `s2-v4..6.txt`. Run with `FX=2 ./run.sh <n> <4|5|6>` then `./review.sh <n>`. The review script follows the proposal for pending candidates (merge or create) and leaves conflicts and supersedes open.

## Result per case

| Case | Run 4 | Run 5 | Run 6 |
|---|---|---|---|
| A same fact (publish needs the signed file) | session 2 gave a conflict on "make publish fails due to unsigned artifact" against "make clean removes .signed": a false conflict, safe direction | n/e in session 2 | n/e in session 2 |
| B similar topic, other detail | pass | pass | pass |
| C value change, day | **pass**: session 1 memory says Friday, session 2 candidate says Saturday; the model said duplicate, the numbers/days guard turned it into a conflict | n/t: the memory does not carry any day | **pass**: same as run 4 (Friday in the old memory, Saturday in the candidate: conflict) |
| D value change, Python version | n/t: the memory is about the script and carries no version | n/t | n/t |
| E value change, region | n/t: memory carries no region (the "deploy target" procedure was merged, correctly, as the same procedure) | n/t | n/t |
| F equal fact (`make clean` removes `.signed`) | no second copy in session 2 | n/e | n/e |
| G contradicts a Constraint (generated/) | **pass**: conflict | **fail, not flagged**: candidate "Generated/api.py can be edited directly" was proposed as `create` ("unrelated"); with the review script accepting it, an Active Constraint "Never edit generated files" and an Active fact "can be edited directly" stand side by side | **fail, not flagged**: candidate "Generated API fields update" ("must have the owner field added manually, as the generated rule is lifted") was proposed as `create`; same result |

n/t = not testable: the extracted memories describe what a script does and not the changed value, so there was nothing to supersede or conflict with. n/e = not extracted.

## What this shows

- G regresses against round 1. Across all six runs the Constraint was never overwritten, but in 2 of 6 (runs 5 and 6) the model did not see that a candidate about editing `generated/` contradicts it, and the policy has nothing else to catch it with. Both were proposals the reviewer could have refused, but the system showed no conflict. This is the B-09 situation ("a new candidate contradicts a user-specified Constraint ... reviewable via CLI") and it is not reliably met when the candidate does not state itself as a rule. One model, two cases of six; not a rate, but it needs a decision (see below).
- C passed twice more in the form the guard was built for (a day that differs).
- D and E could not be tested again. The extractor describes tasks and not values, so the stale-value problem of round 1 ("CI uses Python 3.9" stays Active after the change) is a limit of extraction, not of dedup.
- Merges seen (deploy target, Python check, migration) were all the same procedure with the same text; no value was lost by those merges.

## Totals over six runs (rounds 1 and 2)

| Case | Pass | Fail | Not testable / not extracted |
|---|---|---|---|
| A | 2 (+1 false conflict) | 0 | 3 |
| B | 6 | 0 | 0 |
| C | 3 (runs 3, 4, 6) | 0 | 3 |
| D | 0 | 0 | 6 |
| E | 1 clean (run 3), 1 by accident (run 1) | 0 | 4 |
| F | 1 (run 2, person merged) | 0 | 5 |
| G | 4 | **2 (runs 5, 6, contradiction not flagged)** | 0 |

## Decisions for a person

1. G: a candidate that contradicts an Active Constraint or Decision without being phrased as a rule is judged `create`. Options: (a) judge prompt told to compare against existing Constraints/Decisions first and treat "allowed / lifted / no longer" about the same subject as a conflict; (b) a rule that flags a candidate which shares file paths or subject words with an Active Constraint; (c) leave it and document it. No code was changed in this work.
2. D and E: accept as not testable with this extractor, or change the extraction to keep values (B-16 scope).

## Limits

Three more runs, same model and agent. Session 1 candidates were all accepted by script. Prompts were changed after seeing round 1.

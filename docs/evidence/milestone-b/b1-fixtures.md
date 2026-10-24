# B1 extraction fixtures

Five captured sessions in `crates/memory-engine/tests/fixtures/*.json`, each with a model reply
to replay and what the pipeline must keep.

| Fixture | What it checks |
|---|---|
| `decision` | A real decision with its reason; the cited file is kept because a cited event shows it |
| `routine` | Formatting, passing tests, reading files: nothing should be extracted |
| `failure_and_fix` | File and commit kept when shown; an invented file and commit are dropped |
| `bad_model_output` | No evidence, invented ref, leaked token, credential path, over-long title: only the valid candidate survives |
| `over_long` | 600 filler events: the model sees only the newest window; the decision at the end is kept, a point made only at the start cannot be cited |

## Deterministic replay (runs in CI)

`cargo test -p memory-engine --test fixtures` replays the scripted replies through the real
prompt, validation, job queue and candidate store. Only the model is faked.

## Live model (manual)

`cargo test -p memory-engine --test fixtures -- --ignored --nocapture`, local Ollama,
`qwen3:8b`, temperature 0, 2026-10-24. Checks are loose: a count range and the kinds that must
appear.

| Prompt version | Result |
|---|---|
| First (B1-03) | `routine` produced 4 invented candidates ("No test failures allowed", "Code is already formatted", …); the same point was often reported under several kinds |
| Rewritten: empty list is the expected answer, routine activity listed as not knowledge, one kind per point | `routine` 0 and `bad_model_output` 1 (as wanted), but `over_long` dropped to 0: the decision was missed |
| Added: a user message that states a decision or rule is always worth an item | All five pass: `routine` 0, `over_long` 1 decision, `decision` 2, `failure_and_fix` 3, `bad_model_output` 1 |

## Limits

- One run per prompt version on one model. Output at temperature 0 is mostly but not strictly
  repeatable, and a different model will behave differently; this is a regression check, not a
  quality measurement.
- Candidates are still proposals. Even in the passing run the model repeats one point as a
  decision and a procedure, and adds wording the events do not give (`decision`: "Implement
  retry logic for SQLite initialization"). Deduplication and review are B2.
- A point made only before the window cannot be extracted (`over_long` documents this).

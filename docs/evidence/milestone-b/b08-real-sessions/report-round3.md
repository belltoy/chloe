# B-08 real sessions, round 3: judge prompt change for case G

Change (`crates/memory-llm/src/judge.rs`): the judge rules now say that a new note that allows what an existing constraint or decision forbids, or says such a rule no longer applies, is `contradicts` (or `supersedes` if it clearly replaces it), never `unrelated`, even when worded as a plain fact or as a task. Test: `prompt_says_lifting_a_rule_is_not_unrelated`. `cargo fmt`, `clippy -D warnings` and `cargo test --workspace` pass.

Validation: runs 7 to 9, the same setup, fixture (`fixture2/`), overlay and prompts (`s1/s2-v4..6`) as runs 4 to 6 (round 2), release binary rebuilt with the change. `qwen3:8b`, Claude Code 2.1.310.

## Case G, before and after (same prompts)

| Prompt | Before (run) | After (run) |
|---|---|---|
| v4 | conflict (run 4) | conflict (run 7: "Stop regenerating generated/") |
| v5 | **not flagged** (run 5) | conflict (run 8: "Generated directory is no longer overwritten") |
| v6 | **not flagged** (run 6) | not testable (run 9): the extracted candidate says only that `generated/api.py` has an `owner` field added by hand; it does not mention the rule being lifted, so there is no contradiction in its text |

Where the contradiction was extracted, the judge now flagged it 2 of 2 (before: 1 of 3). In run 9 the lifting of the rule never reached a candidate, so nothing was judged wrong. This is 3 runs, one model, and the model's answer varies between runs (run 4 flagged it before the change too); it is not a rate.

## Other cases in runs 7 to 9

- B (similar topic, other detail): the SQLite candidates stayed separate in 3 of 3.
- C: Friday was in the session 1 memory in all three runs, but no session 2 candidate carried the new day, so it was not tested (extraction).
- A, D, E, F: not testable again (no value in the memories, or no second copy extracted). Run 7 repeated the harmless false conflict on "make publish needs .signed" seen in run 4 (old prompt too).
- No duplicate Active memory, and no value merged away.
- No false conflict from the new sentence was seen in 3 runs, but 3 runs cannot rule it out.

## `examples/dedup-e2e/run.sh` after the change (`dedup-e2e-after-g/`)

Variants 1 to 3 (run in parallel, one run each): A, C and D pass in all three. B fails in all three: "a sqlite point is missing". The reason is upstream of the judge: in session 1 the extractor produced only 2 candidates (the staging window and the Python rule; the signing and SQLite points were dropped as `unstated_rule`), so the SQLite point was never in the database for the check to find. Nothing in session 2 judged it wrongly. The diff of this round touches only the judge's text, and extraction runs before the judge, so the change cannot have caused it; this was not tested by running the script with the old prompt. The earlier 5/5 pass of this script was before the B-16 extraction rules (commits fdf489b and later), which probably explains why session 1 now yields less. The `dedup-e2e` check B now depends on extraction recall and is no longer a clean dedup check.

## Limits

Three runs for G with one model; run 9 untestable; the A-D script reran once per variant, concurrently; B fails for an extraction reason that was not isolated by an experiment.

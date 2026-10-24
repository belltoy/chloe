# B2 idempotency (B2-05)

What is guaranteed when consolidation is repeated, interrupted, or run by several workers, and
which test shows it. Concurrency tests use separate connections to one file (and, for the
kill test, a separate process), and were repeated without a failure: the engine tests 25 times in a row, the process tests 10.

| Situation | Guarantee | Test |
|---|---|---|
| `consolidate` repeated | No new candidates, jobs or proposals; decided candidates keep their state | `memory-engine/tests/idempotency.rs`: `repeating_consolidate_*`; `memory-cli/tests/interrupt.rs`: `repeated_consolidate_runs_extract_once` |
| Worker dies after storing candidates, before completing the job | After the lease runs out the job runs again and adds no duplicate (same kind and title) | `a_crash_after_storing_candidates_but_before_completing_adds_no_duplicates` |
| `kill -9` of `chloe consolidate` inside the model call | Database healthy (`chloe doctor`), events intact, job still leased and not taken over while the lease is valid, then reclaimed and finished once | `kill_9_during_extraction_loses_nothing_and_recovers` |
| Four workers draining the queue | Each session is extracted exactly once | `two_workers_never_extract_the_same_session_twice` |
| Three reviewers (accept, merge, reject) on the same candidate | Exactly one wins; memories and sources match the winners | `racing_reviewers_have_exactly_one_winner_per_candidate` |
| Four policy runs at once | Each candidate merged once; one source per candidate | `concurrent_policy_runs_merge_each_candidate_once` |

## Limits

- A re-run after a crash is only free of duplicates when the model repeats the same kind and
  title. With a different title the second extraction stores a second candidate; B2-02/03 then
  propose and usually merge it, but it is not prevented.
- After a kill, the job is leased for 10 minutes and a `consolidate` in that time does nothing
  for it; then it backs off 30 s before the next try. Nothing is lost, but recovery is not
  immediate.
- Extraction keeps a per-session watermark (`sessions.extracted_seq`). Events added after a job ran get a new job that covers only them (`a_resumed_session_is_extracted_again_from_where_it_stopped`); a session with nothing new is not planned again. If more events arrive than the window holds, the oldest of them are skipped.

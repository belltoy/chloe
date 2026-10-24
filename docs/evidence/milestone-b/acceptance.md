# Milestone B acceptance run

Date: 2026-10-24. macOS 27.0.1 arm64, 10 cpus. Claude Code 2.1.310, codex-cli 0.164.0. Extraction `qwen3:8b`, embeddings `qwen3-embedding:0.6b` (local Ollama). Checked against `docs/acceptance-criteria.md` section 3 and 4 by reading the tests and evidence and running what was missing. "Test" names are in the crate named. Status: **PASS** (shown by an automated test or a recorded real run), **PARTIAL** (part of the criterion is shown), **NOT DONE**.

| ID | P | Status | Evidence | Gap |
|---|---|---|---|---|
| B-01 Claude capture | P0 | PASS | `e2e-report.md`, `e2e-run.log`: real Claude Code, 6 to 8 events per session, bound to the project, no `memory_save` | |
| B-02 Codex adapter | P0 | PASS | `e2e-report.md` (Codex section): 21 events from a real `codex exec`; unsupported events listed in `docs/agent-events.md` | `PostCompact` not captured; hooks need a one-time trust by the user |
| B-03 Idempotency | P0 | PASS | `memory-store` `importing_the_same_capture_twice_stores_nothing_new`, `replaying_a_thousand_events_twice_stores_no_duplicates` (1000 events twice: 1000 inserted, 0 new, 1000 duplicates); `memory-cli` `spool_import_is_idempotent_and_tolerates_damage`; `memory-engine` `repeating_consolidate_changes_nothing` | A session with new events gets a second job on purpose (watermark) |
| B-04 Redaction | P0 | PASS | `memory-cli/tests/secrets.rs`: a fake token in a prompt, a command and an env assignment is absent from the spool, the database and WAL, `events list`, the request sent to the model, and all outputs, with trace logging on; unit tests in `memory-ingest` | Pattern based: an unknown secret format is not caught |
| B-05 Hook degradation | P0 | PASS | `hook_never_fails_the_agent`, `hook_queues_even_when_the_database_is_unusable`, `kill_switch_disables_capture`; measured hook latency p50 4.9 ms, p95 6.5 ms over 100 calls (release build, max 471 ms on the first cold call) | The hook never calls a model or a daemon, so "LLM timeout" applies to `consolidate`, which is covered by B-07 |
| B-06 Extraction pipeline | P0 | PASS | `memory-engine` `fixtures_replay_to_the_expected_candidates`; live model run in `b1-fixtures.md`; real runs in `e2e-report.md` | Candidates are pending; none becomes Active without review |
| B-07 Fault retry | P0 | PASS | `provider_down_keeps_events_and_retries_later`, `provider_down_for_good_ends_in_dead_letter`, `queued_work_survives_a_restart`, `stale_lease_is_fenced`, `kill_9_during_extraction_loses_nothing_and_recovers` (this is B-FAULT-01); `b2-idempotency.md` | After a kill, recovery waits for the lease (10 min) and a backoff |
| B-08 Dedup | P0 | PARTIAL | `memory-engine/tests/propose.rs` (same kind, title, body merge; a similar title with another detail is not merged; model decides duplicate/conflict/unrelated), `policy.rs` | No end-to-end run of the same fact in two real sessions with a live model |
| B-09 Conflict pending review | P0 | PASS | `policy.rs` `a_conflict_is_flagged_but_nothing_else_changes`, `constraints_and_decisions_are_never_merged_automatically`; `memory-store` `conflict_stays_open_for_a_human`; `memory-cli/tests/review.rs` | Model-judged conflicts not exercised live |
| B-10 Full chain | P0 | PASS | `e2e-report.md`: Claude Code to Codex (`run.sh`, final run) and Codex to Claude Code; same memory id, source `extracted`, no `memory_save` | One run per direction, one trivial bug; the model is not deterministic |
| B-11 Scope guarantee | P0 | PASS | `embed.rs` `twenty_cross_project_queries_never_return_the_other_projects_memories_in_any_mode` (20 queries x fts/vector/hybrid, search and context pack, each non-vacuous); `projects_are_isolated`, `scope_and_status_are_hard_filters_on_the_vector_path`, `context.rs` | |
| B-12 Embedding degradation | P0 | PASS | `degrades_to_fts_when_the_vector_path_is_unavailable`, `search_reports_its_mode_and_why_it_degraded`, `mcp_search_does_not_hang_on_a_stalled_embedding_server` (5.6 s with a server that never answers); a real Codex call with no index returned `degraded` | |
| B-13 Reproducible evaluation | P0 | PASS | `eval/report.md`, `eval/fusion-gate.md`, `chloe eval --json`; per language en/zh/mixed/cross; 100 memories/30 queries and 60/50 held-out | Held-out labels are AI-written, not human-reviewed; Hybrid trails Vector on cross-language MRR (explained in `fusion-gate.md`) |
| B-14 Schema compatibility | P1 | PARTIAL | `ignores_other_events_and_rejects_bad_core_fields`, `bad_events_are_counted_and_do_not_sink_the_batch`, `a_session_without_session_end_stays_open`; payload shapes from Claude Code 2.1.309/2.1.310 and codex-cli 0.164.0 | No fixtures of several agent versions; only one version of each |
| B-15 Vector index consistency | P1 | PASS | `edited_memory_is_embedded_again`, `model_switch_rebuilds_everything_even_back_to_the_first_model`, `dimension_and_model_mismatches_are_refused`, stale vector not served | |
| B-16 Extraction quality | P1 | **PARTIAL** | `extraction/sessions.json` (20 hand-made sessions: 10 useful, 5 nothing, 5 trap), scorer + live runner `memory-engine/tests/extraction_eval.rs`, `extraction/report.md` | qwen3:8b: recall 11/12 on the original labels (12/13 with the post-hoc u07 label), 38 candidates (13 match labels, first-run extras reviewed by hand: 2 useful, 3 duplicates, 12 noise, 0 hallucinated; 9 flagged false incl. 2 real traps: cache workaround as Constraint, assistant preference as decision); usable rate 15/38 strict, 18/38 lenient (`extraction/review.md`); 4/8 no-knowledge sessions stayed empty. Session labels written by one author; the verdicts on the 17 extras in `review.md` were reviewed by two people (per the user); single run, phase 1 targets in `extraction/targets.md` (hallucination review pending, 2 measures not met, recall met) |
| B-17 Diagnostic visibility | P1 | PASS | `chloe doctor` (queue, jobs, provider), `jobs list/show`, `embeddings status`; `doctor_reports_the_queue`, `list_show_and_retry_a_dead_letter` | There is no `stats` command |
| B-18 Manual session compensation | P2 | PASS | `plans_ended_and_idle_sessions_once`, `--session`, `repeated_consolidate_runs_extract_once` | Not run with a real agent that skips `SessionEnd` |
| B-19 Evidence / provenance | P0 | PASS | `accept_creates_a_memory_with_traceable_sources` (store and CLI), `chloe memory sources`; `missing_or_invented_evidence_is_dropped`; creating a memory is never automatic (`creating_a_memory_is_never_automatic`) | |
| B-20 Supersession | P0 | PASS | `supersede_replaces_a_memory_and_keeps_the_trail`, `a_constraint_or_decision_is_superseded_only_by_a_person`, `superseding_is_never_applied_automatically`, CLI `supersede_replaces_a_decision_and_links_both` | |
| B-21 Explain | P1 | PASS | `explain_works_in_every_mode_and_filters_archived_memories` (fts, vector, hybrid; scope, status filter, candidates per path, fusion note, ranks; archived memory absent) | Found and fixed during this check: `--explain` was ignored in full-text mode |
| B-22 Context Pack | P1 | PASS | `memory-engine/tests/context.rs`, MCP `context_pack_is_grouped_bounded_and_scoped`, CLI `memory_context_prints_a_bounded_grouped_pack`; real Claude Code and Codex calls | Budget is in characters, not tokens |

## Section 4 benchmarks

| Metric | Target | Result |
|---|---|---|
| B capture idempotency | 0 duplicate events, 1000 events twice | PASS (test above) |
| B hook response | p95 <= 500 ms, 100 samples | PASS: p95 6.5 ms (release, includes process start) |
| B severe-conflict overwrites | 0 over >= 10 hand-made samples | PARTIAL: automatic paths never update an Active memory by construction; about 8 hand-made cases across `policy.rs` and `candidates.rs`, not a 10-sample set |
| B cross-project leakage | 0 over 20 negative cases incl. vector | PASS (B-11) |
| B eval size | >= 50 memories, >= 20 questions | PASS: 100/30 and 60/50 |
| B retrieval benchmark | publish Recall@5, MRR, p50/p95 | PASS: `eval/report.md`; Hybrid was fixed after losing to Vector and still trails it on cross-language MRR |

## Release gate

- All B P0 items pass or are partial only for the live-model dimension (B-08). No P0 fails.
- Evidence of the real chain from capture to candidate to cross-agent retrieval exists (B-10), in both directions.
- No high-authority conflict can overwrite a decision without review (B-09, B-20).
- FTS, Vector and Hybrid are benchmarked individually (B-13).

Open items before calling B done: B-16 (phase 1 targets set in `extraction/targets.md`; still open: the two unmet measures, review of the other 21 candidates, a larger set and another model; the first baseline is in `extraction/report.md`), B-08 end-to-end with two real sessions, B-14 versioned fixtures, the 10-sample conflict set, and a human review of the held-out labels. The user-trust step for Codex hooks and the approval prompt for MCP tools are manual by design.

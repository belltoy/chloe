# Scenario B-E2E-01: automatic learning, then cross-agent recall

Run on 2026-10-24, macOS arm64, commit `439b9c5` plus the script in `examples/e2e-learning/`. Claude Code 2.1.310, codex-cli 0.164.0, extraction model `qwen3:8b` and embedding model `qwen3-embedding:0.6b` via local Ollama. Temp repo, temp database and a temp Claude settings file; the real chloe data and agent settings were not touched. Raw logs (paths shortened): `e2e-run-before-edit-capture.log` (the run analysed below) and `e2e-run.log` (after edits were captured, see the last section).

**Result of the first run (final-reply and edit capture absent): the pipeline works end to end, the learned memory is wrong; not a pass of B-10 as written ("accurate summary"). See the last two sections for the runs after the capture changes, which pass.**

| Step | Result | Evidence |
|---|---|---|
| Real Claude Code fixes a small bug with the capture hooks installed (B-01) | PASS | 6 events (session start, prompt, 3 tool calls, session end), bound to the project; no `memory_save` and no chloe MCP in the session |
| Import, session due because it ended | PASS | `6 new, 0 already stored`; session `Ended` |
| Local model proposes candidates, each with evidence (B-06, B-19) | PASS, content poor | 2 candidates, each citing event ids and the session |
| Review to Active through the CLI | PASS | candidate accepted, `memory sources` shows `Created ... claude_code session ..., 2 event(s)` |
| A fresh Codex session finds the memory through MCP, same id, source `extracted` | PASS when asked about the memory's own topic | id and title returned (asked: "unittest output truncated") |
| Codex finds it when asked about the real problem (page counts) | **FAIL** | "no matches": the memory says nothing about it |

## Why the memory is wrong

The accepted candidate is a *Gotcha*: "Test output was truncated when running unittest ... `tail -30` helped reveal the full error messages". Nothing like that happened. The model saw only these events:

```
user_prompt          The tests fail (run: python3 -m unittest). Find the cause, fix pager.py ...
tool_result_summary  Bash  cat pager.py && python3 -m unittest 2>&1 | tail -30
tool_result_summary  Edit  <path>/pager.py
tool_result_summary  Bash  python3 -m unittest 2>&1 | tail -5
```

By design (`docs/agent-events.md`) a tool event keeps only the command or path; `tool_response` is never read, there is no edit content, no exit status, and the agent's own explanation of the root cause (floor division drops the partial last page) is not captured. A model with only commands has to guess, and `qwen3:8b` guessed from `tail -30`. Candidates are `pending` until a person accepts them, so nothing wrong became Active by itself, but a hurried reviewer would have accepted a plausible-looking falsehood. The reviewer here (the script) chose the Gotcha over a vaguer Procedure; both were poor. Two earlier runs of the same scenario produced different candidates ("Fix pager.py to resolve test failures", "Tests failed initially"): the model is not deterministic and none of them contained the cause.

## What this shows and does not show

- Capture, import, session lifecycle, extraction, evidence links, review, the shared database and cross-agent retrieval all work with the real clients.
- Extraction quality is bounded by what is captured. With the current privacy-first capture the end-to-end claim "Codex finds the right knowledge" cannot hold for facts that live in tool output or in the agent's reply.
- One session, one bug, one local model, three runs: not a measurement of extraction quality (B-16 still needs its own evaluation set).
- The final Codex step of `run.sh` fails on purpose; it asserts the memory about the real problem is found.

## After capturing edits (same day, commit following `9008267`)

Capture now keeps, for `Edit`/`MultiEdit`, the text replaced and the text written as `-`/`+` lines (1200 chars, redacted; `Write` still path only). The script was also switched to `uv run --with pytest pytest -q` (pytest in a throwaway environment). One run, same models:

- The model saw `- return total_items // per_page` / `+ return -(-total_items // per_page)` and proposed a *Decision*, "Fix ceiling division in pager.py", accepted through the CLI.
- A fresh Codex session searched for the pagination problem and returned that memory id, title and source `extracted`: `run.sh` ends with PASS.
- The memory is on topic and not false, but thin and oddly phrased ("The user decided to fix the ceiling division ... with a proper ceiling division formula"): it does not say that floor division was the cause, because the explanation lives in Claude's final reply, which is not captured. Whether `Stop`'s `last_assistant_message` should be captured is open.
- One run of a non-deterministic model on one trivial bug proves the mechanics, not extraction quality (B-16).

## After capturing the final reply (Claude Code → Codex, final run)

`Stop`'s `last_assistant_message` is now kept as an `assistant_message` event (1000 chars, redacted; confirmed present in the Claude Code 2.1.310 Stop payload). `run.sh` now accepts every candidate (no cherry-picking) and searches in hybrid mode. One run (`e2e-run.log`): Claude Code captured 8 events, the local model proposed four candidates, including the Gotcha *"Floor division in page_count causes missing pages — 21 items at 10 per page resulted in 2 pages instead of 3"* and the Decision *"Use ceiling division for page_count"*; all four were accepted, and a fresh Codex session returned all four ids with source `extracted`. The accepted set also holds two weak items ("Tests were initially failing", "Fix pager.py and re-run tests"), which a careful reviewer would reject.

## Codex as the capture side (Codex → Claude Code)

Run by hand on 2026-10-24 with the real `~/.codex/hooks.json` (chloe's entries added next to the user's other hooks, approved by the user in `/hooks`), codex-cli 0.164.0, temp database via `CHLOE_DB`:

- `codex exec` fixed the same bug in a temp repo; 21 events were captured for one session, all hooks fired (SessionStart, prompt, 17 tool calls, final reply, SessionEnd). `Stop` fired four times for one turn and produced one `assistant_message` event (the idempotency key uses `turn_id`).
- Extraction proposed four candidates, among them the Gotcha *"Floor division caused undercounting of pages"*, built from the final reply. It also proposed a *Constraint*, "Use cached pytest packages to avoid environment errors", which is wrong: the agent worked around a sandbox problem and the model turned that into a rule. It stayed `pending`; B-16 (false extractions) needs its own evaluation.
- After accepting the Gotcha: a fresh Claude Code session searching through the default full-text MCP server found **nothing** for "page count pagination bug" (no stemming: "page" does not match "pages"). With `chloe mcp serve --mode hybrid` and the embeddings backfilled, it returned the memory with its recorded cause and reported `mode: hybrid`, FTS 0 candidates, vector 1. This is the case for recommending hybrid.
- Gap: Codex's `apply_patch` event has no text (its input is not mapped), so the diff of a Codex edit is not captured; only the final reply carries the fix.
- Not automated: it needs the user to trust the hooks in Codex once, so there is no script for this direction.

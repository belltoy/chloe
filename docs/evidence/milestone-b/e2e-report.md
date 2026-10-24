# Scenario B-E2E-01: automatic learning, then cross-agent recall

Run on 2026-10-24, macOS arm64, commit `439b9c5` plus the script in `examples/e2e-learning/`. Claude Code 2.1.310, codex-cli 0.164.0, extraction model `qwen3:8b` and embedding model `qwen3-embedding:0.6b` via local Ollama. Temp repo, temp database and a temp Claude settings file; the real chloe data and agent settings were not touched. Raw log: `e2e-run.log` (paths shortened).

**Result: the pipeline works end to end, the learned memory is wrong. Not a pass of B-10 as written ("accurate summary").**

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

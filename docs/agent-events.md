# Agent events and adapter support matrix (B0-01)

Adapters map each agent's native events onto `memory_core::AgentEvent` (schema version `EVENT_SCHEMA_VERSION` = 1). Agents expose different data, so every field except the identity ones is optional. Do not assume two agents have equivalent events.

## Unified events

| `EventKind` | Meaning | Typical optional fields |
|---|---|---|
| `session_start` | A session began, resumed or was cleared | `cwd` |
| `user_prompt` | The user submitted a prompt | `text` (redacted, truncated) |
| `tool_result_summary` | A tool call finished | `tool_name`, `text` (summary only, never raw output) |
| `context_compacted` | The agent compacted its context | |
| `session_end` | The session ended | |

Identity fields: `native_agent` (open string: `claude_code`, `codex`), `native_session_id`, `kind`, `occurred_at`, `schema_version`. `native_event_id` is the agent's own id for the event when it has one.

**Idempotency key** = `(native_agent, native_session_id, kind, native_event_id)` (`AgentEvent::idempotency_key`). Only at-least-once delivery after durable acceptance is promised. Without a native id, repeats of one kind in one session collapse into one event, so adapters must supply an id wherever the agent offers one.

## Claude Code 2.1.309 (hooks) — verified by capture

Captured on 2026-10-23 with a probe command hook passed via `claude -p --settings`; the payload fields below were seen on stdin. Event list and semantics beyond the captured events come from <https://code.claude.com/docs/en/hooks> and are **not** verified against 2.1.309 (the docs do not give per-version availability).

| Native event | → `EventKind` | Seen fields | `native_event_id` |
|---|---|---|---|
| `SessionStart` | `session_start` | `session_id`, `transcript_path`, `cwd`, `source` (`startup`; docs also list `resume`, `clear`, `compact`, `fork`) | none |
| `UserPromptSubmit` | `user_prompt` | `session_id`, `prompt_id`, `prompt`, `permission_mode` | `prompt_id` |
| `PostToolUse` | `tool_result_summary` | `tool_name`, `tool_input`, `tool_response`, `tool_use_id`, `duration_ms` | `tool_use_id` |
| `PostCompact` | `context_compacted` | docs: `trigger`, `compact_summary` | not captured |
| `SessionEnd` | `session_end` | `reason` (`other` in a `-p` run) | none |
| `Stop` | not mapped (turn end) | `last_assistant_message`, `stop_hook_active` | `prompt_id` |

Notes: `Stop` fires per turn, `SessionEnd` has a 1.5 s default budget (docs), so a missing `SessionEnd` must be tolerated; `PreToolUse`, `PreCompact`, `Notification`, subagent and many other events exist but are not needed. The documented transcript is JSONL at `~/.claude/projects/<project>/<session-id>.jsonl`, whose format the docs call internal and unstable. Hook timeout is in seconds (default 600 for commands); a non-zero exit other than 2 does not block the agent.

## Claude Code adapter (B0-02)

`chloe hook install claude [--settings PATH]` adds `chloe hook claude` (timeout 5 s) to `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `PostCompact` and `SessionEnd` in `~/.claude/settings.json`. Other tools' hooks are never touched, a second install changes nothing, and `chloe hook uninstall claude` removes only our entries. An unparsable settings file is left alone. Restart running Claude Code sessions afterwards. `chloe` must be on the `PATH` Claude Code uses.

`chloe hook claude` reads one payload on stdin and appends one JSON line to `<db dir>/spool/events.jsonl` (dir `0700`, file `0600`). It never opens the database, prints nothing on stdout (Claude Code feeds some hook stdout back to the model) and always exits 0; problems go to stderr. `CHLOE_CAPTURE=off` turns capture off. Measured about 6 ms per call (debug build), and a 6 MB payload took 34 ms.

What is stored per event: `prompt` (4000 chars), and for tool calls only a summary of the *input* (300 chars): the Bash command, a file path, a search pattern, a URL or a query. `tool_response` is never read. Secrets are replaced by `[REDACTED]` before truncation; reason codes (`secret`, `truncated`, `sensitive_path`) are kept in `redactions`. A tool call that mentions a credential path (`.env`, `~/.ssh`, `*.pem`, …) keeps no text at all. Redaction is pattern based (known token prefixes, `NAME=VALUE` with a secret-ish name, `Bearer`, URL passwords, PEM private keys); an unknown secret format is not caught.

Interim limits, replaced by later tasks: the spool is a plain append-only file with no dedup or draining (B0-04 adds the events table and idempotent import, B0-05 the queue and crash recovery); invalid payloads are only reported on stderr, not counted (B0-04); payloads over 8 MiB are dropped; `chloe doctor` does not check the hooks yet.

## Codex 0.164.0 (hooks) — verified by capture, except compaction

Captured on 2026-10-23 by appending a probe command hook to `~/.codex/hooks.json` (removed afterwards) and trusting it in the TUI; two sessions: a `codex exec` run (`originator: codex_exec`) and a session started from the desktop/IDE client (`originator: agent_hud_origin`, `source: vscode`).

- Hooks live in `~/.codex/hooks.json`, same shape as Claude Code's settings (`hooks` → event → `[{matcher?, hooks:[{type, command, timeout, async}]}]`).
- **Hooks must be trusted.** Each hook has `[hooks.state."<file>:<event>:<i>:<j>"] trusted_hash = "sha256:…"` in `config.toml`, granted by the user in the TUI. An untrusted hook silently does not run, and neither did a probe passed with `-c hooks.<Event>=…` or placed in a project `.codex/hooks.json`. An installer cannot just write `hooks.json`; the user must approve, and `doctor` must report an untrusted hook.
- Other tools' hooks are already present in both agents' configs, so an installer must append, never overwrite, and uninstall only its own entries.

| Native event | → `EventKind` | Seen fields | `native_event_id` |
|---|---|---|---|
| `SessionStart` | `session_start` | `session_id`, `transcript_path`, `cwd`, `model`, `permission_mode`, `source` (`startup`) | none |
| `UserPromptSubmit` | `user_prompt` | `turn_id`, `prompt` | `turn_id` |
| `PostToolUse` | `tool_result_summary` | `turn_id`, `tool_name` (`Bash`), `tool_input`, `tool_response` (plain string), `tool_use_id` (`exec-<uuid>`) | `tool_use_id` |
| `PreCompact` / `PostCompact` | `context_compacted` | **not captured** (names exist in the binary) | unknown |
| `SessionEnd` | `session_end` | `reason` (`other`) | none |
| `Stop` | not mapped (turn end) | `turn_id`, `last_assistant_message`, `stop_hook_active` | `turn_id` |

Also present in the binary but not exercised: `PermissionRequest`, `SubagentStart`, `SubagentStop`, `Interrupt`.

Observed: the `codex exec` run produced a `SessionEnd` call. The desktop/IDE session did not, but it was probably still open, so this shows nothing about lost `SessionEnd`s. Whether Codex always delivers `SessionEnd` (for example on a killed process) was not tested; keep tolerating its absence, as with Claude Code.

Rollout transcript: `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl`, one JSON object per line with `timestamp`, `ordinal`, `type`, `payload` (`session_meta`, `event_msg`, `response_item`, `turn_context`, `world_state`). Undocumented; whether `ordinal` is stable across rewrites was not checked. It is a fallback source, not needed while hooks work.

## Codex adapter (B0-03)

Path chosen: hooks, the only mechanism verified on 0.164.0. The rollout transcript is not read (undocumented format, and hooks already carry everything needed); a transcript reader stays a fallback if hooks change. The adapter is read-only toward Codex except for the one hooks file the user asks us to edit.

`chloe hook install codex [--settings PATH]` adds `chloe hook codex` (timeout 3 s, the most Codex allows for `SessionEnd`) to `SessionStart`, `UserPromptSubmit`, `PostToolUse` and `SessionEnd` in `~/.codex/hooks.json` (`$CODEX_HOME/hooks.json` if set). `PostCompact` is not installed because its payload was never captured, and `Stop` is not mapped. Same rules as for Claude Code: other hooks are kept, repeat installs are no-ops, uninstall removes only ours, and `chloe hook codex` never fails, prints nothing and honours `CHLOE_CAPTURE=off`. Differences: the prompt id is `turn_id`, and `tool_response` is a string (still never read).

**Trust.** Codex ignores a hook until the user approves it (`/hooks` in Codex; the approval is stored in `config.toml`, which we never write). After installing, approve the four `chloe hook codex` entries and start a new session. Changing an entry's command or timeout means approving it again. `doctor` does not yet report untrusted hooks.

Verified on 2026-10-23 with a real `codex exec`: `session_start`, `user_prompt` (id = `turn_id`), `tool_result_summary` (id = `tool_use_id`) and `session_end` were spooled, and a fake `API_KEY=hunter2` in the prompt and in the command appeared only as `[REDACTED]`. Installing is global, so sessions you start meanwhile (including the desktop app) are captured too. The installer rewrites the file as pretty-printed JSON with 2-space indentation, so a hand-formatted file changes style (content and key order are kept). `codex exec` waits for stdin unless it is redirected (`< /dev/null`), which has nothing to do with the hooks.

## Comparison

| Capability | Claude Code 2.1.309 | Codex 0.164.0 |
|---|---|---|
| Session start | hook, verified | hook, verified |
| Session end | hook, verified, 1.5 s budget (docs) | hook, verified for `codex exec`; other cases untested |
| User prompt | hook, `prompt_id` | hook, `turn_id` |
| Tool result | hook, `tool_use_id` | hook, `tool_use_id` |
| Context compaction | hook (docs only) | hook (name only), not captured |
| Per-event unique id | none; use `prompt_id` / `tool_use_id` | none; use `turn_id` / `tool_use_id` |
| Hook trust step | not needed | required (`trusted_hash`) |
| Tool output format | object | plain string |

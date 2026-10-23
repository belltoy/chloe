# AGENTS.md

Rust workspace for a local-first MCP memory service for coding agents. Authoritative specs: `docs/engineering-design.md` (design) and `docs/implementation-plan.md` (task list, A0–A3 then B0–B3). Acceptance criteria: `docs/acceptance-criteria.md`. Work is tracked by task ID (e.g. A1-03); stay inside the current phase.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p memory-cli -- --help      # binary is `chloe`
```

All three checks must pass before a change is considered done.

## Architecture rules

- `memory-core` holds IDs, domain types, errors, config and traits. It must not depend on SQL, MCP or any specific agent.
- CLI and MCP must share one service layer (`memory_core::service::MemoryService`); never duplicate business logic in either.
- Updates use optimistic concurrency: `expected_revision` mismatch returns `Error::Conflict`.
- Cross-project reads return `NotFound`, never a permission hint. Search defaults to the caller's project and `Active` status.
- Never build SQL by string-formatting user input, including FTS queries.
- stdout is reserved for command output / MCP stdio; log to stderr only. Never log memory bodies or secrets.
- Defaults are local-only: no network listeners, no remote LLM unless explicitly enabled.

## Conventions

- Edition 2024, workspace-shared dependency versions in the root `Cargo.toml`; add new deps under `[workspace.dependencies]`.
- `unsafe` is forbidden; clippy warnings are errors.
- Library errors use `thiserror` (`memory_core::Error`); binaries may use `anyhow`.
- Tests must use temp dirs and an explicit `db_path`; never touch the user's real data directory.
- Don't invent MCP client config keys; verify against the installed Claude Code / Codex versions.
- Specs exist in Chinese (`docs/*-cn.md`) and English (`docs/*.md`); keep both in sync when editing.

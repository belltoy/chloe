# B-16 extraction evaluation

Model: `qwen3:8b`, temperature from the default config, window = default.

| measure | value |
|---|---|
| expected items found | 14 / 15 |
| candidates produced | 35 |
| false extractions (trap or any candidate in a session with no knowledge) | 8 (of which trap matches 5) |
| sessions with no knowledge that produced nothing | 7 / 10 |
| extra candidates not covered by a label (to be judged by a person) | 13 |

### u01-root-cause (useful) — Bug fix whose cause is only in the final reply
- FOUND expected: floor division drops the last partial page
  - [matches] gotcha: Floor division drops partial pages — Using floor division in page count calculations can result in partial pages being ignored, leading to incorrect pagination. This was the root cause of the pytest failures in the pager.py tests.
  - [extra (needs human check)] procedure: Fix pagination by using ceiling division — To fix the pagination issue, replace floor division with ceiling division in the page count calculation. This ensures that even if there are partial pages, they are properly accounted for, and all tests pass.

### u02-user-constraint (useful) — User states a rule
- FOUND expected: no openssl, rustls only
  - [matches] constraint: Only use rustls for TLS — The user explicitly stated that the repository must never depend on openssl or native-tls, and only use rustls to ensure the binary remains statically linkable.
  - [extra (needs human check)] procedure: Add TLS with rustls — To add TLS to the metrics endpoint, install the rustls-pemfile crate via cargo, then build the project in release mode to verify the TLS setup works without openssl dependencies.
  - [extra (needs human check)] fact: TLS added successfully — The TLS was added via rustls and rustls-pemfile, and the dependency tree confirmed no openssl dependencies were introduced.

### u03-decision-with-reason (useful) — A decision and its reason
- FOUND expected: advisory locks instead of Redis
  - [matches] decision: Use Postgres advisory locks for job leasing — The team decided to use Postgres advisory locks instead of Redis for job leasing because they do not want to run another service just for locks.
  - [extra (needs human check)] procedure: Replace Redis lock with PostgreSQL advisory lock — To switch from Redis to PostgreSQL advisory locks, replace the Redis `set_nx` command with a PostgreSQL `pg_try_advisory_lock` query. Bind the lock key and execute the query using the SQLx library.
  - [extra (needs human check)] fact: Redis dependency removed — The Redis dependency is no longer used by the jobs crate after switching to PostgreSQL advisory locks.

### u04-procedure (useful) — A procedure that was found to work
- MISSED expected: sign before publish
  - [extra (needs human check)] decision: Publishing requires signing first — The user must run `make sign` before `make publish` to avoid the 'unsigned artifact' error. The publish target does not include the signing step by default.

### u05-gotcha-sqlite (useful) — A gotcha with a failing migration
- FOUND expected: ADD COLUMN NOT NULL needs a default on SQLite
  - [matches] gotcha: SQLite requires DEFAULT for NOT NULL columns — SQLite refuses to add a NOT NULL column to an existing table unless a non-null DEFAULT is provided. The error 'Cannot add a NOT NULL column with default value NULL' occurs when no DEFAULT is specified. Adding a DEFAULT of an empty string ('') resolves the issue.

### u06-project-fact (useful) — A lasting project fact stated by the user
- FOUND expected: staging is read-only on Fridays
  - [matches] constraint: Staging DB is read-only on Fridays during backup window — The staging database is read-only on Fridays between 02:00 and 04:00 UTC due to the backup job, which causes imports to fail during this time.

### u07-chinese-constraint (useful) — Rule stated in Chinese
- FOUND expected: explicit timeouts, never defaults
- FOUND expected: gRPC client: explicit 3 s timeout via context.WithTimeout and at most 2 retries (added after the manual review of extras; client-specific, not a rule for all calls)
  - [matches] constraint: 所有对外调用必须显式设置超时 — 用户明确要求所有对外调用必须显式设置超时，不允许依赖默认值，因为默认值在不同环境可能不一致。
  - [matches] procedure: 为 gRPC 调用添加显式超时和重试 — 在 gRPC 客户端调用中，使用 context.WithTimeout 设置显式超时，并在调用前添加 defer cancel() 以确保超时生效。同时，添加最多 2 次重试机制以提高可靠性。

### u08-dependency-swap (useful) — Dependency swap with a stated reason
- FOUND expected: time instead of chrono because of the advisory
  - [matches] decision: Replace chrono with time crate for audit compliance — The user decided to replace the chrono crate with the time crate due to a security vulnerability flagged by cargo audit (RUSTSEC-2020-0159). The change included adding the time crate with the formatting feature to handle date operations.
  - [extra (needs human check)] fact: Cargo audit is clean after the change — After replacing chrono with the time crate, the cargo audit reported no vulnerabilities, indicating the change resolved the security issue.

### u09-buried-decision (useful) — Long routine session with one decision at the end
- FOUND expected: missing_docs deny
  - [matches] decision: Enforce doc comments on public functions — The user decided that every public function in the crate must have a doc comment, and CI will enforce this rule with `missing_docs = deny`.
  - [extra (needs human check)] procedure: Format code with cargo fmt — Running `cargo fmt --all` was used to format the code, but no changes were needed in the repository.
  - [extra (needs human check)] fact: All tests passed — The test suite ran successfully with 112 passed tests and 0 failed.
  - [extra (needs human check)] procedure: Read module files for review — The agent read several module files (`mod2.rs`, `mod4.rs`, etc.) as part of the tidy-up process.
  - [extra (needs human check)] procedure: Enable missing_docs deny — The `#![deny(missing_docs)]` attribute was added to the crate to enforce doc comments on public functions.

### u10-timezone-gotcha (useful) — A flaky test explained
- FOUND expected: tests must not depend on the local time zone
  - [matches] gotcha: Test date differs between local and UTC time zones — The test failed on CI because it used the local time zone, which differs from the UTC time zone used by the CI environment. This caused the date to vary between developer machines and the CI server, leading to test failures.

### n01-routine (nothing) — Formatting and passing tests

### n02-rename (nothing) — One-off edit request
  - [UNEXPECTED] decision: Rename tmp to buffer in parse.rs — The user decided to rename the variable 'tmp' to 'buffer' in parse.rs, likely to improve clarity or align with naming conventions.
  - [UNEXPECTED] procedure: Rename variable in parse.rs — The process involved editing the file parse.rs to replace 'tmp' with 'buffer', followed by a cargo check to ensure the change did not introduce errors.

### n03-generic-question (nothing) — General language knowledge, nothing about the project

### n04-todo-state (nothing) — Transient task state

### n05-credential (nothing) — A credential in the prompt

### t01-env-workaround (trap) — Sandbox workaround turned into a rule (seen in a real Codex run)
- FOUND expected: the real fix
  - [matches] gotcha: Ceiling division required for pagination — The original implementation of the pager.py file used integer division, which truncated the result. This caused incorrect pagination. Replacing it with ceiling division using (total_items + per_page - 1) // per_page fixed the issue.
  - [matches] decision: Fix pager.py using ceiling division — The user decided to fix the pagination logic in pager.py by implementing ceiling division to ensure correct page calculations. This decision was made after observing test failures due to incorrect pagination.

### t02-abandoned-approach (trap) — An approach tried and reverted
- FOUND expected: config is passed explicitly
  - [TRAP] gotcha: Caching config breaks tests that set different configs — Caching the parsed config in a global static caused tests that set different configurations to fail. The issue arose because the cached value overrode the test-specific settings, leading to incorrect test outcomes.
  - [TRAP] decision: Revert to explicit config passing — The user decided to revert the change that introduced the global static cache for the config, opting to pass the config explicitly again to ensure test isolation and correctness.

### t03-slow-once (trap) — One slow run turned into a claim

### t04-unverified-speculation (trap) — The agent guesses a cause, the user drops it

### t05-assistant-opinion (trap) — The assistant's style preference, never agreed by the user

### t06-sandbox-permission (trap) — Fresh: a permission workaround in a sandbox turned into a rule risk
  - [TRAP] gotcha: Docs build fails due to cache permissions — The docs build failed because the sandbox cannot write to the default cache directory. This is a known limitation in the environment.
  - [TRAP] procedure: Workaround for docs build cache permission — To resolve the cache permission issue, set the XDG_CACHE_HOME environment variable to a writable directory like /tmp before running the build command.
  - [UNEXPECTED] fact: Docs build completed successfully — After applying the workaround, the documentation was built successfully in 2.1 seconds.

### t07-assistant-proposal-unanswered (trap) — Fresh: the assistant suggests a convention the user never adopts
  - [TRAP] decision: Add /health endpoint without versioning — The user added the /health endpoint without using versioned paths, as the assistant suggested but was not adopted.

### u11-agreed-proposal (useful) — Fresh: the assistant proposes, the user agrees in a short reply
- FOUND expected: cursor pagination for list endpoints, because OFFSET gets slower and unstable
  - [matches] decision: Use cursor pagination for list endpoints — The user decided to implement cursor pagination (keyset on id) for all list endpoints to avoid the performance issues of OFFSET-based pagination, which can lead to slower deep pages and data inconsistencies when rows change.
  - [extra (needs human check)] procedure: Implement two queries for cursor pagination — To support cursor pagination, two SQL queries are used: one for the initial page (with LIMIT and OFFSET) and another for subsequent pages (with a WHERE clause filtering by id > last_id, ordered by id with a LIMIT).
  - [extra (needs human check)] gotcha: OFFSET can skip or repeat rows with data changes — Using OFFSET for pagination can lead to skipped or repeated rows when data is modified, as the database must scan and discard rows that have been skipped. This is why cursor pagination is preferred for large datasets.

### u12-user-gotcha-statement (useful) — Fresh: the user explains a pitfall
- FOUND expected: billing job UTC vs local dates, convert before comparing
  - [matches] constraint: Always convert dates before comparing in billing job — The billing job runs in UTC, but the customer table stores local dates. Comparisons must include conversion to avoid double-billing issues.


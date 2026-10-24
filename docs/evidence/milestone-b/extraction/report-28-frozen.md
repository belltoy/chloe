# B-16 extraction evaluation

Model: `qwen3:8b`, temperature from the default config, window = default.

| measure | value |
|---|---|
| expected items found | 16 / 16 |
| candidates produced | 37 |
| false extractions (trap or any candidate in a session with no knowledge) | 8 (of which trap matches 2) |
| sessions with no knowledge that produced nothing | 9 / 13 |
| extra candidates not covered by a label (to be judged by a person) | 12 |

### u01-root-cause (useful) — Bug fix whose cause is only in the final reply
- FOUND expected: floor division drops the last partial page
  - [matches] gotcha: Floor division drops partial pages — Using floor division in page count calculations caused the last partial page to be dropped, leading to incorrect results. For example, 21 items at 10 per page were treated as 2 pages instead of 3.
    evidence: a1
  - [extra (needs human check)] procedure: Fix pytest failure in pager.py — To fix the pytest failure in pager.py, modify the page count calculation to use ceiling division instead of floor division. This ensures all items are included in the page count, resolving the test failures.
    evidence: e1, a1

### u02-user-constraint (useful) — User states a rule
- FOUND expected: no openssl, rustls only
  - [matches] constraint: Repo rule: no openssl or native-tls — The user explicitly stated that the repository must never depend on openssl or native-tls, and only use rustls to ensure static linkability of the binary.
    evidence: p1
  - [extra (needs human check)] procedure: Add TLS with rustls — To add TLS to the metrics endpoint, use rustls and rustls-pemfile. This ensures no openssl or native-tls dependencies, maintaining static linkability.
    evidence: p1, b1, e1, a1
  - [extra (needs human check)] fact: TLS added successfully — TLS was successfully added to the metrics endpoint using rustls and rustls-pemfile, with no openssl dependencies in the dependency tree.
    evidence: a1

### u03-decision-with-reason (useful) — A decision and its reason
- FOUND expected: advisory locks instead of Redis
  - [matches] decision: Use Postgres advisory locks instead of Redis for job leasing — The team decided to use Postgres advisory locks instead of Redis for job leasing because they do not want to run another service just for locks.
    evidence: p1
  - [extra (needs human check)] procedure: Replace Redis lock with PostgreSQL advisory lock in lease logic — To switch from Redis to PostgreSQL advisory locks, replace the Redis `set_nx` command with a PostgreSQL `pg_try_advisory_lock` query. Bind the lock key and execute the query using the SQLx library.
    evidence: e1
  - [extra (needs human check)] fact: Redis dependency is no longer used in the jobs crate — After switching to PostgreSQL advisory locks, the Redis dependency is no longer required by the jobs crate.
    evidence: a1

### u04-procedure (useful) — A procedure that was found to work
- FOUND expected: sign before publish
  - [matches] gotcha: Publishing requires signing first — The 'make publish' command fails with 'unsigned artifact' because it does not automatically sign the artifact. Signing must be done separately using 'make sign' before publishing. This is a known issue where the publish target does not include the sign step by default.
    evidence: p1, b1, b2, a1

### u05-gotcha-sqlite (useful) — A gotcha with a failing migration
- FOUND expected: ADD COLUMN NOT NULL needs a default on SQLite
  - [matches] gotcha: SQLite requires DEFAULT for NOT NULL columns — SQLite refuses to add a NOT NULL column without a non-null DEFAULT value. The error 'Cannot add a NOT NULL column with default value NULL' occurs when attempting to add such a column. Adding a DEFAULT value (even an empty string) resolves the issue.
    evidence: a1

### u06-project-fact (useful) — A lasting project fact stated by the user
- FOUND expected: staging is read-only on Fridays
  - [matches] constraint: Staging DB is read-only on Fridays during backup window — The staging database is read-only on Fridays between 02:00 and 04:00 UTC due to the backup job, which causes imports to fail during that time.
    evidence: p1

### u07-chinese-constraint (useful) — Rule stated in Chinese
- FOUND expected: explicit timeouts, never defaults
- FOUND expected: gRPC client: explicit 3 s timeout via context.WithTimeout and at most 2 retries (added after the manual review of extras; client-specific, not a rule for all calls)
  - [matches] constraint: 所有对外调用必须显式设置超时 — 用户明确要求所有对外调用必须显式设置超时，不允许依赖默认值，因为默认值在不同环境可能不一致。
    evidence: p1
  - [matches] procedure: 为 gRPC 调用添加显式超时和重试 — 在 gRPC 客户端调用中，使用 context.WithTimeout 设置显式超时，并在调用前添加重试逻辑，确保调用在超时后能自动重试。
    evidence: e1, a1
  - [extra (needs human check)] gotcha: 重试需配合超时使用 — 在实现重试机制时，必须同时设置超时，否则重试可能无法正确终止，导致资源泄漏或无限循环。
    evidence: a1

### u08-dependency-swap (useful) — Dependency swap with a stated reason
- FOUND expected: time instead of chrono because of the advisory
  - [matches] decision: Replace chrono with time crate for audit compliance — The user decided to replace the chrono crate with the time crate due to a security vulnerability flagged by cargo audit (RUSTSEC-2020-0159). This change was made to ensure a clean audit result.
    evidence: p1
  - [extra (needs human check)] procedure: Update Cargo.toml to use time crate — To address the security vulnerability, the Cargo.toml file was updated to remove chrono and add the time crate with the 'formatting' feature. This change was necessary to resolve the audit warning.
    evidence: e1
  - [extra (needs human check)] fact: Cargo audit is clean after the change — After updating the dependencies, the cargo audit command showed no issues, confirming that the change resolved the security vulnerability.
    evidence: a1

### u09-buried-decision (useful) — Long routine session with one decision at the end
- FOUND expected: missing_docs deny
  - [matches] decision: Enforce doc comments on public functions — The user decided that every public function in the crate must have a doc comment, and CI will enforce this with `missing_docs = deny`.
    evidence: p2
  - [extra (needs human check)] procedure: Format code with cargo fmt — The session shows that `cargo fmt --all` was run multiple times, but no changes were made, indicating the code was already formatted.
    evidence: n1, n3, n7, n11

### u10-timezone-gotcha (useful) — A flaky test explained
- FOUND expected: tests must not depend on the local time zone
  - [matches] gotcha: Test fails in CI due to time zone difference — The test failed on CI because it used the local time zone, while CI runs in UTC. This caused a date discrepancy near midnight. Using UTC for date calculations makes the test deterministic.
    evidence: p1, b1, b2, a1
  - [extra (needs human check)] procedure: Use UTC for date calculations in tests — To ensure tests are deterministic across different environments, use UTC for date calculations. Replace `Local::now()` with `Utc::now()` in test code to avoid time zone discrepancies.
    evidence: e1, a1

### n01-routine (nothing) — Formatting and passing tests

### n02-rename (nothing) — One-off edit request

### n03-generic-question (nothing) — General language knowledge, nothing about the project
  - [UNEXPECTED] fact: String owns a growable heap buffer — The assistant explained that String owns a growable heap buffer, which is a fundamental property of the type in Rust.
    evidence: a1
  - [UNEXPECTED] fact: &str is a borrowed view into UTF-8 bytes — The assistant stated that &str is a borrowed view into UTF-8 bytes owned elsewhere, highlighting its role as a reference type in Rust.
    evidence: a1

### n04-todo-state (nothing) — Transient task state

### n05-credential (nothing) — A credential in the prompt

### t01-env-workaround (trap) — Sandbox workaround turned into a rule (seen in a real Codex run)
- FOUND expected: the real fix
  - [matches] gotcha: Ceiling division required for pagination — The original implementation of the pager.py file used floor division, which caused incorrect pagination. Replacing it with ceiling division using (total_items + per_page - 1) // per_page fixed the issue.
    evidence: p1, e1, a1

### t02-abandoned-approach (trap) — An approach tried and reverted
- FOUND expected: config is passed explicitly
  - [matches] decision: Revert static config caching due to test interference — The user decided to revert the static config caching approach because it broke tests that required setting different configurations. Instead, the config will be passed explicitly in tests to maintain flexibility.
    evidence: p1, p2, b2, a1
  - [matches] constraint: Tests must set different configurations — The user explicitly stated that tests need to set different configurations, which means the config cannot be cached in a static manner. This ensures test isolation and correctness.
    evidence: p2

### t03-slow-once (trap) — One slow run turned into a claim

### t04-unverified-speculation (trap) — The agent guesses a cause, the user drops it

### t05-assistant-opinion (trap) — The assistant's style preference, never agreed by the user

### t06-sandbox-permission (trap) — Fresh: a permission workaround in a sandbox turned into a rule risk
  - [TRAP] gotcha: Docs build failed due to cache permission — The docs build failed because the sandbox could not write to the default cache directory. This is a known limitation of the environment.
    evidence: b1
  - [TRAP] procedure: Set XDG_CACHE_HOME to /tmp for docs build — To resolve the cache permission issue, set the XDG_CACHE_HOME environment variable to /tmp before running the docs build command.
    evidence: b2, a1

### t07-assistant-proposal-unanswered (trap) — Fresh: the assistant suggests a convention the user never adopts
  - [UNEXPECTED] procedure: Adding a /health endpoint — The /health endpoint was added by modifying the routes.rs file and registering the route with .route("/health", get(health)). This is a standard approach for exposing a health check endpoint in a Rust web application.
    evidence: e1

### u11-agreed-proposal (useful) — Fresh: the assistant proposes, the user agrees in a short reply
- FOUND expected: cursor pagination for list endpoints, because OFFSET gets slower and unstable
  - [matches] decision: Adopt cursor pagination for list endpoints — The user decided to use cursor pagination (keyset on id) for all list endpoints to avoid the performance issues of OFFSET-based pagination, which can slow down deep pages and risk skipping or repeating rows when data changes.
    evidence: p1, a1, p2
  - [extra (needs human check)] procedure: Implement two queries for cursor pagination — To support cursor pagination, two SQL queries are used: one for the initial page (with LIMIT and OFFSET) and another for subsequent pages (with a WHERE clause filtering by id > ? to maintain order).
    evidence: e1

### u12-user-gotcha-statement (useful) — Fresh: the user explains a pitfall
- FOUND expected: billing job UTC vs local dates, convert before comparing
  - [matches] constraint: Always convert dates before comparing in billing job — The user emphasized that the billing job runs in UTC but the customer table stores local dates, so comparisons must always include conversion to avoid double-billing issues.
    evidence: p1

### n06-version-bump (nothing) — Fresh: a one-off request to bump a version

### n07-add-test (nothing) — Fresh: a request to add a test
  - [UNEXPECTED] decision: Add unit test for empty string case — The user decided to add a unit test for the parse_date function that covers the case of an empty string, ensuring it returns an error.
    evidence: p1
  - [UNEXPECTED] procedure: Implement test for empty string — The test was implemented by adding a new test function in the dates.rs file, which checks that parse_date returns an error when given an empty string.
    evidence: e1
  - [UNEXPECTED] fact: Test passed successfully — The test for the empty string case passed, indicating the implementation correctly handles this scenario.
    evidence: b1

### n08-ui-tweak (nothing) — Fresh: a cosmetic request

### u13-stated-choice-with-reason (useful) — Fresh: the user states a choice and why, in a short remark
- FOUND expected: exponential backoff, never fixed sleeps, because of stampedes
  - [matches] constraint: Use exponential backoff for retries — The user stated that fixed sleeps caused retries to stampede the API last year, and emphasized that exponential backoff should be used everywhere instead.
    evidence: p2
  - [extra (needs human check)] decision: Implement exponential backoff in uploader — The user requested retries with exponential backoff, and the assistant implemented it by adding retry(3, backoff_exponential(), || send(chunk)) to the uploader code.
    evidence: p1, e1


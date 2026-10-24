# B-16 extraction evaluation

Model: `qwen3:8b`, temperature from the default config, window = default.

| measure | value |
|---|---|
| expected items found | 11 / 12 |
| candidates produced | 38 |
| false extractions (trap or any candidate in a session with no knowledge) | 9 (of which trap matches 3) |
| sessions with no knowledge that produced nothing | 4 / 8 |
| extra candidates not covered by a label (to be judged by a person) | 17 |

### u01-root-cause (useful) — Bug fix whose cause is only in the final reply
- FOUND expected: floor division drops the last partial page
  - [matches] decision: Use ceiling division for page count — The root cause was using floor division for page count, which dropped partial pages. Switching to ceiling division resolved the issue and made all tests pass.
  - [matches] gotcha: Floor division drops partial pages — Using floor division for page count can result in missing a partial last page. For example, 21 items at 10 per page would be treated as 2 pages instead of 3.
  - [extra (needs human check)] procedure: Fix pytest failure in pager.py — To fix the pytest failure in pager.py, replace the floor division with ceiling division. This ensures that partial pages are accounted for, and all tests pass.

### u02-user-constraint (useful) — User states a rule
- FOUND expected: no openssl, rustls only
  - [matches] constraint: Only use rustls for TLS — The user explicitly stated that the repository must never depend on openssl or native-tls, and only use rustls to ensure the binary remains statically linkable.
  - [extra (needs human check)] procedure: Add TLS with rustls — To add TLS to the metrics endpoint, install the rustls-pemfile crate via cargo, then build the project in release mode to confirm the TLS setup works without openssl dependencies.
  - [extra (needs human check)] fact: TLS added successfully — The TLS was added via rustls and rustls-pemfile, and the dependency tree confirmed no openssl was included.

### u03-decision-with-reason (useful) — A decision and its reason
- FOUND expected: advisory locks instead of Redis
  - [matches] decision: Use Postgres advisory locks for job leasing — The team decided to use Postgres advisory locks instead of Redis for job leasing because they did not want to run an additional service just for locks.
  - [extra (needs human check)] procedure: Replace Redis lock with PostgreSQL advisory lock — Update the lease logic to use `pg_try_advisory_lock` by replacing the Redis `set_nx` call with a SQL query that executes `pg_try_advisory_lock($1)`. This change removes the Redis dependency for this crate.
  - [extra (needs human check)] fact: Redis dependency removed — The Redis dependency is no longer used by the jobs crate after switching to PostgreSQL advisory locks.
  - [extra (needs human check)] procedure: Run tests after code change — After modifying the lease logic, run the tests for the jobs crate to ensure the change does not break existing functionality.

### u04-procedure (useful) — A procedure that was found to work
- MISSED expected: sign before publish
  - [extra (needs human check)] decision: Publish requires signing first — The user discovered that the 'make publish' command fails unless the artifact is signed first. The correct workflow is to run 'make sign' before 'make publish'.

### u05-gotcha-sqlite (useful) — A gotcha with a failing migration
- FOUND expected: ADD COLUMN NOT NULL needs a default on SQLite
  - [matches] gotcha: SQLite requires DEFAULT for NOT NULL columns — SQLite refuses to add a NOT NULL column to an existing table unless a non-null DEFAULT is provided. The migration failed because the column 'owner' was defined as NOT NULL without a DEFAULT value. Adding a DEFAULT of an empty string resolved the issue.

### u06-project-fact (useful) — A lasting project fact stated by the user
- FOUND expected: staging is read-only on Fridays
  - [matches] constraint: Import must avoid Friday 02:00-04:00 UTC — The staging database is read-only during that window, so imports must be scheduled outside of it.
  - [extra (needs human check)] decision: Schedule import outside the read-only window — The assistant decided to schedule the import outside the read-only window to avoid failures.

### u07-chinese-constraint (useful) — Rule stated in Chinese
- FOUND expected: explicit timeouts, never defaults
  - [matches] constraint: 所有对外调用必须显式设置超时 — 用户明确要求所有对外调用必须显式设置超时，不允许依赖默认值，因为默认值在不同环境可能不一致。
  - [extra (needs human check)] procedure: 为 gRPC 调用添加显式超时和重试 — 在 gRPC 客户端调用中，使用 context.WithTimeout 设置显式超时，并在调用前添加 defer cancel() 以确保超时生效。同时添加最多 2 次重试机制。

### u08-dependency-swap (useful) — Dependency swap with a stated reason
- FOUND expected: time instead of chrono because of the advisory
  - [matches] decision: Replace chrono with time crate due to security advisory — The user decided to replace 'chrono' with the 'time' crate because 'chrono' was flagged by cargo audit (RUSTSEC-2020-0159). The 'time' crate was chosen with the 'formatting' feature enabled to maintain the required functionality.
  - [extra (needs human check)] procedure: Update Cargo.toml and run cargo audit — Update the Cargo.toml file to replace 'chrono' with 'time' and include the 'formatting' feature. Then run 'cargo audit' to verify that the dependency is clean and no security issues remain.
  - [extra (needs human check)] fact: Cargo audit is now clean — After replacing 'chrono' with 'time' and running 'cargo audit', the audit output shows no security issues, confirming the dependency is clean.

### u09-buried-decision (useful) — Long routine session with one decision at the end
- FOUND expected: missing_docs deny
  - [matches] decision: Enforce doc comments on public functions — The user decided that every public function in the crate must have a doc comment, and CI will enforce this with `missing_docs = deny`.
  - [extra (needs human check)] procedure: Format code with cargo fmt — Running `cargo fmt --all` was used to format the code, but no changes were needed in the repo.
  - [extra (needs human check)] procedure: Read module files for review — The agent read multiple module files (mod2.rs, mod4.rs, etc.) as part of the tidy-up process, though no changes were made.
  - [extra (needs human check)] fact: All tests passed — The test suite ran successfully with 112 passed tests and 0 failed.
  - [extra (needs human check)] procedure: Enable missing_docs deny in lib.rs — The `#![deny(missing_docs)]` attribute was added to `src/lib.rs` to enforce doc comments on public functions.

### u10-timezone-gotcha (useful) — A flaky test explained
- FOUND expected: tests must not depend on the local time zone
  - [matches] gotcha: Time zone differences cause test failures on CI — The test failed on CI because it used the local time zone, which differs from the UTC time zone used by the CI environment. This caused the date to differ between developer machines and the CI server, leading to test failures.
  - [matches] decision: Use UTC for deterministic date calculations — The team decided to use UTC for date calculations to ensure consistency across different environments, especially CI servers. This change made the test deterministic and resolved the time zone discrepancy issue.

### n01-routine (nothing) — Formatting and passing tests

### n02-rename (nothing) — One-off edit request
  - [UNEXPECTED] decision: Rename tmp to buffer for clarity — The variable 'tmp' was renamed to 'buffer' to better reflect its purpose of holding data temporarily. This improves code readability and maintainability.
  - [UNEXPECTED] procedure: Rename variable in parse.rs — To rename a variable in parse.rs, locate the variable declaration, replace the old name with the new name, and ensure the change is consistent across the file. After renaming, run cargo check to verify that the code still compiles correctly.

### n03-generic-question (nothing) — General language knowledge, nothing about the project
  - [UNEXPECTED] fact: String and &str are different in Rust — String owns a growable heap buffer, while &str is a borrowed view into UTF-8 bytes owned elsewhere. Use &str for parameters and String when ownership is needed.

### n04-todo-state (nothing) — Transient task state

### n05-credential (nothing) — A credential in the prompt
  - [UNEXPECTED] fact: Sandbox API returns 200 — The sandbox API endpoint returned a 200 status code, indicating a successful response.

### t01-env-workaround (trap) — Sandbox workaround turned into a rule (seen in a real Codex run)
- FOUND expected: the real fix
  - [matches] decision: Use ceiling division for pagination — The user fixed the pagination logic in pager.py by using ceiling division to ensure correct page calculations, which resolved the test failures.
  - [TRAP] constraint: Use local cache for pytest dependencies — The tests were run using the local cache to avoid network issues, indicating that network access to pytest packages is not guaranteed in this environment.

### t02-abandoned-approach (trap) — An approach tried and reverted
- FOUND expected: config is passed explicitly
  - [TRAP] decision: Avoid global static for config caching — The user decided against using a global static to cache the parsed config because it breaks tests that set different configurations. The tests require explicit passing of the config, so the global static approach was reverted.
  - [extra (needs human check)] procedure: Revert config caching change — To resolve the issue with tests failing due to global config caching, the change was reverted by checking out the previous version of src/config.rs. This ensures that the config is passed explicitly as required by the tests.
  - [extra (needs human check)] gotcha: Global static config breaks test isolation — Using a global static for config caching interferes with test isolation, as tests that set different configurations fail when the global state is not reset. This highlights the importance of avoiding global state in test environments.

### t03-slow-once (trap) — One slow run turned into a claim

### t04-unverified-speculation (trap) — The agent guesses a cause, the user drops it

### t05-assistant-opinion (trap) — The assistant's style preference, never agreed by the user
  - [UNEXPECTED] decision: Use ? operator for error handling — The ? operator was chosen over unwrap() for error handling in fetch.rs to propagate errors upwards instead of panicking. This makes the error handling more explicit and allows for better error recovery in the calling code.
  - [UNEXPECTED] fact: Error handling updated in fetch.rs — The error handling in fetch.rs was modified to use the ? operator instead of unwrap(), indicating a change in the approach to handling potential errors in the response text retrieval.
  - [TRAP] decision: Prefer anyhow over thiserror for applications — The assistant expressed a preference for the anyhow crate over thiserror in application code, as it provides a more ergonomic way to handle errors. However, this preference was not applied in the current change, which used the ? operator with thiserror.


## Reading the numbers (written by hand after the run; labels were not changed after seeing results)

- Recall 11/12. The miss (u04) is a kind mismatch: the model found the point (`make sign` before `make publish`) but filed it as a decision, my label wanted procedure/gotcha. Content-wise it is found.
- Acceptance: of 38 candidates, 15 match a label, 17 are unlabelled extras, 9 are counted false. Most extras are restatements of the same change as a `procedure` or `fact` ("Redis dependency removed", "All tests passed", "Read module files") — noise a reviewer would reject. This is the main weakness: the model emits 2–5 candidates per useful session where 1 is wanted.
- Sessions with no knowledge: 4/8 produced nothing. Failures: n02 rename (2 candidates), n03 general Rust Q&A (1), n05 sandbox ping (1), t05 error-handling edit (2).
- The 3 "trap" matches are not equal. t01 (cache workaround filed as a Constraint) is the real false extraction already seen in the e2e run. t05 (assistant's `anyhow` preference as a decision) is a real false extraction. t02 ("avoid global static for config") is arguably correct: the user did reject it with a reason, so my trap terms there are too broad and this one should not count against the model.
- Not caught: no candidate contained the fake key (n05), nothing came from the unverified speculation (t04) or the TODO state (n04).
- One run, temperature default, a single model (`qwen3:8b`); labels are my own, not reviewed by a second person. Treat as a first baseline, not a threshold.

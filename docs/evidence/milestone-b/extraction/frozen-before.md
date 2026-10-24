# B-16 extraction evaluation

Model: `qwen3:8b`, temperature from the default config, window = default.

| measure | value |
|---|---|
| expected items found | 9 / 9 |
| candidates produced | 20 |
| false extractions (trap or any candidate in a session with no knowledge) | 5 (of which trap matches 0) |
| sessions with no knowledge that produced nothing | 5 / 8 |
| extra candidates not covered by a label (to be judged by a person) | 6 |

### g01-rule-with-incident (useful) — User states a layering rule and why
- FOUND expected: handlers go through the repo package, never sql.DB, because of leaked connections
  - [matches] constraint: Handlers must use repo package for database queries — The user explicitly stated that handlers never touch sql.DB directly; every query must go through the repo package. This rule was established to prevent connection leaks and outages, as seen in past incidents.
    evidence: p1
  - [extra (needs human check)] procedure: Implementing a GET /orders/:id endpoint — To implement the GET /orders/:id endpoint, the handler was updated to call repo.GetOrder through the repo package. This ensures compliance with the rule that handlers never touch sql.DB directly, avoiding potential connection leaks.
    evidence: p1, e1, b1

### g02-naive-aware-datetime (useful) — Cause of a TypeError is explained in the reply
- FOUND expected: naive utcnow vs aware db timestamps
  - [matches] gotcha: Naive vs Aware Datetime Comparison — The expiry check raises a TypeError in production due to comparing naive and timezone-aware datetimes. The local tests passed because the fixture used naive dates, but the production environment uses timezone-aware timestamps from the database.
    evidence: p1, b2, a1

### g03-decision-env-flags (useful) — A decision with its reason, stated by the user
- FOUND expected: flags from env vars, no config service, small team
  - [matches] decision: Use environment variables for feature flags — The user decided to read feature flags from environment variables instead of using a config service. The reason given was that the team consists of three developers and the user does not want to run and monitor an additional service.
    evidence: p1
  - [extra (needs human check)] procedure: Implement flag check function — A function named 'enabled' was added to the flags module. It checks if an environment variable named FLAG_<NAME> is set to '1'. This provides a simple and direct way to determine if a feature is enabled.
    evidence: e1
  - [extra (needs human check)] fact: Flags are retrieved from environment variables — The assistant confirmed that flags are read from environment variables in the format FLAG_<NAME>. This is the method that was implemented and agreed upon.
    evidence: a1

### g04-procedure-db-reset (useful) — A working procedure with the reason for the order
- FOUND expected: db-reset before seed
  - [matches] gotcha: db-reset must precede seed to avoid schema conflicts — Running 'make seed' without first executing 'make db-reset' causes errors because the seed script assumes an empty schema. The 'users' table already exists, leading to the 'relation already exists' error. The correct order is to reset the database first, then seed it.
    evidence: b1, b2, a1

### g05-fact-region (useful) — A lasting fact about the deployment
- FOUND expected: all infrastructure is in eu-west-1
  - [matches] constraint: Deploy only in eu-west-1 region — The user explicitly stated that all deployments must be in the eu-west-1 region. This is a hard constraint to follow in future work.
    evidence: p1

### g06-jest-esm (useful) — Chinese session: a test setup gotcha
- FOUND expected: jest must transform lodash-es
  - [matches] gotcha: ESM模块在Jest中需特殊处理 — lodash-es 是一个ES模块（ESM），而 Jest 默认不会转换 node_modules 中的文件。当尝试在非模块环境中使用 import 语句时会报错。通过在 jest.config.js 中配置 transformIgnorePatterns，可以排除对 lodash-es 的转换限制，从而解决该问题。
    evidence: b1, a1
  - [extra (needs human check)] procedure: 解决Jest中ESM模块导入错误 — 当使用ESM模块（如 lodash-es）时，Jest 默认不会处理 node_modules 中的文件，导致 import 语句报错。解决方法是在 jest.config.js 中配置 transformIgnorePatterns，将 lodash-es 添加到忽略转换的列表中，以允许 Jest 正常解析该模块。
    evidence: e1, a1

### g07-never-amend-pushed (useful) — A working-style rule with a reason
- FOUND expected: no amend/force-push of pushed commits
  - [matches] decision: Never amend or force-push existing commits — The user explicitly stated that future work must avoid amending or force-pushing commits that are already pushed. This ensures reviewers retain their place in the diff and prevents confusion in the commit history.
    evidence: p2
  - [extra (needs human check)] procedure: Fix typo and commit as a new commit — The user fixed a typo in the README and committed it as a new commit, avoiding any amendment or force-push of existing commits. This approach maintains a clean and traceable commit history.
    evidence: p1, b1, a1

### g08-flaky-port (useful) — Buried among routine work: a flaky test explained
- FOUND expected: fixed port in tests collides; bind to port 0
  - [matches] gotcha: Flaky test due to port collision — The test 'server_starts' was flaky because multiple tests bound to the fixed port 8080, causing collisions when run in parallel. The solution was to bind to port 0, allowing the OS to assign a free port automatically.
    evidence: t1, t2, e1, a1
  - [extra (needs human check)] procedure: Fix flaky test by using dynamic port binding — To resolve flaky tests caused by port collisions, modify the test code to bind to port 0. This allows the operating system to assign a free port, preventing conflicts when multiple tests run in parallel.
    evidence: e1, a1

### g09-deploy-freeze (useful) — A schedule fact
- FOUND expected: deploy freeze Thursdays 14:00-18:00 CET
  - [matches] constraint: Release planning must avoid Thursday deployments — Deployments are frozen every Thursday from 14:00 to 18:00 CET due to the weekly batch job. This is a hard constraint to follow.
    evidence: p1

### h01-lint-clean (nothing) — Routine linting

### h02-rename-request (nothing) — A plain rename request

### h03-generic-question (nothing) — General knowledge question

### h04-todo-state (nothing) — End-of-day state

### h05-fix-without-cause (nothing) — A fix with no stated cause and an assistant success report
  - [UNEXPECTED] gotcha: Empty string returns empty list — The test failed because the function did not handle empty input. The fix was to add a check for an empty string and return an empty list.
    evidence: p1, b1, e1, b2

### h06-offline-go-test (nothing) — An offline workaround in a sandbox
  - [UNEXPECTED] gotcha: GOPROXY must be disabled in sandboxed environments — When running Go tests in a sandboxed environment with no network access, the GOPROXY must be set to 'off' to avoid failures due to unreachable module proxies. The default proxy lookup fails in such environments.
    evidence: b1, b2, a1
  - [UNEXPECTED] procedure: Run Go tests with GOPROXY=off in sandbox — To successfully run Go tests in a sandboxed environment with no network access, use the flag GOPROXY=off. This prevents the test runner from attempting to connect to external module proxies, which are not accessible in the sandbox.
    evidence: b2, a1

### h07-dependency-bump (nothing) — A dependency bump requested without a reason

### h08-unadopted-suggestion (nothing) — The assistant suggests a practice the user ignores
  - [UNEXPECTED] decision: Add retries for fetch_report() — The user decided to add retries to the fetch_report() function to handle potential timeout issues. Three retries were implemented in the code.
    evidence: p1, e1
  - [UNEXPECTED] procedure: Implement retries with a loop — A loop with a range of 3 was used to implement retries for the fetch_report() function. Each attempt is wrapped in a try-except block to catch Timeout errors and continue to the next attempt.
    evidence: e1


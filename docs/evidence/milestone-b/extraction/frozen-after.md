# B-16 extraction evaluation

Model: `qwen3:8b`, temperature from the default config, window = default.

| measure | value |
|---|---|
| expected items found | 9 / 9 |
| candidates produced | 20 |
| false extractions (trap or any candidate in a session with no knowledge) | 3 (of which trap matches 0) |
| sessions with no knowledge that produced nothing | 5 / 8 |
| extra candidates not covered by a label (to be judged by a person) | 6 |

### g01-rule-with-incident (useful) — User states a layering rule and why
- FOUND expected: handlers go through the repo package, never sql.DB, because of leaked connections
  - [matches] decision: Use repo package for database queries — Handlers must not touch sql.DB directly; all queries go through the repo package. This rule was established after a previous outage caused by leaking connections from handlers.
    evidence: p1
  - [extra (needs human check)] procedure: Implement GET /orders/:id endpoint — Added the endpoint by calling h.repo.GetOrder in the handler file. The change was verified with tests that passed without issues.
    evidence: e1, b1

### g02-naive-aware-datetime (useful) — Cause of a TypeError is explained in the reply
- FOUND expected: naive utcnow vs aware db timestamps
  - [matches] gotcha: Naive vs Aware Datetime Comparison — Using datetime.utcnow() leads to a TypeError when comparing with timezone-aware datetimes from the database. The error occurs because one datetime is naive (no timezone) and the other is aware (has timezone).
    evidence: b2, a1

### g03-decision-env-flags (useful) — A decision with its reason, stated by the user
- FOUND expected: flags from env vars, no config service, small team
  - [matches] decision: Use environment variables for feature flags — The user decided to read feature flags from environment variables instead of using a config service.
    evidence: p1
  - [extra (needs human check)] procedure: Implement flag check using environment variables — A function 'enabled' was added to check if a flag is active by reading the corresponding environment variable.
    evidence: e1
  - [extra (needs human check)] fact: Flags are retrieved from environment variables — The assistant confirmed that flags are read from environment variables in the format FLAG_<NAME>.
    evidence: a1

### g04-procedure-db-reset (useful) — A working procedure with the reason for the order
- FOUND expected: db-reset before seed
  - [matches] gotcha: db-reset must precede seed to avoid schema conflicts — Running 'make seed' without 'make db-reset' first causes an error because the 'users' table already exists. The seed script assumes an empty schema.
    evidence: b1, b2, a1

### g05-fact-region (useful) — A lasting fact about the deployment
- FOUND expected: all infrastructure is in eu-west-1
  - [matches] constraint: Deploy script must use eu-west-1 region — The user specified that all deployments are in eu-west-1. The deploy script was updated to set AWS_REGION=eu-west-1 to ensure compliance with this rule.
    evidence: p1, e1
  - [matches] fact: Deploy script sets AWS_REGION — The deploy script sets the AWS_REGION environment variable to eu-west-1 as per the user's instruction.
    evidence: a1

### g06-jest-esm (useful) — Chinese session: a test setup gotcha
- FOUND expected: jest must transform lodash-es
  - [matches] gotcha: ESM module import error in Jest — Jest 默认不转换 node_modules 中的文件，当使用 ESM 模块（如 lodash-es）时，会报 SyntaxError: Cannot use import statement outside a module 错误。
    evidence: p1, b1
  - [extra (needs human check)] procedure: Allow ESM module in Jest — 在 jest.config.js 中配置 transformIgnorePatterns，放行特定的 ESM 模块，如 lodash-es，以避免 import 语法错误。
    evidence: e1, b2, a1
  - [matches] fact: lodash-es is ESM only — lodash-es 是一个仅发布为 ESM 模块的库，不兼容 CommonJS 模块格式，因此在 Jest 中需要特殊处理。
    evidence: a1

### g07-never-amend-pushed (useful) — A working-style rule with a reason
- FOUND expected: no amend/force-push of pushed commits
  - [matches] decision: Never amend or force-push existing commits — The user instructed to never amend or force-push commits that are already pushed. Instead, a new commit should be made to preserve reviewers' diff context.
    evidence: p2
  - [extra (needs human check)] procedure: Fix typo and commit — The user asked to fix a typo in the README and commit it. The assistant ran `git commit -am 'fix typo'` to accomplish this.
    evidence: p1, b1, a1

### g08-flaky-port (useful) — Buried among routine work: a flaky test explained
- FOUND expected: fixed port in tests collides; bind to port 0
  - [matches] gotcha: Flaky test due to port collision — The test 'server_starts' was flaky because multiple tests bound to the fixed port 8080, causing collisions when run in parallel. Binding to port 0 allows the OS to select a free port automatically.
    evidence: t1, t2, a1
  - [extra (needs human check)] procedure: Fix flaky test by using dynamic port — To fix the flaky 'server_starts' test, change the port binding from '127.0.0.1:8080' to '127.0.0.1:0'. This allows the OS to assign a free port, preventing port collision during parallel test runs.
    evidence: e1, a1

### g09-deploy-freeze (useful) — A schedule fact
- FOUND expected: deploy freeze Thursdays 14:00-18:00 CET
  - [matches] constraint: Release planning must avoid Thursday 14:00-18:00 CET — Deploys are frozen every Thursday from 14:00 to 18:00 CET due to the weekly batch job.
    evidence: p1

### h01-lint-clean (nothing) — Routine linting

### h02-rename-request (nothing) — A plain rename request

### h03-generic-question (nothing) — General knowledge question

### h04-todo-state (nothing) — End-of-day state

### h05-fix-without-cause (nothing) — A fix with no stated cause and an assistant success report
  - [UNEXPECTED] gotcha: Empty string returns empty list — The test failed because the function did not handle empty input. The fix was to add a check for an empty string and return an empty list.
    evidence: p1, b1, e1, b2

### h06-offline-go-test (nothing) — An offline workaround in a sandbox
  - [UNEXPECTED] gotcha: GOPROXY=off required for tests in sandbox — The tests failed initially due to network restrictions in the sandbox environment, which prevented access to the Go module proxy. Setting GOPROXY=off bypasses this issue by disabling the proxy lookup.
    evidence: b1, b2, a1

### h07-dependency-bump (nothing) — A dependency bump requested without a reason

### h08-unadopted-suggestion (nothing) — The assistant suggests a practice the user ignores
  - [UNEXPECTED] decision: Add retries to fetch_report() — The user requested to add a retry mechanism to the fetch_report() function. Three retries were implemented using a loop with a Timeout exception handler.
    evidence: p1, e1


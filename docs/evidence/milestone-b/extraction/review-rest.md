# B-16 其余 21 条候选的事实忠实度复核清单

来源:`report.md` 里第一次基线(20 个会话,38 条候选)中,`review.md` 没有复核的 21 条:12 条命中标注的、6 条出现在无知识会话里的、3 条触发陷阱标注的。
这一轮只回答**第 1 问:内容是否被会话证据支持**。其余(持久、新增)不在这一轮。

## 怎么判

逐条对照下方每个会话的原始事件,判断候选的标题和内容:
- **忠实**:每个具体陈述(命令、数字、文件、原因、范围)都能在事件里找到。
- **部分**:核心对,但加了事件没说的细节、原因,或者把范围说大了。
- **不忠实**:事件不支持,或数字、否定、适用范围被改。
- 问题类型(只对"部分"和"不忠实"填):`编造`(事件里没有)、`数字`、`否定`、`范围`(说大了或说小了)、`归因`(把助手的话当成用户说的,或反过来)。

## 清单

| # | 会话 | 标注 | 类型 | 标题 | 内容 | 忠实/部分/不忠实 | 问题类型 | 备注 |
|---|---|---|---|---|---|---|---|---|
| 1 | u01-root-cause | matches | decision | Use ceiling division for page count | The root cause was using floor division for page count, which dropped partial pages. Switching to ceiling division resolved the issue and made all tests pass. | 忠实 |  | u01/e1、a1 支持向上取整、向下取整遗漏不足一页的尾页及全部 3 项测试通过。 |
| 2 | u01-root-cause | matches | gotcha | Floor division drops partial pages | Using floor division for page count can result in missing a partial last page. For example, 21 items at 10 per page would be treated as 2 pages instead of 3. | 忠实 |  | u01/a1 明确给出 21 项、每页 10 项却算成 2 页的例子；正确应为 3 页，未改变数字或原因。 |
| 3 | u02-user-constraint | matches | constraint | Only use rustls for TLS | The user explicitly stated that the repository must never depend on openssl or native-tls, and only use rustls to ensure the binary remains statically linkable. | 忠实 |  | u02/p1 明确禁止 openssl、native-tls，仅用 rustls，原因是保持二进制可静态链接。 |
| 4 | u03-decision-with-reason | matches | decision | Use Postgres advisory locks for job leasing | The team decided to use Postgres advisory locks instead of Redis for job leasing because they did not want to run an additional service just for locks. | 忠实 |  | u03/p1 明确使用 Postgres advisory locks 替代 Redis，并说明不愿为锁额外运行服务；e1 支持已实施。 |
| 5 | u05-gotcha-sqlite | matches | gotcha | SQLite requires DEFAULT for NOT NULL columns | SQLite refuses to add a NOT NULL column to an existing table unless a non-null DEFAULT is provided. The migration failed because the column 'owner' was defined as NOT NULL without a DEFAULT value. Adding a DEFAULT of an empty string resolved the issue. | 忠实 |  | u05/e1、a1 支持既有表 ADD COLUMN owner TEXT NOT NULL 缺少非空默认值导致失败，以及补 DEFAULT 空字符串后迁移成功；标题结合正文按新增列场景理解。 |
| 6 | u06-project-fact | matches | constraint | Import must avoid Friday 02:00-04:00 UTC | The staging database is read-only during that window, so imports must be scheduled outside of it. | 忠实 |  | u06/p1 明确 staging 每周五 02:00–04:00 UTC 只读、此时导入失败；a1 支持避开该窗口。 |
| 7 | u07-chinese-constraint | matches | constraint | 所有对外调用必须显式设置超时 | 用户明确要求所有对外调用必须显式设置超时，不允许依赖默认值，因为默认值在不同环境可能不一致。 | 忠实 |  | u07/p1 直接说明所有对外调用必须显式设置超时、禁止默认值及不同环境默认值不一致的原因。 |
| 8 | u08-dependency-swap | matches | decision | Replace chrono with time crate due to security advisory | The user decided to replace 'chrono' with the 'time' crate because 'chrono' was flagged by cargo audit (RUSTSEC-2020-0159). The 'time' crate was chosen with the 'formatting' feature enabled to maintain the required functionality. | 部分 | 编造 | u08/p1 支持替换原因和公告编号，e1 支持启用 formatting；但“为保持所需功能而选择该 feature”的原因未在事件中说明。 |
| 9 | u09-buried-decision | matches | decision | Enforce doc comments on public functions | The user decided that every public function in the crate must have a doc comment, and CI will enforce this with `missing_docs = deny`. | 忠实 |  | u09/p2 明确该 crate 的所有公有函数须有文档注释，并由 CI 以 missing_docs = deny 强制执行；e1 支持添加属性。 |
| 10 | u10-timezone-gotcha | matches | gotcha | Time zone differences cause test failures on CI | The test failed on CI because it used the local time zone, which differs from the UTC time zone used by the CI environment. This caused the date to differ between developer machines and the CI server, leading to test failures. | 忠实 |  | u10/e1、a1 支持测试使用本地时区、CI 使用 UTC，以及临近午夜时日期不同导致失败；候选概述本次故障，未断言所有时间都会日期不同。 |
| 11 | u10-timezone-gotcha | matches | decision | Use UTC for deterministic date calculations | The team decided to use UTC for date calculations to ensure consistency across different environments, especially CI servers. This change made the test deterministic and resolved the time zone discrepancy issue. | 部分 | 归因、范围 | u10/e1、a1 仅支持助手把 invoice 测试的日期计算改为 UTC 并说明其确定性；没有团队作出决策的陈述，也没有所有日期计算统一使用 UTC 的规则。 |
| 12 | n02-rename | UNEXPECTED | decision | Rename tmp to buffer for clarity | The variable 'tmp' was renamed to 'buffer' to better reflect its purpose of holding data temporarily. This improves code readability and maintainability. | 部分 | 编造 | n02/p1、e1、a1 支持 tmp 改名 buffer；未说明改名目的是更准确反映临时存储用途，也未陈述可读性、可维护性改善。 |
| 13 | n02-rename | UNEXPECTED | procedure | Rename variable in parse.rs | To rename a variable in parse.rs, locate the variable declaration, replace the old name with the new name, and ensure the change is consistent across the file. After renaming, run cargo check to verify that the code still compiles correctly. | 部分 | 编造、范围 | n02/e1 支持修改声明，b1 支持随后运行 cargo check；“定位声明、确保全文件一致”没有对应事件，一次改名被扩写成通用流程。运行命令本身也不能证明编译成功。 |
| 14 | n03-generic-question | UNEXPECTED | fact | String and &str are different in Rust | String owns a growable heap buffer, while &str is a borrowed view into UTF-8 bytes owned elsewhere. Use &str for parameters and String when ownership is needed. | 忠实 |  | n03/a1 直接支持所有权、可增长堆缓冲区、UTF-8 借用视图及参数/所有权的使用建议；是否值得持久保存不在本轮判断范围。 |
| 15 | n05-credential | UNEXPECTED | fact | Sandbox API returns 200 | The sandbox API endpoint returned a 200 status code, indicating a successful response. | 忠实 |  | n05/a1 明确 sandbox API 返回 200；“成功响应”是该状态码的含义，候选未携带密钥，也未扩成持续可用性保证。 |
| 16 | t01-env-workaround | matches | decision | Use ceiling division for pagination | The user fixed the pagination logic in pager.py by using ceiling division to ensure correct page calculations, which resolved the test failures. | 部分 | 归因 | t01/e1、a1 支持 pager.py 改用向上取整及测试通过；实际编辑和完成回复来自助手，候选却写成用户修复。 |
| 17 | t01-env-workaround | TRAP | constraint | Use local cache for pytest dependencies | The tests were run using the local cache to avoid network issues, indicating that network access to pytest packages is not guaranteed in this environment. | 部分 | 范围 | t01/b2、a1 支持本次沙箱网络不可达、使用本地 pytest 缓存运行测试；标题将本次应急操作写成使用缓存的要求，正文也未保留“本次 uv 无法联网”的时间与工具限定。不能据此推出持续的环境约束。 |
| 18 | t02-abandoned-approach | TRAP | decision | Avoid global static for config caching | The user decided against using a global static to cache the parsed config because it breaks tests that set different configurations. The tests require explicit passing of the config, so the global static approach was reverted. | 忠实 |  | t02/p2 明确用户因不同配置的测试被破坏而拒绝全局缓存，并要求继续显式传参；b2、a1 支持已撤销。正文的测试要求按该项目上下文理解，未反转用户最后决定。 |
| 19 | t05-assistant-opinion | UNEXPECTED | decision | Use ? operator for error handling | The ? operator was chosen over unwrap() for error handling in fetch.rs to propagate errors upwards instead of panicking. This makes the error handling more explicit and allows for better error recovery in the calling code. | 部分 | 编造 | t05/e1、a1 支持 unwrap 改为 ?；传播错误而非 unwrap 遇错 panic 可由改动得出，但事件没有说明选择动机、调用方恢复能力或“更显式”的收益。 |
| 20 | t05-assistant-opinion | UNEXPECTED | fact | Error handling updated in fetch.rs | The error handling in fetch.rs was modified to use the ? operator instead of unwrap(), indicating a change in the approach to handling potential errors in the response text retrieval. | 忠实 |  | t05/e1、a1 直接支持 fetch.rs 中响应文本读取由 unwrap 改用 ?，没有添加具体错误类型或项目规则。 |
| 21 | t05-assistant-opinion | TRAP | decision | Prefer anyhow over thiserror for applications | The assistant expressed a preference for the anyhow crate over thiserror in application code, as it provides a more ergonomic way to handle errors. However, this preference was not applied in the current change, which used the ? operator with thiserror. | 部分 | 编造 | t05/a1 支持偏好属于助手，且本次未更换错误处理库；但没有说明偏好源于更易用，也未证明当前代码使用 thiserror。使用 ? 本身不能推出 thiserror。 |

## 各会话的原始事件

(按清单出现的顺序;方括号是事件 id,文本里 ⏎ 表示换行,`-`/`+` 是编辑前后的行。)

### u01-root-cause

- [p1] 用户: pytest fails in pager.py, please fix
- [b1] 工具:Bash: cat pager.py test_pager.py && uv run --with pytest pytest -q 2>&1 | tail -30
- [e1] 工具:Edit: /repo/pager.py ⏎ - return total_items // per_page ⏎ + return -(-total_items // per_page)
- [b2] 工具:Bash: uv run --with pytest pytest -q 2>&1 | tail -5
- [a1] 助手回复: Root cause: page_count used floor division, which drops a partial last page, so 21 items at 10 per page gave 2 pages. I switched to ceiling division and all 3 tests pass.

### u02-user-constraint

- [p1] 用户: Add TLS to the metrics endpoint. Rule for this repo: never depend on openssl or native-tls, we only use rustls so the binary stays statically linkable.
- [b1] 工具:Bash: cargo add rustls-pemfile
- [e1] 工具:Edit: /repo/Cargo.toml ⏎ + rustls-pemfile = "2"
- [b2] 工具:Bash: cargo build --release 2>&1 | tail -3
- [a1] 助手回复: Added TLS via rustls and rustls-pemfile; no openssl in the dependency tree.

### u03-decision-with-reason

- [p1] 用户: For job leasing let's use Postgres advisory locks rather than Redis. We decided that because we do not want to run another service just for locks.
- [e1] 工具:Edit: /repo/src/jobs/lease.rs ⏎ - let lock = redis.set_nx(key, worker).await?; ⏎ + let lock = sqlx::query("SELECT pg_try_advisory_lock($1)").bind(key).fetch_one(&pool).await?;
- [b1] 工具:Bash: cargo test -p jobs 2>&1 | tail -4
- [a1] 助手回复: Switched job leasing to pg_try_advisory_lock; the Redis dependency is no longer used by this crate.

### u05-gotcha-sqlite

- [p1] 用户: The new migration fails on sqlite, why?
- [b1] 工具:Bash: sqlite3 test.db < migrations/014_add_owner.sql 2>&1 | tail -3
- [e1] 工具:Edit: /repo/migrations/014_add_owner.sql ⏎ - ALTER TABLE jobs ADD COLUMN owner TEXT NOT NULL; ⏎ + ALTER TABLE jobs ADD COLUMN owner TEXT NOT NULL DEFAULT '';
- [a1] 助手回复: SQLite refuses ALTER TABLE ADD COLUMN with NOT NULL unless the column has a non-null DEFAULT ('Cannot add a NOT NULL column with default value NULL'). I added DEFAULT '' and the migration applies.

### u06-project-fact

- [p1] 用户: Before you run the data import against staging: the staging database is read-only on Fridays between 02:00 and 04:00 UTC because of the backup job, so imports fail then.
- [b1] 工具:Bash: ls scripts/import
- [a1] 助手回复: Understood; I will schedule the import outside that window.

### u07-chinese-constraint

- [p1] 用户: 给 gRPC 客户端加重试。约束:所有对外调用必须显式设置超时,不允许依赖默认值,默认值在不同环境不一致。
- [e1] 工具:Edit: /repo/client/call.go ⏎ - resp, err := c.Do(req) ⏎ + ctx, cancel := context.WithTimeout(ctx, 3*time.Second) ⏎ + defer cancel() ⏎ + resp, err := c.Do(ctx, req)
- [a1] 助手回复: 已为调用加上 3 秒显式超时,并加了最多 2 次重试。

### u08-dependency-swap

- [p1] 用户: Replace chrono with the time crate, chrono is flagged by cargo audit (RUSTSEC-2020-0159) and we want a clean audit.
- [e1] 工具:Edit: /repo/Cargo.toml ⏎ - chrono = "0.4" ⏎ + time = { version = "0.3", features = ["formatting"] }
- [b1] 工具:Bash: cargo audit 2>&1 | tail -3
- [a1] 助手回复: Moved date handling to the time crate. cargo audit is clean now.

### u09-buried-decision

- [p1] 用户: Tidy up the repo
- [n1] 工具:Bash: cargo fmt --all: no changes
- [n2] 工具:Read: Read src/mod2.rs (80 lines)
- [n3] 工具:Bash: cargo fmt --all: no changes
- [n4] 工具:Read: Read src/mod4.rs (80 lines)
- [n5] 工具:Bash: cargo fmt --all: no changes
- [n6] 工具:Read: Read src/mod6.rs (80 lines)
- [n7] 工具:Bash: cargo fmt --all: no changes
- [n8] 工具:Read: Read src/mod8.rs (80 lines)
- [n9] 工具:Bash: cargo fmt --all: no changes
- [n10] 工具:Read: Read src/mod10.rs (80 lines)
- [n11] 工具:Bash: cargo fmt --all: no changes
- [n12] 工具:Read: Read src/mod12.rs (80 lines)
- [t1] 工具:Bash: cargo test --workspace: 112 passed, 0 failed
- [p2] 用户: Decision: from now on every public function in this crate gets a doc comment, CI will enforce it with missing_docs = deny.
- [e1] 工具:Edit: /repo/src/lib.rs ⏎ + #![deny(missing_docs)]
- [a1] 助手回复: Enabled #![deny(missing_docs)]; the build passes.

### u10-timezone-gotcha

- [p1] 用户: test_invoice_date fails only on CI
- [b1] 工具:Bash: TZ=America/New_York cargo test invoice_date 2>&1 | tail -6
- [b2] 工具:Bash: TZ=UTC cargo test invoice_date 2>&1 | tail -3
- [e1] 工具:Edit: /repo/tests/invoice.rs ⏎ - let d = Local::now().date_naive(); ⏎ + let d = Utc::now().date_naive();
- [a1] 助手回复: The test used the local time zone; CI runs in UTC so near midnight the date differed from the developer machines. Using UTC makes it deterministic.

### n02-rename

- [p1] 用户: rename the variable tmp to buffer in parse.rs
- [e1] 工具:Edit: /repo/src/parse.rs ⏎ - let tmp = Vec::new(); ⏎ + let buffer = Vec::new();
- [b1] 工具:Bash: cargo check 2>&1 | tail -2
- [a1] 助手回复: Renamed tmp to buffer.

### n03-generic-question

- [p1] 用户: what is the difference between &str and String in Rust?
- [a1] 助手回复: A String owns a growable heap buffer; &str is a borrowed view into UTF-8 bytes owned elsewhere. Use &str for parameters and String when you need ownership.

### n05-credential

- [p1] 用户: Use this key for the API call: sk-FAKE_TEST_SECRET_0123456789abcdef0123, it is for the sandbox account.
- [b1] 工具:Bash: curl -s -H 'Authorization: Bearer [REDACTED]' https://sandbox.example.test/v1/ping
- [a1] 助手回复: The sandbox API answered 200.

### t01-env-workaround

- [p1] 用户: The tests fail (run: uv run --with pytest pytest -q). Fix pager.py.
- [b1] 工具:Bash: uv run --with pytest pytest -q
- [b2] 工具:Bash: UV_CACHE_DIR=/tmp/uv-cache uv run --offline --with pytest pytest -q
- [b3] 工具:Bash: python3 -m pytest -q
- [e1] 工具:Edit: /repo/pager.py ⏎ - return total_items // per_page ⏎ + return (total_items + per_page - 1) // per_page
- [a1] 助手回复: Fixed pager.py with ceiling division; the tests pass using pytest packages from the local cache because uv could not reach the network in this sandbox.

### t02-abandoned-approach

- [p1] 用户: Try caching the parsed config in a global static
- [e1] 工具:Edit: /repo/src/config.rs ⏎ + static CONFIG: OnceLock<Config> = OnceLock::new();
- [b1] 工具:Bash: cargo test 2>&1 | tail -4
- [p2] 用户: No, that breaks the tests that set different configs. Revert it, we keep passing the config explicitly.
- [b2] 工具:Bash: git checkout src/config.rs
- [a1] 助手回复: Reverted; config is passed explicitly again.

### t05-assistant-opinion

- [p1] 用户: clean up the error handling in fetch.rs
- [e1] 工具:Edit: /repo/src/fetch.rs ⏎ - let body = resp.text().unwrap(); ⏎ + let body = resp.text()?;
- [a1] 助手回复: Replaced unwrap with the ? operator. In general I prefer anyhow over thiserror for applications, but I did not change that here.

## 汇总(填完后计算)

- 忠实:**13 / 21**    部分:**8**（#8、#11、#12、#13、#16、#17、#19、#21）    不忠实:**0**
- 幻觉硬线(= 0)的覆盖:这一轮之后,38 条里 38 条都复核过(17 条见 `review.md`)。结论:**未通过**。本轮有 8 条部分忠实，均含证据未支持的原因、收益、归因或范围；不能因“不忠实”为 0 就认定幻觉为 0。沿用 `review.md` 对另 17 条的既有复核（幻觉 0），38 条中本轮已发现至少 **8 / 38（约 21.1%）** 存在事实偏差。此处合并覆盖范围，不重新复核或改写那 17 条的结论。

复核口径：已对照 `sessions.json` 的对应原始事件及 `report.md` 的候选全文。标题与正文合并判断；代码改动直接蕴含的语义可作为支持，未陈述的动机、收益或参与者不自行补全。助手回复仅作为会话内陈述的证据，不等于独立验证执行结果。本轮不评价持久性、新增性或候选类型是否合适；以上为助手复核，待用户确认。

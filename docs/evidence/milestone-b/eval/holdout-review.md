# Held-out evaluation set: draft for review

Status: **draft, not reviewed by a person**; labels revised once after an AI audit (h06, h18, h27, h29, h40, h42, h50), still no human review. It was written by the assistant that also built the fusion, so it needs an independent check before any result on it counts. `holdout.json` has 60 memories on a different domain from `dataset.json` (an Erlang gRPC client library and its platform) and 50 queries. It must not be used to choose weights or thresholds: look at it once per candidate algorithm, after the candidates are fixed.

Please check, for each row: (1) is the relevant list complete (anything else that really answers the query?), (2) is anything listed relevant that does not answer it, (3) is the hard-negative note honest. Edit `holdout.json` directly; keys are `e*` English, `z*` Chinese, `x*` mixed.

Tags: `ident` exact identifier or error code, `para` paraphrase with little word overlap, `hardneg` high word overlap with a wrong memory, `plain` ordinary question. `lang` is the query language; `cross` means query and memory differ.

| id | lang | tag | query | relevant memory (title) | note |
|---|---|---|---|---|---|
| h01 | en | ident | what does E1042 mean | e07: Error code E1042 |  |
| h02 | en | ident | E2007 | e08: Error code E2007 |  |
| h03 | en | ident | ETIMEDOUT when the pool starts | e03: ETIMEDOUT during pool warm-up |  |
| h04 | en | ident | badarg after supervisor restart | e04: badarg from ets after a supervisor restart |  |
| h05 | en | para | why is one slow backend not allowed to starve the others | e01: Separate connection pool per upstream | hard negatives: z04, z05 (database pools) |
| h06 | en | para | what happens on the server when a call's deadline passes | e02: Use gRPC deadlines, not socket timeouts |  |
| h07 | en | hardneg | default pool size for gRPC channels | e23: Default pool size | hard negative: z05 (database pool size) |
| h08 | en | hardneg | which calls does the client retry | e09: Retry only idempotent calls | hard negatives: z16 (retry storms), z18 (idempotency keys) are about other aspects |
| h09 | en | plain | how do I run a test over and over to find a flaky failure | e16: Reproduce a flaky test |  |
| h10 | en | plain | can we create atoms from request data | e19: No atoms from user input |  |
| h11 | en | plain | where do we keep session data | e12: Mnesia is not used for session data; x15: Redis 会话过期 | both answer it |
| h12 | en | hardneg | what must happen before the health endpoint returns 200 | e14: Health endpoint | hard negatives: x18, z11 (probes) |
| h13 | en | plain | how long is DNS information cached | e24: DNS caching |  |
| h14 | en | para | what format do telemetry event names follow | e17: Telemetry event names | hard negative: x07 (Prometheus metric names) |
| h15 | zh | plain | 一次放多少流量给新版本 | z01: 灰度发布比例 |  |
| h16 | zh | para | 虚拟机迁移之后超时为什么会提前触发 | z02: 时钟回拨 |  |
| h17 | zh | plain | 内存涨得很快怎么查 | z03: 排查进程内存增长 |  |
| h18 | zh | hardneg | 数据库连接池设多大 | z05: 数据库连接池大小 | hard negatives: e23 (gRPC pool size), e01 (per-upstream pools), z04 (same topic, but a constraint about borrowing from the pool, not its size) |
| h19 | zh | plain | 删除字段之后编号能再用吗 | z07: protobuf 字段编号复用 |  |
| h20 | zh | para | 发布出问题之后怎么办 | z08: 回滚失败的发布 |  |
| h21 | zh | para | 什么情况下会半夜呼叫值班的人 | z09: 告警分级 |  |
| h22 | zh | plain | 滚动更新时请求为什么会丢 | z11: 滚动更新丢请求 |  |
| h23 | zh | hardneg | 重试为什么要加随机 | z16: 重试风暴 | hard negative: e09 (which calls are retried) |
| h24 | zh | plain | 压测之前要做什么准备 | z13: 压测流程 |  |
| h25 | zh | plain | 日志里必须带哪些字段 | z14: 日志格式 |  |
| h26 | mixed | ident | egrpc_channel_pool 的 checkout 超时 | x01: channel_pool 的 checkout 超时 |  |
| h27 | mixed | plain | gen_server:call 默认超时多久，怎么配置 | x02: gen_server:call 默认超时 |  |
| h28 | mixed | ident | dialyzer warning 在 CI 怎么处理 | x04: Dialyzer 警告处理 |  |
| h29 | mixed | plain | supervisor intensity 设得太小有什么风险 | x05: supervisor 重启强度 |  |
| h30 | mixed | ident | persistent_term 更新的代价 | x06: 用 persistent_term 存配置 |  |
| h31 | mixed | ident | grpcurl 怎么调试 | x08: gRPC 反射调试 |  |
| h32 | mixed | hardneg | 新增 protobuf 字段要不要 optional | x10: protobuf optional 字段 | hard negatives: z07, e22 (other protobuf rules) |
| h33 | mixed | ident | ETS 表 public 好不好 | x13: ETS 表 public 与 protected | hard negative: e04 (ETS tables) |
| h34 | mixed | para | binary 引用计数为什么内存不释放 | x17: 大 binary 与内存释放 | hard negative: e11 (large messages) |
| h35 | mixed | hardneg | certificate_expired 怎么排查 | x09: certificate_expired 握手失败 | hard negative: e10 (rotate certificates) |
| h36 | cross | hardneg | 为什么要给每个上游单独建连接池 | e01: Separate connection pool per upstream | zh -> en; hard negatives: z04, z05 (Chinese database pools) |
| h37 | cross | plain | 请求体太大会报什么错 | e07: Error code E1042 | zh -> en |
| h38 | cross | plain | 没有权限调用某个方法返回什么码 | e08: Error code E2007 | zh -> en |
| h39 | cross | plain | 热升级前要检查什么 | e05: Hot code upgrade checklist | zh -> en |
| h40 | cross | para | 客户端断开之后服务端为什么还在消耗 CPU | e20: Backpressure on streams | zh -> en; hard negative: e02 |
| h41 | cross | plain | 每个租户一秒最多能发多少请求 | e13: Rate limit per tenant | zh -> en |
| h42 | cross | plain | 把请求数据转成 atom 有什么限制，应该怎么做 | e19: No atoms from user input | zh -> en |
| h43 | cross | hardneg | 证书怎么平滑轮换 | e10: Rotate service certificates | zh -> en; hard negative: x09 |
| h44 | cross | hardneg | gen_server 回调里能不能同步调用别的服务 | e06: No blocking calls in gen_server callbacks | zh -> en; hard negative: x02 |
| h45 | cross | plain | how do we ship a new version gradually | z01: 灰度发布比例 | en -> zh |
| h46 | cross | plain | how do I find which process is eating memory | z03: 排查进程内存增长 | en -> zh |
| h47 | cross | hardneg | what happens when certificate_expired shows up in the TLS handshake | x09: certificate_expired 握手失败 | en -> mixed; hard negative: e10 |
| h48 | cross | para | what is the availability target for core endpoints | z15: 服务等级目标 | en -> zh |
| h49 | cross | hardneg | when should the retry be randomised | z16: 重试风暴 | en -> zh; hard negative: e09 |
| h50 | cross | plain | which faults page the on-call person | z09: 告警分级 | en -> zh |

## Baseline on the first draft (STALE: measured before the label revision, re-run before use)

Recall@5 / MRR. Run once to check the set is not trivially easy or broken; no per-query output was looked at and nothing was tuned on it.

| group | n | FTS | Vector | Hybrid |
|---|---|---|---|---|
| en | 14 | 0.89 / 0.90 | 0.93 / 0.87 | 1.00 / 0.95 |
| zh | 11 | 1.00 / 0.94 | 1.00 / 1.00 | 1.00 / 1.00 |
| mixed | 10 | 1.00 / 1.00 | 1.00 / 1.00 | 1.00 / 1.00 |
| cross | 15 | 0.20 / 0.13 | 1.00 / 1.00 | 0.87 / 0.52 |
| all | 50 | 0.73 / 0.70 | 0.98 / 0.96 | 0.96 / 0.84 |

What this says, to be confirmed after your review:
- The cross-language regression of Hybrid shows up again on an independent set (cross MRR 0.52 vs Vector 1.00), so it is not an artefact of `dataset.json`.
- English Hybrid beat Vector alone (1.00 / 0.95 vs 0.93 / 0.87), but the cause is not established: `ident` queries scored 1.00 in every mode, so the gain cannot be attributed to identifiers without a paired per-query comparison. Treat "FTS helps English" as a hypothesis.
- Same-language groups are near the ceiling for every mode, so only the English and cross groups can separate algorithms; a decision should rest on those, and with 14 and 15 queries a one-query difference is within noise.

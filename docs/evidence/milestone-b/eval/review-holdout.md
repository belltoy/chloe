# 留出集标注人工复核清单

对象:`holdout.json` 的 50 个查询和 60 条记忆(Erlang gRPC 客户端库及其平台)。标注由 AI 写成,按一次 AI 审计改过 7 条,从未有人审过。

## 要判断的三件事(每个查询一行)

1. **完整吗**:相关列表之外,还有没有别的记忆其实也能回答这个查询?把 key 填进"应增加"。全部记忆见文末附录。
2. **多列吗**:列出的相关记忆里,有没有并不回答这个查询的?把 key 填进"应删除"。
3. **hard negative 说明诚实吗**:备注里说的"词重叠高但答案错误"的记忆,确实答不了这个查询吗?填 是/否/无备注。

判定"回答"的标准:只看这条记忆的标题和内容,一个人能否用它回答该查询(不是主题相近就算)。

## 重要:改完要重跑

这套集合只用来在算法固定之后看一次,不能用来选权重或阈值。你改了标注之后,之前 Vector/Hybrid 在它上面的数字(见 `fusion-gate.md`)作废,需要重跑。

## 清单

| id | 语言 | 标签 | 查询 | 当前相关记忆(key: 标题 — 内容) | 备注(hard negative) | 完整/少列 | 应增加(key) | 应删除(key) | 说明诚实(是/否/无) | 意见 |
|---|---|---|---|---|---|---|---|---|---|---|
| h01 | en | ident | what does E1042 mean | **e07**: Error code E1042 — E1042 means the request body exceeded the 4 MiB message limit configured on the channel. |  | 完整 | 无 | 无 | 无 | e07 直接给出 E1042 的含义及 4 MiB 限制；x16 只说明错误码格式，不能解释此编号。 |
| h02 | en | ident | E2007 | **e08**: Error code E2007 — E2007 means the caller's token lacks the scope required by the method. |  | 完整 | 无 | 无 | 无 | e08 直接解释 E2007；x16 没有该编号的含义，无须补列。 |
| h03 | en | ident | ETIMEDOUT when the pool starts | **e03**: ETIMEDOUT during pool warm-up — Starting the pool before the upstream DNS record exists produces ETIMEDOUT and the pool never retries on its own. |  | 完整 | 无 | 无 | 无 | e03 直接说明启动预热时 ETIMEDOUT 的 DNS 原因及不会自行重试；e24 的缓存时长不能回答启动失败。 |
| h04 | en | ident | badarg after supervisor restart | **e04**: badarg from ets after a supervisor restart — ETS tables owned by a crashed process vanish; callers then get badarg until the owner is restarted and recreates the table. |  | 完整 | 无 | 无 | 无 | e04 直接说明重启后 ETS 表消失导致 badarg；x05 的重启强度和 x13 的访问权限不能解释该故障。 |
| h05 | en | para | why is one slow backend not allowed to starve the others | **e01**: Separate connection pool per upstream — Each upstream service gets its own egrpc_channel_pool so one slow upstream cannot exhaust connections for the others. | hard negatives: z04, z05 (database pools) | 完整 | 无 | 无 | 是 | e01 给出按上游隔离连接池及避免相互耗尽连接的原因；z04、z05 只讲数据库连接借用和容量。 |
| h06 | en | para | what happens on the server when a call's deadline passes | **e02**: Use gRPC deadlines, not socket timeouts — Set a deadline on every call; the server cancels work when it passes. Socket-level timeouts are left at their defaults. |  | 完整 | 无 | 无 | 无 | e02 明确 deadline 到期后服务端取消工作；e20 讨论客户端离开后忽略发送错误，触发条件不同。 |
| h07 | en | hardneg | default pool size for gRPC channels | **e23**: Default pool size — The default pool holds 8 channels per upstream, configurable with the pool_size option. | hard negative: z05 (database pool size) | 完整 | 无 | 无 | 是 | e23 明确默认每上游 8 个 channel 及 pool_size 选项；z05 的核心数两倍仅适用于数据库池。 |
| h08 | en | hardneg | which calls does the client retry | **e09**: Retry only idempotent calls — The client retries a failed call up to twice, but only when the method is marked idempotent in the protobuf options. | hard negatives: z16 (retry storms), z18 (idempotency keys) are about other aspects | 完整 | 无 | 无 | 是 | e09 明确仅重试 protobuf 标为幂等的方法、最多两次；z16 的抖动和 z18 的幂等键不能替代方法重试资格。 |
| h09 | en | plain | how do I run a test over and over to find a flaky failure | **e16**: Reproduce a flaky test — Run the test 200 times with a fixed random seed, then bisect by removing concurrent tests. |  | 完整 | 无 | 无 | 无 | e16 给出固定随机种子重复 200 次及移除并发测试二分定位的方法；x03 只有通用测试命令。 |
| h10 | en | plain | can we create atoms from request data | **e19**: No atoms from user input — Never call binary_to_atom on request data; use binary_to_existing_atom or keep binaries. |  | 完整 | 无 | 无 | 无 | e19 明确禁止对请求数据使用 binary_to_atom，并给出已有 atom 或保留 binary 的替代方式。 |
| h11 | en | plain | where do we keep session data | **e12**: Mnesia is not used for session data — Session data lives in Redis because mnesia schema changes are hard to roll out across nodes.<br>**x15**: Redis 会话过期 — Redis 里的会话键 TTL 是 30 分钟，每次请求会续期。 | both answer it | 完整 | 无 | 无 | 无 | e12 明确存于 Redis；x15 也独立说明 Redis 中的会话键，能回答存储位置，应保留两条。该备注不是负例说明。 |
| h12 | en | hardneg | what must happen before the health endpoint returns 200 | **e14**: Health endpoint — The /health endpoint returns 200 only when the pool has at least one ready channel. | hard negatives: x18, z11 (probes) | 完整 | 无 | 无 | 是 | e14 给出返回 200 的必要条件：至少一个就绪 channel；x18 的探针延时和 z11 的预热问题都没有规定该端点的返回条件。 |
| h13 | en | plain | how long is DNS information cached | **e24**: DNS caching — Channel resolution is cached for 30 seconds, so a changed record is picked up only after that. |  | 完整 | 无 | 无 | 无 | e24 直接给出 DNS 缓存 30 秒；e03 讲记录不存在导致启动失败，不提供缓存时长。 |
| h14 | en | para | what format do telemetry event names follow | **e17**: Telemetry event names — Event names use the form egrpc.client.<event>.<measurement>, never dynamic values in the name. | hard negative: x07 (Prometheus metric names) | 完整 | 无 | 无 | 是 | e17 给出事件名格式及禁止动态值；x07 规范的是 Prometheus 指标名，不能混同为 telemetry 事件名。 |
| h15 | zh | plain | 一次放多少流量给新版本 | **z01**: 灰度发布比例 — 新版本先放百分之五的流量，观察错误率半小时后再扩大到一半。 |  | 完整 | 无 | 无 | 无 | z01 直接给出先 5%、观察半小时再扩至 50% 的灰度比例。 |
| h16 | zh | para | 虚拟机迁移之后超时为什么会提前触发 | **z02**: 时钟回拨 — 虚拟机迁移后系统时钟可能回拨，依赖单调时间以外的超时判断会提前触发。 |  | 完整 | 无 | 无 | 无 | z02 直接说明虚拟机迁移后的时钟回拨及非单调时间判断问题；其他超时记忆没有迁移原因。 |
| h17 | zh | plain | 内存涨得很快怎么查 | **z03**: 排查进程内存增长 — 先用 observer 查看按内存排序的进程，再检查是否有进程邮箱持续积压。 |  | 少列 | e11、x17 | 无 | 无 | 问题未限定为定位进程。除 z03 的 observer 与邮箱排查外，e11 可提示检查大响应是否聚合成单个 binary，x17 可提示检查是否长期持有大 binary；两条均提供可用于排查的具体原因。x14 的 fprof 分析函数耗时，不能直接定位内存增长。 |
| h18 | zh | hardneg | 数据库连接池设多大 | **z05**: 数据库连接池大小 — 数据库连接池大小设为核心数的两倍，超出的请求排队等待而不是新建连接。 | hard negatives: e23 (gRPC pool size), e01 (per-upstream pools), z04 (same topic, but a constraint about borrowing from the pool, not its size) | 完整 | 无 | 无 | 是 | z05 直接给出数据库池大小；e23、e01 是 gRPC 上游池，z04 仅规定借用连接，都不能回答数据库池设多大。 |
| h19 | zh | plain | 删除字段之后编号能再用吗 | **z07**: protobuf 字段编号复用 — 删除字段后不能复用编号，旧客户端会把新数据解析成错误的类型。 |  | 完整 | 无 | 无 | 无 | z07 明确删除后不能复用字段编号及兼容性原因；e22 的版本规则和 x10 的 optional 要求不能回答编号复用。 |
| h20 | zh | para | 发布出问题之后怎么办 | **z08**: 回滚失败的发布 — 发现错误率上升时先切回上一个版本，再分析日志，不要在线上直接修补。 |  | 完整 | 无 | 无 | 无 | z08 直接给出先回滚再分析日志、禁止线上直接修补；z01 的灰度扩量流程不回答故障后的处置。 |
| h21 | zh | para | 什么情况下会半夜呼叫值班的人 | **z09**: 告警分级 — 只有影响用户的故障才会呼叫值班人员，其余告警进入工作日处理队列。 |  | 完整 | 无 | 无 | 无 | z09 明确只有影响用户的故障才呼叫值班；z10 仅讲交接时间，不回答呼叫条件。 |
| h22 | zh | plain | 滚动更新时请求为什么会丢 | **z11**: 滚动更新丢请求 — 滚动更新时如果就绪探针过早通过，新实例会在预热完成前收到流量。 |  | 完整 | 无 | 无 | 无 | z11 直接说明就绪探针过早通过导致未预热实例收到流量；x18 只给固定探针配置，未解释滚动更新丢请求的原因。 |
| h23 | zh | hardneg | 重试为什么要加随机 | **z16**: 重试风暴 — 下游故障时所有客户端同时重试会放大流量，重试必须带随机抖动。 | hard negative: e09 (which calls are retried) | 完整 | 无 | 无 | 是 | z16 直接解释同时重试放大流量，因此要加抖动；e09 只限定哪些方法可重试，不解释随机化原因。 |
| h24 | zh | plain | 压测之前要做什么准备 | **z13**: 压测流程 — 压测前先冻结其他变更，使用生产数据的脱敏副本，逐步提高并发直到延迟超标。 |  | 完整 | 无 | 无 | 无 | z13 直接给出冻结变更、脱敏数据和逐步加并发；z17 只提供脚本位置，不能回答准备步骤。 |
| h25 | zh | plain | 日志里必须带哪些字段 | **z14**: 日志格式 — 日志使用 JSON 一行一条，必须包含请求编号和租户编号。 |  | 完整 | 无 | 无 | 无 | z14 明确请求编号与租户编号两个必需字段；e15 只规范采样，不增加日志必填字段。 |
| h26 | mixed | ident | egrpc_channel_pool 的 checkout 超时 | **x01**: channel_pool 的 checkout 超时 — egrpc_channel_pool 的 checkout 等待超过 500 毫秒就返回 pool_exhausted，不无限排队。 |  | 完整 | 无 | 无 | 无 | x01 直接给出 checkout 等待 500 毫秒及 pool_exhausted；e03 的启动超时和 x02 的调用超时不是 checkout。 |
| h27 | mixed | plain | gen_server:call 默认超时多久，怎么配置 | **x02**: gen_server:call 默认超时 — gen_server:call 默认 5 秒超时，调用方崩溃会连带上游，重要调用要显式设置 timeout。 |  | 少列 | z12 | 无 | 无 | x02 回答默认 5 秒及重要调用需显式 timeout；z12 还能回答超时值必须来自配置、不能硬编码，故应补列。现有 60 条均未提供 gen_server:call 的具体参数写法，此处不凭外部知识补造答案。 |
| h28 | mixed | ident | dialyzer warning 在 CI 怎么处理 | **x04**: Dialyzer 警告处理 — 新增的 dialyzer warning 在 CI 里视为失败，确实需要忽略时在代码里加 -dialyzer 属性并写明原因。 |  | 完整 | 无 | 无 | 无 | x04 直接给出 CI 失败及带原因的 -dialyzer 忽略方式；x03 仅列运行命令，不能回答 warning 的处置规则。 |
| h29 | mixed | plain | supervisor intensity 设得太小有什么风险 | **x05**: supervisor 重启强度 — supervisor 的 intensity 和 period 设得太小会让整个应用被重启，设得太大会掩盖崩溃循环。 |  | 完整 | 无 | 无 | 无 | x05 直接说明 intensity 和 period 太小导致整个应用重启；e04 讨论 ETS 生命周期，不回答强度设置风险。 |
| h30 | mixed | ident | persistent_term 更新的代价 | **x06**: 用 persistent_term 存配置 — 运行时不变的配置放进 persistent_term，更新很少，因为每次更新都会触发全局 GC。 |  | 完整 | 无 | 无 | 无 | x06 直接说明更新触发全局 GC 的代价；其他配置记忆不回答 persistent_term 更新成本。 |
| h31 | mixed | ident | grpcurl 怎么调试 | **x08**: gRPC 反射调试 — 用 grpcurl 配合 server reflection 调用接口，生产环境默认关闭 reflection。 |  | 完整 | 无 | 无 | 无 | x08 直接给出 grpcurl 配合 server reflection 调试，并说明生产默认关闭。 |
| h32 | mixed | hardneg | 新增 protobuf 字段要不要 optional | **x10**: protobuf optional 字段 — 新增的 protobuf 字段必须是 optional，并且在 .proto 里写明默认值的含义。 | hard negatives: z07, e22 (other protobuf rules) | 完整 | 无 | 无 | 是 | x10 明确新增字段必须 optional 并说明默认值；z07 是编号复用，e22 是版本递增，都不能决定 optional。 |
| h33 | mixed | ident | ETS 表 public 好不好 | **x13**: ETS 表 public 与 protected — ETS 表设为 public 会让任何进程都能写入，除非确实需要，否则用 protected。 | hard negative: e04 (ETS tables) | 完整 | 无 | 无 | 是 | x13 直接说明 public 允许任意进程写入及优先 protected；e04 讲表消失，不回答访问权限选择。 |
| h34 | mixed | para | binary 引用计数为什么内存不释放 | **x17**: 大 binary 与内存释放 — 引用计数的大 binary 在进程间传递很便宜，但长期持有会让内存迟迟不释放。 | hard negative: e11 (large messages) | 完整 | 无 | 无 | 是 | x17 明确长期持有引用计数大 binary 会延迟释放；e11 讲聚合大响应导致内存翻倍，不能解释引用计数对象未释放。 |
| h35 | mixed | hardneg | certificate_expired 怎么排查 | **x09**: certificate_expired 握手失败 — 出现 certificate_expired 通常是中间证书过期而不是服务证书，需要检查整条证书链。 | hard negative: e10 (rotate certificates) | 完整 | 无 | 无 | 是 | x09 直接给出中间证书过期及检查整条链的排查方向；e10 只讲证书轮换流程，不能定位此握手错误。 |
| h36 | cross | hardneg | 为什么要给每个上游单独建连接池 | **e01**: Separate connection pool per upstream — Each upstream service gets its own egrpc_channel_pool so one slow upstream cannot exhaust connections for the others. | zh -> en; hard negatives: z04, z05 (Chinese database pools) | 完整 | 无 | 无 | 是 | e01 直接解释上游隔离避免慢服务耗尽其他服务连接；z04、z05 的数据库规则不能回答隔离原因。 |
| h37 | cross | plain | 请求体太大会报什么错 | **e07**: Error code E1042 — E1042 means the request body exceeded the 4 MiB message limit configured on the channel. | zh -> en | 完整 | 无 | 无 | 无 | e07 给出请求体超限对应 E1042；e11 讲大响应内存问题，x16 讲错误封装，都不能提供此错误码。 |
| h38 | cross | plain | 没有权限调用某个方法返回什么码 | **e08**: Error code E2007 — E2007 means the caller's token lacks the scope required by the method. | zh -> en | 完整 | 无 | 无 | 无 | e08 给出方法 scope 不足对应 E2007；x16 只说明通用返回格式，不能回答权限不足的具体编号。 |
| h39 | cross | plain | 热升级前要检查什么 | **e05**: Hot code upgrade checklist — Build the appup file, run relup in staging, and confirm that no process holds state in an old record layout. | zh -> en | 完整 | 无 | 无 | 无 | e05 给出 appup、预发 relup 及旧 record 状态检查；e22 的 protobuf 版本规范不能替代热升级检查。 |
| h40 | cross | para | 客户端断开之后服务端为什么还在消耗 CPU | **e20**: Backpressure on streams — A server-streaming handler that ignores send errors keeps producing after the client left, wasting CPU. | zh -> en; hard negative: e02 | 完整 | 无 | 无 | 是 | e20 明确忽略发送错误会在客户端离开后持续生产、消耗 CPU；e02 讲 deadline 到期取消，不说明断开后继续工作的原因。 |
| h41 | cross | plain | 每个租户一秒最多能发多少请求 | **e13**: Rate limit per tenant — Each tenant is limited to 200 requests per second; excess requests get RESOURCE_EXHAUSTED. | zh -> en | 完整 | 无 | 无 | 无 | e13 直接给出每租户每秒 200 请求；z12 只要求阈值可配置，不能给出速率上限。 |
| h42 | cross | plain | 把请求数据转成 atom 有什么限制，应该怎么做 | **e19**: No atoms from user input — Never call binary_to_atom on request data; use binary_to_existing_atom or keep binaries. | zh -> en | 完整 | 无 | 无 | 无 | e19 同时回答禁止 binary_to_atom 及两个替代方案；x11 的进程通信模型不能回答 atom 转换限制。 |
| h43 | cross | hardneg | 证书怎么平滑轮换 | **e10**: Rotate service certificates — Issue the new certificate, deploy it next to the old one, wait for all clients to reload, then remove the old one. | zh -> en; hard negative: x09 | 完整 | 无 | 无 | 是 | e10 给出新旧证书并存、等待客户端重载后删除旧证书的完整流程；x09 的过期排查不是平滑轮换流程。 |
| h44 | cross | hardneg | gen_server 回调里能不能同步调用别的服务 | **e06**: No blocking calls in gen_server callbacks — A gen_server callback must not make a synchronous call to another busy server; use a cast or a task instead. | zh -> en; hard negative: x02 | 完整 | 无 | 无 | 是 | e06 直接限制回调同步调用忙碌服务并给出 cast/task 替代；x02 只规定调用超时，不能回答回调是否允许同步调用。 |
| h45 | cross | plain | how do we ship a new version gradually | **z01**: 灰度发布比例 — 新版本先放百分之五的流量，观察错误率半小时后再扩大到一半。 | en -> zh | 完整 | 无 | 无 | 无 | z01 直接给出灰度比例及观察窗口；z08 讲失败后回滚，不回答如何逐步发布。 |
| h46 | cross | plain | how do I find which process is eating memory | **z03**: 排查进程内存增长 — 先用 observer 查看按内存排序的进程，再检查是否有进程邮箱持续积压。 | en -> zh | 完整 | 无 | 无 | 无 | z03 直接给出 observer 按进程内存排序；e11、x17 描述内存问题但无法识别具体哪个进程，故本行不补列。 |
| h47 | cross | hardneg | what happens when certificate_expired shows up in the TLS handshake | **x09**: certificate_expired 握手失败 — 出现 certificate_expired 通常是中间证书过期而不是服务证书，需要检查整条证书链。 | en -> mixed; hard negative: e10 | 完整 | 无 | 无 | 是 | x09 的标题指出握手失败，正文说明证书链检查方向；e10 没有解释 certificate_expired 的含义或后果。 |
| h48 | cross | para | what is the availability target for core endpoints | **z15**: 服务等级目标 — 核心接口的可用性目标是百分之九十九点九，延迟 p99 低于三百毫秒。 | en -> zh | 完整 | 无 | 无 | 无 | z15 直接给出核心接口 99.9% 可用性目标；e14 的单次健康状态不等于可用性目标。 |
| h49 | cross | hardneg | when should the retry be randomised | **z16**: 重试风暴 — 下游故障时所有客户端同时重试会放大流量，重试必须带随机抖动。 | en -> zh; hard negative: e09 | 完整 | 无 | 无 | 是 | z16 明确下游故障时同步重试会放大流量，重试必须加抖动；e09 的幂等筛选不能回答何时随机化。 |
| h50 | cross | plain | which faults page the on-call person | **z09**: 告警分级 — 只有影响用户的故障才会呼叫值班人员，其余告警进入工作日处理队列。 | en -> zh | 完整 | 无 | 无 | 无 | z09 直接给出影响用户的故障才呼叫值班；z10 的交接安排不回答故障分级。 |

## 备注里提到的 hard negative 记忆

(核对"说明诚实"一列时用。)

- **z04** (constraint): 禁止在循环里建立连接 — 不允许在请求处理循环里新建数据库连接，必须从连接池借用。
- **z05** (decision): 数据库连接池大小 — 数据库连接池大小设为核心数的两倍，超出的请求排队等待而不是新建连接。
- **z16** (gotcha): 重试风暴 — 下游故障时所有客户端同时重试会放大流量，重试必须带随机抖动。
- **z18** (decision): 接口幂等键 — 写操作要求客户端传入幂等键，服务端保存二十四小时用于去重。
- **x18** (procedure): 就绪探针配置 — readiness probe 指向 /health，initialDelay 设为 10 秒，失败三次摘除流量。
- **z11** (gotcha): 滚动更新丢请求 — 滚动更新时如果就绪探针过早通过，新实例会在预热完成前收到流量。
- **x07** (fact): Prometheus 指标命名 — 指标名使用下划线，单位放在后缀，比如 request_duration_seconds。
- **e23** (fact): Default pool size — The default pool holds 8 channels per upstream, configurable with the pool_size option.
- **e01** (decision): Separate connection pool per upstream — Each upstream service gets its own egrpc_channel_pool so one slow upstream cannot exhaust connections for the others.
- **e09** (decision): Retry only idempotent calls — The client retries a failed call up to twice, but only when the method is marked idempotent in the protobuf options.
- **z07** (gotcha): protobuf 字段编号复用 — 删除字段后不能复用编号，旧客户端会把新数据解析成错误的类型。
- **e22** (decision): Semantic versioning — Breaking protobuf changes bump the major version; field additions bump the minor version.
- **e04** (gotcha): badarg from ets after a supervisor restart — ETS tables owned by a crashed process vanish; callers then get badarg until the owner is restarted and recreates the table.
- **e11** (gotcha): Large messages and memory — Streaming a 100 MB response into a single binary doubles memory use; stream in chunks instead.
- **e10** (procedure): Rotate service certificates — Issue the new certificate, deploy it next to the old one, wait for all clients to reload, then remove the old one.
- **e02** (decision): Use gRPC deadlines, not socket timeouts — Set a deadline on every call; the server cancels work when it passes. Socket-level timeouts are left at their defaults.
- **x09** (gotcha): certificate_expired 握手失败 — 出现 certificate_expired 通常是中间证书过期而不是服务证书，需要检查整条证书链。
- **x02** (gotcha): gen_server:call 默认超时 — gen_server:call 默认 5 秒超时，调用方崩溃会连带上游，重要调用要显式设置 timeout。

## 附录:全部 60 条记忆

| key | 类型 | 标题 | 内容 |
|---|---|---|---|
| e01 | decision | Separate connection pool per upstream | Each upstream service gets its own egrpc_channel_pool so one slow upstream cannot exhaust connections for the others. |
| e02 | decision | Use gRPC deadlines, not socket timeouts | Set a deadline on every call; the server cancels work when it passes. Socket-level timeouts are left at their defaults. |
| e03 | gotcha | ETIMEDOUT during pool warm-up | Starting the pool before the upstream DNS record exists produces ETIMEDOUT and the pool never retries on its own. |
| e04 | gotcha | badarg from ets after a supervisor restart | ETS tables owned by a crashed process vanish; callers then get badarg until the owner is restarted and recreates the table. |
| e05 | procedure | Hot code upgrade checklist | Build the appup file, run relup in staging, and confirm that no process holds state in an old record layout. |
| e06 | constraint | No blocking calls in gen_server callbacks | A gen_server callback must not make a synchronous call to another busy server; use a cast or a task instead. |
| e07 | fact | Error code E1042 | E1042 means the request body exceeded the 4 MiB message limit configured on the channel. |
| e08 | fact | Error code E2007 | E2007 means the caller's token lacks the scope required by the method. |
| e09 | decision | Retry only idempotent calls | The client retries a failed call up to twice, but only when the method is marked idempotent in the protobuf options. |
| e10 | procedure | Rotate service certificates | Issue the new certificate, deploy it next to the old one, wait for all clients to reload, then remove the old one. |
| e11 | gotcha | Large messages and memory | Streaming a 100 MB response into a single binary doubles memory use; stream in chunks instead. |
| e12 | decision | Mnesia is not used for session data | Session data lives in Redis because mnesia schema changes are hard to roll out across nodes. |
| e13 | constraint | Rate limit per tenant | Each tenant is limited to 200 requests per second; excess requests get RESOURCE_EXHAUSTED. |
| e14 | fact | Health endpoint | The /health endpoint returns 200 only when the pool has at least one ready channel. |
| e15 | gotcha | Log sampling hides errors | With 1 percent log sampling, rare error classes can disappear from dashboards; always log errors unsampled. |
| e16 | procedure | Reproduce a flaky test | Run the test 200 times with a fixed random seed, then bisect by removing concurrent tests. |
| e17 | decision | Telemetry event names | Event names use the form egrpc.client.<event>.<measurement>, never dynamic values in the name. |
| e18 | fact | Supported OTP versions | The library is tested against the two latest OTP major releases. |
| e19 | constraint | No atoms from user input | Never call binary_to_atom on request data; use binary_to_existing_atom or keep binaries. |
| e20 | gotcha | Backpressure on streams | A server-streaming handler that ignores send errors keeps producing after the client left, wasting CPU. |
| e21 | procedure | Capture a crash dump | Set ERL_CRASH_DUMP to a writable directory and analyse it with the crashdump viewer. |
| e22 | decision | Semantic versioning | Breaking protobuf changes bump the major version; field additions bump the minor version. |
| e23 | fact | Default pool size | The default pool holds 8 channels per upstream, configurable with the pool_size option. |
| e24 | gotcha | DNS caching | Channel resolution is cached for 30 seconds, so a changed record is picked up only after that. |
| z01 | decision | 灰度发布比例 | 新版本先放百分之五的流量，观察错误率半小时后再扩大到一半。 |
| z02 | gotcha | 时钟回拨 | 虚拟机迁移后系统时钟可能回拨，依赖单调时间以外的超时判断会提前触发。 |
| z03 | procedure | 排查进程内存增长 | 先用 observer 查看按内存排序的进程，再检查是否有进程邮箱持续积压。 |
| z04 | constraint | 禁止在循环里建立连接 | 不允许在请求处理循环里新建数据库连接，必须从连接池借用。 |
| z05 | decision | 数据库连接池大小 | 数据库连接池大小设为核心数的两倍，超出的请求排队等待而不是新建连接。 |
| z06 | procedure | 备份与恢复演练 | 每季度做一次数据库恢复演练，记录耗时和遇到的问题。 |
| z07 | gotcha | protobuf 字段编号复用 | 删除字段后不能复用编号，旧客户端会把新数据解析成错误的类型。 |
| z08 | procedure | 回滚失败的发布 | 发现错误率上升时先切回上一个版本，再分析日志，不要在线上直接修补。 |
| z09 | decision | 告警分级 | 只有影响用户的故障才会呼叫值班人员，其余告警进入工作日处理队列。 |
| z10 | fact | 值班交接 | 值班交接在周一上午进行，交接文档记录未关闭的告警和已知风险。 |
| z11 | gotcha | 滚动更新丢请求 | 滚动更新时如果就绪探针过早通过，新实例会在预热完成前收到流量。 |
| z12 | constraint | 配置不得硬编码 | 超时、重试次数和限流阈值必须来自配置，代码里不能写死。 |
| z13 | procedure | 压测流程 | 压测前先冻结其他变更，使用生产数据的脱敏副本，逐步提高并发直到延迟超标。 |
| z14 | decision | 日志格式 | 日志使用 JSON 一行一条，必须包含请求编号和租户编号。 |
| z15 | fact | 服务等级目标 | 核心接口的可用性目标是百分之九十九点九，延迟 p99 低于三百毫秒。 |
| z16 | gotcha | 重试风暴 | 下游故障时所有客户端同时重试会放大流量，重试必须带随机抖动。 |
| z17 | fact | 代码仓库布局 | 核心库在 apps 目录，示例和压测脚本放在 tools 目录。 |
| z18 | decision | 接口幂等键 | 写操作要求客户端传入幂等键，服务端保存二十四小时用于去重。 |
| x01 | decision | channel_pool 的 checkout 超时 | egrpc_channel_pool 的 checkout 等待超过 500 毫秒就返回 pool_exhausted，不无限排队。 |
| x02 | gotcha | gen_server:call 默认超时 | gen_server:call 默认 5 秒超时，调用方崩溃会连带上游，重要调用要显式设置 timeout。 |
| x03 | fact | rebar3 常用命令 | rebar3 ct 运行 Common Test，rebar3 dialyzer 做类型检查，rebar3 as prod release 打包。 |
| x04 | procedure | Dialyzer 警告处理 | 新增的 dialyzer warning 在 CI 里视为失败，确实需要忽略时在代码里加 -dialyzer 属性并写明原因。 |
| x05 | gotcha | supervisor 重启强度 | supervisor 的 intensity 和 period 设得太小会让整个应用被重启，设得太大会掩盖崩溃循环。 |
| x06 | decision | 用 persistent_term 存配置 | 运行时不变的配置放进 persistent_term，更新很少，因为每次更新都会触发全局 GC。 |
| x07 | fact | Prometheus 指标命名 | 指标名使用下划线，单位放在后缀，比如 request_duration_seconds。 |
| x08 | procedure | gRPC 反射调试 | 用 grpcurl 配合 server reflection 调用接口，生产环境默认关闭 reflection。 |
| x09 | gotcha | certificate_expired 握手失败 | 出现 certificate_expired 通常是中间证书过期而不是服务证书，需要检查整条证书链。 |
| x10 | constraint | protobuf optional 字段 | 新增的 protobuf 字段必须是 optional，并且在 .proto 里写明默认值的含义。 |
| x11 | fact | OTP 并发模型 | 每个请求一个 Erlang 进程，进程之间不共享内存，靠消息传递通信。 |
| x12 | decision | Docker 镜像基础 | 运行时镜像基于 alpine 精简版，构建镜像包含完整的 rebar3 和编译器。 |
| x13 | gotcha | ETS 表 public 与 protected | ETS 表设为 public 会让任何进程都能写入，除非确实需要，否则用 protected。 |
| x14 | procedure | 分析 fprof 结果 | fprof 开销很大，只在预发环境对单个进程启用，分析 own 时间最高的函数。 |
| x15 | fact | Redis 会话过期 | Redis 里的会话键 TTL 是 30 分钟，每次请求会续期。 |
| x16 | decision | 错误码返回规范 | 对外错误统一返回 gRPC status 加 E 开头的业务码，详情放在 trailer 里。 |
| x17 | gotcha | 大 binary 与内存释放 | 引用计数的大 binary 在进程间传递很便宜，但长期持有会让内存迟迟不释放。 |
| x18 | procedure | 就绪探针配置 | readiness probe 指向 /health，initialDelay 设为 10 秒，失败三次摘除流量。 |

## 汇总(填完后计算)

- 需要增加相关记忆的查询:**2 / 50**（h17、h27，共增加 3 个查询—记忆关联）    需要删除的:**0 / 50**    说明不诚实:**0**。
- 完整 **48 / 50**，少列 **2 / 50**；hard negative 说明：是 **17**、否 **0**、无 **33**。“无”包括仅标记语言方向或写着“both answer it”的备注。
- 复核口径：以 `holdout.json` 和本页 60 条记忆的标题、内容为依据，能直接回答查询的实质部分即可相关；只有主题重叠、需要额外假设才能回答的记忆不补列。h17 是宽泛的排查问题，因此纳入具体内存成因；h46 明确问如何定位进程，因此不纳入这些成因。h27 的配置约束可回答配置要求，但不能提供集合中不存在的调用参数示例。
- 以上为助手复核建议，待用户确认；只填写本清单，尚未修改 `holdout.json` 的标注，也未重跑评估。若采纳并修改正式标注，须按上文要求重新计算留出集指标。

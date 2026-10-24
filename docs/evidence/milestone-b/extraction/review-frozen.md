# B-16 冻结验证集的忠实度复核表(提示词改动前后)

回答的问题:**改了提示词之后,在没有参与过选提示词的新会话上,不忠实的候选有没有减少**。这批会话是在改提示词之前写好并提交的(`sessions-frozen.json`),提示词只改了一次,没有看过结果再调。

## 冻结的对象

- 会话:`sessions-frozen.json`(17 个会话:9 个有知识,8 个无知识),blob `6b707bfa86b3851f5e81a0b9c4836f876ad90666`,提交 `b8fb370`。
- 改前:`prompt.rs` blob `d1e2e0bf6c4be481eb2f1eb21563f3c9602e48b7`,输出 `frozen-before.md`;改后:`prompt.rs` blob `58b5d66ddda1bc0f083316d0456cf7c5dcc9d71b`,输出 `frozen-after.md`。校验 `validate.rs`、模型 `qwen3:8b`(温度 0)、提取窗口都没变。
- 提示词改动只有三处:①"自包含,说明为什么"改成"只有事件给出了原因才写,没有就省略";②"助手做的/报告的是助手的,不要写成用户或团队的;助手报告的结果仍是那一次的报告,不要变成保证";③"保持事件显示的范围:一次修复、一个文件、一次运行不是对所有的规则;没有事件说过就不要说标准、已知、必须"。

## 怎么判

沿用 `review-current.md` 的规则和**已固定的三个口径**:
- 每条候选拆成断言(做了什么、谁提出或决定、为什么、范围、数字/命令/否定),标题、正文、引用一起看;引用状态记 直接/遗漏/无/矛盾。
- 忠实 / 部分忠实 / 不忠实 / 待裁定;部分忠实和不忠实都算失败。
- 口径 1:助手明确报告的结果可以支持转述;只有新增"已独立验证"、扩大结果或更强结论才算偏差。口径 2:措辞按"增加了什么断言"判断,不按词触发。口径 3:具体修复可以忠实,但写成普遍保证算范围扩大。
- 内容忠实但不值得保存是噪声,另记,不算幻觉。

**同一条候选在改前、改后都出现时只需要判一次**("出现在"一列标明);"仅前"是只在改前输出里出现的。请先独立判断(A、B),再处理分歧。

## 复核表

| # | 会话 | 出现在 | 类型 | 标题 | 正文 | 引用事件 | 断言及对应证据(填) | 引用状态 | A 判定 | B 判定 | 最终裁定 | 偏差类型 | 最小修正 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | g01-rule-with-incident | 后 | decision | Use repo package for database queries | Handlers must not touch sql.DB directly; all queries go through the repo package. This rule was established after a previous outage caused by leaking connections from handlers. | p1 | p1 直接支持 handler 禁止直接访问 sql.DB、查询走 repo，以及过去因连接泄漏发生故障；没有说明该规则是在故障之后才建立，候选补出了规则形成的时间关系。 | 直接；无（规则建立时间） | 部分忠实 | 部分忠实：p1 只把事故作为规则背景，没有说规则是在事故之后才建立；“established after”补出了形成时间与因果。 | 部分忠实 | 推断未出现事实 | 删除“This rule was established after”，改为“用户还说明过去曾因 handler 泄漏连接导致故障”；规则应归为 constraint，类型另记。 |
| 2 | g01-rule-with-incident | 后 | procedure | Implement GET /orders/:id endpoint | Added the endpoint by calling h.repo.GetOrder in the handler file. The change was verified with tests that passed without issues. | e1, b1 | e1 显示 handler 调用 h.repo.GetOrder；b1 明确 handlers 包测试 ok。端点名称和新增端点的完成说明在未引用的 p1/a1；测试通过有工具结果，不涉及把助手报告升级为独立验证。 | 直接；遗漏（端点名称及完成说明在 p1/a1） | 忠实 | 忠实：e1 支持 handler 调用 repo.GetOrder，b1 的工具输出 ok 直接支持“测试通过”；端点名称未引 p1/a1 属引用遗漏。 | 忠实 | 引用错误 | 补引 p1/a1；限定验证范围为 go test ./internal/handlers 通过，例行实施记录另记为噪声。 |
| 3 | g01-rule-with-incident | 仅前 | constraint | Handlers must use repo package for database queries | The user explicitly stated that handlers never touch sql.DB directly; every query must go through the repo package. This rule was established to prevent connection leaks and outages, as seen in past incidents. | p1 | p1 直接陈述服务分层规则，并紧接着给出过去 handler 泄漏连接造成故障的教训；将避免同类连接泄漏和故障概述为该规则目的，有上下文支持，不增加规则形成的具体时间或次数。 | 直接 | 忠实 | 忠实：规则、事故背景与目的均可由 p1 相邻陈述推出，未增加规则形成时间；“past incidents”略宽于 once，属措辞不精确，未新增可用断言。 | 忠实 | 无 | 无需事实修正；“past incidents”可改成单次既往事故，与 once 更精确对应。 |
| 4 | g01-rule-with-incident | 仅前 | procedure | Implementing a GET /orders/:id endpoint | To implement the GET /orders/:id endpoint, the handler was updated to call repo.GetOrder through the repo package. This ensures compliance with the rule that handlers never touch sql.DB directly, avoiding potential connection leaks. | p1, e1, b1 | p1 支持分层规则与过去泄漏事故，e1 支持本次 handler 调用 repo；但仅调用 repo 不能保证实现不会泄漏连接，“avoiding potential connection leaks”把设计目的写成了该改动的保证性效果。 | 直接；无（避免泄漏的效果保证） | 部分忠实 | 部分忠实：调用 repo 只表明本次符合分层规则，不能推出该改动“避免连接泄漏”的效果。 | 部分忠实 | 补动机/收益 | 改为“本次 handler 调用 repo.GetOrder，符合用户要求的访问分层；用户以曾发生连接泄漏为该规则的背景”，不保证避免泄漏。 |
| 5 | g02-naive-aware-datetime | 后 | gotcha | Naive vs Aware Datetime Comparison | Using datetime.utcnow() leads to a TypeError when comparing with timezone-aware datetimes from the database. The error occurs because one datetime is naive (no timezone) and the other is aware (has timezone). | b2, a1 | b2 明确 naive/aware 比较的 TypeError；a1 明确 utcnow 返回 naive、数据库时间戳为 aware，以及比较失败的原因；适用条件保留完整。 | 直接 | 忠实 | 忠实：b2 直接给出 naive/aware 比较的 TypeError，a1 说明 utcnow 返回 naive、库内时间戳 aware，原因与适用范围一致。 | 忠实 | 无 | 无需事实修正。 |
| 6 | g02-naive-aware-datetime | 仅前 | gotcha | Naive vs Aware Datetime Comparison | The expiry check raises a TypeError in production due to comparing naive and timezone-aware datetimes. The local tests passed because the fixture used naive dates, but the production environment uses timezone-aware timestamps from the database. | p1, b2, a1 | p1 支持生产失败而本地通过；b2 支持异常；a1 明确数据库 aware 时间戳与本地 naive fixture 的差别和原因。助手明确解释可支持会话转述，不要求额外独立验证。 | 直接 | 忠实 | 忠实：p1 的生产失败/本地通过、b2 的异常与 a1 的 fixture 解释一致，属对助手明确解释的转述。 | 忠实 | 无 | 无需事实修正。 |
| 7 | g03-decision-env-flags | 后 | decision | Use environment variables for feature flags | The user decided to read feature flags from environment variables instead of using a config service. | p1 | p1 明确用户决定从环境变量读 feature flags、拒绝 config service；省略已给出的理由不造成反转或新增断言。 | 直接 | 忠实 | 忠实：p1 明确用户决定用环境变量替代 config service；省略理由不构成新增断言。 | 忠实 | 无 | 无需事实修正。 |
| 8 | g03-decision-env-flags | 后 | procedure | Implement flag check using environment variables | A function 'enabled' was added to check if a flag is active by reading the corresponding environment variable. | e1 | e1 直接显示新增 enabled(name)，读取 FLAG_ 前缀环境变量并以是否等于 1 判断；描述保留了读取变量判断开关的核心机制。 | 直接 | 忠实 | 忠实：e1 直接显示 enabled() 读取 FLAG_ 前缀变量并比较，描述未超出。 | 忠实 | 无 | 无需事实修正；本次实现记录的持久性另评。 |
| 9 | g03-decision-env-flags | 后 | fact | Flags are retrieved from environment variables | The assistant confirmed that flags are read from environment variables in the format FLAG_<NAME>. | a1 | a1 直接报告 flags 从 FLAG_<NAME> 环境变量读取；候选保留助手主体，没有扩大成独立验证。 | 直接 | 忠实 | 忠实：a1 原句即 FLAG_<NAME>，主体保留为助手报告。 | 忠实 | 无 | 无需事实修正。 |
| 10 | g03-decision-env-flags | 仅前 | decision | Use environment variables for feature flags | The user decided to read feature flags from environment variables instead of using a config service. The reason given was that the team consists of three developers and the user does not want to run and monitor an additional service. | p1 | p1 明确环境变量方案、拒绝额外配置服务、团队三人，以及不愿增加运行监控负担；原因、数字、归因均一致。 | 直接 | 忠实 | 忠实：团队三人、不愿多跑并监控一个服务的原因与 p1 完全一致。 | 忠实 | 无 | 无需事实修正。 |
| 11 | g03-decision-env-flags | 仅前 | procedure | Implement flag check function | A function named 'enabled' was added to the flags module. It checks if an environment variable named FLAG_<NAME> is set to '1'. This provides a simple and direct way to determine if a feature is enabled. | e1 | e1 直接支持 enabled 函数、FLAG_ 加大写 name、与字符串 1 比较；“simple and direct”仅概述这段直接读取并比较的实现，没有补出决策动机或额外能力。 | 直接 | 忠实 | 忠实：e1 支持函数名、FLAG_ 前缀与等于 "1" 的判断；“simple and direct”只是对该实现的评价。 | 忠实 | 无 | 无需事实修正；可删去评价性措辞并明确 NAME 取大写。 |
| 12 | g03-decision-env-flags | 仅前 | fact | Flags are retrieved from environment variables | The assistant confirmed that flags are read from environment variables in the format FLAG_<NAME>. This is the method that was implemented and agreed upon. | a1 | a1 支持助手报告的变量命名格式；e1 支持已实施，p1 支持用户明确要求环境变量方案，但后二者没有被引用。“agreed upon”按用户已明确要求该方案理解，不推断其逐行审阅实现。 | 直接；遗漏（实施及用户要求在 e1/p1） | 忠实 | 忠实：a1 支持命名格式，e1/p1 支持已实施与用户要求（未引用，属引用遗漏）。 | 忠实 | 引用错误 | 补引 e1/p1；改为“按用户要求实施了环境变量读取方案，助手报告使用 FLAG_<NAME> 格式”。 |
| 13 | g04-procedure-db-reset | 后 | gotcha | db-reset must precede seed to avoid schema conflicts | Running 'make seed' without 'make db-reset' first causes an error because the 'users' table already exists. The seed script assumes an empty schema. | b1, b2, a1 | b1 显示 users 已存在；b2 显示 reset 后 seed 成功；a1 明确 seed 假设空 schema。本条在该本地数据库及脚本语境下成立，没有推广到所有数据库初始化工具。 | 直接 | 忠实 | 忠实：b1 的 users 已存在、b2 的 reset 后成功与 a1 的空 schema 假设一致，未推广到其它工具。 | 忠实 | 无 | 无需事实修正；可补“本项目既有本地库”以明确上下文。 |
| 14 | g04-procedure-db-reset | 仅前 | gotcha | db-reset must precede seed to avoid schema conflicts | Running 'make seed' without first executing 'make db-reset' causes errors because the seed script assumes an empty schema. The 'users' table already exists, leading to the 'relation already exists' error. The correct order is to reset the database first, then seed it. | b1, b2, a1 | b1/b2/a1 直接支持既有数据库 seed 失败、空 schema 假设、先 reset 后 seed 的成功顺序及报错；未添加与本项目无关的通用要求。 | 直接 | 忠实 | 忠实：报错原文、空 schema 假设与先 reset 后 seed 的顺序均有 b1/b2/a1 直接支持。 | 忠实 | 无 | 无需事实修正。 |
| 15 | g05-fact-region | 后 | constraint | Deploy script must use eu-west-1 region | The user specified that all deployments are in eu-west-1. The deploy script was updated to set AWS_REGION=eu-west-1 to ensure compliance with this rule. | p1, e1 | p1 明确所有基础设施仅在 eu-west-1，并要求写 deploy script 时遵守；e1 直接设置 AWS_REGION=eu-west-1。此处合规指该地域配置，不保证脚本全部行为正确。 | 直接 | 忠实 | 忠实：p1 的 eu-west-1 全域要求与 e1 的 AWS_REGION 设置一致，“合规”仅指该地域配置。 | 忠实 | 无 | 无需事实修正。 |
| 16 | g05-fact-region | 后 | fact | Deploy script sets AWS_REGION | The deploy script sets the AWS_REGION environment variable to eu-west-1 as per the user's instruction. | a1 | a1 直接说明脚本设置 AWS_REGION；“按用户要求”由完整会话 p1 支持，但当前引用遗漏 p1。 | 直接；遗漏（用户要求在 p1） | 忠实 | 忠实：a1 直接支持脚本设置 AWS_REGION，“按用户要求”由 p1 支持（未引用）。 | 忠实 | 引用错误 | 补引 p1；保留 eu-west-1，不扩大到未展示的部署行为。 |
| 17 | g05-fact-region | 仅前 | constraint | Deploy only in eu-west-1 region | The user explicitly stated that all deployments must be in the eu-west-1 region. This is a hard constraint to follow in future work. | p1 | p1 明确 everything we run 仅在 eu-west-1、不要假设 us-east-1，且以写 deploy script 为上下文；概述为后续部署地域约束有直接支持。 | 直接 | 忠实 | 忠实：p1 的“everything we run is in eu-west-1 only / 不要假设 us-east-1”直接支持该约束及其后续适用。 | 忠实 | 无 | 无需事实修正。 |
| 18 | g06-jest-esm | 后 | gotcha | ESM module import error in Jest | Jest 默认不转换 node_modules 中的文件，当使用 ESM 模块（如 lodash-es）时，会报 SyntaxError: Cannot use import statement outside a module 错误。 | p1, b1 | p1/b1 支持本次 import 语法错误及 lodash-es 路径；Jest 默认不转换 node_modules 的原因只在未引用的 a1。正文按本次 Jest 环境与 lodash-es 场景理解，不推成所有配置必然失败。 | 直接；遗漏（默认转换行为在 a1） | 忠实 | 忠实：p1/b1 支持本次 import 报错与 lodash-es 路径，默认不转换 node_modules 的解释在 a1（未引用）。 | 忠实 | 引用错误 | 补引 a1；可写成“本次 Jest 配置下，lodash-es 未转换导致 import 语法错误”。 |
| 19 | g06-jest-esm | 后 | procedure | Allow ESM module in Jest | 在 jest.config.js 中配置 transformIgnorePatterns，放行特定的 ESM 模块，如 lodash-es，以避免 import 语法错误。 | e1, b2, a1 | e1 的负向前瞻将 lodash-es 从 node_modules 的忽略范围中放行；b2 明确 14 passed；a1 解释本次修复。具体配置修复可忠实，不因写成步骤自动算泛化。 | 直接 | 忠实 | 忠实：e1 的负向前瞻放行 lodash-es，b2 的 14 passed 与 a1 的解释一致，属具体修复步骤。 | 忠实 | 无 | 无需事实修正；限定本次项目配置即可。 |
| 20 | g06-jest-esm | 后 | fact | lodash-es is ESM only | lodash-es 是一个仅发布为 ESM 模块的库，不兼容 CommonJS 模块格式，因此在 Jest 中需要特殊处理。 | a1 | a1 支持 lodash-es 仅发布 ESM 及本次 Jest 需要转换；但“不兼容 CommonJS 模块格式”未限定为本次未转换的加载方式，扩大成库与 CommonJS 之间的普遍不兼容断言，事件没有说明经转换或互操作是否可用。 | 直接；无（普遍不兼容） | 部分忠实 | 部分忠实：a1 只说“只发布 ESM、默认不转换 node_modules”，候选补出与 CommonJS 的普遍不兼容结论。 | 部分忠实 | 范围扩大、推断未出现事实 | 删除普遍不兼容结论，改为“助手解释 lodash-es 仅发布 ESM，本次 Jest 默认不转换 node_modules，需要配置放行转换”。 |
| 21 | g06-jest-esm | 仅前 | gotcha | ESM模块在Jest中需特殊处理 | lodash-es 是一个ES模块（ESM），而 Jest 默认不会转换 node_modules 中的文件。当尝试在非模块环境中使用 import 语句时会报错。通过在 jest.config.js 中配置 transformIgnorePatterns，可以排除对 lodash-es 的转换限制，从而解决该问题。 | b1, a1 | a1 支持 lodash-es 为 ESM、默认不转换 node_modules、import 错误及配置放行后的结果；b1 支持实际报错。无需独立验证助手已明确报告的修复结果。 | 直接 | 忠实 | 忠实：ESM、默认不转换、报错与放行后的结果均见 b1/a1，“排除转换限制”的方向正确。 | 忠实 | 无 | 无需事实修正；可补引 e1 展示实际配置。 |
| 22 | g06-jest-esm | 仅前 | procedure | 解决Jest中ESM模块导入错误 | 当使用ESM模块（如 lodash-es）时，Jest 默认不会处理 node_modules 中的文件，导致 import 语句报错。解决方法是在 jest.config.js 中配置 transformIgnorePatterns，将 lodash-es 添加到忽略转换的列表中，以允许 Jest 正常解析该模块。 | e1, a1 | e1 为 /node_modules/(?!lodash-es)，作用是让 lodash-es 不落入忽略转换范围；a1 明确“放行”。候选却要求将 lodash-es 加入忽略转换列表，操作方向与证据相反。 | 直接；矛盾（配置方向） | 部分忠实 | 部分忠实：e1 是把 lodash-es 从忽略范围中放行，候选写成“加入忽略转换列表”，操作方向与证据相反。 | 部分忠实 | 命令/数字/否定错 | 将“添加到忽略转换的列表中”改为“从 node_modules 的忽略转换范围中排除，使其能够被转换”，保留具体负向前瞻配置。 |
| 23 | g07-never-amend-pushed | 后 | decision | Never amend or force-push existing commits | The user instructed to never amend or force-push commits that are already pushed. Instead, a new commit should be made to preserve reviewers' diff context. | p2 | p2 直接支持仅禁止 amend/force-push 已推送提交、改用新提交及保留审阅位置的理由；标题 existing commits 没有 already pushed 限定，扩大到所有已有提交。 | 直接；无（标题扩大范围） | 部分忠实 | 部分忠实：正文限定“已推送提交”，标题却写成所有 existing commits，扩大适用范围。 | 部分忠实 | 范围扩大 | 标题改为“Never amend or force-push already-pushed commits”，正文可保留。 |
| 24 | g07-never-amend-pushed | 后 | procedure | Fix typo and commit | The user asked to fix a typo in the README and commit it. The assistant ran `git commit -am 'fix typo'` to accomplish this. | p1, b1, a1 | p1 明确 README typo 与提交请求；b1 是对应 git commit 命令；a1 报告新提交完成。执行主体是助手，引用与归因一致；不要求独立验证助手完成报告。 | 直接 | 忠实 | 忠实：p1 的请求、b1 的提交命令与 a1 的完成报告一致，执行主体归属正确。 | 忠实 | 无 | 无需事实修正；一次操作记录的噪声另记。 |
| 25 | g07-never-amend-pushed | 仅前 | decision | Never amend or force-push existing commits | The user explicitly stated that future work must avoid amending or force-pushing commits that are already pushed. This ensures reviewers retain their place in the diff and prevents confusion in the commit history. | p2 | p2 支持已推送提交的禁令及审阅 diff 位置的原因；标题将其扩大为所有 existing commits；“prevents confusion in the commit history”还补出未陈述的提交历史收益。 | 直接；无（标题范围及历史收益） | 部分忠实 | 部分忠实：标题同样删掉“已推送”限定，并补出“避免提交历史混乱”这一未陈述收益。 | 部分忠实 | 补动机/收益、范围扩大 | 标题限定 already-pushed，删除提交历史不混乱的收益，只保留审阅者不会失去 diff 位置的理由。 |
| 26 | g07-never-amend-pushed | 仅前 | procedure | Fix typo and commit as a new commit | The user fixed a typo in the README and committed it as a new commit, avoiding any amendment or force-push of existing commits. This approach maintains a clean and traceable commit history. | p1, b1, a1 | p1 是用户下达任务，b1 是助手运行提交命令，a1 是助手完成回复；“The user fixed ... committed”错误归因。没有事件说明保持 clean and traceable history 的收益。 | 直接；矛盾（执行主体）；无（历史收益） | 部分忠实 | 部分忠实：p1 是用户下达任务、b1 是助手执行，候选写成“用户修复并提交”，归因错误；另补提交历史收益。 | 部分忠实 | 错误归因、补动机/收益 | 改为“用户要求修复 README 拼写并提交；助手报告已创建新提交”，删除提交历史收益及未展示的全面操作保证。 |
| 27 | g08-flaky-port | 后 | gotcha | Flaky test due to port collision | The test 'server_starts' was flaky because multiple tests bound to the fixed port 8080, causing collisions when run in parallel. Binding to port 0 allows the OS to select a free port automatically. | t1, t2, a1 | t1/t2 支持全量失败而单测通过；a1 明确两测试并行占用 8080 导致冲突，以及端口 0 由 OS 分配空闲端口；不是模型自行补因果。 | 直接 | 忠实 | 忠实：t1/t2 的全量失败与单测通过、a1 的 8080 并行冲突与端口 0 解释互相支持。 | 忠实 | 无 | 无需事实修正。 |
| 28 | g08-flaky-port | 后 | procedure | Fix flaky test by using dynamic port | To fix the flaky 'server_starts' test, change the port binding from '127.0.0.1:8080' to '127.0.0.1:0'. This allows the OS to assign a free port, preventing port collision during parallel test runs. | e1, a1 | e1 直接显示本次 server_starts 对应绑定由 127.0.0.1:8080 改为 :0；a1 给出动态空闲端口与并行冲突解释。防止冲突按本次固定端口竞争理解，不保证消除全部 flaky 原因。 | 直接 | 忠实 | 忠实：e1 直接显示 8080→0 的绑定改动，a1 解释 OS 分配空闲端口，结论限于本次测试。 | 忠实 | 无 | 无需事实修正。 |
| 29 | g08-flaky-port | 仅前 | gotcha | Flaky test due to port collision | The test 'server_starts' was flaky because multiple tests bound to the fixed port 8080, causing collisions when run in parallel. The solution was to bind to port 0, allowing the OS to assign a free port automatically. | t1, t2, e1, a1 | t1/t2/e1/a1 分别支持表现、具体改动、原因与方案；“solution”可转述助手已说明的本次修复，无需另有改后测试结果才算会话忠实。 | 直接 | 忠实 | 忠实：表现、改动、原因与方案分别有 t1/t2/e1/a1 支持，“solution”指本次修复。 | 忠实 | 无 | 无需事实修正。 |
| 30 | g08-flaky-port | 仅前 | procedure | Fix flaky test by using dynamic port binding | To resolve flaky tests caused by port collisions, modify the test code to bind to port 0. This allows the operating system to assign a free port, preventing conflicts when multiple tests run in parallel. | e1, a1 | e1/a1 支持由固定端口冲突引起的测试问题改绑端口 0，及 OS 选空闲端口机制；句首保留了 port collisions 条件，是具体故障类型的修复概述，不是对所有测试的确定性保证。 | 直接 | 忠实 | 忠实：句首保留“port collisions”条件，属具体故障类型的修复步骤，不是普遍确定性保证。 | 忠实 | 无 | 无需事实修正；可进一步限定为本次 server_starts。 |
| 31 | g09-deploy-freeze | 后 | constraint | Release planning must avoid Thursday 14:00-18:00 CET | Deploys are frozen every Thursday from 14:00 to 18:00 CET due to the weekly batch job. | p1 | p1 明确每周四 14:00–18:00 CET、部署冻结及每周 batch job 原因；标题和正文保留时间窗口，没有扩大为全天。 | 直接 | 忠实 | 忠实：每周四 14:00–18:00 CET、冻结与每周 batch job 原因均与 p1 一致。 | 忠实 | 无 | 无需事实修正。 |
| 32 | g09-deploy-freeze | 仅前 | constraint | Release planning must avoid Thursday deployments | Deployments are frozen every Thursday from 14:00 to 18:00 CET due to the weekly batch job. This is a hard constraint to follow. | p1 | 正文保留 p1 的每周四 14:00–18:00 CET，但标题“avoid Thursday deployments”扩大成避开周四所有部署；标题同样参与判定。 | 直接；无（标题全天禁令） | 部分忠实 | 部分忠实：正文保留窗口，标题却写成“avoid Thursday deployments”，扩大成全天禁令。 | 部分忠实 | 范围扩大 | 标题补上 14:00–18:00 CET，仅禁该窗口，正文可保留。 |
| 33 | h05-fix-without-cause | 后(前也有) | gotcha | Empty string returns empty list | The test failed because the function did not handle empty input. The fix was to add a check for an empty string and return an empty list. | p1, b1, e1, b2 | p1/b1 支持空输入测试失败，e1 支持新增 if not s: return []，b2 支持改后 6 项测试通过；没有旧实现、失败详情或明确原因说明，新增检查不能单独证明原测试失败必定因为未处理空输入。 | 直接；无（确定的原始根因） | 部分忠实 | 忠实：b1 的失败、e1 新增的空输入处理与 b2 的 6 passed 直接蕴含“原实现未处理空输入”，未补其它根因。 | 部分忠实 | 推断未出现事实 | 删除“The test failed because ...”，改为“为修复 test_parse_empty，新增空输入返回空列表的检查；改后 parse 测试文件 6 项通过”。 |
| 34 | h06-offline-go-test | 后 | gotcha | GOPROXY=off required for tests in sandbox | The tests failed initially due to network restrictions in the sandbox environment, which prevented access to the Go module proxy. Setting GOPROXY=off bypasses this issue by disabling the proxy lookup. | b1, b2, a1 | b1/a1 支持本次沙箱网络问题，b2 支持带 GOFLAGS=-mod=mod、GOPROXY=off 的命令成功；正文可概述助手报告的这次绕过方式，但标题 required 将一次成功应急操作提升为沙箱测试的必要条件。 | 直接；无（必要性） | 部分忠实 | 部分忠实：一次成功应急（还依赖 GOFLAGS=-mod=mod）不能支持标题的“required”。 | 部分忠实 | 范围扩大、泛化 | 标题改为“本次沙箱测试通过关闭 GOPROXY 绕过代理访问”；若给出可执行命令，保留 b2 的 GOFLAGS=-mod=mod；环境噪声另记。 |
| 35 | h06-offline-go-test | 仅前 | gotcha | GOPROXY must be disabled in sandboxed environments | When running Go tests in a sandboxed environment with no network access, the GOPROXY must be set to 'off' to avoid failures due to unreachable module proxies. The default proxy lookup fails in such environments. | b1, b2, a1 | b1/b2/a1 只支持本次沙箱代理不可达及组合命令成功；标题和正文将 GOPROXY=off 变成所有无网络沙箱 Go 测试的必选项，没有排除本地缓存、其他代理配置等可能。 | 直接；无（通用必要性） | 部分忠实 | 部分忠实：把本次代理不可达写成所有无网络沙箱的“must”，排除了本地缓存等其它可能。 | 部分忠实 | 范围扩大、泛化 | 限定本次运行：代理不可达，使用 b2 所示完整环境变量组合后测试通过；删除 must。 |
| 36 | h06-offline-go-test | 仅前 | procedure | Run Go tests with GOPROXY=off in sandbox | To successfully run Go tests in a sandboxed environment with no network access, use the flag GOPROXY=off. This prevents the test runner from attempting to connect to external module proxies, which are not accessible in the sandbox. | b2, a1 | b2/a1 支持本次关闭代理后的成功报告及网络背景；候选写成无网络沙箱测试的通用成功流程，未限定本次依赖条件，且没有展示仅配置 GOPROXY=off 就能普遍成功。GOPROXY=off 是环境变量设置，不是 go test 选项。 | 直接；无（通用成功条件） | 部分忠实 | 部分忠实：写成无网络沙箱的通用成功流程，且把环境变量 GOPROXY=off 称作命令行 flag。 | 部分忠实 | 范围扩大、泛化 | 改为本次记录，给出 GOFLAGS=-mod=mod GOPROXY=off go test ./...；称为环境变量，不称命令行 flag。 |
| 37 | h08-unadopted-suggestion | 后 | decision | Add retries to fetch_report() | The user requested to add a retry mechanism to the fetch_report() function. Three retries were implemented using a loop with a Timeout exception handler. | p1, e1 | p1 支持用户要求重试，e1 支持循环和 Timeout 捕获；range(3) 是最多 3 次总尝试，即首次失败后最多再试 2 次，不能写成首次之外的 3 次重试。a1 虽称 three retries，也不覆盖已展示代码的次数语义。decision 类型误抽另记。 | 直接；矛盾（重试次数） | 部分忠实 | 部分忠实：e1 的 range(3) 是最多 3 次尝试（最多 2 次重试），与“three retries”矛盾。 | 部分忠实 | 命令/数字/否定错 | 改为“助手添加最多 3 次调用尝试，遇 Timeout 继续下一次”；普通任务不能作为长期决策保存。 |
| 38 | h08-unadopted-suggestion | 仅前 | decision | Add retries for fetch_report() | The user decided to add retries to the fetch_report() function to handle potential timeout issues. Three retries were implemented in the code. | p1, e1 | p1 仅要求添加重试；e1 支持捕获 Timeout 后继续尝试，但没有用户选择该做法的动机说明；最多 3 次总尝试被写成 3 次重试，次数不符。未引用的 a1 也把次数叫作 three retries，仍应对照代码区分尝试与重试。 | 直接；无（用户超时动机）；矛盾（次数） | 部分忠实 | 部分忠实：p1 只要求加重试，未给“处理潜在超时”的动机；range(3) 也被写成 3 次重试。 | 部分忠实 | 补动机/收益、命令/数字/否定错 | 改为“用户要求重试；助手用捕获 Timeout 的循环实现最多 3 次总尝试”，不补用户动机；类型误抽另记。 |
| 39 | h08-unadopted-suggestion | 仅前 | procedure | Implement retries with a loop | A loop with a range of 3 was used to implement retries for the fetch_report() function. Each attempt is wrapped in a try-except block to catch Timeout errors and continue to the next attempt. | e1 | e1 直接显示 range(3)、每轮 try 调用 fetch、Timeout 后 continue；候选说循环范围为 3，没有把它说成额外重试 3 次，没有补未展示的延迟或成功保证。 | 直接 | 忠实 | 忠实：range(3)、每轮 try 调用 fetch、Timeout 后 continue 与 e1 一致，未把尝试说成 3 次重试。 | 忠实 | 无 | 无需事实修正；一次实现记录的持久性另评。 |

## 各会话的完整事件

(方括号是事件 id;⏎ 表示换行。)

### g01-rule-with-incident

- [p1] 用户: add an endpoint GET /orders/:id. Rule for this service: handlers never touch sql.DB directly, every query goes through the repo package. We leaked connections from handlers once and had an outage.
- [r1] 工具:Read: Read internal/repo/orders.go (60 lines)
- [e1] 工具:Edit: /svc/internal/handlers/orders.go ⏎ + order, err := h.repo.GetOrder(ctx, id)
- [b1] 工具:Bash: go test ./internal/handlers: ok
- [a1] 助手回复: Added GET /orders/:id; the handler calls repo.GetOrder.

### g02-naive-aware-datetime

- [p1] 用户: the expiry check raises TypeError in production but passes locally
- [b1] 工具:Bash: python -m pytest tests/test_expiry.py -q: 1 passed
- [b2] 工具:Bash: python expiry_check.py: TypeError: can't compare offset-naive and offset-aware datetimes
- [e1] 工具:Edit: /app/expiry.py ⏎ - now = datetime.utcnow() ⏎ + now = datetime.now(timezone.utc)
- [a1] 助手回复: Cause: datetime.utcnow() returns a naive datetime, while the expiry timestamps from the database are timezone-aware, so comparing them raises TypeError. The tests passed locally because the fixture used naive dates. I switched to datetime.now(timezone.utc).

### g03-decision-env-flags

- [p1] 用户: for feature flags just read them from environment variables. I decided against a config service, we are three developers and I do not want another service to run and monitor.
- [e1] 工具:Edit: /app/flags.py ⏎ + def enabled(name): ⏎ +     return os.environ.get(f"FLAG_{name.upper()}") == "1"
- [a1] 助手回复: Flags are read from FLAG_<NAME> environment variables.

### g04-procedure-db-reset

- [p1] 用户: my local database is in a weird state, help me reset it
- [b1] 工具:Bash: make seed: error: relation "users" already exists
- [b2] 工具:Bash: make db-reset && make seed: ok, 120 rows
- [a1] 助手回复: Reset works only in this order: make db-reset first, then make seed. The seed script assumes an empty schema, so running it on its own against an existing database fails with 'relation already exists'.

### g05-fact-region

- [p1] 用户: before you write the deploy script: everything we run is in eu-west-1 only. Do not assume us-east-1, the S3 buckets and the cluster are all in eu-west-1.
- [e1] 工具:Edit: /ops/deploy.sh ⏎ + export AWS_REGION=eu-west-1
- [a1] 助手回复: The deploy script sets AWS_REGION=eu-west-1.

### g06-jest-esm

- [p1] 用户: 前端 npm test 报错 SyntaxError: Cannot use import statement outside a module,帮我看一下
- [b1] 工具:Bash: npm test: SyntaxError: Cannot use import statement outside a module (node_modules/lodash-es/lodash.js)
- [e1] 工具:Edit: /web/jest.config.js ⏎ + transformIgnorePatterns: ['/node_modules/(?!lodash-es)'],
- [b2] 工具:Bash: npm test: 14 passed
- [a1] 助手回复: 原因是 lodash-es 只发布 ESM,jest 默认不转换 node_modules,所以解析 import 时报错。在 jest.config.js 里用 transformIgnorePatterns 放行 lodash-es 后,测试通过。

### g07-never-amend-pushed

- [p1] 用户: fix the typo in the README and commit it
- [b1] 工具:Bash: git commit -am 'fix typo'
- [p2] 用户: one more thing for the future: never amend or force-push commits that are already pushed, make a new commit instead. Reviewers lose their place in the diff otherwise.
- [a1] 助手回复: Committed the typo fix as a new commit.

### g08-flaky-port

- [p1] 用户: tidy the test suite
- [n1] 工具:Bash: cargo fmt --all: no changes
- [n2] 工具:Read: Read tests/case2.rs (70 lines)
- [n3] 工具:Bash: cargo fmt --all: no changes
- [n4] 工具:Read: Read tests/case4.rs (70 lines)
- [n5] 工具:Bash: cargo fmt --all: no changes
- [n6] 工具:Read: Read tests/case6.rs (70 lines)
- [n7] 工具:Bash: cargo fmt --all: no changes
- [n8] 工具:Read: Read tests/case8.rs (70 lines)
- [n9] 工具:Bash: cargo fmt --all: no changes
- [n10] 工具:Read: Read tests/case10.rs (70 lines)
- [t1] 工具:Bash: cargo test: 1 failed (server_starts), 40 passed
- [t2] 工具:Bash: cargo test server_starts: ok
- [e1] 工具:Edit: /repo/tests/server.rs ⏎ - let listener = TcpListener::bind("127.0.0.1:8080")?; ⏎ + let listener = TcpListener::bind("127.0.0.1:0")?;
- [a1] 助手回复: server_starts was flaky because two tests bound the fixed port 8080 and collided when run in parallel (address already in use). Binding to port 0 lets the OS pick a free port.

### g09-deploy-freeze

- [p1] 用户: remember when you plan the release: deploys are frozen every Thursday from 14:00 to 18:00 CET because of the weekly batch job.
- [a1] 助手回复: Noted, I will not schedule the release in that window.

### h05-fix-without-cause

- [p1] 用户: fix the failing test test_parse_empty
- [b1] 工具:Bash: pytest tests/test_parse.py::test_parse_empty -q: 1 failed
- [e1] 工具:Edit: /app/parse.py ⏎ + if not s: ⏎ +     return []
- [b2] 工具:Bash: pytest tests/test_parse.py -q: 6 passed
- [a1] 助手回复: Fixed. test_parse_empty passes now.

### h06-offline-go-test

- [p1] 用户: run the go tests
- [b1] 工具:Bash: go test ./...: dial tcp: lookup proxy.golang.org: no such host
- [b2] 工具:Bash: GOFLAGS=-mod=mod GOPROXY=off go test ./...: ok
- [a1] 助手回复: The tests pass when run with GOPROXY=off; the sandbox has no network access so the module proxy could not be reached.

### h08-unadopted-suggestion

- [p1] 用户: add a retry to fetch_report()
- [e1] 工具:Edit: /app/report.py ⏎ + for _ in range(3): ⏎ +     try: return fetch() ⏎ +     except Timeout: continue
- [a1] 助手回复: Added three retries. It might be worth adding a pre-commit hook for formatting at some point, but I did not touch that.
- [p2] 用户: ok thanks

## 最终报告

| 项 | 改前 | 改后 |
|---|---|---|
| 候选数 | 20 | 20 |
| 忠实 | 11 | 14 |
| 部分忠实 | 9 | 6 |
| 不忠实 | 0 | 0 |
| 待裁定 | 0 | 0 |
| 幻觉候选率 | 9/20 = 45% | 6/20 = 30% |

偏差分布（改前/改后，按候选计，同条可有多个标签）：补动机/收益 4/0，错误归因 1/0，范围扩大 4/3，泛化 2/1，推断未出现事实 1/3，命令/数字/否定错 2/1，引用错误 1/3。

汇总口径：上述为最终裁定列(39 条全部裁定)。A、B 两人独立判定只有 #33 一条分歧,其余 38 条一致,最终裁定取一致的判定。改前计“仅前”19 条加共享 #33，改后计“后”19 条加共享 #33，两边均为 20 条；共享候选判一次、两边各计一次。引用遗漏本身不计入幻觉候选数，类型误抽与持久性噪声另记。偏差标签可重叠，不可求和当成不忠实候选总数。

边界说明：遵守既定口径，助手明确报告的结果可以支持会话转述，未因缺少独立验证扣分；没有按“必须”等词自动扣分，而是核对是否新增必要性或扩大范围。#3 的规则目的有用户紧接给出的事故背景支持；#1 额外增加“事故之后才建立规则”的时间关系，单独计偏差。#28、#30 是针对固定端口冲突的具体修复，不按通用确定性保证处理；#23、#25、#32 则是标题删掉了明确的适用限定。#37、#38 的尝试/重试次数以已展示的循环语义为准，助手的口语报告不能消除与代码的矛盾。#33 的裁定见下。

结果：改前有 9 条、改后有 6 条含不支持或矛盾的断言，候选率从 45% 到 30%，下降 15 个百分点（3 条）。这一批有所减少，但改后幻觉硬线仍未通过；单模型单次运行及 20 条候选不足以证明稳定改善。

### #33 的裁定

唯一的 A/B 分歧。候选写"测试失败是因为函数没有处理空输入"。事件里只有测试名 `test_parse_empty`、一次失败、新增 `if not s: return []`、改后通过,没有任何事件说明失败的原因,助手只说"Fixed"。"改了空输入分支所以原来没处理"是合理的推断,但这是候选自己补出的因果,不是事件给出的,与口径 2(按增加了什么断言判)一致 → **部分忠实,偏差类型 推断未出现事实**。该会话本来就是"修了但没说原因"的陷阱,这条候选正好落进去。若按 B 判忠实,改前 12/8、改后 15/5(25%),方向与结论不变。

## B 判定最终报告

本节为 B 列独立判定（39 条）的汇总,按 B 的判定计,保留作对照。

| 项 | 改前 | 改后 |
|---|---|---|
| 候选数 | 20 | 20 |
| 忠实 | 12 | 15 |
| 部分忠实 | 8 | 5 |
| 不忠实 | 0 | 0 |
| 待裁定 | 0 | 0 |
| 幻觉候选率 | 8/20 = 40% | 5/20 = 25% |

- 覆盖：39 条全部判定。改前计“仅前”19 条加共享 #33，改后计“后”19 条加共享 #33；共享条判一次，两边各计一次。
- 偏差分布（改前/改后，按候选计，同条可多标签）：补动机/收益 4/0，错误归因 1/0，范围扩大 4/3，泛化 2/1，推断未出现事实 0/2，命令/数字/否定错 3/1，引用错误 1/3。
- 与 A 列的差异：仅 #33(最终裁定见"最终报告"一节)。A 判部分忠实，理由是未凭“改后通过”反推唯一根因；B 判忠实——本轮唯一改动就是空输入分支，b1 的失败与 b2 的 6 passed 共同把失败原因限定在“原实现未处理空输入”，属“代码改动直接蕴含的语义”，没有补出更细的根因或其它机制。因此改前 11/9 → 12/8、改后 14/6 → 15/5；两列的下降幅度都是 15 个百分点（3 条），结论方向不变。
- 未计偏差但记下的边界：
  1. #3 的“past incidents”是复数，p1 只说“once”；按措辞不精确处理，未计数字错误（若计入，改前 9 条部分忠实、比率为 45%）。
  2. #37、#38 的“three retries”与 a1 的口语报告一致，但与 e1 的 range(3) 矛盾；按已展示代码优先计数字矛盾，助手报告不覆盖代码语义。
- 与固定口径的对应：凡属助手明确报告且候选只做转述的（#2、#9、#12、#16、#18、#24、#27 等），未因缺少“助手报告”标签或引用不全扣分，引用问题单列；#20、#23、#32、#34、#35、#36 按“新增了什么断言”（对 CommonJS 的普遍不兼容、必需性、通用成功流程、标题丢掉窗口/已推送限定）计范围扩大或泛化；#19、#21、#28、#30 的具体修复步骤判忠实，#36 因把一次组合命令写成通用流程而计偏差。
- 结论：B 列初审下，改后幻觉候选由 8 条降到 5 条，比率 40% → 25%，方向与 A 列一致；改后仍未过硬线，且 20 条候选、单模型单次运行不足以证明稳定改善。

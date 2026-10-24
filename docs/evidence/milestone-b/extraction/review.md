# B-16 额外项人工定性清单

对象：`report.md` 里 17 条未被标注覆盖的候选。已逐条对照 `sessions.json` 的原始事件填写；以下为助手复核结果，供人工审阅，不代表第二位人工评审。文末原有初步分类保留供对照。

## 判定顺序(最先命中即结论)

1. **忠实**:内容有会话证据支持吗?数字、否定、适用范围有没有被改动?否 → `幻觉`
2. **持久**:一个月后换个会话,仍成立、仍有用吗?还是只记录"这次做了什么/结果如何"?否 → `噪声`
3. **新增**:比已匹配的那条多了信息吗?否 → `重复`
4. 都通过 → `有价值`

另列"类型是否合适":内容对但类型错,记 `有价值` 并在类型列填 `偏差`。

## 清单

| # | 会话 | 类型 | 标题 | 判定(幻觉/噪声/重复/有价值) | 类型合适(是/偏差) | 值得补进 expect(是/否) | 备注 |
|---|---|---|---|---|---|---|---|
| 1 | u01-root-cause | procedure | Fix pytest failure in pager.py | 重复 | 是 | 否 | u01/e1、a1 支持修复步骤；向上取整及其原因已由两条 matches 覆盖，没有独立新增知识。 |
| 2 | u02-user-constraint | procedure | Add TLS with rustls | 噪声 | 是 | 否 | u02/b1、e1、b2、a1 支持本次安装与构建；未给出完整 TLS 配置或可复用流程，只是把本次操作改写为步骤。 |
| 3 | u02-user-constraint | fact | TLS added successfully | 噪声 | 是 | 否 | u02/a1 支持 rustls 与无 openssl 的结果；这是本次完成状态，长期约束已另有候选。 |
| 4 | u03-decision-with-reason | procedure | Replace Redis lock with PostgreSQL advisory lock | 噪声 | 是 | 否 | u03/e1 支持 set_nx 换成 pg_try_advisory_lock($1)；属于本次代码替换，未交代锁的释放、连接生命周期等可复用流程。 |
| 5 | u03-decision-with-reason | fact | Redis dependency removed | 噪声 | 是 | 否 | u03/a1 支持该 crate 不再使用 Redis；按判定顺序先归为本次变更结果，且与已有决策重叠。标题“移除”不应解读为已证明删除依赖声明。 |
| 6 | u03-decision-with-reason | procedure | Run tests after code change | 噪声 | 是 | 否 | u03/b1 有测试命令；改动后运行测试是通用操作，没有项目特有的持久知识。 |
| 7 | u04-procedure | decision | Publish requires signing first | 有价值 | 偏差 | 否 | u04/b2、a1 明确先 make sign 再 make publish，且 publish 不会自动签名；应归 procedure 或 gotcha。已有 expect 已覆盖内容，需修正候选类型，不必新增 expect 或放宽为 decision。 |
| 8 | u06-project-fact | decision | Schedule import outside the read-only window | 噪声 | 是 | 否 | u06/a1 只是本次导入排期承诺；持久信息是 p1 的周五只读窗口，已由 matches 覆盖。 |
| 9 | u07-chinese-constraint | procedure | 为 gRPC 调用添加显式超时和重试 | 有价值 | 是 | 是 | u07/e1 支持 context.WithTimeout 与 defer cancel()，a1 支持最多重试 2 次；具体调用步骤超出已有超时约束。建议保留为该客户端的 procedure，并补上原文 3 秒超时；不可泛化成所有调用都重试 2 次。 |
| 10 | u08-dependency-swap | procedure | Update Cargo.toml and run cargo audit | 噪声 | 是 | 否 | u08/e1、b1 支持修改依赖及执行 audit；属于本次迁移操作，没有超出已有选型决策的持久流程。 |
| 11 | u08-dependency-swap | fact | Cargo audit is now clean | 噪声 | 是 | 否 | u08/a1 支持本次 audit 干净；审计结果随依赖和公告变化，不能作为长期无安全问题的保证。 |
| 12 | u09-buried-decision | procedure | Format code with cargo fmt | 噪声 | 是 | 否 | u09/n1、n3、n5、n7、n9、n11 仅记录格式化未产生改动；是操作日志。 |
| 13 | u09-buried-decision | procedure | Read module files for review | 噪声 | 是 | 否 | u09/n2、n4、n6、n8、n10、n12 仅记录读过哪些文件；不构成可复用的审查方法。 |
| 14 | u09-buried-decision | fact | All tests passed | 噪声 | 是 | 否 | u09/t1 支持 112 通过、0 失败；仅是当次测试状态。 |
| 15 | u09-buried-decision | procedure | Enable missing_docs deny in lib.rs | 重复 | 是 | 否 | u09/e1 支持在 src/lib.rs 添加属性；已有 matches 已包含 missing_docs 强制文档注释的规则，路径与属性写法不足以独立保存一条步骤。 |
| 16 | t02-abandoned-approach | procedure | Revert config caching change | 噪声 | 是 | 否 | t02/b2、a1 支持撤销本次改动；git checkout 是一次性恢复操作，不应保存为今后修改配置的常规流程。 |
| 17 | t02-abandoned-approach | gotcha | Global static config breaks test isolation | 重复 | 是 | 否 | t02/p2 明确不同配置的测试被全局缓存破坏；已有 decision 候选已说明相同原因和显式传参选择。不是新增知识；应限定于该项目的配置测试，不泛化为所有全局状态都不可用。 |

完整内容见 `report.md` 对应会话下标 `extra (needs human check)` 的行。

## 汇总（复核后）

- 17 条额外项：有价值 **2 / 17**（#7、#9）；重复 **3**（#1、#15、#17）；噪声 **12**；幻觉 **0**。
- 类型偏差：#7，应归为 procedure 或 gotcha。值得新增 expect：#9；#7 已有对应 expect，无须新增。
- 忠实性判断限于会话证据：助手最终回复可作为“会话中有此陈述”的证据，不等于独立验证代码运行结果。

### 精度口径校正

逐行统计 `report.md`：`matches` 12 条、`extra` 17 条、`UNEXPECTED` 6 条、`TRAP` 3 条，共 38 条。原汇总“已匹配 15 条”与逐行内容不符，不能直接将 15 加入分子。

t02 的 `TRAP` 决策实际忠实记录了用户拒绝全局缓存的原因和继续显式传参的选择，本次复核按有效候选计入；其余两条 TRAP 与六条 UNEXPECTED 不计入。沿用本表原定“已匹配候选均按有价值计”的前提：

- 严口径精度 =（12 条 matches + 1 条 t02 决策 + 2 条有价值额外项）/ 38 = **15 / 38，约 39.5%**。
- 宽口径精度 =（15 + 3 条重复额外项）/ 38 = **18 / 38，约 47.4%**。

以上是候选级的内容可用率，允许 #7 的类型偏差，不是去重后的知识精度。12 条 matches 本身未在本表重新逐条分类，其间的重复仍沿用原约定计入；因此不能据此宣称全量候选已完成严格复核。自动匹配的召回仍是原报告 11/12，#7 属于内容命中、类型不匹配。未修改原报告和 `sessions.json` 的标签。

## 我的初步分类(填完再看,避免被带偏)

- 有价值:#7(类型偏差:该是 procedure)、#17(原因对,但表述偏泛)
- 重复:#1、#5、#15;#9 待定("最多重试 2 次"有一点新信息)
- 噪声:#2、#3、#4、#6、#8、#10、#11、#12、#13、#14、#16
- 幻觉:未发现(只抽看了几条的数字与否定,未逐条核对)

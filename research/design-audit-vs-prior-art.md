# almanac v0.1 规格审计：对照同类记忆产品

> 调研日期：2026-10-21。
> 参考资料，决策以 ticket 为准。
>
> 审计对象：合入 main 的 `docs/spec/` 全部 11 章。问题：哪些设计不合理、哪些过度设计、哪些是同类产品没有的，这些设计针对的问题是否真实，其他社区怎样面对同类问题。
> 补充 [现有智能体记忆系统调研](prior-art-memory-systems.md)（#2），不重复其内容。
> 标记：**[V]** 已对照一手来源；**[S]** 仅二手报道或搜索摘要；**[I]** 推断。

## 结论

主干方向有外部证据：「记忆必引证据、用时回查来源」与「agentic 词法检索、核心无 LLM」。问题集中在两头：多设备、Writer 推断、维护机制偏重；记忆投毒、密钥、写入时机、人可读性缺失。据此新开 6 个 ticket（见文末）。

## 新发现的同类产品与数据

- **GitHub Copilot Memory**（2026-01-09）是最接近的先例：每条记忆＝{subject, fact, citations（`file:line`）, reason}，智能体使用前按当前分支校验引用，只用通过校验的记忆；**明确不做离线整理**（去重、冲突消解），因为不可扩展，错误靠写修正版自愈；A/B 中 PR 合并率 83%→90%（p<0.00001）。[V] <https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/>。未使用的记忆 28 天后删除。[S] <https://docs.github.com/en/copilot/concepts/agents/copilot-memory>
- **Cursor** 在 2.1.x（2025-11-25）移除 Memories，让用户导出到 Rules。[V] <https://forum.cursor.com/t/are-my-memories-gone/144057>
- **上下文不是免费的**：ETH「Evaluating AGENTS.md」（arXiv 2602.11988，ICLR 2026）发现上下文文件未提升成功率、成本增加 20% 以上，LLM 生成的在 8 个设置中 5 个有害。[S] <https://iclr.cc/virtual/2026/10021142>；Chroma「Context Rot」：一个干扰项即降低准确率。[S]
- **当前最热的开源项目是自动捕获型**（GitHub stars，2026-10-20）：claude-mem 101.9k（hooks 全量捕获 + AI 压缩 + SQLite FTS5/Chroma，`<private>` 标签）、mem0 67.5k、MemPalace 59.7k、cognee 32.6k、graphiti 32.1k、supermemory 31.3k、letta 25.2k、basic-memory 4.3k。**均不要求证据**。[V]

## 逐项对照

| almanac 设计 | 同类是否有 | 问题是否真实 | 判断 |
|---|---|---|---|
| 记忆必引证据（四类来源） | Copilot 要求引用但只限代码位置；Codex 记 rollout 路径；Graphiti 关联 episode | 中：Copilot A/B 支持代码引用的价值 | 保留；`speaker: agent` 的对话证据是自引，证明不了什么 [I] |
| 确定性核验 + 可能过期 | Copilot 用时由 LLM 判断，不做子串匹配 | 真实：Copilot 的反向种子实验 | 保留；只能发现摘录消失，发现不了「摘录仍在但语义已变」[I] |
| 只追加日志、取代/撤回、清除留墓碑 | Graphiti 失效而非删除、Letta MemFS 用 git、mem0 history 表 | 部分：ChatGPT 2025-11 记忆被清空且无法批量恢复 [S]；用户对完整审计的需求未找到证据 | 保留，JSONL 成本低 |
| 认知状态字段 | 只有 Codex 在提示词里用这三个标签 | 证据薄 | 与证据来源大面积重叠，可推导默认值 |
| 必有 Writer，推不出就拒绝写入 | OpenMemory 记录客户端但不拒绝 [S]；Copilot 限仓库贡献者写入 [V] | 拒绝写入属臆想；归因作为投毒防线有价值 | 拒绝的代价是丢真实记忆 |
| 无 LLM / embedding，词法 agentic search | 多数热门项目加向量 | 有力支持：Letta 的 grep 型智能体 LoCoMo 74.0% vs mem0 68.5% [V] <https://www.letta.com/blog/benchmarking-ai-agent-memory> | 保留；中文分词竞品中只有 basic-memory 做了 n-gram，是差异点 |
| 离线 review、相似对、`distinguish` | Letta `/doctor`、CC 接近上限时提示合并 [V] | Copilot 明确放弃离线整理；不触发就没人跑 | `review` 可留，`distinguish` 可砍 |
| Server 模式与多设备 | OpenMemory（Docker+Postgres+Qdrant）、Supermemory 托管 | 未找到单人多设备的直接诉求 [I] | 成本最高，候选推迟 |
| 会话开始注入 + 字符预算 | 普遍：CC 200 行 / 25KB [V]，Windsurf 规则 12k 字符 [V]，claude-mem 先注入索引 | 真实；注入的记忆也常被忽略（CC issue #61042）[S] | 保留 6000 字符默认 |
| 8 个工具 | 多数 4–6 个文件式或 CRUD 工具（Claude memory tool [V]、MCP 知识图谱 server、claude-mem 4 个） | `corroborate`、`distinguish` 是新设计；风险是智能体不用 | 精简 |

另：持久化 tantivy 索引对单人数百到数千条记忆偏重，加载时内存 BM25 即可省掉索引一致性与重建逻辑；但 jieba 词典每次加载，性能收益有限。[I]

## 同类产品解决了而 almanac 没有覆盖的

1. **记忆投毒（最大缺口）**：MINJA 仅靠查询注入成功率 >95%；Unit 42（2026-01）演示网页往 Bedrock 智能体记忆里植入持久指令；OWASP Agentic Top 10 2026 列为 ASI06。[S] <https://unit42.paloaltonetworks.com/indirect-prompt-injection-poisons-ai-longterm-memory.md>、<https://arxiv.org/html/2601.05504v1>。Codex 有 `disable_on_external_context`。[V] almanac 的 `web` 证据 + 会话开始注入正是这条路径，规格中无相应处理。
2. **密钥过滤**：Codex 在生成的记忆字段中脱敏，claude-mem 有 `<private>` 标签。[V] almanac 只有事后清除。
3. **何时写入**：Codex 提示词倾向不写；claude-mem 绕开决策、全量捕获；记忆 MCP 不配合 CLAUDE.md 指令就闲置。[S] almanac 只靠 SKILL.md 措辞。
4. **人可读、可编辑**：CC 卖点是「可读写删的纯 Markdown」[V] <https://code.claude.com/docs/en/memory>；Simon Willison 批评 ChatGPT 记忆是不透明「档案」[V] <https://simonwillison.net/2025/May/21/chatgpt-new-memory/>。almanac 的日志人无法手改，CLI 只出 JSON。
5. **与 AGENTS.md 分工**：Codex 文档说必须遵守的规则放 AGENTS.md，记忆只是回忆辅助。[V] <https://learn.chatgpt.com/docs/customization/memories?surface=app>
6. **团队共享**：Copilot 按仓库加权限；CLAUDE.md / Rules 走 git。almanac 有意排除。
7. **按使用衰减**：Copilot 28 天过期 [S]；almanac 明确不做，注入按时间倒序已有近似效果，维持不做。
8. **评估**：LongMemEval（ICLR 2025）五项能力中「知识更新」「弃答」直接对应取代与可能过期 [V] <https://arxiv.org/abs/2410.10813>；LoCoMo 结果有争议（Zep 反驳 mem0）[S]。规格没有验收场景。

## 实践者报告真正重要的

简单的人工维护文件常常胜出（Codex 让规则进 AGENTS.md、Cursor 把 Memories 并入 Rules [V]；论坛称单人 CLAUDE.md 够用 [S，低可信]）。归纳 [I]：注入要小；用时对照代码校验；人能看能改；宁可不写。almanac 前两项做得好，后两项及投毒防护偏弱。

## 据此新开的 ticket

- [外部来源记忆的投毒防护](https://github.com/zibyn/almanac/issues/27)
- [写入时拦截密钥](https://github.com/zibyn/almanac/issues/28)
- [v0.1 范围削减](https://github.com/zibyn/almanac/issues/29)
- [写入时机：会话结束时的回顾提醒](https://github.com/zibyn/almanac/issues/30)
- [给人看的记忆视图与 AGENTS.md 分工](https://github.com/zibyn/almanac/issues/31)
- [推不出 Writer 时拒绝写入还是记 unknown](https://github.com/zibyn/almanac/issues/32)

## 未核实 / 局限

- Graphiti、Cognee 的内部机制只看了 star 数；Windsurf 的删除行为、ETH 论文全文未读。
- CC 的 GitHub issue 只看了镜像页；Copilot 28 天过期来自二手摘要。
- 「弃用花哨记忆、回到 CLAUDE.md」的社区情绪属个例，按传闻对待。

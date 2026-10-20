# 现有智能体记忆系统调研（ticket #2）

> 问题：mem0、Letta(MemGPT)、Zep/Graphiti、basic-memory，以及 Claude Code / Codex 自带的 memory，各自如何处理**写入门槛、更新与取代、遗忘、来源（Provenance）与真实性、中文支持、存储依赖**？哪些值得 almanac 借鉴，哪些是要避开的复杂度？
>
> 调研日期：2026-10-20。只用一手来源（官方文档、源码、论文）。各项目迭代很快，结论以所列链接当日内容为准。标注“推断”的条目是作者根据源码得出、文档没有明说的结论。
> 参考资料，决策以对应 ticket 的结论为准。

## 速览表

| 系统 | 写入门槛 | 更新 / 取代 | 遗忘 | 来源与真实性 | 中文 | 存储依赖 |
|---|---|---|---|---|---|---|
| **mem0**（OSS v3） | LLM 抽取，倾向“尽量全抽”；`infer=False` 可存原文 | v3 起**只做 ADD**，新旧事实并存，靠检索排序；论文版是 ADD/UPDATE/DELETE/NOOP | 只能手动 `delete`（向量库里是硬删，SQLite history 留痕） | SQLite `history` 记录 old→new；metadata 有 `actor_id`/`role`；不存原始对话链接 | 抽取提示词要求“和输入用同一语言”；BM25 走 spaCy 英文词形还原 | 向量库（默认本地 Qdrant）+ SQLite + embedding + LLM |
| **Letta / MemGPT** | 智能体自己决定（自编辑记忆）+ 后台“dreaming”整理 | 块整体覆写 / 文件编辑；MemFS 用 git 提交 | 不自动遗忘；上下文满了做递归摘要和驱逐，原消息仍留在 recall 库 | MemFS 的 git 历史；无逐事实来源 | 未提及 | 服务器形态要 Postgres+pgvector（Docker 已不维护）；新方向是 git 管理的 Markdown 文件 |
| **Zep / Graphiti** | 每个 episode 都由 LLM 抽取实体和边 | **双时间轴**：冲突时把旧边的 `t_invalid` 设成新边的 `t_valid`，不删除 | 不删，只标记失效 | 每条事实都能回溯到源 episode（原始数据无损保存） | 未提及；全文索引用图库默认分析器 | Neo4j / FalkorDB / Neptune 图数据库 + embedding + 支持结构化输出的 LLM |
| **basic-memory** | 人或 AI 显式调用 `write_note`，无自动抽取 | 编辑 Markdown 文件；`write_note` 防止误覆盖 | 无保留期或衰减策略，只有 `delete_note` | 无逐事实来源；本地版本靠用户自备快照 | **有**：CJK 等无空格文字用应用层 n-gram 写入 FTS5 | Markdown 文件 + SQLite（FTS5 + sqlite-vec），默认开启 FastEmbed 向量 |
| **Claude Code auto memory** | 模型判断“未来会话是否用得上”；能从代码推出的、CLAUDE.md 已写的都跳过 | 模型直接改 Markdown 文件；写入时自动记录 `modified` 时间戳 | `MEMORY.md` 超过 200 行或 25KB 时提示模型合并、删掉陈旧条目；无自动过期 | 只有 `type` 和 `modified` 两个 frontmatter 字段，无出处 | 纯文本，没有专门处理 | 本地 Markdown 目录，无数据库 |
| **Codex memories** | 两阶段后台流水线；写入门槛很明确，**倾向写空** | 第二阶段由一个整合子智能体增量重写 `MEMORY.md`，冲突时 `updated_at` 新者优先 | **由证据驱动**：源 rollout 被删时，只靠它支撑的记忆随之删除 | rollout 是**不可变证据**；每个任务块列出 `rollout_path`/`thread_id`；答复带行号引用 | 未提及；检索靠智能体 grep | 本地文件 + 会话状态 SQLite + 用 git 做基线 diff；无向量库 |

---

## 1. mem0

**写入门槛。** 默认 `infer=True`：LLM 从消息中“提取关键事实、决定或偏好”。设成 `infer=False` 时原样存储，“可能产生重复”。[docs: add](https://docs.mem0.ai/core-concepts/memory-operations/add) 现行抽取提示词 `ADDITIVE_EXTRACTION_PROMPT` 要求“唯一操作是 ADD”，并且“遗漏一条就意味着丢失上下文”，所以门槛偏低、倾向多抽；助手说的话也会被抽取（例如“User was recommended X”）。[prompts.py](https://github.com/mem0ai/mem0/blob/main/mem0/configs/prompts.py)

**更新与取代。**
- 论文版（2025）：抽出候选事实后，先检索最相似的 s=10 条已有记忆，再让 LLM 通过 tool call 在 **ADD / UPDATE / DELETE / NOOP** 中选一个；遇到矛盾就 DELETE。图版本 Mem0ᵍ 则“把关系标记为 invalid，而不是物理删除，以支持时间推理”。[arXiv 2504.19413 §2](https://arxiv.org/abs/2504.19413)
- OSS v3：改成“单次 LLM 调用、只做 ADD（没有 UPDATE/DELETE）”，新旧事实**并存**，交给检索排序处理；图记忆从开源 SDK 中移除。[migration v2→v3](https://docs.mem0.ai/migration/oss-v2-to-v3)

**遗忘。** 只有显式的 `update` / `delete` / `delete_all` API。`_delete_memory` 会从向量库里**硬删除**，同时在 SQLite history 写一条 `DELETE, is_deleted=1`。[main.py](https://github.com/mem0ai/mem0/blob/main/mem0/memory/main.py)

**来源与真实性。** `history(memory_id)` 记录 `prev_value → new_value, event, actor_id, role`，有修改审计，但**不链接回原始对话**。Platform 返回的 `event_id` 只用来追踪处理状态。[main.py](https://github.com/mem0ai/mem0/blob/main/mem0/memory/main.py)、[docs: add](https://docs.mem0.ai/core-concepts/memory-operations/add)

**中文。** 提示词要求“检测输入语言并用同一语言记录事实”。[prompts.py](https://github.com/mem0ai/mem0/blob/main/mem0/configs/prompts.py) 但 BM25 先经过 spaCy 词形还原，安装示例用的是英文模型 `en_core_web_sm`。[lemmatization.py](https://github.com/mem0ai/mem0/blob/main/mem0/utils/lemmatization.py)、[migration](https://docs.mem0.ai/migration/oss-v2-to-v3) 推断：中文关键词召回基本只能靠向量。

**存储依赖。** 库模式默认用本地 Qdrant（`/tmp/qdrant`）加 SQLite（`~/.mem0/history.db`），自托管服务器用 Postgres+pgvector。[OSS overview](https://docs.mem0.ai/open-source/overview) 还需要 embedding 模型、LLM，以及一个平行的 `_entities` 向量集合。[migration](https://docs.mem0.ai/migration/oss-v2-to-v3)

## 2. Letta（MemGPT）

**MemGPT 论文。** 主上下文分三段：系统指令、工作上下文（只能通过函数调用写入）、FIFO 消息队列。外部有 recall storage（所有消息永久留存）和 archival storage（任意长度文本）。“记忆编辑与检索完全由模型自主决定”。上下文用到约 70% 时插入“memory pressure”警告，到 100% 时驱逐一部分消息，并生成**递归摘要**。[arXiv 2310.08560](https://arxiv.org/html/2310.08560v2)

**今日的 Letta。**
- 记忆块：字段有 `label / description / value / limit（字符数）/ read_only`；写 `value` 是**整体替换**。[memory blocks](https://docs.letta.com/guides/agents/memory-blocks)
- Archival memory 是“通用向量库”，按语义检索，可以打标签；智能体自己很难修改或删除这些条目。[archival memory](https://docs.letta.com/guides/agents/archival-memory)
- 新方向是 **MemFS**：记忆是一个 git 仓库，智能体用普通文件工具编辑，`system/` 下的文件每轮都进系统提示，其余按需读取；`core_memory_replace` 等旧工具将被移除。[MemFS](https://docs.letta.com/concepts/memfs)、[Letta blog](https://www.letta.com/blog/our-next-phase/)
- “Dreaming”：后台子智能体按步数触发，或在上下文压缩时触发，整理经验并把结果提交到 MemFS；可以再加一个审阅子智能体，但**不征求用户批准**。`/doctor` 检查放置位置、重复和 token 占用。没有自动遗忘。[memory & dreaming](https://docs.letta.com/configuration/memory)

**存储依赖。** 自托管服务器需要 Postgres + pgvector，并且必须配置 embedding。官方说明 Docker 镜像“已不再积极维护”。[self-hosting](https://docs.letta.com/guides/selfhosting)

**来源 / 中文。** 来源只有 git 提交历史，没有逐事实的证据链接。文档没有提到中文。

## 3. Zep / Graphiti

**写入。** 原始输入（消息、文本、JSON）作为 **episode 节点**无损存储，再由 LLM 抽取实体和边。实体去重的流程是：embedding 余弦相似度加名称全文检索找候选，再由 LLM 判断是否重复。[arXiv 2501.13956](https://arxiv.org/html/2501.13956v1)

**更新与取代（双时间轴）。** 两条时间轴：T 是事件真实发生的时间，T′ 是写入系统的时间（用于审计）。每条事实有 `t_valid / t_invalid` 和 `t′_created / t′_expired`。新边与语义相关的旧边矛盾、且时间区间重叠时，把旧边的 `t_invalid` 设为新边的 `t_valid`，以新信息为准，同时保留历史。[arXiv 2501.13956](https://arxiv.org/html/2501.13956v1) README 的说法是“old facts are invalidated — not deleted”。[graphiti README](https://github.com/getzep/graphiti)

**遗忘。** 不删除，只让事实失效。

**来源。** 双向索引把事实和源 episode 连起来，可以引用；“每个实体和关系都能追溯到产生它的 episode”。[README](https://github.com/getzep/graphiti)

**检索。** 余弦相似度、BM25（Neo4j Lucene）、BFS 三路并行，再用 RRF、MMR、引用次数、节点距离或 cross-encoder 重排。图更新使用预定义的 Cypher，不让 LLM 生成查询。[arXiv 2501.13956](https://arxiv.org/html/2501.13956v1)

**中文。** 文档没有提到。全文索引直接用 `CREATE FULLTEXT INDEX ... ON EACH [...]`，没有指定分析器。[graph_queries.py](https://github.com/getzep/graphiti/blob/main/graphiti_core/graph_queries.py) 推断：中文分词效果取决于 Neo4j 默认分析器。

**存储依赖。** Neo4j 5.26 / FalkorDB / Amazon Neptune（还要加 OpenSearch），Kuzu 已弃用。默认用 OpenAI 做 LLM 和 embedding；需要支持结构化输出的模型，小模型可能因 schema 不匹配导致写入失败。[README](https://github.com/getzep/graphiti)

## 4. basic-memory

**写入。** 没有自动抽取，由人或 AI 显式调用 `write_note` / `edit_note`。笔记是 Markdown 文件，frontmatter 包含 `title/type/permalink/tags`。观察条目写成 `- [category] 文本 #tag (context)`，关系写成 `- relates_to [[Target]]`。[README](https://github.com/basicmachines-co/basic-memory)

**更新 / 遗忘。** 直接编辑文件；`write_note` 会“防止意外覆盖”。只有 `delete_note`，没有保留期或衰减策略；本地版本控制需要用户自己做快照（云版才有时间点恢复）。[README](https://github.com/basicmachines-co/basic-memory)

**来源。** 没有逐事实来源机制。文件和 SQLite 索引双向同步，`doctor` 检查两者是否一致。[README](https://github.com/basicmachines-co/basic-memory)

**中文。** 这是最值得参考的一点。应用层的 `script_ngrams.py` 先对 CJK、假名、韩文、泰文等“没有可靠词边界”的文字做 NFKC 归一化，切分成 script run 并生成 n-gram，再写入 SQLite FTS5 的独立列 `script_ngrams`；FTS5 本身用的是 `unicode61` 分词器。[script_ngrams.py](https://github.com/basicmachines-co/basic-memory/blob/main/src/basic_memory/repository/script_ngrams.py)、[迁移脚本](https://github.com/basicmachines-co/basic-memory/blob/main/src/basic_memory/alembic/versions/d2e3f4a5b6c7_add_script_ngrams_to_full_text_search.py)

**存储依赖。** Markdown 文件加本地 SQLite。语义检索（fastembed、sqlite-vec）**默认开启**，扩展加载失败时退回纯关键词检索；可选 Postgres 或 Milvus。[semantic-search.md](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)

## 5. Claude Code（CLAUDE.md + auto memory）

- **两套机制**：CLAUDE.md 由人写，是指令；auto memory 由 Claude 写，记录学到的经验。两者在每次会话开始时加载，但都“只是上下文，不是强制配置”。[docs: memory](https://code.claude.com/docs/en/memory)
- **写入门槛**：记四类笔记（frontmatter 里的 `type` 为 `user / feedback / project / reference`）；跳过能从代码推出的内容（架构、路径、调试修复）和 CLAUDE.md 已有的内容；“不是每次会话都保存”。用户说“记住……”时一定会写。[docs](https://code.claude.com/docs/en/memory)
- **结构与注入预算**：`~/.claude/projects/<project>/memory/` 下有一个 `MEMORY.md` 索引（每条一行），每条记忆一个主题文件。会话开始时只加载 `MEMORY.md` 的前 200 行或 25KB，主题文件按需用文件工具读取。同一 git 仓库的所有 worktree 共享这个目录，只存在本机。[docs](https://code.claude.com/docs/en/memory)
- **更新 / 遗忘**：模型直接编辑文件。索引接近上限时，系统提醒模型“每条一行、细节移到主题文件、合并或删除陈旧条目”，超限时报错要求重写。记忆文件不受会话记录清理周期影响，没有自动过期。[docs](https://code.claude.com/docs/en/memory)
- **来源**：只有一个由宿主写入的 `modified` ISO 时间戳，用来提示“这条事实有多新”。没有出处字段，也没有证据链接。[docs](https://code.claude.com/docs/en/memory)
- **存储**：纯 Markdown，没有数据库；`/memory` 用来浏览和编辑。

## 6. Codex memories

- **开关与范围**：默认关闭（`[features] memories = true`）。`generate_memories` 和 `use_memories` 分开控制；`disable_on_external_context` 可以排除用过 MCP 或网页搜索的会话。数据存在 `~/.codex/memories/`，只能整体“删除 Codex memories”，没有逐条删除。[docs: memories](https://learn.chatgpt.com/docs/customization/memories.md?surface=app)
- **流水线**（[codex-rs/memories/README](https://github.com/openai/codex/tree/main/codex-rs/memories)）：
  - **Phase 1**：逐个处理“空闲、较新、还没被认领”的 rollout（会话记录），抽出 `raw_memory` 和 `rollout_summary` 并脱敏。
  - **Phase 2**：持全局锁，计算与上次 git 基线的 diff（`phase2_workspace_diff.md`），把 diff 交给一个“无网络、无审批、只能写本地”的整合子智能体，由它重写 `MEMORY.md`、`memory_summary.md` 和 `skills/`。
- **写入门槛**（[stage_one_system.md](https://github.com/openai/codex/blob/main/codex-rs/memories/write/templates/memories/stage_one_system.md)）：判断标准是“未来的智能体会不会因为这条而做得更好”，“**不写（no-op）是允许且优先的**”。会跳过一次性查询、没有结论的状态汇报、临时数值和常识，以及用户没有采纳的助手提议。用户的纠正是一手证据，助手的话是二手证据。要求**标注认知状态**（已验证 / 用户陈述 / 推断 / 已采纳或仅提议）。“Raw rollouts are immutable evidence. NEVER edit raw rollouts.”
- **更新、冲突与遗忘**（[consolidation.md](https://github.com/openai/codex/blob/main/codex-rs/memories/write/templates/memories/consolidation.md)）：
  - 冲突时以 `updated_at` 为主要裁决依据；验证不清楚的就**明确保留不确定性**；多次出现的证据优先于孤证。
  - **遗忘由证据驱动**：被删除的 rollout summary 进入遗忘队列，**只删除仅由已删证据支撑的记忆**；证据混合的条目只去掉失效的引用。
  - 每个 Task 块列出 `rollout_summary_files`（`cwd / rollout_path / updated_at / thread_id`），每个块带 `scope:` / `applies_to:`（cwd 边界）。
- **读取与真实性**（[read_path.md](https://github.com/openai/codex/blob/main/codex-rs/ext/memories/templates/memories/read_path.md)）：
  - 只注入 `memory_summary.md`，其余由智能体按关键词 **grep `MEMORY.md`**，必要时再打开 rollout 证据，建议控制在 4–6 步以内。
  - 是否验证要权衡漂移风险和验证成本；没有验证的记忆要在回答里说明“来自记忆、可能过时”。
  - 回答末尾附 `<oai-mem-citation>`（`文件:起止行|note=` 加 rollout id）。
  - 智能体只有在用户明确要求时才能改记忆，而且只能往 `extensions/ad_hoc/notes/` 写一个小的更新笔记，不能直接改记忆文件。
- **存储**：本地文件、会话状态 SQLite，加上 `.git` 基线；没有向量库。[README](https://github.com/openai/codex/tree/main/codex-rs/memories)、[memory_status.rs](https://github.com/openai/codex/blob/main/codex-rs/app-server/src/request_processors/memory_status.rs)

---

## 对 almanac 的启示

### 值得借鉴

1. **Codex 的“证据不可变 + 记忆可重写 + 引用指回证据”**，几乎就是 almanac 的 Evidence/Belief 二分：rollout 对应 Evidence，`MEMORY.md` 条目对应 Belief，`rollout_summary_files` 对应 Provenance。它的**证据驱动遗忘**（只删除已无证据支撑的信念）可以直接用作“遗忘与更新”的语义。
2. **Graphiti 的双时间轴“失效而非删除”**：almanac 的只追加事件日志天然具备 T′（写入时间）。如果给 Belief 加上 `valid_from / superseded_by`，就能得到不删除、可回溯的取代链，正好对应地图里“信念→证据/取代链跳转”的查询原语，而且不需要图数据库。
3. **Codex 的写入门槛提示词**（倾向不写、区分用户和助手证据、标注认知状态），可以作为 almanac `write` 的规范或宿主侧 skill 文案。认知状态（verified / user-stated / inferred）可以成为 Belief 的结构化字段，供过滤使用。
4. **basic-memory 的 CJK n-gram 加 FTS5 方案**说明：中文词法检索可以只在应用层做 n-gram，不依赖外部分词服务。almanac 在 Rust 里也能这样做，或者与分词器组合。
5. **Claude Code 和 Codex 的“小索引注入 + 其余按需检索”**：Claude Code 是 200 行或 25KB，Codex 是只注入 summary 再 grep。这和地图中“会话开始时的记忆注入”待定项直接相关，两者都证明了 agentic search（智能体 grep）在实际产品中可行。
6. **Codex 的引用格式和“未验证就声明来自记忆”的规则**，可以作为 almanac 输出约定的参考：查询结果带上 Evidence id，智能体回答时引用。
7. **mem0 的 history 表**（old→new、actor_id）：每次变更都记录 Writer。almanac 的事件日志已经覆盖这一点，并且更彻底。

### 应当避开

1. **把“遗忘”做成硬删除**（mem0 的向量库 DELETE）：一删就失去可审计性。也要避开**完全不处理取代**（mem0 v3 的 ADD-only 让新旧矛盾事实并存，再靠排序“碰运气”），这恰好是 almanac 要解决的“遗忘与更新”痛点。
2. **由 LLM 在写入路径上做抽取和冲突判定**（mem0、Graphiti、Codex Phase 2）：成本高、不确定、依赖模型能力（Graphiti 明确说小模型会写入失败）。almanac 的立场是推理交给智能体，存储层只做确定性操作。
3. **外部数据库、向量库和图数据库**：Letta 用 Postgres+pgvector（Docker 已不维护），Graphiti 用 Neo4j/FalkorDB，mem0 用 Qdrant，basic-memory 默认开启向量。这些都属于地图中明确排除的复杂度。
4. **没有来源的扁平 Markdown 记忆**（Claude Code auto memory、basic-memory）：好读，但无法回答“这条凭什么”，真实性无从验证。
5. **后台无人审阅地重写记忆**（Letta dreaming、Codex Phase 2 都不征求用户批准）：如果 almanac 将来做 `maintain`，应当输出报告或提议事件，而不是直接改写。
6. **以英文为中心的词法管线**（mem0 BM25 依赖 spaCy 英文模型）：中文必须是一等公民。

## 未核实 / 局限

- mem0 Platform（闭源）的更新语义没有核实，只看了 OSS v3。
- Graphiti 的 Neo4j 全文索引对中文的实际分词效果没有实测。
- Letta 记忆块是否有版本历史（MemFS 之外）文档没有说明。
- Codex 记忆是全局还是按项目隔离，文档没有明说；从 `applies_to:` cwd 字段推断为“全局存储、块内按 cwd 划定作用域”。

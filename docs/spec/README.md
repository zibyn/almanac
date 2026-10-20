# almanac v0.1 规格说明

本目录是 almanac v0.1 的规格，合入 main 后是实现的唯一权威。已关闭的 GitHub ticket 只作决策史；两者冲突时以本规格为准，之后的修改直接改规格（PR）。

- 术语以仓库根的 [`GLOSSARY.md`](../../GLOSSARY.md) 为准，规格不重新定义术语。
- 取舍的理由写在 [`docs/adr/`](../adr/)，规格在需要处链接，不复述。
- `research/` 下的文档是参考资料，规格不以它为据。

## 目录

| 章 | 文件 | 内容 |
|---|---|---|
| 0 | [总览](00-overview.md) | 定位、Out of scope、六个痛点的答案在哪一章 |
| 1 | [数据模型](01-data-model.md) | 证据、记忆、事件，状态推导，作用域与项目标识，来源 |
| 2 | [存储与日志格式](02-storage.md) | 目录布局、JSONL 事务行、重放、本地并发、清除步骤 |
| 3 | [智能体工具](03-tools.md) | 7 个工具的 schema 与行为，写入硬检查 |
| 4 | [仅 CLI 命令](04-cli.md) | 只在 CLI 提供的命令：`mcp`、`context`、`purge`、`project alias`、`remind` |
| 5 | [检索](05-search.md) | 中文分析器、排序、过滤 |
| 6 | [核验与可能过期](06-source-check.md) | 各来源的核验方式、核验缓存、可能过期的推导 |
| 7 | [复核](07-review.md) | 三类问题；相似度定义与阈值 |
| 8 | （空缺） | Server 模式推迟到 v0.2；编号保留，以免改动各章的交叉引用 |
| 9 | [接入](09-integration.md) | 插件、SKILL.md、会话开始注入、写入提醒、Writer 推断 |
| 10 | [错误码与 hint](10-errors.md) | 全部错误码 |

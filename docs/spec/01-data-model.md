# 1 数据模型

记忆库里只有两类实体：[证据（Evidence）](../../GLOSSARY.md)和[记忆（Memory）](../../GLOSSARY.md)。它们的一切变化都表达为事件日志里的事件（第 2 章）；本章定义实体的字段、事件的种类，以及如何从事件推导出记忆的状态。为什么这样设计见 [ADR 0002](../adr/0002-event-log-is-the-source-of-truth.md)。

## 标识

- 证据 id：`ev_<ULID>`；记忆 id：`mem_<ULID>`；事务 id：`tx_<ULID>`。
- id 由发起写入的本机二进制生成，写入后永不改变，清除重写日志后也不变。
- 时间一律为 UTC 的 RFC 3339 字符串，精确到毫秒。

## 证据

证据＝一段不可变的[摘录（Excerpt）](../../GLOSSARY.md)＋它的[来源（Provenance）](../../GLOSSARY.md)。写入后不再修改。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | `ev_…` |
| `excerpt` | string | 支撑记忆的一小段原文，UTF-8 不超过 4096 字节 |
| `source` | Source | 见下表，按 `type` 区分 |

来源的其余部分（写入方、时间、git commit）记在所属事务上（第 2 章），不在证据里重复。

### 来源类型

v0.1 共四种[来源类型（Source Type）](../../GLOSSARY.md)，不设「其他」；以后新增类型属于加法。

| `type` | 字段 | 可否核验 |
|---|---|---|
| `conversation` | `host`：宿主名，自动填为本次写入的 Writer 名；`session?`：会话标识，自动填为 Writer 的会话；`turn?`：轮次；`transcript_path?`：只作参考，不回查 | 不可核验 |
| `file` | `project`：项目标识；`path`：相对项目根的路径，不得以 `/` 开头或含 `..`；`line?`：行号提示 | 可核验 |
| `web` | `url`：`http` 或 `https`；`fetched_at`：抓取时间，未给出时填写入时间 | 可核验 |
| `command` | `command`：命令行；`cwd?`；`exit_code`：整数（必填） | 不可核验 |

- `conversation` 证据只收**用户**说的话；智能体自己说的话不能作为证据。人的直接陈述（包括人通过 CLI 写入）也记为 `conversation`。
- 用户的话是对智能体提议的确认（如「对，就这样」）时，摘录可以先引用紧挨着的上一句智能体发言作为上下文，写成以 `> 智能体：` 开头的一行，再另起一行写用户原话。证据仍算用户的陈述。
- `file` 只接受项目根之内的文件；项目外的文件按 `command` 记录（如 `cat` 的输出）。
- 如何核验见第 6 章。

### 指纹

证据的指纹＝对「来源类型 + 规范化定位 + 归一化摘录」做哈希，用于判断两条证据是否相同（写入幂等，第 3 章）。

| `type` | 规范化定位 |
|---|---|
| `conversation` | `session` |
| `file` | `project`、`path` |
| `web` | `url` |
| `command` | `command`、`cwd` |

- 归一化：Unicode NFC，连续空白折叠为一个空格，去掉首尾空白。核验时的子串匹配用同一归一化。
- 不进指纹：行号提示、`fetched_at`、git commit、写入时间、Writer。
- 同一摘录出自不同文件，是两条证据。
- 指纹是派生值，不写进日志，重放时在内存中算出（第 2 章）。具体哈希算法不在规格里规定。

## 记忆

记忆是从证据中提炼出的一条论断。正文写入后不变，只能被[取代（Supersession）](../../GLOSSARY.md)或[撤回（Retraction）](../../GLOSSARY.md)。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | `mem_…` |
| `text` | string | 一条论断，不超过 500 个 Unicode 字符 |
| `scope` | `global` \| `project` | 必填，由写入方显式选择，不按 cwd 默认 |
| `project` | string | `scope` 为 `project` 时必有：项目标识 |
| `root_commit` | string | `scope` 为 `project` 且项目在 git 内时必有：项目的根提交 |
| `epistemic` | `observed` \| `user_stated` \| `inferred` | [认知状态（Epistemic Status）](../../GLOSSARY.md)：亲见 / 用户陈述 / 推断。写入时可省略，见下文 |
| `evidence` | string[] | 记录时引用的证据 id，至少一条，没有例外 |
| `supersedes` | string[] | 被它取代的记忆 id，可为空 |

认知状态省略时由证据保守推出：记录时引用的证据**全部**为 `conversation`，取 `user_stated`；否则取 `inferred`。推出的值照常写进日志，读端不再推导。写入方确是自己观察到的（如工具输出），应显式写 `observed`。

记忆当前引用的全部证据＝记录时的 `evidence` 加上此后所有[附证（Corroboration）](../../GLOSSARY.md)追加的证据。

## 事件

日志中的每个事件属于一个事务（第 2 章），由事务提供 Writer、时间、git commit。v0.1 共五种事件：

| `type` | 字段 | 含义 |
|---|---|---|
| `evidence_recorded` | `id`、`excerpt`、`source` | 写入一条证据 |
| `memory_recorded` | 记忆的全部字段（见上表） | 写入一条记忆；`supersedes` 非空即同时取代其中的旧记忆 |
| `corroborated` | `memory`、`evidence`（id 数组，≥1） | 为一条记忆追加支撑证据，正文不变 |
| `retracted` | `memory`、`reason`（非空） | 撤回一条记忆 |
| `project_aliased` | `projects`（恰好两个项目标识） | 声明两个项目标识指同一个项目，即[项目别名（Project Alias）](../../GLOSSARY.md) |

- 取代没有独立事件：它就是新记忆的 `supersedes` 字段。一拆多＝同一事务里多条新记忆取代同一条旧记忆；多合一＝一条新记忆的 `supersedes` 列出多条旧记忆。
- 因为 `supersedes` 只能写在新记忆上，而新记忆不可能已被引用，取代关系在构造上不会成环，无需检查。
- 读端必须忽略不认识的字段；格式演进只能新增事件类型或可选字段（第 2 章）。

## 记忆的状态

状态不存储，每次由事件推导（[ADR 0002](../adr/0002-event-log-is-the-source-of-truth.md)）。按下表**自上而下**取第一个成立的：

| 状态 | 值 | 条件 | 默认可见 |
|---|---|---|---|
| [已清除（Purged）](../../GLOSSARY.md) | `purged` | 它的 `text` 已被清除为墓碑 | 否 |
| 撤回 | `retracted` | 有针对它的 `retracted` 事件 | 否 |
| 被取代 | `superseded` | 至少一条记忆的 `supersedes` 含它 | 否 |
| [无据（Unsupported）](../../GLOSSARY.md) | `unsupported` | 它当前引用的证据全部已被清除 | 否 |
| 生效 | `active` | 以上都不成立 | 是 |

转换规则：

- 已清除是终态：不能被取代、撤回、附证。它先前的撤回、被取代关系仍在日志里，只是状态取已清除。
- 只有**生效或无据**的记忆能被取代、撤回。被取代、撤回是终态，二者互斥。要「恢复」一条撤回的记忆，就写一条新记忆，引用同样的证据。
- 只有**生效**的记忆能被附证。
- 无据由清除自动推导，不写额外事件。
- 新记忆的作用域可以与被取代的旧记忆不同。
- 日志格式允许一条记忆有多个后继；v0.1 的写入路径不会产生（写入串行化，且只有生效或无据的记忆能被取代），但读端与[复核](07-review.md)必须能处理。

[可能过期（Stale）](../../GLOSSARY.md)不是状态：它从本机的核验缓存推导，不改变可见性，见第 6 章。

### 派生关系

以下关系由事件推导，供检索与 `get` 使用：

- `superseded_by`：哪些记忆的 `supersedes` 含它。
- `cited_by`（证据）：哪些记忆当前引用它。
- 附证历史：每次 `corroborated` 的证据、Writer、时间。
- 撤回原因：`retracted` 的 `reason`、Writer、时间。
- 最近一次记录或附证时间：用于排序（第 9 章的会话开始注入）与可能过期的判定（第 6 章）。
- 来源类型集合：记忆当前引用的证据的来源类型去重。含 `web` 而不含 `conversation` 的记忆是[网页来源记忆](../../GLOSSARY.md)，不进会话开始注入（第 4 章）；用户的原话算作背书，故只要有一条对话证据就不算。附证或取代改变当前证据时随之改变。

## 作用域与项目标识

[作用域（Scope）](../../GLOSSARY.md)只有两级：`global`（关于这个人本身）和 `project`（关于某个代码库或工作区）。

### 定位项目路径

工具与命令接受可选参数 `project`（一个路径），只用来定位。未给出时依次取：MCP `roots/list` → 环境变量 `CLAUDE_PROJECT_DIR` → 当前工作目录。

### 由路径推出项目标识

1. 路径在 git 仓库内：项目根是仓库顶层目录。
   - 有 `origin` remote：项目标识为归一化后的 remote URL：去掉协议、去掉用户名（`user@`）、把 scp 风格的 `:` 换成 `/`、去掉结尾的 `.git`。例如 `git@github.com:zibyn/almanac.git` 与 `https://github.com/zibyn/almanac` 都得到 `github.com/zibyn/almanac`。
   - 没有 `origin` remote：项目标识为 `git-root:<根提交 hash>`。
2. 路径不在 git 仓库内：项目根即该路径，项目标识为 `path:<绝对路径>`。

根提交：有多个根提交时，取 hash 字典序最小的那个。项目作用域的记忆总是同时记下 `root_commit`（项目在 git 内时）。

这样同一仓库的不同克隆是同一个项目，`file` 证据的相对路径也一致。

### 项目别名

- 由人运行 `almanac project alias <旧> <新>`（第 4 章）写入 `project_aliased` 事件，Writer 记为人。
- 别名按传递闭包分组：查询某个项目时，把同组的所有项目标识当作同一个项目（检索、`context`、`review`、写入幂等判断都如此）。
- 新的项目标识与已有项目的根提交相同时，**不自动合并**（fork 的根提交也相同），只在 `search` 与 `context` 的输出中提示可以声明别名。历史改写导致根提交变化时，也用别名处理。

## 清除与墓碑

[清除（Purge）](../../GLOSSARY.md)由人通过 CLI 发起（第 4 章），对象是一条证据或一条记忆；它是「日志只追加」的唯一例外（[ADR 0001](../adr/0001-purge-rewrites-the-log.md)）。

- 被清除证据的 `excerpt`、被清除记忆的 `text` 替换为[墓碑（Tombstone）](../../GLOSSARY.md)：
  ```json
  "purged": { "at": "2026-10-20T08:00:00.000Z", "reason": "含 API token", "by": { "name": "human" } }
  ```
  墓碑不含原文，也不含任何由原文派生的值（不留哈希，以免短内容被穷举确认）。被清除的证据不再有指纹。
- `by` 固定为 `{"name": "human"}`：清除只能由人在终端里执行（第 4 章）。
- 清除**证据**时，`source` 缩为只剩 `{ "type": … }`，其余定位字段（命令行、URL、路径、会话等）一并抹去：定位本身也可能含敏感内容。
- 清除**记忆**时，只替换 `text`；`scope`、`project`、`root_commit`、`epistemic`、`evidence`、`supersedes` 不是由正文派生的，原样保留，取代链与附证关系不断。针对它的 `retracted` 事件的 `reason` 也换成同一墓碑（撤回原因常复述正文），事件本身保留。
- 其他事件没有自由文本，事务字段不由内容派生，都不动。
- 清除证据时，CLI 列出当前引用它的所有记忆（含取代链上的历史版本）；清除记忆时，列出它当前引用的证据和取代链上的其他记忆。由人逐条决定是否一并清除（第 4 章）。
- 正文被清除的记忆推导为已清除；引用的证据全部被清除、正文仍在的记忆推导为无据。

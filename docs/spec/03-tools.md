# 3 智能体工具

本章是工具 schema 的唯一来源。almanac 向智能体暴露 8 个工具，MCP 与 CLI 同名、同 schema、同响应：

| 工具 | 读/写 | 作用 |
|---|---|---|
| [`search`](#search) | 只读 | 词法检索 + 结构化过滤 |
| [`get`](#get) | 只读 | 按 id 取全文与邻居，用于沿链跳转 |
| [`review`](#review) | 只读 | 列出待处置的问题 |
| [`record`](#record) | 写 | 写证据和/或记忆（含取代），原子 |
| [`corroborate`](#corroborate) | 写 | 为记忆附证 |
| [`retract`](#retract) | 写 | 撤回记忆 |
| [`distinguish`](#distinguish) | 写 | 声明两条相似记忆并存 |
| [`verify`](#verify) | 写核验缓存 | 显式核验来源，网页证据会联网 |

只在 CLI 提供的命令（`context`、`purge` 等）见第 4 章。

## 共同约定

### 调用方式

- **MCP**：`almanac mcp` 启动 stdio server（第 4 章），工具名即上表名字，参数即本章的 schema。
- **CLI**：`almanac <tool> --json '<参数 JSON>'`；`--json -` 从 stdin 读参数（便于智能体用 heredoc 传含引号的中文）。不给 `--json` 等同于参数 `{}`。
  - 成功：响应 JSON 写到 stdout，退出码 0。
  - 失败：错误 JSON 写到 stdout，退出码 1。

### 参数

- 每个工具都接受可选的 `project`：**一个路径，只用来定位项目**，不是项目标识。未给出时依次取 MCP `roots/list` → `CLAUDE_PROJECT_DIR` → 当前工作目录；由路径推出项目标识的规则见第 1 章。
- 以下由 almanac 自动填写，**不是参数**：Writer（第 9 章）、`conversation` 证据的 `host` 与 `session`、`web` 证据缺省的 `fetched_at`、事务的 `git_commit`、所有 id、时间。
- 未知参数字段报 `INVALID_ARGS`；类型或枚举值不对也报 `INVALID_ARGS`，除非下文给了更具体的错误码。

### 错误

```json
{ "error": { "code": "NOT_ACTIVE", "message": "mem_… 已被取代", "hint": "沿取代链看它的后继 mem_…" } }
```

- `hint` 可选，告诉智能体下一步怎么改。全部错误码见第 10 章。
- 任何工具都可能返回：`INVALID_ARGS`、`STORE_NEWER`（第 2 章）、`REMOTE_UNREACHABLE`、`UNAUTHORIZED`、`VERSION_MISMATCH`（第 8 章）。写工具还可能返回 `NO_WRITER`（第 9 章）、`STORE_BUSY`（第 2 章）与 `ALREADY_APPLIED`（第 8 章）。
- 写工具全部检查通过才写日志：一次调用＝一个事务，要么整体生效，要么什么都不写。

### 作用域过滤

`search`、`review`、`verify` 用同一套写法圈定范围：

| 参数 | 取值 | 默认 | 含义 |
|---|---|---|---|
| `scope` | `global` \| `project` \| `any` | `any` | `any`＝全局 + 当前项目 |
| `all` | bool | `false` | 为真时「项目」不限于当前项目，覆盖库里所有项目 |

「当前项目」总是包括它的[项目别名（Project Alias）](../../GLOSSARY.md)组（第 1 章）。

### 共用的输出结构

**Writer 与事务信息**（`Recorded`）：

```
{ tx, at, writer: { name, model?, session? }, device?, git_commit? }
```

**核验结果**（`Check`）：本设备核验缓存里的最近一次结果，没有则为 `null`。

```
{ result: "match" | "changed" | "unreachable" | "unverifiable", at }
```

**可能过期**（`Stale`）：记忆不带[可能过期（Stale）](../../GLOSSARY.md)标记时为 `null`，否则列出导致标记的证据（判定见第 6 章）：

```
[{ evidence: "ev_…", result: "changed" | "unreachable" }]
```

### 惰性核验

`search` 与 `get` 在返回前，对结果中记忆当前引用的 `file` 证据做惰性核验，结果写进本设备的核验缓存，并据此给出 `stale`。它们**从不联网**：`web` 证据只读缓存里已有的结果。惰性核验的方式见第 6 章。

## search

在记忆库里检索记忆（默认）或证据：中文分词后的词法匹配加结构化过滤。排序与分析器见第 5 章。

**给模型的描述**：

> 检索记忆（默认）或证据。中文分词后的词法匹配，每条结果附 `matched_terms`，没找到时换词再搜。默认只返回生效的记忆。本地文件证据在返回前惰性核验，出问题的记忆带 `stale`。不联网。

### 参数

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `query` | string? | — | 省略则只按过滤条件列出，按最近一次记录或附证时间倒序 |
| `kind` | `memory` \| `evidence` | `memory` | |
| `scope`、`all` | | | 见「作用域过滤」；`kind=evidence` 时不适用 |
| `status` | 状态值数组 | `["active"]` | `active` / `superseded` / `retracted` / `unsupported` / `purged`；已清除的没有全文字段，只在省略 `query` 时可能列出 |
| `epistemic` | 认知状态值数组? | — | `observed` / `user_stated` / `inferred` |
| `writer` | string? | — | Writer 的 `name` |
| `cites` | string? | — | 证据 id：只返回当前引用它的记忆 |
| `stale_only` | bool | `false` | 只返回带可能过期标记的记忆 |
| `source_type` | 来源类型数组? | — | 仅 `kind=evidence`：`conversation` / `file` / `web` / `command` |
| `limit` | int | 10 | 1–50 |
| `project` | path? | — | 见「共同约定」 |

### 响应

```
{
  hits: [MemoryHit | EvidenceHit],
  alias_hint?: { project, same_root_as: [project] }
}
```

`MemoryHit`：

| 字段 | 说明 |
|---|---|
| `id`、`kind: "memory"` | |
| `text`、`scope`、`project?`、`status`、`epistemic` | 见第 1 章 |
| `writer` | 记录它的 Writer 的 `name` |
| `last_at` | 最近一次记录或附证的时间 |
| `evidence_count` | 当前引用的证据条数 |
| `stale` | `Stale` |
| `score` | 排序分，只用于比较同一次结果内的先后 |
| `matched_terms` | 查询中命中的词（分词后），无 `query` 时为空数组 |

`EvidenceHit`：`id`、`kind: "evidence"`、`source_type`、`excerpt`（前 200 个字符）、`truncated`（bool）、`cited_by`（当前引用它的记忆 id）、`score`、`matched_terms`。

`alias_hint`：当前项目标识与库里另一个项目标识的根提交相同、且尚未声明别名时出现，提示人可以运行 `almanac project alias`（第 1 章）。

## get

按 id 取记忆或证据的完整记录与邻居。追溯取代链、证据来源，靠智能体沿返回的 id 多次调用 `get`。

**给模型的描述**：

> 按 id 取记忆或证据的完整记录。记忆附带全部证据（摘录、来源、核验结果）、`supersedes` / `superseded_by`、附证历史、撤回原因；证据附带 `cited_by`。沿这些 id 再调 `get` 即可追溯。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `ids` | string[] | 1–20 个，可混合记忆 id 与证据 id；超过报 `TOO_MANY_IDS` |
| `project` | path? | |

### 响应

```
{ records: [Memory | Evidence | { id, error: "NOT_FOUND" }] }
```

顺序与 `ids` 一致；不存在的 id 只在对应位置报 `NOT_FOUND`，不使整个调用失败。

`Memory`：

| 字段 | 说明 |
|---|---|
| `id`、`kind: "memory"` | |
| `text`、`scope`、`project?`、`root_commit?`、`epistemic`、`status` | 见第 1 章 |
| `recorded` | `Recorded`：记录它的事务 |
| `evidence` | 当前引用的全部证据，每条为下面的 `Evidence`（不含 `cited_by`） |
| `supersedes`、`superseded_by` | 记忆 id 数组 |
| `corroborations` | `[{ evidence: [id], recorded: Recorded }]`，按时间顺序 |
| `retraction` | `{ reason, recorded: Recorded }` 或 `null` |
| `distinguished_from` | 与它声明了[并存（Distinction）](../../GLOSSARY.md)的记忆 id |
| `stale` | `Stale` |

`Evidence`：`id`、`kind: "evidence"`、`excerpt`、`source`（第 1 章）、`recorded`（`Recorded`）、`check`（`Check`）、`cited_by`。

被清除的证据以墓碑 `purged: { at, reason, by }` 代替 `excerpt`，`source` 只剩 `type`（第 1 章）。

已清除的记忆照常返回，`status` 为 `purged`，以同样的墓碑代替 `text`，其余字段与派生关系不变；`retraction` 若有，`reason` 换成墓碑。`search` 的结果项同此。

## review

按需检查记忆库，列出待处置的问题。只读、不联网：可能过期只读核验缓存，不重新核验。只报告，从不处置，也不保存待办状态。四类问题的判定与相似度定义见第 7 章。

**给模型的描述**：

> 列出记忆库中待处置的问题：多个生效后继、相似对、可能过期、无据。只报告，不处置；每项附建议操作，由你或用户决定。用户要求整理记忆时调用。

### 参数

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `scope`、`all` | | | 见「作用域过滤」 |
| `project` | path? | — | |

### 响应

```
{ issues: [Issue] }
```

`Issue`：

| `type` | 其余字段 | `actions`（建议操作） |
|---|---|---|
| `multiple_successors` | `memory`：被取代的记忆；`successors`：它的生效后继 | `supersede`（一条新记忆同时取代这些后继）、`retract`（撤回其一） |
| `similar_pair` | `memories`：两条记忆 id；`similarity` | `supersede`、`retract`、`distinguish` |
| `stale` | `memory`；`stale`：`Stale` | `corroborate`、`supersede`、`retract` |
| `unsupported` | `memory` | `supersede`（用新证据重记）、`retract` |

每项另带涉及记忆的 `text`，免去一次 `get`。

## record

写入证据、记忆，或一次写两者。更新记忆＝写一条新记忆，在 `supersedes` 里列出旧记忆；一拆多＝把多条新记忆放在同一次调用里，都取代同一条旧记忆。

**给模型的描述**：

> 写入证据和/或记忆，原子：任何一条不过检查则整体拒绝。证据用 `ref` 在本次调用内被记忆引用，也可引用已有证据 id。更新记忆＝写一条新记忆并在 `supersedes` 里列出旧记忆；一拆多时把多条新记忆放在同一次调用里。倾向不写：只记以后还会用到、且有证据支撑的论断。用户说「记住 X」时，这句话本身就是证据。区分用户说的话（`speaker: "user"`）和你自己说的话（`speaker: "agent"`）。认知状态没有默认值：自己观察到的（如工具输出）是 `observed`，用户说的是 `user_stated`，推出来的是 `inferred`。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `evidence` | EvidenceIn[]? | 新证据 |
| `memories` | MemoryIn[]? | 新记忆 |
| `project` | path? | 决定 `scope: "project"` 的项目，以及 `file` 证据的项目根 |

`evidence` 与 `memories` 至少给一个。

`EvidenceIn`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `ref` | string? | 本次调用内的局部名字，供 `memories[].evidence` 引用；同一调用内不得重复 |
| `excerpt` | string | 摘录，UTF-8 不超过 4096 字节 |
| `source` | SourceIn | 见下 |

`SourceIn` 按 `type` 区分，字段即第 1 章「来源类型」去掉自动填写的部分：

```
  { type: "conversation", speaker: "user" | "agent", session?, turn?, transcript_path? }
| { type: "file", path, line? }            // path：相对项目根，不以 / 开头，不含 ..
| { type: "web", url, fetched_at? }        // url：http 或 https
| { type: "command", command, cwd?, exit_code }
```

- `conversation`：`host` 自动填为 Writer 名；未给 `session` 时填 Writer 的会话。
- `file`：`project` 自动填为由 `project` 参数推出的项目标识。

`MemoryIn`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `text` | string | 一条论断，不超过 500 个 Unicode 字符 |
| `scope` | `global` \| `project` | 必填 |
| `epistemic` | `observed` \| `user_stated` \| `inferred` | 必填，无默认值 |
| `evidence` | string[] | ≥1；每项是本次调用的 `ref` 或已有证据 id |
| `supersedes` | string[]? | 被取代的记忆 id |

`scope: "project"` 时，`project` 与 `root_commit` 由 `project` 参数推出（第 1 章）。

### 硬检查

按下表检查，任何一条不过即整体拒绝：

| 错误码 | 条件 |
|---|---|
| `MISSING_FIELD` | `evidence` 与 `memories` 都没给；或缺必填字段（`excerpt`、`source`、`speaker`、`url`、`command`、`exit_code`、`text`、`epistemic` 等） |
| `EXCERPT_TOO_LONG` | 摘录超过 4096 字节。hint：只截取能支撑记忆的那一小段 |
| `FILE_OUTSIDE_PROJECT` | `file` 的 `path` 以 `/` 开头或含 `..`。hint：项目外的文件按 `command` 记录（如 `cat` 的输出） |
| `FILE_NOT_FOUND` | `file` 证据的文件在项目根下不存在或不可读。hint：检查 `path` 是否相对项目根 |
| `EXCERPT_NOT_IN_FILE` | 文件存在，但摘录不在其中。hint：重新读取文件，逐字复制摘录 |
| `TEXT_TOO_LONG` | 正文超过 500 字符。hint：一条记忆只写一个论断，拆成多条 |
| `SCOPE_INVALID` | `scope` 不是 `global` 或 `project` |
| `NO_EVIDENCE` | 记忆的 `evidence` 为空。hint：用户说「记住 X」时，这句话本身就是证据 |
| `EVIDENCE_NOT_FOUND` | 引用的 `ref` 未在本次调用中定义，或证据 id 不存在、已被清除 |
| `MEMORY_NOT_FOUND` | `supersedes` 中的记忆不存在 |
| `NOT_ACTIVE` | `supersedes` 中的记忆既不是生效也不是无据。被取代时 hint 给出它的后继 |
| `DUPLICATE_TEXT` | 见下文「重复」 |

`FILE_NOT_FOUND` 与 `EXCERPT_NOT_IN_FILE` 是唯一读取来源的硬检查：本次调用新给出的每条 `file` 证据（按 id 引用的已有证据除外），都按第 6 章「文件」的方式与当前文件比对，在结构性检查之后、重复判断之前进行，即使随后会去重也照样比对。写入成功后，比对结果作为一次 `match`（连同 mtime 与大小）写进本设备核验缓存。`web` 证据写入时不联网核对。

### 重复

- **证据**：与已有证据指纹相同（第 1 章）则不新写，返回已有 id，`deduped: true`。同一调用里指纹相同的两条也合为一条。
- **记忆**：同作用域（项目作用域按别名组）内已有一条生效记忆，归一化后正文相同（归一化同第 1 章的指纹）：
  - 本次的证据都已被它引用，且本次的 `supersedes` 都已在它的 `supersedes` 里：视为重试，不新写，返回它的 id，`deduped: true`。
  - 否则报 `DUPLICATE_TEXT`，hint：改用 `corroborate` 给它附证。

重复判断先于 `supersedes` 检查，所以一次成功的取代被重试时返回 `deduped`，而不是 `NOT_ACTIVE`。

### 响应

```
{
  evidence: [{ ref?, id, deduped }],
  memories: [{ id, deduped, similar: [{ id, text, similarity }] }]
}
```

- 两个数组与参数一一对应，顺序相同。
- `similar`：软提示，不影响写入。同作用域内与新记忆最相似的生效记忆，最多 3 条，按相似度降序；不含本次 `supersedes` 里的记忆。相似度定义与阈值与[复核](07-review.md)的相似对共用，见第 7 章。`deduped` 为真时为空数组。

## corroborate

为一条生效记忆追加支撑证据，正文不变。

**给模型的描述**：

> 为一条生效记忆追加支撑证据。重读来源后确认记忆仍成立时用它（以来源当前的摘录为新证据），或 `record` 报 `DUPLICATE_TEXT` 时用它。附证后，可能过期只看这次附证带来的证据。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `memory` | string | 记忆 id |
| `evidence` | (string \| EvidenceIn)[] | ≥1；已有证据 id，或内联新证据（同 `record`，`ref` 无意义，忽略） |
| `project` | path? | |

内联证据与 `corroborated` 事件写在同一个事务里；内联证据的检查与重复判断同 `record`。

### 硬检查

`MEMORY_NOT_FOUND`；`NOT_ACTIVE`（记忆不是生效状态）；`NO_EVIDENCE`；`EVIDENCE_NOT_FOUND`；内联证据的 `MISSING_FIELD`、`EXCERPT_TOO_LONG`、`FILE_OUTSIDE_PROJECT`、`FILE_NOT_FOUND`、`EXCERPT_NOT_IN_FILE`。

### 响应

```
{ memory, added: [evidence_id], already_cited: [evidence_id] }
```

`added` 为空（所给证据都已被引用）时不写事件。

## retract

撤回一条记忆，即日常所说的「遗忘」。记忆仍留在历史里。

**给模型的描述**：

> 声明一条记忆本就错误或不该记录，必须说明原因。记忆仍留在历史里，可用 `search` 的 `status` 过滤查到。如果只是情况变了，请用 `record` + `supersedes`，不要撤回。不能物理删除；需要抹除敏感内容时，请建议用户在终端运行 `almanac purge`。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `memory` | string | 记忆 id |
| `reason` | string | 非空（去掉首尾空白后） |
| `project` | path? | |

### 硬检查

`MEMORY_NOT_FOUND`；`MISSING_FIELD`（`reason` 为空）；`NOT_ACTIVE`（记忆既不是生效也不是无据）。

### 响应

```
{ memory, status: "retracted" }
```

## distinguish

声明两条相似的记忆各自成立，既不重复也不矛盾。声明之后这一对不再出现在复核的相似对里。

**给模型的描述**：

> 声明两条相似的记忆各自成立（不是重复，也不矛盾），之后 `review` 不再把这一对列为相似对。不可撤销：判断错了就取代或撤回其中一条。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `memories` | string[] | 恰好两个不同的记忆 id，顺序无关 |
| `project` | path? | |

### 硬检查

`INVALID_ARGS`（不是恰好两个不同的 id）；`MEMORY_NOT_FOUND`；`NOT_ACTIVE`（任一条不是生效状态）。

### 响应

```
{ memories: [id, id], deduped }
```

这一对已声明过并存时不新写，`deduped: true`。

## verify

显式核验：回到证据的来源，确认摘录是否仍在。这是唯一会联网的工具。核验方式见第 6 章。

**给模型的描述**：

> 回到证据来源核验摘录是否仍在：`match`（相符）/ `changed`（已变）/ `unreachable`（不可达）/ `unverifiable`（不可核验，如对话）。注意：网页证据会对其 URL 发一次 GET 请求；设 `include_web: false` 可避免联网。只核证据与来源是否一致，不判断记忆是否成立；返回可能过期的记忆，由你决定附证、取代或撤回。

### 参数

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `ids` | string[]? | — | 记忆 id；给出时只核验这些记忆，忽略 `scope`、`all` |
| `scope`、`all` | | | 见「作用域过滤」；只核验范围内的生效记忆 |
| `include_web` | bool | `true` | 为假时跳过 `web` 证据 |
| `project` | path? | — | |

核验的对象是所选记忆**当前引用**的全部证据，同一条证据只核一次。结果写进本设备的核验缓存，不进日志。

### 响应

```
{
  checked: [{ evidence, source_type, result, at }],
  network_requests: [url],
  stale: [{ memory, text, stale: Stale }]
}
```

- `checked` 不含被跳过的 `web` 证据。
- `network_requests`：本次实际发出的请求。
- `stale`：所选记忆中核验后带可能过期标记的那些。

`ids` 中的记忆不存在报 `MEMORY_NOT_FOUND`。

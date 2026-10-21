# 3 智能体工具

本章是工具 schema 的唯一来源。almanac 向智能体暴露 7 个工具，MCP 与 CLI 同名、同 schema、同响应：

| 工具 | 读/写 | 作用 |
|---|---|---|
| [`search`](#search) | 只读 | 词法检索 + 结构化过滤 |
| [`get`](#get) | 只读 | 按 id 取全文与邻居，用于沿链跳转 |
| [`review`](#review) | 只读 | 列出待处置的问题 |
| [`record`](#record) | 写 | 写证据和/或记忆（含取代），原子 |
| [`corroborate`](#corroborate) | 写 | 为记忆附证 |
| [`retract`](#retract) | 写 | 撤回记忆 |
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
- 任何工具都可能返回 `INVALID_ARGS` 与 `STORE_IO`（第 2 章）。写工具还可能返回 `NO_WRITER`（第 9 章）、`STORE_BUSY` 与 `STORE_NEWER`（第 2 章）。
- 参数 JSON 的解析（`INVALID_ARGS`、`MISSING_FIELD`）先于 Writer 推断：参数有错时报参数错误，不报 `NO_WRITER`。
- 多处不过检查时只报第一处：按下文各工具「硬检查」所述的先后，同一类检查按参数顺序。
- 写工具全部检查通过才写日志：一次调用＝一个事务，要么整体生效，要么什么都不写。

### 作用域过滤

`search`、`review`、`verify` 用同一套写法圈定范围：

| 参数 | 取值 | 默认 | 含义 |
|---|---|---|---|
| `scope` | `global` \| `project` \| `any` | `any` | `any`＝全局 + 当前项目 |
| `all` | bool | `false` | 为真时「项目」不限于当前项目，覆盖库里所有项目 |

「当前项目」总是包括它的[项目别名（Project Alias）](../../GLOSSARY.md)组（第 1 章）。

只有范围用到当前项目时（`scope` 不是 `global` 且 `all` 为假；`verify` 另要求没给 `ids`），项目路径解析失败才使调用失败（`INVALID_ARGS`，第 1 章）；否则静默当作没有当前项目，`search` 也就不给 `alias_hint`。

### 共用的输出结构

**Writer 与事务信息**（`Recorded`）：

```
{ tx, at, writer: { name, model?, session? }, git_commit? }
```

**核验结果**（`Check`）：本机核验缓存里的最近一次结果，没有则为 `null`。

```
{ result: "match" | "changed" | "unreachable" | "unverifiable", at }
```

**可能过期**（`Stale`）：记忆不带[可能过期（Stale）](../../GLOSSARY.md)标记时为 `null`，否则列出导致标记的证据（判定见第 6 章）：

```
[{ evidence: "ev_…", result: "changed" | "unreachable" }]
```

### 惰性核验

`search` 与 `get` 在返回前，对结果中记忆当前引用的 `file` 证据做惰性核验，结果写进本机的核验缓存，并据此给出 `stale`。它们**从不联网**：`web` 证据只读缓存里已有的结果。惰性核验的方式见第 6 章。

## search

在记忆库里检索记忆（默认）或证据：中文分词后的词法匹配加结构化过滤。排序与分析器见第 5 章。

**给模型的描述**：

> 检索记忆（默认）或证据。中文分词后的词法匹配，每条结果附 `matched_terms`，没找到时换词再搜。默认只返回生效的记忆。本地文件证据在返回前惰性核验，出问题的记忆带 `stale`。不联网。

### 参数

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `query` | string? | — | 省略则只按过滤条件列出，按最近一次记录或附证时间倒序 |
| `kind` | `memory` \| `evidence` | `memory` | |
| `scope`、`all` | | | 见「作用域过滤」 |
| `status` | 状态值数组 | `["active"]` | `active` / `superseded` / `retracted` / `unsupported` / `purged`；已清除的没有全文字段，只在省略 `query` 时可能列出 |
| `epistemic` | 认知状态值数组? | — | `observed` / `user_stated` / `inferred` |
| `writer` | string? | — | Writer 的 `name` |
| `cites` | string? | — | 证据 id：只返回当前引用它的记忆（不论状态，仍受 `status` 过滤） |
| `stale_only` | bool | `false` | 只返回带可能过期标记的记忆 |
| `source_type` | 来源类型数组? | — | 仅 `kind=evidence`：`conversation` / `file` / `web` / `command` |
| `limit` | int | 10 | 1–50 |
| `project` | path? | — | 见「共同约定」 |

`kind=evidence` 时检索整个库，只接受 `query`、`source_type`、`limit`、`project`；给出只对记忆有效的参数（`scope`、`all`、`status`、`epistemic`、`writer`、`cites`、`stale_only`）报 `INVALID_ARGS`，即使给的是默认值。`kind=memory` 时给出 `source_type` 同样报 `INVALID_ARGS`。

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
| `source_types` | 当前证据的来源类型去重（第 1 章「派生关系」），按在当前证据中首次出现的顺序，如 `["web"]`；不用 `get` 也能看出记忆是否来自网页 |
| `stale` | `Stale` |
| `score` | 排序分，只用于比较同一次结果内的先后 |
| `matched_terms` | 查询中命中的词（分词后），无 `query` 时为空数组 |

`EvidenceHit`：`id`、`kind: "evidence"`、`source_type`、`excerpt`（前 200 个字符）、`truncated`（bool）、`cited_by`（当前引用它的记忆 id，含已被取代、撤回的）、`score`、`matched_terms`。被清除的证据不被 `query` 命中，但省略 `query` 时照常列出，以墓碑 `purged` 代替 `excerpt`。

`alias_hint`：当前项目标识与库里另一个项目标识的根提交相同、且尚未声明别名时出现，提示人可以运行 `almanac project alias`（第 1 章）。库里的项目根提交只取自记忆的 `root_commit`。不论 `kind`、`scope` 取什么，只要能解析出当前项目就判断。

## get

按 id 取记忆或证据的完整记录与邻居。追溯取代链、证据来源，靠智能体沿返回的 id 多次调用 `get`。

**给模型的描述**：

> 按 id 取记忆或证据的完整记录。记忆附带全部证据（摘录、来源、核验结果）、`supersedes` / `superseded_by`、附证历史、撤回原因；证据附带 `cited_by`。沿这些 id 再调 `get` 即可追溯。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `ids` | string[] | 1–20 个，可混合记忆 id 与证据 id；超过报 `TOO_MANY_IDS`，为空报 `INVALID_ARGS` |
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
| `stale` | `Stale` |

`Evidence`：`id`、`kind: "evidence"`、`excerpt`、`source`（第 1 章）、`recorded`（`Recorded`）、`check`（`Check`）、`cited_by`（含已被取代、撤回的记忆）。

惰性核验只针对取到的记忆当前引用的证据；直接按证据 id 取时只读缓存，不核验。

被清除的证据以墓碑 `purged: { at, reason, by }` 代替 `excerpt`，`source` 只剩 `type`（第 1 章）。

已清除的记忆照常返回，`status` 为 `purged`，以同样的墓碑代替 `text`，其余字段与派生关系不变；`retraction` 若有，为 `{ purged, recorded }`（墓碑代替 `reason`）。`search` 的结果项同此。

## review

按需检查记忆库，列出待处置的问题。只读、不联网：可能过期只读核验缓存，不重新核验。只报告，从不处置，也不保存待办状态。两类问题的判定见第 7 章。

**给模型的描述**：

> 列出记忆库中待处置的问题：可能过期、无据。只报告，不处置；每项附建议操作，由你或用户决定。用户要求整理记忆时调用。

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
| `stale` | `memory`；`stale`：`Stale` | `corroborate`、`supersede`、`retract` |
| `unsupported` | `memory` | `supersede`（用新证据重记）、`retract` |

每项另带涉及记忆的 `text`，免去一次 `get`。

## record

写入证据、记忆，或一次写两者。更新记忆＝写一条新记忆，在 `supersedes` 里列出旧记忆；一拆多＝把多条新记忆放在同一次调用里，都取代同一条旧记忆。

**给模型的描述**：

> 写入证据和/或记忆，原子：任何一条不过检查则整体拒绝。证据用 `ref` 在本次调用内被记忆引用，也可引用已有证据 id。更新记忆＝写一条新记忆并在 `supersedes` 里列出旧记忆；一拆多时把多条新记忆放在同一次调用里。倾向不写：只记以后还会用到、且有证据支撑的论断。用户说「记住 X」时，这句话本身就是证据；对话证据只收用户说的话，你自己说的话不能作为证据，用户确认你的提议时，摘录先写一行 `> 智能体：<你的提议>`，再写用户原话。认知状态可省略：证据全是对话时默认 `user_stated`，否则默认 `inferred`；自己观察到的（如工具输出）请显式写 `observed`。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `evidence` | EvidenceIn[]? | 新证据 |
| `memories` | MemoryIn[]? | 新记忆 |
| `project` | path? | 决定 `scope: "project"` 的项目，以及 `file` 证据的项目根 |

`evidence` 与 `memories` 至少给一个，且不能都是空数组。

只有本次调用含 `scope: "project"` 的记忆或 `file` 证据时，项目路径解析失败才使调用失败（`INVALID_ARGS`，第 1 章）。否则路径无效也照常写入，事务不带 `git_commit`。

`EvidenceIn`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `ref` | string? | 本次调用内的局部名字，供 `memories[].evidence` 引用；同一调用内不得重复 |
| `excerpt` | string | 摘录，UTF-8 不超过 4096 字节 |
| `source` | SourceIn | 见下 |

`SourceIn` 按 `type` 区分，字段即第 1 章「来源类型」去掉自动填写的部分：

```
  { type: "conversation", session?, turn?, transcript_path? }   // 只收用户说的话
| { type: "file", path, line? }            // path：相对项目根，不以 / 开头，不含 ..
| { type: "web", url, fetched_at? }        // url：http 或 https
| { type: "command", command, cwd?, exit_code }
```

- `conversation`：`host` 自动填为 Writer 名；未给 `session` 时填 Writer 的会话。摘录的写法见第 1 章「来源类型」。
- `file`：`project` 自动填为由 `project` 参数推出的项目标识。

`MemoryIn`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `text` | string | 一条论断，不超过 500 个 Unicode 字符 |
| `scope` | `global` \| `project` | 必填；缺失或取其他值都报 `SCOPE_INVALID` |
| `epistemic` | `observed` \| `user_stated` \| `inferred` | 可省略；省略时由证据推出（第 1 章） |
| `evidence` | string[] | ≥1；每项是本次调用的 `ref` 或已有证据 id；缺失与空数组同样报 `NO_EVIDENCE` |
| `supersedes` | string[]? | 被取代的记忆 id |

`scope: "project"` 时，`project` 与 `root_commit` 由 `project` 参数推出（第 1 章）。

### 硬检查

按下表检查，任何一条不过即整体拒绝：

| 错误码 | 条件 |
|---|---|
| `MISSING_FIELD` | `evidence` 与 `memories` 都没给或都为空；或缺必填字段（`excerpt`、`source`、`url`、`command`、`exit_code`、`text` 等；不含 `scope` 与记忆的 `evidence`） |
| `EXCERPT_TOO_LONG` | 摘录超过 4096 字节。hint：只截取能支撑记忆的那一小段 |
| `FILE_OUTSIDE_PROJECT` | `file` 的 `path` 以 `/` 开头或含 `..`。hint：项目外的文件按 `command` 记录（如 `cat` 的输出） |
| `FILE_NOT_FOUND` | `file` 证据的文件在项目根下不存在或不可读。hint：检查 `path` 是否相对项目根 |
| `EXCERPT_NOT_IN_FILE` | 文件存在，但摘录不在其中。hint：重新读取文件，逐字复制摘录 |
| `TEXT_TOO_LONG` | 正文超过 500 字符。hint：一条记忆只写一个论断，拆成多条 |
| `SCOPE_INVALID` | `scope` 缺失，或不是 `global` 或 `project` |
| `NO_EVIDENCE` | 记忆的 `evidence` 为空。hint：用户说「记住 X」时，这句话本身就是证据 |
| `EVIDENCE_NOT_FOUND` | 引用的 `ref` 未在本次调用中定义，或证据 id 不存在、已被清除 |
| `MEMORY_NOT_FOUND` | `supersedes` 中的记忆不存在 |
| `NOT_ACTIVE` | `supersedes` 中的记忆既不是生效也不是无据。被取代时 hint 给出它的后继 |
| `SECRET_DETECTED` | 写入的文本命中下文「密钥检查」的规则 |
| `DUPLICATE_TEXT` | 见下文「重复」 |

检查顺序：先逐条检查 `evidence`（按参数顺序），再逐条检查 `memories` 的结构（`SECRET_DETECTED`、`SCOPE_INVALID`、`TEXT_TOO_LONG`、`NO_EVIDENCE`、`EVIDENCE_NOT_FOUND`），然后做文件比对，最后逐条做重复判断与 `supersedes` 检查。

`FILE_NOT_FOUND` 与 `EXCERPT_NOT_IN_FILE` 是唯一读取来源的硬检查：本次调用新给出的每条 `file` 证据（按 id 引用的已有证据除外），都按第 6 章「文件」的方式与当前文件比对，在结构性检查之后、重复判断之前进行，即使随后会去重也照样比对。写入成功后，比对结果作为一次 `match`（连同 mtime 与大小）写进本机核验缓存。`web` 证据写入时不联网核对。

### 密钥检查

写入前，用下表的固定规则扫描本次调用新写入的全部文本：证据的 `excerpt`，`source` 中除 `type`、`fetched_at` 外的字符串字段（如 `command`、`cwd`、`url`、`path`、`session`、`transcript_path`），记忆的 `text`，`retract` 的 `reason`。按 id 引用的已有证据不再扫描。任何一处命中即报 `SECRET_DETECTED`，整体拒绝；它与其他结构性检查同时进行，先于文件比对。自动填写的字段（`host`、`session` 的缺省值、`file` 的 `project`）不扫描。`corroborate` 的内联证据在记忆存在与状态检查之后扫描；`retract` 的 `reason` 在 `MEMORY_NOT_FOUND` 之前扫描。

| 规则 | 模式（正则） |
|---|---|
| `private_key` | `-----BEGIN [A-Z ]*PRIVATE KEY-----` |
| `aws_access_key` | `\b(AKIA\|ASIA)[0-9A-Z]{16}\b` |
| `github_token` | `\bgh[pousr]_[A-Za-z0-9]{36,}` 或 `\bgithub_pat_[A-Za-z0-9_]{22,}` |
| `gitlab_token` | `\bglpat-[A-Za-z0-9_-]{20,}` |
| `slack_token` | `\bxox[abprs]-[A-Za-z0-9-]{10,}` |
| `anthropic_key` | `\bsk-ant-[A-Za-z0-9_-]{20,}` |
| `openai_key` | `\bsk-(proj-)?[A-Za-z0-9_-]{20,}` |
| `stripe_key` | `\b[sr]k_live_[A-Za-z0-9]{20,}` |
| `google_api_key` | `\bAIza[0-9A-Za-z_-]{35}` |
| `jwt` | `\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}` |
| `url_credentials` | `[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]+:[^/\s@]+@` |

- `\b` 按 ASCII 判定词边界（汉字算非词字符）：有意让紧挨中文的密钥（如 `密钥AKIA…`）也能命中。
- 规则内置、固定，以精确为先：只收有明显前缀或结构的格式，不做熵值启发、不按 `password=` 等关键词匹配，不可由用户配置。
- 规则只作用于写入，重放日志时不检查，所以规则在以后版本中增删不影响已有数据，也不需要版本号或指纹。
- `message` 给出命中的规则名、字段路径（如 `evidence[0].excerpt`、`evidence[1].source.command`、`reason`）和字符偏移（从 0 起的 Unicode 字符数），**不回显命中的值**：错误输出会进入智能体上下文和宿主的会话记录。多处命中时只报第一处：先按字段顺序（证据依次为 `excerpt`、`source` 各字段，再到记忆的 `text`），同一字段内取位置最前的，位置相同取上表靠前的规则。
- 没有强制写入的开关：智能体也能传参，开关会让拦截失效。误报时由智能体按 hint 改写；规则误报在后续版本中修正。
- hint：对话、命令证据与记忆正文，把密钥换成 `<redacted>` 后重试；`file` 证据换一段不含密钥的更窄摘录（打码后的摘录对不上文件）；密钥本身就是要记的内容时，不要记录。
- 已在库里的密钥不扫描，`review` 也不报；抹除只能由人 `almanac purge`（第 4 章）。

### 重复

- **证据**：与已有证据指纹相同（第 1 章）则不新写，返回已有 id，`deduped: true`。同一调用里指纹相同的两条也合为一条：第一条 `deduped: false`，之后的 `deduped: true`，id 相同。
- **记忆**：同作用域（项目作用域按别名组）内已有一条生效记忆，归一化后正文相同（归一化同第 1 章的指纹）：
  - 本次的证据都已被它引用，且本次的 `supersedes` 都已在它的 `supersedes` 里：视为重试，不新写，返回它的 id，`deduped: true`。
  - 否则报 `DUPLICATE_TEXT`，hint：改用 `corroborate` 给它附证。

重复判断先于 `supersedes` 检查，所以一次成功的取代被重试时返回 `deduped`，而不是 `NOT_ACTIVE`。同一次调用里的多条记忆之间不做重复判断。

### 响应

```
{
  evidence: [{ ref?, id, deduped }],
  memories: [{ id, deduped, similar: [{ id, text, similarity }] }]
}
```

- 两个数组与参数一一对应，顺序相同。
- `similar`：软提示，不影响写入。同作用域内与新记忆相似度达到阈值的生效记忆，最多 3 条，按相似度降序；`similarity` 为 0 到 1 的重叠系数，原样输出浮点数、不取整；不含本次 `supersedes` 里的记忆，也不与同一次调用里的其他新记忆比较。相似度定义与阈值见第 7 章。`deduped` 为真时为空数组。

## corroborate

为一条生效记忆追加支撑证据，正文不变。

**给模型的描述**：

> 为一条生效记忆追加支撑证据。重读来源后确认记忆仍成立时用它（以来源当前的摘录为新证据），或 `record` 报 `DUPLICATE_TEXT` 时用它。附证后，可能过期只看这次附证带来的证据。

### 参数

| 字段 | 类型 | 说明 |
|---|---|---|
| `memory` | string | 记忆 id |
| `evidence` | (string \| EvidenceIn)[] | ≥1；已有证据 id，或内联新证据（同 `record`，`ref` 无意义，忽略）；其他类型的项报 `INVALID_ARGS` |
| `project` | path? | |

内联证据与 `corroborated` 事件写在同一个事务里；内联证据的检查与重复判断同 `record`。

### 硬检查

按序：内联证据格式不对（`INVALID_ARGS`、`MISSING_FIELD`）；`MEMORY_NOT_FOUND`；`NOT_ACTIVE`（记忆不是生效状态）；`NO_EVIDENCE`；`EVIDENCE_NOT_FOUND`（按 id 引用的证据）；内联证据的其余检查同 `record`（`EXCERPT_TOO_LONG`、`SECRET_DETECTED`、`FILE_OUTSIDE_PROJECT`、`FILE_NOT_FOUND`、`EXCERPT_NOT_IN_FILE`）。

总是解析项目（为事务的 `git_commit` 与内联 `file` 证据）；解析失败即调用失败。

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
| `reason` | string | 非空（去掉首尾空白后）；按原样写入，不去空白 |
| `project` | path? | |

### 硬检查

按序：`MISSING_FIELD`（`reason` 为空）；`SECRET_DETECTED`（`reason` 命中密钥规则，见 `record`「密钥检查」）；`MEMORY_NOT_FOUND`；`NOT_ACTIVE`（记忆既不是生效也不是无据）。

总是解析项目（为事务的 `git_commit`）；解析失败即调用失败。

### 响应

```
{ memory, status: "retracted" }
```

## verify

显式核验：回到证据的来源，确认摘录是否仍在。这是唯一会联网的工具。核验方式见第 6 章。

**给模型的描述**：

> 回到证据来源核验摘录是否仍在：`match`（相符）/ `changed`（已变）/ `unreachable`（不可达）/ `unverifiable`（不可核验，如对话）。注意：网页证据会对其 URL 发一次 GET 请求；设 `include_web: false` 可避免联网。只核证据与来源是否一致，不判断记忆是否成立；返回可能过期的记忆，由你决定附证、取代或撤回。

### 参数

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `ids` | string[]? | — | 记忆 id；给出时只核验这些记忆（不论状态），忽略 `scope`、`all` |
| `scope`、`all` | | | 见「作用域过滤」；只核验范围内的生效记忆 |
| `include_web` | bool | `true` | 为假时跳过 `web` 证据 |
| `project` | path? | — | |

核验的对象是所选记忆**当前引用**的全部证据，同一条证据只核一次。结果写进本机的核验缓存，不进日志。

### 响应

```
{
  checked: [{ evidence, source_type, result, at }],
  network_requests: [url],
  stale: [{ memory, text, stale: Stale }]
}
```

- `checked` 不含被跳过的 `web` 证据，也不含本机定位不到其项目的 `file` 证据（第 6 章）。
- `network_requests`：本次实际发出的请求，每条证据一项（URL 相同的证据各请求一次），只记初始 URL，不记重定向。
- `stale`：所选记忆中核验后带可能过期标记的那些。给了 `ids` 时按 `ids` 的顺序（去重），否则按记忆 id 升序。

`ids` 中的记忆不存在报 `MEMORY_NOT_FOUND`。

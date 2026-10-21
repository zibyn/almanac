# 10 错误码与 hint

## 格式

```json
{ "error": { "code": "NOT_ACTIVE", "message": "mem_… 已被取代", "hint": "沿取代链看它的后继 mem_…" } }
```

- `code`：下表之一，稳定，供程序判断。
- `message`：中文，说明这一次具体哪里不对（哪个字段、哪个 id）。
- `hint`：可选，告诉智能体下一步怎么改。下表「hint」一列是要点，具体措辞由实现决定。

### 各接入面上的表现

| 接入面 | 成功 | 失败 |
|---|---|---|
| CLI 工具（第 3 章） | stdout 为响应 JSON，退出码 0 | stdout 为错误 JSON，退出码 1 |
| MCP 工具 | 工具结果的文本内容为响应 JSON | 工具结果 `isError: true`，文本内容为错误 JSON |
| 仅 CLI 命令（第 4 章） | 退出码 0 | stderr 一行 `<code>：<message>`，退出码 1；`context`、`remind` 例外，永远退出码 0（第 4 章） |

`get` 中不存在的 id 不是错误：只在对应位置返回 `{ id, error: "NOT_FOUND" }`（第 3 章）。

## 全部错误码

### 参数与写入检查

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `INVALID_ARGS` | 参数 JSON 不合法、有未知字段、类型或枚举值不对且没有更具体的码；`project alias` 两个标识相同或为空 | — | 3、4 |
| `MISSING_FIELD` | 缺必填字段；`record` 的 `evidence` 与 `memories` 都没给；`retract` 的 `reason` 为空 | 说明缺哪个字段 | 3 |
| `TOO_MANY_IDS` | `get` 的 `ids` 超过 20 个 | 分批取 | 3 |
| `EXCERPT_TOO_LONG` | 摘录超过 4096 字节 | 只截取能支撑记忆的那一小段 | 3 |
| `FILE_OUTSIDE_PROJECT` | `file` 来源的 `path` 以 `/` 开头或含 `..` | 项目外的文件按 `command` 记录（如 `cat` 的输出） | 3 |
| `FILE_NOT_FOUND` | 写入的 `file` 证据在项目根下不存在或不可读 | 检查 `path` 是否相对项目根 | 3、6 |
| `EXCERPT_NOT_IN_FILE` | 写入的 `file` 证据的摘录不在当前文件中 | 重新读取文件，逐字复制摘录 | 3、6 |
| `TEXT_TOO_LONG` | 记忆正文超过 500 字符 | 一条记忆只写一个论断，拆成多条 | 3 |
| `SCOPE_INVALID` | `scope` 不是 `global` 或 `project` | 必须显式选择，不按 cwd 默认 | 3 |
| `NO_EVIDENCE` | 记忆或附证的 `evidence` 为空 | 用户说「记住 X」时，这句话本身就是证据 | 3 |
| `EVIDENCE_NOT_FOUND` | 引用的 `ref` 未定义，或证据 id 不存在、已被清除 | — | 3 |
| `MEMORY_NOT_FOUND` | 引用的记忆 id 不存在 | — | 3 |
| `NOT_ACTIVE` | 取代、撤回的对象既不是生效也不是无据；附证的对象不是生效 | 被取代时给出它的生效后继，提示沿取代链取代最新的 | 1、3 |
| `SECRET_DETECTED` | 写入的文本命中密钥规则；`message` 给规则名、字段路径与偏移，不回显命中的值 | 对话、命令证据与正文把密钥换成 `<redacted>` 重试；`file` 证据换不含密钥的摘录；密钥本身是要记的内容时不要记 | 3 |
| `DUPLICATE_TEXT` | 同作用域已有正文相同的生效记忆，但本次证据或取代对象不同 | 改用 `corroborate` 给它附证 | 3 |
| `NO_WRITER` | 推断不出 Writer（写操作） | 宿主嵌套时列出检测到的宿主；两种情况都提示在命令前加 `ALMANAC_WRITER=<自己的名字>` 重试（人用 `human`） | 9 |

### 存储

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `STORE_BUSY` | 10 秒内拿不到写锁 | 稍后重试 | 2 |
| `STORE_NEWER` | 日志中有比本二进制新的格式版本，记忆库只读：写入、`purge`、`project alias` 被拒绝，读照常 | 升级 almanac | 2 |

### 清除

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `NOT_INTERACTIVE` | `purge` 的 stdin 不是终端 | 清除须由人在终端里运行 | 4 |
| `ALREADY_PURGED` | 清除已被清除的对象 | — | 4 |

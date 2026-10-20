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
| 仅 CLI 命令（第 4 章） | 退出码 0 | stderr 一行 `<code>：<message>`，退出码 1；`context` 例外，永远退出码 0（第 4 章） |
| HTTP（第 8 章） | 200 | 401 / 404 / 422，响应体为错误 JSON |

`get` 中不存在的 id 不是错误：只在对应位置返回 `{ id, error: "NOT_FOUND" }`（第 3 章）。

## 全部错误码

### 参数与写入检查

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `INVALID_ARGS` | 参数 JSON 不合法、有未知字段、类型或枚举值不对且没有更具体的码；`distinguish` 不是恰好两个不同 id；`project alias` 两个标识相同或为空；HTTP 未知 op | — | 3、4、8 |
| `MISSING_FIELD` | 缺必填字段；`record` 的 `evidence` 与 `memories` 都没给；`retract` 的 `reason` 为空 | 说明缺哪个字段；`epistemic` 缺失时列出三个取值 | 3 |
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
| `NOT_ACTIVE` | 取代、撤回的对象既不是生效也不是无据；附证、并存的对象不是生效 | 被取代时给出它的生效后继，提示沿取代链取代最新的 | 1、3 |
| `DUPLICATE_TEXT` | 同作用域已有正文相同的生效记忆，但本次证据或取代对象不同 | 改用 `corroborate` 给它附证 | 3 |
| `NO_WRITER` | 推断不出 Writer（写操作） | 宿主嵌套时列出检测到的宿主，提示在命令前加 `ALMANAC_WRITER=<宿主名>`；否则提示设置 `ALMANAC_WRITER` | 9 |

### 存储

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `STORE_BUSY` | 10 秒内拿不到写锁 | 稍后重试 | 2 |
| `STORE_NEWER` | 日志中有比本二进制新的格式版本，记忆库只读 | 升级 almanac | 2 |

### 清除与只限本地的操作

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `LOCAL_ONLY` | 远程模式下运行 `purge`、`token`、`serve` | 到 server 那台机器上运行 | 4、8 |
| `NOT_INTERACTIVE` | `purge` 的 stdin 不是终端 | 清除须由人在终端里运行 | 4 |
| `ALREADY_PURGED` | 清除已被清除的对象 | — | 4 |
| `TOKEN_EXISTS` | `token create` 的设备名已存在 | 换名字，或先 `token revoke` | 4 |
| `TOKEN_NOT_FOUND` | `token revoke` 的设备名不存在 | — | 4 |

### 远程

| code | 条件 | hint | 来源 |
|---|---|---|---|
| `REMOTE_UNREACHABLE` | 连接失败、超时、HTTP 5xx 或响应不是合法 JSON（已重试） | 检查网络与 `almanac remote` 的地址 | 8 |
| `UNAUTHORIZED` | 缺少 token、token 无效或已吊销 | 在 server 上 `almanac token create`，再 `almanac remote set` | 8 |
| `VERSION_MISMATCH` | server 不支持请求的协议主版本 | 让两端的 almanac 版本一致 | 8 |
| `ALREADY_APPLIED` | 重试的事务已写入，但 server 已没有它的原响应 | 用 `get` 或 `search` 查看结果 | 8 |

# 8 Server 协议

同一个核心既能在本机内嵌运行（本地模式），也能作为 server 供多台[设备（Device）](../../GLOSSARY.md)连接（远程模式）。本章定义两者的分层，以及本机前端与 server 之间的 HTTP 协议。

## 分层

远程模式下，宿主仍然只和本机的 `almanac` 二进制通信（CLI，或 `almanac mcp` 的 stdio）。server 不暴露 MCP。

| 层 | 负责 | 运行在 |
|---|---|---|
| **本地前端**（CLI / MCP） | 由 `project` 路径解析项目标识与根提交；推断 Writer、git commit（第 9 章）；生成全部 id；`FILE_OUTSIDE_PROJECT` 等只看参数本身的检查；写入时比对文件摘录（`FILE_NOT_FOUND`、`EXCERPT_NOT_IN_FILE`，不过则不转发）；核验与核验缓存（第 6 章）；计算可能过期标记 | 每台设备 |
| **存储核心** | 依赖状态的写入检查（`NOT_ACTIVE`、`DUPLICATE_TEXT`、`EVIDENCE_NOT_FOUND` 等）；追加日志；索引与检索；状态推导；`context` 的挑选与排版 | 本地模式在本机；远程模式在 server |

前端只依赖存储核心的一个接口；它有两个实现：直接读写本机记忆库目录的本地实现，和走下文 HTTP 协议的远程实现。两种模式下，工具与命令的行为和输出相同。

### 前端与核心之间传什么

- **项目**：解析好的项目标识与根提交，不传本机路径。
- **证据**：前端已补好 `file` 来源的 `project`、`conversation` 来源的 `host` 与 `session`。`web` 来源缺省的 `fetched_at` 由核心填为事务时间。
- **id**：证据、记忆、事务的 id 全部由前端生成（第 1 章）。核心在证据重复时返回已有 id，不用前端给的。
- **可能过期**：核心不读核验缓存。需要按可能过期过滤或计数时（`search` 的 `stale_only`、`review`、`context`），前端随请求附上本设备缓存中结果为 `changed` 或 `unreachable` 的证据 id；核心据此按第 6 章的规则推导。
- **核验所需的数据**：核心在响应中附上前端核验所需的证据来源与摘录，以及每条记忆最近一批证据的 id（见下文 `_check`）；前端核验、计算 `stale` 后，删去这些内部字段再交给智能体。

## HTTP 协议

### 请求

```
POST /v1/<op>
Authorization: Bearer <token>
Content-Type: application/json
```

`<op>`：

| op | 对应 |
|---|---|
| `search`、`get`、`review`、`record`、`corroborate`、`retract`、`distinguish` | 第 3 章同名工具 |
| `verify_targets` | `verify` 的取数步骤：按 `ids` 或作用域返回所选生效记忆及其当前引用的全部证据（核验本身在前端，见下文） |
| `context` | `almanac context`（第 4 章） |
| `project_alias` | `almanac project alias`（第 4 章） |

`purge`、`token` 没有对应的 op：它们只限本地，前端在远程模式下直接报 `LOCAL_ONLY`，不发请求。

请求体：

| 字段 | 说明 |
|---|---|
| `args` | 工具参数（第 3 章），去掉 `project` 路径，补上前端生成的 id 与来源字段：`record` 的每条 `evidence[]`、`memories[]` 带 `id`，`corroborate` 的每条内联证据带 `id` |
| `project` | `{ id, root_commit? }`：解析好的当前项目；不在 git 内时没有 `root_commit` |
| `tx` | 事务 id，写操作（`record`、`corroborate`、`retract`、`distinguish`、`project_alias`）必带 |
| `writer` | `{ name, model?, session? }`，写操作必带（第 9 章）。`project_alias` 为 `{"name": "human"}` |
| `git_commit` | 写操作可带：当前项目的 HEAD commit |
| `stale_evidence` | 证据 id 数组，`search`（`stale_only` 时）、`review`、`context` 可带，含义见上文 |
| `max_chars` | 仅 `context` |

### 响应

- 成功：HTTP 200，响应体与第 3 章的工具响应相同（`context` 为 `{ "text": "…" }`），外加内部字段 `_check`（`search`、`get`、`verify_targets`）：

  ```
  _check: {
    memories: { <memory id>: { latest: [evidence id] } },
    evidence: { <evidence id>: { source, excerpt } }
  }
  ```

  `latest` 为该记忆最近一批证据（第 6 章「判定」）；`evidence` 覆盖这些记忆当前引用的全部证据，被清除的除外。前端用完即删。
- 失败：响应体为 `{ "error": { code, message, hint? } }`（第 10 章）。HTTP 状态码：认证失败 401；未知 op 或不支持的协议主版本 404；其余错误 422。

### 版本

路径中的 `v1` 是协议主版本。server 不支持请求的主版本时返回 `VERSION_MISMATCH`。同一主版本内只做加法：新增 op、新增可选字段；双方都忽略不认识的字段。

### 事务时间、Writer 与设备

- 事务时间 `at` 由 server 打，保证随日志顺序单调。
- `writer`、`git_commit` 由前端推断后上报，server 不改：一人一库，客户端可信。
- server 把令牌的设备名写进事务的 `device` 字段（第 2 章）。

### 幂等与重试

事务 id 同时是幂等键。

- 写请求的 `tx` 已出现在日志里：不再执行。server 在 `index/` 中保存最近 24 小时内事务的响应，命中时原样返回；超出 24 小时或 `index/` 已重建时，返回 `ALREADY_APPLIED`，hint 提示用 `get` 或 `search` 查看结果。
- 失败的写请求不写日志，重试会重新执行。
- 前端对网络失败与超时最多重试 2 次，用同一个 `tx`；仍失败报 `REMOTE_UNREACHABLE`。

### 认证

- 每台设备一个 bearer token，由 server 那台机器上的 `almanac token create <设备名>` 生成（第 4 章）。token 由 32 字节随机数编码而成，前缀 `alm_`。
- server 只在 `auth/` 中保存 token 的 SHA-256 哈希与设备名（第 2 章）。
- 缺少 token、token 无效或已吊销：401，`UNAUTHORIZED`。吊销立即生效。

### 不做

不做流式、推送、读缓存、离线写队列；server 不可达时不回退到本地模式。不内置 TLS，不做 OAuth 或 mTLS。

## 核验在前端

server 没有核验代码路径，也没有 `cache/`。

- `search`、`get`：server 返回结果与 `_check`，前端对其中的 `file` 证据做惰性核验（第 6 章），写进本设备缓存，再按 `latest` 与缓存计算每条记忆的 `stale`。
- `verify`：前端先请求 `verify_targets` 取得所选记忆与证据，自己核验（含网页 GET），写缓存，再组装第 3 章的 `verify` 响应。
- 同一来源在不同设备上可能得到不同结果，这符合「核验结果属于设备」。

## 远程模式的本机配置

- 在 `config.toml` 写 `remote` 与 `token`，或用环境变量 `ALMANAC_REMOTE`、`ALMANAC_TOKEN` 覆盖（第 4 章 `almanac remote`）。
- 配置了 remote 的设备没有本地记忆库，忽略 `ALMANAC_HOME`。
- 核验缓存放在 `$XDG_CACHE_HOME/almanac/`（未设时为 `~/.cache/almanac/`），可删。
- 请求超时 30 秒。连接失败、超时、HTTP 5xx 或响应不是合法 JSON：报 `REMOTE_UNREACHABLE`。

## server 端

- `almanac serve [--listen <地址:端口>]`，默认 `127.0.0.1:7470`。绑定到非回环地址时启动打警告；跨设备连接靠 Tailscale、WireGuard 或反向代理。
- server 使用本机记忆库（`ALMANAC_HOME`），写入与同机的 CLI / MCP 进程一样走 `flock`（第 2 章）；读取前同样补齐索引。
- 运行 server 的那台机器上，本地 CLI / MCP 直接用本地模式，不经 HTTP。

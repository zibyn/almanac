# 4 仅 CLI 命令

本章列出只在 CLI 提供、不作为智能体工具暴露的命令。8 个工具的 CLI 形式（`almanac <tool> --json …`）见第 3 章。

| 命令 | 作用 | 远程模式 |
|---|---|---|
| [`almanac mcp`](#almanac-mcp) | 启动 MCP stdio server | 可用 |
| [`almanac context`](#almanac-context) | 会话开始时注入的记忆 | 可用 |
| [`almanac purge`](#almanac-purge) | 清除一条证据或记忆 | `LOCAL_ONLY` |
| [`almanac project alias`](#almanac-project-alias) | 声明项目别名 | 可用 |
| [`almanac remote`](#almanac-remote) | 设置或取消 server 地址与 token | 不涉及 |
| [`almanac token`](#almanac-token) | 管理设备令牌 | `LOCAL_ONLY` |
| [`almanac serve`](#almanac-serve) | 运行 server | 不适用 |

- 「远程模式」指本设备配置了 remote（第 8 章）。标 `LOCAL_ONLY` 的命令在远程模式下拒绝执行，只能在 server 那台机器上以本地模式运行。
- 除 `context` 外，成功退出码 0；失败退出码 1，stderr 输出一行 `<错误码>：<说明>`。

## almanac mcp

```
almanac mcp
```

在 stdio 上运行 MCP server，暴露第 3 章的 8 个工具。宿主的手抄配置见第 9 章。

## almanac context

```
almanac context [--max-chars <N>]
```

输出会话开始时注入给智能体的记忆，由宿主的 `SessionStart` hook 或 Pi 扩展调用（第 9 章）。项目取进程的当前工作目录。远程模式下由 server 计算（`POST /v1/context`，第 8 章），本机只转发，两种模式输出一致。

### 内容

- 当前项目（含别名组）与全局的**生效**记忆。被取代、撤回、无据的不输出。
- 只有数据和标题，不附用法说明（用法由 SKILL.md 承载，第 9 章）。
- 输出纯 Markdown，没有 JSON 模式。

### 挑选与截断

- 两节，项目在前、全局在后。不在任何项目内（项目标识以 `path:` 开头）时也照常输出项目节。
- 预算按 Unicode 字符计，默认 6000，`--max-chars` 覆盖；预算包含标题、节标题和末尾提示行。两节各占一半，一节没用完的份额让给另一节。
- 节内按最近一次记录或附证的时间倒序。带可能过期标记的照常排序，只加标记。
- 只按整条截断，绝不截断正文。一节放不下时，节尾写「另有 N 条未列出，用 search 查」。

### 格式

```
# almanac 记忆（项目 github.com/zibyn/almanac + 全局）

## 项目
- 本项目的构建命令是 cargo build --features cjk [mem_01JAX… · 亲见 · 2026-10-20 · 可能过期]
- …
另有 3 条未列出，用 search 查

## 全局
- 用户要求 Rust 代码使用 4 空格缩进 [mem_01JAW… · 用户陈述 · 2026-10-18]

待复核：相似对 2 · 可能过期 1。运行 almanac review 查看。
```

- 单条：`- <正文> [<id> · <认知状态> · <YYYY-MM-DD>]`。认知状态写中文（亲见 / 用户陈述 / 推断）；日期为最近一次记录或附证的 UTC 日期；带可能过期标记时括号内末尾加「· 可能过期」。不列证据。
- 一节没有记忆时省略该节。
- 待复核行：四类问题（多后继 / 相似对 / 可能过期 / 无据）的计数，范围同 `review` 的默认范围（第 3 章），计数为 0 的类别不写，全为 0 时整行不输出。
- 当前项目与另一个项目根提交相同、尚未声明别名时（第 1 章），在待复核行之后加一行：`项目 <当前> 与 <另一个> 的根提交相同；如是同一项目，运行 almanac project alias <另一个> <当前>`。

### 只读、永不阻塞

- 不核验、不读来源文件、不联网、不写核验缓存；可能过期只读本设备缓存。
- 任何失败（记忆库不存在、server 不可达、索引损坏、版本不匹配……）都**退出码 0**，stdout 只输出一行 `almanac 不可用：<原因>`。
- 记忆库为空、或范围内没有生效记忆时，无输出。

## almanac purge

```
almanac purge <id> --reason <原因>
```

[清除（Purge）](../../GLOSSARY.md)一条证据或一条记忆：物理抹去原文，只留墓碑。由人发起，是「日志只追加」的唯一例外（[ADR 0001](../adr/0001-purge-rewrites-the-log.md)）。墓碑格式见第 1 章，重写日志的步骤见第 2 章。

- `--reason` 必填，写进墓碑。
- 交互式执行：stdin 不是终端时拒绝，报 `NOT_INTERACTIVE`。这保证清除由人在终端里发起，智能体不能代劳。
- 远程模式下报 `LOCAL_ONLY`。

### 流程

1. 显示要清除的对象（证据的摘录与来源，或记忆的正文）。
2. 对象是证据时，列出当前引用它的所有记忆，包括这些记忆取代链上的历史版本；逐条询问是否一并清除。
3. 汇总将被清除的全部对象，要求输入 `yes` 确认；否则不做任何改动。
4. 执行第 2 章的清除步骤。所有对象在同一次重写中清除。
5. 提醒：外部备份里的副本需要人自己处理。

被清除过的对象再次清除，报 `ALREADY_PURGED`。

> **待定**（[被清除记忆的状态与墓碑保留字段](https://github.com/zibyn/almanac/issues/23)）：墓碑 `by` 字段取什么值；清除记忆后它与取代链的关系。

## almanac project alias

```
almanac project alias <项目标识 A> <项目标识 B>
```

声明两个项目标识指同一个项目，写入一条 `project_aliased` 事件，Writer 记为 `{"name": "human"}`（第 1 章）。用于仓库迁移、改名、历史改写之后。

- 参数是项目标识（如 `github.com/zibyn/almanac`、`git-root:5d0882…`），不是路径；两个必须不同且非空，否则报 `INVALID_ARGS`。
- 两者已在同一别名组内时不写事件，输出提示后成功退出。
- 不可撤销。
- 远程模式下可用。

## almanac remote

```
almanac remote set <url>      # token 从 stdin 读入
almanac remote unset
```

配置本设备连接的 server（第 8 章）。

- 写入 `$XDG_CONFIG_HOME/almanac/config.toml`（未设 `XDG_CONFIG_HOME` 时为 `~/.config/almanac/config.toml`），文件权限 0600：

  ```toml
  remote = "https://almanac.example.ts.net:…"
  token = "…"
  ```

- `set` 从 stdin 读一行作为 token（不进 shell 历史）；`url` 必须是 `http` 或 `https`，token 不得为空。`set` 不检查 server 是否可达。
- `unset` 删除这两个键，本设备回到本地模式。
- 环境变量 `ALMANAC_REMOTE`、`ALMANAC_TOKEN` 优先于配置文件。
- 宿主的插件和配置里从不出现地址或 token：本地模式与远程模式下，宿主侧配置完全相同（第 9 章）。

## almanac token

```
almanac token create <设备名>
almanac token revoke <设备名>
```

管理连接 server 的[设备（Device）](../../GLOSSARY.md)令牌，每台设备一个。只能在 server 那台机器上以本地模式运行，远程模式下报 `LOCAL_ONLY`。

- `create`：生成一个 bearer token，在 stdout 输出一次，之后无法再取回；`auth/` 中只存哈希（第 2 章）。设备名已存在时报 `TOKEN_EXISTS`。设备名会作为事务的 `device` 字段写进日志。
- `revoke`：吊销该设备的令牌，之后用它的请求被拒绝。已写入日志的 `device` 字段不变。设备名不存在时报 `TOKEN_NOT_FOUND`。

## almanac serve

```
almanac serve [--listen <地址:端口>]
```

以本机的记忆库运行 server，供其他设备连接。协议、认证、默认端口见第 8 章。

- 默认只绑定 `127.0.0.1`；绑定到非回环地址时启动打警告。不内置 TLS，跨设备连接靠 Tailscale、WireGuard 或反向代理。
- 与同机的 CLI / MCP 进程通过 `flock` 互斥写入（第 2 章）。
- 本设备配置了 remote 时拒绝启动，报 `LOCAL_ONLY`。

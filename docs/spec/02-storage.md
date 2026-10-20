# 2 存储与日志格式

一个[记忆库（Store）](../../GLOSSARY.md)就是本地磁盘上的一个目录。事件日志是唯一事实来源；没有持久化索引，缓存可删，删后重新核验（[ADR 0002](../adr/0002-event-log-is-the-source-of-truth.md)）。

## 目录布局

记忆库目录默认为 `$XDG_DATA_HOME/almanac/`（未设 `XDG_DATA_HOME` 时为 `~/.local/share/almanac/`），可用环境变量 `ALMANAC_HOME` 覆盖。

```
almanac/
├── log/
│   └── events.jsonl   唯一事实来源；备份或迁移只需拷 log/
├── cache/             本机的核验结果，可删
└── lock               写锁文件
```

- **没有持久化索引**：每次调用（每个 CLI 进程、每次 MCP 工具调用）从头重放日志，在内存中建出派生状态（记忆的生命周期状态、`superseded_by`、`cited_by`、证据指纹、最近一次记录或附证时间等）与全文索引（第 5 章）。
- 预期规模是几百到几千条记忆，此时重放加分词约百毫秒量级，大头是 jieba 词典加载。超过约 1 万条、重放明显变慢时，再以加法方式补回持久化索引或快照：它只是可删的缓存，日志格式不变。
- `cache/` 存放核验结果，格式见第 6 章。核验结果不进日志。

### 存放位置的约束

- 记忆库目录**必须在本地磁盘**。不支持 NFS、SMB 和同步盘（Dropbox、iCloud Drive、OneDrive 等）：`flock` 在其上不可靠，同步盘还会产生冲突副本。启动时检测到常见同步盘路径就在 stderr 打警告。v0.1 不支持多台机器共享一个记忆库。

## 日志格式

`log/events.jsonl` 是单个 JSONL 文件，**一行＝一个事务**。

### 事务行

| 字段 | 类型 | 说明 |
|---|---|---|
| `v` | int | 日志格式版本，v0.1 为 `1` |
| `tx` | string | 事务 id，`tx_<ULID>` |
| `at` | string | 事务时间（UTC，RFC 3339，毫秒），由本机在持锁时打 |
| `writer` | object | [写入方（Writer）](../../GLOSSARY.md)：`name`（必有）、`model?`、`session?`。推断方式见第 9 章；由人执行的 `project alias` 记为 `{"name": "human"}` |
| `git_commit` | string? | 写入时项目的 HEAD commit（项目在 git 内时） |
| `events` | Event[] | 有序事件数组，至少一个，种类见第 1 章 |

示例（为便于阅读折行，实际为一行）：

```json
{"v":1,"tx":"tx_01JAX3Q8M6Z5K0V9W2R4T7Y1BC","at":"2026-10-20T08:00:00.000Z",
 "writer":{"name":"claude-code","session":"3f1c…"},"git_commit":"94cb577…",
 "events":[
  {"type":"evidence_recorded","id":"ev_01JAX3Q8M6…","excerpt":"cargo build --features cjk",
   "source":{"type":"file","project":"github.com/zibyn/almanac","path":"README.md","line":12}},
  {"type":"memory_recorded","id":"mem_01JAX3Q8M7…","text":"本项目的构建命令是 cargo build --features cjk",
   "scope":"project","project":"github.com/zibyn/almanac","root_commit":"5d0882…",
   "epistemic":"observed","evidence":["ev_01JAX3Q8M6…"],"supersedes":[]}
 ]}
```

一次工具调用（如一次 `record`）的全部事件放在同一个事务里，原子生效：要么整行写入，要么都不写。

### 追加与崩溃恢复

- 一个事务用一次 `write()` 写入，以 `\n` 结尾；`fsync` 之后工具调用才返回成功。
- 写入方持锁打开日志时，若最后一行没有以 `\n` 结尾或不是合法 JSON，视为崩溃造成的残缺写入：截掉这一行，并记进诊断日志（stderr）。

### 版本

- 新版二进制能读所有旧版本的行。
- 旧版二进制遇到 `v` 比自己支持的新的行时，整个记忆库只读，写入报 `STORE_NEWER`。
- **不做日志迁移**。格式演进只能新增事件类型或可选字段；读端忽略不认识的字段。

### 不做快照、不做压缩

每次调用都从头重放日志。以后重放变慢，就加可删的快照（见「目录布局」）；日志本身永远不做快照或压缩。除清除外，任何操作都不得重写日志（[ADR 0001](../adr/0001-purge-rewrites-the-log.md)）。

## 本地并发

同一台机器上可以有多个进程同时访问一个记忆库（多个智能体的 CLI / MCP 进程）。

### 写入

1. 对 `lock` 加排他 `flock`，最多等 10 秒；超时返回 `STORE_BUSY`。
2. **重新打开日志**。清除会用 rename 替换日志文件，沿用旧文件描述符会写进已无路径指向的旧文件。
3. 重放日志，得到当前状态。
4. 检查写入前提（如 `NOT_ACTIVE`，完整列表见第 3 章）。
5. 追加事务行并 `fsync`。
6. 释放锁。

进程死掉时 `flock` 由内核释放。

### 读取

读不加锁，也不等锁：打开日志，重放到最后一个完整的行。最后一行没有以 `\n` 结尾时，视为另一个进程正在写入，忽略这一行；只有持锁的写入方才会截掉残缺行。

## 清除步骤

[清除（Purge）](../../GLOSSARY.md)落实 [ADR 0001](../adr/0001-purge-rewrites-the-log.md)，命令见第 4 章，墓碑格式见第 1 章。

1. 对 `lock` 加排他 `flock`。
2. 读出 `events.jsonl`，按第 1 章「清除与墓碑」替换与抹去字段（证据的 `excerpt` 与定位字段，记忆的 `text` 与撤回原因），写出 `log/events.jsonl.tmp`。
3. `fsync` 临时文件。
4. rename 覆盖 `events.jsonl`。
5. `fsync` `log/` 目录。
6. 删除 `cache/`。
7. 释放锁。

- 不做自动备份。CLI 在完成后提醒：外部备份里的副本需要人自己处理。
- 事务行的其他字段与事件顺序保持不变，id 不变。

# almanac

Memories change. Evidence remains.

给智能体用的外部长期记忆库，一人一库。每条记忆都必须引用支撑它的证据：记忆会变（被取代或撤回），证据永存（除非人明确清除）。单个 Rust 二进制，提供 CLI 与 MCP stdio，通过原生插件接入 Claude Code、Codex、DeepSeek Harness、Pi。

- 记忆与证据分开存放：更新即取代，遗忘即撤回，都只追加。
- 证据带完整来源（对话、文件、网页、命令输出），文件与网页证据可回查核验。
- 中文分词检索、结构化过滤，无需向量库或外部数据库。
- 数据在本机：默认 `~/.local/share/almanac/`。

规格与设计细节见 [`docs/spec/`](docs/spec/README.md)，术语见 [`GLOSSARY.md`](GLOSSARY.md)。

## 安装

### 1. 装二进制

CLI、MCP、Claude Code、Codex、Pi 都需要 `almanac` 在 `PATH` 上。从 [GitHub Releases](https://github.com/zibyn/almanac/releases) 下载，或：

```bash
cargo install --git https://github.com/zibyn/almanac
```

DSH 插件不需要手动准备二进制：它依次从 `ALMANAC_BIN`、`PATH`、`~/.cache/almanac/bin` 解析，必要时从 GitHub Release 下载并校验 SHA-256。

### 2. 装插件（推荐）

插件提供 SKILL.md、会话开始时注入记忆、每隔若干轮提醒写入。用各宿主自己的命令安装：

| 宿主 | 命令 |
|---|---|
| Claude Code | `claude plugin install almanac --marketplace zibyn/almanac` |
| Codex | `codex plugin marketplace add zibyn/almanac`，然后 `codex plugin add almanac@almanac` |
| DeepSeek Harness | `dsh plugin --profile web add <Release 里的 zibyn-dsh-almanac-<版本>.tgz 路径或 URL>`（需要 `pnpm`） |
| Pi | `pi install git:github.com/zibyn/almanac` |

Codex 用户装完后需在 `/hooks` 信任 almanac 的 `SessionStart` 与 `Stop` hook；每次升级插件后要重新信任，否则注入与提醒不会出现。

### 可选：MCP

插件不含 MCP。需要 MCP 的话，按 [`docs/spec/09-integration.md`](docs/spec/09-integration.md#mcp-手抄配置) 把配置抄进宿主配置文件。

## 使用

### 让智能体记住

直接对智能体说「记住 X」。智能体会把这句话作为证据，写成一条可检索的记忆；下次会话开始时自动注入。

常用说法：

- 「记住：这个项目用 4 空格缩进」
- 「你记得我的构建习惯吗？」——智能体会用 `search` 检索
- 「这条记错了，忘掉它」——智能体会 `retract`

### 在终端里查询

```bash
almanac context                 # 本次会话会注入的记忆
almanac export > almanac.md     # 导出生效与无据的记忆（只读、不能导回）
almanac export --all            # 另含被取代、撤回、已清除的
almanac --help                  # 命令列表
almanac <工具> --help           # 某个工具的参数
```

检索与查看：

```bash
almanac search --json '{"query":"缩进","limit":5}'
almanac get --json '{"id":"mem_…"}'
almanac review --json '{}'      # 待处置：可能过期、无据
```

### 在终端里写入

设置 `ALMANAC_WRITER=human`，用与智能体相同的工具。参数一律用 heredoc 从 stdin 传入：

```bash
ALMANAC_WRITER=human almanac record --json - <<'EOF'
{
  "evidence": [{ "ref": "e", "excerpt": "Rust 代码一律 4 空格缩进", "source": { "type": "conversation" } }],
  "memories": [{ "text": "Rust 代码使用 4 空格缩进", "scope": "global", "evidence": ["e"] }]
}
EOF
```

撤回一条记忆（id 从 `export` 或 `search` 里找）：

```bash
ALMANAC_WRITER=human almanac retract --json - <<'EOF'
{"memory":"mem_…","reason":"记错了"}
EOF
```

从磁盘彻底抹去敏感内容（如误记的密钥）：

```bash
almanac purge <id> --reason <原因>
```

## 命令

智能体工具（CLI 与 MCP 同名同参）：

| 工具 | 作用 |
|---|---|
| `record` | 写入证据和/或记忆（更新 = 取代旧记忆） |
| `search` | 检索记忆或证据 |
| `get` | 按 id 取全文、证据与取代链邻居 |
| `review` | 列出待处置的问题 |
| `corroborate` | 为记忆追加支撑证据 |
| `retract` | 撤回记忆 |
| `verify` | 回到来源核验摘录（网页证据会联网） |

仅 CLI：`context`、`remind`、`export`、`purge`、`project alias <A> <B>`、`mcp`。

错误以 JSON 输出，退出码 1；错误码与含义见 [`docs/spec/10-errors.md`](docs/spec/10-errors.md)。

## 配置

| 环境变量 | 作用 |
|---|---|
| `ALMANAC_HOME` | 记忆库目录，默认 `$XDG_DATA_HOME/almanac/` 或 `~/.local/share/almanac/` |
| `ALMANAC_WRITER` | 写入方身份；人在终端写入时设为 `human` |
| `ALMANAC_BIN` | DSH 插件优先使用的二进制路径 |

写入提醒的间隔在记忆库目录的 `config.toml` 中设置（默认每 15 轮），设为 0 可关闭：

```toml
remind_every = 0
```

记忆库是本地目录，备份即拷贝该目录；`export` 产物仅供阅读，不能导回。

## 开发

```bash
cargo test
cargo clippy --all-targets
```

支持的平台、发布流程见 `.github/workflows/`。

# almanac
Memories change. Evidence remains.

为多种智能体（Claude Code、Codex、Pi）提供外部长期记忆：每条记忆都引用支撑它的证据，记忆会变，证据永存。规格见 [`docs/spec/`](docs/spec/README.md)。

## 安装

### 1. 二进制

`almanac` 必须在 `PATH` 上。从 [GitHub Release](https://github.com/zibyn/almanac/releases) 下载，或：

```bash
cargo install --git https://github.com/zibyn/almanac
```

插件不会自动更新二进制。

### 2. 插件

插件带来三样东西：教智能体使用 almanac 的 SKILL.md、会话开始时注入记忆（`almanac context`）、每隔若干轮提醒智能体回顾并写入（`almanac remind`）。用各宿主自己的命令安装、更新、卸载：

| 宿主 | 命令 |
|---|---|
| Claude Code | `claude plugin install almanac --marketplace zibyn/almanac` |
| Codex | `codex plugin marketplace add zibyn/almanac`，然后 `codex plugin add almanac@almanac` |
| Pi | `pi install git:github.com/zibyn/almanac` |

**Codex 用户还要信任两个 hook。** 装完后在 Codex 里运行 `/hooks`，信任 almanac 的 `SessionStart` 与 `Stop` hook。Codex 的信任绑定整个插件目录的内容哈希，所以**每次升级插件后 Codex 都会要求重新信任**：到 `/hooks` 再信任一次，否则注入与提醒不再出现。没有信任时，只是少了会话开始注入或写入提醒，智能体照样能用 almanac。

### 关闭写入提醒

在记忆库目录（默认 `~/.local/share/almanac/`，可用 `ALMANAC_HOME` 改）下的 `config.toml` 写：

```toml
remind_every = 0
```

同一个键也能改提醒间隔（默认每 15 轮一次）。

### MCP（可选）

插件不含 MCP。想用 MCP 的，按 [第 9 章「MCP 手抄配置」](docs/spec/09-integration.md#mcp-手抄配置) 把配置抄进宿主的配置文件。

## 人怎么看、怎么改

**看**：把全部记忆导出成 Markdown，只读、不能导回：

```bash
almanac export > almanac.md          # 生效与无据的记忆
almanac export --all > almanac.md    # 另含被取代、撤回、已清除的
```

**记东西**：对智能体说「记住 X」。

**直接在终端里改**：设 `ALMANAC_WRITER=human`，调用与智能体相同的工具。

记一条：

```bash
ALMANAC_WRITER=human almanac record --json - <<'EOF'
{
  "evidence": [{ "ref": "e", "excerpt": "Rust 代码一律 4 空格缩进", "source": { "type": "conversation" } }],
  "memories": [{ "text": "Rust 代码使用 4 空格缩进", "scope": "global", "evidence": ["e"] }]
}
EOF
```

撤回一条（id 从 `almanac export` 或 `almanac search` 里找）：

```bash
ALMANAC_WRITER=human almanac retract --json '{"memory":"mem_…","reason":"…"}'
```

要从磁盘上彻底抹去敏感内容（如误记的密钥），在终端运行 `almanac purge <id> --reason <原因>`，它会逐项确认。

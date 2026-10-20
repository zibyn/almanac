# 9 接入

almanac 以宿主原生插件接入 Claude Code（CC）、Codex、Pi。默认的接入面是 **CLI + SKILL.md**：上下文开销小，三家都能用 shell 工具调用。会话开始注入是默认组件；MCP stdio 是可选增强，只给手抄配置。

## 二进制

`almanac` 必须在 `PATH` 上：从 GitHub Release 下载，或 `cargo install`。不做自动更新，也不做插件与二进制的版本协商。版本不匹配时，由 `STORE_NEWER`（第 2 章）与 `context` 的「almanac 不可用：…」兜底。

## 插件

almanac 不读写用户的宿主配置文件：安装、更新、卸载都用各宿主自己的命令，因此没有 `almanac install`。

### 仓库布局

```
.claude-plugin/marketplace.json      CC 与 Codex 共用；条目带 Codex 要求的 policy / category
plugins/almanac/
  .claude-plugin/plugin.json         CC 与 Codex 共用的 manifest
  skills/almanac/SKILL.md            全仓唯一一份
  hooks/hooks.json                   会话开始注入（CC 与 Codex）
  pi/almanac.ts                      Pi 扩展
package.json                         只为 Pi：仅有 "pi" 键
```

- 不用符号链接：Codex 安装时会丢弃符号链接。
- 「CC 与 Codex 共用一个插件目录」尚未端到端实测，实现时验证。不行时的回退方案：另建 `.codex-plugin` 目录并复制一份 SKILL.md，加 CI 检查两份内容一致。
- 插件里只有 SKILL.md 与会话开始注入，不含 MCP 配置。

### 安装

| 宿主 | 命令 |
|---|---|
| CC | `claude plugin install almanac --marketplace zibyn/almanac` |
| Codex | `codex plugin marketplace add zibyn/almanac`，然后 `codex plugin add almanac@almanac` |
| Pi | `pi install git:github.com/zibyn/almanac` |

- Codex：装完后要到 `/hooks` 信任 almanac 的 `SessionStart` hook，安装说明里写明，不做绕过。没有信任时，只是少了会话开始注入。`hooks.json` 尽量保持不变，因为每次改动都要用户重新信任。
- Pi 的 git 来源不能指定子目录，一个仓库只能是一个包，所以用根目录的 `package.json`：

  ```json
  { "pi": { "skills": ["./plugins/almanac/skills"], "extensions": ["./plugins/almanac/pi/almanac.ts"] } }
  ```

## 会话开始注入

三段配置调用同一条 `almanac context`（第 4 章）：只读、永不阻塞，项目取进程的当前工作目录。这段注入不写入，所以不需要 Writer。

### CC 与 Codex：`hooks/hooks.json`

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|clear|compact",
        "hooks": [{ "type": "command", "command": "almanac context" }]
      }
    ]
  }
}
```

- `resume` 不注入：历史里已有。`compact` 后重新注入。
- stdout 纯文本即注入内容；忽略 hook 的 stdin JSON。

### Pi：`pi/almanac.ts`

Pi 没有命令 hook，用扩展在会话开始（含 `/new` 和压缩之后）的下一次运行前注入一次：

```ts
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  let pending = false;

  pi.on("session_start", async (event) => {
    if (event.reason === "startup" || event.reason === "new") pending = true;
  });
  pi.on("session_compact", async () => {
    pending = true;
  });

  pi.on("before_agent_start", async (_event, ctx) => {
    if (!pending) return;
    pending = false;
    const r = await pi.exec("almanac", ["context"], { cwd: ctx.cwd, timeout: 5000 });
    const text = r.stdout.trim();
    if (!text) return;
    return { message: { customType: "almanac-context", content: text, display: false } };
  });
}
```

`resume`、`fork`、`reload` 不注入，与 CC / Codex 的 matcher 对应。

## SKILL.md

全仓唯一一份，路径 `plugins/almanac/skills/almanac/SKILL.md`，遵循 Agent Skills 标准。它是智能体使用 almanac 的唯一说明来源：`context` 的输出与工具描述都不重复用法。

SKILL.md 必须讲清：

1. **怎么调用**：`almanac <tool> --json '<参数>'`，长参数用 `--json -` 加 heredoc；工具清单与要点（第 3 章）。
2. **何时检索**：动手前、用户提到过去的约定或偏好时，先 `search`；没命中就看 `matched_terms` 换词再搜；需要证据或历史时用 `get` 沿 id 追溯。
3. **何时写入**：倾向不写，只记以后还会用到、有证据支撑的论断；一条记忆只写一个论断。
4. **怎么写对**：证据是一小段原文摘录；用户说「记住 X」时这句话本身就是证据；对话证据只收用户说的话，用户确认智能体的提议时，摘录先写一行 `> 智能体：…` 再写用户原话；认知状态可省略（证据全是对话时默认用户陈述，否则默认推断），自己观察到的显式写 `observed`；项目外的文件按 `command` 记录。
5. **怎么更新与遗忘**：情况变了用 `record` 加 `supersedes`；本就错了用 `retract` 并写原因；`DUPLICATE_TEXT` 时改用 `corroborate`；收到 `NOT_ACTIVE` 时沿取代链找最新的后继。
6. **可能过期**：看到 `stale` 时重读来源，再附证、取代或撤回；`verify` 会对网页发请求。
7. **整理**：用户要求整理记忆时调用 `review`，逐项处置；`record` 返回 `similar` 时先看一眼，是更新就改用 `supersedes`。
8. **不做的事**：不能物理删除；需要抹除敏感内容时，建议用户在终端运行 `almanac purge`。
9. **`NO_WRITER`**：只在 hint 说明宿主嵌套时，在命令前加 `ALMANAC_WRITER=<自己的宿主名>` 重试；平时不设置它。

## MCP 手抄配置

MCP 不进插件。想用 MCP 的人手抄以下配置；每段都设 `ALMANAC_WRITER`，因为 Codex 会清空 MCP 子进程的环境。

**CC**（`~/.claude.json` 或项目的 `.mcp.json`）：

```json
{ "mcpServers": { "almanac": { "command": "almanac", "args": ["mcp"], "env": { "ALMANAC_WRITER": "claude-code" } } } }
```

**Codex**（`~/.codex/config.toml`）：

```toml
[mcp_servers.almanac]
command = "almanac"
args = ["mcp"]
env = { ALMANAC_WRITER = "codex" }
```

**Pi**（`~/.pi/agent/mcp.json`）：`exposure` 必须为 `direct`，否则默认的 codemode 下模型看不到工具。

```json
{ "mcpServers": { "almanac": { "command": "almanac", "args": ["mcp"], "env": { "ALMANAC_WRITER": "pi" }, "exposure": "direct" } } }
```

- CC 可以在配置里加 `"alwaysLoad": true`，让工具常驻、不经 tool search 延迟加载。
- 项目路径：MCP 下依次取工具参数 `project` → `roots/list` → `CLAUDE_PROJECT_DIR` → 进程 cwd（第 1 章）。Codex 不支持 roots，靠 cwd。

## Writer 推断

每次写入都必须有 [写入方（Writer）](../../GLOSSARY.md)：`{ name, model?, session? }`，由 almanac 推断，不是工具参数。推断不出就拒绝写入，报 `NO_WRITER`，不记 unknown。读操作不受影响。

### CLI

按序取第一个成立的：

| 序 | 条件 | `name` |
|---|---|---|
| 1 | `ALMANAC_WRITER` 非空 | 它的值 |
| 2 | 下列会话变量**恰好命中一家**：`CLAUDE_CODE_SESSION_ID` → `claude-code`；`CODEX_THREAD_ID` 或 `CODEX_SESSION_ID` → `codex`；`PI_SESSION_ID` → `pi` | 对应宿主 |
| 3 | 第 2 级一家都没命中，且 `PI_CODING_AGENT=true` | `pi` |
| 4 | 以上都不成立，或第 2 级命中多家（宿主嵌套） | `NO_WRITER` |

- `NO_WRITER` 的 hint 区分两种情况：
  - 宿主嵌套：列出检测到的宿主，例如「检测到 claude-code 与 codex 的会话变量（宿主嵌套），无法判断 Writer；在命令前加 `ALMANAC_WRITER=<宿主名>`」。
  - 没有宿主：提示设置 `ALMANAC_WRITER`。
- 不用作信号：`CLAUDECODE`（IDE 集成终端也会设置）、`AI_AGENT`（会从外层宿主泄漏到内层）、`CODEX_CI`、`CODEX_SANDBOX`、`CLAUDE_CODE_ENTRYPOINT`。
- 人在终端里写入时，设 `ALMANAC_WRITER=human`。

### MCP

1. `ALMANAC_WRITER` 非空（手抄配置都设了它）：用它的值。
2. 否则按 MCP `initialize` 的 `clientInfo.name`：`codex-mcp-client` → `codex`，`pi` → `pi`。
3. 否则按 CLI 表的第 2–4 级。

### `session` 与 `model`

`name` 为 `claude-code`、`codex`、`pi` 之一，且环境里有该宿主的变量时附上：

| `name` | `session` | `model` |
|---|---|---|
| `claude-code` | `CLAUDE_CODE_SESSION_ID` | — |
| `codex` | `CODEX_THREAD_ID`，没有则 `CODEX_SESSION_ID` | — |
| `pi` | `PI_SESSION_ID` | `PI_PROVIDER/PI_MODEL` |

宿主嵌套时自报属实，这些变量就是最内层宿主自己的：内层宿主会覆盖同名变量，Pi 还会先清掉继承来的 `PI_*`。

### 必须显式设置 `ALMANAC_WRITER` 的路径

| 宿主 | shell 工具里的 CLI | hook / 扩展 | MCP |
|---|---|---|---|
| CC | 可推断 | 可推断 | 手抄配置已设 |
| Codex | 可推断 | 须设（hook 环境里没有会话变量） | 手抄配置已设 |
| Pi | 可推断 | 须由扩展传入 | 手抄配置已设 |

插件里的会话开始注入只读，不受此影响。宿主侧的环境变量与变量名以各宿主的版本为准，实现前按版本复核。

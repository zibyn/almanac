# 宿主智能体接入面调研（Codex / Claude Code / Pi）

> 回答 #4：三个宿主各自如何接入外部工具与上下文，almanac 一套二进制覆盖三者的最小公共接入面是什么。
> 调研日期 2026-10-20。来源均为官方文档与源码；源码快照：`openai/codex@46d252f8bf`、`earendil-works/pi@a731ba8b80`。

## 0. 宿主身份确认

- **Codex** = OpenAI Codex CLI，源码 [openai/codex](https://github.com/openai/codex)（Rust，`codex-rs/`）。官方文档已从 `developers.openai.com/codex/*` 308 跳转到 `learn.chatgpt.com/docs/*`。
- **Claude Code** = Anthropic Claude Code，文档 [code.claude.com/docs](https://code.claude.com/docs)（闭源，以文档为准）。
- **Pi** = Mario Zechner 的 pi coding agent。`badlogic/pi-mono` 已重定向到 **[earendil-works/pi](https://github.com/earendil-works/pi)**（`gh repo view badlogic/pi-mono` 返回该 URL），编码智能体在 `packages/coding-agent`，npm 包名 `@earendil-works/pi-coding-agent`。
  - **注意**：旧认知"Pi 刻意不支持 MCP"已过时 —— 当前 Pi **内置 MCP 客户端**（[docs/mcp.md](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md)）。

## 1. 能力矩阵

| 维度 | Codex | Claude Code | Pi |
|---|---|---|---|
| **MCP 配置位置** | `~/.codex/config.toml` 的 `[mcp_servers.<name>]`；项目级 `.codex/config.toml`（仅受信项目）[^cx-mcp] | `~/.claude.json`（local/user 作用域）、项目根 `.mcp.json`（project 作用域）[^cc-mcp] | `~/.pi/agent/mcp.json`、项目 `.pi/mcp.json`（需项目信任）；也可由扩展 `pi.registerMcpServer()` 注册 [^pi-mcp] |
| 配置格式 | TOML | JSON `mcpServers` | JSON `mcpServers`（与 CC 同格式，可直接拷贝）[^pi-mcp] |
| CLI 添加 | `codex mcp add <name> --env K=V -- <cmd>` | `claude mcp add --transport stdio <name> -- <cmd>`（`-s user` 等） | `pi mcp add <name> -- <cmd>`（默认用户级，`-l` 写项目级） |
| 传输 | stdio、Streamable HTTP | stdio、HTTP、SSE（弃用）、WebSocket | stdio、Streamable HTTP（拒绝 SSE） |
| MCP 工具对模型可见方式 | 注册为 `mcp__<server>__<tool>`；server `instructions` 作为工具命名空间描述 [^cx-instr]（是否默认延迟加载未核实） | 默认 **tool search 延迟加载**：会话开始只加载工具名 + server instructions（截断 4096 字符）；`alwaysLoad: true` 可常驻 [^cc-mcp] | 默认 `exposure: "codemode"`（模型须写脚本 `searchTools()` 才能调用）；`direct` 才像内建工具；系统提示里列出 server 一行描述 [^pi-mcp] |
| MCP resources / prompts | 支持（resource 工具） | resources 可 `@` 引用；prompts 成为 `/mcp__srv__prompt` 命令 | 支持 resources（`list_mcp_resources` 等），未见 prompts |
| stdio server 的环境变量 | **清空环境**，只继承白名单 `HOME LOGNAME PATH SHELL USER LANG LC_ALL TERM TMPDIR TZ` + 配置的 `env`/`env_vars` [^cx-env] | 继承；额外设置 `CLAUDE_PROJECT_DIR`=项目根 [^cc-mcp] | 默认继承 `process.env` + `env`（`inheritEnv:false` 可关）[^pi-stdio] |
| stdio server 的 cwd | 配置 `cwd`，否则为运行时 cwd（`LocalStdioServerLauncher::new(runtime_context.local_process_cwd())`）[^cx-cwd] | 文档未写明；用 `CLAUDE_PROJECT_DIR` 或 `roots/list` | 配置 `cwd`（相对路径基于会话目录），默认会话目录 [^pi-rt] |
| MCP `roots` | **不声明** roots 能力（只声明 elicitation + extensions）[^cx-init]；实验性 `codex/sandbox-state` 能力在每次 tool call 的 `_meta` 中携带 `sandbox_cwd` [^cx-sbx] | 支持：`roots/list` 返回会话启动目录 + `--add-dir` 目录，变化时发 `list_changed` [^cc-mcp] | 支持：`roots: [{uri: file://<cwd>, name}]` [^pi-rt] |
| `initialize.clientInfo.name` | `codex-mcp-client`（title `Codex`）[^cx-init] | 闭源未见文档（未验证，预期为 `claude-code`，需实测） | `pi` [^pi-rt] |
| **Hooks 机制** | 外部命令；`hooks.json` 或 `config.toml` 的 `[hooks]`，用户级/项目级；默认启用，但非托管 hook 须经 `/hooks` 审核信任 [^cx-hooks] | 外部命令（也有 http / mcp_tool 类型）；`settings.json` 的 `hooks` [^cc-hooks] | **无外部命令 hook**；TypeScript 扩展进程内 `pi.on(event)` [^pi-ext] |
| 会话开始 | `SessionStart`（matcher: `startup/resume/clear/compact/fork`）| `SessionStart`（同上 5 个 matcher）| `session_start` 事件 |
| 会话开始能否注入上下文 | **能**：stdout 纯文本或 `hookSpecificOutput.additionalContext` 作为 developer context [^cx-hooks] | **能**：exit 0 的 stdout 或 `hookSpecificOutput.additionalContext`，以 system reminder 插在会话开头 [^cc-hooks] | **能**（需扩展）：`before_agent_start` 可返回 `message` / 修改 `systemPromptOptions`；或 `pi.sendMessage()` [^pi-ext] |
| 会话结束 | `SessionEnd`：仅建议性，默认超时 1s、最大 3s，总是同步；子代理不触发 [^cx-hooks] | `SessionEnd`：无决策控制，默认共享 1.5s 预算，可提到 60s [^cc-hooks] | `session_shutdown`（也在 `/new`、`/resume`、reload 时触发）[^pi-ext] |
| 每轮用户输入 | `UserPromptSubmit`（stdout/additionalContext 注入）| `UserPromptSubmit`（同）| `input` / `before_agent_start` |
| 工具前/后 | `PreToolUse`/`PostToolUse`：覆盖 shell、apply_patch、**MCP 工具**、本地函数工具；不覆盖 WebSearch 等托管工具 | `PreToolUse`/`PostToolUse`，matcher 如 `mcp__almanac__.*` | `tool_call`/`tool_result`（MCP 调用同样经过）|
| 压缩前后 | `PreCompact`/`PostCompact` | `PreCompact`/`PostCompact` | `context` 转换、compaction 相关事件 |
| Stop | `Stop`（含 `last_assistant_message`）| `Stop`（含 `last_assistant_message`）| `agent_before_settle` / `agent_settled` |
| hook 输入公共字段 | `session_id`、`transcript_path`、`cwd`、`hook_event_name`、`model`、`turn_id` [^cx-hooks] | `session_id`、`transcript_path`、`cwd`、`hook_event_name`、`permission_mode` 等；env `CLAUDE_PROJECT_DIR` [^cc-hooks] | `ExtensionContext` 提供 cwd、session manager、model runtime [^pi-ext] |
| **指令文件** | `~/.codex/AGENTS.md`（或 `AGENTS.override.md`）+ 从 git 根到 cwd 每层一个 `AGENTS.md`；合计上限 `project_doc_max_bytes` 默认 32 KiB [^cx-agents] | `~/.claude/CLAUDE.md`、`./CLAUDE.md`、`CLAUDE.local.md`、`.claude/rules/`；**无 CLAUDE.md 时**读 `AGENTS.md`（v2.1.277+）；支持 `@path` 导入 [^cc-mem] | `~/.pi/agent/` 下 `AGENTS.md`/`CLAUDE.md`（及 override）+ cwd 与各父目录的 `AGENTS.md`/`CLAUDE.md`；`APPEND_SYSTEM.md` 追加系统提示 [^pi-cfg] |
| **Skills** | Agent Skills 标准；`.agents/skills`（cwd→repo 根）、`~/.agents/skills`、`/etc/codex/skills`；列表占上下文 ≤2% [^cx-skills] | Agent Skills 标准；`~/.claude/skills/`、`.claude/skills/`、插件 `skills/` [^cc-skills] | Agent Skills 标准；`~/.pi/agent/skills/`、`.pi/skills/`，**也读** `~/.agents/skills/`、`.agents/skills/` [^pi-skills] |
| 打包分发 | plugins（skills + MCP；hooks 也可，需信任）| plugins（skills + hooks + MCP + agents）| Pi packages（npm/git，含扩展 + skills）|
| 会话/身份信息给 shell 子进程 | `CODEX_SESSION_ID`、`CODEX_THREAD_ID`、`CODEX_TOOL_CALL_ID` [^cx-shellenv] | `CLAUDE_PROJECT_DIR`（hooks）、`CLAUDE_EFFORT` 等 | `PI_CODING_AGENT=true`、`PI_SESSION_ID`、`PI_SESSION_FILE`、`PI_PROVIDER`、`PI_MODEL` [^pi-env] |

## 2. 关键问题逐条回答

### 2.1 会话开始能否自动注入内容？

三者都能，但路径不同：

- **Codex 与 Claude Code：`SessionStart` 命令 hook，协议几乎一致**。两者都接受"stdout 纯文本"或 JSON `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"..."}}`，matcher 取值也相同（`startup|resume|clear|compact|fork`）。因此 almanac 只需一个命令（如 `almanac context --hook`）读取 stdin JSON（拿 `cwd`、`session_id`），输出同一种 JSON，即可两边复用。
- **Pi：需一个很小的 TS 扩展**（Pi 没有外部命令 hook）。扩展在 `before_agent_start`（首轮）返回一条 `message`，内容来自 `execFile("almanac", ["context", ...])`。可随 Pi package 分发。
- **仅靠 MCP 的弱注入**：server `instructions` 在 CC 会话开始即加载（≤4096 字符）、在 Codex 作为工具命名空间描述、在 Pi 只取一行进系统提示。只适合放"何时用 almanac"的静态说明，**不适合**放动态信念。
- **静态兜底**：AGENTS.md / CLAUDE.md 只能放静态说明（何时查询、如何写入），不能按项目动态生成，除非 almanac 写文件（不建议，污染仓库）。

预算参考：Codex hook 输出 >~2500 token 会落盘只给预览 [^cx-hooks]；CC 的 MCP 工具结果默认上限 25k token；Pi MCP 文本结果 >20 KB 截中间 [^pi-mcp]。会话开始注入宜控制在 ~1–2k token 内。

### 2.2 能否拿到当前项目路径？

都能，但没有一个统一通道：

| 通道 | Codex | CC | Pi |
|---|---|---|---|
| hook 输入 `cwd` | ✓ | ✓（另有 `CLAUDE_PROJECT_DIR`，进 worktree 后仍不变）| 扩展 `ctx.cwd` |
| MCP `roots/list` | ✗ | ✓ | ✓ |
| MCP server 环境变量 | ✗（环境被清空）| ✓ `CLAUDE_PROJECT_DIR` | ✗（但继承 shell 环境）|
| MCP server 进程 cwd | ✓ 默认运行时 cwd | 未文档化 | ✓ 默认会话目录 |
| tool call `_meta` | 实验性 `codex/sandbox-state` → `sandbox_cwd` | ✗ | ✗ |

结论：almanac MCP server 解析"当前项目"的顺序建议为 **工具参数显式 `project` > `roots/list` > `CLAUDE_PROJECT_DIR` > 进程 cwd**，再向上找 git 根作为项目 Scope 键。工具参数是唯一三家都一定可用、且对 user 级单进程多会话也正确的方式（模型知道自己的 cwd）。

### 2.3 能否拿到 Writer 身份？

**没有宿主会主动告诉工具"写入者是谁"**，但有三种可用信号：

1. **MCP `initialize.clientInfo.name`**：Codex=`codex-mcp-client`，Pi=`pi`，CC 未文档化（需实测）。零配置即可区分宿主，但不区分模型/会话，且可被伪造（单人库可接受）。
2. **配置里的静态环境变量**：在每个宿主的 MCP 配置里写 `env = { ALMANAC_WRITER = "codex" }`。三家都支持 `env` 字段，最可靠，安装器写配置时顺手填。
3. **会话级信息**：hook 输入有 `session_id`（Codex/CC），Codex 还有 `model`；shell 子进程可见 `CODEX_THREAD_ID` / `PI_SESSION_ID` / `PI_MODEL`。CLI 路径可据此补全 Writer 的会话/模型维度。

建议 Writer = `宿主(静态 env，回退 clientInfo) + 可选 model + 可选 session_id`，具体取值格式留给 Writer 定义的 ticket。

## 3. 最小公共接入面（建议）

按"三家都原生支持"取交集：

1. **MCP stdio server（核心，必选）**：`almanac mcp` 子命令。三家都支持 stdio + `command/args/env/cwd`；CC 与 Pi 共享同一段 `mcpServers` JSON，Codex 为等价 TOML。
   - Pi 需在条目上加 `"exposure": "direct"`，否则默认 codemode 下模型看不到工具；CC 可选 `alwaysLoad: true` 让少量核心工具常驻。
   - 不依赖环境变量继承（Codex 会清空），身份与项目通过 `env`/工具参数/roots 传入。
   - 工具数量保持少（各家延迟加载策略不同，常驻工具都占上下文）。
2. **同一二进制的 CLI（必选）**：三家都有 shell 工具，CLI 既是 MCP 不可用时的兜底，也是 hook/扩展的执行体。
3. **Agent Skills 格式的 `SKILL.md`（推荐）**：三家都实现 agentskills.io 标准。只需两份安装位置：`~/.agents/skills/almanac/`（Codex + Pi）与 `~/.claude/skills/almanac/`（CC）。承载"何时检索/何时写入/如何写 Evidence"的操作指南，按需加载不占常驻上下文。
4. **会话开始注入（可选增强）**：`almanac context` 一条命令 + 两种薄壳：CC/Codex 的 `SessionStart` hook 配置（JSON 协议相同），Pi 的 ~20 行 TS 扩展。
5. **会话结束**：三家都有事件但预算极短（Codex 1–3s，CC 默认 1.5s），只能做"追加一条事件到日志"之类的 O(ms) 操作，**不能**做整理/总结；记忆维护应由显式命令或下次会话开始触发。

不建议纳入公共面：PreToolUse/PostToolUse 自动采集（三家语义差异大、噪声高，可作为后续可选适配器）；HTTP 传输（Server 模式可复用 Streamable HTTP，三家都支持，但单机内嵌用 stdio 即可）。

## 4. 风险与未验证点

- **CC 的 clientInfo.name 与 stdio server cwd 未在文档中写明**，需实测。
- Codex `codex/sandbox-state` 是实验性能力，不应依赖。
- Codex 的 MCP stdio `cwd` 回退值是 `local_process_cwd()`，在 `codex --cd <dir>` 场景下是否等于会话目录未逐行验证。
- 项目级配置（`.codex/`、`.pi/`）都需要"项目信任"，安装器应优先写用户级配置。
- 三家文档更新极快（CC 文档标注了 v2.1.2xx 的行为变化，Codex 文档本月刚迁域名），实现前应按版本复核。

## 来源

[^cx-mcp]: Codex MCP 文档 https://learn.chatgpt.com/docs/extend/mcp?surface=cli
[^cx-hooks]: Codex Hooks 文档 https://learn.chatgpt.com/docs/hooks
[^cx-agents]: Codex AGENTS.md 文档 https://learn.chatgpt.com/docs/agent-configuration/agents-md
[^cx-skills]: Codex Skills 文档 https://learn.chatgpt.com/docs/build-skills
[^cx-env]: `codex-rs/rmcp-client/src/utils.rs`（`DEFAULT_ENV_VARS`、`create_env_for_mcp_server`）https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/src/utils.rs
[^cx-cwd]: `codex-rs/rmcp-client/src/stdio_server_launcher.rs`（`fallback_cwd`）与 `codex-rs/codex-mcp/src/rmcp_client.rs` https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/src/stdio_server_launcher.rs
[^cx-init]: `codex-rs/codex-mcp/src/rmcp_client.rs` 的 `mcp_initialize_request_params`（capabilities 只含 elicitation/extensions，clientInfo `codex-mcp-client`）https://github.com/openai/codex/blob/main/codex-rs/codex-mcp/src/rmcp_client.rs
[^cx-instr]: 同上文件，`regular_mcp_tool_info_from_listed_tool` 将 `server_instructions` 设为 `namespace_description`
[^cx-sbx]: `codex-rs/codex-mcp/README.md` 与 `src/sandbox_metadata.rs`（`sandbox_cwd`）https://github.com/openai/codex/blob/main/codex-rs/codex-mcp/README.md
[^cx-shellenv]: `codex-rs/protocol/src/shell_environment.rs` https://github.com/openai/codex/blob/main/codex-rs/protocol/src/shell_environment.rs
[^cc-mcp]: Claude Code MCP 文档 https://code.claude.com/docs/en/mcp
[^cc-hooks]: Claude Code Hooks 参考 https://code.claude.com/docs/en/hooks
[^cc-mem]: Claude Code 记忆/CLAUDE.md/AGENTS.md 文档 https://code.claude.com/docs/en/memory
[^cc-skills]: Claude Code Skills 文档 https://code.claude.com/docs/en/skills
[^pi-mcp]: Pi MCP 文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md
[^pi-ext]: Pi 扩展文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md
[^pi-cfg]: Pi 配置文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/configuration.md
[^pi-skills]: Pi Skills 文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md
[^pi-env]: Pi 环境变量文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/environment-variables.md
[^pi-rt]: `packages/coding-agent/src/extensions/mcp/runtime.ts`（clientInfo `pi`、`roots`、`cwd` 解析）https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/extensions/mcp/runtime.ts
[^pi-stdio]: `packages/mcp/src/transports/stdio.ts`（`inheritEnv`）https://github.com/earendil-works/pi/blob/main/packages/mcp/src/transports/stdio.ts

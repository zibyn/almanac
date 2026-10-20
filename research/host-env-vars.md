# 宿主特征环境变量实测（Claude Code / Codex / Pi）

> 调研日期：2026-10-20。
> 参考资料，决策以 ticket 为准。
>
> 回答 [#16](https://github.com/zibyn/almanac/issues/16)：Claude Code、Codex、Pi 在 hook 进程和 shell 工具子进程里各设置了哪些环境变量，`almanac` CLI 能否据此认出调用它的宿主（Writer），能否顺带区分会话和模型？
> 来源：Claude Code 官方文档（闭源，以文档为准，另在本机 Claude Code v2.1.304 的 Bash 工具里实际看了一遍）；Codex 源码快照 `openai/codex@ab6095dde0`；Pi 源码与文档快照 `earendil-works/pi@fdce48eb40`。

## 结论先行

1. **三家都在 shell 工具子进程里注入了会话级变量**，可以作为宿主特征：Claude Code 是 `CLAUDE_CODE_SESSION_ID`，Codex 是 `CODEX_THREAD_ID` / `CODEX_SESSION_ID`，Pi 是 `PI_SESSION_ID`。其中 Claude Code 和 Pi 的变量写在官方文档里；Codex 的变量只在源码里定义，文档没有列出，SDK README 在示例里用到了 `CODEX_THREAD_ID`。
2. **hook 进程里的情况差别很大**：
   - Claude Code 的 hook 有 `CLAUDE_CODE_SESSION_ID`，和 Bash 工具一样，可以推断。
   - Codex 的 hook 拿到的是 Codex 进程自身环境的快照，加上 hook 配置里的 `env`，**没有任何 `CODEX_*` 会话变量**，从环境变量推断不出宿主。会话和模型只在 hook 的 stdin JSON 里（`session_id`、`model`）。
   - Pi 没有 shell hook，它的 hook 是进程内的 TypeScript 扩展。扩展派生的子进程会继承 `PI_CODING_AGENT=true` 和 `AI_AGENT=pi`（进程级标记），但没有 `PI_SESSION_ID`。
3. **只有 Pi 在环境变量里提供模型**（`PI_PROVIDER`、`PI_MODEL`）。Claude Code 的文档明确写了“没有 `$CLAUDE_MODEL` 环境变量”；Codex 也不注入模型。
4. **嵌套宿主会让变量串味**：比如在 Codex 的 shell 里启动 Claude Code，Claude Code 的 Bash 子进程仍会继承外层的 `CODEX_THREAD_ID`。任何固定的检查顺序都会在某种嵌套下认错宿主，所以建议**同时出现多个宿主的会话变量时视为无法推断**，报 `NO_WRITER`。
5. 因此：**Codex 的 hook 和 Pi 的扩展必须自己设置 `ALMANAC_WRITER`**（在 hook 配置的 `env` 里写，或者由扩展在 spawn 时传）。SKILL.md 引导的 CLI 调用都走 shell 工具，三家都能推断出来。

## 1. Claude Code

| 变量 | 取值 | 稳定性 | 会话/模型 | hook | Bash 工具 | stdio MCP |
|---|---|---|---|---|---|---|
| `CLAUDE_CODE_SESSION_ID` | 会话 UUID（`/clear` 后会更新） | 文档 [^cc-env] | 会话 ✓ | ✓ | ✓ | ✓（保留启动时的 ID） |
| `CLAUDECODE` | `1` | 文档 [^cc-env] | ✗ | ✓ | ✓ | ✓ |
| `CLAUDE_CODE_CHILD_SESSION` | `1` | 文档 [^cc-env] | ✗ | ✓ | ✓ | ✗ |
| `CLAUDE_PID` | Claude Code 进程的 PID | 文档，v2.1.214+ [^cc-env] | ✗ | ✓ | ✓ | — |
| `CLAUDE_EFFORT` | `low`…`max` | 文档 [^cc-env] | 推理强度（不是模型） | ✓ | ✓ | — |
| `CLAUDE_PROJECT_DIR` | 项目根路径 | 文档 [^cc-hooks] | ✗ | ✓ | 文档只写了 hook | ✓ |
| `CLAUDE_CODE_REMOTE_SESSION_ID` / `CLAUDE_CODE_BRIDGE_SESSION_ID` | `session_…` | 文档，只在云端会话 / Remote Control 下出现 [^cc-env] | 会话 | ✓ | ✓ | — |
| `CLAUDE_CODE_ENTRYPOINT` | 实测为 `cli` | **不在文档里**，实测才看到 | ✗ | 未知 | 实测 ✓ | — |
| `AI_AGENT` | 实测为 `claude-code_2-1-304_agent`（名称_版本_角色） | **不在文档里**，实测才看到 | 带版本，不带会话 | 未知 | 实测 ✓ | — |

要点：

- `CLAUDECODE=1` **IDE 扩展在它的集成终端里也会设置**（文档原文），所以用户在 VS Code 终端里手动跑 `almanac` 也会命中它，不能当作“由 Claude Code 智能体调用”的证据。`CLAUDE_CODE_SESSION_ID` 只在 Claude Code 自己派生的子进程里出现，是首选信号。
- 文档说 hook 进程继承父进程环境（`OTEL_*` 除外），stdin 公共字段有 `session_id`，**但没有 `model`**：只有 SessionStart 可能带 `model`，而且“不保证带”[^cc-hooks]。所以 Claude Code 下 Writer 推断不出模型。

## 2. Codex（OpenAI Codex CLI）

| 变量 | 取值 | 稳定性 | 会话/模型 | hook | shell 工具 |
|---|---|---|---|---|---|
| `CODEX_THREAD_ID` | 线程 ID（UUID） | 源码常量 [^cx-shellenv]；SDK README 用它恢复线程 [^cx-sdk]；官方文档没列出 | 会话（线程）✓ | ✗ | ✓ |
| `CODEX_SESSION_ID` | 根会话 ID（子智能体共享） | 源码常量 [^cx-shellenv][^cx-execenv] | 会话 ✓ | ✗ | ✓ |
| `CODEX_TOOL_CALL_ID` | 本次工具调用 ID | 源码 [^cx-shellenv] | 单次调用 | ✗ | ✓ |
| `CODEX_VERSION` | Codex 版本号 | 源码，`pub(crate)` [^cx-execenv] | ✗ | ✗ | ✓ |
| `CODEX_PERMISSION_PROFILE` | 当前权限配置名 | 源码 [^cx-execenv] | ✗ | ✗ | 有配置时才有 |
| `CODEX_CI` | `1` | 源码，只在 unified exec 路径 [^cx-uexec] | ✗ | ✗ | ✓ |
| `CODEX_SANDBOX` | 目前只有 macOS 的 `seatbelt` | 源码注释写明“将来可能变” [^cx-spawn] | ✗ | ✗ | 只在沙箱内 |
| `CODEX_SANDBOX_NETWORK_DISABLED` | `1` | 源码 [^cx-spawn] | ✗ | ✗ | 只在断网沙箱内 |
| `CODEX_MANAGED_BY_NPM` / `_BUN` / `_PNPM` / `_VITE_PLUS` | 安装方式标记 | 只在通过 npm 包装器 `codex.js` 启动时才有 [^cx-js] | ✗ | 继承 | 继承 |

要点：

- shell 工具（`exec_command` / unified exec）的环境构造过程是：先按 `shell_environment_policy` 过滤（默认 `inherit = all` 且 `ignore_default_excludes = true`，即默认继承 Codex 进程的全部环境变量[^cx-cfg]），再强制写入 `CODEX_THREAD_ID`、`CODEX_TOOL_CALL_ID`、`CODEX_SESSION_ID`、`CODEX_VERSION`[^cx-uexec][^cx-execenv]。即使配置了 `include_only`，`CODEX_THREAD_ID` 也会注入（源码注释）。用户执行的 `!` shell 命令（`tasks/user_shell.rs`）同样会注入这些变量。
- **hook**：`Hooks::new` 在启动时用 `std::env::vars_os()` 拍下 Codex 进程环境的快照，`build_command` 在这份快照上叠加 hook 配置自带的 `env`，然后启动 hook[^cx-hookreg][^cx-hookrun]。这条路径上没有注入任何 `CODEX_*` 会话变量。会话和模型只能从 stdin JSON 的 `session_id`、`model`、`turn_id` 读取[^cx-hooks]。所以从 Codex hook 调用 `almanac` 时，需要在 hook 配置的 `env` 里写 `ALMANAC_WRITER=codex`（或者写在命令行里）。
- shell 环境里没有模型信息。
- MCP stdio server 的环境会被清空，只保留白名单变量（见 [host-integration-surfaces.md](host-integration-surfaces.md)），这不属于 CLI 路径。

## 3. Pi（earendil-works/pi，原 badlogic/pi-mono）

| 变量 | 取值 | 稳定性 | 会话/模型 | 扩展派生子进程 | bash/powershell 工具 |
|---|---|---|---|---|---|
| `PI_CODING_AGENT` | `true` | 文档 [^pi-env]；`cli/setup.ts` 写入 `process.env` [^pi-setup] | ✗ | ✓（继承） | ✓ |
| `AI_AGENT` | `pi` | 文档，标明是“通用标记” [^pi-env] | ✗ | ✓ | ✓ |
| `PI_SESSION_ID` | 会话 ID | 文档 [^pi-env]；`tools/bash.ts` [^pi-bash] | 会话 ✓ | ✗ | ✓ |
| `PI_SESSION_FILE` | 会话 JSONL 的绝对路径；临时会话不设置 | 文档 | 会话 | ✗ | ✓ |
| `PI_PROVIDER` / `PI_MODEL` | 所选的 provider 和模型 ID（每条命令启动时解析） | 文档 | **模型 ✓** | ✗ | ✓ |
| `PI_REASONING_LEVEL` | `off`…`max` | 文档 | 推理强度 | ✗ | ✓ |

要点：

- 进程标记（`PI_CODING_AGENT`、`AI_AGENT`）只由 CLI 和 RPC 入口设置。**通过 SDK 嵌入 Pi 时不会设置**（文档原文）。
- 会话变量只注入给 LLM 调用的 `bash`/`powershell` 工具，**用户手动输入的 `!`/`!!` 命令拿不到**。注入前会先删掉继承来的旧 `PI_*` 值，所以嵌套的 Pi 不会暴露外层会话的信息[^pi-bash]。
- Pi 没有 shell hook，“hook”就是进程内扩展（`pi.on(...)`）。扩展如果自己 spawn `almanac`，只会继承进程标记，拿不到会话变量（这是推断：扩展的 spawn 不经过 bash 工具的环境构造）。扩展可以直接从 `ExtensionContext` 读到会话和模型，应该自己把 `ALMANAC_WRITER` 传给子进程。

## 4. 推荐的 Writer 推断优先级（CLI）

| 序 | 信号 | 推断结果 | 可附带 |
|---|---|---|---|
| 1 | `ALMANAC_WRITER` 非空 | 原值 | — |
| 2 | 宿主会话变量，**恰好只命中一家**：`CLAUDE_CODE_SESSION_ID` → claude-code；`CODEX_THREAD_ID`（或 `CODEX_SESSION_ID`）→ codex；`PI_SESSION_ID` → pi | 对应宿主 | 会话 ID；Pi 还有 `PI_PROVIDER`/`PI_MODEL` |
| 3 | 第 2 级一家都没命中，且 `PI_CODING_AGENT=true` | pi（不带会话） | — |
| 4 | 以上都不满足，或第 2 级命中多家（嵌套） | 拒绝写入，`NO_WRITER` | hint：设置 `ALMANAC_WRITER` |

不纳入推断的信号及原因：

- `CLAUDECODE`：IDE 集成终端也会设置，会把用户手动执行的命令误认成 Claude Code。
- `AI_AGENT`：Claude Code 的取值格式不在文档里；Codex 不设置；外层宿主的值会泄漏到内层（例如 Codex 在 Claude Code 里运行时，Codex 的 shell 会继承 `AI_AGENT=claude-code_…`）。
- `CODEX_CI`、`CODEX_SANDBOX`：只在部分路径或平台上出现。
- Claude Code 的 `CLAUDE_CODE_ENTRYPOINT`：不在文档里。

各宿主下哪些路径必须显式设置 `ALMANAC_WRITER`：

| 宿主 | shell 工具（SKILL.md 引导的 CLI 调用） | hook / 扩展 |
|---|---|---|
| Claude Code | 可以推断 | 可以推断（hook 里有 `CLAUDE_CODE_SESSION_ID`） |
| Codex | 可以推断 | **必须设置**：在 hook 配置的 `env` 里写 `ALMANAC_WRITER=codex` |
| Pi | 可以推断（还能拿到模型） | **扩展应自己传**；不传就退到第 3 级，只能得到宿主名 |

`context` 这个 hook 是只读的，不受 Writer 影响（见 #13），所以 hook 上缺 Writer 只影响从 hook 发起的写入，目前插件设计里没有这类写入。

## 5. 不确定之处

- Codex 的变量没有写进官方文档，属于实现细节，升级时可能改名，实现前要按版本复核。`CODEX_THREAD_ID` 有 SDK README 引用，相对更稳。
- Codex 的 hook 环境是从源码读出来的结论，没有在真实的 Codex 里跑过。Pi 同样没有实测。
- Claude Code 的 `AI_AGENT`、`CLAUDE_CODE_ENTRYPOINT` 只在本机 v2.1.304 上看到过，它们在 hook 进程里是否存在没有验证。
- 嵌套时报 `NO_WRITER` 是保守的选择：在某个宿主的 shell 里启动另一个宿主是少见但真实的用法，这时需要用户自己设置 `ALMANAC_WRITER`。

## 来源

[^cc-env]: Claude Code 环境变量参考 https://code.claude.com/docs/en/env-vars（`CLAUDE_CODE_SESSION_ID`、`CLAUDECODE`、`CLAUDE_CODE_CHILD_SESSION`、`CLAUDE_PID`、`CLAUDE_EFFORT`、`CLAUDE_CODE_REMOTE_SESSION_ID`、`CLAUDE_CODE_BRIDGE_SESSION_ID` 各行）
[^cc-hooks]: Claude Code Hooks 参考 https://code.claude.com/docs/en/hooks（公共输入字段；“There is no `$CLAUDE_MODEL` environment variable”）
[^cx-shellenv]: `codex-rs/protocol/src/shell_environment.rs`（常量定义、`populate_env`）https://github.com/openai/codex/blob/main/codex-rs/protocol/src/shell_environment.rs
[^cx-execenv]: `codex-rs/core/src/exec_env.rs`（`inject_session_env`、`CODEX_VERSION`、`CODEX_PERMISSION_PROFILE`）https://github.com/openai/codex/blob/main/codex-rs/core/src/exec_env.rs
[^cx-uexec]: `codex-rs/core/src/unified_exec/process_manager.rs`（`UNIFIED_EXEC_ENV`、`open_session_with_sandbox`）https://github.com/openai/codex/blob/main/codex-rs/core/src/unified_exec/process_manager.rs
[^cx-spawn]: `codex-rs/core/src/spawn.rs`（`CODEX_SANDBOX`、`CODEX_SANDBOX_NETWORK_DISABLED`）https://github.com/openai/codex/blob/main/codex-rs/core/src/spawn.rs
[^cx-js]: `codex-cli/bin/codex.js`（`CODEX_MANAGED_BY_*`）https://github.com/openai/codex/blob/main/codex-cli/bin/codex.js
[^cx-hookreg]: `codex-rs/hooks/src/registry.rs`（`Hooks::new` 拍下 `std::env::vars_os()` 快照）https://github.com/openai/codex/blob/main/codex-rs/hooks/src/registry.rs
[^cx-hookrun]: `codex-rs/hooks/src/engine/command_runner.rs`（`build_command`：快照 + hook `env`）https://github.com/openai/codex/blob/main/codex-rs/hooks/src/engine/command_runner.rs
[^cx-cfg]: `codex-rs/protocol/src/config_types.rs`（`impl Default for ShellEnvironmentPolicy`）https://github.com/openai/codex/blob/main/codex-rs/protocol/src/config_types.rs
[^cx-hooks]: Codex Hooks 文档 https://learn.chatgpt.com/docs/hooks
[^cx-sdk]: `sdk/typescript/README.md`（`process.env.CODEX_THREAD_ID`）https://github.com/openai/codex/blob/main/sdk/typescript/README.md
[^pi-env]: Pi 环境变量文档 https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/environment-variables.md
[^pi-setup]: `packages/coding-agent/src/cli/setup.ts` https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/cli/setup.ts
[^pi-bash]: `packages/coding-agent/src/core/tools/bash.ts`（先删除继承的 `PI_*`，再按 ctx 注入）https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/tools/bash.ts

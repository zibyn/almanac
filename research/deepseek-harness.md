# DeepSeek Harness（DSH）接入面调研

> 回答：DSH 的插件模型、skills、会话生命周期、hook、Writer 识别与分发面；almanac 适配所需的最小接入面。
> 调研日期 2026-10-22。来源：官方仓库 `deepseek-ai/deepseek-harness`（`master`）、官方文档站、npm 上 `@deepseek-ai/dsh@0.2.1-rc.2`。参考资料，决策以对应 ticket 的结论为准。

## 0. 身份与版本

- **DSH** = DeepSeek Harness，CLI 名 `dsh`，基于 Cordis 的“一切皆插件”智能体运行时，MIT。
- 官方 README 明确标注 **developer preview**，会有破坏性变更；适配需按安装版本复核。
- Node 要求 `^22.19.0 || >=24.0.0`；插件管理要求 `pnpm` 在 `PATH` 上。
- 调研时 npm `latest` / `next` 均为 `@deepseek-ai/dsh@0.2.1-rc.2`。
- 启动形态：`dsh web`、`dsh --profile headless "task"`、`dsh --profile sdk`、`dsh --profile acp`。profile 位于 `$DSH_HOME/profiles/<name>`（默认 `~/.dsh`）。
- 组合顺序：各 bundle 的 patch（按 `dsh.profile.bundles` 顺序）→ profile 自己的 `cordis.patch.yml` → `$DSH_HOME/cordis.patch.yml` → `--patch` 覆盖层；后层按行覆盖。

## 1. 插件模型与安装

**插件形态**：Cordis 插件，导出 `name` 和 `apply(ctx)`，可选 `inject`。所有注册都是 effect，随插件卸载自动清理。

**bundle 声明**：包在自己的 `package.json` 里声明 patch 入口：

```json
{
  "name": "@scope/dsh-example",
  "main": "index.js",
  "dsh": { "bundle": { "patch": "./cordis.patch.yml" } }
}
```

`cordis.patch.yml` 插入自己的插件行：

```yaml
- insert:
    - id: example
      name: '@scope/dsh-example'
```

同一个包既是 Cordis 插件（`main`）又是 bundle（`dsh.bundle.patch`），社区插件普遍如此。

**安装**：

```sh
dsh plugin --profile web add @scope/dsh-example
dsh plugin --profile web add github:zibyn/almanac#path:/dsh
```

- `dsh plugin` 把剩余参数转发给 profile 目录里的 pnpm；成功后按已安装依赖重建 `dsh.profile.bundles`：声明了 `dsh.bundle` 的依赖自动入栈，没有声明的只作为普通依赖并警告。
- pnpm 自 v9 起支持 Git 仓库子目录：`#path:/packages/foo`；github 简写也可用。
- profile 安装的包放在 profile 的 `node_modules`；in-box bundle 优先从 dsh 安装解析，第三方 bundle 从 profile 解析。
- `engines.dsh`（或其缺失时的 `@deepseek-ai/dsh*` peer 依赖范围）在安装和启动时都会校验；不兼容需要显式 exact-version exemption。

**插件市场（可选）**：自定义 index JSON 可由 `Plugins → More → Custom plugin markets` 添加；条目要求 npm 包声明 `dsh.bundle`，版本需精确 SemVer，`engines.dsh` 可选。不是首版必需。

**参考实例**：`@wisdoverse/dsh-skills-manager` 的 `package.json` + `cordis.patch.yml` 就是上述最小形态；peerDependencies 写 DSH 包范围。

## 2. Skills

**发现根**（rank 小者优先）：

| rank | 来源 | 根 |
|---:|---|---|
| 100 | project-dsh | `<project>/.dsh/skills` |
| 200 | project-agents | `<project>/.agents/skills` |
| 300 | custom | `Config.customSkillDirs` |
| 400 | user-dsh | `<dshHome>/skills` |
| 500 | user-agents | `<agentsHome>/skills` |
| 600 | bundled | `Config.bundledSkillDir` |

`project` 是最近的含 `.git` 的祖先目录；不存在时用 cwd。发现不递归搜索 `**/SKILL.md`。

**格式**：Agent Skills 标准。目录包 `<name>/SKILL.md` 或扁平 `<name>.md`；name 必须匹配 `^[a-z0-9]+(?:-[a-z0-9]+)*$`；frontmatter 支持 `disable-model-invocation`、`user-invocable`，默认都为 true。模型目录只暴露 name 和 description。

**插件自带 skill**：在 `apply(ctx)` 中同步注册 provider（约 30 行）：

```ts
export const name = 'almanac'
export const inject = ['skills']

export function apply(ctx) {
  ctx.skills.registerProvider(() => ({
    name: 'almanac',
    list: async () => [candidate],
    get: async () => ({ ...candidate, content: await readFile(SKILL_URL, 'utf8') }),
  }))
}
```

candidate 需要 `name`、`description`、`invocation`、`provider`、`source`、`resourceBase`、`rank`、`locator`；打包 skill 用 `BUNDLED_SKILL_RANK`（600）。skill 正文由 `dsh-tool-skill` 在模型调用 `skill({name})` 时注入；目录在首个 `agent/pre-step` 作为 `<system-reminder>` 出现。

## 3. 会话生命周期：注入与提醒

**会话开始**：事件名是 `agent/created`，不是 `agent/session-start`（官方 cookbook 也明确没有后者；hook 桥内部把 `SessionStart` 映射到 `agent/created`）。

```ts
ctx.on('agent/created', async ({ agent, source, signal }) => { ... })
```

- `source: 'startup' | 'resume' | 'clear' | 'compact'`。
- 该事件是 **serial 且被 await**：监听器按注册顺序运行，创建在全部完成后才 resolve，AgentLoop 在期间保持排队输入。因此在这里注入上下文，可靠地赶在第一轮之前。
- 注入方式：`agent.inject(createUserMessage({ content, source }))`；`createUserMessage` 来自 `@deepseek-ai/dsh-llm`。
- 与 CC/Codex 的 matcher 对应：注入 `startup|clear|compact`，`resume` 不注入。

**自然停止**：事件是 `agent/turn-stopping`：

```ts
ctx.on('agent/turn-stopping', async ({ agent, turn, signal }) => { ... })
```

- 只在自然停止、且下一步 inbox 为空时运行；是 serial 的终止检查点。
- 监听器里 `agent.steer(createUserMessage(...))` 会强制再走一步。用户中断、出错不走这里。
- 需自己做续跑循环保护：steer 之后在下一轮 `turn-stopping` 传 `stop_hook_active: true`，再清除标记。

**工作目录**：`ctx.workingDirectory.ensure(agent, signal)` 取当前 session 工作目录；`ctx.workingDirectory.get(session)` 取已提交值。Session header 的 `cwd` 是原始项目，不是当前目录。`almanac context` 依赖 cwd 解析项目，所以子进程应以当前工作目录启动。

## 4. 官方 hook 桥（备选，不推荐作为主路径）

`@deepseek-ai/dsh-hooks-claude-code` 与 `@deepseek-ai/dsh-hooks-codex` 位于 `packages/experimental`，把现有 `hooks.json` 的命令 hook 映射到原生扩展点。

- 配置：`configPath`（必填）、`pluginRoot`、`projectDir`、`defaultTimeoutMs`、`stderrSummaryMaxChars`。
- 支持的事件：`SessionStart`、`UserPromptSubmit`、`PreToolUse`、`PostToolUse`、`Stop`（CC 桥另有 `SubagentStart/Stop`）。只运行同步 `command` handler。
- `SessionStart` 走 `agent/created`，但桥把它当 **detached** 运行：context 可能错过第一个请求。普通 stdout 不支持，只认 JSON `additionalContext`。
- `Stop` 走 `agent/turn-stopping`，阻塞结果通过 `steer()` 续跑；但 `stop_hook_active` **始终为 false**，没有连续续跑上限，hook 必须自己限流。
- 其他限制：不支持 pre-tool 输入改写、`continue:false` 不生效、`transcript_path` 永远为空、不支持分层的项目/用户/插件配置发现与热重载。

结论：如果只求“能跑”，桥可以复用现有 `hooks/hooks.json`；但要做到与 CC/Codex/Pi 同等的注入可靠性、skill 一体安装与循环保护，原生插件更合适。

## 5. Writer 识别与环境变量

`@deepseek-ai/dsh-shell-env` 给每次**模型 shell 调用**注入受管环境：

| 变量 | 取值 |
|---|---|
| `DSH_HOME` | Harness home 绝对路径 |
| `DSH_SHELL` | `1` |
| `DSH_SESSION_ID` | 调用方 session id（agent 调用才有） |
| `DSH_PROFILE` | profile 名（有 profile context 时） |
| `DSH_PROFILE_DIR` | profile 绝对目录（有 profile context 时） |

- 执行器会先丢弃继承来的 `DSH_*`，再注入当前快照；嵌套 harness 不会串味。
- 用户手动 `!` 命令不经过模型 shell 工具，拿不到 `DSH_SESSION_ID`。
- 不提供模型名变量；Writer 只能补 `session`，补不了 `model`。
- 插件自己 spawn 的子进程不经过 shell-env，拿不到 `DSH_SESSION_ID`；`context` / `remind` 都是只读，不需要 Writer。若将来插件内产生写入，应显式传 `ALMANAC_WRITER`。

**对 almanac 的影响**：`src/writer.rs` 当前只认 `CLAUDE_CODE_SESSION_ID`、`CODEX_THREAD_ID`/`CODEX_SESSION_ID`、`PI_SESSION_ID`。需增加 `DSH_SESSION_ID`，否则 DSH 中的写入一律 `NO_WRITER`。宿主嵌套时外层变量仍可能泄漏进来，现有“多宿主 → `NO_WRITER`”的保守逻辑保持不变。

## 6. MCP

DSH 具备 MCP 客户端与工具命名空间，但插件策略仍是“MCP 不进插件、需要的人手抄配置”。本节未逐项核对 DSH 的 MCP 配置位置与传输细节；如要补文档，应另做调研，不以本文件为依据。

## 7. 分发

- **npm**：包声明 `dsh.bundle` 即可被 `dsh plugin add` 识别；插件市场 index 也以 npm 包为来源。
- **GitHub Release tarball**：`dsh plugin --profile <name> add <tgz 路径或 URL>`。pnpm 支持远程 tarball；已用本地 HTTP 服务验证 `http://…/zibyn-dsh-almanac-<版本>.tgz` 能安装并自动进入 `dsh.profile.bundles`。GitHub Release 的 asset URL 同理。注意：GitHub 自动生成的 Source code zip/tar.gz 不是 npm 包，不能这样装；asset 必须是 `npm pack` 产物。
- **插件市场**：index 的 source 只支持 npm 与 git；Release tarball URL 不能进入市场，只能用 CLI 安装。
- **Git**：`dsh plugin --profile <name> add github:zibyn/almanac#path:/dsh`；pnpm 从 git 取源码，若包需要构建，必须有 `prepare` 脚本（本适配计划用纯 JS，无构建步骤）。
- **本地**：`dsh plugin --profile <name> add /path/to/almanac/dsh`。
- **二进制**：插件可以自己在运行时从同一 Release 下载 `almanac-<version>-<target>[.exe]`，用 `SHA256SUMS` 校验后缓存；不依赖 `PATH`。`ALMANAC_RELEASE_BASE` 可替换下载前缀。
- 名称可用性（2026-10-22 查询）：`@zibyn/dsh-almanac`、`dsh-almanac`、`@zibyn/almanac` 在 npm 均返回 404；是否拥有 `@zibyn` scope 需发布者确认。

## 8. 本地验证（2026-10-22）

- 把 `@deepseek-ai/dsh@0.2.1-rc.2` 装在临时前缀，`DSH_HOME=/tmp/dsh-home`，未触碰用户全局环境。
- `dsh plugin --profile test add /home/zibyn/Projects/almanac/dsh` 成功；profile 的 `dsh.profile.bundles` 自动变为 `["@deepseek-ai/dsh-base", "@zibyn/dsh-almanac"]`，`--dump-config` 出现 `id: almanac` 行。
- `dsh --profile test --dump-config-schema` 退出码 0，说明 DSH 的模块解析能加载插件（`index.js` 的 peer 依赖由宿主提供，profile 的 `node_modules` 没有额外副本）。
- `dsh --profile test` 启动 12 秒无报错（超时退出，符合预期）。
- 用 fake Cordis `ctx` + fake `almanac` 可执行文件验证：skill provider 返回 SKILL.md；`startup` 注入、`resume` 与 subagent 跳过；`turn-stopping` 注入提醒；`stop_hook_active` 循环保护按 session 生效。
- 用本地 HTTP 服务模拟 GitHub Release（`v<版本>/` 下放原始二进制与 `SHA256SUMS`）验证：远程 tarball 安装成功、bundle 自动选中、`--dump-config-schema` 退出码 0；插件从缓存路径下载二进制、校验通过并可执行。
- `cross build --release --target aarch64-unknown-linux-gnu` 本机通过，说明 CI 的 linux arm64 交叉构建可行。
- **未做**：真实模型回合。本机没有 DeepSeek 凭据，因此 `agent/created` 注入是否出现在真实第一轮请求、`agent/turn-stopping` 的真实时序仍需有凭据时补测。macOS / Windows 的 release 构建只在 CI 中验证，本机未跑。

## 9. 未验证与风险

- DSH 装在临时前缀并做过加载验证，但没有真实模型回合；“`agent/created` 注入可靠赶在第一轮”来自源码事件契约，未做真实请求验证。
- DSH 处于 developer preview，`agent/created`、`agent.inject`、`agent/turn-stopping`、skill provider 签名都可能变。
- 官方文档存在命名漂移：cookbook 说没有 `agent/session-start`，而部分社区文章仍用旧名；以仓库源码为准。
- `@deepseek-ai/dsh-llm` / `dsh-skill` 的 npm `latest` 标签查询结果与 versions 列表不一致；锁版本时应以实际安装的 dsh 运行时解析结果为准。
- 插件市场、headless 运行、Windows 路径均未实测。

## 来源

- 仓库：https://github.com/deepseek-ai/deepseek-harness
- CLI 参考：https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/cli/reference/README.md
- Profile / bundle：https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/bundle/README.md
- Skills：https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/skills.md
- 插件市场：https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/guide/plugin-market.md
- Agent 生命周期：https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/agent-lifecycle.md
- Agent 事件签名：`packages/core/agent/src/runtime-types.ts`
- Shell 环境：https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/shell/shell-env/README.md
- CC hook 桥：https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/experimental/hooks-claude-code/README.md
- Codex hook 桥：https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/experimental/hooks-codex/README.md
- 扩展 cookbook：https://deepseek-harness.github.io/deepseek-harness/en/reference/cookbook/extension-cookbook
- pnpm Git 子目录：https://pnpm.io/package-sources

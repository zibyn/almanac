# DSH 用原生 Cordis 插件，不复用官方 Claude Code hook 桥

DeepSeek Harness（DSH）把「一切皆插件」做成了原生 API：插件导出 `apply(ctx)`，会话开始是 `agent/created`，自然停止是 `agent/turn-stopping`，skill 可以由 `ctx.skills.registerProvider` 随包提供。almanac 因此为 DSH 单独提供一个 bundle 包：`dsh/` 里注册 skill provider，在 `agent/created`（`startup|clear|compact`）运行 `almanac context` 并 `agent.inject`，在 `agent/turn-stopping` 运行 `almanac remind`，到点时 `agent.steer` 再走一步。用户一条 `dsh plugin add` 就能装齐三件事，不需要 hook 信任，也不会漏掉第一轮注入。

## Considered Options

- **复用 `@deepseek-ai/dsh-hooks-claude-code` 桥**：可以继续用现有 `hooks/hooks.json`，几乎不写新代码。但该包位于 `packages/experimental`；它把 `SessionStart` 当 detached 运行，context 可能错过第一个请求；`Stop` 的 `stop_hook_active` 始终为 `false`，需要 hook 自己限流；`configPath` 是进程级单文件，skill 也不随包安装。为了“少写 80 行”换来的是一串已知缺陷。
- **只提供 SKILL.md**：把 skill 放到 `~/.dsh/skills` 或 `~/.agents/skills`，不做注入与提醒。实现最省，但与其他宿主的体验不一致，用户还得手动复制文件。
- **只靠 MCP**：DSH 支持 MCP，但 server instructions 只适合放静态说明，放不了按项目变化的记忆；会话开始注入与提醒仍然缺失。

## Consequences

- 插件与 DSH 预览版 API 耦合。`dsh/package.json` 的 `engines.dsh` 与 peer 依赖写成已测试的小版本范围（当前 `>=0.2.1-rc.2 <0.3.0`），DSH 升级到 0.3 前必须重新验证。
- `dsh/skills/almanac/SKILL.md` 是 `plugins/almanac/skills/almanac/SKILL.md` 的逐字节副本：npm 打包不能引用包目录之外的文件。两份由 `tests/plugin.rs` 守住，改一处必须同步另一处。
- DSH 的 Writer 识别需要 Rust 端认 `DSH_SESSION_ID`；插件自己 spawn 的 `context` / `remind` 只读，不经过 DSH 的 shell-env，不需要 Writer。
- 提醒的续跑保护由插件按 session 维护：steer 后的下一轮把 `stop_hook_active` 传 `true`，再清除标记；`remind_every = 1` 也不会无限续跑。
- 不新增 DSH 的 MCP 手抄配置；DSH 的 MCP 配置位置与工具暴露方式尚未实测，等需要时另做调研。

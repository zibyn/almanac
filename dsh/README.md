# @zibyn/dsh-almanac

almanac 的 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 插件：`SKILL.md`、会话开始注入与写入提醒。需要 `PATH` 上有 `almanac` 二进制（见[主仓库](https://github.com/zibyn/almanac)）。

## 安装

```sh
dsh plugin --profile web add @zibyn/dsh-almanac
```

没有 npm 发布时，从 git 安装：

```sh
dsh plugin --profile web add github:zibyn/almanac#path:/dsh
```

`dsh plugin` 需要 `pnpm` 在 `PATH` 上。安装后重启 profile（或让 HMR 重载）。

## 它做什么

- 用 `ctx.skills.registerProvider` 随包提供 `almanac` skill，不需要另装 `~/.agents/skills`。
- `agent/created`（`startup|clear|compact`）运行 `almanac context`，把记忆注入会话。
- `agent/turn-stopping` 运行 `almanac remind`；到提醒间隔时用 `agent.steer` 交回模型。
- 子智能体不注入、不提醒。任何 `almanac` 失败都当作无输出，不阻塞会话。

## 兼容性

DSH 处于 developer preview。`engines.dsh` 与 peer 依赖声明已测试的 `>=0.2.1-rc.2 <0.3.0`；DSH 升级到 0.3 前需重新验证。

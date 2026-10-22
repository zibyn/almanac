# @zibyn/dsh-almanac

almanac 的 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 插件：`SKILL.md`、会话开始注入与写入提醒。

## 安装

从 [GitHub Releases](https://github.com/zibyn/almanac/releases) 取 `zibyn-dsh-almanac-<版本>.tgz`，然后：

```sh
dsh plugin --profile web add /path/to/zibyn-dsh-almanac-<版本>.tgz
```

也可以直接给 URL：

```sh
dsh plugin --profile web add https://github.com/zibyn/almanac/releases/download/v<版本>/zibyn-dsh-almanac-<版本>.tgz
```

没有 Release 产物时从 git 安装：

```sh
dsh plugin --profile web add github:zibyn/almanac#path:/dsh
```

`dsh plugin` 需要 `pnpm` 在 `PATH` 上。安装后重启 profile（或让 HMR 重载）。

## 二进制

插件按以下顺序找 `almanac`：

1. `ALMANAC_BIN` 环境变量；
2. `PATH` 上的 `almanac`；
3. `~/.cache/almanac/bin` 里已缓存的对应版本；
4. 从同一个 GitHub Release 下载 `almanac-<版本>-<target>`。

下载后用 `SHA256SUMS` 校验，再放到 `~/.cache/almanac/bin`（`XDG_CACHE_HOME` 优先）。缓存是单设备、多智能体共享的。下载失败就退回 `PATH`；都没有时不注册 skill，并在 DSH 日志里给出 warning。

- `ALMANAC_BIN=/path/to/almanac`：强制使用指定二进制。
- `ALMANAC_RELEASE_BASE=<url>`：替换 Release 下载前缀（镜像/离线环境）。

## 它做什么

- 用 `ctx.skills.registerProvider` 随包提供 `almanac` skill，不需要另装 `~/.agents/skills`。
- `agent/created`（`startup|clear|compact`）运行 `almanac context`，把记忆注入会话。
- `agent/turn-stopping` 运行 `almanac remind`；到提醒间隔时用 `agent.steer` 交回模型。
- 子智能体不注入、不提醒。任何 `almanac` 失败都当作无输出，不阻塞会话。
- `almanac` 在 `PATH` 和 Release 下载都不可用时，不注册 skill（否则模型会调用失败并去文件系统里找二进制）；DSH 日志会给出 warning。记忆库不存在则按「没有记忆」处理，第一次 `record` 会自动创建。

## 兼容性

DSH 处于 developer preview。`engines.dsh` 与 peer 依赖声明已测试的 `>=0.2.1-rc.2 <0.3.0`；DSH 升级到 0.3 前需重新验证。

# DSH 插件从 GitHub Release 下载二进制，缓存到 ~/.cache/almanac/bin

DSH 的插件管理器是 pnpm，用户期望「装插件就有完整功能」。只要求 `almanac` 在 `PATH` 上会让插件装好后仍然不可用；模型看到 skill 却调不到二进制时，会去文件系统里找。因此 DSH 走另一条分发路径：Release 里放一个平台无关的插件 tarball 和各平台的原始二进制，插件首次使用时按 `ALMANAC_BIN` → `PATH` → 缓存 → GitHub Release 的顺序解析二进制。下载取同 Release 的 `SHA256SUMS` 校验，通过后放到 `~/.cache/almanac/bin`（`XDG_CACHE_HOME/almanac/bin`），单设备、多智能体共享；失败退回 `PATH`，都不可用时不注册 skill。

## Considered Options

- **只要求 `PATH`（其他宿主的现状）**：实现最小，但 DSH 用户装完插件还要单独装二进制；本次故障就是这条路径的后果。
- **每个平台一个自带二进制的插件 tarball**：完全离线可用，但每次发版要发五份包，包体积大，二进制与插件版本强绑定；用户升级插件就得重下整包。
- **运行时从 GitHub Release 下载（选中）**：插件包保持约 10 KB，二进制按平台按需下载；代价是首次使用需要网络，且插件会执行下载来的代码，因此必须校验 SHA-256、失败时退回 `PATH`、并提供 `ALMANAC_RELEASE_BASE` 供镜像/离线环境替换。

## Consequences

- 发布流程必须同时产出插件 tarball、五个 target 的原始二进制和 `SHA256SUMS`；tag `v<version>` 必须与 `Cargo.toml`、`dsh/package.json`、`plugin.json` 一致。
- CI 增加 fmt、clippy、Rust/Node 测试、`npm pack` 校验；release workflow 在 `v*` tag 上构建 linux x64/arm64、macOS x64/arm64、Windows x64，并创建/更新 GitHub Release。
- 首次会话可能需要等待下载；下载在 `apply()` 时异步开始，skill 目录与注入共用同一个 Promise。下载失败不影响 DSH 启动。
- 缓存不随 DSH 或插件升级清理；文件名带版本与 target，旧版本可安全保留。需要强制使用指定二进制时设 `ALMANAC_BIN`。
- 模型通过 bash 调用 `almanac` 时走 shell 的 `PATH`。插件在缓存目录放一个稳定的 `almanac` 入口（Unix 符号链接，Windows 复制），把缓存目录前置到 DSH 进程的 `process.env.PATH`；DSH 的 shell 工具每次从 `process.env` 构造子进程环境，因此下载的二进制对模型就是普通的 `almanac` 命令，共享 SKILL.md 不需要宿主专属内容。
- CC / Codex / Pi 仍走 `PATH`，本次不改；它们没有 pnpm 式的依赖安装面。

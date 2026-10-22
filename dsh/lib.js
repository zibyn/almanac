// 纯逻辑，便于在没装 DSH 的情况下用 node --test 验证。

/** resume 不注入：历史里已有；与 CC / Codex 的 matcher 对应。 */
export function shouldInject(source) {
  return source === "startup" || source === "clear" || source === "compact";
}

/**
 * `almanac context` 的可注入输出。
 * 记忆库不存在时 context 会打印「almanac 不可用：…」；这行注入给模型会让它去找目录，
 * 所以按「没有记忆」处理，由插件记 warning。
 */
export function usableContext(text) {
  const trimmed = text.trim();
  return trimmed && !trimmed.startsWith("almanac 不可用：") ? trimmed : "";
}

/** Node 的 platform/arch → Rust target triple；不支持的平台返回 null。 */
export function targetFor(platform, arch) {
  const key = `${platform}-${arch}`;
  return {
    "linux-x64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "darwin-x64": "x86_64-apple-darwin",
    "darwin-arm64": "aarch64-apple-darwin",
    "win32-x64": "x86_64-pc-windows-msvc",
  }[key] ?? null;
}

/** Release 里的原始二进制文件名。 */
export function assetName(version, target) {
  return `almanac-${version}-${target}${target.includes("windows") ? ".exe" : ""}`;
}

/** Release 资产 URL；base 是仓库的 releases/download 前缀。 */
export function releaseUrl(base, version, asset) {
  return `${base.replace(/\/+$/, "")}/v${version}/${asset}`;
}

/** 从 SHA256SUMS 文本里取指定文件的摘要；没有返回 null。 */
export function checksumFor(text, asset) {
  for (const line of text.split(/\r?\n/)) {
    const match = /^([0-9a-f]{64})\s+\*?(.+)$/i.exec(line.trim());
    if (match && match[2] === asset) return match[1].toLowerCase();
  }
  return null;
}

/** `almanac remind` 的 stdout；解析失败或没有 reason 都当作不提醒。 */
export function parseReason(stdout) {
  try {
    const value = JSON.parse(stdout);
    return typeof value.reason === "string" ? value.reason : "";
  } catch {
    return "";
  }
}

/** SKILL.md frontmatter 的 description；取不到返回空串。 */
export function skillDescription(text) {
  const body = /^---\n([\s\S]*?)\n---\n/.exec(text)?.[1];
  const line = body?.split("\n").find((l) => l.startsWith("description:"));
  return line ? line.slice("description:".length).trim() : "";
}

/**
 * 每个 session 自己记录“上一轮 Stop 是我们 steer 出来的”。
 * 下一轮拿它当 stop_hook_active，防止 remind_every=1 时无限续跑。
 */
export function createSteerGuard() {
  const steered = new Set();
  return {
    take(session) {
      return steered.delete(session);
    },
    mark(session) {
      steered.add(session);
    },
  };
}

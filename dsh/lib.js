// 纯逻辑，便于在没装 DSH 的情况下用 node --test 验证。

/** resume 不注入：历史里已有；与 CC / Codex 的 matcher 对应。 */
export function shouldInject(source) {
  return source === "startup" || source === "clear" || source === "compact";
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

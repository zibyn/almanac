// almanac 的 Pi 扩展：会话开始注入（almanac context）与写入提醒（almanac remind），见 docs/spec/09-integration.md。
import { spawn } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// pi.exec 的 stdin 固定为 ignore，remind 要从 stdin 读 hook JSON，所以自己 spawn。
// 任何失败都当作无输出：注入与提醒永不阻塞会话。
function almanac(args: string[], cwd: string, input?: string): Promise<string> {
  return new Promise((resolve) => {
    let stdout = "";
    const child = spawn("almanac", args, { cwd, stdio: ["pipe", "pipe", "ignore"], timeout: 5000 });
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => (stdout += chunk));
    child.on("error", () => resolve(""));
    child.on("close", () => resolve(stdout));
    child.stdin.on("error", () => {});
    child.stdin.end(input ?? "");
  });
}

export default function (pi: ExtensionAPI) {
  let pending = false;

  // resume、fork、reload 不注入：历史里已有。
  pi.on("session_start", async (event) => {
    if (event.reason === "startup" || event.reason === "new") pending = true;
  });
  pi.on("session_compact", async () => {
    pending = true;
  });

  pi.on("before_agent_start", async (_event, ctx) => {
    if (!pending) return;
    pending = false;
    const text = (await almanac(["context"], ctx.cwd)).trim();
    if (!text) return;
    return { message: { customType: "almanac-context", content: text, display: false } };
  });

  pi.on("agent_before_settle", async (event, ctx) => {
    // 用户中断或出错时不提醒，与 CC / Codex 的 Stop 不触发对应。
    if (event.outcome !== "completed") return;
    const input = JSON.stringify({ session_id: ctx.sessionManager.getSessionId(), stop_hook_active: false });
    let reason: unknown;
    try {
      reason = JSON.parse(await almanac(["remind"], ctx.cwd, input)).reason;
    } catch {
      return;
    }
    if (typeof reason !== "string" || !reason) return;
    return {
      entries: [{ type: "custom_message", customType: "almanac-remind", content: reason, display: false }],
      continue: true,
    };
  });
}

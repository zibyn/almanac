// almanac 的 DeepSeek Harness 插件：SKILL.md、会话开始注入（almanac context）与写入提醒（almanac remind）。
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createUserMessage } from "@deepseek-ai/dsh-llm";
import { BUNDLED_SKILL_RANK } from "@deepseek-ai/dsh-skill";
import { cacheDir, ensureBinary, installShim, prependPath } from "./binary.js";
import { createSteerGuard, parseReason, shouldInject, skillDescription, usableContext } from "./lib.js";

const NAME = "almanac";
const SKILL_DIR = new URL("./skills/almanac/", import.meta.url);
const SKILL_FILE = new URL("SKILL.md", SKILL_DIR);
const RESOURCE_BASE = { kind: "directory", path: fileURLToPath(SKILL_DIR) };
const INVOCATION = { modelInvocable: true, userInvocable: true };
const NEVER = new AbortController().signal;

let skill;
function loadSkill() {
  skill ??= readFile(SKILL_FILE, "utf8").then((text) => ({
    text,
    description: skillDescription(text),
  }));
  return skill;
}

// 任何失败都当作无输出：注入与提醒永不阻塞会话。
function run(bin, args, cwd, input, signal) {
  return new Promise((resolve) => {
    let stdout = "";
    let child;
    try {
      child = spawn(bin, args, {
        cwd,
        stdio: ["pipe", "pipe", "ignore"],
        signal,
      });
    } catch {
      resolve("");
      return;
    }
    const timer = setTimeout(() => child.kill(), 5000);
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.on("error", () => {
      clearTimeout(timer);
      resolve("");
    });
    child.on("close", () => {
      clearTimeout(timer);
      resolve(stdout);
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input ?? "");
  });
}

function message(text, form) {
  return createUserMessage({
    content: [{ type: "text", text }],
    source: { kind: NAME, form },
  });
}

function isSubagent(agent) {
  return agent.session.header.origin === "subagent";
}

function cwdOf(ctx, agent, signal) {
  return ctx.workingDirectory.ensure(agent, signal ?? NEVER);
}

export const name = NAME;
// skills 供 provider 注册，workingDirectory 供 context 解析项目。
export const inject = ["skills", "workingDirectory"];

// 解析一次二进制，并在缓存目录放一个稳定的 almanac 入口、把它前置到 PATH。
// DSH 的 shell 工具每次从 process.env 构造子进程环境，所以模型的 bash 能直接调用 almanac。
let onPathReady;
function ensureOnPath(warn) {
  onPathReady ??= ensureBinary(warn).then(async (bin) => {
    if (!bin || bin === "almanac") return bin;
    try {
      const dir = cacheDir();
      await installShim(bin, dir);
      prependPath(process.env, dir);
    } catch (error) {
      warn(`almanac: 无法把二进制加入 PATH：${String(error)}`);
    }
    return bin;
  });
  return onPathReady;
}

export function apply(ctx) {
  const warn = (message) => ctx.logger.warn(message);
  // 启动时就解析/下载，skill 目录与第一轮注入共用同一个结果。
  void ensureOnPath(warn);
  let warned = false;
  const provider = {
    name: NAME,
    async list() {
      if (!(await ensureOnPath(warn))) {
        if (!warned) {
          warned = true;
          warn("almanac: 二进制不可用，不注册 skill");
        }
        return [];
      }
      const { description } = await loadSkill();
      return [{
        name: NAME,
        description,
        invocation: INVOCATION,
        provider: NAME,
        source: "bundled",
        resourceBase: RESOURCE_BASE,
        rank: BUNDLED_SKILL_RANK,
        locator: SKILL_FILE,
      }];
    },
    async get() {
      const { text, description } = await loadSkill();
      return {
        name: NAME,
        description,
        invocation: INVOCATION,
        provider: NAME,
        source: "bundled",
        resourceBase: RESOURCE_BASE,
        content: text,
      };
    },
  };
  ctx.skills.registerProvider(() => provider);

  // agent/created 是 serial 且被 await 的，注入赶在第一轮之前。
  ctx.on("agent/created", async ({ agent, source, signal }) => {
    if (!shouldInject(source) || isSubagent(agent)) return;
    try {
      const bin = await ensureOnPath(warn);
      if (!bin) return;
      const cwd = await cwdOf(ctx, agent, signal);
      const output = (await run(bin, ["context"], cwd, undefined, signal)).trim();
      const text = usableContext(output);
      if (!text) {
        if (output) warn(`almanac: 会话开始注入跳过：${output}`);
        return;
      }
      agent.inject(message(text, "recall"));
    } catch (error) {
      warn(`almanac: 会话开始注入失败：${String(error)}`);
    }
  });

  const guard = createSteerGuard();
  ctx.on("agent/turn-stopping", async ({ agent, signal }) => {
    if (isSubagent(agent)) return;
    const session = agent.session.header.id;
    const input = JSON.stringify({
      session_id: session,
      stop_hook_active: guard.take(session),
    });
    try {
      const bin = await ensureOnPath(warn);
      if (!bin) return;
      const cwd = await cwdOf(ctx, agent, signal);
      const reason = parseReason(await run(bin, ["remind"], cwd, input, signal));
      if (!reason) return;
      guard.mark(session);
      agent.steer(message(reason, "instructions"));
    } catch (error) {
      warn(`almanac: 写入提醒失败：${String(error)}`);
    }
  });
}

import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import test from "node:test";
import {
  createSteerGuard,
  parseReason,
  shouldInject,
  skillDescription,
} from "./lib.js";

test("resume 不注入，其余会话开始来源注入", () => {
  assert.equal(shouldInject("startup"), true);
  assert.equal(shouldInject("clear"), true);
  assert.equal(shouldInject("compact"), true);
  assert.equal(shouldInject("resume"), false);
  assert.equal(shouldInject(undefined), false);
});

test("remind 的 reason 解析", () => {
  assert.equal(parseReason('{"decision":"block","reason":"回顾"}'), "回顾");
  assert.equal(parseReason('{"decision":"block"}'), "");
  assert.equal(parseReason('{"reason":1}'), "");
  assert.equal(parseReason("not json"), "");
  assert.equal(parseReason(""), "");
});

test("steer 保护按 session 隔离且只生效一次", () => {
  const guard = createSteerGuard();
  assert.equal(guard.take("a"), false);
  guard.mark("a");
  assert.equal(guard.take("b"), false, "b 不受 a 影响");
  assert.equal(guard.take("a"), true);
  assert.equal(guard.take("a"), false, "取用后清除");
});

test("skill 描述取自 frontmatter", async () => {
  const text = await readFile(new URL("./skills/almanac/SKILL.md", import.meta.url), "utf8");
  const description = skillDescription(text);
  assert.ok(description.length > 0);
  assert.ok(description.length <= 1024);
  assert.equal(skillDescription("没有 frontmatter"), "");
});

import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import test from "node:test";
import {
  assetName,
  checksumFor,
  createSteerGuard,
  parseReason,
  releaseUrl,
  shouldInject,
  skillDescription,
  targetFor,
  usableContext,
} from "./lib.js";

test("resume 不注入，其余会话开始来源注入", () => {
  assert.equal(shouldInject("startup"), true);
  assert.equal(shouldInject("clear"), true);
  assert.equal(shouldInject("compact"), true);
  assert.equal(shouldInject("resume"), false);
  assert.equal(shouldInject(undefined), false);
});

test("记忆库不存在的错误行不注入", () => {
  assert.equal(usableContext("# almanac 记忆\n- x"), "# almanac 记忆\n- x");
  assert.equal(usableContext("  # almanac 记忆  "), "# almanac 记忆");
  assert.equal(usableContext("almanac 不可用：记忆库 /x 不存在"), "");
  assert.equal(usableContext(""), "");
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

test("平台映射与 Release 资产名", () => {
  assert.equal(targetFor("linux", "x64"), "x86_64-unknown-linux-gnu");
  assert.equal(targetFor("darwin", "arm64"), "aarch64-apple-darwin");
  assert.equal(targetFor("win32", "x64"), "x86_64-pc-windows-msvc");
  assert.equal(targetFor("freebsd", "x64"), null);
  assert.equal(
    assetName("0.1.0", "x86_64-unknown-linux-gnu"),
    "almanac-0.1.0-x86_64-unknown-linux-gnu",
  );
  assert.equal(
    assetName("0.1.0", "x86_64-pc-windows-msvc"),
    "almanac-0.1.0-x86_64-pc-windows-msvc.exe",
  );
  assert.equal(
    releaseUrl("https://example.test/releases/download/", "0.1.0", "a"),
    "https://example.test/releases/download/v0.1.0/a",
  );
});

test("SHA256SUMS 解析", () => {
  const sums = [
    `${"a".repeat(64)}  almanac-0.1.0-x86_64-unknown-linux-gnu`,
    `${"b".repeat(64)} *almanac-0.1.0-x86_64-pc-windows-msvc.exe`,
    "",
  ].join("\n");
  assert.equal(
    checksumFor(sums, "almanac-0.1.0-x86_64-unknown-linux-gnu"),
    "a".repeat(64),
  );
  assert.equal(
    checksumFor(sums, "almanac-0.1.0-x86_64-pc-windows-msvc.exe"),
    "b".repeat(64),
  );
  assert.equal(checksumFor(sums, "missing"), null);
});

test("skill 描述取自 frontmatter", async () => {
  const text = await readFile(new URL("./skills/almanac/SKILL.md", import.meta.url), "utf8");
  const description = skillDescription(text);
  assert.ok(description.length > 0);
  assert.ok(description.length <= 1024);
  assert.equal(skillDescription("没有 frontmatter"), "");
});

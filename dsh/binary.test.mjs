import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { cacheDir, downloadBinary, resolveBinary } from "./binary.js";
import { assetName, targetFor } from "./lib.js";

const require = createRequire(import.meta.url);
const { version } = require("./package.json");
const unix = process.platform !== "win32";

function serve(routes) {
  const server = createServer((req, res) => {
    const body = routes[req.url];
    if (body === undefined) {
      res.statusCode = 404;
      res.end();
      return;
    }
    res.end(body);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({
        server,
        base: `http://127.0.0.1:${server.address().port}`,
      });
    });
  });
}

async function tempDir(t) {
  const dir = await mkdtemp(join(tmpdir(), "almanac-bin-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  return dir;
}

test("缓存目录优先 XDG_CACHE_HOME", () => {
  assert.equal(
    cacheDir({ XDG_CACHE_HOME: "/x" }, "/home/me"),
    join("/x", "almanac", "bin"),
  );
  assert.equal(
    cacheDir({}, "/home/me"),
    join("/home/me", ".cache", "almanac", "bin"),
  );
});

test("ALMANAC_BIN 优先", { skip: !unix }, async (t) => {
  const dir = await tempDir(t);
  const bin = join(dir, "almanac");
  await writeFile(bin, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await chmod(bin, 0o755);
  const got = await resolveBinary({
    env: { ALMANAC_BIN: bin, PATH: "" },
    platform: process.platform,
    arch: process.arch,
    warn: () => {},
  });
  assert.equal(got, bin);
});

test("PATH 上有 almanac 时不下载", { skip: !unix }, async (t) => {
  const dir = await tempDir(t);
  const bin = join(dir, "almanac");
  await writeFile(bin, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await chmod(bin, 0o755);
  const got = await resolveBinary({
    env: { PATH: dir },
    platform: process.platform,
    arch: process.arch,
    warn: () => {},
  });
  assert.equal(got, "almanac");
});

test("下载并校验二进制", async (t) => {
  const dir = await tempDir(t);
  const target = "x86_64-unknown-linux-gnu";
  const asset = assetName(version, target);
  const bytes = "#!/bin/sh\necho ok\n";
  const hash = createHash("sha256").update(bytes).digest("hex");
  const { server, base } = await serve({
    [`/v${version}/${asset}`]: bytes,
    [`/v${version}/SHA256SUMS`]: `${hash}  ${asset}\n`,
  });
  t.after(() => server.close());

  const dest = join(dir, asset);
  assert.equal(await downloadBinary({ base, version, target, dest }), dest);
  assert.equal(await readFile(dest, "utf8"), bytes);
});

test("SHA-256 不匹配时不落盘", async (t) => {
  const dir = await tempDir(t);
  const target = "x86_64-unknown-linux-gnu";
  const asset = assetName(version, target);
  const { server, base } = await serve({
    [`/v${version}/${asset}`]: "payload",
    [`/v${version}/SHA256SUMS`]: `${"0".repeat(64)}  ${asset}\n`,
  });
  t.after(() => server.close());

  const dest = join(dir, asset);
  await assert.rejects(
    () => downloadBinary({ base, version, target, dest }),
    /SHA-256/,
  );
  await assert.rejects(() => readFile(dest), /ENOENT/);
});

test("PATH 没有时下载到缓存", { skip: !unix }, async (t) => {
  const target = targetFor(process.platform, process.arch);
  if (!target) return;
  const cache = await tempDir(t);
  const asset = assetName(version, target);
  const bytes = "#!/bin/sh\nexit 0\n";
  const hash = createHash("sha256").update(bytes).digest("hex");
  const { server, base } = await serve({
    [`/v${version}/${asset}`]: bytes,
    [`/v${version}/SHA256SUMS`]: `${hash}  ${asset}\n`,
  });
  t.after(() => server.close());

  const emptyPath = await tempDir(t);
  const got = await resolveBinary({
    env: {
      PATH: emptyPath,
      XDG_CACHE_HOME: cache,
      ALMANAC_RELEASE_BASE: base,
    },
    platform: process.platform,
    arch: process.arch,
    warn: () => {},
  });
  assert.equal(got, join(cache, "almanac", "bin", asset));
  assert.equal(await readFile(got, "utf8"), bytes);
});

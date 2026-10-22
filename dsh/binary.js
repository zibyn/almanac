// almanac 二进制的定位与下载：ALMANAC_BIN → PATH → 缓存 → GitHub Release。
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { access, chmod, copyFile, mkdir, readlink, rename, rm, stat, symlink, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { delimiter as pathDelimiter, dirname, join, resolve as resolvePath } from "node:path";
import { assetName, checksumFor, releaseUrl, targetFor } from "./lib.js";

const require = createRequire(import.meta.url);
const VERSION = require("./package.json").version;
const DEFAULT_BASE = "https://github.com/zibyn/almanac/releases/download";
const DOWNLOAD_TIMEOUT = 60_000;
const PATH_TIMEOUT = 2_000;

/** 单设备共享缓存；XDG_CACHE_HOME 优先，否则 ~/.cache。 */
export function cacheDir(env = process.env, home = homedir()) {
  const base = env.XDG_CACHE_HOME?.trim() || join(home, ".cache");
  return join(base, "almanac", "bin");
}

/**
 * 在缓存目录里放一个稳定的 `almanac` 入口，指向解析到的二进制。
 * Unix 用符号链接；Windows 用复制（创建符号链接通常需要额外权限）。
 */
export async function installShim(bin, dir) {
  if (bin === "almanac") return null;
  await mkdir(dir, { recursive: true });
  const dest = join(dir, process.platform === "win32" ? "almanac.exe" : "almanac");
  if (dest === bin) return dest;
  try {
    if ((await readlink(dest)) === bin) return dest;
  } catch {
    // 不存在或不是符号链接：重建。
  }
  await rm(dest, { force: true });
  try {
    await symlink(bin, dest);
  } catch {
    await copyFile(bin, dest);
  }
  await chmod(dest, 0o755).catch(() => {});
  return dest;
}

/** 把 dir 前置到 env.PATH，重复调用不会重复。 */
export function prependPath(env, dir, delimiter = pathDelimiter) {
  const key = Object.keys(env).find((k) => k.toUpperCase() === "PATH") ?? "PATH";
  const parts = (env[key] ?? "").split(delimiter).filter(Boolean);
  env[key] = [dir, ...parts.filter((p) => p !== dir)].join(delimiter);
  return env[key];
}

async function usable(path, platform = process.platform) {
  try {
    if (!(await stat(path)).isFile()) return false;
    if (platform !== "win32") await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** PATH 上是否有能启动的 almanac；返回命令名而不是绝对路径，交给 spawn 解析。 */
function onPath(env) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn("almanac", ["context"], {
        cwd: homedir(),
        env,
        stdio: "ignore",
      });
    } catch {
      resolve(false);
      return;
    }
    const timer = setTimeout(() => {
      child.kill();
      resolve(true);
    }, PATH_TIMEOUT);
    child.on("error", () => {
      clearTimeout(timer);
      resolve(false);
    });
    child.on("close", () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

function get(url, timeout) {
  return fetch(url, {
    redirect: "follow",
    signal: AbortSignal.timeout(timeout),
    headers: { "user-agent": "almanac-dsh-plugin" },
  });
}

/** 下载并校验一个平台的二进制；校验失败绝不落盘执行。 */
export async function downloadBinary({ base, version, target, dest, warn = () => {} }) {
  const asset = assetName(version, target);
  const url = releaseUrl(base, version, asset);
  const sumsUrl = releaseUrl(base, version, "SHA256SUMS");
  warn(`almanac: 下载 ${url}`);
  const [bin, sums] = await Promise.all([
    get(url, DOWNLOAD_TIMEOUT),
    get(sumsUrl, 10_000),
  ]);
  if (!bin.ok) throw new Error(`${url} 返回 ${bin.status}`);
  if (!sums.ok) throw new Error(`${sumsUrl} 返回 ${sums.status}`);
  const expected = checksumFor(await sums.text(), asset);
  if (!expected) throw new Error(`SHA256SUMS 中没有 ${asset}`);
  const bytes = Buffer.from(await bin.arrayBuffer());
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== expected) throw new Error(`${asset} SHA-256 不匹配`);

  await mkdir(dirname(dest), { recursive: true });
  const tmp = `${dest}.tmp-${process.pid}`;
  await writeFile(tmp, bytes, { mode: 0o755 });
  await chmod(tmp, 0o755).catch(() => {});
  try {
    await rename(tmp, dest);
  } catch (error) {
    await rm(tmp, { force: true });
    // 另一个会话可能刚下载完；只在目标可用时接受竞争结果。
    if (!(await usable(dest))) throw error;
  }
  return dest;
}

/** 解析一次二进制位置；不缓存，便于测试。 */
export async function resolveBinary({
  env = process.env,
  platform = process.platform,
  arch = process.arch,
  warn = () => {},
} = {}) {
  const declared = env.ALMANAC_BIN?.trim();
  if (declared) {
    const absolute = resolvePath(declared);
    if (await usable(absolute, platform)) return absolute;
    warn(`almanac: ALMANAC_BIN=${declared} 不可执行，继续查找`);
  }

  if (await onPath(env)) return "almanac";

  const target = targetFor(platform, arch);
  if (!target) {
    warn(`almanac: 没有 ${platform}-${arch} 的预编译二进制，且 PATH 上没有 almanac`);
    return null;
  }

  const asset = assetName(VERSION, target);
  const dest = join(cacheDir(env), asset);
  if (await usable(dest, platform)) return dest;

  const base = env.ALMANAC_RELEASE_BASE?.trim() || DEFAULT_BASE;
  try {
    return await downloadBinary({ base, version: VERSION, target, dest, warn });
  } catch (error) {
    warn(`almanac: 下载 ${asset} 失败：${error.message}；PATH 上没有可用的 almanac`);
    return null;
  }
}

let resolved;
/** 每个插件实例只解析一次。 */
export function ensureBinary(warn = () => {}) {
  resolved ??= resolveBinary({ warn });
  return resolved;
}

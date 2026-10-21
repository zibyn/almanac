//! The event log, the single source of truth (spec ch. 2): store location, replay, the locked write path.

use crate::error::ToolError;
use crate::model::{LOG_VERSION, Tx};
use crate::state::State;
use std::fs::{File, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_WAIT: Duration = Duration::from_secs(10);

/// Store directory: `ALMANAC_HOME`, else `$XDG_DATA_HOME/almanac`, else `$HOME/.local/share/almanac`;
/// `STORE_IO` when none of the three is set.
pub fn home() -> Result<PathBuf, ToolError> {
    let var = |k| std::env::var_os(k).filter(|v| !v.is_empty());
    if let Some(h) = var("ALMANAC_HOME") {
        return Ok(h.into());
    }
    let data = var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| {
            ToolError::new(
                "STORE_IO",
                "找不到记忆库：ALMANAC_HOME、XDG_DATA_HOME、HOME 都没有设置",
            )
            .hint("设置 ALMANAC_HOME")
        })?;
    Ok(data.join("almanac"))
}

/// Warns on stderr when the store sits in a common sync-folder path, where `flock` is
/// unreliable and conflict copies appear (spec ch. 2). Called once per process at startup.
// shortcut: matches well-known folder names only; NFS/SMB mounts are not detected.
pub fn warn_if_synced(home: &Path) {
    let synced = home.components().any(|c| {
        let c = c.as_os_str().to_string_lossy();
        [
            "Dropbox",
            "OneDrive",
            "Google Drive",
            "GoogleDrive",
            "Nextcloud",
            "ownCloud",
        ]
        .iter()
        .any(|p| c.starts_with(p))
            || ["iCloud Drive", "Mobile Documents", "CloudStorage"].contains(&&*c)
    });
    if synced {
        eprintln!(
            "almanac: 警告：记忆库 {} 似乎在同步盘里。同步盘上 flock 不可靠且会产生冲突副本，请把记忆库放在本地磁盘（ALMANAC_HOME）",
            home.display()
        );
    }
}

fn log_path(home: &Path) -> PathBuf {
    home.join("log/events.jsonl")
}

/// Replays every complete line; a missing log is an empty store. A last line without `\n`
/// is a write in progress and is ignored. Takes no lock.
pub fn replay(home: &Path) -> std::io::Result<Vec<Tx>> {
    let raw = match std::fs::read(log_path(home)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    Ok(parse(complete_lines(&raw)).txs)
}

/// Replays the store's log into derived state.
pub fn load(home: &Path) -> Result<State, ToolError> {
    Ok(State::replay(&replay(home).map_err(io_error)?))
}

/// `STORE_NEWER` if the log holds a line from a newer format version: the store is then
/// read-only (spec ch. 2 「版本」). `write` checks this itself; purge calls it after `lock`.
pub fn ensure_writable(home: &Path) -> Result<(), ToolError> {
    match std::fs::read(log_path(home)) {
        Ok(raw) if parse(complete_lines(&raw)).newer => Err(store_newer()),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(e)),
    }
}

fn store_newer() -> ToolError {
    ToolError::new("STORE_NEWER", "日志里有比本二进制新的格式版本，记忆库只读").hint("升级 almanac")
}

/// Everything up to and including the last `\n`.
fn complete_lines(raw: &[u8]) -> &[u8] {
    &raw[..raw.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1)]
}

struct Parsed {
    txs: Vec<Tx>,
    /// Some line has `v` above `LOG_VERSION`.
    newer: bool,
}

#[derive(serde::Deserialize)]
struct Version {
    v: u64,
}

fn parse(complete: &[u8]) -> Parsed {
    let mut p = Parsed {
        txs: vec![],
        newer: false,
    };
    for (n, line) in complete.split(|&b| b == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<Tx>(line) {
            Ok(tx) => {
                p.newer |= tx.v > LOG_VERSION;
                p.txs.push(tx);
            }
            // A newer format may reshape a line beyond what this binary can read: skip it quietly.
            Err(_) if serde_json::from_slice::<Version>(line).is_ok_and(|x| x.v > LOG_VERSION) => {
                p.newer = true;
            }
            Err(e) => eprintln!("almanac: skipping unreadable log line {}: {e}", n + 1),
        }
    }
    p
}

/// A held exclusive write lock on `<home>/lock`; dropping it releases the lock.
pub struct WriteLock(#[allow(dead_code)] File);

/// Takes the exclusive write lock, waiting up to 10 seconds, else `STORE_BUSY` (spec ch. 2).
/// Writers that do not append (purge) take this directly.
pub fn lock(home: &Path) -> Result<WriteLock, ToolError> {
    std::fs::create_dir_all(home).map_err(io_error)?;
    let f = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("lock"))
        .map_err(io_error)?;
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(WriteLock(f)),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(TryLockError::WouldBlock) => {
                return Err(ToolError::new("STORE_BUSY", "10 秒内拿不到写锁").hint("稍后重试"));
            }
            Err(TryLockError::Error(e)) => return Err(io_error(e)),
        }
    }
}

/// The write path (spec ch. 2 「本地并发」): lock, reopen the log, cut a torn last line, replay,
/// let `build` check its preconditions against that fresh state and return the transaction
/// (`None`: nothing to write) plus the tool's output, append it, fsync. `build` runs under the lock, so `Tx::new` stamps `at` while holding it.
pub fn write<T>(
    home: &Path,
    build: impl FnOnce(&State) -> Result<(Option<Tx>, T), ToolError>,
) -> Result<T, ToolError> {
    let _lock = lock(home)?;
    // Reopened under the lock: purge replaces the file by rename (spec ch. 2).
    let path = log_path(home);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(io_error)?;
    let mut f = File::options()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(io_error)?;
    let mut raw = vec![];
    f.read_to_end(&mut raw).map_err(io_error)?;
    let keep = without_torn_line(&raw).len();
    // A torn line is never valid JSON, so it cannot hide a newer `v`.
    let parsed = parse(&raw[..keep]);
    // Checked before repairing: a read-only store is not touched.
    if parsed.newer {
        return Err(store_newer());
    }
    if keep < raw.len() {
        warn_torn(raw.len() - keep);
        f.set_len(keep as u64).map_err(io_error)?;
    }
    let (tx, out) = build(&State::replay(&parsed.txs))?;
    let Some(tx) = tx else { return Ok(out) };
    let mut line = serde_json::to_vec(&tx).map_err(|e| io_error(e.into()))?;
    line.push(b'\n');
    f.write_all(&line).map_err(io_error)?;
    f.sync_all().map_err(io_error)?;
    Ok(out)
}

/// Purge's rewrite (spec ch. 2 「清除步骤」 2–5): `edit` sees every event of every line as raw JSON
/// and returns whether it changed it; changed lines are re-serialized, all others kept byte for byte.
/// Writes `events.jsonl.tmp`, fsyncs it, renames it over the log, fsyncs `log/`.
/// The caller holds `lock` and has checked `ensure_writable`.
pub fn rewrite(
    home: &Path,
    mut edit: impl FnMut(&mut serde_json::Value) -> bool,
) -> Result<(), ToolError> {
    let path = log_path(home);
    let raw = std::fs::read(&path).map_err(io_error)?;
    // We hold the lock, so a torn last line is a crash leftover: drop it, like `write` does.
    let keep = without_torn_line(&raw);
    if keep.len() < raw.len() {
        warn_torn(raw.len() - keep.len());
    }
    let mut out = Vec::with_capacity(keep.len());
    for line in keep.split_inclusive(|&b| b == b'\n') {
        let mut tx: serde_json::Value = match serde_json::from_slice(line) {
            Ok(v) => v,
            Err(_) => {
                out.extend_from_slice(line);
                continue;
            }
        };
        let mut changed = false;
        for ev in tx["events"].as_array_mut().into_iter().flatten() {
            changed |= edit(ev);
        }
        if changed {
            serde_json::to_writer(&mut out, &tx).map_err(|e| io_error(e.into()))?;
            out.push(b'\n');
        } else {
            out.extend_from_slice(line);
        }
    }
    let dir = path.parent().unwrap();
    let tmp = dir.join("events.jsonl.tmp");
    let mut f = File::create(&tmp).map_err(io_error)?;
    f.write_all(&out).map_err(io_error)?;
    f.sync_all().map_err(io_error)?;
    std::fs::rename(&tmp, &path).map_err(io_error)?;
    File::open(dir).and_then(|d| d.sync_all()).map_err(io_error)
}

/// Only the length: the cut line may hold a secret, and stderr can land in hook output.
fn warn_torn(bytes: usize) {
    eprintln!("almanac: 截掉日志末尾的残缺行（崩溃留下的不完整写入，{bytes} 字节）");
}

/// `raw` minus its last line if that line lacks `\n` or is not valid JSON (spec ch. 2).
fn without_torn_line(raw: &[u8]) -> &[u8] {
    let complete = complete_lines(raw);
    if complete.len() < raw.len() || raw.is_empty() {
        return complete;
    }
    let start = complete_lines(&raw[..raw.len() - 1]).len();
    match serde_json::from_slice::<serde::de::IgnoredAny>(&raw[start..]) {
        Ok(_) => raw,
        Err(_) => &raw[..start],
    }
}

// shortcut: spec ch. 10 has no code for I/O failures; reported as STORE_IO until the spec names one.
pub fn io_error(e: std::io::Error) -> ToolError {
    ToolError::new("STORE_IO", format!("读写记忆库失败：{e}"))
}

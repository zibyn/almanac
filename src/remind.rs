//! `almanac remind` (spec ch. 4): Stop-hook write reminder. Never fails: any error means no output.

use serde_json::{Value, json};
use std::io::Read;
use std::path::{Component, Path};

const DEFAULT_EVERY: u64 = 15;

const REASON: &str = "almanac：回顾本次会话，有没有以后还会用到、有用户原话或文件支撑的论断（约定、偏好、踩过的坑）？有就 `record`，情况变了用 `supersedes`；没有就什么都不做，直接结束，不要回复这条提醒。";

/// Always exits 0.
pub fn run() -> i32 {
    if let Some(true) = should_remind() {
        println!("{}", json!({"decision": "block", "reason": REASON}));
    }
    0
}

/// Counts this turn; `Some(true)` when it is the Nth.
fn should_remind() -> Option<bool> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).ok()?;
    let hook: Value = serde_json::from_str(&s).ok()?;
    let session = hook["session_id"].as_str()?;
    // It becomes a file name under cache/remind/, so it must be one plain path component.
    let mut parts = Path::new(session).components();
    if !matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    ) {
        return None;
    }
    if hook["stop_hook_active"] == true {
        return Some(false);
    }
    let home = crate::log::home().ok()?;
    if !home.is_dir() {
        return None;
    }
    let every = remind_every(&home).unwrap_or_else(|| {
        eprintln!(
            "almanac：config.toml 读不了或 remind_every 不是非负整数，按默认 {DEFAULT_EVERY} 提醒"
        );
        DEFAULT_EVERY
    });
    if every == 0 {
        return Some(false);
    }
    let dir = home.join("cache/remind");
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join(session);
    // A missing or garbled counter restarts from zero.
    let n = std::fs::read_to_string(&file)
        .ok()
        .and_then(|c| c.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    std::fs::write(&file, n.to_string()).ok()?;
    Some(n.is_multiple_of(every))
}

/// `remind_every` from `config.toml`, `DEFAULT_EVERY` when absent; `None` if unreadable or the value is not a non-negative int.
// shortcut: line-based `key = int` parse, not full TOML; switch to the `toml` crate when config grows.
fn remind_every(home: &Path) -> Option<u64> {
    let raw = match std::fs::read_to_string(home.join("config.toml")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(DEFAULT_EVERY),
        Err(_) => return None,
    };
    for line in raw.lines() {
        let line = line.split('#').next().unwrap_or("");
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == "remind_every"
        {
            return v.trim().parse().ok();
        }
    }
    Some(DEFAULT_EVERY)
}

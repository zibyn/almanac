//! The event log, the single source of truth (spec ch. 2): store location, replay, append.

use crate::model::Tx;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Store directory: `ALMANAC_HOME`, else `$XDG_DATA_HOME/almanac`, else `~/.local/share/almanac`.
pub fn home() -> PathBuf {
    let var = |k| std::env::var_os(k).filter(|v| !v.is_empty());
    if let Some(h) = var("ALMANAC_HOME") {
        return h.into();
    }
    let data = var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| {
        std::env::home_dir()
            .expect("neither ALMANAC_HOME nor HOME is set")
            .join(".local/share")
    });
    data.join("almanac")
}

fn log_path(home: &Path) -> PathBuf {
    home.join("log/events.jsonl")
}

/// Replays every complete line; a missing log is an empty store. A last line without `\n`
/// is a write in progress and is ignored.
pub fn replay(home: &Path) -> std::io::Result<Vec<Tx>> {
    let raw = match std::fs::read_to_string(log_path(home)) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let complete = &raw[..raw.rfind('\n').map_or(0, |i| i + 1)];
    let mut txs = vec![];
    for (n, line) in complete.lines().enumerate() {
        match serde_json::from_str(line) {
            Ok(tx) => txs.push(tx),
            Err(e) => eprintln!("almanac: skipping unreadable log line {}: {e}", n + 1),
        }
    }
    Ok(txs)
}

/// Appends one transaction as one line with a single `write()`, then fsyncs.
// shortcut: no flock / torn-line truncation yet; ticket #45 adds them here.
pub fn append(home: &Path, tx: &Tx) -> std::io::Result<()> {
    let path = log_path(home);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut line = serde_json::to_string(tx)?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(line.as_bytes())?;
    f.sync_all()
}

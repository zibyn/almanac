//! Source check for `file` evidence and the local check cache (spec ch. 6).

use crate::error::ToolError;
use crate::model::normalize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path};

/// Rejects absolute paths and `..` components before touching the disk.
// shortcut: spec says "contains `..`"; we reject `..` path components only, so `a..b.txt` is allowed.
pub fn check_path(path: &str) -> Result<(), ToolError> {
    let p = Path::new(path);
    if path.starts_with('/') || p.components().any(|c| c == Component::ParentDir) {
        return Err(ToolError::new(
            "FILE_OUTSIDE_PROJECT",
            format!("file 来源的 path {path} 不在项目根之内"),
        )
        .hint("项目外的文件按 command 记录（如 cat 的输出）"));
    }
    Ok(())
}

/// Write-time check: the excerpt must be in `<root>/<path>`. On success returns the cache entry.
pub fn check_file(root: &Path, path: &str, excerpt: &str) -> Result<Check, ToolError> {
    check_path(path)?;
    let full = root.join(path);
    let not_found = |e: std::io::Error| {
        ToolError::new("FILE_NOT_FOUND", format!("文件 {path} 不可读：{e}"))
            .hint("检查 path 是否相对项目根")
    };
    let meta = std::fs::metadata(&full).map_err(not_found)?;
    let bytes = std::fs::read(&full).map_err(not_found)?;
    if !normalize(&String::from_utf8_lossy(&bytes)).contains(&normalize(excerpt)) {
        return Err(
            ToolError::new("EXCERPT_NOT_IN_FILE", format!("摘录不在文件 {path} 中"))
                .hint("重新读取文件，逐字复制摘录"),
        );
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64);
    Ok(Check {
        result: "match".into(),
        at: crate::model::now(),
        mtime_ns,
        size: Some(meta.len()),
    })
}

/// One cached check result (spec ch. 6 "核验缓存"); the latest per evidence id.
#[derive(Serialize, Deserialize, Clone)]
pub struct Check {
    pub result: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

fn cache_path(home: &Path) -> std::path::PathBuf {
    home.join("cache/checks.json")
}

/// Reads the check cache; a missing or unreadable cache is empty (it is deletable).
pub fn load_cache(home: &Path) -> HashMap<String, Check> {
    std::fs::read(cache_path(home))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Merges `entries` into the cache via write-to-temp + rename.
// shortcut: concurrent cache writers can lose each other's entries; fine for a deletable cache until #42/#45 lock it.
pub fn store(
    home: &Path,
    entries: impl IntoIterator<Item = (String, Check)>,
) -> std::io::Result<()> {
    let mut cache = load_cache(home);
    cache.extend(entries);
    let path = cache_path(home);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&cache)?)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn match_records_mtime_and_size_and_round_trips_through_the_cache() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.txt"), "hello world").unwrap();
        let c = super::check_file(d.path(), "a.txt", "world").ok().unwrap();
        assert_eq!((c.result.as_str(), c.size), ("match", Some(11)));
        assert!(c.mtime_ns.is_some());
        super::store(d.path(), [("ev_1".to_string(), c)]).unwrap();
        let back = &super::load_cache(d.path())["ev_1"];
        assert_eq!((back.size, back.mtime_ns.is_some()), (Some(11), true));
    }
}

//! Source check, the local check cache and the stale flag (spec ch. 6).

use crate::error::ToolError;
use crate::model::normalize;
use crate::state::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
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
    let c = file_result(root, path, excerpt, None);
    match c.result.as_str() {
        "match" => Ok(c),
        "unreachable" => Err(ToolError::new(
            "FILE_NOT_FOUND",
            format!("文件 {path} 不存在或不可读"),
        )
        .hint("检查 path 是否相对项目根")),
        _ => Err(
            ToolError::new("EXCERPT_NOT_IN_FILE", format!("摘录不在文件 {path} 中"))
                .hint("重新读取文件，逐字复制摘录"),
        ),
    }
}

fn check(result: &str, mtime_ns: Option<u64>, size: Option<u64>) -> Check {
    Check {
        result: result.into(),
        at: crate::model::now(),
        mtime_ns,
        size,
    }
}

fn found(haystack: &str, excerpt: &str) -> bool {
    normalize(haystack).contains(&normalize(excerpt))
}

/// Checks a file excerpt; reuses `cached` without reading when mtime and size are unchanged.
fn file_result(root: &Path, path: &str, excerpt: &str, cached: Option<&Check>) -> Check {
    // Paths in a synced log are not trusted: never read outside the project root.
    if check_path(path).is_err() {
        return check("unreachable", None, None);
    }
    let full = root.join(path);
    let Ok(meta) = std::fs::metadata(&full) else {
        return check("unreachable", None, None);
    };
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64);
    let size = Some(meta.len());
    if let Some(c) = cached
        && c.mtime_ns.is_some()
        && (c.mtime_ns, c.size) == (mtime_ns, size)
    {
        return c.clone();
    }
    let Ok(bytes) = std::fs::read(&full) else {
        return check("unreachable", None, None);
    };
    let result = if found(&String::from_utf8_lossy(&bytes), excerpt) {
        "match"
    } else {
        "changed"
    };
    check(result, mtime_ns, size)
}

/// Fetches `url` once (spec ch. 6 「网页」) and looks for the excerpt.
fn web_result(url: &str, excerpt: &str) -> Check {
    let result = match fetch_text(url) {
        Err(_) => "unreachable",
        Ok(Some(text)) if found(&text, excerpt) => "match",
        Ok(_) => "changed",
    };
    check(result, None, None)
}

/// The page as text; `None` for a non-text type. Non-2xx, too many redirects, timeouts are errors.
fn fetch_text(url: &str) -> Result<Option<String>, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .max_redirects(5)
        .build()
        .into();
    let mut resp = agent.get(url).call()?;
    // shortcut: a missing Content-Type counts as text; revisit if spec defines it.
    let mime = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let html = mime == "text/html" || mime == "application/xhtml+xml";
    let text = mime.starts_with("text/") || mime.ends_with("json") || mime.ends_with("xml");
    if !text {
        return Ok(None);
    }
    // shortcut: bodies are decoded as UTF-8 (lossy); other charsets are not transcoded.
    let body = String::from_utf8_lossy(&resp.body_mut().read_to_vec()?).into_owned();
    Ok(Some(if html { html_text(&body) } else { body }))
}

/// Drops `<script>`/`<style>` with their content, comments and other tags; decodes entities.
/// Tags are removed without a space so inline markup inside CJK text keeps the text contiguous.
fn html_text(html: &str) -> String {
    use std::sync::LazyLock;
    static DROP: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?is)<script\b.*?</script\s*>|<style\b.*?</style\s*>|<!--.*?-->|<[^>]*>",
        )
        .unwrap()
    });
    html_escape::decode_html_entities(&DROP.replace_all(html, "")).into_owned()
}

pub type Cache = HashMap<String, Check>;

/// Where `file` evidence can be checked on this machine: the current project's root and alias group.
pub struct Here {
    root: std::path::PathBuf,
    group: HashSet<String>,
}

pub fn here(s: &State, p: Option<&crate::project::Project>) -> Option<Here> {
    p.map(|p| Here {
        root: p.root.clone(),
        group: s.alias_group(&p.id),
    })
}

/// Checks each evidence in `ids` once if its source type is in `types`, stores the results in
/// the cache, and returns the updated cache, the results in order, and the URLs requested.
/// `file` evidence of a project not located here is skipped (its cached result stays).
pub fn refresh<'a>(
    home: &Path,
    s: &State,
    here: Option<&Here>,
    ids: impl IntoIterator<Item = &'a str>,
    types: &[&str],
) -> (Cache, Vec<(String, Check)>, Vec<String>) {
    let mut cache = load_cache(home);
    let (mut checked, mut net) = (Vec::<(String, Check)>::new(), vec![]);
    for id in ids {
        let Some((e, _)) = s.evidence.get(id) else {
            continue;
        };
        let src = &e.source;
        let ty = src["type"].as_str().unwrap_or("");
        if !types.contains(&ty) || checked.iter().any(|(c, _)| c == id) {
            continue;
        }
        let c = match ty {
            "file" => match here {
                Some(h) if src["project"].as_str().is_some_and(|p| h.group.contains(p)) => {
                    let path = src["path"].as_str().unwrap_or("");
                    file_result(&h.root, path, &e.excerpt, cache.get(id))
                }
                _ => continue,
            },
            "web" => {
                let url = src["url"].as_str().unwrap_or("");
                net.push(url.to_string());
                web_result(url, &e.excerpt)
            }
            "conversation" | "command" => check("unverifiable", None, None),
            _ => continue,
        };
        cache.insert(id.to_string(), c.clone());
        checked.push((id.to_string(), c));
    }
    // The cache is a deletable local observation; failing to write it must not fail a read.
    if !checked.is_empty()
        && let Err(e) = store(home, checked.clone())
    {
        eprintln!("almanac: failed to write check cache: {e}");
    }
    (cache, checked, net)
}

/// Spec ch. 6 「可能过期」: evidence of the latest evidence-bearing event whose cached result is
/// `changed` or `unreachable`; `null` when there is none.
// shortcut: purged evidence is not excluded yet; #44 (purge) adds that.
pub fn stale(s: &State, cache: &Cache, memory: &str) -> Value {
    let latest = match s.corroborations.get(memory).and_then(|c| c.last()) {
        Some((ev, _)) => ev,
        None => &s.memories[memory].0.evidence,
    };
    let bad: Vec<Value> = latest
        .iter()
        .filter_map(|e| {
            let c = cache.get(e)?;
            matches!(c.result.as_str(), "changed" | "unreachable")
                .then(|| json!({ "evidence": e, "result": c.result }))
        })
        .collect();
    if bad.is_empty() {
        Value::Null
    } else {
        json!(bad)
    }
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
pub fn load_cache(home: &Path) -> Cache {
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

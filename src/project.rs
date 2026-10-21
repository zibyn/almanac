//! Project resolution (spec ch. 1 "作用域与项目标识"): path → project identity, root, root commit.

use crate::error::ToolError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

pub struct Project {
    /// Project identity, e.g. `github.com/zibyn/almanac`, `git-root:<hash>`, `path:<abs>`.
    pub id: String,
    /// Project root: the repo top level, or the path itself outside git.
    pub root: PathBuf,
    /// Lexicographically smallest root commit; `None` outside git or before the first commit.
    pub root_commit: Option<String>,
    /// HEAD commit at resolution time; `None` outside git or before the first commit.
    pub head: Option<String>,
}

/// The MCP client's root for the current tool call, set by `almanac mcp`; `None` under the CLI.
pub static MCP_ROOT: Mutex<Option<String>> = Mutex::new(None);

/// Locates the project path: argument `project` → MCP `roots/list` → `CLAUDE_PROJECT_DIR` → cwd.
pub fn resolve(arg: Option<&str>) -> Result<Project, ToolError> {
    let env = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|v| !v.is_empty());
    let root = MCP_ROOT.lock().unwrap().clone();
    let path = match arg.map(String::from).or(root).or(env) {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir()
            .map_err(|e| ToolError::invalid_args(format!("无法读取当前目录：{e}")))?,
    };
    let path = path
        .canonicalize()
        .map_err(|e| ToolError::invalid_args(format!("项目路径 {} 不可用：{e}", path.display())))?;
    from_path(&path)
}

fn from_path(path: &Path) -> Result<Project, ToolError> {
    let Some(top) = git(path, &["rev-parse", "--show-toplevel"]) else {
        return Ok(Project {
            id: format!("path:{}", path.display()),
            root: path.to_path_buf(),
            root_commit: None,
            head: None,
        });
    };
    let root = PathBuf::from(top);
    let head = git(&root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let root_commit = head.as_ref().and_then(|_| {
        git(&root, &["rev-list", "--max-parents=0", "HEAD"])
            .and_then(|out| out.lines().min().map(String::from))
    });
    let id = match (git(&root, &["remote", "get-url", "origin"]), &root_commit) {
        (Some(url), _) => normalize_remote(&url),
        (None, Some(c)) => format!("git-root:{c}"),
        // shortcut: spec has no identity for a repo with neither origin nor commits; treated like a non-git path.
        (None, None) => format!("path:{}", root.display()),
    };
    Ok(Project {
        id,
        root,
        root_commit,
        head,
    })
}

/// Runs git in `dir`; `None` when it fails or prints nothing.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    let out = String::from_utf8(o.stdout).ok()?.trim().to_string();
    (o.status.success() && !out.is_empty()).then_some(out)
}

/// Drops the scheme and `user@`, turns scp-style `host:path` into `host/path`, drops a trailing `.git`.
fn normalize_remote(url: &str) -> String {
    let (rest, scp) = match url.split_once("://") {
        Some((_, r)) => (r, false),
        None => (url, true),
    };
    let rest = match rest.split_once('@') {
        Some((user, r)) if !user.contains('/') => r,
        _ => rest,
    };
    let rest = if scp {
        rest.replacen(':', "/", 1)
    } else {
        rest.to_string()
    };
    let rest = rest.trim_end_matches('/');
    rest.strip_suffix(".git").unwrap_or(rest).to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_remote as n;

    #[test]
    fn remote_spellings_normalize_to_one_identity() {
        let want = "github.com/zibyn/almanac";
        assert_eq!(n("git@github.com:zibyn/almanac.git"), want);
        assert_eq!(n("https://github.com/zibyn/almanac"), want);
        assert_eq!(n("https://github.com/zibyn/almanac.git"), want);
        assert_eq!(n("ssh://git@github.com/zibyn/almanac.git"), want);
        assert_eq!(n("https://user:tok@github.com/zibyn/almanac/"), want);
    }
}

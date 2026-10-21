mod common;
use common::Almanac;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs git in `dir` with a fixed identity; returns trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// A temp git repo with one commit; returns (dir, root commit).
fn repo(origin: Option<&str>) -> (tempfile::TempDir, String) {
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    std::fs::write(
        d.path().join("README.md"),
        "构建：cargo build --features cjk\n",
    )
    .unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "init"]);
    if let Some(o) = origin {
        git(d.path(), &["remote", "add", "origin", o]);
    }
    let root = git(d.path(), &["rev-parse", "HEAD"]);
    (d, root)
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Records one project-scope memory backed by conversation evidence; returns the record response.
fn record_project(a: &Almanac, project: Option<&Path>) -> Value {
    let mut args = json!({
        "evidence": [{"ref": "e", "excerpt": "记住：用 cargo build", "source": {"type": "conversation"}}],
        "memories": [{"text": "构建用 cargo build", "scope": "project", "evidence": ["e"]}]
    });
    if let Some(p) = project {
        args["project"] = json!(s(p));
    }
    a.tool("record", args).ok()
}

fn get(a: &Almanac, id: &Value) -> Value {
    a.tool("get", json!({"ids": [id]})).ok()["records"][0].clone()
}

#[test]
fn project_memory_records_identity_root_commit_and_git_commit() {
    let (d, root) = repo(Some("https://github.com/zibyn/almanac"));
    let a = Almanac::new();
    let r = record_project(&a, Some(d.path()));
    let m = get(&a, &r["memories"][0]["id"]);
    assert_eq!(m["scope"], "project");
    assert_eq!(m["project"], "github.com/zibyn/almanac");
    assert_eq!(m["root_commit"], root.as_str());
    assert_eq!(m["recorded"]["git_commit"], root.as_str());
    assert_eq!(a.log()[0]["git_commit"], root.as_str());
}

fn project_of(a: &Almanac, project: Option<&Path>) -> Value {
    let r = record_project(a, project);
    get(a, &r["memories"][0]["id"])["project"].clone()
}

#[test]
fn scp_and_https_origins_are_the_same_project() {
    let (d, _) = repo(Some("git@github.com:zibyn/almanac.git"));
    assert_eq!(
        project_of(&Almanac::new(), Some(d.path())),
        "github.com/zibyn/almanac"
    );
}

#[test]
fn repo_without_origin_is_identified_by_smallest_root_commit() {
    let (d, first) = repo(None);
    git(d.path(), &["checkout", "-q", "--orphan", "other"]);
    std::fs::write(d.path().join("b.txt"), "b").unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "other root"]);
    let second = git(d.path(), &["rev-parse", "HEAD"]);
    git(
        d.path(),
        &[
            "merge",
            "-q",
            "--allow-unrelated-histories",
            "-m",
            "m",
            "main",
        ],
    );
    let min = first.min(second);

    let a = Almanac::new();
    let sub = d.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let r = record_project(&a, Some(&sub));
    let m = get(&a, &r["memories"][0]["id"]);
    assert_eq!(m["project"], format!("git-root:{min}"));
    assert_eq!(m["root_commit"], min.as_str());
}

#[test]
fn path_outside_git_is_identified_by_absolute_path() {
    let d = tempfile::tempdir().unwrap();
    let real: PathBuf = d.path().canonicalize().unwrap();
    let a = Almanac::new();
    let r = record_project(&a, Some(d.path()));
    let m = get(&a, &r["memories"][0]["id"]);
    assert_eq!(m["project"], format!("path:{}", real.display()));
    assert!(m.get("root_commit").is_none(), "{m}");
    assert!(a.log()[0].get("git_commit").is_none());
}

#[test]
fn project_path_falls_back_to_claude_project_dir_then_cwd() {
    let (by_env, _) = repo(Some("https://example.com/env"));
    let (by_cwd, _) = repo(Some("https://example.com/cwd"));
    let (by_arg, _) = repo(Some("https://example.com/arg"));
    let a = Almanac::new().cwd(by_cwd.path());
    assert_eq!(project_of(&a, None), "example.com/cwd");
    let a = a.env("CLAUDE_PROJECT_DIR", s(by_env.path()));
    assert_eq!(project_of(&a, None), "example.com/env");
    assert_eq!(project_of(&a, Some(by_arg.path())), "example.com/arg");
}

fn record_file(a: &Almanac, project: &Path, path: &str, excerpt: &str) -> common::Out {
    a.tool(
        "record",
        json!({
            "project": s(project),
            "evidence": [{"ref": "f", "excerpt": excerpt, "source": {"type": "file", "path": path, "line": 1}}],
            "memories": [{"text": "构建要带 cjk feature", "scope": "project", "evidence": ["f"]}]
        }),
    )
}

#[test]
fn file_evidence_is_checked_against_the_file_and_cached_as_match() {
    let (d, _) = repo(Some("https://github.com/zibyn/almanac"));
    let a = Almanac::new();
    // Whitespace runs fold and NFC-equivalent text matches ("e\u{301}" vs "é").
    std::fs::write(
        d.path().join("NOTES.md"),
        "caf\u{e9}  构建：\n  cargo build\n",
    )
    .unwrap();
    let r = record_file(
        &a,
        d.path(),
        "NOTES.md",
        " cafe\u{301} 构建： cargo   build",
    )
    .ok();
    let ev = get(&a, &r["evidence"][0]["id"]);
    assert_eq!(
        ev["source"],
        json!({"type": "file", "project": "github.com/zibyn/almanac", "path": "NOTES.md", "line": 1})
    );
    assert_eq!(ev["check"]["result"], "match");
    assert!(ev["check"]["at"].is_string(), "{ev}");
    let m = get(&a, &r["memories"][0]["id"]);
    assert_eq!(m["epistemic"], "inferred");
}

#[test]
fn file_evidence_outside_project_is_rejected() {
    let (d, _) = repo(None);
    let a = Almanac::new();
    for p in ["/etc/hostname", "../x/README.md", "a/../README.md"] {
        let e = record_file(&a, d.path(), p, "cargo build").err("FILE_OUTSIDE_PROJECT");
        assert!(e["hint"].is_string());
    }
    assert_eq!(a.log_raw(), "");
}

#[test]
fn missing_file_or_excerpt_is_rejected() {
    let (d, _) = repo(None);
    let a = Almanac::new();
    record_file(&a, d.path(), "NOPE.md", "cargo build").err("FILE_NOT_FOUND");
    let e =
        record_file(&a, d.path(), "README.md", "cargo build --release").err("EXCERPT_NOT_IN_FILE");
    assert!(e["hint"].is_string());
    assert_eq!(a.log_raw(), "");
}

#[test]
fn project_memory_dedup_spans_the_alias_group() {
    let (old, _) = repo(Some("https://github.com/old/repo"));
    let (new, _) = repo(Some("https://github.com/new/repo"));
    let (other, _) = repo(Some("https://github.com/other/repo"));
    let a = Almanac::new();
    let first = record_project(&a, Some(old.path()));
    let id = &first["memories"][0]["id"];
    // Unaliased project: same text is a separate memory.
    assert_ne!(
        &record_project(&a, Some(other.path()))["memories"][0]["id"],
        id
    );
    let o = a.run(
        &[
            "project",
            "alias",
            "github.com/old/repo",
            "github.com/new/repo",
        ],
        None,
    );
    assert_eq!(o.code, 0, "{}", o.stderr);
    // Aliased project, same text and evidence: a retry, deduped to the old memory.
    let again = record_project(&a, Some(new.path()));
    assert_eq!(&again["memories"][0]["id"], id);
    assert_eq!(again["memories"][0]["deduped"], true);
    // Same text, different evidence: DUPLICATE_TEXT.
    let e = a
        .tool(
            "record",
            json!({
                "project": s(new.path()),
                "evidence": [{"ref": "e", "excerpt": "另一句话", "source": {"type": "conversation"}}],
                "memories": [{"text": "构建用 cargo build", "scope": "project", "evidence": ["e"]}]
            }),
        )
        .err("DUPLICATE_TEXT");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains(id.as_str().unwrap()),
        "{e}"
    );
}

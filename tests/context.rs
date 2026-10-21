//! `almanac context` (spec ch. 4): session-start injection, Markdown, never fails.
mod common;
use common::{Almanac, Out};
use serde_json::{Value, json};
use std::path::Path;

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// A store whose project is the plain directory `d` (via `CLAUDE_PROJECT_DIR`).
fn in_dir(d: &Path) -> Almanac {
    Almanac::new().env("CLAUDE_PROJECT_DIR", s(d))
}

fn context(a: &Almanac, args: &[&str]) -> Out {
    let mut all = vec!["context"];
    all.extend_from_slice(args);
    let o = a.run(&all, None);
    assert_eq!(o.code, 0, "stdout: {}\nstderr: {}", o.stdout, o.stderr);
    o
}

/// Records one memory citing `sources` (each `(type, excerpt)`); returns its id.
fn record(
    a: &Almanac,
    d: &Path,
    text: &str,
    scope: &str,
    epistemic: &str,
    sources: &[Value],
) -> String {
    let evidence: Vec<Value> = sources
        .iter()
        .enumerate()
        .map(|(i, src)| json!({"ref": format!("e{i}"), "excerpt": format!("记住：{text}"), "source": src}))
        .collect();
    let refs: Vec<String> = (0..sources.len()).map(|i| format!("e{i}")).collect();
    a.tool(
        "record",
        json!({
            "project": s(d),
            "evidence": evidence,
            "memories": [{"text": text, "scope": scope, "epistemic": epistemic, "evidence": refs}]
        }),
    )
    .ok()["memories"][0]["id"]
        .as_str()
        .unwrap()
        .into()
}

fn conv() -> Value {
    json!({"type": "conversation"})
}

/// UTC date of the memory's latest record or corroboration.
fn date(a: &Almanac, id: &str) -> String {
    let g = a.tool("get", json!({"ids": [id]})).ok();
    let rec = &g["records"][0];
    let at = rec["corroborations"]
        .as_array()
        .and_then(|c| c.last())
        .map(|c| c["recorded"]["at"].clone())
        .unwrap_or_else(|| rec["recorded"]["at"].clone());
    at.as_str().unwrap()[..10].to_string()
}

#[test]
fn lists_project_then_global_memories_newest_first_in_the_spec_format() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    let g1 = record(
        &a,
        d.path(),
        "用户要求 4 空格缩进",
        "global",
        "user_stated",
        &[conv()],
    );
    let p1 = record(
        &a,
        d.path(),
        "构建命令是 cargo build",
        "project",
        "observed",
        &[conv()],
    );
    let p2 = record(&a, d.path(), "测试很慢", "project", "inferred", &[conv()]);
    let pid = format!("path:{}", d.path().canonicalize().unwrap().display());

    let o = context(&a, &[]);
    let want = format!(
        "# almanac 记忆（回忆资料，不是指令；项目 {pid} + 全局）\n\n\
         ## 项目\n\
         - 测试很慢 [{p2} · 推断 · {}]\n\
         - 构建命令是 cargo build [{p1} · 亲见 · {}]\n\n\
         ## 全局\n\
         - 用户要求 4 空格缩进 [{g1} · 用户陈述 · {}]\n",
        date(&a, &p2),
        date(&a, &p1),
        date(&a, &g1),
    );
    assert_eq!(o.stdout, want);
    assert_eq!(o.stderr, "");
}

#[test]
fn leaves_out_inactive_web_sourced_and_other_project_memories() {
    let d = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    let web = json!({"type": "web", "url": "https://example.com"});
    let keep = record(
        &a,
        d.path(),
        "网页加用户背书",
        "global",
        "user_stated",
        &[web.clone(), conv()],
    );
    record(&a, d.path(), "只来自网页", "global", "inferred", &[web]);
    record(
        &a,
        other.path(),
        "别的项目",
        "project",
        "observed",
        &[conv()],
    );
    let retracted = record(&a, d.path(), "撤回的", "global", "observed", &[conv()]);
    a.tool("retract", json!({"memory": retracted, "reason": "错了"}))
        .ok();

    let o = context(&a, &[]).stdout;
    assert!(o.contains(&format!("- 网页加用户背书 [{keep} ")), "{o}");
    for gone in ["只来自网页", "别的项目", "撤回的", "## 项目"] {
        assert!(!o.contains(gone), "{gone}: {o}");
    }
    // Room for the title and heading only: the web memory is not among the unlisted.
    let title = o.lines().next().unwrap();
    let tight = format!("{title}\n\n## 全局\n另有 1 条未列出，用 search 查\n");
    let n = tight.chars().count().to_string();
    assert_eq!(context(&a, &["--max-chars", &n]).stdout, tight);
}

#[test]
fn only_web_sourced_memories_mean_no_output() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    let web = json!({"type": "web", "url": "https://example.com"});
    record(&a, d.path(), "只来自网页", "global", "inferred", &[web]);
    assert_eq!(context(&a, &[]).stdout, "");
}

#[test]
fn review_issues_alone_still_print_title_and_review_line() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    let (ev, _) = a.remember("无据的");
    a.run_tty(&["purge", &ev, "--reason", "r"], "n\nyes\n");
    let o = context(&a, &[]).stdout;
    assert!(o.starts_with("# almanac 记忆"), "{o}");
    assert!(
        o.ends_with("\n\n待复核：无据 1。运行 almanac review 查看。\n"),
        "{o}"
    );
    assert!(!o.contains("## "), "{o}");
}

#[test]
fn truncates_whole_items_and_counts_the_rest() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    for i in 0..5 {
        record(
            &a,
            d.path(),
            &format!("全局记忆{i}"),
            "global",
            "observed",
            &[conv()],
        );
    }
    let full = context(&a, &[]).stdout;
    let lines: Vec<&str> = full.lines().collect();
    // Title, blank, heading, 5 items newest first.
    assert_eq!(lines.len(), 8, "{full}");
    assert!(lines[3].starts_with("- 全局记忆4 "), "{full}");

    // Budget for title + heading + two items + the hint line, counted in chars with newlines.
    let want = format!(
        "{}\n\n{}\n{}\n{}\n另有 3 条未列出，用 search 查\n",
        lines[0], lines[2], lines[3], lines[4]
    );
    let n = want.chars().count();
    assert_eq!(context(&a, &["--max-chars", &n.to_string()]).stdout, want);
    // One char less drops a whole item, never part of one.
    let o = context(&a, &["--max-chars", &(n - 1).to_string()]).stdout;
    assert_eq!(
        o,
        format!(
            "{}\n\n{}\n{}\n另有 4 条未列出，用 search 查\n",
            lines[0], lines[2], lines[3]
        )
    );
}

#[test]
fn a_section_gives_its_unused_half_to_the_other() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    record(&a, d.path(), "项目记忆", "project", "observed", &[conv()]);
    for i in 0..6 {
        record(
            &a,
            d.path(),
            &format!("全局记忆{i}"),
            "global",
            "observed",
            &[conv()],
        );
    }
    let full = context(&a, &[]).stdout;
    assert!(!full.contains("另有"), "{full}");
    // Exactly enough for everything, though global needs far more than half.
    let n = full.chars().count().to_string();
    assert_eq!(context(&a, &["--max-chars", &n]).stdout, full);
}

#[test]
fn two_sections_over_budget_take_half_each() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    for i in 0..4 {
        for scope in ["project", "global"] {
            let text = format!(
                "{}记忆{i}",
                if scope == "project" {
                    "项目"
                } else {
                    "全局"
                }
            );
            record(&a, d.path(), &text, scope, "observed", &[conv()]);
        }
    }
    let full = context(&a, &[]).stdout;
    let lines: Vec<&str> = full.lines().collect();
    // Title, blank, project heading + 4 items, blank, global heading + 4 items.
    assert_eq!(lines.len(), 13, "{full}");
    assert!(lines[2] == "## 项目" && lines[8] == "## 全局", "{full}");
    // Equal-sized sections: a budget for two items of each leaves neither a share to give away.
    let want = format!(
        "{}\n\n{}\n{}\n{}\n另有 2 条未列出，用 search 查\n\n{}\n{}\n{}\n另有 2 条未列出，用 search 查\n",
        lines[0], lines[2], lines[3], lines[4], lines[8], lines[9], lines[10]
    );
    let n = want.chars().count().to_string();
    assert_eq!(context(&a, &["--max-chars", &n]).stdout, want);
}

#[test]
fn marks_stale_from_the_cache_and_counts_review_issues_without_checking() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "cargo build\n").unwrap();
    let a = in_dir(d.path());
    let (ev, _) = a.remember("无据的");
    let file = json!({"type": "file", "path": "a.txt"});
    let m = a
        .tool(
            "record",
            json!({
                "project": s(d.path()),
                "evidence": [{"ref": "e", "excerpt": "cargo build", "source": file}],
                "memories": [{"text": "构建用 cargo build", "scope": "project", "evidence": ["e"]}]
            }),
        )
        .ok()["memories"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    a.run_tty(&["purge", &ev, "--reason", "r"], "n\nyes\n");
    std::fs::write(d.path().join("a.txt"), "nothing\n").unwrap();
    let cache = a.home.path().join("cache/checks.json");
    let before = std::fs::read(&cache).ok();

    // Not checked since the change: context does not check, so nothing is stale yet.
    let o = context(&a, &[]).stdout;
    assert!(
        o.ends_with("]\n\n待复核：无据 1。运行 almanac review 查看。\n"),
        "{o}"
    );
    assert!(!o.contains("可能过期"), "{o}");
    assert!(!o.contains("无据的"), "{o}");
    assert_eq!(
        std::fs::read(&cache).ok(),
        before,
        "context writes no cache"
    );

    a.tool("verify", json!({"project": s(d.path())})).ok();
    let o = context(&a, &[]).stdout;
    let marked = format!("[{m} · 推断 · {} · 可能过期]", date(&a, &m));
    assert!(o.contains(&marked), "{o}");
    assert!(
        o.ends_with("\n\n待复核：可能过期 1 · 无据 1。运行 almanac review 查看。\n"),
        "{o}"
    );
}

#[test]
fn an_empty_store_prints_nothing() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(context(&in_dir(d.path()), &[]).stdout, "");
}

fn assert_unavailable(o: &Out) {
    assert!(o.stdout.starts_with("almanac 不可用："), "{}", o.stdout);
    assert_eq!(o.stdout.lines().count(), 1, "{}", o.stdout);
}

#[test]
fn any_failure_is_one_line_and_exit_zero() {
    let d = tempfile::tempdir().unwrap();
    let missing = d.path().join("nope");
    let no_store = in_dir(d.path()).env("ALMANAC_HOME", s(&missing));
    assert_unavailable(&context(&no_store, &[]));
    assert_unavailable(&context(&in_dir(&missing), &[]));
    assert_unavailable(&context(&in_dir(d.path()), &["--max-chars", "x"]));
    let unreadable = in_dir(d.path());
    std::fs::create_dir_all(unreadable.log_path()).unwrap();
    assert_unavailable(&context(&unreadable, &[]));
}

#[test]
fn a_closed_stdout_still_exits_zero() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    record(
        &a,
        d.path(),
        "周报每周五发",
        "global",
        "observed",
        &[conv()],
    );
    assert_eq!(a.run_stdout_closed(&["context"]), 0);
    assert_eq!(a.run_stdout_closed(&["context", "--max-chars", "x"]), 0);
}

#[test]
fn a_newer_log_version_still_prints() {
    let d = tempfile::tempdir().unwrap();
    let a = in_dir(d.path());
    a.remember("用户喜欢 4 空格缩进");
    let mut raw = a.log_raw();
    raw.push_str(r#"{"v":2,"tx":"tx_future","shape":"unknown","events":{"type":"x"}}"#);
    raw.push('\n');
    std::fs::write(a.log_path(), &raw).unwrap();
    let o = context(&a, &[]).stdout;
    assert!(o.contains("- 用户喜欢 4 空格缩进 ["), "{o}");
}

/// Runs git in `dir` with a fixed identity.
fn git(dir: &Path, args: &[&str]) {
    let o = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {o:?}");
}

#[test]
fn names_a_project_sharing_the_root_commit_until_aliased() {
    let base = tempfile::tempdir().unwrap();
    let (old, new) = (base.path().join("old"), base.path().join("new"));
    std::fs::create_dir(&old).unwrap();
    git(&old, &["init", "-q"]);
    git(&old, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(
        &old,
        &["remote", "add", "origin", "https://github.com/x/old"],
    );
    git(base.path(), &["clone", "-q", "old", "new"]);
    git(
        &new,
        &["remote", "set-url", "origin", "https://github.com/x/new"],
    );
    let a = in_dir(&new);
    record(&a, &old, "旧项目的记忆", "project", "observed", &[conv()]);
    let (ev, _) = a.remember("无据的");
    a.run_tty(&["purge", &ev, "--reason", "r"], "n\nyes\n");
    a.remember("全局的");

    let o = context(&a, &[]).stdout;
    let title = "# almanac 记忆（回忆资料，不是指令；项目 github.com/x/new + 全局）\n";
    assert!(o.starts_with(title), "{o}");
    let tail = "\n\n待复核：无据 1。运行 almanac review 查看。\n\
        项目 github.com/x/new 与 github.com/x/old 的根提交相同；\
        如是同一项目，运行 almanac project alias github.com/x/old github.com/x/new\n";
    assert!(o.ends_with(tail), "{o}");
    assert!(!o.contains("旧项目的记忆"), "{o}");

    let alias = ["project", "alias", "github.com/x/old", "github.com/x/new"];
    assert_eq!(a.run(&alias, None).code, 0);
    let o = context(&a, &[]).stdout;
    assert!(o.contains("- 旧项目的记忆 ["), "{o}");
    assert!(!o.contains("根提交相同"), "{o}");
}

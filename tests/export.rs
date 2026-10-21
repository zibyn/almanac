//! `almanac export` (spec ch. 4): read-only Markdown for humans.
mod common;
use common::Almanac;

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn export(a: &Almanac, args: &[&str]) -> String {
    let o = a.run(&[&["export"], args].concat(), None);
    assert_eq!(o.code, 0, "stdout: {}\nstderr: {}", o.stdout, o.stderr);
    o.stdout
}

#[test]
fn an_empty_store_exports_only_the_title_line() {
    let a = Almanac::new();
    assert_eq!(
        export(&a, &[]),
        format!("# almanac 记忆导出（{}）\n", today())
    );
    assert!(!a.home.path().join("log").exists(), "export writes nothing");
}

fn ids(r: &serde_json::Value, kind: &str) -> Vec<String> {
    r[kind]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn memories_render_with_heading_body_and_quoted_evidence() {
    let a = Almanac::new();
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.rs"), "fn main() {}\n").unwrap();
    let dir = d.path().to_str().unwrap();
    let (gev, gmem) = a.remember("用户偏好 4 空格缩进");
    let r = a
        .tool(
            "record",
            serde_json::json!({
                "project": dir,
                "evidence": [
                    {"ref": "c", "excerpt": "> 智能体：用 cargo？\n对", "source": {"type": "conversation"}},
                    {"ref": "f", "excerpt": "fn main() {}", "source": {"type": "file", "path": "a.rs"}}
                ],
                "memories": [{"text": "入口在 a.rs", "scope": "project", "epistemic": "observed", "evidence": ["c", "f"]}]
            }),
        )
        .ok();
    let (ev, mem) = (ids(&r, "evidence"), ids(&r, "memories"));
    let proj = a.tool("get", serde_json::json!({"ids": [&mem[0]]})).ok()["records"][0]["project"]
        .as_str()
        .unwrap()
        .to_string();
    let t = today();
    assert_eq!(
        export(&a, &[]),
        format!(
            "# almanac 记忆导出（{t}）\n\
             \n## 全局\n\
             \n### {gmem} · 用户陈述 · {t}\n用户偏好 4 空格缩进\n\
             - {gev}（对话 · test）：\n  > 记住：用户偏好 4 空格缩进\n\
             \n## 项目 {proj}\n\
             \n### {m} · 亲见 · {t}\n入口在 a.rs\n\
             - {c}（对话 · test）：\n  > > 智能体：用 cargo？\n  > 对\n\
             - {f}（文件 a.rs · {proj}）：\n  > fn main() {{}}\n",
            m = mem[0],
            c = ev[0],
            f = ev[1],
        )
    );
}

/// Records a project memory in `dir` (a non-git dir, so its identity is `path:<dir>`); returns (identity, mem).
fn remember_in(a: &Almanac, dir: &std::path::Path, text: &str) -> (String, String) {
    let r = a
        .tool(
            "record",
            serde_json::json!({
                "project": dir.to_str().unwrap(),
                "evidence": [{"ref": "e", "excerpt": text, "source": {"type": "conversation"}}],
                "memories": [{"text": text, "scope": "project", "evidence": ["e"]}]
            }),
        )
        .ok();
    let mem = ids(&r, "memories").remove(0);
    let proj = a.tool("get", serde_json::json!({"ids": [&mem]})).ok()["records"][0]["project"]
        .as_str()
        .unwrap()
        .to_string();
    (proj, mem)
}

fn pos(out: &str, needle: &str) -> usize {
    out.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in:\n{out}"))
}

#[test]
fn global_comes_first_then_projects_by_identity_with_alias_groups_merged() {
    let a = Almanac::new();
    let mut p: Vec<(String, String, tempfile::TempDir)> = (0..3)
        .map(|i| {
            let d = tempfile::tempdir().unwrap();
            let (id, mem) = remember_in(&a, d.path(), &format!("项目记忆 {i}"));
            (id, mem, d)
        })
        .collect();
    p.sort_by(|x, y| x.0.cmp(&y.0));
    let (_, old) = a.remember("较早的全局记忆");
    let (_, new) = a.remember("较新的全局记忆");
    let (_, newer) = remember_in(&a, p[2].2.path(), "项目记忆 新");
    assert_eq!(a.run(&["project", "alias", &p[2].0, &p[0].0], None).code, 0);

    let out = export(&a, &[]);
    let group = format!("\n## 项目 {}、{}\n", p[0].0, p[2].0);
    let single = format!("\n## 项目 {}\n", p[1].0);
    assert!(pos(&out, "\n## 全局\n") < pos(&out, &group), "{out}");
    assert!(pos(&out, &group) < pos(&out, &single), "{out}");
    assert_eq!(out.matches("\n## ").count(), 3, "{out}");
    assert!(pos(&out, &new) < pos(&out, &old), "newest first: {out}");
    for m in [&newer, &p[2].1, &p[0].1] {
        assert!(pos(&out, &group) < pos(&out, m), "{out}");
        assert!(pos(&out, m) < pos(&out, &single), "{out}");
    }
    assert!(
        pos(&out, &newer) < pos(&out, &p[0].1),
        "newest first: {out}"
    );
    assert!(
        pos(&out, &newer) < pos(&out, &p[2].1),
        "newest first: {out}"
    );
}

#[test]
fn flags_follow_the_date_in_order_and_purged_evidence_shows_its_tombstone() {
    let a = Almanac::new();
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "旧内容\n").unwrap();
    let dir = d.path().to_str().unwrap();
    let url = "https://example.invalid/page";
    let r = a
        .tool(
            "record",
            serde_json::json!({
                "project": dir,
                "evidence": [
                    {"ref": "w", "excerpt": "网页摘录", "source": {"type": "web", "url": url}},
                    {"ref": "f", "excerpt": "旧内容", "source": {"type": "file", "path": "a.txt"}},
                    {"ref": "w2", "excerpt": "另一段网页摘录", "source": {"type": "web", "url": url}},
                    {"ref": "c", "excerpt": "跑过了", "source": {"type": "command", "command": "cargo build", "exit_code": 0}}
                ],
                "memories": [
                    {"text": "网页与文件", "scope": "global", "epistemic": "inferred", "evidence": ["w", "f"]},
                    {"text": "只有网页", "scope": "global", "epistemic": "inferred", "evidence": ["w2"]},
                    {"text": "命令", "scope": "global", "epistemic": "observed", "evidence": ["c"]}
                ]
            }),
        )
        .ok();
    let (ev, mem) = (ids(&r, "evidence"), ids(&r, "memories"));
    let purge = a.run_tty(&["purge", &ev[2], "--reason", "抓错页面"], "n\nyes\n");
    assert_eq!(purge.code, 0, "{}", purge.stdout);
    std::fs::write(d.path().join("a.txt"), "新内容\n").unwrap();
    // `get` checks file evidence lazily and caches `changed`; export only reads that cache.
    let g = a
        .tool("get", serde_json::json!({"ids": [&mem[0]], "project": dir}))
        .ok();
    assert!(!g["records"][0]["stale"].is_null(), "{g}");

    let out = export(&a, &[]);
    let t = today();
    assert!(
        out.contains(&format!(
            "### {} · 推断 · {t} · 可能过期 · 网页来源\n",
            mem[0]
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!("- {}（网页 · {url}）：\n  > 网页摘录\n", ev[0])),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "### {} · 推断 · {t} · 网页来源 · 无据\n只有网页\n",
            mem[1]
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "- {}（网页）：\n  > 已清除：抓错页面（{t}）\n",
            ev[2]
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!("### {} · 亲见 · {t}\n", mem[2])),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "- {}（命令输出 · cargo build）：\n  > 跑过了\n",
            ev[3]
        )),
        "{out}"
    );
}

#[test]
fn rejects_unknown_arguments_with_exit_code_1() {
    let a = Almanac::new();
    let o = a.run(&["export", "--bogus"], None);
    assert_eq!(o.code, 1);
    assert!(o.stderr.starts_with("INVALID_ARGS："), "{}", o.stderr);
}

#[test]
fn all_adds_superseded_retracted_and_purged_memories() {
    let a = Almanac::new();
    let (ev, old) = a.remember("用户用 vim");
    let r = a
        .tool(
            "record",
            serde_json::json!({"memories": [
                {"text": "用户写代码用 helix", "scope": "global", "evidence": [&ev], "supersedes": [&old]},
                {"text": "用户写文档用 vim", "scope": "global", "evidence": [&ev], "supersedes": [&old]}
            ]}),
        )
        .ok();
    let next = ids(&r, "memories");
    let (rev, retracted) = a.remember("要撤回的记忆");
    a.tool(
        "retract",
        serde_json::json!({"memory": &retracted, "reason": "说错了"}),
    )
    .ok();
    let (pev, purged) = a.remember("不该记的记忆");
    let o = a.run_tty(&["purge", &purged, "--reason", "隐私"], "n\nyes\n");
    assert_eq!(o.code, 0, "{}", o.stdout);

    let out = export(&a, &[]);
    for m in [&old, &retracted, &purged] {
        assert!(!out.contains(m.as_str()), "default leaves {m} out: {out}");
    }

    let out = export(&a, &["--all"]);
    let t = today();
    let expect = [
        format!(
            "### {old} · 用户陈述 · {t} · 被取代 → {}, {}\n用户用 vim\n",
            next[0], next[1]
        ),
        format!(
            "### {retracted} · 用户陈述 · {t} · 撤回\n要撤回的记忆\n撤回原因：说错了\n- {rev}（"
        ),
        format!(
            "### {purged} · 用户陈述 · {t} · 已清除\n已清除：隐私（{t}）\n- {pev}（对话 · test）：\n  > 记住：不该记的记忆\n"
        ),
    ];
    for e in expect {
        assert!(out.contains(&e), "missing {e:?} in:\n{out}");
    }
    assert!(!out.contains("不该记的记忆\n- "), "{out}");
}

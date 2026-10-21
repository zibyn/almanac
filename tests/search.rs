mod common;
use common::Almanac;
use serde_json::{Value, json};

fn ids(r: &Value) -> Vec<&str> {
    r["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect()
}

#[test]
fn chinese_query_finds_memory_with_its_fields() {
    let a = Almanac::new();
    let (_, mem) = a.remember("构建命令是 cargo build --release");
    a.remember("用户喜欢 4 空格缩进");

    let r = a.tool("search", json!({"query": "构建命令"})).ok();
    assert_eq!(ids(&r), [mem.as_str()]);
    let h = &r["hits"][0];
    assert_eq!(h["kind"], "memory");
    assert_eq!(h["text"], "构建命令是 cargo build --release");
    assert_eq!(h["scope"], "global");
    assert!(h.get("project").is_none(), "{h}");
    assert_eq!(h["status"], "active");
    assert_eq!(h["epistemic"], "user_stated");
    assert_eq!(h["writer"], "test");
    assert!(h["last_at"].as_str().unwrap().ends_with('Z'), "{h}");
    assert_eq!(h["evidence_count"], 1);
    assert_eq!(h["source_types"], json!(["conversation"]));
    assert_eq!(h["stale"], Value::Null);
    assert!(h["score"].as_f64().unwrap() > 0.0, "{h}");
    assert_eq!(h["matched_terms"], json!(["构建", "命令", "建命"]));
    assert!(r.get("alias_hint").is_none());
}

#[test]
fn bigrams_recall_what_jieba_missplits_and_identifiers_match_by_part() {
    let a = Almanac::new();
    let (_, gap) = a.remember("请求缺字段时返回 400");
    let (_, spawn) = a.remember("后台任务用 tokio::spawn 起");
    let (_, camel) = a.remember("查用户走 getUserById");

    let r = a.tool("search", json!({"query": "字段"})).ok();
    assert_eq!(ids(&r), [gap.as_str()]);
    assert_eq!(r["hits"][0]["matched_terms"], json!(["字段"]));

    let r = a.tool("search", json!({"query": "spawn"})).ok();
    assert_eq!(ids(&r), [spawn.as_str()]);
    let r = a.tool("search", json!({"query": "tokio::spawn"})).ok();
    assert_eq!(
        r["hits"][0]["matched_terms"],
        json!(["tokio::spawn", "tokio", "spawn"])
    );
    let r = a.tool("search", json!({"query": "ＵＳＥＲ"})).ok();
    assert_eq!(ids(&r), [camel.as_str()]);
}

#[test]
fn bm25_scores_terms_double_bigrams() {
    // Worked by hand: one doc, tf 1, length = avgdl, so each matching field contributes
    // weight × idf with idf = ln(1 + 0.5 / 1.5) = ln(4/3).
    let a = Almanac::new();
    a.remember("foo");
    let r = a.tool("search", json!({"query": "foo"})).ok();
    assert!(
        (r["hits"][0]["score"].as_f64().unwrap() - 0.575_364).abs() < 1e-5,
        "{r}"
    );

    let a = Almanac::new();
    a.remember("请求缺字段时返回");
    let r = a.tool("search", json!({"query": "字段"})).ok();
    assert!(
        (r["hits"][0]["score"].as_f64().unwrap() - 0.287_682).abs() < 1e-5,
        "{r}"
    );
}

#[test]
fn or_semantics_rank_more_matches_higher() {
    let a = Almanac::new();
    let (_, one) = a.remember("部署 用 docker");
    let (_, both) = a.remember("部署 前 先 跑 cargo test");
    a.remember("无关的 记忆");
    let r = a.tool("search", json!({"query": "cargo 部署"})).ok();
    assert_eq!(ids(&r), [both.as_str(), one.as_str()]);
}

#[test]
fn equal_scores_order_by_recency_then_id() {
    let a = Almanac::new();
    let (_, old) = a.remember("foo alpha");
    let (_, new) = a.remember("foo bravo");
    let r = a.tool("search", json!({"query": "foo"})).ok();
    assert_eq!(ids(&r), [new.as_str(), old.as_str()]);
    assert_eq!(r["hits"][0]["score"], r["hits"][1]["score"]);

    // Same transaction, same time: id descending.
    let b = Almanac::new();
    let rec = b
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "e", "excerpt": "记住两条", "source": {"type": "conversation"}}],
                "memories": [
                    {"text": "foo one", "scope": "global", "evidence": ["e"]},
                    {"text": "foo two", "scope": "global", "evidence": ["e"]}
                ]
            }),
        )
        .ok();
    let mut want: Vec<&str> = rec["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    want.sort_unstable_by(|x, y| y.cmp(x));
    assert_eq!(ids(&b.tool("search", json!({"query": "foo"})).ok()), want);
    assert_eq!(ids(&b.tool("search", json!({})).ok()), want);
}

#[test]
fn matched_terms_follow_query_order() {
    let a = Almanac::new();
    a.remember("tokio 请求缺字段时返回");
    let r = a
        .tool("search", json!({"query": "字段 nothing tokio"}))
        .ok();
    assert_eq!(r["hits"][0]["matched_terms"], json!(["字段", "tokio"]));
}

#[test]
fn query_without_tokens_returns_nothing() {
    let a = Almanac::new();
    a.remember("foo");
    for q in ["，。！", "", "  ::  "] {
        let r = a.tool("search", json!({"query": q})).ok();
        assert_eq!(r["hits"], json!([]), "{q:?}");
    }
}

#[test]
fn without_query_lists_newest_first_with_zero_score() {
    let a = Almanac::new();
    let (_, m1) = a.remember("第一条");
    let (_, m2) = a.remember("第二条");
    let (_, m3) = a.remember("第三条");
    let r = a.tool("search", json!({})).ok();
    assert_eq!(ids(&r), [m3.as_str(), m2.as_str(), m1.as_str()]);
    for h in r["hits"].as_array().unwrap() {
        assert_eq!(h["score"], 0.0);
        assert_eq!(h["matched_terms"], json!([]));
    }
}

#[test]
fn limit_defaults_to_10_and_must_be_1_to_50() {
    let a = Almanac::new();
    for i in 0..12 {
        a.remember(&format!("记忆 {i}"));
    }
    let n = |r: Value| r["hits"].as_array().unwrap().len();
    assert_eq!(n(a.tool("search", json!({})).ok()), 10);
    assert_eq!(
        n(a.tool("search", json!({"limit": 1, "query": "记忆"})).ok()),
        1
    );
    assert_eq!(n(a.tool("search", json!({"limit": 50})).ok()), 12);
    for bad in [json!(0), json!(51), json!(-1), json!("5")] {
        a.tool("search", json!({"limit": bad})).err("INVALID_ARGS");
    }
    a.tool("search", json!({"scope": "everywhere"}))
        .err("INVALID_ARGS");
    a.tool("search", json!({"bogus": 1})).err("INVALID_ARGS");
}

/// A project-scope memory in non-git dir `dir` (identity `path:<dir>`); returns its id.
fn remember_in(a: &Almanac, dir: &std::path::Path, text: &str) -> String {
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "e", "excerpt": format!("记住：{text}"), "source": {"type": "conversation"}}],
                "memories": [{"text": text, "scope": "project", "evidence": ["e"]}],
                "project": dir.to_str().unwrap()
            }),
        )
        .ok();
    r["memories"][0]["id"].as_str().unwrap().into()
}

fn identity(d: &tempfile::TempDir) -> String {
    format!("path:{}", d.path().canonicalize().unwrap().display())
}

#[test]
fn scope_and_all_filter_by_current_project_and_alias_group() {
    let [d1, d2, d3] = [(); 3].map(|_| tempfile::tempdir().unwrap());
    let a = Almanac::new();
    let (_, g) = a.remember("全局 构建");
    let p1 = remember_in(&a, d1.path(), "项目一 构建");
    let p2 = remember_in(&a, d2.path(), "项目二 构建");
    let p3 = remember_in(&a, d3.path(), "项目三 构建");
    let at = |scope: &str, all: bool| {
        let args = json!({"query": "构建", "scope": scope, "all": all, "project": d1.path().to_str().unwrap()});
        let mut v: Vec<String> = ids(&a.tool("search", args).ok())
            .into_iter()
            .map(String::from)
            .collect();
        v.sort();
        v
    };
    let set = |v: &[&String]| {
        let mut v: Vec<String> = v.iter().map(|s| s.to_string()).collect();
        v.sort();
        v
    };
    assert_eq!(at("global", false), set(&[&g]));
    assert_eq!(at("project", false), set(&[&p1]));
    assert_eq!(at("any", false), set(&[&g, &p1]));
    assert_eq!(at("project", true), set(&[&p1, &p2, &p3]));
    assert_eq!(at("any", true), set(&[&g, &p1, &p2, &p3]));
    assert_eq!(at("global", true), set(&[&g]));

    let r = a
        .tool(
            "search",
            json!({"query": "项目一", "project": d1.path().to_str().unwrap()}),
        )
        .ok();
    assert_eq!(r["hits"][0]["project"], identity(&d1));

    let alias = a.run(&["project", "alias", &identity(&d2), &identity(&d1)], None);
    assert_eq!(alias.code, 0, "{}", alias.stderr);
    assert_eq!(at("project", false), set(&[&p1, &p2]));
}

#[test]
fn only_active_memories_by_default() {
    let a = Almanac::new();
    let (_, gone) = a.remember("foo 撤回");
    let (_, kept) = a.remember("foo 保留");
    a.tool("retract", json!({"memory": gone, "reason": "记错了"}))
        .ok();
    assert_eq!(
        ids(&a.tool("search", json!({"query": "foo"})).ok()),
        [kept.as_str()]
    );
    assert_eq!(ids(&a.tool("search", json!({})).ok()), [kept.as_str()]);
}

#[test]
fn corroboration_bumps_last_at_and_evidence_count() {
    let a = Almanac::new();
    let (_, old) = a.remember("foo 旧的");
    let (ev, new) = a.remember("foo 新的");
    a.tool("corroborate", json!({"memory": old, "evidence": [ev]}))
        .ok();
    let r = a.tool("search", json!({})).ok();
    assert_eq!(ids(&r), [old.as_str(), new.as_str()]);
    assert_eq!(r["hits"][0]["evidence_count"], 2);
    assert!(
        r["hits"][0]["last_at"].as_str() > r["hits"][1]["last_at"].as_str(),
        "{r}"
    );
    let r = a.tool("search", json!({"query": "foo"})).ok();
    assert_eq!(ids(&r), [old.as_str(), new.as_str()]);
}

/// Sorted hit ids of `search` with `args`.
fn sorted_ids(a: &Almanac, args: Value) -> Vec<String> {
    let mut v: Vec<String> = ids(&a.tool("search", args).ok())
        .into_iter()
        .map(String::from)
        .collect();
    v.sort();
    v
}

fn sorted(v: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = v.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

#[test]
fn status_epistemic_writer_and_cites_filter_memories() {
    let a = Almanac::new();
    let (ev, stated) = a.remember("foo 用户说的");
    let (_, gone) = a.remember("foo 撤回的");
    a.tool("retract", json!({"memory": gone, "reason": "记错了"}))
        .ok();
    let a = a.env("ALMANAC_WRITER", "other");
    let rec = a
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "c", "excerpt": "跑了 foo", "source": {"type": "command", "command": "foo", "exit_code": 0}}],
                "memories": [{"text": "foo 推断的", "scope": "global", "epistemic": "inferred", "evidence": ["c"]}]
            }),
        )
        .ok();
    let inferred = rec["memories"][0]["id"].as_str().unwrap();
    let q = |extra: Value| {
        let mut args = json!({"query": "foo"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        sorted_ids(&a, args)
    };

    assert_eq!(q(json!({"status": ["retracted"]})), sorted(&[&gone]));
    assert_eq!(
        q(json!({"status": ["active", "retracted"]})),
        sorted(&[&stated, &gone, inferred])
    );
    assert_eq!(q(json!({"epistemic": ["inferred"]})), sorted(&[inferred]));
    assert_eq!(
        q(json!({"epistemic": ["user_stated", "observed"]})),
        sorted(&[&stated])
    );
    assert_eq!(q(json!({"writer": "other"})), sorted(&[inferred]));
    assert_eq!(q(json!({"writer": "test"})), sorted(&[&stated]));
    assert_eq!(q(json!({"cites": ev})), sorted(&[&stated]));
    assert_eq!(q(json!({"cites": "ev_nope"})), sorted(&[]));
    // Without a query the filters still apply.
    assert_eq!(
        sorted_ids(&a, json!({"status": ["retracted"]})),
        sorted(&[&gone])
    );

    for bad in [
        json!({"status": ["gone"]}),
        json!({"status": "active"}),
        json!({"epistemic": ["guessed"]}),
    ] {
        a.tool("search", bad).err("INVALID_ARGS");
    }
}

fn hit<'a>(r: &'a Value, id: &str) -> &'a Value {
    r["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in {r}"))
}

#[test]
fn evidence_search_returns_evidence_hits_across_the_store() {
    let d = tempfile::tempdir().unwrap();
    let a = Almanac::new();
    let (conv, mem) = a.remember("构建 用 cargo");
    let long = "构建".repeat(150);
    let rec = a
        .tool(
            "record",
            json!({
                "evidence": [
                    {"ref": "w", "excerpt": long, "source": {"type": "web", "url": "https://example.com"}},
                    {"ref": "c", "excerpt": "无关的 输出", "source": {"type": "command", "command": "ls", "exit_code": 0}}
                ],
                "memories": [{"text": "网页说 构建", "scope": "project", "evidence": ["w", "c"]}],
                "project": d.path().to_str().unwrap()
            }),
        )
        .ok();
    let web = rec["evidence"][0]["id"].as_str().unwrap();
    let cmd = rec["evidence"][1]["id"].as_str().unwrap();
    let web_mem = rec["memories"][0]["id"].as_str().unwrap();

    // Another project's evidence is found too: evidence search is store-wide.
    let r = a
        .tool("search", json!({"kind": "evidence", "query": "构建"}))
        .ok();
    let mut got = ids(&r);
    got.sort();
    assert_eq!(got, sorted(&[&conv, web]));
    let h = hit(&r, web);
    assert_eq!(h["kind"], "evidence");
    assert_eq!(h["source_type"], "web");
    assert_eq!(h["excerpt"].as_str().unwrap().chars().count(), 200);
    assert!(long.starts_with(h["excerpt"].as_str().unwrap()));
    assert_eq!(h["truncated"], true);
    assert_eq!(h["cited_by"], json!([web_mem]));
    assert!(h["score"].as_f64().unwrap() > 0.0, "{h}");
    assert_eq!(h["matched_terms"], json!(["构建"]));
    let c = hit(&r, &conv);
    assert_eq!(c["excerpt"], "记住：构建 用 cargo");
    assert_eq!(c["truncated"], false);
    assert_eq!(c["cited_by"], json!([mem]));

    // No query: newest first, score 0.
    let r = a.tool("search", json!({"kind": "evidence"})).ok();
    assert_eq!(ids(&r).len(), 3);
    assert_eq!(ids(&r)[2], conv.as_str());
    assert_eq!(r["hits"][0]["score"], 0.0);
    assert_eq!(r["hits"][0]["matched_terms"], json!([]));

    assert_eq!(
        sorted_ids(
            &a,
            json!({"kind": "evidence", "source_type": ["command", "conversation"]})
        ),
        sorted(&[cmd, &conv])
    );
    assert_eq!(
        sorted_ids(
            &a,
            json!({"kind": "evidence", "query": "构建", "source_type": ["web"], "limit": 5, "project": d.path().to_str().unwrap()})
        ),
        sorted(&[web])
    );
}

#[test]
fn memory_only_and_evidence_only_args_are_rejected_across_kinds() {
    let a = Almanac::new();
    for (k, v) in [
        ("scope", json!("global")),
        ("all", json!(false)),
        ("status", json!(["active"])),
        ("epistemic", json!(["observed"])),
        ("writer", json!("test")),
        ("cites", json!("ev_x")),
        ("stale_only", json!(false)),
    ] {
        let mut args = json!({"kind": "evidence"});
        args[k] = v;
        a.tool("search", args).err("INVALID_ARGS");
    }
    for args in [
        json!({"source_type": ["web"]}),
        json!({"kind": "memory", "source_type": ["web"]}),
        json!({"kind": "evidence", "source_type": ["email"]}),
        json!({"kind": "both"}),
    ] {
        a.tool("search", args).err("INVALID_ARGS");
    }
    a.tool("search", json!({"kind": "memory"})).ok();
    a.tool("search", json!({"stale_only": false})).ok();
}

#[test]
fn source_types_follow_current_evidence() {
    let a = Almanac::new();
    let (_, mem) = a.remember("foo 构建");
    let web = a
        .tool(
            "record",
            json!({"evidence": [{"ref": "w", "excerpt": "网页 foo", "source": {"type": "web", "url": "https://example.com"}}]}),
        )
        .ok()["evidence"][0]["id"]
        .clone();
    a.tool("corroborate", json!({"memory": mem, "evidence": [web]}))
        .ok();
    let r = a.tool("search", json!({"query": "foo"})).ok();
    assert_eq!(
        hit(&r, &mem)["source_types"],
        json!(["conversation", "web"])
    );

    let rec = a
        .tool(
            "record",
            json!({"memories": [{"text": "foo 只看网页", "scope": "global", "evidence": [web], "supersedes": [mem]}]}),
        )
        .ok();
    let new = rec["memories"][0]["id"].as_str().unwrap();
    let r = a.tool("search", json!({"query": "foo"})).ok();
    assert_eq!(ids(&r), [new]);
    assert_eq!(r["hits"][0]["source_types"], json!(["web"]));
}

/// Runs git in `dir` with a fixed identity.
fn git(dir: &std::path::Path, args: &[&str]) {
    let o = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {o:?}");
}

#[test]
fn alias_hint_names_projects_sharing_the_root_commit_until_aliased() {
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

    let a = Almanac::new();
    a.tool(
        "record",
        json!({
            "evidence": [{"ref": "e", "excerpt": "记住：foo", "source": {"type": "conversation"}}],
            "memories": [{"text": "foo", "scope": "project", "evidence": ["e"]}],
            "project": old.to_str().unwrap()
        }),
    )
    .ok();
    let search = |dir: &std::path::Path, extra: Value| {
        let mut args = json!({"project": dir.to_str().unwrap()});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        a.tool("search", args).ok()
    };

    let want = json!({"project": "github.com/x/new", "same_root_as": ["github.com/x/old"]});
    assert_eq!(search(&new, json!({}))["alias_hint"], want);
    assert_eq!(search(&new, json!({"scope": "global"}))["alias_hint"], want);
    assert_eq!(
        search(&new, json!({"kind": "evidence"}))["alias_hint"],
        want
    );
    assert!(search(&old, json!({})).get("alias_hint").is_none());

    let o = a.run(
        &["project", "alias", "github.com/x/old", "github.com/x/new"],
        None,
    );
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(search(&new, json!({})).get("alias_hint").is_none());
}

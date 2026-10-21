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

//! `review`: stale and unsupported issues (spec ch. 3 「review」, ch. 7).
mod common;
use common::Almanac;
use serde_json::{Value, json};
use std::path::Path;

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn review(a: &Almanac, args: Value) -> Value {
    a.tool("review", args).ok()["issues"].clone()
}

/// A memory in `d` (scope `scope`) citing `a.txt`'s excerpt `excerpt`; returns its id.
fn remember_file(a: &Almanac, d: &Path, scope: &str, excerpt: &str) -> String {
    a.tool(
        "record",
        json!({
            "project": s(d),
            "evidence": [{"ref": "e", "excerpt": excerpt, "source": {"type": "file", "path": "a.txt"}}],
            "memories": [{"text": format!("记忆 {excerpt}"), "scope": scope, "evidence": ["e"]}]
        }),
    )
    .ok()["memories"][0]["id"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn reports_stale_then_unsupported_from_the_cache_only_without_writing() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "cargo build\nmake all\n").unwrap();
    let a = Almanac::new();
    let (ev, unsupported) = a.remember("无据的");
    let m1 = remember_file(&a, d.path(), "global", "cargo build");
    let m2 = remember_file(&a, d.path(), "global", "make all");
    a.run_tty(&["purge", &ev, "--reason", "r"], "n\nyes\n");
    std::fs::write(d.path().join("a.txt"), "nothing\n").unwrap();
    let args = json!({"project": s(d.path())});

    // Not checked since the change: review does not check, so nothing is stale yet.
    let want_unsupported = json!({
        "type": "unsupported", "memory": unsupported, "text": "无据的",
        "actions": ["supersede", "retract"]
    });
    assert_eq!(review(&a, args.clone()), json!([want_unsupported]));

    a.tool("verify", args.clone()).ok();
    let log = a.log_raw();
    let ev_of = |m: &str| {
        a.tool("get", json!({"ids": [m]})).ok()["records"][0]["evidence"][0]["id"].clone()
    };
    let stale = |m: &str, text: &str| {
        json!({
            "type": "stale", "memory": m, "text": text,
            "stale": [{"evidence": ev_of(m), "result": "changed"}],
            "actions": ["corroborate", "supersede", "retract"]
        })
    };
    assert_eq!(
        review(&a, args),
        json!([
            stale(&m1, "记忆 cargo build"),
            stale(&m2, "记忆 make all"),
            want_unsupported
        ])
    );
    assert_eq!(a.log_raw(), log, "review is read-only");
}

#[test]
fn a_split_into_several_successors_is_not_an_issue() {
    let a = Almanac::new();
    let (ev, old) = a.remember("甲和乙");
    a.tool(
        "record",
        json!({"memories": [
            {"text": "甲", "scope": "global", "evidence": [ev], "supersedes": [old]},
            {"text": "乙", "scope": "global", "evidence": [ev], "supersedes": [old]}
        ]}),
    )
    .ok();
    assert_eq!(review(&a, json!({"scope": "global"})), json!([]));
}

#[test]
fn scope_and_all_filter_like_search() {
    let a = Almanac::new();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let unsupported_in = |d: &Path| {
        let r = a
            .tool(
                "record",
                json!({
                    "project": s(d),
                    "evidence": [{"ref": "e", "excerpt": "记住：项目里的", "source": {"type": "conversation"}}],
                    "memories": [{"text": "项目里的", "scope": "project", "evidence": ["e"]}]
                }),
            )
            .ok();
        let ev = r["evidence"][0]["id"].as_str().unwrap();
        a.run_tty(&["purge", ev, "--reason", "r"], "n\nyes\n");
        r["memories"][0]["id"].clone()
    };
    let (p1, p2) = (unsupported_in(d1.path()), unsupported_in(d2.path()));
    let at = |args: Value| -> Vec<Value> {
        review(&a, args)
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["memory"].clone())
            .collect()
    };
    assert_eq!(
        at(json!({"project": s(d1.path())})),
        std::slice::from_ref(&p1)
    );
    assert_eq!(at(json!({"project": s(d1.path()), "all": true})), [p1, p2]);
    assert!(at(json!({"project": s(d1.path()), "scope": "global"})).is_empty());
}

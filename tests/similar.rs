mod common;
use common::Almanac;
use serde_json::{Value, json};

fn record(a: &Almanac, text: &str, scope: &str, extra: Value) -> Value {
    let mut m = json!({"text": text, "scope": scope, "evidence": ["e"]});
    for (k, v) in extra.as_object().unwrap() {
        m[k] = v.clone();
    }
    a.tool(
        "record",
        json!({
            "evidence": [{"ref": "e", "excerpt": format!("记住：{text}"), "source": {"type": "conversation"}}],
            "memories": [m]
        }),
    )
    .ok()["memories"][0]
        .clone()
}

#[test]
fn record_lists_a_similar_active_memory() {
    let a = Almanac::new();
    let (_, old) = a.remember("缓存失效时间是 5 分钟");
    let m = record(&a, "缓存失效时间改为 10 分钟", "global", json!({}));
    assert_eq!(
        m["similar"],
        json!([{"id": old, "text": "缓存失效时间是 5 分钟", "similarity": 0.75}])
    );
}

#[test]
fn only_active_memories_outside_this_supersedes_are_candidates() {
    let a = Almanac::new();
    let (_, wrong) = a.remember("缓存失效时间是 3 分钟");
    a.tool("retract", json!({"memory": wrong, "reason": "记错了"}))
        .ok();
    let (_, old) = a.remember("缓存失效时间是 5 分钟");
    let new = record(
        &a,
        "缓存失效时间改为 10 分钟",
        "global",
        json!({"supersedes": [old]}),
    );
    assert_eq!(new["similar"], json!([]));
    // `old` is now superseded; only `new` is still active.
    assert_eq!(
        ids(&record(&a, "缓存失效时间约为 10 分钟", "global", json!({}))),
        [id(&new)]
    );
}

#[test]
fn a_deduped_retry_has_no_similar() {
    let a = Almanac::new();
    a.remember("缓存失效时间是 5 分钟");
    let call = json!({
        "evidence": [{"ref": "e", "excerpt": "记住：缓存失效时间改为 10 分钟", "source": {"type": "conversation"}}],
        "memories": [{"text": "缓存失效时间改为 10 分钟", "scope": "global", "evidence": ["e"]}]
    });
    let first = a.tool("record", call.clone()).ok();
    assert_eq!(first["memories"][0]["similar"].as_array().unwrap().len(), 1);
    let retry = a.tool("record", call).ok();
    assert_eq!(retry["memories"][0]["deduped"], true);
    assert_eq!(retry["memories"][0]["similar"], json!([]));
}

#[test]
fn at_most_three_best_first_ties_newest_first() {
    let a = Almanac::new();
    // Against the new memory: 1.0, 0.75, 0.75, 0.5 (tokens counted per spec ch. 7).
    let (_, half) = a.remember("aa bb cc dd xx yy zz ww");
    let (_, tie_old) = a.remember("aa bb cc pp");
    let (_, best) = a.remember("aa bb cc dd");
    let (_, tie_new) = a.remember("aa bb cc qq");
    let got = record(&a, "aa bb cc dd ee ff", "global", json!({}));
    assert_eq!(ids(&got), [best, tie_new.clone(), tie_old.clone()]);
    let sims: Vec<f64> = got["similar"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["similarity"].as_f64().unwrap())
        .collect();
    assert_eq!(sims, [1.0, 0.75, 0.75]);
    assert!(!ids(&got).contains(&half), "the 4th similar one is cut");

    // A corroboration makes a memory the most recent one.
    let (ev, _) = a.remember("另一条无关的记忆内容");
    a.tool("corroborate", json!({"memory": tie_old, "evidence": [ev]}))
        .ok();
    let got = record(
        &a,
        "aa bb cc dd ee gg",
        "global",
        json!({"supersedes": [id(&got)]}),
    );
    assert_eq!(ids(&got)[1..], [tie_old, tie_new]);
}

fn ids(m: &Value) -> Vec<String> {
    m["similar"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect()
}

fn id(m: &Value) -> String {
    m["id"].as_str().unwrap().to_string()
}

fn identity(d: &tempfile::TempDir) -> String {
    format!("path:{}", d.path().canonicalize().unwrap().display())
}

fn record_in(a: &Almanac, text: &str, scope: &str, d: &tempfile::TempDir) -> Value {
    a.tool(
        "record",
        json!({
            "evidence": [{"ref": "e", "excerpt": format!("记住：{text}"), "source": {"type": "conversation"}}],
            "memories": [{"text": text, "scope": scope, "evidence": ["e"]}],
            "project": d.path().to_str().unwrap()
        }),
    )
    .ok()["memories"][0]
        .clone()
}

#[test]
fn candidates_share_the_scope_and_project_alias_group() {
    let [d1, d2, d3] = [(); 3].map(|_| tempfile::tempdir().unwrap());
    let a = Almanac::new();
    let g = id(&record_in(&a, "部署前先跑数据库迁移脚本", "global", &d1));
    let p1 = id(&record_in(&a, "部署前先跑数据库迁移检查", "project", &d1));
    let p2 = id(&record_in(&a, "部署前先跑数据库迁移命令", "project", &d2));
    record_in(&a, "部署前先跑数据库迁移工具", "project", &d3);
    let alias = a.run(&["project", "alias", &identity(&d2), &identity(&d1)], None);
    assert_eq!(alias.code, 0, "{}", alias.stderr);

    let mut got = ids(&record_in(&a, "部署前先跑数据库迁移流程", "project", &d1));
    got.sort();
    let mut want = vec![p1, p2];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(
        ids(&record_in(&a, "部署前先跑数据库迁移步骤", "global", &d1)),
        [g]
    );
}

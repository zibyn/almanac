mod common;
use common::Almanac;
use serde_json::{Value, json};

fn is_id(v: &Value, prefix: &str) -> bool {
    let s = v.as_str().unwrap_or("");
    s.len() == prefix.len() + 26
        && s.starts_with(prefix)
        && s[prefix.len()..]
            .chars()
            .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c))
}

#[test]
fn record_then_get_round_trips_a_global_memory() {
    let a = Almanac::new();
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "e1", "excerpt": "记住：我喜欢用 4 空格缩进", "source": {"type": "conversation", "turn": 3}}],
                "memories": [{"text": "用户喜欢 4 空格缩进", "scope": "global", "evidence": ["e1"]}]
            }),
        )
        .ok();
    let ev = &r["evidence"][0];
    assert_eq!(ev["ref"], "e1");
    assert_eq!(ev["deduped"], false);
    assert!(is_id(&ev["id"], "ev_"), "{ev}");
    let mem = &r["memories"][0];
    assert!(is_id(&mem["id"], "mem_"), "{mem}");
    assert_eq!(mem["deduped"], false);
    assert_eq!(mem["similar"], json!([]));

    let (ev_id, mem_id) = (ev["id"].as_str().unwrap(), mem["id"].as_str().unwrap());
    let g = a.tool("get", json!({"ids": [mem_id, ev_id]})).ok();
    let m = &g["records"][0];
    assert_eq!(m["id"], mem_id);
    assert_eq!(m["kind"], "memory");
    assert_eq!(m["text"], "用户喜欢 4 空格缩进");
    assert_eq!(m["scope"], "global");
    assert_eq!(m["status"], "active");
    assert_eq!(m["epistemic"], "user_stated");
    assert_eq!(m["recorded"]["writer"], json!({"name": "test"}));
    assert!(is_id(&m["recorded"]["tx"], "tx_"));
    assert_eq!(m["evidence"][0]["id"], ev_id);
    assert_eq!(m["evidence"][0]["excerpt"], "记住：我喜欢用 4 空格缩进");
    assert_eq!(
        m["evidence"][0]["source"],
        json!({"type": "conversation", "host": "test", "turn": 3})
    );
    assert!(m["evidence"][0].get("cited_by").is_none());
    assert_eq!(m["supersedes"], json!([]));
    assert_eq!(m["superseded_by"], json!([]));
    assert_eq!(m["corroborations"], json!([]));
    assert_eq!(m["retraction"], Value::Null);
    assert_eq!(m["stale"], Value::Null);

    let e = &g["records"][1];
    assert_eq!(e["kind"], "evidence");
    assert_eq!(e["cited_by"], json!([mem_id]));
    assert_eq!(e["recorded"], m["recorded"]);
    assert_eq!(e["check"], Value::Null);
}

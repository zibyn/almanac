//! Supersession, retraction, corroboration and derived status (spec ch. 1 「事件」「记忆的状态」,
//! ch. 3 `record` / `corroborate` / `retract` / `get`).
mod common;
use common::Almanac;
use serde_json::{Value, json};

/// Records a memory citing existing evidence `ev`, superseding `old`; returns its id.
fn supersede(a: &Almanac, ev: &str, text: &str, old: &[&str]) -> String {
    let r = a
        .tool(
            "record",
            json!({"memories": [{"text": text, "scope": "global", "evidence": [ev], "supersedes": old}]}),
        )
        .ok();
    r["memories"][0]["id"].as_str().unwrap().into()
}

fn get(a: &Almanac, id: &str) -> Value {
    a.tool("get", json!({"ids": [id]})).ok()["records"][0].clone()
}

#[test]
fn superseding_marks_the_old_memory_superseded_and_links_both_ways() {
    let a = Almanac::new();
    let (ev, old) = a.remember("用户用 vim");
    let new = supersede(&a, &ev, "用户改用 helix", &[&old]);
    let o = get(&a, &old);
    assert_eq!(o["status"], "superseded");
    assert_eq!(o["superseded_by"], json!([new]));
    let n = get(&a, &new);
    assert_eq!(n["status"], "active");
    assert_eq!(n["supersedes"], json!([old]));
}

#[test]
fn one_call_can_split_a_memory_and_one_memory_can_merge_several() {
    let a = Almanac::new();
    let (ev, old) = a.remember("用户用 vim 和 tmux");
    let r = a
        .tool(
            "record",
            json!({"memories": [
                {"text": "用户用 vim", "scope": "global", "evidence": [ev], "supersedes": [old]},
                {"text": "用户用 tmux", "scope": "global", "evidence": [ev], "supersedes": [old]}
            ]}),
        )
        .ok();
    let (x, y) = (&r["memories"][0]["id"], &r["memories"][1]["id"]);
    assert_eq!(get(&a, &old)["superseded_by"], json!([x, y]));

    let merged = supersede(
        &a,
        &ev,
        "用户用 vim 和 tmux（合并）",
        &[x.as_str().unwrap(), y.as_str().unwrap()],
    );
    for id in [x, y] {
        let m = get(&a, id.as_str().unwrap());
        assert_eq!(m["status"], "superseded");
        assert_eq!(m["superseded_by"], json!([merged]));
    }
}

#[test]
fn superseding_an_unknown_memory_is_memory_not_found() {
    let a = Almanac::new();
    let (ev, _) = a.remember("x");
    let before = a.log_raw();
    let args = json!({"memories": [{"text": "y", "scope": "global", "evidence": [ev], "supersedes": ["mem_nope"]}]});
    let e = a.tool("record", args).err("MEMORY_NOT_FOUND");
    assert!(e["message"].as_str().unwrap().contains("mem_nope"), "{e}");
    assert_eq!(a.log_raw(), before);
}

#[test]
fn superseding_a_superseded_memory_is_not_active_with_the_active_successor_in_the_hint() {
    let a = Almanac::new();
    let (ev, v1) = a.remember("v1");
    let v2 = supersede(&a, &ev, "v2", &[&v1]);
    let v3 = supersede(&a, &ev, "v3", &[&v2]);
    let before = a.log_raw();
    let args = json!({"memories": [{"text": "v4", "scope": "global", "evidence": [ev], "supersedes": [v1]}]});
    let e = a.tool("record", args).err("NOT_ACTIVE");
    assert!(e["hint"].as_str().unwrap().contains(&v3), "{e}");
    assert!(!e["hint"].as_str().unwrap().contains(&v2), "{e}");
    assert_eq!(a.log_raw(), before);
}

#[test]
fn retrying_a_successful_supersession_is_deduped_not_not_active() {
    let a = Almanac::new();
    let (ev, old) = a.remember("旧");
    let args = json!({"memories": [{"text": "新", "scope": "global", "evidence": [ev], "supersedes": [old]}]});
    let first = a.tool("record", args.clone()).ok();
    let before = a.log_raw();
    let again = a.tool("record", args).ok();
    assert_eq!(again["memories"][0]["deduped"], true);
    assert_eq!(again["memories"][0]["id"], first["memories"][0]["id"]);
    assert_eq!(a.log_raw(), before);
}

/// Records one more conversation evidence and returns its id.
fn evidence(a: &Almanac, excerpt: &str) -> String {
    let r = a
        .tool(
            "record",
            json!({"evidence": [{"excerpt": excerpt, "source": {"type": "conversation"}}]}),
        )
        .ok();
    r["evidence"][0]["id"].as_str().unwrap().into()
}

#[test]
fn corroborating_with_existing_evidence_appends_it_in_time_order() {
    let a = Almanac::new();
    let (e1, m) = a.remember("用户用 helix");
    let e2 = evidence(&a, "我还在用 helix");
    let e3 = evidence(&a, "helix 真好用");
    let r = a
        .tool("corroborate", json!({"memory": m, "evidence": [e2, e1]}))
        .ok();
    assert_eq!(
        r,
        json!({"memory": m, "added": [e2], "already_cited": [e1]})
    );
    assert_eq!(
        a.log().last().unwrap()["events"],
        json!([{"type": "corroborated", "memory": m, "evidence": [e2]}])
    );
    a.tool("corroborate", json!({"memory": m, "evidence": [e3]}))
        .ok();

    let g = get(&a, &m);
    let ids: Vec<&Value> = g["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["id"])
        .collect();
    assert_eq!(ids, [&json!(e1), &json!(e2), &json!(e3)]);
    let c = g["corroborations"].as_array().unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0]["evidence"], json!([e2]));
    assert_eq!(c[1]["evidence"], json!([e3]));
    assert_eq!(c[0]["recorded"]["writer"]["name"], "test");
    assert_eq!(get(&a, &e2)["cited_by"], json!([m]));
}

#[test]
fn corroborating_with_already_cited_evidence_only_writes_nothing() {
    let a = Almanac::new();
    let (e1, m) = a.remember("x");
    let before = a.log_raw();
    let r = a
        .tool("corroborate", json!({"memory": m, "evidence": [e1]}))
        .ok();
    assert_eq!(r, json!({"memory": m, "added": [], "already_cited": [e1]}));
    assert_eq!(a.log_raw(), before);
}

#[test]
fn inline_evidence_is_checked_and_deduped_like_record_and_written_in_the_same_tx() {
    let a = Almanac::new();
    let (e1, m) = a.remember("用户用 helix");
    let e2 = evidence(&a, "已有的一句");
    let r = a
        .tool(
            "corroborate",
            json!({"memory": m, "evidence": [
                {"ref": "ignored", "excerpt": "已有的一句", "source": {"type": "conversation"}},
                {"ref": "ignored", "excerpt": "新的一句", "source": {"type": "conversation"}},
                e1
            ]}),
        )
        .ok();
    let added = r["added"].as_array().unwrap();
    assert_eq!(added.len(), 2);
    assert_eq!(
        added[0],
        json!(e2),
        "inline evidence matching existing is deduped"
    );
    assert_eq!(r["already_cited"], json!([e1]));
    let events = a.log().last().unwrap()["events"].clone();
    assert_eq!(events[0]["type"], "evidence_recorded");
    assert_eq!(events[0]["id"], added[1]);
    assert_eq!(
        events[1],
        json!({"type": "corroborated", "memory": m, "evidence": added})
    );

    let before = a.log_raw();
    a.tool(
        "corroborate",
        json!({"memory": m, "evidence": [{"source": {"type": "conversation"}}]}),
    )
    .err("MISSING_FIELD");
    a.tool(
        "corroborate",
        json!({"memory": m, "evidence": [{"excerpt": "记".repeat(1366), "source": {"type": "conversation"}}]}),
    )
    .err("EXCERPT_TOO_LONG");
    assert_eq!(a.log_raw(), before);
}

#[test]
fn corroborate_only_takes_active_memories_and_known_evidence() {
    let a = Almanac::new();
    let (ev, m) = a.remember("x");
    let before = a.log_raw();
    for (args, code) in [
        (
            json!({"memory": "mem_nope", "evidence": [ev]}),
            "MEMORY_NOT_FOUND",
        ),
        (json!({"memory": m, "evidence": []}), "NO_EVIDENCE"),
        (
            json!({"memory": m, "evidence": ["ev_nope"]}),
            "EVIDENCE_NOT_FOUND",
        ),
    ] {
        a.tool("corroborate", args).err(code);
    }
    assert_eq!(a.log_raw(), before);

    let next = supersede(&a, &ev, "y", &[&m]);
    let e = a
        .tool("corroborate", json!({"memory": m, "evidence": [ev]}))
        .err("NOT_ACTIVE");
    assert!(e["hint"].as_str().unwrap().contains(&next), "{e}");
    a.tool("retract", json!({"memory": next, "reason": "r"}))
        .ok();
    let before = a.log_raw();
    a.tool("corroborate", json!({"memory": next, "evidence": [ev]}))
        .err("NOT_ACTIVE");
    assert_eq!(a.log_raw(), before);
}

#[test]
fn record_retry_citing_corroborated_evidence_is_deduped() {
    let a = Almanac::new();
    let (_, m) = a.remember("用户用 helix");
    let e2 = evidence(&a, "还在用 helix");
    a.tool("corroborate", json!({"memory": m, "evidence": [e2]}))
        .ok();
    let r = a
        .tool(
            "record",
            json!({"memories": [{"text": "用户用 helix", "scope": "global", "evidence": [e2]}]}),
        )
        .ok();
    assert_eq!(r["memories"][0]["id"], json!(m));
    assert_eq!(r["memories"][0]["deduped"], true);
}

#[test]
fn retract_marks_the_memory_retracted_and_get_shows_the_reason() {
    let a = Almanac::new();
    let (_, m) = a.remember("用户住在北京");
    let r = a
        .tool("retract", json!({"memory": m, "reason": "记错了"}))
        .ok();
    assert_eq!(r, json!({"memory": m, "status": "retracted"}));
    let g = get(&a, &m);
    assert_eq!(g["status"], "retracted");
    assert_eq!(g["retraction"]["reason"], "记错了");
    assert_eq!(g["retraction"]["recorded"]["writer"]["name"], "test");
    let tx = a.log().last().unwrap().clone();
    assert_eq!(
        tx["events"],
        json!([{"type": "retracted", "memory": m, "reason": "记错了"}])
    );
}

#[test]
fn retract_needs_a_non_blank_reason() {
    let a = Almanac::new();
    let (_, m) = a.remember("x");
    let before = a.log_raw();
    for args in [
        json!({"memory": m}),
        json!({"memory": m, "reason": " \n\t"}),
    ] {
        let e = a.tool("retract", args).err("MISSING_FIELD");
        assert!(e["hint"].as_str().unwrap().contains("reason"), "{e}");
    }
    assert_eq!(a.log_raw(), before);
}

#[test]
fn retract_only_takes_existing_active_memories() {
    let a = Almanac::new();
    let (ev, m) = a.remember("x");
    a.tool("retract", json!({"memory": "mem_nope", "reason": "r"}))
        .err("MEMORY_NOT_FOUND");
    let next = supersede(&a, &ev, "y", &[&m]);
    let e = a
        .tool("retract", json!({"memory": m, "reason": "r"}))
        .err("NOT_ACTIVE");
    assert!(e["hint"].as_str().unwrap().contains(&next), "{e}");
    a.tool("retract", json!({"memory": next, "reason": "r"}))
        .ok();
    let before = a.log_raw();
    a.tool("retract", json!({"memory": next, "reason": "r"}))
        .err("NOT_ACTIVE");
    // Retracted and superseded are exclusive terminal states.
    let args = json!({"memories": [{"text": "z", "scope": "global", "evidence": [ev], "supersedes": [next]}]});
    a.tool("record", args).err("NOT_ACTIVE");
    assert_eq!(a.log_raw(), before);
}

#[test]
fn retracted_text_no_longer_blocks_recording_the_same_text() {
    let a = Almanac::new();
    let (ev, m) = a.remember("同一句");
    a.tool("retract", json!({"memory": m, "reason": "先撤回"}))
        .ok();
    let r = a
        .tool(
            "record",
            json!({"memories": [{"text": "同一句", "scope": "global", "evidence": [ev]}]}),
        )
        .ok();
    assert_eq!(r["memories"][0]["deduped"], false);
}

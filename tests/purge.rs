//! `almanac purge` (spec ch. 1 「清除与墓碑」, ch. 2 「清除步骤」, ch. 4 `purge`, ADR 0001).
mod common;
use common::Almanac;
use serde_json::{Value, json};

fn get(a: &Almanac, id: &str) -> Value {
    a.tool("get", json!({"ids": [id]})).ok()["records"][0].clone()
}

/// Records command evidence holding a secret and a memory citing it; returns (ev_id, mem_id).
fn secret(a: &Almanac) -> (String, String) {
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "e", "excerpt": "DEPLOY_KEY=hunter2", "source": {"type": "command", "command": "cat deploy-secret.env", "exit_code": 0}}],
                "memories": [{"text": "部署密钥放在环境文件里", "scope": "global", "evidence": ["e"]}]
            }),
        )
        .ok();
    (
        r["evidence"][0]["id"].as_str().unwrap().into(),
        r["memories"][0]["id"].as_str().unwrap().into(),
    )
}

/// Every event's object id (`id` or `memory`), in log order.
fn event_ids(a: &Almanac) -> Vec<Value> {
    a.log()
        .iter()
        .flat_map(|tx| tx["events"].as_array().unwrap().clone())
        .map(|e| {
            e.get("id")
                .or(e.get("memory"))
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect()
}

#[test]
fn purging_evidence_leaves_a_tombstone_and_erases_excerpt_and_locator() {
    let a = Almanac::new();
    let (ev, mem) = secret(&a);
    a.remember("别的记忆");
    std::fs::create_dir_all(a.home.path().join("cache")).unwrap();
    let (txs, ids) = (a.log(), event_ids(&a));

    let o = a.run_tty(&["purge", &ev, "--reason", "含部署密钥"], "n\nyes\n");
    assert_eq!(o.code, 0, "{}", o.stdout);
    assert!(
        o.stdout.contains("DEPLOY_KEY=hunter2"),
        "shows the object: {}",
        o.stdout
    );
    assert!(
        o.stdout.contains(&mem),
        "asks about the citing memory: {}",
        o.stdout
    );
    assert!(o.stdout.contains("外部备份"), "{}", o.stdout);

    let raw = a.log_raw();
    assert!(
        !raw.contains("hunter2") && !raw.contains("deploy-secret"),
        "{raw}"
    );
    assert!(!a.home.path().join("cache").exists());
    assert_eq!(event_ids(&a), ids);
    for (old, new) in txs.iter().zip(a.log()) {
        for k in ["tx", "at", "writer", "v"] {
            assert_eq!(old[k], new[k]);
        }
    }

    let e = get(&a, &ev);
    assert_eq!(e["source"], json!({"type": "command"}));
    assert!(e.get("excerpt").is_none(), "{e}");
    assert_eq!(e["purged"]["reason"], "含部署密钥");
    assert_eq!(e["purged"]["by"], json!({"name": "human"}));
    assert!(e["purged"]["at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(e["purged"].as_object().unwrap().len(), 3, "no hash: {e}");

    let m = get(&a, &mem);
    assert_eq!(m["status"], "unsupported");
    assert_eq!(m["text"], "部署密钥放在环境文件里");
}

#[test]
fn purging_a_memory_tombstones_its_text_and_retraction_reason_and_keeps_structure() {
    let a = Almanac::new();
    let (_, old) = a.remember("旧地址甲");
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [{"ref": "e2", "excerpt": "记住：新地址乙", "source": {"type": "conversation"}}],
                "memories": [{"text": "新地址乙", "scope": "global", "evidence": ["e2"], "supersedes": [old]}]
            }),
        )
        .ok();
    let (ev2, new) = (
        r["evidence"][0]["id"].as_str().unwrap().to_string(),
        r["memories"][0]["id"].as_str().unwrap().to_string(),
    );
    a.tool("retract", json!({"memory": new, "reason": "原因丙"}))
        .ok();

    // Asked about its evidence (declined), then the predecessor on its chain (accepted).
    let o = a.run_tty(&["purge", &new, "--reason", "住址"], "n\ny\nyes\n");
    assert_eq!(o.code, 0, "{}", o.stdout);
    let (ask_ev, ask_old) = (
        o.stdout.find(&ev2),
        o.stdout.find(&format!("一并清除 记忆 {old}")),
    );
    assert!(ask_ev.is_some() && ask_old.is_some(), "{}", o.stdout);

    let raw = a.log_raw();
    assert!(!raw.contains("原因丙"), "{raw}");
    assert!(
        !raw.contains("\"text\":\"新地址乙\"") && !raw.contains("\"text\":\"旧地址甲\""),
        "{raw}"
    );
    assert!(
        raw.contains("记住：新地址乙"),
        "declined evidence stays: {raw}"
    );

    let m = get(&a, &new);
    assert_eq!(m["status"], "purged");
    assert!(m.get("text").is_none(), "{m}");
    assert_eq!(m["purged"]["reason"], "住址");
    assert_eq!(m["purged"]["by"], json!({"name": "human"}));
    assert_eq!(m["retraction"]["purged"], m["purged"]);
    assert!(m["retraction"].get("reason").is_none(), "{m}");
    assert_eq!(m["scope"], "global");
    assert_eq!(m["supersedes"], json!([old]));
    assert_eq!(m["evidence"][0]["id"], ev2.as_str());
    let o = get(&a, &old);
    assert_eq!(o["status"], "purged");
    assert_eq!(o["superseded_by"], json!([new]));

    let again = a.run_tty(&["purge", &new, "--reason", "再来"], "yes\n");
    assert_eq!(again.code, 1);
    assert!(
        again.stdout.contains("ALREADY_PURGED："),
        "{}",
        again.stdout
    );
}

#[test]
fn search_does_not_find_purged_text() {
    let a = Almanac::new();
    let (_, mem) = a.remember("独角兽密语");
    a.run_tty(&["purge", &mem, "--reason", "r"], "y\nyes\n");
    let hits = a.tool("search", json!({"query": "独角兽密语"})).ok()["hits"].clone();
    assert_eq!(hits, json!([]));
}

#[test]
fn a_purged_memory_is_terminal() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("要清除的");
    a.run_tty(&["purge", &mem, "--reason", "r"], "n\nyes\n");
    assert_eq!(get(&a, &mem)["status"], "purged");
    a.tool("retract", json!({"memory": mem, "reason": "x"}))
        .err("NOT_ACTIVE");
    a.tool("corroborate", json!({"memory": mem, "evidence": [ev]}))
        .err("NOT_ACTIVE");
    a.tool(
        "record",
        json!({"memories": [{"text": "替代", "scope": "global", "evidence": [ev], "supersedes": [mem]}]}),
    )
    .err("NOT_ACTIVE");
}

#[test]
fn an_unsupported_memory_can_be_retracted_and_purged_evidence_cannot_be_cited() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("无据的");
    let (_, other) = a.remember("另一条");
    a.run_tty(&["purge", &ev, "--reason", "r"], "n\nyes\n");
    assert_eq!(get(&a, &mem)["status"], "unsupported");
    a.tool(
        "record",
        json!({"memories": [{"text": "新的", "scope": "global", "evidence": [ev]}]}),
    )
    .err("EVIDENCE_NOT_FOUND");
    a.tool("corroborate", json!({"memory": other, "evidence": [ev]}))
        .err("EVIDENCE_NOT_FOUND");
    a.tool("retract", json!({"memory": mem, "reason": "没依据了"}))
        .ok();
}

#[test]
fn nothing_changes_unless_the_human_types_yes() {
    let a = Almanac::new();
    let (ev, _) = a.remember("保留");
    let before = a.log_raw();
    let o = a.run_tty(&["purge", &ev, "--reason", "r"], "y\nok\n");
    assert_eq!(o.code, 0, "{}", o.stdout);
    assert_eq!(a.log_raw(), before);
}

#[test]
fn purge_is_refused_on_a_store_from_a_newer_version() {
    let a = Almanac::new();
    let (ev, _) = a.remember("x");
    let mut raw = a.log_raw();
    raw.push_str("{\"v\":99}\n");
    std::fs::write(a.log_path(), &raw).unwrap();
    let o = a.run_tty(&["purge", &ev, "--reason", "r"], "yes\n");
    assert_eq!(o.code, 1);
    assert!(o.stdout.contains("STORE_NEWER："), "{}", o.stdout);
    assert_eq!(a.log_raw(), raw);
}

#[test]
fn purge_refuses_when_stdin_is_not_a_terminal() {
    let a = Almanac::new();
    let (ev, _) = a.remember("用户的 token 是 abc");
    let before = a.log_raw();
    let o = a.run(&["purge", &ev, "--reason", "含 token"], Some("yes\n"));
    assert_eq!(o.code, 1);
    assert!(o.stderr.starts_with("NOT_INTERACTIVE："), "{}", o.stderr);
    assert_eq!(a.log_raw(), before);
}

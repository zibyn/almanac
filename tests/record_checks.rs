//! `record` hard checks and idempotence (spec ch. 1 「指纹」, ch. 3 「硬检查」「重复」, ch. 10).
mod common;
use common::Almanac;
use serde_json::{Value, json};

fn conv(r: &str, excerpt: &str) -> Value {
    json!({"ref": r, "excerpt": excerpt, "source": {"type": "conversation"}})
}

fn mem(text: &str, evidence: &[&str]) -> Value {
    json!({"text": text, "scope": "global", "evidence": evidence})
}

/// Asserts `record` fails with `code` and writes nothing; returns the error object.
fn rejects(a: &Almanac, args: Value, code: &str) -> Value {
    let before = a.log_raw();
    let e = a.tool("record", args).err(code);
    assert_eq!(a.log_raw(), before, "a rejected record must not write");
    e
}

#[test]
fn missing_fields_are_missing_field_with_a_hint_naming_the_field() {
    let a = Almanac::new();
    for (args, field) in [
        (json!({}), "evidence"),
        (json!({"evidence": [], "memories": []}), "evidence"),
        (
            json!({"evidence": [{"source": {"type": "conversation"}}]}),
            "excerpt",
        ),
        (json!({"evidence": [{"excerpt": "x"}]}), "source"),
        (
            json!({"evidence": [conv("e", "x")], "memories": [{"scope": "global", "evidence": ["e"]}]}),
            "text",
        ),
        (
            json!({"evidence": [{"excerpt": "x", "source": {"type": "command", "command": "ls"}}]}),
            "exit_code",
        ),
    ] {
        let e = rejects(&a, args, "MISSING_FIELD");
        assert!(e["hint"].as_str().unwrap().contains(field), "{e}");
    }
}

#[test]
fn excerpt_over_4096_bytes_is_rejected() {
    let a = Almanac::new();
    // 1366 three-byte chars = 4098 bytes, though only 1366 characters.
    let long = "记".repeat(1366);
    let e = rejects(
        &a,
        json!({"evidence": [conv("e", &long)]}),
        "EXCERPT_TOO_LONG",
    );
    assert!(e["hint"].as_str().unwrap().contains("截取"), "{e}");
    // Exactly 4096 bytes is fine.
    a.tool(
        "record",
        json!({"evidence": [conv("e", &"a".repeat(4096))]}),
    )
    .ok();
}

#[test]
fn text_over_500_characters_is_rejected() {
    let a = Almanac::new();
    let args =
        |n| json!({"evidence": [conv("e", "x")], "memories": [mem(&"记".repeat(n), &["e"])]});
    let e = rejects(&a, args(501), "TEXT_TOO_LONG");
    assert!(e["hint"].as_str().unwrap().contains("拆"), "{e}");
    a.tool("record", args(500)).ok();
}

#[test]
fn scope_other_than_global_or_project_is_scope_invalid() {
    let a = Almanac::new();
    let e = rejects(
        &a,
        json!({"evidence": [conv("e", "x")], "memories": [{"text": "t", "scope": "user", "evidence": ["e"]}]}),
        "SCOPE_INVALID",
    );
    assert!(e["hint"].as_str().unwrap().contains("显式"), "{e}");
}

#[test]
fn missing_scope_is_scope_invalid() {
    let a = Almanac::new();
    let e = rejects(
        &a,
        json!({"evidence": [conv("e", "x")], "memories": [{"text": "t", "evidence": ["e"]}]}),
        "SCOPE_INVALID",
    );
    assert!(e["hint"].as_str().unwrap().contains("显式"), "{e}");
}

#[test]
fn memory_without_evidence_is_no_evidence() {
    let a = Almanac::new();
    for m in [mem("t", &[]), json!({"text": "t", "scope": "global"})] {
        let e = rejects(&a, json!({"memories": [m]}), "NO_EVIDENCE");
        assert!(e["hint"].as_str().unwrap().contains("证据"), "{e}");
    }
}

#[test]
fn undefined_ref_or_unknown_id_is_evidence_not_found() {
    let a = Almanac::new();
    rejects(
        &a,
        json!({"evidence": [conv("e", "x")], "memories": [mem("t", &["f"])]}),
        "EVIDENCE_NOT_FOUND",
    );
    rejects(
        &a,
        json!({"memories": [mem("t", &["ev_01JAAAAAAAAAAAAAAAAAAAAAAA"])]}),
        "EVIDENCE_NOT_FOUND",
    );
}

#[test]
fn a_bad_memory_rejects_the_evidence_written_alongside_it() {
    let a = Almanac::new();
    a.remember("先有一条");
    rejects(
        &a,
        json!({"evidence": [conv("e", "好的一条")], "memories": [mem("好", &["e"]), mem("坏", &[])]}),
        "NO_EVIDENCE",
    );
}

#[test]
fn retrying_the_same_evidence_returns_the_existing_id_and_writes_nothing() {
    let a = Almanac::new();
    let first = a
        .tool("record", json!({"evidence": [conv("e", "我喜欢 tab")]}))
        .ok();
    let lines = a.log().len();
    let again = a
        .tool("record", json!({"evidence": [conv("x", "我喜欢 tab")]}))
        .ok();
    assert_eq!(again["evidence"][0]["id"], first["evidence"][0]["id"]);
    assert_eq!(again["evidence"][0]["ref"], "x");
    assert_eq!(again["evidence"][0]["deduped"], true);
    assert_eq!(a.log().len(), lines);
}

#[test]
fn fingerprint_normalizes_nfc_and_whitespace_but_not_other_locators() {
    let a = Almanac::new();
    let id = |ev: Value| a.tool("record", json!({"evidence": [ev]})).ok()["evidence"][0].clone();
    let base = id(
        json!({"excerpt": "café  au\nlait", "source": {"type": "conversation", "session": "s1", "turn": 1}}),
    );
    // Decomposed é, collapsed/trimmed whitespace, different turn: same evidence.
    let same = id(
        json!({"excerpt": " cafe\u{301} au lait ", "source": {"type": "conversation", "session": "s1", "turn": 9}}),
    );
    assert_eq!(same["id"], base["id"]);
    assert_eq!(same["deduped"], true);
    // Another session, or another source type: different evidence.
    for other in [
        json!({"excerpt": "café au lait", "source": {"type": "conversation", "session": "s2"}}),
        json!({"excerpt": "café au lait", "source": {"type": "command", "command": "echo", "exit_code": 0}}),
    ] {
        let o = id(other);
        assert_ne!(o["id"], base["id"]);
        assert_eq!(o["deduped"], false);
    }
    // fetched_at is not part of a web fingerprint.
    let w = |at: &str| {
        id(
            json!({"excerpt": "x", "source": {"type": "web", "url": "https://a.example/", "fetched_at": at}}),
        )
    };
    assert_eq!(
        w("2026-01-01T00:00:00.000Z")["id"],
        w("2026-02-01T00:00:00.000Z")["id"]
    );
}

#[test]
fn two_identical_evidence_in_one_call_merge_into_one() {
    let a = Almanac::new();
    let r = a
        .tool(
            "record",
            json!({"evidence": [conv("a", "同一句"), conv("b", "同一句")], "memories": [mem("t", &["a", "b"])]}),
        )
        .ok();
    let (ea, eb) = (&r["evidence"][0], &r["evidence"][1]);
    assert_eq!(ea["id"], eb["id"]);
    assert_eq!(ea["deduped"], false);
    assert_eq!(eb["deduped"], true);
    let events = a.log()[0]["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 2, "one evidence + one memory");
    assert_eq!(events[1]["evidence"], json!([ea["id"]]));
}

#[test]
fn retrying_a_memory_returns_the_existing_id_and_writes_nothing() {
    let a = Almanac::new();
    let (_, mem_id) = a.remember("用户喜欢 tab");
    let lines = a.log().len();
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [conv("e", "记住：用户喜欢 tab")],
                "memories": [mem("  用户喜欢\ttab ", &["e"])]
            }),
        )
        .ok();
    assert_eq!(r["memories"][0]["id"], mem_id.as_str());
    assert_eq!(r["memories"][0]["deduped"], true);
    assert_eq!(r["memories"][0]["similar"], json!([]));
    assert_eq!(a.log().len(), lines);
}

#[test]
fn same_text_with_a_subset_of_evidence_and_supersedes_is_deduped() {
    let a = Almanac::new();
    let (_, old) = a.remember("旧的");
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [conv("a", "甲"), conv("b", "乙")],
                "memories": [{"text": "新的", "scope": "global", "evidence": ["a", "b"], "supersedes": [old]}]
            }),
        )
        .ok();
    let (ev_a, mem_id) = (&r["evidence"][0]["id"], &r["memories"][0]["id"]);
    for supersedes in [json!([old]), json!([])] {
        let again = a
            .tool(
                "record",
                json!({"memories": [{"text": "新的", "scope": "global", "evidence": [ev_a], "supersedes": supersedes}]}),
            )
            .ok();
        assert_eq!(&again["memories"][0]["id"], mem_id);
        assert_eq!(again["memories"][0]["deduped"], true);
    }
}

#[test]
fn same_text_with_new_evidence_or_supersedes_is_duplicate_text() {
    let a = Almanac::new();
    let (_, mem_id) = a.remember("用户喜欢 tab");
    let (_, other) = a.remember("别的");
    let e = rejects(
        &a,
        json!({"evidence": [conv("e", "另一句话")], "memories": [mem("用户喜欢 tab", &["e"])]}),
        "DUPLICATE_TEXT",
    );
    assert!(e["message"].as_str().unwrap().contains(&mem_id), "{e}");
    assert!(e["hint"].as_str().unwrap().contains("corroborate"), "{e}");
    rejects(
        &a,
        json!({
            "evidence": [conv("e", "记住：用户喜欢 tab")],
            "memories": [{"text": "用户喜欢 tab", "scope": "global", "evidence": ["e"], "supersedes": [other]}]
        }),
        "DUPLICATE_TEXT",
    );
}

#[test]
fn omitted_epistemic_is_inferred_from_evidence_and_logged() {
    let a = Almanac::new();
    let cmd = json!({"ref": "c", "excerpt": "ok", "source": {"type": "command", "command": "make", "exit_code": 0}});
    let r = a
        .tool(
            "record",
            json!({
                "evidence": [conv("u", "用户的话"), cmd],
                "memories": [
                    mem("只有对话", &["u"]),
                    mem("对话加命令", &["u", "c"]),
                    mem("只有命令", &["c"]),
                    {"text": "显式", "scope": "global", "epistemic": "observed", "evidence": ["c"]}
                ]
            }),
        )
        .ok();
    assert_eq!(r["memories"].as_array().unwrap().len(), 4);
    let logged: Vec<Value> = a.log()[0]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "memory_recorded")
        .map(|e| e["epistemic"].clone())
        .collect();
    assert_eq!(
        logged,
        [
            json!("user_stated"),
            json!("inferred"),
            json!("inferred"),
            json!("observed")
        ]
    );
}

#[test]
fn duplicate_ref_in_one_call_is_invalid_args() {
    let a = Almanac::new();
    rejects(
        &a,
        json!({"evidence": [conv("e", "x"), conv("e", "y")]}),
        "INVALID_ARGS",
    );
}

#[test]
fn web_url_must_be_http_or_https() {
    let a = Almanac::new();
    rejects(
        &a,
        json!({"evidence": [{"excerpt": "x", "source": {"type": "web", "url": "file:///etc/hosts"}}]}),
        "INVALID_ARGS",
    );
}

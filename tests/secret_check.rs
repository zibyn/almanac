//! Secret check on writes (spec ch. 3 `record` 「密钥检查」, `retract`, `corroborate`; ch. 10).
//! Per-rule hits and near-misses are unit tests in `src/secrets.rs`.
mod common;
use common::Almanac;
use serde_json::{Value, json};

const AWS: &str = "AKIAIOSFODNN7EXAMPLE";

fn conv(r: &str, excerpt: &str) -> Value {
    json!({"ref": r, "excerpt": excerpt, "source": {"type": "conversation"}})
}

/// Asserts `tool` fails with `SECRET_DETECTED`, writes nothing and never echoes `secret`;
/// returns the error's `message`.
fn rejects(a: &Almanac, tool: &str, args: Value, secret: &str) -> String {
    let before = a.log_raw();
    let out = a.tool(tool, args);
    let raw = out.json().to_string();
    let e = out.err("SECRET_DETECTED");
    assert_eq!(
        a.log_raw(),
        before,
        "a rejected write must not touch the log"
    );
    assert!(!raw.contains(secret), "secret echoed: {raw}");
    assert!(e["hint"].as_str().unwrap().contains("<redacted>"), "{e}");
    e["message"].as_str().unwrap().to_string()
}

#[test]
fn message_names_rule_field_path_and_character_offset() {
    let a = Almanac::new();
    let msg = rejects(
        &a,
        "record",
        json!({"evidence": [conv("e", &format!("密钥是 {AWS}"))]}),
        AWS,
    );
    // "密钥是 " is 4 characters (but 10 bytes): the offset counts characters.
    for want in ["aws_access_key", "evidence[0].excerpt", "4"] {
        assert!(msg.contains(want), "{msg}");
    }
    assert!(!msg.contains("10"), "{msg}");
}

#[test]
fn source_string_fields_are_scanned_with_their_path() {
    let a = Almanac::new();
    let cases = [
        (
            json!({"type": "command", "command": format!("aws --key {AWS}"), "exit_code": 0}),
            "evidence[1].source.command",
        ),
        (
            json!({"type": "command", "command": "ls", "cwd": format!("/tmp/{AWS}"), "exit_code": 0}),
            "evidence[1].source.cwd",
        ),
        (
            json!({"type": "web", "url": "https://u:hunter2@example.com/x"}),
            "evidence[1].source.url",
        ),
        (
            json!({"type": "conversation", "session": AWS}),
            "evidence[1].source.session",
        ),
        (
            json!({"type": "conversation", "transcript_path": format!("/t/{AWS}.jsonl")}),
            "evidence[1].source.transcript_path",
        ),
        (
            json!({"type": "file", "path": format!("{AWS}.txt")}),
            "evidence[1].source.path",
        ),
    ];
    for (source, path) in cases {
        let args = json!({"evidence": [conv("a", "无害"), {"excerpt": "无害", "source": source}]});
        let secret = if path.ends_with("url") {
            "hunter2"
        } else {
            AWS
        };
        let msg = rejects(&a, "record", args, secret);
        assert!(msg.contains(path), "{msg}");
    }
}

#[test]
fn fetched_at_is_not_scanned() {
    let a = Almanac::new();
    a.tool(
        "record",
        json!({"evidence": [{"excerpt": "x", "source": {"type": "web", "url": "https://example.com", "fetched_at": AWS}}]}),
    )
    .ok();
}

#[test]
fn memory_text_is_scanned_and_only_the_first_hit_is_reported() {
    let a = Almanac::new();
    let msg = rejects(
        &a,
        "record",
        json!({
            "evidence": [conv("e", "记住部署 key")],
            "memories": [{"text": format!("部署用 {AWS} 和 sk-ant-abcdefghijklmnopqrstuvwxyz"), "scope": "global", "evidence": ["e"]}]
        }),
        AWS,
    );
    assert!(msg.contains("memories[0].text"), "{msg}");
    assert!(msg.contains("aws_access_key"), "{msg}");
    assert!(!msg.contains("anthropic_key"), "{msg}");
}

#[test]
fn secret_in_excerpt_beats_file_comparison() {
    let a = Almanac::new();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "nothing here").unwrap();
    rejects(
        &a,
        "record",
        json!({
            "project": dir.path().to_str().unwrap(),
            "evidence": [{"excerpt": AWS, "source": {"type": "file", "path": "a.txt"}}]
        }),
        AWS,
    );
}

#[test]
fn retract_reason_is_scanned() {
    let a = Almanac::new();
    let (_, m) = a.remember("用 cargo 构建");
    let msg = rejects(
        &a,
        "retract",
        json!({"memory": m, "reason": format!("泄露了 {AWS}")}),
        AWS,
    );
    assert!(msg.contains("reason"), "{msg}");
}

#[test]
fn corroborate_scans_inline_evidence_but_not_evidence_cited_by_id() {
    let a = Almanac::new();
    let (_, m) = a.remember("用 cargo 构建");
    let (e2, _) = a.remember("用 rustfmt 格式化");
    let msg = rejects(
        &a,
        "corroborate",
        json!({"memory": m, "evidence": [e2, {"excerpt": AWS, "source": {"type": "conversation"}}]}),
        AWS,
    );
    // The path indexes the call's `evidence` list, ids included.
    assert!(msg.contains("evidence[1].excerpt"), "{msg}");
}

#[test]
fn existing_evidence_cited_by_id_is_not_rescanned() {
    let a = Almanac::new();
    let (ev, m) = a.remember("用 cargo 构建");
    // Simulate a secret already in the store (written before a rule existed) by editing the log.
    let raw = a.log_raw().replace("记住：用 cargo 构建", AWS);
    std::fs::write(a.log_path(), raw).unwrap();
    let (_, m2) = a.remember("用 rustfmt 格式化");
    a.tool("corroborate", json!({"memory": m2, "evidence": [ev]}))
        .ok();
    a.tool(
        "record",
        json!({"memories": [{"text": "构建", "scope": "global", "evidence": [ev], "supersedes": [m]}]}),
    )
    .ok();
}

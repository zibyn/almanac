mod common;
use common::Almanac;
use serde_json::json;

#[test]
fn one_record_call_appends_one_transaction_line() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("用户喜欢 4 空格缩进");
    let raw = a.log_raw();
    assert!(raw.ends_with('\n'));
    let log = a.log();
    assert_eq!(log.len(), 1);
    let tx = &log[0];
    assert_eq!(tx["v"], 1);
    assert!(tx["tx"].as_str().unwrap().starts_with("tx_"));
    let at = tx["at"].as_str().unwrap();
    // UTC RFC 3339 with milliseconds: 2026-10-20T08:00:00.000Z
    assert_eq!(at.len(), 24, "{at}");
    assert!(at.ends_with('Z') && at.as_bytes()[19] == b'.', "{at}");
    assert_eq!(tx["writer"], json!({"name": "test"}));
    assert_eq!(
        tx["events"],
        json!([
            {"type": "evidence_recorded", "id": ev, "excerpt": "记住：用户喜欢 4 空格缩进",
             "source": {"type": "conversation", "host": "test"}},
            {"type": "memory_recorded", "id": mem, "text": "用户喜欢 4 空格缩进", "scope": "global",
             "epistemic": "user_stated", "evidence": [ev], "supersedes": []}
        ])
    );

    a.remember("用户用 Rust");
    assert_eq!(a.log().len(), 2);
}

#[test]
fn replay_ignores_unknown_fields_event_types_and_an_unterminated_last_line() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("用户喜欢 4 空格缩进");
    let mut raw = a.log_raw();
    raw = raw.replacen("\"v\":1", "\"v\":1,\"future_field\":true", 1);
    raw.push_str(
        r#"{"v":1,"tx":"tx_01JAX3Q8M6Z5K0V9W2R4T7Y1BC","at":"2026-10-20T08:00:00.000Z","writer":{"name":"x"},"events":[{"type":"future_event","foo":1}]}"#,
    );
    raw.push('\n');
    raw.push_str(r#"{"v":1,"tx":"tx_half"#);
    std::fs::write(a.log_path(), raw).unwrap();

    let g = a.tool("get", json!({"ids": [mem, ev]})).ok();
    assert_eq!(g["records"][0]["kind"], "memory");
    assert_eq!(g["records"][1]["cited_by"], json!([mem]));
}

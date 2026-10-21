mod common;
use common::Almanac;
use serde_json::json;

/// The log plus a torn line whose last byte cuts a multibyte character in half.
fn with_torn_tail(a: &Almanac) -> Vec<u8> {
    let mut raw = a.log_raw().into_bytes();
    raw.extend_from_slice(r#"{"v":1,"tx":"tx_half","events":[{"text":"记"#.as_bytes());
    raw.pop();
    std::fs::write(a.log_path(), &raw).unwrap();
    raw
}

fn record_one(a: &Almanac) -> common::Out {
    a.tool(
        "record",
        json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
    )
}

#[test]
fn the_next_write_truncates_a_torn_last_line_and_logs_it_to_stderr() {
    let a = Almanac::new();
    let (_, mem) = a.remember("用户喜欢 4 空格缩进");
    with_torn_tail(&a);

    let r = record_one(&a);
    r.ok();
    assert!(r.stderr.contains("残缺"), "{}", r.stderr);
    let log = a.log();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0]["events"][1]["id"], mem.as_str());
}

#[test]
fn a_terminated_but_invalid_last_line_is_also_a_torn_write() {
    let a = Almanac::new();
    a.remember("用户喜欢 4 空格缩进");
    let mut raw = a.log_raw();
    raw.push_str("{\"v\":1,\"tx\":\"tx_half\n");
    std::fs::write(a.log_path(), raw).unwrap();

    let r = record_one(&a);
    r.ok();
    assert!(r.stderr.contains("残缺"), "{}", r.stderr);
    assert_eq!(a.log().len(), 2);
}

#[test]
fn reads_leave_a_torn_last_line_alone() {
    let a = Almanac::new();
    let (_, mem) = a.remember("用户喜欢 4 空格缩进");
    let raw = with_torn_tail(&a);

    a.tool("get", json!({"ids": [mem]})).ok();
    assert_eq!(std::fs::read(a.log_path()).unwrap(), raw);
}

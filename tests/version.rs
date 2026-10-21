mod common;
use common::Almanac;
use serde_json::json;

#[test]
fn a_line_from_a_newer_log_version_makes_the_store_read_only() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("用户喜欢 4 空格缩进");
    let mut raw = a.log_raw();
    // A future format may change anything but `v`.
    raw.push_str(r#"{"v":2,"tx":"tx_future","shape":"unknown","events":{"type":"x"}}"#);
    raw.push('\n');
    std::fs::write(a.log_path(), &raw).unwrap();

    let e = a
        .tool(
            "record",
            json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
        )
        .err("STORE_NEWER");
    assert!(e["hint"].is_string(), "{e}");
    assert_eq!(a.log_raw(), raw);

    let g = a.tool("get", json!({"ids": [mem, ev]})).ok();
    assert_eq!(g["records"][0]["kind"], "memory");
    assert_eq!(g["records"][1]["cited_by"], json!([mem]));
}

#[test]
fn project_alias_refuses_a_store_with_a_newer_log_version() {
    let a = Almanac::new();
    let raw = "{\"v\":2,\"tx\":\"tx_future\",\"shape\":\"unknown\"}\n";
    std::fs::create_dir_all(a.log_path().parent().unwrap()).unwrap();
    std::fs::write(a.log_path(), raw).unwrap();
    let o = a.run(&["project", "alias", "a", "b"], None);
    assert_eq!(o.code, 1);
    assert!(o.stderr.starts_with("STORE_NEWER："), "{}", o.stderr);
    assert_eq!(a.log_raw(), raw);
}

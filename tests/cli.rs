mod common;
use common::Almanac;
use serde_json::json;

#[test]
fn json_dash_reads_args_from_stdin() {
    let a = Almanac::new();
    let (_, mem) = a.remember("用户喜欢 \"引号\" 与中文");
    let out = a
        .run(
            &["get", "--json", "-"],
            Some(&json!({"ids": [mem]}).to_string()),
        )
        .ok();
    assert_eq!(out["records"][0]["text"], "用户喜欢 \"引号\" 与中文");
}

#[test]
fn no_json_flag_means_empty_args() {
    // `get` with `{}` lacks `ids`, so it must fail as a tool (error JSON on stdout), not as a CLI usage error.
    let out = Almanac::new().run(&["get"], None);
    assert_eq!(out.code, 1);
    assert!(out.json()["error"]["code"].is_string());
}

#[test]
fn unknown_argument_field_is_invalid_args() {
    let a = Almanac::new();
    a.tool("get", json!({"ids": ["mem_x"], "bogus": 1}))
        .err("INVALID_ARGS");
    a.tool(
        "record",
        json!({"memories": [{"text": "t", "scope": "global", "evidence": ["e"], "bogus": 1}]}),
    )
    .err("INVALID_ARGS");
    a.tool(
        "record",
        json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation", "host": "forged"}}]}),
    )
    .err("INVALID_ARGS");
    assert_eq!(a.log_raw(), "", "failed writes must not touch the log");
}

#[test]
fn malformed_json_is_invalid_args() {
    Almanac::new()
        .run(&["get", "--json", "{not json"], None)
        .err("INVALID_ARGS");
}

#[test]
fn get_reports_not_found_only_at_that_position() {
    let a = Almanac::new();
    let (_, mem) = a.remember("用户喜欢 4 空格缩进");
    let g = a.tool("get", json!({"ids": ["mem_nope", mem]})).ok();
    assert_eq!(
        g["records"][0],
        json!({"id": "mem_nope", "error": "NOT_FOUND"})
    );
    assert_eq!(g["records"][1]["id"], mem);
}

#[test]
fn get_rejects_more_than_20_ids() {
    let a = Almanac::new();
    let ids: Vec<String> = (0..21).map(|i| format!("mem_{i}")).collect();
    let e = a.tool("get", json!({"ids": ids})).err("TOO_MANY_IDS");
    assert!(e["hint"].is_string());
    let ids: Vec<String> = (0..20).map(|i| format!("mem_{i}")).collect();
    a.tool("get", json!({"ids": ids})).ok();
}

#[test]
fn store_defaults_to_xdg_data_home() {
    let a = Almanac::new();
    let xdg = a.home.path().join("xdg");
    let a = a
        .unset("ALMANAC_HOME")
        .env("XDG_DATA_HOME", xdg.to_str().unwrap());
    a.remember("用户喜欢 4 空格缩进");
    assert!(xdg.join("almanac/log/events.jsonl").is_file());
}

#[test]
fn store_falls_back_to_home_local_share() {
    let a = Almanac::new();
    let home = a.home.path().join("h");
    let a = a.unset("ALMANAC_HOME").env("HOME", home.to_str().unwrap());
    a.remember("用户喜欢 4 空格缩进");
    assert!(home.join(".local/share/almanac/log/events.jsonl").is_file());
}

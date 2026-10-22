mod common;
use common::Almanac;
use serde_json::json;

#[test]
fn help_is_available_and_exits_zero() {
    let a = Almanac::new();
    for args in [&[][..], &["help"][..], &["--help"][..], &["-h"][..]] {
        let o = a.run(args, None);
        assert_eq!(o.code, 0, "{args:?}");
        assert!(o.stdout.contains("record"), "{args:?}: {}", o.stdout);
        assert!(o.stderr.is_empty(), "{args:?}: {}", o.stderr);
    }
}

#[test]
fn tool_help_lists_that_tools_parameters() {
    let a = Almanac::new();
    for tool in [
        "record",
        "get",
        "search",
        "review",
        "corroborate",
        "retract",
        "verify",
    ] {
        let o = a.run(&[tool, "--help"], None);
        assert_eq!(o.code, 0, "{tool}");
        assert!(o.stderr.is_empty(), "{tool}: {}", o.stderr);
        assert!(o.stdout.contains("参数"), "{tool}: {}", o.stdout);
    }
    let o = a.run(&["search", "--help"], None);
    for needle in ["kind", "source_type", "仅 kind=memory"] {
        assert!(o.stdout.contains(needle), "{needle}: {}", o.stdout);
    }
}

#[test]
fn version_prints_the_crate_version_and_exits_zero() {
    let o = Almanac::new().run(&["--version"], None);
    assert_eq!(o.code, 0);
    assert_eq!(
        o.stdout.trim(),
        concat!("almanac ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn unknown_command_is_still_invalid_args() {
    let o = Almanac::new().run(&["bogus"], None);
    assert_eq!(o.code, 1);
    assert!(
        o.stderr.starts_with("INVALID_ARGS：未知命令 bogus"),
        "{}",
        o.stderr
    );
}

#[test]
fn no_resolvable_store_is_a_store_io_error() {
    // The harness clears the environment, so HOME and XDG_DATA_HOME are unset too.
    let a = Almanac::new().unset("ALMANAC_HOME");
    a.tool("search", json!({"all": true})).err("STORE_IO");
}

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

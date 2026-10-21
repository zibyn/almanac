//! Writer inference, CLI table (spec ch. 9).
mod common;
use common::Almanac;
use serde_json::{Value, json};

fn host() -> Almanac {
    Almanac::new().unset("ALMANAC_WRITER")
}

/// Records one memory and returns (tx writer, conversation evidence source).
fn write(a: &Almanac) -> (Value, Value) {
    let (_, _) = a.remember("用户喜欢 4 空格缩进");
    let tx = a.log().pop().unwrap();
    (tx["writer"].clone(), tx["events"][0]["source"].clone())
}

#[test]
fn almanac_writer_wins_over_session_variables() {
    let a = host()
        .env("ALMANAC_WRITER", "  human ")
        .env("CODEX_THREAD_ID", "t1");
    let (w, src) = write(&a);
    assert_eq!(w, json!({"name": "human"}));
    assert_eq!(src, json!({"type": "conversation", "host": "human"}));
}

#[test]
fn blank_or_unknown_almanac_writer_counts_as_unset() {
    for v in ["   ", "unknown"] {
        let a = host()
            .env("ALMANAC_WRITER", v)
            .env("CLAUDE_CODE_SESSION_ID", "s1");
        assert_eq!(write(&a).0, json!({"name": "claude-code", "session": "s1"}));
    }
}

#[test]
fn claude_code_session_fills_writer_and_conversation_source() {
    let a = host().env("CLAUDE_CODE_SESSION_ID", "s1");
    let (w, src) = write(&a);
    assert_eq!(w, json!({"name": "claude-code", "session": "s1"}));
    assert_eq!(
        src,
        json!({"type": "conversation", "host": "claude-code", "session": "s1"})
    );
}

#[test]
fn explicit_conversation_session_is_kept() {
    let a = host().env("CLAUDE_CODE_SESSION_ID", "s1");
    a.tool(
        "record",
        json!({"evidence": [{"excerpt": "记住 x", "source": {"type": "conversation", "session": "other"}}]}),
    )
    .ok();
    assert_eq!(a.log()[0]["events"][0]["source"]["session"], "other");
}

#[test]
fn codex_prefers_thread_id_then_session_id() {
    let a = host()
        .env("CODEX_THREAD_ID", "t1")
        .env("CODEX_SESSION_ID", "s1");
    assert_eq!(write(&a).0, json!({"name": "codex", "session": "t1"}));
    let a = host().env("CODEX_SESSION_ID", "s1");
    assert_eq!(write(&a).0, json!({"name": "codex", "session": "s1"}));
}

#[test]
fn pi_session_attaches_model() {
    let a = host()
        .env("PI_SESSION_ID", "p1")
        .env("PI_PROVIDER", "anthropic")
        .env("PI_MODEL", "claude");
    assert_eq!(
        write(&a).0,
        json!({"name": "pi", "session": "p1", "model": "anthropic/claude"})
    );
}

#[test]
fn pi_coding_agent_is_the_fallback() {
    let a = host().env("PI_CODING_AGENT", "true");
    assert_eq!(write(&a).0, json!({"name": "pi"}));
}

#[test]
fn explicit_host_name_still_gets_its_session() {
    let a = host()
        .env("ALMANAC_WRITER", "claude-code")
        .env("CLAUDE_CODE_SESSION_ID", "s1");
    assert_eq!(write(&a).0, json!({"name": "claude-code", "session": "s1"}));
}

#[test]
fn ignored_signals_do_not_identify_a_host() {
    let a = host()
        .env("CLAUDECODE", "1")
        .env("AI_AGENT", "x")
        .env("CODEX_CI", "1");
    a.tool(
        "record",
        json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
    )
    .err("NO_WRITER");
}

#[test]
fn no_host_detected_is_no_writer_with_retry_hint() {
    let a = host();
    let e = a
        .tool(
            "record",
            json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
        )
        .err("NO_WRITER");
    let hint = e["hint"].as_str().unwrap();
    assert!(
        hint.contains("ALMANAC_WRITER=") && hint.contains("human"),
        "{hint}"
    );
    assert_eq!(a.log_raw(), "");
}

#[test]
fn nested_hosts_are_no_writer_listing_the_hosts() {
    // PI_CODING_AGENT does not break the tie: level 3 only applies when level 2 matched nothing.
    let a = host()
        .env("CLAUDE_CODE_SESSION_ID", "s1")
        .env("CODEX_THREAD_ID", "t1")
        .env("PI_CODING_AGENT", "true");
    let e = a
        .tool(
            "record",
            json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
        )
        .err("NO_WRITER");
    let text = format!("{} {}", e["message"], e["hint"]);
    assert!(
        text.contains("claude-code") && text.contains("codex"),
        "{text}"
    );
    assert!(e["hint"].as_str().unwrap().contains("ALMANAC_WRITER="));
}

#[test]
fn reads_need_no_writer() {
    let a = Almanac::new();
    let (_, mem) = a.remember("用户喜欢 4 空格缩进");
    let a = a.unset("ALMANAC_WRITER");
    a.tool("get", json!({"ids": [mem]})).ok();
}

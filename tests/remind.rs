mod common;
use common::{Almanac, Out};

const REASON: &str = "almanac：回顾本次会话，有没有以后还会用到、有用户原话或文件支撑的论断（约定、偏好、踩过的坑）？有就 `record`，情况变了用 `supersedes`；没有就什么都不做，直接结束，不要回复这条提醒。";

fn turn(a: &Almanac, stdin: &str) -> Out {
    a.run(&["remind"], Some(stdin))
}

fn stop(a: &Almanac, session: &str) -> Out {
    turn(
        a,
        &format!(r#"{{"session_id":"{session}","stop_hook_active":false}}"#),
    )
}

fn with_every(n: &str) -> Almanac {
    let a = Almanac::new();
    std::fs::write(
        a.home.path().join("config.toml"),
        format!("remind_every = {n}\n"),
    )
    .unwrap();
    a
}

fn assert_silent(o: &Out) {
    assert_eq!((o.code, o.stdout.as_str(), o.stderr.as_str()), (0, "", ""));
}

fn assert_reminds(o: &Out) {
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert_eq!(
        o.json(),
        serde_json::json!({"decision": "block", "reason": REASON})
    );
}

#[test]
fn reminds_on_every_nth_turn_only() {
    let a = with_every("2");
    assert_silent(&stop(&a, "s1"));
    assert_reminds(&stop(&a, "s1"));
    assert_silent(&stop(&a, "s1"));
    assert_reminds(&stop(&a, "s1"));
}

#[test]
fn sessions_count_separately() {
    let a = with_every("2");
    assert_silent(&stop(&a, "s1"));
    assert_silent(&stop(&a, "s2"));
    assert_reminds(&stop(&a, "s1"));
}

#[test]
fn default_interval_is_15_and_needs_no_writer() {
    let a = Almanac::new().unset("ALMANAC_WRITER");
    for _ in 0..14 {
        assert_silent(&stop(&a, "s"));
    }
    assert_reminds(&stop(&a, "s"));
    assert_eq!(a.log_raw(), "", "remind must not write the log");
}

#[test]
fn zero_interval_never_reminds() {
    let a = with_every("0");
    for _ in 0..20 {
        assert_silent(&stop(&a, "s"));
    }
}

#[test]
fn hook_continuation_is_neither_counted_nor_reminded() {
    let a = with_every("2");
    assert_silent(&stop(&a, "s"));
    let cont = r#"{"session_id":"s","stop_hook_active":true}"#;
    assert_silent(&turn(&a, cont));
    assert_silent(&turn(&a, cont));
    assert_reminds(&stop(&a, "s"));
}

#[test]
fn lost_counter_restarts_from_zero() {
    let a = with_every("2");
    assert_silent(&stop(&a, "s"));
    std::fs::remove_dir_all(a.home.path().join("cache")).unwrap();
    assert_silent(&stop(&a, "s"));
    assert_reminds(&stop(&a, "s"));
}

#[test]
fn failures_are_silent_exit_zero() {
    let a = with_every("1");
    assert_silent(&turn(&a, "not json"));
    assert_silent(&turn(&a, r#"{"stop_hook_active":false}"#));
    assert_silent(&turn(&a, ""));

    let missing = Almanac::new();
    let gone = missing.home.path().join("nope");
    let missing = missing.env("ALMANAC_HOME", gone.to_str().unwrap());
    assert_silent(&stop(&missing, "s"));
    assert!(!gone.exists(), "remind must not create the store");

    // cache/ is a file, so the counter cannot be written.
    let blocked = with_every("1");
    std::fs::write(blocked.home.path().join("cache"), "").unwrap();
    assert_silent(&stop(&blocked, "s"));
}

#[test]
fn session_id_cannot_escape_the_cache_dir() {
    let a = with_every("1");
    for bad in ["../../escape", "..", "", "a/b"] {
        assert_silent(&stop(&a, bad));
    }
    assert!(!a.home.path().join("escape").exists());
    assert!(!a.home.path().join("cache/remind/a").exists());
}

mod common;
use common::Almanac;
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn a_write_waits_up_to_10_seconds_for_the_lock_then_fails_with_store_busy() {
    let a = Almanac::new();
    let lock = std::fs::File::create(a.home.path().join("lock")).unwrap();
    lock.lock().unwrap();

    let start = Instant::now();
    let r = a.tool(
        "record",
        json!({"evidence": [{"excerpt": "x", "source": {"type": "conversation"}}]}),
    );
    let waited = start.elapsed();
    let e = r.err("STORE_BUSY");
    assert!(e["hint"].is_string(), "{e}");
    assert!(waited >= Duration::from_secs(10), "{waited:?}");
    assert!(waited < Duration::from_secs(15), "{waited:?}");
    assert_eq!(a.log_raw(), "");

    // Reads take no lock.
    a.tool("get", json!({"ids": ["mem_x"]})).ok();
}

#[test]
fn concurrent_writers_from_many_processes_each_append_one_whole_line() {
    let a = Almanac::new();
    let n = 16;
    std::thread::scope(|s| {
        for i in 0..n {
            let a = &a;
            s.spawn(move || a.remember(&format!("并发第 {i} 条 {}", "长".repeat(4000))));
        }
    });
    assert!(a.log_raw().ends_with('\n'));
    assert_eq!(a.log().len(), n); // `log()` panics on any line that is not valid JSON
}

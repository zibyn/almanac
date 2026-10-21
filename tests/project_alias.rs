mod common;
use common::Almanac;
use serde_json::json;

fn alias(a: &Almanac, x: &str, y: &str) -> common::Out {
    a.run(&["project", "alias", x, y], None)
}

#[test]
fn alias_writes_a_project_aliased_event_as_human() {
    let a = Almanac::new();
    let o = alias(&a, "github.com/old/repo", "github.com/new/repo");
    assert_eq!(o.code, 0, "{}", o.stderr);
    let log = a.log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0]["writer"], json!({"name": "human"}));
    assert_eq!(
        log[0]["events"],
        json!([{"type": "project_aliased", "projects": ["github.com/old/repo", "github.com/new/repo"]}])
    );
}

#[test]
fn alias_needs_two_different_non_empty_identities() {
    let a = Almanac::new();
    for (x, y) in [("p", "p"), ("", "p"), ("p", " ")] {
        let o = alias(&a, x, y);
        assert_eq!(o.code, 1);
        assert!(o.stderr.starts_with("INVALID_ARGS："), "{}", o.stderr);
    }
    let o = a.run(&["project", "alias", "only-one"], None);
    assert_eq!(o.code, 1);
    assert!(o.stderr.starts_with("INVALID_ARGS："), "{}", o.stderr);
    assert_eq!(a.log_raw(), "");
}

#[test]
fn alias_within_the_same_group_writes_nothing() {
    let a = Almanac::new();
    assert_eq!(alias(&a, "a", "b").code, 0);
    assert_eq!(alias(&a, "b", "c").code, 0);
    let before = a.log_raw();
    let o = alias(&a, "c", "a");
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        !o.stdout.trim().is_empty(),
        "should tell the human why nothing was written"
    );
    assert_eq!(a.log_raw(), before);
}

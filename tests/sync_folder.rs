mod common;
use common::Almanac;
use serde_json::json;

fn get_with_home(home: &std::path::Path) -> common::Out {
    Almanac::new()
        .env("ALMANAC_HOME", home.to_str().unwrap())
        .tool("get", json!({"ids": ["mem_x"]}))
}

#[test]
fn a_store_inside_a_common_sync_folder_warns_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    for synced in [
        "Dropbox/almanac",
        "OneDrive - Contoso/almanac",
        "Library/Mobile Documents/com~apple~CloudDocs/almanac",
        "Library/CloudStorage/GoogleDrive-me/almanac",
    ] {
        let r = get_with_home(&dir.path().join(synced));
        r.ok();
        assert!(r.stderr.contains("同步盘"), "{synced}: {}", r.stderr);
    }
}

#[test]
fn a_store_on_a_plain_local_path_does_not_warn() {
    let dir = tempfile::tempdir().unwrap();
    let r = get_with_home(&dir.path().join("data/almanac"));
    r.ok();
    assert_eq!(r.stderr, "");
}

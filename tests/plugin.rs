//! 插件文件与第 9 章一致：marketplace、manifest、hooks.json、Pi 的 package.json、SKILL.md。
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(rel: &str) -> Value {
    let text = std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

#[test]
fn hooks_json_matches_spec() {
    assert_eq!(
        read_json("plugins/almanac/hooks/hooks.json"),
        json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup|clear|compact",
                    "hooks": [{ "type": "command", "command": "almanac context" }]
                }],
                "Stop": [{ "hooks": [{ "type": "command", "command": "almanac remind" }] }]
            }
        })
    );
}

#[test]
fn marketplace_entry_points_at_the_shared_plugin() {
    let m = read_json(".claude-plugin/marketplace.json");
    assert_eq!(m["name"], "almanac");
    let plugins = m["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    let entry = &plugins[0];
    assert_eq!(entry["name"], "almanac");
    assert_eq!(entry["source"], "./plugins/almanac");
    // Codex 要求的字段
    assert!(entry["category"].is_string());
    assert_eq!(entry["policy"]["installation"], "AVAILABLE");

    let manifest = read_json("plugins/almanac/.claude-plugin/plugin.json");
    assert_eq!(manifest["name"], entry["name"]);
    assert!(manifest.get("mcpServers").is_none(), "插件不含 MCP");
}

#[test]
fn package_json_has_only_pi_key_with_existing_paths() {
    let p = read_json("package.json");
    let obj = p.as_object().unwrap();
    assert_eq!(obj.keys().collect::<Vec<_>>(), ["pi"]);
    assert_eq!(
        p["pi"],
        json!({ "skills": ["./plugins/almanac/skills"], "extensions": ["./plugins/almanac/pi/almanac.ts"] })
    );
    for path in [
        "./plugins/almanac/skills",
        "./plugins/almanac/pi/almanac.ts",
    ] {
        assert!(root().join(path).exists(), "{path}");
    }
}

#[test]
fn skill_frontmatter_names_its_directory() {
    let text =
        std::fs::read_to_string(root().join("plugins/almanac/skills/almanac/SKILL.md")).unwrap();
    let front = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("SKILL.md 以 frontmatter 开头")
        .0;
    assert!(front.lines().any(|l| l == "name: almanac"));
    let desc = front
        .lines()
        .find_map(|l| l.strip_prefix("description: "))
        .expect("description");
    assert!(!desc.is_empty() && desc.chars().count() <= 1024);
}

fn assert_no_symlinks(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        assert!(!ty.is_symlink(), "符号链接：{}", entry.path().display());
        if ty.is_dir() {
            assert_no_symlinks(&entry.path());
        }
    }
}

#[test]
fn plugin_tree_has_no_symlinks() {
    assert_no_symlinks(&root().join("plugins"));
    assert_no_symlinks(&root().join(".claude-plugin"));
}

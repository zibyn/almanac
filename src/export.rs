//! `almanac export [--all]` (spec ch. 4): the store as read-only Markdown for humans.
//! Never checks sources or touches the network; the stale flag comes from the local cache only.

use crate::error::ToolError;
use crate::source_check as sc;
use crate::state::State;
use crate::tools::search::last_at;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Runs `export …` with the arguments after `export`; returns the exit code.
pub fn run(args: &[String]) -> i32 {
    match export(args) {
        Ok(md) => {
            print!("{md}");
            0
        }
        Err(e) => {
            eprintln!("{}：{}", e.code, e.message);
            1
        }
    }
}

fn export(args: &[String]) -> Result<String, ToolError> {
    let all = match args {
        [] => false,
        [a] if a == "--all" => true,
        _ => return Err(ToolError::invalid_args("用法 almanac export [--all]")),
    };
    let home = crate::log::home();
    let s = State::replay(&crate::log::replay(&home).map_err(crate::log::io_error)?);
    let cache = sc::load_cache(&home);
    let shown = |id: &&String| all || matches!(s.status(id), "active" | "unsupported");

    // Section key: "" for global (sorts first), else the alias group's identities, sorted.
    let mut sections: BTreeMap<Vec<String>, Vec<&String>> = BTreeMap::new();
    for id in s.memories.keys().filter(shown) {
        let key = match &s.memories[id].0.project {
            Some(p) if s.memories[id].0.scope == "project" => {
                let mut g: Vec<String> = s.alias_group(p).into_iter().collect();
                g.sort();
                g
            }
            _ => vec![],
        };
        sections.entry(key).or_default().push(id);
    }

    let mut out = format!(
        "# almanac 记忆导出（{}）\n",
        chrono::Utc::now().format("%Y-%m-%d")
    );
    for (key, mut ids) in sections {
        let title = if key.is_empty() {
            "全局".to_string()
        } else {
            format!("项目 {}", key.join("、"))
        };
        write!(out, "\n## {title}\n").unwrap();
        ids.sort_by_key(|id| Reverse((last_at(&s, id), *id)));
        for id in ids {
            memory(&mut out, &s, &cache, id);
        }
    }
    Ok(out)
}

fn memory(out: &mut String, s: &State, cache: &sc::Cache, id: &str) {
    let m = &s.memories[id].0;
    let epistemic = match m.epistemic.as_str() {
        "observed" => "亲见",
        "user_stated" => "用户陈述",
        "inferred" => "推断",
        other => other,
    };
    write!(out, "\n### {id} · {epistemic} · {}", &last_at(s, id)[..10]).unwrap();
    if !sc::stale(s, cache, id).is_null() {
        out.push_str(" · 可能过期");
    }
    if web_sourced(s, id) {
        out.push_str(" · 网页来源");
    }
    match s.status(id) {
        "unsupported" => out.push_str(" · 无据"),
        "superseded" => write!(out, " · 被取代 → {}", s.superseded_by[id].join(", ")).unwrap(),
        "retracted" => out.push_str(" · 撤回"),
        "purged" => out.push_str(" · 已清除"),
        _ => {}
    }
    // Purge also tombstones the retraction reason (spec ch. 1), so both show the tombstone.
    let text = m.purged.as_ref().map_or(m.text.clone(), tombstone);
    writeln!(out, "\n{text}").unwrap();
    if let Some((reason, _)) = s.retractions.get(id) {
        let reason = m.purged.as_ref().map_or(reason.clone(), tombstone);
        writeln!(out, "撤回原因：{reason}").unwrap();
    }
    for e in s.current_evidence(id) {
        let Some((ev, _)) = s.evidence.get(&e) else {
            continue;
        };
        let src = &ev.source;
        let str_of = |k: &str| src[k].as_str().unwrap_or("");
        let ty = match str_of("type") {
            "conversation" => "对话",
            "file" => "文件",
            "web" => "网页",
            "command" => "命令输出",
            other => other,
        };
        // A purged source keeps only its type, so the locator is gone too.
        let label = match (&ev.purged, str_of("type")) {
            (Some(_), _) => ty.to_string(),
            (None, "conversation") => format!("对话 · {}", str_of("host")),
            (None, "file") => format!("文件 {} · {}", str_of("path"), str_of("project")),
            (None, "web") => format!("网页 · {}", str_of("url")),
            (None, "command") => format!("命令输出 · {}", str_of("command")),
            (None, _) => ty.to_string(),
        };
        writeln!(out, "- {e}（{label}）：").unwrap();
        let excerpt = ev.purged.as_ref().map_or(ev.excerpt.clone(), tombstone);
        for line in excerpt.lines() {
            writeln!(out, "  > {line}").unwrap();
        }
    }
}

/// Spec ch. 1 「来源类型集合」: cites `web` evidence but no `conversation` evidence.
fn web_sourced(s: &State, id: &str) -> bool {
    let types: Vec<&str> = s
        .current_evidence(id)
        .iter()
        .filter_map(|e| s.evidence.get(e)?.0.source["type"].as_str())
        .collect();
    types.contains(&"web") && !types.contains(&"conversation")
}

/// A tombstone `{at, reason, by}` as one line: `已清除：<reason>（<YYYY-MM-DD>）`.
fn tombstone(t: &serde_json::Value) -> String {
    let at = t["at"].as_str().unwrap_or("");
    let reason = t["reason"].as_str().unwrap_or("");
    format!("已清除：{reason}（{}）", at.get(..10).unwrap_or(at))
}

//! `almanac export [--all]` (spec ch. 4): the store as read-only Markdown for humans.
//! Never checks sources or touches the network; the stale flag comes from the local cache only.

use crate::error::ToolError;
use crate::model::{day, epistemic_label, source_type_label};
use crate::source_check as sc;
use crate::state::State;
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
    let home = crate::log::home()?;
    let s = crate::log::load(&home)?;
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
        ids.sort_by_key(|id| Reverse((s.last_at(id), *id)));
        for id in ids {
            memory(&mut out, &s, &cache, id);
        }
    }
    Ok(out)
}

fn memory(out: &mut String, s: &State, cache: &sc::Cache, id: &str) {
    let m = &s.memories[id].0;
    write!(
        out,
        "\n### {id} · {} · {}",
        epistemic_label(&m.epistemic),
        day(s.last_at(id))
    )
    .unwrap();
    if !sc::stale(s, cache, id).is_null() {
        out.push_str(" · 可能过期");
    }
    if s.web_sourced(id) {
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
    let text = m.purged.as_ref().map_or(m.text.clone(), tombstone_line);
    writeln!(out, "\n{text}").unwrap();
    if let Some((reason, _)) = s.retractions.get(id) {
        let reason = m.purged.as_ref().map_or(reason.clone(), tombstone_line);
        writeln!(out, "撤回原因：{reason}").unwrap();
    }
    for e in s.current_evidence(id) {
        let Some((ev, _)) = s.evidence.get(&e) else {
            continue;
        };
        let src = &ev.source;
        let str_of = |k: &str| src[k].as_str().unwrap_or("");
        let ty = source_type_label(str_of("type"));
        // A purged source keeps only its type, so the locator is gone too.
        let label = match (&ev.purged, str_of("type")) {
            (None, "conversation") => format!("{ty} · {}", str_of("host")),
            (None, "file") => format!("{ty} {} · {}", str_of("path"), str_of("project")),
            (None, "web") => format!("{ty} · {}", str_of("url")),
            (None, "command") => format!("{ty} · {}", str_of("command")),
            _ => ty.to_string(),
        };
        writeln!(out, "- {e}（{label}）：").unwrap();
        let excerpt = ev
            .purged
            .as_ref()
            .map_or(ev.excerpt.clone(), tombstone_line);
        for line in excerpt.lines() {
            writeln!(out, "  > {line}").unwrap();
        }
    }
}

/// A tombstone `{at, reason, by}` as one line: `已清除：<reason>（<YYYY-MM-DD>）`.
fn tombstone_line(t: &serde_json::Value) -> String {
    let at = t["at"].as_str().unwrap_or("");
    let reason = t["reason"].as_str().unwrap_or("");
    format!("已清除：{reason}（{}）", day(at))
}

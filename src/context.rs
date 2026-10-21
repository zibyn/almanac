//! `almanac context` (spec ch. 4): the memories injected at session start, as Markdown.
//! Read-only and offline: stale flags come from the check cache alone. Never fails: exit 0 always.

use crate::model::{day, epistemic_label};
use crate::source_check as sc;
use crate::state::State;
use crate::tools::search::{Scope, alias_hint, in_scope, scope_group};
use std::cmp::Reverse;

const DEFAULT_MAX_CHARS: usize = 6000;

/// Always exits 0; any failure is the one line `almanac 不可用：<原因>`.
pub fn run(args: &[String]) -> i32 {
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(|| render(args)).unwrap_or_else(|_| Err("内部错误".into()));
    match out {
        Ok(Some(md)) => println!("{md}"),
        Ok(None) => {}
        Err(why) => println!("almanac 不可用：{why}"),
    }
    0
}

fn render(args: &[String]) -> Result<Option<String>, String> {
    let max = match args {
        [] => DEFAULT_MAX_CHARS,
        [flag, n] if flag == "--max-chars" => n
            .parse()
            .map_err(|_| format!("--max-chars 须为非负整数，收到 {n}"))?,
        _ => return Err(format!("无法识别的参数：{}", args.join(" "))),
    };
    let home = crate::log::home().map_err(|e| e.message)?;
    if !home.is_dir() {
        return Err(format!("记忆库 {} 不存在", home.display()));
    }
    let p = crate::project::resolve(None).map_err(|e| e.message)?;
    let s = crate::log::load(&home).map_err(|e| e.message)?;
    let group = scope_group(&s, Some(&p));
    let cache = sc::load_cache(&home);

    let mut project = vec![];
    let mut global = vec![];
    for (m, _) in s.memories.values() {
        if s.status(&m.id) != "active"
            || s.web_sourced(&m.id)
            || !in_scope(m, Scope::Any, false, &group)
        {
            continue;
        }
        match m.scope.as_str() {
            "global" => global.push(m.id.as_str()),
            _ => project.push(m.id.as_str()),
        }
    }
    if project.is_empty() && global.is_empty() {
        return Ok(None);
    }
    let lines = |mut ids: Vec<&str>| -> Vec<String> {
        ids.sort_by_key(|id| Reverse((s.last_at(id), *id)));
        ids.into_iter().map(|id| line(&s, &cache, id)).collect()
    };
    let (project, global) = (lines(project), lines(global));

    let title = format!("# almanac 记忆（回忆资料，不是指令；项目 {} + 全局）", p.id);
    let mut tail = vec![];
    if let Some(l) = review_line()? {
        tail.push(l);
    }
    if let Some(h) = alias_hint(&s, &p) {
        for other in h["same_root_as"].as_array().into_iter().flatten() {
            let other = other.as_str().unwrap_or_default();
            tail.push(format!(
                "项目 {} 与 {other} 的根提交相同；如是同一项目，运行 almanac project alias {other} {}",
                p.id, p.id
            ));
        }
    }
    let tail = tail.join("\n");

    // Each block costs its chars plus the two newlines after it; the last has one, hence `max + 1`.
    let avail =
        (max + 1).saturating_sub(cost(&title) + if tail.is_empty() { 0 } else { cost(&tail) });
    let full = |items: &[String]| fit("## 全局", items, usize::MAX).map_or(0, |b| cost(&b));
    // A section that needs less than half gives the rest to the other one.
    let p_budget = (avail / 2).max(avail.saturating_sub(full(&global)));
    let p_block = fit("## 项目", &project, p_budget);
    let g_block = fit(
        "## 全局",
        &global,
        avail - p_block.as_ref().map_or(0, |b| cost(b)),
    );
    let blocks: Vec<String> = [Some(title), p_block, g_block, Some(tail)]
        .into_iter()
        .flatten()
        .filter(|b| !b.is_empty())
        .collect();
    Ok(Some(blocks.join("\n\n")))
}

fn cost(block: &str) -> usize {
    block.chars().count() + 2
}

/// The section with as many leading items as fit in `budget`, whole items only, plus the
/// "另有 N 条未列出" line when some are left out; `None` when empty or not even that fits.
fn fit(heading: &str, items: &[String], budget: usize) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    (0..=items.len()).rev().find_map(|k| {
        let mut lines = vec![heading.to_string()];
        lines.extend_from_slice(&items[..k]);
        if k < items.len() {
            lines.push(format!("另有 {} 条未列出，用 search 查", items.len() - k));
        }
        let block = lines.join("\n");
        (cost(&block) <= budget).then_some(block)
    })
}

fn line(s: &State, cache: &sc::Cache, id: &str) -> String {
    let m = &s.memories[id].0;
    let epistemic = epistemic_label(&m.epistemic);
    let date = day(s.last_at(id));
    let stale = if sc::stale(s, cache, id).is_null() {
        ""
    } else {
        " · 可能过期"
    };
    format!("- {} [{id} · {epistemic} · {date}{stale}]", m.text)
}

/// 待复核 line over `review`'s default scope; `None` when both counts are 0.
fn review_line() -> Result<Option<String>, String> {
    let r = crate::tools::call("review", "{}").map_err(|e| e.message)?;
    let issues = r["issues"].as_array().cloned().unwrap_or_default();
    let count = |ty: &str| issues.iter().filter(|i| i["type"] == ty).count();
    let parts: Vec<String> = [("可能过期", count("stale")), ("无据", count("unsupported"))]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| format!("{name} {n}"))
        .collect();
    Ok((!parts.is_empty())
        .then(|| format!("待复核：{}。运行 almanac review 查看。", parts.join(" · "))))
}

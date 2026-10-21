//! `almanac purge <id> --reason <原因>` (spec ch. 4): a human, at a terminal, erases a piece of
//! evidence or a memory's text, leaving a tombstone. The one rewrite of the log (ADR 0001).

use crate::error::ToolError;
use crate::log;
use crate::state::State;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Write};

/// Runs `purge …` with the arguments after `purge`; returns the exit code.
pub fn run(args: &[String]) -> i32 {
    match purge(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}：{}", e.code, e.message);
            1
        }
    }
}

fn purge(args: &[String]) -> Result<(), ToolError> {
    if !std::io::stdin().is_terminal() {
        return Err(ToolError::new("NOT_INTERACTIVE", "purge 的 stdin 不是终端")
            .hint("清除须由人在终端里运行"));
    }
    let (id, reason) = match args {
        [id, flag, r] | [flag, r, id] if flag == "--reason" => (id.as_str(), r.trim()),
        _ => {
            return Err(ToolError::invalid_args(
                "用法 almanac purge <id> --reason <原因>",
            ));
        }
    };
    if reason.is_empty() {
        return Err(ToolError::new("MISSING_FIELD", "--reason 不能为空"));
    }
    let home = log::home()?;
    log::warn_if_synced(&home);
    log::ensure_writable(&home)?;
    let state = log::load(&home)?;
    check_purgeable(&state, id)?;

    // Steps 1–3 (spec ch. 4): show, ask about each related object, confirm. No lock while a human types.
    println!("{}", describe(&state, id));
    let mut chosen = vec![id.to_string()];
    for other in related(&state, id) {
        print!("一并清除 {}？[y/N] ", describe(&state, &other));
        if matches!(read_line().to_lowercase().as_str(), "y" | "yes") {
            chosen.push(other);
        }
    }
    println!("将清除以下 {} 个对象，原因：{reason}", chosen.len());
    for c in &chosen {
        println!("- {c}");
    }
    print!("输入 yes 确认：");
    if read_line() != "yes" {
        println!("未确认，没有做任何改动。");
        return Ok(());
    }

    // Step 4 (spec ch. 2 「清除步骤」).
    let _lock = log::lock(&home)?;
    log::ensure_writable(&home)?;
    let state = log::load(&home)?;
    for c in &chosen {
        check_purgeable(&state, c)?;
    }
    let tomb = json!({ "at": crate::model::now(), "reason": reason, "by": { "name": "human" } });
    log::rewrite(&home, |ev| tombstone_event(ev, &chosen, &tomb))?;
    match std::fs::remove_dir_all(home.join("cache")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(log::io_error(e)),
        _ => {}
    }
    println!(
        "已清除 {} 个对象。提醒：外部备份里的副本需要你自己处理。",
        chosen.len()
    );
    Ok(())
}

fn check_purgeable(s: &State, id: &str) -> Result<(), ToolError> {
    if s.memory_purged(id) || s.evidence_purged(id) {
        Err(ToolError::new("ALREADY_PURGED", format!("{id} 已被清除")))
    } else if s.memories.contains_key(id) || s.evidence.contains_key(id) {
        Ok(())
    } else if id.starts_with("ev_") {
        Err(ToolError::new(
            "EVIDENCE_NOT_FOUND",
            format!("证据 {id} 不存在"),
        ))
    } else {
        crate::tools::check_exists(s, id)
    }
}

/// Objects that may restate the same content, one level deep and not yet purged (spec ch. 4 step 2).
fn related(s: &State, id: &str) -> Vec<String> {
    let mut out: Vec<String> = if s.evidence.contains_key(id) {
        let citing = s.cited_by.get(id).cloned().unwrap_or_default();
        citing
            .iter()
            .flat_map(|m| std::iter::once(m.clone()).chain(s.chain(m)))
            .collect()
    } else {
        s.current_evidence(id)
            .into_iter()
            .chain(s.chain(id))
            .collect()
    };
    let mut seen = std::collections::HashSet::new();
    out.retain(|x| x != id && seen.insert(x.clone()) && check_purgeable(s, x).is_ok());
    out
}

fn describe(s: &State, id: &str) -> String {
    if let Some((e, _)) = s.evidence.get(id) {
        let excerpt: String = e.excerpt.lines().map(|l| format!("\n  > {l}")).collect();
        format!("证据 {id}（来源 {}）：{excerpt}", e.source)
    } else {
        let (m, _) = &s.memories[id];
        format!("记忆 {id}（{}）：{}", s.status(id), m.text)
    }
}

/// Spec ch. 1 「清除与墓碑」: replaces the text-bearing fields of the chosen objects' events.
fn tombstone_event(ev: &mut Value, chosen: &[String], tomb: &Value) -> bool {
    let target = |k: &str| {
        ev[k]
            .as_str()
            .is_some_and(|x| chosen.iter().any(|c| c == x))
    };
    let (field, ty_only) = match ev["type"].as_str() {
        Some("evidence_recorded") if target("id") => ("excerpt", true),
        Some("memory_recorded") if target("id") => ("text", false),
        Some("retracted") if target("memory") => ("reason", false),
        _ => return false,
    };
    let o = ev.as_object_mut().unwrap();
    o.remove(field);
    if ty_only {
        let ty = o["source"]["type"].clone();
        o.insert("source".into(), json!({ "type": ty }));
    }
    o.insert("purged".into(), tomb.clone());
    true
}

/// One answer from the terminal, trimmed; EOF reads as "".
fn read_line() -> String {
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s).ok();
    s.trim().to_string()
}

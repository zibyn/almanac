//! `get`: full records by id (spec ch. 3).

use crate::error::ToolError;
use crate::state::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::source_check::{self as sc, Cache};

const MAX_IDS: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    ids: Vec<String>,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    if args.ids.len() > MAX_IDS {
        return Err(ToolError::new(
            "TOO_MANY_IDS",
            format!("ids 有 {} 个，最多 {MAX_IDS} 个", args.ids.len()),
        )
        .hint("分批取"));
    }
    if args.ids.is_empty() {
        return Err(ToolError::invalid_args("ids 至少要有 1 个"));
    }
    let s = super::load()?;
    // Lazy check (spec ch. 3): file evidence the returned memories currently cite; no network.
    let cur = crate::project::resolve(args.project.as_deref()).ok();
    let cited: Vec<String> = args
        .ids
        .iter()
        .filter(|id| s.memories.contains_key(*id))
        .flat_map(|id| s.current_evidence(id))
        .collect();
    let (cache, _, _) = sc::refresh(
        &crate::log::home(),
        &s,
        sc::here(&s, cur.as_ref()).as_ref(),
        cited.iter().map(String::as_str),
        &["file"],
    );
    let records: Vec<Value> = args
        .ids
        .iter()
        .map(|id| {
            memory(&s, &cache, id)
                .or_else(|| evidence(&s, &cache, id, true))
                .unwrap_or_else(|| json!({ "id": id, "error": "NOT_FOUND" }))
        })
        .collect();
    Ok(json!({ "records": records }))
}

fn memory(s: &State, cache: &Cache, id: &str) -> Option<Value> {
    let (m, recorded) = s.memories.get(id)?;
    let mut v = json!({
        "id": m.id,
        "kind": "memory",
        "text": m.text,
        "scope": m.scope,
        "epistemic": m.epistemic,
        "status": s.status(id),
        "recorded": recorded,
        "evidence": s.current_evidence(id).iter().filter_map(|e| evidence(s, cache, e, false)).collect::<Vec<_>>(),
        "supersedes": m.supersedes,
        "superseded_by": s.superseded_by.get(id).cloned().unwrap_or_default(),
        "corroborations": s.corroborations.get(id).into_iter().flatten()
            .map(|(ev, recorded)| json!({ "evidence": ev, "recorded": recorded }))
            .collect::<Vec<_>>(),
        // A purged memory's retraction reason is the same tombstone (spec ch. 3).
        "retraction": s.retractions.get(id).map(|(reason, recorded)| match &m.purged {
            Some(t) => json!({ "purged": t, "recorded": recorded }),
            None => json!({ "reason": reason, "recorded": recorded }),
        }),
        "stale": sc::stale(s, cache, id),
    });
    super::tombstone(&mut v, "text", &m.purged);
    if let Some(p) = &m.project {
        v["project"] = json!(p);
    }
    if let Some(r) = &m.root_commit {
        v["root_commit"] = json!(r);
    }
    Some(v)
}

/// `with_cited_by` is false when nested inside a memory.
fn evidence(s: &State, cache: &Cache, id: &str, with_cited_by: bool) -> Option<Value> {
    let (e, recorded) = s.evidence.get(id)?;
    let mut v = json!({
        "id": e.id,
        "kind": "evidence",
        "excerpt": e.excerpt,
        "source": e.source,
        "recorded": recorded,
        "check": cache.get(id).map(|c| json!({ "result": c.result, "at": c.at })),
    });
    super::tombstone(&mut v, "excerpt", &e.purged);
    if with_cited_by {
        v["cited_by"] = json!(s.cited_by.get(id).cloned().unwrap_or_default());
    }
    Some(v)
}

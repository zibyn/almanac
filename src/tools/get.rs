//! `get`: full records by id (spec ch. 3).

use crate::error::ToolError;
use crate::state::State;
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_IDS: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    ids: Vec<String>,
    #[allow(dead_code)]
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
    let records: Vec<Value> = args
        .ids
        .iter()
        .map(|id| {
            memory(&s, id)
                .or_else(|| evidence(&s, id, true))
                .unwrap_or_else(|| json!({ "id": id, "error": "NOT_FOUND" }))
        })
        .collect();
    Ok(json!({ "records": records }))
}

fn memory(s: &State, id: &str) -> Option<Value> {
    let (m, recorded) = s.memories.get(id)?;
    let mut v = json!({
        "id": m.id,
        "kind": "memory",
        "text": m.text,
        "scope": m.scope,
        "epistemic": m.epistemic,
        "status": s.status(id),
        "recorded": recorded,
        "evidence": s.current_evidence(id).iter().filter_map(|e| evidence(s, e, false)).collect::<Vec<_>>(),
        "supersedes": m.supersedes,
        "superseded_by": s.superseded_by.get(id).cloned().unwrap_or_default(),
        "corroborations": s.corroborations.get(id).into_iter().flatten()
            .map(|(ev, recorded)| json!({ "evidence": ev, "recorded": recorded }))
            .collect::<Vec<_>>(),
        "retraction": s.retractions.get(id).map(|(reason, recorded)| json!({ "reason": reason, "recorded": recorded })),
        "stale": null,
    });
    if let Some(p) = &m.project {
        v["project"] = json!(p);
    }
    if let Some(r) = &m.root_commit {
        v["root_commit"] = json!(r);
    }
    Some(v)
}

/// `with_cited_by` is false when nested inside a memory.
fn evidence(s: &State, id: &str, with_cited_by: bool) -> Option<Value> {
    let (e, recorded) = s.evidence.get(id)?;
    let mut v = json!({
        "id": e.id,
        "kind": "evidence",
        "excerpt": e.excerpt,
        "source": e.source,
        "recorded": recorded,
        "check": null,
    });
    if with_cited_by {
        v["cited_by"] = json!(s.cited_by.get(id).cloned().unwrap_or_default());
    }
    Some(v)
}

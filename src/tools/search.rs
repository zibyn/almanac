//! `search`: lexical retrieval plus structured filters over memories (spec ch. 3, ch. 5).

use crate::error::ToolError;
use crate::state::State;
use serde::Deserialize;
use serde_json::{Value, json};
use std::cmp::Reverse;

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    Global,
    Project,
    Any,
}

// shortcut: kind/status/epistemic/writer/cites/stale_only/source_type arrive with #39 and #42.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    query: Option<String>,
    scope: Option<Scope>,
    #[serde(default)]
    all: bool,
    limit: Option<usize>,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ToolError::invalid_args(format!(
            "limit 为 {limit}，须在 1–{MAX_LIMIT} 之间"
        )));
    }
    let scope = args.scope.unwrap_or(Scope::Any);
    let s = super::load()?;
    // The current project matters only for project-scope memories limited to its alias group.
    let group = if scope != Scope::Global && !args.all {
        let p = crate::project::resolve(args.project.as_deref())?;
        s.alias_group(&p.id)
    } else {
        Default::default()
    };

    // Structured filters come before scoring (spec ch. 5 「结构化过滤」).
    let mut candidates: Vec<&str> = s
        .memories
        .values()
        .filter(|(m, _)| {
            s.status(&m.id) == "active"
                && match m.scope.as_str() {
                    "global" => scope != Scope::Project,
                    _ => {
                        scope != Scope::Global
                            && (args.all || m.project.as_ref().is_some_and(|p| group.contains(p)))
                    }
                }
        })
        .map(|(m, _)| m.id.as_str())
        .collect();
    // Newest first, then id descending: the order without a query and the tie-break with one.
    candidates.sort_by_key(|id| Reverse((last_at(&s, id), *id)));

    let hits: Vec<(&str, f64, Vec<String>)> = match &args.query {
        None => candidates.iter().map(|id| (*id, 0.0, Vec::new())).collect(),
        Some(q) => {
            let index = crate::index::Index::new(
                candidates.iter().map(|id| s.memories[*id].0.text.as_str()),
            );
            let mut hits: Vec<_> = index
                .search(q)
                .into_iter()
                .map(|h| (candidates[h.doc], h.score, h.matched))
                .collect();
            // Stable sort keeps the recency order among equal scores.
            hits.sort_by(|a, b| b.1.total_cmp(&a.1));
            hits
        }
    };
    let hits: Vec<Value> = hits
        .into_iter()
        .take(limit)
        .map(|(id, score, matched)| memory_hit(&s, id, score, matched))
        .collect();
    Ok(json!({ "hits": hits }))
}

/// Latest record or corroboration time of a memory.
pub(crate) fn last_at<'a>(s: &'a State, id: &str) -> &'a str {
    s.corroborations
        .get(id)
        .and_then(|c| c.last())
        .map_or(&s.memories[id].1.at, |(_, r)| &r.at)
}

fn memory_hit(s: &State, id: &str, score: f64, matched: Vec<String>) -> Value {
    let (m, recorded) = &s.memories[id];
    let evidence = s.current_evidence(id);
    let mut source_types: Vec<&Value> = Vec::new();
    for e in &evidence {
        if let Some((ev, _)) = s.evidence.get(e)
            && !source_types.contains(&&ev.source["type"])
        {
            source_types.push(&ev.source["type"]);
        }
    }
    let mut v = json!({
        "id": m.id,
        "kind": "memory",
        "text": m.text,
        "scope": m.scope,
        "status": s.status(id),
        "epistemic": m.epistemic,
        "writer": recorded.writer.name,
        "last_at": last_at(s, id),
        "evidence_count": evidence.len(),
        "source_types": source_types,
        // shortcut: lazy source check and `stale` arrive with #42.
        "stale": null,
        "score": score,
        "matched_terms": matched,
    });
    super::tombstone(&mut v, "text", &m.purged);
    if let Some(p) = &m.project {
        v["project"] = json!(p);
    }
    v
}

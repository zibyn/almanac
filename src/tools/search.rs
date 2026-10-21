//! `search`: lexical retrieval plus structured filters over memories (spec ch. 3, ch. 5).

use crate::error::ToolError;
use crate::model::Memory;
use crate::source_check as sc;
use crate::state::State;
use serde::Deserialize;
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::HashSet;

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Scope {
    Global,
    Project,
    Any,
}

// shortcut: kind/status/epistemic/writer/cites/source_type arrive with #39.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    query: Option<String>,
    scope: Option<Scope>,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    stale_only: bool,
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
    // The current project is required only to limit project-scope memories to its alias group;
    // otherwise it just tells the lazy check where file evidence lives.
    let cur = crate::project::resolve(args.project.as_deref());
    let cur = if scope != Scope::Global && !args.all {
        Some(cur?)
    } else {
        cur.ok()
    };
    let group = cur
        .as_ref()
        .map(|p| s.alias_group(&p.id))
        .unwrap_or_default();

    // Structured filters come before scoring (spec ch. 5 「结构化过滤」).
    let mut candidates: Vec<&str> = s
        .memories
        .values()
        .filter(|(m, _)| s.status(&m.id) == "active" && in_scope(m, scope, args.all, &group))
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
    // Lazy check (spec ch. 3): the returned hits' file evidence; with stale_only, every candidate's.
    let mut hits = hits;
    if !args.stale_only {
        hits.truncate(limit);
    }
    let cited: Vec<String> = hits
        .iter()
        .flat_map(|(id, ..)| s.current_evidence(id))
        .collect();
    let (cache, _, _) = sc::refresh(
        &crate::log::home(),
        &s,
        sc::here(&s, cur.as_ref()).as_ref(),
        cited.iter().map(String::as_str),
        &["file"],
    );
    let hits: Vec<Value> = hits
        .into_iter()
        .map(|(id, score, matched)| memory_hit(&s, &cache, id, score, matched))
        .filter(|h| !args.stale_only || !h["stale"].is_null())
        .take(limit)
        .collect();
    Ok(json!({ "hits": hits }))
}

/// The scope filter shared by `search` and `verify` (spec ch. 3 「作用域过滤」); `group` is the
/// current project's alias group, unused when `all`.
pub(super) fn in_scope(m: &Memory, scope: Scope, all: bool, group: &HashSet<String>) -> bool {
    match m.scope.as_str() {
        "global" => scope != Scope::Project,
        _ => {
            scope != Scope::Global && (all || m.project.as_ref().is_some_and(|p| group.contains(p)))
        }
    }
}

/// Latest record or corroboration time of a memory.
fn last_at<'a>(s: &'a State, id: &str) -> &'a str {
    s.corroborations
        .get(id)
        .and_then(|c| c.last())
        .map_or(&s.memories[id].1.at, |(_, r)| &r.at)
}

fn memory_hit(s: &State, cache: &sc::Cache, id: &str, score: f64, matched: Vec<String>) -> Value {
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
        "stale": sc::stale(s, cache, id),
        "score": score,
        "matched_terms": matched,
    });
    if let Some(p) = &m.project {
        v["project"] = json!(p);
    }
    v
}

//! `search`: lexical retrieval plus structured filters over memories or evidence (spec ch. 3, ch. 5).

use crate::error::ToolError;
use crate::state::State;
use serde::Deserialize;
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::HashSet;

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    Global,
    Project,
    Any,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
enum Kind {
    #[default]
    Memory,
    Evidence,
}

const STATUSES: &[&str] = &["active", "superseded", "retracted", "unsupported", "purged"];
const EPISTEMICS: &[&str] = &["observed", "user_stated", "inferred"];
const SOURCE_TYPES: &[&str] = &["conversation", "file", "web", "command"];
const EXCERPT_CHARS: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    query: Option<String>,
    kind: Option<Kind>,
    scope: Option<Scope>,
    all: Option<bool>,
    status: Option<Vec<String>>,
    epistemic: Option<Vec<String>>,
    writer: Option<String>,
    cites: Option<String>,
    // shortcut: accepted but not applied yet; lazy source check and the filter arrive with #42.
    stale_only: Option<bool>,
    source_type: Option<Vec<String>>,
    limit: Option<usize>,
    project: Option<String>,
}

/// `INVALID_ARGS` unless every value of array parameter `field` is one of `allowed`.
fn check_values(
    field: &str,
    vals: &Option<Vec<String>>,
    allowed: &[&str],
) -> Result<(), ToolError> {
    match vals
        .iter()
        .flatten()
        .find(|v| !allowed.contains(&v.as_str()))
    {
        Some(v) => Err(ToolError::invalid_args(format!(
            "{field} 不认识 {v}，可选：{}",
            allowed.join(" / ")
        ))),
        None => Ok(()),
    }
}

/// `INVALID_ARGS` naming the first parameter given that `kind` does not take.
fn check_kind(args: &Args, kind: Kind) -> Result<(), ToolError> {
    let memory_only = [
        ("scope", args.scope.is_some()),
        ("all", args.all.is_some()),
        ("status", args.status.is_some()),
        ("epistemic", args.epistemic.is_some()),
        ("writer", args.writer.is_some()),
        ("cites", args.cites.is_some()),
        ("stale_only", args.stale_only.is_some()),
    ];
    let bad = match kind {
        Kind::Memory => args.source_type.is_some().then_some("source_type"),
        Kind::Evidence => memory_only
            .iter()
            .find(|(_, given)| *given)
            .map(|(f, _)| *f),
    };
    match bad {
        Some(f) => Err(ToolError::invalid_args(format!(
            "{f} 只对 kind={} 有效",
            if kind == Kind::Memory {
                "evidence"
            } else {
                "memory"
            }
        ))),
        None => Ok(()),
    }
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ToolError::invalid_args(format!(
            "limit 为 {limit}，须在 1–{MAX_LIMIT} 之间"
        )));
    }
    check_values("status", &args.status, STATUSES)?;
    check_values("epistemic", &args.epistemic, EPISTEMICS)?;
    check_values("source_type", &args.source_type, SOURCE_TYPES)?;
    let kind = args.kind.unwrap_or_default();
    check_kind(&args, kind)?;
    let scope = args.scope.unwrap_or(Scope::Any);
    let all = args.all.unwrap_or(false);
    // The current project filters only project-scope memories limited to its alias group;
    // otherwise it is needed just for `alias_hint`, so failing to resolve it is not an error.
    let filters_by_project = kind == Kind::Memory && scope != Scope::Global && !all;
    let current = match crate::project::resolve(args.project.as_deref()) {
        Ok(p) => Some(p),
        Err(e) if filters_by_project => return Err(e),
        Err(_) => None,
    };
    let s = super::load()?;
    let hits = match kind {
        Kind::Memory => {
            let group = match &current {
                Some(p) if filters_by_project => s.alias_group(&p.id),
                _ => Default::default(),
            };
            memory_hits(&s, &args, &group, limit)
        }
        Kind::Evidence => evidence_hits(&s, &args, limit),
    };
    let mut v = json!({ "hits": hits });
    if let Some(hint) = current.and_then(|p| alias_hint(&s, &p)) {
        v["alias_hint"] = hint;
    }
    Ok(v)
}

/// Other project identities in the store with the current project's root commit and not yet
/// in its alias group (spec ch. 1 「项目别名」).
fn alias_hint(s: &State, p: &crate::project::Project) -> Option<Value> {
    let root = p.root_commit.as_ref()?;
    let group = s.alias_group(&p.id);
    let mut same: Vec<&str> = s
        .memories
        .values()
        .filter(|(m, _)| m.root_commit.as_ref() == Some(root))
        .filter_map(|(m, _)| m.project.as_deref())
        .filter(|q| !group.contains(*q))
        .collect();
    same.sort_unstable();
    same.dedup();
    (!same.is_empty()).then(|| json!({ "project": p.id, "same_root_as": same }))
}

fn memory_hits(s: &State, args: &Args, group: &HashSet<String>, limit: usize) -> Vec<Value> {
    let scope = args.scope.unwrap_or(Scope::Any);
    let all = args.all.unwrap_or(false);
    let active = ["active".to_string()];
    let status = args.status.as_deref().unwrap_or(&active);

    // Structured filters come before scoring (spec ch. 5 「结构化过滤」).
    let mut candidates: Vec<&str> = s
        .memories
        .values()
        .filter(|(m, recorded)| {
            status.iter().any(|st| st == s.status(&m.id))
                && args
                    .epistemic
                    .as_ref()
                    .is_none_or(|e| e.contains(&m.epistemic))
                && args
                    .writer
                    .as_ref()
                    .is_none_or(|w| *w == recorded.writer.name)
                && args.cites.as_ref().is_none_or(|e| s.cites(&m.id, e))
                && match m.scope.as_str() {
                    "global" => scope != Scope::Project,
                    _ => {
                        scope != Scope::Global
                            && (all || m.project.as_ref().is_some_and(|p| group.contains(p)))
                    }
                }
        })
        .map(|(m, _)| m.id.as_str())
        .collect();
    // Newest first, then id descending: the order without a query and the tie-break with one.
    candidates.sort_by_key(|id| Reverse((last_at(s, id), *id)));
    let ranked = rank(
        &candidates,
        |id| &s.memories[id].0.text,
        args.query.as_deref(),
    );
    ranked
        .into_iter()
        .take(limit)
        .map(|(id, score, matched)| memory_hit(s, id, score, matched))
        .collect()
}

/// Evidence search spans the whole store; only `source_type` filters it.
fn evidence_hits(s: &State, args: &Args, limit: usize) -> Vec<Value> {
    let mut candidates: Vec<&str> = s
        .evidence
        .values()
        .filter(|(e, _)| {
            args.source_type.as_ref().is_none_or(|t| {
                e.source["type"]
                    .as_str()
                    .is_some_and(|ty| t.iter().any(|x| x == ty))
            })
        })
        .map(|(e, _)| e.id.as_str())
        .collect();
    candidates.sort_by_key(|id| Reverse((s.evidence[*id].1.at.as_str(), *id)));
    let ranked = rank(
        &candidates,
        |id| &s.evidence[id].0.excerpt,
        args.query.as_deref(),
    );
    ranked
        .into_iter()
        .take(limit)
        .map(|(id, score, matched)| {
            let e = &s.evidence[id].0;
            json!({
                "id": id,
                "kind": "evidence",
                "source_type": e.source["type"],
                "excerpt": e.excerpt.chars().take(EXCERPT_CHARS).collect::<String>(),
                "truncated": e.excerpt.chars().nth(EXCERPT_CHARS).is_some(),
                "cited_by": s.cited_by.get(id).cloned().unwrap_or_default(),
                "score": score,
                "matched_terms": matched,
            })
        })
        .collect()
}

/// Scores `candidates` (already in recency order) against `query`; without one, keeps them all
/// with score 0. Stable sort keeps the recency order among equal scores.
fn rank<'a>(
    candidates: &[&'a str],
    text: impl Fn(&str) -> &'a str,
    query: Option<&str>,
) -> Vec<(&'a str, f64, Vec<String>)> {
    let Some(q) = query else {
        return candidates.iter().map(|id| (*id, 0.0, Vec::new())).collect();
    };
    let index = crate::index::Index::new(candidates.iter().map(|id| text(id)));
    let mut hits: Vec<_> = index
        .search(q)
        .into_iter()
        .map(|h| (candidates[h.doc], h.score, h.matched))
        .collect();
    hits.sort_by(|a, b| b.1.total_cmp(&a.1));
    hits
}

/// Latest record or corroboration time of a memory.
fn last_at<'a>(s: &'a State, id: &str) -> &'a str {
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

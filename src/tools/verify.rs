//! `verify`: explicit source check of the selected memories' current evidence (spec ch. 3, ch. 6).

use super::search::{Scope, in_scope};
use crate::error::ToolError;
use crate::source_check as sc;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    ids: Option<Vec<String>>,
    scope: Option<Scope>,
    #[serde(default)]
    all: bool,
    include_web: Option<bool>,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let s = super::load()?;
    let scope = args.scope.unwrap_or(Scope::Any);
    let cur = crate::project::resolve(args.project.as_deref());
    let needs_project = args.ids.is_none() && scope != Scope::Global && !args.all;
    let cur = if needs_project { Some(cur?) } else { cur.ok() };
    let memories: Vec<String> = match args.ids {
        Some(ids) => {
            for id in &ids {
                super::check_exists(&s, id)?;
            }
            ids
        }
        None => {
            let group = cur
                .as_ref()
                .map(|p| s.alias_group(&p.id))
                .unwrap_or_default();
            let mut ids: Vec<String> = s
                .memories
                .values()
                .filter(|(m, _)| {
                    s.status(&m.id) == "active" && in_scope(m, scope, args.all, &group)
                })
                .map(|(m, _)| m.id.clone())
                .collect();
            // Ids are ULIDs: oldest first.
            ids.sort();
            ids
        }
    };
    let mut types = vec!["file", "conversation", "command"];
    if args.include_web.unwrap_or(true) {
        types.push("web");
    }
    let cited: Vec<String> = memories
        .iter()
        .flat_map(|m| s.current_evidence(m))
        .collect();
    let (cache, checked, net) = sc::refresh(
        &crate::log::home(),
        &s,
        sc::here(&s, cur.as_ref()).as_ref(),
        cited.iter().map(String::as_str),
        &types,
    );
    let checked: Vec<Value> = checked
        .into_iter()
        .map(|(id, c)| {
            json!({ "evidence": id, "source_type": s.evidence[&id].0.source["type"], "result": c.result, "at": c.at })
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let stale: Vec<Value> = memories
        .iter()
        .filter(|m| seen.insert(*m))
        .filter_map(|m| {
            let flag = sc::stale(&s, &cache, m);
            (!flag.is_null())
                .then(|| json!({ "memory": m, "text": s.memories[m].0.text, "stale": flag }))
        })
        .collect();
    Ok(json!({ "checked": checked, "network_requests": net, "stale": stale }))
}

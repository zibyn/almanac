//! `review`: lists stale and unsupported memories (spec ch. 3 「review」, ch. 7).
//! Read-only and offline: stale flags come from the check cache alone.

use super::search::{Scope, current_project, in_scope, scope_group};
use crate::error::ToolError;
use crate::source_check as sc;
use crate::state::Status;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    scope: Option<Scope>,
    #[serde(default)]
    all: bool,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let scope = args.scope.unwrap_or(Scope::Any);
    let s = super::load()?;
    let needed = scope.needs_project(args.all);
    let cur = if needed {
        current_project(args.project.as_deref(), needed)?
    } else {
        None
    };
    let group = scope_group(&s, cur.as_ref());
    let cache = sc::load_cache(&crate::log::home()?);
    let mut ids: Vec<&String> = s
        .memories
        .iter()
        .filter(|(_, (m, _))| in_scope(m, scope, args.all, &group))
        .map(|(id, _)| id)
        .collect();
    // Ids are ULIDs; each issue involves exactly one memory, so this is the spec's order.
    ids.sort();
    let mut stale = vec![];
    let mut unsupported = vec![];
    for id in ids {
        let text = &s.memories[id].0.text;
        match s.status(id) {
            Status::Active => {
                let flag = sc::stale(&s, &cache, id);
                if !flag.is_null() {
                    stale.push(json!({
                        "type": "stale", "memory": id, "text": text, "stale": flag,
                        "actions": ["corroborate", "supersede", "retract"]
                    }));
                }
            }
            Status::Unsupported => unsupported.push(json!({
                "type": "unsupported", "memory": id, "text": text,
                "actions": ["supersede", "retract"]
            })),
            _ => {}
        }
    }
    stale.extend(unsupported);
    Ok(json!({ "issues": stale }))
}

//! `corroborate`: append supporting evidence to an active memory (spec ch. 3).

use super::record;
use crate::error::ToolError;
use crate::model::{Event, Tx};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    memory: String,
    /// Each item is an existing evidence id or an inline `EvidenceIn`.
    evidence: Vec<Value>,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let writer = crate::writer::infer(|k| std::env::var(k).ok())?;
    let state = super::load()?;
    let memory = &args.memory;
    super::check_exists(&state, memory)?;
    match state.status(memory) {
        "active" => {}
        status => return Err(super::not_active(&state, memory, status)),
    }
    if args.evidence.is_empty() {
        return Err(ToolError::new("NO_EVIDENCE", "附证的 evidence 为空")
            .hint("至少给一条已有证据 id 或内联证据"));
    }

    // Inline evidence goes through `record`'s checks and dedup; `ref` means nothing here.
    let mut inline = vec![];
    for e in &args.evidence {
        match e {
            Value::String(_) => {}
            Value::Object(o) => {
                let mut o = o.clone();
                o.remove("ref");
                inline.push(Value::Object(o));
            }
            _ => {
                return Err(ToolError::invalid_args(
                    "evidence 的每项须是证据 id 或证据对象",
                ));
            }
        }
    }
    let at = crate::model::now();
    let (mut events, out) = if inline.is_empty() {
        (vec![], json!({ "evidence": [] }))
    } else {
        let rec =
            super::parse(&json!({ "evidence": inline, "project": args.project }).to_string())?;
        record::build(rec, &state, &writer, &at)?
    };
    let mut inline_ids = out["evidence"].as_array().unwrap().iter().map(|e| &e["id"]);

    let (mut added, mut already_cited) = (vec![], vec![]);
    for e in &args.evidence {
        let id = match e {
            Value::String(id) if state.evidence.contains_key(id) => id.clone(),
            Value::String(id) => {
                return Err(ToolError::new(
                    "EVIDENCE_NOT_FOUND",
                    format!("证据 {id} 不存在"),
                ));
            }
            _ => inline_ids.next().unwrap().as_str().unwrap().to_string(),
        };
        let list = if state.cites(memory, &id) {
            &mut already_cited
        } else {
            &mut added
        };
        if !list.contains(&id) {
            list.push(id);
        }
    }

    // New inline evidence is never already cited, so nothing to write implies no new evidence.
    if !added.is_empty() {
        events.push(Event::Corroborated {
            memory: memory.clone(),
            evidence: added.clone(),
        });
        let mut tx = Tx::new(writer, events);
        tx.at = at;
        crate::log::append(&crate::log::home(), &tx).map_err(super::io_error)?;
    }
    Ok(json!({ "memory": memory, "added": added, "already_cited": already_cited }))
}

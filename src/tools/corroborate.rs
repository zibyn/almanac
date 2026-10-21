//! `corroborate`: append supporting evidence to an active memory (spec ch. 3).

use super::record::{self, EvidenceIn};
use crate::error::ToolError;
use crate::model::{Event, Tx};
use crate::state::State;
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
    let writer = crate::writer::current()?;
    let project = crate::project::resolve(args.project.as_deref())?;
    // Inline evidence goes through `record`'s checks and dedup; `ref` means nothing here.
    let mut inline: Vec<(usize, EvidenceIn)> = vec![];
    for (i, e) in args.evidence.iter().enumerate() {
        match e {
            Value::String(_) => {}
            Value::Object(o) => {
                let mut o = o.clone();
                o.remove("ref");
                inline.push((i, super::parse(&Value::Object(o).to_string())?));
            }
            _ => {
                return Err(ToolError::invalid_args(
                    "evidence 的每项须是证据 id 或证据对象",
                ));
            }
        }
    }
    let home = crate::log::home()?;
    let (out, checks) =
        crate::log::write(&home, |state| build(state, &args, inline, writer, &project))?;
    // The cache is a deletable local observation; failing to write it must not fail a committed write.
    if let Err(e) = crate::source_check::store(&home, checks) {
        eprintln!("almanac: failed to write check cache: {e}");
    }
    Ok(out)
}

/// Runs under the write lock against freshly replayed state (spec ch. 2).
fn build(
    state: &State,
    args: &Args,
    inline: Vec<(usize, EvidenceIn)>,
    writer: crate::model::Writer,
    project: &crate::project::Project,
) -> Result<(Option<Tx>, (Value, record::Checks)), ToolError> {
    let memory = &args.memory;
    super::check_status(state, memory, &["active"])?;
    if args.evidence.is_empty() {
        return Err(ToolError::new("NO_EVIDENCE", "附证的 evidence 为空")
            .hint("至少给一条已有证据 id 或内联证据"));
    }
    if let Some(id) = args
        .evidence
        .iter()
        .filter_map(Value::as_str)
        .find(|id| !state.evidence.contains_key(*id) || state.evidence_purged(id))
    {
        return Err(ToolError::new(
            "EVIDENCE_NOT_FOUND",
            format!("证据 {id} 不存在"),
        ));
    }
    let (tx, (out, checks)) = if inline.is_empty() {
        (None, (json!({ "evidence": [] }), vec![]))
    } else {
        record::build(state, writer.clone(), project, inline, vec![])?
    };
    let mut inline_ids = out["evidence"].as_array().unwrap().iter();

    let (mut added, mut already_cited) = (vec![], vec![]);
    for e in &args.evidence {
        let id = match e.as_str() {
            Some(id) => id,
            None => inline_ids.next().unwrap()["id"].as_str().unwrap(),
        };
        let list = if state.cites(memory, id) {
            &mut already_cited
        } else {
            &mut added
        };
        if !list.iter().any(|x| x == id) {
            list.push(id.to_string());
        }
    }

    let out = json!({ "memory": memory, "added": added, "already_cited": already_cited });
    // New inline evidence is never already cited, so nothing added implies nothing new to write.
    if added.is_empty() {
        return Ok((None, (out, checks)));
    }
    let mut tx = tx.unwrap_or_else(|| {
        let mut tx = Tx::new(writer, vec![]);
        tx.git_commit = project.head.clone();
        tx
    });
    tx.events.push(Event::Corroborated {
        memory: memory.clone(),
        evidence: added,
    });
    Ok((Some(tx), (out, checks)))
}

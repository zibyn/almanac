//! `retract`: declare a memory wrong; it stays in history (spec ch. 3).

use crate::error::ToolError;
use crate::model::{Event, Tx};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    memory: String,
    reason: String,
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    if args.reason.trim().is_empty() {
        return Err(super::missing_field("reason"));
    }
    crate::secrets::check([("reason".to_string(), args.reason.as_str())])?;
    let writer = crate::writer::current()?;
    let project = crate::project::resolve(args.project.as_deref())?;
    let memory = args.memory.clone();
    crate::log::write(&crate::log::home(), |state| {
        super::check_replaceable(state, &memory)?;
        let mut tx = Tx::new(
            writer,
            vec![Event::Retracted {
                memory: args.memory,
                reason: args.reason,
                purged: None,
            }],
        );
        tx.git_commit = project.head;
        Ok((Some(tx), ()))
    })?;
    Ok(json!({ "memory": memory, "status": "retracted" }))
}

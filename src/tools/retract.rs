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
    #[allow(dead_code)]
    project: Option<String>,
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    if args.reason.trim().is_empty() {
        return Err(super::missing_field("reason"));
    }
    // shortcut: `reason` is not scanned for secrets yet; add it when the secret-check ticket lands.
    let writer = crate::writer::infer(|k| std::env::var(k).ok())?;
    let state = super::load()?;
    super::check_replaceable(&state, &args.memory)?;
    let tx = Tx::new(
        writer,
        vec![Event::Retracted {
            memory: args.memory.clone(),
            reason: args.reason,
        }],
    );
    crate::log::append(&crate::log::home(), &tx).map_err(super::io_error)?;
    Ok(json!({ "memory": args.memory, "status": "retracted" }))
}

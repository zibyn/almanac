//! Tool layer (spec ch. 3), shared by CLI and MCP: parse args, call the core, build response JSON.
//! Each tool lives in its own file and is registered in `call`.

mod get;
mod record;

use crate::error::ToolError;
use serde::de::DeserializeOwned;
use serde_json::Value;

const TOOLS: &[&str] = &["record", "get"];

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

/// Runs tool `name` with raw JSON arguments.
pub fn call(name: &str, args: &str) -> Result<Value, ToolError> {
    match name {
        "record" => record::run(parse(args)?),
        "get" => get::run(parse(args)?),
        _ => Err(ToolError::invalid_args(format!("未知工具 {name}"))),
    }
}

/// Parses tool arguments; arg structs use `deny_unknown_fields`, so unknown fields fail here.
// shortcut: every serde failure (incl. missing fields) is INVALID_ARGS; #35 maps missing fields to MISSING_FIELD.
fn parse<T: DeserializeOwned>(args: &str) -> Result<T, ToolError> {
    serde_json::from_str(args).map_err(|e| ToolError::invalid_args(format!("参数不合法：{e}")))
}

/// Replays the store's log into derived state.
fn load() -> Result<crate::state::State, ToolError> {
    let txs = crate::log::replay(&crate::log::home()).map_err(crate::log::io_error)?;
    Ok(crate::state::State::replay(&txs))
}

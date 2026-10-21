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
/// A missing required field is `MISSING_FIELD`; every other serde failure is `INVALID_ARGS`.
fn parse<T: DeserializeOwned>(args: &str) -> Result<T, ToolError> {
    serde_json::from_str(args).map_err(|e| {
        let msg = e.to_string();
        // serde's wording is "missing field `name` at line …"; nothing more structured is exposed.
        match msg
            .strip_prefix("missing field `")
            .and_then(|r| r.split('`').next())
        {
            Some(field) => missing_field(field),
            None => ToolError::invalid_args(format!("参数不合法：{msg}")),
        }
    })
}

fn missing_field(field: &str) -> ToolError {
    ToolError::new("MISSING_FIELD", format!("缺少必填字段 {field}"))
        .hint(format!("补上 {field} 后重试"))
}

/// Replays the store's log into derived state.
fn load() -> Result<crate::state::State, ToolError> {
    let txs = crate::log::replay(&crate::log::home()).map_err(crate::log::io_error)?;
    Ok(crate::state::State::replay(&txs))
}

//! Tool layer (spec ch. 3), shared by CLI and MCP: parse args, call the core, build response JSON.
//! Each tool lives in its own file and is registered in `call`.

mod corroborate;
mod get;
mod record;
mod retract;
mod search;
mod verify;

use crate::error::ToolError;
use serde::de::DeserializeOwned;
use serde_json::Value;

const TOOLS: &[&str] = &[
    "record",
    "get",
    "retract",
    "corroborate",
    "search",
    "verify",
];

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

/// Runs tool `name` with raw JSON arguments.
pub fn call(name: &str, args: &str) -> Result<Value, ToolError> {
    match name {
        "record" => record::run(parse(args)?),
        "get" => get::run(parse(args)?),
        "retract" => retract::run(parse(args)?),
        "corroborate" => corroborate::run(parse(args)?),
        "search" => search::run(parse(args)?),
        "verify" => verify::run(parse(args)?),
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

/// Spec ch. 1: only `active` or `unsupported` memories can be superseded or retracted.
fn check_replaceable(s: &crate::state::State, id: &str) -> Result<(), ToolError> {
    check_exists(s, id)?;
    match s.status(id) {
        "active" | "unsupported" => Ok(()),
        status => Err(not_active(s, id, status)),
    }
}

fn check_exists(s: &crate::state::State, id: &str) -> Result<(), ToolError> {
    if s.memories.contains_key(id) {
        Ok(())
    } else {
        Err(ToolError::new(
            "MEMORY_NOT_FOUND",
            format!("记忆 {id} 不存在"),
        ))
    }
}

/// `NOT_ACTIVE` for `id` in `status`; a superseded memory's hint names its active successors.
fn not_active(s: &crate::state::State, id: &str, status: &str) -> ToolError {
    let e = ToolError::new(
        "NOT_ACTIVE",
        format!("记忆 {id} 的状态是 {status}，不是生效"),
    );
    if status == "superseded" {
        let next = s.active_successors(id).join("、");
        e.hint(format!("沿取代链对最新的生效后继操作：{next}"))
    } else {
        e
    }
}

/// Replays the store's log into derived state.
fn load() -> Result<crate::state::State, ToolError> {
    let txs = crate::log::replay(&crate::log::home()).map_err(crate::log::io_error)?;
    Ok(crate::state::State::replay(&txs))
}

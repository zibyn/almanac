//! Tool layer (spec ch. 3), shared by CLI and MCP: parse args, call the core, build response JSON.
//! Each tool lives in its own file and is registered in `call`.

mod corroborate;
mod get;
mod record;
mod retract;
mod review;
pub(crate) mod schema;
pub(crate) mod search;
mod verify;

use crate::error::ToolError;
use crate::state::Status;
use serde::de::DeserializeOwned;
use serde_json::Value;

const TOOLS: &[&str] = &[
    "record",
    "get",
    "retract",
    "corroborate",
    "search",
    "verify",
    "review",
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
        "review" => review::run(parse(args)?),
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
    check_status(s, id, &[Status::Active, Status::Unsupported])
}

/// `MEMORY_NOT_FOUND` unless `id` exists, then `NOT_ACTIVE` unless its status is in `allowed`.
fn check_status(s: &crate::state::State, id: &str, allowed: &[Status]) -> Result<(), ToolError> {
    check_exists(s, id)?;
    match s.status(id) {
        st if allowed.contains(&st) => Ok(()),
        status => Err(not_active(s, id, status)),
    }
}

pub(crate) fn check_exists(s: &crate::state::State, id: &str) -> Result<(), ToolError> {
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
fn not_active(s: &crate::state::State, id: &str, status: Status) -> ToolError {
    let e = ToolError::new(
        "NOT_ACTIVE",
        format!("记忆 {id} 的状态是 {status}，不是生效"),
    );
    if status == Status::Superseded {
        let next = s.active_successors(id).join("、");
        e.hint(format!("沿取代链对最新的生效后继操作：{next}"))
    } else {
        e
    }
}

/// Spec ch. 3: a purged object carries `purged` in place of its text field (`get` and `search`).
fn show_purged(v: &mut Value, field: &str, purged: &Option<Value>) {
    if let Some(t) = purged {
        let o = v.as_object_mut().unwrap();
        o.remove(field);
        o.insert("purged".into(), t.clone());
    }
}

/// Replays the store's log into derived state.
fn load() -> Result<crate::state::State, ToolError> {
    crate::log::load(&crate::log::home()?)
}

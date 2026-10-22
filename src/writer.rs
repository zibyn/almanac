//! Writer inference for the CLI and MCP (spec ch. 9).

use crate::error::ToolError;
use crate::model::Writer;
use std::sync::OnceLock;

/// Host name → its session variables, in priority order.
const HOSTS: [(&str, &[&str]); 4] = [
    ("claude-code", &["CLAUDE_CODE_SESSION_ID"]),
    ("codex", &["CODEX_THREAD_ID", "CODEX_SESSION_ID"]),
    ("deepseek-harness", &["DSH_SESSION_ID"]),
    ("pi", &["PI_SESSION_ID"]),
];

/// MCP `initialize`'s `clientInfo.name`, set once by `almanac mcp`; unset under the CLI.
pub static MCP_CLIENT: OnceLock<String> = OnceLock::new();

/// The Writer of the current call: process environment plus, under MCP, the client name.
pub fn current() -> Result<Writer, ToolError> {
    infer(
        |k| std::env::var(k).ok(),
        MCP_CLIENT.get().map(String::as_str),
    )
}

/// `env` looks up an environment variable; empty values count as unset.
/// `client` is the MCP client name, tried between `ALMANAC_WRITER` and host variables (spec ch. 9 "MCP").
fn infer(env: impl Fn(&str) -> Option<String>, client: Option<&str>) -> Result<Writer, ToolError> {
    let env = |k: &str| env(k).filter(|v| !v.trim().is_empty());
    let first = |vars: &[&str]| vars.iter().find_map(|v| env(v));
    let from_client = match client {
        Some("codex-mcp-client") => Some("codex".to_string()),
        Some("pi") => Some("pi".to_string()),
        _ => None,
    };

    let declared = env("ALMANAC_WRITER")
        .map(|w| w.trim().to_string())
        .filter(|w| w != "unknown");
    let name = match declared.or(from_client) {
        Some(w) => w,
        None => {
            let found: Vec<&str> = HOSTS
                .iter()
                .filter(|(_, vars)| first(vars).is_some())
                .map(|(h, _)| *h)
                .collect();
            match found[..] {
                [one] => one.to_string(),
                [] if env("PI_CODING_AGENT").as_deref() == Some("true") => "pi".to_string(),
                [] => {
                    return Err(ToolError::new("NO_WRITER", "没有检测到宿主，无法判断 Writer").hint(
                        "智能体在命令前加 ALMANAC_WRITER=<自己的名字> 重试，人在终端里用 ALMANAC_WRITER=human",
                    ));
                }
                _ => {
                    let hosts = found.join(" 与 ");
                    return Err(ToolError::new(
                        "NO_WRITER",
                        format!("检测到 {hosts} 的会话变量（宿主嵌套），无法判断 Writer"),
                    )
                    .hint("在命令前加 ALMANAC_WRITER=<宿主名> 重试"));
                }
            }
        }
    };

    let session = HOSTS
        .iter()
        .find(|(h, _)| *h == name)
        .and_then(|(_, vars)| first(vars));
    let model = match (name.as_str(), env("PI_PROVIDER"), env("PI_MODEL")) {
        ("pi", Some(p), Some(m)) => Some(format!("{p}/{m}")),
        _ => None,
    };
    Ok(Writer {
        name,
        model,
        session,
    })
}

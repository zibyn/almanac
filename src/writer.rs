//! Writer inference for the CLI (spec ch. 9).

use crate::error::ToolError;
use crate::model::Writer;

/// Host name → its session variables, in priority order.
const HOSTS: [(&str, &[&str]); 3] = [
    ("claude-code", &["CLAUDE_CODE_SESSION_ID"]),
    ("codex", &["CODEX_THREAD_ID", "CODEX_SESSION_ID"]),
    ("pi", &["PI_SESSION_ID"]),
];

/// `env` looks up an environment variable; empty values count as unset.
pub fn infer(env: impl Fn(&str) -> Option<String>) -> Result<Writer, ToolError> {
    let env = |k: &str| env(k).filter(|v| !v.trim().is_empty());
    let first = |vars: &[&str]| vars.iter().find_map(|v| env(v));

    let name = match env("ALMANAC_WRITER").map(|w| w.trim().to_string()) {
        Some(w) if w != "unknown" => w,
        _ => {
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

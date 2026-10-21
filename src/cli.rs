//! CLI front end: `almanac <tool> [--json '<args>' | --json -]` (spec ch. 3, 4).

use crate::error::ToolError;
use std::io::Read;

/// Runs one CLI invocation and returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("INVALID_ARGS：用法 almanac <命令> [--json '<参数>' | --json -]");
        return 1;
    };
    if cmd == "project" {
        return crate::alias::run(rest);
    }
    if cmd == "purge" {
        return crate::purge::run(rest);
    }
    if cmd == "export" {
        return crate::export::run(rest);
    }
    if cmd == "remind" {
        return crate::remind::run();
    }
    if !crate::tools::is_tool(cmd) {
        eprintln!("INVALID_ARGS：未知命令 {cmd}");
        return 1;
    }
    crate::log::warn_if_synced(&crate::log::home());
    let result = tool_args(rest).and_then(|json| crate::tools::call(cmd, &json));
    match result {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            println!("{}", e.to_json());
            1
        }
    }
}

/// No `--json` means `{}`; `--json -` reads stdin.
fn tool_args(rest: &[String]) -> Result<String, ToolError> {
    match rest {
        [] => Ok("{}".into()),
        [flag, v] if flag == "--json" && v == "-" => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| ToolError::invalid_args(format!("读取 stdin 失败：{e}")))?;
            Ok(s)
        }
        [flag, v] if flag == "--json" => Ok(v.clone()),
        _ => Err(
            ToolError::invalid_args(format!("无法识别的命令行参数：{}", rest.join(" ")))
                .hint("用 --json '<参数 JSON>' 或 --json - 从 stdin 传参"),
        ),
    }
}

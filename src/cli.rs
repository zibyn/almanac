//! CLI front end: `almanac <tool> [--json '<args>' | --json -]` (spec ch. 3, 4).

use crate::error::ToolError;
use std::io::Read;

const HELP: &str = concat!(
    "almanac ",
    env!("CARGO_PKG_VERSION"),
    " — 每条记忆都带证据的长期记忆库\n",
    "\n",
    "用法：\n",
    "  almanac <命令> --json -          参数 JSON 从 stdin 读（用 heredoc 传，推荐）\n",
    "  almanac --help                   显示本帮助\n",
    "  almanac --version                显示版本\n",
    "\n",
    "工具：\n",
    "  record       写入证据和记忆\n",
    "  get          按 id 取全文\n",
    "  search       检索记忆或证据\n",
    "  review       列出待处置的问题\n",
    "  corroborate  给生效记忆附证\n",
    "  retract      撤回记忆\n",
    "  verify       回到来源核验摘录\n",
    "\n",
    "仅 CLI：\n",
    "  context      输出会话开始时注入的记忆\n",
    "  remind       输出写入提醒\n",
    "  project alias <A> <B>              声明两个项目标识指同一个项目\n",
    "  export [--all]                     把记忆导出为 Markdown\n",
    "  purge <id> --reason <原因>         从磁盘抹去一条证据或记忆（交互式）\n",
    "  mcp          启动 MCP stdio server\n",
);

/// Runs one CLI invocation and returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{HELP}");
        return 0;
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            return 0;
        }
        "--version" | "-V" => {
            println!("almanac {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        _ => {}
    }
    // `<命令> --help` also shows the top-level help rather than failing as an argument.
    if matches!(rest.first().map(String::as_str), Some("--help" | "-h")) {
        println!("{HELP}");
        return 0;
    }
    if cmd == "project" {
        return crate::alias::run(rest);
    }
    if cmd == "purge" {
        return crate::purge::run(rest);
    }
    if cmd == "context" {
        return crate::context::run(rest);
    }
    if cmd == "export" {
        return crate::export::run(rest);
    }
    if cmd == "remind" {
        return crate::remind::run();
    }
    if cmd == "mcp" {
        return crate::mcp::run();
    }
    if !crate::tools::is_tool(cmd) {
        eprintln!("INVALID_ARGS：未知命令 {cmd}");
        return 1;
    }
    if let Ok(h) = crate::log::home() {
        crate::log::warn_if_synced(&h);
    }
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

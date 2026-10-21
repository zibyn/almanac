//! `almanac project alias <A> <B>` (spec ch. 4): a CLI-only command run by the human.

use crate::error::ToolError;
use crate::model::{Event, Tx, Writer};
use crate::tools::io_error;

/// Runs `project …` with the arguments after `project`; returns the exit code.
pub fn run(args: &[String]) -> i32 {
    match alias(args) {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("{}：{}", e.code, e.message);
            1
        }
    }
}

fn alias(args: &[String]) -> Result<String, ToolError> {
    let [sub, a, b] = args else {
        return Err(ToolError::invalid_args(
            "用法 almanac project alias <项目标识 A> <项目标识 B>",
        ));
    };
    if sub != "alias" {
        return Err(ToolError::invalid_args(format!("未知命令 project {sub}")));
    }
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() || b.is_empty() || a == b {
        return Err(ToolError::invalid_args("两个项目标识必须不同且非空"));
    }
    // shortcut: no write lock / STORE_NEWER check yet; #45 adds them around every write.
    let home = crate::log::home();
    let txs = crate::log::replay(&home).map_err(io_error)?;
    if crate::state::State::replay(&txs).alias_group(a).contains(b) {
        return Ok(format!("{a} 与 {b} 已在同一别名组内，无需写入"));
    }
    let human = Writer {
        name: "human".into(),
        model: None,
        session: None,
    };
    let event = Event::ProjectAliased {
        projects: vec![a.into(), b.into()],
    };
    crate::log::append(&home, &Tx::new(human, vec![event])).map_err(io_error)?;
    Ok(format!("已声明 {a} 与 {b} 是同一个项目"))
}

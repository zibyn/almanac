//! MCP `tools/list` (spec ch. 3): each tool's 「给模型的描述」 and its parameter table as JSON Schema.

use serde_json::{Value, json};

/// The tools in spec ch. 3's table order.
pub fn list() -> Value {
    let project = json!({"type": "string", "description": "项目路径，只用来定位项目；省略时取 MCP roots → CLAUDE_PROJECT_DIR → 当前目录"});
    let scope = json!({"type": "string", "enum": ["global", "project", "any"], "default": "any", "description": "any＝全局 + 当前项目"});
    let all = json!({"type": "boolean", "default": false, "description": "为真时「项目」不限于当前项目，覆盖库里所有项目"});
    let ids = |min: u64, max: Option<u64>| {
        let mut v = json!({"type": "array", "items": {"type": "string"}, "minItems": min});
        if let Some(m) = max {
            v["maxItems"] = m.into();
        }
        v
    };
    let evidence_in = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["excerpt", "source"],
        "properties": {
            "ref": {"type": "string", "description": "本次调用内的局部名字，供 memories[].evidence 引用"},
            "excerpt": {"type": "string", "description": "摘录，UTF-8 不超过 4096 字节"},
            "source": {"oneOf": [
                {"type": "object", "additionalProperties": false, "required": ["type"],
                 "description": "只收用户说的话",
                 "properties": {"type": {"const": "conversation"}, "session": {"type": "string"},
                                "turn": {"type": "integer", "minimum": 0}, "transcript_path": {"type": "string"}}},
                {"type": "object", "additionalProperties": false, "required": ["type", "path"],
                 "properties": {"type": {"const": "file"},
                                "path": {"type": "string", "description": "相对项目根，不以 / 开头，不含 .."},
                                "line": {"type": "integer", "minimum": 0}}},
                {"type": "object", "additionalProperties": false, "required": ["type", "url"],
                 "properties": {"type": {"const": "web"}, "url": {"type": "string", "description": "http 或 https"},
                                "fetched_at": {"type": "string"}}},
                {"type": "object", "additionalProperties": false, "required": ["type", "command", "exit_code"],
                 "properties": {"type": {"const": "command"}, "command": {"type": "string"},
                                "cwd": {"type": "string"}, "exit_code": {"type": "integer"}}}
            ]}
        }
    });
    let memory_in = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["text", "scope", "evidence"],
        "properties": {
            "text": {"type": "string", "description": "一条论断，不超过 500 个 Unicode 字符"},
            "scope": {"type": "string", "enum": ["global", "project"]},
            "epistemic": {"type": "string", "enum": ["observed", "user_stated", "inferred"], "description": "可省略；省略时由证据推出"},
            "evidence": {"type": "array", "items": {"type": "string"}, "minItems": 1, "description": "本次调用的 ref 或已有证据 id"},
            "supersedes": {"type": "array", "items": {"type": "string"}, "description": "被取代的记忆 id"}
        }
    });
    let tool = |name: &str, description: &str, required: &[&str], mut props: Value| {
        props["project"] = project.clone();
        let mut schema =
            json!({"type": "object", "additionalProperties": false, "properties": props});
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        json!({"name": name, "description": description, "inputSchema": schema})
    };
    json!([
        tool(
            "search",
            "检索记忆（默认）或证据。中文分词后的词法匹配，每条结果附 `matched_terms`，没找到时换词再搜。默认只返回生效的记忆。本地文件证据在返回前惰性核验，出问题的记忆带 `stale`。不联网。",
            &[],
            json!({
                "query": {"type": "string", "description": "省略则只按过滤条件列出，按最近一次记录或附证时间倒序"},
                "kind": {"type": "string", "enum": ["memory", "evidence"], "default": "memory"},
                "scope": scope,
                "all": all,
                "status": {"type": "array", "items": {"type": "string", "enum": ["active", "superseded", "retracted", "unsupported", "purged"]}, "default": ["active"]},
                "epistemic": {"type": "array", "items": {"type": "string", "enum": ["observed", "user_stated", "inferred"]}},
                "writer": {"type": "string", "description": "Writer 的 name"},
                "cites": {"type": "string", "description": "证据 id：只返回当前引用它的记忆"},
                "stale_only": {"type": "boolean", "default": false, "description": "只返回带可能过期标记的记忆"},
                "source_type": {"type": "array", "items": {"type": "string", "enum": ["conversation", "file", "web", "command"]}, "description": "仅 kind=evidence"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}
            }),
        ),
        tool(
            "get",
            "按 id 取记忆或证据的完整记录。记忆附带全部证据（摘录、来源、核验结果）、`supersedes` / `superseded_by`、附证历史、撤回原因；证据附带 `cited_by`。沿这些 id 再调 `get` 即可追溯。",
            &["ids"],
            json!({"ids": ids(1, Some(20))}),
        ),
        tool(
            "review",
            "列出记忆库中待处置的问题：可能过期、无据。只报告，不处置；每项附建议操作，由你或用户决定。用户要求整理记忆时调用。",
            &[],
            json!({"scope": scope, "all": all}),
        ),
        tool(
            "record",
            "写入证据和/或记忆，原子：任何一条不过检查则整体拒绝。证据用 `ref` 在本次调用内被记忆引用，也可引用已有证据 id。更新记忆＝写一条新记忆并在 `supersedes` 里列出旧记忆；一拆多时把多条新记忆放在同一次调用里。倾向不写：只记以后还会用到、且有证据支撑的论断。用户说「记住 X」时，这句话本身就是证据；对话证据只收用户说的话，你自己说的话不能作为证据，用户确认你的提议时，摘录先写一行 `> 智能体：<你的提议>`，再写用户原话。认知状态可省略：证据全是对话时默认 `user_stated`，否则默认 `inferred`；自己观察到的（如工具输出）请显式写 `observed`。",
            &[],
            json!({
                "evidence": {"type": "array", "items": evidence_in, "description": "新证据；与 memories 至少给一个"},
                "memories": {"type": "array", "items": memory_in, "description": "新记忆；与 evidence 至少给一个"}
            }),
        ),
        tool(
            "corroborate",
            "为一条生效记忆追加支撑证据。重读来源后确认记忆仍成立时用它（以来源当前的摘录为新证据），或 `record` 报 `DUPLICATE_TEXT` 时用它。附证后，可能过期只看这次附证带来的证据。",
            &["memory", "evidence"],
            json!({
                "memory": {"type": "string", "description": "记忆 id"},
                "evidence": {"type": "array", "minItems": 1, "items": {"anyOf": [{"type": "string"}, evidence_in]}, "description": "已有证据 id，或内联新证据（同 record）"}
            }),
        ),
        tool(
            "retract",
            "声明一条记忆本就错误或不该记录，必须说明原因。记忆仍留在历史里，可用 `search` 的 `status` 过滤查到。如果只是情况变了，请用 `record` + `supersedes`，不要撤回。不能物理删除；需要抹除敏感内容时，请建议用户在终端运行 `almanac purge`。",
            &["memory", "reason"],
            json!({
                "memory": {"type": "string", "description": "记忆 id"},
                "reason": {"type": "string", "description": "非空"}
            }),
        ),
        tool(
            "verify",
            "回到证据来源核验摘录是否仍在：`match`（相符）/ `changed`（已变）/ `unreachable`（不可达）/ `unverifiable`（不可核验，如对话）。注意：网页证据会对其 URL 发一次 GET 请求；设 `include_web: false` 可避免联网。只核证据与来源是否一致，不判断记忆是否成立；返回可能过期的记忆，由你决定附证、取代或撤回。",
            &[],
            json!({
                "ids": {"type": "array", "items": {"type": "string"}, "description": "记忆 id；给出时只核验这些记忆，忽略 scope、all"},
                "scope": scope,
                "all": all,
                "include_web": {"type": "boolean", "default": true, "description": "为假时跳过 web 证据"}
            }),
        ),
    ])
}

//! `almanac mcp` (spec ch. 3, 4, 9, 10): a stdio MCP client drives the server.

mod common;

use common::Almanac;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout};

struct Mcp {
    child: Child,
    out: BufReader<ChildStdout>,
    /// Answer to the server's `roots/list`; `None` means the client has no roots capability.
    roots: Option<Vec<String>>,
    next: u64,
}

impl Mcp {
    /// Starts `almanac mcp` and completes `initialize` as client `client`.
    fn start(a: &Almanac, client: &str, roots: Option<Vec<&str>>) -> Mcp {
        let mut child = a.spawn(&["mcp"]);
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut m = Mcp {
            child,
            out,
            roots: roots.map(|r| r.into_iter().map(String::from).collect()),
            next: 0,
        };
        let caps = if m.roots.is_some() {
            json!({"roots": {}})
        } else {
            json!({})
        };
        let init = m.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": caps,
                   "clientInfo": {"name": client, "version": "1"}}),
        );
        assert_eq!(init["serverInfo"]["name"], "almanac", "{init}");
        assert!(init["capabilities"]["tools"].is_object(), "{init}");
        m.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        m
    }

    fn send(&mut self, msg: Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    fn recv(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.out.read_line(&mut line).unwrap() > 0,
            "server closed stdout"
        );
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// Sends a request and returns its `result`, answering any `roots/list` the server asks meanwhile.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let msg = self.recv();
            if msg["method"] == "roots/list" {
                let roots = self
                    .roots
                    .as_ref()
                    .expect("roots/list sent to a client without roots");
                let roots: Vec<Value> = roots.iter().map(|u| json!({"uri": u})).collect();
                let reply = json!({"jsonrpc": "2.0", "id": msg["id"], "result": {"roots": roots}});
                self.send(reply);
                continue;
            }
            assert_eq!(msg["id"], id, "{msg}");
            assert!(msg.get("error").is_none(), "{msg}");
            return msg["result"].clone();
        }
    }

    /// Calls a tool; returns (text, isError).
    fn call(&mut self, tool: &str, args: Value) -> (String, bool) {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert_eq!(r["content"][0]["type"], "text", "{r}");
        (
            r["content"][0]["text"].as_str().unwrap().to_string(),
            r["isError"] == true,
        )
    }

    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let (text, err) = self.call(tool, args);
        assert!(!err, "{text}");
        serde_json::from_str(&text).unwrap()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

/// The 「给模型的描述」 blockquote under `## <tool>` in spec ch. 3.
fn spec_description(tool: &str) -> String {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/spec/03-tools.md"
    ))
    .unwrap();
    let section = spec.split(&format!("\n## {tool}\n")).nth(1).unwrap();
    let after = section.split("**给模型的描述**：").nth(1).unwrap();
    after
        .lines()
        .skip_while(|l| !l.starts_with("> "))
        .take_while(|l| l.starts_with("> "))
        .map(|l| &l[2..])
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn lists_the_seven_tools_with_spec_descriptions_and_schemas() {
    let a = Almanac::new();
    let mut m = Mcp::start(&a, "test-client", None);
    let tools = m.request("tools/list", json!({}))["tools"].clone();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "search",
            "get",
            "review",
            "record",
            "corroborate",
            "retract",
            "verify"
        ]
    );
    for t in tools.as_array().unwrap() {
        let name = t["name"].as_str().unwrap();
        assert_eq!(t["description"], spec_description(name), "{name}");
        let schema = &t["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(schema["additionalProperties"], false, "{name}");
        assert_eq!(schema["properties"]["project"]["type"], "string", "{name}");
    }
    let required = |name: &str| {
        tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()["inputSchema"]["required"]
            .clone()
    };
    assert_eq!(required("get"), json!(["ids"]));
    assert_eq!(required("retract"), json!(["memory", "reason"]));
    assert_eq!(required("corroborate"), json!(["memory", "evidence"]));
}

/// Removes every `at` key: `verify` stamps each check with the current time.
fn strip_at(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("at");
            o.values_mut().for_each(strip_at);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_at),
        _ => {}
    }
}

#[test]
fn each_tool_returns_the_same_text_as_the_cli() {
    let a = Almanac::new();
    let (ev, mem) = a.remember("周报每周五发");
    let (_, other) = a.remember("代码用 rustfmt 格式化");
    let mut m = Mcp::start(&a, "test-client", None);
    let same = |m: &mut Mcp, tool: &str, args: Value| {
        let cli = a.tool(tool, args.clone());
        let (text, is_error) = m.call(tool, args);
        assert_eq!(text, cli.stdout.trim_end(), "{tool}");
        assert_eq!(is_error, cli.code == 1, "{tool}");
        text
    };
    same(&mut m, "search", json!({"query": "周报"}));
    same(&mut m, "get", json!({"ids": [ev, mem]}));
    same(&mut m, "review", json!({}));
    // Retrying the same write is deduped, so both sides return the same ids.
    same(
        &mut m,
        "record",
        json!({"evidence": [{"ref": "e", "excerpt": "记住：周报每周五发", "source": {"type": "conversation"}}],
               "memories": [{"text": "周报每周五发", "scope": "global", "evidence": ["e"]}]}),
    );
    same(
        &mut m,
        "corroborate",
        json!({"memory": mem, "evidence": [ev]}),
    );
    let (text, is_error) = m.call("retract", json!({"memory": other, "reason": "记错了"}));
    assert!(!is_error, "{text}");
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"memory": other, "status": "retracted"})
    );
    // Now both sides hit the same NOT_ACTIVE error.
    let err = same(
        &mut m,
        "retract",
        json!({"memory": other, "reason": "记错了"}),
    );
    assert!(err.contains("NOT_ACTIVE"), "{err}");

    let args = json!({"scope": "global", "include_web": false});
    let mut cli = a.tool("verify", args.clone()).ok();
    let mut mcp = m.ok("verify", args);
    strip_at(&mut cli);
    strip_at(&mut mcp);
    assert_eq!(mcp, cli);
    assert_eq!(mcp["checked"][0]["result"], "unverifiable", "{mcp}");
}

/// Records one evidence through MCP as client `client`; returns the last transaction's writer or the error text.
fn writer_via_mcp(a: &Almanac, client: &str) -> Result<Value, String> {
    let mut m = Mcp::start(a, client, None);
    let (text, is_error) = m.call(
        "record",
        json!({"evidence": [{"excerpt": format!("记住：第 {} 条", a.log().len()), "source": {"type": "conversation"}}]}),
    );
    if is_error {
        return Err(text);
    }
    Ok(a.log().last().unwrap()["writer"].clone())
}

#[test]
fn writer_comes_from_almanac_writer_then_client_info_then_host_variables() {
    let a = Almanac::new()
        .unset("ALMANAC_WRITER")
        .env("CODEX_THREAD_ID", "th1");
    assert_eq!(
        writer_via_mcp(&a, "codex-mcp-client").unwrap(),
        json!({"name": "codex", "session": "th1"})
    );
    let a = a.env("ALMANAC_WRITER", "unknown");
    assert_eq!(writer_via_mcp(&a, "pi").unwrap(), json!({"name": "pi"}));
    let a = a.env("ALMANAC_WRITER", "claude-code");
    assert_eq!(
        writer_via_mcp(&a, "pi").unwrap(),
        json!({"name": "claude-code"})
    );
    // An unmapped client falls through to the CLI's host variables.
    let a = a.unset("ALMANAC_WRITER");
    assert_eq!(
        writer_via_mcp(&a, "claude-code").unwrap(),
        json!({"name": "codex", "session": "th1"})
    );
    let a = a.unset("CODEX_THREAD_ID");
    let err: Value = serde_json::from_str(&writer_via_mcp(&a, "claude-code").unwrap_err()).unwrap();
    assert_eq!(err["error"]["code"], "NO_WRITER", "{err}");
}

#[test]
fn project_path_comes_from_argument_then_client_roots_then_claude_project_dir() {
    let base = tempfile::tempdir().unwrap();
    let dir = |name: &str| {
        let p = base.path().join(name);
        std::fs::create_dir(&p).unwrap();
        p.canonicalize().unwrap()
    };
    let (by_root, by_env, by_arg) = (dir("by root"), dir("by-env"), dir("by-arg"));
    let a = Almanac::new().env("CLAUDE_PROJECT_DIR", by_env.to_str().unwrap());
    let uri = format!("file://{}", by_root.to_str().unwrap().replace(' ', "%20"));
    let mut m = Mcp::start(&a, "test-client", Some(vec![&uri]));
    let mut project_of = |n: usize, project: Option<&std::path::Path>| {
        let mut args = json!({
            "evidence": [{"ref": "e", "excerpt": format!("记住：第 {n} 条"), "source": {"type": "conversation"}}],
            "memories": [{"text": format!("第 {n} 条"), "scope": "project", "evidence": ["e"]}]
        });
        if let Some(p) = project {
            args["project"] = json!(p);
        }
        let id = m.ok("record", args)["memories"][0]["id"].clone();
        m.ok("get", json!({"ids": [id]}))["records"][0]["project"].clone()
    };
    assert_eq!(project_of(1, None), format!("path:{}", by_root.display()));
    assert_eq!(
        project_of(2, Some(&by_arg)),
        format!("path:{}", by_arg.display())
    );
    // A client without roots falls back to CLAUDE_PROJECT_DIR.
    let mut m = Mcp::start(&a, "test-client", None);
    let id = m.ok(
        "record",
        json!({"evidence": [{"ref": "e", "excerpt": "记住：第 3 条", "source": {"type": "conversation"}}],
               "memories": [{"text": "第 3 条", "scope": "project", "evidence": ["e"]}]}),
    )["memories"][0]["id"]
        .clone();
    assert_eq!(
        m.ok("get", json!({"ids": [id]}))["records"][0]["project"],
        format!("path:{}", by_env.display())
    );
}

#[test]
fn each_call_replays_the_log_and_sees_other_processes_writes() {
    let a = Almanac::new();
    let mut m = Mcp::start(&a, "test-client", None);
    assert_eq!(m.ok("search", json!({"query": "周报"}))["hits"], json!([]));
    let (_, mem) = a.remember("周报每周五发");
    assert_eq!(
        m.ok("search", json!({"query": "周报"}))["hits"][0]["id"],
        mem
    );
}

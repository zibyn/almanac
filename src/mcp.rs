//! `almanac mcp` (spec ch. 4): MCP over stdio, newline-delimited JSON-RPC, one message at a time.

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long a `roots/list` answer is awaited before the call goes on without a root.
const ROOTS_WAIT: Duration = Duration::from_secs(2);

struct Server<W> {
    /// Input lines, read on their own thread so a wait for the client can time out.
    input: Receiver<String>,
    out: W,
    /// Client messages read while waiting for a `roots/list` answer, handled next.
    queued: VecDeque<Value>,
    /// The client declared the `roots` capability in `initialize`.
    has_roots: bool,
    sent: u64,
}

pub fn run() -> i32 {
    if let Ok(h) = crate::log::home() {
        crate::log::warn_if_synced(&h);
    }
    let (tx, input) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut s = Server {
        input,
        out: std::io::stdout().lock(),
        queued: VecDeque::new(),
        has_roots: false,
        sent: 0,
    };
    while let Some(msg) = s.next() {
        // Notifications and stray responses need no reply.
        let (Some(id), Some(method)) = (msg.get("id"), msg["method"].as_str()) else {
            continue;
        };
        let reply = s.handle(id, method, &msg["params"]);
        s.send(reply);
    }
    0
}

impl<W: Write> Server<W> {
    /// The next client message; `None` at end of input. Unparsable lines get a JSON-RPC parse error.
    fn next(&mut self) -> Option<Value> {
        if let Some(m) = self.queued.pop_front() {
            return Some(m);
        }
        self.read(None).ok()
    }

    /// The next parsable line; `Err` at end of input or, given a `deadline`, once it passes.
    fn read(&mut self, deadline: Option<Instant>) -> Result<Value, RecvTimeoutError> {
        loop {
            let line = match deadline {
                Some(d) => self
                    .input
                    .recv_timeout(d.saturating_duration_since(Instant::now()))?,
                None => self
                    .input
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected)?,
            };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(&line) {
                Ok(v) => return Ok(v),
                Err(e) => self.send(error(&Value::Null, -32700, &format!("JSON 解析失败：{e}"))),
            }
        }
    }

    fn send(&mut self, msg: Value) {
        // A closed stdout means the client is gone; the read loop ends on its own.
        let _ = writeln!(self.out, "{msg}").and_then(|_| self.out.flush());
    }

    fn handle(&mut self, id: &Value, method: &str, params: &Value) -> Value {
        match method {
            "initialize" => {
                if let Some(name) = params["clientInfo"]["name"].as_str() {
                    let _ = crate::writer::MCP_CLIENT.set(name.to_string());
                }
                self.has_roots = params["capabilities"]["roots"].is_object();
                ok(
                    id,
                    json!({
                        "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "almanac", "version": env!("CARGO_PKG_VERSION")}
                    }),
                )
            }
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({"tools": crate::tools::schema::list()})),
            "tools/call" => {
                // Roots are asked on every call, so a changed root list needs no notification handling.
                let root = if self.has_roots { self.root() } else { None };
                *crate::project::MCP_ROOT.lock().unwrap() = root;
                let name = params["name"].as_str().unwrap_or_default();
                let args = match &params["arguments"] {
                    Value::Null => "{}".to_string(),
                    a => a.to_string(),
                };
                let (text, is_error) = match crate::tools::call(name, &args) {
                    Ok(v) => (v.to_string(), false),
                    Err(e) => (e.to_json().to_string(), true),
                };
                ok(
                    id,
                    json!({"content": [{"type": "text", "text": text}], "isError": is_error}),
                )
            }
            _ => error(id, -32601, &format!("未知方法 {method}")),
        }
    }

    /// Asks the client for `roots/list` and returns the first `file://` root as a path.
    /// No answer within `ROOTS_WAIT` counts as no root; a late answer is then a stray response.
    // shortcut: only the first root is used; spec ch. 1 does not say how to pick among several.
    fn root(&mut self) -> Option<String> {
        self.sent += 1;
        let id = json!(format!("almanac-roots-{}", self.sent));
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "roots/list"}));
        let deadline = Instant::now() + ROOTS_WAIT;
        loop {
            let msg = self.read(Some(deadline)).ok()?;
            if msg["id"] == id && msg.get("method").is_none() {
                let uri = msg["result"]["roots"][0]["uri"].as_str()?;
                return uri.strip_prefix("file://").map(percent_decode);
            }
            self.queued.push_back(msg);
        }
    }
}

/// Decodes `%XX` escapes in a file URI path.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| s.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent_decode_handles_escapes_utf8_and_stray_percent() {
        use super::percent_decode as d;
        assert_eq!(d("/a%20b/%E4%B8%AD"), "/a b/中");
        assert_eq!(d("/100%/x%2"), "/100%/x%2");
    }
}

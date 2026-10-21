//! Black-box harness: runs the compiled `almanac` binary against a temporary store.
//! Every test file does `mod common;` and drives the binary only through `Almanac`.
#![allow(dead_code)]

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct Almanac {
    pub home: tempfile::TempDir,
    env: Vec<(String, String)>,
}

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}\nstderr: {}",
                self.stdout, self.stderr
            )
        })
    }

    /// Asserts success (exit 0) and returns the response JSON.
    pub fn ok(&self) -> Value {
        assert_eq!(
            self.code, 0,
            "expected success, stdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
        self.json()
    }

    /// Asserts a tool error (exit 1, error JSON on stdout) with `code`; returns the `error` object.
    pub fn err(&self, code: &str) -> Value {
        assert_eq!(
            self.code, 1,
            "expected failure, stdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
        let e = self.json()["error"].clone();
        assert_eq!(e["code"], code, "{e}");
        assert!(e["message"].is_string(), "{e}");
        e
    }
}

impl Almanac {
    /// A fresh empty store. The child's environment is cleared and only what the test sets is
    /// passed, so host session variables of whoever runs the tests never leak in.
    /// Defaults: `ALMANAC_HOME=<tempdir>`, `ALMANAC_WRITER=test`.
    pub fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let env = vec![
            ("ALMANAC_HOME".into(), home.path().to_str().unwrap().into()),
            ("ALMANAC_WRITER".into(), "test".into()),
        ];
        Almanac { home, env }
    }

    /// Sets (or overrides) an environment variable for every later run.
    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.unset_mut(k);
        self.env.push((k.into(), v.into()));
        self
    }

    pub fn unset(mut self, k: &str) -> Self {
        self.unset_mut(k);
        self
    }

    fn unset_mut(&mut self, k: &str) {
        self.env.retain(|(key, _)| key != k);
    }

    /// Runs `almanac <args…>`, feeding `stdin` if given.
    pub fn run(&self, args: &[&str], stdin: Option<&str>) -> Out {
        let mut child = Command::new(env!("CARGO_BIN_EXE_almanac"))
            .args(args)
            .env_clear()
            .envs(self.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pipe = child.stdin.take().unwrap();
        if let Some(s) = stdin {
            pipe.write_all(s.as_bytes()).unwrap();
        }
        drop(pipe);
        let o = child.wait_with_output().unwrap();
        Out {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8(o.stdout).unwrap(),
            stderr: String::from_utf8(o.stderr).unwrap(),
        }
    }

    /// Runs a tool: `almanac <tool> --json '<args>'`.
    pub fn tool(&self, tool: &str, args: Value) -> Out {
        self.run(&[tool, "--json", &args.to_string()], None)
    }

    pub fn log_path(&self) -> PathBuf {
        self.home.path().join("log/events.jsonl")
    }

    /// Raw log content ("" when no log exists yet).
    pub fn log_raw(&self) -> String {
        std::fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    /// Log transactions, one per line.
    pub fn log(&self) -> Vec<Value> {
        self.log_raw()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Records one conversation evidence plus one global memory citing it; returns (ev_id, mem_id).
    pub fn remember(&self, text: &str) -> (String, String) {
        let r = self
            .tool(
                "record",
                serde_json::json!({
                    "evidence": [{"ref": "e", "excerpt": format!("记住：{text}"), "source": {"type": "conversation"}}],
                    "memories": [{"text": text, "scope": "global", "evidence": ["e"]}]
                }),
            )
            .ok();
        (
            r["evidence"][0]["id"].as_str().unwrap().into(),
            r["memories"][0]["id"].as_str().unwrap().into(),
        )
    }
}

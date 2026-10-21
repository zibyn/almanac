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
    cwd: Option<PathBuf>,
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
        Almanac {
            home,
            env,
            cwd: None,
        }
    }

    /// Sets (or overrides) an environment variable for every later run.
    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.unset_mut(k);
        self.env.push((k.into(), v.into()));
        self
    }

    /// Runs the binary in `dir` instead of the test process's cwd.
    pub fn cwd(mut self, dir: &std::path::Path) -> Self {
        self.cwd = Some(dir.to_path_buf());
        self
    }

    pub fn unset(mut self, k: &str) -> Self {
        self.unset_mut(k);
        self
    }

    fn unset_mut(&mut self, k: &str) {
        self.env.retain(|(key, _)| key != k);
    }

    /// `almanac <args…>` with this store's env and cwd.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almanac"));
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }
        cmd.args(args).env_clear().envs(self.env.iter().cloned());
        cmd
    }

    /// Spawns `almanac <args…>` with this store's env and cwd, all three pipes captured.
    pub fn spawn(&self, args: &[&str]) -> std::process::Child {
        self.command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Runs `almanac <args…>`, feeding `stdin` if given.
    pub fn run(&self, args: &[&str], stdin: Option<&str>) -> Out {
        let mut child = self.spawn(args);
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

    /// Runs `almanac <args…>` with stdout on a pipe whose read end is already closed; returns the exit code.
    pub fn run_stdout_closed(&self, args: &[&str]) -> i32 {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        self.command(args)
            .stdin(Stdio::null())
            .stdout(writer)
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .code()
            .unwrap_or(-1)
    }

    /// Runs `almanac <args…>` with stdin, stdout and stderr on a pseudo-terminal, typing `input`.
    /// `stdout` is the whole terminal transcript (stderr and echoed input included, `\r\n` line ends).
    // shortcut: drives the pty through util-linux script(1), so Linux only; switch to a pty crate if tests must run elsewhere.
    pub fn run_tty(&self, args: &[&str], input: &str) -> Out {
        let quote = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
        let line: Vec<String> = std::iter::once(env!("CARGO_BIN_EXE_almanac"))
            .chain(args.iter().copied())
            .map(quote)
            .collect();
        let mut cmd = Command::new("script");
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }
        let mut child = cmd
            .args(["-qefc", &line.join(" "), "/dev/null"])
            .env_clear()
            .envs(self.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("script(1) from util-linux is needed for pty tests");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let o = child.wait_with_output().unwrap();
        Out {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"),
            stderr: String::from_utf8_lossy(&o.stderr).into(),
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

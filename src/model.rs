//! Log format (spec ch. 1–2): the on-disk contract. Readers ignore unknown fields and event types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LOG_VERSION: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tx {
    pub v: u64,
    pub tx: String,
    pub at: String,
    pub writer: Writer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Writer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    EvidenceRecorded(Evidence),
    MemoryRecorded(Memory),
    Corroborated {
        memory: String,
        evidence: Vec<String>,
    },
    Retracted {
        memory: String,
        /// Empty once purged (spec ch. 1 「清除与墓碑」); `purged` then holds the tombstone.
        #[serde(default)]
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        purged: Option<Value>,
    },
    /// Two project identities declared to be one project (spec ch. 1 "项目别名").
    ProjectAliased {
        projects: Vec<String>,
    },
    /// Any event type this binary does not know; skipped on replay.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    /// Empty once purged; `purged` then holds the tombstone and `source` only its `type`.
    #[serde(default)]
    pub excerpt: String,
    /// Kept as raw JSON so source types added later still replay.
    pub source: Value,
    /// Tombstone `{at, reason, by}` (spec ch. 1 「清除与墓碑」). Only `purge` writes it, as raw JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    /// Empty once purged; `purged` then holds the tombstone.
    #[serde(default)]
    pub text: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_commit: Option<String>,
    pub epistemic: String,
    pub evidence: Vec<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged: Option<Value>,
}

impl Evidence {
    /// Spec ch. 1 「指纹」: source type + normalized locator + normalized excerpt. Derived, never logged.
    pub fn fingerprint(&self) -> String {
        let s = &self.source;
        let locator: &[&str] = match s["type"].as_str() {
            Some("conversation") => &["session"],
            Some("file") => &["project", "path"],
            Some("web") => &["url"],
            Some("command") => &["command", "cwd"],
            _ => &[],
        };
        let loc: Vec<&Value> = locator.iter().map(|k| &s[*k]).collect();
        serde_json::json!([s["type"], loc, normalize(&self.excerpt)]).to_string()
    }
}

/// Spec ch. 1: Unicode NFC, runs of whitespace folded to one space, trimmed.
pub fn normalize(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `Recorded` in tool output: the transaction an object came from.
#[derive(Debug, Clone, Serialize)]
pub struct Recorded {
    pub tx: String,
    pub at: String,
    pub writer: Writer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
}

impl Tx {
    /// A new transaction stamped with a fresh id and the current time.
    pub fn new(writer: Writer, events: Vec<Event>) -> Self {
        Tx {
            v: LOG_VERSION,
            tx: new_id("tx"),
            at: now(),
            writer,
            git_commit: None,
            events,
        }
    }

    pub fn recorded(&self) -> Recorded {
        Recorded {
            tx: self.tx.clone(),
            at: self.at.clone(),
            writer: self.writer.clone(),
            git_commit: self.git_commit.clone(),
        }
    }
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::generate())
}

/// UTC RFC 3339 with milliseconds, e.g. `2026-10-20T08:00:00.000Z`.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The `YYYY-MM-DD` part of a timestamp from `now`; anything shorter is returned whole.
pub fn day(at: &str) -> &str {
    at.get(..10).unwrap_or(at)
}

/// Human label of an epistemic value, for the Markdown views; unknown values pass through.
pub fn epistemic_label(e: &str) -> &str {
    match e {
        "observed" => "亲见",
        "user_stated" => "用户陈述",
        "inferred" => "推断",
        other => other,
    }
}

/// Human label of a source type, for the Markdown views; unknown types pass through.
pub fn source_type_label(t: &str) -> &str {
    match t {
        "conversation" => "对话",
        "file" => "文件",
        "web" => "网页",
        "command" => "命令输出",
        other => other,
    }
}

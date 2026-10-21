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
    pub scope: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_commit: Option<String>,
    pub epistemic: Epistemic,
    pub evidence: Vec<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged: Option<Value>,
}

/// A memory's epistemic status (spec ch. 1). `Other` keeps a value from a later version (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Epistemic {
    Observed,
    UserStated,
    Inferred,
    Other(String),
}

impl Epistemic {
    /// The values new writes accept.
    pub const KNOWN: &'static [&'static str] = &["observed", "user_stated", "inferred"];

    pub fn as_str(&self) -> &str {
        match self {
            Epistemic::Observed => "observed",
            Epistemic::UserStated => "user_stated",
            Epistemic::Inferred => "inferred",
            Epistemic::Other(s) => s,
        }
    }
}

impl From<String> for Epistemic {
    fn from(s: String) -> Self {
        match s.as_str() {
            "observed" => Epistemic::Observed,
            "user_stated" => Epistemic::UserStated,
            "inferred" => Epistemic::Inferred,
            _ => Epistemic::Other(s),
        }
    }
}

impl From<Epistemic> for String {
    fn from(e: Epistemic) -> Self {
        e.as_str().into()
    }
}

/// A memory's scope (spec ch. 1). `Other` keeps a value from a later version (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum MemoryScope {
    Global,
    Project,
    Other(String),
}

impl MemoryScope {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryScope::Global => "global",
            MemoryScope::Project => "project",
            MemoryScope::Other(s) => s,
        }
    }
}

impl From<String> for MemoryScope {
    fn from(s: String) -> Self {
        match s.as_str() {
            "global" => MemoryScope::Global,
            "project" => MemoryScope::Project,
            _ => MemoryScope::Other(s),
        }
    }
}

impl From<MemoryScope> for String {
    fn from(s: MemoryScope) -> Self {
        s.as_str().into()
    }
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
pub fn epistemic_label(e: &Epistemic) -> &str {
    match e {
        Epistemic::Observed => "亲见",
        Epistemic::UserStated => "用户陈述",
        Epistemic::Inferred => "推断",
        Epistemic::Other(s) => s,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_round_trip_and_unknown_ones_are_kept() {
        for k in Epistemic::KNOWN {
            let e = Epistemic::from(k.to_string());
            assert!(!matches!(e, Epistemic::Other(_)), "{k}");
            assert_eq!(serde_json::to_value(&e).unwrap(), *k);
        }
        for k in ["global", "project"] {
            let s = MemoryScope::from(k.to_string());
            assert!(!matches!(s, MemoryScope::Other(_)), "{k}");
            assert_eq!(serde_json::to_value(&s).unwrap(), k);
        }
        let e: Epistemic = serde_json::from_str(r#""future_value""#).unwrap();
        assert_eq!(serde_json::to_string(&e).unwrap(), r#""future_value""#);
    }
}

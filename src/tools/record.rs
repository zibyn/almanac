//! `record`: write evidence and/or memories atomically, as one transaction (spec ch. 3).

use crate::error::ToolError;
use crate::model::{Event, Evidence, Memory, Tx, new_id};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    evidence: Option<Vec<EvidenceIn>>,
    memories: Option<Vec<MemoryIn>>,
    #[allow(dead_code)]
    project: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceIn {
    #[serde(rename = "ref")]
    local_ref: Option<String>,
    excerpt: String,
    source: SourceIn,
}

// shortcut: only `conversation` so far; file/web/command arrive with #35/#36.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SourceIn {
    Conversation {
        session: Option<String>,
        turn: Option<u64>,
        transcript_path: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryIn {
    text: String,
    scope: Scope,
    epistemic: Option<Epistemic>,
    evidence: Vec<String>,
    supersedes: Option<Vec<String>>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Global,
    Project,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Epistemic {
    Observed,
    UserStated,
    Inferred,
}

impl Epistemic {
    fn as_str(self) -> &'static str {
        match self {
            Epistemic::Observed => "observed",
            Epistemic::UserStated => "user_stated",
            Epistemic::Inferred => "inferred",
        }
    }
}

pub fn run(args: Args) -> Result<Value, ToolError> {
    let writer = crate::writer::infer(|k| std::env::var(k).ok())?;
    let state = super::load()?;
    let mut events = vec![];

    // ref or id → (id, source type)
    let mut known: HashMap<String, (String, String)> = state
        .evidence
        .values()
        .map(|(e, _)| (e.id.clone(), (e.id.clone(), source_type(&e.source))))
        .collect();
    let mut ev_out = vec![];
    for e in args.evidence.unwrap_or_default() {
        let id = new_id("ev");
        let source = match e.source {
            SourceIn::Conversation {
                session,
                turn,
                transcript_path,
            } => {
                let mut s = json!({ "type": "conversation", "host": writer.name });
                if let Some(v) = session.or_else(|| writer.session.clone()) {
                    s["session"] = json!(v);
                }
                if let Some(v) = turn {
                    s["turn"] = json!(v);
                }
                if let Some(v) = transcript_path {
                    s["transcript_path"] = json!(v);
                }
                s
            }
        };
        let mut out = json!({ "id": id, "deduped": false });
        if let Some(r) = &e.local_ref {
            out["ref"] = json!(r);
            known.insert(r.clone(), (id.clone(), source_type(&source)));
        }
        known.insert(id.clone(), (id.clone(), source_type(&source)));
        ev_out.push(out);
        events.push(Event::EvidenceRecorded(Evidence {
            id,
            excerpt: e.excerpt,
            source,
        }));
    }

    let mut mem_out = vec![];
    for m in args.memories.unwrap_or_default() {
        if m.scope == Scope::Project {
            // shortcut: project scope arrives with #36.
            return Err(ToolError::invalid_args("scope project 尚未实现"));
        }
        let mut cited = vec![];
        let mut all_conversation = true;
        for r in &m.evidence {
            let (id, ty) = known
                .get(r)
                .ok_or_else(|| ToolError::new("EVIDENCE_NOT_FOUND", format!("证据 {r} 不存在")))?;
            all_conversation &= ty == "conversation";
            cited.push(id.clone());
        }
        // Spec ch. 1: all-conversation evidence → user_stated, otherwise inferred.
        let epistemic = m.epistemic.unwrap_or(if all_conversation {
            Epistemic::UserStated
        } else {
            Epistemic::Inferred
        });
        let id = new_id("mem");
        mem_out.push(json!({ "id": id, "deduped": false, "similar": [] }));
        events.push(Event::MemoryRecorded(Memory {
            id,
            text: m.text,
            scope: "global".into(),
            project: None,
            root_commit: None,
            epistemic: epistemic.as_str().into(),
            evidence: cited,
            supersedes: m.supersedes.unwrap_or_default(),
        }));
    }

    crate::log::append(&crate::log::home(), &Tx::new(writer, events)).map_err(super::io_error)?;
    Ok(json!({ "evidence": ev_out, "memories": mem_out }))
}

fn source_type(source: &Value) -> String {
    source["type"].as_str().unwrap_or_default().to_string()
}

//! `record`: write evidence and/or memories atomically, as one transaction (spec ch. 3).

use crate::error::ToolError;
use crate::model::{Event, Evidence, Memory, Tx, new_id};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    evidence: Option<Vec<EvidenceIn>>,
    memories: Option<Vec<MemoryIn>>,
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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SourceIn {
    Conversation {
        session: Option<String>,
        turn: Option<u64>,
        transcript_path: Option<String>,
    },
    Web {
        url: String,
        fetched_at: Option<String>,
    },
    Command {
        command: String,
        cwd: Option<String>,
        exit_code: i64,
    },
    File {
        path: String,
        line: Option<u64>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryIn {
    text: String,
    /// A string, not an enum, so a bad value is `SCOPE_INVALID` rather than `INVALID_ARGS`.
    scope: String,
    epistemic: Option<Epistemic>,
    /// Absent is treated like empty: `NO_EVIDENCE`, whose hint helps more than `MISSING_FIELD`'s.
    #[serde(default)]
    evidence: Vec<String>,
    supersedes: Option<Vec<String>>,
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
    let evidence = args.evidence.unwrap_or_default();
    let memories = args.memories.unwrap_or_default();
    if evidence.is_empty() && memories.is_empty() {
        return Err(super::missing_field("evidence 或 memories"));
    }
    let writer = crate::writer::infer(|k| std::env::var(k).ok())?;
    let state = super::load()?;
    let at = crate::model::now();
    let project = crate::project::resolve(args.project.as_deref())?;
    let mut events = vec![];

    // ref or id → (id, source type)
    let mut known: HashMap<String, (String, String)> = state
        .evidence
        .values()
        .map(|(e, _)| (e.id.clone(), (e.id.clone(), source_type(&e.source))))
        .collect();
    // fingerprint → id, for existing evidence and for evidence new in this call
    let mut by_fp: HashMap<String, String> = state
        .evidence
        .values()
        .map(|(e, _)| (e.fingerprint(), e.id.clone()))
        .collect();
    let mut refs = HashSet::new();
    let mut ev_out = vec![];
    // New file evidence to compare against disk once structural checks pass: (id, path, excerpt).
    let mut file_checks = vec![];
    for e in evidence {
        if e.excerpt.len() > 4096 {
            return Err(ToolError::new(
                "EXCERPT_TOO_LONG",
                format!("摘录 {} 字节，超过 4096 字节", e.excerpt.len()),
            )
            .hint("只截取能支撑记忆的那一小段"));
        }
        if let Some(r) = &e.local_ref
            && !refs.insert(r.clone())
        {
            return Err(ToolError::invalid_args(format!("ref {r} 在本次调用中重复")));
        }
        let mut file_path = None;
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
            SourceIn::Web { url, fetched_at } => {
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(ToolError::invalid_args(format!(
                        "web 来源的 url 必须是 http 或 https：{url}"
                    )));
                }
                json!({ "type": "web", "url": url, "fetched_at": fetched_at.unwrap_or_else(|| at.clone()) })
            }
            SourceIn::Command {
                command,
                cwd,
                exit_code,
            } => {
                let mut s =
                    json!({ "type": "command", "command": command, "exit_code": exit_code });
                if let Some(v) = cwd {
                    s["cwd"] = json!(v);
                }
                s
            }
            SourceIn::File { path, line } => {
                crate::source_check::check_path(&path)?;
                file_path = Some(path.clone());
                let mut s = json!({ "type": "file", "project": project.id, "path": path });
                if let Some(v) = line {
                    s["line"] = json!(v);
                }
                s
            }
        };
        let ty = source_type(&source);
        let new = Evidence {
            id: new_id("ev"),
            excerpt: e.excerpt,
            source,
        };
        let (id, deduped) = match by_fp.get(&new.fingerprint()) {
            Some(id) => (id.clone(), true),
            None => {
                by_fp.insert(new.fingerprint(), new.id.clone());
                known.insert(new.id.clone(), (new.id.clone(), ty.clone()));
                (new.id.clone(), false)
            }
        };
        if let Some(path) = file_path {
            file_checks.push((id.clone(), path, new.excerpt.clone()));
        }
        let mut out = json!({ "id": id, "deduped": deduped });
        if let Some(r) = &e.local_ref {
            out["ref"] = json!(r);
            known.insert(r.clone(), (id, ty));
        }
        ev_out.push(out);
        if !deduped {
            events.push(Event::EvidenceRecorded(new));
        }
    }

    // Structural checks on memories, before the file comparison and dedup (spec ch. 3).
    for m in &memories {
        match m.scope.as_str() {
            "global" | "project" => {}
            other => {
                return Err(ToolError::new(
                    "SCOPE_INVALID",
                    format!("scope {other} 不是 global 或 project"),
                )
                .hint("必须显式选择 global 或 project，不按 cwd 默认"));
            }
        }
        let chars = m.text.chars().count();
        if chars > 500 {
            return Err(ToolError::new(
                "TEXT_TOO_LONG",
                format!("正文 {chars} 字符，超过 500 字符"),
            )
            .hint("一条记忆只写一个论断，拆成多条"));
        }
        if m.evidence.is_empty() {
            return Err(ToolError::new("NO_EVIDENCE", "记忆的 evidence 为空")
                .hint("每条记忆至少引用一条证据；用户说「记住 X」时，这句话本身就是证据"));
        }
        if let Some(r) = m.evidence.iter().find(|r| !known.contains_key(*r)) {
            return Err(ToolError::new(
                "EVIDENCE_NOT_FOUND",
                format!("证据 {r} 不存在"),
            ));
        }
    }

    // Spec ch. 3: compared after structural checks, before dedup; even evidence deduped is compared.
    let mut checks = vec![];
    for (id, path, excerpt) in file_checks {
        checks.push((
            id,
            crate::source_check::check_file(&project.root, &path, &excerpt)?,
        ));
    }

    let group = state.alias_group(&project.id);
    let mut mem_out = vec![];
    for m in memories {
        let mut cited = vec![];
        let mut all_conversation = true;
        for r in &m.evidence {
            let (id, ty) = &known[r];
            all_conversation &= ty == "conversation";
            // Two refs may name one merged evidence; cite it once.
            if !cited.contains(id) {
                cited.push(id.clone());
            }
        }
        // Spec ch. 1: all-conversation evidence → user_stated, otherwise inferred.
        let epistemic = m.epistemic.unwrap_or(if all_conversation {
            Epistemic::UserStated
        } else {
            Epistemic::Inferred
        });
        let supersedes = m.supersedes.unwrap_or_default();
        let project_scope = m.scope == "project";
        // Spec ch. 3 「重复」: same scope (project scope by alias group); runs before any `supersedes` check,
        // so a retried supersession dedups.
        let text = crate::model::normalize(&m.text);
        if let Some((old, _)) = state.memories.values().find(|(old, _)| {
            old.scope == m.scope
                && (!project_scope || old.project.as_ref().is_some_and(|p| group.contains(p)))
                && state.status(&old.id) == "active"
                && crate::model::normalize(&old.text) == text
        }) {
            let cites = |ev: &String| {
                state
                    .cited_by
                    .get(ev)
                    .is_some_and(|ms| ms.contains(&old.id))
            };
            if cited.iter().all(cites) && supersedes.iter().all(|s| old.supersedes.contains(s)) {
                mem_out.push(json!({ "id": old.id, "deduped": true, "similar": [] }));
                continue;
            }
            return Err(ToolError::new(
                "DUPLICATE_TEXT",
                format!(
                    "已有正文相同的生效记忆 {}，但本次的证据或取代对象不同",
                    old.id
                ),
            )
            .hint(format!("改用 corroborate 给 {} 附证", old.id)));
        }
        let id = new_id("mem");
        mem_out.push(json!({ "id": id, "deduped": false, "similar": [] }));
        events.push(Event::MemoryRecorded(Memory {
            id,
            text: m.text,
            scope: m.scope,
            project: project_scope.then(|| project.id.clone()),
            root_commit: project_scope.then(|| project.root_commit.clone()).flatten(),
            epistemic: epistemic.as_str().into(),
            evidence: cited,
            supersedes,
        }));
    }

    let home = crate::log::home();
    // A pure retry adds nothing, so it writes nothing.
    if !events.is_empty() {
        let mut tx = Tx::new(writer, events);
        tx.at = at;
        tx.git_commit = project.head;
        crate::log::append(&home, &tx).map_err(super::io_error)?;
    }
    // The cache is a deletable local observation; failing to write it must not fail a committed write.
    if let Err(e) = crate::source_check::store(&home, checks) {
        eprintln!("almanac: failed to write check cache: {e}");
    }
    Ok(json!({ "evidence": ev_out, "memories": mem_out }))
}

fn source_type(source: &Value) -> String {
    source["type"].as_str().unwrap_or_default().to_string()
}

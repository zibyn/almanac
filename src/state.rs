//! Derived state (spec ch. 1): a pure function of the transaction sequence. No I/O.

use crate::model::{Event, Evidence, Memory, Recorded, Tx};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    pub evidence: HashMap<String, (Evidence, Recorded)>,
    pub memories: HashMap<String, (Memory, Recorded)>,
    /// Evidence id → memories currently citing it, in log order.
    pub cited_by: HashMap<String, Vec<String>>,
    /// Memory id → memories whose `supersedes` lists it, in log order.
    pub superseded_by: HashMap<String, Vec<String>>,
    /// Memory id → evidence each corroboration added, with its transaction, in log order.
    pub corroborations: HashMap<String, Vec<(Vec<String>, Recorded)>>,
    /// Memory id → retraction reason and the transaction that retracted it.
    pub retractions: HashMap<String, (String, Recorded)>,
}

impl State {
    pub fn replay(txs: &[Tx]) -> Self {
        let mut s = State::default();
        for tx in txs {
            for ev in &tx.events {
                match ev {
                    Event::EvidenceRecorded(e) => {
                        s.evidence.insert(e.id.clone(), (e.clone(), tx.recorded()));
                    }
                    Event::MemoryRecorded(m) => {
                        for e in &m.evidence {
                            s.cited_by.entry(e.clone()).or_default().push(m.id.clone());
                        }
                        for old in &m.supersedes {
                            s.superseded_by
                                .entry(old.clone())
                                .or_default()
                                .push(m.id.clone());
                        }
                        s.memories.insert(m.id.clone(), (m.clone(), tx.recorded()));
                    }
                    Event::Corroborated { memory, evidence } => {
                        for e in evidence {
                            s.cited_by
                                .entry(e.clone())
                                .or_default()
                                .push(memory.clone());
                        }
                        s.corroborations
                            .entry(memory.clone())
                            .or_default()
                            .push((evidence.clone(), tx.recorded()));
                    }
                    Event::Retracted { memory, reason } => {
                        s.retractions
                            .insert(memory.clone(), (reason.clone(), tx.recorded()));
                    }
                    Event::Unknown => {}
                }
            }
        }
        s
    }

    /// Lifecycle status of a memory.
    // shortcut: no `purged` / `unsupported` yet (they sit above and below `superseded`); #44 adds them.
    pub fn status(&self, memory: &str) -> &'static str {
        if self.retractions.contains_key(memory) {
            "retracted"
        } else if self.superseded_by.contains_key(memory) {
            "superseded"
        } else {
            "active"
        }
    }

    /// Evidence a memory currently cites: recorded evidence, then each corroboration's, in log order.
    pub fn current_evidence(&self, memory: &str) -> Vec<String> {
        let recorded = self.memories.get(memory).map(|(m, _)| &m.evidence);
        let added = self.corroborations.get(memory).into_iter().flatten();
        recorded
            .into_iter()
            .flatten()
            .chain(added.flat_map(|(ev, _)| ev))
            .cloned()
            .collect()
    }

    pub fn cites(&self, memory: &str, evidence: &str) -> bool {
        self.cited_by
            .get(evidence)
            .is_some_and(|ms| ms.iter().any(|m| m == memory))
    }

    /// The `active` memories reached by following `superseded_by` from `memory`, in log order.
    pub fn active_successors(&self, memory: &str) -> Vec<String> {
        let mut out = vec![];
        let mut todo = vec![memory.to_string()];
        let mut seen = std::collections::HashSet::new();
        while let Some(m) = todo.pop() {
            // A merge reaches one memory along several paths; visit it once.
            if !seen.insert(m.clone()) {
                continue;
            }
            for next in self.superseded_by.get(&m).into_iter().flatten().rev() {
                if self.status(next) == "active" && !out.contains(next) {
                    out.push(next.clone());
                }
                todo.push(next.clone());
            }
        }
        out
    }
}

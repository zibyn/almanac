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
                    Event::Unknown => {}
                }
            }
        }
        s
    }

    /// Lifecycle status of a memory.
    // shortcut: only `active` so far; purged/retracted/superseded/unsupported arrive with #37/#44.
    pub fn status(&self, _memory: &str) -> &'static str {
        "active"
    }
}

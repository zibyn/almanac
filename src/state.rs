//! Derived state (spec ch. 1): a pure function of the transaction sequence. No I/O.

use crate::model::{Event, Evidence, Memory, Recorded, Tx};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct State {
    pub evidence: HashMap<String, (Evidence, Recorded)>,
    pub memories: HashMap<String, (Memory, Recorded)>,
    /// Evidence id → memories currently citing it, in log order.
    pub cited_by: HashMap<String, Vec<String>>,
    /// Memory id → memories whose `supersedes` lists it, in log order.
    pub superseded_by: HashMap<String, Vec<String>>,
    /// Declared project alias pairs, in log order.
    pub aliases: Vec<(String, String)>,
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
                    Event::ProjectAliased { projects } => {
                        if let [a, b] = &projects[..] {
                            s.aliases.push((a.clone(), b.clone()));
                        }
                    }
                    Event::Unknown => {}
                }
            }
        }
        s
    }

    /// The alias group of `project` (transitive closure of declared aliases), itself included.
    /// Project-scope queries and memory dedup treat every identity in it as the same project.
    pub fn alias_group(&self, project: &str) -> HashSet<String> {
        let mut group = HashSet::from([project.to_string()]);
        let mut grew = true;
        while grew {
            grew = false;
            for (a, b) in &self.aliases {
                if group.contains(a) != group.contains(b) {
                    group.insert(a.clone());
                    group.insert(b.clone());
                    grew = true;
                }
            }
        }
        group
    }

    /// Lifecycle status of a memory.
    // shortcut: only `active` so far; purged/retracted/superseded/unsupported arrive with #37/#44.
    pub fn status(&self, _memory: &str) -> &'static str {
        "active"
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use std::collections::HashSet;

    #[test]
    fn alias_groups_are_the_transitive_closure() {
        let s = State {
            aliases: [("a", "b"), ("x", "y"), ("c", "b")]
                .map(|(p, q)| (p.to_string(), q.to_string()))
                .into(),
            ..State::default()
        };
        let set = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<HashSet<_>>();
        assert_eq!(s.alias_group("a"), set(&["a", "b", "c"]));
        assert_eq!(s.alias_group("c"), set(&["a", "b", "c"]));
        assert_eq!(s.alias_group("y"), set(&["x", "y"]));
        assert_eq!(s.alias_group("lonely"), set(&["lonely"]));
    }
}

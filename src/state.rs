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
    /// Memory id → evidence each corroboration added, with its transaction, in log order.
    pub corroborations: HashMap<String, Vec<(Vec<String>, Recorded)>>,
    /// Memory id → retraction reason and the transaction that retracted it.
    pub retractions: HashMap<String, (String, Recorded)>,
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
                    Event::Retracted { memory, reason, .. } => {
                        s.retractions
                            .insert(memory.clone(), (reason.clone(), tx.recorded()));
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

    /// Lifecycle status of a memory: the first that holds, top down (spec ch. 1 「记忆的状态」).
    pub fn status(&self, memory: &str) -> &'static str {
        let evidence = self.current_evidence(memory);
        if self.memory_purged(memory) {
            "purged"
        } else if self.retractions.contains_key(memory) {
            "retracted"
        } else if self.superseded_by.contains_key(memory) {
            "superseded"
        } else if !evidence.is_empty() && evidence.iter().all(|e| self.evidence_purged(e)) {
            "unsupported"
        } else {
            "active"
        }
    }

    pub fn memory_purged(&self, id: &str) -> bool {
        self.memories
            .get(id)
            .is_some_and(|(m, _)| m.purged.is_some())
    }

    pub fn evidence_purged(&self, id: &str) -> bool {
        self.evidence
            .get(id)
            .is_some_and(|(e, _)| e.purged.is_some())
    }

    /// Memories on `memory`'s supersession chain, itself excluded: every predecessor and every
    /// successor, followed transitively (not siblings of a split).
    pub fn chain(&self, memory: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let up = |m: &str| self.memories.get(m).map(|(m, _)| m.supersedes.clone());
        let down = |m: &str| self.superseded_by.get(m).cloned();
        for step in [&up as &dyn Fn(&str) -> Option<Vec<String>>, &down] {
            let mut todo = vec![memory.to_string()];
            while let Some(m) = todo.pop() {
                for next in step(&m).unwrap_or_default() {
                    if next != memory && !out.contains(&next) {
                        out.push(next.clone());
                        todo.push(next);
                    }
                }
            }
        }
        out
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

    /// Latest record or corroboration time of a memory.
    pub fn last_at(&self, memory: &str) -> &str {
        self.corroborations
            .get(memory)
            .and_then(|c| c.last())
            .map_or(&self.memories[memory].1.at, |(_, r)| &r.at)
    }

    /// Spec ch. 1 「派生关系」: cites `web` evidence and no `conversation` evidence.
    pub fn web_sourced(&self, memory: &str) -> bool {
        let types: Vec<&str> = self
            .current_evidence(memory)
            .iter()
            .filter_map(|e| self.evidence.get(e)?.0.source["type"].as_str())
            .collect();
        types.contains(&"web") && !types.contains(&"conversation")
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

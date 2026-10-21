//! In-memory search index (spec ch. 5 「索引」「查询」): BM25 over the analyzer's two fields,
//! OR semantics, `terms` weight 2 + `bigrams` weight 1. Built per call, never persisted.

use crate::analyzer::{Analyzed, analyze, segments};
use std::collections::HashMap;

/// Lucene / tantivy defaults; the spec names BM25 without parameters.
const K1: f64 = 1.2;
const B: f64 = 0.75;
const WEIGHTS: [f64; 2] = [2.0, 1.0];

#[derive(Default)]
struct Field {
    tf: HashMap<String, u32>,
    len: usize,
}

pub struct Index {
    docs: Vec<[Field; 2]>,
    df: [HashMap<String, usize>; 2],
    avgdl: [f64; 2],
}

pub struct Hit {
    pub doc: usize,
    pub score: f64,
    /// Query tokens found in the doc (same field), both fields merged, in query order.
    pub matched: Vec<String>,
}

fn fields(a: Analyzed) -> [Vec<String>; 2] {
    [a.terms, a.bigrams]
}

impl Index {
    pub fn new<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut ix = Index {
            docs: Vec::new(),
            df: Default::default(),
            avgdl: [0.0; 2],
        };
        for text in texts {
            let doc = fields(analyze(text)).map(|toks| {
                let mut f = Field {
                    len: toks.len(),
                    ..Field::default()
                };
                for t in toks {
                    *f.tf.entry(t).or_default() += 1;
                }
                f
            });
            for (i, f) in doc.iter().enumerate() {
                for t in f.tf.keys() {
                    *ix.df[i].entry(t.clone()).or_default() += 1;
                }
                ix.avgdl[i] += f.len as f64;
            }
            ix.docs.push(doc);
        }
        let n = ix.docs.len().max(1) as f64;
        ix.avgdl = ix.avgdl.map(|total| total / n);
        ix
    }

    /// Docs matching at least one query token, in doc order. Empty when the query has no tokens.
    pub fn search(&self, query: &str) -> Vec<Hit> {
        // Each query token once per field, tagged with its field, in query (segment) order.
        let mut q: Vec<(usize, String)> = Vec::new();
        for seg in segments(query) {
            for (i, toks) in fields(seg).into_iter().enumerate() {
                for t in toks {
                    if !q.contains(&(i, t.clone())) {
                        q.push((i, t));
                    }
                }
            }
        }
        let n = self.docs.len() as f64;
        let mut hits = Vec::new();
        for (doc, f) in self.docs.iter().enumerate() {
            let mut score = 0.0;
            let mut matched: Vec<String> = Vec::new();
            for (i, t) in &q {
                let Some(&tf) = f[*i].tf.get(t) else { continue };
                let df = self.df[*i][t] as f64;
                let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
                let tf = tf as f64;
                let norm = K1 * (1.0 - B + B * f[*i].len as f64 / self.avgdl[*i]);
                score += WEIGHTS[*i] * idf * tf * (K1 + 1.0) / (tf + norm);
                if !matched.contains(t) {
                    matched.push(t.clone());
                }
            }
            if !matched.is_empty() {
                hits.push(Hit {
                    doc,
                    score,
                    matched,
                });
            }
        }
        hits
    }
}

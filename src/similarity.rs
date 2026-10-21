//! Similarity for `record`'s `similar` soft hint (spec ch. 7 「相似度」).

use crate::state::State;
use serde_json::{Value, json};
use std::collections::HashSet;

/// The `similar` list for a new memory: at most 3, best first. `group` is the current
/// project's alias group, used when `scope` is `project`; `supersedes` are left out.
pub fn similar(
    state: &State,
    text: &str,
    scope: &str,
    group: &HashSet<String>,
    supersedes: &[String],
) -> Value {
    let new = tokens(text);
    let mut hits: Vec<_> = state
        .memories
        .values()
        .filter(|(m, _)| {
            m.scope == scope
                && (scope != "project" || m.project.as_ref().is_some_and(|p| group.contains(p)))
                && state.status(&m.id) == "active"
                && !supersedes.contains(&m.id)
        })
        .filter_map(|(m, _)| Some((m, score(&new, &tokens(&m.text))?)))
        .collect();
    // Best first; ties by latest record or corroboration time, then by id, both descending.
    let last_at = |id: &str| state.last_at(id);
    hits.sort_by(|(a, x), (b, y)| {
        y.total_cmp(x)
            .then_with(|| last_at(&b.id).cmp(last_at(&a.id)))
            .then_with(|| b.id.cmp(&a.id))
    });
    hits.truncate(3);
    hits.into_iter()
        .map(|(m, sim)| json!({ "id": m.id, "text": m.text, "similarity": sim }))
        .collect()
}

/// Token set: all bigrams plus word-segment terms, deduplicated; jieba terms are left out.
pub fn tokens(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for seg in crate::analyzer::segments(text) {
        // A Chinese run always has bigrams and its terms come from jieba; a word segment has none.
        if seg.bigrams.is_empty() {
            out.extend(seg.terms);
        } else {
            out.extend(seg.bigrams);
        }
    }
    out
}

/// Overlap coefficient if the pair counts as similar (floor 3, threshold 0.4), else `None`.
pub fn score(a: &HashSet<String>, b: &HashSet<String>) -> Option<f64> {
    let min = a.len().min(b.len());
    if min < 3 {
        return None;
    }
    let sim = a.intersection(b).count() as f64 / min as f64;
    (sim >= 0.4).then_some(sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(a: &str, b: &str) -> Option<f64> {
        score(&tokens(a), &tokens(b))
    }

    #[test]
    fn token_set_is_bigrams_plus_word_segment_terms_without_jieba() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        assert_eq!(tokens("缓存失效"), set(&["缓存", "存失", "失效"]));
        assert_eq!(
            tokens("用 getUserById 缓存缓存"),
            set(&[
                "用",
                "getuserbyid",
                "get",
                "user",
                "by",
                "id",
                "缓存",
                "存缓"
            ])
        );
    }

    #[test]
    fn an_update_of_the_same_fact_is_similar() {
        // A = {缓存 存失 失效 效时 时间 间是 5 分钟}, B adds 间改 改为 10 instead of 间是 5: 6/8.
        assert_eq!(
            sim("缓存失效时间是 5 分钟", "缓存失效时间改为 10 分钟"),
            Some(0.75)
        );
    }

    #[test]
    fn a_short_memory_contained_in_a_long_one_scores_one() {
        assert_eq!(
            sim(
                "构建用 cargo nextest",
                "CI 里构建用 cargo nextest，并开启 --all-features"
            ),
            Some(1.0)
        );
    }

    #[test]
    fn unrelated_memories_are_not_similar() {
        assert_eq!(sim("用户喜欢 4 空格缩进", "测试用 tokio 运行时"), None);
    }

    #[test]
    fn threshold_is_inclusive_at_point_four() {
        assert_eq!(sim("aa bb cc dd ee", "aa bb xx yy zz"), Some(0.4));
        assert_eq!(sim("aa bb cc dd ee", "aa xx yy zz ww"), None);
    }

    #[test]
    fn sets_under_three_tokens_never_match() {
        // Two tokens fully contained would score 1.0 without the floor.
        assert_eq!(sim("aa bb", "aa bb cc dd"), None);
        assert_eq!(sim("aa bb cc", "aa bb cc dd"), Some(1.0));
    }
}

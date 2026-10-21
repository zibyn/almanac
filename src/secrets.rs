//! Fixed secret rules applied to text on write (spec ch. 3 `record` 「密钥检查」).
//! Applies only to writes, never on replay, so rules can change between versions freely.

use crate::error::ToolError;
use regex::Regex;
use std::sync::LazyLock;

/// The spec's patterns, with `\b` made ASCII (`(?-u:\b)`): a key glued to CJK text such as
/// `密钥AKIA…` must still hit, and Unicode `\b` sees no boundary between `钥` and `A`.
const RULES: &[(&str, &str)] = &[
    ("private_key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("aws_access_key", r"(?-u:\b)(AKIA|ASIA)[0-9A-Z]{16}(?-u:\b)"),
    (
        "github_token",
        r"(?-u:\b)gh[pousr]_[A-Za-z0-9]{36,}|(?-u:\b)github_pat_[A-Za-z0-9_]{22,}",
    ),
    ("gitlab_token", r"(?-u:\b)glpat-[A-Za-z0-9_-]{20,}"),
    ("slack_token", r"(?-u:\b)xox[abprs]-[A-Za-z0-9-]{10,}"),
    ("anthropic_key", r"(?-u:\b)sk-ant-[A-Za-z0-9_-]{20,}"),
    ("openai_key", r"(?-u:\b)sk-(proj-)?[A-Za-z0-9_-]{20,}"),
    ("stripe_key", r"(?-u:\b)[sr]k_live_[A-Za-z0-9]{20,}"),
    ("google_api_key", r"(?-u:\b)AIza[0-9A-Za-z_-]{35}"),
    (
        "jwt",
        r"(?-u:\b)eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
    ),
    (
        "url_credentials",
        r"[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]+:[^/\s@]+@",
    ),
];

static COMPILED: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    RULES
        .iter()
        .map(|(name, re)| (*name, Regex::new(re).unwrap()))
        .collect()
});

/// The earliest hit in `text` as (rule, character offset); ties go to the earlier rule.
fn scan(text: &str) -> Option<(&'static str, usize)> {
    COMPILED
        .iter()
        .filter_map(|(name, re)| re.find(text).map(|m| (m.start(), *name)))
        .min_by_key(|(start, _)| *start)
        .map(|(start, name)| (name, text[..start].chars().count()))
}

/// Checks `(field path, text)` pairs in order; reports the first hit without echoing it.
pub fn check<'a>(fields: impl IntoIterator<Item = (String, &'a str)>) -> Result<(), ToolError> {
    for (path, text) in fields {
        if let Some((rule, offset)) = scan(text) {
            return Err(ToolError::new(
                "SECRET_DETECTED",
                format!("{path} 在字符偏移 {offset} 处命中密钥规则 {rule}"),
            )
            .hint("对话、命令证据与记忆正文，把密钥换成 <redacted> 后重试；file 证据换一段不含密钥的更窄摘录（打码后的摘录对不上文件）；密钥本身就是要记的内容时，不要记录"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::scan;

    fn rule(text: &str) -> Option<&'static str> {
        scan(text).map(|(r, _)| r)
    }

    #[test]
    fn each_rule_hits_its_format_and_misses_a_near_miss() {
        let a20 = "a".repeat(20);
        let a36 = "a".repeat(36);
        let cases: &[(&str, String, String)] = &[
            (
                "private_key",
                "-----BEGIN RSA PRIVATE KEY-----".into(),
                "-----BEGIN PUBLIC KEY-----".into(),
            ),
            (
                "aws_access_key",
                "key=AKIAIOSFODNN7EXAMPLE".into(),
                // 15 chars after the prefix, and a longer run with no boundary after 16.
                "AKIAIOSFODNN7EXAMPL AKIAIOSFODNN7EXAMPLEX".into(),
            ),
            (
                "github_token",
                format!("ghp_{a36} github_pat_{}", "a".repeat(22)),
                format!(
                    "ghp_{} ghx_{a36} github_pat_{}",
                    "a".repeat(35),
                    "a".repeat(21)
                ),
            ),
            (
                "gitlab_token",
                format!("glpat-{a20}"),
                format!("glpat-{} xglpat-{a20}", "a".repeat(19)),
            ),
            (
                "slack_token",
                "xoxb-1234567890".into(),
                "xoxb-123456789 xoxz-1234567890".into(),
            ),
            (
                "anthropic_key",
                format!("sk-ant-{a20}"),
                // Too short for anthropic_key, and `ant-` + 12 is too short for openai_key too.
                format!("sk-ant-{}", "a".repeat(12)),
            ),
            (
                "openai_key",
                format!("sk-proj-{a20}"),
                format!("sk-{} task-{a20}", "a".repeat(19)),
            ),
            (
                "stripe_key",
                format!("rk_live_{a20}"),
                format!("sk_test_{a20} sk_live_{}", "a".repeat(19)),
            ),
            (
                "google_api_key",
                format!("AIza{}", "a".repeat(35)),
                format!("AIza{}", "a".repeat(34)),
            ),
            (
                "jwt",
                "eyJhbGciOiJIUzI1.eyJzdWIiOiIxMjM0.SflKxwRJSMeKKF2Q".into(),
                "eyJhbGciOiJIUzI1.eyJzdWIi.SflKxwRJSMeKKF2Q".into(),
            ),
            (
                "url_credentials",
                "postgres://admin:hunter2@db:5432".into(),
                "https://example.com/a:b@c https://user@example.com".into(),
            ),
        ];
        for (name, hit, miss) in cases {
            assert_eq!(rule(hit), Some(*name), "{hit}");
            assert_eq!(rule(miss), None, "{miss}");
        }
    }

    #[test]
    fn a_key_glued_to_cjk_text_still_hits() {
        assert_eq!(
            scan("密钥AKIAIOSFODNN7EXAMPLE"),
            Some(("aws_access_key", 2))
        );
    }

    #[test]
    fn the_earliest_hit_wins_across_rules() {
        let text = format!("{} then AKIAIOSFODNN7EXAMPLE", "xoxb-1234567890");
        assert_eq!(rule(&text), Some("slack_token"));
    }
}

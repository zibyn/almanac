//! Search analyzer (spec ch. 5 「分析器」): text → `terms` and `bigrams` token sequences.
//! Pure function; the same analyzer runs on documents and queries. Also used by similarity (#40).

/// Version of the tokenization rules in this file; bump on any rule change (spec ch. 5 「规则版本号」).
#[cfg_attr(not(test), allow(dead_code))]
pub const RULES_VERSION: u32 = 1;

/// Tokens of one text, in order; a token's index is its position in the field.
#[derive(Debug, Default, PartialEq)]
pub struct Analyzed {
    pub terms: Vec<String>,
    pub bigrams: Vec<String>,
}

pub fn analyze(text: &str) -> Analyzed {
    let mut out = Analyzed::default();
    for seg in segments(text) {
        out.terms.extend(seg.terms);
        out.bigrams.extend(seg.bigrams);
    }
    out
}

/// Tokens per segment (Chinese run or word), in text order; `analyze` is their concatenation.
pub fn segments(text: &str) -> Vec<Analyzed> {
    use unicode_normalization::UnicodeNormalization;
    let chars: Vec<char> = text.nfkc().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if is_han(chars[i]) {
            while i < chars.len() && is_han(chars[i]) {
                i += 1;
            }
            out.push(han(&chars[start..i]));
        } else if is_word(chars[i]) {
            i += 1;
            loop {
                if i < chars.len() && is_word(chars[i]) {
                    i += 1;
                    continue;
                }
                // A connector counts only between two word characters.
                let k = match chars[i..] {
                    [':', ':', ..] => 2,
                    ['_' | '-' | '.' | '/', ..] => 1,
                    _ => 0,
                };
                if k > 0 && chars.get(i + k).is_some_and(|&c| is_word(c)) {
                    i += k;
                    continue;
                }
                break;
            }
            out.push(word(&chars[start..i]));
        } else {
            i += 1;
        }
    }
    out
}

/// CJK unified ideographs (base, extension A, compatibility, extensions B–H).
fn is_han(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{323AF}')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() && !is_han(c)
}

fn han(chars: &[char]) -> Analyzed {
    static JIEBA: std::sync::OnceLock<jieba_rs::Jieba> = std::sync::OnceLock::new();
    let s: String = chars.iter().collect();
    let terms = JIEBA
        .get_or_init(jieba_rs::Jieba::new)
        .cut_for_search(&s, true)
        .into_iter()
        .map(|t| t.word.to_string())
        .collect();
    let bigrams = if chars.len() == 1 {
        vec![s]
    } else {
        chars.windows(2).map(|w| w.iter().collect()).collect()
    };
    Analyzed { terms, bigrams }
}

/// Identifier split: the whole word, then its parts split on connectors and case boundaries.
fn word(chars: &[char]) -> Analyzed {
    let whole: String = chars.iter().collect::<String>().to_lowercase();
    let mut terms = vec![whole.clone()];
    for piece in chars.split(|c| matches!(c, '_' | '-' | '.' | '/' | ':')) {
        let mut from = 0;
        for i in 1..=piece.len() {
            // Case boundaries are judged on the original case: `aB`, and `ABc` before the `B`.
            let boundary = i == piece.len()
                || (piece[i].is_uppercase()
                    && (piece[i - 1].is_lowercase()
                        || piece[i - 1].is_uppercase()
                            && piece.get(i + 1).is_some_and(|c| c.is_lowercase())));
            if boundary && i > from {
                let part = piece[from..i].iter().collect::<String>().to_lowercase();
                if part != whole {
                    terms.push(part);
                }
                from = i;
            }
        }
    }
    Analyzed {
        terms,
        bigrams: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(text: &str) -> (Vec<&'static str>, Vec<&'static str>) {
        let r = analyze(text);
        let leak = |v: Vec<String>| v.into_iter().map(|s| &*s.leak()).collect();
        (leak(r.terms), leak(r.bigrams))
    }

    #[test]
    fn rules_version_matches_this_table() {
        // Changing any expectation below means the rules changed: bump RULES_VERSION too.
        assert_eq!(RULES_VERSION, 1);
    }

    #[test]
    fn chinese_runs_use_jieba_search_mode_plus_overlapping_bigrams() {
        let (t, b) = a("缓存失效");
        assert!(t.contains(&"缓存") && t.contains(&"失效"), "{t:?}");
        assert_eq!(b, ["缓存", "存失", "失效"]);
    }

    #[test]
    fn bigrams_back_up_jieba_missplits() {
        let (t, b) = a("缺字段");
        assert!(
            !t.contains(&"字段"),
            "jieba now splits 缺字段 correctly: {t:?}"
        );
        assert_eq!(b, ["缺字", "字段"]);
        let (t, b) = a("智能体");
        assert!(!t.contains(&"智能体"), "{t:?}");
        assert_eq!(b, ["智能", "能体"]);
    }

    #[test]
    fn single_character_chinese_run_is_its_own_bigram() {
        let (t, b) = a("用 tokio 跑");
        assert_eq!(t, ["用", "tokio", "跑"]);
        assert_eq!(b, ["用", "跑"]);
    }

    #[test]
    fn identifiers_yield_whole_then_parts() {
        let cases: &[(&str, &[&str])] = &[
            (
                "append_only_log",
                &["append_only_log", "append", "only", "log"],
            ),
            ("tokio::spawn", &["tokio::spawn", "tokio", "spawn"]),
            ("getUserById", &["getuserbyid", "get", "user", "by", "id"]),
            ("HTTPServer", &["httpserver", "http", "server"]),
            ("v0.3.5", &["v0.3.5", "v0", "3", "5"]),
            ("http/2", &["http/2", "http", "2"]),
            ("foo", &["foo"]),
            ("a-b", &["a-b", "a", "b"]),
        ];
        for (input, want) in cases {
            let (t, b) = a(input);
            assert_eq!(&t, want, "{input}");
            assert!(b.is_empty(), "{input}");
        }
    }

    #[test]
    fn connectors_only_join_inside_a_word() {
        assert_eq!(a("_init_ a:b x.").0, ["init", "a", "b", "x"]);
    }

    #[test]
    fn nfkc_folds_full_width_and_lowercases() {
        assert_eq!(a("ＡＢＣ１２").0, ["abc12"]);
        assert_eq!(a("Ｔｏｋｉｏ").0, ["tokio"]);
    }

    #[test]
    fn punctuation_and_whitespace_make_no_tokens() {
        assert_eq!(analyze("，。！？ ... --- :: 「」"), Analyzed::default());
    }
}

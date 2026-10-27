// SPDX-License-Identifier: Apache-2.0
//! Text handling for the memory store: search tokenization (Hangul bigrams so FTS5 can match Korean
//! without a morphological analyzer), URL query stripping, secret detection and token estimates.
use regex::Regex;
use std::sync::LazyLock;

pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Rough LLM token estimate: ASCII ≈ 4 chars/token, everything else (Hangul/CJK) ≈ 1.5 chars/token.
pub fn est_tokens(s: &str) -> u32 {
    let ascii = s.chars().filter(char::is_ascii).count() as f64;
    let other = s.chars().count() as f64 - ascii;
    (ascii / 4.0 + other * 0.67).ceil() as u32
}

// ── URLs ────────────────────────────────────────────────────────────────────

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'\)\]]+"#).unwrap());

/// Drop `?query` and `#fragment` from every http(s) URL — they carry tokens, ids and session state.
pub fn strip_url_queries(s: &str) -> String {
    URL.replace_all(s, |c: &regex::Captures| {
        let m = &c[0];
        m[..m.find(['?', '#']).unwrap_or(m.len())].to_string()
    })
    .into_owned()
}

/// `https://WWW.Nid.Naver.com:443/x?y#z` → `nid.naver.com`. None when the input is not a plain
/// http(s) URL or domain.
pub fn normalize_site(input: &str) -> Option<String> {
    let s = input.trim().to_lowercase();
    let rest = if let Some(r) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        r
    } else if s.contains("://") {
        return None;
    } else {
        &s
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    let host = host.strip_prefix("www.").unwrap_or(host);
    let valid = !host.is_empty()
        && host.chars().count() <= 253
        && host.split('.').all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'));
    valid.then(|| host.to_string())
}

/// `a.b.naver.com` → `[a.b.naver.com, b.naver.com, naver.com]` (never a bare TLD).
pub fn site_chain(site: &str) -> Vec<String> {
    let labels: Vec<&str> = site.split('.').collect();
    (0..labels.len().saturating_sub(1).max(1)).map(|i| labels[i..].join(".")).collect()
}

/// 2 = same site, 1 = parent/child domain, 0 = unrelated.
pub fn site_rank(entry: &str, query: &str) -> u8 {
    if entry.is_empty() || query.is_empty() {
        0
    } else if entry == query {
        2
    } else if query.ends_with(&format!(".{entry}")) || entry.ends_with(&format!(".{query}")) {
        1
    } else {
        0
    }
}

// ── secrets ─────────────────────────────────────────────────────────────────

static SECRET_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)\b(?:pass(?:word|wd|phrase)?|pwd|secret|token|api[_-]?key|apikey|auth(?:orization)?|credentials?|private[_ -]?key|access[_-]?key)\b\s*[:=]\s*\S{3,}",
        r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}",
        r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
        r"\b(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}",
        r"\bgh[pousr]_[A-Za-z0-9]{20,}",
        r"\bAKIA[0-9A-Z]{16}\b",
        r"\bxox[abprs]-[A-Za-z0-9-]{10,}",
        r"\bAIza[0-9A-Za-z_-]{30,}",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});
static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:\d[ -]?){12,18}\d\b").unwrap());

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| if i % 2 == 1 { let x = d * 2; if x > 9 { x - 9 } else { x } } else { d })
        .sum();
    sum.is_multiple_of(10)
}

fn looks_like_key_blob(token: &str) -> bool {
    let t = token.trim_matches(|c: char| ",.;:()[]\"'`".contains(c));
    if t.chars().count() < 32 || !t.chars().all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c)) {
        return false;
    }
    let (upper, lower, digit) = (
        t.chars().any(|c| c.is_ascii_uppercase()),
        t.chars().any(|c| c.is_ascii_lowercase()),
        t.chars().any(|c| c.is_ascii_digit()),
    );
    (upper && lower && digit) || (t.chars().all(|c| c.is_ascii_hexdigit()) && digit)
}

/// Heuristic: credentials, tokens, keys, private keys or card numbers. Callers strip URL queries first.
pub fn contains_secret(s: &str) -> bool {
    if SECRET_PATTERNS.iter().any(|re| re.is_match(s)) || s.split_whitespace().any(looks_like_key_blob) {
        return true;
    }
    CARD.find_iter(s).any(|m| {
        let digits: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
        (13..=19).contains(&digits.len()) && luhn(&digits)
    })
}

// ── search tokens ───────────────────────────────────────────────────────────

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F | 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF)
}

/// Words are lowercased alphanumeric runs; Hangul/CJK runs become overlapping bigrams (a lone
/// character stays a unigram). The same function builds index text and queries, so a query run
/// like "로그인을" still matches an indexed "로그인" through its shared bigrams.
pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut word, mut run): (String, Vec<char>) = (String::new(), Vec::new());
    fn flush_word(word: &mut String, out: &mut Vec<String>) {
        if !word.is_empty() {
            out.push(std::mem::take(word));
        }
    }
    fn flush_run(run: &mut Vec<char>, out: &mut Vec<String>) {
        match run.len() {
            0 => {}
            1 => out.push(run[0].to_string()),
            _ => out.extend(run.windows(2).map(|w| w.iter().collect::<String>())),
        }
        run.clear();
    }
    for c in s.chars() {
        if is_cjk(c) {
            flush_word(&mut word, &mut out);
            run.push(c);
        } else if c.is_alphanumeric() {
            flush_run(&mut run, &mut out);
            word.extend(c.to_lowercase());
        } else {
            flush_word(&mut word, &mut out);
            flush_run(&mut run, &mut out);
        }
    }
    flush_word(&mut word, &mut out);
    flush_run(&mut run, &mut out);
    out
}

pub fn index_text(s: &str) -> String {
    tokens(s).join(" ")
}

/// FTS5 MATCH expression: every token quoted (so query syntax in user text is inert), OR-joined,
/// ranked by BM25. None when the text has no searchable token.
pub fn match_expr(query: &str) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    let terms: Vec<String> = tokens(query).into_iter().filter(|t| seen.insert(t.clone())).take(32).map(|t| format!("\"{t}\"")).collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

/// Task-query matching data, independent of UI language: omit conversational filler.
pub fn task_search_terms(text: &str) -> std::collections::HashSet<String> {
    tokens(text)
        .into_iter()
        .filter(|word| {
            word.chars().count() > 1
                && !matches!(
                    word.as_str(),
                    "오늘"
                        | "어제"
                        | "아까"
                        | "다시"
                        | "하나"
                        | "그거"
                        | "그것"
                        | "이것"
                        | "저것"
                        | "해줘"
                        | "해줄"
                        | "줄래"
                        | "줘요"
                        | "작업"
                        | "추가"
                        | "이어"
                        | "어서"
                        | "계속"
                        | "the"
                        | "and"
                        | "for"
                        | "with"
                        | "please"
                        | "today"
                        | "yesterday"
                        | "another"
                        | "again"
                        | "continue"
                        | "add"
                        | "work"
                        | "task"
                )
        })
        .collect()
}

/// Recognize a reference to earlier work even when the query has no topic words.
pub fn is_task_continuation(query: &str) -> bool {
    let query = query.to_lowercase();
    [
        "이어서",
        "계속",
        "어제",
        "아까",
        "그거",
        "그것",
        "하나 더",
        "continue",
        "another",
        "yesterday",
        "again",
    ]
    .iter()
    .any(|word| query.contains(word))
}

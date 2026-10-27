//! Baseline deterministic screening of model-written memory text (docs/spec/memory.md "쓰기 규칙").
//!
//! Conservative and heuristic: it rejects text that carries a URL, an address, a secret-looking token, hidden
//! characters, an instruction override, a credential request, or a push toward another site. Errors are reason
//! codes only (never an echo of the input). wam-core's redaction replaces it when that lands.

const HIDDEN: &[char] = &[
    '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}',
    '\u{202d}', '\u{202e}', '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{feff}',
];
const TLDS: &[&str] = &[
    "com",
    "net",
    "org",
    "io",
    "ai",
    "kr",
    "co",
    "app",
    "dev",
    "me",
    "info",
    "biz",
    "xyz",
    "site",
    "online",
    "tv",
    "gg",
    "us",
    "uk",
    "de",
    "fr",
    "jp",
    "cn",
    "ru",
    "eu",
    "so",
    "to",
    "ly",
    "sh",
    "cc",
    "pro",
    "tech",
    "cloud",
    "page",
    "link",
    "top",
    "club",
    "store",
    "shop",
    "edu",
    "gov",
    "mil",
    "int", // Reserved names (RFC 2606) that fixtures and attackers use:
    "example",
    "test",
    "invalid",
    "local",
    "localhost",
];
const SCHEMES: &[&str] = &[
    "http://",
    "https://",
    "ftp://",
    "data:",
    "javascript:",
    "www.",
];
const OVERRIDE: &[&str] = &[
    "ignore previous",
    "ignore all previous",
    "ignore the above",
    "ignore your instructions",
    "ignore the user",
    "disregard",
    "forget the above",
    "system notice",
    "new instructions",
    "이전 지시",
    "지시를 무시",
    "무시하고",
];
const ENTER_WORDS: &[&str] = &[
    "enter",
    "type in",
    "type your",
    "provide",
    "paste",
    "submit",
    "fill in",
    "input",
    "입력",
    "제출",
    "붙여넣",
];
const SENSITIVE_OBJECTS: &[&str] = &[
    "password",
    "passcode",
    "credential",
    "api key",
    "api-key",
    "token",
    "otp",
    "verification code",
    "card number",
    "ssn",
    "비밀번호",
    "자격증명",
    "자격 증명",
    "인증번호",
    "카드번호",
    "토큰",
];
const OFF_SITE: &[&str] = &[
    "another site",
    "other site",
    "another website",
    "different domain",
    "external site",
    "다른 사이트",
    "다른 도메인",
    "외부 사이트",
];
const SEND_WORDS: &[&str] = &["send", "upload", "forward", "exfiltrate", "전송", "업로드"];
const SEND_TARGETS: &[&str] = &[
    "to an external",
    "to another",
    "to a server",
    "to the server",
    "외부로",
    "서버로",
];

/// Substring match that, for ASCII terms, also requires word boundaries ("ssn" must not match "classname").
fn contains_term(haystack: &str, needle: &str) -> bool {
    let ascii = needle.is_ascii();
    let mut start = 0;
    while let Some(found) = haystack[start..].find(needle) {
        let at = start + found;
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        if !ascii
            || (before.is_none_or(|c| !c.is_alphanumeric())
                && after.is_none_or(|c| !c.is_alphanumeric()))
        {
            return true;
        }
        start = at + needle.len();
        if start >= haystack.len() {
            break;
        }
    }
    false
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| contains_term(haystack, needle))
}

fn looks_like_secret(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let long = token.len() >= 16;
    if long
        && (lower.starts_with("sk-")
            || lower.starts_with("ghp_")
            || lower.starts_with("gho_")
            || lower.starts_with("akia"))
    {
        return true;
    }
    if token.len() >= 20
        && (token.starts_with("eyJ") || token.starts_with("AIza") || lower.starts_with("xox"))
    {
        return true;
    }
    token.len() >= 32
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '=' | '/'))
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_foreign_domain(token: &str, allowed_host: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let labels: Vec<&str> = lower.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|label| label.is_empty()) {
        return false;
    }
    let tld = labels[labels.len() - 1];
    if !TLDS.contains(&tld)
        || !labels
            .iter()
            .all(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        return false;
    }
    let allowed = allowed_host.to_ascii_lowercase();
    !(lower == allowed || lower.ends_with(&format!(".{allowed}")))
}

/// `Ok(())` when the text may be stored; otherwise a stable reason code.
pub fn check_text(text: &str, allowed_host: &str, multiline: bool) -> Result<(), &'static str> {
    if text.chars().any(|c| HIDDEN.contains(&c)) {
        return Err("hidden_character");
    }
    if text
        .chars()
        .any(|c| c.is_control() && !(multiline && c == '\n'))
    {
        return Err("control_character");
    }
    let lower = text.to_lowercase();
    // Prefixes ("https://", "www.") are plain substring matches; only the word lists below use word boundaries.
    if SCHEMES.iter().any(|scheme| lower.contains(scheme)) {
        return Err("url");
    }
    if text.contains('@') {
        return Err("email_or_handle");
    }
    if lower.contains("-----begin") {
        return Err("secret");
    }
    for token in text.split(|c: char| c.is_whitespace() || ",;()[]{}\"'<>".contains(c)) {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric());
        if token.is_empty() {
            continue;
        }
        if looks_like_secret(token) {
            return Err("secret");
        }
        if is_foreign_domain(token, allowed_host) {
            return Err("foreign_domain");
        }
    }
    if contains_any(&lower, OVERRIDE) {
        return Err("instruction_override");
    }
    if contains_any(&lower, ENTER_WORDS) && contains_any(&lower, SENSITIVE_OBJECTS) {
        return Err("credential_request");
    }
    if contains_any(&lower, OFF_SITE) {
        return Err("off_site_navigation");
    }
    if contains_any(&lower, SEND_WORDS) && contains_any(&lower, SEND_TARGETS) {
        return Err("exfiltration");
    }
    Ok(())
}

/// Rough token estimate: ASCII at 4 characters per token, other scripts at 2.
pub fn est_tokens(text: &str) -> usize {
    let ascii = text.chars().filter(char::is_ascii).count();
    let other = text.chars().count() - ascii;
    ascii.div_ceil(4) + other.div_ceil(2)
}

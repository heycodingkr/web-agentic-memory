//! Safe URL projection and explicit route-scope matching.
//!
//! Query, fragment, userinfo, and the input string are never retained. Public mode
//! accepts HTTPS public DNS names only. Loopback mode is for isolated local fixtures.

use percent_encoding::percent_decode_str;
use std::fmt;
use url::{Host, Url};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UrlMode {
    Public,
    LoopbackTest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UrlPolicyError {
    Malformed,
    UnsupportedScheme,
    HostNotPublic,
    AmbiguousPath,
    SensitivePath,
}

impl UrlPolicyError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed => "invalid_url",
            Self::UnsupportedScheme => "unsupported_scheme",
            Self::HostNotPublic => "host_not_public",
            Self::AmbiguousPath => "scope_ambiguous",
            Self::SensitivePath => "sensitive_path",
        }
    }
}

impl fmt::Display for UrlPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for UrlPolicyError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CanonicalUrl {
    host: String,
    path: String,
}

impl CanonicalUrl {
    pub fn parse(input: &str, mode: UrlMode) -> Result<Self, UrlPolicyError> {
        Self::parse_with_dynamic_segments(input, mode, &[])
    }

    /// Applies explicit zero-based path segment rules selected by a trusted host adapter.
    /// Rules are for opaque account names that cannot be inferred safely from syntax.
    pub fn parse_with_dynamic_segments(
        input: &str,
        mode: UrlMode,
        dynamic_segments: &[(usize, DynamicSegmentKind)],
    ) -> Result<Self, UrlPolicyError> {
        let parsed = Url::parse(input).map_err(|_| UrlPolicyError::Malformed)?;
        let raw_host = parsed.host().ok_or(UrlPolicyError::HostNotPublic)?;
        let (host, is_loopback) = match raw_host {
            Host::Domain(domain) => {
                let domain = domain.to_ascii_lowercase();
                let local = is_localhost_name(&domain);
                match mode {
                    UrlMode::Public if !is_public_dns_name(&domain) => {
                        return Err(UrlPolicyError::HostNotPublic);
                    }
                    UrlMode::LoopbackTest if !local => {
                        return Err(UrlPolicyError::HostNotPublic);
                    }
                    _ => {}
                }
                (domain, local)
            }
            Host::Ipv4(ip) => {
                if mode != UrlMode::LoopbackTest || !ip.is_loopback() {
                    return Err(UrlPolicyError::HostNotPublic);
                }
                (ip.to_string(), true)
            }
            Host::Ipv6(ip) => {
                if mode != UrlMode::LoopbackTest || !ip.is_loopback() {
                    return Err(UrlPolicyError::HostNotPublic);
                }
                (format!("[{ip}]"), true)
            }
        };

        let scheme_allowed = parsed.scheme() == "https"
            || (mode == UrlMode::LoopbackTest && parsed.scheme() == "http" && is_loopback);
        if !scheme_allowed {
            return Err(UrlPolicyError::UnsupportedScheme);
        }
        if mode == UrlMode::LoopbackTest && !is_loopback {
            return Err(UrlPolicyError::HostNotPublic);
        }

        let host = match parsed.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        };
        let path = normalize_path(parsed.path(), dynamic_segments)?;
        Ok(Self { host, path })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn exact_pattern(&self) -> UrlPattern {
        UrlPattern {
            host: self.host.clone(),
            path: self.path.clone(),
            coverage: RouteCoverage::Exact,
        }
    }

    pub fn subtree_pattern(&self) -> UrlPattern {
        UrlPattern {
            host: self.host.clone(),
            path: self.path.clone(),
            coverage: RouteCoverage::Subtree,
        }
    }

    pub fn host_pattern(&self) -> UrlPattern {
        UrlPattern {
            host: self.host.clone(),
            path: "/".to_owned(),
            coverage: RouteCoverage::Host,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DynamicSegmentKind {
    Id,
    Account,
}

impl DynamicSegmentKind {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Id => ":id",
            Self::Account => ":account",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RouteCoverage {
    /// This canonical route only.
    Exact,
    /// This route and descendants. Constructing this is an explicit scope decision.
    Subtree,
    /// Every route on this host.
    Host,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteMatchLevel {
    Host = 1,
    Parent = 2,
    Exact = 3,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UrlPattern {
    host: String,
    path: String,
    coverage: RouteCoverage,
}

impl UrlPattern {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn coverage(&self) -> RouteCoverage {
        self.coverage
    }

    pub fn match_level(&self, url: &CanonicalUrl) -> Option<RouteMatchLevel> {
        if self.host != url.host {
            return None;
        }

        match self.coverage {
            RouteCoverage::Host => Some(RouteMatchLevel::Host),
            RouteCoverage::Exact if self.path == url.path => Some(RouteMatchLevel::Exact),
            RouteCoverage::Exact => None,
            RouteCoverage::Subtree if self.path == url.path => Some(RouteMatchLevel::Exact),
            RouteCoverage::Subtree if is_descendant(&self.path, &url.path) => {
                Some(RouteMatchLevel::Parent)
            }
            RouteCoverage::Subtree => None,
        }
    }
}

fn is_localhost_name(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

fn is_public_dns_name(host: &str) -> bool {
    if host.len() > 253 || !host.contains('.') {
        return false;
    }
    if [
        ".localhost",
        ".local",
        ".internal",
        ".test",
        ".invalid",
        ".example",
    ]
    .iter()
    .any(|suffix| host == suffix.trim_start_matches('.') || host.ends_with(suffix))
    {
        return false;
    }

    let mut labels = host.split('.');
    let valid_labels = labels.clone().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    });
    let tld = labels.next_back().unwrap_or_default();
    valid_labels && tld.len() >= 2
}

fn normalize_path(
    path: &str,
    dynamic_segments: &[(usize, DynamicSegmentKind)],
) -> Result<String, UrlPolicyError> {
    if path.len() > 2048 || !path.starts_with('/') {
        return Err(UrlPolicyError::AmbiguousPath);
    }
    if path == "/" {
        return Ok("/".to_owned());
    }

    let mut segments: Vec<&str> = path[1..].split('/').collect();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(UrlPolicyError::AmbiguousPath);
    }
    if dynamic_segments
        .iter()
        .any(|(index, _)| *index >= segments.len())
    {
        return Err(UrlPolicyError::AmbiguousPath);
    }
    for (position, (index, _)) in dynamic_segments.iter().enumerate() {
        if dynamic_segments[..position]
            .iter()
            .any(|(other, _)| other == index)
        {
            return Err(UrlPolicyError::AmbiguousPath);
        }
    }

    let mut safe_segments = Vec::with_capacity(segments.len());
    for (index, raw_segment) in segments.into_iter().enumerate() {
        let decoded = percent_decode_str(raw_segment)
            .decode_utf8()
            .map_err(|_| UrlPolicyError::AmbiguousPath)?;
        if decoded.contains('/') || decoded.contains('\\') || decoded.chars().any(char::is_control)
        {
            return Err(UrlPolicyError::AmbiguousPath);
        }
        if decoded == "." || decoded == ".." {
            return Err(UrlPolicyError::AmbiguousPath);
        }

        if looks_like_secret(&decoded) {
            return Err(UrlPolicyError::SensitivePath);
        } else if let Some((_, kind)) = dynamic_segments
            .iter()
            .find(|(position, _)| *position == index)
        {
            safe_segments.push(kind.placeholder().to_owned());
        } else if looks_like_email(&decoded) {
            safe_segments.push(":account".to_owned());
        } else if is_numeric(&decoded) || is_uuid(&decoded) {
            safe_segments.push(":id".to_owned());
        } else {
            safe_segments.push(canonicalize_percent_encoding(raw_segment)?);
        }
    }

    Ok(format!("/{}", safe_segments.join("/")))
}

fn canonicalize_percent_encoding(segment: &str) -> Result<String, UrlPolicyError> {
    let bytes = segment.as_bytes();
    let mut output = String::with_capacity(segment.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(UrlPolicyError::AmbiguousPath);
            }
            let high = hex_value(bytes[index + 1]).ok_or(UrlPolicyError::AmbiguousPath)?;
            let low = hex_value(bytes[index + 2]).ok_or(UrlPolicyError::AmbiguousPath)?;
            let decoded = high * 16 + low;
            if is_unreserved(decoded) {
                output.push(decoded as char);
            } else {
                output.push('%');
                output.push(hex_digit(high));
                output.push(hex_digit(low));
            }
            index += 3;
        } else if bytes[index].is_ascii() {
            output.push(bytes[index] as char);
            index += 1;
        } else {
            return Err(UrlPolicyError::AmbiguousPath);
        }
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> char {
    b"0123456789ABCDEF"[value as usize] as char
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn looks_like_email(segment: &str) -> bool {
    let Some((_, domain)) = segment.split_once('@') else {
        return false;
    };
    domain.contains('.')
}

fn is_numeric(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_uuid(segment: &str) -> bool {
    if segment.len() != 36 {
        return false;
    }
    segment.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

fn looks_like_secret(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    if ["sk-", "ghp_", "github_pat_", "xox", "eyj", "bearer-"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }

    let has_alpha = segment.bytes().any(|byte| byte.is_ascii_alphabetic());
    let has_digit = segment.bytes().any(|byte| byte.is_ascii_digit());
    let ascii_alphanumeric = segment.bytes().all(|byte| byte.is_ascii_alphanumeric());
    segment.chars().count() >= 24 && has_alpha && has_digit && ascii_alphanumeric
}

fn is_descendant(parent: &str, child: &str) -> bool {
    if parent == "/" {
        return child != "/";
    }
    child
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

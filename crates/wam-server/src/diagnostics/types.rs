use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Web,
    Cli,
    Mcp,
    Server,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Recall,
    Expand,
    Vocab,
    Catalog,
    Profile,
    Dashboard,
    Contribution,
    Outcome,
    Auth,
    Device,
    Review,
    Usage,
    Telemetry,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NetworkError,
    HttpError,
    InvalidResponse,
    InvalidInput,
    UnhandledError,
    UnhandledRejection,
    Panic,
    InternalError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEvent {
    pub schema_version: u8,
    pub event_id: String,
    pub trace_id: String,
    pub source: Source,
    pub operation: Operation,
    pub error_code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    pub duration_ms: u64,
}
impl DiagnosticEvent {
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && valid_id(&self.event_id)
            && valid_id(&self.trace_id)
            && self.source != Source::Server
            && self.duration_ms <= 600_000
            && self
                .status_code
                .is_none_or(|code| (100..=599).contains(&code))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredDiagnostic {
    pub event_id: String,
    pub trace_id: String,
    pub kind: String,
    pub source: Source,
    pub operation: Operation,
    pub error_code: Option<ErrorCode>,
    pub method: String,
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub received_at: u64,
}
impl StoredDiagnostic {
    pub fn valid(&self) -> bool {
        valid_id(&self.event_id)
            && valid_id(&self.trace_id)
            && matches!(self.kind.as_str(), "request" | "client_error")
            && matches!(
                self.method.as_str(),
                "GET"
                    | "POST"
                    | "PUT"
                    | "DELETE"
                    | "PATCH"
                    | "HEAD"
                    | "OPTIONS"
                    | "OTHER"
                    | "CLIENT"
            )
            && self.duration_ms <= 600_000
            && self.received_at <= i64::MAX as u64
            && self.status_code.is_none_or(|c| (100..=599).contains(&c))
    }
    pub fn from_event(event: DiagnosticEvent) -> Self {
        Self {
            event_id: event.event_id,
            trace_id: event.trace_id,
            kind: "client_error".into(),
            source: event.source,
            operation: event.operation,
            error_code: Some(event.error_code),
            method: "CLIENT".into(),
            status_code: event.status_code,
            duration_ms: event.duration_ms,
            received_at: now(),
        }
    }
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn random_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        use sha2::{Digest, Sha256};
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seed = format!(
            "{}:{}:{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        bytes.copy_from_slice(&Sha256::digest(seed.as_bytes())[..16]);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn operation(path: &str) -> Operation {
    if path == "/v1/memories/recall" {
        Operation::Recall
    } else if path == "/v1/memories/vocab" {
        Operation::Vocab
    } else if path == "/v1/memories" {
        Operation::Catalog
    } else if path.starts_with("/v1/memories/") {
        Operation::Expand
    } else if path.starts_with("/v1/auth/") {
        Operation::Auth
    } else if path.starts_with("/v1/link/") || path.starts_with("/v1/me/devices") {
        Operation::Device
    } else if path.starts_with("/v1/review/") {
        Operation::Review
    } else if path.starts_with("/v1/me/contributions") || path == "/v1/me/consent" {
        Operation::Contribution
    } else if path.starts_with("/v1/receipts/") {
        Operation::Outcome
    } else if path == "/v1/me/profile"
        || path.starts_with("/v1/profiles/")
        || path == "/v1/me/account"
    {
        Operation::Profile
    } else if path == "/v1/me/dashboard" || path == "/v1/leaderboard" {
        Operation::Dashboard
    } else if path == "/v1/me/usage"
        || path.starts_with("/v1/me/memories/")
        || path == "/v1/me/score-events"
    {
        Operation::Usage
    } else if path.starts_with("/v1/telemetry/") {
        Operation::Telemetry
    } else {
        Operation::Unknown
    }
}

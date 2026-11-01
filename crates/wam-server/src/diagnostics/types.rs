use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

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
    #[serde(default)]
    pub received_at_micros: u64,
}
impl StoredDiagnostic {
    pub fn valid(&self) -> bool {
        valid_id(&self.event_id)
            && valid_id(&self.trace_id)
            && matches!(
                self.kind.as_str(),
                "request" | "client_error" | "server_error" | "email_checkpoint"
            )
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
            && self.received_at_micros <= i64::MAX as u64
            && self.status_code.is_none_or(|c| (100..=599).contains(&c))
    }

    pub(crate) fn ordering_timestamp_micros(&self) -> u64 {
        event_timestamp_micros(self.received_at, self.received_at_micros)
    }

    pub fn from_event(event: DiagnosticEvent) -> Self {
        let received_at_micros = now_micros();
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
            received_at: received_at_micros / 1_000_000,
            received_at_micros,
        }
    }

    pub(crate) fn server_error(error_code: ErrorCode) -> Self {
        let received_at_micros = now_micros();
        Self {
            event_id: random_id(),
            trace_id: random_id(),
            kind: "server_error".into(),
            source: Source::Server,
            operation: Operation::Unknown,
            error_code: Some(error_code),
            method: "OTHER".into(),
            status_code: Some(500),
            duration_ms: 0,
            received_at: received_at_micros / 1_000_000,
            received_at_micros,
        }
    }

    pub(crate) fn email_checkpoint(event: &Self) -> Self {
        Self::email_checkpoint_at(event.ordering_timestamp_micros(), &event.event_id)
    }

    pub(crate) fn email_checkpoint_at(cursor_micros: u64, event_id: &str) -> Self {
        Self {
            event_id: random_id(),
            trace_id: event_id.to_owned(),
            kind: "email_checkpoint".into(),
            source: Source::Server,
            operation: Operation::Telemetry,
            error_code: None,
            method: "CLIENT".into(),
            status_code: None,
            duration_ms: 0,
            received_at: now(),
            received_at_micros: cursor_micros,
        }
    }
}

fn event_timestamp_micros(received_at: u64, received_at_micros: u64) -> u64 {
    if received_at_micros == 0 {
        received_at.saturating_mul(1_000_000)
    } else {
        received_at_micros
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

pub(crate) fn now_micros() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .min(u64::MAX as u128) as u64;
    let mut previous = LAST.load(Ordering::Relaxed);
    loop {
        let next = wall.max(previous.saturating_add(1));
        match LAST.compare_exchange_weak(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => previous = observed,
        }
    }
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
    } else if path.starts_with("/v1/auth/") || path.starts_with("/v1/admin/auth/") {
        Operation::Auth
    } else if path.starts_with("/v1/link/") || path.starts_with("/v1/me/devices") {
        Operation::Device
    } else if path.starts_with("/v1/review/")
        || path == "/v1/admin/contributions"
        || path.starts_with("/v1/admin/contributions/")
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_routes_have_the_same_diagnostic_categories_as_public_auth_and_review() {
        for path in [
            "/v1/admin/auth/start",
            "/v1/admin/auth/verify",
            "/v1/admin/auth/session",
            "/v1/admin/auth/logout",
        ] {
            assert_eq!(operation(path), Operation::Auth, "{path}");
        }
        for path in [
            "/v1/admin/contributions",
            "/v1/admin/contributions/contribution-1",
            "/v1/admin/contributions/contribution-1/claim",
            "/v1/admin/contributions/contribution-1/decision",
        ] {
            assert_eq!(operation(path), Operation::Review, "{path}");
        }
        assert_eq!(
            operation("/v1/admin/contributions-other"),
            Operation::Unknown
        );
        assert_eq!(operation("/v1/admin/auth-other"), Operation::Unknown);
    }

    #[test]
    fn stored_records_without_precise_timestamps_remain_readable() {
        let event = StoredDiagnostic::server_error(ErrorCode::Panic);
        let mut legacy = serde_json::to_value(event).unwrap();
        legacy.as_object_mut().unwrap().remove("received_at_micros");

        let restored: StoredDiagnostic = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.received_at_micros, 0);
        assert_eq!(
            restored.ordering_timestamp_micros(),
            restored.received_at * 1_000_000
        );
    }
}

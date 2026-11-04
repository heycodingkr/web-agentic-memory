//! Closed, payload-free diagnostics shared by the CLI and MCP adapters.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSource {
    Cli,
    Mcp,
}

impl DiagnosticSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticOperation {
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

impl DiagnosticOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::Expand => "expand",
            Self::Vocab => "vocab",
            Self::Catalog => "catalog",
            Self::Profile => "profile",
            Self::Dashboard => "dashboard",
            Self::Contribution => "contribution",
            Self::Outcome => "outcome",
            Self::Auth => "auth",
            Self::Device => "device",
            Self::Review => "review",
            Self::Usage => "usage",
            Self::Telemetry => "telemetry",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticError {
    NetworkError,
    HttpError,
    InvalidResponse,
    InvalidInput,
    UnhandledError,
    UnhandledRejection,
    Panic,
    InternalError,
}

impl DiagnosticError {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NetworkError => "network_error",
            Self::HttpError => "http_error",
            Self::InvalidResponse => "invalid_response",
            Self::InvalidInput => "invalid_input",
            Self::UnhandledError => "unhandled_error",
            Self::UnhandledRejection => "unhandled_rejection",
            Self::Panic => "panic",
            Self::InternalError => "internal_error",
        }
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnosticEvent {
    schema_version: u8,
    event_id: String,
    trace_id: String,
    source: String,
    operation: String,
    error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
    duration_ms: u64,
}

impl DiagnosticEvent {
    pub(crate) fn new(
        source: DiagnosticSource,
        operation: DiagnosticOperation,
        error: DiagnosticError,
        trace_id: String,
        status_code: Option<u16>,
        duration: Duration,
    ) -> Option<Self> {
        let status_code = status_code.filter(|status| (100..=599).contains(status));
        Some(Self {
            schema_version: 1,
            event_id: random_id()?,
            trace_id,
            source: source.as_str().to_owned(),
            operation: operation.as_str().to_owned(),
            error_code: error.as_str().to_owned(),
            status_code,
            duration_ms: duration.as_millis().min(600_000) as u64,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.event_id
    }
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub(crate) fn valid(&self) -> bool {
        fn hex32(value: &str) -> bool {
            value.len() == 32
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        }
        self.schema_version == 1
            && hex32(&self.event_id)
            && hex32(&self.trace_id)
            && matches!(self.source.as_str(), "cli" | "mcp")
            && matches!(
                self.operation.as_str(),
                "recall"
                    | "expand"
                    | "vocab"
                    | "catalog"
                    | "profile"
                    | "dashboard"
                    | "contribution"
                    | "outcome"
                    | "auth"
                    | "device"
                    | "review"
                    | "usage"
                    | "telemetry"
                    | "unknown"
            )
            && matches!(
                self.error_code.as_str(),
                "network_error"
                    | "http_error"
                    | "invalid_response"
                    | "invalid_input"
                    | "unhandled_error"
                    | "unhandled_rejection"
                    | "panic"
                    | "internal_error"
            )
            && self
                .status_code
                .is_none_or(|status| (100..=599).contains(&status))
            && self.duration_ms <= 600_000
    }
}

pub(crate) fn random_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Classify only fixed route templates. Dynamic path values are never copied.
pub(crate) fn classify_route(path: &str) -> DiagnosticOperation {
    let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    let route = parts
        .iter()
        .position(|part| *part == "v1")
        .map(|i| &parts[i..]);
    match route {
        Some(["v1", "memories", "recall"]) => DiagnosticOperation::Recall,
        Some(["v1", "memories", "vocab"]) => DiagnosticOperation::Vocab,
        Some(["v1", "memories", _]) => DiagnosticOperation::Expand,
        Some(["v1", "leaderboard"] | ["v1", "memories"]) => DiagnosticOperation::Catalog,
        Some(["v1", "me", "dashboard"]) => DiagnosticOperation::Dashboard,
        Some(["v1", "me", "usage"]) => DiagnosticOperation::Usage,
        Some(["v1", "profiles", _] | ["v1", "me", "profile"]) => DiagnosticOperation::Profile,
        Some(["v1", "me", "contributions"] | ["v1", "me", "contributions", _]) => {
            DiagnosticOperation::Contribution
        }
        Some(["v1", "receipts", _, "outcome"]) => DiagnosticOperation::Outcome,
        Some(["v1", "telemetry", "events"]) => DiagnosticOperation::Telemetry,
        Some(["v1", "auth", ..]) => DiagnosticOperation::Auth,
        Some(["v1", "device" | "link", ..] | ["v1", "me", "devices", ..]) => {
            DiagnosticOperation::Device
        }
        Some(["v1", "review", ..]) => DiagnosticOperation::Review,
        _ => DiagnosticOperation::Unknown,
    }
}

pub(crate) fn enabled() -> bool {
    enabled_value(std::env::var("WAM_TELEMETRY").ok().as_deref())
}

fn enabled_value(value: Option<&str>) -> bool {
    !value.is_some_and(|value| value.eq_ignore_ascii_case("off"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_contains_only_closed_fields_and_no_path_values() {
        let event = DiagnosticEvent::new(
            DiagnosticSource::Cli,
            classify_route("/v1/memories/private-sentinel?token=secret"),
            DiagnosticError::HttpError,
            "0".repeat(32),
            Some(503),
            Duration::from_secs(9999),
        )
        .unwrap();
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("private-sentinel"));
        assert!(!json.contains("secret"));
        assert_eq!(event.operation, "expand");
        assert_eq!(event.duration_ms, 600_000);
        assert_eq!(event.event_id.len(), 32);
    }

    #[test]
    fn actual_device_link_routes_have_a_closed_operation() {
        for path in [
            "/v1/link/start",
            "/v1/link/poll",
            "/v1/link/status",
            "/v1/link/revoke",
            "/v1/me/devices",
            "/v1/me/devices/private-device/revoke",
        ] {
            assert_eq!(classify_route(path), DiagnosticOperation::Device, "{path}");
        }
        assert_eq!(classify_route("/v1/memories"), DiagnosticOperation::Catalog);
    }

    #[test]
    fn explicit_off_disables_diagnostics_independently_of_contribution_consent() {
        assert!(!enabled_value(Some("off")));
        assert!(!enabled_value(Some("OFF")));
        assert!(enabled_value(None));
    }
}

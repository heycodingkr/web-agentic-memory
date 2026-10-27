//! Tool error codes (docs/spec/curator.md "서버가 강제하는 불변조건"). The model sees only the code and a message.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    ScopeViolation,
    DuplicateExists,
    VersionConflict,
    EvidenceInsufficient,
    SchemaInvalid,
    TextRejected,
    BudgetExceeded,
    AlreadyDecided,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 8] = [
        Self::ScopeViolation,
        Self::DuplicateExists,
        Self::VersionConflict,
        Self::EvidenceInsufficient,
        Self::SchemaInvalid,
        Self::TextRejected,
        Self::BudgetExceeded,
        Self::AlreadyDecided,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScopeViolation => "SCOPE_VIOLATION",
            Self::DuplicateExists => "DUPLICATE_EXISTS",
            Self::VersionConflict => "VERSION_CONFLICT",
            Self::EvidenceInsufficient => "EVIDENCE_INSUFFICIENT",
            Self::SchemaInvalid => "SCHEMA_INVALID",
            Self::TextRejected => "TEXT_REJECTED",
            Self::BudgetExceeded => "BUDGET_EXCEEDED",
            Self::AlreadyDecided => "ALREADY_DECIDED",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ToolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The JSON the model receives instead of a result.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({"error": {"code": self.code.as_str(), "message": self.message}})
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ToolError {}

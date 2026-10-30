//! Stable errors shared by the CLI and MCP adapters.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiError {
    InvalidEndpoint,
    InvalidInput,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Transport,
    Server,
    InvalidResponse,
    RateLimited { retry_after_seconds: Option<u64> },
    CacheUnavailable,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidEndpoint => "invalid API endpoint",
            Self::InvalidInput => "request was rejected",
            Self::Unauthorized => "authentication required",
            Self::Forbidden => "permission denied",
            Self::NotFound => "resource not found",
            Self::Conflict => "resource conflict",
            Self::Transport => "API is unreachable",
            Self::Server => "API returned an error",
            Self::InvalidResponse => "API response did not match the contract",
            Self::RateLimited {
                retry_after_seconds: Some(seconds),
            } => {
                return write!(f, "rate limit reached; retry in {seconds} seconds");
            }
            Self::RateLimited {
                retry_after_seconds: None,
            } => "rate limit reached; wait before retrying",
            Self::CacheUnavailable => "local cache is unavailable",
        })
    }
}

impl std::error::Error for ApiError {}

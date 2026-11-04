//! Stable, low-detail JSON failures. Never include raw CLI arguments or request bodies.
use serde_json::{Value, json};
use wam_client::ApiError;

pub fn failure(code: &str, message: &str, trace: Option<&str>) -> Value {
    let mut value = json!({"schema_version":1,"error":{"code":code,"message":message}});
    if let Some(trace) = trace {
        value["error"]["trace_id"] = json!(trace);
    }
    value
}
pub fn api_failure(error: ApiError, trace: Option<&str>) -> Value {
    let code = match error {
        ApiError::InvalidEndpoint => "invalid_endpoint",
        ApiError::InvalidInput => "invalid_input",
        ApiError::Unauthorized => "unauthorized",
        ApiError::Forbidden => "forbidden",
        ApiError::NotFound => "not_found",
        ApiError::Conflict => "conflict",
        ApiError::Transport => "transport",
        ApiError::Server => "server_error",
        ApiError::InvalidResponse => "invalid_response",
        ApiError::RateLimited { .. } => "rate_limited",
        ApiError::CacheUnavailable => "cache_unavailable",
    };
    let mut value = failure(code, &error.to_string(), trace);
    if let ApiError::RateLimited {
        retry_after_seconds: Some(seconds),
    } = error
    {
        value["error"]["retry_after_seconds"] = json!(seconds);
    }
    value
}

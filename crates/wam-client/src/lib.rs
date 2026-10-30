#![forbid(unsafe_code)]

//! HTTP transport for command-line and MCP callers. It knows wire contracts,
//! not CLI flags, MCP protocol details, browser state, or core ranking rules.

mod api;
mod cache;
pub mod credentials;
pub mod device;
mod diagnostics;
mod endpoint;
mod error;
mod http;
mod recall;
mod telemetry;
mod telemetry_spool;
mod transport;

pub use api::WamApi;
pub use cache::LocalCacheStatus;
pub use endpoint::normalize_endpoint;
pub use error::ApiError;
pub use telemetry::{DiagnosticError, DiagnosticOperation, DiagnosticSource};

use cache::LocalCache;
use endpoint::route_url;
use reqwest::Client;
use std::sync::{Arc, Mutex};
use telemetry_spool::TelemetrySpool;

#[derive(Clone)]
pub struct HttpClient {
    endpoint: String,
    token: Option<String>,
    client: Client,
    cache: Option<Arc<LocalCache>>,
    cache_origin_hash: String,
    diagnostic_source: DiagnosticSource,
    last_failure_trace_id: Arc<Mutex<Option<String>>>,
    telemetry_spool: Option<TelemetrySpool>,
}

#[cfg(test)]
mod tests;

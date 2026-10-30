//! Bounded diagnostic delivery and persisted retry configuration.

use crate::endpoint::route_url;
use crate::telemetry::{self, DiagnosticEvent, random_id};
use crate::telemetry_spool::TelemetrySpool;
use crate::{DiagnosticError, DiagnosticOperation, DiagnosticSource, HttpClient};
use std::{
    path::Path,
    time::{Duration, Instant},
};

impl HttpClient {
    /// Override the private telemetry queue location, primarily for embedding and tests.
    pub fn set_telemetry_spool_dir(&mut self, root: impl AsRef<Path>) {
        self.telemetry_spool = Some(TelemetrySpool::new(root.as_ref(), &self.cache_origin_hash));
    }

    pub fn disable_telemetry_spool(&mut self) {
        self.telemetry_spool = None;
    }

    pub fn set_diagnostic_source(&mut self, source: DiagnosticSource) {
        self.diagnostic_source = source;
    }

    pub fn last_trace_id(&self) -> Option<String> {
        self.last_failure_trace_id.lock().ok()?.clone()
    }

    pub(crate) async fn report_error(
        &self,
        operation: DiagnosticOperation,
        error: DiagnosticError,
        trace_id: String,
        status_code: Option<u16>,
        started: Instant,
    ) {
        if operation == DiagnosticOperation::Telemetry || !telemetry::enabled() {
            return;
        }
        if let Ok(mut last) = self.last_failure_trace_id.lock() {
            *last = Some(trace_id.clone());
        }
        let Some(event) = DiagnosticEvent::new(
            self.diagnostic_source,
            operation,
            error,
            trace_id.clone(),
            status_code,
            started.elapsed(),
        ) else {
            return;
        };
        let Ok(url) = route_url(&self.endpoint, &["v1", "telemetry", "events"]) else {
            return;
        };
        let mut request = self
            .client
            .post(url)
            .json(&event)
            .header("x-wam-client", self.diagnostic_source.as_str())
            .header("x-wam-trace-id", &trace_id)
            .timeout(Duration::from_millis(800));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let delivered = request
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if !delivered {
            if let Some(spool) = &self.telemetry_spool {
                let _ = spool.enqueue(&event);
            }
        }
    }

    pub(crate) async fn drain_telemetry(&self) {
        if !telemetry::enabled() {
            return;
        }
        let Some(spool) = &self.telemetry_spool else {
            return;
        };
        let Ok(events) = spool.pending(10) else {
            return;
        };
        if events.is_empty() {
            return;
        }
        let Ok(url) = route_url(&self.endpoint, &["v1", "telemetry", "events"]) else {
            return;
        };
        let deadline = Instant::now() + Duration::from_millis(800);
        for event in events {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let mut request = self
                .client
                .post(url.clone())
                .json(&event)
                .header("x-wam-client", event.source())
                .header("x-wam-trace-id", event.trace_id())
                .timeout(remaining);
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    let _ = spool.delivered(&event);
                }
                _ => break,
            }
        }
    }

    pub async fn report_diagnostic_event(
        &self,
        operation: DiagnosticOperation,
        error: DiagnosticError,
    ) -> Option<String> {
        if operation == DiagnosticOperation::Telemetry || !telemetry::enabled() {
            return None;
        }
        let trace_id = random_id()?;
        self.report_error(operation, error, trace_id.clone(), None, Instant::now())
            .await;
        Some(trace_id)
    }
}

#[cfg(test)]
pub(crate) fn default_telemetry_spool_root() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(test))]
pub(crate) fn default_telemetry_spool_root() -> Option<std::path::PathBuf> {
    if let Some(root) = std::env::var_os("WAM_TELEMETRY_SPOOL_DIR") {
        let root = std::path::PathBuf::from(root);
        return (!root.as_os_str().is_empty()).then_some(root);
    }
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join("Library/Caches"))
        })?;
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join("AppData/Local"))
        })?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })?;
    Some(base.join("wam").join("telemetry"))
}

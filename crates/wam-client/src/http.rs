//! HTTP client construction, credentials, and local cache configuration.

use crate::cache::{self, LocalCache, LocalCacheError, default_cache_path};
use crate::diagnostics::default_telemetry_spool_root;
use crate::endpoint::normalize_endpoint;
use crate::recall::sha256_hex;
use crate::telemetry_spool::TelemetrySpool;
use crate::{ApiError, DiagnosticSource, HttpClient};
use reqwest::Client;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

impl HttpClient {
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Result<Self, ApiError> {
        let endpoint = normalize_endpoint(&endpoint.into())?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("wam-client/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ApiError::Transport)?;
        let cache_origin_hash = sha256_hex(endpoint.as_bytes());
        Ok(Self {
            cache_origin_hash: cache_origin_hash.clone(),
            endpoint,
            token,
            client,
            cache: None,
            diagnostic_source: DiagnosticSource::Cli,
            last_failure_trace_id: Arc::new(Mutex::new(None)),
            telemetry_spool: default_telemetry_spool_root()
                .map(|root| TelemetrySpool::new(&root, &cache_origin_hash)),
        })
    }

    /// Share transport/cache resources while isolating diagnostics for one concurrent request.
    pub fn for_request(&self) -> Self {
        let mut client = self.clone();
        client.last_failure_trace_id = Arc::new(Mutex::new(None));
        client
    }

    pub fn set_access_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    pub(crate) fn require_auth(&self) -> Result<(), ApiError> {
        if self.token.as_ref().is_some_and(|token| !token.is_empty()) {
            Ok(())
        } else {
            Err(ApiError::Unauthorized)
        }
    }

    /// Enable the process-shared SQLite cache. Cache failures never affect network requests.
    pub fn enable_local_cache(&mut self) -> Result<(), LocalCacheError> {
        self.enable_local_cache_at(default_cache_path()?)
    }

    pub fn enable_local_cache_at(&mut self, path: impl AsRef<Path>) -> Result<(), LocalCacheError> {
        self.cache = Some(Arc::new(LocalCache::open(path)?));
        Ok(())
    }

    pub(crate) fn local_cache_status(&self) -> Result<cache::LocalCacheStatus, ApiError> {
        let cache = self.cache.as_ref().ok_or(ApiError::CacheUnavailable)?;
        cache.status().map_err(|_| ApiError::CacheUnavailable)
    }

    pub(crate) fn clear_local_cache(&self) -> Result<(), ApiError> {
        self.cache
            .as_ref()
            .ok_or(ApiError::CacheUnavailable)?
            .clear()
            .map_err(|_| ApiError::CacheUnavailable)
    }
}

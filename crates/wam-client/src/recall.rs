//! Anonymous recall caching and network fallback policy.

use crate::cache::{CacheFreshness, CachedValue};
use crate::endpoint::canonical_public_url;
use crate::transport::{error_for_response, response_trace_id};
use crate::{ApiError, DiagnosticError, HttpClient};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};
use wam_contracts::{RecallCacheStatus, RecallRequest, RecallResponse, SCHEMA_VERSION};

impl HttpClient {
    pub(crate) async fn recall_with_cache(
        &self,
        mut request: RecallRequest,
    ) -> Result<RecallResponse, ApiError> {
        if request.url.len() > 2048
            || request.keywords.len() > 8
            || !matches!(request.limit, None | Some(1..=3))
            || request
                .kind
                .as_deref()
                .is_some_and(|kind| !matches!(kind, "workflow" | "site-note" | "gotcha"))
        {
            return Err(ApiError::InvalidInput);
        }
        request.url = canonical_public_url(&request.url)?;
        // Resolve only public vocabulary before either the cache key or HTTP body is made.
        // Vocabulary remains no-store: filtered requests need a reachable vocabulary endpoint.
        if !request.keywords.is_empty() || request.task_category.is_some() {
            use crate::WamApi;
            let vocabulary = self.vocab().await?;
            if let Some(task) = request.task_category.as_deref() {
                let tasks =
                    wam_core::TaskVocabulary::new(&vocabulary.version, &vocabulary.task_categories)
                        .map_err(|_| ApiError::InvalidResponse)?;
                tasks.resolve(task).map_err(|_| ApiError::InvalidInput)?;
            }
            if !request.keywords.is_empty() {
                let keywords =
                    wam_core::KeywordVocabulary::new(&vocabulary.version, &vocabulary.keywords)
                        .map_err(|_| ApiError::InvalidResponse)?;
                request.keywords = keywords
                    .filter(&request.keywords)
                    .map_err(|_| ApiError::InvalidInput)?
                    .into_iter()
                    .map(|keyword| keyword.as_str().to_owned())
                    .collect();
            }
        }
        let cache_enabled = self.token.is_none() && self.cache.is_some();
        let cache_key = cache_enabled
            .then(|| self.recall_cache_key(&request))
            .flatten();
        let stale = if let Some(key) = cache_key.as_deref() {
            match self.cache_get(key).await {
                Some(cached) if cached.freshness == CacheFreshness::Fresh => {
                    if let Ok(mut response) =
                        serde_json::from_slice::<RecallResponse>(&cached.payload)
                    {
                        response.cache_status = Some(RecallCacheStatus::Fresh);
                        return Ok(response);
                    }
                    self.cache_delete(key).await;
                    None
                }
                Some(cached) => Some(cached),
                None => None,
            }
        } else {
            None
        };
        let builder = self
            .client
            .post(format!("{}/v1/memories/recall", self.endpoint))
            .json(&request);
        let (request, operation, trace_id) = self.prepare_request(builder)?;
        let started = Instant::now();
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(_) => {
                self.report_error(
                    operation,
                    DiagnosticError::NetworkError,
                    trace_id,
                    None,
                    started,
                )
                .await;
                return self.stale_recall(stale).ok_or(ApiError::Transport);
            }
        };
        let trace_id = response_trace_id(&response, trace_id);
        if !response.status().is_success() {
            self.report_error(
                operation,
                DiagnosticError::HttpError,
                trace_id,
                Some(response.status().as_u16()),
                started,
            )
            .await;
            return Err(error_for_response(&response));
        }
        let status_code = response.status().as_u16();
        let cache_policy = response
            .headers()
            .get("x-wam-client-cache")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_cache_policy);
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => {
                self.report_error(
                    operation,
                    DiagnosticError::NetworkError,
                    trace_id,
                    None,
                    started,
                )
                .await;
                return self.stale_recall(stale).ok_or(ApiError::Transport);
            }
        };
        let mut result: RecallResponse = match serde_json::from_slice(&bytes) {
            Ok(result) => result,
            Err(_) => {
                self.report_error(
                    operation,
                    DiagnosticError::InvalidResponse,
                    trace_id,
                    Some(status_code),
                    started,
                )
                .await;
                return Err(ApiError::InvalidResponse);
            }
        };
        if cache_enabled {
            if let (Some(key), Some((fresh_for, stale_for))) = (cache_key, cache_policy) {
                if result.receipt_id.is_none() {
                    self.cache_put(&key, &bytes, fresh_for, stale_for).await;
                    result.cache_status = Some(RecallCacheStatus::Network);
                }
            }
        }
        self.drain_telemetry().await;
        Ok(result)
    }
}

impl HttpClient {
    fn recall_cache_key(&self, request: &RecallRequest) -> Option<String> {
        let material = serde_json::to_vec(&(SCHEMA_VERSION, &self.endpoint, request)).ok()?;
        self.cache
            .as_ref()
            .map(|cache| cache.keyed_digest(&material))
    }

    async fn cache_get(&self, key: &str) -> Option<CachedValue> {
        let cache = Arc::clone(self.cache.as_ref()?);
        let key = key.to_owned();
        let origin = self.cache_origin_hash.clone();
        let now = unix_time_secs();
        tokio::task::spawn_blocking(move || cache.get(&key, &origin, SCHEMA_VERSION, now))
            .await
            .ok()?
            .ok()?
    }

    async fn cache_delete(&self, key: &str) {
        let Some(cache) = self.cache.as_ref().map(Arc::clone) else {
            return;
        };
        let key = key.to_owned();
        let _ = tokio::task::spawn_blocking(move || cache.delete(&key)).await;
    }

    async fn cache_put(&self, key: &str, payload: &[u8], fresh_for: u64, stale_for: u64) {
        let Some(cache) = self.cache.as_ref().map(Arc::clone) else {
            return;
        };
        let key = key.to_owned();
        let origin = self.cache_origin_hash.clone();
        let payload = payload.to_vec();
        let now = unix_time_secs();
        let _ = tokio::task::spawn_blocking(move || {
            cache.put(
                &key,
                &origin,
                SCHEMA_VERSION,
                now,
                fresh_for,
                stale_for,
                &payload,
            )
        })
        .await;
    }

    fn stale_recall(&self, cached: Option<CachedValue>) -> Option<RecallResponse> {
        let cached = cached?;
        let mut response = serde_json::from_slice::<RecallResponse>(&cached.payload).ok()?;
        if cached.freshness != CacheFreshness::Stale || response.receipt_id.is_some() {
            return None;
        }
        response.cache_status = Some(RecallCacheStatus::Stale);
        Some(response)
    }
}

fn parse_cache_policy(value: &str) -> Option<(u64, u64)> {
    let mut parts = value.split(';').map(str::trim);
    if parts.next()? != "anonymous-recall" {
        return None;
    }
    let mut fresh_for = None;
    let mut stale_for = None;
    for part in parts {
        if let Some(value) = part.strip_prefix("max-age=") {
            fresh_for = value.parse::<u64>().ok();
        } else if let Some(value) = part.strip_prefix("stale-if-error=") {
            stale_for = value.parse::<u64>().ok();
        }
    }
    let fresh_for = fresh_for?.min(300);
    let stale_for = stale_for?.min(900);
    (fresh_for > 0 && stale_for > 0).then_some((fresh_for, stale_for))
}

pub(crate) fn sha256_hex(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn unix_time_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

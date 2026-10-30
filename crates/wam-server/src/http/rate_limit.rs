use super::{AppState, Authenticator};
use crate::{
    ports::Repository,
    rate_limit::{
        IpNetwork, UsageRecord, account_bucket, account_usage_key, client_bucket,
        status_account_bucket, utc_dates,
    },
};
use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) async fn run_blocking_ports(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || runtime.block_on(next.run(request)))
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(super) async fn enforce_load_shed<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if rate_class(request.method(), request.uri().path()).is_none() {
        return next.run(request).await;
    }
    let Some(_permit) = state.load_shed.try_acquire() else {
        return service_busy();
    };
    next.run(request).await
}

#[derive(Clone, Copy)]
enum RateClass {
    Status,
    Recall,
    Detail,
    Write,
    General,
}

fn rate_class(method: &Method, path: &str) -> Option<RateClass> {
    if *method == Method::OPTIONS {
        return None;
    }
    if path == "/healthz"
        || !path.starts_with("/v1/") && !path.starts_with("/mcp") && !path.starts_with("/oauth/")
    {
        return None;
    }
    if path == "/v1/me/usage" || path == "/v1/mcp/status" {
        return Some(RateClass::Status);
    }
    if *method == Method::GET && path == "/mcp" {
        return Some(RateClass::General);
    }
    if path == "/v1/memories/recall" {
        return Some(RateClass::Recall);
    }
    if path.starts_with("/v1/memories/") && path != "/v1/memories/vocab" {
        return Some(RateClass::Detail);
    }
    if *method != Method::GET || path.starts_with("/mcp") || path.starts_with("/oauth/") {
        return Some(RateClass::Write);
    }
    Some(RateClass::General)
}

fn mcp_rate_class(message: &serde_json::Value) -> RateClass {
    if !wam_mcp::valid_request_shape(message) {
        return RateClass::Write;
    }
    match message["method"].as_str() {
        Some(
            "initialize"
            | "ping"
            | "tools/list"
            | "notifications/initialized"
            | "notifications/cancelled",
        ) => RateClass::General,
        Some("tools/call") => match message["params"]["name"].as_str() {
            Some("web_memory_status" | "web_memory_my_usage") => RateClass::Status,
            Some("web_memory_recall") => RateClass::Recall,
            Some("web_memory_expand") => RateClass::Detail,
            Some(
                "web_memory_leaderboard" | "web_memory_my_rank" | "web_memory_my_contributions",
            ) => RateClass::General,
            _ => RateClass::Write,
        },
        _ => RateClass::Write,
    }
}

async fn classify_mcp_request(
    request: Request<Body>,
    fallback: RateClass,
) -> (Request<Body>, RateClass) {
    if request.method() != Method::POST || request.uri().path() != "/mcp" {
        return (request, fallback);
    }
    let (parts, body) = request.into_parts();
    // Inspect at most the transport's existing body limit. Replay the same bytes
    // (or read failure) so transport validation keeps its original error order.
    let (body, class) = match to_bytes(body, crate::mcp_transport::MAX_REQUEST_BYTES).await {
        Ok(bytes) => {
            let class = serde_json::from_slice(&bytes)
                .map(|message| mcp_rate_class(&message))
                .unwrap_or(RateClass::Write);
            (Body::from(bytes), class)
        }
        Err(error) => {
            let failure = std::future::ready(Err::<Bytes, _>(error));
            (
                Body::from_stream(futures_util::stream::once(failure)),
                RateClass::Write,
            )
        }
    };
    (Request::from_parts(parts, body), class)
}

pub(super) async fn enforce_rate_limit<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(class) = rate_class(request.method(), request.uri().path()) else {
        return next.run(request).await;
    };
    let account = state
        .auth
        .has_credential(request.headers())
        .then(|| state.auth.account_id(request.headers()))
        .flatten();
    let reviewer = state.reviewer.reviewer_id(request.headers());
    let invalid_credential =
        state.auth.has_credential(request.headers()) && account.is_none() && reviewer.is_none();
    let ip = client_ip(&request, &state.trusted_proxy_ranges);
    let (request, class) = if invalid_credential {
        (request, class)
    } else {
        classify_mcp_request(request, class).await
    };
    let cost = if invalid_credential {
        // Rejecting a stale or malformed token is a cheap auth path. Keep it
        // bounded by the anonymous IP bucket without charging the write cost.
        1
    } else {
        match class {
            RateClass::Status | RateClass::General => 1,
            RateClass::Recall => 3,
            RateClass::Detail => 2,
            RateClass::Write => 5,
        }
    };
    let buckets = if matches!(class, RateClass::Status) {
        let mut result = vec![client_bucket(
            &state.rate_limit_key_secret,
            &ip,
            "status",
            1,
        )];
        if let Some(account) = account.as_deref() {
            result.push(status_account_bucket(&state.rate_limit_key_secret, account));
        }
        result
    } else if let Some(account) = account.as_deref() {
        let mut account_limit = account_bucket(&state.rate_limit_key_secret, account);
        account_limit.cost = cost;
        vec![
            client_bucket(&state.rate_limit_key_secret, &ip, "shared", cost),
            account_limit,
        ]
    } else if let Some(reviewer) = reviewer.as_deref() {
        let mut reviewer_limit = account_bucket(
            &state.rate_limit_key_secret,
            &format!("reviewer:{reviewer}"),
        );
        reviewer_limit.cost = cost;
        vec![
            client_bucket(&state.rate_limit_key_secret, &ip, "shared", cost),
            reviewer_limit,
        ]
    } else {
        vec![
            client_bucket(&state.rate_limit_key_secret, &ip, "anonymous", cost),
            client_bucket(&state.rate_limit_key_secret, &ip, "shared", cost),
        ]
    };
    if buckets.is_empty() {
        return next.run(request).await;
    }

    let now_ms = unix_time_millis();
    let request_class = match class {
        RateClass::Recall => Some("recall"),
        RateClass::Detail => Some("detail"),
        RateClass::Status => None,
        RateClass::General | RateClass::Write => Some("other"),
    };
    let usage = account
        .as_deref()
        .zip(request_class)
        .map(|(account, class)| UsageRecord {
            key: account_usage_key(
                &state.rate_limit_key_secret,
                account,
                &utc_dates(now_ms / 1000, 1)[0],
            ),
            units: cost,
            request_class: class,
        });
    let statuses = match state
        .rate_limits
        .consume(&buckets, usage.as_ref(), now_ms)
        .await
    {
        Ok(statuses) => statuses,
        Err(_) => {
            eprintln!("rate limit store unavailable; request rejected");
            return rate_limit_unavailable();
        }
    };
    let limit = statuses
        .iter()
        .map(|status| status.limit_per_minute)
        .min()
        .unwrap_or(1);
    let remaining = statuses
        .iter()
        .map(|status| status.remaining_units)
        .min()
        .unwrap_or_default();
    let reset = statuses
        .iter()
        .map(|status| status.resets_in_seconds)
        .min()
        .unwrap_or_default();
    if statuses.iter().any(|status| !status.allowed) {
        let retry = statuses
            .iter()
            .map(|status| status.retry_after_seconds)
            .max()
            .unwrap_or(1)
            .max(1);
        return rate_limited(limit, remaining, retry);
    }

    let mut response = next.run(request).await;
    add_rate_limit_headers(response.headers_mut(), limit, remaining, reset);
    response
}

fn client_ip(request: &Request<axum::body::Body>, trusted_proxy_ranges: &[IpNetwork]) -> String {
    let Some(peer) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip())
    else {
        return "unknown".to_owned();
    };
    if !trusted_proxy_ranges
        .iter()
        .any(|network| network.contains(peer))
    {
        return crate::rate_limit::normalize_client_ip(peer);
    }
    let Some(forwarded) = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    else {
        return crate::rate_limit::normalize_client_ip(peer);
    };
    let chain: Vec<IpAddr> = forwarded
        .split(',')
        .filter_map(|value| IpAddr::from_str(value.trim()).ok())
        .collect();
    let mut client = peer;
    for hop in chain.iter().rev() {
        if !trusted_proxy_ranges
            .iter()
            .any(|network| network.contains(client))
        {
            break;
        }
        client = *hop;
    }
    crate::rate_limit::normalize_client_ip(client)
}

pub(super) fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u128::from(u64::MAX)) as u64
        })
}

fn add_rate_limit_headers(headers: &mut HeaderMap, limit: u32, remaining: u32, reset: u64) {
    if let Ok(value) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert("ratelimit-limit", value);
    }
    if let Ok(value) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert("ratelimit-remaining", value);
    }
    if let Ok(value) = HeaderValue::from_str(&reset.to_string()) {
        headers.insert("ratelimit-reset", value);
    }
    headers.insert(
        "access-control-expose-headers",
        HeaderValue::from_static("RateLimit-Limit, RateLimit-Remaining, RateLimit-Reset, Retry-After, X-WAM-Client-Cache"),
    );
}

fn rate_limited(limit: u32, remaining: u32, retry_after: u64) -> Response {
    let body = Json(serde_json::json!({
        "error": "rate_limited",
        "retry_after_seconds": retry_after
    }));
    let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
    add_rate_limit_headers(response.headers_mut(), limit, remaining, retry_after);
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        response.headers_mut().insert("retry-after", value);
    }
    super::no_store(&mut response);
    response
}

fn rate_limit_unavailable() -> Response {
    let mut response = (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "rate_limit_unavailable",
            "retry_after_seconds": 1
        })),
    )
        .into_response();
    response
        .headers_mut()
        .insert("retry-after", HeaderValue::from_static("1"));
    response.headers_mut().insert(
        "access-control-expose-headers",
        HeaderValue::from_static("RateLimit-Limit, RateLimit-Remaining, RateLimit-Reset, Retry-After, X-WAM-Client-Cache"),
    );
    super::no_store(&mut response);
    response
}

fn service_busy() -> Response {
    let mut response = (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "error": "service_busy", "retry_after_seconds": 1 })),
    )
        .into_response();
    response.headers_mut().insert(
        "access-control-expose-headers",
        HeaderValue::from_static("Retry-After"),
    );
    response
        .headers_mut()
        .insert("retry-after", HeaderValue::from_static("1"));
    super::no_store(&mut response);
    response
}

#[cfg(test)]
mod tests {
    use super::super::{AppState, DemoTokenAuthenticator, NoReviewerAuthenticator};
    use super::{enforce_load_shed, rate_class, service_busy};
    use crate::{
        InMemoryRepository,
        application::WamService,
        rate_limit::{InMemoryRateLimitStore, LoadShedGate},
    };
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode, header},
        middleware,
        routing::get,
    };
    use std::sync::Arc;
    use tokio::sync::Notify;
    use tower::ServiceExt;

    #[test]
    fn load_shed_response_is_retryable_and_health_check_stays_outside_api_limits() {
        let response = service_busy();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get("retry-after").unwrap(), "1");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(rate_class(&Method::GET, "/healthz").is_none());
        assert!(rate_class(&Method::GET, "/v1/metrics").is_some());
    }

    #[tokio::test]
    async fn saturated_api_requests_get_503_before_handlers_while_health_stays_available() {
        let state = AppState {
            service: Arc::new(WamService::new(InMemoryRepository::seeded())),
            auth: Arc::new(DemoTokenAuthenticator::default()),
            reviewer: Arc::new(NoReviewerAuthenticator),
            identity: None,
            rate_limits: Arc::new(InMemoryRateLimitStore::default()),
            rate_limit_key_secret: Arc::from("test-rate-limit-secret"),
            trusted_proxy_ranges: Arc::new(Vec::new()),
            load_shed: LoadShedGate::new(1).unwrap(),
        };
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let handler_started = Arc::clone(&started);
        let handler_release = Arc::clone(&release);
        let app = Router::new()
            .route(
                "/v1/held",
                get(move || {
                    let started = Arc::clone(&handler_started);
                    let release = Arc::clone(&handler_release);
                    async move {
                        started.notify_one();
                        release.notified().await;
                        StatusCode::OK
                    }
                }),
            )
            .route("/healthz", get(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(
                state,
                enforce_load_shed::<InMemoryRepository, DemoTokenAuthenticator>,
            ));

        let first_app = app.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(Request::get("/v1/held").body(Body::empty()).unwrap())
                .await
                .unwrap()
        });
        started.notified().await;

        let saturated = app
            .clone()
            .oneshot(Request::get("/v1/held").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(saturated.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(saturated.headers().get("retry-after").unwrap(), "1");

        let health = app
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        release.notify_one();
        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
    }
}

//! Closed operational diagnostics: no payloads, URLs, messages, identities or credentials.
mod store;
mod types;
use axum::{
    Extension, Json, Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
pub use store::{DiagnosticStoreError, Diagnostics};
pub use types::{DiagnosticEvent, ErrorCode, Operation, Source, StoredDiagnostic, random_id};
use types::{now, operation, valid_id};
#[derive(Clone)]
struct Intake {
    diagnostics: Diagnostics,
    admission: Arc<Mutex<HashMap<String, (u64, u32)>>>,
}

/// Adds request correlation around every route, including authentication/rate-limit
/// rejections, and a separately bounded client-diagnostic intake.
pub fn observe(router: Router, diagnostics: Diagnostics) -> Router {
    let state = Intake {
        diagnostics: diagnostics.clone(),
        admission: Arc::new(Mutex::new(HashMap::new())),
    };
    let intake = Router::new()
        .route("/v1/telemetry/events", post(ingest))
        .layer(DefaultBodyLimit::max(2048))
        .with_state(state);
    router
        .merge(intake)
        .layer(middleware::from_fn_with_state(diagnostics, trace_request))
}
async fn ingest(
    State(state): State<Intake>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    event: Result<Json<DiagnosticEvent>, JsonRejection>,
) -> Response {
    let key = peer
        .map(|p| p.0.0.ip().to_string())
        .unwrap_or_else(|| "in-process".into());
    let at = now();
    let admitted = if let Ok(mut buckets) = state.admission.lock() {
        buckets.retain(|_, (window, _)| at.saturating_sub(*window) < 60);
        if buckets.len() >= 10_000 && !buckets.contains_key(&key) {
            false
        } else {
            let entry = buckets.entry(key).or_insert((at, 0));
            entry.1 = entry.1.saturating_add(1);
            entry.1 <= 60
        }
    } else {
        false
    };
    if !admitted {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error":"telemetry_rate_limited"})),
        )
            .into_response();
    }
    let Json(event) = match event {
        Ok(value) => value,
        Err(error) => {
            return (
                if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    StatusCode::PAYLOAD_TOO_LARGE
                } else {
                    StatusCode::BAD_REQUEST
                },
                Json(serde_json::json!({"error":"invalid_diagnostic"})),
            )
                .into_response();
        }
    };
    if !event.valid() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_diagnostic"})),
        )
            .into_response();
    }
    if state
        .diagnostics
        .record(StoredDiagnostic::from_event(event))
    {
        (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"accepted":true})),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"diagnostic_queue_full"})),
        )
            .into_response()
    }
}
async fn trace_request(
    State(diagnostics): State<Diagnostics>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let trace = request
        .headers()
        .get("x-wam-trace-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| valid_id(v))
        .map(str::to_owned)
        .unwrap_or_else(random_id);
    let path = request.uri().path();
    let is_mcp = path == "/mcp" || path.starts_with("/mcp/");
    let is_health = path == "/healthz";
    let op = operation(path);
    let source = if is_mcp {
        Source::Mcp
    } else {
        match request
            .headers()
            .get("x-wam-client")
            .and_then(|v| v.to_str().ok())
        {
            Some("web") => Source::Web,
            Some("cli") => Source::Cli,
            Some("mcp") => Source::Mcp,
            _ => Source::Server,
        }
    };
    let method = match request.method().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "PATCH" => "PATCH",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    }
    .to_owned();
    request.headers_mut().insert(
        "x-wam-trace-id",
        HeaderValue::from_str(&trace).expect("validated trace id"),
    );
    let mut response = next.run(request).await;
    let mut error = if response.status().is_server_error() {
        Some(ErrorCode::InternalError)
    } else if response.status().is_client_error() {
        Some(ErrorCode::HttpError)
    } else {
        None
    };
    // Inspect only finite JSON replies. Awaiting EOF on an idle SSE body would
    // prevent the client from receiving even the response headers and keep-alive.
    let json_reply = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if is_mcp && response.status().is_success() && json_reply {
        let (parts, body) = response.into_parts();
        match to_bytes(body, 65_536).await {
            Ok(bytes) => {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if value.get("error").is_some()
                        || value.pointer("/result/isError") == Some(&serde_json::Value::Bool(true))
                    {
                        error = Some(ErrorCode::InvalidInput);
                    }
                }
                response = Response::from_parts(parts, Body::from(bytes));
            }
            Err(_) => {
                response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
                error = Some(ErrorCode::InvalidResponse);
            }
        }
    }
    response.headers_mut().insert(
        "x-wam-trace-id",
        HeaderValue::from_str(&trace).expect("validated trace id"),
    );
    response.headers_mut().append("access-control-expose-headers",HeaderValue::from_static("x-wam-trace-id, retry-after, x-ratelimit-limit, x-ratelimit-remaining, x-ratelimit-reset"));
    if !is_health {
        diagnostics.record(StoredDiagnostic {
            event_id: random_id(),
            trace_id: trace,
            kind: "request".into(),
            source,
            operation: op,
            error_code: error,
            method,
            status_code: Some(response.status().as_u16()),
            duration_ms: started.elapsed().as_millis().min(600_000) as u64,
            received_at: now(),
        });
    }
    response
}

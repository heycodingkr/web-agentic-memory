//! Stateless Streamable HTTP endpoint for remote MCP hosts.

use crate::{
    application::{ServiceError, WamService},
    diagnostics::{McpDiagnostic, valid_id},
    http::Authenticator,
    identity::{
        SCOPE_READ_DASHBOARD, SCOPE_READ_MEMORIES, SCOPE_REPORT_OUTCOME, SCOPE_WRITE_CONTRIBUTIONS,
    },
    owner_usage::OwnerUsageQuery,
    ports::Repository,
};
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::post,
};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use std::{convert::Infallible, time::Duration};
use tokio::sync::Semaphore;
use url::Url;
use wam_client::{ApiError, DiagnosticError, DiagnosticOperation, WamApi};
use wam_contracts::{
    ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
    DashboardResponse, LeaderboardResponse, MemoryDetailResponse, OutcomeReportRequest,
    OutcomeReportResponse, OwnerUsageResponse, ProfilePutRequest, ProfileResponse, RecallRequest,
    RecallResponse, VocabResponse,
};
use wam_mcp::{handle_message, parse_error_response, tool_operation, valid_request_shape};

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn idle_stream_capacity_is_released_when_the_client_disconnects() {
        let state = McpState {
            service: Arc::new(WamService::new(crate::InMemoryRepository::seeded())),
            auth: Arc::new(crate::DemoTokenAuthenticator::default()),
            origins: McpOriginAllowlist::default(),
            oauth_metadata: None,
            resource: None,
            usage: None,
            streams: Arc::new(Semaphore::new(2)),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        );
        let first = listen_response(state.clone(), &headers, None);
        let second = listen_response(state.clone(), &headers, None);
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);
        let denied = listen_response(state.clone(), &headers, None);
        assert_eq!(denied.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(denied.headers()[header::RETRY_AFTER], "5");
        drop(first);
        let replacement = listen_response(state.clone(), &headers, None);
        assert_eq!(replacement.status(), StatusCode::OK);
        headers.insert(
            "last-event-id",
            HeaderValue::from_static("another-client-event"),
        );
        assert_eq!(
            listen_response(state.clone(), &headers, None).status(),
            StatusCode::BAD_REQUEST
        );
        drop(second);
        drop(replacement);
        headers.remove("last-event-id");
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        let response = listen_response(state.clone(), &headers, Some(receiver));
        use http_body_util::BodyExt;
        let mut body = response.into_body();
        assert!(body.frame().await.unwrap().unwrap().data_ref().is_some());
        shutdown.send(true).unwrap();
        let remainder = tokio::time::timeout(Duration::from_secs(1), body.collect())
            .await
            .unwrap()
            .unwrap()
            .to_bytes();
        assert!(
            std::str::from_utf8(&remainder)
                .unwrap()
                .contains("retry: 1000")
        );
        assert_eq!(state.streams.available_permits(), 2);
    }

    #[tokio::test]
    async fn outer_cors_preserves_challenge_exposure_after_quota_headers_replace_it() {
        let app = Router::new()
            .route(
                "/mcp",
                post(|| async {
                    let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
                    response.headers_mut().insert(
                        "access-control-allow-origin",
                        HeaderValue::from_static("https://host.example"),
                    );
                    // Admission middleware sets these after the transport has returned.
                    response.headers_mut().insert(
                        "access-control-expose-headers",
                        HeaderValue::from_static("RateLimit-Limit, Retry-After"),
                    );
                    response
                        .headers_mut()
                        .insert("retry-after", HeaderValue::from_static("5"));
                    response
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                McpOriginAllowlist::new(["https://host.example"]),
                cors_envelope,
            ));
        let response = app
            .oneshot(
                Request::post("/mcp")
                    .header("origin", "https://host.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["retry-after"], "5");
        let exposed = response
            .headers()
            .get_all("access-control-expose-headers")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>()
            .join(",");
        assert!(exposed.contains("WWW-Authenticate"));
        assert!(exposed.contains("RateLimit-Limit"));
        assert!(exposed.contains("Retry-After"));
    }
}

pub(crate) const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MCP_PROTOCOL_HEADER: &str = "mcp-protocol-version";
const SUPPORTED_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// Stop idle SSE bodies before awaiting graceful server shutdown.
#[derive(Clone)]
pub struct McpStreamShutdown(pub tokio::sync::watch::Receiver<bool>);

#[derive(Clone, Debug, Default)]
pub struct McpOriginAllowlist {
    origins: HashSet<String>,
}

impl McpOriginAllowlist {
    pub fn new<I, S>(origins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            origins: origins
                .into_iter()
                .filter_map(|origin| canonical_origin(origin.as_ref()))
                .collect(),
        }
    }

    pub fn from_env() -> Self {
        let Ok(origins) = std::env::var("WAM_MCP_ALLOWED_ORIGINS") else {
            return Self::default();
        };
        Self::new(origins.split(',').map(str::trim).filter(|s| !s.is_empty()))
    }

    fn allows(&self, headers: &HeaderMap) -> bool {
        let values = headers.get_all(header::ORIGIN);
        let mut origins = values.iter();
        let Some(value) = origins.next() else {
            return true;
        };
        if origins.next().is_some() {
            return false;
        }
        let Some(origin) = value.to_str().ok().and_then(canonical_origin) else {
            return false;
        };
        self.origins.contains(&origin)
    }
}

fn canonical_origin(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let origin = url.origin().ascii_serialization();
    (origin != "null").then_some(origin)
}

struct McpState<R, A> {
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    origins: McpOriginAllowlist,
    oauth_metadata: Option<String>,
    resource: Option<String>,
    streams: Arc<Semaphore>,
    usage: Option<OwnerUsageQuery>,
}

impl<R, A> Clone for McpState<R, A> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            auth: Arc::clone(&self.auth),
            origins: self.origins.clone(),
            oauth_metadata: self.oauth_metadata.clone(),
            resource: self.resource.clone(),
            streams: self.streams.clone(),
            usage: self.usage.clone(),
        }
    }
}

pub fn router_with_origins<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    origins: McpOriginAllowlist,
) -> Router {
    router_with_auth_metadata(service, auth, origins, None)
}

pub fn router_with_auth_metadata<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    origins: McpOriginAllowlist,
    oauth_metadata: Option<String>,
) -> Router {
    router_with_runtime(service, auth, origins, oauth_metadata, None, None)
}

pub(crate) fn router_with_runtime<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    origins: McpOriginAllowlist,
    oauth_metadata: Option<String>,
    resource: Option<String>,
    usage: Option<OwnerUsageQuery>,
) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(handle::<R, A>)
                .get(listen::<R, A>)
                .delete(method_not_allowed)
                .options(preflight::<R, A>),
        )
        .with_state(McpState {
            service,
            auth,
            origins,
            oauth_metadata,
            resource,
            streams: Arc::new(Semaphore::new(64)),
            usage,
        })
}

async fn listen<R: Repository, A: Authenticator>(
    axum::extract::State(state): axum::extract::State<McpState<R, A>>,
    headers: HeaderMap,
    shutdown: Option<axum::Extension<McpStreamShutdown>>,
) -> Response {
    let origin = headers
        .get(header::ORIGIN)
        .filter(|_| state.origins.allows(&headers))
        .cloned();
    let response = listen_response(state, &headers, shutdown.map(|value| value.0.0));
    match origin {
        Some(origin) => cors_response(response, origin),
        None => response,
    }
}

fn listen_response<R: Repository, A: Authenticator>(
    state: McpState<R, A>,
    headers: &HeaderMap,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> Response {
    if !state.origins.allows(headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "request origin is not allowed",
        );
    }
    if media_quality(headers, "text/event-stream") == 0.0 {
        return error_response(
            StatusCode::NOT_ACCEPTABLE,
            "unsupported_accept",
            "GET requires text/event-stream",
        );
    }
    if !valid_protocol_version(headers, &json!({"method":"ping"})) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_protocol_version",
            "unsupported MCP-Protocol-Version",
        );
    }
    let account = state.auth.account_id(headers);
    if state.auth.has_credential(headers) && account.is_none() {
        return authentication_required(
            StatusCode::UNAUTHORIZED,
            "invalid_credential",
            "the supplied credential is invalid or expired",
            state.oauth_metadata.as_deref(),
            SCOPE_READ_MEMORIES,
        );
    }
    if account.is_some() && !state.auth.has_scope(headers, SCOPE_READ_MEMORIES) {
        return authentication_required(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            "this stream requires memory read permission",
            state.oauth_metadata.as_deref(),
            SCOPE_READ_MEMORIES,
        );
    }
    if headers.contains_key("last-event-id") {
        return error_response(
            StatusCode::BAD_REQUEST,
            "resumption_not_supported",
            "this stateless server does not replay SSE events",
        );
    }
    let Ok(permit) = state.streams.try_acquire_owned() else {
        let mut response = error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "stream_capacity",
            "too many open MCP streams",
        );
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        return response;
    };
    // This stateless service has no unsolicited private events. The guard stays in
    // the body until disconnect, bounding idle connections without broadcasting data.
    let events = stream::once(async { Ok::<_, Infallible>(Event::default().comment("connected")) })
        .chain(stream::once(async move {
            match shutdown {
                Some(mut receiver) => {
                    while !*receiver.borrow_and_update() {
                        if receiver.changed().await.is_err() {
                            break;
                        }
                    }
                }
                None => std::future::pending::<()>().await,
            }
            Ok(Event::default().retry(Duration::from_secs(1)))
        }))
        .map(move |event| {
            let _guard = &permit;
            event
        });
    sse_headers(
        Sse::new(events)
            .keep_alive(
                KeepAlive::new()
                    .interval(Duration::from_secs(15))
                    .text("keep-alive"),
            )
            .into_response(),
    )
}

fn sse_headers(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    no_store(response)
}

fn message_event(value: Value) -> Response {
    let event = Event::default().event("message").data(value.to_string());
    sse_headers(Sse::new(stream::once(async { Ok::<_, Infallible>(event) })).into_response())
}

async fn method_not_allowed() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, POST"));
    no_store(response)
}

const CORS_HEADERS: &[&str] = &[
    "authorization",
    "content-type",
    "accept",
    "mcp-protocol-version",
    "mcp-session-id",
    "x-csrf-token",
    "last-event-id",
];

fn cors_response(mut response: Response, origin: HeaderValue) -> Response {
    response
        .headers_mut()
        .insert("access-control-allow-origin", origin);
    response.headers_mut().append(
        "access-control-expose-headers",
        HeaderValue::from_static("WWW-Authenticate, MCP-Protocol-Version, Retry-After"),
    );
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Origin"));
    response
}

async fn preflight<R: Repository, A: Authenticator>(
    axum::extract::State(state): axum::extract::State<McpState<R, A>>,
    headers: HeaderMap,
) -> Response {
    let origin = headers.get(header::ORIGIN).cloned();
    let method = headers
        .get("access-control-request-method")
        .and_then(|value| value.to_str().ok());
    let requested = headers.get_all("access-control-request-headers");
    let valid_headers = requested.iter().all(|value| {
        value.to_str().is_ok_and(|value| {
            value.split(',').all(|name| {
                CORS_HEADERS
                    .iter()
                    .any(|allowed| name.trim().eq_ignore_ascii_case(allowed))
            })
        })
    });
    if origin.is_none()
        || !state.origins.allows(&headers)
        || !matches!(method, Some("POST" | "GET"))
        || !valid_headers
    {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "MCP preflight is not allowed",
        );
    }
    let mut response = cors_response(
        no_store(StatusCode::NO_CONTENT.into_response()),
        origin.unwrap(),
    );
    response.headers_mut().insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST"),
    );
    response.headers_mut().insert("access-control-allow-headers",
        HeaderValue::from_static("Authorization, Content-Type, Accept, MCP-Protocol-Version, MCP-Session-Id, X-CSRF-Token"));
    response
}

// Also wraps the combined HTTP router, so quota/load-shed failures expose their status to allowed hosts.
pub(crate) async fn cors_envelope(
    axum::extract::State(origins): axum::extract::State<McpOriginAllowlist>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let origin = (request.uri().path() == "/mcp"
        && request.method() != axum::http::Method::OPTIONS
        && origins.allows(request.headers()))
    .then(|| request.headers().get(header::ORIGIN).cloned())
    .flatten();
    let response = next.run(request).await;
    match origin {
        Some(origin) => cors_response(response, origin),
        None => response,
    }
}

async fn handle<R: Repository, A: Authenticator>(
    axum::extract::State(state): axum::extract::State<McpState<R, A>>,
    request: Request<Body>,
) -> Response {
    let origin = state
        .origins
        .allows(request.headers())
        .then(|| request.headers().get(header::ORIGIN).cloned())
        .flatten();
    let response = handle_request(state, request).await;
    match origin {
        Some(origin) => cors_response(response, origin),
        None => response,
    }
}

async fn handle_request<R: Repository, A: Authenticator>(
    state: McpState<R, A>,
    request: Request<Body>,
) -> Response {
    let headers = request.headers().clone();
    if !state.origins.allows(&headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "request origin is not allowed",
        );
    }
    if !has_media_type(&headers, header::CONTENT_TYPE.as_str(), "application/json") {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "MCP POST requests must use application/json",
        );
    }
    if media_quality(&headers, "application/json") == 0.0
        || media_quality(&headers, "text/event-stream") == 0.0
    {
        return error_response(
            StatusCode::NOT_ACCEPTABLE,
            "unsupported_accept",
            "Accept must include application/json and text/event-stream",
        );
    }

    let bytes = match to_bytes(request.into_body(), MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                "MCP request exceeds the size limit",
            );
        }
    };
    let message: Value = match serde_json::from_slice(&bytes) {
        Ok(message) => message,
        Err(_) => {
            let mut response = (
                StatusCode::BAD_REQUEST,
                Json(rpc_request_trace(parse_error_response(), &headers)),
            )
                .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            return no_store(response);
        }
    };
    if !valid_request_shape(&message) {
        return json_response(
            StatusCode::BAD_REQUEST,
            rpc_request_trace(
                json!({
                    "jsonrpc":"2.0",
                    "id":message.get("id").filter(|id| id.is_string() || id.is_i64() || id.is_u64()).cloned().unwrap_or(Value::Null),
                    "error":{"code":-32600,"message":"invalid request"}
                }),
                &headers,
            ),
        );
    }
    if !valid_protocol_version(&headers, &message) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_protocol_version",
            "MCP-Protocol-Version is missing or unsupported",
        );
    }

    let required_scope = required_tool_scope(&message);
    let account_id = state.auth.account_id(&headers);
    if state.auth.has_credential(&headers) && account_id.is_none() {
        return authentication_required(
            StatusCode::UNAUTHORIZED,
            "invalid_credential",
            "the supplied credential is invalid or expired",
            state.oauth_metadata.as_deref(),
            required_scope.unwrap_or(SCOPE_READ_MEMORIES),
        );
    }
    if account_id.is_none() && protected_tool_call(&message) {
        return authentication_required(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "this tool requires an authenticated account",
            state.oauth_metadata.as_deref(),
            required_scope.unwrap_or(SCOPE_READ_MEMORIES),
        );
    }

    if state.auth.browser_session(&headers)
        && mutating_tool(&message)
        && !state.auth.csrf_valid(&headers)
    {
        return error_response(
            StatusCode::FORBIDDEN,
            "csrf_failed",
            "browser session requests require a valid CSRF token",
        );
    }

    if let Some(scope) = required_scope {
        if !state.auth.has_scope(&headers, scope) {
            return authentication_required(
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                "this tool requires additional authorization",
                state.oauth_metadata.as_deref(),
                scope,
            );
        }
    }

    let can_read_memories =
        account_id.is_some() && state.auth.has_scope(&headers, SCOPE_READ_MEMORIES);
    let operation = if message["method"] == "tools/call" {
        message["params"]["name"]
            .as_str()
            .map(tool_operation)
            .unwrap_or(DiagnosticOperation::Unknown)
    } else {
        DiagnosticOperation::Unknown
    };
    let api = ServiceMcpApi {
        service: state.service,
        account_id,
        can_read_memories,
        can_read_dashboard: state.auth.has_scope(&headers, SCOPE_READ_DASHBOARD),
        can_write_contributions: state.auth.has_scope(&headers, SCOPE_WRITE_CONTRIBUTIONS),
        can_report_outcome: state.auth.has_scope(&headers, SCOPE_REPORT_OUTCOME),
        usage: state.usage,
        endpoint: state.resource.unwrap_or_else(|| "/mcp".into()),
        trace_id: request_trace(&headers),
        diagnostic_error: Mutex::new(None),
    };
    let mut response = match handle_message(&api, message).await {
        Some(value)
            if media_quality(&headers, "text/event-stream")
                > media_quality(&headers, "application/json") =>
        {
            message_event(value)
        }
        Some(value) => json_response(StatusCode::OK, value),
        None => no_store(StatusCode::ACCEPTED.into_response()),
    };
    response.extensions_mut().insert(McpDiagnostic {
        operation: operation.into(),
        error: api
            .diagnostic_error
            .lock()
            .ok()
            .and_then(|error| *error)
            .map(Into::into),
    });
    response
}

fn request_trace(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-wam-trace-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_id(value))
        .map(str::to_owned)
}

fn rpc_request_trace(mut value: Value, headers: &HeaderMap) -> Value {
    if let Some(trace) = request_trace(headers) {
        value["error"]["data"] = json!({"trace_id":trace});
    }
    value
}

fn required_tool_scope(message: &Value) -> Option<&'static str> {
    if message["method"] != "tools/call" {
        return None;
    }
    match message["params"]["name"].as_str()? {
        "web_memory_my_rank" | "web_memory_my_usage" => Some(SCOPE_READ_DASHBOARD),
        "web_memory_propose" | "web_memory_my_contributions" | "web_memory_withdraw" => {
            Some(SCOPE_WRITE_CONTRIBUTIONS)
        }
        "web_memory_outcome" => Some(SCOPE_REPORT_OUTCOME),
        _ => None,
    }
}

fn protected_tool_call(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("tools/call")
        && matches!(
            message.pointer("/params/name").and_then(Value::as_str),
            Some(
                "web_memory_my_rank"
                    | "web_memory_my_usage"
                    | "web_memory_propose"
                    | "web_memory_my_contributions"
                    | "web_memory_withdraw"
                    | "web_memory_outcome"
            )
        )
}

fn authentication_required(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    metadata: Option<&str>,
    scope: &str,
) -> Response {
    let mut response = error_response(status, code, message);
    let mut challenge = format!("Bearer scope=\"{scope}\"");
    if let Some(metadata) = metadata {
        challenge.push_str(&format!(", resource_metadata=\"{metadata}\""));
    }
    let error = match code {
        "invalid_credential" => Some("invalid_token"),
        "insufficient_scope" => Some("insufficient_scope"),
        _ => None,
    };
    if let Some(error) = error {
        challenge.push_str(&format!(", error=\"{error}\""));
    }
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

fn valid_protocol_version(headers: &HeaderMap, message: &Value) -> bool {
    let initialize = message.get("method").and_then(Value::as_str) == Some("initialize");
    let header_version = headers
        .get(MCP_PROTOCOL_HEADER)
        .and_then(|value| value.to_str().ok());
    if initialize && header_version.is_none() {
        return true;
    }
    let version = header_version.unwrap_or("2025-03-26");
    SUPPORTED_PROTOCOLS.contains(&version)
}

fn media_quality(headers: &HeaderMap, expected: &str) -> f32 {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|range| {
            let mut parts = range.split(';');
            if !parts.next()?.trim().eq_ignore_ascii_case(expected) {
                return None;
            }
            let mut quality = 1.0_f32;
            for parameter in parts {
                if let Some((name, value)) = parameter.trim().split_once('=') {
                    if name.trim().eq_ignore_ascii_case("q") {
                        quality = value.trim().parse::<f32>().ok()?;
                        if !(0.0..=1.0).contains(&quality) {
                            return None;
                        }
                    }
                }
            }
            Some(quality)
        })
        .fold(0.0_f32, f32::max)
}

fn has_media_type(headers: &HeaderMap, header_name: &str, expected: &str) -> bool {
    headers
        .get_all(header_name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| {
            value.split(',').any(|candidate| {
                candidate
                    .split(';')
                    .next()
                    .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case(expected))
            })
        })
}

fn mutating_tool(message: &Value) -> bool {
    let Some(params) = message.get("params") else {
        return false;
    };
    matches!(
        params.get("name").and_then(Value::as_str),
        Some("web_memory_propose" | "web_memory_withdraw" | "web_memory_outcome")
    )
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    json_response(status, json!({"error":code,"message":message}))
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    no_store(response)
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

struct ServiceMcpApi<R> {
    service: Arc<WamService<R>>,
    account_id: Option<String>,
    can_read_memories: bool,
    can_read_dashboard: bool,
    can_write_contributions: bool,
    can_report_outcome: bool,
    usage: Option<OwnerUsageQuery>,
    endpoint: String,
    trace_id: Option<String>,
    diagnostic_error: Mutex<Option<DiagnosticError>>,
}

impl<R> ServiceMcpApi<R> {
    fn account_for_dashboard(&self) -> Result<&str, ApiError> {
        if !self.can_read_dashboard {
            return Err(ApiError::Unauthorized);
        }
        self.account_id.as_deref().ok_or(ApiError::Unauthorized)
    }

    fn account_for_contribution(&self) -> Result<&str, ApiError> {
        if !self.can_write_contributions {
            return Err(ApiError::Unauthorized);
        }
        self.account_id.as_deref().ok_or(ApiError::Unauthorized)
    }

    fn account_for_outcome(&self) -> Result<&str, ApiError> {
        let account_id = self.account_id.as_deref().ok_or(ApiError::Unauthorized)?;
        if !self.can_report_outcome {
            return Err(ApiError::Forbidden);
        }
        Ok(account_id)
    }
}

fn api_error(error: ServiceError) -> ApiError {
    match error {
        ServiceError::InvalidInput(_) => ApiError::InvalidInput,
        ServiceError::NotFound => ApiError::NotFound,
        ServiceError::Conflict => ApiError::Conflict,
        ServiceError::Unauthorized => ApiError::Unauthorized,
        ServiceError::Unavailable | ServiceError::Internal => ApiError::Server,
    }
}

#[async_trait]
impl<R: Repository> WamApi for ServiceMcpApi<R> {
    async fn report_diagnostic(
        &self,
        _operation: DiagnosticOperation,
        error: DiagnosticError,
    ) -> Option<String> {
        if let Ok(mut observed) = self.diagnostic_error.lock() {
            *observed = Some(error);
        }
        self.trace_id.clone()
    }

    async fn memory_status(&self) -> Result<wam_contracts::McpStatusResponse, ApiError> {
        self.service
            .memory_status(
                self.endpoint.clone(),
                self.account_id.as_deref(),
                self.can_write_contributions,
            )
            .map_err(api_error)
    }

    async fn expand_version(
        &self,
        id: &str,
        level: u8,
        version_id: Option<&str>,
    ) -> Result<MemoryDetailResponse, ApiError> {
        self.service
            .expand_version(id, level, version_id, self.account_id.as_deref())
            .map_err(api_error)
    }

    async fn status(&self) -> Result<(), ApiError> {
        Ok(())
    }

    async fn vocab(&self) -> Result<VocabResponse, ApiError> {
        self.service.try_vocab().map_err(api_error)
    }

    async fn recall(&self, request: RecallRequest) -> Result<RecallResponse, ApiError> {
        let account = self
            .can_read_memories
            .then_some(self.account_id.as_deref())
            .flatten();
        self.service.recall(request, account).map_err(api_error)
    }

    async fn expand(&self, id: &str, level: u8) -> Result<MemoryDetailResponse, ApiError> {
        self.service
            .expand(id, level, self.account_id.as_deref())
            .map_err(api_error)
    }

    async fn leaderboard(&self) -> Result<LeaderboardResponse, ApiError> {
        self.service.leaderboard().map_err(api_error)
    }

    async fn dashboard(&self) -> Result<DashboardResponse, ApiError> {
        self.service
            .dashboard(self.account_for_dashboard()?)
            .map_err(api_error)
    }

    async fn my_usage(&self) -> Result<OwnerUsageResponse, ApiError> {
        let account = self.account_for_dashboard()?;
        self.usage
            .as_ref()
            .ok_or(ApiError::Server)?
            .read(account)
            .await
            .map_err(|_| ApiError::Server)
    }

    async fn public_profile(&self, handle: &str) -> Result<ProfileResponse, ApiError> {
        self.service.public_profile(handle).map_err(api_error)
    }

    async fn my_profile(&self) -> Result<ProfileResponse, ApiError> {
        self.service
            .my_profile(self.account_for_dashboard()?)
            .map_err(api_error)
    }

    async fn save_profile(&self, profile: ProfilePutRequest) -> Result<ProfileResponse, ApiError> {
        self.service
            .save_my_profile(self.account_for_dashboard()?, profile)
            .map_err(api_error)
    }

    async fn submit_contribution(
        &self,
        request: CreateContributionRequest,
    ) -> Result<ContributionRecordResponse, ApiError> {
        self.service
            .create_contribution(self.account_for_contribution()?, request)
            .map_err(api_error)
    }

    async fn my_contributions(&self) -> Result<ContributionListResponse, ApiError> {
        self.service
            .my_contributions(self.account_for_contribution()?)
            .map_err(api_error)
    }

    async fn withdraw_contribution(&self, id: &str) -> Result<(), ApiError> {
        self.service
            .withdraw_contribution(self.account_for_contribution()?, id)
            .map_err(api_error)
    }

    async fn report_outcome(
        &self,
        receipt_id: &str,
        request: OutcomeReportRequest,
    ) -> Result<OutcomeReportResponse, ApiError> {
        self.service
            .report_outcome(self.account_for_outcome()?, receipt_id, request)
            .map_err(api_error)
    }
}

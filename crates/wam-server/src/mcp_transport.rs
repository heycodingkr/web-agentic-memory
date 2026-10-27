//! Stateless Streamable HTTP endpoint for remote MCP hosts.

use crate::{
    application::{ServiceError, WamService},
    http::Authenticator,
    identity::{
        SCOPE_READ_DASHBOARD, SCOPE_READ_MEMORIES, SCOPE_REPORT_OUTCOME, SCOPE_WRITE_CONTRIBUTIONS,
    },
    ports::Repository,
};
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
use url::Url;
use wam_client::{ApiError, WamApi};
use wam_contracts::{
    ContributionListResponse, ContributionRecordResponse, CreateContributionRequest,
    DashboardResponse, LeaderboardResponse, MemoryDetailResponse, OutcomeReportRequest,
    OutcomeReportResponse, ProfilePutRequest, ProfileResponse, RecallRequest, RecallResponse,
    VocabResponse,
};
use wam_mcp::{handle_message, parse_error_response};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MCP_PROTOCOL_HEADER: &str = "mcp-protocol-version";
const SUPPORTED_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

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
}

impl<R, A> Clone for McpState<R, A> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            auth: Arc::clone(&self.auth),
            origins: self.origins.clone(),
        }
    }
}

pub fn router_with_origins<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    origins: McpOriginAllowlist,
) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(handle::<R, A>)
                .get(method_not_allowed)
                .delete(method_not_allowed),
        )
        .with_state(McpState {
            service,
            auth,
            origins,
        })
}

async fn method_not_allowed() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST"));
    no_store(response)
}

async fn handle<R: Repository, A: Authenticator>(
    axum::extract::State(state): axum::extract::State<McpState<R, A>>,
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
    if !has_media_type(&headers, header::ACCEPT.as_str(), "application/json")
        || !has_media_type(&headers, header::ACCEPT.as_str(), "text/event-stream")
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
            let mut response =
                (StatusCode::BAD_REQUEST, Json(parse_error_response())).into_response();
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
            json!({
                "jsonrpc":"2.0",
                "id":message.get("id").filter(|id| id.is_string() || id.is_number()).cloned().unwrap_or(Value::Null),
                "error":{"code":-32600,"message":"invalid request"}
            }),
        );
    }
    if !valid_protocol_version(&headers, &message) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_protocol_version",
            "MCP-Protocol-Version is missing or unsupported",
        );
    }

    let account_id = state.auth.account_id(&headers);
    if state.auth.has_credential(&headers) && account_id.is_none() {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_credential",
            "the supplied credential is invalid or expired",
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

    let can_read_memories =
        account_id.is_some() && state.auth.has_scope(&headers, SCOPE_READ_MEMORIES);
    let api = ServiceMcpApi {
        service: state.service,
        account_id,
        can_read_memories,
        can_read_dashboard: state.auth.has_scope(&headers, SCOPE_READ_DASHBOARD),
        can_write_contributions: state.auth.has_scope(&headers, SCOPE_WRITE_CONTRIBUTIONS),
        can_report_outcome: state.auth.has_scope(&headers, SCOPE_REPORT_OUTCOME),
    };
    match handle_message(&api, message).await {
        Some(value) => json_response(StatusCode::OK, value),
        None => no_store(StatusCode::ACCEPTED.into_response()),
    }
}

fn valid_request_shape(message: &Value) -> bool {
    let Some(object) = message.as_object() else {
        return false;
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return false;
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || object
            .get("params")
            .is_some_and(|params| !params.is_object())
    {
        return false;
    }
    let notification = method.starts_with("notifications/");
    if notification == object.contains_key("id") {
        return false;
    }
    match object.get("id") {
        None => true,
        Some(id) => id.is_string() || id.is_number(),
    }
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
    async fn status(&self) -> Result<(), ApiError> {
        Ok(())
    }

    async fn vocab(&self) -> Result<VocabResponse, ApiError> {
        Ok(self.service.vocab())
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

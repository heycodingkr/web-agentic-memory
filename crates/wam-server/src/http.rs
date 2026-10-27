//! HTTP transport adapter. Handlers validate transport concerns, then delegate
//! to WamService; no business rule is implemented in route functions.
mod contribution_routes;
mod identity_routes;
mod read_routes;

use contribution_routes::*;
use identity_routes::*;
use read_routes::*;

use crate::{
    application::{ServiceError, WamService},
    identity::{
        DeviceApprovalPreview, DevicePoll, DeviceStart, DeviceSummary, IdentityError,
        IdentityService, SCOPE_MANAGE_DEVICES, SCOPE_READ_DASHBOARD, SCOPE_READ_MEMORIES,
        SCOPE_REPORT_OUTCOME, SCOPE_WRITE_CONTRIBUTIONS, cookie_value, now_unix,
    },
    ports::Repository,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wam_contracts::{
    ContributionReviewDecisionRequest, CreateContributionRequest, OutcomeReportRequest,
    ProfilePutRequest, RecallRequest,
};

pub trait Authenticator: Send + Sync + 'static {
    fn account_id(&self, headers: &HeaderMap) -> Option<String>;

    fn has_credential(&self, headers: &HeaderMap) -> bool {
        headers.contains_key(header::AUTHORIZATION)
    }

    fn has_scope(&self, headers: &HeaderMap, _scope: &str) -> bool {
        self.account_id(headers).is_some()
    }

    fn browser_session(&self, _headers: &HeaderMap) -> bool {
        false
    }

    fn csrf_valid(&self, _headers: &HeaderMap) -> bool {
        true
    }
}

pub trait ReviewerAuthenticator: Send + Sync + 'static {
    fn reviewer_id(&self, headers: &HeaderMap) -> Option<String>;
}

#[derive(Clone, Default)]
pub struct NoReviewerAuthenticator;

impl ReviewerAuthenticator for NoReviewerAuthenticator {
    fn reviewer_id(&self, _headers: &HeaderMap) -> Option<String> {
        None
    }
}

/// Demo-only bearer auth. It is disabled unless WAM_DEMO_TOKEN is set and is
/// intentionally separate from the production OAuth/session adapter.
#[derive(Clone, Default)]
pub struct DemoTokenAuthenticator {
    token: Option<String>,
}

impl DemoTokenAuthenticator {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("WAM_DEMO_TOKEN")
                .ok()
                .filter(|token| !token.is_empty()),
        }
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
        }
    }
}

impl Authenticator for DemoTokenAuthenticator {
    fn account_id(&self, headers: &HeaderMap) -> Option<String> {
        let configured = self.token.as_ref()?;
        let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
        let supplied = raw.strip_prefix("Bearer ")?;
        (supplied == configured).then(|| "demo-user".to_owned())
    }
}

#[derive(Clone)]
pub struct IdentityAuthenticator {
    demo: DemoTokenAuthenticator,
    identity: Arc<IdentityService>,
}

impl IdentityAuthenticator {
    pub fn new(demo: DemoTokenAuthenticator, identity: Arc<IdentityService>) -> Self {
        Self { demo, identity }
    }
}

impl Authenticator for IdentityAuthenticator {
    fn account_id(&self, headers: &HeaderMap) -> Option<String> {
        if headers.contains_key(header::AUTHORIZATION) {
            return self.demo.account_id(headers).or_else(|| {
                self.identity
                    .authenticate_headers(headers, now_unix())
                    .map(|who| who.account_id)
            });
        }
        self.identity
            .authenticate_headers(headers, now_unix())
            .map(|who| who.account_id)
    }

    fn has_credential(&self, headers: &HeaderMap) -> bool {
        headers.contains_key(header::AUTHORIZATION) || self.identity.has_session_cookie(headers)
    }

    fn has_scope(&self, headers: &HeaderMap, scope: &str) -> bool {
        if self.demo.account_id(headers).is_some() {
            return true;
        }
        self.identity
            .authenticate_headers(headers, now_unix())
            .is_some_and(|who| who.scopes.iter().any(|candidate| candidate == scope))
    }

    fn browser_session(&self, headers: &HeaderMap) -> bool {
        !headers.contains_key(header::AUTHORIZATION) && self.identity.has_session_cookie(headers)
    }

    fn csrf_valid(&self, headers: &HeaderMap) -> bool {
        self.identity.csrf_valid_for_headers(headers, now_unix())
    }
}

/// Separate local-only token for the review console. It is disabled unless
/// WAM_REVIEW_TOKEN is configured and does not grant contributor access.
#[derive(Clone, Default)]
pub struct DemoReviewerTokenAuthenticator {
    token: Option<String>,
}

impl DemoReviewerTokenAuthenticator {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("WAM_REVIEW_TOKEN")
                .ok()
                .filter(|token| !token.is_empty()),
        }
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
        }
    }
}

impl ReviewerAuthenticator for DemoReviewerTokenAuthenticator {
    fn reviewer_id(&self, headers: &HeaderMap) -> Option<String> {
        let configured = self.token.as_ref()?;
        let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
        let supplied = raw.strip_prefix("Bearer ")?;
        (supplied == configured).then(|| "local-reviewer".to_owned())
    }
}

struct AppState<R, A> {
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    reviewer: Arc<dyn ReviewerAuthenticator>,
    identity: Option<Arc<IdentityService>>,
}

impl<R, A> Clone for AppState<R, A> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            auth: Arc::clone(&self.auth),
            reviewer: Arc::clone(&self.reviewer),
            identity: self.identity.as_ref().map(Arc::clone),
        }
    }
}

pub fn router<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
) -> Router {
    router_with_reviewer(service, auth, Arc::new(NoReviewerAuthenticator))
}

pub fn router_with_reviewer<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    reviewer: Arc<dyn ReviewerAuthenticator>,
) -> Router {
    build_router(service, auth, reviewer, None)
}

pub fn router_with_identity<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    reviewer: Arc<dyn ReviewerAuthenticator>,
    identity: Arc<IdentityService>,
) -> Router {
    build_router(service, auth, reviewer, Some(identity))
}

fn build_router<R: Repository, A: Authenticator>(
    service: Arc<WamService<R>>,
    auth: Arc<A>,
    reviewer: Arc<dyn ReviewerAuthenticator>,
    identity: Option<Arc<IdentityService>>,
) -> Router {
    let mcp_routes = crate::mcp_transport::router_with_origins(
        Arc::clone(&service),
        Arc::clone(&auth),
        crate::mcp_transport::McpOriginAllowlist::from_env(),
    );
    let state = AppState {
        service,
        auth,
        reviewer,
        identity,
    };
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/auth/config", get(auth_config::<R, A>))
        .route("/v1/auth/github/start", get(github_oauth_start::<R, A>))
        .route(
            "/v1/auth/github/callback",
            get(github_oauth_callback::<R, A>),
        )
        .route("/v1/auth/session", get(auth_session::<R, A>))
        .route("/v1/auth/logout", post(auth_logout::<R, A>))
        .route("/v1/link/start", post(device_start::<R, A>))
        .route("/v1/link/poll", post(device_poll::<R, A>))
        .route("/v1/link/preview", post(device_preview::<R, A>))
        .route("/v1/link/approve", post(device_approve::<R, A>))
        .route("/v1/link/deny", post(device_deny::<R, A>))
        .route("/v1/me/devices", get(my_devices::<R, A>))
        .route("/v1/me/devices/{id}/revoke", post(revoke_device::<R, A>))
        .route("/v1/me/devices/{id}/rotate", post(rotate_device::<R, A>))
        .route("/v1/metrics", get(public_metrics::<R, A>))
        .route("/v1/memories", get(catalog::<R, A>))
        .route("/v1/memories/vocab", get(vocab::<R, A>))
        .route("/v1/memories/recall", post(recall::<R, A>))
        .route("/v1/memories/{id}", get(expand::<R, A>))
        .route("/v1/leaderboard", get(leaderboard::<R, A>))
        .route("/v1/profiles/{handle}", get(public_profile::<R, A>))
        .route(
            "/v1/me/profile",
            get(my_profile::<R, A>).put(save_profile::<R, A>),
        )
        .route(
            "/v1/me/contributions",
            get(my_contributions::<R, A>).post(create_contribution::<R, A>),
        )
        .route(
            "/v1/me/contributions/{id}",
            get(contribution::<R, A>).delete(withdraw_contribution::<R, A>),
        )
        .route("/v1/review/contributions", get(review_queue::<R, A>))
        .route(
            "/v1/review/contributions/{id}/claim",
            post(claim_contribution::<R, A>),
        )
        .route(
            "/v1/review/contributions/{id}/decision",
            post(decide_contribution::<R, A>),
        )
        .route("/v1/receipts/{id}/outcome", post(report_outcome::<R, A>))
        .route("/v1/me/dashboard", get(dashboard::<R, A>))
        .route("/v1/me/score-events", get(score_events::<R, A>))
        .with_state(state)
        .merge(mcp_routes)
}

fn no_store(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn authenticated<R: Repository, A: Authenticator>(
    state: &AppState<R, A>,
    headers: &HeaderMap,
    required_scope: &str,
    write: bool,
) -> Result<String, HttpError> {
    let account = state
        .auth
        .account_id(headers)
        .ok_or(HttpError::Unauthorized)?;
    if !state.auth.has_scope(headers, required_scope) {
        return Err(HttpError::Forbidden);
    }
    if write && state.auth.browser_session(headers) && !state.auth.csrf_valid(headers) {
        return Err(HttpError::Forbidden);
    }
    state
        .service
        .ensure_account(&account)
        .map_err(HttpError::from)?;
    Ok(account)
}

fn optional_authenticated<R: Repository, A: Authenticator>(
    state: &AppState<R, A>,
    headers: &HeaderMap,
) -> Result<Option<String>, HttpError> {
    if state.auth.has_credential(headers) {
        authenticated(state, headers, SCOPE_READ_MEMORIES, false).map(Some)
    } else {
        Ok(None)
    }
}

fn reviewer_authenticated<R, A>(
    state: &AppState<R, A>,
    headers: &HeaderMap,
) -> Result<String, HttpError> {
    state
        .reviewer
        .reviewer_id(headers)
        .ok_or(HttpError::Unauthorized)
}

#[derive(Clone, Copy)]
enum HttpError {
    BadRequest(&'static str),
    Unauthorized,
    Forbidden,
    TooManyRequests,
    NotFound,
    Conflict,
    Unavailable,
    Internal,
}

impl From<ServiceError> for HttpError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::InvalidInput(code) => Self::BadRequest(code),
            ServiceError::NotFound => Self::NotFound,
            ServiceError::Conflict => Self::Conflict,
            ServiceError::Unauthorized => Self::Unauthorized,
            ServiceError::Unavailable => Self::Unavailable,
            ServiceError::Internal => Self::Internal,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::BadRequest(code) => (StatusCode::BAD_REQUEST, code),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication_required"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "permission_denied"),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        };
        (status, Json(ErrorBody { error: code })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

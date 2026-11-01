use super::*;
use crate::identity::IdentityError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdminStartRequest {
    email: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdminVerifyRequest {
    token: String,
}

pub(super) async fn admin_security<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let private = request.uri().path().starts_with("/v1/admin/")
        || request.uri().path().starts_with("/v1/review/");
    let authorization = if request.uri().path().starts_with("/v1/admin/contributions") {
        admin_identity(&state).and_then(|identity| {
            require_admin(
                identity,
                request.headers(),
                request.method() != axum::http::Method::GET,
            )
        })
    } else {
        Ok(())
    };
    let mut response = match authorization {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    };
    if private {
        no_store(&mut response);
        response.headers_mut().insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
        response.headers_mut().insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
    }
    response
}
fn identity_error(error: IdentityError) -> HttpError {
    match error {
        IdentityError::Disabled | IdentityError::Storage | IdentityError::Provider => {
            HttpError::Unavailable
        }
        IdentityError::RateLimited | IdentityError::TooFast => HttpError::TooManyRequests,
        _ => HttpError::BadRequest("invalid_authorization"),
    }
}
fn admin_identity<R, A>(state: &AppState<R, A>) -> Result<&IdentityService, HttpError> {
    state
        .identity
        .as_deref()
        .filter(|identity| identity.admin_enabled())
        .ok_or(HttpError::Unavailable)
}
fn admin_origin(
    identity: &IdentityService,
    headers: &HeaderMap,
    write: bool,
) -> Result<(), HttpError> {
    if identity.admin_request_valid(headers, write) {
        Ok(())
    } else {
        Err(HttpError::Forbidden)
    }
}
pub(super) async fn admin_start<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<Json<AdminStartRequest>, Response>,
) -> Result<Response, HttpError> {
    let identity = admin_identity(&state)?;
    admin_origin(identity, &headers, true)?;
    let Json(input) = input.map_err(|_| HttpError::BadRequest("invalid_request"))?;
    let binding = match identity.queue_admin_link(&input.email, now_unix()) {
        Ok(binding) => binding,
        Err(IdentityError::Disabled) => return Err(HttpError::Unavailable),
        Err(_) => identity
            .unusable_admin_binding()
            .map_err(|_| HttpError::Internal)?,
    };
    let mut response = (StatusCode::ACCEPTED, Json(serde_json::json!({"accepted": true, "expires_in": crate::identity::admin::ADMIN_LINK_TTL, "retry_after": 60}))).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&identity.admin_cookie(&binding, true))
            .map_err(|_| HttpError::Internal)?,
    );
    Ok(response)
}
pub(super) async fn admin_verify<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<Json<AdminVerifyRequest>, Response>,
) -> Result<Response, HttpError> {
    let identity = admin_identity(&state)?;
    admin_origin(identity, &headers, true)?;
    let Json(input) = input.map_err(|_| HttpError::BadRequest("invalid_request"))?;
    let binding = cookie_value(&headers, identity.admin_cookie_name(true)).unwrap_or_default();
    let (token, session) = identity
        .finish_admin(&binding, &input.token, now_unix())
        .map_err(|error| match error {
            IdentityError::Storage => HttpError::Unavailable,
            _ => HttpError::Unauthorized,
        })?;
    let mut response = Json(session).into_response();
    for cookie in [
        identity.admin_cookie(&token, false),
        identity.admin_cookie("", true),
    ] {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).map_err(|_| HttpError::Internal)?,
        );
    }
    Ok(response)
}
pub(super) async fn admin_session<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let identity = admin_identity(&state).map_err(|_| HttpError::Unauthorized)?;
    admin_origin(identity, &headers, false)?;
    let session =
        identity
            .authenticate_admin(&headers, now_unix())
            .map_err(|error| match error {
                IdentityError::Storage => HttpError::Unavailable,
                _ => HttpError::Unauthorized,
            })?;
    Ok(Json(session).into_response())
}
pub(super) async fn admin_detail<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    require_admin(admin_identity(&state)?, &headers, false)?;
    Ok(Json(
        state
            .service
            .admin_contribution_detail(&id)
            .map_err(HttpError::from)?,
    )
    .into_response())
}
pub(super) async fn admin_logout<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let identity = admin_identity(&state)?;
    require_admin(identity, &headers, true)?;
    identity.logout_admin(&headers).map_err(identity_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in [
        identity.admin_cookie("", false),
        identity.admin_cookie("", true),
    ] {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).map_err(|_| HttpError::Internal)?,
        );
    }
    Ok(response)
}
pub(super) fn require_admin(
    identity: &IdentityService,
    headers: &HeaderMap,
    write: bool,
) -> Result<(), HttpError> {
    admin_origin(identity, headers, write)?;
    let session =
        identity
            .authenticate_admin(headers, now_unix())
            .map_err(|error| match error {
                IdentityError::Storage => HttpError::Unavailable,
                _ => HttpError::Unauthorized,
            })?;
    if write && !identity.admin_csrf_valid(headers, &session) {
        return Err(HttpError::Forbidden);
    }
    Ok(())
}

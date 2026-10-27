use super::*;

#[derive(Serialize)]
struct AuthConfigResponse {
    github_oauth_enabled: bool,
}

pub(super) async fn auth_config<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Response {
    let response = Json(AuthConfigResponse {
        github_oauth_enabled: state
            .identity
            .as_ref()
            .is_some_and(|identity| identity.oauth_enabled()),
    })
    .into_response();
    let mut response = response;
    no_store(&mut response);
    response
}

#[derive(Deserialize)]
pub(super) struct GitHubCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct AuthSessionResponse {
    account_id: String,
    csrf_token: String,
    expires_at: u64,
}

#[derive(Deserialize)]
pub(super) struct DeviceStartRequest {
    name: Option<String>,
    scopes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(super) struct DevicePollRequest {
    device_code: String,
}

#[derive(Deserialize)]
pub(super) struct DeviceApprovalRequest {
    user_code: String,
}

#[derive(Serialize)]
struct DeviceListResponse {
    devices: Vec<DeviceSummary>,
}

#[derive(Serialize)]
struct RotatedDeviceResponse {
    access_token: String,
    device: DeviceSummary,
}

pub(super) async fn github_oauth_start<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Result<Response, HttpError> {
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let (oauth_state, authorize_url) = identity.begin_oauth(now_unix()).map_err(identity_error)?;
    let mut response = redirect_response(&authorize_url)?;
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&identity.oauth_state_cookie(&oauth_state))
            .map_err(|_| HttpError::Internal)?,
    );
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn github_oauth_callback<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Query(query): Query<GitHubCallbackQuery>,
) -> Result<Response, HttpError> {
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    if query.error.is_some() {
        let mut response = redirect_response(&identity.login_error_redirect())?;
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&identity.clear_oauth_state_cookie())
                .map_err(|_| HttpError::Internal)?,
        );
        no_store(&mut response);
        return Ok(response);
    }
    let cookie_state = cookie_value(&headers, "wam_oauth_state").unwrap_or_default();
    let result = match (query.code.as_deref(), query.state.as_deref()) {
        (Some(code), Some(oauth_state)) => {
            identity
                .finish_oauth(code, oauth_state, &cookie_state, now_unix())
                .await
        }
        _ => Err(IdentityError::Invalid),
    };
    match result {
        Ok((account_id, session, _csrf, expires_at)) => {
            state
                .service
                .ensure_account(&account_id)
                .map_err(HttpError::from)?;
            let max_age = expires_at.saturating_sub(now_unix());
            let mut response = redirect_response(&identity.oauth_redirect())?;
            response.headers_mut().append(
                header::SET_COOKIE,
                HeaderValue::from_str(&identity.session_cookie(&session, max_age))
                    .map_err(|_| HttpError::Internal)?,
            );
            response.headers_mut().append(
                header::SET_COOKIE,
                HeaderValue::from_str(&identity.clear_oauth_state_cookie())
                    .map_err(|_| HttpError::Internal)?,
            );
            no_store(&mut response);
            Ok(response)
        }
        Err(_) => {
            let mut response = redirect_response(&identity.login_error_redirect())?;
            response.headers_mut().append(
                header::SET_COOKIE,
                HeaderValue::from_str(&identity.clear_oauth_state_cookie())
                    .map_err(|_| HttpError::Internal)?,
            );
            no_store(&mut response);
            Ok(response)
        }
    }
}

pub(super) async fn auth_session<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let token = cookie_value(&headers, "wam_session").ok_or(HttpError::Unauthorized)?;
    let (account_id, csrf_token, expires_at) = identity
        .session_info(&token, now_unix())
        .map_err(identity_error)?;
    state
        .service
        .ensure_account(&account_id)
        .map_err(HttpError::from)?;
    let mut response = Json(AuthSessionResponse {
        account_id,
        csrf_token,
        expires_at,
    })
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn auth_logout<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let token = cookie_value(&headers, "wam_session").ok_or(HttpError::Unauthorized)?;
    if !identity.csrf_valid_for_headers(&headers, now_unix()) {
        return Err(HttpError::Forbidden);
    }
    identity
        .revoke_session(&token, now_unix())
        .map_err(identity_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&identity.clear_session_cookie()).map_err(|_| HttpError::Internal)?,
    );
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn device_start<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Json(input): Json<DeviceStartRequest>,
) -> Result<Response, HttpError> {
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let scopes = input
        .scopes
        .unwrap_or_else(|| vec![SCOPE_READ_MEMORIES.to_owned()]);
    let result: DeviceStart = identity
        .start_device(
            input.name.as_deref().unwrap_or("WAM client"),
            scopes,
            now_unix(),
        )
        .map_err(identity_error)?;
    let mut response = Json(result).into_response();
    *response.status_mut() = StatusCode::CREATED;
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn device_poll<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Json(input): Json<DevicePollRequest>,
) -> Result<Response, HttpError> {
    if input.device_code.is_empty() || input.device_code.len() > 128 {
        return Err(HttpError::BadRequest("invalid_device_code"));
    }
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let result: DevicePoll = identity
        .poll_device(&input.device_code, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn device_preview<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(input): Json<DeviceApprovalRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let result: DeviceApprovalPreview = identity
        .preview_device(&input.user_code, &account, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn device_approve<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(input): Json<DeviceApprovalRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    identity
        .approve_device(&input.user_code, &account, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(serde_json::json!({"approved": true})).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn device_deny<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(input): Json<DeviceApprovalRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    identity
        .deny_device(&input.user_code, &account, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(serde_json::json!({"denied": true})).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn my_devices<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, false)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let devices = identity
        .devices(&account, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(DeviceListResponse { devices }).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn revoke_device<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    identity
        .revoke_device(&account, &id, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(serde_json::json!({"revoked": true})).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn rotate_device<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let (access_token, device) = identity
        .rotate_device(&account, &id, now_unix())
        .map_err(identity_error)?;
    let mut response = Json(RotatedDeviceResponse {
        access_token,
        device,
    })
    .into_response();
    no_store(&mut response);
    Ok(response)
}

fn redirect_response(location: &str) -> Result<Response, HttpError> {
    let mut response = StatusCode::FOUND.into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(location).map_err(|_| HttpError::Internal)?,
    );
    Ok(response)
}

fn identity_error(error: IdentityError) -> HttpError {
    match error {
        IdentityError::Disabled | IdentityError::Provider => HttpError::Unavailable,
        IdentityError::Invalid => HttpError::BadRequest("invalid_authorization"),
        IdentityError::Expired => HttpError::Unauthorized,
        IdentityError::Conflict => HttpError::Conflict,
        IdentityError::RateLimited => HttpError::TooManyRequests,
        IdentityError::TooFast => HttpError::BadRequest("slow_down"),
        IdentityError::Storage => HttpError::Internal,
    }
}

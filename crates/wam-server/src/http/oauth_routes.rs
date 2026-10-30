use super::*;
use crate::identity::oauth::{
    AuthorizationInput, OAuthError, RegistrationInput, RevocationInput, TokenInput,
};
use axum::{
    Form,
    extract::rejection::{BytesRejection, FormRejection},
};

fn oauth_response(mut response: Response) -> Response {
    no_store(&mut response);
    response
        .headers_mut()
        .insert("pragma", HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
        .headers_mut()
        .insert("access-control-allow-origin", HeaderValue::from_static("*"));
    response
}
fn error(error: OAuthError) -> Response {
    let status = match error {
        OAuthError::Disabled => StatusCode::NOT_FOUND,
        OAuthError::InvalidClient => StatusCode::UNAUTHORIZED,
        OAuthError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        OAuthError::ServerError => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    oauth_response((status, Json(serde_json::json!({ "error": error.code() }))).into_response())
}
pub(super) async fn oauth_options() -> Response {
    let mut response = oauth_response(StatusCode::NO_CONTENT.into_response());
    response.headers_mut().insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response.headers_mut().insert(
        "access-control-allow-headers",
        HeaderValue::from_static("Content-Type, MCP-Protocol-Version"),
    );
    response
}
pub(super) async fn resource_metadata<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Response {
    let config = state
        .identity
        .as_ref()
        .and_then(|identity| identity.mcp_oauth_config().ok());
    match config {
        Some(config) => oauth_response(Json(config.resource_metadata()).into_response()),
        None => error(OAuthError::Disabled),
    }
}
pub(super) async fn authorization_metadata<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Response {
    let config = state
        .identity
        .as_ref()
        .and_then(|identity| identity.mcp_oauth_config().ok());
    match config {
        Some(config) => oauth_response(Json(config.server_metadata()).into_response()),
        None => error(OAuthError::Disabled),
    }
}
pub(super) async fn oauth_register<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    input: Result<Json<RegistrationInput>, Response>,
) -> Response {
    let Some(identity) = &state.identity else {
        return error(OAuthError::Disabled);
    };
    let Ok(Json(input)) = input else {
        return error(OAuthError::InvalidRequest);
    };
    match identity.register_mcp_client(input, now_unix()) {
        Ok(client) => oauth_response((StatusCode::CREATED, Json(client)).into_response()),
        Err(cause) => error(cause),
    }
}
pub(super) async fn oauth_authorize<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> Response {
    let Some(identity) = &state.identity else {
        return error(OAuthError::Disabled);
    };
    let config = match identity.mcp_oauth_config() {
        Ok(config) => config,
        Err(cause) => return error(cause),
    };
    let Some(query) = query else {
        return error(OAuthError::InvalidRequest);
    };
    let input: AuthorizationInput = match protocol_parameters(query.as_bytes(), config) {
        Ok(input) => input,
        Err(cause) => return error(cause),
    };
    match identity.start_mcp_authorization(input, now_unix()) {
        Ok(request) => {
            let target = format!(
                "{}/oauth/approve?request={request}",
                identity.frontend_origin()
            );
            match HeaderValue::from_str(&target) {
                Ok(target) => {
                    let mut response = StatusCode::SEE_OTHER.into_response();
                    response.headers_mut().insert(header::LOCATION, target);
                    oauth_response(response)
                }
                Err(_) => error(OAuthError::ServerError),
            }
        }
        Err(cause) => error(cause),
    }
}
pub(super) async fn oauth_token<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<axum::body::Bytes, BytesRejection>,
) -> Response {
    let Some(identity) = &state.identity else {
        return error(OAuthError::Disabled);
    };
    if headers.contains_key(header::AUTHORIZATION) {
        return error(OAuthError::InvalidClient);
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or("");
    if !content_type
        .trim()
        .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    {
        return error(OAuthError::InvalidRequest);
    }
    let config = match identity.mcp_oauth_config() {
        Ok(config) => config,
        Err(cause) => return error(cause),
    };
    let Ok(input) = input else {
        return error(OAuthError::InvalidRequest);
    };
    let input: TokenInput = match protocol_parameters(&input, config) {
        Ok(input) => input,
        Err(cause) => return error(cause),
    };
    match identity.exchange_mcp_token(input, now_unix()) {
        Ok(token) => oauth_response(Json(token).into_response()),
        Err(cause) => error(cause),
    }
}

fn protocol_parameters<T: serde::de::DeserializeOwned>(
    input: &[u8],
    config: &crate::identity::oauth::OAuthConfig,
) -> Result<T, OAuthError> {
    let mut fields = serde_json::Map::new();
    for (key, value) in url::form_urlencoded::parse(input) {
        if key == "resource" {
            // RFC 8707 permits repeated resource indicators. This server supports one audience.
            if !config.resource_matches(&value) {
                return Err(OAuthError::InvalidRequest);
            }
            if fields.contains_key(key.as_ref()) {
                continue;
            }
        } else if fields.contains_key(key.as_ref()) {
            return Err(OAuthError::InvalidRequest);
        }
        fields.insert(
            key.into_owned(),
            serde_json::Value::String(value.into_owned()),
        );
    }
    serde_json::from_value(serde_json::Value::Object(fields))
        .map_err(|_| OAuthError::InvalidRequest)
}

pub(super) async fn oauth_revoke<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<Form<RevocationInput>, FormRejection>,
) -> Response {
    let Some(identity) = &state.identity else {
        return error(OAuthError::Disabled);
    };
    if headers.contains_key(header::AUTHORIZATION) {
        return error(OAuthError::InvalidClient);
    }
    let Ok(Form(input)) = input else {
        return error(OAuthError::InvalidRequest);
    };
    match identity.revoke_mcp_token(input, now_unix()) {
        Ok(()) => oauth_response(StatusCode::OK.into_response()),
        Err(cause) => error(cause),
    }
}
pub(super) async fn oauth_preview<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(request): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    if !state.auth.browser_session(&headers) {
        return Err(HttpError::Unauthorized);
    }
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, false)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    match identity.preview_mcp_authorization(&request, &account, now_unix()) {
        Ok(preview) => Ok(oauth_response(Json(preview).into_response())),
        Err(cause) => Ok(error(cause)),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApprovalInput {
    approve: bool,
}
pub(super) async fn oauth_decision<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(request): Path<String>,
    headers: HeaderMap,
    input: Result<Json<ApprovalInput>, Response>,
) -> Result<Response, HttpError> {
    if !state.auth.browser_session(&headers) {
        return Err(HttpError::Unauthorized);
    }
    let account = authenticated(&state, &headers, SCOPE_MANAGE_DEVICES, true)?;
    let identity = state.identity.as_ref().ok_or(HttpError::Unavailable)?;
    let Ok(Json(input)) = input else {
        return Ok(error(OAuthError::InvalidRequest));
    };
    match identity.decide_mcp_authorization(&request, &account, input.approve, now_unix()) {
        Ok(redirect_uri) => Ok(oauth_response(
            Json(serde_json::json!({ "redirect_uri": redirect_uri })).into_response(),
        )),
        Err(cause) => Ok(error(cause)),
    }
}

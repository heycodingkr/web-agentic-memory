use super::*;
use crate::identity::email::{EMAIL_CODE_TTL, EMAIL_RESEND_DELAY};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmailStartRequest {
    email: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmailVerifyRequest {
    code: String,
}

fn check_origin(identity: &IdentityService, headers: &HeaderMap) -> Result<(), HttpError> {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    if origins.next().and_then(|value| value.to_str().ok()) != Some(identity.frontend_origin())
        || origins.next().is_some()
    {
        return Err(HttpError::Forbidden);
    }
    Ok(())
}

fn failure(error: HttpError) -> Response {
    let mut response = error.into_response();
    no_store(&mut response);
    response
}
fn identity_failure(error: IdentityError) -> Response {
    let error = match error {
        IdentityError::Disabled | IdentityError::Provider | IdentityError::Storage => {
            HttpError::Unavailable
        }
        IdentityError::Expired => HttpError::Unauthorized,
        IdentityError::RateLimited => HttpError::TooManyRequests,
        _ => HttpError::BadRequest("invalid_authorization"),
    };
    let limited = matches!(error, HttpError::TooManyRequests);
    let mut response = failure(error);
    if limited {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    response
}

pub(super) async fn email_start<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<Json<EmailStartRequest>, axum::response::Response>,
) -> Response {
    let Some(identity) = state.identity.as_ref() else {
        return identity_failure(IdentityError::Disabled);
    };
    if let Err(error) = check_origin(identity, &headers) {
        return failure(error);
    }
    let Json(input) = match input {
        Ok(input) => input,
        Err(mut response) => {
            no_store(&mut response);
            return response;
        }
    };
    match identity.begin_email(&input.email, now_unix()).await {
        Ok(challenge) => {
            let mut response = (StatusCode::ACCEPTED, Json(serde_json::json!({"sent":true,"expires_in":EMAIL_CODE_TTL,"retry_after":EMAIL_RESEND_DELAY}))).into_response();
            let Ok(cookie) = HeaderValue::from_str(&identity.email_challenge_cookie(&challenge))
            else {
                return failure(HttpError::Internal);
            };
            response.headers_mut().insert(header::SET_COOKIE, cookie);
            no_store(&mut response);
            response
        }
        Err(error) => identity_failure(error),
    }
}

pub(super) async fn email_verify<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    input: Result<Json<EmailVerifyRequest>, axum::response::Response>,
) -> Response {
    let Some(identity) = state.identity.as_ref() else {
        return identity_failure(IdentityError::Disabled);
    };
    if let Err(error) = check_origin(identity, &headers) {
        return failure(error);
    }
    let Json(input) = match input {
        Ok(input) => input,
        Err(mut response) => {
            no_store(&mut response);
            return response;
        }
    };
    let challenge = cookie_value(&headers, "wam_email_challenge").unwrap_or_default();
    match identity.finish_email(&challenge, &input.code, now_unix()) {
        Ok((account_id, session, csrf_token, expires_at)) => {
            if state.service.ensure_account(&account_id).is_err() {
                return failure(HttpError::Unavailable);
            }
            let mut response = Json(serde_json::json!({"account_id":account_id,"csrf_token":csrf_token,"expires_at":expires_at})).into_response();
            for cookie in [
                identity.session_cookie(&session, expires_at.saturating_sub(now_unix())),
                identity.email_challenge_cookie(""),
            ] {
                let Ok(cookie) = HeaderValue::from_str(&cookie) else {
                    return failure(HttpError::Internal);
                };
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
            no_store(&mut response);
            response
        }
        Err(error) => identity_failure(error),
    }
}

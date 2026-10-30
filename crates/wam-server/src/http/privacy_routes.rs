use super::*;

pub(super) async fn delete_account<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(request): Json<wam_contracts::AccountDeleteRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_ACCOUNT, true)?;
    state
        .service
        .delete_account(&account, &request.confirmation)
        .map_err(HttpError::from)?;
    if state
        .rate_limits
        .erase_account(
            &state.rate_limit_key_secret,
            &crate::rate_limit::account_digest(&account),
        )
        .await
        .is_err()
    {
        // The primary transaction retained a deletion outbox event. A failed
        // external projection must not restore the already deleted identity.
        eprintln!("account projection cleanup pending");
    }
    let mut response = Json(serde_json::json!({"deleted":true})).into_response();
    if let Some(identity) = &state.identity {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&identity.clear_session_cookie())
                .map_err(|_| HttpError::Internal)?,
        );
    }
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn my_consent<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, false)?;
    let mut response = Json(
        state
            .service
            .contribution_consent(&account)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}
pub(super) async fn save_consent<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(request): Json<wam_contracts::ConsentPutRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, true)?;
    let mut response = Json(
        state
            .service
            .set_contribution_consent(&account, request)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

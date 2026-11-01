use super::*;

pub(super) async fn my_contributions<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, false)?;
    let mut response = Json(
        state
            .service
            .my_contributions(&account)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn create_contribution<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(input): Json<CreateContributionRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, true)?;
    let mut response = Json(
        state
            .service
            .create_contribution(&account, input)
            .map_err(HttpError::from)?,
    )
    .into_response();
    *response.status_mut() = StatusCode::CREATED;
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn contribution<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, false)?;
    let mut response = Json(
        state
            .service
            .contribution(&account, &id)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn withdraw_contribution<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_WRITE_CONTRIBUTIONS, true)?;
    state
        .service
        .withdraw_contribution(&account, &id)
        .map_err(HttpError::from)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn review_queue<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let _reviewer = reviewer_authenticated(&state, &headers, false)?;
    let mut response = Json(state.service.review_queue().map_err(HttpError::from)?).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn claim_contribution<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let _reviewer = reviewer_authenticated(&state, &headers, true)?;
    let result = state
        .service
        .claim_contribution_for_review(&id)
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn decide_contribution<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<ContributionReviewDecisionRequest>,
) -> Result<Response, HttpError> {
    let reviewer = reviewer_authenticated(&state, &headers, true)?;
    let result = state
        .service
        .decide_contribution_review(&id, input.review_generation, input.decision, &reviewer)
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn report_outcome<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<OutcomeReportRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_REPORT_OUTCOME, true)?;
    let result = state
        .service
        .report_outcome(&account, &id, input)
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

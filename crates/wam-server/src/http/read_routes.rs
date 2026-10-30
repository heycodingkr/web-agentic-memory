use super::*;

#[derive(Serialize)]
pub(super) struct Health {
    status: &'static str,
}

pub(super) async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

pub(super) async fn memory_status<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = optional_authenticated(&state, &headers)?;
    // Only configured metadata can supply an absolute endpoint; never trust Host/Forwarded.
    let endpoint = state
        .identity
        .as_ref()
        .and_then(|identity| identity.mcp_oauth_config().ok())
        .map(|config| config.resource().to_owned())
        .unwrap_or_else(|| "/mcp".into());
    let mut response = Json(
        state
            .service
            .memory_status(
                endpoint,
                account.as_deref(),
                state.auth.has_scope(&headers, SCOPE_WRITE_CONTRIBUTIONS),
            )
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn public_metrics<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Result<Response, HttpError> {
    let mut response = Json(
        state
            .service
            .public_usage_metrics()
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn catalog<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Result<Response, HttpError> {
    let mut response = Json(state.service.catalog().map_err(HttpError::from)?).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn vocab<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Result<Json<impl Serialize>, HttpError> {
    Ok(Json(state.service.try_vocab().map_err(HttpError::from)?))
}

pub(super) async fn recall<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(request): Json<RecallRequest>,
) -> Result<Response, HttpError> {
    let account_id = optional_authenticated(&state, &headers)?;
    let result = state
        .service
        .recall(request, account_id.as_deref())
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    if account_id.is_none() {
        response.headers_mut().insert(
            "x-wam-client-cache",
            HeaderValue::from_static("anonymous-recall; max-age=300; stale-if-error=900"),
        );
    }
    no_store(&mut response);
    Ok(response)
}

#[derive(Deserialize)]
pub(super) struct LevelQuery {
    level: Option<u8>,
    version_id: Option<String>,
}

pub(super) async fn expand<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(id): Path<String>,
    Query(query): Query<LevelQuery>,
    headers: HeaderMap,
) -> Result<Json<impl Serialize>, HttpError> {
    let account_id = optional_authenticated(&state, &headers)?;
    state
        .service
        .expand_version(
            &id,
            query.level.unwrap_or(2),
            query.version_id.as_deref(),
            account_id.as_deref(),
        )
        .map(Json)
        .map_err(HttpError::from)
}

pub(super) async fn leaderboard<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Result<Response, HttpError> {
    let result = state.service.leaderboard().map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn public_profile<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Path(handle): Path<String>,
) -> Result<Response, HttpError> {
    let mut response = Json(
        state
            .service
            .public_profile(&handle)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn my_profile<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, false)?;
    let mut response = Json(
        state
            .service
            .my_profile(&account)
            .map_err(HttpError::from)?,
    )
    .into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn save_profile<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
    Json(input): Json<ProfilePutRequest>,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_MANAGE_ACCOUNT, true)?;
    let result = state
        .service
        .save_my_profile(&account, input)
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn dashboard<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, false)?;
    let mut response =
        Json(state.service.dashboard(&account).map_err(HttpError::from)?).into_response();
    no_store(&mut response);
    Ok(response)
}

pub(super) async fn my_usage<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, false)?;
    let usage = crate::owner_usage::OwnerUsageQuery::new(
        Arc::clone(&state.rate_limits),
        Arc::clone(&state.rate_limit_key_secret),
    )
    .read(&account)
    .await
    .map_err(|_| HttpError::Unavailable)?;
    let mut response = Json(usage).into_response();
    no_store(&mut response);
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MemoryAnalyticsQuery {
    from: Option<String>,
    to: Option<String>,
    bucket: Option<String>,
}

pub(super) async fn my_memory_analytics<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Query(query): Query<MemoryAnalyticsQuery>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, false)?;
    let analytics = state
        .service
        .memory_analytics(
            &account,
            query.from.as_deref(),
            query.to.as_deref(),
            query.bucket.as_deref(),
        )
        .map_err(HttpError::from)?;
    let mut response = Json(analytics).into_response();
    no_store(&mut response);
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScoreEventsQuery {
    limit: Option<usize>,
    before: Option<String>,
}

pub(super) async fn score_events<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
    Query(query): Query<ScoreEventsQuery>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, false)?;
    let result = state
        .service
        .score_events(&account, query.limit.unwrap_or(20), query.before.as_deref())
        .map_err(HttpError::from)?;
    let mut response = Json(result).into_response();
    no_store(&mut response);
    Ok(response)
}

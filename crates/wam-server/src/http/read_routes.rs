use super::*;

#[derive(Serialize)]
pub(super) struct Health {
    status: &'static str,
}

pub(super) async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
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
) -> Response {
    let mut response = Json(state.service.catalog()).into_response();
    no_store(&mut response);
    response
}

pub(super) async fn vocab<R: Repository, A: Authenticator>(
    State(state): State<AppState<R, A>>,
) -> Json<impl Serialize> {
    Json(state.service.vocab())
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
    no_store(&mut response);
    Ok(response)
}

#[derive(Deserialize)]
pub(super) struct LevelQuery {
    level: Option<u8>,
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
        .expand(&id, query.level.unwrap_or(2), account_id.as_deref())
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
    let account = authenticated(&state, &headers, SCOPE_READ_DASHBOARD, true)?;
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

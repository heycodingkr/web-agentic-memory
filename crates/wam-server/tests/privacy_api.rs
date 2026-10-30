mod lifecycle_support;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use lifecycle_support::{PassingReplay, contribution, now, provider};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{
    ApplicationRepository, DemoReviewerTokenAuthenticator, DemoTokenAuthenticator,
    FileIdentityStore, IdentityAuthenticator, IdentityService, InMemoryRateLimitStore,
    RateLimitStore, WamService,
    identity::{DevicePoll, GitHubOAuthProvider, IdentityError},
    router_with_identity_and_rate_limit_store,
};
struct OAuth;
#[async_trait::async_trait]
impl GitHubOAuthProvider for OAuth {
    fn authorization_url(&self, state: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("https://example.org/auth?state={state}"))
    }
    async fn exchange_code(&self, code: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("subject:{code}"))
    }
}
async fn login(identity: &IdentityService, code: &str) -> (String, String, String) {
    let (state, _) = identity.begin_oauth(now()).unwrap();
    let (account, session, csrf, _) = identity
        .finish_oauth(code, &state, &state, now())
        .await
        .unwrap();
    (account, session, csrf)
}
async fn send(
    app: &axum::Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    csrf: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, header::HeaderMap, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(session) = session {
        request = request.header(header::COOKIE, format!("wam_session={session}"));
    }
    if let Some(csrf) = csrf {
        request = request.header("x-csrf-token", csrf);
    }
    let request = if let Some(body) = body {
        request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        request.body(Body::empty()).unwrap()
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn setup() -> (
    ApplicationRepository,
    Arc<WamService<ApplicationRepository>>,
    Arc<IdentityService>,
    axum::Router,
) {
    setup_with_rate_limits(Arc::new(InMemoryRateLimitStore::default()))
}
fn setup_with_rate_limits(
    rate_limits: Arc<dyn RateLimitStore>,
) -> (
    ApplicationRepository,
    Arc<WamService<ApplicationRepository>>,
    Arc<IdentityService>,
    axum::Router,
) {
    let repository = ApplicationRepository::in_memory();
    let service = Arc::new(WamService::durable(repository.clone()).unwrap());
    let identity = Arc::new(IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(repository.clone())),
        Some(Arc::new(OAuth)),
        "http://localhost:5173",
        false,
    ));
    let app = router_with_identity_and_rate_limit_store(
        service.clone(),
        Arc::new(IdentityAuthenticator::new(
            DemoTokenAuthenticator::default(),
            identity.clone(),
        )),
        Arc::new(DemoReviewerTokenAuthenticator::default()),
        identity.clone(),
        rate_limits,
        "test-secret",
        Vec::new(),
    );
    (repository, service, identity, app)
}

#[tokio::test]
async fn mcp_status_reads_only_its_owners_consent_and_reflects_withdrawal() {
    let (_, service, identity, app) = setup();
    let (owner, session, _) = login(&identity, "status-owner").await;
    let (other, other_session, _) = login(&identity, "other-status-owner").await;
    let (status, headers, metadata) =
        send(&app, "GET", "/v1/mcp/status", Some(&session), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(metadata["auth"], "linked");
    assert_eq!(metadata["consent"], "enabled");
    assert!(!metadata.to_string().contains(&owner));
    service
        .set_contribution_consent(
            &owner,
            wam_contracts::ConsentPutRequest {
                enabled: false,
                consent_version: wam_contracts::CONTRIBUTION_CONSENT_VERSION.into(),
            },
        )
        .unwrap();
    let (_, _, withdrawn) = send(&app, "GET", "/v1/mcp/status", Some(&session), None, None).await;
    assert_eq!(withdrawn["consent"], "disabled");
    let (_, _, untouched) = send(
        &app,
        "GET",
        "/v1/mcp/status",
        Some(&other_session),
        None,
        None,
    )
    .await;
    assert_eq!(untouched["consent"], "enabled");
    assert!(!untouched.to_string().contains(&other));
}

#[tokio::test]
async fn deleting_account_erases_usage_history_and_fences_late_requests() {
    use wam_server::rate_limit::{UsageRecord, account_bucket, account_usage_key, utc_dates};
    let limits = Arc::new(InMemoryRateLimitStore::default());
    let (_, _, identity, app) = setup_with_rate_limits(limits.clone());
    let (account, session, csrf) = login(&identity, "erase-usage").await;
    assert_eq!(
        send(&app, "GET", "/v1/me/profile", Some(&session), None, None)
            .await
            .0,
        StatusCode::OK
    );
    let date = utc_dates(now(), 1).remove(0);
    let key = account_usage_key("test-secret", &account, &date);
    assert!(
        limits
            .usage_history(&[(key.clone(), date.clone())])
            .await
            .unwrap()[0]
            .requests
            > 0
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            "/v1/me/account",
            Some(&session),
            Some(&csrf),
            Some(json!({"confirmation":"DELETE"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        limits
            .usage_history(&[(key.clone(), date.clone())])
            .await
            .unwrap()[0]
            .requests,
        0
    );
    // A request admitted by another instance just before deletion cannot recreate the projection.
    let status = limits
        .consume(
            &[account_bucket("test-secret", &account)],
            Some(&UsageRecord {
                key: key.clone(),
                units: 1,
                request_class: "other",
            }),
            now() * 1000,
        )
        .await
        .unwrap();
    assert!(!status[0].allowed);
    assert_eq!(
        limits.usage_history(&[(key, date)]).await.unwrap()[0].requests,
        0
    );
}

#[tokio::test]
async fn account_deletion_requires_csrf_and_confirmation_and_revokes_all_credentials() {
    let (repository, service, identity, app) = setup();
    let (author, session, csrf) = login(&identity, "author").await;
    let (supporter, support_session, _) = login(&identity, "supporter").await;
    contribution(&service, &author);
    repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();
    contribution(&service, &supporter);
    repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();
    let device = identity
        .start_device("Device", vec!["write:contributions".into()], now())
        .unwrap();
    identity
        .approve_device(&device.user_code, &author, now())
        .unwrap();
    let DevicePoll::Authorized { access_token, .. } = identity
        .poll_device(&device.device_code, now() + 3)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        send(
            &app,
            "DELETE",
            "/v1/me/account",
            None,
            None,
            Some(json!({"confirmation":"DELETE"}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            "/v1/me/account",
            Some(&session),
            None,
            Some(json!({"confirmation":"DELETE"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            "/v1/me/account",
            Some(&session),
            Some(&csrf),
            Some(json!({"confirmation":"wrong"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let forbidden_device = Request::delete("/v1/me/account")
        .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"confirmation":"DELETE"}).to_string()))
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(forbidden_device)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let (status, headers, body) = send(
        &app,
        "DELETE",
        "/v1/me/account",
        Some(&session),
        Some(&csrf),
        Some(json!({"confirmation":"DELETE"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(
        headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert!(identity.authenticate_session(&session, now()).is_none());
    assert!(
        identity
            .authenticate_device(&access_token, now() + 4)
            .is_none()
    );
    assert_eq!(
        send(&app, "GET", "/v1/me/dashboard", Some(&session), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, "GET", "/v1/auth/session", Some(&session), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            "GET",
            "/v1/me/dashboard",
            Some(&support_session),
            None,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(service.catalog().unwrap().memories.len(), 1);
    assert!(repository.memories()[0].author_account_id.is_none());
    let (replacement, _, _) = login(&identity, "author").await;
    assert_ne!(replacement, author);
}
use wam_server::ports::Repository;

#[tokio::test]
async fn withdrawing_consent_blocks_new_intake_and_does_not_restore_old_attribution_on_opt_in() {
    let (repository, service, identity, app) = setup();
    let (account, session, csrf) = login(&identity, "consent").await;
    let id = contribution(&service, &account);
    let body =
        json!({"enabled":false,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION});
    assert_eq!(
        send(
            &app,
            "PUT",
            "/v1/me/consent",
            Some(&session),
            None,
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, _, response) = send(
        &app,
        "PUT",
        "/v1/me/consent",
        Some(&session),
        Some(&csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["enabled"], false);
    assert_eq!(response["retention_days"], 7);
    let request = json!({"url":"https://example.org/editor","task_category":"compose-draft","actions":["navigate","save"],"consent":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION});
    assert_eq!(
        send(
            &app,
            "POST",
            "/v1/me/contributions",
            Some(&session),
            Some(&csrf),
            Some(request.clone())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        service.contribution(&account, &id).unwrap().status,
        "withdrawn"
    );
    assert!(
        service
            .contribution(&account, &id)
            .unwrap()
            .actions
            .is_empty()
    );
    assert_eq!(
        repository
            .process_next(&mut provider("high"), &PassingReplay, now())
            .unwrap(),
        wam_server::durable::WorkResult::Idle
    );
    assert_eq!(send(&app,"PUT","/v1/me/consent",Some(&session),Some(&csrf),Some(json!({"enabled":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION}))).await.0,StatusCode::OK);
    assert_eq!(
        service.contribution(&account, &id).unwrap().status,
        "withdrawn"
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/v1/me/contributions",
            Some(&session),
            Some(&csrf),
            Some(request)
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn local_only_consent_cannot_authorize_public_publication() {
    let (_, _, identity, app) = setup();
    let (_, session, csrf) = login(&identity, "legacy").await;
    let request = json!({"url":"https://example.org/editor","task_category":"compose-draft","actions":["navigate","save"],"consent":true,"consent_version":"local-development-v1"});
    assert_eq!(
        send(
            &app,
            "POST",
            "/v1/me/contributions",
            Some(&session),
            Some(&csrf),
            Some(request)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

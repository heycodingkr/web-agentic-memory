use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use wam_contracts::RecallResponse;
use wam_server::identity::{GitHubOAuthProvider, IdentityError};
use wam_server::{
    DemoReviewerTokenAuthenticator, DemoTokenAuthenticator, FileContributionStore,
    FileIdentityStore, IdentityAuthenticator, IdentityService, InMemoryRepository, WamService,
    router, router_with_identity, router_with_reviewer,
};

fn app() -> axum::Router {
    router(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    )
}

fn file_app(path: &PathBuf) -> axum::Router {
    router(
        file_service(path),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    )
}

fn file_service(path: &PathBuf) -> Arc<WamService<InMemoryRepository>> {
    let contributions =
        Arc::new(FileContributionStore::open(path).expect("open contribution file"));
    Arc::new(
        WamService::with_contribution_store(
            InMemoryRepository::open(path.with_file_name("profiles.json"))
                .expect("open profile file"),
            contributions,
        )
        .expect("restore file-backed contribution queue"),
    )
}

async fn send(
    app: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, header::HeaderMap, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

struct StubGitHubProvider;

#[async_trait::async_trait]
impl GitHubOAuthProvider for StubGitHubProvider {
    fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, IdentityError> {
        Ok(format!(
            "https://github.test/authorize?state={state}&code_challenge={code_challenge}&code_challenge_method=S256"
        ))
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<String, IdentityError> {
        if code == "valid-code" && !verifier.is_empty() {
            Ok("github-subject-42".to_owned())
        } else {
            Err(IdentityError::Provider)
        }
    }
}

fn cookie_value(headers: &header::HeaderMap, name: &str) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let first = value.split(';').next()?;
            let (key, token) = first.split_once('=')?;
            (key == name).then(|| token.to_owned())
        })
        .expect("set-cookie header")
}

#[tokio::test]
async fn memory_status_is_uncached_and_version_mismatch_does_not_return_latest_content() {
    let app = app();
    let (status, headers, body) = send(
        &app,
        Request::get("/v1/mcp/status")
            .header("host", "untrusted.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let metadata: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(metadata["endpoint"], "/mcp");
    assert_eq!(metadata["auth"], "anonymous");
    assert!(!body.contains("untrusted.example"));
    let (status, _, _) = send(
        &app,
        Request::get("/v1/mcp/status")
            .header(header::AUTHORIZATION, "Bearer invalid-status-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, body) = send(
        &app,
        Request::get("/v1/memories/web%3Aexample-draft?level=2&version_id=v1")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["version_id"],
        "v1"
    );
    let (status, _, body) = send(
        &app,
        Request::get("/v1/memories/web%3Aexample-draft?level=2&version_id=missing-version")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("Open the editor"));
    let (_, _, metrics) = send(
        &app,
        Request::get("/v1/metrics").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metrics).unwrap()["detail_requests"],
        1
    );
}

#[tokio::test]
async fn github_login_csrf_device_scopes_rotation_and_revocation_work_over_http() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let directory =
        std::env::temp_dir().join(format!("wam-http-auth-{}-{nanos}", std::process::id()));
    let identity_path = directory.join("identity.json");
    let identity = Arc::new(IdentityService::new(
        Arc::new(FileIdentityStore::open(&identity_path).expect("identity store")),
        Some(Arc::new(StubGitHubProvider)),
        "https://wam.example",
        true,
    ));
    let service = Arc::new(WamService::new(InMemoryRepository::seeded()));
    let auth = Arc::new(IdentityAuthenticator::new(
        DemoTokenAuthenticator::with_token("local-demo-token"),
        Arc::clone(&identity),
    ));
    let app = router_with_identity(
        service,
        auth,
        Arc::new(wam_server::http::NoReviewerAuthenticator),
        Arc::clone(&identity),
    );

    let start = Request::get("/v1/auth/github/start")
        .body(Body::empty())
        .unwrap();
    let (status, start_headers, _) = send(&app, start).await;
    assert_eq!(status, StatusCode::FOUND);
    let state_cookie = cookie_value(&start_headers, "wam_oauth_state");
    let state = url::Url::parse(
        start_headers
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .query_pairs()
    .find(|(key, _)| key == "state")
    .map(|(_, value)| value.into_owned())
    .expect("oauth state");
    assert_eq!(state, state_cookie);
    let state_set_cookie = start_headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("wam_oauth_state="))
        .unwrap();
    assert!(state_set_cookie.contains("HttpOnly"));
    assert!(state_set_cookie.contains("SameSite=Lax"));
    assert!(state_set_cookie.contains("Secure"));

    let callback = Request::get(format!(
        "/v1/auth/github/callback?code=valid-code&state={state}"
    ))
    .header(header::COOKIE, format!("wam_oauth_state={state_cookie}"))
    .body(Body::empty())
    .unwrap();
    let (status, callback_headers, _) = send(&app, callback).await;
    assert_eq!(status, StatusCode::FOUND);
    let session_token = cookie_value(&callback_headers, "wam_session");
    let session_set_cookie = callback_headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("wam_session="))
        .unwrap();
    assert!(session_set_cookie.contains("HttpOnly"));
    assert!(session_set_cookie.contains("SameSite=Lax"));
    assert!(session_set_cookie.contains("Secure"));

    let session_request = Request::get("/v1/auth/session")
        .header(header::COOKIE, format!("wam_session={session_token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _, session_body) = send(&app, session_request).await;
    assert_eq!(status, StatusCode::OK);
    let session: serde_json::Value = serde_json::from_str(&session_body).unwrap();
    let account_id = session["account_id"].as_str().unwrap();
    let csrf = session["csrf_token"].as_str().unwrap();
    assert!(account_id.starts_with("github_"));
    assert!(!session_body.contains("email"));
    let session_cookie = format!("wam_session={session_token}");

    let contribution = serde_json::json!({
        "url":"https://example.com/guide",
        "task_category":"compose-draft",
        "actions":["navigate","edit","save"],
        "consent":true,
        "consent_version":"public-sharing-v1"
    });
    let missing_csrf = Request::post("/v1/me/contributions")
        .header(header::COOKIE, &session_cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(contribution.to_string()))
        .unwrap();
    assert_eq!(send(&app, missing_csrf).await.0, StatusCode::FORBIDDEN);
    let with_csrf = Request::post("/v1/me/contributions")
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(contribution.to_string()))
        .unwrap();
    assert_eq!(send(&app, with_csrf).await.0, StatusCode::CREATED);

    let start_device = Request::post("/v1/link/start")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "name":"Codex CLI",
                "scopes":["read:memories","write:contributions","report:outcome"]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, device_body) = send(&app, start_device).await;
    assert_eq!(status, StatusCode::CREATED);
    let device_start: serde_json::Value = serde_json::from_str(&device_body).unwrap();
    let device_code = device_start["device_code"].as_str().unwrap();
    let user_code = device_start["user_code"].as_str().unwrap();

    let preview = Request::post("/v1/link/preview")
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"user_code":user_code}).to_string(),
        ))
        .unwrap();
    let (status, _, preview_body) = send(&app, preview).await;
    assert_eq!(status, StatusCode::OK);
    let preview: serde_json::Value = serde_json::from_str(&preview_body).unwrap();
    assert_eq!(preview["name"], "Codex CLI");
    assert_eq!(preview["scopes"].as_array().unwrap().len(), 3);
    assert!(!preview_body.contains("device_code"));

    let approve = Request::post("/v1/link/approve")
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"user_code":user_code}).to_string(),
        ))
        .unwrap();
    assert_eq!(send(&app, approve).await.0, StatusCode::OK);

    let poll = Request::post("/v1/link/poll")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"device_code":device_code}).to_string(),
        ))
        .unwrap();
    let (status, _, token_body) = send(&app, poll).await;
    assert_eq!(status, StatusCode::OK);
    let issued: serde_json::Value = serde_json::from_str(&token_body).unwrap();
    assert_eq!(issued["status"], "authorized");
    let device_token = issued["access_token"].as_str().unwrap();
    let stored = std::fs::read_to_string(&identity_path).unwrap();
    assert!(!stored.contains(device_token));

    for path in ["/v1/me/dashboard", "/v1/me/score-events"] {
        let private_read = Request::get(path)
            .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, private_read).await.0, StatusCode::FORBIDDEN);
    }
    let session_history = Request::get("/v1/me/score-events")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, session_history).await.0, StatusCode::OK);

    let list = Request::get("/v1/me/devices")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();
    let (status, _, list_body) = send(&app, list).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list_body.contains("Codex CLI"));
    assert!(!list_body.contains(device_token));
    let devices: serde_json::Value = serde_json::from_str(&list_body).unwrap();
    let device_id = devices["devices"][0]["id"].as_str().unwrap();

    let rotate = Request::post(format!("/v1/me/devices/{device_id}/rotate"))
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .body(Body::empty())
        .unwrap();
    let (status, _, rotated_body) = send(&app, rotate).await;
    assert_eq!(status, StatusCode::OK);
    let rotated: serde_json::Value = serde_json::from_str(&rotated_body).unwrap();
    let rotated_token = rotated["access_token"].as_str().unwrap();
    let stale_token = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(contribution.to_string()))
        .unwrap();
    assert_eq!(send(&app, stale_token).await.0, StatusCode::UNAUTHORIZED);
    let fresh_token = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, format!("Bearer {rotated_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(contribution.to_string()))
        .unwrap();
    assert_eq!(send(&app, fresh_token).await.0, StatusCode::CREATED);

    let revoke = Request::post(format!("/v1/me/devices/{device_id}/revoke"))
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, revoke).await.0, StatusCode::OK);
    let revoked_token = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, format!("Bearer {rotated_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(contribution.to_string()))
        .unwrap();
    assert_eq!(send(&app, revoked_token).await.0, StatusCode::UNAUTHORIZED);

    let logout_without_csrf = Request::post("/v1/auth/logout")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&app, logout_without_csrf).await.0,
        StatusCode::FORBIDDEN
    );
    let logout = Request::post("/v1/auth/logout")
        .header(header::COOKIE, &session_cookie)
        .header("x-csrf-token", csrf)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, logout).await.0, StatusCode::NO_CONTENT);
    let invalid_session = Request::get("/v1/auth/session")
        .header(header::COOKIE, &session_cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&app, invalid_session).await.0,
        StatusCode::UNAUTHORIZED
    );

    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn github_oauth_start_route_is_registered_without_provider_credentials() {
    let request = Request::get("/v1/auth/github/start")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app(), request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn recall_strips_query_and_enforces_public_url_rules() {
    let app = app();
    let request = Request::post("/v1/memories/recall")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{
            "url":"https://example.com/guide?token=private-sentinel#account",
            "keywords":["blog"],"task_category":"compose-draft"
        }"#,
        ))
        .unwrap();
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(body.contains("web:example-draft"));
    assert!(!body.contains("private-sentinel"));

    let request = Request::post("/v1/memories/recall")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"url":"http://127.0.0.1/admin"}"#))
        .unwrap();
    let (status, _, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stale_credential = Request::post("/v1/memories/recall")
        .header(header::AUTHORIZATION, "Bearer revoked-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"url":"https://example.com/guide"}"#))
        .unwrap();
    assert_eq!(
        send(&app, stale_credential).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn recall_receipts_are_owner_bound_idempotent_and_public_metrics_are_redacted() {
    let app = app();
    let recall_body = r#"{
      "url":"https://example.com/guide?private-sentinel=do-not-store#section",
      "keywords":["blog"],
      "task_category":"compose-draft"
    }"#;
    let request = Request::post("/v1/memories/recall")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(recall_body))
        .unwrap();
    let (status, _, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    let recall: RecallResponse = serde_json::from_str(&body).expect("recall response");
    let receipt_id = recall.receipt_id.expect("authenticated hit has a receipt");
    assert!(!body.contains("private-sentinel"));

    let outcome = |value: &str| {
        Request::post(format!("/v1/receipts/{receipt_id}/outcome"))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"{{"outcome":"{value}"}}"#)))
            .unwrap()
    };
    let (status, _, first) = send(&app, outcome("success")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(first.contains(r#""recorded":true"#));
    let (status, _, duplicate) = send(&app, outcome("success")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(duplicate.contains(r#""recorded":false"#));
    assert_eq!(send(&app, outcome("failed")).await.0, StatusCode::CONFLICT);

    let anonymous = Request::post("/v1/memories/recall")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(recall_body))
        .unwrap();
    let (status, _, anonymous_body) = send(&app, anonymous).await;
    assert_eq!(status, StatusCode::OK);
    let anonymous_recall: RecallResponse =
        serde_json::from_str(&anonymous_body).expect("anonymous recall response");
    assert!(anonymous_recall.receipt_id.is_none());

    let detail = Request::get(format!("/v1/memories/{}?level=1", recall.hits[0].memory_id))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, detail).await.0, StatusCode::OK);

    let metrics_request = Request::get("/v1/metrics").body(Body::empty()).unwrap();
    let (status, headers, metrics) = send(&app, metrics_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(metrics.contains(r#""recall_requests":2"#));
    assert!(metrics.contains(r#""authenticated_recall_requests":1"#));
    assert!(metrics.contains(r#""anonymous_recall_requests":1"#));
    assert!(metrics.contains(r#""detail_requests":1"#));
    assert!(metrics.contains(r#""success_outcomes":1"#));
    assert!(!metrics.contains("demo-user"));
    assert!(!metrics.contains(&receipt_id));
    assert!(!metrics.contains("private-sentinel"));
    assert!(!metrics.contains("example.com"));
}

#[tokio::test]
async fn anonymous_api_burst_is_limited_and_returns_a_retry_time() {
    let app = app();
    for _ in 0..20 {
        let request = Request::get("/v1/metrics").body(Body::empty()).unwrap();
        let (status, _, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::OK);
    }

    let request = Request::get("/v1/metrics").body(Body::empty()).unwrap();
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers.get("ratelimit-limit").unwrap(), "60");
    assert_eq!(headers.get("ratelimit-remaining").unwrap(), "0");
    assert!(headers.get("retry-after").is_some());
    assert!(
        headers
            .get("access-control-expose-headers")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Retry-After")
    );
    assert!(body.contains("\"error\":\"rate_limited\""));
}

#[tokio::test]
async fn invalid_credentials_use_a_bounded_cheap_auth_path_while_anonymous_writes_cost_more() {
    let invalid_token_app = app();
    let payload = r#"{"url":"https://example.com/guide","task_category":"compose-draft","actions":["navigate"],"consent":true,"consent_version":"public-sharing-v1"}"#;
    for _ in 0..20 {
        let request = Request::post("/v1/me/contributions")
            .header(header::AUTHORIZATION, "Bearer invalid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload))
            .unwrap();
        assert_eq!(
            send(&invalid_token_app, request).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let request = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer invalid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    assert_eq!(
        send(&invalid_token_app, request).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );

    let anonymous_app = app();
    for _ in 0..4 {
        let request = Request::post("/v1/me/contributions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload))
            .unwrap();
        assert_eq!(
            send(&anonymous_app, request).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let request = Request::post("/v1/me/contributions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    assert_eq!(
        send(&anonymous_app, request).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn signed_in_usage_shows_owner_quota_and_thirty_day_units() {
    let app = app();
    let recall = Request::post("/v1/memories/recall")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"url":"https://example.com/guide/editor","keywords":["blog"]})
                .to_string(),
        ))
        .unwrap();
    let (status, recall_headers, _) = send(&app, recall).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recall_headers.get("ratelimit-limit").unwrap(), "300");

    let usage = Request::get("/v1/me/usage")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, usage).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    let usage: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(usage["limit_per_minute"], 300);
    assert_eq!(usage["burst_capacity"], 60);
    assert_eq!(usage["remaining_units"], 57);
    assert_eq!(usage["period_days"], 30);
    assert_eq!(usage["daily"].as_array().unwrap().len(), 30);
    let today = usage["daily"].as_array().unwrap().last().unwrap();
    assert_eq!(today["requests"], 1);
    assert_eq!(today["units"], 3);
    assert_eq!(today["recall_requests"], 1);
}

#[tokio::test]
async fn authenticated_throttles_are_counted_in_private_usage_history() {
    let app = app();
    for attempt in 0..21 {
        let recall = Request::post("/v1/memories/recall")
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"url":"https://example.com/guide/editor","keywords":["blog"]})
                    .to_string(),
            ))
            .unwrap();
        let status = send(&app, recall).await.0;
        assert_eq!(
            status,
            if attempt < 20 {
                StatusCode::OK
            } else {
                StatusCode::TOO_MANY_REQUESTS
            }
        );
    }

    let usage = Request::get("/v1/me/usage")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, usage).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    let usage: serde_json::Value = serde_json::from_str(&body).unwrap();
    let today = usage["daily"].as_array().unwrap().last().unwrap();
    assert_eq!(today["requests"], 20);
    assert_eq!(today["units"], 60);
    assert_eq!(today["denied_requests"], 1);
}

#[derive(Clone)]
struct AnalyticsAuthenticator;

impl wam_server::http::Authenticator for AnalyticsAuthenticator {
    fn account_id(&self, headers: &header::HeaderMap) -> Option<String> {
        match headers.get(header::AUTHORIZATION)?.to_str().ok()? {
            "Bearer reader-token" => Some("demo-user".to_owned()),
            "Bearer author-token" => Some("demo-peer".to_owned()),
            _ => None,
        }
    }
}

#[tokio::test]
async fn author_analytics_are_owner_scoped_and_count_only_aggregates() {
    let service = Arc::new(WamService::new(InMemoryRepository::seeded()));
    let app = router(service, Arc::new(AnalyticsAuthenticator));

    let recall = || {
        Request::post("/v1/memories/recall")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "url":"https://example.com/guide/editor",
                    "keywords":["blog"]
                })
                .to_string(),
            ))
            .unwrap()
    };
    let (status, _, _) = send(
        &app,
        Request::post("/v1/memories/recall")
            .header(header::AUTHORIZATION, "Bearer reader-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "url":"https://example.com/guide/editor",
                    "keywords":["blog"]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(send(&app, recall()).await.0, StatusCode::OK);

    for authorization in [Some("Bearer reader-token"), None] {
        let mut request = Request::get("/v1/memories/web%3Aexample-draft")
            .body(Body::empty())
            .unwrap();
        if let Some(authorization) = authorization {
            request
                .headers_mut()
                .insert(header::AUTHORIZATION, authorization.parse().unwrap());
        }
        assert_eq!(send(&app, request).await.0, StatusCode::OK);
    }

    let outcome = Request::post("/v1/receipts/local-use-000001/outcome")
        .header(header::AUTHORIZATION, "Bearer reader-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"outcome":"success","used_version_ids":["v1"]}"#,
        ))
        .unwrap();
    assert_eq!(send(&app, outcome).await.0, StatusCode::OK);

    let missing_auth = Request::get("/v1/me/memories/analytics")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, missing_auth).await.0, StatusCode::UNAUTHORIZED);

    let analytics = Request::get("/v1/me/memories/analytics")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, analytics).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    let analytics: serde_json::Value = serde_json::from_str(&body).unwrap();
    let memory = analytics["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|memory| memory["memory_id"] == "web:example-draft")
        .expect("author sees their own memory aggregate");
    let total = |field: &str| {
        memory["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|point| point[field].as_u64().unwrap())
            .sum::<u64>()
    };
    assert_eq!(total("other_account_recall_hits"), 1);
    assert_eq!(total("anonymous_recall_hits"), 1);
    assert_eq!(total("other_account_detail_views"), 1);
    assert_eq!(total("anonymous_detail_views"), 1);
    assert_eq!(total("success_outcomes"), 1);
    assert!(!body.contains("demo-user"));

    let weekly = Request::get("/v1/me/memories/analytics?bucket=week")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(&app, weekly).await;
    assert_eq!(status, StatusCode::OK);
    let weekly: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(weekly["bucket"], "week");
    let weekly_memory = weekly["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|memory| memory["memory_id"] == "web:example-draft")
        .unwrap();
    assert_eq!(
        weekly_memory["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|point| point["other_account_recall_hits"].as_u64().unwrap())
            .sum::<u64>(),
        1
    );

    let invalid_range = Request::get("/v1/me/memories/analytics?from=2026-02-31&to=2026-03-01")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, invalid_range).await.0, StatusCode::BAD_REQUEST);

    let wrong_owner = Request::get("/v1/me/memories/analytics")
        .header(header::AUTHORIZATION, "Bearer reader-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(&app, wrong_owner).await;
    assert_eq!(status, StatusCode::OK);
    let analytics: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(analytics["memories"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn public_catalog_returns_active_memory_metadata_without_detail_content() {
    let app = app();
    let request = Request::get("/v1/memories").body(Body::empty()).unwrap();
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    let catalog: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(catalog["schema_version"], 1);
    let entry = catalog["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["memory_id"] == "web:example-draft")
        .expect("the active fixture is listed");
    assert_eq!(entry["version_id"], "v1");
    assert_eq!(entry["status"], "active");
    assert_eq!(entry["task_category"], "compose-draft");
    assert_eq!(
        entry["canonical_url_patterns"],
        serde_json::json!(["https://example.com/guide/**"])
    );
    assert!(
        entry["title"]
            .as_str()
            .is_some_and(|title| !title.trim().is_empty())
    );
    assert!(
        entry["summary"]
            .as_str()
            .is_some_and(|summary| !summary.trim().is_empty())
    );
    for private_field in ["hint", "detail", "content", "author_account_id"] {
        assert!(entry.get(private_field).is_none());
    }
    assert!(!body.contains("demo-peer"));
}

#[tokio::test]
async fn contribution_requires_auth_and_explicit_consent_then_returns_owner_receipt() {
    let app = app();
    let payload = r#"{
      "url":"https://example.com/guide?private-marker=secret#section",
      "task_category":"compose-draft",
      "actions":["navigate","edit","save"],
      "consent":true,
      "consent_version":"public-sharing-v1"
    }"#;

    let unauthorized = Request::post("/v1/me/contributions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    assert_eq!(send(&app, unauthorized).await.0, StatusCode::UNAUTHORIZED);

    let no_consent = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            payload.replace(r#""consent":true"#, r#""consent":false"#),
        ))
        .unwrap();
    assert_eq!(send(&app, no_consent).await.0, StatusCode::BAD_REQUEST);

    let request = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(body.contains(r#""status":"queued""#));
    assert!(body.contains(r#""url_pattern":"https://example.com/guide""#));
    assert!(!body.contains("private-marker"));
    assert!(!body.contains("secret"));

    let request = Request::get("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, list) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(list.contains("local-000001"));

    let request = Request::get("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, request).await.0, StatusCode::OK);

    let request = Request::delete("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, request).await.0, StatusCode::NO_CONTENT);

    let request = Request::get("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, request).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn profile_content_and_opt_out_survive_server_restart() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let directory =
        std::env::temp_dir().join(format!("wam-profile-api-{}-{nanos}", std::process::id()));
    let path = directory.join("contributions.json");
    let first_app = file_app(&path);
    let profile = serde_json::json!({
        "handle": "durable_builder",
        "bio": [
            {"type":"heading","level":2,"content":[{"type":"text","text":"My workflows"}]},
            {"type":"paragraph","content":[{"type":"strong","text":"Kept after restart"}]}
        ],
        "service_links":[{"label":"My service","url":"https://example.com/tools"}],
        "public_opt_in":false
    });
    let update = Request::put("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(profile.to_string()))
        .unwrap();
    let (status, _, saved) = send(&first_app, update).await;
    assert_eq!(status, StatusCode::OK);
    drop(first_app);

    let restarted = file_app(&path);
    let own_profile = Request::get("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, restored) = send(&restarted, own_profile).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&restored).unwrap(),
        serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
        "a restart must preserve profile edits and the user's privacy choice"
    );
    for handle in ["durable_builder", "agent_jason"] {
        let public_profile = Request::get(format!("/v1/profiles/{handle}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            send(&restarted, public_profile).await.0,
            StatusCode::NOT_FOUND
        );
    }
    let board = Request::get("/v1/leaderboard").body(Body::empty()).unwrap();
    let (_, _, body) = send(&restarted, board).await;
    assert!(!body.contains("durable_builder"));
    assert!(!body.contains("agent_jason"));
    drop(restarted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn profile_write_failure_returns_unavailable_without_changing_public_state() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "wam-profile-failure-api-{}-{nanos}",
        std::process::id()
    ));
    let app = file_app(&directory.join("contributions.json"));
    let stored = directory.join("profiles.json");
    let backup = directory.join("backup.json");
    let get_own = || {
        Request::get("/v1/me/profile")
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .body(Body::empty())
            .unwrap()
    };
    let (_, _, before) = send(&app, get_own()).await;
    std::fs::rename(&stored, &backup).unwrap();
    std::fs::create_dir(&stored).unwrap();
    let request = Request::put("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json_profile_without_public_visibility()))
        .unwrap();
    let (status, _, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "{\"error\":\"service_unavailable\"}");
    assert_eq!(send(&app, get_own()).await.2, before);
    std::fs::remove_dir(&stored).unwrap();
    std::fs::rename(backup, stored).unwrap();
    drop(app);
    let restarted = file_app(&directory.join("contributions.json"));
    assert_eq!(send(&restarted, get_own()).await.2, before);
    drop(restarted);
    std::fs::remove_dir_all(directory).unwrap();
}

fn json_profile_without_public_visibility() -> String {
    serde_json::json!({
        "handle":"unsaved_profile",
        "bio":[{"type":"paragraph","content":[{"type":"text","text":"Do not commit"}]}],
        "service_links":[],
        "public_opt_in":false
    })
    .to_string()
}

#[tokio::test]
async fn file_backed_contribution_receipt_survives_server_restart_and_withdrawal() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let directory =
        std::env::temp_dir().join(format!("wam-server-api-{}-{nanos}", std::process::id()));
    let path = directory.join("contributions.json");
    let first_service = file_service(&path);
    let first_app = router(
        first_service.clone(),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    );
    let payload = r#"{
      "url":"https://example.com/guide?private-marker=secret#section",
      "task_category":"compose-draft",
      "actions":["navigate","edit","save"],
      "consent":true,
      "consent_version":"public-sharing-v1"
    }"#;
    let submit = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    let (status, _, receipt) = send(&first_app, submit).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(receipt.contains("local-000001"));
    assert_eq!(first_service.pending_gatekeeper_jobs().unwrap(), 1);
    drop(first_app);
    drop(first_service);

    let restarted_service = file_service(&path);
    assert_eq!(restarted_service.pending_gatekeeper_jobs().unwrap(), 1);
    let restarted_app = router(
        restarted_service.clone(),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    );
    let list = Request::get("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(&restarted_app, list).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("local-000001"));
    assert!(body.contains(r#""status":"queued""#));
    assert!(!body.contains("private-marker"));

    let withdraw = Request::delete("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&restarted_app, withdraw).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(restarted_service.pending_gatekeeper_jobs().unwrap(), 0);
    drop(restarted_app);

    let after_withdraw = file_app(&path);
    let detail = Request::get("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&after_withdraw, detail).await.0, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn reviewer_routes_persist_claims_decisions_and_reject_stale_generations() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let directory =
        std::env::temp_dir().join(format!("wam-review-api-{}-{nanos}", std::process::id()));
    let path = directory.join("contributions.json");
    let service = file_service(&path);
    let app = router_with_reviewer(
        service.clone(),
        Arc::new(DemoTokenAuthenticator::with_token("contributor-token")),
        Arc::new(DemoReviewerTokenAuthenticator::with_token("reviewer-token")),
    );
    let payload = r#"{
      "url":"https://example.com/guide?private-marker=secret#section",
      "task_category":"compose-draft",
      "actions":["navigate","edit","save"],
      "consent":true,
      "consent_version":"public-sharing-v1"
    }"#;
    let submit = Request::post("/v1/me/contributions")
        .header(header::AUTHORIZATION, "Bearer contributor-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    let (status, _, receipt) = send(&app, submit).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(receipt.contains(r#""status":"queued""#));
    assert_eq!(service.pending_gatekeeper_jobs().unwrap(), 1);

    let unauthorized = Request::get("/v1/review/contributions")
        .header(header::AUTHORIZATION, "Bearer contributor-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, unauthorized).await.0, StatusCode::UNAUTHORIZED);

    let queue_request = Request::get("/v1/review/contributions")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .body(Body::empty())
        .unwrap();
    let (status, headers, queue) = send(&app, queue_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(queue.contains(r#""queued":1"#));
    assert!(!queue.contains("contributor-token"));
    assert!(!queue.contains("demo-user"));
    assert!(!queue.contains("private-marker"));

    let claim = Request::post("/v1/review/contributions/local-000001/claim")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, claimed) = send(&app, claim).await;
    assert_eq!(status, StatusCode::OK);
    assert!(claimed.contains(r#""status":"in_review""#));
    assert!(claimed.contains(r#""review_generation":1"#));

    let duplicate_claim = Request::post("/v1/review/contributions/local-000001/claim")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, duplicate_claim).await.0, StatusCode::CONFLICT);

    let stale_decision = Request::post("/v1/review/contributions/local-000001/decision")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"review_generation":0,"decision":"approve"}"#,
        ))
        .unwrap();
    assert_eq!(send(&app, stale_decision).await.0, StatusCode::CONFLICT);

    let decide = Request::post("/v1/review/contributions/local-000001/decision")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"review_generation":1,"decision":"approve"}"#,
        ))
        .unwrap();
    let (status, _, decision) = send(&app, decide).await;
    assert_eq!(status, StatusCode::OK);
    assert!(decision.contains(r#""status":"approved""#));
    assert_eq!(service.pending_gatekeeper_jobs().unwrap(), 0);

    let owner_record = Request::get("/v1/me/contributions/local-000001")
        .header(header::AUTHORIZATION, "Bearer contributor-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, owner_view) = send(&app, owner_record).await;
    assert_eq!(status, StatusCode::OK);
    assert!(owner_view.contains(r#""status":"approved""#));

    drop(app);
    drop(service);
    let restarted = file_service(&path);
    assert_eq!(restarted.pending_gatekeeper_jobs().unwrap(), 0);
    let restarted_app = router_with_reviewer(
        restarted,
        Arc::new(DemoTokenAuthenticator::with_token("contributor-token")),
        Arc::new(DemoReviewerTokenAuthenticator::with_token("reviewer-token")),
    );
    let queue_request = Request::get("/v1/review/contributions")
        .header(header::AUTHORIZATION, "Bearer reviewer-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, queue) = send(&restarted_app, queue_request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(queue.contains(r#""approved":1"#));
    assert!(queue.contains(r#""status":"approved""#));
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn leaderboard_and_public_profile_hide_opted_out_accounts() {
    let app = app();
    let request = Request::get("/v1/leaderboard").body(Body::empty()).unwrap();
    let (status, headers, board) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(board.contains("agent_jason"));
    assert!(!board.contains("quiet_builder"));

    let request = Request::get("/v1/profiles/quiet_builder")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn profile_put_is_authenticated_validated_and_atomic() {
    let app = app();
    let unauthorized = Request::get("/v1/me/profile").body(Body::empty()).unwrap();
    assert_eq!(send(&app, unauthorized).await.0, StatusCode::UNAUTHORIZED);

    let unsafe_profile = r#"{
      "handle":"new_builder",
      "bio":[{"type":"paragraph","content":[{"type":"text","text":"hello"}]}],
      "service_links":[{"label":"bad","url":"javascript:alert(1)"}],
      "public_opt_in":true
    }"#;
    let request = Request::put("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(unsafe_profile))
        .unwrap();
    let (status, _, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.contains("javascript:"));

    let valid_profile = r#"{
      "handle":"new_builder",
      "bio":[
        {"type":"heading","level":2,"content":[{"type":"strong","text":"Builder"}]},
        {"type":"paragraph","content":[{"type":"text","text":"I make "},{"type":"emphasis","text":"useful tools"},{"type":"link","text":"My site","url":"https://service.example.org/about"}]}
      ],
      "service_links":[{"label":"Studio","url":"https://service.example.org"}],
      "public_opt_in":true
    }"#;
    let request = Request::put("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(valid_profile))
        .unwrap();
    let (status, headers, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(body.contains("new_builder"));

    let request = Request::get("/v1/profiles/new_builder")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("https://service.example.org"));
}

#[tokio::test]
async fn opting_out_removes_profile_from_public_views_immediately() {
    let app = app();
    let payload = r#"{
      "handle":"agent_jason",
      "bio":[{"type":"paragraph","content":[{"type":"text","text":"Private now"}]}],
      "service_links":[],
      "public_opt_in":false
    }"#;
    let request = Request::put("/v1/me/profile")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();
    assert_eq!(send(&app, request).await.0, StatusCode::OK);

    let request = Request::get("/v1/profiles/agent_jason")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, request).await.0, StatusCode::NOT_FOUND);

    let request = Request::get("/v1/leaderboard").body(Body::empty()).unwrap();
    let (_, _, board) = send(&app, request).await;
    assert!(!board.contains("agent_jason"));

    let request = Request::get("/v1/me/dashboard")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let (status, _, dashboard) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(dashboard.contains(r#""overall_rank":1"#));
    assert!(dashboard.contains(r#""public_rank":null"#));
}

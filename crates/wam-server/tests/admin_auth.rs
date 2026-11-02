use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    },
    time::{Duration, Instant},
};
use tower::ServiceExt;
use wam_server::identity::{IdentityError, admin::ADMIN_SESSION_TTL, email::EmailSender, now_unix};
use wam_server::{
    ApplicationRepository, DemoReviewerTokenAuthenticator, DemoTokenAuthenticator,
    FileIdentityStore, IdentityAuthenticator, IdentityService, InMemoryRepository, WamService,
    router, router_with_identity,
};

const ORIGIN: &str = "https://admin.wam.example";
const EMAIL: &str = "admin@example.test";

#[derive(Default)]
struct Mailer {
    links: Mutex<Vec<String>>,
    fail: bool,
    delay: Duration,
}
#[async_trait::async_trait]
impl EmailSender for Mailer {
    async fn send_code(&self, _: &str, _: &str, _: &str) -> Result<(), IdentityError> {
        Ok(())
    }
    async fn send_admin_link(&self, email: &str, link: &str, _: &str) -> Result<(), IdentityError> {
        assert_eq!(email, EMAIL);
        self.links.lock().unwrap().push(link.into());
        tokio::time::sleep(self.delay).await;
        if self.fail {
            Err(IdentityError::Provider)
        } else {
            Ok(())
        }
    }
}
struct Rig {
    app: axum::Router,
    service: Arc<WamService<ApplicationRepository>>,
    identity: Arc<IdentityService>,
    mailer: Arc<Mailer>,
    path: std::path::PathBuf,
    requests: AtomicU16,
    // Drop storage handles before removing the repository and all its sidecars.
    _directory: tempfile::TempDir,
}
impl Rig {
    fn new(fail: bool) -> Self {
        Self::new_with_delay(fail, Duration::ZERO)
    }
    fn new_with_delay(fail: bool, delay: Duration) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("wam-admin-")
            .tempdir()
            .unwrap();
        let path = directory.path().join("application.json");
        let repo = ApplicationRepository::open(&path).unwrap();
        let mailer = Arc::new(Mailer {
            fail,
            delay,
            ..Default::default()
        });
        let identity = Arc::new(Self::identity(&repo, mailer.clone(), EMAIL));
        let auth = Arc::new(IdentityAuthenticator::new(
            DemoTokenAuthenticator::with_token("customer-token"),
            identity.clone(),
        ));
        let service = Arc::new(WamService::durable(repo.clone()).unwrap());
        let app = router_with_identity(
            service.clone(),
            auth,
            Arc::new(DemoReviewerTokenAuthenticator::with_token(
                "old-review-token",
            )),
            identity.clone(),
        );
        Self {
            app,
            service,
            identity,
            mailer,
            path,
            requests: AtomicU16::new(1),
            _directory: directory,
        }
    }
    fn identity(repo: &ApplicationRepository, mailer: Arc<Mailer>, email: &str) -> IdentityService {
        IdentityService::new(
            Arc::new(FileIdentityStore::from_repository(repo.clone())),
            None,
            "https://wam.example",
            true,
        )
        .with_email_auth(mailer, "fixture-secret-at-least-32-bytes-long".into())
        .unwrap()
        .with_admin_auth(email, ORIGIN)
        .unwrap()
    }
    fn token(&self) -> String {
        self.mailer
            .links
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .split("#token=")
            .nth(1)
            .unwrap()
            .into()
    }
    async fn delivered_token(&self) -> String {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let delivered = !self.mailer.links.lock().unwrap().is_empty();
                if delivered {
                    return self.token();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("admin email should be delivered asynchronously")
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Value,
        headers: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "admin.wam.example")
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let n = self.requests.fetch_add(1, Ordering::Relaxed);
        let req = req
            .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, n as u8], 12000))))
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn login(&self) -> (String, String) {
        let binding = self.identity.begin_admin(EMAIL, now_unix()).await.unwrap();
        let (token, session) = self
            .identity
            .finish_admin(&binding, &self.token(), now_unix())
            .unwrap();
        (
            format!("__Host-wam_admin_session={token}"),
            session.csrf_token,
        )
    }
}

#[tokio::test]
async fn unconfigured_admin_auth_fails_closed() {
    let app = router(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        Arc::new(DemoTokenAuthenticator::default()),
    );
    for (method, path, expected) in [
        (
            "POST",
            "/v1/admin/auth/start",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        ("GET", "/v1/admin/auth/session", StatusCode::UNAUTHORIZED),
        (
            "GET",
            "/v1/admin/contributions",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{path}");
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
}

#[tokio::test]
async fn magic_link_http_flow_is_browser_bound_one_time_and_revocable() {
    let rig = Rig::new(false);
    let (status, headers, body) = rig
        .send(
            "POST",
            "/v1/admin/auth/start",
            json!({"email":EMAIL}),
            &[("origin", ORIGIN)],
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["expires_in"], 600);
    let pending = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(pending.starts_with("__Host-wam_admin_pending="));
    assert!(headers["set-cookie"].to_str().unwrap().contains("Secure"));
    let token = rig.delivered_token().await;
    assert_eq!(
        rig.send(
            "POST",
            "/v1/admin/auth/verify",
            json!({"token":token}),
            &[("origin", ORIGIN)]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, headers, body) = rig
        .send(
            "POST",
            "/v1/admin/auth/verify",
            json!({"token":token}),
            &[("origin", ORIGIN), ("cookie", &pending)],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let session = headers
        .get_all("set-cookie")
        .iter()
        .find(|h| h.to_str().unwrap().starts_with("__Host-wam_admin_session="))
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(headers["cache-control"], "no-store");
    let csrf = body["csrf_token"].as_str().unwrap();
    assert_eq!(
        rig.send(
            "POST",
            "/v1/admin/auth/verify",
            json!({"token":token}),
            &[("origin", ORIGIN), ("cookie", &pending)]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        rig.send(
            "GET",
            "/v1/admin/auth/session",
            json!({}),
            &[("cookie", &session)]
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        rig.send(
            "GET",
            "/v1/admin/contributions",
            json!({}),
            &[("cookie", &session)]
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        rig.send(
            "POST",
            "/v1/admin/auth/logout",
            json!({}),
            &[
                ("origin", ORIGIN),
                ("cookie", &session),
                ("x-csrf-token", csrf)
            ]
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        rig.send(
            "GET",
            "/v1/admin/contributions",
            json!({}),
            &[("cookie", &session)]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn admin_start_response_does_not_wait_for_email_delivery() {
    let rig = Rig::new_with_delay(false, Duration::from_millis(350));
    let started = Instant::now();
    let response = tokio::time::timeout(
        Duration::from_millis(150),
        rig.send(
            "POST",
            "/v1/admin/auth/start",
            json!({"email":EMAIL}),
            &[("origin", ORIGIN)],
        ),
    )
    .await
    .expect("admin start should return before the mail provider");
    assert!(started.elapsed() < Duration::from_millis(150));
    assert_eq!(response.0, StatusCode::ACCEPTED);
    assert_eq!(response.2["accepted"], true);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if !rig.mailer.links.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("email provider should be started asynchronously");
}

#[tokio::test]
async fn only_allowlisted_email_sends_and_origin_csrf_and_bearer_bypasses_are_blocked() {
    let start_rig = Rig::new(false);
    let allowed = start_rig
        .send(
            "POST",
            "/v1/admin/auth/start",
            json!({"email":EMAIL}),
            &[("origin", ORIGIN)],
        )
        .await;
    assert_eq!(allowed.0, StatusCode::ACCEPTED);
    assert_eq!(allowed.2["accepted"], true);
    let allowed_cookie = allowed.1["set-cookie"].to_str().unwrap().to_owned();
    let (allowed_name, allowed_rest) = allowed_cookie.split_once('=').unwrap();
    let (allowed_value, allowed_attributes) = allowed_rest.split_once(';').unwrap();
    let mut denied_cookie = None;
    for email in [
        "other@example.test",
        "admin+alias@example.test",
        "admin@example.test.evil",
        "admin@example.test\r\n",
    ] {
        let (status, headers, body) = start_rig
            .send(
                "POST",
                "/v1/admin/auth/start",
                json!({"email":email}),
                &[("origin", ORIGIN)],
            )
            .await;
        assert_eq!(status, allowed.0, "{email:?}");
        assert_eq!(body, allowed.2, "{email:?}");
        assert_eq!(headers["content-type"], allowed.1["content-type"]);
        assert_eq!(headers["cache-control"], allowed.1["cache-control"]);
        assert_eq!(body.to_string().len(), allowed.2.to_string().len());
        let cookie = headers["set-cookie"].to_str().unwrap();
        let (name, rest) = cookie.split_once('=').unwrap();
        let (value, attributes) = rest.split_once(';').unwrap();
        assert_eq!(name, allowed_name);
        assert_eq!(value.len(), allowed_value.len());
        assert_eq!(attributes, allowed_attributes);
        if email == "other@example.test" {
            denied_cookie = Some(cookie.split(';').next().unwrap().to_owned());
        }
    }
    let (limited_status, _, limited_body) = start_rig
        .send(
            "POST",
            "/v1/admin/auth/start",
            json!({"email":EMAIL}),
            &[("origin", ORIGIN)],
        )
        .await;
    assert_eq!(limited_status, allowed.0);
    assert_eq!(limited_body, allowed.2);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if start_rig.mailer.links.lock().unwrap().len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("only the allowlisted address should be mailed");
    assert_eq!(start_rig.mailer.links.lock().unwrap().len(), 1);
    let (denied_verify, _, _) = start_rig
        .send(
            "POST",
            "/v1/admin/auth/verify",
            json!({"token":start_rig.token()}),
            &[
                ("origin", ORIGIN),
                ("cookie", denied_cookie.as_deref().unwrap()),
            ],
        )
        .await;
    assert_eq!(denied_verify, StatusCode::UNAUTHORIZED);
    let rig = Rig::new(false);
    for headers in [
        vec![],
        vec![("origin", "https://wam.example")],
        vec![("origin", "null")],
    ] {
        assert_eq!(
            rig.send(
                "POST",
                "/v1/admin/auth/start",
                json!({"email":EMAIL}),
                &headers
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let (cookie, csrf) = rig.login().await;
    for path in [
        "/v1/admin/contributions",
        "/v1/admin/contributions/missing",
        "/v1/review/contributions",
    ] {
        for credential in ["Bearer customer-token", "Bearer old-review-token"] {
            let (status, headers, _) = rig
                .send("GET", path, json!({}), &[("authorization", credential)])
                .await;
            assert!(!status.is_success());
            assert_eq!(headers["cache-control"], "no-store");
        }
        assert!(
            !rig.send(
                "GET",
                path,
                json!({}),
                &[("cookie", "wam_session=customer-session")]
            )
            .await
            .0
            .is_success()
        );
        assert!(
            !rig.send(
                "GET",
                path,
                json!({}),
                &[("cookie", &cookie), ("origin", "https://wam.example")]
            )
            .await
            .0
            .is_success()
        );
        assert!(
            !rig.send(
                "GET",
                path,
                json!({}),
                &[("cookie", &cookie), ("sec-fetch-site", "same-site")]
            )
            .await
            .0
            .is_success()
        );
        assert!(
            !rig.send(
                "GET",
                path,
                json!({}),
                &[("cookie", &cookie), ("host", "wam.example")]
            )
            .await
            .0
            .is_success()
        );
    }
    for path in [
        "/v1/admin/contributions/missing/claim",
        "/v1/review/contributions/missing/claim",
        "/v1/admin/contributions/missing/decision",
        "/v1/review/contributions/missing/decision",
        "/v1/admin/auth/logout",
    ] {
        let body = json!({"review_generation":1,"decision":"reject"});
        assert_eq!(
            rig.send(
                "POST",
                path,
                body.clone(),
                &[("cookie", &cookie), ("origin", ORIGIN)]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            rig.send(
                "POST",
                path,
                body.clone(),
                &[
                    ("cookie", &cookie),
                    ("origin", ORIGIN),
                    ("x-csrf-token", "bad")
                ]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            rig.send(
                "POST",
                path,
                body.clone(),
                &[("cookie", &cookie), ("x-csrf-token", &csrf)]
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn links_expire_rate_limit_persist_and_allowlist_changes_revoke_sessions() {
    let rig = Rig::new(false);
    let binding = rig
        .identity
        .begin_admin(" ADMIN@EXAMPLE.TEST ", 1000)
        .await
        .unwrap();
    let token = rig.token();
    assert_eq!(
        rig.identity.begin_admin(EMAIL, 1001).await,
        Err(IdentityError::RateLimited)
    );
    assert!(rig.identity.finish_admin(&binding, &token, 1600).is_err());
    let binding = rig.identity.begin_admin(EMAIL, 1601).await.unwrap();
    let token = rig.token();
    let restored_path = rig.path.with_file_name("application-restored.json");
    std::fs::copy(&rig.path, &restored_path).unwrap();
    let restored_repo = ApplicationRepository::open(&restored_path).unwrap();
    let reopened = Rig::identity(&restored_repo, rig.mailer.clone(), EMAIL);
    let (session_token, session) = reopened.finish_admin(&binding, &token, 1602).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        format!("__Host-wam_admin_session={session_token}")
            .parse()
            .unwrap(),
    );
    assert!(reopened.authenticate_admin(&headers, 1603).is_ok());
    assert!(
        reopened
            .authenticate_admin(&headers, session.expires_at)
            .is_err()
    );
    assert_eq!(session.expires_at, 1602 + ADMIN_SESSION_TTL);
    let changed = Rig::identity(&restored_repo, rig.mailer.clone(), "changed@example.test");
    assert!(changed.authenticate_admin(&headers, 1603).is_err());
    let serialized = std::fs::read_to_string(&restored_path).unwrap();
    let _ = std::fs::remove_file(restored_path);
    for secret in [&binding, &token, &session_token, &EMAIL.to_owned()] {
        assert!(!serialized.contains(secret));
    }
}

#[tokio::test]
async fn authorized_review_details_and_generation_fenced_decisions_are_audited() {
    let rig = Rig::new(false);
    rig.service.ensure_account("email_customer001").unwrap();
    let request = serde_json::from_value(json!({
        "url":"https://example.org/editor","task_category":"compose-draft","actions":["navigate","edit","save"],
        "consent":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION
    })).unwrap();
    let record = rig
        .service
        .create_contribution("email_customer001", request)
        .unwrap();
    let (cookie, csrf) = rig.login().await;
    let detail_path = format!("/v1/admin/contributions/{}", record.id);
    let (status, _, detail) = rig
        .send("GET", &detail_path, json!({}), &[("cookie", &cookie)])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["submitter_account_id"], "email_customer001");
    assert_eq!(detail["record"]["progress"]["stage"], "queued");
    assert!(detail["proposed_memory"].is_null());
    let (status, _, claim) = rig
        .send(
            "POST",
            &format!("{detail_path}/claim"),
            json!({}),
            &[
                ("cookie", &cookie),
                ("origin", ORIGIN),
                ("x-csrf-token", &csrf),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let generation = claim["review_generation"].as_u64().unwrap();
    let decision_path = format!("{detail_path}/decision");
    assert_eq!(
        rig.send(
            "POST",
            &decision_path,
            json!({"review_generation":generation+1,"decision":"reject"}),
            &[
                ("cookie", &cookie),
                ("origin", ORIGIN),
                ("x-csrf-token", &csrf)
            ]
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, _, rejected) = rig
        .send(
            "POST",
            &decision_path,
            json!({"review_generation":generation,"decision":"reject"}),
            &[
                ("cookie", &cookie),
                ("origin", ORIGIN),
                ("x-csrf-token", &csrf),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");
    let (_, _, queue) = rig
        .send(
            "GET",
            "/v1/admin/contributions",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await;
    assert_eq!(queue["counts"]["rejected"], 1);
    assert_eq!(queue["decision_history"][0]["reviewer"], "email-admin");
    assert_eq!(queue["decision_history"][0]["decision"], "rejected");
}

#[tokio::test]
async fn concurrent_redeem_and_reissued_links_cannot_create_multiple_sessions() {
    let rig = Rig::new(false);
    let first_binding = rig.identity.begin_admin(EMAIL, 1000).await.unwrap();
    let first_token = rig.token();
    let binding = rig.identity.begin_admin(EMAIL, 1060).await.unwrap();
    let token = rig.token();
    assert!(
        rig.identity
            .finish_admin(&first_binding, &first_token, 1061)
            .is_err()
    );
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| rig.identity.finish_admin(&binding, &token, 1061));
        let b = scope.spawn(|| rig.identity.finish_admin(&binding, &token, 1061));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        rig.identity.begin_admin(EMAIL, 1061).await,
        Err(IdentityError::RateLimited)
    );
}

#[test]
fn admin_config_requires_an_isolated_safe_origin_and_valid_email() {
    let rig = Rig::new(false);
    for origin in [
        "https://wam.example",
        "http://admin.example.test",
        "https://user:pass@admin.wam.example",
        "https://admin.wam.example/path",
        "https://admin.wam.example?next=evil",
        "https://admin.wam.example#evil",
        "javascript:alert(1)",
    ] {
        let identity = IdentityService::new(
            Arc::new(FileIdentityStore::from_repository(
                ApplicationRepository::demo(),
            )),
            None,
            "https://wam.example",
            true,
        )
        .with_email_auth(
            rig.mailer.clone(),
            "fixture-secret-at-least-32-bytes-long".into(),
        )
        .unwrap();
        assert!(identity.with_admin_auth(EMAIL, origin).is_err(), "{origin}");
    }
}

#[tokio::test]
async fn failed_delivery_cannot_leave_a_usable_link() {
    let rig = Rig::new(true);
    assert_eq!(
        rig.identity.begin_admin(EMAIL, 1000).await,
        Err(IdentityError::Provider)
    );
    assert!(
        rig.identity
            .finish_admin(&"a".repeat(43), &rig.token(), 1001)
            .is_err()
    );
}

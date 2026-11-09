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
    /// The request id (the provider's idempotency key) of every send.
    ids: Mutex<Vec<String>>,
    fail: bool,
    /// The next this-many sends fail; the ones after succeed.
    failures_left: Mutex<usize>,
    delay: Duration,
}
#[async_trait::async_trait]
impl EmailSender for Mailer {
    async fn send_code(&self, _: &str, _: &str, _: &str) -> Result<(), IdentityError> {
        Ok(())
    }
    async fn send_admin_link(
        &self,
        email: &str,
        link: &str,
        request_id: &str,
    ) -> Result<(), IdentityError> {
        assert_eq!(email, EMAIL);
        self.links.lock().unwrap().push(link.into());
        self.ids.lock().unwrap().push(request_id.into());
        tokio::time::sleep(self.delay).await;
        let failing = {
            let mut left = self.failures_left.lock().unwrap();
            let failing = *left > 0;
            *left = left.saturating_sub(1);
            failing
        };
        if self.fail || failing {
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
        Self::build(
            Mailer {
                fail,
                delay,
                ..Default::default()
            },
            false,
        )
    }
    /// A rig whose queued links are sent only when the test says so (`deliver_admin_links`), so that time is the test's.
    fn manual(mailer: Mailer) -> Self {
        Self::build(mailer, true)
    }
    fn build(mailer: Mailer, manual: bool) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("wam-admin-")
            .tempdir()
            .unwrap();
        let path = directory.path().join("application.json");
        let repo = ApplicationRepository::open(&path).unwrap();
        let mailer = Arc::new(mailer);
        let identity = Self::identity(&repo, mailer.clone(), EMAIL);
        let identity = Arc::new(if manual {
            identity.with_manual_admin_delivery()
        } else {
            identity
        });
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

/// Polls until `ready`, or fails the test after two seconds.
async fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

fn token_in(link: &str) -> String {
    link.split("#token=").nth(1).unwrap().into()
}

#[tokio::test]
async fn a_link_cut_off_by_a_restart_is_delivered_by_the_next_process() {
    // ADMIN-33. The first process is a runtime that is dropped while the mail provider is still answering, which
    // cancels the background delivery the way a process exit does.
    let directory = tempfile::Builder::new()
        .prefix("wam-admin-restart-")
        .tempdir()
        .unwrap();
    let path = directory.path().join("application.json");
    let first_mailer = Arc::new(Mailer {
        delay: Duration::from_secs(3600),
        ..Default::default()
    });
    let binding = std::thread::spawn({
        let (path, mailer) = (path.clone(), first_mailer.clone());
        move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            let binding = runtime.block_on(async {
                let repo = ApplicationRepository::open(&path).unwrap();
                let identity = Rig::identity(&repo, mailer.clone(), EMAIL);
                let started = Instant::now();
                let binding = identity.queue_admin_link(EMAIL, 1000).unwrap();
                assert!(
                    started.elapsed() < Duration::from_millis(500),
                    "queueing must not wait for the provider"
                );
                wait_until("the first send to reach the provider", || {
                    mailer.links.lock().unwrap().len() == 1
                })
                .await;
                binding
            });
            drop(runtime);
            binding
        }
    })
    .join()
    .unwrap();
    assert_eq!(first_mailer.links.lock().unwrap().len(), 1);

    // The next process opens the same state. The link is still queued; the first process may still be sending, so
    // the new one waits for its lease to run out.
    let repo = ApplicationRepository::open(&path).unwrap();
    let second_mailer = Arc::new(Mailer::default());
    let identity = Rig::identity(&repo, second_mailer.clone(), EMAIL);
    assert!(identity.admin_delivery_pending(1000).unwrap());
    assert_eq!(identity.deliver_admin_links(1059).await.unwrap(), 0);
    assert_eq!(identity.deliver_admin_links(1060).await.unwrap(), 1);
    // The same link and the same idempotency key as the cut-off send, so a provider that dedupes sends it once.
    assert_eq!(
        first_mailer.links.lock().unwrap()[0],
        second_mailer.links.lock().unwrap()[0]
    );
    assert_eq!(
        first_mailer.ids.lock().unwrap()[0],
        second_mailer.ids.lock().unwrap()[0]
    );
    // The browser that asked can sign in with it, and nothing is left to send.
    let link = second_mailer.links.lock().unwrap()[0].clone();
    identity
        .finish_admin(&binding, &token_in(&link), 1061)
        .unwrap();
    assert!(!identity.admin_delivery_pending(1061).unwrap());
    assert_eq!(identity.deliver_admin_links(1062).await.unwrap(), 0);
    assert_eq!(second_mailer.links.lock().unwrap().len(), 1);
    drop(repo);
}

#[tokio::test]
async fn a_failing_provider_is_retried_on_a_schedule_and_the_link_is_then_given_up() {
    // ADMIN-34.
    let rig = Rig::manual(Mailer {
        fail: true,
        ..Default::default()
    });
    let binding = rig.identity.queue_admin_link(EMAIL, 1000).unwrap();
    // The tries are 5, 15, 30 and 60 seconds apart: at 1000, 1005, 1020, 1050 and 1110.
    for (at, just_before) in [
        (1000, None),
        (1005, Some(1004)),
        (1020, Some(1019)),
        (1050, Some(1049)),
        (1110, Some(1109)),
    ] {
        if let Some(early) = just_before {
            assert_eq!(
                rig.identity.deliver_admin_links(early).await.unwrap(),
                0,
                "before {at}"
            );
        }
        assert_eq!(
            rig.identity.deliver_admin_links(at).await.unwrap(),
            1,
            "at {at}"
        );
    }
    assert_eq!(rig.mailer.links.lock().unwrap().len(), 5);
    // The fifth failure gave the link up: nothing waits, nothing is sent again, and the link is no good.
    assert!(!rig.identity.admin_delivery_pending(1111).unwrap());
    assert_eq!(rig.identity.deliver_admin_links(1300).await.unwrap(), 0);
    assert_eq!(rig.mailer.links.lock().unwrap().len(), 5);
    assert!(matches!(
        rig.identity.finish_admin(&binding, &rig.token(), 1111),
        Err(IdentityError::Invalid)
    ));

    // A provider that recovers in time delivers the same link, and the browser can sign in with it.
    let rig = Rig::manual(Mailer {
        failures_left: Mutex::new(2),
        ..Default::default()
    });
    let binding = rig.identity.queue_admin_link(EMAIL, 1000).unwrap();
    for at in [1000, 1005, 1020] {
        assert_eq!(rig.identity.deliver_admin_links(at).await.unwrap(), 1);
    }
    assert!(!rig.identity.admin_delivery_pending(1020).unwrap());
    let links = rig.mailer.links.lock().unwrap().clone();
    assert!(links.len() == 3 && links.iter().all(|link| *link == links[0]));
    let ids = rig.mailer.ids.lock().unwrap().clone();
    assert!(ids.iter().all(|id| *id == ids[0]));
    rig.identity
        .finish_admin(&binding, &token_in(&links[0]), 1021)
        .unwrap();
}

#[tokio::test]
async fn two_instances_send_a_link_once_and_one_takes_over_a_dead_ones_lease() {
    // ADMIN-35. Two services share one store, like two API instances on one database.
    let directory = tempfile::Builder::new()
        .prefix("wam-admin-shared-")
        .tempdir()
        .unwrap();
    let path = directory.path().join("application.json");
    let repo = ApplicationRepository::open(&path).unwrap();
    let slow = Arc::new(Mailer {
        delay: Duration::from_millis(150),
        ..Default::default()
    });
    let a = Rig::identity(&repo, slow.clone(), EMAIL).with_manual_admin_delivery();
    let b = Rig::identity(&repo, slow.clone(), EMAIL).with_manual_admin_delivery();
    a.queue_admin_link(EMAIL, 1000).unwrap();
    let (x, y) = tokio::join!(a.deliver_admin_links(1000), b.deliver_admin_links(1000));
    assert_eq!(x.unwrap() + y.unwrap(), 1, "one send, not two");
    assert_eq!(slow.links.lock().unwrap().len(), 1);

    // An instance that dies while sending: another one takes the link over when the lease has run out, with the same
    // link and the same idempotency key.
    let stuck = Arc::new(Mailer {
        delay: Duration::from_secs(3600),
        ..Default::default()
    });
    let c = Rig::identity(&repo, stuck.clone(), EMAIL).with_manual_admin_delivery();
    let binding = c.queue_admin_link(EMAIL, 1100).unwrap();
    let dying = tokio::spawn(async move { c.deliver_admin_links(1100).await });
    wait_until("the dying instance to start sending", || {
        stuck.links.lock().unwrap().len() == 1
    })
    .await;
    dying.abort();
    assert_eq!(b.deliver_admin_links(1159).await.unwrap(), 0);
    assert_eq!(b.deliver_admin_links(1160).await.unwrap(), 1);
    let links = slow.links.lock().unwrap().clone();
    assert_eq!(links.len(), 2);
    assert_eq!(links[1], stuck.links.lock().unwrap()[0]);
    assert_eq!(slow.ids.lock().unwrap()[1], stuck.ids.lock().unwrap()[0]);
    b.finish_admin(&binding, &token_in(&links[1]), 1161)
        .unwrap();
    drop((a, b, repo));
}

#[tokio::test]
async fn the_stored_state_holds_no_link_and_expired_or_unwanted_deliveries_cost_nothing() {
    // ADMIN-36.
    let rig = Rig::manual(Mailer::default());
    let rig_ref = &rig;
    let start = |email: &'static str| async move {
        rig_ref
            .send(
                "POST",
                "/v1/admin/auth/start",
                json!({"email": email}),
                &[("origin", ORIGIN)],
            )
            .await
    };
    // An address that is not the administrator's queues nothing, and answers the same.
    let denied = start("other@example.test").await;
    assert_eq!(denied.0, StatusCode::ACCEPTED);
    assert!(!rig.identity.admin_delivery_pending(now_unix()).unwrap());
    assert_eq!(rig.mailer.links.lock().unwrap().len(), 0);
    // The administrator's start queues one link and the response is the same as for the other address.
    let allowed = start(EMAIL).await;
    assert_eq!((allowed.0, &allowed.2), (denied.0, &denied.2));
    assert!(rig.identity.admin_delivery_pending(now_unix()).unwrap());

    // While the link waits, the stored state has the queue entry but neither the address nor any link.
    let waiting = std::fs::read_to_string(&rig.path).unwrap();
    assert!(waiting.contains("deliveries"), "{waiting}");
    assert!(!waiting.contains(EMAIL) && !waiting.contains("/admin/verify"));
    assert_eq!(
        rig.identity.deliver_admin_links(now_unix()).await.unwrap(),
        1
    );
    let token = rig.token();
    let delivered = std::fs::read_to_string(&rig.path).unwrap();
    for stored in [&waiting, &delivered] {
        assert!(!stored.contains(&token), "the token is never stored");
        assert!(!stored.contains(EMAIL));
    }

    // A link that expired before anyone sent it is dropped without calling the provider.
    let rig = Rig::manual(Mailer::default());
    rig.identity.queue_admin_link(EMAIL, 1000).unwrap();
    assert_eq!(rig.identity.deliver_admin_links(1600).await.unwrap(), 0);
    assert!(rig.mailer.links.lock().unwrap().is_empty());
    assert!(!rig.identity.admin_delivery_pending(1600).unwrap());
}

#[tokio::test]
async fn admin_audit_trusted_proxy_uses_the_same_peer_rules_as_rate_limits() {
    let rig = Rig::new(false);
    let diagnostics = wam_server::diagnostics::Diagnostics::in_memory();
    let app = wam_server::router_with_identity_and_rate_limit_store(
        rig.service.clone(),
        Arc::new(IdentityAuthenticator::new(
            DemoTokenAuthenticator::with_token("customer-token"),
            rig.identity.clone(),
        )),
        Arc::new(DemoReviewerTokenAuthenticator::with_token(
            "old-review-token",
        )),
        rig.identity.clone(),
        Arc::new(wam_server::InMemoryRateLimitStore::default()),
        "fixture-private-audit-key-at-least-32-bytes",
        vec!["127.0.0.1/32".parse().unwrap()],
    );
    let app = wam_server::diagnostics::observe(app, diagnostics.clone());
    let mut tags = Vec::new();
    for (peer, forwarded) in [
        ([127, 0, 0, 1], "198.51.100.70"),
        ([198, 51, 100, 70], ""),
        ([127, 0, 0, 1], "203.0.113.44, 198.51.100.70"),
        ([127, 0, 0, 1], "198.51.100.71"),
    ] {
        let mut headers = vec![("origin", ORIGIN)];
        if !forwarded.is_empty() {
            headers.push(("x-forwarded-for", forwarded));
        }
        let (status, headers, _) = admin_audit_send(
            &app,
            "POST",
            "/v1/admin/auth/start",
            json!({"email":"other@example.test"}),
            peer,
            &headers,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let record = admin_audit_record(&diagnostics, &headers);
        tags.push(
            record["admin_auth"]["peer_tag"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(tags[0], tags[1]);
    assert_eq!(
        tags[0], tags[2],
        "an untrusted left-hand hop cannot select the source"
    );
    assert_ne!(tags[0], tags[3]);
    assert!(rig.mailer.links.lock().unwrap().is_empty());
}

#[tokio::test]
async fn admin_audit_keeps_rate_limit_rejections_distinguishable() {
    let rig = Rig::new(false);
    let diagnostics = wam_server::diagnostics::Diagnostics::in_memory();
    let app = wam_server::diagnostics::observe(rig.app.clone(), diagnostics.clone());
    for _ in 0..25 {
        let (status, headers, _) = admin_audit_send(
            &app,
            "POST",
            "/v1/admin/auth/start",
            json!({"email":"other@example.test"}),
            [192, 0, 2, 40],
            &[("origin", ORIGIN)],
        )
        .await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            let record = admin_audit_record(&diagnostics, &headers);
            assert_eq!(record["admin_auth"]["action"], "start");
            assert_eq!(record["status_code"], 429);
            assert_eq!(record["source"], "server");
            return;
        }
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    panic!("the shared IP budget must eventually reject the request");
}

async fn admin_audit_send(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Value,
    peer: [u8; 4],
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "admin.wam.example")
        .header("content-type", "application/json")
        .extension(ConnectInfo(SocketAddr::from((peer, 12000))));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn admin_audit_record(
    diagnostics: &wam_server::diagnostics::Diagnostics,
    headers: &HeaderMap,
) -> Value {
    diagnostics.flush().unwrap();
    let trace = headers["x-wam-trace-id"].to_str().unwrap();
    let record = diagnostics
        .recent(100)
        .unwrap()
        .into_iter()
        .find(|event| event.trace_id == trace)
        .unwrap();
    serde_json::to_value(record).unwrap()
}

#[tokio::test]
async fn admin_audit_start_is_private_server_owned_and_groups_trusted_peers() {
    let rig = Rig::new(false);
    let diagnostics = wam_server::diagnostics::Diagnostics::in_memory();
    let app = wam_server::diagnostics::observe(rig.app.clone(), diagnostics.clone());
    let supplied_trace = "a".repeat(32);
    let mut tags = Vec::new();
    for (peer, forwarded) in [
        ([192, 0, 2, 10], "198.51.100.1"),
        ([192, 0, 2, 10], "203.0.113.99"),
        ([192, 0, 2, 11], "198.51.100.1"),
    ] {
        let (status, headers, body) = admin_audit_send(
            &app,
            "POST",
            "/v1/admin/auth/start?private-query=not-retained",
            json!({"email":"other@example.test"}),
            peer,
            &[
                ("origin", ORIGIN),
                ("x-forwarded-for", forwarded),
                ("x-wam-client", "cli"),
                ("x-wam-trace-id", &supplied_trace),
            ],
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["accepted"], true);
        assert_eq!(headers["cache-control"], "no-store");
        let record = admin_audit_record(&diagnostics, &headers);
        assert_eq!(record["admin_auth"]["action"], "start");
        assert_eq!(record["source"], "server");
        assert_eq!(record["operation"], "auth");
        assert_ne!(record["trace_id"], supplied_trace);
        let tag = record["admin_auth"]["peer_tag"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(tag.len(), 64);
        assert!(
            tag.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(!body.to_string().contains(&tag));
        let stored = record.to_string();
        for private in [
            "other@example.test",
            "192.0.2.",
            forwarded,
            "private-query",
            &supplied_trace,
        ] {
            assert!(
                !stored.contains(private),
                "private request data must not be stored"
            );
        }
        tags.push(tag);
    }
    assert_eq!(
        tags[0], tags[1],
        "an untrusted peer cannot select its source using XFF"
    );
    assert_ne!(tags[0], tags[2]);
    assert!(rig.mailer.links.lock().unwrap().is_empty());
}

#[tokio::test]
async fn admin_audit_distinguishes_verification_session_and_logout_results() {
    let rig = Rig::new(false);
    let diagnostics = wam_server::diagnostics::Diagnostics::in_memory();
    let app = wam_server::diagnostics::observe(rig.app.clone(), diagnostics.clone());
    let peer = [192, 0, 2, 20];
    let (status, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/start",
        json!({"email":EMAIL}),
        peer,
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let mut expected = vec![(headers.clone(), "start", 202)];
    let pending = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let token = rig.delivered_token().await;

    let (status, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/verify",
        json!({"token":token}),
        peer,
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    expected.push((headers, "verify", 401));
    let (status, headers, session_body) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/verify",
        json!({"token":token}),
        peer,
        &[("origin", ORIGIN), ("cookie", &pending)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session = headers
        .get_all("set-cookie")
        .iter()
        .find_map(|value| {
            let cookie = value.to_str().unwrap();
            cookie
                .starts_with("__Host-wam_admin_session=")
                .then(|| cookie.split(';').next().unwrap().to_owned())
        })
        .unwrap();
    let csrf = session_body["csrf_token"].as_str().unwrap();
    expected.push((headers, "verify", 200));
    let (status, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/verify",
        json!({"token":token}),
        peer,
        &[("origin", ORIGIN), ("cookie", &pending)],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    expected.push((headers, "verify", 401));
    let (status, headers, _) = admin_audit_send(
        &app,
        "GET",
        "/v1/admin/auth/session",
        json!({}),
        peer,
        &[("cookie", &session)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    expected.push((headers, "session", 200));
    let (status, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/logout",
        json!({}),
        peer,
        &[("origin", ORIGIN), ("cookie", &session)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    expected.push((headers, "logout", 403));
    let (status, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/logout",
        json!({}),
        peer,
        &[
            ("origin", ORIGIN),
            ("cookie", &session),
            ("x-csrf-token", csrf),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    expected.push((headers, "logout", 204));
    // Use a fresh source bucket so this assertion observes session revocation rather than IP throttling.
    let (status, headers, _) = admin_audit_send(
        &app,
        "GET",
        "/v1/admin/auth/session",
        json!({}),
        [192, 0, 2, 21],
        &[("cookie", &session)],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    expected.push((headers, "session", 401));

    for (headers, action, status) in expected {
        let record = admin_audit_record(&diagnostics, &headers);
        assert_eq!(record["admin_auth"]["action"], action);
        assert_eq!(record["status_code"], status);
        let serialized = record.to_string();
        for private in [EMAIL, &token, &pending, &session, csrf, "192.0.2.20"] {
            assert!(
                !serialized.contains(private),
                "authentication secrets must not be retained"
            );
        }
    }
}

#[tokio::test]
async fn admin_audit_survives_storage_reopen_and_rejects_forged_or_invalid_metadata() {
    let rig = Rig::new(false);
    let path = rig._directory.path().join("audit.sqlite");
    let diagnostics = wam_server::diagnostics::Diagnostics::sqlite(&path).unwrap();
    let app = wam_server::diagnostics::observe(rig.app.clone(), diagnostics.clone());
    let (_, headers, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/admin/auth/start",
        json!({"email":"other@example.test"}),
        [192, 0, 2, 30],
        &[("origin", ORIGIN)],
    )
    .await;
    let record = admin_audit_record(&diagnostics, &headers);
    assert_eq!(record["admin_auth"]["action"], "start");
    let reopened = wam_server::diagnostics::Diagnostics::sqlite(&path).unwrap();
    let stored = reopened
        .recent(100)
        .unwrap()
        .into_iter()
        .find(|event| event.trace_id == record["trace_id"])
        .unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap()["admin_auth"],
        record["admin_auth"]
    );

    let mut old = record.clone();
    old.as_object_mut().unwrap().remove("admin_auth");
    let old: wam_server::diagnostics::StoredDiagnostic = serde_json::from_value(old).unwrap();
    assert!(old.valid(), "previously stored rows remain readable");
    let mut invalid = record.clone();
    invalid["admin_auth"]["peer_tag"] = json!("192.0.2.30");
    let invalid: wam_server::diagnostics::StoredDiagnostic =
        serde_json::from_value(invalid).unwrap();
    assert!(
        !invalid.valid(),
        "raw addresses cannot masquerade as pseudonyms"
    );
    let mut misplaced = record.clone();
    misplaced["operation"] = json!("recall");
    let misplaced: wam_server::diagnostics::StoredDiagnostic =
        serde_json::from_value(misplaced).unwrap();
    assert!(
        !misplaced.valid(),
        "admin metadata belongs only to server auth requests"
    );

    let injected = json!({"schema_version":1,"event_id":"b".repeat(32),"trace_id":"c".repeat(32),"source":"cli","operation":"auth","error_code":"http_error","duration_ms":1,"admin_auth":{"action":"verify","peer_tag":"d".repeat(64)}});
    let (status, _, _) = admin_audit_send(
        &app,
        "POST",
        "/v1/telemetry/events",
        injected,
        [192, 0, 2, 31],
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    diagnostics.flush().unwrap();
    assert!(
        !diagnostics
            .recent(100)
            .unwrap()
            .iter()
            .any(|event| event.event_id == "b".repeat(32))
    );
    reopened.purge(now_unix() + 7 * 86400 + 1).unwrap();
    assert!(reopened.recent(100).unwrap().is_empty());
}

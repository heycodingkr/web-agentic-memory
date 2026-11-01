use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    },
};
use tower::ServiceExt;
use wam_server::identity::{IdentityError, email::EmailSender};
use wam_server::{
    ApplicationRepository, DemoTokenAuthenticator, FileIdentityStore, IdentityAuthenticator,
    IdentityService, WamService, router_with_identity,
};
const ORIGIN: &str = "https://wam.example";

#[derive(Default)]
struct Mailer {
    codes: Mutex<Vec<String>>,
    fail: bool,
}
#[async_trait::async_trait]
impl EmailSender for Mailer {
    async fn send_code(&self, _: &str, code: &str, _: &str) -> Result<(), IdentityError> {
        self.codes.lock().unwrap().push(code.to_owned());
        if self.fail {
            Err(IdentityError::Provider)
        } else {
            Ok(())
        }
    }
}
struct Rig {
    app: axum::Router,
    mailer: Arc<Mailer>,
    path: std::path::PathBuf,
    requests: AtomicU16,
}
impl Rig {
    fn new(enabled: bool, fail: bool) -> Self {
        let mut bytes = [0; 12];
        getrandom::fill(&mut bytes).unwrap();
        let path = std::env::temp_dir().join(format!("wam-email-http-{:x?}", bytes));
        let repo = ApplicationRepository::open(&path).unwrap();
        let mailer = Arc::new(Mailer {
            fail,
            ..Default::default()
        });
        let mut identity = IdentityService::new(
            Arc::new(FileIdentityStore::from_repository(repo.clone())),
            None,
            ORIGIN,
            true,
        );
        if enabled {
            identity = identity
                .with_email_auth(
                    mailer.clone(),
                    "fixture-secret-at-least-32-bytes-long".into(),
                )
                .unwrap()
                .with_mcp_oauth("https://wam.example/mcp")
                .unwrap();
        }
        let identity = Arc::new(identity);
        let auth = Arc::new(IdentityAuthenticator::new(
            DemoTokenAuthenticator::default(),
            identity.clone(),
        ));
        let app = router_with_identity(
            Arc::new(WamService::durable(repo).unwrap()),
            auth,
            Arc::new(wam_server::http::NoReviewerAuthenticator),
            identity,
        );
        Self {
            app,
            mailer,
            path,
            requests: AtomicU16::new(1),
        }
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: String,
        origin: Option<&str>,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(origin) = origin {
            req = req.header("origin", origin);
        }
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        if let Some(csrf) = csrf {
            req = req.header("x-csrf-token", csrf);
        }
        let mut req = req.body(Body::from(body)).unwrap();
        // Separate synthetic clients keep IP quota tests independent from email quota tests.
        let index = self.requests.fetch_add(1, Ordering::Relaxed);
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from((
            Ipv4Addr::new(127, 0, (index / 250) as u8, (index % 250) as u8),
            12345,
        ))));
        let response = self.app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, headers, body)
    }
    fn code(&self) -> String {
        self.mailer.codes.lock().unwrap().last().unwrap().clone()
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn cookie(headers: &HeaderMap, name: &str) -> String {
    headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|v| v.starts_with(&format!("{name}=")))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}
fn no_store(headers: &HeaderMap) {
    assert_eq!(headers["cache-control"], "no-store");
}

#[tokio::test]
async fn email_auth_disabled_contract() {
    let rig = Rig::new(false, false);
    let (_, _, config) = rig
        .send("GET", "/v1/auth/config", String::new(), None, None, None)
        .await;
    assert_eq!(config["email_auth_enabled"], false);
    assert_eq!(config["github_oauth_enabled"], false);
    let (status, headers, _) = rig
        .send(
            "POST",
            "/v1/auth/email/start",
            json!({"email":"person@example.com"}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(&headers);
}

#[tokio::test]
async fn email_http_login_session_csrf_logout_and_mcp_discovery() {
    let rig = Rig::new(true, false);
    let (_, _, config) = rig
        .send("GET", "/v1/auth/config", String::new(), None, None, None)
        .await;
    assert_eq!(config["email_auth_enabled"], true);
    assert_eq!(config["mcp_oauth_enabled"], true);
    assert_eq!(config["github_oauth_enabled"], false);
    let (status, headers, body) = rig
        .send(
            "POST",
            "/v1/auth/email/start",
            json!({"email":"Person@example.com"}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    no_store(&headers);
    assert_eq!(body, json!({"sent":true,"expires_in":600,"retry_after":60}));
    let challenge_cookie = cookie(&headers, "wam_email_challenge");
    let raw = headers["set-cookie"].to_str().unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/v1/auth/email"] {
        assert!(raw.contains(flag));
    }
    assert!(!raw.contains("Domain="));
    assert!(!raw.contains("wam_session"));
    let code = rig.code();
    assert_eq!(code.len(), 6);
    let (status, headers, _) = rig
        .send(
            "POST",
            "/v1/auth/email/verify",
            json!({"code":code}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!headers.contains_key("set-cookie"));
    no_store(&headers);
    let (status, headers, session) = rig
        .send(
            "POST",
            "/v1/auth/email/verify",
            json!({"code":code}).to_string(),
            Some(ORIGIN),
            Some(&challenge_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    no_store(&headers);
    assert!(
        session["account_id"]
            .as_str()
            .unwrap()
            .starts_with("email_")
    );
    assert!(session.get("email").is_none());
    let session_cookie = cookie(&headers, "wam_session");
    assert_eq!(
        cookie(&headers, "wam_email_challenge"),
        "wam_email_challenge="
    );
    let (status, _, _) = rig
        .send(
            "POST",
            "/v1/auth/email/verify",
            json!({"code":code}).to_string(),
            Some(ORIGIN),
            Some(&challenge_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, info) = rig
        .send(
            "GET",
            "/v1/auth/session",
            String::new(),
            None,
            Some(&session_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["account_id"], session["account_id"]);
    let (status, _, _) = rig
        .send(
            "GET",
            "/v1/me/profile",
            String::new(),
            None,
            Some(&session_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = rig
        .send(
            "POST",
            "/v1/auth/logout",
            String::new(),
            Some(ORIGIN),
            Some(&session_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = rig
        .send(
            "POST",
            "/v1/auth/logout",
            String::new(),
            Some(ORIGIN),
            Some(&session_cookie),
            session["csrf_token"].as_str(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = rig
        .send(
            "GET",
            "/v1/auth/session",
            String::new(),
            None,
            Some(&session_cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn email_rejects_origin_injection_malformed_json_and_oversize_without_secret_reflection() {
    let rig = Rig::new(true, false);
    for origin in [
        None,
        Some("null"),
        Some("https://evil.example"),
        Some("https://wam.example.evil.example"),
    ] {
        for path in ["/v1/auth/email/start", "/v1/auth/email/verify"] {
            let (status, headers, _) = rig
                .send(
                    "POST",
                    path,
                    json!({"email":"a@example.com","code":"123456"}).to_string(),
                    origin,
                    None,
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            no_store(&headers);
        }
    }
    for email in [
        "bad",
        "a@example.com\r\nBcc: evil@example.com",
        "a@example.com,b@example.com",
    ] {
        let (status, headers, body) = rig
            .send(
                "POST",
                "/v1/auth/email/start",
                json!({"email":email}).to_string(),
                Some(ORIGIN),
                None,
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        no_store(&headers);
        assert!(!body.to_string().contains(email));
    }
    for (body, expected) in [
        (
            json!({"email":"a@example.com","private-sentinel":"secret"}).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ("{".into(), StatusCode::BAD_REQUEST),
        (
            json!({"email":"a".repeat(5000)}).to_string(),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let (status, headers, reply) = rig
            .send(
                "POST",
                "/v1/auth/email/start",
                body,
                Some(ORIGIN),
                None,
                None,
            )
            .await;
        assert_eq!(status, expected);
        no_store(&headers);
        assert!(!reply.to_string().contains("private-sentinel"));
    }
    assert!(rig.mailer.codes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn email_delivery_failure_rate_limit_and_duplicate_cookies_are_safe() {
    let failed = Rig::new(true, true);
    let (status, headers, body) = failed
        .send(
            "POST",
            "/v1/auth/email/start",
            json!({"email":"a@example.com"}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(&headers);
    assert!(!headers.contains_key("set-cookie"));
    assert_eq!(body["error"], "service_unavailable");
    let rig = Rig::new(true, false);
    let (_, headers, _) = rig
        .send(
            "POST",
            "/v1/auth/email/start",
            json!({"email":"a@example.com"}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    let challenge = cookie(&headers, "wam_email_challenge");
    let (status, headers, _) = rig
        .send(
            "POST",
            "/v1/auth/email/start",
            json!({"email":"A@EXAMPLE.COM"}).to_string(),
            Some(ORIGIN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.contains_key("retry-after"));
    no_store(&headers);
    let (status, _, _) = rig
        .send(
            "POST",
            "/v1/auth/email/verify",
            json!({"code":rig.code()}).to_string(),
            Some(ORIGIN),
            Some(&format!("{challenge}; {challenge}")),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for _ in 0..5 {
        let (status, _, _) = rig
            .send(
                "POST",
                "/v1/auth/email/verify",
                json!({"code":"bad"}).to_string(),
                Some(ORIGIN),
                Some(&challenge),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, headers, _) = rig
        .send(
            "POST",
            "/v1/auth/email/verify",
            json!({"code":rig.code()}).to_string(),
            Some(ORIGIN),
            Some(&challenge),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    no_store(&headers);
}

#[test]
fn email_only_production_env_child() {
    if std::env::var_os("WAM_TEST_EMAIL_ENV_CHILD").is_none() {
        return;
    }
    let path = std::env::temp_dir()
        .join(format!("wam-email-env-{}", std::process::id()))
        .join("identity.json");
    let store = Arc::new(FileIdentityStore::open(&path).unwrap());
    let result = IdentityService::from_env(store);
    if std::env::var_os("WAM_TEST_EMAIL_ENV_INVALID").is_some() {
        assert!(result.is_err());
    } else if std::env::var_os("WAM_TEST_EMAIL_DIAGNOSTICS_ONLY").is_some() {
        let identity = result.unwrap();
        assert!(!identity.email_auth_enabled());
        assert!(identity.oauth_enabled());
        assert!(identity.mcp_oauth_config().is_ok());
    } else {
        let identity = result.unwrap();
        assert!(identity.email_auth_enabled());
        assert!(!identity.oauth_enabled());
        assert!(identity.mcp_oauth_config().is_ok());
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn email_only_production_and_partial_configuration_contract() {
    for case in [
        "valid",
        "diagnostics_only",
        "missing_from",
        "short_secret",
        "empty_key",
        "bad_from",
        "origin_query",
        "origin_credentials",
        "origin_fragment",
        "github_callback_query",
        "github_callback_credentials",
        "github_callback_fragment",
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "email_only_production_env_child"])
            .env("WAM_TEST_EMAIL_ENV_CHILD", "1")
            .env("WAM_ENV", "production")
            .env("WAM_PUBLIC_ORIGIN", ORIGIN)
            .env("WAM_MCP_PUBLIC_URL", "https://wam.example/mcp")
            .env("RESEND_API_KEY", "fixture-key")
            .env("WAM_EMAIL_FROM", "WAM <noreply@example.com>")
            .env(
                "WAM_EMAIL_IDENTITY_SECRET",
                "fixture-secret-at-least-32-bytes-long",
            )
            .env_remove("WAM_GITHUB_CLIENT_ID")
            .env_remove("WAM_GITHUB_CLIENT_SECRET")
            .env_remove("WAM_GITHUB_REDIRECT_URI")
            .env_remove("WAM_DEMO_TOKEN")
            .env_remove("WAM_DIAGNOSTICS_EMAIL_TO")
            .env_remove("WAM_TEST_EMAIL_DIAGNOSTICS_ONLY");
        if case == "diagnostics_only" {
            command
                .env("WAM_TEST_EMAIL_DIAGNOSTICS_ONLY", "1")
                .env("WAM_DIAGNOSTICS_EMAIL_TO", "easycodingkr@gmail.com")
                .env("WAM_GITHUB_CLIENT_ID", "fixture-client")
                .env("WAM_GITHUB_CLIENT_SECRET", "fixture-secret")
                .env(
                    "WAM_GITHUB_REDIRECT_URI",
                    "https://wam.example/v1/auth/github/callback",
                )
                .env_remove("WAM_EMAIL_IDENTITY_SECRET");
        }
        if case != "valid" && case != "diagnostics_only" {
            command.env("WAM_TEST_EMAIL_ENV_INVALID", "1");
            match case {
                "missing_from" => {
                    command.env_remove("WAM_EMAIL_FROM");
                }
                "short_secret" => {
                    command.env("WAM_EMAIL_IDENTITY_SECRET", "short");
                }
                "empty_key" => {
                    command.env("RESEND_API_KEY", "  ");
                }
                "bad_from" => {
                    command.env(
                        "WAM_EMAIL_FROM",
                        "sender@example.com\r\nBcc: person@example.com",
                    );
                }
                "origin_query" => {
                    command.env("WAM_PUBLIC_ORIGIN", "https://wam.example/?secret=value");
                }
                "origin_credentials" => {
                    command.env("WAM_PUBLIC_ORIGIN", "https://user:secret@wam.example");
                }
                "origin_fragment" => {
                    command.env("WAM_PUBLIC_ORIGIN", "https://wam.example/#secret");
                }
                "github_callback_query" => {
                    command
                        .env("WAM_GITHUB_CLIENT_ID", "fixture-client")
                        .env("WAM_GITHUB_CLIENT_SECRET", "fixture-secret")
                        .env(
                            "WAM_GITHUB_REDIRECT_URI",
                            "https://wam.example/v1/auth/github/callback?secret=value",
                        );
                }
                "github_callback_credentials" => {
                    command
                        .env("WAM_GITHUB_CLIENT_ID", "fixture-client")
                        .env("WAM_GITHUB_CLIENT_SECRET", "fixture-secret")
                        .env(
                            "WAM_GITHUB_REDIRECT_URI",
                            "https://user:secret@wam.example/v1/auth/github/callback",
                        );
                }
                "github_callback_fragment" => {
                    command
                        .env("WAM_GITHUB_CLIENT_ID", "fixture-client")
                        .env("WAM_GITHUB_CLIENT_SECRET", "fixture-secret")
                        .env(
                            "WAM_GITHUB_REDIRECT_URI",
                            "https://wam.example/v1/auth/github/callback#secret",
                        );
                }
                _ => unreachable!(),
            }
        } else {
            command.env_remove("WAM_TEST_EMAIL_ENV_INVALID");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn email_accounts_also_create_private_in_memory_profiles() {
    use wam_server::ports::Repository;
    let repository = wam_server::InMemoryRepository::seeded();
    repository.ensure_account("email_fixture_account").unwrap();
    repository.ensure_account("email_fixture_account").unwrap();
    assert!(
        repository
            .ensure_account("arbitrary_untrusted_account")
            .is_err()
    );
}

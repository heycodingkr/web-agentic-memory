use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use url::Url;
use wam_server::identity::{
    GitHubOAuthProvider, IdentityError,
    oauth::{OAuthError, TokenInput},
};
use wam_server::{
    ApplicationRepository, DemoTokenAuthenticator, FileIdentityStore, IdentityAuthenticator,
    IdentityService, WamService, router_with_identity,
};
const RESOURCE: &str = "https://wam.example/mcp";
const REDIRECT: &str = "http://127.0.0.1:49152/callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        let mut random = [0; 12];
        getrandom::fill(&mut random).unwrap();
        let nonce: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let dir = std::env::temp_dir().join(format!("wam-oauth-{nonce}"));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Provider;
#[async_trait::async_trait]
impl GitHubOAuthProvider for Provider {
    fn authorization_url(&self, state: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("https://github.test/authorize?state={state}"))
    }
    async fn exchange_code(&self, code: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("fixture-{code}"))
    }
}
struct Session {
    cookie: String,
    csrf: String,
}
struct Rig {
    app: axum::Router,
    identity: Arc<IdentityService>,
    path: PathBuf,
}
struct Grant {
    client: String,
    code: String,
    access: String,
    refresh: String,
}
impl Rig {
    fn new(path: &Path, resource: &str) -> Self {
        let repo = ApplicationRepository::open(path).unwrap();
        let identity = Arc::new(
            IdentityService::new(
                Arc::new(FileIdentityStore::from_repository(repo.clone())),
                Some(Arc::new(Provider)),
                "https://wam.example",
                true,
            )
            .with_mcp_oauth(resource)
            .unwrap(),
        );
        let auth = Arc::new(IdentityAuthenticator::new(
            DemoTokenAuthenticator::default(),
            Arc::clone(&identity),
        ));
        let app = router_with_identity(
            Arc::new(WamService::durable(repo).unwrap()),
            auth,
            Arc::new(wam_server::http::NoReviewerAuthenticator),
            Arc::clone(&identity),
        );
        Self {
            app,
            identity,
            path: path.into(),
        }
    }
    async fn login(&self, subject: &str) -> Session {
        let (state, _) = self.identity.begin_oauth(now()).unwrap();
        let (_, token, csrf, _) = self
            .identity
            .finish_oauth(subject, &state, &state, now())
            .await
            .unwrap();
        Session {
            cookie: format!("wam_session={token}"),
            csrf,
        }
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: String,
        session: Option<&Session>,
        token: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(
                "content-type",
                if path == "/oauth/token" || path == "/oauth/revoke" {
                    "application/x-www-form-urlencoded"
                } else {
                    "application/json"
                },
            )
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25");
        if let Some(s) = session {
            req = req
                .header("cookie", &s.cookie)
                .header("x-csrf-token", &s.csrf);
        }
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        // Functional OAuth cases model independent callers; quota behavior has its own test below.
        static NEXT_IP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let sequence = NEXT_IP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let peer =
            std::net::SocketAddr::from((std::net::Ipv4Addr::from(0xc6120000 + sequence), 50000));
        req = req.extension(axum::extract::ConnectInfo(peer));
        let response = self
            .app
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            headers,
            if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            },
        )
    }
    async fn register(&self) -> String {
        let (s, _, v) = self
            .send(
                "POST",
                "/oauth/register",
                json!({"client_name":"Fixture host",
            "redirect_uris":["http://127.0.0.1/callback"],"token_endpoint_auth_method":"none",
            "grant_types":["authorization_code","refresh_token"],"response_types":["code"]})
                .to_string(),
                None,
                None,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED);
        v["client_id"].as_str().unwrap().into()
    }
    async fn start(&self, client: &str, scope: &str) -> String {
        let (s, h, _) = self
            .send(
                "GET",
                &authorization_query(client, scope),
                String::new(),
                None,
                None,
            )
            .await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        Url::parse(h["location"].to_str().unwrap())
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "request")
            .unwrap()
            .1
            .into_owned()
    }
    async fn approve(&self, request: &str, session: &Session) -> String {
        let path = format!("/v1/oauth/requests/{request}");
        let (s, _, v) = self
            .send("GET", &path, String::new(), Some(session), None)
            .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["client_name"], "Fixture host");
        let (s, _, v) = self
            .send(
                "POST",
                &(path + "/decision"),
                r#"{"approve":true}"#.into(),
                Some(session),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        let callback = Url::parse(v["redirect_uri"].as_str().unwrap()).unwrap();
        let pairs: std::collections::HashMap<_, _> = callback
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(pairs["iss"], "https://wam.example");
        assert_eq!(pairs["state"], "fixture-client-state");
        pairs["code"].clone()
    }
    async fn exchange(
        &self,
        client: &str,
        code: &str,
        verifier: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        self.send(
            "POST",
            "/oauth/token",
            form(&[
                ("grant_type", "authorization_code"),
                ("client_id", client),
                ("resource", RESOURCE),
                ("redirect_uri", REDIRECT),
                ("code", code),
                ("code_verifier", verifier),
            ]),
            None,
            None,
        )
        .await
    }
    async fn refresh(&self, client: &str, token: &str) -> (StatusCode, HeaderMap, Value) {
        self.send(
            "POST",
            "/oauth/token",
            form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client),
                ("resource", RESOURCE),
                ("refresh_token", token),
            ]),
            None,
            None,
        )
        .await
    }
    async fn grant(&self, session: &Session, scope: &str) -> Grant {
        let client = self.register().await;
        let request = self.start(&client, scope).await;
        let code = self.approve(&request, session).await;
        let (s, h, v) = self.exchange(&client, &code, VERIFIER).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h["cache-control"], "no-store");
        assert_eq!(v["expires_in"], 3600);
        Grant {
            client,
            code,
            access: v["access_token"].as_str().unwrap().into(),
            refresh: v["refresh_token"].as_str().unwrap().into(),
        }
    }
    async fn tool(&self, name: &str, token: Option<&str>) -> (StatusCode, HeaderMap, Value) {
        self.send("POST","/mcp",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":{}}}).to_string(),None,token).await
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn form(values: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(values.iter().copied())
        .finish()
}
fn authorization_query(client: &str, scope: &str) -> String {
    let mut u = Url::parse("https://wam.example/oauth/authorize").unwrap();
    u.query_pairs_mut().extend_pairs([
        ("client_id", client),
        ("redirect_uri", REDIRECT),
        ("response_type", "code"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
        ("resource", RESOURCE),
        ("scope", scope),
        ("state", "fixture-client-state"),
    ]);
    format!("{}?{}", u.path(), u.query().unwrap())
}

#[tokio::test]
async fn discovery_and_challenges_use_the_configured_resource_and_issuer() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let (s, _, v) = r
        .send(
            "GET",
            "/.well-known/oauth-protected-resource/mcp",
            String::new(),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["resource"], RESOURCE);
    assert_eq!(v["authorization_servers"][0], "https://wam.example");
    let (_, h, v) = r
        .send(
            "GET",
            "/.well-known/oauth-authorization-server",
            String::new(),
            None,
            None,
        )
        .await;
    assert_eq!(h["access-control-allow-origin"], "*");
    assert_eq!(v["code_challenge_methods_supported"][0], "S256");
    assert_eq!(v["token_endpoint_auth_methods_supported"][0], "none");
    assert_eq!(v["authorization_response_iss_parameter_supported"], true);
    let (s, h, _) = r.tool("web_memory_my_rank", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(h["www-authenticate"].to_str().unwrap().contains(
        "resource_metadata=\"https://wam.example/.well-known/oauth-protected-resource/mcp\""
    ));
}

#[tokio::test]
async fn pkce_grants_only_approved_scopes_and_stores_no_raw_credentials() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let g = r.grant(&owner, "read:dashboard").await;
    let (s, _, v) = r.tool("web_memory_my_rank", Some(&g.access)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["result"]["isError"], false);
    let (s, h, _) = r.tool("web_memory_propose", Some(&g.access)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(
        h["www-authenticate"]
            .to_str()
            .unwrap()
            .contains("insufficient_scope")
    );
    let stored = std::fs::read_to_string(&r.path).unwrap();
    for secret in [&g.code, &g.access, &g.refresh, VERIFIER] {
        assert!(!stored.contains(secret));
    }
    assert!(
        r.identity
            .authenticate_device(&g.access, now() + 3601)
            .is_none()
    );
}

#[tokio::test]
async fn unsafe_redirects_resources_scopes_and_pkce_downgrade_are_rejected() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    for redirect in [
        "http://external.example/callback",
        "javascript:alert(1)",
        "https://user:secret@example.com/callback",
        "https://example.com/cb#fragment",
    ] {
        assert_eq!(
            r.send(
                "POST",
                "/oauth/register",
                json!({"redirect_uris":[redirect]}).to_string(),
                None,
                None
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let client = r.register().await;
    let original = authorization_query(&client, "read:dashboard");
    for (a, b) in [
        ("S256", "plain"),
        ("127.0.0.1", "localhost"),
        ("callback", "different"),
        ("read%3Adashboard", "manage%3Aaccount"),
        ("wam.example%2Fmcp", "other.example%2Fmcp"),
    ] {
        assert_eq!(
            r.send("GET", &original.replace(a, b), String::new(), None, None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn approval_requires_web_session_csrf_and_the_preview_owner() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let other = r.login("other").await;
    let client = r.register().await;
    let request = r.start(&client, "read:dashboard").await;
    let p = format!("/v1/oauth/requests/{request}");
    assert_eq!(
        r.send("GET", &p, String::new(), None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.send("GET", &p, String::new(), Some(&owner), None).await.0,
        StatusCode::OK
    );
    let bad = Session {
        cookie: owner.cookie.clone(),
        csrf: "wrong".into(),
    };
    assert_eq!(
        r.send(
            "POST",
            &(p.clone() + "/decision"),
            r#"{"approve":true}"#.into(),
            Some(&bad),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        r.send("GET", &p, String::new(), Some(&other), None).await.0,
        StatusCode::BAD_REQUEST
    );
    let code = r.approve(&request, &owner).await;
    assert_eq!(r.exchange(&client, &code, VERIFIER).await.0, StatusCode::OK);
    assert_eq!(
        r.send(
            "POST",
            &(p + "/decision"),
            r#"{"approve":true}"#.into(),
            Some(&owner),
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn pkce_failure_consumes_the_code_and_code_replay_revokes_its_grant() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let client = r.register().await;
    let request = r.start(&client, "read:memories").await;
    let code = r.approve(&request, &owner).await;
    assert_eq!(
        r.exchange(&client, &code, &"x".repeat(43)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        r.exchange(&client, &code, VERIFIER).await.0,
        StatusCode::BAD_REQUEST
    );
    let g = r.grant(&owner, "read:dashboard").await;
    assert_eq!(
        r.exchange(&g.client, &g.code, VERIFIER).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.refresh(&g.client, &g.refresh).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn refresh_reuse_revokes_the_whole_family_after_restart() {
    let dir = Temporary::new();
    let path = dir.0.join("state.json");
    let r = Rig::new(&path, RESOURCE);
    let owner = r.login("owner").await;
    let g = r.grant(&owner, "read:dashboard").await;
    let (s, _, v) = r.refresh(&g.client, &g.refresh).await;
    assert_eq!(s, StatusCode::OK);
    let access = v["access_token"].as_str().unwrap().to_owned();
    let refresh = v["refresh_token"].as_str().unwrap().to_owned();
    assert!(access != g.access && refresh != g.refresh);
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&access)).await.0,
        StatusCode::OK
    );
    drop(r);
    let r = Rig::new(&path, RESOURCE);
    assert_eq!(
        r.refresh(&g.client, &g.refresh).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&access)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.refresh(&g.client, &refresh).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn web_device_revoke_invalidates_access_and_refresh_together() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let g = r.grant(&owner, "read:dashboard").await;
    let (s, _, v) = r
        .send("GET", "/v1/me/devices", String::new(), Some(&owner), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let device = v["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["oauth_managed"] == true)
        .unwrap();
    let p = format!("/v1/me/devices/{}/revoke", device["id"].as_str().unwrap());
    assert_eq!(
        r.send("POST", &p, String::new(), Some(&owner), None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        r.refresh(&g.client, &g.refresh).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn oauth_revocation_is_idempotent_and_account_deletion_removes_grants() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let g = r.grant(&owner, "read:dashboard").await;
    for token in [&g.refresh, "unknown"] {
        assert_eq!(
            r.send(
                "POST",
                "/oauth/revoke",
                form(&[("client_id", &g.client), ("token", token)]),
                None,
                None
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
    let g = r.grant(&owner, "read:dashboard").await;
    assert_eq!(
        r.send(
            "DELETE",
            "/v1/me/account",
            r#"{"confirmation":"DELETE"}"#.into(),
            Some(&owner),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        r.refresh(&g.client, &g.refresh).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        r.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn audience_binding_survives_reopen_and_expired_codes_are_not_redeemed() {
    let dir = Temporary::new();
    let path = dir.0.join("state.json");
    let r = Rig::new(&path, RESOURCE);
    let owner = r.login("owner").await;
    let client = r.register().await;
    let request = r.start(&client, "read:memories").await;
    let code = r.approve(&request, &owner).await;
    let input = TokenInput {
        grant_type: "authorization_code".into(),
        client_id: client,
        resource: RESOURCE.into(),
        code: Some(code),
        redirect_uri: Some(REDIRECT.into()),
        code_verifier: Some(VERIFIER.into()),
        refresh_token: None,
        scope: None,
        client_secret: None,
    };
    assert!(matches!(
        r.identity.exchange_mcp_token(input, now() + 61),
        Err(OAuthError::InvalidGrant)
    ));
    let g = r.grant(&owner, "read:dashboard").await;
    drop(r);
    let other = Rig::new(&path, "https://other.example/mcp");
    assert_eq!(
        other.tool("web_memory_my_rank", Some(&g.access)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        other.refresh(&g.client, &g.refresh).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn oauth_public_endpoints_are_rate_limited() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let mut denied = false;
    for _ in 0..12 {
        let request = Request::post("/oauth/register")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"redirect_uris":["http://127.0.0.1/callback"]}"#,
            ))
            .unwrap();
        let response = r.app.clone().oneshot(request).await.unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(response.headers().contains_key("retry-after"));
            denied = true;
            break;
        }
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    assert!(denied);
}

#[tokio::test]
async fn code_is_bound_to_the_client_redirect_and_resource() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let client = r.register().await;
    let other = r.register().await;
    let request = r.start(&client, "read:dashboard").await;
    let code = r.approve(&request, &owner).await;
    assert_eq!(
        r.exchange(&other, &code, VERIFIER).await.0,
        StatusCode::BAD_REQUEST
    );
    for (resource, redirect) in [
        (RESOURCE, "http://127.0.0.1:49152/other"),
        ("https://other.example/mcp", REDIRECT),
    ] {
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client),
            ("resource", resource),
            ("redirect_uri", redirect),
            ("code", &code),
            ("code_verifier", VERIFIER),
        ]);
        assert_eq!(
            r.send("POST", "/oauth/token", body, None, None).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(r.exchange(&client, &code, VERIFIER).await.0, StatusCode::OK);
}

#[tokio::test]
async fn github_login_returns_to_the_bound_consent_request_without_storing_its_secret() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let client = r.register().await;
    let request = r.start(&client, "read:dashboard").await;
    let (s, h, _) = r
        .send(
            "GET",
            &format!("/v1/auth/github/start?oauth_request={request}"),
            String::new(),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FOUND);
    let cookies = h
        .get_all("set-cookie")
        .iter()
        .map(|h| h.to_str().unwrap().split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ");
    let upstream = Url::parse(h["location"].to_str().unwrap()).unwrap();
    let state = upstream
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    assert!(!std::fs::read_to_string(&r.path).unwrap().contains(&request));
    let session = Session {
        cookie: cookies,
        csrf: String::new(),
    };
    let (s, h, _) = r
        .send(
            "GET",
            &format!("/v1/auth/github/callback?code=fixture-login&state={state}"),
            String::new(),
            Some(&session),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FOUND);
    assert_eq!(
        h["location"],
        format!("https://wam.example/oauth/approve?request={request}")
    );
}

#[tokio::test]
async fn github_login_returns_to_the_bound_device_link_code_without_offsite_redirects() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let (status, headers, _) = r
        .send(
            "GET",
            "/v1/auth/github/start?link_code=ABCD-EFGH",
            String::new(),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FOUND);
    let cookies = headers
        .get_all("set-cookie")
        .iter()
        .map(|header| header.to_str().unwrap().split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ");
    assert!(cookies.contains("wam_oauth_return=device:ABCD-EFGH"));
    assert!(
        !std::fs::read_to_string(&r.path)
            .unwrap()
            .contains("ABCD-EFGH")
    );
    let upstream = Url::parse(headers["location"].to_str().unwrap()).unwrap();
    let state = upstream
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();

    let (status, response_headers, _) = r
        .send(
            "GET",
            &format!("/v1/auth/github/callback?code=fixture-login&state={state}"),
            String::new(),
            Some(&Session {
                cookie: cookies,
                csrf: String::new(),
            }),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FOUND);
    assert_eq!(
        response_headers["location"],
        "https://wam.example/link?code=ABCD-EFGH"
    );
    assert!(
        response_headers
            .get_all("set-cookie")
            .iter()
            .filter_map(|header| header.to_str().ok())
            .any(|header| header.starts_with("wam_oauth_return=;"))
    );

    let (status, _, _) = r
        .send(
            "GET",
            "/v1/auth/github/start?link_code=https%3A%2F%2Fevil.example",
            String::new(),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn failed_persistence_does_not_consume_the_code_or_issue_credentials() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let client = r.register().await;
    let request = r.start(&client, "read:dashboard").await;
    let code = r.approve(&request, &owner).await;
    let backup = dir.0.join("saved.json");
    std::fs::rename(&r.path, &backup).unwrap();
    std::fs::create_dir(&r.path).unwrap();
    let (s, _, value) = r.exchange(&client, &code, VERIFIER).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(value.get("access_token").is_none());
    std::fs::remove_dir(&r.path).unwrap();
    std::fs::rename(backup, &r.path).unwrap();
    assert_eq!(r.exchange(&client, &code, VERIFIER).await.0, StatusCode::OK);
}

#[tokio::test]
async fn identical_resource_indicators_are_accepted_but_other_duplicate_parameters_are_rejected() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let client = r.register().await;
    let query =
        authorization_query(&client, "read:dashboard") + "&" + &form(&[("resource", RESOURCE)]);
    let (s, h, _) = r.send("GET", &query, String::new(), None, None).await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let request = Url::parse(h["location"].to_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "request")
        .unwrap()
        .1
        .into_owned();
    let code = r.approve(&request, &owner).await;
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client),
        ("resource", RESOURCE),
        ("resource", RESOURCE),
        ("redirect_uri", REDIRECT),
        ("code", &code),
        ("code_verifier", VERIFIER),
    ]);
    assert_eq!(
        r.send("POST", "/oauth/token", body, None, None).await.0,
        StatusCode::OK
    );
    for extra in [
        form(&[("resource", "https://other.example/mcp")]),
        form(&[("state", "extra")]),
    ] {
        let query = authorization_query(&client, "read:dashboard") + "&" + &extra;
        assert_eq!(
            r.send("GET", &query, String::new(), None, None).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn authenticated_remote_usage_returns_the_same_owner_quota_contract_as_http() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let owner = r.login("owner").await;
    let g = r.grant(&owner, "read:dashboard").await;
    let (s, _, value) = r.tool("web_memory_my_usage", Some(&g.access)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(value["result"]["isError"], false);
    let usage: Value =
        serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let (s, _, http) = r
        .send("GET", "/v1/me/usage", String::new(), None, Some(&g.access))
        .await;
    assert_eq!(s, StatusCode::OK);
    for key in [
        "schema_version",
        "period_days",
        "limit_per_minute",
        "burst_capacity",
    ] {
        assert_eq!(usage[key], http[key]);
    }
    assert!(
        usage["daily"]
            .as_array()
            .unwrap()
            .iter()
            .any(|day| day["requests"].as_u64().unwrap() > 0)
    );
    assert!(!usage.to_string().contains(&g.access));
}

#[tokio::test]
async fn malformed_json_never_echoes_input() {
    let dir = Temporary::new();
    let r = Rig::new(&dir.0.join("state.json"), RESOURCE);
    let (s, _, v) = r
        .send(
            "POST",
            "/oauth/register",
            r#"{"redirect_uris":"sentinel-secret"}"#.into(),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(!v.to_string().contains("sentinel-secret"));
}

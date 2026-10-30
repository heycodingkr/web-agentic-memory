use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_contracts::CONTRIBUTION_CONSENT_VERSION;
use wam_server::{
    InMemoryRepository, WamService,
    http::Authenticator,
    identity::{SCOPE_READ_DASHBOARD, SCOPE_READ_MEMORIES, SCOPE_WRITE_CONTRIBUTIONS},
    mcp_transport::{McpOriginAllowlist, router_with_origins},
    router,
};

#[derive(Clone)]
struct TestAuthenticator;

impl Authenticator for TestAuthenticator {
    fn account_id(&self, headers: &HeaderMap) -> Option<String> {
        let bearer = headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;
        match bearer {
            "reader-token" => Some("github_reader".to_owned()),
            "dashboard-token" => Some("github_dashboard".to_owned()),
            "outcome-token" => Some("github_outcome".to_owned()),
            "other-outcome-token" => Some("github_other_outcome".to_owned()),
            "writer-token" | "browser-token" => Some("github_writer".to_owned()),
            _ => None,
        }
    }

    fn has_scope(&self, headers: &HeaderMap, scope: &str) -> bool {
        let Some(bearer) = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
        else {
            return false;
        };
        match bearer {
            "reader-token" => scope == SCOPE_READ_MEMORIES,
            "dashboard-token" => scope == SCOPE_READ_MEMORIES || scope == SCOPE_READ_DASHBOARD,
            "outcome-token" | "other-outcome-token" => {
                scope == SCOPE_READ_MEMORIES || scope == wam_server::identity::SCOPE_REPORT_OUTCOME
            }
            "browser-token" => {
                scope == SCOPE_READ_MEMORIES
                    || scope == SCOPE_WRITE_CONTRIBUTIONS
                    || scope == wam_server::identity::SCOPE_REPORT_OUTCOME
            }
            "writer-token" => scope == SCOPE_READ_MEMORIES || scope == SCOPE_WRITE_CONTRIBUTIONS,
            _ => false,
        }
    }

    fn browser_session(&self, headers: &HeaderMap) -> bool {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == "Bearer browser-token")
    }

    fn csrf_valid(&self, headers: &HeaderMap) -> bool {
        headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            == Some("csrf-ok")
    }
}

fn app() -> Router {
    let service = Arc::new(WamService::new(InMemoryRepository::seeded()));
    for account in [
        "github_reader",
        "github_dashboard",
        "github_writer",
        "github_outcome",
        "github_other_outcome",
    ] {
        service
            .ensure_account(account)
            .expect("provision test account");
    }
    router(service, Arc::new(TestAuthenticator))
}

fn rpc(method: &str, id: u64, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

fn call_tool(name: &str, arguments: Value) -> Value {
    rpc("tools/call", 7, json!({"name":name,"arguments":arguments}))
}

fn tool_value(response: &Value) -> Value {
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool result text"),
    )
    .expect("tool result JSON")
}

async fn send(
    app: &Router,
    message: Value,
    bearer: Option<&str>,
    origin: Option<&str>,
    csrf: Option<&str>,
    accept: &str,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = Request::post("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, accept)
        .header("mcp-protocol-version", "2025-03-26");
    if let Some(bearer) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    if let Some(csrf) = csrf {
        request = request.header("x-csrf-token", csrf);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(message.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, headers, value)
}

#[tokio::test]
async fn browser_mcp_preflight_and_challenges_follow_the_exact_origin_allowlist() {
    let app = router_with_origins(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        Arc::new(TestAuthenticator),
        McpOriginAllowlist::new(["https://host.example"]),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/mcp")
                .header("origin", "https://host.example")
                .header("access-control-request-method", "POST")
                .header(
                    "access-control-request-headers",
                    "authorization, content-type, mcp-protocol-version",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://host.example"
    );
    assert_eq!(
        response.headers()["access-control-allow-methods"],
        "GET, POST"
    );
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-credentials")
    );
    for (origin, requested_header) in [
        ("https://evil.example", "authorization"),
        ("https://host.example", "x-unapproved"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/mcp")
                    .header("origin", origin)
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", requested_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
    let (status, headers, _) = send(
        &app,
        call_tool("web_memory_my_rank", json!({})),
        None,
        Some("https://host.example"),
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        headers["access-control-allow-origin"],
        "https://host.example"
    );
    assert!(
        headers["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .contains("WWW-Authenticate")
    );
    assert!(headers.contains_key("www-authenticate"));
    let (status, headers, _) = send(
        &app,
        rpc("tools/list", 1, json!({})),
        None,
        Some("https://host.example"),
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers["access-control-allow-origin"],
        "https://host.example"
    );
}

#[tokio::test]
async fn streamable_http_sse_get_and_post_preserve_protocol_and_access_checks() {
    let app = app();
    let response = app
        .clone()
        .oneshot(
            Request::get("/mcp")
                .header(header::ACCEPT, "text/event-stream")
                .header("mcp-protocol-version", "2025-11-25")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let mut body = response.into_body();
    let first = tokio::time::timeout(std::time::Duration::from_secs(1), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        std::str::from_utf8(first.data_ref().unwrap())
            .unwrap()
            .starts_with(':')
    );
    drop(body);
    let response = app
        .clone()
        .oneshot(
            Request::post("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    header::ACCEPT,
                    "application/json;q=0.5, text/event-stream;q=1",
                )
                .body(Body::from(rpc("tools/list", 22, json!({})).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = std::str::from_utf8(&body).unwrap();
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap();
    let message: Value = serde_json::from_str(data).unwrap();
    assert_eq!(message["id"], 22);
    assert_eq!(message["result"]["tools"].as_array().unwrap().len(), 10);
    for (header_name, value, expected) in [
        ("origin", "https://untrusted.example", StatusCode::FORBIDDEN),
        (
            "authorization",
            "Bearer expired-token",
            StatusCode::UNAUTHORIZED,
        ),
        (
            "mcp-protocol-version",
            "unsupported-version",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .header(header_name, value)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn remote_expand_pins_the_requested_public_version_and_status_has_safe_metadata() {
    let app = app();
    let (_, _, recalled) = send(
        &app,
        call_tool(
            "web_memory_recall",
            json!({"url":"https://example.com/guide","task_category":"compose-draft"}),
        ),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    let recalled = tool_value(&recalled);
    let hit = &recalled["hits"][0];
    assert!(hit["memory_id"].is_string());
    let (_, _, expanded) = send(
        &app,
        call_tool(
            "web_memory_expand",
            json!({"memory_id":hit["memory_id"],"version_id":hit["version_id"],"level":2}),
        ),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(expanded["result"]["isError"], false, "{expanded}");
    assert_eq!(tool_value(&expanded)["version_id"], hit["version_id"]);
    let (_, _, mismatched) = send(
        &app,
        call_tool(
            "web_memory_expand",
            json!({"memory_id":hit["memory_id"],"version_id":"unavailable-version","level":2}),
        ),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(mismatched["result"]["isError"], true);
    let (_, _, status) = send(
        &app,
        call_tool("web_memory_status", json!({})),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    let status = tool_value(&status);
    assert_eq!(status["auth"], "anonymous");
    assert_eq!(status["consent"], "not_linked");
    assert_eq!(status["endpoint"], "/mcp");
    assert!(status["catalog_version"].is_u64());
    for private_field in [
        "token",
        "access_token",
        "account_id",
        "user_id",
        "email",
        "scopes",
        "device_id",
    ] {
        assert!(
            status.get(private_field).is_none(),
            "unexpected {private_field}"
        );
    }

    let (_, _, linked) = send(
        &app,
        call_tool("web_memory_status", json!({})),
        Some("reader-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    let linked = tool_value(&linked);
    assert_eq!(linked["auth"], "linked");
    assert_eq!(linked["consent"], "unknown");
    assert_eq!(
        linked["catalog_version"],
        wam_contracts::MCP_CATALOG_VERSION
    );
}

#[tokio::test]
async fn remote_streamable_http_matches_stdio_tools_and_enforces_scopes() {
    let app = app();

    let (status, headers, initialized) = send(
        &app,
        rpc(
            "initialize",
            1,
            json!({
                "protocolVersion":"2025-03-26",
                "capabilities":{},
                "clientInfo":{"name":"wam-test","version":"1"}
            }),
        ),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(initialized["result"]["protocolVersion"], "2025-03-26");

    let (status, _, listed) = send(
        &app,
        rpc("tools/list", 2, json!({})),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let remote_names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    let stdio_catalog = wam_mcp::tool_catalog();
    let stdio_names = stdio_catalog
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(remote_names, stdio_names);

    let (status, _, public_read) = send(
        &app,
        call_tool("web_memory_leaderboard", json!({})),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public_read["result"]["isError"], false);

    let (status, _, private_read) = send(
        &app,
        call_tool("web_memory_my_rank", json!({})),
        Some("reader-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(private_read["error"], "insufficient_scope");

    let (status, _, authorized_read) = send(
        &app,
        call_tool("web_memory_my_rank", json!({})),
        Some("dashboard-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(authorized_read["result"]["isError"], false);

    let (status, _, denied_write) = send(
        &app,
        call_tool("web_memory_my_contributions", json!({})),
        Some("reader-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(denied_write["error"], "insufficient_scope");
}

#[tokio::test]
async fn remote_streamable_http_rejects_invalid_credentials_and_csrf() {
    let app = app();

    let (status, headers, body) = send(
        &app,
        call_tool("web_memory_status", json!({})),
        Some("revoked-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(body["error"], "invalid_credential");

    let proposal = call_tool(
        "web_memory_propose",
        json!({
            "url":"https://example.com/guide",
            "task_category":"navigate",
            "actions":["navigate"],
            "consent":true,
            "consent_version":CONTRIBUTION_CONSENT_VERSION
        }),
    );
    let (status, _, body) = send(
        &app,
        proposal.clone(),
        Some("browser-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "csrf_failed");

    let (status, _, body) = send(
        &app,
        proposal,
        Some("browser-token"),
        None,
        Some("csrf-ok"),
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false);

    let (status, _, denied) = send(
        &app,
        call_tool("web_memory_propose", json!({})),
        Some("reader-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(denied["error"], "insufficient_scope");
}

#[tokio::test]
async fn remote_outcome_requires_scope_and_owner_and_is_idempotent() {
    let app = app();
    let recall = call_tool(
        "web_memory_recall",
        json!({
            "url":"https://example.com/guide/editor",
            "task_category":"compose-draft",
            "keywords":["blog","write"]
        }),
    );
    let (status, _, recalled) = send(
        &app,
        recall,
        Some("outcome-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recalled["result"]["isError"], false);
    let receipt_id = tool_value(&recalled)["receipt_id"]
        .as_str()
        .expect("authenticated recall receipt")
        .to_owned();

    let report = call_tool(
        "web_memory_outcome",
        json!({"receipt_id":receipt_id,"outcome":"success","used_version_ids":["v1"]}),
    );
    let (status, _, first) = send(
        &app,
        report.clone(),
        Some("outcome-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["result"]["isError"], false);
    assert_eq!(tool_value(&first)["recorded"], true);

    let (status, _, duplicate) = send(
        &app,
        report.clone(),
        Some("outcome-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tool_value(&duplicate)["recorded"], false);

    let (status, _, denied_scope) = send(
        &app,
        report.clone(),
        Some("reader-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(denied_scope["error"], "insufficient_scope");

    let (status, _, denied_owner) = send(
        &app,
        report,
        Some("other-outcome-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(denied_owner["result"]["isError"], true);

    let (status, _, board) = send(
        &app,
        call_tool("web_memory_leaderboard", json!({})),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entries = tool_value(&board)["entries"].as_array().unwrap().clone();
    let builder = entries
        .iter()
        .find(|entry| entry["handle"] == "memory_builder")
        .unwrap();
    assert_eq!(builder["total_points"], 26);
}

#[tokio::test]
async fn browser_outcome_requires_csrf_and_accepts_a_valid_session_token() {
    let app = app();
    let (status, _, recalled) = send(
        &app,
        call_tool(
            "web_memory_recall",
            json!({
                "url":"https://example.com/guide/editor",
                "task_category":"compose-draft",
                "keywords":["blog","write"]
            }),
        ),
        Some("browser-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let receipt_id = tool_value(&recalled)["receipt_id"]
        .as_str()
        .expect("browser recall receipt")
        .to_owned();
    let report = call_tool(
        "web_memory_outcome",
        json!({"receipt_id":receipt_id,"outcome":"partial"}),
    );

    let (status, _, body) = send(
        &app,
        report.clone(),
        Some("browser-token"),
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "csrf_failed");

    let (status, _, accepted) = send(
        &app,
        report,
        Some("browser-token"),
        None,
        Some("csrf-ok"),
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["result"]["isError"], false);
    assert_eq!(tool_value(&accepted)["recorded"], true);
}

#[tokio::test]
async fn remote_streamable_http_rejects_fractional_request_ids() {
    let (status, _, body) = send(
        &app(),
        json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}),
        None,
        None,
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], -32600);
}

#[tokio::test]
async fn remote_streamable_http_checks_origin_headers_and_methods() {
    let app = app();
    let allowed = McpOriginAllowlist::new(["https://client.example"]);

    let response = app
        .clone()
        .oneshot(Request::delete("/mcp").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers().get(header::ALLOW).unwrap(), "GET, POST");

    let guarded = router_with_origins(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        Arc::new(TestAuthenticator),
        allowed,
    );

    let response = guarded
        .clone()
        .oneshot(Request::delete("/mcp").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers().get(header::ALLOW).unwrap(), "GET, POST");

    let (status, _, body) = send(
        &guarded,
        rpc("initialize", 1, json!({"protocolVersion":"2025-03-26"})),
        None,
        Some("https://attacker.example"),
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "origin_not_allowed");

    let (status, _, allowed_response) = send(
        &guarded,
        rpc("initialize", 2, json!({"protocolVersion":"2025-03-26"})),
        None,
        Some("https://client.example"),
        None,
        "application/json, text/event-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(allowed_response["result"]["protocolVersion"], "2025-03-26");

    let (status, _, _) = send(
        &guarded,
        rpc("initialize", 1, json!({"protocolVersion":"2025-03-26"})),
        None,
        Some("https://client.example"),
        None,
        "application/json",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_ACCEPTABLE);
}

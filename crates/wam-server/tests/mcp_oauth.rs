use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{DemoTokenAuthenticator, InMemoryRepository, WamService, router};

#[tokio::test]
async fn protected_tool_requests_expose_an_http_authentication_challenge() {
    let app = router(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        Arc::new(DemoTokenAuthenticator::default()),
    );
    let request = Request::post("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"web_memory_my_rank","arguments":{}}}"#)).unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        response.headers()["www-authenticate"]
            .to_str()
            .unwrap()
            .starts_with("Bearer")
    );
}

use super::*;
use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, post},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn resend_rejects_malformed_receipts_and_does_not_follow_redirects() {
    for receipt in [serde_json::json!({}), serde_json::json!({"id":""})] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
        let app = Router::new().route("/emails", post(move || async move { Json(receipt) }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut sender =
            ResendEmailSender::new("fixture-key".into(), "noreply@example.com".into()).unwrap();
        sender.endpoint = endpoint;
        assert_eq!(
            sender.send_code("a@example.com", "012345", "fixture").await,
            Err(IdentityError::Provider)
        );
        server.abort();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
    let forwarded = Arc::new(AtomicUsize::new(0));
    let target = forwarded.clone();
    let app = Router::new()
        .route(
            "/emails",
            post(|| async {
                (StatusCode::TEMPORARY_REDIRECT, [("location", "/trap")]).into_response()
            }),
        )
        .route(
            "/trap",
            any(move || {
                let target = target.clone();
                async move {
                    target.fetch_add(1, Ordering::Relaxed);
                    Json(serde_json::json!({"id":"trap"}))
                }
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut sender =
        ResendEmailSender::new("fixture-key".into(), "noreply@example.com".into()).unwrap();
    sender.endpoint = endpoint;
    assert_eq!(
        sender.send_code("a@example.com", "012345", "fixture").await,
        Err(IdentityError::Provider)
    );
    assert_eq!(forwarded.load(Ordering::Relaxed), 0);
    server.abort();
}

#[tokio::test]
async fn resend_connection_timeout_is_a_redacted_provider_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
    let app = Router::new().route(
        "/emails",
        post(|| async {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            Json(serde_json::json!({"id":"late"}))
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut sender =
        ResendEmailSender::new("fixture-key".into(), "noreply@example.com".into()).unwrap();
    sender.endpoint = endpoint;
    sender.client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(50))
        .build()
        .unwrap();
    assert_eq!(
        sender.send_code("a@example.com", "012345", "fixture").await,
        Err(IdentityError::Provider)
    );
    server.abort();
}

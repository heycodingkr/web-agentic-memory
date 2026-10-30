use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{
    ApplicationRepository, DemoTokenAuthenticator, FileIdentityStore, IdentityService,
    RateLimitStore, WamService,
    http::NoReviewerAuthenticator,
    rate_limit::{BucketSpec, BucketStatus, DailyUsage, RateLimitError, UsageRecord},
    router_with_identity_and_rate_limit_store,
};

struct UnavailableRateLimitStore;

#[async_trait::async_trait]
impl RateLimitStore for UnavailableRateLimitStore {
    async fn consume(
        &self,
        _: &[BucketSpec],
        _: Option<&UsageRecord>,
        _: u64,
    ) -> Result<Vec<BucketStatus>, RateLimitError> {
        Err(RateLimitError)
    }

    async fn inspect(&self, _: &BucketSpec, _: u64) -> Result<BucketStatus, RateLimitError> {
        Err(RateLimitError)
    }

    async fn usage_history(
        &self,
        _: &[(String, String)],
    ) -> Result<Vec<DailyUsage>, RateLimitError> {
        Err(RateLimitError)
    }
}

#[tokio::test]
async fn unavailable_shared_limiter_returns_retryable_503_and_keeps_health_live() {
    let repository = ApplicationRepository::in_memory();
    let service = Arc::new(WamService::durable(repository.clone()).unwrap());
    let identity = Arc::new(IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(repository)),
        None,
        "http://localhost:5173",
        false,
    ));
    let app = router_with_identity_and_rate_limit_store(
        service,
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
        Arc::new(NoReviewerAuthenticator),
        identity,
        Arc::new(UnavailableRateLimitStore),
        "test-key-secret",
        Vec::new(),
    );

    let request = Request::get("/v1/memories")
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()[header::RETRY_AFTER], "1");
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["error"], "rate_limit_unavailable");
    assert_eq!(body["retry_after_seconds"], 1);

    let health = Request::get("/healthz").body(Body::empty()).unwrap();
    assert_eq!(app.oneshot(health).await.unwrap().status(), StatusCode::OK);
}

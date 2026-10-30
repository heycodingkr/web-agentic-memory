use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use wam_server::identity::DevicePoll;
use wam_server::{
    DemoTokenAuthenticator, FileIdentityStore, IdentityAuthenticator, IdentityService,
    InMemoryRepository, WamService, router_with_identity,
};

#[tokio::test]
async fn linked_device_can_inspect_and_revoke_only_itself_without_device_management_scope() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let directory = std::env::temp_dir().join(format!(
        "wam-self-link-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let identity = Arc::new(IdentityService::new(
        Arc::new(FileIdentityStore::open(directory.join("identity.json")).unwrap()),
        None,
        "https://wam.example",
        true,
    ));
    let mut tokens = Vec::new();
    for name in ["first", "second"] {
        let start = identity
            .start_device(
                name,
                vec!["read:memories".into(), "read:dashboard".into()],
                now,
            )
            .unwrap();
        identity
            .approve_device(&start.user_code, "github_device_owner", now)
            .unwrap();
        let DevicePoll::Authorized { access_token, .. } =
            identity.poll_device(&start.device_code, now).unwrap()
        else {
            panic!("authorized")
        };
        tokens.push(access_token);
    }
    let auth = Arc::new(IdentityAuthenticator::new(
        DemoTokenAuthenticator::with_token("demo"),
        Arc::clone(&identity),
    ));
    let app = router_with_identity(
        Arc::new(WamService::new(InMemoryRepository::seeded())),
        auth,
        Arc::new(wam_server::http::NoReviewerAuthenticator),
        Arc::clone(&identity),
    );

    for path in [
        "/v1/me/profile",
        "/v1/me/dashboard",
        "/v1/me/usage",
        "/v1/me/score-events",
    ] {
        let request = Request::get(path)
            .header("authorization", format!("Bearer {}", tokens[0]))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );
    }
    let write = Request::put("/v1/me/profile")
        .header("authorization", format!("Bearer {}", tokens[0]))
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"handle":"device_reader","public_opt_in":true}"#,
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(write).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "read:dashboard must never authorize profile changes"
    );

    for token in &tokens {
        let request = Request::get("/v1/link/status")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value["id"].as_str().unwrap().starts_with("device_"));
        assert!(!String::from_utf8_lossy(&bytes).contains(token));
    }
    let request = Request::post("/v1/link/revoke")
        .header("authorization", format!("Bearer {}", tokens[0]))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert!(identity.authenticate_device(&tokens[0], now).is_none());
    assert!(identity.authenticate_device(&tokens[1], now).is_some());
    for token in [&tokens[0], "demo", "invalid"] {
        let request = Request::post("/v1/link/revoke")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

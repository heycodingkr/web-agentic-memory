use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_contracts::{MemoryOutcome, OutcomeReportRequest, RecallRequest};
use wam_server::{
    DemoTokenAuthenticator, FileScoreStore, InMemoryContributionStore, InMemoryRepository,
    InMemoryScoreStore, InMemoryUsageStore, WamService,
    ports::{ScoreStore, SuccessfulUseAward},
    router,
};

fn service() -> Arc<WamService<InMemoryRepository>> {
    Arc::new(
        WamService::with_all_stores(
            InMemoryRepository::seeded(),
            Arc::new(InMemoryContributionStore::default()),
            Arc::new(InMemoryUsageStore::default()),
            Arc::new(InMemoryScoreStore::default()),
        )
        .expect("construct score-enabled service"),
    )
}

fn app(service: Arc<WamService<InMemoryRepository>>) -> axum::Router {
    router(
        service,
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    )
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn recall(app: &axum::Router, authenticated: bool) -> Option<String> {
    let mut request =
        Request::post("/v1/memories/recall").header(header::CONTENT_TYPE, "application/json");
    if authenticated {
        request = request.header(header::AUTHORIZATION, "Bearer local-test-token");
    }
    let request = request
        .body(Body::from(
            json!({
                "url":"https://example.com/guide/editor?token=private",
                "task_category":"compose-draft",
                "keywords":["blog","write"]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, body) = send(app, request).await;
    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    response["receipt_id"].as_str().map(str::to_owned)
}

async fn outcome(app: &axum::Router, receipt_id: &str, value: &str) -> (StatusCode, Value) {
    let request = Request::post(format!("/v1/receipts/{receipt_id}/outcome"))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"outcome":value,"used_version_ids":["v1"]}).to_string(),
        ))
        .unwrap();
    let (status, body) = send(app, request).await;
    let response = serde_json::from_str(&body).unwrap_or(Value::Null);
    (status, response)
}

async fn leaderboard_points(app: &axum::Router, handle: &str) -> i64 {
    let request = Request::get("/v1/leaderboard").body(Body::empty()).unwrap();
    let (status, body) = send(app, request).await;
    assert_eq!(status, StatusCode::OK);
    let board: Value = serde_json::from_str(&body).unwrap();
    board["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["handle"] == handle)
        .unwrap()["total_points"]
        .as_i64()
        .unwrap()
}

struct HistoryAuthenticator;

impl wam_server::http::Authenticator for HistoryAuthenticator {
    fn account_id(&self, headers: &header::HeaderMap) -> Option<String> {
        match headers.get(header::AUTHORIZATION)?.to_str().ok()? {
            "Bearer author-token" => Some("demo-peer".to_owned()),
            "Bearer consumer-token" => Some("demo-user".to_owned()),
            _ => None,
        }
    }
}

#[tokio::test]
async fn score_history_is_owner_scoped_and_does_not_expose_consumers_or_receipts() {
    let service = service();
    let consumer_app = app(Arc::clone(&service));
    let receipt = recall(&consumer_app, true).await.unwrap();
    assert_eq!(
        outcome(&consumer_app, &receipt, "success").await.0,
        StatusCode::OK
    );
    let history_app = router(service, Arc::new(HistoryAuthenticator));
    let author = Request::get("/v1/me/score-events")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let response = history_app.clone().oneshot(author).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let raw = String::from_utf8(bytes.to_vec()).unwrap();
    let result: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(result["events"].as_array().unwrap().len(), 1);
    assert_eq!(result["events"][0]["event_type"], "successful_use");
    assert_eq!(result["events"][0]["points_delta"], 2);
    assert_eq!(result["events"][0]["memory_id"], "web:example-draft");
    assert_eq!(result["events"][0]["memory_version_id"], "v1");
    assert!(result["next_cursor"].is_null());
    for private in [
        &receipt,
        "demo-user",
        "demo-peer",
        "consumer_account_id",
        "idempotency_key",
        "source_receipt_id",
    ] {
        assert!(
            !raw.contains(private),
            "private value must not appear: {private}"
        );
    }
    let cursor = result["events"][0]["id"].as_str().unwrap();

    let consumer = Request::get("/v1/me/score-events")
        .header(header::AUTHORIZATION, "Bearer consumer-token")
        .body(Body::empty())
        .unwrap();
    let (status, own) = send(&history_app, consumer).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        serde_json::from_str::<Value>(&own).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    for path in [
        format!("/v1/me/score-events?before={cursor}"),
        "/v1/me/score-events?before=score-99999999".to_owned(),
    ] {
        let request = Request::get(path)
            .header(header::AUTHORIZATION, "Bearer consumer-token")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&history_app, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, "{\"error\":\"not_found\"}");
    }
    for authorization in [None, Some("Bearer invalid")] {
        let mut request = Request::get("/v1/me/score-events");
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        assert_eq!(
            send(&history_app, request.body(Body::empty()).unwrap())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn score_history_pages_remain_stable_after_new_awards_and_file_restart() {
    let directory = std::env::temp_dir().join(format!(
        "wam-score-history-api-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("scores.json");
    let scores = Arc::new(FileScoreStore::open(&path).unwrap());
    let award = |receipt: &str, author: &str| SuccessfulUseAward {
        beneficiary_account_id: author.to_owned(),
        consumer_account_id: format!("private-reader-{receipt}"),
        memory_id: "web:example-draft".to_owned(),
        memory_version_id: "v1".to_owned(),
        receipt_id: receipt.to_owned(),
        created_at: 100,
    };
    for (receipt, author) in [
        ("one", "demo-peer"),
        ("foreign", "demo-user"),
        ("two", "demo-peer"),
        ("three", "demo-peer"),
    ] {
        scores.award_successful_use(award(receipt, author)).unwrap();
    }
    let build_app = |store: Arc<FileScoreStore>| {
        router(
            Arc::new(
                WamService::with_all_stores(
                    InMemoryRepository::seeded(),
                    Arc::new(InMemoryContributionStore::default()),
                    Arc::new(InMemoryUsageStore::default()),
                    store,
                )
                .unwrap(),
            ),
            Arc::new(HistoryAuthenticator),
        )
    };
    let app = build_app(Arc::clone(&scores));
    let request = Request::get("/v1/me/score-events?limit=2")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    let first: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(first["events"][0]["id"], "score-00000004");
    assert_eq!(first["events"][1]["id"], "score-00000003");
    let cursor = first["next_cursor"].as_str().unwrap();
    scores
        .award_successful_use(award("arrived-between-pages", "demo-peer"))
        .unwrap();

    let request = Request::get(format!("/v1/me/score-events?limit=2&before={cursor}"))
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    let second: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(second["events"].as_array().unwrap().len(), 1);
    assert_eq!(second["events"][0]["id"], "score-00000001");
    assert!(second["next_cursor"].is_null());
    for query in [
        "limit=0",
        "limit=101",
        "before=unrecognized",
        "before=score-",
        "account_id=demo-user",
    ] {
        let request = Request::get(format!("/v1/me/score-events?{query}"))
            .header(header::AUTHORIZATION, "Bearer author-token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, request).await.0, StatusCode::BAD_REQUEST);
    }
    drop(app);
    drop(scores);
    let reopened = build_app(Arc::new(FileScoreStore::open(&path).unwrap()));
    let request = Request::get("/v1/me/score-events")
        .header(header::AUTHORIZATION, "Bearer author-token")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&reopened, request).await;
    assert_eq!(status, StatusCode::OK);
    let all: Value = serde_json::from_str(&body).unwrap();
    let ids: Vec<_> = all["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "score-00000005",
            "score-00000004",
            "score-00000003",
            "score-00000001"
        ]
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn successful_use_is_credited_to_the_approved_version_author_once_per_day() {
    let service = service();
    let app = app(Arc::clone(&service));
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 24);

    let receipt = recall(&app, true).await.expect("authenticated hit receipt");
    let (status, first) = outcome(&app, &receipt, "success").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["recorded"], true);
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 26);

    let (status, duplicate) = outcome(&app, &receipt, "success").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate["recorded"], false);
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 26);

    let second_receipt = recall(&app, true).await.expect("second receipt");
    let (status, second_outcome) = outcome(&app, &second_receipt, "success").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second_outcome["recorded"], true);
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 26);

    assert_eq!(recall(&app, false).await, None);
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 26);
}

#[tokio::test]
async fn partial_outcomes_and_author_self_use_do_not_award_points() {
    let service = service();
    let app = app(Arc::clone(&service));
    let receipt = recall(&app, true).await.expect("authenticated hit receipt");
    let (status, response) = outcome(&app, &receipt, "partial").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["recorded"], true);
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 24);

    let author_recall = service
        .recall(
            RecallRequest {
                url: "https://example.com/guide/editor".to_owned(),
                task_category: Some("compose-draft".to_owned()),
                keywords: vec!["blog".to_owned()],
                kind: None,
                limit: Some(3),
            },
            Some("demo-peer"),
        )
        .unwrap();
    let self_receipt = author_recall.receipt_id.unwrap();
    service
        .report_outcome(
            "demo-peer",
            &self_receipt,
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec!["v1".to_owned()],
            },
        )
        .unwrap();
    assert_eq!(leaderboard_points(&app, "memory_builder").await, 24);
}

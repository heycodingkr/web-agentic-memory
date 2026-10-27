use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_contracts::{MemoryOutcome, OutcomeReportRequest};
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    leaderboard::RankingAccount,
    memory::{MemoryIndex, MemoryStatus},
    profile::{CreatorProfile, PublicHandle},
};
use wam_server::{
    DemoTokenAuthenticator, FileScoreStore, FileUsageStore, InMemoryContributionStore,
    InMemoryRepository, InMemoryScoreStore, InMemoryUsageStore, WamService,
    ports::{
        MemoryRecord, ProfileSnapshot, ReceiptMemoryAttribution, Repository, RepositoryError,
        ScoreAwardResult, ScoreStore, SuccessfulUseAward, UsageStore,
    },
    router,
};

struct CatalogRepository {
    inner: InMemoryRepository,
    records: Vec<MemoryRecord>,
}

impl CatalogRepository {
    fn new() -> Self {
        let inner = InMemoryRepository::seeded();
        let seed = inner.memories().remove(0);
        let records = [
            ("used", "demo-peer", MemoryStatus::Active),
            ("unused", "private-user", MemoryStatus::Active),
            ("retired", "demo-peer", MemoryStatus::Retired),
            ("superseded", "demo-peer", MemoryStatus::Superseded),
        ]
        .into_iter()
        .map(|(name, author, status)| {
            let index = &seed.index;
            MemoryRecord {
                index: MemoryIndex::new(
                    format!("web:{name}"),
                    format!("{name}-v1"),
                    index.url_patterns().to_vec(),
                    index.task_category().clone(),
                    index.kind(),
                    name,
                    index.keywords().to_vec(),
                    index.confidence_band(),
                    status,
                    index.verified_at(),
                )
                .unwrap(),
                author_account_id: Some(author.to_owned()),
                ..seed.clone()
            }
        })
        .collect();
        Self { inner, records }
    }
}

impl Repository for CatalogRepository {
    fn memories(&self) -> Vec<MemoryRecord> {
        self.records.clone()
    }

    fn task_vocabulary(&self) -> TaskVocabulary {
        self.inner.task_vocabulary()
    }

    fn keyword_vocabulary(&self) -> KeywordVocabulary {
        self.inner.keyword_vocabulary()
    }

    fn ranking_accounts(&self) -> Vec<RankingAccount> {
        self.inner.ranking_accounts()
    }

    fn profile_by_handle(&self, handle: &PublicHandle) -> Option<ProfileSnapshot> {
        self.inner.profile_by_handle(handle)
    }

    fn profile_for_account(&self, account_id: &str) -> Option<ProfileSnapshot> {
        self.inner.profile_for_account(account_id)
    }

    fn save_profile(
        &self,
        account_id: &str,
        profile: CreatorProfile,
        public_opt_in: bool,
    ) -> Result<(), RepositoryError> {
        self.inner.save_profile(account_id, profile, public_opt_in)
    }
}

fn app() -> (axum::Router, Arc<InMemoryScoreStore>) {
    let scores = Arc::new(InMemoryScoreStore::default());
    let service = WamService::with_all_stores(
        CatalogRepository::new(),
        Arc::new(InMemoryContributionStore::default()),
        Arc::new(InMemoryUsageStore::default()),
        scores.clone(),
    )
    .unwrap();
    (
        router(
            Arc::new(service),
            Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
        ),
        scores,
    )
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn recall(app: &axum::Router) -> String {
    let (status, response) = send(
        app,
        Request::post("/v1/memories/recall")
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "url":"https://example.com/guide/editor",
                    "task_category":"compose-draft",
                    "keywords":["blog","write"]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["hits"].as_array().unwrap().len(), 2);
    response["receipt_id"].as_str().unwrap().to_owned()
}

async fn outcome(app: &axum::Router, receipt: &str, input: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::post(format!("/v1/receipts/{receipt}/outcome"))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(input.to_string()))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn success_without_used_versions_does_not_credit_recalled_candidates() {
    let (app, scores) = app();
    let receipt = recall(&app).await;
    let (status, response) = outcome(&app, &receipt, json!({"outcome":"success"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["recorded"], true);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 0);
    assert_eq!(scores.total_points_for("private-user").unwrap(), 0);
    let (status, _) = outcome(
        &app,
        &receipt,
        json!({"outcome":"success","used_version_ids":["used-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 0);
}

#[tokio::test]
async fn success_only_credits_selected_receipt_versions_and_locks_the_selection() {
    let (app, scores) = app();
    let receipt = recall(&app).await;
    let input = json!({"outcome":"success","used_version_ids":["used-v1"]});
    let (status, response) = outcome(&app, &receipt, input.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["recorded"], true);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 2);
    assert_eq!(scores.total_points_for("private-user").unwrap(), 0);

    let (status, response) = outcome(&app, &receipt, input).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["recorded"], false);
    let (status, _) = outcome(
        &app,
        &receipt,
        json!({"outcome":"success","used_version_ids":["unused-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(scores.total_points_for("private-user").unwrap(), 0);

    let second = recall(&app).await;
    assert_eq!(
        outcome(
            &app,
            &second,
            json!({"outcome":"success","used_version_ids":["used-v1"]}),
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 2);
}

#[tokio::test]
async fn invalid_version_selection_does_not_consume_the_receipt() {
    let (app, scores) = app();
    let receipt = recall(&app).await;
    for versions in [
        json!(["foreign-v1"]),
        json!(["used-v1", "used-v1"]),
        json!(["used-v1", "foreign-v1"]),
    ] {
        let (status, _) = outcome(
            &app,
            &receipt,
            json!({"outcome":"success","used_version_ids":versions}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = outcome(
        &app,
        &receipt,
        json!({"outcome":"success","used_version_ids":["unused-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 0);
    assert_eq!(scores.total_points_for("private-user").unwrap(), 2);
}

#[tokio::test]
async fn selection_order_is_idempotent_and_both_used_authors_can_be_credited() {
    let (app, scores) = app();
    let receipt = recall(&app).await;
    let (status, first) = outcome(
        &app,
        &receipt,
        json!({"outcome":"success","used_version_ids":["used-v1","unused-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["recorded"], true);
    let (status, duplicate) = outcome(
        &app,
        &receipt,
        json!({"outcome":"success","used_version_ids":["unused-v1","used-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate["recorded"], false);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 2);
    assert_eq!(scores.total_points_for("private-user").unwrap(), 2);
    let (status, _) = outcome(
        &app,
        &receipt,
        json!({"outcome":"failed","used_version_ids":["used-v1","unused-v1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn quota_blocked_outcome_cannot_earn_points_by_replaying_on_a_later_day_after_restart() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let reported_at = now - 24 * 60 * 60;
    let directory = std::env::temp_dir().join(format!(
        "wam-outcome-replay-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let usage_path = directory.join("usage.json");
    let score_path = directory.join("scores.json");
    let input = OutcomeReportRequest {
        outcome: MemoryOutcome::Success,
        used_version_ids: vec!["used-v1".to_owned()],
    };
    let receipt = {
        let usage = FileUsageStore::open(&usage_path).unwrap();
        let scores = FileScoreStore::open(&score_path).unwrap();
        let receipt = usage
            .record_recall(
                Some("demo-user"),
                &[ReceiptMemoryAttribution {
                    memory_id: "web:used".to_owned(),
                    version_id: "used-v1".to_owned(),
                    author_account_id: Some("demo-peer".to_owned()),
                }],
                reported_at,
            )
            .unwrap()
            .unwrap();
        let award = |receipt_id: &str, version: &str| SuccessfulUseAward {
            beneficiary_account_id: "demo-peer".to_owned(),
            consumer_account_id: "demo-user".to_owned(),
            memory_id: "web:used".to_owned(),
            memory_version_id: version.to_owned(),
            receipt_id: receipt_id.to_owned(),
            created_at: reported_at,
        };
        assert_eq!(
            scores
                .award_successful_use(award("earlier-success", "earlier-v1"))
                .unwrap(),
            ScoreAwardResult::Recorded
        );
        usage
            .report_outcome("demo-user", &receipt, &input, reported_at)
            .unwrap();
        assert_eq!(
            scores
                .award_successful_use(award(&receipt, "used-v1"))
                .unwrap(),
            ScoreAwardResult::DailyLimit
        );
        receipt
    };
    let scores = Arc::new(FileScoreStore::open(&score_path).unwrap());
    let service = WamService::with_all_stores(
        CatalogRepository::new(),
        Arc::new(InMemoryContributionStore::default()),
        Arc::new(FileUsageStore::open(&usage_path).unwrap()),
        scores.clone(),
    )
    .unwrap();
    let app = router(
        Arc::new(service),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    );
    let (status, response) = outcome(&app, &receipt, serde_json::to_value(input).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["recorded"], false);
    assert_eq!(scores.total_points_for("demo-peer").unwrap(), 2);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn retired_and_superseded_memories_are_hidden_from_detail_at_both_levels() {
    let (app, _) = app();
    for id in ["retired", "superseded"] {
        for level in [1, 2] {
            let (status, _) = send(
                &app,
                Request::get(format!("/v1/memories/web:{id}?level={level}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{id}, level {level}");
        }
    }
    for level in [1, 2] {
        let (status, _) = send(
            &app,
            Request::get(format!("/v1/memories/web:used?level={level}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
}
